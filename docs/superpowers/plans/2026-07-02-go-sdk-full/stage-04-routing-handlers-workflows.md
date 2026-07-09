# Stage 4: Routing, Server Handlers, And High-Level Workflows

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Implement notification routing, server-to-client handlers, high-level thread/turn/login workflows, input helpers, streams, and cancellation behavior.

**Architecture:** Rust-owned lifecycle metadata defines SDK-neutral wire routing and wire cleanup triggers. A checked-in Go lifecycle mapping owns high-level handle close, client close, context cancel, timeout, overflow behavior, queues, and ergonomic helpers.

**Tech Stack:** Go concurrency, generated `protocol`, `internal/jsonrpc`, mock app-server harness.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:300-347`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:382-455`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:500-555`
- `sdk/python/src/openai_codex/_message_router.py`
- `sdk/python/src/openai_codex/api.py`
- `sdk/python/tests/test_app_server_run.py`
- `sdk/python/tests/test_app_server_streaming.py`

## Files

- Create: `sdk/go/router.go`
- Create: `sdk/go/lifecycle_mapping.go`
- Create: `sdk/go/stream.go`
- Create: `sdk/go/thread.go`
- Modify: `codex-rs/app-server/src/request_processors/turn_processor.rs`
- Modify if Stage 1 has not already landed the exact server-owned limit contract: `codex-rs/app-server-protocol/src/protocol/common.rs`, `codex-rs/app-server-protocol/src/protocol/v2/turn.rs`, `codex-rs/app-server-protocol/src/protocol/v2/tests.rs`, generated app-server schema fixtures, and generated Go protocol/manifest outputs.
- Create: `sdk/go/output_schema.go`
- Create: `sdk/go/turn.go`
- Create: `sdk/go/accounts.go`
- Create: `sdk/go/reviews.go`
- Create: `sdk/go/mcp.go`
- Create: `sdk/go/login.go`
- Create: `sdk/go/input.go`
- Modify: `sdk/go/handlers.go`
- Modify generated: `sdk/go/handlers_generated.go`
- Modify generated: `sdk/go/protocol/server_request_metadata.go`
- Create: `sdk/go/router_test.go`
- Create: `sdk/go/lifecycle_mapping_test.go`
- Create: `sdk/go/thread_test.go`
- Create: `sdk/go/login_test.go`
- Create: `sdk/go/accounts_test.go`
- Create: `sdk/go/reviews_test.go`
- Create: `sdk/go/mcp_test.go`
- Create: `sdk/go/handlers_test.go`
- Create: `sdk/go/internal/testharness/*`

## Tasks

### Task 4.1: Build Checked-In Go Lifecycle Mapping

- [ ] Create `sdk/go/lifecycle_mapping.go` with one mapping row per high-level handle kind. Each row must reference a Rust-owned lifecycle `resourceDomain`/`startMethod` and add Go-owned triggers only:
  - `handleClose`
  - `clientClose`
  - `contextCancel`
  - `timeout`
  - `overflow`.
- [ ] Do not add Go-owned triggers to the Rust manifest. The Go mapping may depend on Rust wire lifecycle identities but must remain Go API metadata.
- [ ] Add `sdk/go/lifecycle_mapping_test.go` that:
  - verifies every Rust-owned lifecycle row used by a high-level handle has a Go mapping
  - verifies every Go mapping references an existing generated Rust lifecycle row
  - verifies filesystem watch cleanup includes `handleClose`, `clientClose`, `timeout`, and `overflow` in Go mapping, while Rust metadata only carries wire cleanup such as `fs/unwatch`.

### Task 4.2: Build Notification Router

- [ ] Implement router domains from generated lifecycle metadata:
  - thread
  - turn
  - command
  - process
  - filesystem watch
  - OAuth/login
  - realtime
  - fuzzy file search sessions
  - MCP OAuth/elicitation/tool/resource workflows
  - plugin updates and plugin OAuth/elicitation workflows
  - marketplace update/mutation notifications
  - app list updates
  - review lifecycle
  - remote control pairing/session notifications
  - external agents
  - skills/hooks updates
  - account/model/environment/collaboration-mode update notifications when manifest exposes them
  - Windows sandbox and experimental feature notifications when manifest exposes them
  - global subscribers.
- [ ] Consume generated `NotificationRoutingStrategy`, `RoutingRef`, and `IdentityExtractor` metadata for all routed notifications. Do not infer routing from method-name prefixes or hand-maintained Go maps.
- [ ] Add a routing-table parity test that compares generated routing metadata against `appendix-current-protocol-inventory.md#server-notification-routing-review-seed` for current notifications before exercising Go router behavior.
- [ ] When a notification has multiple identity extractors, deliver it to every matching live handle/resource stream and preserve optional-identity behavior defined by the manifest. If any routed key accepts the notification, do not create stale pending entries for alternate extractor keys on the same notification.
- [ ] Preserve unknown server notifications for forward compatibility. Any server notification method not present in generated typed metadata, including future methods and schema-excluded/raw-only methods, must be delivered to raw/global subscribers as `UnknownNotification` with original method, raw params, and top-level trace metadata intact. Unknown delivery must not break routing for known notifications, must not be treated as a terminal lifecycle event, and must respect bounded buffers/backpressure.
- [ ] Implement bounded queues using normalized `ClientLimits`; pending-buffer queue/map overflow must persist an overflow state for the affected routed handle key and surface `OverflowError` on later subscription instead of silently presenting a truncated stream/result as successful. Overflow sentinels for discarded pending keys must remain reachable until lifecycle inactivity timeout, explicit close, or subscription drain; later overflows for distinct keys must not evict earlier sentinels.
- [ ] Pending replay for multi-domain subscriptions, including turn/review streams that subscribe to all generated turn-scoped domains, must preserve original server arrival order across routed domains. If the same notification is reachable through multiple subscribed keys, replay it once.
- [ ] Validate `ClientConfig.NotificationOptOuts` before `initialize`: unknown notification methods are typed configuration errors, and default high-level mode rejects opt-outs only when they disable a currently implemented/enabled Stage 4 lifecycle-dependent high-level workflow.
- [ ] Respect `ClientModeRawOnly` disabled workflow metadata from Stage 3. Metadata must be deterministic and workflow-level for the currently implemented Stage 4 high-level workflows, including dependency details for opted-out notifications. Raw-only metadata must name browser-login and device-code-login handles separately from API-key login, because API-key login is a safe one-shot account wrapper and must not be reported as disabled. Any high-level API whose lifecycle depends on opted-out notifications must return a typed configuration error before starting work; safe one-shot account wrappers that do not require notification lifecycle cleanup (`Accounts.Read`, `Accounts.Usage`, `Accounts.RateLimits`, `Accounts.Logout`, API-key login) may continue through the generated raw client.
- [ ] Implement cleanup triggers from Rust wire metadata plus checked-in Go lifecycle mapping:
  - JSON-RPC response
  - terminal notification
  - explicit method response
  - handle close
  - client close
  - context cancel
  - timeout
  - overflow.
- [ ] Tests:
  - early turn notification buffering
  - multiple concurrent turns stream independently
  - a notification delivered to a live turn stream does not create stale alternate-key pending state or evict unrelated early pending events
  - pending replay is ordered before live delivery for the subscribed keys; a live terminal notification cannot close a stream before older buffered pending notifications are replayed
  - cross-domain pending replay preserves server arrival order for turn/review streams, including interleaved `turn/*` and `item/*` events
  - pending queue overflow and pending map overflow close only the affected handle with `OverflowError` when transport is trustworthy, including repeated distinct pending-map overflows where earlier overflow sentinels must survive until subscription or inactivity cleanup
  - orphaned buffers clean after timeout
  - per-domain routing tests for turn, command, process, filesystem watch, OAuth/login, realtime, fuzzy search session, MCP, plugin, marketplace, app, review, remote control, external agent, and skills/hooks notifications
  - generated lifecycle/resource-domain parity for terminal dependencies that Stage 4 relies on, including `fs/watch` -> `fs/changed` using the `fs` notification route domain and `mcpServer/oauth/login` -> `mcpServer/oauthLogin/completed` using the `mcpServer` route domain rather than synthetic domains that have no routed notifications
  - unknown notification is delivered to raw/global subscribers with method and raw params preserved, while known routed notifications still go to the correct lifecycle streams
  - default high-level mode rejects unknown opt-outs and currently enabled lifecycle-conflicting opt-outs before `initialize`, unrelated future/non-enabled lifecycle opt-outs still initialize, harmless opt-outs still initialize, raw-only accepts otherwise-conflicting opt-outs with exact workflow-level disabled metadata that distinguishes disabled browser/device login handles from available API-key login, raw-only safe account wrappers still work, and raw-only disabled `Thread.Run`, process stream, watch, login, MCP OAuth/elicitation, realtime, fuzzy search session, and remote-control handles fail before sending a request when their required notifications were opted out.

### Task 4.3: Implement Streams

- [ ] Implement receive-only stream channels plus `Err()` and `Close()`.
- [ ] `Close()` unsubscribes local consumer and does not interrupt remote work.
- [ ] Stream cancellation follows the cancellation matrix.
- [ ] Tests cover close, context cancel, terminal notification, and client close.

### Task 4.4: Implement Server Handlers

- [ ] Consume `sdk/go/handlers_generated.go` and `sdk/go/protocol/server_request_metadata.go` from Stage 2. Do not hand-maintain request coverage in handwritten code.
- [ ] Implement the handwritten executor around generated decode/dispatch helpers in `sdk/go/handlers.go`.
- [ ] The generated artifacts must own:
  - typed optional handler slots for every SDK-public server request
  - function adapter types
  - server request method-to-payload/response mapping
  - compatibility dispatch rows for non-public server requests
  - handler capability metadata for local registration, dispatch, docs, and tests.
- [ ] Implement compatibility dispatch for non-public server requests according to generated manifest visibility.
- [ ] Handler execution must use bounded queue, concurrency limit, and timeout.
- [ ] Reader goroutine must never execute user handler inline.
- [ ] Handler callbacks into the same SDK client are supported through the normal request path. The reader goroutine, waiter map lock, writer lock, and handler executor queue lock must not be held while user callback code runs. Reentrant client calls from a handler must either complete normally or fail only with the underlying request/context error, never with a deadlock or an implementation-defined reentrant-use rejection.
- [ ] While a handler is blocked or waiting on its timeout, unrelated client response routing and notification routing must continue. The server-request executor must not own the only receive path, waiter map lock, or writer lock for the duration of user callback execution.
- [ ] When the bounded handler queue or concurrency limit is exceeded, return a typed busy/overload JSON-RPC error for that server request and keep the connection usable for later requests.
- [ ] Initialize capabilities must use only fields that exist in the shared Rust `InitializeCapabilities` protocol at the time of implementation: `experimental_api`, `request_attestation`, `mcp_server_openai_form_elicitation`, and notification opt-outs. Handler flows such as approvals, ChatGPT token refresh, dynamic tools, user input, permissions, and current time must be represented in generated local handler metadata and dispatch behavior, but must not invent Go-only initialize fields ignored by Rust serde.
- [ ] If implementation genuinely needs a new negotiated initialize capability, move that protocol change back to Stage 1 first with Rust schema fixture updates, app-server behavior, Python/TypeScript fallout checks, docs, and fresh review before Stage 4 proceeds.
- [ ] Tests:
  - generated coverage test fails if any `ServerRequest` lacks handler or compatibility metadata
  - capability negotiation test covers only the shared initialize capabilities above and separately verifies local handler capability metadata for every generated SDK-public handler
  - registered handler success
  - missing handler unsupported-method error
  - handler error maps to JSON-RPC error without leaking secrets
  - handler timeout keeps connection usable
  - handler queue/concurrency overload returns typed busy error and keeps connection usable
  - a blocked handler does not block an unrelated response from completing a pending client request
  - concurrent server requests interleaved with client responses route to the correct handler/waiter without deadlock
  - handler callback into the same client succeeds through the normal request path while unrelated response and notification routing continues
  - unknown server request raw hook.

### Task 4.5: Implement Input Helpers

- [ ] Implement:
  - `Text(string)`
  - image URL input. Current Rust app-server rejects `http`/`https` remote image URLs; the Go helper must fail closed with a typed SDK error before JSON-RPC write for remote URLs and direct callers to `DataURL`/inline data URLs instead.
  - data URL input
  - local image input with safe bounded file read using `ClientConfig.Limits.MaxLocalInputBytes` and default `DefaultMaxLocalInputBytes = 16 MiB`
  - skill input
  - mention input
  - string shorthand where Go type system permits.
- [ ] Local image/file helpers must check file size with `os.Stat` when available before reading, must read through an `io.LimitedReader` or equivalent when metadata is unavailable or races, and must return a typed `LocalInputSizeError` before any JSON-RPC write when size exceeds `MaxLocalInputBytes`. They must not rely on `MaxFrameBytes` as the first bound after reading into memory.
- [ ] Tests marshal helpers to generated protocol params.
- [ ] Tests cover local image/file inputs below the limit, exactly at the limit, and over the limit; the over-limit case must prove no unbounded read occurs and no request is written to the transport.

### Task 4.6: Implement Core Threads And Turns

- [ ] Implement `ThreadsClient.Start`.
- [ ] Implement `Thread.Run`, `Thread.Turn`, `TurnHandle.Stream`, `TurnHandle.Steer`, `TurnHandle.Interrupt`.
- [ ] Implement `Reviews.Start` as a high-level handle workflow, not only a thin generated RPC call:
  - `func (c *ReviewsClient) Start(ctx context.Context, opts ReviewStartOptions) (*ReviewHandle, error)`
  - `func (h *ReviewHandle) Events(ctx context.Context) (*ReviewStream, error)`
  - `func (h *ReviewHandle) Wait(ctx context.Context) (*ReviewResult, error)`
  `ReviewHandle` must own `reviewThreadId` and `turn.id` from `ReviewStartResponse`, subscribe only to routed notifications for that review thread/turn identity, terminate on the ordinary review turn lifecycle such as `turn/completed`, and collect the review result from the review turn stream/result. `item/autoApprovalReview/*` belongs only to approval/Guardian item notifications and must not be used as the lifecycle for `review/start`. The handle must clean up subscriptions on completion, context cancellation, and client close.
- [ ] Implement `MCP.OAuthLogin` as a high-level handle workflow, not only a thin generated RPC call:
  - `func (c *MCPClient) OAuthLogin(ctx context.Context, opts MCPOAuthLoginOptions) (*MCPOAuthHandle, error)`
  - `func (h *MCPOAuthHandle) Wait(ctx context.Context) (*MCPOAuthResult, error)`
  - `func (h *MCPOAuthHandle) Cancel(ctx context.Context) error` if the current manifest exposes a safe cancel/follow-up path; otherwise document the manifest-backed not-applicable reason.
  `MCPOAuthHandle` must route `mcpServer/oauthLogin/completed` by server name/thread identity from the manifest and must not require callers to use a global notification filter. `threadId` is optional in the current completion payload: the handle must always subscribe by MCP server `name`, add `threadId` routing when present, accept name-matched completion without `threadId`, and ignore mismatched `threadId` without closing the wait stream.
- [ ] Implement `Accounts.LoginWithAPIKey(ctx, key APIKey) error` or an equivalent option-based helper over the existing `LoginAccountParams::ApiKey` protocol variant. Errors, logs, test names, and docs must never echo the API key value; use a typed redacted secret wrapper for examples/tests.
- [ ] Implement and test Stage 4 high-level account thin wrappers that are marked `implemented-stage4` in generated inventory (`Accounts.Read`, `Accounts.Usage`, `Accounts.RateLimits`, and `Accounts.Logout`). Tests must call the high-level wrappers, not only the generated raw client, and must prove the expected wire methods/params are sent.
- [ ] Use exact high-level signatures that expose JSON-RPC failures as Go errors before returning handles or streams:
  - `func (t *Thread) Run(ctx context.Context, input Input, opts TurnOptions) (*RunResult, error)`
  - `func (t *Thread) Turn(ctx context.Context, input Input, opts TurnOptions) (*TurnHandle, error)`
  - `func (h *TurnHandle) Stream(ctx context.Context) (*TurnStream, error)`
  - `func (h *TurnHandle) Steer(ctx context.Context, input Input, opts ...SteerOptions) error`
  - `func (h *TurnHandle) Interrupt(ctx context.Context) error`
  Raw generated params such as `protocol.TurnStartParams` and `protocol.TurnSteerParams` remain available only through generated/raw resource APIs, not through these high-level handle methods.
- [ ] Leave full thread namespace coverage such as resume/fork/list/archive/settings/memory/Guardian-style methods to Stage 5's method-level resource matrix; this task owns only the high-level first-screen workflow.
- [ ] Define high-value option structs in `thread.go` or a focused `thread_options.go` only from current generated protocol fields. The generated matrix is the authority; no public option may exist unless it maps to a current first-class generated field or this stage first adds a reviewed protocol-change task, source anchor, tests, and schema fixture updates. Do not invent Go-only options for thread/turn fields that are absent from the current Rust params, such as thread-level `ReasoningEffort`, `InstructionOverride`, `PromptOverride`, `ClientUserMessageID`, `CollaborationMode`, or turn-level `Provider`.
  - `ThreadStartOptions` may map only to current `ThreadStartParams` fields: `Model`, `ModelProvider`, `AllowProviderModelFallback`, `ServiceTier`, `CWD`, `RuntimeWorkspaceRoots`, `ApprovalPolicy`, `ApprovalsReviewer`, `Sandbox`, `Permissions`, `Config`, `ServiceName`, `BaseInstructions`, `DeveloperInstructions`, `Personality`, `MultiAgentMode`, `Ephemeral`, `HistoryMode`, `SessionStartSource`, `ThreadSource`, `Environments`, `DynamicTools`, `SelectedCapabilityRoots`, `MockExperimentalField`, and `ExperimentalRawEvents`, with experimental fields gated by generated metadata.
  - `ThreadResumeOptions` may map only to current `ThreadResumeParams` fields: `ThreadID`, `History`, `Path`, `Model`, `ModelProvider`, `ServiceTier`, `CWD`, `RuntimeWorkspaceRoots`, `ApprovalPolicy`, `ApprovalsReviewer`, `Sandbox`, `Permissions`, `Config`, `BaseInstructions`, `DeveloperInstructions`, `Personality`, `ExcludeTurns`, and `InitialTurnsPage`, with experimental fields gated by generated metadata.
  - `ThreadForkOptions` may map only to current `ThreadForkParams` fields: `ThreadID`, `LastTurnID`, `Path`, `Model`, `ModelProvider`, `ServiceTier`, `CWD`, `RuntimeWorkspaceRoots`, `ApprovalPolicy`, `ApprovalsReviewer`, `Sandbox`, `Permissions`, `Config`, `BaseInstructions`, `DeveloperInstructions`, `Ephemeral`, `ThreadSource`, and `ExcludeTurns`, with experimental fields gated by generated metadata.
  - `TurnOptions` may map only to current `TurnStartParams` fields other than SDK-owned `ThreadID` and required `Input`: `ClientUserMessageID`, `ResponsesAPIClientMetadata`, `AdditionalContext`, `Environments`, `CWD`, `RuntimeWorkspaceRoots`, `ApprovalPolicy`, `ApprovalsReviewer`, `SandboxPolicy`, `Permissions`, `Model`, `ServiceTier`, `Effort`, `Summary`, `Personality`, `OutputSchema`, `CollaborationMode`, and `MultiAgentMode`, with experimental fields gated by generated metadata. `SteerOptions` may map only to current `TurnSteerParams` fields other than SDK-owned `ThreadID`, `ExpectedTurnID`, and required `Input`: `ClientUserMessageID`, `ResponsesAPIClientMetadata`, and `AdditionalContext`; start-only options such as model, sandbox, permissions, output schema, environments, effort, and collaboration settings must not be accepted by `TurnHandle.Steer`. `AdditionalContext` is model-visible context and must not be exposed as an unbounded raw map: high-level `TurnOptions`, `SteerOptions`, and generated/raw `turn/start` and `turn/steer` senders must enforce normalized `ClientLimits` before any JSON-RPC write. Required limits: max entry count, max key bytes, max value bytes, max aggregate key+value bytes, and a documented guarantee that a single fragment cannot exceed the repo's 10K-token item cap under the chosen byte limits; any default that could plausibly cross 1K tokens must be called out for manual review in the stage handoff. If those limits cannot be source-backed in Stage 4, omit `AdditionalContext` from high-level options and mark generated/raw `AdditionalContext` calls unsupported with a typed error until the Rust/app-server limits land.
- [ ] Add Rust/app-server enforcement before `map_additional_context` in `codex-rs/app-server/src/request_processors/turn_processor.rs` for both `turn/start` and `turn/steer`, using the same bounded contract exposed in the protocol/manifest. Do not rely solely on Go SDK validation, because raw clients and other SDKs can send the same fields. The Rust side must reject over-limit additional-context entry count, key bytes, value bytes, total bytes, and any item that would violate model-visible context caps before constructing `core::protocol::AdditionalContextEntry`.
- [ ] Add generated metadata fields or manifest constants for additional-context limits, for example `maxAdditionalContextEntries`, `maxAdditionalContextKeyBytes`, `maxAdditionalContextValueBytes`, and `maxAdditionalContextTotalBytes`, so Go high-level and raw senders use the server-owned contract instead of Go-only guesses. Include schema/manifest drift tests proving these limits are present before `AdditionalContext` is public.
- [ ] Create `sdk/go/output_schema.go` with typed helpers for the current first-class `TurnStartParams.outputSchema` field. The public API must accept structured schema values without callers building raw JSON maps for normal turn workflows, for example a `JSONSchema(name string, schema protocol.JsonSchema)` helper or an equivalent typed wrapper that preserves generated protocol types.
- [ ] Add a thread/turn option coverage test that inspects generated current `ThreadStartParams`, `ThreadResumeParams`, `ThreadForkParams`, and `TurnStartParams` fields. Every current first-class field must have an ergonomic option/helper mapping or an explicit reviewed omission reason in a small checked table, and every public option field must map back to a current generated protocol field unless a reviewed protocol-change stage added that field first. This is separate from raw generated protocol reachability.
- [ ] For each option struct, add tests that compare the entire generated params object for a populated fixture, a zero-value fixture, and a stable-mode fixture containing an experimental setting. Stable-mode experimental option use must be rejected before a JSON-RPC frame is written.
- [ ] Add an `OutputSchema` params equality test proving `TurnOptions.OutputSchema` maps exactly to generated `TurnStartParams.outputSchema` and is used by both `Thread.Run` and streaming `Thread.Turn`.
- [ ] Add below-limit, at-limit, and over-limit tests for `AdditionalContext` in high-level `Thread.Run`, streaming `Thread.Turn`, `TurnHandle.Steer`, and generated/raw `turn/start` and `turn/steer`. Tests must prove over-limit cases return a typed SDK/config error before write, and Rust app-server tests must prove over-limit raw requests are rejected before model-visible context is built.
- [ ] `RunResult` must include completed turn id, status, error, timestamps, duration, final response, items, and token usage.
- [ ] `Thread.Run` cancellation must not imply remote interrupt unless explicit option requests it.
- [ ] Tests:
  - successful run result collection
  - streaming deltas
  - failed turns
  - token usage
  - steer
  - interrupt
  - review start handle routes started/completed notifications, returns `ReviewResult`, and removes subscriptions after terminal completion
  - MCP OAuth handle routes completion without global filtering, accepts completion with omitted optional `threadId`, ignores wrong-thread completion without closing the wait stream, and handles context cancellation cleanup
  - API-key login helper maps to the generated `LoginAccountParams::ApiKey` variant without leaking the key in errors/logs.

### Task 4.7: Implement Login Handles

- [ ] Implement browser and device-code `LoginHandle` with `Wait` and `Cancel`.
- [ ] `LoginHandle.Wait` must subscribe to both the specific `loginId` route and the manifest's global-fallback account route. If `account/login/completed` arrives without `loginId`, the handle must fail closed with a typed unsupported/uncorrelated-completion error instead of hanging or treating the completion as successful.
- [ ] Tests use mocked auth/Responses only:
  - unauthenticated account read
  - fake API-key login
  - device/browser login start/wait/cancel
  - usage/rate-limit fixture read.
- [ ] No CI test may call real auth services or depend on user credentials.

### Task 4.8: Verify And Commit

- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cd codex-rs
just test -p codex-app-server-protocol
just test -p codex-app-server
cd ../sdk/go
go test ./...
```

- [ ] Commit:

```bash
git add sdk/go
git add codex-rs/app-server/src/request_processors/turn_processor.rs
git add codex-rs/app-server-protocol/src/protocol/common.rs codex-rs/app-server-protocol/src/protocol/v2/turn.rs codex-rs/app-server-protocol/src/protocol/v2/tests.rs codex-rs/app-server/README.md
# If the schema/generator checks changed generated protocol or fixture outputs, add the exact changed paths shown by git status in the same commit.
git commit -m "feat(go-sdk): add routing and high-level workflows"
```

## Stage Review

Fresh blind engineering and product reviews are mandatory because this stage defines the user-facing SDK experience.
