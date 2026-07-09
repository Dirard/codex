package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *EnvironmentsClient) Add(ctx context.Context, params protocol.EnvironmentAddParams) (protocol.EnvironmentAddResponse, error) {
	if c == nil || c.client == nil {
		return protocol.EnvironmentAddResponse{}, &ClosedError{}
	}
	return c.client.Raw().EnvironmentAdd(ctx, params)
}

func (c *EnvironmentsClient) Info(ctx context.Context, params protocol.EnvironmentInfoParams) (protocol.EnvironmentInfoResponse, error) {
	if c == nil || c.client == nil {
		return protocol.EnvironmentInfoResponse{}, &ClosedError{}
	}
	return c.client.Raw().EnvironmentInfo(ctx, params)
}
