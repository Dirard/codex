package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *MemoryClient) Reset(ctx context.Context) (protocol.MemoryResetResponse, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	return c.client.Raw().MemoryReset(ctx)
}
