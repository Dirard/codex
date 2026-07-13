# Codex Go SDK

The Go SDK exposes the Codex app-server API from the `github.com/openai/codex/sdk/go` module and the generated protocol package at `github.com/openai/codex/sdk/go/protocol`.

## API Boundary

The root `codex` package is the ergonomic public API for client configuration, resource clients, high-level handles, server handlers, limits, errors, and test transports. The `protocol` package is public typed raw access generated from the app-server schema and manifest; it is useful for exact request and response payloads, and it is coupled to the matching runtime protocol digest. The `internal/` packages, generator manifests, schema fixtures, and drift-test artifacts are not public API.

## Install

```bash
go get github.com/openai/codex/sdk/go
```

The minimum supported Go version is `go 1.25`, as declared by `sdk/go/go.mod`.

## Runtime

Create a client with a compatible `codex app-server` runtime from the same checkout or from a release known to match the SDK protocol digest:

```go
client, err := codex.NewClient(ctx, codex.ClientConfig{
    CodexPath: "/opt/codex/bin/codex",
})
```

`ProtocolModeExperimental` is the zero-value default. Use `ProtocolModeStable` only when the caller intentionally opts out of experimental methods and fields:

```go
client, err := codex.NewClient(ctx, codex.ClientConfig{
    CodexPath:    "/opt/codex/bin/codex",
    ProtocolMode: codex.ProtocolModeStable,
})
```

Runtime mismatch errors are actionable: point `ClientConfig.CodexPath` at a same-checkout runtime, update the runtime to the SDK commit, or pin the SDK to the runtime's matching commit or `sdk/go/vX.Y.Z` tag. Pre-release consumers should pin an exact commit or a reviewed prerelease tag, not a moving branch. Future `v2+` module imports will use semantic import paths such as `github.com/openai/codex/sdk/go/v2` and `github.com/openai/codex/sdk/go/v2/protocol`; `v0` and `v1` keep `github.com/openai/codex/sdk/go` and `github.com/openai/codex/sdk/go/protocol`.

## Configuration

Zero-valued `ClientLimits` fields use SDK defaults. Negative values fail during client construction. Additional-context overrides above the app-server protocol caps are clamped to the protocol maxima; the other positive fields are accepted as explicit caller-chosen local limits.

| Field | Default |
| --- | --- |
| `MaxFrameBytes` | 16 MiB |
| `MaxLocalInputBytes` | 16 MiB |
| `MaxAdditionalContextEntries` | 8 |
| `MaxAdditionalContextKeyBytes` | 128 bytes |
| `MaxAdditionalContextValueBytes` | 1000 bytes |
| `MaxAdditionalContextTotalBytes` | 4096 bytes |
| `ResourceStreamQueue` | 256 |
| `ResourceStreamQueueBytes` | 64 MiB |
| `PendingTurnQueue` | 512 |
| `PendingTurnMap` | 128 |
| `PendingNotificationBytes` | 64 MiB |
| `GlobalSubscriberQueue` | 512 |
| `GlobalSubscriberQueueBytes` | 64 MiB |
| `HandlerConcurrency` | 16 |
| `HandlerQueue` | 256 |
| `HandlerTimeout` | 60s |
| `StderrRingBytes` | 64 KiB |
| `LifecycleInactivityTimeout` | 5m |

`ConfigOverrides` are restricted to the audited non-secret keys `model` and `sandbox_mode`; unsupported paths fail before runtime lookup. Do not pass API keys, OAuth tokens, cookies, private keys, or passwords through config overrides, logs, examples, or error text.

<!-- codex-go-sdk-resource:Config -->
<!-- codex-go-sdk-docs:configRequirements/read -->

```go
requirements, err := client.Config.ReadRequirements(ctx)
_, _ = requirements, err
```

## Auth

Use `Accounts` for ChatGPT/device-code/API-key login, account reads, usage, rate limits, workspace messages, and reset-credit operations.

<!-- codex-go-sdk-resource:Accounts -->
<!-- codex-go-sdk-docs:account/rateLimitResetCredit/consume -->
<!-- codex-go-sdk-docs:account/workspaceMessages/read -->
<!-- codex-go-sdk-docs:account/sendAddCreditsNudgeEmail -->

