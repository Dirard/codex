package jsonrpc

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"sync"
)

type StdioOptions struct {
	Path          string
	Args          []string
	Dir           string
	Env           []string
	MaxFrameBytes int64
	StderrBytes   int
}

type StdioTransport struct {
	transport  *StreamTransport
	cmd        *exec.Cmd
	stderr     *Ring
	waitDone   chan error
	stderrDone chan struct{}
	closeOnce  sync.Once
	closeErr   error
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
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		_ = stdin.Close()
		return nil, err
	}
	stderrPipe, err := cmd.StderrPipe()
	if err != nil {
		_ = stdin.Close()
		_ = stdout.Close()
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		_ = stdin.Close()
		_ = stdout.Close()
		_ = stderrPipe.Close()
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		_ = stdin.Close()
		_ = stdout.Close()
		_ = stderrPipe.Close()
		if cmd.Process != nil {
			_ = cmd.Process.Kill()
		}
		_ = cmd.Wait()
		return nil, err
	}
	stderr := NewRing(opts.StderrBytes)
	stderrDone := make(chan struct{})
	go func() {
		defer close(stderrDone)
		_, _ = io.Copy(stderr, stderrPipe)
	}()
	t := &StdioTransport{
		transport:  NewStreamTransport(stdout, stdin, opts.MaxFrameBytes),
		cmd:        cmd,
		stderr:     stderr,
		waitDone:   make(chan error, 1),
		stderrDone: stderrDone,
	}
	go func() {
		t.waitDone <- cmd.Wait()
	}()
	return t, nil
}

func (t *StdioTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	return t.transport.Receive(ctx)
}

func (t *StdioTransport) Send(ctx context.Context, frame json.RawMessage) error {
	return t.transport.Send(ctx, frame)
}

func (t *StdioTransport) Close() error {
	t.closeOnce.Do(func() {
		t.closeErr = t.transport.Close()
		if t.cmd.Process != nil {
			_ = t.cmd.Process.Kill()
		}
		if t.waitDone != nil {
			<-t.waitDone
		}
		if t.stderrDone != nil {
			<-t.stderrDone
		}
	})
	return t.closeErr
}

func (t *StdioTransport) StderrTail() string {
	if t == nil || t.stderr == nil {
		return ""
	}
	return t.stderr.String()
}
