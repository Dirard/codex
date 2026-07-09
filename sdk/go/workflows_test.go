package codex

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

func TestThreadRunCollectsFinalResponseAndTokenUsage(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{Model: "test-model"})
	if err != nil {
		t.Fatal(err)
	}
	outputSchema, err := JSONSchema("answer", ObjectSchema(map[string]JSONSchemaSpec{
		"value": StringSchema(),
	}, "value"))
	if err != nil {
		t.Fatal(err)
	}

	resultCh := make(chan struct {
		result *RunResult
		err    error
	}, 1)
	go func() {
		result, err := thread.Run(context.Background(), Text("hello"), TurnOptions{
			AdditionalContext: map[string]protocol.AdditionalContextEntry{
				"note": {Kind: protocol.AdditionalContextKindUntrusted, Value: "short"},
			},
			OutputSchema: outputSchema,
		})
		resultCh <- struct {
			result *RunResult
			err    error
		}{result: result, err: err}
	}()

	waitForMethod(t, transport, "turn/start")
	assertTurnStartOutputSchema(t, requestParamsForMethod(t, transport, "turn/start"))
	transport.deliverNotification("thread/tokenUsage/updated", mustJSON(t, protocol.ThreadTokenUsageUpdatedNotification{
		ThreadID: "thread-1",
		TurnID:   "turn-1",
		TokenUsage: protocol.ThreadTokenUsage{
			Last:  protocol.TokenUsageBreakdown{TotalTokens: 7},
			Total: protocol.TokenUsageBreakdown{TotalTokens: 11},
		},
	}), nil)
	transport.deliverNotification("item/completed", mustJSON(t, protocol.ItemCompletedNotification{
		ThreadID: "thread-1",
		TurnID:   "turn-1",
		Item:     protocol.ThreadItem{ID: protocol.SomeNonNull("item-1"), TypeValue: "agentMessage", Text: protocol.SomeNonNull("commentary"), Phase: protocol.Some(protocol.MessagePhaseCommentary)},
	}), nil)
	transport.deliverNotification("item/completed", mustJSON(t, protocol.ItemCompletedNotification{
		ThreadID: "thread-1",
		TurnID:   "turn-1",
		Item:     protocol.ThreadItem{ID: protocol.SomeNonNull("item-2"), TypeValue: "agentMessage", Text: protocol.SomeNonNull("final"), Phase: protocol.Some(protocol.MessagePhaseFinalAnswer)},
	}), nil)
	transport.deliverNotification("turn/completed", mustJSON(t, protocol.TurnCompletedNotification{
		ThreadID: "thread-1",
		Turn: protocol.Turn{
			ID:     "turn-1",
			Status: protocol.TurnStatusCompleted,
			Items:  []protocol.ThreadItem{},
		},
	}), nil)

	result := receiveRunResult(t, resultCh)
	if result.TurnID != "turn-1" || result.Status != protocol.TurnStatusCompleted {
		t.Fatalf("result = %#v", result)
	}
	if result.FinalResponse != "final" {
		t.Fatalf("FinalResponse = %q, want final", result.FinalResponse)
	}
	if len(result.Items) != 2 {
		t.Fatalf("items = %#v, want 2 collected item/completed payloads", result.Items)
	}
	usage, ok := result.TokenUsage.Value()
	if !ok || usage.Total.TotalTokens != 11 {
		t.Fatalf("TokenUsage = %#v, %v; want total 11", usage, ok)
	}
}

