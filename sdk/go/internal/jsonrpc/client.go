package jsonrpc

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

type ServerRequestHandler interface {
	HandleServerRequest(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error)
}

type ServerNotificationHandler interface {
	HandleServerNotification(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) error
}

type safeError interface {
	SafeJSONRPCMessage() string
}

type ClientOptions struct {
	HandlerConcurrency int
	HandlerQueue       int
	HandlerTimeout     time.Duration
	OnTermination      func(error) error
}

type Client struct {
	transport Transport
	handler   ServerRequestHandler

	sendLock chan struct{}

	waitersMu sync.Mutex
	waiters   map[string]chan response
	nextID    atomic.Uint64

	closeMu     sync.Mutex
	closed      bool
	terminalErr error
	done        chan struct{}
	ctx         context.Context
	cancel      context.CancelFunc

	handlerQueue   chan Envelope
	serverReplies  chan Envelope
	handlerSlots   chan struct{}
	handlerTimeout time.Duration
	onTermination  func(error) error
}

type terminationOrigin uint8

const (
	terminationExplicitClose terminationOrigin = iota
	terminationUnexpected
)

const minimumServerReplyQueue = 256

type handlerResult struct {
	result any
	err    error
}

type response struct {
	env Envelope
	err error
}

func NewClient(transport Transport, handler ServerRequestHandler) *Client {
	return NewClientWithOptions(transport, handler, ClientOptions{})
}

func NewClientWithOptions(transport Transport, handler ServerRequestHandler, opts ClientOptions) *Client {
	opts = normalizeClientOptions(opts)
	ctx, cancel := context.WithCancel(context.Background())
	replyQueueSize := opts.HandlerQueue
	if replyQueueSize < minimumServerReplyQueue {
		replyQueueSize = minimumServerReplyQueue
	}
	c := &Client{
		transport:      transport,
		handler:        handler,
		sendLock:       make(chan struct{}, 1),
		waiters:        map[string]chan response{},
		done:           make(chan struct{}),
		ctx:            ctx,
		cancel:         cancel,
		handlerQueue:   make(chan Envelope, opts.HandlerQueue),
		serverReplies:  make(chan Envelope, replyQueueSize),
		handlerSlots:   make(chan struct{}, opts.HandlerConcurrency),
		handlerTimeout: opts.HandlerTimeout,
		onTermination:  opts.OnTermination,
	}
	for i := 0; i < opts.HandlerConcurrency; i++ {
		go c.serverRequestWorker()
	}
	go c.serverReplyWorker()
	go c.receiveLoop()
	return c
}

func normalizeClientOptions(opts ClientOptions) ClientOptions {
	if opts.HandlerConcurrency <= 0 {
		opts.HandlerConcurrency = 16
	}
	if opts.HandlerQueue <= 0 {
		opts.HandlerQueue = 256
	}
	if opts.HandlerTimeout <= 0 {
		opts.HandlerTimeout = 60 * time.Second
	}
	return opts
}

func (c *Client) Call(ctx context.Context, method string, params any, result any, trace json.RawMessage) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	id := protocol.StringRequestID(fmt.Sprintf("go-%d", c.nextID.Add(1)))
	key, err := requestIDKey(id)
	if err != nil {
		return err
	}
	waiter := make(chan response, 1)
	if err := c.addWaiter(key, waiter); err != nil {
		return err
	}

	env := Envelope{ID: &id, Method: method, Trace: trace}
	if params != nil {
		data, err := json.Marshal(params)
		if err != nil {
			c.removeWaiter(key)
			return err
		}
		env.Params = data
	}
	frame, err := json.Marshal(env)
	if err != nil {
		c.removeWaiter(key)
		return err
	}
	if err := c.send(ctx, frame); err != nil {
		c.removeWaiter(key)
		return err
	}

	select {
	case item := <-waiter:
		if item.err != nil {
			return item.err
		}
		if item.env.Error != nil {
			return item.env.Error
		}
		if result != nil {
			if len(item.env.Result) == 0 {
				return fmt.Errorf("missing result for %s", method)
			}
			if err := json.Unmarshal(item.env.Result, result); err != nil {
				return err
			}
		}
		return nil
	case <-ctx.Done():
		c.removeWaiter(key)
		return ctx.Err()
	case <-c.done:
		c.removeWaiter(key)
		return c.terminalError()
	}
}

