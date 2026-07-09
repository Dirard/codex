package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestAccountsStage5EThinWrappers(t *testing.T) {
	isolateTestCodexHome(t)
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "consume-rate-limit-reset-credit",
			method: "account/rateLimitResetCredit/consume",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Accounts.ConsumeRateLimitResetCredit(ctx, protocol.ConsumeAccountRateLimitResetCreditParams{
					IDempotencyKey: "reset-credit-idempotency-key",
				})
				return err
			},
		},
		{
			name:   "read-workspace-messages",
			method: "account/workspaceMessages/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Accounts.ReadWorkspaceMessages(ctx)
				return err
			},
		},
		{
			name:   "send-add-credits-nudge-email",
			method: "account/sendAddCreditsNudgeEmail",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Accounts.SendAddCreditsNudgeEmail(ctx, protocol.SendAddCreditsNudgeEmailParams{
					CreditType: protocol.AddCreditsNudgeCreditTypeCredits,
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
