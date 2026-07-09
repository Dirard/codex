package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestPluginsThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "list",
			method: "plugin/list",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.List(ctx, protocol.PluginListParams{})
				return err
			},
		},
		{
			name:   "installed",
			method: "plugin/installed",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.Installed(ctx, protocol.PluginInstalledParams{})
				return err
			},
		},
		{
			name:   "read",
			method: "plugin/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.Read(ctx, protocol.PluginReadParams{PluginName: "plugin"})
				return err
			},
		},
		{
			name:   "skill-read",
			method: "plugin/skill/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.ReadSkill(ctx, protocol.PluginSkillReadParams{
					RemoteMarketplaceName: "marketplace",
					RemotePluginID:        "plugin-id",
					SkillName:             "skill",
				})
				return err
			},
		},
		{
			name:   "share-save",
			method: "plugin/share/save",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.SaveShare(ctx, protocol.PluginShareSaveParams{PluginPath: "/repo/plugin"})
				return err
			},
		},
		{
			name:   "share-update-targets",
			method: "plugin/share/updateTargets",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.UpdateShareTargets(ctx, protocol.PluginShareUpdateTargetsParams{
					Discoverability: protocol.PluginShareUpdateDiscoverabilityPrivate,
					RemotePluginID:  "plugin-id",
					ShareTargets:    []protocol.PluginShareTarget{},
				})
				return err
			},
		},
		{
			name:   "share-list",
			method: "plugin/share/list",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.ListShares(ctx, protocol.PluginShareListParams{})
				return err
			},
		},
		{
			name:   "share-checkout",
			method: "plugin/share/checkout",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.CheckoutShare(ctx, protocol.PluginShareCheckoutParams{RemotePluginID: "plugin-id"})
				return err
			},
		},
		{
			name:   "share-delete",
			method: "plugin/share/delete",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.DeleteShare(ctx, protocol.PluginShareDeleteParams{RemotePluginID: "plugin-id"})
				return err
			},
		},
		{
			name:   "install",
			method: "plugin/install",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.Install(ctx, protocol.PluginInstallParams{PluginName: "plugin"})
				return err
			},
		},
		{
			name:   "uninstall",
			method: "plugin/uninstall",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Plugins.Uninstall(ctx, protocol.PluginUninstallParams{PluginID: "plugin-id"})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })
			failMethod(transport, tt.method)

			err = tt.call(context.Background(), client)
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T(%v), want *RPCError", err, err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
		})
	}
}
