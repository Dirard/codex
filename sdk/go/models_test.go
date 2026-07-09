package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestModelsThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "list",
			method: "model/list",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Models.List(ctx, protocol.ModelListParams{})
				return err
			},
		},
		{
			name:   "read-provider-capabilities",
			method: "modelProvider/capabilities/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Models.ReadProviderCapabilities(ctx, protocol.ModelProviderCapabilitiesReadParams{})
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