func (c *Client) CallAsync(ctx context.Context, method string, params any, result any, trace json.RawMessage) (<-chan error, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	id := protocol.StringRequestID(fmt.Sprintf("go-%d", c.nextID.Add(1)))
	key, err := requestIDKey(id)
	if err != nil {
		return nil, err
	}
	waiter := make(chan response, 1)
	if err := c.addWaiter(key, waiter); err != nil {
		return nil, err
	}

	env := Envelope{ID: &id, Method: method, Trace: trace}
	if params != nil {
		data, err := json.Marshal(params)
		if err != nil {
			c.removeWaiter(key)
			return nil, err
		}
		env.Params = data
	}
	frame, err := json.Marshal(env)
	if err != nil {
		c.removeWaiter(key)
		return nil, err
	}
	if err := c.send(ctx, frame); err != nil {
		c.removeWaiter(key)
		return nil, err
	}

	done := make(chan error, 1)
	go func() {
		select {
		case item := <-waiter:
			if item.err != nil {
				done <- item.err
				return
			}
			if item.env.Error != nil {
				done <- item.env.Error
				return
			}
			if result != nil {
				if len(item.env.Result) == 0 {
					done <- fmt.Errorf("missing result for %s", method)
					return
				}
				if err := json.Unmarshal(item.env.Result, result); err != nil {
					done <- err
					return
				}
			}
			done <- nil
		case <-c.done:
			c.removeWaiter(key)
			done <- c.terminalError()
		}
	}()
	return done, nil
}

func (c *Client) Notify(ctx context.Context, method string, params any, trace json.RawMessage) error {
	env := Envelope{Method: method, Trace: trace}
	if params != nil {
		data, err := json.Marshal(params)
		if err != nil {
			return err
		}
		env.Params = data
	}
	frame, err := json.Marshal(env)
	if err != nil {
		return err
	}
	return c.send(ctx, frame)
}

func (c *Client) Close() error {
	return c.terminate(&ClosedError{}, terminationExplicitClose)
}

func (c *Client) terminate(waiterErr error, origin terminationOrigin) error {
	c.closeMu.Lock()
	if c.closed {
		done := c.done
		c.closeMu.Unlock()
		<-done
		return nil
	}
	c.closed = true
	c.cancel()
	c.closeMu.Unlock()

	closeErr := c.transport.Close()
	if origin == terminationUnexpected && c.onTermination != nil {
		if terminalErr := c.onTermination(waiterErr); terminalErr != nil {
			waiterErr = terminalErr
		}
	}
	c.failAll(waiterErr)
	c.closeMu.Lock()
	c.terminalErr = waiterErr
	close(c.done)
	c.closeMu.Unlock()
	return closeErr
}

func (c *Client) addWaiter(key string, waiter chan response) error {
	c.waitersMu.Lock()
	defer c.waitersMu.Unlock()
	if c.isClosed() {
		return &ClosedError{}
	}
	c.waiters[key] = waiter
	return nil
}

func (c *Client) removeWaiter(key string) {
	c.waitersMu.Lock()
	defer c.waitersMu.Unlock()
	delete(c.waiters, key)
}

func (c *Client) send(ctx context.Context, frame json.RawMessage) error {
	if c.isClosed() {
		return &ClosedError{}
	}
	select {
	case c.sendLock <- struct{}{}:
		defer func() { <-c.sendLock }()
	case <-ctx.Done():
		return ctx.Err()
	case <-c.done:
		return &ClosedError{}
	}
	if c.isClosed() {
		return &ClosedError{}
	}
	return c.transport.Send(ctx, frame)
}

func (c *Client) receiveLoop() {
	for {
		frame, err := c.transport.Receive(c.ctx)
		if err != nil {
			if c.isClosed() || errors.Is(err, context.Canceled) {
				return
			}
			_ = c.terminate(err, terminationUnexpected)
			return
		}
		var env Envelope
		if err := json.Unmarshal(frame, &env); err != nil {
			_ = c.terminate(err, terminationUnexpected)
			return
		}
		if err := c.route(env); err != nil {
			_ = c.terminate(err, terminationUnexpected)
			return
		}
	}
}

func (c *Client) route(env Envelope) error {
	if err := validateInboundEnvelope(env); err != nil {
		return err
	}
	if env.ID != nil && env.Method == "" {
		key, err := requestIDKey(*env.ID)
		if err != nil {
			return nil
		}
		c.waitersMu.Lock()
		waiter := c.waiters[key]
		delete(c.waiters, key)
		c.waitersMu.Unlock()
		if waiter != nil {
			waiter <- response{env: env}
		}
		return nil
	}
	if env.Method != "" && env.ID != nil {
		select {
		case c.handlerQueue <- env:
		case <-c.done:
		default:
			c.sendServerError(env, -32001, "codex sdk server request queue is full")
		}
		return nil
	}
	if env.Method != "" {
		if handler, ok := c.handler.(ServerNotificationHandler); ok {
			return handler.HandleServerNotification(c.ctx, env.Method, env.Params, env.Trace)
		}
	}
	return nil
}

