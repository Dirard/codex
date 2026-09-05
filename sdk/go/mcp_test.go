package codex

import (
	"context"
	"errors"
	"testing"
	"time"

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

func TestMCPOAuthWaitPreservesQueuedCompletionForAnotherThread(t *testing.T) {
	ctx := context.Background()
	transport := newWorkflowTransport(t)
	transport.responses["mcpServer/oauth/login"] = mustJSON(t, protocol.McpServerOauthLoginResponse{
		AuthorizationURL: "https://example.test/oauth",
	})
	client, err := NewClient(ctx, ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	first, err := client.MCP.OAuthLogin(ctx, MCPOAuthLoginOptions{Name: "server-1", ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
	second, err := client.MCP.OAuthLogin(ctx, MCPOAuthLoginOptions{Name: "server-1", ThreadID: "thread-2"})
	if err != nil {
		t.Fatal(err)
	}
	for _, completion := range []protocol.McpServerOauthLoginCompletedNotification{
		{Name: "server-1", Success: true, ThreadID: protocol.Some("thread-1")},
		{Name: "server-1", Success: false, ThreadID: protocol.Some("thread-2")},
	} {
		if err := client.HandleServerNotification(
			ctx,
			"mcpServer/oauthLogin/completed",
			mustJSON(t, completion),
			nil,
		); err != nil {
			t.Fatal(err)
		}
	}

	waitCtx, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	for _, test := range []struct {
		handle *MCPOAuthHandle
		want   MCPOAuthResult
	}{
		{handle: first, want: MCPOAuthResult{Name: "server-1", Success: true}},
		{handle: second, want: MCPOAuthResult{Name: "server-1", Success: false}},
	} {
		result, err := test.handle.Wait(waitCtx)
		if err != nil {
			t.Fatal(err)
		}
		if result == nil || *result != test.want {
			t.Fatalf("result = %#v, want %#v", result, test.want)
		}
	}

	client.router.mu.Lock()
	pendingKeys := len(client.router.pending)
	pendingBytes := client.router.pendingBytes
	client.router.mu.Unlock()
	if pendingKeys != 0 || pendingBytes != 0 {
		t.Fatalf("pending backlog = %d keys, %d bytes; want empty", pendingKeys, pendingBytes)
	}
}