```go
_, _ = client.Accounts.ConsumeRateLimitResetCredit(ctx, protocol.ConsumeAccountRateLimitResetCreditParams{IDempotencyKey: "reset-credit-idempotency-key"})
_, _ = client.Accounts.ReadWorkspaceMessages(ctx)
_, _ = client.Accounts.SendAddCreditsNudgeEmail(ctx, protocol.SendAddCreditsNudgeEmailParams{CreditType: protocol.AddCreditsNudgeCreditTypeCredits})
```

See `examples/login_account` for login and account-read flows.

## Thread Lifecycle

Threads are started through `Threads.Start`, then turns are started with `Thread.Run` or `Thread.Turn`. Thread handle methods inject the thread identity for follow-up operations such as unsubscribe. Lower-level thread maintenance methods stay available through `Threads`.

<!-- codex-go-sdk-resource:Threads -->
<!-- codex-go-sdk-docs:thread/unsubscribe -->
<!-- codex-go-sdk-docs:thread/name/set -->
<!-- codex-go-sdk-docs:thread/goal/set -->
<!-- codex-go-sdk-docs:thread/goal/get -->
<!-- codex-go-sdk-docs:thread/goal/clear -->
<!-- codex-go-sdk-docs:thread/metadata/update -->
<!-- codex-go-sdk-docs:thread/compact/start -->
<!-- codex-go-sdk-docs:thread/shellCommand -->
<!-- codex-go-sdk-docs:thread/approveGuardianDeniedAction -->
<!-- codex-go-sdk-docs:thread/rollback -->
<!-- codex-go-sdk-docs:thread/increment_elicitation -->
<!-- codex-go-sdk-docs:thread/decrement_elicitation -->
<!-- codex-go-sdk-docs:thread/settings/update -->
<!-- codex-go-sdk-docs:thread/memoryMode/set -->
<!-- codex-go-sdk-docs:thread/backgroundTerminals/clean -->
<!-- codex-go-sdk-docs:thread/backgroundTerminals/list -->
<!-- codex-go-sdk-docs:thread/backgroundTerminals/terminate -->
<!-- codex-go-sdk-docs:thread/inject_items -->
<!-- codex-go-sdk-docs:thread/turns/list -->
<!-- codex-go-sdk-docs:thread/items/list -->
<!-- codex-go-sdk-docs:thread/loaded/list -->

```go
thread, _ := client.Threads.Resume(ctx, codex.ThreadResumeOptions{ThreadID: "thread-id"})
_ = thread.Unsubscribe(ctx)
_, _ = client.Threads.SetName(ctx, protocol.ThreadSetNameParams{})
_, _ = client.Threads.SetGoal(ctx, protocol.ThreadGoalSetParams{})
_, _ = client.Threads.GetGoal(ctx, protocol.ThreadGoalGetParams{})
_, _ = client.Threads.ClearGoal(ctx, protocol.ThreadGoalClearParams{})
_, _ = client.Threads.UpdateMetadata(ctx, protocol.ThreadMetadataUpdateParams{})
_, _ = client.Threads.StartCompaction(ctx, protocol.ThreadCompactStartParams{})
_, _ = client.Threads.ShellCommand(ctx, protocol.ThreadShellCommandParams{})
_, _ = client.Threads.ApproveGuardianDeniedAction(ctx, protocol.ThreadApproveGuardianDeniedActionParams{})
_, _ = client.Threads.Rollback(ctx, protocol.ThreadRollbackParams{})
_, _ = thread.IncrementElicitation(ctx)
_, _ = client.Threads.IncrementElicitation(ctx, thread.ID())
_, _ = thread.DecrementElicitation(ctx)
_, _ = client.Threads.DecrementElicitation(ctx, thread.ID())
_, _ = client.Threads.UpdateSettings(ctx, protocol.ThreadSettingsUpdateParams{})
_, _ = client.Threads.SetMemoryMode(ctx, protocol.ThreadMemoryModeSetParams{})
_, _ = client.Threads.CleanBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsCleanParams{})
_, _ = client.Threads.ListBackgroundTerminals(ctx, protocol.ThreadBackgroundTerminalsListParams{})
_, _ = client.Threads.TerminateBackgroundTerminal(ctx, protocol.ThreadBackgroundTerminalsTerminateParams{})
_, _ = client.Threads.InjectItems(ctx, protocol.ThreadInjectItemsParams{})
_, _ = client.Threads.ListTurns(ctx, protocol.ThreadTurnsListParams{})
_, _ = client.Threads.ListItems(ctx, protocol.ThreadItemsListParams{})
_, _ = client.Threads.ListLoaded(ctx, protocol.ThreadLoadedListParams{})
```

