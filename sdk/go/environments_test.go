package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestEnvironmentsThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "add",
			method: "environment/add",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Environments.Add(ctx, protocol.EnvironmentAddParams{
					EnvironmentID: "env-1",
					ExecServerURL: "http://127.0.0.1:9876",
				})
				return err
			},
		},
		{
			name:   "info",
			method: "environment/info",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Environments.Info(ctx, protocol.EnvironmentInfoParams{EnvironmentID: "env-1"})
				return err
			},
		},
		{
			name:   "status",
			method: "environment/status",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Environments.Status(ctx, protocol.EnvironmentStatusParams{EnvironmentID: "env-1"})
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