func TestFinalResponsePrefersFinalAgentMessage(t *testing.T) {
	items := []protocol.ThreadItem{
		{
			ID:        protocol.SomeNonNull("fallback-before-final"),
			TypeValue: "agentMessage",
			Text:      protocol.SomeNonNull("fallback before final"),
		},
		{
			ID:        protocol.SomeNonNull("final"),
			TypeValue: "agentMessage",
			Text:      protocol.SomeNonNull("final answer"),
			Phase:     protocol.Some(protocol.MessagePhaseFinalAnswer),
		},
		{
			ID:        protocol.SomeNonNull("fallback-after-final"),
			TypeValue: "agentMessage",
			Text:      protocol.SomeNonNull("fallback after final"),
		},
		{
			ID:        protocol.SomeNonNull("user-after-final"),
			TypeValue: "userMessage",
			Text:      protocol.SomeNonNull("user text after final"),
		},
		{
			ID:        protocol.SomeNonNull("commentary-after-final"),
			TypeValue: "agentMessage",
			Text:      protocol.SomeNonNull("commentary after final"),
			Phase:     protocol.Some(protocol.MessagePhaseCommentary),
		},
	}

	if got := finalResponseFromItems(items); got != "final answer" {
		t.Fatalf("final response = %q, want explicit final agent message", got)
	}
}

func TestClientNotificationsReceivesUnknownAndKnownRoutedNotifications(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	stream, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	transport.deliverNotification("future/event", json.RawMessage(`{"value":1}`), json.RawMessage(`{"traceId":"trace-1"}`))
	unknown := receiveNotification(t, stream)
	payload, ok := unknown.Payload.(UnknownNotification)
	if !ok {
		t.Fatalf("unknown payload = %#v, want UnknownNotification", unknown.Payload)
	}
	if payload.Method != "future/event" || string(payload.Params) != `{"value":1}` || string(payload.Trace) != `{"traceId":"trace-1"}` {
		t.Fatalf("unknown payload = %#v", payload)
	}

	transport.deliverNotification("turn/completed", mustJSON(t, protocol.TurnCompletedNotification{
		ThreadID: "thread-1",
		Turn: protocol.Turn{
			ID:     "turn-1",
			Status: protocol.TurnStatusCompleted,
			Items:  []protocol.ThreadItem{},
		},
	}), nil)
	known := receiveNotification(t, stream)
	if known.Method != "turn/completed" {
		t.Fatalf("known method = %q", known.Method)
	}
	if _, ok := known.Payload.(protocol.TurnCompletedNotification); !ok {
		t.Fatalf("known payload = %#v, want TurnCompletedNotification", known.Payload)
	}
}

func TestLoginWithAPIKeyRedactsReturnedErrors(t *testing.T) {
	const secret = "sk-test-secret-value"
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.errors["account/login/start"] = &jsonrpc.RPCError{
		Code:    -32000,
		Message: "server rejected API key " + secret,
		Data:    json.RawMessage(`{"echoedKey":"` + secret + `"}`),
	}
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	err = client.Accounts.LoginWithAPIKey(context.Background(), APIKey(secret))
	if err == nil {
		t.Fatal("expected API-key login error")
	}
	if strings.Contains(err.Error(), secret) {
		t.Fatalf("API-key login error leaked secret: %v", err)
	}
	if unwrapped := errors.Unwrap(err); unwrapped == nil || strings.Contains(unwrapped.Error(), secret) {
		t.Fatalf("API-key login unwrapped error leaked secret: %v", unwrapped)
	}
	var rpcErr *jsonrpc.RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatal("API-key login error should remain inspectable as RPCError")
	}
	if rpcErr.Code != -32000 {
		t.Fatalf("RPCError code = %d, want -32000", rpcErr.Code)
	}
	if strings.Contains(rpcErr.Message, secret) || strings.Contains(string(rpcErr.Data), secret) {
		t.Fatalf("RPCError leaked secret after redaction: %#v", rpcErr)
	}
}

