package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *HooksClient) List(ctx context.Context, params protocol.HooksListParams) (protocol.HooksListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.HooksListResponse{}, &ClosedError{}
	}
	return c.client.Raw().HooksList(ctx, params)
}
