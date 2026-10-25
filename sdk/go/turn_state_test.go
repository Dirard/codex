package codex

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func startTestTurn(t *testing.T, opts TurnOptions) (*TurnHandle, *Client, *scriptedTransport) {
	t.Helper()
	client, transport := newStage5Client(t)
	transport.responses["turn/start"] = json.RawMessage(`{"turn":{"id":"turn-1","items":[],"status":"inProgress"}}`)
	thread := &Thread{client: client, id: "thread-1"}
	handle, err := thread.Turn(context.Background(), Text("inspect"), opts)
	if err != nil {
		t.Fatal(err)
	}
	return handle, client, transport
}

func assertTurnResult(t *testing.T, result *RunResult, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("Wait: %v", err)
	}
	if result.TurnID != "turn-1" || result.Status != protocol.TurnStatusCompleted || result.FinalResponse != "done" || len(result.Items) != 1 {
		t.Fatalf("result = %+v", result)
	}
}

// R1.3: Wait after a stream of the handle consumed every event still returns the result.
func TestTurnHandleWaitAfterStreamReturnsResult(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{})
	stream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	if methods := drainStream(t, stream); len(methods) != 2 {
		t.Fatalf("stream = %v", methods)
	}
	result, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, result, err)
}

// R1.1: concurrent Waits get equal results that do not share their Items slices.
func TestTurnHandleConcurrentWaitsGetIndependentCopies(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{})
	const waiters = 8
	var wg sync.WaitGroup
	results := make([]*RunResult, waiters)
	errs := make([]error, waiters)
	for i := range waiters {
		wg.Add(1)
		go func() {
			defer wg.Done()
			results[i], errs[i] = handle.Wait(boundedContext(t))
		}()
	}
	time.Sleep(20 * time.Millisecond)
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	wg.Wait()
	for i := range waiters {
		assertTurnResult(t, results[i], errs[i])
	}
	results[0].Items[0] = protocol.ThreadItem{}
	again, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, again, err)
	if again.Items[0].TypeValue != "agentMessage" {
		t.Fatal("a caller's edit of its result changed the recorded result")
	}
}

// R1.4: a Wait whose ctx ends returns the ctx error; the next Wait gets the outcome.
func TestTurnHandleWaitResumesAfterContextEnds(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{})
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Millisecond)
	defer cancel()
	if _, err := handle.Wait(ctx); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("err = %v, want deadline exceeded", err)
	}
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	result, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, result, err)
	if methodWasSent(t, transport, "turn/interrupt") {
		t.Fatal("cancellation without InterruptOnCancel reached the runtime")
	}
}

// R1.1: a decode error of the turn's terminal event is the outcome of every Wait and of a
// late stream.
func TestTurnHandleRecordsTerminalError(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{})
	transport.deliverNotification("turn/completed", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1","status":7}}`), nil)
	for i := range 2 {
		_, err := handle.Wait(boundedContext(t))
		var decodeErr *DecodeError
		if !errors.As(err, &decodeErr) {
			t.Fatalf("wait %d err = %T %v, want *DecodeError", i, err, err)
		}
	}
	stream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := stream.Next(boundedContext(t)); ok {
		t.Fatal("late stream delivered an event")
	}
	var decodeErr *DecodeError
	if !errors.As(stream.Err(), &decodeErr) {
		t.Fatalf("late stream err = %v, want *DecodeError", stream.Err())
	}
}

// R1.5: a cancelled Run stops following its turn, so later events of the turn are buffered
// for nobody instead of feeding a forgotten subscription.
func TestRunReleasesSubscriptionWhenContextEnds(t *testing.T) {
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
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		_, err := thread.Run(ctx, Text("hello"), TurnOptions{})
		done <- err
	}()
	waitForMethod(t, transport, "turn/start")
	time.Sleep(20 * time.Millisecond)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("Run err = %v, want context.Canceled", err)
	}
	client.router.mu.Lock()
	streams := len(client.router.streams)
	client.router.mu.Unlock()
	if streams != 0 {
		t.Fatalf("router still has %d keyed subscriptions after Run returned", streams)
	}
}

// R1.4: a login Wait cancelled while it confirms the account resumes there instead of
// waiting again for the completion event it already consumed.
func TestLoginHandleWaitResumesAfterCancelDuringAccountRead(t *testing.T) {
	handle, transport := startTestLogin(t)
	release := transport.deferResponse("account/read", json.RawMessage(`{"account":{"type":"apiKey"},"requiresOpenaiAuth":true}`))
	transport.deliverNotification("account/login/completed", mustJSON(t, protocol.AccountLoginCompletedNotification{
		LoginID: protocol.Some("login-1"),
		Success: true,
	}), nil)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		_, err := handle.Wait(ctx)
		done <- err
	}()
	waitForMethod(t, transport, "account/read")
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("first Wait err = %v, want context.Canceled", err)
	}
	release()
	result, err := handle.Wait(boundedContext(t))
	if err != nil || result.LoginID != "login-1" || !result.Success {
		t.Fatalf("resumed Wait = %+v, %v", result, err)
	}
}
