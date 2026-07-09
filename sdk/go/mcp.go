package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

type MCPOAuthLoginOptions struct {
	Name        string
	Scopes      []string
	ThreadID    string
	TimeoutSecs int64
}

type MCPOAuthHandle struct {
	client           *Client
	name             string
	threadID         string
	authorizationURL string
}

type MCPOAuthResult struct {
	Name    string
	Success bool
	Error   string
}

func (c *MCPClient) OAuthLogin(ctx context.Context, opts MCPOAuthLoginOptions) (*MCPOAuthHandle, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("mcp oauth login", "mcpServer/oauth/login"); err != nil {
		return nil, err
	}
	params := protocol.McpServerOauthLoginParams{Name: opts.Name}
	if len(opts.Scopes) > 0 {
		params.Scopes = protocol.Some(opts.Scopes)
	}
	if opts.ThreadID != "" {
		params.ThreadID = protocol.Some(opts.ThreadID)
	}
	if opts.TimeoutSecs > 0 {
		params.TimeoutSecs = protocol.Some(opts.TimeoutSecs)
	}
	response, err := c.client.Raw().McpServerOauthLogin(ctx, params)
	if err != nil {
		return nil, err
	}
	return &MCPOAuthHandle{client: c.client, name: opts.Name, threadID: opts.ThreadID, authorizationURL: response.AuthorizationURL}, nil
}

func (h *MCPOAuthHandle) AuthorizationURL() string {
	if h == nil {
		return ""
	}
	return h.authorizationURL
}

func (h *MCPOAuthHandle) Wait(ctx context.Context) (*MCPOAuthResult, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("mcp oauth wait"); err != nil {
		return nil, err
	}
	keys := []routerKey{
		{domain: "mcpServer", identity: h.name},
	}
	if h.threadID != "" {
		keys = append(keys, routerKey{domain: "mcpServer", identity: h.threadID})
	}
	stream := h.client.router.subscribeKeys(keys, mcpOAuthCompletionFilter(h.name, h.threadID))
	defer stream.Close()
	for {
		notification, ok := stream.Next(ctx)
		if !ok {
			if err := stream.Err(); err != nil {
				return nil, err
			}
			return nil, &ClosedError{}
		}
		payload, ok := notification.Payload.(protocol.McpServerOauthLoginCompletedNotification)
		if !ok || payload.Name != h.name {
			continue
		}
		if h.threadID != "" {
			threadID, ok := payload.ThreadID.Value()
			if ok && threadID != h.threadID {
				continue
			}
		}
		errorText, _ := payload.Error.Value()
		return &MCPOAuthResult{Name: payload.Name, Success: payload.Success, Error: errorText}, nil
	}
}

func mcpOAuthCompletionFilter(name string, threadID string) func(Notification) bool {
	return func(notification Notification) bool {
		payload, ok := notification.Payload.(protocol.McpServerOauthLoginCompletedNotification)
		if !ok || payload.Name != name {
			return false
		}
		if threadID == "" {
			return true
		}
		gotThreadID, ok := payload.ThreadID.Value()
		return !ok || gotThreadID == threadID
	}
}

func (h *MCPOAuthHandle) Cancel(context.Context) error {
	return &UnsupportedError{Reason: "current manifest exposes no safe MCP OAuth cancel method"}
}
