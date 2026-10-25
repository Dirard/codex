// Package codextest provides an in-process fake Codex app-server for tests of code that
// uses the Go SDK.
//
// A Server implements the codex.Transport contract, so it is passed to codex.NewClient as
// ClientConfig.Transport. It answers the initialize handshake with the protocol digests of
// this SDK build, answers requests from a per-method script, and lets the test send
// notifications and server requests:
//
//	srv := codextest.NewServer()
//	defer srv.Close()
//	srv.Respond("thread/start", codextest.ThreadStartResponse("thread-1"))
//	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
//
// The package does not import the root codex package, so the SDK's own tests can use it.
package codextest

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sync"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

// JSON-RPC error codes the fake server uses.
const (
	CodeInvalidRequest int64 = -32600
	CodeMethodNotFound int64 = -32601
	CodeInternalError  int64 = -32603
)

// DefaultUserAgent is the userAgent the fake server reports in initialize. It has the shape
// of a dev runtime's userAgent.
const DefaultUserAgent = "codex_app_server/0.0.0 (Linux 7.0; x86_64) codextest (codex_go_sdk; 0.0.0-dev)"

// ErrClosed is returned by Server methods after Close or Disconnect.
var ErrClosed = errors.New("codextest: server closed")

// Error is a JSON-RPC error. A HandlerFunc returns it to answer with a specific code;
// Request returns it when the client answered a server request with an error.
type Error struct {
	Code    int64
	Message string
	Data    json.RawMessage
}

func (e *Error) Error() string {
	return fmt.Sprintf("codextest: JSON-RPC error %d: %s", e.Code, e.Message)
}

// Request is a JSON-RPC request or notification the client sent.
type Request struct {
	// ID is the request id; it is zero for notifications.
	ID     protocol.RequestID
	Method string
	Params json.RawMessage
	Trace  json.RawMessage
}

// Decode unmarshals the request params into v.
func (r Request) Decode(v any) error {
	if len(r.Params) == 0 {
		return json.Unmarshal([]byte("null"), v)
	}
	return json.Unmarshal(r.Params, v)
}

// Call is a client request being answered by a HandlerFunc.
type Call struct {
	Request
	after []json.RawMessage
}

// NotifyAfter queues a notification that the server sends right after the response to
// this call, before anything else the call's handler did not queue.
func (c *Call) NotifyAfter(method string, params any) error {
	frame, err := encodeNotification(method, params)
	if err != nil {
		return err
	}
	c.after = append(c.after, frame)
	return nil
}

// HandlerFunc answers one client request. It returns the result, which is marshaled to
// JSON (nil becomes an empty object), or an error: an *Error is sent as is and any other
// error as an internal error. ctx ends when the server closes.
type HandlerFunc func(ctx context.Context, call *Call) (any, error)

// Option configures a Server.
type Option func(*Server)

// WithUserAgent sets the userAgent reported in initialize.
func WithUserAgent(userAgent string) Option {
	return func(s *Server) { s.userAgent = userAgent }
}

// WithInitializeResponse replaces the whole initialize response, for example to test how
// the client handles a runtime with other protocol digests.
func WithInitializeResponse(response protocol.InitializeResponse) Option {
	return func(s *Server) { s.initializeResponse = &response }
}

// Server is an in-process fake app-server. All methods are safe for concurrent use.
type Server struct {
	userAgent          string
	initializeResponse *protocol.InitializeResponse

	ctx    context.Context
	cancel context.CancelFunc

	mu             sync.Mutex
	outbound       []json.RawMessage
	wake           chan struct{}
	closed         bool
	disconnected   error
	script         map[string]scriptEntry
	requests       []Request
	notifications  []Request
	changed        chan struct{}
	unhandled      []string
	violations     []error
	initParams     *protocol.InitializeParams
	initializeSeen bool
	initialized    bool
	initializedCh  chan struct{}
	serverRequests map[string]chan serverReply
	nextServerID   int64
}

// scriptEntry answers one method; inline entries answer synchronously, so their response
// keeps its place relative to messages the test queues after seeing the request.
type scriptEntry struct {
	handler HandlerFunc
	inline  bool
}

type serverReply struct {
	result json.RawMessage
	err    error
}

// NewServer returns a fake app-server with an empty script.
func NewServer(opts ...Option) *Server {
	ctx, cancel := context.WithCancel(context.Background())
	s := &Server{
		userAgent:      DefaultUserAgent,
		ctx:            ctx,
		cancel:         cancel,
		wake:           make(chan struct{}, 1),
		script:         map[string]scriptEntry{},
		changed:        make(chan struct{}),
		initializedCh:  make(chan struct{}),
		serverRequests: map[string]chan serverReply{},
	}
	for _, opt := range opts {
		opt(s)
	}
	return s
}

