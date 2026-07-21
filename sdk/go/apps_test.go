package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestAppsThinWrappers(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	failMethod(transport, "app/list")

	_, err = client.Apps.List(context.Background(), protocol.AppsListParams{})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "app/list")

	failMethod(transport, "app/read")
	_, err = client.Apps.Read(context.Background(), protocol.AppsReadParams{})
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "app/read")

	failMethod(transport, "app/installed")
	_, err = client.Apps.Installed(context.Background(), protocol.AppsInstalledParams{})
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "app/installed")
}

func TestAppsListUpdatedNotificationRoutes(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	stream, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	transport.deliverNotification("app/list/updated", json.RawMessage(`{"data":[]}`), nil)
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	notification, ok := stream.Next(ctx)
	if !ok {
		t.Fatalf("stream closed before app/list/updated: %v", stream.Err())
	}
	if notification.Method != "app/list/updated" {
		t.Fatalf("notification method = %q, want app/list/updated", notification.Method)
	}
	if _, ok := notification.Payload.(protocol.AppListUpdatedNotification); !ok {
		t.Fatalf("notification payload = %T, want protocol.AppListUpdatedNotification", notification.Payload)
	}
}
