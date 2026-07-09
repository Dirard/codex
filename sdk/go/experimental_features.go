package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *ExperimentalFeaturesClient) List(ctx context.Context, params protocol.ExperimentalFeatureListParams) (protocol.ExperimentalFeatureListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ExperimentalFeatureListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ExperimentalFeatureList(ctx, params)
}

func (c *ExperimentalFeaturesClient) SetEnablement(ctx context.Context, params protocol.ExperimentalFeatureEnablementSetParams) (protocol.ExperimentalFeatureEnablementSetResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ExperimentalFeatureEnablementSetResponse{}, &ClosedError{}
	}
	return c.client.Raw().ExperimentalFeatureEnablementSet(ctx, params)
}