// Handle scripts method with a handler. Each request runs its handler in a goroutine, so a
// handler may block, for example to hold a response back.
func (s *Server) Handle(method string, handler HandlerFunc) {
	s.setScript(method, scriptEntry{handler: handler})
}

// Respond scripts method with a fixed result, sent as soon as the request arrives.
func (s *Server) Respond(method string, result any) {
	s.setScript(method, scriptEntry{inline: true, handler: func(context.Context, *Call) (any, error) {
		return result, nil
	}})
}

// RespondError scripts method with a JSON-RPC error.
func (s *Server) RespondError(method string, code int64, message string) {
	s.setScript(method, scriptEntry{inline: true, handler: func(context.Context, *Call) (any, error) {
		return nil, &Error{Code: code, Message: message}
	}})
}

func (s *Server) setScript(method string, entry scriptEntry) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.script[method] = entry
}

// Notify sends a notification to the client. Messages reach the client in the order they
// were queued; Notify never waits for the client to read.
func (s *Server) Notify(method string, params any) error {
	frame, err := encodeNotification(method, params)
	if err != nil {
		return err
	}
	return s.enqueue(frame)
}

// Request sends a server request, such as an approval, and returns the client's result.
// A client error response is returned as *Error.
func (s *Server) Request(ctx context.Context, method string, params any) (json.RawMessage, error) {
	data, err := encodeParams(params)
	if err != nil {
		return nil, err
	}
	s.mu.Lock()
	s.nextServerID++
	id := protocol.StringRequestID(fmt.Sprintf("codextest-%d", s.nextServerID))
	key, err := requestKey(id)
	if err != nil {
		s.mu.Unlock()
		return nil, err
	}
	reply := make(chan serverReply, 1)
	s.serverRequests[key] = reply
	s.mu.Unlock()
	forget := func() {
		s.mu.Lock()
		delete(s.serverRequests, key)
		s.mu.Unlock()
	}
	frame, err := json.Marshal(jsonrpc.Envelope{ID: &id, Method: method, Params: data})
	if err != nil {
		forget()
		return nil, err
	}
	if err := s.enqueue(frame); err != nil {
		forget()
		return nil, err
	}
	select {
	case answer := <-reply:
		return answer.result, answer.err
	case <-ctx.Done():
		forget()
		return nil, ctx.Err()
	case <-s.ctx.Done():
		forget()
		return nil, ErrClosed
	}
}

// Requests returns the client requests received so far, initialize included, in order.
func (s *Server) Requests() []Request {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]Request(nil), s.requests...)
}

// Notifications returns the client notifications received so far, such as initialized.
func (s *Server) Notifications() []Request {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]Request(nil), s.notifications...)
}

// WaitForRequest returns the first request with method, waiting until it arrives.
func (s *Server) WaitForRequest(ctx context.Context, method string) (Request, error) {
	for {
		s.mu.Lock()
		for _, request := range s.requests {
			if request.Method == method {
				s.mu.Unlock()
				return request, nil
			}
		}
		changed := s.changed
		s.mu.Unlock()
		select {
		case <-changed:
		case <-ctx.Done():
			return Request{}, ctx.Err()
		case <-s.ctx.Done():
			return Request{}, ErrClosed
		}
	}
}

// InitializeParams returns the params of the client's initialize request, once received.
func (s *Server) InitializeParams() (protocol.InitializeParams, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.initParams == nil {
		return protocol.InitializeParams{}, false
	}
	return *s.initParams, true
}

// WaitInitialized waits for the client's initialized notification.
func (s *Server) WaitInitialized(ctx context.Context) error {
	select {
	case <-s.initializedCh:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	case <-s.ctx.Done():
		return ErrClosed
	}
}

// Unhandled returns the methods the client called without a scripted answer; each got
// error CodeMethodNotFound.
func (s *Server) Unhandled() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]string(nil), s.unhandled...)
}

// Err reports protocol violations by the client so far, or nil: a request before
// initialize, initialize or initialized sent twice, initialized before initialize, or a
// request or notification between initialize and initialized. A client that never sends
// initialized and nothing else afterwards is not reported; use WaitInitialized for that.
func (s *Server) Err() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return errors.Join(s.violations...)
}

// Disconnect simulates the runtime going away: the client receives the messages already
// queued and then err, or io.EOF when err is nil, as if the runtime exited.
func (s *Server) Disconnect(err error) {
	if err == nil {
		err = io.EOF
	}
	s.mu.Lock()
	if s.disconnected == nil && !s.closed {
		s.disconnected = err
	}
	s.mu.Unlock()
	s.cancel()
	s.signal()
}

