package jsonrpc

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
)

// runtimeOutputDrain is how long stdout may stay idle, and stderr be copied, after the
// runtime exited. A runtime descendant can keep the pipes open; it must not hide the exit
// or block Close.
const runtimeOutputDrain = time.Second

type StdioOptions struct {
	Path          string
	Args          []string
	Dir           string
	Env           []string
	MaxFrameBytes int64
	StderrBytes   int
	// ShutdownGrace is how long Close waits for the runtime to exit after closing its
	// stdin before it kills the runtime. Zero kills it at once.
	ShutdownGrace time.Duration
}

type StdioTransport struct {
	transport *StreamTransport
	cmd       *exec.Cmd
	stderr    *Ring
	stdout    *runtimeStdout
	// exited is closed once cmd.Wait returned.
	exited        chan struct{}
	shutdownGrace time.Duration
	// killedByClose is set when Close sent a kill before observing the exit.
	killedByClose atomic.Bool
	closeOnce     sync.Once
	closeErr      error
}

// runtimeStdout reads the runtime's stdout. After the runtime exited it returns what the
// runtime left in the pipe and then EOF once the pipe stays idle, even while a descendant
// still holds the write end. Each read re-arms the idle deadline, so a slow reader loses
// nothing.
type runtimeStdout struct {
	file   *os.File
	exited chan struct{}
	once   sync.Once
	done   chan struct{}
}

func (r *runtimeStdout) Read(p []byte) (int, error) {
	select {
	case <-r.exited:
		_ = r.file.SetReadDeadline(time.Now().Add(runtimeOutputDrain))
	default:
	}
	n, err := r.file.Read(p)
	if errors.Is(err, os.ErrDeadlineExceeded) {
		err = io.EOF
	}
	if err != nil {
		r.signal()
	}
	return n, err
}

// Close tolerates a pipe the drain already closed after the runtime exited.
func (r *runtimeStdout) Close() error {
	r.signal()
	if err := r.file.Close(); err != nil && !errors.Is(err, os.ErrClosed) {
		return err
	}
	return nil
}

// runtimeExited bounds a read that was already blocked when the runtime exited. Pipes
// without deadline support (Windows) are closed after the drain instead, so a reader
// slower than the drain can miss frames there.
func (r *runtimeStdout) runtimeExited() {
	if err := r.file.SetReadDeadline(time.Now().Add(runtimeOutputDrain)); err == nil {
		return
	}
	select {
	case <-r.done:
	case <-time.After(runtimeOutputDrain):
		_ = r.Close()
	}
}

func (r *runtimeStdout) signal() {
	r.once.Do(func() { close(r.done) })
}

// pumpRuntimeStdout copies the runtime's stdout to the frame reader. Once the frame reader
// is closed it keeps reading and discards the output, so a runtime that is shutting down
// after Close never blocks on a full stdout pipe. The end of stdout reaches the frame
// reader after every byte before it.
func pumpRuntimeStdout(stdout *runtimeStdout, frames *io.PipeWriter) {
	buf := make([]byte, 32*1024)
	forward := true
	for {
		n, err := stdout.Read(buf)
		if n > 0 && forward {
			if _, writeErr := frames.Write(buf[:n]); writeErr != nil {
				forward = false
			}
		}
		if err != nil {
			_ = frames.CloseWithError(err)
			_ = stdout.Close()
			return
		}
	}
}

// exitedPipeWriter tolerates a stdin pipe that cmd.Wait already closed after the runtime
// exited.
type exitedPipeWriter struct {
	io.WriteCloser
}

func (w exitedPipeWriter) Close() error {
	if err := w.WriteCloser.Close(); err != nil && !errors.Is(err, os.ErrClosed) {
		return err
	}
	return nil
}

