package jsonrpc

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestEnvelopePreservesRequestIDAndLargeErrorCode(t *testing.T) {
	var env Envelope
	data := []byte(`{"id":922337203685477580,"error":{"code":4294967298,"message":"wide"}}`)
	if err := json.Unmarshal(data, &env); err != nil {
		t.Fatal(err)
	}
	if env.ID == nil {
		t.Fatal("id missing")
	}
	encoded, err := json.Marshal(env.ID)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != "922337203685477580" {
		t.Fatalf("encoded id = %s", encoded)
	}
	if env.Error.Code != 4294967298 {
		t.Fatalf("code = %d", env.Error.Code)
	}

	stringID := protocol.StringRequestID("req-1")
	env = Envelope{ID: &stringID}
	encoded, err = json.Marshal(env)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"id":"req-1"}` {
		t.Fatalf("encoded string id = %s", encoded)
	}
}

func TestClientCorrelatesResponsesAndSerializesWrites(t *testing.T) {
	transport := newMemoryTransport()
	client := NewClient(transport, nil)
	defer client.Close()

	const calls = 20
	var wg sync.WaitGroup
	for i := 0; i < calls; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			var result struct {
				OK bool `json:"ok"`
			}
			if err := client.Call(context.Background(), "test/method", nil, &result, nil); err != nil {
				t.Errorf("Call() error = %v", err)
				return
			}
			if !result.OK {
				t.Error("result.OK = false")
			}
		}()
	}

	for i := 0; i < calls; i++ {
		frame := transport.nextSent(t)
		var env Envelope
		if err := json.Unmarshal(frame, &env); err != nil {
			t.Fatal(err)
		}
		if env.ID == nil {
			t.Fatal("request id missing")
		}
		transport.deliver(Envelope{ID: env.ID, Result: json.RawMessage(`{"ok":true}`)})
	}
	wg.Wait()
}

func TestClientCancellationRemovesWaiterAndLateResponseIsDrained(t *testing.T) {
	transport := newMemoryTransport()
	client := NewClient(transport, nil)
	defer client.Close()

	ctx, cancel := context.WithCancel(context.Background())
	errCh := make(chan error, 1)
	go func() {
		var result map[string]any
		errCh <- client.Call(ctx, "slow", nil, &result, nil)
	}()
	frame := transport.nextSent(t)
	cancel()
	if err := <-errCh; !errors.Is(err, context.Canceled) {
		t.Fatalf("err = %v, want context.Canceled", err)
	}
	var env Envelope
	if err := json.Unmarshal(frame, &env); err != nil {
		t.Fatal(err)
	}
	transport.deliver(Envelope{ID: env.ID, Result: json.RawMessage(`{"late":true}`)})

	var result struct {
		OK bool `json:"ok"`
	}
	callDone := make(chan error, 1)
	go func() {
		callDone <- client.Call(context.Background(), "next", nil, &result, nil)
	}()
	next := transport.nextSent(t)
	if err := json.Unmarshal(next, &env); err != nil {
		t.Fatal(err)
	}
	transport.deliver(Envelope{ID: env.ID, Result: json.RawMessage(`{"ok":true}`)})
	if err := <-callDone; err != nil {
		t.Fatal(err)
	}
}

func TestCallAsyncKeepsResponseOwnershipAfterSendContextCancellation(t *testing.T) {
	transport := newMemoryTransport()
	client := NewClient(transport, nil)
	defer client.Close()

	ctx, cancel := context.WithCancel(context.Background())
	var result struct {
		OK bool `json:"ok"`
	}
	done, err := client.CallAsync(ctx, "command/exec", nil, &result, nil)
	if err != nil {
		t.Fatal(err)
	}
	request := decodeEnvelope(t, transport.nextSent(t))
	cancel()
	select {
	case err := <-done:
		t.Fatalf("CallAsync completed after send-context cancellation: %v", err)
	case <-time.After(50 * time.Millisecond):
	}
	transport.deliver(Envelope{ID: request.ID, Result: json.RawMessage(`{"ok":true}`)})
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	if !result.OK {
		t.Fatal("result.OK = false")
	}
}

func TestInvalidInboundEnvelopeTerminatesClientWithDecodeError(t *testing.T) {
	tests := map[string]json.RawMessage{
		"empty":                 json.RawMessage(`{}`),
		"error-missing-code":    json.RawMessage(`{"id":"server-1","error":{"message":"bad"}}`),
		"error-missing-message": json.RawMessage(`{"id":"server-1","error":{"code":1}}`),
		"error-null-code":       json.RawMessage(`{"id":"server-1","error":{"code":null,"message":"bad"}}`),
		"error-null-message":    json.RawMessage(`{"id":"server-1","error":{"code":1,"message":null}}`),
		"notification-null-id":  json.RawMessage(`{"id":null,"method":"server/event"}`),
		"response-without-body": json.RawMessage(`{"id":"server-1"}`),
		"response-with-both":    json.RawMessage(`{"id":"server-1","result":{},"error":{"code":1,"message":"bad"}}`),
		"request-with-result":   json.RawMessage(`{"id":"server-1","method":"server/request","result":{}}`),
	}
	for name, frame := range tests {
		t.Run(name, func(t *testing.T) {
			transport := newMemoryTransport()
			client := NewClient(transport, nil)
			defer client.Close()

			transport.recv <- frame
			select {
			case <-client.done:
			case <-time.After(time.Second):
				t.Fatal("client did not terminate after invalid inbound envelope")
			}
			var decodeErr *DecodeError
			if err := client.terminalError(); !errors.As(err, &decodeErr) {
				t.Fatalf("terminal error = %T(%v), want *DecodeError", err, err)
			}
		})
	}
}

func TestInboundRPCErrorAllowsPresentZeroValues(t *testing.T) {
	var env Envelope
	if err := json.Unmarshal([]byte(`{"id":"server-1","error":{"code":0,"message":""}}`), &env); err != nil {
		t.Fatal(err)
	}
	if err := validateInboundEnvelope(env); err != nil {
		t.Fatalf("validateInboundEnvelope() error = %v", err)
	}
}

func TestCallContextCancellationWhileWaitingForWriteSlot(t *testing.T) {
	transport := newBlockingSendTransport()
	client := NewClient(transport, nil)
	defer client.Close()

	firstDone := make(chan error, 1)
	go func() {
		firstDone <- client.Notify(context.Background(), "hold/write", nil, nil)
	}()
	<-transport.started

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
	defer cancel()
	callDone := make(chan error, 1)
	go func() {
		callDone <- client.Call(ctx, "queued/write", nil, nil, nil)
	}()

	select {
	case err := <-callDone:
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("err = %v, want context deadline", err)
		}
	case <-time.After(500 * time.Millisecond):
		transport.releaseFirst()
		t.Fatal("queued call did not observe context cancellation while waiting for writer")
	}
	if got := client.waiterCount(); got != 0 {
		t.Fatalf("waiters = %d, want 0", got)
	}
	if got := transport.sendCount(); got != 1 {
		t.Fatalf("send count = %d, want only the blocked first write", got)
	}

	transport.releaseFirst()
	if err := <-firstDone; err != nil {
		t.Fatal(err)
	}
}

func TestServerRequestTraceVisibleToHandler(t *testing.T) {
	transport := newMemoryTransport()
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		if method != "server/request" {
			t.Fatalf("method = %s", method)
		}
		if string(trace) != `{"traceparent":"00-abc"}` {
			t.Fatalf("trace = %s", trace)
		}
		return map[string]bool{"ok": true}, nil
	})
	client := NewClient(transport, handler)
	defer client.Close()

	id := protocol.IntRequestID(7)
	transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`), Trace: json.RawMessage(`{"traceparent":"00-abc"}`)})
	frame := transport.nextSent(t)
	var reply Envelope
	if err := json.Unmarshal(frame, &reply); err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(reply.ID)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != "7" {
		t.Fatalf("reply id = %s", encoded)
	}
}

