package codex

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

var benchRouteItemCompleted = func() json.RawMessage {
	text, err := json.Marshal(strings.Repeat("The quick brown fox jumps over the lazy dog. ", 250))
	if err != nil {
		panic(err)
	}
	return json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","completedAtMs":5,"item":{"type":"agentMessage","id":"item-1","text":` + string(text) + `,"phase":"final_answer"}}`)
}()

var benchRouteDelta = json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","delta":"The quick brown fox "}`)

// BenchmarkNotificationRouterRoute measures one notification through the router to a turn
// subscriber: typed decode, routing keys and the turn filter.
func BenchmarkNotificationRouterRoute(b *testing.B) {
	limits, err := normalizeLimits(ClientLimits{})
	if err != nil {
		b.Fatal(err)
	}
	for _, bench := range []struct {
		name   string
		method string
		params json.RawMessage
	}{
		{"item_completed_11KB", "item/completed", benchRouteItemCompleted},
		{"agent_message_delta", "item/agentMessage/delta", benchRouteDelta},
	} {
		b.Run(bench.name, func(b *testing.B) {
			router := newNotificationRouter(limits)
			defer router.close()
			stream := router.subscribeTurn("thread-1", "turn-1")
			ctx := context.Background()
			b.ReportAllocs()
			b.SetBytes(int64(len(bench.params)))
			for b.Loop() {
				if err := router.route(ctx, bench.method, bench.params, nil); err != nil {
					b.Fatal(err)
				}
				if _, ok := stream.Next(ctx); !ok {
					b.Fatalf("stream closed: %v", stream.Err())
				}
			}
		})
	}
}

// routeIdentities is the identity work route does for one notification with one turn
// subscriber: routing keys, pending keys and the turn filter.
func routeIdentities(metadata protocol.ServerNotificationRoutingMetadata, params json.RawMessage, notification Notification, filter func(Notification, *paramIndex) bool) {
	index := newParamIndex(params)
	_ = routingKeys(metadata, index)
	_ = pendingRoutingKeys(metadata, index)
	_ = filter(notification, index)
}

// BenchmarkRoutingKeys isolates the identity extraction the router does per notification.
func BenchmarkRoutingKeys(b *testing.B) {
	metadata := protocol.ServerNotificationRoutingByMethod["item/completed"]
	filter := indexedTurnFilter("thread-1", "turn-1")
	notification := Notification{Method: "item/completed", RawParams: benchRouteItemCompleted}
	b.ReportAllocs()
	b.SetBytes(int64(len(benchRouteItemCompleted)))
	for b.Loop() {
		routeIdentities(metadata, benchRouteItemCompleted, notification, filter)
	}
}
