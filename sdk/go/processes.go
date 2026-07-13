package codex

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"strings"
	"sync"

	"github.com/openai/codex/sdk/go/protocol"
)

type ProcessSpawnOptions struct {
	Command            []string
	CWD                string
	Size               *TerminalSize
	TTY                *bool
	StreamStdin        *bool
	StreamStdoutStderr *bool
	OutputBytesCap     *uint64
	TimeoutMS          *int64
}

type ProcessHandle struct {
	client *Client
	handle string
	state  *processState
}

type processState struct {
	mu            sync.Mutex
	stream        *NotificationStream
	streamClaimed bool
	exited        bool
	pty           bool
}

func (c *ProcessesClient) Spawn(ctx context.Context, opts ProcessSpawnOptions) (*ProcessHandle, protocol.ProcessSpawnResponse, error) {
	if c == nil || c.client == nil {
		return nil, nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("process spawn", "process/spawn"); err != nil {
		return nil, nil, err
	}
	if err := validateProcessSpawnOptions(opts); err != nil {
		return nil, nil, err
	}
	process := c.reserveProcess(opts)
	response, err := c.client.Raw().ProcessSpawn(ctx, processSpawnParams(opts, process.handle))
	if err != nil {
		c.releaseProcess(process.handle)
		return nil, response, err
	}
	return process, response, nil
}

func (h *ProcessHandle) ID() string {
	if h == nil {
		return ""
	}
	return h.handle
}

func (h *ProcessHandle) Stream(ctx context.Context) (*NotificationStream, error) {
	if h == nil || h.client == nil || h.state == nil {
		return nil, &ClosedError{}
	}
	stream, err := h.state.claimStream(h.handle)
	if err != nil {
		return nil, err
	}
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (h *ProcessHandle) WriteStdin(ctx context.Context, data []byte) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().ProcessWriteStdin(ctx, protocol.ProcessWriteStdinParams{
		DeltaBase64:   protocol.Some(base64.StdEncoding.EncodeToString(data)),
		ProcessHandle: h.handle,
	})
	return err
}

func (h *ProcessHandle) CloseStdin(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().ProcessWriteStdin(ctx, protocol.ProcessWriteStdinParams{
		CloseStdin:    protocol.SomeNonNull(true),
		ProcessHandle: h.handle,
	})
	return err
}

func (h *ProcessHandle) ResizePTY(ctx context.Context, size TerminalSize) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	if err := h.ensurePTY(); err != nil {
		return err
	}
	_, err := h.client.Raw().ProcessResizePty(ctx, protocol.ProcessResizePtyParams{
		ProcessHandle: h.handle,
		Size:          processTerminalSize(size),
	})
	return err
}

func (h *ProcessHandle) Kill(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().ProcessKill(ctx, protocol.ProcessKillParams{ProcessHandle: h.handle})
	return err
}

func (h *ProcessHandle) ensureActive() error {
	if h == nil || h.client == nil || h.client.Processes == nil {
		return &ClosedError{}
	}
	if !h.client.Processes.isProcessActive(h.handle) {
		return &ConflictError{Reason: fmt.Sprintf("process %s is no longer active", h.handle)}
	}
	return nil
}

func (h *ProcessHandle) ensurePTY() error {
	if h == nil || h.state == nil {
		return &ClosedError{}
	}
	if !h.state.hasPTY() {
		return &ConfigError{Reason: "process resize requires TTY true"}
	}
	return nil
}

func (c *ProcessesClient) reserveProcess(opts ProcessSpawnOptions) *ProcessHandle {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.activeProcesses == nil {
		c.activeProcesses = map[string]*ProcessHandle{}
	}
	c.nextHandleID++
	handle := fmt.Sprintf("go-process-%d", c.nextHandleID)
	state := &processState{
		stream: c.client.router.subscribe("process", handle),
		pty:    opts.TTY != nil && *opts.TTY,
	}
	process := &ProcessHandle{
		client: c.client,
		handle: handle,
		state:  state,
	}
	c.activeProcesses[process.handle] = process
	return process
}

