package codex

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestOutboundTraceStaysTopLevel(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["memory/reset"] = json.RawMessage(`{}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	ctx := WithCallOptions(context.Background(), CallOptions{Trace: &TraceContext{TraceParent: "00-abc", TraceState: "state"}})
	if _, err := client.Raw().MemoryReset(ctx); err != nil {
		t.Fatal(err)
	}
	frame := transport.lastFrame(t)
	var object map[string]json.RawMessage
	if err := json.Unmarshal(frame, &object); err != nil {
		t.Fatal(err)
	}
	if _, ok := object["trace"]; !ok {
		t.Fatalf("trace missing from top-level frame: %s", frame)
	}
	var params map[string]json.RawMessage
	if len(object["params"]) > 0 {
		if err := json.Unmarshal(object["params"], &params); err != nil {
			t.Fatal(err)
		}
		if _, ok := params["trace"]; ok {
			t.Fatalf("trace leaked into params: %s", object["params"])
		}
	}
	if string(object["trace"]) != `{"traceparent":"00-abc","tracestate":"state"}` {
		t.Fatalf("trace = %s", object["trace"])
	}
}

func TestInboundTraceVisibleToServerHandler(t *testing.T) {
	traceSeen := make(chan *TraceContext, 1)
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Handlers: ServerHandlers{
			CurrentTime: CurrentTimeCurrentTimeReadFunc(func(ctx context.Context, params protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error) {
				trace, ok := TraceFromContext(ctx)
				if !ok {
					t.Fatal("trace missing")
				}
				traceSeen <- trace
				return protocol.CurrentTimeReadResponse{}, nil
			}),
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	transport.deliverServerRequest("currentTime/read", json.RawMessage(`{"threadId":"thread-1"}`), json.RawMessage(`{"traceparent":"00-def"}`))
	trace := <-traceSeen
	if trace.TraceParent != "00-def" {
		t.Fatalf("trace parent = %s", trace.TraceParent)
	}
}
