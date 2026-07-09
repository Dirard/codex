package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestHooksResourceWrappersSendMatrixMethods(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)
	failMethod(transport, "hooks/list")

	_, err := client.Hooks.List(ctx, protocol.HooksListParams{})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T, want *RPCError", err)
	}
	assertMethod(t, transport.lastFrame(t), "hooks/list")
}