func TestFatalReceiveErrorClosesClientAndRejectsNewCalls(t *testing.T) {
	transport := newMemoryTransport()
	client := NewClient(transport, nil)
	transport.recv <- json.RawMessage(`{`)

	select {
	case <-client.done:
	case <-time.After(2 * time.Second):
		t.Fatal("client did not close after fatal receive error")
	}
	err := client.Call(context.Background(), "after/fatal", nil, nil, nil)
	var closed *ClosedError
	if !errors.As(err, &closed) {
		t.Fatalf("err = %T, want *ClosedError", err)
	}
	select {
	case frame := <-transport.sent:
		t.Fatalf("call wrote after terminal receive failure: %s", frame)
	default:
	}
}

func TestServerRequestHandlerTimeoutAndCloseCancellation(t *testing.T) {
	transport := newMemoryTransport()
	started := make(chan struct{})
	handlerDone := make(chan error, 1)
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		close(started)
		<-ctx.Done()
		handlerDone <- ctx.Err()
		return nil, ctx.Err()
	})
	client := NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     10 * time.Millisecond,
	})
	defer client.Close()

	id := protocol.IntRequestID(1)
	transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
	<-started
	frame := transport.nextSent(t)
	reply := decodeEnvelope(t, frame)
	if reply.Error == nil || reply.Error.Message != "codex sdk server request handler timed out" {
		t.Fatalf("reply error = %#v", reply.Error)
	}
	if err := <-handlerDone; !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("handler err = %v, want context deadline", err)
	}

	transport = newMemoryTransport()
	started = make(chan struct{})
	handlerDone = make(chan error, 1)
	handler = handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		close(started)
		<-ctx.Done()
		handlerDone <- ctx.Err()
		return nil, ctx.Err()
	})
	client = NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     time.Second,
	})
	id = protocol.IntRequestID(2)
	transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
	<-started
	if err := client.Close(); err != nil {
		t.Fatal(err)
	}
	if err := <-handlerDone; !errors.Is(err, context.Canceled) {
		t.Fatalf("handler err = %v, want context canceled", err)
	}
}

