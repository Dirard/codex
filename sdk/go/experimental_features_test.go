package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestExperimentalFeaturesThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "list",
			method: "experimentalFeature/list",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.ExperimentalFeatures.List(ctx, protocol.ExperimentalFeatureListParams{
					ThreadID: protocol.Some("thread-1"),
				})
				return err
			},
		},
		{
			name:   "set-enablements",
			method: "experimentalFeature/enablement/set",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.ExperimentalFeatures.SetEnablement(ctx, protocol.ExperimentalFeatureEnablementSetParams{
					Enablement: map[string]bool{"feature": true},
				})
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

func TestExperimentalFeatureListStableModeStillUsesStableMetadata(t *testing.T) {
	transport := newScriptedInitializedTransport(t, stableInitializePayload())
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:    transport,
		ProtocolMode: ProtocolModeStable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	failMethod(transport, "experimentalFeature/list")

	_, err = client.ExperimentalFeatures.List(context.Background(), protocol.ExperimentalFeatureListParams{})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "experimentalFeature/list")
}
