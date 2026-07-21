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

func (c *AppsClient) Read(ctx context.Context, params protocol.AppsReadParams) (protocol.AppsReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.AppsReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().AppRead(ctx, params)
}

func (c *AppsClient) Installed(ctx context.Context, params protocol.AppsInstalledParams) (protocol.AppsInstalledResponse, error) {
	if c == nil || c.client == nil {
		return protocol.AppsInstalledResponse{}, &ClosedError{}
	}
	return c.client.Raw().AppInstalled(ctx, params)
}
