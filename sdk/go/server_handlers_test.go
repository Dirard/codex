package codex

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

func TestGeneratedMultiMethodHandlerFuncsSatisfyServerHandlers(t *testing.T) {
	handlers := ServerHandlers{
		Approvals: ApprovalsHandlerFuncs{
			ItemCommandExecutionRequestApproval: func(context.Context, protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
				return protocol.CommandExecutionRequestApprovalResponse{}, nil
			},
			ItemFileChangeRequestApproval: func(context.Context, protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error) {
				return protocol.FileChangeRequestApprovalResponse{}, nil
			},
		},
	}
	if handlers.Approvals == nil {
		t.Fatal("Approvals handler should be assignable")
	}
}

func TestUnknownServerRequestHandlerReceivesRawParams(t *testing.T) {
	var got UnknownServerRequest
	handlers := ServerHandlers{
		Unknown: UnknownServerRequestFunc(func(_ context.Context, request UnknownServerRequest) (any, error) {
			got = request
			return map[string]string{"ok": "true"}, nil
		}),
	}
	result, err := handlers.DispatchServerRequest(context.Background(), "future/request", json.RawMessage(`{"value":1}`))
	if err != nil {
		t.Fatal(err)
	}
	if got.Method != "future/request" || string(got.Params) != `{"value":1}` {
		t.Fatalf("unknown request = %#v", got)
	}
	if result == nil {
		t.Fatal("unknown handler result was nil")
	}
}

func TestSDKServerHandlerRegisteredSuccess(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Handlers: ServerHandlers{
			Approvals: ApprovalsHandlerFuncs{
				ItemCommandExecutionRequestApproval: func(_ context.Context, params protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
					if params.ThreadID != "thread-1" || params.TurnID != "turn-1" || params.ItemID != "item-1" {
						t.Fatalf("params = %#v", params)
					}
					return protocol.CommandExecutionRequestApprovalResponse{
						Decision: protocol.CommandExecutionApprovalDecisionAccept,
					}, nil
				},
			},
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	requestID := protocol.IntRequestID(201)
	transport.deliverServerRequestWithID(t, requestID, "item/commandExecution/requestApproval", commandApprovalRequestParams(), nil)

	reply := waitForServerReply(t, transport, requestID)
	if reply.Error != nil {
		t.Fatalf("reply error = %#v", reply.Error)
	}
	if len(reply.Result) == 0 {
		t.Fatal("registered handler reply omitted result")
	}
}

func TestSDKServerHandlerMissingHandlerReturnsUnsupported(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	requestID := protocol.IntRequestID(202)
	transport.deliverServerRequestWithID(t, requestID, "item/commandExecution/requestApproval", commandApprovalRequestParams(), nil)

	reply := waitForServerReply(t, transport, requestID)
	if reply.Error == nil {
		t.Fatal("missing handler reply did not contain an error")
	}
	if !strings.Contains(reply.Error.Message, "codex sdk unsupported: server handler") {
		t.Fatalf("reply error = %#v", reply.Error)
	}
}

func TestSDKServerHandlerBlockedHandlerDoesNotBlockUnrelatedResponse(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/usage/read"] = json.RawMessage(`{"summary":{}}`)
	started := make(chan struct{})
	release := make(chan struct{})
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Handlers: ServerHandlers{
			Approvals: ApprovalsHandlerFuncs{
				ItemCommandExecutionRequestApproval: func(context.Context, protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
					close(started)
					<-release
					return protocol.CommandExecutionRequestApprovalResponse{
						Decision: protocol.CommandExecutionApprovalDecisionAccept,
					}, nil
				},
			},
		},
		Limits: ClientLimits{HandlerTimeout: time.Second},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		close(release)
		_ = client.Close()
	})

	transport.deliverServerRequestWithID(t, protocol.IntRequestID(203), "item/commandExecution/requestApproval", commandApprovalRequestParams(), nil)
	<-started

	done := make(chan error, 1)
	go func() {
		_, err := client.Raw().AccountUsageRead(context.Background())
		done <- err
	}()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("blocked handler prevented unrelated client response routing")
	}
}

func TestSDKServerHandlerCallbackIntoSameClientUsesNormalRequestPath(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/usage/read"] = json.RawMessage(`{"summary":{}}`)
	var client *Client
	var err error
	client, err = NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Handlers: ServerHandlers{
			Approvals: ApprovalsHandlerFuncs{
				ItemCommandExecutionRequestApproval: func(ctx context.Context, _ protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
					if _, err := client.Raw().AccountUsageRead(ctx); err != nil {
						return protocol.CommandExecutionRequestApprovalResponse{}, err
					}
					return protocol.CommandExecutionRequestApprovalResponse{
						Decision: protocol.CommandExecutionApprovalDecisionAccept,
					}, nil
				},
			},
		},
		Limits: ClientLimits{HandlerTimeout: time.Second},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	requestID := protocol.IntRequestID(204)
	transport.deliverServerRequestWithID(t, requestID, "item/commandExecution/requestApproval", commandApprovalRequestParams(), nil)

	reply := waitForServerReply(t, transport, requestID)
	if reply.Error != nil {
		t.Fatalf("reply error = %#v", reply.Error)
	}
	waitForMethod(t, transport, "account/usage/read")
}

func (t *scriptedTransport) deliverServerRequestWithID(tb testing.TB, id protocol.RequestID, method string, params json.RawMessage, trace json.RawMessage) {
	tb.Helper()
	env := jsonrpc.Envelope{ID: &id, Method: method, Params: params, Trace: trace}
	data, err := json.Marshal(env)
	if err != nil {
		tb.Fatal(err)
	}
	t.recv <- data
}

func waitForServerReply(t *testing.T, transport *scriptedTransport, id protocol.RequestID) jsonrpc.Envelope {
	t.Helper()
	wantID := requestIDJSON(t, id)
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		for _, frame := range transport.sentFrames() {
			var env jsonrpc.Envelope
			if err := json.Unmarshal(frame, &env); err != nil {
				t.Fatal(err)
			}
			if env.ID == nil || env.Method != "" {
				continue
			}
			if requestIDJSON(t, *env.ID) == wantID {
				return env
			}
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("server reply for id %s was not sent", wantID)
	return jsonrpc.Envelope{}
}

func requestIDJSON(t *testing.T, id protocol.RequestID) string {
	t.Helper()
	data, err := json.Marshal(id)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

func commandApprovalRequestParams() json.RawMessage {
	return json.RawMessage(`{"itemId":"item-1","startedAtMs":1,"threadId":"thread-1","turnId":"turn-1"}`)
}
