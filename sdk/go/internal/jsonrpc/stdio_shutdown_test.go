package jsonrpc

import (
	"context"
	"runtime"
	"testing"
	"time"
)

func startShellRuntime(t *testing.T, script string, grace time.Duration) *StdioTransport {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell fake runtime is POSIX-only")
	}
	stdio, err := StartStdio(context.Background(), StdioOptions{
		Path:          "/bin/sh",
		Args:          []string{"-c", script},
		MaxFrameBytes: 1 << 20,
		StderrBytes:   4096,
		ShutdownGrace: grace,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = stdio.Close() })
	return stdio
}

func closeTimed(t *testing.T, stdio *StdioTransport) time.Duration {
	t.Helper()
	start := time.Now()
	if err := stdio.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	return time.Since(start)
}

// R4.1, R4.4: a runtime that exits once its stdin closes shuts down by itself and reports
// its own exit code.
func TestStdioTransportCloseLetsRuntimeExitOnStdinEOF(t *testing.T) {
	stdio := startShellRuntime(t, `cat > /dev/null; exit 7`, 10*time.Second)
	if elapsed := closeTimed(t, stdio); elapsed > 5*time.Second {
		t.Fatalf("Close took %v for a runtime that exits on EOF", elapsed)
	}
	if stdio.killedByClose.Load() {
		t.Fatal("Close killed a runtime that was exiting on its own")
	}
	exitCode, signal, _ := stdio.TerminationDetails(time.Second)
	if exitCode == nil || *exitCode != 7 || signal != "" {
		t.Fatalf("exit code = %v, signal = %q; want the runtime's own exit code 7", exitCode, signal)
	}
}

// R4.1, R4.4: a runtime that ignores EOF is killed after the grace period, not before, and
// that kill is not reported as an external signal.
func TestStdioTransportCloseKillsRuntimeAfterGrace(t *testing.T) {
	const grace = 300 * time.Millisecond
	stdio := startShellRuntime(t, `exec sleep 30`, grace)
	elapsed := closeTimed(t, stdio)
	if elapsed < grace || elapsed > grace+5*time.Second {
		t.Fatalf("Close took %v, want the %v grace period then a kill", elapsed, grace)
	}
	if !stdio.killedByClose.Load() {
		t.Fatal("Close did not kill a runtime that ignored EOF")
	}
	exitCode, signal, _ := stdio.TerminationDetails(time.Second)
	if exitCode != nil || signal != "" {
		t.Fatalf("exit code = %v, signal = %q; the SDK's own kill must not be reported", exitCode, signal)
	}
}

// R4.2: without a grace period Close kills a running runtime at once.
func TestStdioTransportCloseWithoutGraceKillsAtOnce(t *testing.T) {
	stdio := startShellRuntime(t, `exec sleep 30`, 0)
	if elapsed := closeTimed(t, stdio); elapsed > 3*time.Second {
		t.Fatalf("Close took %v without a grace period", elapsed)
	}
	if !stdio.killedByClose.Load() {
		t.Fatal("Close did not kill the runtime")
	}
}

// R4.1: output the runtime writes while it shuts down is drained, so it cannot block on a
// full stdout pipe or die of a closed one.
func TestStdioTransportCloseDrainsStdoutWhileRuntimeExits(t *testing.T) {
	script := `cat > /dev/null; head -c 1048576 /dev/zero | tr '\0' 'x'; echo; exit 5`
	stdio := startShellRuntime(t, script, 10*time.Second)
	if elapsed := closeTimed(t, stdio); elapsed > 5*time.Second {
		t.Fatalf("Close took %v; the runtime blocked on stdout", elapsed)
	}
	exitCode, signal, _ := stdio.TerminationDetails(time.Second)
	if exitCode == nil || *exitCode != 5 || signal != "" {
		t.Fatalf("exit code = %v, signal = %q; want 5 after writing its shutdown output", exitCode, signal)
	}
}

// Close unblocks a Receive waiting on a live runtime without waiting for the runtime.
func TestStdioTransportCloseUnblocksReceiveBeforeRuntimeExits(t *testing.T) {
	stdio := startShellRuntime(t, `cat > /dev/null; sleep 2; exit 0`, 10*time.Second)
	received := make(chan error, 1)
	go func() {
		_, err := stdio.Receive(context.Background())
		received <- err
	}()
	time.Sleep(50 * time.Millisecond)
	closed := make(chan struct{})
	go func() {
		_ = stdio.Close()
		close(closed)
	}()
	select {
	case err := <-received:
		if err == nil {
			t.Fatal("Receive returned a frame from a closed transport")
		}
	case <-time.After(time.Second):
		t.Fatal("Receive stayed blocked until the runtime exited")
	}
	select {
	case <-closed:
		t.Fatal("Close returned before the runtime exited")
	default:
	}
	<-closed
}
