package codex

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestNotificationRouterRoutesCurrentDomains(t *testing.T) {
	router := newTestNotificationRouter(t)
	ctx := context.Background()
	tests := []struct {
		name     string
		method   string
		domain   string
		identity string
		params   json.RawMessage
	}{
		{name: "turn", method: "turn/plan/updated", domain: "turn", identity: "turn-1", params: json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1"}`)},
		{name: "item", method: "item/plan/delta", domain: "item", identity: "turn-1", params: json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","delta":"x"}`)},
		{name: "hook", method: "hook/started", domain: "hook", identity: "turn-1", params: json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","run":{"id":"run-1"}}`)},
		{name: "command", method: "command/exec/outputDelta", domain: "command", identity: "proc-1", params: json.RawMessage(`{"processId":"proc-1","delta":"x"}`)},
		{name: "process", method: "process/outputDelta", domain: "process", identity: "handle-1", params: json.RawMessage(`{"processHandle":"handle-1","delta":"x"}`)},
		{name: "fs", method: "fs/changed", domain: "fs", identity: "watch-1", params: json.RawMessage(`{"watchId":"watch-1","changes":[]}`)},
		{name: "mcp", method: "mcpServer/oauthLogin/completed", domain: "mcpServer", identity: "server-1", params: json.RawMessage(`{"name":"server-1","success":true}`)},
		{name: "account-login", method: "account/login/completed", domain: "account", identity: "login-1", params: json.RawMessage(`{"loginId":"login-1","success":true}`)},
		{name: "remote-control", method: "remoteControl/status/changed", domain: "remoteControl", identity: "install-1", params: json.RawMessage(`{"installationId":"install-1","serverName":"server","status":"disabled"}`)},
		{name: "external-agent", method: "externalAgentConfig/import/progress", domain: "externalAgentConfig", identity: "import-1", params: json.RawMessage(`{"importId":"import-1"}`)},
		{name: "fuzzy-search", method: "fuzzyFileSearch/sessionUpdated", domain: "fuzzyFileSearch", identity: "session-1", params: json.RawMessage(`{"sessionId":"session-1"}`)},
		{name: "model", method: "model/rerouted", domain: "model", identity: "turn-1", params: json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1"}`)},
		{name: "warning", method: "warning", domain: "warning", identity: "thread-1", params: json.RawMessage(`{"threadId":"thread-1","message":"careful"}`)},
		{name: "realtime-thread", method: "thread/realtime/started", domain: "thread", identity: "thread-1", params: json.RawMessage(`{"threadId":"thread-1"}`)},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			stream := router.subscribe(tt.domain, tt.identity)
			defer stream.Close()
			routeNotificationForTest(router, ctx, tt.method, tt.params, nil)
			notification := nextNotificationForTest(t, stream)
			if notification.Method != tt.method {
				t.Fatalf("method = %q, want %q", notification.Method, tt.method)
			}
		})
	}
}

func TestNotificationRouterRoutesGlobalOptionalAndUnknownNotifications(t *testing.T) {
	router := newTestNotificationRouter(t)
	global := router.subscribeGlobal()
	defer global.Close()
	warningsByThread := router.subscribe("warning", "thread-1")
	defer warningsByThread.Close()

	routeNotificationForTest(router, context.Background(), "skills/changed", json.RawMessage(`{}`), nil)
	if got := nextNotificationForTest(t, global); got.Method != "skills/changed" {
		t.Fatalf("global method = %q, want skills/changed", got.Method)
	}

	routeNotificationForTest(router, context.Background(), "warning", json.RawMessage(`{"threadId":"thread-1","message":"careful"}`), nil)
	if got := nextNotificationForTest(t, warningsByThread); got.Method != "warning" {
		t.Fatalf("warning method = %q, want warning", got.Method)
	}
	if got := nextNotificationForTest(t, global); got.Method != "warning" {
		t.Fatalf("global warning method = %q, want warning", got.Method)
	}

	routeNotificationForTest(router, context.Background(), "warning", json.RawMessage(`{"message":"careful"}`), nil)
	if got := nextNotificationForTest(t, global); got.Method != "warning" {
		t.Fatalf("global fallback method = %q, want warning", got.Method)
	}

	routeNotificationForTest(router, context.Background(), "future/event", json.RawMessage(`{"value":1}`), json.RawMessage(`{"traceId":"trace-1"}`))
	unknown := nextNotificationForTest(t, global)
	payload, ok := unknown.Payload.(UnknownNotification)
	if !ok {
		t.Fatalf("unknown payload = %#v, want UnknownNotification", unknown.Payload)
	}
	if payload.Method != "future/event" || string(payload.Params) != `{"value":1}` || string(payload.Trace) != `{"traceId":"trace-1"}` {
		t.Fatalf("unknown payload = %#v", payload)
	}
}

func TestNotificationRouterClosesTerminalDomainStream(t *testing.T) {
	router := newTestNotificationRouter(t)
	stream := router.subscribe("process", "handle-1")
	defer stream.Close()

	routeNotificationForTest(router, context.Background(), "process/exited", json.RawMessage(`{"processHandle":"handle-1","exitCode":0}`), nil)
	if got := nextNotificationForTest(t, stream); got.Method != "process/exited" {
		t.Fatalf("method = %q, want process/exited", got.Method)
	}
	if got, ok := stream.Next(context.Background()); ok {
		t.Fatalf("stream produced %#v after terminal notification", got)
	}
	if err := stream.Err(); err != nil {
		t.Fatalf("stream error = %v, want nil", err)
	}
}

func TestNotificationRouterPendingQueueOverflowClosesOnlyAffectedHandle(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 1,
		PendingTurnMap:   4,
	})
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"overflowed"}`), nil)
	routeNotificationForTest(router, ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"overflowed"}`), nil)
	routeNotificationForTest(router, ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"healthy"}`), nil)

	overflowed := router.subscribeTurn("thread-1", "overflowed")
	defer overflowed.Close()
	var overflowErr *OverflowError
	if !errors.As(overflowed.Err(), &overflowErr) {
		t.Fatalf("overflowed stream err = %T %[1]v, want *OverflowError", overflowed.Err())
	}

	healthy := router.subscribeTurn("thread-1", "healthy")
	defer healthy.Close()
	if healthy.Err() != nil {
		t.Fatalf("healthy stream err = %v, want nil", healthy.Err())
	}
	if got := nextNotificationForTest(t, healthy); got.Method != "turn/plan/updated" {
		t.Fatalf("healthy method = %q, want turn/plan/updated", got.Method)
	}
}

func TestNotificationRouterPendingNotificationBytesAreBounded(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue:         4,
		PendingTurnMap:           4,
		PendingNotificationBytes: 512,
	})
	t.Cleanup(router.close)
	params := json.RawMessage(fmt.Sprintf(
		`{"processHandle":"large","delta":"%s"}`,
		strings.Repeat("x", 512),
	))
	routeNotificationForTest(router, context.Background(), "process/outputDelta", params, nil)

	stream := router.subscribe("process", "large")
	defer stream.Close()
	var overflowErr *OverflowError
	if !errors.As(stream.Err(), &overflowErr) {
		t.Fatalf("stream error = %T %[1]v, want *OverflowError", stream.Err())
	}
	router.mu.Lock()
	defer router.mu.Unlock()
	if router.pendingBytes > router.limits.PendingNotificationBytes {
		t.Fatalf("pending bytes = %d, cap = %d", router.pendingBytes, router.limits.PendingNotificationBytes)
	}
}

func TestNotificationRouterGlobalSubscriberBytesAreBounded(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		GlobalSubscriberQueue:      4,
		GlobalSubscriberQueueBytes: 512,
	})
	t.Cleanup(router.close)
	stream := router.subscribeGlobal()
	defer stream.Close()
	params := json.RawMessage(fmt.Sprintf(`{"value":"%s"}`, strings.Repeat("x", 512)))
	routeNotificationForTest(router, context.Background(), "future/event", params, nil)

	var overflowErr *OverflowError
	if !errors.As(stream.Err(), &overflowErr) {
		t.Fatalf("stream error = %T %[1]v, want *OverflowError", stream.Err())
	}
}

func TestNotificationRouterPendingMapOverflowStateIsBounded(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   2,
	})
	t.Cleanup(router.close)

	for i := range 1000 {
		params := json.RawMessage(fmt.Sprintf(`{"processHandle":"process-%d","delta":"x"}`, i))
		routeNotificationForTest(router, context.Background(), "process/outputDelta", params, nil)
	}

	router.mu.Lock()
	defer router.mu.Unlock()
	if got := len(router.pending); got > router.limits.PendingTurnMap {
		t.Fatalf("pending entries = %d, cap = %d", got, router.limits.PendingTurnMap)
	}
	if got := len(router.overflow); got > router.limits.PendingTurnMap {
		t.Fatalf("overflow entries = %d, cap = %d", got, router.limits.PendingTurnMap)
	}
	if got := len(router.timers); got > router.limits.PendingTurnMap+1 {
		t.Fatalf("pending timers = %d, cap = %d", got, router.limits.PendingTurnMap+1)
	}
}

func TestNotificationRouterMalformedKnownTerminalNotificationFailsClosed(t *testing.T) {
	router := newTestNotificationRouter(t)
	stream := router.subscribeTurn("thread-1", "turn-1")
	t.Cleanup(func() { _ = stream.Close() })

	router.route(
		context.Background(),
		"turn/completed",
		json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1","status":7}}`),
		nil,
	)
	if notification, ok := stream.Next(context.Background()); ok {
		t.Fatalf("unexpected notification: %#v", notification)
	}
	var decodeErr *DecodeError
	if !errors.As(stream.Err(), &decodeErr) {
		t.Fatalf("stream err = %T(%v), want *DecodeError", stream.Err(), stream.Err())
	}
	if errors.Unwrap(decodeErr) == nil {
		t.Fatal("decode error does not preserve the generated decoder cause")
	}
}

