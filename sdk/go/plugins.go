package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *PluginsClient) List(ctx context.Context, params protocol.PluginListParams) (protocol.PluginListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginListResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginList(ctx, params)
}

func (c *PluginsClient) Installed(ctx context.Context, params protocol.PluginInstalledParams) (protocol.PluginInstalledResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginInstalledResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginInstalled(ctx, params)
}

func (c *PluginsClient) Read(ctx context.Context, params protocol.PluginReadParams) (protocol.PluginReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginRead(ctx, params)
}

func (c *PluginsClient) ReadSkill(ctx context.Context, params protocol.PluginSkillReadParams) (protocol.PluginSkillReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginSkillReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginSkillRead(ctx, params)
}

func (c *PluginsClient) SaveShare(ctx context.Context, params protocol.PluginShareSaveParams) (protocol.PluginShareSaveResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginShareSaveResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginShareSave(ctx, params)
}

func (c *PluginsClient) UpdateShareTargets(ctx context.Context, params protocol.PluginShareUpdateTargetsParams) (protocol.PluginShareUpdateTargetsResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginShareUpdateTargetsResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginShareUpdateTargets(ctx, params)
}

func (c *PluginsClient) ListShares(ctx context.Context, params protocol.PluginShareListParams) (protocol.PluginShareListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginShareListResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginShareList(ctx, params)
}

func (c *PluginsClient) CheckoutShare(ctx context.Context, params protocol.PluginShareCheckoutParams) (protocol.PluginShareCheckoutResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginShareCheckoutResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginShareCheckout(ctx, params)
}

func (c *PluginsClient) DeleteShare(ctx context.Context, params protocol.PluginShareDeleteParams) (protocol.PluginShareDeleteResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginShareDeleteResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginShareDelete(ctx, params)
}

func (c *PluginsClient) Install(ctx context.Context, params protocol.PluginInstallParams) (protocol.PluginInstallResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginInstallResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginInstall(ctx, params)
}

func (c *PluginsClient) Uninstall(ctx context.Context, params protocol.PluginUninstallParams) (protocol.PluginUninstallResponse, error) {
	if c == nil || c.client == nil {
		return protocol.PluginUninstallResponse{}, &ClosedError{}
	}
	return c.client.Raw().PluginUninstall(ctx, params)
}
