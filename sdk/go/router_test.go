package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"
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
			router.route(ctx, tt.method, tt.params, nil)
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

	router.route(context.Background(), "skills/changed", json.RawMessage(`{}`), nil)
	if got := nextNotificationForTest(t, global); got.Method != "skills/changed" {
		t.Fatalf("global method = %q, want skills/changed", got.Method)
	}

	router.route(context.Background(), "warning", json.RawMessage(`{"threadId":"thread-1","message":"careful"}`), nil)
	if got := nextNotificationForTest(t, warningsByThread); got.Method != "warning" {
		t.Fatalf("warning method = %q, want warning", got.Method)
	}
	if got := nextNotificationForTest(t, global); got.Method != "warning" {
		t.Fatalf("global warning method = %q, want warning", got.Method)
	}

	router.route(context.Background(), "warning", json.RawMessage(`{"message":"careful"}`), nil)
	if got := nextNotificationForTest(t, global); got.Method != "warning" {
		t.Fatalf("global fallback method = %q, want warning", got.Method)
	}

	router.route(context.Background(), "future/event", json.RawMessage(`{"value":1}`), json.RawMessage(`{"traceId":"trace-1"}`))
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

	router.route(context.Background(), "process/exited", json.RawMessage(`{"processHandle":"handle-1","exitCode":0}`), nil)
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

	router.route(ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"overflowed"}`), nil)
	router.route(ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"overflowed"}`), nil)
	router.route(ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"healthy"}`), nil)

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

func TestNotificationRouterPendingMapOverflowClosesEvictedHandle(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	router.route(ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"evicted","delta":"a"}`), nil)
	router.route(ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"retained","delta":"b"}`), nil)

	evicted := router.subscribe("process", "evicted")
	defer evicted.Close()
	var overflowErr *OverflowError
	if !errors.As(evicted.Err(), &overflowErr) {
		t.Fatalf("evicted stream err = %T %[1]v, want *OverflowError", evicted.Err())
	}

	retained := router.subscribe("process", "retained")
	defer retained.Close()
	if retained.Err() != nil {
		t.Fatalf("retained stream err = %v, want nil", retained.Err())
	}
	if got := nextNotificationForTest(t, retained); got.Method != "process/outputDelta" {
		t.Fatalf("retained method = %q, want process/outputDelta", got.Method)
	}
}

func TestNotificationRouterOverflowSentinelsSurviveDistinctMapOverflows(t *testing.T) {
	router := newLimitedTestNotificationRouter(t, ClientLimits{
		PendingTurnQueue: 4,
		PendingTurnMap:   1,
	})
	ctx := context.Background()

	router.route(ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"evicted-1","delta":"a"}`), nil)
	router.route(ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"evicted-2","delta":"b"}`), nil)
	router.route(ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"retained","delta":"c"}`), nil)

	evicted1 := router.subscribe("process", "evicted-1")
	defer evicted1.Close()
	var overflowErr *OverflowError
	if !errors.As(evicted1.Err(), &overflowErr) {
		t.Fatalf("evicted-1 stream err = %T %[1]v, want *OverflowError", evicted1.Err())
	}

	evicted2 := router.subscribe("process", "evicted-2")
	defer evicted2.Close()
	overflowErr = nil
	if !errors.As(evicted2.Err(), &overflowErr) {
		t.Fatalf("evicted-2 stream err = %T %[1]v, want *OverflowError", evicted2.Err())
	}

	retained := router.subscribe("process", "retained")
	defer retained.Close()
	if retained.Err() != nil {
		t.Fatalf("retained stream err = %v, want nil", retained.Err())
	}
	if got := nextNotificationForTest(t, retained); got.Method != "process/outputDelta" {
		t.Fatalf("retained method = %q, want process/outputDelta", got.Method)
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
	router.route(ctx, "process/outputDelta", json.RawMessage(`{"processHandle":"process-waiting","delta":"before"}`), nil)
	router.route(ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-live"}`), nil)

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
	router.route(ctx, "turn/plan/updated", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1"}`), nil)

	turnFilter := notificationTurnFilter("thread-1", "turn-1")
	triggered := false
	stream := router.subscribeKeys(turnScopedRouterKeys("turn-1"), func(notification Notification) bool {
		if notification.Method == "turn/plan/updated" && !triggered {
			triggered = true
			router.route(ctx, "turn/completed", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1"}}`), nil)
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

	router.route(ctx, "turn/started", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1"}}`), nil)
	router.route(ctx, "item/completed", json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","item":{"id":"item-1","type":"agentMessage","text":"hello"}}`), nil)
	router.route(ctx, "turn/completed", json.RawMessage(`{"threadId":"thread-1","turn":{"id":"turn-1","status":"completed","items":[]}}`), nil)

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
