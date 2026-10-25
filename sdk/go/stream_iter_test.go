package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func filledStream(t *testing.T, notifications ...Notification) *NotificationStream {
	t.Helper()
	stream := newNotificationStream(len(notifications)+1, DefaultResourceStreamQueueBytes, nil)
	for _, notification := range notifications {
		if !stream.send(notification) {
			t.Fatal("test stream overflowed")
		}
	}
	return stream
}

func deltaNotification(text string) Notification {
	return Notification{Method: "item/agentMessage/delta", Payload: protocol.AgentMessageDeltaNotification{Delta: text}}
}

// R3.1: notifications come in order and a clean close ends the loop without an error.
func TestNotificationStreamAllYieldsInOrderAndEndsCleanly(t *testing.T) {
	stream := filledStream(t, deltaNotification("a"), deltaNotification("b"))
	_ = stream.Close()
	var methods []string
	for notification, err := range stream.All(context.Background()) {
		if err != nil {
			t.Fatalf("unexpected error %v", err)
		}
		methods = append(methods, notification.Payload.(protocol.AgentMessageDeltaNotification).Delta)
	}
	if len(methods) != 2 || methods[0] != "a" || methods[1] != "b" {
		t.Fatalf("yielded %v", methods)
	}
}

// R3.1: a stream that closes with an error yields it exactly once, after the events.
func TestNotificationStreamAllYieldsTerminalErrorOnce(t *testing.T) {
	stream := filledStream(t, deltaNotification("a"))
	overflow := &OverflowError{Reason: "test"}
	stream.closeWithError(overflow)
	var events, failures int
	for _, err := range stream.All(context.Background()) {
		if err != nil {
			failures++
			if !errors.Is(err, overflow) {
				t.Fatalf("err = %v", err)
			}
			continue
		}
		events++
	}
	// Events buffered before the close are still delivered ahead of the error.
	if events != 1 || failures != 1 {
		t.Fatalf("events = %d, errors = %d", events, failures)
	}
}

// R3.1: the ctx error ends the iteration.
func TestNotificationStreamAllStopsOnContextEnd(t *testing.T) {
	stream := newNotificationStream(1, DefaultResourceStreamQueueBytes, nil)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	var got error
	for _, err := range stream.All(ctx) {
		got = err
	}
	if !errors.Is(got, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", got)
	}
}

// R3.2: breaking out leaves the stream open for further reads.
func TestNotificationStreamAllBreakLeavesStreamOpen(t *testing.T) {
	stream := filledStream(t, deltaNotification("a"), deltaNotification("b"))
	for range stream.All(context.Background()) {
		break
	}
	notification, ok := stream.Next(context.Background())
	if !ok || notification.Payload.(protocol.AgentMessageDeltaNotification).Delta != "b" {
		t.Fatalf("after break: %+v, %v (err %v)", notification, ok, stream.Err())
	}
	var nilStream *NotificationStream
	for _, err := range nilStream.All(context.Background()) {
		var closed *ClosedError
		if !errors.As(err, &closed) {
			t.Fatalf("nil stream err = %v", err)
		}
	}
}

// R3.3: Payloads keeps only payloads of the requested type.
func TestPayloadsFiltersByType(t *testing.T) {
	stream := filledStream(t,
		deltaNotification("a"),
		Notification{Method: "turn/started", Payload: protocol.TurnStartedNotification{ThreadID: "thread-1"}},
		deltaNotification("b"),
	)
	stream.closeWithError(&OverflowError{Reason: "test"})
	var text string
	var failure error
	for delta, err := range Payloads[protocol.AgentMessageDeltaNotification](context.Background(), stream) {
		if err != nil {
			failure = err
			continue
		}
		text += delta.Delta
	}
	var overflow *OverflowError
	if text != "ab" || !errors.As(failure, &overflow) {
		t.Fatalf("text = %q, err = %v", text, failure)
	}
}

// R3.1: a turn stream ranges to the end of the turn.
func TestTurnStreamAllEndsAtTurnCompletion(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{})
	stream, err := handle.Stream(boundedContext(t))
	if err != nil {
		t.Fatal(err)
	}
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	var methods []string
	for notification, err := range stream.All(boundedContext(t)) {
		if err != nil {
			t.Fatal(err)
		}
		methods = append(methods, notification.Method)
	}
	if len(methods) != 2 || methods[1] != "turn/completed" {
		t.Fatalf("methods = %v", methods)
	}
}
