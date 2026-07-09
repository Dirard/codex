package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestConfigResourceThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "read",
			method: "config/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Config.Read(ctx, protocol.ConfigReadParams{})
				return err
			},
		},
		{
			name:   "write-value",
			method: "config/value/write",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Config.WriteValue(ctx, protocol.ConfigValueWriteParams{})
				return err
			},
		},
		{
			name:   "batch-write",
			method: "config/batchWrite",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Config.BatchWrite(ctx, protocol.ConfigBatchWriteParams{})
				return err
			},
		},
		{
			name:   "requirements-read",
			method: "configRequirements/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Config.ReadRequirements(ctx)
				return err
			},
		},
		{
			name:   "mcp-server-reload",
			method: "config/mcpServer/reload",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Config.ReloadMCPServers(ctx)
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