func (c *ProcessesClient) releaseProcess(handle string) {
	c.mu.Lock()
	process := c.activeProcesses[handle]
	var stream *NotificationStream
	if process != nil && process.state != nil {
		process.state.mu.Lock()
		process.state.exited = true
		stream = process.state.stream
		process.state.mu.Unlock()
	}
	delete(c.activeProcesses, handle)
	c.mu.Unlock()
	if stream != nil {
		_ = stream.Close()
	}
}

func (c *ProcessesClient) isProcessActive(handle string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	process := c.activeProcesses[handle]
	if process == nil || process.state == nil {
		return false
	}
	process.state.mu.Lock()
	defer process.state.mu.Unlock()
	return !process.state.exited
}

func (c *Client) observeProcessLifecycle(method string, params []byte) string {
	if c == nil || c.Processes == nil || method != "process/exited" {
		return ""
	}
	var payload protocol.ProcessExitedNotification
	if err := json.Unmarshal(params, &payload); err != nil || payload.ProcessHandle == "" {
		return ""
	}
	c.Processes.mu.Lock()
	process := c.Processes.activeProcesses[payload.ProcessHandle]
	if process != nil && process.state != nil {
		process.state.mu.Lock()
		process.state.exited = true
		process.state.mu.Unlock()
	}
	c.Processes.mu.Unlock()
	return payload.ProcessHandle
}

func processSpawnParams(opts ProcessSpawnOptions, handle string) protocol.ProcessSpawnParams {
	params := protocol.ProcessSpawnParams{
		Command:            opts.Command,
		Cwd:                protocol.AbsolutePathBuf(opts.CWD),
		ProcessHandle:      handle,
		StreamStdin:        protocol.SomeNonNull(true),
		StreamStdoutStderr: protocol.SomeNonNull(true),
	}
	if opts.Size != nil {
		params.Size = protocol.Some(processTerminalSize(*opts.Size))
	}
	if opts.TTY != nil {
		params.Tty = protocol.SomeNonNull(*opts.TTY)
	}
	if opts.OutputBytesCap != nil {
		params.OutputBytesCap = protocol.Some(*opts.OutputBytesCap)
	}
	if opts.TimeoutMS != nil {
		params.TimeoutMs = protocol.Some(*opts.TimeoutMS)
	}
	return params
}

func validateProcessSpawnOptions(opts ProcessSpawnOptions) error {
	if len(opts.Command) == 0 {
		return &ConfigError{Reason: "process spawn requires a command"}
	}
	if !isLikelyAbsolutePath(opts.CWD) {
		return &ConfigError{Reason: "process spawn requires absolute CWD"}
	}
	if opts.Size != nil && (opts.TTY == nil || !*opts.TTY) {
		return &ConfigError{Reason: "process spawn Size requires TTY true"}
	}
	if opts.StreamStdin != nil && !*opts.StreamStdin {
		return &ConfigError{Reason: "process spawn streamStdin must stay enabled for ProcessHandle"}
	}
	if opts.StreamStdoutStderr != nil && !*opts.StreamStdoutStderr {
		return &ConfigError{Reason: "process spawn streamStdoutStderr must stay enabled for ProcessHandle"}
	}
	return nil
}

func isLikelyAbsolutePath(path string) bool {
	if strings.HasPrefix(path, "/") || strings.HasPrefix(path, `\\`) {
		return true
	}
	if len(path) < 3 || path[1] != ':' || (path[2] != '\\' && path[2] != '/') {
		return false
	}
	return isASCIIAlpha(path[0])
}

func isASCIIAlpha(value byte) bool {
	return (value >= 'A' && value <= 'Z') || (value >= 'a' && value <= 'z')
}

func processTerminalSize(size TerminalSize) protocol.ProcessTerminalSize {
	return protocol.ProcessTerminalSize{Rows: size.Rows, Cols: size.Cols}
}

func (s *processState) claimStream(handle string) (*NotificationStream, error) {
	if s == nil {
		return nil, &ClosedError{}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.stream == nil {
		return nil, &ClosedError{}
	}
	if s.streamClaimed {
		return nil, &ConflictError{Reason: fmt.Sprintf("process stream %s has already been claimed", handle)}
	}
	s.streamClaimed = true
	return s.stream, nil
}

func (s *processState) hasPTY() bool {
	if s == nil {
		return false
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.pty
}
