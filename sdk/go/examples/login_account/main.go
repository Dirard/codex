package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
)

// codex-go-sdk-resource:Accounts
// codex-go-sdk-docs:account/login/start
// codex-go-sdk-docs:account/login/cancel
// codex-go-sdk-docs:account/logout
// codex-go-sdk-docs:account/rateLimits/read
// codex-go-sdk-docs:account/read
// codex-go-sdk-docs:account/usage/read
func loginAndReadAccount(ctx context.Context, client *codex.Client, apiKey codex.APIKey, bedrockRegion string) error {
	login, err := client.Accounts.StartDeviceCodeLogin(ctx)
	if err != nil {
		return err
	}
	_ = login.UserCode()
	_ = login.VerificationURL()
	_ = login.Cancel(ctx)
	login, err = client.Accounts.StartChatGPTLogin(ctx)
	if err != nil {
		return err
	}
	_, _ = login.Wait(ctx)
	if err := client.Accounts.LoginWithAPIKey(ctx, apiKey); err != nil {
		return err
	}
	if err := client.Accounts.LoginWithAmazonBedrock(ctx, apiKey, bedrockRegion); err != nil {
		return err
	}
	if _, err := client.Accounts.Read(ctx, false); err != nil {
		return err
	}
	if _, err := client.Accounts.Usage(ctx); err != nil {
		return err
	}
	if _, err := client.Accounts.RateLimits(ctx); err != nil {
		return err
	}
	return client.Accounts.Logout(ctx)
}

func main() {}
