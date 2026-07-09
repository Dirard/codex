package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *SkillsClient) List(ctx context.Context, params protocol.SkillsListParams) (protocol.SkillsListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.SkillsListResponse{}, &ClosedError{}
	}
	return c.client.Raw().SkillsList(ctx, params)
}

func (c *SkillsClient) SetExtraRoots(ctx context.Context, params protocol.SkillsExtraRootsSetParams) (protocol.SkillsExtraRootsSetResponse, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	return c.client.Raw().SkillsExtraRootsSet(ctx, params)
}

func (c *SkillsClient) WriteConfig(ctx context.Context, params protocol.SkillsConfigWriteParams) (protocol.SkillsConfigWriteResponse, error) {
	if c == nil || c.client == nil {
		return protocol.SkillsConfigWriteResponse{}, &ClosedError{}
	}
	return c.client.Raw().SkillsConfigWrite(ctx, params)
}
