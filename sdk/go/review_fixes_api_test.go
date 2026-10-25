package codex

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// Tests of the API and state added by the fixes for the independent review
// (docs/fork/reviews/2026-10-25-go-sdk-features-review.md).

func routerKeyedStreams(client *Client) int {
	client.router.mu.Lock()
	defer client.router.mu.Unlock()
	return len(client.router.streams)
}

func routerKeptBytes(client *Client) int64 {
	client.router.mu.Lock()
	defer client.router.mu.Unlock()
	return client.router.keptBytes
}

// R1.8: Release drops the subscription, the kept events and the result; pending and later
// calls fail with *ClosedError.
func TestReleaseStopsFollowingTheTurn(t *testing.T) {
	handle, client, transport := startTestTurn(t, TurnOptions{})
	first, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	_ = first.Close()
	deliverDelta(t, transport, "thread-1", "turn-1", "kept")
	routedBarrier(t, client)
	if routerKeptBytes(client) == 0 || routerKeyedStreams(client) == 0 {
		t.Fatal("the handle neither kept the event nor followed the turn before Release")
	}
	pending := make(chan error, 1)
	go func() {
		_, err := handle.Wait(context.Background())
		pending <- err
	}()
	time.Sleep(10 * time.Millisecond)

	handle.Release()
	handle.Release()
	var closedErr *ClosedError
	select {
	case err := <-pending:
		if !errors.As(err, &closedErr) {
			t.Fatalf("pending Wait err = %v, want *ClosedError", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("a pending Wait still waits after Release")
	}
	if kept, streams := routerKeptBytes(client), routerKeyedStreams(client); kept != 0 || streams != 0 {
		t.Fatalf("after Release the client still holds %d kept bytes and %d keyed subscriptions", kept, streams)
	}
	if _, err := handle.Wait(boundedContext(t)); !errors.As(err, &closedErr) {
		t.Fatalf("Wait after Release err = %v, want *ClosedError", err)
	}
	stream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if replayed, closed := queuedNotifications(stream); len(replayed) != 0 || !closed || !errors.As(stream.Err(), &closedErr) {
		t.Fatalf("Stream after Release = %d events, closed %v, err %v", len(replayed), closed, stream.Err())
	}
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("Release interrupted the turn")
	}
	// The turn's later events are buffered for a new subscriber instead of a released one.
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	routedBarrier(t, client)
	client.router.mu.Lock()
	pendingKeys := len(client.router.pending)
	client.router.mu.Unlock()
	if pendingKeys == 0 {
		t.Fatal("events after Release did not go to the router's pending buffer")
	}
}

// R1.8: Release closes the open streams of the handle, including the review handle's.
func TestReleaseClosesOpenStreams(t *testing.T) {
	handle, _ := startTestReview(t)
	events, err := handle.Events(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	handle.Release()
	var closedErr *ClosedError
	if _, closed := queuedNotifications(events); !closed || !errors.As(events.Err(), &closedErr) {
		t.Fatalf("open stream after Release: closed %v, err %v", closed, events.Err())
	}
	if _, err := handle.Wait(boundedContext(t)); !errors.As(err, &closedErr) {
		t.Fatalf("review Wait after Release err = %v", err)
	}
	var nilHandle *TurnHandle
	nilHandle.Release()
}

// R1.6: the budget comes back when a stream takes the kept events and when they expire.
func TestKeptEventsReturnTheirBudget(t *testing.T) {
	handle, client, transport := startTurnWithLimits(t, ClientLimits{LifecycleInactivityTimeout: 50 * time.Millisecond}, TurnOptions{})
	first, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	_ = first.Close()
	deliverDelta(t, transport, "thread-1", "turn-1", "taken")
	routedBarrier(t, client)
	if routerKeptBytes(client) == 0 {
		t.Fatal("no bytes reserved for the kept event")
	}
	second, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if kept := routerKeptBytes(client); kept != 0 {
		t.Fatalf("%d kept bytes still reserved after a stream took the events", kept)
	}
	_ = second.Close()
	deliverDelta(t, transport, "thread-1", "turn-1", "expires")
	routedBarrier(t, client)
	deadline := time.Now().Add(2 * time.Second)
	for routerKeptBytes(client) != 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	if kept := routerKeptBytes(client); kept != 0 {
		t.Fatalf("%d kept bytes still reserved after the inactivity timeout", kept)
	}
}

// R2.4: once turn/completed was folded, a cancellation does not interrupt the turn, even
// before the subscription closed.
func TestNoInterruptAfterTurnCompletedWasFolded(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	handle.state.deliver(Notification{
		Method: "turn/completed",
		Payload: protocol.TurnCompletedNotification{
			ThreadID: "thread-1",
			Turn:     protocol.Turn{ID: "turn-1", Items: []protocol.ThreadItem{}, Status: protocol.TurnStatusCompleted},
		},
	})
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := handle.Wait(cancelled); !errors.Is(err, context.Canceled) {
		t.Fatalf("Wait err = %v", err)
	}
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("turn/interrupt sent for a turn whose turn/completed was already received")
	}
}

// R4.9: CloseContext bounds the wait for a runtime that does not exit.
func TestCloseContextKillsRuntimeWhenContextEnds(t *testing.T) {
	client, err := NewClient(context.Background(), ClientConfig{CodexPath: writeSlowExitRuntime(t, 10*time.Second)})
	if err != nil {
		t.Fatal(err)
	}
	pending := make(chan error, 1)
	go func() {
		_, err := client.Raw().MemoryReset(context.Background())
		pending <- err
	}()
	time.Sleep(50 * time.Millisecond)
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()
	start := time.Now()
	err = client.CloseContext(ctx)
	elapsed := time.Since(start)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("CloseContext err = %v, want context.DeadlineExceeded", err)
	}
	if elapsed < 300*time.Millisecond || elapsed > 3*time.Second {
		t.Fatalf("CloseContext took %v, want about the 300ms ctx", elapsed)
	}
	var closedErr *ClosedError
	if err := <-pending; !errors.As(err, &closedErr) {
		t.Fatalf("pending call err = %v, want *ClosedError", err)
	}
	// A Close after that returns at once.
	if err := client.Close(); err != nil {
		t.Fatalf("Close after CloseContext: %v", err)
	}
}

