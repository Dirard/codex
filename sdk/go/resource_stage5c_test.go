package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"
)

func requestStringParam(t *testing.T, params json.RawMessage, field string) string {
	t.Helper()
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	value, ok := raw[field]
	if !ok {
		t.Fatalf("%s missing from params %s", field, params)
	}
	var got string
	if err := json.Unmarshal(value, &got); err != nil {
		t.Fatalf("%s = %s: %v", field, value, err)
	}
	return got
}

func assertRequestStringParam(t *testing.T, params json.RawMessage, field string, want string) {
	t.Helper()
	if got := requestStringParam(t, params, field); got != want {
		t.Fatalf("%s = %q, want %q; params = %s", field, got, want, params)
	}
}

func assertRequestStringSliceParam(t *testing.T, params json.RawMessage, field string, want []string) {
	t.Helper()
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	value, ok := raw[field]
	if !ok {
		t.Fatalf("%s missing from params %s", field, params)
	}
	var got []string
	if err := json.Unmarshal(value, &got); err != nil {
		t.Fatalf("%s = %s: %v", field, value, err)
	}
	if len(got) != len(want) {
		t.Fatalf("%s = %#v, want %#v", field, got, want)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Fatalf("%s = %#v, want %#v", field, got, want)
		}
	}
}

func assertRequestBoolParam(t *testing.T, params json.RawMessage, field string, want bool) {
	t.Helper()
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	value, ok := raw[field]
	if !ok {
		t.Fatalf("%s missing from params %s", field, params)
	}
	var got bool
	if err := json.Unmarshal(value, &got); err != nil {
		t.Fatalf("%s = %s: %v", field, value, err)
	}
	if got != want {
		t.Fatalf("%s = %v, want %v; params = %s", field, got, want, params)
	}
}

func assertConflictError(t *testing.T, err error) {
	t.Helper()
	var conflict *ConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("err = %T(%v), want *ConflictError", err, err)
	}
}

func nextTestNotification(t *testing.T, stream *NotificationStream) Notification {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	notification, ok := stream.Next(ctx)
	if !ok {
		t.Fatalf("stream closed before notification; err = %v", stream.Err())
	}
	return notification
}

func expectClosedStream(t *testing.T, stream *NotificationStream) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	notification, ok := stream.Next(ctx)
	if ok {
		t.Fatalf("stream produced unexpected notification after close: %#v", notification)
	}
	if errors.Is(stream.Err(), context.DeadlineExceeded) {
		t.Fatal("stream remained open")
	}
}

func assertMethodOrder(t *testing.T, transport *scriptedTransport, before string, after string) {
	t.Helper()
	beforeIndex := -1
	afterIndex := -1
	for index, frame := range transport.sentFrames() {
		switch methodFromFrame(t, frame) {
		case before:
			if beforeIndex == -1 {
				beforeIndex = index
			}
		case after:
			if afterIndex == -1 {
				afterIndex = index
			}
		}
	}
	if beforeIndex == -1 {
		t.Fatalf("method %q was not sent", before)
	}
	if afterIndex == -1 {
		t.Fatalf("method %q was not sent", after)
	}
	if beforeIndex > afterIndex {
		t.Fatalf("%s sent at index %d after %s at index %d", before, beforeIndex, after, afterIndex)
	}
}
