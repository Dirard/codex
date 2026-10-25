# Codex Go SDK

The Go SDK exposes the Codex app-server API from the `github.com/openai/codex/sdk/go` module and the generated protocol package at `github.com/openai/codex/sdk/go/protocol`.

## API Boundary

The root `codex` package is the ergonomic public API for client configuration, resource clients, high-level handles, server handlers, limits, errors, and test transports. The `codextest` package is a public in-process fake app-server for tests of code that uses the SDK. The `protocol` package is public typed raw access generated from the app-server schema and manifest; it is useful for exact request and response payloads, and it is coupled to the matching runtime protocol digest. The `internal/` packages, generator manifests, schema fixtures, and drift-test artifacts are not public API.

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

`ShutdownGracePeriod` (outside `ClientLimits`) is how long `Close` lets a runtime the SDK launched exit on its own after its stdin is closed; zero means 5s, a negative value kills the runtime at once, and it cannot be combined with `Transport`.

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
<!-- codex-go-sdk-docs:account/gatewayOAuth/read -->
<!-- codex-go-sdk-docs:account/gatewayOAuth/login -->
<!-- codex-go-sdk-docs:account/gatewayOAuth/cancel -->
<!-- codex-go-sdk-docs:account/workspaceMessages/read -->
<!-- codex-go-sdk-docs:account/sendAddCreditsNudgeEmail -->

```go
_, _ = client.Accounts.ConsumeRateLimitResetCredit(ctx, protocol.ConsumeAccountRateLimitResetCreditParams{IDempotencyKey: "reset-credit-idempotency-key"})
_, _ = client.Accounts.ReadWorkspaceMessages(ctx)
_, _ = client.Accounts.SendAddCreditsNudgeEmail(ctx, protocol.SendAddCreditsNudgeEmailParams{CreditType: protocol.AddCreditsNudgeCreditTypeCredits})
```

For a gateway sign-in UI, set `ClientConfig.ExplicitGatewayOAuth`, probe `GatewayOAuthRead` on every connection, and use `StartGatewayOAuthLogin` to receive the authorization URL without enabling automatic browser login. Do not fall back automatically when the probe fails.
Only one managed gateway login may be active per client. `Wait` follows the login RPC result, including errors without a notification. Cancellation stops an unsent request locally; once login has been sent, it sends the connection-scoped cancel RPC. A failed cancellation retains login ownership until the login RPC finishes. A login that completes immediately can return a handle with an empty authorization URL.

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
<!-- codex-go-sdk-docs:thread/attachment/add -->
<!-- codex-go-sdk-docs:thread/attachment/list -->
<!-- codex-go-sdk-docs:thread/attachment/remove -->
<!-- codex-go-sdk-docs:thread/compact/start -->
<!-- codex-go-sdk-docs:thread/shellCommand -->
<!-- codex-go-sdk-docs:thread/approveGuardianDeniedAction -->
<!-- codex-go-sdk-docs:thread/revert -->
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
<!-- codex-go-sdk-docs:thread/timeline/list -->
<!-- codex-go-sdk-docs:thread/loaded/list -->

```go
thread, _ := client.Threads.Resume(ctx, codex.ThreadResumeOptions{ThreadID: "thread-id"})
_ = thread.Unsubscribe(ctx)
_, _ = client.Threads.SetName(ctx, protocol.ThreadSetNameParams{})
_, _ = client.Threads.SetGoal(ctx, protocol.ThreadGoalSetParams{})
_, _ = client.Threads.GetGoal(ctx, protocol.ThreadGoalGetParams{})
_, _ = client.Threads.ClearGoal(ctx, protocol.ThreadGoalClearParams{})
_, _ = client.Threads.UpdateMetadata(ctx, protocol.ThreadMetadataUpdateParams{})
_, _ = client.Threads.AddAttachment(ctx, protocol.ThreadAttachmentAddParams{})
_, _ = client.Threads.ListAttachments(ctx, protocol.ThreadAttachmentListParams{})
_, _ = client.Threads.RemoveAttachment(ctx, protocol.ThreadAttachmentRemoveParams{})
_, _ = client.Threads.StartCompaction(ctx, protocol.ThreadCompactStartParams{})
_, _ = client.Threads.ShellCommand(ctx, protocol.ThreadShellCommandParams{})
_, _ = client.Threads.ApproveGuardianDeniedAction(ctx, protocol.ThreadApproveGuardianDeniedActionParams{})
_, _ = client.Threads.Revert(ctx, protocol.ThreadRevertParams{})
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
_, _ = client.Threads.ListTimeline(ctx, protocol.ThreadTimelineListParams{})
_, _ = client.Threads.ListLoaded(ctx, protocol.ThreadLoadedListParams{})
```

## Runs, Streaming, And Output Schema

`examples/run` shows sync-style `Run`, images through `DataURL`, `ImageFileID`, or `LocalImage`, and structured output through `TurnOptions.OutputSchema`. This is the per-turn SDK surface; the README does not promise a separate general response-format API beyond the app-server protocol fields implemented and tested by the SDK.

`examples/streaming` shows `Thread.Turn`, streamed notifications, `Steer`, `Interrupt`, and live turn-settings updates.

Streams can be ranged over (Go 1.23+ iterators). `stream.All(ctx)` yields each notification with a nil error and, when the stream closes with an error, that error once; a clean close, such as after `turn/completed`, just ends the loop, and `break` leaves the stream open. `codex.Payloads[T]` keeps only payloads of one type:

