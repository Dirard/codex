package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *MarketplaceClient) Add(ctx context.Context, params protocol.MarketplaceAddParams) (protocol.MarketplaceAddResponse, error) {
	if c == nil || c.client == nil {
		return protocol.MarketplaceAddResponse{}, &ClosedError{}
	}
	return c.client.Raw().MarketplaceAdd(ctx, params)
}

func (c *MarketplaceClient) Remove(ctx context.Context, params protocol.MarketplaceRemoveParams) (protocol.MarketplaceRemoveResponse, error) {
	if c == nil || c.client == nil {
		return protocol.MarketplaceRemoveResponse{}, &ClosedError{}
	}
	return c.client.Raw().MarketplaceRemove(ctx, params)
}

func (c *MarketplaceClient) Upgrade(ctx context.Context, params protocol.MarketplaceUpgradeParams) (protocol.MarketplaceUpgradeResponse, error) {
	if c == nil || c.client == nil {
		return protocol.MarketplaceUpgradeResponse{}, &ClosedError{}
	}
	return c.client.Raw().MarketplaceUpgrade(ctx, params)
}