func TestServerRequestHandlerTimeoutRepliesWhenHandlerIgnoresContext(t *testing.T) {
	transport := newMemoryTransport()
	started := make(chan struct{})
	release := make(chan struct{})
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		close(started)
		<-release
		return map[string]bool{"late": true}, nil
	})
	client := NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     10 * time.Millisecond,
	})
	defer func() {
		close(release)
		_ = client.Close()
	}()

	id := protocol.IntRequestID(10)
	transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
	<-started
	frame := transport.nextSent(t)
	reply := decodeEnvelope(t, frame)
	if reply.Error == nil || reply.Error.Message != "codex sdk server request handler timed out" {
		t.Fatalf("reply error = %#v", reply.Error)
	}
}

func TestServerRequestHandlerQueueIsBounded(t *testing.T) {
	transport := newMemoryTransport()
	started := make(chan struct{})
	release := make(chan struct{})
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		select {
		case started <- struct{}{}:
		default:
		}
		<-release
		return map[string]bool{"ok": true}, nil
	})
	client := NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     time.Second,
	})
	defer client.Close()

	for i := 1; i <= 3; i++ {
		id := protocol.IntRequestID(int64(i))
		transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
		if i == 1 {
			<-started
		}
	}
	frame := transport.nextSent(t)
	reply := decodeEnvelope(t, frame)
	if reply.Error == nil || reply.Error.Message != "codex sdk server request queue is full" {
		t.Fatalf("reply error = %#v", reply.Error)
	}
	encoded, err := json.Marshal(reply.ID)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != "3" {
		t.Fatalf("reply id = %s, want 3", encoded)
	}
	close(release)
}

func TestServerRequestQueueOverloadDoesNotBlockResponseRouting(t *testing.T) {
	transport := newMemoryTransport()
	started := make(chan struct{})
	release := make(chan struct{})
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		select {
		case <-started:
		default:
			close(started)
		}
		<-release
		return nil, nil
	})
	client := NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     time.Second,
	})
	defer func() {
		close(release)
		_ = client.Close()
	}()

	callDone := make(chan error, 1)
	go func() {
		callDone <- client.Call(context.Background(), "client/request", nil, nil, nil)
	}()
	request := decodeEnvelope(t, transport.nextSent(t))
	for range cap(transport.sent) {
		transport.sent <- json.RawMessage(`{}`)
	}

	for i := 1; i <= 3; i++ {
		id := protocol.IntRequestID(int64(i))
		transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
		if i == 1 {
			<-started
		}
	}
	transport.deliver(Envelope{ID: request.ID, Result: json.RawMessage(`{}`)})

	select {
	case err := <-callDone:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("response routing blocked behind an overloaded server-request reply")
	}
}

