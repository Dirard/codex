package codex

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestFuzzyFileSearchSearchWrapper(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["fuzzyFileSearch"] = json.RawMessage(`{"files":[]}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	_, err = client.FuzzyFileSearch.Search(context.Background(), protocol.FuzzyFileSearchParams{
		Query: "main.go",
		Roots: []string{"/repo"},
	})
	if err != nil {
		t.Fatal(err)
	}
	assertMethod(t, transport.lastFrame(t), "fuzzyFileSearch")
}

func TestFuzzyFileSearchSessionInjectsOwnedIdentity(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	session, err := client.FuzzyFileSearch.StartSession(context.Background(), FuzzySearchSessionOptions{Roots: []string{"/repo"}})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(session.ID(), "go-fuzzy-file-search-") {
		t.Fatalf("session ID = %q", session.ID())
	}
	startFrame := transport.lastFrame(t)
	assertMethod(t, startFrame, "fuzzyFileSearch/sessionStart")
	assertRequestStringParam(t, paramsFromFrame(t, startFrame), "sessionId", session.ID())
	assertRequestStringSliceParam(t, paramsFromFrame(t, startFrame), "roots", []string{"/repo"})

	transport.errors["fuzzyFileSearch/sessionUpdate"] = &RPCError{Code: -32000, Message: "captured update"}
	err = session.Update(context.Background(), FuzzySearchUpdate{Query: "main.go"})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	updateFrame := transport.lastFrame(t)
	assertMethod(t, updateFrame, "fuzzyFileSearch/sessionUpdate")
	assertRequestStringParam(t, paramsFromFrame(t, updateFrame), "sessionId", session.ID())
	assertRequestStringParam(t, paramsFromFrame(t, updateFrame), "query", "main.go")

	delete(transport.errors, "fuzzyFileSearch/sessionUpdate")
	err = session.Close(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	closeFrame := transport.lastFrame(t)
	assertMethod(t, closeFrame, "fuzzyFileSearch/sessionStop")
	assertRequestStringParam(t, paramsFromFrame(t, closeFrame), "sessionId", session.ID())
}

func TestFuzzyFileSearchSessionIDsAreUniqueAcrossClients(t *testing.T) {
	firstTransport := newScriptedInitializedTransport(t, nil)
	firstClient, err := NewClient(context.Background(), ClientConfig{Transport: firstTransport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = firstClient.Close() })

	secondTransport := newScriptedInitializedTransport(t, nil)
	secondClient, err := NewClient(context.Background(), ClientConfig{Transport: secondTransport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = secondClient.Close() })

	firstSession, err := firstClient.FuzzyFileSearch.StartSession(context.Background(), FuzzySearchSessionOptions{Roots: []string{"/repo"}})
	if err != nil {
		t.Fatal(err)
	}
	secondSession, err := secondClient.FuzzyFileSearch.StartSession(context.Background(), FuzzySearchSessionOptions{Roots: []string{"/repo"}})
	if err != nil {
		t.Fatal(err)
	}
	if firstSession.ID() == secondSession.ID() {
		t.Fatalf("session IDs collided across clients: %q", firstSession.ID())
	}
}

func TestFuzzyFileSearchSessionStreamRoutesBySessionID(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	session, err := client.FuzzyFileSearch.StartSession(context.Background(), FuzzySearchSessionOptions{Roots: []string{"/repo"}})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := session.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = stream.Close() })

	transport.deliverNotification("fuzzyFileSearch/sessionUpdated", json.RawMessage(`{"sessionId":"other","query":"ignored","files":[]}`), nil)
	payload, err := json.Marshal(protocol.FuzzyFileSearchSessionUpdatedNotification{
		SessionID: session.ID(),
		Query:     "main.go",
		Files:     []protocol.FuzzyFileSearchResult{},
	})
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("fuzzyFileSearch/sessionUpdated", payload, nil)

	notification := nextTestNotification(t, stream)
	if notification.Method != "fuzzyFileSearch/sessionUpdated" {
		t.Fatalf("notification method = %q", notification.Method)
	}
	typed, ok := notification.Payload.(protocol.FuzzyFileSearchSessionUpdatedNotification)
	if !ok {
		t.Fatalf("payload = %T, want FuzzyFileSearchSessionUpdatedNotification", notification.Payload)
	}
	if typed.SessionID != session.ID() {
		t.Fatalf("sessionId = %q, want %q", typed.SessionID, session.ID())
	}
}

func TestFuzzyFileSearchSessionCompletedKeepsHandleActiveUntilClose(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	session, err := client.FuzzyFileSearch.StartSession(context.Background(), FuzzySearchSessionOptions{Roots: []string{"/repo"}})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := session.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}

	transport.deliverNotification("fuzzyFileSearch/sessionCompleted", mustJSON(t, protocol.FuzzyFileSearchSessionCompletedNotification{
		SessionID: session.ID(),
	}), nil)

	notification := nextTestNotification(t, stream)
	if notification.Method != "fuzzyFileSearch/sessionCompleted" {
		t.Fatalf("terminal notification method = %q", notification.Method)
	}
	typed, ok := notification.Payload.(protocol.FuzzyFileSearchSessionCompletedNotification)
	if !ok {
		t.Fatalf("payload = %T, want FuzzyFileSearchSessionCompletedNotification", notification.Payload)
	}
	if typed.SessionID != session.ID() {
		t.Fatalf("sessionId = %q, want %q", typed.SessionID, session.ID())
	}
	expectClosedStream(t, stream)

	beforeUpdate := methodFrameCount(t, transport, "fuzzyFileSearch/sessionUpdate")
	if err := session.Update(context.Background(), FuzzySearchUpdate{Query: "after"}); err != nil {
		t.Fatalf("update after search completion: %v", err)
	}
	if got := methodFrameCount(t, transport, "fuzzyFileSearch/sessionUpdate"); got != beforeUpdate+1 {
		t.Fatalf("fuzzyFileSearch/sessionUpdate sent %d times, want %d", got, beforeUpdate+1)
	}
	updateParams := requestParamsForMethod(t, transport, "fuzzyFileSearch/sessionUpdate")
	assertRequestStringParam(t, updateParams, "sessionId", session.ID())
	assertRequestStringParam(t, updateParams, "query", "after")

	nextStream, err := session.Stream(context.Background())
	if err != nil {
		t.Fatalf("stream after search completion: %v", err)
	}
	_ = nextStream.Close()

	beforeStop := methodFrameCount(t, transport, "fuzzyFileSearch/sessionStop")
	if err := session.Close(context.Background()); err != nil {
		t.Fatalf("close after terminal completion: %v", err)
	}
	if got := methodFrameCount(t, transport, "fuzzyFileSearch/sessionStop"); got != beforeStop+1 {
		t.Fatalf("fuzzyFileSearch/sessionStop sent %d times, want %d", got, beforeStop+1)
	}
}

func TestFuzzyFileSearchSessionStableModeRejectsExperimentalStartBeforeWrite(t *testing.T) {
	transport := newScriptedInitializedTransport(t, stableInitializePayload())
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:    transport,
		ProtocolMode: ProtocolModeStable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	_, err = client.FuzzyFileSearch.StartSession(context.Background(), FuzzySearchSessionOptions{Roots: []string{"/repo"}})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T(%v), want *ConfigError", err, err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("experimental fuzzy session start reached transport in stable mode")
	}
}
