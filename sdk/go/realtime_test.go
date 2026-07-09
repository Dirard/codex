package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestRealtimeSessionInjectsThreadIdentity(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)

	session, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{
		ThreadID: "thread-1",
		Model:    "gpt-5.4",
		Prompt:   "speak plainly",
	})
	if err != nil {
		t.Fatal(err)
	}
	if session.ID() == "" {
		t.Fatal("realtime session id is empty")
	}
	startParams := requestParamsForMethod(t, transport, "thread/realtime/start")
	assertRequestThreadID(t, startParams, "thread-1")
	assertRealtimeSessionID(t, startParams, session.ID())
	assertRealtimeOutputModality(t, startParams, "audio")

	calls := []struct {
		name   string
		method string
		call   func() error
	}{
		{
			name: "append audio", method: "thread/realtime/appendAudio",
			call: func() error { return session.AppendAudio(ctx, AudioChunk{Data: "base64-audio"}) },
		},
		{
			name: "append text", method: "thread/realtime/appendText",
			call: func() error { return session.AppendText(ctx, "hello") },
		},
		{
			name: "append speech", method: "thread/realtime/appendSpeech",
			call: func() error { return session.AppendSpeech(ctx, SpeechInput{Text: "hello"}) },
		},
	}
	for _, tt := range calls {
		t.Run(tt.name, func(t *testing.T) {
			failMethod(transport, tt.method)
			err := tt.call()
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T, want *RPCError", err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
			assertRequestThreadID(t, requestParamsForMethod(t, transport, tt.method), "thread-1")
		})
	}
}

func TestRealtimeStartConflictsPerThreadUntilClosedAfterStop(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)
	transport.responses["thread/realtime/stop"] = json.RawMessage(`{}`)

	session, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := session.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	_, _, err = client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	var conflict *ConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("err = %T, want *ConflictError", err)
	}
	if got := methodCount(t, transport, "thread/realtime/start"); got != 1 {
		t.Fatalf("thread/realtime/start sent %d times, want 1", got)
	}
	if err := session.Stop(ctx); err != nil {
		t.Fatal(err)
	}
	assertRealtimeStreamClosed(t, stream)
	_, _, err = client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if !errors.As(err, &conflict) {
		t.Fatalf("err = %T, want *ConflictError before realtime closed", err)
	}
	if got := methodCount(t, transport, "thread/realtime/start"); got != 1 {
		t.Fatalf("thread/realtime/start sent %d times before closed, want 1", got)
	}

	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	if _, err := waitForRealtimeStart(ctx, client, "thread-1"); err != nil {
		t.Fatalf("start after realtime closed: %v", err)
	}
	if got := methodCount(t, transport, "thread/realtime/start"); got != 2 {
		t.Fatalf("thread/realtime/start sent %d times after closed, want 2", got)
	}
}

func TestRealtimeStreamsAreThreadScoped(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)

	first, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	second, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-2"})
	if err != nil {
		t.Fatal(err)
	}
	firstStream, err := first.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer firstStream.Close()
	secondStream, err := second.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer secondStream.Close()

	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)

	notification, ok := firstStream.Next(ctx)
	if !ok {
		t.Fatalf("first stream closed: %v", firstStream.Err())
	}
	if notification.Method != "thread/realtime/started" {
		t.Fatalf("notification method = %q", notification.Method)
	}
	timeout, cancel := context.WithTimeout(ctx, 20*time.Millisecond)
	defer cancel()
	if notification, ok := secondStream.Next(timeout); ok {
		t.Fatalf("second stream received cross-thread notification: %#v", notification)
	}
}