func TestServerRequestReplyBacklogKeepsConnectionUsable(t *testing.T) {
	transport := newGatedMemoryTransport()
	started := make(chan struct{})
	release := make(chan struct{})
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		select {
		case <-started:
		default:
			close(started)
		}
		<-release
		return map[string]bool{"ok": true}, nil
	})
	client := NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     2 * time.Second,
	})
	defer client.Close()

	for i := 1; i <= 8; i++ {
		id := protocol.IntRequestID(int64(i))
		transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
		if i == 1 {
			<-started
		}
		if i == 3 {
			<-transport.sendStarted
		}
	}

	callDone := make(chan error, 1)
	go func() {
		callDone <- client.Call(context.Background(), "client/request", nil, nil, nil)
	}()
	waitForCondition(t, func() bool { return client.waiterCount() == 1 }, "client request waiter was not registered")
	callID := protocol.StringRequestID("go-1")
	transport.deliver(Envelope{ID: &callID, Result: json.RawMessage(`{}`)})
	waitForCondition(t, func() bool { return client.waiterCount() == 0 }, "client response was not routed while the writer was blocked")

	transport.releaseSends()
	select {
	case err := <-callDone:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("client request did not complete after the writer was released")
	}
	close(release)

	wantReplies := map[string]bool{}
	for i := 1; i <= 8; i++ {
		wantReplies[fmt.Sprint(i)] = i >= 3
	}
	for len(wantReplies) > 0 {
		reply := decodeEnvelope(t, transport.nextSent(t))
		if reply.ID == nil {
			continue
		}
		encoded, err := json.Marshal(reply.ID)
		if err != nil {
			t.Fatal(err)
		}
		id := string(encoded)
		wantBusy, ok := wantReplies[id]
		if !ok {
			continue
		}
		if wantBusy && (reply.Error == nil || reply.Error.Code != -32001) {
			t.Fatalf("overload reply %s error = %#v", id, reply.Error)
		}
		delete(wantReplies, id)
	}

	laterID := protocol.IntRequestID(100)
	transport.deliver(Envelope{ID: &laterID, Method: "server/request", Params: json.RawMessage(`{}`)})
	for {
		reply := decodeEnvelope(t, transport.nextSent(t))
		if reply.ID == nil {
			continue
		}
		encoded, err := json.Marshal(reply.ID)
		if err != nil {
			t.Fatal(err)
		}
		if string(encoded) == "100" {
			if reply.Error != nil {
				t.Fatalf("later reply error = %#v", reply.Error)
			}
			return
		}
	}
}

func TestServerRequestHandlerErrorIsRedacted(t *testing.T) {
	transport := newMemoryTransport()
	handler := handlerFunc(func(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
		return nil, errors.New("token=super-secret auth cookie")
	})
	client := NewClientWithOptions(transport, handler, ClientOptions{
		HandlerConcurrency: 1,
		HandlerQueue:       1,
		HandlerTimeout:     time.Second,
	})
	defer client.Close()

	id := protocol.IntRequestID(8)
	transport.deliver(Envelope{ID: &id, Method: "server/request", Params: json.RawMessage(`{}`)})
	frame := transport.nextSent(t)
	for _, leaked := range []string{"token", "super-secret", "auth", "cookie"} {
		if strings.Contains(string(frame), leaked) {
			t.Fatalf("handler error leaked %q in frame: %s", leaked, frame)
		}
	}
	reply := decodeEnvelope(t, frame)
	if reply.Error == nil || reply.Error.Message != "codex sdk server request handler failed" {
		t.Fatalf("reply error = %#v", reply.Error)
	}
}

func decodeEnvelope(tb testing.TB, frame json.RawMessage) Envelope {
	tb.Helper()
	var env Envelope
	if err := json.Unmarshal(frame, &env); err != nil {
		tb.Fatal(err)
	}
	return env
}

