package codex

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// Regressions found by the independent review of the handle, cancellation and Close
// changes (docs/fork/reviews/2026-10-25-go-sdk-features-review.md). These tests use only
// API that existed before the fixes, so they also show the defects on the reviewed code.

// routedBarrier returns once the client routed every frame the transport queued before the
// call: the response of a request arrives after them, and notifications are routed in the
// order they arrive.
func routedBarrier(t *testing.T, client *Client) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if _, err := client.Raw().MemoryReset(ctx); err != nil {
		t.Fatalf("barrier call: %v", err)
	}
}

// queuedNotifications takes what a stream already holds without waiting, and reports
// whether the stream is closed.
func queuedNotifications(stream *NotificationStream) ([]Notification, bool) {
	var queued []Notification
	for {
		select {
		case notification, ok := <-stream.Events():
			if !ok {
				return queued, true
			}
			queued = append(queued, notification)
		default:
			return queued, false
		}
	}
}

func deliverDelta(t *testing.T, transport *scriptedTransport, threadID string, turnID string, delta string) {
	t.Helper()
	transport.deliverNotification("item/agentMessage/delta", mustJSON(t, protocol.AgentMessageDeltaNotification{
		ThreadID: threadID, TurnID: turnID, ItemID: "item-1", Delta: delta,
	}), nil)
}

func startTurnWithLimits(t *testing.T, limits ClientLimits, opts TurnOptions) (*TurnHandle, *Client, *scriptedTransport) {
	t.Helper()
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport, Limits: limits})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	transport.mu.Lock()
	transport.responses["turn/start"] = json.RawMessage(`{"turn":{"id":"turn-1","items":[],"status":"inProgress"}}`)
	transport.mu.Unlock()
	handle, err := (&Thread{client: client, id: "thread-1"}).Turn(context.Background(), Text("x"), opts)
	if err != nil {
		t.Fatal(err)
	}
	return handle, client, transport
}

// crashingTransport is a scripted runtime that crashes: Receive fails once crash is
// closed, and closing the transport after that blocks until release, as cmd.Wait does
// while a descendant of the runtime keeps its stderr open.
type crashingTransport struct {
	*scriptedTransport
	crash       chan struct{}
	closing     chan struct{}
	release     chan struct{}
	closingOnce sync.Once
}

func newCrashingTransport(t *testing.T) *crashingTransport {
	return &crashingTransport{
		scriptedTransport: newScriptedInitializedTransport(t, nil),
		crash:             make(chan struct{}),
		closing:           make(chan struct{}),
		release:           make(chan struct{}),
	}
}

