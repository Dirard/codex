package codextest_test

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/codextest"
	"github.com/openai/codex/sdk/go/protocol"
)

func testContext(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	t.Cleanup(cancel)
	return ctx
}

func newClient(t *testing.T, srv *codextest.Server, cfg codex.ClientConfig) *codex.Client {
	t.Helper()
	cfg.Transport = srv
	client, err := codex.NewClient(testContext(t), cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	return client
}

// scriptTurn answers turn/start and then plays a turn that answers text.
func scriptTurn(srv *codextest.Server, turnID string, text string) {
	srv.Handle("turn/start", func(ctx context.Context, call *codextest.Call) (any, error) {
		var params protocol.TurnStartParams
		if err := call.Decode(&params); err != nil {
			return nil, err
		}
		if err := call.NotifyAfter("item/completed", codextest.AgentMessageCompleted(params.ThreadID, turnID, "item-1", text)); err != nil {
			return nil, err
		}
		if err := call.NotifyAfter("turn/completed", codextest.TurnCompleted(params.ThreadID, turnID, protocol.TurnStatusCompleted)); err != nil {
			return nil, err
		}
		return codextest.TurnStartResponse(turnID), nil
	})
}

// R5.1–R5.3: a full Thread.Run against the fake server, with the handshake checked.
func TestServerRunsAThreadTurn(t *testing.T) {
	srv := codextest.NewServer()
	defer srv.Close()
	srv.Respond("thread/start", codextest.ThreadStartResponse("thread-1"))
	scriptTurn(srv, "turn-1", "4")
	client := newClient(t, srv, codex.ClientConfig{ClientName: "consumer_test"})
	ctx := testContext(t)

	thread, err := client.Threads.Start(ctx, codex.ThreadStartOptions{Model: "m"})
	if err != nil {
		t.Fatal(err)
	}
	result, err := thread.Run(ctx, codex.Text("2+2?"), codex.TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if result.FinalResponse != "4" || result.Status != protocol.TurnStatusCompleted {
		t.Fatalf("result = %+v", result)
	}

	if err := srv.WaitInitialized(ctx); err != nil {
		t.Fatal(err)
	}
	params, ok := srv.InitializeParams()
	if !ok || params.ClientInfo.Name != "consumer_test" {
		t.Fatalf("initialize params = %+v, %v", params, ok)
	}
	turnStart, err := srv.WaitForRequest(ctx, "turn/start")
	if err != nil {
		t.Fatal(err)
	}
	var turnParams protocol.TurnStartParams
	if err := turnStart.Decode(&turnParams); err != nil || turnParams.ThreadID != "thread-1" {
		t.Fatalf("turn/start params = %+v, %v", turnParams, err)
	}
	if methods := requestMethods(srv); strings.Join(methods, ",") != "initialize,thread/start,turn/start" {
		t.Fatalf("requests = %v", methods)
	}
	if err := srv.Err(); err != nil {
		t.Fatalf("handshake violations: %v", err)
	}
	if unhandled := srv.Unhandled(); len(unhandled) != 0 {
		t.Fatalf("unhandled = %v", unhandled)
	}
}

func requestMethods(srv *codextest.Server) []string {
	var methods []string
	for _, request := range srv.Requests() {
		methods = append(methods, request.Method)
	}
	return methods
}

// R5.2: the fake answers with the mode the client asked for, so both modes start.
func TestServerFollowsTheClientProtocolMode(t *testing.T) {
	for _, mode := range []codex.ProtocolMode{codex.ProtocolModeExperimental, codex.ProtocolModeStable} {
		srv := codextest.NewServer()
		client := newClient(t, srv, codex.ClientConfig{ProtocolMode: mode})
		if client.Metadata().ProtocolMode != mode {
			t.Fatalf("mode = %v, want %v", client.Metadata().ProtocolMode, mode)
		}
		_ = srv.Close()
	}
}

// R5.2: a replaced initialize response lets a test drive compatibility failures.
func TestServerInitializeOverride(t *testing.T) {
	srv := codextest.NewServer(codextest.WithInitializeResponse(protocol.InitializeResponse{
		ActiveProtocolMode:         protocol.ActiveProtocolModeExperimental,
		CodexHome:                  "/tmp/codextest",
		PlatformFamily:             "unix",
		PlatformOs:                 "linux",
		UserAgent:                  codextest.DefaultUserAgent,
		ExperimentalProtocolDigest: "sha256:other",
	}))
	defer srv.Close()
	_, err := codex.NewClient(testContext(t), codex.ClientConfig{Transport: srv})
	var compatibilityErr *codex.CompatibilityError
	if !errors.As(err, &compatibilityErr) {
		t.Fatalf("err = %T %v, want *CompatibilityError", err, err)
	}
}

// R5.3: an unscripted method fails with method-not-found and is recorded.
func TestServerUnscriptedMethod(t *testing.T) {
	srv := codextest.NewServer()
	defer srv.Close()
	srv.RespondError("thread/list", -32000, "list failed")
	client := newClient(t, srv, codex.ClientConfig{})
	ctx := testContext(t)
	_, err := client.Raw().MemoryReset(ctx)
	var rpcErr *codex.RPCError
	if !errors.As(err, &rpcErr) || rpcErr.Code != codextest.CodeMethodNotFound {
		t.Fatalf("err = %v, want method not found", err)
	}
	_, err = client.Threads.List(ctx, protocol.ThreadListParams{})
	if !errors.As(err, &rpcErr) || rpcErr.Code != -32000 || rpcErr.Message != "list failed" {
		t.Fatalf("scripted error = %v", err)
	}
	if unhandled := srv.Unhandled(); len(unhandled) != 1 || unhandled[0] != "memory/reset" {
		t.Fatalf("unhandled = %v", unhandled)
	}
}

// R5.2: a request before initialize is answered with an error and recorded as a violation.
func TestServerRecordsHandshakeViolations(t *testing.T) {
	srv := codextest.NewServer()
	defer srv.Close()
	ctx := testContext(t)
	if err := srv.Send(ctx, json.RawMessage(`{"id":1,"method":"thread/list","params":{}}`)); err != nil {
		t.Fatal(err)
	}
	frame, err := srv.Receive(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(frame), `"code":-32600`) {
		t.Fatalf("response = %s", frame)
	}
	if err := srv.Send(ctx, json.RawMessage(`{"method":"initialized"}`)); err != nil {
		t.Fatal(err)
	}
	violations := srv.Err()
	if violations == nil || !strings.Contains(violations.Error(), "thread/list before initialize") ||
		!strings.Contains(violations.Error(), "initialized before initialize") {
		t.Fatalf("violations = %v", violations)
	}
}

// R5.4: a server request reaches the client's handler and its answer comes back.
func TestServerRequestReachesClientHandler(t *testing.T) {
	srv := codextest.NewServer()
	defer srv.Close()
	newClient(t, srv, codex.ClientConfig{Handlers: codex.ServerHandlers{
		CurrentTime: codex.CurrentTimeCurrentTimeReadFunc(func(_ context.Context, params protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error) {
			if params.ThreadID != "thread-1" {
				return protocol.CurrentTimeReadResponse{}, errors.New("wrong thread")
			}
			return protocol.CurrentTimeReadResponse{CurrentTimeAt: 42}, nil
		}),
	}})
	ctx := testContext(t)
	if err := srv.WaitInitialized(ctx); err != nil {
		t.Fatal(err)
	}
	result, err := srv.Request(ctx, "currentTime/read", protocol.CurrentTimeReadParams{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	var response protocol.CurrentTimeReadResponse
	if err := json.Unmarshal(result, &response); err != nil || response.CurrentTimeAt != 42 {
		t.Fatalf("response = %s, %v", result, err)
	}
	_, err = srv.Request(ctx, "currentTime/read", protocol.CurrentTimeReadParams{ThreadID: "other"})
	var rpcErr *codextest.Error
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T %v, want *codextest.Error", err, err)
	}
}

// R5.4: notifications reach streams in order, and Disconnect looks like a runtime exit.
func TestServerNotificationsAndDisconnect(t *testing.T) {
	srv := codextest.NewServer()
	client := newClient(t, srv, codex.ClientConfig{})
	ctx := testContext(t)
	stream, err := client.Notifications(ctx)
	if err != nil {
		t.Fatal(err)
	}
	for _, text := range []string{"a", "b", "c"} {
		if err := srv.Notify("item/agentMessage/delta", protocol.AgentMessageDeltaNotification{ThreadID: "t", TurnID: "u", ItemID: "i", Delta: text}); err != nil {
			t.Fatal(err)
		}
	}
	srv.Disconnect(nil)
	var text string
	var failure error
	for delta, err := range codex.Payloads[protocol.AgentMessageDeltaNotification](ctx, stream) {
		if err != nil {
			failure = err
			break
		}
		text += delta.Delta
	}
	var transportErr *codex.TransportError
	if text != "abc" || !errors.As(failure, &transportErr) {
		t.Fatalf("text = %q, err = %v", text, failure)
	}
	if err := srv.Notify("item/agentMessage/delta", nil); !errors.Is(err, codextest.ErrClosed) {
		t.Fatalf("Notify after Disconnect = %v", err)
	}
}

// R5.3: a blocking handler holds its response back without blocking the client.
func TestServerHandlerMayBlock(t *testing.T) {
	srv := codextest.NewServer()
	defer srv.Close()
	release := make(chan struct{})
	srv.Handle("memory/reset", func(ctx context.Context, _ *codextest.Call) (any, error) {
		select {
		case <-release:
			return nil, nil
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	})
	srv.Respond("thread/list", json.RawMessage(`{"data":[]}`))
	client := newClient(t, srv, codex.ClientConfig{})
	ctx := testContext(t)
	done := make(chan error, 1)
	go func() {
		_, err := client.Raw().MemoryReset(ctx)
		done <- err
	}()
	if _, err := srv.WaitForRequest(ctx, "memory/reset"); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Threads.List(ctx, protocol.ThreadListParams{}); err != nil {
		t.Fatalf("a blocked handler blocked another call: %v", err)
	}
	close(release)
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

// R5.1: codextest must stay importable by the SDK's own tests.
func TestCodextestDoesNotImportRootPackage(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, file := range files {
		if strings.HasSuffix(file, "_test.go") {
			continue
		}
		data, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Contains(string(data), `"github.com/openai/codex/sdk/go"`) {
			t.Fatalf("%s imports the root SDK package", file)
		}
	}
}
