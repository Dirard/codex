package codex

import (
	"context"
	"encoding/json"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// These tests use only the handle API that predates the handle state machine, so they also
// reproduce the hangs on the code before it (docs/fork/05-go-sdk.md R8).

const repeatedWaitTimeout = time.Second

func startTestReview(t *testing.T) (*ReviewHandle, *scriptedTransport) {
	t.Helper()
	transport := newWorkflowTransport(t)
	transport.responses["review/start"] = mustJSON(t, protocol.ReviewStartResponse{
		ReviewThreadID: "review-thread-1",
		Turn:           protocol.Turn{ID: "review-turn-1", Items: []protocol.ThreadItem{}, Status: protocol.TurnStatusInProgress},
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	handle, err := client.Reviews.Start(context.Background(), ReviewStartOptions{ThreadID: "thread-1", Target: UncommittedChangesReviewTarget()})
	if err != nil {
		t.Fatal(err)
	}
	return handle, transport
}

func deliverTurnEvents(t *testing.T, transport *scriptedTransport, threadID string, turnID string, text string) {
	t.Helper()
	transport.deliverNotification("item/completed", mustJSON(t, protocol.ItemCompletedNotification{
		ThreadID: threadID,
		TurnID:   turnID,
		Item: protocol.ThreadItem{ID: protocol.SomeNonNull("item-1"), TypeValue: "agentMessage",
			Text: protocol.SomeNonNull(text), Phase: protocol.Some(protocol.MessagePhaseFinalAnswer)},
	}), nil)
	transport.deliverNotification("turn/completed", mustJSON(t, protocol.TurnCompletedNotification{
		ThreadID: threadID,
		Turn:     protocol.Turn{ID: turnID, Items: []protocol.ThreadItem{}, Status: protocol.TurnStatusCompleted},
	}), nil)
}

func boundedContext(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), repeatedWaitTimeout)
	t.Cleanup(cancel)
	return ctx
}

func drainStream(t *testing.T, stream *NotificationStream) []string {
	t.Helper()
	ctx := boundedContext(t)
	var methods []string
	for {
		notification, ok := stream.Next(ctx)
		if !ok {
			if err := stream.Err(); err != nil {
				t.Fatalf("stream ended with %v after %v", err, methods)
			}
			return methods
		}
		methods = append(methods, notification.Method)
	}
}

func assertReviewResult(t *testing.T, result *ReviewResult, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("Wait: %v", err)
	}
	if result.TurnID != "review-turn-1" || result.Status != protocol.TurnStatusCompleted || result.FinalResponse != "done" || len(result.Items) != 1 {
		t.Fatalf("result = %+v", result)
	}
}

// R1.1: a second Wait returns the first outcome instead of waiting for a terminal event
// the first Wait already consumed.
func TestReviewHandleRepeatedWaitReturnsSameResult(t *testing.T) {
	handle, transport := startTestReview(t)
	deliverTurnEvents(t, transport, "review-thread-1", "review-turn-1", "done")
	first, err := handle.Wait(boundedContext(t))
	assertReviewResult(t, first, err)
	second, err := handle.Wait(boundedContext(t))
	assertReviewResult(t, second, err)
}

