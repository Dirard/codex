package codex

import (
	"context"
	"strings"
	"testing"
)

type compiledResourceCallsite struct {
	wrapperName string
	convention  string
	callsite    string
	compile     func(context.Context, *Client)
}

var compiledResourceCallsites = map[string]compiledResourceCallsite{
	"account/login/start": {
		wrapperName: "Accounts.StartChatGPTLogin / Accounts.StartDeviceCodeLogin / Accounts.LoginWithAPIKey / LoginHandle",
		convention:  "handle-start",
		callsite:    `login, err := client.Accounts.StartDeviceCodeLogin(ctx); login, err = client.Accounts.StartChatGPTLogin(ctx); err = client.Accounts.LoginWithAPIKey(ctx, codex.APIKey("test-key")); result, err := login.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			login, err := client.Accounts.StartDeviceCodeLogin(ctx)
			_ = err
			login, err = client.Accounts.StartChatGPTLogin(ctx)
			_ = err
			err = client.Accounts.LoginWithAPIKey(ctx, APIKey("test-key"))
			_ = err
			result, err := login.Wait(ctx)
			_, _ = result, err
		},
	},
	"account/login/cancel": {
		wrapperName: "LoginHandle.Cancel",
		convention:  "handle-followup",
		callsite:    `login.Cancel(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var login *LoginHandle
			err := login.Cancel(ctx)
			_ = err
		},
	},
	"account/logout": {
		wrapperName: "Accounts.Logout",
		convention:  "thin",
		callsite:    `client.Accounts.Logout(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			err := client.Accounts.Logout(ctx)
			_ = err
		},
	},
	"account/rateLimits/read": {
		wrapperName: "Accounts.RateLimits",
		convention:  "thin",
		callsite:    `client.Accounts.RateLimits(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			rateLimits, err := client.Accounts.RateLimits(ctx)
			_, _ = rateLimits, err
		},
	},
	"account/usage/read": {
		wrapperName: "Accounts.Usage",
		convention:  "thin",
		callsite:    `client.Accounts.Usage(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			usage, err := client.Accounts.Usage(ctx)
			_, _ = usage, err
		},
	},
	"account/read": {
		wrapperName: "Accounts.Read",
		convention:  "thin",
		callsite:    `client.Accounts.Read(ctx, false)`,
		compile: func(ctx context.Context, client *Client) {
			account, err := client.Accounts.Read(ctx, false)
			_, _ = account, err
		},
	},
	"mcpServer/oauth/login": {
		wrapperName: "MCP.OAuthLogin / MCPOAuthHandle",
		convention:  "handle-start",
		callsite:    `oauth, err := client.MCP.OAuthLogin(ctx, codex.MCPOAuthLoginOptions{Name: "github"}); result, err := oauth.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			oauth, err := client.MCP.OAuthLogin(ctx, MCPOAuthLoginOptions{Name: "github"})
			_ = err
			result, err := oauth.Wait(ctx)
			_, _ = result, err
		},
	},
	"review/start": {
		wrapperName: "Reviews.Start / ReviewHandle",
		convention:  "handle-start",
		callsite:    `review, err := client.Reviews.Start(ctx, codex.ReviewStartOptions{ThreadID: thread.ID()}); result, err := review.Wait(ctx)`,
		compile: func(ctx context.Context, client *Client) {
			var thread *Thread
			review, err := client.Reviews.Start(ctx, ReviewStartOptions{ThreadID: thread.ID()})
			_ = err
			result, err := review.Wait(ctx)
			_, _ = result, err
		},
	},
	"thread/start": {
		wrapperName: "Threads.Start",
		convention:  "high-level",
		callsite:    `client.Threads.Start(ctx, codex.ThreadStartOptions{CWD: "/repo", Permissions: "workspace-write"})`,
		compile: func(ctx context.Context, client *Client) {
			thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: "/repo", Permissions: "workspace-write"})
			_, _ = thread, err
		},
	},
	"turn/start": {
		wrapperName: "Thread.Run / Thread.Turn / TurnHandle.Stream",
		convention:  "high-level",
		callsite:    `thread.Run(ctx, codex.Text("inspect this repo"), codex.TurnOptions{Model: "gpt-5.4"}); turn, err := thread.Turn(ctx, codex.Text("continue"), codex.TurnOptions{}); stream, err := turn.Stream(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var thread *Thread
			result, err := thread.Run(ctx, Text("inspect this repo"), TurnOptions{Model: "gpt-5.4"})
			_, _ = result, err
			turn, err := thread.Turn(ctx, Text("continue"), TurnOptions{})
			_ = err
			stream, err := turn.Stream(ctx)
			_, _ = stream, err
		},
	},
	"turn/steer": {
		wrapperName: "TurnHandle.Steer",
		convention:  "handle-followup",
		callsite:    `turn.Steer(ctx, codex.Text("steer toward tests"))`,
		compile: func(ctx context.Context, _ *Client) {
			var turn *TurnHandle
			err := turn.Steer(ctx, Text("steer toward tests"))
			_ = err
		},
	},
	"turn/interrupt": {
		wrapperName: "TurnHandle.Interrupt",
		convention:  "handle-followup",
		callsite:    `turn.Interrupt(ctx)`,
		compile: func(ctx context.Context, _ *Client) {
			var turn *TurnHandle
			err := turn.Interrupt(ctx)
			_ = err
		},
	},
}

func TestResourceCallsites(t *testing.T) {
	for _, row := range generatedResourceCoverage {
		if row.SDKVisibility != "public" || !strings.HasPrefix(row.ImplementationStatus, "implemented-") {
			continue
		}
		callsite, ok := compiledResourceCallsites[row.Method]
		if !ok {
			t.Fatalf("%s is %s but has no compiled resource callsite", row.Method, row.ImplementationStatus)
		}
		if row.WrapperName != callsite.wrapperName {
			t.Fatalf("%s wrapper = %q, want %q", row.Method, row.WrapperName, callsite.wrapperName)
		}
		if row.SignatureConventionID != callsite.convention {
			t.Fatalf("%s signature convention = %q, want %q", row.Method, row.SignatureConventionID, callsite.convention)
		}
		if row.CompileCallsite != callsite.callsite {
			t.Fatalf("%s compile callsite = %q, want %q", row.Method, row.CompileCallsite, callsite.callsite)
		}
		if callsite.compile == nil {
			t.Fatalf("%s has no typed compile function", row.Method)
		}
	}
	for method := range compiledResourceCallsites {
		if !hasImplementedResourceCoverage(method) {
			t.Fatalf("%s has a compiled callsite but is not implemented public resource coverage", method)
		}
	}

	if false {
		var client *Client
		var ctx context.Context
		for _, callsite := range compiledResourceCallsites {
			callsite.compile(ctx, client)
		}
	}
}

func hasImplementedResourceCoverage(method string) bool {
	for _, row := range generatedResourceCoverage {
		if row.Method == method &&
			row.SDKVisibility == "public" &&
			strings.HasPrefix(row.ImplementationStatus, "implemented-") {
			return true
		}
	}
	return false
}
