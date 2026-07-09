package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *PermissionProfilesClient) List(ctx context.Context, params protocol.PermissionProfileListParams) (protocol.PermissionProfileListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PermissionProfileListResponse{}, &ClosedError{}
	}
	return c.client.Raw().PermissionProfileList(ctx, params)
}
