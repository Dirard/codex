package codex

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// Regressions found by the review of the handle, cancellation and shutdown changes.

func writeShellRuntime(t *testing.T, body string) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell fake runtime is POSIX-only")
	}
	path := filepath.Join(t.TempDir(), "fake-codex")
	if err := os.WriteFile(path, []byte("#!/bin/sh\n"+body), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

func initializeResponseLine() string {
	return fmt.Sprintf(`{"id":"go-1","result":{"userAgent":"codex_app_server/0.0.0 (Linux 7.0; x86_64) codex-go-test (codex_go_sdk; 0.0.0-dev)","codexHome":"/tmp/codex","platformFamily":"unix","platformOs":"linux","stableProtocolDigest":"%s","experimentalProtocolDigest":"%s","stableSchemaDigest":"%s","experimentalSchemaDigest":"%s","stableManifestDigest":"%s","experimentalManifestDigest":"%s","activeProtocolMode":"experimental"}}`,
		protocol.StableProtocolDigest, protocol.ExperimentalProtocolDigest, protocol.StableSchemaDigest, protocol.ExperimentalSchemaDigest, protocol.StableManifestDigest, protocol.ExperimentalManifestDigest)
}

// A connection that fails while the runtime still runs reports the failure at once: the
// shutdown grace period belongs to Close only (review P2-1).
func TestFailedConnectionDoesNotWaitForShutdownGrace(t *testing.T) {
	path := writeShellRuntime(t, fmt.Sprintf(`IFS= read -r line
printf '%%s\n' '%s'
IFS= read -r line || true
IFS= read -r line || true
printf 'not json\n'
exec sleep 30
`, initializeResponseLine()))
	client, err := NewClient(context.Background(), ClientConfig{CodexPath: path})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	start := time.Now()
	_, err = client.Raw().MemoryReset(context.Background())
	var decodeErr *DecodeError
	if !errors.As(err, &decodeErr) {
		t.Fatalf("err = %T %v, want *DecodeError for the malformed frame", err, err)
	}
	if elapsed := time.Since(start); elapsed > DefaultShutdownGracePeriod/2 {
		t.Fatalf("the failure took %v to reach the call", elapsed)
	}
}

// NewClient returns at its ctx deadline when the runtime never answers initialize
// (review P2-1).
func TestNewClientFailureDoesNotWaitForShutdownGrace(t *testing.T) {
	path := writeShellRuntime(t, "exec sleep 30\n")
	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()
	start := time.Now()
	if _, err := NewClient(ctx, ClientConfig{CodexPath: path}); err == nil {
		t.Fatal("NewClient succeeded against a silent runtime")
	}
	if elapsed := time.Since(start); elapsed > DefaultShutdownGracePeriod/2 {
		t.Fatalf("NewClient returned after %v", elapsed)
	}
}

// Events that arrive while no stream of the handle is open reach the next stream, as the
// router's pending buffer delivered them before (review P2-2).
func TestTurnHandleReopenedStreamGetsEventsMissedWhileClosed(t *testing.T) {
	handle, _, transport := startTestTurn(t, TurnOptions{})
	first, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	_ = first.Close()
	deliverTurnEvents(t, transport, "thread-1", "turn-1", "done")
	result, err := handle.Wait(boundedContext(t))
	assertTurnResult(t, result, err)
	reopened, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if methods := drainStream(t, reopened); len(methods) != 2 || methods[0] != "item/completed" || methods[1] != "turn/completed" {
		t.Fatalf("reopened stream = %v", methods)
	}
	if again, err := handle.Stream(context.Background()); err != nil || len(drainStream(t, again)) != 0 {
		t.Fatalf("a stream after the replay got events again (%v)", err)
	}
}

// The events kept for the next stream are bounded like the pending buffer: the oldest
// give way and the stream then reports the loss.
func TestTurnHandleKeptEventsAreBounded(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport, Limits: ClientLimits{PendingTurnQueue: 2}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	transport.responses["turn/start"] = json.RawMessage(`{"turn":{"id":"turn-1","items":[],"status":"inProgress"}}`)
	handle, err := (&Thread{client: client, id: "thread-1"}).Turn(context.Background(), Text("x"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}
	first, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	_ = first.Close()
	for _, delta := range []string{"a", "b", "c"} {
		transport.deliverNotification("item/agentMessage/delta", mustJSON(t, protocol.AgentMessageDeltaNotification{
			ThreadID: "thread-1", TurnID: "turn-1", ItemID: "item-1", Delta: delta,
		}), nil)
	}
	waitForRouted(t, handle, 3)
	reopened, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	var text string
	var failure error
	for delta, err := range Payloads[protocol.AgentMessageDeltaNotification](boundedContext(t), reopened) {
		if err != nil {
			failure = err
			break
		}
		text += delta.Delta
	}
	var overflow *OverflowError
	if text != "bc" || !errors.As(failure, &overflow) {
		t.Fatalf("reopened stream = %q, %v; want the newest two events, then *OverflowError", text, failure)
	}
}

// waitForRouted waits until the handle's pump observed count events while no stream was
// open.
func waitForRouted(t *testing.T, handle *TurnHandle, count int) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		handle.state.mu.Lock()
		kept := len(handle.state.journal)
		lost := handle.state.journalLost
		handle.state.mu.Unlock()
		if lost || kept >= count {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("the handle did not observe %d events", count)
}

// A Run that fails for a reason other than its ctx also releases its subscription
// (review P3-1).
func TestRunReleasesSubscriptionOnResultOverflow(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport, Limits: ClientLimits{MaxRunResultItems: 1}})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() {
		_, err := thread.Run(context.Background(), Text("x"), TurnOptions{})
		done <- err
	}()
	waitForMethod(t, transport, "turn/start")
	for _, id := range []string{"item-1", "item-2"} {
		transport.deliverNotification("item/completed", mustJSON(t, protocol.ItemCompletedNotification{
			ThreadID: "thread-1", TurnID: "turn-1",
			Item: protocol.ThreadItem{ID: protocol.SomeNonNull(id), TypeValue: "agentMessage", Text: protocol.SomeNonNull("x")},
		}), nil)
	}
	var overflow *OverflowError
	if err := <-done; !errors.As(err, &overflow) {
		t.Fatalf("Run err = %v, want *OverflowError", err)
	}
	client.router.mu.Lock()
	streams := len(client.router.streams)
	client.router.mu.Unlock()
	if streams != 0 {
		t.Fatalf("router still has %d keyed subscriptions after Run failed", streams)
	}
}
