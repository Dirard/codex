package codex

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestAPIKeyFormattingIsRedacted(t *testing.T) {
	key := APIKey("sensitive-value")
	for _, formatted := range []string{fmt.Sprintf("%s", key), fmt.Sprintf("%v", key), fmt.Sprintf("%#v", key)} {
		if formatted != "[redacted]" {
			t.Fatalf("formatted API key = %q, want redacted marker", formatted)
		}
	}
}

func TestLoginWithAmazonBedrockSendsTypedCredentials(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/login/start"] = json.RawMessage(`{"type":"amazonBedrock"}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	if err := client.Accounts.LoginWithAmazonBedrock(context.Background(), APIKey("sensitive-value"), "eu-central-1"); err != nil {
		t.Fatal(err)
	}
	params := requestParamsForMethod(t, transport, "account/login/start")
	var login protocol.LoginAccountParams
	if err := json.Unmarshal(params, &login); err != nil {
		t.Fatal(err)
	}
	if login.TypeValue != "amazonBedrock" {
		t.Fatalf("login type = %q, want amazonBedrock", login.TypeValue)
	}
	region, ok := login.Region.Value()
	if !ok || region != "eu-central-1" {
		t.Fatalf("region = %q, %v; want eu-central-1, true", region, ok)
	}
	if key, ok := login.APIKey.Value(); !ok || key == "" {
		t.Fatal("amazon Bedrock API key was not encoded")
	}
}

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
