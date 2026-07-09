package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestCollaborationModesThinWrappers(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	failMethod(transport, "collaborationMode/list")

	_, err = client.CollaborationModes.List(context.Background(), protocol.CollaborationModeListParams{})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "collaborationMode/list")
}
