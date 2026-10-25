package codex

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// writeSlowExitRuntime writes a fake runtime that answers initialize, never answers the
// next request, and needs exitDelay to shut down after its stdin closes.
func writeSlowExitRuntime(t *testing.T, exitDelay time.Duration) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell fake runtime is POSIX-only")
	}
	path := filepath.Join(t.TempDir(), "slow-exit-codex")
	script := fmt.Sprintf(`#!/bin/sh
IFS= read -r line
printf '%%s\n' '{"id":"go-1","result":{"userAgent":"codex_app_server/0.0.0 (Linux 7.0; x86_64) codex-go-test (codex_go_sdk; 0.0.0-dev)","codexHome":"/tmp/codex","platformFamily":"unix","platformOs":"linux","stableProtocolDigest":"%s","experimentalProtocolDigest":"%s","stableSchemaDigest":"%s","experimentalSchemaDigest":"%s","stableManifestDigest":"%s","experimentalManifestDigest":"%s","activeProtocolMode":"experimental"}}'
cat > /dev/null
sleep %.1f
exit 0
`, protocol.StableProtocolDigest, protocol.ExperimentalProtocolDigest, protocol.StableSchemaDigest, protocol.ExperimentalSchemaDigest, protocol.StableManifestDigest, protocol.ExperimentalManifestDigest, exitDelay.Seconds())
	if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

// R4.1, R4.3: Close fails a pending call and closes streams, the streams of turn handles
// included, right away, then waits for the runtime to finish shutting down instead of
// killing it.
func TestClientCloseFailsPendingWorkBeforeRuntimeShutsDown(t *testing.T) {
	const exitDelay = 2 * time.Second
	client, err := NewClient(context.Background(), ClientConfig{CodexPath: writeSlowExitRuntime(t, exitDelay)})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	turnStream, err := newTurnHandle(client, "thread-1", "turn-1", false).Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	pending := make(chan error, 1)
	go func() {
		_, err := client.Raw().MemoryReset(context.Background())
		pending <- err
	}()
	time.Sleep(100 * time.Millisecond)

	start := time.Now()
	closed := make(chan error, 1)
	go func() { closed <- client.Close() }()
	select {
	case err := <-pending:
		var closedErr *ClosedError
		if !errors.As(err, &closedErr) {
			t.Fatalf("pending call err = %T %v, want *ClosedError", err, err)
		}
		if elapsed := time.Since(start); elapsed > exitDelay/2 {
			t.Fatalf("pending call failed after %v, only once the runtime exited", elapsed)
		}
	case <-time.After(exitDelay / 2):
		t.Fatal("pending call still waiting while the runtime shuts down")
	}
	if _, ok := stream.Next(context.Background()); ok {
		t.Fatal("stream delivered after Close")
	}
	var closedErr *ClosedError
	if !errors.As(stream.Err(), &closedErr) {
		t.Fatalf("stream err = %v, want *ClosedError", stream.Err())
	}
	select {
	case <-turnStream.done:
		if !errors.As(turnStream.Err(), &closedErr) {
			t.Fatalf("turn stream err = %v, want *ClosedError", turnStream.Err())
		}
	default:
		t.Fatal("turn stream still open while the runtime shuts down")
	}
	if err := <-closed; err != nil {
		t.Fatalf("Close: %v", err)
	}
	if elapsed := time.Since(start); elapsed < exitDelay {
		t.Fatalf("Close returned after %v, before the runtime finished its %v shutdown", elapsed, exitDelay)
	}
}

// R4.2: a negative grace period kills the runtime at once.
func TestClientCloseWithNegativeGraceKillsAtOnce(t *testing.T) {
	client, err := NewClient(context.Background(), ClientConfig{
		CodexPath:           writeSlowExitRuntime(t, 10*time.Second),
		ShutdownGracePeriod: -1,
	})
	if err != nil {
		t.Fatal(err)
	}
	start := time.Now()
	if err := client.Close(); err != nil {
		t.Fatal(err)
	}
	if elapsed := time.Since(start); elapsed > 5*time.Second {
		t.Fatalf("Close took %v with a negative grace period", elapsed)
	}
}

// R4.2, R4.5: the grace period defaults to 5s and belongs to launched runtimes only.
func TestShutdownGracePeriodConfiguration(t *testing.T) {
	for configured, want := range map[time.Duration]time.Duration{
		0:               DefaultShutdownGracePeriod,
		-time.Second:    0,
		3 * time.Second: 3 * time.Second,
	} {
		if got := shutdownGrace(configured); got != want {
			t.Fatalf("shutdownGrace(%v) = %v, want %v", configured, got, want)
		}
	}
	if DefaultShutdownGracePeriod != 5*time.Second {
		t.Fatalf("DefaultShutdownGracePeriod = %v", DefaultShutdownGracePeriod)
	}
	_, err := NewClient(context.Background(), ClientConfig{
		Transport:           newScriptedInitializedTransport(t, nil),
		ShutdownGracePeriod: time.Second,
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T %v, want *ConfigError", err, err)
	}
}
