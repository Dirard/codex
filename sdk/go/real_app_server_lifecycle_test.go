package codex

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// newRealAppServerClientInHome starts a real runtime on codexHome, so a second client can
// open the same state after the first one closed.
func newRealAppServerClientInHome(t *testing.T, codexHome string, workdir string, fixture *realResponsesFixture) *Client {
	t.Helper()
	writeRealAppServerConfig(t, codexHome, fixture.URL())
	client, err := NewClient(realAppServerContext(t), ClientConfig{
		CodexPath: requireRealAppServerRuntime(t),
		CWD:       workdir,
		Env:       map[string]string{"CODEX_HOME": codexHome},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	return client
}

// R4.1, R4.4: Close lets the real runtime exit on its own after EOF on stdin, and the
// finished turn is on disk for the next runtime (05 A1).
func TestRealAppServerCloseLetsRuntimeExitAndKeepsTheTurn(t *testing.T) {
	requireRealAppServerRuntime(t)
	fixture := newRealResponsesFixture(t, realAssistantResponseSSE("kept after close"))
	codexHome := t.TempDir()
	workdir := t.TempDir()
	client := newRealAppServerClientInHome(t, codexHome, workdir, fixture)
	ctx := realAppServerContext(t)
	thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := thread.Run(ctx, Text("say ok"), TurnOptions{CWD: workdir}); err != nil {
		t.Fatal(err)
	}
	start := time.Now()
	if err := client.Close(); err != nil {
		t.Fatal(err)
	}
	t.Logf("Close took %v", time.Since(start))
	exitCode, signal, stderrTail := client.runtimeProcess.TerminationDetails(time.Second)
	if exitCode == nil || *exitCode != 0 || signal != "" {
		t.Fatalf("runtime exit code = %v, signal = %q, stderr = %q; want a clean exit without a kill", exitCode, signal, stderrTail)
	}

	reopened := newRealAppServerClientInHome(t, codexHome, workdir, fixture)
	response, err := reopened.Threads.Read(ctx, protocol.ThreadReadParams{ThreadID: thread.ID(), IncludeTurns: protocol.SomeNonNull(true)})
	if err != nil {
		t.Fatal(err)
	}
	turns := response.Thread.Turns
	if len(turns) != 1 || turns[0].Status != protocol.TurnStatusCompleted {
		t.Fatalf("turns after reopening = %+v", turns)
	}
}

// R1.1, R1.3: against the real runtime, Wait after the stream was drained returns the
// result, and again on a second call.
func TestRealAppServerTurnWaitAfterStream(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t, realAssistantResponseSSE("waited after stream"))
	ctx := realAppServerContext(t)
	thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	handle, err := thread.Turn(ctx, Text("say ok"), TurnOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := handle.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	for _, err := range stream.All(ctx) {
		if err != nil {
			t.Fatal(err)
		}
	}
	for i := range 2 {
		result, err := handle.Wait(ctx)
		if err != nil {
			t.Fatalf("wait %d: %v", i, err)
		}
		if result.FinalResponse != "waited after stream" || result.Status != protocol.TurnStatusCompleted {
			t.Fatalf("wait %d result = %+v", i, result)
		}
	}
}

// R2.1: a cancelled Run with InterruptOnCancel ends the real turn as interrupted.
func TestRealAppServerRunInterruptOnCancel(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t, realHangingResponse)
	ctx := realAppServerContext(t)
	thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	notifications, err := client.Notifications(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer notifications.Close()
	runCtx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	if _, err := thread.Run(runCtx, Text("never finishes"), TurnOptions{CWD: workdir, InterruptOnCancel: true}); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("Run err = %v, want deadline exceeded", err)
	}
	for completed, err := range Payloads[protocol.TurnCompletedNotification](ctx, notifications) {
		if err != nil {
			t.Fatalf("waiting for the interrupted turn: %v", err)
		}
		if completed.ThreadID != thread.ID() {
			continue
		}
		if completed.Turn.Status != protocol.TurnStatusInterrupted {
			t.Fatalf("turn status = %s, want interrupted", completed.Turn.Status)
		}
		return
	}
	t.Fatalf("notifications ended without turn/completed: %v", notifications.Err())
}