func validateInboundEnvelope(env Envelope) error {
	if env.idPresent && env.ID == nil {
		return &DecodeError{Reason: "id must not be null"}
	}
	hasID := env.ID != nil
	hasMethod := env.Method != ""
	hasParams := len(env.Params) > 0
	hasResult := len(env.Result) > 0
	hasError := env.Error != nil
	if hasError && (!env.Error.codePresent || !env.Error.messagePresent) {
		return &DecodeError{Reason: "error must contain code and message"}
	}

	switch {
	case hasID && !hasMethod:
		if hasParams || hasResult == hasError {
			return &DecodeError{Reason: "response must contain exactly one of result or error"}
		}
	case hasID && hasMethod:
		if hasResult || hasError {
			return &DecodeError{Reason: "request must not contain result or error"}
		}
	case !hasID && hasMethod:
		if hasResult || hasError {
			return &DecodeError{Reason: "notification must not contain result or error"}
		}
	default:
		return &DecodeError{Reason: "envelope must be a response, request, or notification"}
	}
	return nil
}

func (c *Client) serverRequestWorker() {
	for {
		select {
		case env := <-c.handlerQueue:
			c.handleServerRequest(env)
		case <-c.done:
			return
		}
	}
}

func (c *Client) handleServerRequest(env Envelope) {
	ctx, cancel := context.WithTimeout(c.ctx, c.handlerTimeout)
	defer cancel()

	if c.handler == nil {
		c.queueServerReply(Envelope{
			ID:    env.ID,
			Error: &RPCError{Code: -32000, Message: safeHandlerErrorMessage(ctx, fmt.Errorf("server request handler is not configured"))},
		})
		return
	}
	select {
	case c.handlerSlots <- struct{}{}:
	case <-c.done:
		return
	default:
		c.sendServerError(env, -32001, "codex sdk server request handler concurrency is full")
		return
	}

	resultCh := make(chan handlerResult, 1)
	go func() {
		defer func() { <-c.handlerSlots }()
		result, err := c.handler.HandleServerRequest(ctx, env.Method, env.Params, env.Trace)
		resultCh <- handlerResult{result: result, err: err}
	}()

	var item handlerResult
	select {
	case item = <-resultCh:
	case <-ctx.Done():
		item.err = ctx.Err()
	case <-c.done:
		return
	}
	reply := Envelope{ID: env.ID}
	if item.err != nil {
		reply.Error = &RPCError{Code: -32000, Message: safeHandlerErrorMessage(ctx, item.err)}
	} else if item.result != nil {
		data, marshalErr := json.Marshal(item.result)
		if marshalErr != nil {
			reply.Error = &RPCError{Code: -32000, Message: "codex sdk server request handler result could not be encoded"}
		} else {
			reply.Result = data
		}
	} else {
		reply.Result = []byte("null")
	}
	c.queueServerReply(reply)
}

func (c *Client) sendServerError(env Envelope, code int64, message string) {
	c.queueServerReply(Envelope{ID: env.ID, Error: &RPCError{Code: code, Message: message}})
}

func (c *Client) queueServerReply(reply Envelope) {
	select {
	case c.serverReplies <- reply:
	case <-c.done:
	default:
		_ = c.terminate(&ClosedError{Reason: "server reply queue overflow"}, terminationUnexpected)
	}
}

func (c *Client) serverReplyWorker() {
	for {
		select {
		case reply := <-c.serverReplies:
			frame, err := json.Marshal(reply)
			if err != nil {
				_ = c.terminate(err, terminationUnexpected)
				return
			}
			ctx, cancel := context.WithTimeout(c.ctx, c.handlerTimeout)
			err = c.send(ctx, frame)
			cancel()
			if err != nil && !c.isClosed() {
				_ = c.terminate(err, terminationUnexpected)
				return
			}
		case <-c.done:
			return
		}
	}
}

func safeHandlerErrorMessage(ctx context.Context, err error) string {
	if errors.Is(err, context.DeadlineExceeded) || errors.Is(ctx.Err(), context.DeadlineExceeded) {
		return "codex sdk server request handler timed out"
	}
	if errors.Is(err, context.Canceled) || errors.Is(ctx.Err(), context.Canceled) {
		return "codex sdk server request handler canceled"
	}
	var safe safeError
	if errors.As(err, &safe) {
		return safe.SafeJSONRPCMessage()
	}
	return "codex sdk server request handler failed"
}

func (c *Client) failAll(err error) {
	c.waitersMu.Lock()
	waiters := c.waiters
	c.waiters = map[string]chan response{}
	c.waitersMu.Unlock()
	for _, waiter := range waiters {
		waiter <- response{err: err}
	}
}

func (c *Client) isClosed() bool {
	c.closeMu.Lock()
	defer c.closeMu.Unlock()
	return c.closed
}

func (c *Client) terminalError() error {
	c.closeMu.Lock()
	defer c.closeMu.Unlock()
	if c.terminalErr != nil {
		return c.terminalErr
	}
	return &ClosedError{}
}
