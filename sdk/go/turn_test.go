package codex

import (
	"context"
	"encoding/json"
	"testing"
)

func TestTurnHandleInjectsThreadAndTurnIdentity(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["turn/start"] = json.RawMessage(`{"turn":{"id":"turn-1","items":[],"status":"inProgress"}}`)
	transport.responses["turn/steer"] = json.RawMessage(`{"turnId":"turn-1"}`)

	thread := &Thread{client: client, id: "thread-1"}
	handle, err := thread.Turn(ctx, Text("inspect"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	assertRequestThreadID(t, requestParamsForMethod(t, transport, "turn/start"), "thread-1")
	if handle.ID() != "turn-1" {
		t.Fatalf("turn id = %q, want turn-1", handle.ID())
	}

	if err := handle.Steer(ctx, Text("prefer tests")); err != nil {
		t.Fatal(err)
	}
	steerParams := requestParamsForMethod(t, transport, "turn/steer")
	assertRequestThreadID(t, steerParams, "thread-1")
	assertRequestStringField(t, steerParams, "expectedTurnId", "turn-1")

	if err := handle.Interrupt(ctx); err != nil {
		t.Fatal(err)
	}
	interruptParams := requestParamsForMethod(t, transport, "turn/interrupt")
	assertRequestThreadID(t, interruptParams, "thread-1")
	assertRequestStringField(t, interruptParams, "turnId", "turn-1")
}

func assertRequestStringField(t *testing.T, params json.RawMessage, field string, want string) {
	t.Helper()
	var raw map[string]any
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	got, ok := raw[field].(string)
	if !ok || got != want {
		t.Fatalf("%s = %#v, want %q; params = %s", field, raw[field], want, params)
	}
}