func TestNotificationRouterPendingMapOverflowPreservesTrackedHandle(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"tracked","delta":"a"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"overflowed","delta":"b"}`), nil)

	tracked := router.subscribe("process", "tracked")
	defer tracked.Close()
	if tracked.Err() != nil {
		t.Fatalf("tracked stream err = %v, want nil", tracked.Err())
	}
	if got := nextNotificationForTest(t, tracked); got.Method != "process/outputDelta" {
		t.Fatalf("tracked method = %q, want process/outputDelta", got.Method)
	}

	overflowed := router.subscribe("process", "overflowed")
	defer overflowed.Close()
	var overflowErr *OverflowError
	if !errors.As(overflowed.Err(), &overflowErr) {
		t.Fatalf("overflowed stream err = %T %[1]v, want *OverflowError", overflowed.Err())
	}
}

func TestNotificationRouterPendingMapOverflowDoesNotPoisonUnrelatedHandle(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"tracked","delta":"a"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"overflowed","delta":"b"}`), nil)

	unrelated := router.subscribe("process", "unrelated")
	defer unrelated.Close()
	if err := unrelated.Err(); err != nil {
		t.Fatalf("unrelated stream err = %v, want nil", err)
	}
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"unrelated","delta":"live"}`), nil)
	if got := nextNotificationForTest(t, unrelated); got.Method != "process/outputDelta" {
		t.Fatalf("unrelated method = %q, want process/outputDelta", got.Method)
	}
}

func TestNotificationRouterOverflowSentinelsSurviveDistinctMapOverflows(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   2,
	})
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"tracked-1","delta":"a"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"tracked-2","delta":"b"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"overflowed-1","delta":"b"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"overflowed-2","delta":"c"}`), nil)

	for _, identity := range []string{"tracked-1", "tracked-2"} {
		tracked := router.subscribe("process", identity)
		defer tracked.Close()
		if tracked.Err() != nil {
			t.Fatalf("%s stream err = %v, want nil", identity, tracked.Err())
		}
		if got := nextNotificationForTest(t, tracked); got.Method != "process/outputDelta" {
			t.Fatalf("%s method = %q, want process/outputDelta", identity, got.Method)
		}
	}

	for _, identity := range []string{"overflowed-1", "overflowed-2"} {
		overflowed := router.subscribe("process", identity)
		defer overflowed.Close()
		var overflowErr *OverflowError
		if !errors.As(overflowed.Err(), &overflowErr) {
			t.Fatalf("%s stream err = %T %[2]v, want *OverflowError", identity, overflowed.Err())
		}
	}
}

