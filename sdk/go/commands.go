package codex

import (
	"context"
	"encoding/base64"
	"fmt"
	"sync"

	"github.com/openai/codex/sdk/go/protocol"
)

type TerminalSize struct {
	Rows uint16
	Cols uint16
}

type CommandExecOptions struct {
	Command []string
	CWD     string
	Size    *TerminalSize
	TTY     *bool
}

type CommandHandle struct {
	client    *Client
	processID string
	state     *commandExecState
}

type commandExecState struct {
	mu            sync.Mutex
	stream        *NotificationStream
	streamClaimed bool
	response      protocol.CommandExecResponse
	err           error
	completed     bool
	done          chan struct{}
}

func (c *CommandsClient) Exec(ctx context.Context, opts CommandExecOptions) (*CommandHandle, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("command exec", "command/exec"); err != nil {
		return nil, err
	}
	if err := validateCommandExecOptions(opts); err != nil {
		return nil, err
	}
	processID := c.nextCommandProcessID()
	state := &commandExecState{
		stream: c.client.router.subscribe("command", processID),
		done:   make(chan struct{}),
	}
	handle := &CommandHandle{client: c.client, processID: processID, state: state}
	params := commandExecParams(opts, processID)
	var response protocol.CommandExecResponse
	done, err := c.client.callAsync(ctx, "command/exec", params, &response, protocol.MethodMetadataByMethod["command/exec"])
	if err != nil {
		state.complete(protocol.CommandExecResponse{}, err)
		return nil, err
	}
	go func() {
		err := <-done
		state.complete(response, err)
	}()
	return handle, nil
}

func (h *CommandHandle) ID() string {
	if h == nil {
		return ""
	}
	return h.processID
}

func (h *CommandHandle) Stream(ctx context.Context) (*NotificationStream, error) {
	if h == nil || h.client == nil || h.state == nil {
		return nil, &ClosedError{}
	}
	stream, err := h.state.claimStream()
	if err != nil {
		return nil, err
	}
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (h *CommandHandle) Wait(ctx context.Context) (protocol.CommandExecResponse, error) {
	if h == nil || h.state == nil {
		return protocol.CommandExecResponse{}, &ClosedError{}
	}
	if ctx == nil {
		ctx = context.Background()
	}
	select {
	case <-h.state.done:
		return h.state.result()
	case <-ctx.Done():
		return protocol.CommandExecResponse{}, ctx.Err()
	}
}

func (h *CommandHandle) Write(ctx context.Context, data []byte) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().CommandExecWrite(ctx, protocol.CommandExecWriteParams{
		DeltaBase64: protocol.Some(base64.StdEncoding.EncodeToString(data)),
		ProcessID:   h.processID,
	})
	return err
}

func (h *CommandHandle) CloseStdin(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().CommandExecWrite(ctx, protocol.CommandExecWriteParams{
		CloseStdin: protocol.SomeNonNull(true),
		ProcessID:  h.processID,
	})
	return err
}

func (h *CommandHandle) Terminate(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().CommandExecTerminate(ctx, protocol.CommandExecTerminateParams{ProcessID: h.processID})
	return err
}

func (h *CommandHandle) Resize(ctx context.Context, size TerminalSize) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.ensureActive(); err != nil {
		return err
	}
	_, err := h.client.Raw().CommandExecResize(ctx, protocol.CommandExecResizeParams{
		ProcessID: h.processID,
		Size:      commandTerminalSize(size),
	})
	return err
}

func (h *CommandHandle) ensureActive() error {
	if h == nil || h.state == nil {
		return &ClosedError{}
	}
	if h.state.isCompleted() {
		return &ConflictError{Reason: fmt.Sprintf("command process %s is no longer active", h.processID)}
	}
	return nil
}

func (s *commandExecState) claimStream() (*NotificationStream, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.streamClaimed {
		return nil, &ConflictError{Reason: "command stream has already been claimed"}
	}
	s.streamClaimed = true
	return s.stream, nil
}

func (s *commandExecState) complete(response protocol.CommandExecResponse, err error) {
	s.mu.Lock()
	if s.completed {
		s.mu.Unlock()
		return
	}
	s.response = response
	s.err = err
	s.completed = true
	stream := s.stream
	close(s.done)
	s.mu.Unlock()
	stream.closeWithError(err)
}

func (s *commandExecState) result() (protocol.CommandExecResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.response, s.err
}

func (s *commandExecState) isCompleted() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.completed
}

func (c *CommandsClient) nextCommandProcessID() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.nextProcessID++
	return fmt.Sprintf("go-command-%d", c.nextProcessID)
}

func commandExecParams(opts CommandExecOptions, processID string) protocol.CommandExecParams {
	params := protocol.CommandExecParams{
		Command:            opts.Command,
		ProcessID:          protocol.Some(processID),
		StreamStdin:        protocol.SomeNonNull(true),
		StreamStdoutStderr: protocol.SomeNonNull(true),
	}
	if opts.CWD != "" {
		params.Cwd = protocol.Some(opts.CWD)
	}
	if opts.Size != nil {
		params.Size = protocol.Some(commandTerminalSize(*opts.Size))
	}
	if opts.TTY != nil {
		params.Tty = protocol.SomeNonNull(*opts.TTY)
	}
	return params
}

func validateCommandExecOptions(opts CommandExecOptions) error {
	if len(opts.Command) == 0 {
		return &ConfigError{Reason: "command exec requires a command"}
	}
	if opts.Size != nil && (opts.TTY == nil || !*opts.TTY) {
		return &ConfigError{Reason: "command exec Size requires TTY true"}
	}
	return nil
}

func commandTerminalSize(size TerminalSize) protocol.CommandExecTerminalSize {
	return protocol.CommandExecTerminalSize{Rows: size.Rows, Cols: size.Cols}
}