func TestDeviceCodeLoginHandleExposesUserPromptFields(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.responses["account/login/start"] = mustJSON(t, protocol.LoginAccountResponse{
		TypeValue:       "chatgptDeviceCode",
		LoginID:         protocol.SomeNonNull("login-1"),
		VerificationURL: protocol.SomeNonNull("https://example.com/device"),
		UserCode:        protocol.SomeNonNull("ABCD-EFGH"),
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	login, err := client.Accounts.StartDeviceCodeLogin(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	assertMethod(t, transport.lastFrame(t), "account/login/start")
	if got := login.ID(); got != "login-1" {
		t.Fatalf("login ID = %q, want login-1", got)
	}
	if got := login.VerificationURL(); got != "https://example.com/device" {
		t.Fatalf("verification URL = %q, want https://example.com/device", got)
	}
	if got := login.UserCode(); got != "ABCD-EFGH" {
		t.Fatalf("user code = %q, want ABCD-EFGH", got)
	}

	transport.deliverNotification("account/login/completed", mustJSON(t, protocol.AccountLoginCompletedNotification{
		LoginID: protocol.Some("login-1"),
		Success: true,
	}), nil)
	result, err := login.Wait(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if result.LoginID != "login-1" || !result.Success {
		t.Fatalf("result = %#v", result)
	}
}

func TestTurnStreamReceivesBufferedAndInterleavedNotifications(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	handle, err := thread.Turn(context.Background(), Text("hello"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	waitForMethod(t, transport, "turn/start")

	transport.deliverNotification("item/agentMessage/delta", json.RawMessage(`{"threadId":"thread-1","turnId":"other-turn","itemId":"item-other","delta":"other"}`), nil)
	transport.deliverNotification("item/agentMessage/delta", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","delta":"mine"}`), nil)

	stream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	notification, ok := stream.Next(context.Background())
	if !ok {
		t.Fatalf("stream closed: %v", stream.Err())
	}
	if notification.Method != "item/agentMessage/delta" {
		t.Fatalf("method = %q", notification.Method)
	}
	payload, ok := notification.Payload.(protocol.AgentMessageDeltaNotification)
	if !ok || payload.TurnID != "turn-1" {
		t.Fatalf("payload = %#v, %v", notification.Payload, ok)
	}

	transport.deliverNotification("item/plan/delta", mustJSON(t, protocol.PlanDeltaNotification{
		ThreadID: "thread-1",
		TurnID:   "turn-1",
		ItemID:   "item-plan",
		Delta:    "step",
	}), nil)
	notification, ok = stream.Next(context.Background())
	if !ok {
		t.Fatalf("stream closed before plan delta: %v", stream.Err())
	}
	plan, ok := notification.Payload.(protocol.PlanDeltaNotification)
	if !ok || plan.Delta != "step" {
		t.Fatalf("plan payload = %#v, %v", notification.Payload, ok)
	}

	transport.deliverNotification("error", mustJSON(t, protocol.ErrorNotification{
		Error:    protocol.TurnError{Message: "model failed"},
		ThreadID: "thread-1",
		TurnID:   "turn-1",
	}), nil)
	notification, ok = stream.Next(context.Background())
	if !ok {
		t.Fatalf("stream closed before turn error: %v", stream.Err())
	}
	turnError, ok := notification.Payload.(protocol.ErrorNotification)
	if !ok || turnError.Error.Message != "model failed" {
		t.Fatalf("error payload = %#v, %v", notification.Payload, ok)
	}

	transport.deliverNotification("model/rerouted", mustJSON(t, protocol.ModelReroutedNotification{
		FromModel: "gpt-5.4",
		Reason:    protocol.ModelRerouteReasonHighRiskCyberActivity,
		ThreadID:  "thread-1",
		ToModel:   "gpt-5.5",
		TurnID:    "turn-1",
	}), nil)
	notification, ok = stream.Next(context.Background())
	if !ok {
		t.Fatalf("stream closed before model reroute: %v", stream.Err())
	}
	rerouted, ok := notification.Payload.(protocol.ModelReroutedNotification)
	if !ok || rerouted.TurnID != "turn-1" {
		t.Fatalf("model payload = %#v, %v", notification.Payload, ok)
	}
}

func TestTurnSteerAdditionalContextLimitIsCheckedBeforeWrite(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Limits: ClientLimits{
			MaxAdditionalContextEntries:    1,
			MaxAdditionalContextKeyBytes:   8,
			MaxAdditionalContextValueBytes: 3,
			MaxAdditionalContextTotalBytes: 8,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	handle, err := thread.Turn(context.Background(), Text("hello"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	before := len(transport.sentFrames())
	err = handle.Steer(context.Background(), Text("more"), SteerOptions{
		AdditionalContext: map[string]protocol.AdditionalContextEntry{
			"note": {Kind: protocol.AdditionalContextKindUntrusted, Value: "1234"},
		},
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) || !strings.Contains(configErr.Reason, "value") {
		t.Fatalf("err = %T %v, want additionalContext value ConfigError", err, err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("over-limit steer additionalContext reached transport")
	}
}

func TestHighLevelAdditionalContextUsesProtocolCapsWhenLimitsAreRaised(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Limits: ClientLimits{
			MaxAdditionalContextEntries:    protocol.MaxAdditionalContextEntries + 1,
			MaxAdditionalContextKeyBytes:   protocol.MaxAdditionalContextKeyBytes + 1,
			MaxAdditionalContextValueBytes: protocol.MaxAdditionalContextValueBytes + 1,
			MaxAdditionalContextTotalBytes: protocol.MaxAdditionalContextTotalBytes + 1,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	overLimitContext := map[string]protocol.AdditionalContextEntry{
		"note": {
			Kind:  protocol.AdditionalContextKindUntrusted,
			Value: strings.Repeat("x", protocol.MaxAdditionalContextValueBytes+1),
		},
	}

	framesBeforeTurn := len(transport.sentFrames())
	_, err = thread.Turn(context.Background(), Text("hello"), TurnOptions{AdditionalContext: overLimitContext})
	var configErr *ConfigError
	if !errors.As(err, &configErr) || !strings.Contains(configErr.Reason, "value") {
		t.Fatalf("turn err = %T %v, want additionalContext value ConfigError", err, err)
	}
	if len(transport.sentFrames()) != framesBeforeTurn {
		t.Fatal("over-protocol-cap turn additionalContext reached transport")
	}

	handle, err := thread.Turn(context.Background(), Text("hello"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	framesBeforeSteer := len(transport.sentFrames())
	err = handle.Steer(context.Background(), Text("more"), SteerOptions{AdditionalContext: overLimitContext})
	if !errors.As(err, &configErr) || !strings.Contains(configErr.Reason, "value") {
		t.Fatalf("steer err = %T %v, want additionalContext value ConfigError", err, err)
	}
	if len(transport.sentFrames()) != framesBeforeSteer {
		t.Fatal("over-protocol-cap steer additionalContext reached transport")
	}
}

func TestAccountLoginHandleWaitsForCompletion(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.responses["account/login/start"] = mustJSON(t, protocol.LoginAccountResponse{
		TypeValue: "chatgpt",
		LoginID:   protocol.SomeNonNull("login-1"),
		AuthURL:   protocol.SomeNonNull("https://example.test/auth"),
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.Accounts.StartChatGPTLogin(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if handle.ID() != "login-1" || handle.AuthURL() != "https://example.test/auth" {
		t.Fatalf("handle id/url = %q/%q", handle.ID(), handle.AuthURL())
	}

	resultCh := make(chan struct {
		result *LoginResult
		err    error
	}, 1)
	go func() {
		result, err := handle.Wait(context.Background())
		resultCh <- struct {
			result *LoginResult
			err    error
		}{result: result, err: err}
	}()
	transport.deliverNotification("account/login/completed", mustJSON(t, protocol.AccountLoginCompletedNotification{
		LoginID: protocol.Some("login-1"),
		Success: true,
	}), nil)

	select {
	case item := <-resultCh:
		if item.err != nil {
			t.Fatal(item.err)
		}
		if item.result.LoginID != "login-1" || !item.result.Success {
			t.Fatalf("result = %#v", item.result)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for login result")
	}
}

func TestAccountLoginHandleMissingLoginIDCompletionFailsClosed(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.responses["account/login/start"] = mustJSON(t, protocol.LoginAccountResponse{
		TypeValue: "chatgpt",
		LoginID:   protocol.SomeNonNull("login-1"),
		AuthURL:   protocol.SomeNonNull("https://example.test/auth"),
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.Accounts.StartChatGPTLogin(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("account/login/completed", json.RawMessage(`{"success":true}`), nil)

	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err = handle.Wait(ctx)
	var unsupported *UnsupportedError
	if !errors.As(err, &unsupported) || !strings.Contains(unsupported.Reason, "loginId") {
		t.Fatalf("err = %T %v, want missing-loginId *UnsupportedError", err, err)
	}
}

func TestMCPOAuthHandleWaitsForCompletion(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["mcpServer/oauth/login"] = mustJSON(t, protocol.McpServerOauthLoginResponse{
		AuthorizationURL: "https://example.test/oauth",
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.MCP.OAuthLogin(context.Background(), MCPOAuthLoginOptions{Name: "server-1", ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	if handle.AuthorizationURL() != "https://example.test/oauth" {
		t.Fatalf("AuthorizationURL = %q", handle.AuthorizationURL())
	}

	resultCh := make(chan struct {
		result *MCPOAuthResult
		err    error
	}, 1)
	go func() {
		result, err := handle.Wait(context.Background())
		resultCh <- struct {
			result *MCPOAuthResult
			err    error
		}{result: result, err: err}
	}()
	transport.deliverNotification("mcpServer/oauthLogin/completed", mustJSON(t, protocol.McpServerOauthLoginCompletedNotification{
		Name:     "server-1",
		Success:  true,
		ThreadID: protocol.Some("thread-2"),
	}), nil)
	select {
	case item := <-resultCh:
		t.Fatalf("wrong-thread completion returned result=%#v err=%v", item.result, item.err)
	case <-time.After(100 * time.Millisecond):
	}

	transport.deliverNotification("mcpServer/oauthLogin/completed", mustJSON(t, protocol.McpServerOauthLoginCompletedNotification{
		Name:     "server-1",
		Success:  true,
		ThreadID: protocol.Some("thread-1"),
	}), nil)

	select {
	case item := <-resultCh:
		if item.err != nil {
			t.Fatal(item.err)
		}
		if item.result.Name != "server-1" || !item.result.Success {
			t.Fatalf("result = %#v", item.result)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for MCP OAuth result")
	}
}

func TestMCPOAuthHandleAcceptsNameMatchedCompletionWithoutThreadID(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["mcpServer/oauth/login"] = mustJSON(t, protocol.McpServerOauthLoginResponse{
		AuthorizationURL: "https://example.test/oauth",
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.MCP.OAuthLogin(context.Background(), MCPOAuthLoginOptions{Name: "server-1", ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	resultCh := make(chan struct {
		result *MCPOAuthResult
		err    error
	}, 1)
	go func() {
		result, err := handle.Wait(context.Background())
		resultCh <- struct {
			result *MCPOAuthResult
			err    error
		}{result: result, err: err}
	}()

	transport.deliverNotification("mcpServer/oauthLogin/completed", mustJSON(t, protocol.McpServerOauthLoginCompletedNotification{
		Name:    "server-1",
		Success: true,
	}), nil)

	select {
	case item := <-resultCh:
		if item.err != nil {
			t.Fatal(item.err)
		}
		if item.result.Name != "server-1" || !item.result.Success {
			t.Fatalf("result = %#v", item.result)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for MCP OAuth result without thread ID")
	}
}

func TestReviewHandleWaitsForTurnCompletion(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["review/start"] = mustJSON(t, protocol.ReviewStartResponse{
		ReviewThreadID: "review-thread-1",
		Turn: protocol.Turn{
			ID:     "review-turn-1",
			Items:  []protocol.ThreadItem{},
			Status: protocol.TurnStatusInProgress,
		},
	})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.Reviews.Start(context.Background(), ReviewStartOptions{
		ThreadID: "thread-1",
		Target:   UncommittedChangesReviewTarget(),
	})
	if err != nil {
		t.Fatal(err)
	}
	if handle.ReviewThreadID() != "review-thread-1" || handle.TurnID() != "review-turn-1" {
		t.Fatalf("review handle = %q/%q", handle.ReviewThreadID(), handle.TurnID())
	}

	resultCh := make(chan struct {
		result *ReviewResult
		err    error
	}, 1)
	go func() {
		result, err := handle.Wait(context.Background())
		resultCh <- struct {
			result *ReviewResult
			err    error
		}{result: result, err: err}
	}()
	transport.deliverNotification("turn/completed", mustJSON(t, protocol.TurnCompletedNotification{
		ThreadID: "other-thread",
		Turn: protocol.Turn{
			ID:     "review-turn-1",
			Items:  []protocol.ThreadItem{},
			Status: protocol.TurnStatusCompleted,
		},
	}), nil)
	select {
	case item := <-resultCh:
		t.Fatalf("wrong-thread review completion returned result=%#v err=%v", item.result, item.err)
	case <-time.After(100 * time.Millisecond):
	}
	transport.deliverNotification("turn/completed", mustJSON(t, protocol.TurnCompletedNotification{
		ThreadID: "review-thread-1",
		Turn: protocol.Turn{
			ID:     "review-turn-1",
			Items:  []protocol.ThreadItem{},
			Status: protocol.TurnStatusCompleted,
		},
	}), nil)

	result := receiveRunResult(t, resultCh)
	if result.TurnID != "review-turn-1" || result.Status != protocol.TurnStatusCompleted {
		t.Fatalf("result = %#v", result)
	}
}

func TestReviewStartRejectsMalformedHandleIDs(t *testing.T) {
	tests := []struct {
		name          string
		response      protocol.ReviewStartResponse
		reasonSnippet string
	}{
		{
			name: "missing-review-thread-id",
			response: protocol.ReviewStartResponse{
				Turn: protocol.Turn{
					ID:     "review-turn-1",
					Items:  []protocol.ThreadItem{},
					Status: protocol.TurnStatusInProgress,
				},
			},
			reasonSnippet: "reviewThreadId",
		},
		{
			name: "missing-turn-id",
			response: protocol.ReviewStartResponse{
				ReviewThreadID: "review-thread-1",
				Turn: protocol.Turn{
					Items:  []protocol.ThreadItem{},
					Status: protocol.TurnStatusInProgress,
				},
			},
			reasonSnippet: "turn.id",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newWorkflowTransport(t)
			transport.responses["review/start"] = mustJSON(t, tt.response)
			client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })

			handle, err := client.Reviews.Start(context.Background(), ReviewStartOptions{
				ThreadID: "thread-1",
				Target:   UncommittedChangesReviewTarget(),
			})
			if handle != nil {
				t.Fatalf("handle = %#v, want nil", handle)
			}
			var unsupported *UnsupportedError
			if !errors.As(err, &unsupported) || !strings.Contains(unsupported.Reason, tt.reasonSnippet) {
				t.Fatalf("err = %T(%v), want *UnsupportedError containing %q", err, err, tt.reasonSnippet)
			}
		})
	}
}

func TestHighLevelWorkflowsRejectRawOnlyMode(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["remoteControl/pairing/status"] = mustJSON(t, protocol.RemoteControlPairingStatusResponse{Claimed: true})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport, Mode: ClientModeRawOnly})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	ctx := context.Background()
	tests := []struct {
		name   string
		method string
		call   func() error
	}{
		{
			name:   "thread start",
			method: "thread/start",
			call: func() error {
				_, err := client.Threads.Start(ctx, ThreadStartOptions{})
				return err
			},
		},
		{
			name:   "thread resume",
			method: "thread/resume",
			call: func() error {
				_, err := client.Threads.Resume(ctx, ThreadResumeOptions{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name:   "thread fork",
			method: "thread/fork",
			call: func() error {
				_, err := client.Threads.Fork(ctx, ThreadForkOptions{ThreadID: "thread-1"})
				return err
			},
		},
		{
			name:   "remote control pairing",
			method: "remoteControl/pairing/start",
			call: func() error {
				_, _, err := client.RemoteControl.StartPairing(ctx, RemoteControlPairingOptions{ManualCode: true})
				return err
			},
		},
		{
			name:   "remote control pairing status",
			method: "remoteControl/pairing/status",
			call: func() error {
				_, err := client.RemoteControl.PairingStatus(ctx, "pair-code")
				return err
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.call()
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T, want *ConfigError", err)
			}
			if methodWasSent(t, transport, tt.method) {
				t.Fatalf("%s was sent in raw-only mode", tt.method)
			}
		})
	}
	_, err = client.Raw().RemoteControlPairingStatus(ctx, protocol.RemoteControlPairingStatusParams{
		PairingCode: protocol.Some("pair-code"),
	})
	if err != nil {
		t.Fatalf("raw remoteControl/pairing/status err = %v", err)
	}
	if !methodWasSent(t, transport, "remoteControl/pairing/status") {
		t.Fatal("raw remoteControl/pairing/status was not sent in raw-only mode")
	}
}

func TestRawOnlyModeAllowsSafeAccountWrappers(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.responses["account/read"] = json.RawMessage(`{"requiresOpenaiAuth":false}`)
	transport.responses["account/login/start"] = json.RawMessage(`{"type":"apiKey"}`)
	transport.responses["account/usage/read"] = json.RawMessage(`{"summary":{}}`)
	transport.responses["account/rateLimits/read"] = json.RawMessage(`{"rateLimits":{}}`)
	transport.responses["account/logout"] = json.RawMessage(`{}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport, Mode: ClientModeRawOnly})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	if _, err := client.Accounts.Read(context.Background(), true); err != nil {
		t.Fatal(err)
	}
	assertAccountReadRefreshToken(t, requestParamsForMethod(t, transport, "account/read"))
	if err := client.Accounts.LoginWithAPIKey(context.Background(), APIKey("test-api-key")); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Accounts.Usage(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Accounts.RateLimits(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := client.Accounts.Logout(context.Background()); err != nil {
		t.Fatal(err)
	}
	for _, method := range []string{"account/read", "account/login/start", "account/usage/read", "account/rateLimits/read", "account/logout"} {
		if !methodWasSent(t, transport, method) {
			t.Fatalf("%s was not sent through raw-only safe account wrapper", method)
		}
	}
}

func TestRawOnlyNotificationOptOutBlocksDependentWorkflowButAllowsRaw(t *testing.T) {
	transport := newWorkflowTransport(t)
	transport.responses["account/usage/read"] = json.RawMessage(`{"summary":{}}`)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:           transport,
		Mode:                ClientModeRawOnly,
		NotificationOptOuts: NotificationOptOuts{Methods: []string{"turn/completed"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	_, err = client.Threads.Start(context.Background(), ThreadStartOptions{})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if methodWasSent(t, transport, "thread/start") {
		t.Fatal("thread/start was sent after raw-only dependent workflow opt-out")
	}

	if _, err := client.Raw().AccountUsageRead(context.Background()); err != nil {
		t.Fatalf("raw account usage should remain available: %v", err)
	}
	waitForMethod(t, transport, "account/usage/read")
}

func TestAccountsThinWrappersUseHighLevelClient(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newWorkflowTransport(t)
	transport.responses["account/read"] = json.RawMessage(`{"requiresOpenaiAuth":false}`)
	transport.responses["account/usage/read"] = json.RawMessage(`{"summary":{}}`)
	transport.responses["account/rateLimits/read"] = json.RawMessage(`{"rateLimits":{}}`)
	transport.responses["account/logout"] = json.RawMessage(`{}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	if _, err := client.Accounts.Read(context.Background(), true); err != nil {
		t.Fatal(err)
	}
	assertAccountReadRefreshToken(t, requestParamsForMethod(t, transport, "account/read"))
	if _, err := client.Accounts.Usage(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Accounts.RateLimits(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := client.Accounts.Logout(context.Background()); err != nil {
		t.Fatal(err)
	}
	for _, method := range []string{"account/read", "account/usage/read", "account/rateLimits/read", "account/logout"} {
		if !methodWasSent(t, transport, method) {
			t.Fatalf("%s was not sent through high-level account wrapper", method)
		}
	}
}

func newWorkflowTransport(t *testing.T) *scriptedTransport {
	t.Helper()
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["thread/start"] = mustJSON(t, protocol.ThreadStartResponse{
		ApprovalPolicy:    protocol.AskForApproval{},
		ApprovalsReviewer: protocol.ApprovalsReviewerAutoReview,
		Cwd:               protocol.AbsolutePathBuf("/tmp"),
		Model:             "test-model",
		ModelProvider:     "test-provider",
		Sandbox:           protocol.SandboxPolicy{TypeValue: "dangerFullAccess"},
		Thread: protocol.Thread{
			CliVersion:    "0.0.0-dev",
			CreatedAt:     1,
			Cwd:           protocol.AbsolutePathBuf("/tmp"),
			ID:            "thread-1",
			ModelProvider: "test-provider",
			Preview:       "",
			SessionID:     "session-1",
			Source:        protocol.SessionSource{},
			Status:        protocol.ThreadStatus{TypeValue: "idle"},
			Turns:         []protocol.Turn{},
			UpdatedAt:     1,
		},
	})
	transport.responses["turn/start"] = mustJSON(t, protocol.TurnStartResponse{
		Turn: protocol.Turn{
			ID:     "turn-1",
			Items:  []protocol.ThreadItem{},
			Status: protocol.TurnStatusInProgress,
		},
	})
	transport.responses["turn/steer"] = mustJSON(t, protocol.TurnSteerResponse{TurnID: "turn-1"})
	return transport
}

func waitForMethod(t *testing.T, transport *scriptedTransport, method string) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		for _, frame := range transport.sentFrames() {
			if methodFromFrame(t, frame) == method {
				return
			}
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("method %q was not sent", method)
}

func assertAccountReadRefreshToken(t *testing.T, params json.RawMessage) {
	t.Helper()
	var raw struct {
		RefreshToken bool `json:"refreshToken"`
	}
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if !raw.RefreshToken {
		t.Fatal("account/read refreshToken = false, want true")
	}
}

func methodWasSent(t *testing.T, transport *scriptedTransport, method string) bool {
	t.Helper()
	for _, frame := range transport.sentFrames() {
		if methodFromFrame(t, frame) == method {
			return true
		}
	}
	return false
}

func requestParamsForMethod(t *testing.T, transport *scriptedTransport, method string) json.RawMessage {
	t.Helper()
	frames := transport.sentFrames()
	for i := len(frames) - 1; i >= 0; i-- {
		var request struct {
			Method string          `json:"method"`
			Params json.RawMessage `json:"params"`
		}
		if err := json.Unmarshal(frames[i], &request); err != nil {
			t.Fatal(err)
		}
		if request.Method == method {
			return request.Params
		}
	}
	t.Fatalf("method %q was not sent", method)
	return nil
}

func assertTurnStartOutputSchema(t *testing.T, params json.RawMessage) {
	t.Helper()
	var raw struct {
		OutputSchema json.RawMessage `json:"outputSchema"`
	}
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if len(raw.OutputSchema) == 0 {
		t.Fatal("turn/start omitted outputSchema")
	}
	var schema struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(raw.OutputSchema, &schema); err != nil {
		t.Fatal(err)
	}
	if schema.Name != "answer" {
		t.Fatalf("output schema name = %q, want answer", schema.Name)
	}
}

func receiveRunResult(t *testing.T, ch <-chan struct {
	result *RunResult
	err    error
}) *RunResult {
	t.Helper()
	select {
	case item := <-ch:
		if item.err != nil {
			t.Fatal(item.err)
		}
		return item.result
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for run result")
		return nil
	}
}

func receiveNotification(t *testing.T, stream *NotificationStream) Notification {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	notification, ok := stream.Next(ctx)
	if !ok {
		t.Fatalf("stream closed: %v", stream.Err())
	}
	return notification
}

func mustJSON(t *testing.T, value any) json.RawMessage {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return data
}
