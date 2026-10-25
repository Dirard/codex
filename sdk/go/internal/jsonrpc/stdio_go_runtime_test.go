package jsonrpc

import (
	"context"
	"fmt"
	"io"
	"os"
	"syscall"
	"testing"
	"time"
)

// fakeRuntimeEnv makes the test binary act as a fake runtime instead of running tests, so
// the shutdown tests below need no POSIX shell and also run on Windows.
const fakeRuntimeEnv = "CODEX_GO_SDK_JSONRPC_FAKE_RUNTIME"

func TestMain(m *testing.M) {
	if mode := os.Getenv(fakeRuntimeEnv); mode != "" {
		os.Exit(runFakeRuntime(mode))
	}
	os.Exit(m.Run())
}

// runFakeRuntime reads stdin until EOF, like the runtime does, then exits with code 0
// ("exit-on-eof") or keeps running until it is killed ("ignore-eof").
func runFakeRuntime(mode string) int {
	_, _ = io.Copy(io.Discard, os.Stdin)
	switch mode {
	case "exit-on-eof":
		return 0
	case "ignore-eof":
		time.Sleep(time.Minute)
		return 0
	default:
		fmt.Fprintf(os.Stderr, "unknown fake runtime mode %q\n", mode)
		return 2
	}
}

func startGoRuntime(t *testing.T, mode string, grace time.Duration) *StdioTransport {
	t.Helper()
	executable, err := os.Executable()
	if err != nil {
		t.Skipf("test binary path: %v", err)
	}
	stdio, err := StartStdio(context.Background(), StdioOptions{
		Path: executable,
		// The race detector of the re-executed binary would otherwise sleep a second at exit.
		Env:           append(os.Environ(), fakeRuntimeEnv+"="+mode, "GORACE=atexit_sleep_ms=0"),
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

// R4.1, R4.4: a runtime that exits once its stdin closes shuts down by itself.
func TestStdioTransportCloseLetsGoRuntimeExit(t *testing.T) {
	stdio := startGoRuntime(t, "exit-on-eof", 10*time.Second)
	if elapsed := closeTimed(t, stdio); elapsed > 5*time.Second {
		t.Fatalf("Close took %v for a runtime that exits on EOF", elapsed)
	}
	if stdio.killedByClose.Load() {
		t.Fatal("Close killed a runtime that was exiting on its own")
	}
	exitCode, signal, _ := stdio.TerminationDetails(time.Second)
	if exitCode == nil || *exitCode != 0 || signal != "" {
		t.Fatalf("exit code = %v, signal = %q; want the runtime's own exit code 0", exitCode, signal)
	}
}

// R4.1, R4.4, R4.8: a runtime that ignores EOF is killed after the grace period, and the
// kill is reported neither as a signal (POSIX) nor as exit code 1 (Windows).
func TestStdioTransportCloseKillsGoRuntimeAfterGrace(t *testing.T) {
	const grace = 300 * time.Millisecond
	stdio := startGoRuntime(t, "ignore-eof", grace)
	elapsed := closeTimed(t, stdio)
	if elapsed < grace || elapsed > grace+5*time.Second {
		t.Fatalf("Close took %v, want the %v grace period before the kill", elapsed, grace)
	}
	if !stdio.killedByClose.Load() {
		t.Fatal("Close did not kill a runtime that ignored EOF")
	}
	exitCode, signal, _ := stdio.TerminationDetails(time.Second)
	if exitCode != nil || signal != "" {
		t.Fatalf("exit code = %v, signal = %q; want the kill by Close unreported", deref(exitCode), signal)
	}
}

// R4.9: CloseNow during a Close that waits out a long grace period kills the runtime at
// once, and both return.
func TestStdioTransportCloseNowEndsTheGracePeriod(t *testing.T) {
	stdio := startGoRuntime(t, "ignore-eof", time.Minute)
	closed := make(chan error, 1)
	start := time.Now()
	go func() { closed <- stdio.Close() }()
	time.Sleep(100 * time.Millisecond)
	closedNow := make(chan error, 1)
	go func() { closedNow <- stdio.CloseNow() }()
	for _, result := range []chan error{closed, closedNow} {
		select {
		case err := <-result:
			if err != nil {
				t.Fatalf("close: %v", err)
			}
		case <-time.After(10 * time.Second):
			t.Fatal("Close kept waiting for the grace period after CloseNow")
		}
	}
	if elapsed := time.Since(start); elapsed > 10*time.Second {
		t.Fatalf("Close and CloseNow took %v", elapsed)
	}
	if !stdio.killedByClose.Load() {
		t.Fatal("the runtime was not killed")
	}
}

// R4.4, R4.8: which exits the SDK reports.
func TestExitReport(t *testing.T) {
	for _, tc := range []struct {
		name          string
		goos          string
		code          int
		signaled      bool
		sig           syscall.Signal
		killedByClose bool
		wantCode      *int
		wantSignal    string
	}{
		{name: "own exit", goos: "linux", code: 3, wantCode: intPtr(3)},
		{name: "own exit after a late kill", goos: "linux", code: 3, killedByClose: true, wantCode: intPtr(3)},
		{name: "kill by Close", goos: "linux", code: -1, signaled: true, sig: syscall.SIGKILL, killedByClose: true},
		{name: "external kill", goos: "linux", code: -1, signaled: true, sig: syscall.SIGKILL, wantSignal: syscall.SIGKILL.String()},
		{name: "other signal while closing", goos: "linux", code: -1, signaled: true, sig: syscall.SIGTERM, killedByClose: true, wantSignal: syscall.SIGTERM.String()},
		{name: "windows kill by Close", goos: "windows", code: 1, killedByClose: true},
		{name: "windows own exit code 1", goos: "windows", code: 1, wantCode: intPtr(1)},
		{name: "windows own exit before the kill", goos: "windows", code: 4, killedByClose: true, wantCode: intPtr(4)},
	} {
		t.Run(tc.name, func(t *testing.T) {
			code, signal := exitReport(tc.goos, tc.code, tc.signaled, tc.sig, tc.killedByClose)
			if deref(code) != deref(tc.wantCode) || (code == nil) != (tc.wantCode == nil) || signal != tc.wantSignal {
				t.Fatalf("exitReport = %v, %q; want %v, %q", deref(code), signal, deref(tc.wantCode), tc.wantSignal)
			}
		})
	}
}

func intPtr(v int) *int { return &v }

func deref(v *int) any {
	if v == nil {
		return nil
	}
	return *v
}