// R1.3: Wait after Events was read to the end still returns the result.
func TestReviewHandleWaitAfterEventsReturnsResult(t *testing.T) {
	handle, transport := startTestReview(t)
	deliverTurnEvents(t, transport, "review-thread-1", "review-turn-1", "done")
	events, err := handle.Events(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if methods := drainStream(t, events); len(methods) != 2 {
		t.Fatalf("events = %v", methods)
	}
	result, err := handle.Wait(boundedContext(t))
	assertReviewResult(t, result, err)
}

// R1.1: concurrent Waits share one outcome.
func TestReviewHandleConcurrentWaitsShareResult(t *testing.T) {
	handle, transport := startTestReview(t)
	const waiters = 8
	var wg sync.WaitGroup
	results := make([]*ReviewResult, waiters)
	errs := make([]error, waiters)
	for i := range waiters {
		wg.Add(1)
		go func() {
			defer wg.Done()
			results[i], errs[i] = handle.Wait(boundedContext(t))
		}()
	}
	time.Sleep(20 * time.Millisecond)
	deliverTurnEvents(t, transport, "review-thread-1", "review-turn-1", "done")
	wg.Wait()
	for i := range waiters {
		assertReviewResult(t, results[i], errs[i])
	}
}

// R1.2: a stream requested after the turn finished is closed instead of open forever.
func TestTurnHandleStreamAfterFinishIsClosed(t *testing.T) {
	client, transport := newStage5Client(t)
	transport.responses["turn/start"] = json.RawMessage(`{"turn":{"id":"turn-1","items":[],"status":"inProgress"}}`)
	thread := &Thread{client: client, id: "thread-1"}
	handle, err := thread.Turn(context.Background(), Text("inspect"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	first, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if methods := drainStream(t, first); len(methods) != 2 {
		t.Fatalf("first stream = %v", methods)
	}
	second, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if methods := drainStream(t, second); len(methods) != 0 {
		t.Fatalf("second stream replayed %v", methods)
	}
}

func startTestLogin(t *testing.T) (*LoginHandle, *scriptedTransport) {
	t.Helper()
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.responses["account/login/start"] = mustJSON(t, protocol.LoginAccountResponse{
		TypeValue: "chatgpt",
		LoginID:   protocol.SomeNonNull("login-1"),
		AuthURL:   protocol.SomeNonNull("https://example.test/auth"),
	})
	transport.responses["account/read"] = json.RawMessage(`{"account":{"type":"apiKey"},"requiresOpenaiAuth":true}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	handle, err := client.Accounts.StartChatGPTLogin(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	return handle, transport
}

// R1.1: repeated and concurrent login Waits return the same outcome.
func TestLoginHandleRepeatedAndConcurrentWaitReturnSameResult(t *testing.T) {
	handle, transport := startTestLogin(t)
	const waiters = 4
	var wg sync.WaitGroup
	results := make([]*LoginResult, waiters)
	errs := make([]error, waiters)
	for i := range waiters {
		wg.Add(1)
		go func() {
			defer wg.Done()
			results[i], errs[i] = handle.Wait(boundedContext(t))
		}()
	}
	time.Sleep(20 * time.Millisecond)
	transport.deliverNotification("account/login/completed", mustJSON(t, protocol.AccountLoginCompletedNotification{
		LoginID: protocol.Some("login-1"),
		Success: true,
	}), nil)
	wg.Wait()
	again, err := handle.Wait(boundedContext(t))
	results = append(results, again)
	errs = append(errs, err)
	for i, result := range results {
		if errs[i] != nil || result == nil || result.LoginID != "login-1" || !result.Success {
			t.Fatalf("wait %d = %+v, %v", i, result, errs[i])
		}
	}
}

// R1.1: a second MCP OAuth Wait returns the first outcome.
func TestMCPOAuthHandleRepeatedWaitReturnsSameResult(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["mcpServer/oauth/login"] = mustJSON(t, protocol.McpServerOauthLoginResponse{
		AuthorizationURL: "https://example.test/oauth",
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	handle, err := client.MCP.OAuthLogin(context.Background(), MCPOAuthLoginOptions{Name: "server-1"})
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("mcpServer/oauthLogin/completed", mustJSON(t, protocol.McpServerOauthLoginCompletedNotification{
		Name:    "server-1",
		Success: true,
	}), nil)
	for i := range 2 {
		result, err := handle.Wait(boundedContext(t))
		if err != nil || result.Name != "server-1" || !result.Success {
			t.Fatalf("wait %d = %+v, %v", i, result, err)
		}
	}
}

// Command handles already kept their outcome; keep it that way.
func TestCommandHandleRepeatedWaitReturnsSameResult(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["command/exec"] = json.RawMessage(`{"exitCode":0,"stdout":"ok","stderr":""}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	handle, err := client.Commands.Exec(context.Background(), CommandExecOptions{Command: []string{"true"}})
	if err != nil {
		t.Fatal(err)
	}
	for i := range 2 {
		response, err := handle.Wait(boundedContext(t))
		if err != nil || response.ExitCode != 0 {
			t.Fatalf("wait %d = %+v, %v", i, response, err)
		}
	}
}