func TestRealtimeClosedNotificationReleasesSessionAndClosesStream(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)

	session, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := session.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-2"}`), nil)
	_, _, err = client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	var conflict *ConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("err = %T, want *ConflictError after other thread close", err)
	}
	if got := methodCount(t, transport, "thread/realtime/start"); got != 1 {
		t.Fatalf("thread/realtime/start sent %d times after other thread close, want 1", got)
	}

	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	if notification, ok := stream.Next(ctx); !ok {
		t.Fatalf("stream closed before delivering terminal notification: %v", stream.Err())
	} else if notification.Method != "thread/realtime/closed" {
		t.Fatalf("terminal method = %q, want thread/realtime/closed", notification.Method)
	}
	assertRealtimeStreamClosed(t, stream)
	if _, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"}); err != nil {
		t.Fatalf("start after realtime closed notification: %v", err)
	}
	if got := methodCount(t, transport, "thread/realtime/start"); got != 2 {
		t.Fatalf("thread/realtime/start sent %d times after own thread close, want 2", got)
	}
}

func TestRealtimeStaleHandleCannotSendFollowups(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)
	transport.responses["thread/realtime/stop"] = json.RawMessage(`{}`)

	stale, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := stale.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if err := stale.Stop(ctx); err != nil {
		t.Fatal(err)
	}
	assertRealtimeStreamClosed(t, stream)
	assertRealtimeInactiveError(t, stale.AppendText(ctx, "late text"))
	assertRealtimeInactiveError(t, stale.AppendAudio(ctx, AudioChunk{Data: "late-audio"}))
	assertRealtimeInactiveError(t, stale.AppendSpeech(ctx, SpeechInput{Text: "late speech"}))
	assertRealtimeInactiveError(t, stale.Stop(ctx))
	_, err = stale.Stream(ctx)
	assertRealtimeInactiveError(t, err)
	if got := methodCount(t, transport, "thread/realtime/appendText"); got != 0 {
		t.Fatalf("stale appendText sent %d requests, want 0", got)
	}
	if got := methodCount(t, transport, "thread/realtime/stop"); got != 1 {
		t.Fatalf("thread/realtime/stop sent %d times, want only initial stop", got)
	}

	client, transport = newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)
	active, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	replacement, err := waitForRealtimeStart(ctx, client, "thread-1")
	if err != nil {
		t.Fatal(err)
	}
	assertRealtimeInactiveError(t, active.AppendText(ctx, "late text"))
	assertRealtimeInactiveError(t, active.Stop(ctx))
	if got := methodCount(t, transport, "thread/realtime/appendText"); got != 0 {
		t.Fatalf("stale appendText after restart sent %d requests, want 0", got)
	}
	if got := methodCount(t, transport, "thread/realtime/stop"); got != 0 {
		t.Fatalf("stale stop after restart sent %d requests, want 0", got)
	}
	if err := replacement.AppendText(ctx, "current text"); err != nil {
		t.Fatalf("replacement append: %v", err)
	}
	if got := methodCount(t, transport, "thread/realtime/appendText"); got != 1 {
		t.Fatalf("replacement appendText sent %d requests, want 1", got)
	}
}

func TestRealtimePendingClosedDoesNotPoisonRestartedStream(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)

	_, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	replacement, err := waitForRealtimeStart(ctx, client, "thread-1")
	if err != nil {
		t.Fatal(err)
	}
	stream, err := replacement.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	next := make(chan streamResult, 1)
	go func() {
		notification, ok := stream.Next(ctx)
		next <- streamResult{notification: notification, ok: ok, err: stream.Err()}
	}()
	select {
	case result := <-next:
		t.Fatalf("replacement stream received stale pending event: %#v", result)
	case <-time.After(50 * time.Millisecond):
	}

	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	select {
	case result := <-next:
		if !result.ok {
			t.Fatalf("replacement stream closed: %v", result.err)
		}
		if result.notification.Method != "thread/realtime/started" {
			t.Fatalf("replacement stream method = %q, want thread/realtime/started", result.notification.Method)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("replacement stream did not receive new session notification")
	}
}

