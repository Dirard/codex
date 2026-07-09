package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestMarketplaceThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "add",
			method: "marketplace/add",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Marketplace.Add(ctx, protocol.MarketplaceAddParams{Source: "https://example.test/marketplace.git"})
				return err
			},
		},
		{
			name:   "remove",
			method: "marketplace/remove",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Marketplace.Remove(ctx, protocol.MarketplaceRemoveParams{MarketplaceName: "default"})
				return err
			},
		},
		{
			name:   "upgrade",
			method: "marketplace/upgrade",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Marketplace.Upgrade(ctx, protocol.MarketplaceUpgradeParams{})
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