## Runs, Streaming, And Output Schema

`examples/run` shows sync-style `Run`, images through `DataURL` or `LocalImage`, and structured output through `TurnOptions.OutputSchema`. This is the per-turn SDK surface; the README does not promise a separate general response-format API beyond the app-server protocol fields implemented and tested by the SDK.

`examples/streaming` shows `Thread.Turn`, streamed notifications, `Steer`, and `Interrupt`.

## Examples

- `examples/run`: thread start, sync-style `Run`, image inputs, and structured output.
- `examples/streaming`: streaming turns, steering, interrupt, and event iteration.
- `examples/login_account`: login, account read, usage, rate limits, and logout.
- `examples/server_handlers`: approval, dynamic tool, user-input, permission, MCP elicitation, token refresh, attestation, and current-time handlers.
- `examples/resources`: broad resource inventory, including filesystem, commands, processes, MCP, plugins, realtime, models, environments, remote control, collaboration modes, external agents, memory, feedback, Windows sandbox, experimental features, and permission profiles.
- `examples/reviews`: review start and result waiting.
- `examples/skills_hooks`: skills, hooks, and plugin skill reads.
- `examples/raw_protocol`: raw typed protocol calls beside the high-level wrapper.
- `examples/test_harness`: `CODEX_EXEC_PATH` runtime setup and injected transport harness setup.

## Server Handlers

Use `ClientConfig.Handlers` to answer app-server requests. Public handler examples cover approval requests, dynamic tools, user input, permissions, ChatGPT token refresh, MCP elicitation, attestation, and `currentTime/read`.

<!-- codex-go-sdk-handler-docs:account/chatgptAuthTokens/refresh chatgpt-token-refresh -->
<!-- codex-go-sdk-handler-docs:attestation/generate attestation-generate -->
<!-- codex-go-sdk-handler-docs:currentTime/read current-time-read -->

```go
handlers := codex.ServerHandlers{
    ChatGPTTokenRefresh: codex.ChatGPTTokenRefreshAccountChatgptAuthTokensRefreshFunc(func(context.Context, protocol.ChatgptAuthTokensRefreshParams) (protocol.ChatgptAuthTokensRefreshResponse, error) {
        return protocol.ChatgptAuthTokensRefreshResponse{}, nil
    }),
    Attestation: codex.AttestationAttestationGenerateFunc(func(context.Context, protocol.AttestationGenerateParams) (protocol.AttestationGenerateResponse, error) {
        return protocol.AttestationGenerateResponse{}, nil
    }),
    CurrentTime: codex.CurrentTimeCurrentTimeReadFunc(func(context.Context, protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error) {
        return protocol.CurrentTimeReadResponse{}, nil
    }),
}
client, err := codex.NewClient(ctx, codex.ClientConfig{
    CodexPath: "/opt/codex/bin/codex",
    Handlers:  handlers,
})
_, _ = client, err
```

Deprecated compatibility requests are handled internally for protocol compatibility and are not documented as first-class public handlers.

## Resource Workflows

`examples/resources` covers the resource wrapper inventory: apps, config, filesystem, commands, processes, MCP, marketplace, plugins, realtime, models, environments, remote control, collaboration modes, external agents, memory, feedback, fuzzy file search, Windows sandbox, experimental features, and permission profiles.

MCP structured content is available through generated protocol response types such as `McpServerToolCallResponse`; callers should use the typed `protocol` package instead of stringly typed JSON.

OS-specific resources compile on every supported OS. For example, `WindowsSandbox.Readiness` returns a typed unsupported-platform status on non-Windows runtimes, while Windows runtimes still use the app-server method.

## Raw Typed Client

`client.Raw()` exposes generated typed protocol methods for callers that need exact JSON-RPC request shapes. Prefer high-level resource wrappers when they own lifecycle identity or stream routing.

See `examples/raw_protocol` for a typed raw method call beside the equivalent high-level wrapper.

## Release Notes

Go module releases use subdirectory tags such as `sdk/go/v1.2.3`. Published tags are immutable; a bad published release is superseded by a higher patch version or documented retraction rather than being overwritten.

This README and the examples are package-level documentation only; runtime packaging, CI wiring, live app-server/auth proof, and release readiness are validated by later Stage 6/Stage 7 gates.