func TestNotificationRouterOverflowSentinelCapacityFailsClosed(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"tracked","delta":"a"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"overflowed-1","delta":"b"}`), nil)
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"overflowed-2","delta":"c"}`), nil)

	stream := router.subscribe("process", "unrelated")
	defer stream.Close()
	var overflowErr *OverflowError
	if !errors.As(stream.Err(), &overflowErr) {
		t.Fatalf("router terminal err = %T %[1]v, want *OverflowError", stream.Err())
	}
	if overflowErr.Reason != "pending overflow sentinel capacity exceeded" {
		t.Fatalf("overflow reason = %q", overflowErr.Reason)
	}
}

func TestNotificationRouterLiveDeliveryDoesNotCreateAlternatePendingKeys(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	liveTurn := router.subscribeTurn("thread-1", "turn-live")
	defer liveTurn.Close()
	routeNotificationForTest(router, ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"process-waiting","delta":"before"}`), nil)
	routeNotificationForTest(router, ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-live"}`), nil)

	if got := nextNotificationForTest(t, liveTurn); got.Method != "turn/plan/updated" {
		t.Fatalf("live turn method = %q, want turn/plan/updated", got.Method)
	}
	waitingProcess := router.subscribe("process", "process-waiting")
	defer waitingProcess.Close()
	if err := waitingProcess.Err(); err != nil {
		t.Fatalf("waiting process stream err = %v, want nil", err)
	}
	if got := nextNotificationForTest(t, waitingProcess); got.Method != "process/outputDelta" {
		t.Fatalf("waiting process method = %q, want process/outputDelta", got.Method)
	}
}

