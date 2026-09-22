package codex

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

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
			name:   "read-rate-limits-with-params",
			method: "account/rateLimits/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Accounts.RateLimitsWithParams(ctx, protocol.GetAccountRateLimitsParams{
					ExcludeResetCreditDetails: protocol.SomeNonNull(true),
					SupportsLunaReserve:       protocol.SomeNonNull(true),
				})
				return err
			},
		},
		{
			name:   "read-gateway-oauth",
			method: "account/gatewayOAuth/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.Accounts.GatewayOAuthRead(ctx)
				return err
			},
		},
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

func TestGatewayOAuthHandleWaitsForCompletion(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
		ProviderID:   "gateway",
		ProviderName: "Gateway",
		Required:     true,
		Status:       protocol.Some(protocol.GatewayOAuthStatusNotReady),
	})
	releaseLogin := transport.deferResponse("account/gatewayOAuth/login", json.RawMessage(`{}`))
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:            transport,
		ExplicitGatewayOAuth: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	go func() {
		time.Sleep(10 * time.Millisecond)
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			ProviderID: "other-provider",
			Status:     protocol.GatewayOAuthStatusStarted,
		}), nil)
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			ProviderID: "gateway",
			Status:     protocol.GatewayOAuthStatusStarted,
		}), nil)
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			AuthURL:    protocol.Some("https://example.test/gateway"),
			ProviderID: "gateway",
			Status:     protocol.GatewayOAuthStatusStarted,
		}), nil)
	}()

	handle, err := client.Accounts.StartGatewayOAuthLogin(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if params := requestParamsForMethod(t, transport, "account/gatewayOAuth/login"); string(params) != "" {
		t.Fatalf("login params = %s; want omitted", params)
	}
	if handle.ProviderID() != "gateway" || handle.AuthURL() != "https://example.test/gateway" {
		t.Fatalf("handle provider/url = %q/%q", handle.ProviderID(), handle.AuthURL())
	}

	releaseLogin()
	transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
		ProviderID: "gateway",
		Status:     protocol.GatewayOAuthStatusSucceeded,
	}), nil)

	result, err := handle.Wait(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if result.ProviderID != "gateway" || result.Status != protocol.GatewayOAuthStatusSucceeded {
		t.Fatalf("result = %#v", result)
	}
}

func TestGatewayOAuthLoginRequiresExplicitOptIn(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	_, err = client.Accounts.StartGatewayOAuthLogin(context.Background())
	var configErr *ConfigError
	if !errors.As(err, &configErr) || !strings.Contains(configErr.Reason, "ExplicitGatewayOAuth") {
		t.Fatalf("err = %T(%v), want explicit opt-in ConfigError", err, err)
	}
	for _, frame := range transport.sentFrames()[1:] {
		if methodFromFrame(t, frame) != "initialized" {
			t.Fatalf("explicit opt-in guard sent frame %s", frame)
		}
	}
}

func TestGatewayOAuthHandleAcceptsTerminalNotificationBeforeStarted(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
		ProviderID: "gateway",
		Required:   true,
	})
	releaseLogin := transport.deferResponse("account/gatewayOAuth/login", json.RawMessage(`{}`))
	failMethod(transport, "account/gatewayOAuth/login")
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:            transport,
		ExplicitGatewayOAuth: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	go func() {
		waitForMethod(t, transport, "account/gatewayOAuth/login")
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			ProviderID: "gateway",
			Status:     protocol.GatewayOAuthStatusFailed,
			Error:      protocol.Some("gateway rejected login"),
		}), nil)
		releaseLogin()
	}()

	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err = client.Accounts.StartGatewayOAuthLogin(ctx)
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want terminal-before-start RPC failure", err, err)
	}
}

func TestGatewayOAuthHandleCancelSendsExplicitCancel(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
		ProviderID: "gateway",
		Required:   true,
	})
	transport.deferResponse("account/gatewayOAuth/login", json.RawMessage(`{}`))
	transport.responses["account/gatewayOAuth/cancel"] = json.RawMessage(`{}`)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:            transport,
		ExplicitGatewayOAuth: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	go func() {
		time.Sleep(10 * time.Millisecond)
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			AuthURL:    protocol.Some("https://example.test/gateway"),
			ProviderID: "gateway",
			Status:     protocol.GatewayOAuthStatusStarted,
		}), nil)
	}()
	handle, err := client.Accounts.StartGatewayOAuthLogin(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err = client.Accounts.StartGatewayOAuthLogin(ctx)
	var unsupported *UnsupportedError
	if !errors.As(err, &unsupported) {
		t.Fatalf("overlapping login err = %T(%v), want UnsupportedError", err, err)
	}
	if err := handle.Cancel(context.Background()); err != nil {
		t.Fatal(err)
	}
	assertMethod(t, transport.lastFrame(t), "account/gatewayOAuth/cancel")
	if params := requestParamsForMethod(t, transport, "account/gatewayOAuth/cancel"); string(params) != "" {
		t.Fatalf("cancel params = %s; want omitted", params)
	}
	frames := len(transport.sentFrames())
	if err := handle.Cancel(context.Background()); err != nil {
		t.Fatal(err)
	}
	if len(transport.sentFrames()) != frames {
		t.Fatal("a completed handle sent another connection-scoped cancellation")
	}
}

