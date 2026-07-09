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
	HandleServerNotification(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage)
}

type safeError interface {
	SafeJSONRPCMessage() string
}

type ClientOptions struct {
	HandlerConcurrency int
	HandlerQueue       int
	HandlerTimeout     time.Duration
}

type Client struct {
	transport Transport
	handler   ServerRequestHandler

	sendLock chan struct{}

	waitersMu sync.Mutex
	waiters   map[string]chan response
	nextID    atomic.Uint64

	closeMu sync.Mutex
	closed  bool
	done    chan struct{}
	ctx     context.Context
	cancel  context.CancelFunc

	handlerQueue   chan Envelope
	handlerSlots   chan struct{}
	handlerTimeout time.Duration
}

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
	c := &Client{
		transport:      transport,
		handler:        handler,
		sendLock:       make(chan struct{}, 1),
		waiters:        map[string]chan response{},
		done:           make(chan struct{}),
		ctx:            ctx,
		cancel:         cancel,
		handlerQueue:   make(chan Envelope, opts.HandlerQueue),
		handlerSlots:   make(chan struct{}, opts.HandlerConcurrency),
		handlerTimeout: opts.HandlerTimeout,
	}
	for i := 0; i < opts.HandlerConcurrency; i++ {
		go c.serverRequestWorker()
	}
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
		return &ClosedError{}
	}
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
	return c.terminate(&ClosedError{}, true)
}

func (c *Client) terminate(waiterErr error, closeTransport bool) error {
	c.closeMu.Lock()
	if c.closed {
		c.closeMu.Unlock()
		return nil
	}
	c.closed = true
	c.cancel()
	close(c.done)
	c.closeMu.Unlock()

	var closeErr error
	if closeTransport {
		closeErr = c.transport.Close()
	}
	c.failAll(waiterErr)
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
			_ = c.terminate(err, true)
			return
		}
		var env Envelope
		if err := json.Unmarshal(frame, &env); err != nil {
			_ = c.terminate(err, true)
			return
		}
		c.route(env)
	}
}

func (c *Client) route(env Envelope) {
	if env.ID != nil && env.Method == "" {
		key, err := requestIDKey(*env.ID)
		if err != nil {
			return
		}
		c.waitersMu.Lock()
		waiter := c.waiters[key]
		delete(c.waiters, key)
		c.waitersMu.Unlock()
		if waiter != nil {
			waiter <- response{env: env}
		}
		return
	}
	if env.Method != "" && env.ID != nil {
		select {
		case c.handlerQueue <- env:
		case <-c.done:
		default:
			c.sendServerError(env, -32001, "codex sdk server request queue is full")
		}
		return
	}
	if env.Method != "" {
		if handler, ok := c.handler.(ServerNotificationHandler); ok {
			handler.HandleServerNotification(c.ctx, env.Method, env.Params, env.Trace)
		}
	}
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
		c.sendServerReply(Envelope{
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
	c.sendServerReply(reply)
}

func (c *Client) sendServerError(env Envelope, code int64, message string) {
	c.sendServerReply(Envelope{ID: env.ID, Error: &RPCError{Code: code, Message: message}})
}

func (c *Client) sendServerReply(reply Envelope) {
	frame, err := json.Marshal(reply)
	if err != nil {
		return
	}
	ctx, cancel := context.WithTimeout(c.ctx, c.handlerTimeout)
	defer cancel()
	_ = c.send(ctx, frame)
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
