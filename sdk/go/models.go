package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *ModelsClient) List(ctx context.Context, params protocol.ModelListParams) (protocol.ModelListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ModelListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ModelList(ctx, params)
}

func (c *ModelsClient) ReadProviderCapabilities(ctx context.Context, params protocol.ModelProviderCapabilitiesReadParams) (protocol.ModelProviderCapabilitiesReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ModelProviderCapabilitiesReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().ModelProviderCapabilitiesRead(ctx, params)
}
