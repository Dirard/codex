package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestSkillsResourceWrappersSendMatrixMethods(t *testing.T) {
	ctx := context.Background()
	client, transport := newStage5Client(t)

	calls := []struct {
		method string
		call   func() error
	}{
		{
			method: "skills/list",
			call: func() error {
				_, err := client.Skills.List(ctx, protocol.SkillsListParams{})
				return err
			},
		},
		{
			method: "skills/extraRoots/set",
			call: func() error {
				_, err := client.Skills.SetExtraRoots(ctx, protocol.SkillsExtraRootsSetParams{})
				return err
			},
		},
		{
			method: "skills/config/write",
			call: func() error {
				_, err := client.Skills.WriteConfig(ctx, protocol.SkillsConfigWriteParams{})
				return err
			},
		},
	}

	for _, tt := range calls {
		t.Run(tt.method, func(t *testing.T) {
			failMethod(transport, tt.method)
			err := tt.call()
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T, want *RPCError", err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
		})
	}
}
