package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *ConfigClient) Read(ctx context.Context, params protocol.ConfigReadParams) (protocol.ConfigReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ConfigReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().ConfigRead(ctx, params)
}

func (c *ConfigClient) WriteValue(ctx context.Context, params protocol.ConfigValueWriteParams) (protocol.ConfigWriteResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ConfigWriteResponse{}, &ClosedError{}
	}
	return c.client.Raw().ConfigValueWrite(ctx, params)
}

func (c *ConfigClient) BatchWrite(ctx context.Context, params protocol.ConfigBatchWriteParams) (protocol.ConfigWriteResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ConfigWriteResponse{}, &ClosedError{}
	}
	return c.client.Raw().ConfigBatchWrite(ctx, params)
}

func (c *ConfigClient) ReadRequirements(ctx context.Context) (protocol.ConfigRequirementsReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ConfigRequirementsReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().ConfigRequirementsRead(ctx)
}

func (c *ConfigClient) ReloadMCPServers(ctx context.Context) (protocol.McpServerRefreshResponse, error) {
	if c == nil || c.client == nil {
		return protocol.McpServerRefreshResponse{}, &ClosedError{}
	}
	return c.client.Raw().ConfigMcpServerReload(ctx)
}
