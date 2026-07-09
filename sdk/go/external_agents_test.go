package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestExternalAgentsThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "detect-config",
			method: "externalAgentConfig/detect",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.ExternalAgents.DetectConfig(ctx, protocol.ExternalAgentConfigDetectParams{
					Cwds:        protocol.Some([]string{"/repo"}),
					IncludeHome: protocol.SomeNonNull(true),
				})
				return err
			},
		},
		{
			name:   "import-config",
			method: "externalAgentConfig/import",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.ExternalAgents.ImportConfig(ctx, protocol.ExternalAgentConfigImportParams{
					MigrationItems: []protocol.ExternalAgentConfigMigrationItem{},
					Source:         protocol.Some("codex"),
				})
				return err
			},
		},
		{
			name:   "read-import-histories",
			method: "externalAgentConfig/import/readHistories",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.ExternalAgents.ReadImportHistories(ctx)
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })
			failMethod(transport, tt.method)

			err = tt.call(context.Background(), client)
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T(%v), want *RPCError", err, err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
		})
	}
}
