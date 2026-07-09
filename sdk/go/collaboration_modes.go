package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *CollaborationModesClient) List(ctx context.Context, params protocol.CollaborationModeListParams) (protocol.CollaborationModeListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.CollaborationModeListResponse{}, &ClosedError{}
	}
	return c.client.Raw().CollaborationModeList(ctx, params)
}