func TestGatewayOAuthStartContextCancellationCancelsLogin(t *testing.T) {
	for _, afterStarted := range []bool{false, true} {
		t.Run(fmt.Sprintf("after_started=%t", afterStarted), func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
				ProviderID: "gateway",
				Required:   true,
			})
			releaseLogin := transport.deferResponse("account/gatewayOAuth/login", json.RawMessage(`{}`))
			transport.responses["account/gatewayOAuth/cancel"] = json.RawMessage(`{}`)
			client, err := NewClient(context.Background(), ClientConfig{
				Transport:            transport,
				ExplicitGatewayOAuth: true,
			})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })

			ctx, cancel := context.WithTimeout(context.Background(), time.Second)
			defer cancel()
			go func() {
				waitForMethod(t, transport, "account/gatewayOAuth/login")
				if afterStarted {
					transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
						AuthURL:    protocol.Some("https://example.test/gateway"),
						ProviderID: "gateway",
						Status:     protocol.GatewayOAuthStatusStarted,
					}), nil)
				} else {
					cancel()
				}
			}()
			handle, err := client.Accounts.StartGatewayOAuthLogin(ctx)
			if afterStarted {
				if err != nil {
					t.Fatal(err)
				}
				cancel()
				_, err = handle.Wait(ctx)
			}
			if !errors.Is(err, context.Canceled) {
				t.Fatalf("err = %T(%v), want context cancellation", err, err)
			}
			assertMethod(t, transport.lastFrame(t), "account/gatewayOAuth/cancel")
			releaseLogin()
		})
	}
}

func TestGatewayOAuthStartReturnsRPCOutcomeWithoutNotification(t *testing.T) {
	for _, failed := range []bool{false, true} {
		t.Run(fmt.Sprintf("failed=%t", failed), func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
				ProviderID: "gateway",
				Required:   true,
			})
			transport.responses["account/gatewayOAuth/login"] = json.RawMessage(`{}`)
			if failed {
				failMethod(transport, "account/gatewayOAuth/login")
			}
			client, err := NewClient(context.Background(), ClientConfig{
				Transport:            transport,
				ExplicitGatewayOAuth: true,
			})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })
			ctx, cancel := context.WithTimeout(context.Background(), time.Second)
			defer cancel()
			handle, err := client.Accounts.StartGatewayOAuthLogin(ctx)
			if failed {
				var rpcErr *RPCError
				if !errors.As(err, &rpcErr) {
					t.Fatalf("err = %T(%v), want RPC rejection without waiting for a notification", err, err)
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			result, err := handle.Wait(ctx)
			if err != nil || result.ProviderID != "gateway" || result.Status != protocol.GatewayOAuthStatusSucceeded {
				t.Fatalf("result = %#v, err = %v", result, err)
			}
		})
	}
}

func TestGatewayOAuthWaitReturnsRPCErrorWithoutTerminalNotification(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["account/gatewayOAuth/read"] = mustJSON(t, protocol.GatewayOAuthReadResponse{
		ProviderID: "gateway",
		Required:   true,
	})
	releaseLogin := transport.deferResponse("account/gatewayOAuth/login", json.RawMessage(`{}`))
	failMethod(transport, "account/gatewayOAuth/login")
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:            transport,
		ExplicitGatewayOAuth: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	go func() {
		waitForMethod(t, transport, "account/gatewayOAuth/login")
		transport.deliverNotification("account/gatewayOAuth/changed", mustJSON(t, protocol.GatewayOAuthChangedNotification{
			AuthURL:    protocol.Some("https://example.test/gateway"),
			ProviderID: "gateway",
			Status:     protocol.GatewayOAuthStatusStarted,
		}), nil)
	}()
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	handle, err := client.Accounts.StartGatewayOAuthLogin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	releaseLogin()
	_, err = handle.Wait(ctx)
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want RPC error without waiting for terminal notification", err, err)
	}
}

func TestAccountRateLimitsWireParams(t *testing.T) {
	isolateTestCodexHome(t)
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	failMethod(transport, "account/rateLimits/read")

	if _, err := client.Accounts.RateLimits(context.Background()); err == nil {
		t.Fatal("legacy rate-limits call unexpectedly succeeded")
	}
	if got := string(requestParamsForMethod(t, transport, "account/rateLimits/read")); got != "" {
		t.Fatalf("legacy rate-limits params = %s; want omitted", got)
	}

	_, err = client.Accounts.RateLimitsWithParams(context.Background(), protocol.GetAccountRateLimitsParams{
		ExcludeResetCreditDetails: protocol.SomeNonNull(true),
		SupportsLunaReserve:       protocol.SomeNonNull(true),
	})
	if err == nil {
		t.Fatal("parameterized rate-limits call unexpectedly succeeded")
	}
	const want = `{"excludeResetCreditDetails":true,"supportsLunaReserve":true}`
	if got := string(requestParamsForMethod(t, transport, "account/rateLimits/read")); got != want {
		t.Fatalf("parameterized rate-limits params = %s; want %s", got, want)
	}
}