type handlerFunc func(context.Context, string, json.RawMessage, json.RawMessage) (any, error)

func (f handlerFunc) HandleServerRequest(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
	return f(ctx, method, params, trace)
}

type memoryTransport struct {
	recv   chan json.RawMessage
	sent   chan json.RawMessage
	closed chan struct{}
	once   sync.Once
}

type gatedMemoryTransport struct {
	*memoryTransport
	sendStarted chan struct{}
	sendRelease chan struct{}
	startOnce   sync.Once
	releaseOnce sync.Once
}

func newGatedMemoryTransport() *gatedMemoryTransport {
	return &gatedMemoryTransport{
		memoryTransport: newMemoryTransport(),
		sendStarted:     make(chan struct{}),
		sendRelease:     make(chan struct{}),
	}
}

func (t *gatedMemoryTransport) Send(ctx context.Context, frame json.RawMessage) error {
	t.startOnce.Do(func() { close(t.sendStarted) })
	select {
	case <-t.sendRelease:
		return t.memoryTransport.Send(ctx, frame)
	case <-ctx.Done():
		return ctx.Err()
	case <-t.closed:
		return &ClosedError{}
	}
}

func (t *gatedMemoryTransport) releaseSends() {
	t.releaseOnce.Do(func() { close(t.sendRelease) })
}

func newMemoryTransport() *memoryTransport {
	return &memoryTransport{
		recv:   make(chan json.RawMessage, 32),
		sent:   make(chan json.RawMessage, 32),
		closed: make(chan struct{}),
	}
}

func (t *memoryTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	select {
	case frame := <-t.recv:
		return frame, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-t.closed:
		return nil, &ClosedError{}
	}
}

func (t *memoryTransport) Send(ctx context.Context, frame json.RawMessage) error {
	select {
	case t.sent <- append(json.RawMessage(nil), frame...):
		return nil
	case <-ctx.Done():
		return ctx.Err()
	case <-t.closed:
		return &ClosedError{}
	}
}

func (t *memoryTransport) Close() error {
	t.once.Do(func() { close(t.closed) })
	return nil
}

func (t *memoryTransport) deliver(env Envelope) {
	data, err := json.Marshal(env)
	if err != nil {
		panic(err)
	}
	t.recv <- data
}

func (t *memoryTransport) nextSent(tb testing.TB) json.RawMessage {
	tb.Helper()
	select {
	case frame := <-t.sent:
		return frame
	case <-time.After(2 * time.Second):
		tb.Fatal("timed out waiting for sent frame")
	}
	return nil
}

type blockingSendTransport struct {
	started chan struct{}
	release chan struct{}
	closed  chan struct{}

	once sync.Once
	mu   sync.Mutex
	sent int
}

func newBlockingSendTransport() *blockingSendTransport {
	return &blockingSendTransport{
		started: make(chan struct{}),
		release: make(chan struct{}),
		closed:  make(chan struct{}),
	}
}

func (t *blockingSendTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-t.closed:
		return nil, &ClosedError{}
	}
}

func (t *blockingSendTransport) Send(ctx context.Context, frame json.RawMessage) error {
	t.mu.Lock()
	t.sent++
	count := t.sent
	t.mu.Unlock()
	if count != 1 {
		if err := ctx.Err(); err != nil {
			return err
		}
		return nil
	}
	t.once.Do(func() { close(t.started) })
	select {
	case <-t.release:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	case <-t.closed:
		return &ClosedError{}
	}
}

func (t *blockingSendTransport) Close() error {
	select {
	case <-t.closed:
	default:
		close(t.closed)
	}
	return nil
}

func (t *blockingSendTransport) releaseFirst() {
	select {
	case <-t.release:
	default:
		close(t.release)
	}
}

func (t *blockingSendTransport) sendCount() int {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.sent
}

func (c *Client) waiterCount() int {
	c.waitersMu.Lock()
	defer c.waitersMu.Unlock()
	return len(c.waiters)
}

func waitForCondition(tb testing.TB, condition func() bool, failure string) {
	tb.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if condition() {
			return
		}
		time.Sleep(time.Millisecond)
	}
	tb.Fatal(failure)
}