func StartStdio(ctx context.Context, opts StdioOptions) (*StdioTransport, error) {
	if opts.Path == "" {
		return nil, fmt.Errorf("codex path is required")
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	cmd := exec.Command(opts.Path, opts.Args...)
	cmd.Dir = opts.Dir
	cmd.Env = opts.Env
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	// The parent owns the read end of stdout, so cmd.Wait cannot close it under the
	// reader and drop the last frames of an exiting runtime.
	stdoutRead, stdoutWrite, err := os.Pipe()
	if err != nil {
		_ = stdin.Close()
		return nil, err
	}
	cmd.Stdout = stdoutWrite
	stderr := NewRing(opts.StderrBytes)
	cmd.Stderr = stderr
	cmd.WaitDelay = runtimeOutputDrain
	if err := cmd.Start(); err != nil {
		_ = stdin.Close()
		_ = stdoutRead.Close()
		_ = stdoutWrite.Close()
		return nil, err
	}
	_ = stdoutWrite.Close()
	if err := ctx.Err(); err != nil {
		_ = stdin.Close()
		_ = stdoutRead.Close()
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
		}
		_ = cmd.Wait()
		return nil, err
	}
	exited := make(chan struct{})
	stdout := &runtimeStdout{file: stdoutRead, exited: exited, done: make(chan struct{})}
	// Closing the frame reader unblocks Receive at once, while the pump keeps draining the
	// runtime's stdout until the runtime exits.
	frames, framesWriter := io.Pipe()
	go pumpRuntimeStdout(stdout, framesWriter)
	t := &StdioTransport{
		transport:     NewStreamTransport(frames, exitedPipeWriter{stdin}, opts.MaxFrameBytes),
		cmd:           cmd,
		stderr:        stderr,
		stdout:        stdout,
		exited:        exited,
		shutdownGrace: opts.ShutdownGrace,
	}
	go func() {
		_ = cmd.Wait()
		close(exited)
		stdout.runtimeExited()
	}()
	return t, nil
}

func (t *StdioTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	return t.transport.Receive(ctx)
}

func (t *StdioTransport) Send(ctx context.Context, frame json.RawMessage) error {
	return t.transport.Send(ctx, frame)
}

// Close shuts the runtime down in order: it closes the runtime's stdin, which asks the
// runtime to exit, keeps draining its stdout, waits up to the shutdown grace period for
// the exit and only then kills the runtime. Close returns once the runtime exited.
func (t *StdioTransport) Close() error {
	return t.close(t.shutdownGrace)
}

// CloseNow kills a runtime that is still running without the grace period, for a
// connection that already failed or never finished starting.
func (t *StdioTransport) CloseNow() error {
	return t.close(0)
}

func (t *StdioTransport) close(grace time.Duration) error {
	t.closeOnce.Do(func() {
		t.closeErr = t.transport.Close()
		if t.cmd.Process != nil && !t.exitedWithin(grace) {
			t.killedByClose.Store(true)
			_ = t.cmd.Process.Kill()
		}
		if t.exited != nil {
			<-t.exited
		}
		if t.stdout != nil {
			_ = t.stdout.Close()
		}
	})
	return t.closeErr
}

func (t *StdioTransport) exitedWithin(grace time.Duration) bool {
	if t.exited == nil {
		return false
	}
	if grace <= 0 {
		select {
		case <-t.exited:
			return true
		default:
			return false
		}
	}
	timer := time.NewTimer(grace)
	defer timer.Stop()
	select {
	case <-t.exited:
		return true
	case <-timer.C:
		return false
	}
}

// TerminationDetails waits up to timeout for the runtime to exit and returns its exit
// code and terminating signal, when known, and the redacted stderr tail. The kill that
// Close itself sends is not reported, since the SDK ended the runtime.
func (t *StdioTransport) TerminationDetails(timeout time.Duration) (exitCode *int, signal string, stderrTail string) {
	if t == nil {
		return nil, "", ""
	}
	if t.exited != nil {
		select {
		case <-t.exited:
			if state := t.cmd.ProcessState; state != nil {
				if status, ok := state.Sys().(syscall.WaitStatus); ok && status.Signaled() {
					if status.Signal() != syscall.SIGKILL || !t.killedByClose.Load() {
						signal = status.Signal().String()
					}
				} else {
					code := state.ExitCode()
					exitCode = &code
				}
			}
		case <-time.After(timeout):
		}
	}
	return exitCode, signal, t.StderrTail()
}

func (t *StdioTransport) StderrTail() string {
	if t == nil || t.stderr == nil {
		return ""
	}
	return t.stderr.String()
}