```go
stream, _ := turn.Stream(ctx)
for delta, err := range codex.Payloads[protocol.AgentMessageDeltaNotification](ctx, stream) {
    if err != nil {
        return err
    }
    fmt.Print(delta.Delta)
}
result, err := turn.Wait(ctx)
```

## Handle Lifecycle And Cancellation

- **One outcome per handle.** A `TurnHandle` or `ReviewHandle` subscribes to its turn once, on the first `Stream`/`Events` or `Wait`, and records the outcome. Repeated and concurrent `Wait` calls return the same result or error, `Wait` still returns the result after a stream consumed every event, and a stream requested after the turn finished is already closed (with no error after `turn/completed`). A stream sees events from the moment it subscribes; the first subscriber also gets the events buffered since the turn started. The handle keeps the collected result, bounded by `MaxRunResultItems` and `MaxRunResultBytes`, while it is referenced. `LoginHandle.Wait` and `MCPOAuthHandle.Wait` also record their outcome. `CommandHandle.Wait` and `GatewayOAuthHandle.Wait` already did.
- **Cancelled waits resume.** When the ctx of a `Wait` ends first, `Wait` returns the ctx error and the handle keeps following the turn or login, so a later `Wait` returns the outcome. `Thread.Run` owns its handle, so a cancelled `Run` stops following the turn.
- **Local cancellation by default.** Ending a ctx stops only the local wait or stream; the turn keeps running on the runtime and can still be stopped with `TurnHandle.Interrupt`. With `TurnOptions.InterruptOnCancel` (or `ReviewStartOptions.InterruptOnCancel`), ending the ctx of `Run`, `Wait`, `Stream`/`Events`, or of a `Turn`/`Run`/`Reviews.Start` call whose start request was already sent, interrupts the turn on the runtime once, with a context detached from the cancelled one and bounded by 5s. Closing a stream with `Close`, and cancelling after the turn finished, never interrupt. A failed interrupt is joined to the returned ctx error. `NotificationStream.Next(ctx)` still closes its stream when its own ctx ends; only the ctx passed to `Stream` or `Wait` decides about the interrupt.

## Failures And Back-Pressure

- **Runtime crash.** When the runtime exits or the connection drops, calls and streams fail with `*TransportError`. For a runtime the SDK launched, `ExitCode` holds its exit code (or `Signal` the signal that killed it) and `StderrTail` the redacted end of its stderr; `errors.Unwrap` returns the underlying transport failure. Frames the runtime wrote before exiting are still delivered, and a runtime descendant that inherited its pipes cannot hide the exit for more than a couple of seconds.
- **Unsubscribed events.** Events for a turn, process or other resource that no stream observes yet are buffered so a late `Stream()` still sees them. The buffer is bounded by `ClientLimits.PendingTurnMap`, `PendingTurnQueue` and `PendingNotificationBytes`. When it is full, the oldest buffered resource is evicted: subscribing to it later fails with `*OverflowError`, while the client and all other streams keep working. Records of evicted resources are bounded too; the oldest are forgotten the same way buffered events expire after `LifecycleInactivityTimeout`.
- **Malformed known notifications.** If a notification with a known method does not decode, the streams it is routed to close with `*DecodeError`, and so does a later subscription to the same resource; global streams receive a `MalformedNotification` carrying the method, raw params and the error, and the connection stays open. Closing a stream does not stop the turn or process on the runtime: interrupt it explicitly or use `InterruptOnCancel` if it must stop. A malformed JSON-RPC frame still ends the connection.
- **Shutdown.** `Close` first fails pending calls and closes streams with `*ClosedError`, then closes the runtime's stdin, keeps draining its stdout and waits up to `ShutdownGracePeriod` for it to exit, and kills it only after that. The runtime of the same build exits on EOF within tens of milliseconds when idle; its own RPC drain may take longer while work is in flight.

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
- `examples/codextest`: a consumer function and its test against the `codextest` fake app-server.

## Testing With codextest

`codextest.Server` is an in-process fake app-server: pass it as `ClientConfig.Transport`. It answers `initialize` with this SDK build's protocol digests and the protocol mode the client asked for, records the initialize params, and reports handshake violations through `Err()`. Script each method with `Respond` (fixed result), `RespondError`, or `Handle` (a handler that may block and may queue notifications right after its response with `Call.NotifyAfter`); unscripted methods fail with JSON-RPC error `-32601` and are listed by `Unhandled()`. `Notify` sends notifications, `Request` sends a server request such as an approval and returns the client's answer, `Disconnect` simulates a runtime exit, and `Requests`/`WaitForRequest` inspect what the client sent. Builders such as `ThreadStartResponse`, `TurnStartResponse`, `AgentMessageCompleted` and `TurnCompleted` cover a typical turn.

```go
srv := codextest.NewServer()
defer srv.Close()
srv.Respond("thread/start", codextest.ThreadStartResponse("thread-1"))
srv.Handle("turn/start", func(ctx context.Context, call *codextest.Call) (any, error) {
    _ = call.NotifyAfter("item/completed", codextest.AgentMessageCompleted("thread-1", "turn-1", "item-1", "4"))
    _ = call.NotifyAfter("turn/completed", codextest.TurnCompleted("thread-1", "turn-1", protocol.TurnStatusCompleted))
    return codextest.TurnStartResponse("turn-1"), nil
})
client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
```

See `examples/codextest` for a complete test, including `InterruptOnCancel`.

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

## Versioning

Go module releases use subdirectory tags such as `sdk/go/v1.2.3`. Published tags are immutable; a bad published release is superseded by a higher patch version or documented retraction rather than being overwritten.
