package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *AppsClient) List(ctx context.Context, params protocol.AppsListParams) (protocol.AppsListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.AppsListResponse{}, &ClosedError{}
	}
	return c.client.Raw().AppList(ctx, params)
}