// Close closes the transport. Receive and Send fail afterwards, and running handlers see
// their ctx end.
func (s *Server) Close() error {
	s.mu.Lock()
	s.closed = true
	s.outbound = nil
	s.mu.Unlock()
	s.cancel()
	s.signal()
	return nil
}

// Receive implements codex.Transport: it returns the next message for the client.
func (s *Server) Receive(ctx context.Context) (json.RawMessage, error) {
	for {
		s.mu.Lock()
		if s.closed {
			s.mu.Unlock()
			return nil, &jsonrpc.ClosedError{}
		}
		if len(s.outbound) > 0 {
			frame := s.outbound[0]
			s.outbound[0] = nil
			s.outbound = s.outbound[1:]
			s.mu.Unlock()
			return frame, nil
		}
		if s.disconnected != nil {
			err := s.disconnected
			s.mu.Unlock()
			return nil, err
		}
		s.mu.Unlock()
		select {
		case <-s.wake:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
}

// Send implements codex.Transport: it takes one message from the client.
func (s *Server) Send(_ context.Context, frame json.RawMessage) error {
	var env jsonrpc.Envelope
	if err := json.Unmarshal(frame, &env); err != nil {
		s.violation(fmt.Errorf("client sent an invalid frame: %w", err))
		return err
	}
	s.mu.Lock()
	if s.closed || s.disconnected != nil {
		s.mu.Unlock()
		return &jsonrpc.ClosedError{}
	}
	s.mu.Unlock()
	switch {
	case env.ID != nil && env.Method != "":
		s.handleRequest(Request{ID: *env.ID, Method: env.Method, Params: clone(env.Params), Trace: clone(env.Trace)})
	case env.ID != nil:
		s.handleReply(*env.ID, env)
	case env.Method != "":
		s.handleNotification(Request{Method: env.Method, Params: clone(env.Params), Trace: clone(env.Trace)})
	default:
		s.violation(fmt.Errorf("client sent a frame that is neither request, response nor notification"))
	}
	return nil
}

func (s *Server) handleRequest(request Request) {
	s.mu.Lock()
	s.requests = append(s.requests, request)
	close(s.changed)
	s.changed = make(chan struct{})
	switch {
	case request.Method == "initialize":
		s.mu.Unlock()
		s.answerInitialize(request)
		return
	case !s.initializeSeen:
		s.violations = append(s.violations, fmt.Errorf("client sent %s before initialize", request.Method))
		s.mu.Unlock()
		s.reply(request.ID, nil, &Error{Code: CodeInvalidRequest, Message: "Not initialized"}, nil)
		return
	case !s.initialized:
		s.violations = append(s.violations, fmt.Errorf("client sent %s before the initialized notification", request.Method))
	}
	entry, scripted := s.script[request.Method]
	if !scripted {
		s.unhandled = append(s.unhandled, request.Method)
		s.mu.Unlock()
		s.reply(request.ID, nil, &Error{Code: CodeMethodNotFound, Message: "codextest: no answer scripted for " + request.Method}, nil)
		return
	}
	s.mu.Unlock()
	call := &Call{Request: request}
	if entry.inline {
		result, err := entry.handler(s.ctx, call)
		s.reply(request.ID, result, err, call.after)
		return
	}
	go func() {
		result, err := entry.handler(s.ctx, call)
		s.reply(request.ID, result, err, call.after)
	}()
}

func (s *Server) answerInitialize(request Request) {
	var params protocol.InitializeParams
	paramsErr := request.Decode(&params)
	s.mu.Lock()
	if s.initializeSeen {
		s.violations = append(s.violations, errors.New("client sent initialize twice"))
	}
	s.initializeSeen = true
	if paramsErr != nil {
		s.violations = append(s.violations, fmt.Errorf("invalid initialize params: %w", paramsErr))
	} else {
		s.initParams = &params
	}
	response := s.defaultInitializeResponse(params)
	if s.initializeResponse != nil {
		response = *s.initializeResponse
	}
	s.mu.Unlock()
	s.reply(request.ID, response, nil, nil)
}

// defaultInitializeResponse matches this SDK build and the mode the client asked for, as
// the runtime of the same build would.
func (s *Server) defaultInitializeResponse(params protocol.InitializeParams) protocol.InitializeResponse {
	mode := protocol.ActiveProtocolModeStable
	if capabilities, ok := params.Capabilities.Value(); ok {
		if experimental, ok := capabilities.ExperimentalAPI.Value(); ok && experimental {
			mode = protocol.ActiveProtocolModeExperimental
		}
	}
	return protocol.InitializeResponse{
		ActiveProtocolMode:         mode,
		CodexHome:                  "/tmp/codextest",
		PlatformFamily:             "unix",
		PlatformOs:                 "linux",
		UserAgent:                  s.userAgent,
		StableProtocolDigest:       protocol.StableProtocolDigest,
		ExperimentalProtocolDigest: protocol.ExperimentalProtocolDigest,
		StableSchemaDigest:         protocol.StableSchemaDigest,
		ExperimentalSchemaDigest:   protocol.ExperimentalSchemaDigest,
		StableManifestDigest:       protocol.StableManifestDigest,
		ExperimentalManifestDigest: protocol.ExperimentalManifestDigest,
	}
}

func (s *Server) handleNotification(notification Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.notifications = append(s.notifications, notification)
	if notification.Method != "initialized" {
		if s.initializeSeen && !s.initialized {
			s.violations = append(s.violations, fmt.Errorf("client sent %s before the initialized notification", notification.Method))
		}
		return
	}
	switch {
	case !s.initializeSeen:
		s.violations = append(s.violations, errors.New("client sent initialized before initialize"))
	case s.initialized:
		s.violations = append(s.violations, errors.New("client sent initialized twice"))
	default:
		s.initialized = true
		close(s.initializedCh)
	}
}

func (s *Server) handleReply(id protocol.RequestID, env jsonrpc.Envelope) {
	key, err := requestKey(id)
	if err != nil {
		s.violation(fmt.Errorf("client replied with an invalid id: %w", err))
		return
	}
	s.mu.Lock()
	reply := s.serverRequests[key]
	delete(s.serverRequests, key)
	s.mu.Unlock()
	if reply == nil {
		s.violation(fmt.Errorf("client replied to unknown server request %s", key))
		return
	}
	if env.Error != nil {
		reply <- serverReply{err: &Error{Code: env.Error.Code, Message: env.Error.Message, Data: clone(env.Error.Data)}}
		return
	}
	reply <- serverReply{result: clone(env.Result)}
}

func (s *Server) reply(id protocol.RequestID, result any, err error, after []json.RawMessage) {
	env := jsonrpc.Envelope{ID: &id}
	if err != nil {
		var rpcErr *Error
		if !errors.As(err, &rpcErr) {
			rpcErr = &Error{Code: CodeInternalError, Message: err.Error()}
		}
		env.Error = &jsonrpc.RPCError{Code: rpcErr.Code, Message: rpcErr.Message, Data: rpcErr.Data}
	} else {
		data, marshalErr := encodeResult(result)
		if marshalErr != nil {
			env.Error = &jsonrpc.RPCError{Code: CodeInternalError, Message: "codextest: cannot encode result: " + marshalErr.Error()}
		} else {
			env.Result = data
		}
	}
	frame, marshalErr := json.Marshal(env)
	if marshalErr != nil {
		s.violation(fmt.Errorf("cannot encode response: %w", marshalErr))
		return
	}
	_ = s.enqueue(append([]json.RawMessage{frame}, after...)...)
}

func (s *Server) enqueue(frames ...json.RawMessage) error {
	s.mu.Lock()
	if s.closed || s.disconnected != nil {
		s.mu.Unlock()
		return ErrClosed
	}
	s.outbound = append(s.outbound, frames...)
	s.mu.Unlock()
	s.signal()
	return nil
}

func (s *Server) signal() {
	select {
	case s.wake <- struct{}{}:
	default:
	}
}

func (s *Server) violation(err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.violations = append(s.violations, err)
}

func encodeNotification(method string, params any) (json.RawMessage, error) {
	data, err := encodeParams(params)
	if err != nil {
		return nil, err
	}
	return json.Marshal(jsonrpc.Envelope{Method: method, Params: data})
}

func encodeParams(params any) (json.RawMessage, error) {
	if params == nil {
		return nil, nil
	}
	if raw, ok := params.(json.RawMessage); ok {
		return clone(raw), nil
	}
	return json.Marshal(params)
}

func encodeResult(result any) (json.RawMessage, error) {
	if result == nil {
		return json.RawMessage(`{}`), nil
	}
	if raw, ok := result.(json.RawMessage); ok {
		return clone(raw), nil
	}
	return json.Marshal(result)
}

func requestKey(id protocol.RequestID) (string, error) {
	data, err := json.Marshal(id)
	if err != nil {
		return "", err
	}
	return string(data), nil
}

func clone(data []byte) json.RawMessage {
	if data == nil {
		return nil
	}
	return append(json.RawMessage(nil), data...)
}
