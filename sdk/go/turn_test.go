package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
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

func TestCollectRunResultRejectsStreamClosedBeforeTerminalNotification(t *testing.T) {
	stream := newNotificationStream(1, DefaultResourceStreamQueueBytes, nil)
	if err := stream.Close(); err != nil {
		t.Fatal(err)
	}

	limits, normalizeErr := normalizeLimits(ClientLimits{})
	if normalizeErr != nil {
		t.Fatal(normalizeErr)
	}
	_, err := collectRunResult(context.Background(), "turn-1", stream, limits)
	var decodeErr *DecodeError
	if !errors.As(err, &decodeErr) {
		t.Fatalf("err = %T(%v), want *DecodeError", err, err)
	}
}

func TestCollectRunResultLimitsStreamedItems(t *testing.T) {
	stream := newNotificationStream(2, DefaultResourceStreamQueueBytes, nil)
	for _, itemID := range []string{"item-1", "item-2"} {
		stream.send(Notification{
			RawParams: json.RawMessage(`{"item":{}}`),
			Payload: protocol.ItemCompletedNotification{
				ThreadID: "thread-1",
				TurnID:   "turn-1",
				Item:     protocol.ThreadItem{ID: protocol.SomeNonNull(itemID), TypeValue: "agentMessage"},
			},
		})
	}

	_, err := collectRunResult(context.Background(), "turn-1", stream, ClientLimits{
		MaxRunResultItems: 1,
		MaxRunResultBytes: DefaultMaxRunResultBytes,
	})
	var overflowErr *OverflowError
	if !errors.As(err, &overflowErr) {
		t.Fatalf("err = %T(%v), want *OverflowError", err, err)
	}
}

func TestCollectRunResultLimitsTerminalFallbackItems(t *testing.T) {
	stream := newNotificationStream(1, DefaultResourceStreamQueueBytes, nil)
	stream.send(Notification{
		RawParams: json.RawMessage(`{"turn":{"items":[{},{}]}}`),
		Payload: protocol.TurnCompletedNotification{
			ThreadID: "thread-1",
			Turn: protocol.Turn{
				ID:     "turn-1",
				Status: protocol.TurnStatusCompleted,
				Items: []protocol.ThreadItem{
					{ID: protocol.SomeNonNull("item-1"), TypeValue: "agentMessage"},
					{ID: protocol.SomeNonNull("item-2"), TypeValue: "agentMessage"},
				},
			},
		},
	})

	_, err := collectRunResult(context.Background(), "turn-1", stream, ClientLimits{
		MaxRunResultItems: 1,
		MaxRunResultBytes: DefaultMaxRunResultBytes,
	})
	var overflowErr *OverflowError
	if !errors.As(err, &overflowErr) {
		t.Fatalf("err = %T(%v), want *OverflowError", err, err)
	}
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
