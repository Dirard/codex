package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func countSentMethod(t *testing.T, transport *scriptedTransport, method string) int {
	t.Helper()
	count := 0
	for _, frame := range transport.sentFrames() {
		if methodFromFrame(t, frame) == method {
			count++
		}
	}
	return count
}

func assertInterruptSent(t *testing.T, transport *scriptedTransport, threadID string, turnID string) {
	t.Helper()
	params := requestParamsForMethod(t, transport, "turn/interrupt")
	assertRequestThreadID(t, params, threadID)
	assertRequestStringField(t, params, "turnId", turnID)
}

func runInBackground(thread *Thread, ctx context.Context, opts TurnOptions) <-chan error {
	done := make(chan error, 1)
	go func() {
		_, err := thread.Run(ctx, Text("hello"), opts)
		done <- err
	}()
	return done
}

func startWorkflowThread(t *testing.T) (*Thread, *scriptedTransport) {
	t.Helper()
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
	return thread, transport
}

// R2.1, R2.5: a cancelled Run with InterruptOnCancel stops the turn on the runtime before
// it returns the context error.
func TestRunInterruptOnCancelInterruptsTurn(t *testing.T) {
	thread, transport := startWorkflowThread(t)
	ctx, cancel := context.WithCancel(context.Background())
	done := runInBackground(thread, ctx, TurnOptions{InterruptOnCancel: true})
	waitForMethod(t, transport, "turn/start")
	time.Sleep(20 * time.Millisecond)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("Run err = %v, want context.Canceled", err)
	}
	assertInterruptSent(t, transport, "thread-1", "turn-1")
}

// R2.3: without the option cancellation stays local.
func TestRunWithoutInterruptOnCancelKeepsTurnRunning(t *testing.T) {
	thread, transport := startWorkflowThread(t)
	ctx, cancel := context.WithCancel(context.Background())
	done := runInBackground(thread, ctx, TurnOptions{})
	waitForMethod(t, transport, "turn/start")
	time.Sleep(20 * time.Millisecond)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("Run err = %v, want context.Canceled", err)
	}
	time.Sleep(50 * time.Millisecond)
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("turn/interrupt sent without InterruptOnCancel")
	}
}

// R2.1: ending the ctx passed to Stream interrupts the turn.
func TestStreamContextInterruptsTurn(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	ctx, cancel := context.WithCancel(context.Background())
	stream, err := handle.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	cancel()
	if _, ok := stream.Next(context.Background()); ok {
		t.Fatal("cancelled stream delivered an event")
	}
	if !errors.Is(stream.Err(), context.Canceled) {
		t.Fatalf("stream err = %v", stream.Err())
	}
	waitForMethod(t, transport, "turn/interrupt")
	assertInterruptSent(t, transport, "thread-1", "turn-1")
}

// R2.4: closing a stream is not a cancellation, and a finished turn is not interrupted.
func TestInterruptOnCancelSkipsClosedStreamsAndFinishedTurns(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	ctx, cancel := context.WithCancel(context.Background())
	stream, err := handle.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	_ = stream.Close()
	cancel()
	time.Sleep(50 * time.Millisecond)
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("closing a stream interrupted the turn")
	}

	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	result, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, result, err)
	cancelled, cancelLate := context.WithCancel(context.Background())
	cancelLate()
	// A finished turn returns its result even to an already cancelled Wait.
	result, err = handle.Wait(cancelled)
	assertTurnResult(t, result, err)
	late, err := handle.Stream(cancelled)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := late.Next(context.Background()); ok {
		t.Fatal("stream of a finished turn delivered an event")
	}
	time.Sleep(50 * time.Millisecond)
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("a finished turn was interrupted")
	}
}

// R2.1: the interrupt is sent once however many waits and streams are cancelled.
func TestInterruptOnCancelIsSentOnce(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	streamCtx, cancelStream := context.WithCancel(context.Background())
	if _, err := handle.Stream(streamCtx); err != nil {
		t.Fatal(err)
	}
	for range 3 {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
		if _, err := handle.Wait(ctx); !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("Wait err = %v", err)
		}
		cancel()
	}
	cancelStream()
	time.Sleep(50 * time.Millisecond)
	if count := countSentMethod(t, transport, "turn/interrupt"); count != 1 {
		t.Fatalf("turn/interrupt sent %d times, want 1", count)
	}
}

// R2.2: a turn/start whose ctx ends before the response is interrupted once the turn id
// arrives.
func TestTurnStartCancelledBeforeResponseInterruptsStartedTurn(t *testing.T) {
	thread, transport := startWorkflowThread(t)
	release := transport.deferResponse("turn/start", mustJSON(t, protocol.TurnStartResponse{
		Turn: protocol.Turn{ID: "turn-1", Items: []protocol.ThreadItem{}, Status: protocol.TurnStatusInProgress},
	}))
	ctx, cancel := context.WithCancel(context.Background())
	done := runInBackground(thread, ctx, TurnOptions{InterruptOnCancel: true})
	waitForMethod(t, transport, "turn/start")
	cancel()
	time.Sleep(20 * time.Millisecond)
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("interrupt sent before the turn id was known")
	}
	release()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("Run err = %v, want context.Canceled", err)
	}
	assertInterruptSent(t, transport, "thread-1", "turn-1")
}

// R2.1: a review turn is interrupted on its review thread.
func TestReviewWaitInterruptOnCancelInterruptsReviewTurn(t *testing.T) {
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
	handle, err := client.Reviews.Start(context.Background(), ReviewStartOptions{
		ThreadID:          "thread-1",
		Target:            UncommittedChangesReviewTarget(),
		InterruptOnCancel: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	if _, err := handle.Wait(ctx); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("Wait err = %v", err)
	}
	assertInterruptSent(t, transport, "review-thread-1", "review-turn-1")
}

// R2.5: a failed interrupt is reported together with the context error.
func TestInterruptOnCancelReportsInterruptFailure(t *testing.T) {
	thread, transport := startWorkflowThread(t)
	transport.errors["turn/interrupt"] = &RPCError{Code: -32000, Message: "no active turn"}
	ctx, cancel := context.WithCancel(context.Background())
	done := runInBackground(thread, ctx, TurnOptions{InterruptOnCancel: true})
	waitForMethod(t, transport, "turn/start")
	time.Sleep(20 * time.Millisecond)
	cancel()
	err := <-done
	var rpcErr *RPCError
	if !errors.Is(err, context.Canceled) || !errors.As(err, &rpcErr) || rpcErr.Message != "no active turn" {
		t.Fatalf("Run err = %v, want context.Canceled joined with the interrupt error", err)
	}
}

func TestInterruptOnCancelTurnStartKeepsResponseWhenNotCancelled(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{InterruptOnCancel: true})
	if handle.ID() != "turn-1" {
		t.Fatalf("turn id = %q", handle.ID())
	}
	params := requestParamsForMethod(t, transport, "turn/start")
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if _, ok := raw["interruptOnCancel"]; ok {
		t.Fatal("the SDK-only option leaked into turn/start params")
	}
}
