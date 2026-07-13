package codex

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestAuthFixtureUnauthenticatedAccountRead(t *testing.T) {
	client, fixture, _ := newAuthFixtureClient(t)
	response, err := client.Accounts.Read(authFixtureContext(t), false)
	if err != nil {
		t.Fatal(err)
	}
	if account, ok := response.Account.Value(); ok {
		t.Fatalf("account = %#v, want unauthenticated empty account", account)
	}
	if !response.RequiresOpenaiAuth {
		t.Fatal("account/read requiresOpenaiAuth = false, want true for the auth fixture provider")
	}
	fixture.assertNoAuthOrBackendRequests(t)
}

func TestAuthFixtureFakeAPIKeyLogin(t *testing.T) {
	client, fixture, codexHome := newAuthFixtureClient(t)
	ctx := authFixtureContext(t)
	if err := client.Accounts.LoginWithAPIKey(ctx, APIKey("fixture-api-key")); err != nil {
		t.Fatal(err)
	}
	response, err := client.Accounts.Read(ctx, false)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := response.Account.Value(); !ok {
		t.Fatal("account/read did not observe the API-key login")
	}
	if info, err := os.Stat(filepath.Join(codexHome, "auth.json")); err != nil {
		t.Fatal(err)
	} else if info.Mode().Perm()&0o077 != 0 {
		t.Fatalf("auth.json permissions = %o, want owner-only", info.Mode().Perm())
	}
	fixture.assertNoAuthOrBackendRequests(t)
}

func TestAuthFixtureDeviceCodeFlow(t *testing.T) {
	client, fixture, _ := newAuthFixtureClient(t)
	completeAuthFixtureDeviceLogin(t, client)
	fixture.assertRequested(t, map[string]int{
		"POST /api/accounts/deviceauth/usercode": 1,
		"POST /api/accounts/deviceauth/token":    1,
		"POST /oauth/token":                      1,
	})
}

func TestAuthFixtureUsageAndRateLimitRead(t *testing.T) {
	client, fixture, _ := newAuthFixtureClient(t)
	ctx := authFixtureContext(t)
	completeAuthFixtureDeviceLogin(t, client)

	if _, err := client.Accounts.Usage(ctx); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Accounts.RateLimits(ctx); err != nil {
		t.Fatal(err)
	}
	response, err := client.Accounts.ConsumeRateLimitResetCredit(ctx, protocol.ConsumeAccountRateLimitResetCreditParams{
		IDempotencyKey: "fixture-reset-credit",
	})
	if err != nil {
		t.Fatal(err)
	}
	if response.Outcome != protocol.ConsumeAccountRateLimitResetCreditOutcomeNothingToReset {
		t.Fatalf("consume outcome = %q", response.Outcome)
	}
	fixture.assertRequested(t, map[string]int{
		"GET /api/codex/profiles/me":                       1,
		"GET /api/codex/usage":                             1,
		"GET /api/codex/rate-limit-reset-credits":          1,
		"POST /api/codex/rate-limit-reset-credits/consume": 1,
	})
	fixture.assertBackendRequestsAuthenticated(t)
}

func completeAuthFixtureDeviceLogin(t *testing.T, client *Client) {
	t.Helper()
	ctx := authFixtureContext(t)
	login, err := client.Accounts.StartDeviceCodeLogin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if login.ID() == "" || login.VerificationURL() == "" || login.UserCode() != "TEST-CODE" {
		t.Fatalf("device login metadata = id %q verification %q code %q", login.ID(), login.VerificationURL(), login.UserCode())
	}
	result, err := login.Wait(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if !result.Success || result.LoginID != login.ID() {
		t.Fatalf("device login result = %#v", result)
	}
}

func authFixtureContext(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), realAppServerTimeout)
	t.Cleanup(cancel)
	return ctx
}