func TestNotificationRouterReplaysPendingBeforeLiveTerminalDelivery(t *testing.T) {
	router := newTestNotificationRouter(t)
	ctx := context.Background()
	routeNotificationForTest(router, ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1"}`), nil)

	turnFilter := notificationTurnFilter("thread-1", "turn-1")
	triggered := false
	stream := router.subscribeKeys(turnScopedRouterKeys("turn-1"), func(notification Notification) bool {
		if notification.Method == "turn/plan/updated" && !triggered {
			triggered = true
			routeNotificationForTest(router, ctx, "turn/completed", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1"}}`), nil)
		}
		return turnFilter(notification)
	})
	defer stream.Close()

	if got := nextNotificationForTest(t, stream); got.Method != "turn/plan/updated" {
		t.Fatalf("first method = %q, want turn/plan/updated", got.Method)
	}
	if got := nextNotificationForTest(t, stream); got.Method != "turn/completed" {
		t.Fatalf("second method = %q, want turn/completed", got.Method)
	}
	if got, ok := stream.Next(context.Background()); ok {
		t.Fatalf("stream produced %#v after terminal notification", got)
	}
	if err := stream.Err(); err != nil {
		t.Fatalf("stream error = %v, want nil", err)
	}
}

func TestNotificationRouterReplaysCrossDomainPendingInArrivalOrder(t *testing.T) {
	router := newTestNotificationRouter(t)
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "turn/started", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1"}}`), nil)
	routeNotificationForTest(router, ctx, "item/completed", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","item":{"id":"item-1","type":"agentMessage","text":"hello"}}`), nil)
	routeNotificationForTest(router, ctx, "turn/completed", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[]}}`), nil)

	stream := router.subscribeTurn("thread-1", "turn-1")
	defer stream.Close()

	for _, want := range []string{"turn/started", "item/completed", "turn/completed"} {
		if got := nextNotificationForTest(t, stream); got.Method != want {
			t.Fatalf("method = %q, want %q", got.Method, want)
		}
	}
	if got, ok := stream.Next(context.Background()); ok {
		t.Fatalf("stream produced %#v after terminal notification", got)
	}
	if err := stream.Err(); err != nil {
		t.Fatalf("stream error = %v, want nil", err)
	}
}

func TestNotificationRouterDoesNotBufferUnclaimableThreadNotifications(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	routeNotificationForTest(router, ctx, "thread/started", json.RawMessage(`{"thread":{"id":"thread-1"}}`), nil)
	routeNotificationForTest(router, ctx, "thread/started", json.RawMessage(`{"thread":{"id":"thread-2"}}`), nil)
	routeNotificationForTest(router, ctx, "mcpServer/startupStatus/updated", json.RawMessage(`{"name":"server-1"}`), nil)
	routeNotificationForTest(router, ctx, "mcpServer/startupStatus/updated", json.RawMessage(`{"name":"server-2"}`), nil)
	routeNotificationForTest(router, ctx, "turn/started", json.RawMessage(`{"threadId":"thread-3","turn":{"id":"turn-1"}}`), nil)

	stream := router.subscribeTurn("thread-3", "turn-1")
	defer stream.Close()
	if got := nextNotificationForTest(t, stream); got.Method != "turn/started" {
		t.Fatalf("method = %q, want turn/started", got.Method)
	}
	if err := stream.Err(); err != nil {
		t.Fatalf("stream error = %v, want nil", err)
	}
}

func TestNotificationRouterClaimRemovesPendingAliases(t *testing.T) {
	router := newTestNotificationRouter(t)
	notification := Notification{Method: "future/event"}
	keys := []routerKey{
		{domain: "process", identity: "resource-1"},
		{domain: "command", identity: "resource-1"},
	}
	router.mu.Lock()
	router.nextPendingSeq++
	pending := pendingNotification{seq: router.nextPendingSeq, notification: notification}
	for _, key := range keys {
		router.appendPendingLocked(key, pending)
	}
	router.mu.Unlock()

	stream := router.subscribe("process", "resource-1")
	defer stream.Close()
	if got := nextNotificationForTest(t, stream); got.Method != notification.Method {
		t.Fatalf("method = %q, want %q", got.Method, notification.Method)
	}

	router.mu.Lock()
	defer router.mu.Unlock()
	if len(router.pending) != 0 {
		t.Fatalf("pending aliases = %#v, want none", router.pending)
	}
	if router.pendingBytes != 0 {
		t.Fatalf("pending bytes = %d, want 0", router.pendingBytes)
	}
}

func newTestNotificationRouter(t *testing.T) *notificationRouter {
	t.Helper()
	limits, err := normalizeLimits(ClientLimits{})
	if err != nil {
		t.Fatal(err)
	}
	return newNotificationRouter(limits)
}

func newLimitedTestNotificationRouter(t *testing.T, limitOverrides ClientLimits) *notificationRouter {
	t.Helper()
	limits, err := normalizeLimits(limitOverrides)
	if err != nil {
		t.Fatal(err)
	}
	return newNotificationRouter(limits)
}

func nextNotificationForTest(t *testing.T, stream *NotificationStream) Notification {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	notification, ok := stream.Next(ctx)
	if !ok {
		t.Fatalf("stream closed before notification: %v", stream.Err())
	}
	return notification
}

func routeNotificationForTest(router *notificationRouter, ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) {
	notification := Notification{
		Method:    method,
		RawParams: append([]byte(nil), params...),
		Trace:     append([]byte(nil), trace...),
	}
	metadata, known := protocol.ServerNotificationRoutingByMethod[method]
	if known {
		notification.Payload = append(json.RawMessage(nil), params...)
	} else {
		notification.Payload = UnknownNotification{
			Method: method,
			Params: append([]byte(nil), params...),
			Trace:  append([]byte(nil), trace...),
		}
	}
	keys := routingKeys(metadata, params)
	pendingKeys := pendingRoutingKeys(metadata, params)
	realtimeKeys := realtimeRoutingKeys(method, params)
	keys = append(keys, realtimeKeys...)
	pendingKeys = append(pendingKeys, realtimeKeys...)
	router.deliver(ctx, notification, keys, pendingKeys, true)
}