func (t *crashingTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	select {
	case frame := <-t.recv:
		return frame, nil
	case <-t.crash:
		return nil, io.EOF
	case <-t.done:
		return nil, &ClosedError{}
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (t *crashingTransport) closeAfterCrash() error {
	t.closingOnce.Do(func() { close(t.closing) })
	<-t.release
	return t.scriptedTransport.Close()
}

func (t *crashingTransport) Close() error    { return t.closeAfterCrash() }
func (t *crashingTransport) CloseNow() error { return t.closeAfterCrash() }

func (t *crashingTransport) TerminationDetails(time.Duration) (*int, string, string) {
	code := 3
	return &code, "", "runtime panicked"
}

func assertCrashError(t *testing.T, what string, err error) {
	t.Helper()
	var transportErr *TransportError
	if !errors.As(err, &transportErr) {
		t.Fatalf("%s err = %T %v, want *TransportError", what, err, err)
	}
	if transportErr.ExitCode == nil || *transportErr.ExitCode != 3 || transportErr.StderrTail != "runtime panicked" {
		t.Fatalf("%s err = %+v, want exit code 3 and the stderr tail", what, transportErr)
	}
}

func drainToError(t *testing.T, stream *NotificationStream) error {
	t.Helper()
	ctx := boundedContext(t)
	for {
		if _, ok := stream.Next(ctx); !ok {
			return stream.Err()
		}
	}
}

// R4.7 (review P2-1): Close called while an unexpected termination is already under way
// waits for it; streams, handles and pending calls all get its *TransportError.
func TestCloseDuringUnexpectedTerminationKeepsTransportError(t *testing.T) {
	transport := newCrashingTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	// The fake reports the termination details of a launched runtime.
	client.runtimeProcess = transport
	transport.mu.Lock()
	transport.responses["turn/start"] = json.RawMessage(`{"turn":{"id":"turn-1","items":[],"status":"inProgress"}}`)
	transport.mu.Unlock()
	handle, err := (&Thread{client: client, id: "thread-1"}).Turn(context.Background(), Text("x"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	turnStream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	global, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	_ = transport.deferResponse("memory/reset", json.RawMessage(`{}`))
	pending := make(chan error, 1)
	go func() {
		_, err := client.Raw().MemoryReset(context.Background())
		pending <- err
	}()
	waitForMethod(t, transport.scriptedTransport, "memory/reset")

	close(transport.crash)
	<-transport.closing // the SDK is ending the failed connection
	closed := make(chan error, 1)
	go func() { closed <- client.Close() }()
	// Give a Close that does not wait for the termination time to replace its error.
	deadline := time.Now().Add(200 * time.Millisecond)
	for time.Now().Before(deadline) && client.terminationError() == nil {
		time.Sleep(time.Millisecond)
	}
	close(transport.release)
	if err := <-closed; err != nil {
		t.Fatalf("Close: %v", err)
	}
	assertCrashError(t, "pending call", <-pending)
	assertCrashError(t, "global stream", drainToError(t, global))
	assertCrashError(t, "turn stream", drainToError(t, turnStream))
	_, err = handle.Wait(boundedContext(t))
	assertCrashError(t, "turn Wait", err)
}

// R4.7 (review P2-1, end to end): a runtime that exits with code 3 while a descendant
// keeps its stderr open, and a Close during the detection of that exit.
func TestCloseWhileRuntimeCrashKeepsExitCode(t *testing.T) {
	path := writeShellRuntime(t, fmt.Sprintf(`IFS= read -r line
printf '%%s\n' '%s'
IFS= read -r line
IFS= read -r line
sleep 3 >/dev/null </dev/null &
exit 3
`, initializeResponseLine()))
	client, err := NewClient(context.Background(), ClientConfig{CodexPath: path})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	pending := make(chan error, 1)
	go func() {
		_, err := client.Raw().MemoryReset(context.Background())
		pending <- err
	}()
	// The exit is seen at once; its details wait about a second for the stderr copy.
	time.Sleep(300 * time.Millisecond)
	if err := client.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	for _, got := range []struct {
		what string
		err  error
	}{{"pending call", <-pending}, {"stream", drainToError(t, stream)}} {
		var transportErr *TransportError
		if !errors.As(got.err, &transportErr) || transportErr.ExitCode == nil || *transportErr.ExitCode != 3 {
			t.Fatalf("%s err = %T %v, want *TransportError with exit code 3", got.what, got.err, got.err)
		}
	}
}

// R4.3 (review P3-1): when Close returns, the streams of turn handles are closed too.
func TestClientCloseClosesTurnStreamsBeforeReturning(t *testing.T) {
	const rounds = 300
	open := 0
	for range rounds {
		transport := newScriptedInitializedTransport(t, nil)
		client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
		if err != nil {
			t.Fatal(err)
		}
		handle := newTurnHandle(client, "thread-1", "turn-1", false)
		stream, err := handle.Stream(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		_ = client.Close()
		select {
		case <-stream.done:
			var closedErr *ClosedError
			if !errors.As(stream.Err(), &closedErr) {
				t.Fatalf("stream err = %v, want *ClosedError", stream.Err())
			}
		default:
			open++
		}
	}
	if open != 0 {
		t.Fatalf("%d of %d turn streams were still open when Close returned", open, rounds)
	}
}

// R1.6 (review P2-3): events that arrive after a Wait returned because of its ctx reach
// the first stream of the handle.
func TestStreamAfterTimedOutWaitGetsEventsInBetween(t *testing.T) {
	t.Run("review", func(t *testing.T) {
		handle, transport := startTestReview(t)
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
		defer cancel()
		if _, err := handle.Wait(ctx); !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("Wait err = %v", err)
		}
		deliverDelta(t, transport, "review-thread-1", "review-turn-1", "kept")
		routedBarrier(t, handle.client)
		events, err := handle.Events(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		transport.deliverNotification("turn/completed", mustJSON(t, protocol.TurnCompletedNotification{
			ThreadID: "review-thread-1",
			Turn:     protocol.Turn{ID: "review-turn-1", Items: []protocol.ThreadItem{}, Status: protocol.TurnStatusCompleted},
		}), nil)
		if methods := drainStream(t, events); len(methods) != 2 || methods[0] != "item/agentMessage/delta" || methods[1] != "turn/completed" {
			t.Fatalf("Events after a timed-out Wait = %v, want [item/agentMessage/delta turn/completed]", methods)
		}
	})
	t.Run("turn", func(t *testing.T) {
		handle, client, transport := startTestTurn(t, TurnOptions{})
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
		defer cancel()
		if _, err := handle.Wait(ctx); !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("Wait err = %v", err)
		}
		deliverDelta(t, transport, "thread-1", "turn-1", "kept")
		routedBarrier(t, client)
		stream, err := handle.Stream(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
		if methods := drainStream(t, stream); len(methods) != 3 || methods[0] != "item/agentMessage/delta" {
			t.Fatalf("Stream after a timed-out Wait = %v, want the delta first", methods)
		}
	})
}

// R1.6 (review P2-2): the events handles keep for their next stream share the client's
// pending budget; a handle that lost some reports the loss to its next stream.
func TestKeptEventsShareTheClientPendingBudget(t *testing.T) {
	const budget = 1 << 20
	const handles = 20
	const deltasPerHandle = 5
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport, Limits: ClientLimits{PendingNotificationBytes: budget}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	turns := make([]*TurnHandle, handles)
	for i := range turns {
		turns[i] = newTurnHandle(client, "thread-1", fmt.Sprintf("turn-%d", i), false)
		stream, err := turns[i].Stream(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		_ = stream.Close()
	}
	big := strings.Repeat("x", 200*1024)
	for i := range turns {
		for range deltasPerHandle {
			deliverDelta(t, transport, "thread-1", fmt.Sprintf("turn-%d", i), big)
		}
	}
	routedBarrier(t, client)
	var keptBytes int64
	lossReported := 0
	for i, handle := range turns {
		stream, err := handle.Stream(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		replayed, closed := queuedNotifications(stream)
		for _, notification := range replayed {
			keptBytes += notificationRetainedBytes(notification)
		}
		if len(replayed) < deltasPerHandle {
			var overflow *OverflowError
			if !closed || !errors.As(stream.Err(), &overflow) {
				t.Fatalf("handle %d replayed %d of %d events without reporting the loss (closed %v, err %v)", i, len(replayed), deltasPerHandle, closed, stream.Err())
			}
			lossReported++
		}
	}
	if keptBytes > budget {
		t.Fatalf("handles kept %d bytes for their next streams, over the %d byte pending budget", keptBytes, budget)
	}
	if lossReported == 0 {
		t.Fatal("no handle reported a loss although the events exceed the budget")
	}
}

// R1.6 (review P2-2): kept events expire like the router's pending events, and the next
// stream reports that they were dropped.
func TestKeptEventsExpireAfterInactivityTimeout(t *testing.T) {
	handle, client, transport := startTurnWithLimits(t, ClientLimits{LifecycleInactivityTimeout: 50 * time.Millisecond}, TurnOptions{})
	first, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	_ = first.Close()
	deliverDelta(t, transport, "thread-1", "turn-1", "stale")
	routedBarrier(t, client)
	time.Sleep(250 * time.Millisecond)
	reopened, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	replayed, closed := queuedNotifications(reopened)
	var overflow *OverflowError
	if len(replayed) != 0 || !closed || !errors.As(reopened.Err(), &overflow) {
		t.Fatalf("stream after the timeout = %d events, closed %v, err %v; want none and *OverflowError", len(replayed), closed, reopened.Err())
	}
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	result, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, result, err)
}

// R1.7 (review P2-4): a stream nobody reads overflows on its own; the handle still gets
// the turn's result.
func TestUnreadStreamDoesNotFailTheHandle(t *testing.T) {
	handle, client, transport := startTurnWithLimits(t, ClientLimits{ResourceStreamQueue: 2}, TurnOptions{})
	stream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	for i := range 6 {
		deliverDelta(t, transport, "thread-1", "turn-1", fmt.Sprint(i))
	}
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	routedBarrier(t, client)
	result, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, result, err)
	if _, closed := queuedNotifications(stream); !closed {
		t.Fatal("the unread stream is still open")
	}
	var overflow *OverflowError
	if !errors.As(stream.Err(), &overflow) {
		t.Fatalf("unread stream err = %v, want its own *OverflowError", stream.Err())
	}
}

// R1.7 (review P2-2): an open turn stream needs no goroutine of its own.
func TestOpenTurnStreamsNeedNoGoroutines(t *testing.T) {
	client, _ := newStage5Client(t)
	before := runtime.NumGoroutine()
	streams := make([]*NotificationStream, 0, 100)
	for i := range 100 {
		stream, err := newTurnHandle(client, "thread-1", fmt.Sprintf("turn-%d", i), false).Stream(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		streams = append(streams, stream)
	}
	if added := runtime.NumGoroutine() - before; added > 10 {
		t.Fatalf("100 open turn streams added %d goroutines", added)
	}
	runtime.KeepAlive(streams)
}

// R2.6 (review P3-2): an interrupt that failed after a stream's ctx ended is sent again
// by the next cancellation, and a cancelled Wait returns that attempt's error.
func TestFailedStreamInterruptIsRetried(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	transport.mu.Lock()
	transport.errors["turn/interrupt"] = &RPCError{Code: -32000, Message: "runtime busy"}
	transport.mu.Unlock()
	ctx, cancel := context.WithCancel(context.Background())
	stream, err := handle.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	cancel()
	if err := drainToError(t, stream); !errors.Is(err, context.Canceled) {
		t.Fatalf("stream err = %v, want context.Canceled at once", err)
	}
	waitForMethod(t, transport, "turn/interrupt")

	cancelled, cancelNow := context.WithCancel(context.Background())
	cancelNow()
	var waitErr error
	deadline := time.Now().Add(2 * time.Second)
	for countSentMethod(t, transport, "turn/interrupt") < 2 && time.Now().Before(deadline) {
		_, waitErr = handle.Wait(cancelled)
		time.Sleep(time.Millisecond)
	}
	if sent := countSentMethod(t, transport, "turn/interrupt"); sent != 2 {
		t.Fatalf("turn/interrupt sent %d times, want a retry after the failed one", sent)
	}
	var rpcErr *RPCError
	if !errors.Is(waitErr, context.Canceled) || !errors.As(waitErr, &rpcErr) {
		t.Fatalf("cancelled Wait err = %v, want the ctx error joined with the retry's error", waitErr)
	}

	transport.mu.Lock()
	delete(transport.errors, "turn/interrupt")
	transport.mu.Unlock()
	if _, err := handle.Wait(cancelled); !errors.Is(err, context.Canceled) || errors.As(err, &rpcErr) {
		t.Fatalf("Wait err = %v, want only the ctx error after a successful interrupt", err)
	}
	for range 2 {
		_, _ = handle.Wait(cancelled)
	}
	if sent := countSentMethod(t, transport, "turn/interrupt"); sent != 3 {
		t.Fatalf("turn/interrupt sent %d times, want no more after one succeeded", sent)
	}
}

// R2.1 (review P3-4): a successful explicit Interrupt counts; a later cancellation does
// not interrupt the turn again.
func TestExplicitInterruptEndsInterruptOnCancel(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	if err := handle.Interrupt(boundedContext(t)); err != nil {
		t.Fatal(err)
	}
	cancelled, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := handle.Wait(cancelled); !errors.Is(err, context.Canceled) {
		t.Fatalf("Wait err = %v", err)
	}
	if sent := countSentMethod(t, transport, "turn/interrupt"); sent != 1 {
		t.Fatalf("turn/interrupt sent %d times, want only the explicit one", sent)
	}
}

// R2.5 (review P3-4): when the turn completes while the cancelled Wait interrupts it,
// Wait returns the result.
func TestWaitReturnsResultThatArrivedDuringInterrupt(t *testing.T) {
	handle, client, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	release := transport.deferResponse("turn/interrupt", json.RawMessage(`{}`))
	ctx, cancel := context.WithCancel(context.Background())
	type outcome struct {
		result *RunResult
		err    error
	}
	done := make(chan outcome, 1)
	go func() {
		result, err := handle.Wait(ctx)
		done <- outcome{result, err}
	}()
	cancel()
	waitForMethod(t, transport, "turn/interrupt")
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	routedBarrier(t, client)
	release()
	got := <-done
	assertTurnResult(t, got.result, got.err)
}

// R2.2 (review P3-3): a cancelled start whose late response is an error returns that
// error with the ctx error.
func TestCancelledInterruptibleStartKeepsStartError(t *testing.T) {
	thread, transport := startWorkflowThread(t)
	release := transport.deferResponse("turn/start", json.RawMessage(`{}`))
	transport.mu.Lock()
	transport.errors["turn/start"] = &RPCError{Code: -32000, Message: "start failed"}
	transport.mu.Unlock()
	ctx, cancel := context.WithCancel(context.Background())
	done := runInBackground(thread, ctx, TurnOptions{InterruptOnCancel: true})
	waitForMethod(t, transport, "turn/start")
	cancel()
	time.Sleep(20 * time.Millisecond)
	release()
	err := <-done
	var rpcErr *RPCError
	if !errors.Is(err, context.Canceled) || !errors.As(err, &rpcErr) || rpcErr.Message != "start failed" {
		t.Fatalf("Run err = %v, want the ctx error joined with the start error", err)
	}
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("a turn that never started was interrupted")
	}
}

// R1.9 (review P3-5): an account check that failed after the login completed is not the
// login's outcome; the next Wait checks again.
func TestLoginWaitRetriesFailedAccountCheck(t *testing.T) {
	handle, transport := startTestLogin(t)
	transport.mu.Lock()
	transport.errors["account/read"] = &RPCError{Code: -32000, Message: "account store busy"}
	transport.mu.Unlock()
	transport.deliverNotification("account/login/completed", mustJSON(t, protocol.AccountLoginCompletedNotification{
		LoginID: protocol.Some("login-1"),
		Success: true,
	}), nil)
	var rpcErr *RPCError
	if _, err := handle.Wait(boundedContext(t)); !errors.As(err, &rpcErr) {
		t.Fatalf("first Wait err = %v, want the account check error", err)
	}
	transport.mu.Lock()
	delete(transport.errors, "account/read")
	transport.mu.Unlock()
	result, err := handle.Wait(boundedContext(t))
	if err != nil || result == nil || !result.Success || result.LoginID != "login-1" {
		t.Fatalf("second Wait = %+v, %v; want the login result", result, err)
	}
}
