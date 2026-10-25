package main

import (
	"context"
	"testing"
	"time"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/codextest"
	"github.com/openai/codex/sdk/go/protocol"
)

func TestAnswer(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// New closes the server when the test ends and fails the test on protocol
	// violations and on methods the script does not answer.
	srv := codextest.New(t)
	srv.Respond("thread/start", codextest.ThreadStartResponse("thread-1"))
	srv.Handle("turn/start", func(ctx context.Context, call *codextest.Call) (any, error) {
		var params protocol.TurnStartParams
		if err := call.Decode(&params); err != nil {
			return nil, err
		}
		// The turn's events follow its response, as with the real runtime.
		_ = call.NotifyAfter("item/completed", codextest.AgentMessageCompleted(params.ThreadID, "turn-1", "item-1", "4"))
		_ = call.NotifyAfter("turn/completed", codextest.TurnCompleted(params.ThreadID, "turn-1", protocol.TurnStatusCompleted))
		return codextest.TurnStartResponse("turn-1"), nil
	})

	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()

	answer, err := Answer(ctx, client, "What is 2+2?")
	if err != nil {
		t.Fatal(err)
	}
	if answer != "4" {
		t.Fatalf("answer = %q, want 4", answer)
	}
}

func TestAnswerInterruptsTheTurnWhenCancelled(t *testing.T) {
	waitCtx, cancelWait := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelWait()

	srv := codextest.New(t)
	srv.Respond("thread/start", codextest.ThreadStartResponse("thread-1"))
	// The turn starts but never completes.
	srv.Respond("turn/start", codextest.TurnStartResponse("turn-1"))
	srv.Respond("turn/interrupt", protocol.TurnInterruptResponse{})

	client, err := codex.NewClient(waitCtx, codex.ClientConfig{Transport: srv})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	answered := make(chan error, 1)
	go func() {
		_, err := Answer(ctx, client, "Take your time")
		answered <- err
	}()
	// Cancel once the turn is running, not after a fixed delay.
	if _, err := srv.WaitForRequest(waitCtx, "turn/start"); err != nil {
		t.Fatal(err)
	}
	cancel()
	if err := <-answered; err == nil {
		t.Fatal("Answer returned without an answer")
	}
	interrupt, err := srv.WaitForRequest(waitCtx, "turn/interrupt")
	if err != nil {
		t.Fatal(err)
	}
	var params protocol.TurnInterruptParams
	if err := interrupt.Decode(&params); err != nil || params.TurnID != "turn-1" {
		t.Fatalf("turn/interrupt params = %+v, %v", params, err)
	}
}
