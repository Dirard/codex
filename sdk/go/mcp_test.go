package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestMCPThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "list-status",
			method: "mcpServerStatus/list",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.MCP.ListStatus(ctx, protocol.ListMcpServerStatusParams{})
				return err
			},
		},
		{
			name:   "read-resource",
			method: "mcpServer/resource/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.MCP.ReadResource(ctx, protocol.McpResourceReadParams{Server: "github", Uri: "file:///README.md"})
				return err
			},
		},
		{
			name:   "call-tool",
			method: "mcpServer/tool/call",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.MCP.CallTool(ctx, protocol.McpServerToolCallParams{Server: "github", ThreadID: "thread-id", Tool: "search"})
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