func TestRealtimePendingEventsDoNotReplayIntoRestartedStream(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)

	_, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	replacement, err := waitForRealtimeStart(ctx, client, "thread-1")
	if err != nil {
		t.Fatal(err)
	}
	stream, err := replacement.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	next := make(chan streamResult, 1)
	go func() {
		notification, ok := stream.Next(ctx)
		next <- streamResult{notification: notification, ok: ok, err: stream.Err()}
	}()
	select {
	case result := <-next:
		t.Fatalf("replacement stream received stale pending event: %#v", result)
	case <-time.After(50 * time.Millisecond):
	}

	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	select {
	case result := <-next:
		if !result.ok {
			t.Fatalf("replacement stream closed: %v", result.err)
		}
		if result.notification.Method != "thread/realtime/started" {
			t.Fatalf("replacement stream method = %q, want thread/realtime/started", result.notification.Method)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("replacement stream did not receive new session notification")
	}
}

func TestRealtimeOverflowedPendingEventsDoNotPoisonRestartedStream(t *testing.T) {
	ctx := context.Background()
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(ctx, ClientConfig{
		Transport: transport,
		Limits: ClientLimits{
			PendingTurnQueue: 1,
			PendingTurnMap:   4,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)

	_, _, err = client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("thread/name/updated", json.RawMessage(`{"threadId":"thread-1","name":"old name"}`), nil)
	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)

	replacement, err := waitForRealtimeStart(ctx, client, "thread-1")
	if err != nil {
		t.Fatal(err)
	}
	stream, err := replacement.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	next := make(chan streamResult, 1)
	go func() {
		notification, ok := stream.Next(ctx)
		next <- streamResult{notification: notification, ok: ok, err: stream.Err()}
	}()
	select {
	case result := <-next:
		t.Fatalf("replacement stream received stale overflow result: %#v", result)
	case <-time.After(50 * time.Millisecond):
	}

	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	select {
	case result := <-next:
		if !result.ok {
			t.Fatalf("replacement stream closed: %v", result.err)
		}
		if result.notification.Method != "thread/realtime/started" {
			t.Fatalf("replacement stream method = %q, want thread/realtime/started", result.notification.Method)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("replacement stream did not receive new started notification")
	}
}

func TestRealtimeStaleEventsWhileStoppingDoNotReplayAfterRestart(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)
	transport.responses["thread/realtime/stop"] = json.RawMessage(`{}`)

	session, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	if err := session.Stop(ctx); err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	_, _, err = client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	var conflict *ConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("err = %T, want *ConflictError before realtime closed", err)
	}

	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	replacement, err := waitForRealtimeStart(ctx, client, "thread-1")
	if err != nil {
		t.Fatalf("replacement start after realtime closed: %v", err)
	}
	stream, err := replacement.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	next := make(chan streamResult, 1)
	go func() {
		notification, ok := stream.Next(ctx)
		next <- streamResult{notification: notification, ok: ok, err: stream.Err()}
	}()

	select {
	case result := <-next:
		t.Fatalf("replacement stream received stale pending event: %#v", result)
	case <-time.After(50 * time.Millisecond):
	}

	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	select {
	case result := <-next:
		if !result.ok {
			t.Fatalf("replacement stream closed: %v", result.err)
		}
		if result.notification.Method != "thread/realtime/started" {
			t.Fatalf("replacement stream method = %q, want thread/realtime/started", result.notification.Method)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("replacement stream did not receive new started notification")
	}

	if err := replacement.AppendText(ctx, "current text"); err != nil {
		t.Fatalf("replacement append after stale pending events: %v", err)
	}
	if got := methodCount(t, transport, "thread/realtime/appendText"); got != 1 {
		t.Fatalf("replacement appendText sent %d requests, want 1", got)
	}
}

func TestRealtimeReplacementCloseAfterStoppedSessionClosedReleasesReplacement(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	transport.responses["thread/realtime/start"] = json.RawMessage(`{}`)
	transport.responses["thread/realtime/stop"] = json.RawMessage(`{}`)

	session, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	if err := session.Stop(ctx); err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	replacement, err := waitForRealtimeStart(ctx, client, "thread-1")
	if err != nil {
		t.Fatal(err)
	}
	stream, err := replacement.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	next := make(chan streamResult, 1)
	go func() {
		notification, ok := stream.Next(ctx)
		next <- streamResult{notification: notification, ok: ok, err: stream.Err()}
	}()

	transport.deliverNotification("thread/realtime/started", json.RawMessage(`{"threadId":"thread-1","version":"v1"}`), nil)
	select {
	case result := <-next:
		if !result.ok {
			t.Fatalf("replacement stream closed after started: %v", result.err)
		}
		if result.notification.Method != "thread/realtime/started" {
			t.Fatalf("replacement stream method = %q, want thread/realtime/started", result.notification.Method)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("replacement stream did not receive started notification")
	}

	transport.deliverNotification("thread/realtime/closed", json.RawMessage(`{"threadId":"thread-1"}`), nil)
	if notification, ok := stream.Next(ctx); !ok {
		t.Fatalf("replacement stream closed before terminal notification: %v", stream.Err())
	} else if notification.Method != "thread/realtime/closed" {
		t.Fatalf("replacement terminal method = %q, want thread/realtime/closed", notification.Method)
	}
	assertRealtimeStreamClosed(t, stream)
	assertRealtimeInactiveError(t, replacement.AppendText(ctx, "late text"))
}

func TestRealtimeListVoicesSendsMatrixMethod(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	failMethod(transport, "thread/realtime/listVoices")

	_, err := client.Realtime.ListVoices(ctx, protocol.ThreadRealtimeListVoicesParams{})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T, want *RPCError", err)
	}
	assertMethod(t, transport.lastFrame(t), "thread/realtime/listVoices")
}

func assertRealtimeSessionID(t *testing.T, params json.RawMessage, want string) {
	t.Helper()
	var raw struct {
		RealtimeSessionID string `json:"realtimeSessionId"`
	}
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if raw.RealtimeSessionID != want {
		t.Fatalf("realtimeSessionId = %q, want %q; params = %s", raw.RealtimeSessionID, want, params)
	}
}

func assertRealtimeOutputModality(t *testing.T, params json.RawMessage, want string) {
	t.Helper()
	var raw struct {
		OutputModality string `json:"outputModality"`
	}
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if raw.OutputModality != want {
		t.Fatalf("outputModality = %q, want %q; params = %s", raw.OutputModality, want, params)
	}
}

func methodCount(t *testing.T, transport *scriptedTransport, method string) int {
	t.Helper()
	count := 0
	for _, frame := range transport.sentFrames() {
		if methodFromFrame(t, frame) == method {
			count++
		}
	}
	return count
}

func assertRealtimeInactiveError(t *testing.T, err error) {
	t.Helper()
	var conflict *ConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("err = %T, want *ConflictError", err)
	}
}

func assertRealtimeStreamClosed(t *testing.T, stream *RealtimeStream) {
	t.Helper()
	closed := make(chan streamResult, 1)
	go func() {
		notification, ok := stream.Next(context.Background())
		closed <- streamResult{notification: notification, ok: ok, err: stream.Err()}
	}()
	select {
	case result := <-closed:
		if result.ok {
			t.Fatalf("stream received notification after close: %#v", result.notification)
		}
		if result.err != nil {
			t.Fatalf("stream err = %v, want nil close", result.err)
		}
	case <-time.After(200 * time.Millisecond):
		_ = stream.Close()
		<-closed
		t.Fatal("stream did not close")
	}
}

func waitForRealtimeStart(ctx context.Context, client *Client, threadID string) (*RealtimeSession, error) {
	deadline := time.Now().Add(2 * time.Second)
	for {
		session, _, err := client.Realtime.Start(ctx, RealtimeStartOptions{ThreadID: threadID})
		if err == nil {
			return session, nil
		}
		var conflict *ConflictError
		if !errors.As(err, &conflict) || time.Now().After(deadline) {
			return nil, err
		}
		time.Sleep(10 * time.Millisecond)
	}
}

type streamResult struct {
	notification Notification
	ok           bool
	err          error
}
