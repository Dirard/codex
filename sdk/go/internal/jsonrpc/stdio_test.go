package jsonrpc

import (
	"bytes"
	"context"
	"errors"
	"io"
	"os/exec"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestStreamTransportReadsLargeFramesAndRejectsOversized(t *testing.T) {
	frame := `{"data":"` + strings.Repeat("x", 70*1024) + `"}`
	reader := io.NopCloser(strings.NewReader(frame + "\n"))
	writer := nopWriteCloser{Writer: &bytes.Buffer{}}
	transport := NewStreamTransport(reader, writer, int64(len(frame)))
	got, err := transport.Receive(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != len(frame) {
		t.Fatalf("len = %d, want %d", len(got), len(frame))
	}

	reader = io.NopCloser(strings.NewReader(frame + "x\n"))
	transport = NewStreamTransport(reader, writer, int64(len(frame)))
	_, err = transport.Receive(context.Background())
	var sizeErr *FrameSizeError
	if !errors.As(err, &sizeErr) {
		t.Fatalf("err = %T, want *FrameSizeError", err)
	}
}

func TestStreamTransportAcceptsFrameAtDefaultLimit(t *testing.T) {
	frame := []byte(`{"data":"`)
	frame = append(frame, bytes.Repeat([]byte("x"), 16*1024*1024-len(frame)-2)...)
	frame = append(frame, []byte(`"}`)...)
	reader := io.NopCloser(bytes.NewReader(append(frame, '\n')))
	writer := nopWriteCloser{Writer: &bytes.Buffer{}}
	transport := NewStreamTransport(reader, writer, int64(len(frame)))
	got, err := transport.Receive(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != len(frame) {
		t.Fatalf("len = %d, want %d", len(got), len(frame))
	}
}

func TestStreamTransportCloseUnblocksReceive(t *testing.T) {
	reader, writer := io.Pipe()
	transport := NewStreamTransport(reader, nopWriteCloser{Writer: &bytes.Buffer{}}, 1024)
	errCh := make(chan error, 1)
	go func() {
		_, err := transport.Receive(context.Background())
		errCh <- err
	}()
	if err := transport.Close(); err != nil {
		t.Fatal(err)
	}
	_ = writer.Close()
	var closed *ClosedError
	if err := <-errCh; !errors.As(err, &closed) {
		t.Fatalf("err = %T, want *ClosedError", err)
	}
}

func TestStreamTransportContextCancellationUnblocksReceive(t *testing.T) {
	reader, writer := io.Pipe()
	transport := NewStreamTransport(reader, nopWriteCloser{Writer: &bytes.Buffer{}}, 1024)
	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		_, err := transport.Receive(ctx)
		errCh <- err
	}()
	cancel()
	defer writer.Close()
	if err := <-errCh; !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", err)
	}
}

func TestStreamTransportSerializesConcurrentWrites(t *testing.T) {
	var buf bytes.Buffer
	transport := NewStreamTransport(io.NopCloser(strings.NewReader("")), nopWriteCloser{Writer: &buf}, 1024)
	var wg sync.WaitGroup
	for i := 0; i < 25; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if err := transport.Send(context.Background(), []byte(`{"ok":true}`)); err != nil {
				t.Errorf("Send() error = %v", err)
			}
		}()
	}
	wg.Wait()
	lines := strings.Split(strings.TrimSuffix(buf.String(), "\n"), "\n")
	if len(lines) != 25 {
		t.Fatalf("lines = %d", len(lines))
	}
	for _, line := range lines {
		if line != `{"ok":true}` {
			t.Fatalf("interleaved line: %q", line)
		}
	}
}

func TestStreamTransportRejectsOversizedOutboundFrameBeforeWrite(t *testing.T) {
	var buf bytes.Buffer
	transport := NewStreamTransport(io.NopCloser(strings.NewReader("")), nopWriteCloser{Writer: &buf}, 5)
	err := transport.Send(context.Background(), []byte(`{"too":"large"}`))
	var sizeErr *FrameSizeError
	if !errors.As(err, &sizeErr) {
		t.Fatalf("err = %T, want *FrameSizeError", err)
	}
	if buf.Len() != 0 {
		t.Fatalf("oversized frame wrote %d bytes", buf.Len())
	}
}

func TestStreamTransportContextCancellationUnblocksSend(t *testing.T) {
	writer := newBlockingWriteCloser()
	transport := NewStreamTransport(io.NopCloser(strings.NewReader("")), writer, 1024)
	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		errCh <- transport.Send(ctx, []byte(`{"ok":true}`))
	}()
	<-writer.started
	cancel()
	if err := <-errCh; !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", err)
	}
}

func TestStdioTransportCloseWaitsForProcessWait(t *testing.T) {
	transport := NewStreamTransport(io.NopCloser(strings.NewReader("")), nopWriteCloser{Writer: &bytes.Buffer{}}, 1024)
	stdio := &StdioTransport{
		transport: transport,
		cmd:       &exec.Cmd{},
		exited:    make(chan struct{}),
	}
	done := make(chan error, 1)
	go func() {
		done <- stdio.Close()
	}()

	select {
	case err := <-done:
		t.Fatalf("Close returned before process wait finished: %v", err)
	case <-time.After(50 * time.Millisecond):
	}

	close(stdio.exited)
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Close did not return after process wait finished")
	}
}

type nopWriteCloser struct {
	io.Writer
}

func (n nopWriteCloser) Close() error { return nil }

type blockingWriteCloser struct {
	started chan struct{}
	closed  chan struct{}
	once    sync.Once
}