// R4.9: CloseContext of a runtime that exits in time returns nil, and a CloseContext that
// joins a Close in progress cuts its grace period short.
func TestCloseContextWaitsForTimelyExitAndHurriesClose(t *testing.T) {
	client, err := NewClient(context.Background(), ClientConfig{CodexPath: writeSlowExitRuntime(t, 0)})
	if err != nil {
		t.Fatal(err)
	}
	if err := client.CloseContext(boundedContext(t)); err != nil {
		t.Fatalf("CloseContext: %v", err)
	}

	slow, err := NewClient(context.Background(), ClientConfig{CodexPath: writeSlowExitRuntime(t, 10*time.Second)})
	if err != nil {
		t.Fatal(err)
	}
	closed := make(chan error, 1)
	start := time.Now()
	go func() { closed <- slow.Close() }()
	time.Sleep(50 * time.Millisecond)
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	if err := slow.CloseContext(ctx); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("CloseContext joining a Close err = %v", err)
	}
	if err := <-closed; err != nil {
		t.Fatalf("Close: %v", err)
	}
	if elapsed := time.Since(start); elapsed > 3*time.Second {
		t.Fatalf("Close took %v although CloseContext's ctx ended after 200ms", elapsed)
	}
}

// R4.9: with an injected transport CloseContext is Close.
func TestCloseContextWithInjectedTransport(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	if err := client.CloseContext(boundedContext(t)); err != nil {
		t.Fatalf("CloseContext: %v", err)
	}
	var nilClient *Client
	if err := nilClient.CloseContext(context.Background()); err != nil {
		t.Fatal(err)
	}
}