func newBlockingWriteCloser() *blockingWriteCloser {
	return &blockingWriteCloser{
		started: make(chan struct{}),
		closed:  make(chan struct{}),
	}
}

func (w *blockingWriteCloser) Write([]byte) (int, error) {
	w.once.Do(func() { close(w.started) })
	<-w.closed
	return 0, io.ErrClosedPipe
}

func (w *blockingWriteCloser) Close() error {
	select {
	case <-w.closed:
	default:
		close(w.closed)
	}
	return nil
}

// A runtime that writes its last frame and exits while the reader is still busy must not
// lose that frame to cmd.Wait closing the pipes; its exit code and stderr stay available.
func TestStdioTransportReadsFinalFramesAndReportsTermination(t *testing.T) {
	ctx := context.Background()
	script := `printf '%s\n' '{"id":1,"result":{"final":true}}'; printf 'fatal: runtime crashed\n' >&2; exit 3`
	for attempt := 0; attempt < 5; attempt++ {
		stdio, err := StartStdio(ctx, StdioOptions{
			Path:          "/bin/sh",
			Args:          []string{"-c", script},
			MaxFrameBytes: 1 << 20,
			StderrBytes:   4096,
		})
		if err != nil {
			t.Fatal(err)
		}
		// The runtime exits before the busy reader gets to its last frame.
		time.Sleep(100 * time.Millisecond)
		frame, err := stdio.Receive(ctx)
		if err != nil {
			t.Fatalf("attempt %d: final frame lost: %v", attempt, err)
		}
		if !strings.Contains(string(frame), `"final":true`) {
			t.Fatalf("attempt %d: frame = %s", attempt, frame)
		}
		if _, err := stdio.Receive(ctx); err == nil {
			t.Fatalf("attempt %d: expected end of stream", attempt)
		}
		exitCode, _, stderrTail := stdio.TerminationDetails(2 * time.Second)
		if exitCode == nil || *exitCode != 3 {
			t.Fatalf("attempt %d: exit code = %v, want 3", attempt, exitCode)
		}
		if !strings.Contains(stderrTail, "fatal: runtime crashed") {
			t.Fatalf("attempt %d: stderr tail = %q", attempt, stderrTail)
		}
		if err := stdio.Close(); err != nil {
			t.Fatalf("attempt %d: close: %v", attempt, err)
		}
	}
}

// A runtime descendant that inherits stdout and stderr must neither hide the runtime's
// exit nor make Close wait for the descendant.
func TestStdioTransportExitIsNotHiddenByInheritedPipes(t *testing.T) {
	ctx := context.Background()
	script := `sleep 4 & printf '%s\n' '{"id":1,"result":{}}'; printf 'fatal: runtime crashed\n' >&2; exit 3`
	stdio, err := StartStdio(ctx, StdioOptions{
		Path:          "/bin/sh",
		Args:          []string{"-c", script},
		MaxFrameBytes: 1 << 20,
		StderrBytes:   4096,
	})
	if err != nil {
		t.Fatal(err)
	}
	start := time.Now()
	if _, err := stdio.Receive(ctx); err != nil {
		t.Fatalf("final frame lost: %v", err)
	}
	if _, err := stdio.Receive(ctx); err == nil {
		t.Fatal("expected end of stream after the runtime exited")
	}
	if elapsed := time.Since(start); elapsed > 3*time.Second {
		t.Fatalf("runtime exit noticed after %v; the descendant hid it", elapsed)
	}
	exitCode, _, stderrTail := stdio.TerminationDetails(2 * time.Second)
	if exitCode == nil || *exitCode != 3 {
		t.Fatalf("exit code = %v, want 3", exitCode)
	}
	if !strings.Contains(stderrTail, "fatal: runtime crashed") {
		t.Fatalf("stderr tail = %q", stderrTail)
	}
	closeStart := time.Now()
	if err := stdio.Close(); err != nil {
		t.Fatal(err)
	}
	if elapsed := time.Since(closeStart); elapsed > 3*time.Second {
		t.Fatalf("Close took %v while a descendant held the pipes", elapsed)
	}
}

// A runtime killed by a signal reports the signal instead of a bare exit code.
func TestStdioTransportReportsTerminatingSignal(t *testing.T) {
	ctx := context.Background()
	stdio, err := StartStdio(ctx, StdioOptions{
		Path:          "/bin/sh",
		Args:          []string{"-c", `kill -TERM $$`},
		MaxFrameBytes: 1 << 20,
		StderrBytes:   4096,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer stdio.Close()
	if _, err := stdio.Receive(ctx); err == nil {
		t.Fatal("expected end of stream")
	}
	if _, signal, _ := stdio.TerminationDetails(2 * time.Second); signal != "terminated" {
		t.Fatalf("signal = %q, want terminated", signal)
	}
}

// A reader busier than the post-exit drain window still gets the frames the runtime
// left in the pipe.
func TestStdioTransportSlowReaderKeepsFramesAfterExit(t *testing.T) {
	ctx := context.Background()
	stdio, err := StartStdio(ctx, StdioOptions{
		Path:          "/bin/sh",
		Args:          []string{"-c", `printf '%s\n' '{"id":1,"result":{"final":true}}'; exit 0`},
		MaxFrameBytes: 1 << 20,
		StderrBytes:   4096,
	})
	if err != nil {
		t.Fatal(err)
	}
	defer stdio.Close()
	time.Sleep(runtimeOutputDrain + 500*time.Millisecond)
	frame, err := stdio.Receive(ctx)
	if err != nil || !strings.Contains(string(frame), `"final":true`) {
		t.Fatalf("frame = %s, err = %v", frame, err)
	}
}
