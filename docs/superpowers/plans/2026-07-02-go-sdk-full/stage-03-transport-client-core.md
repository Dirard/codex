# Stage 3: Transport, Runtime Startup, And Client Core

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Implement the JSON-RPC runtime core: stdio process startup, injected transport, request correlation, serialized writes, stderr drain, initialize handshake, compatibility checks, config validation, close semantics, and typed errors.

**Architecture:** One reader owns stdout, one writer path owns stdin, request waiters preserve JSON-RPC id shape, and `NewClient` either returns a fully initialized client or cleans up all resources.

**Tech Stack:** Go standard library (`os/exec`, `encoding/json`, `bufio`, `context`, `sync`), generated `protocol`.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:130-211`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:382-425`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:582-586`
- `sdk/python/src/openai_codex/client.py`
- `sdk/python/src/openai_codex/_message_router.py`

## Files

- Modify: `sdk/go/client.go`
- Modify: `sdk/go/config.go`
- Modify: `sdk/go/errors.go`
- Modify: `sdk/go/limits.go`
- Inspect only unless adding release-rejection coverage: `codex-rs/cli/src/main.rs`, `codex-rs/app-server/src/main.rs`, `codex-rs/app-server/src/config_manager.rs`, `codex-rs/login/src/auth/agent_identity.rs`, and `codex-rs/agent-identity/src/lib.rs`. Do not add SDK-only auth redirect hooks, managed-config bypasses, or hidden startup hooks to the release/default `CODEX_EXEC_PATH app-server` path.
- Create/modify: Rust app-server/CLI/login tests only to prove release/default `codex app-server --listen stdio://` rejects or ignores ambient debug/test hook envs and hidden startup args. Mock auth/account coverage must use injected Go transport or a distinct test-only fixture runtime that cannot be confused with release-shaped `CODEX_EXEC_PATH`.
- Create: `sdk/go/raw.go`
- Create: `sdk/go/trace.go`
- Create: `sdk/go/internal/jsonrpc/envelope.go`
- Create: `sdk/go/internal/jsonrpc/transport.go`
- Create: `sdk/go/internal/jsonrpc/stdio.go`
- Create: `sdk/go/internal/jsonrpc/client.go`
- Create: `sdk/go/internal/jsonrpc/ring.go`
- Create: `sdk/go/internal/jsonrpc/client_test.go`
- Create: `sdk/go/internal/jsonrpc/stdio_test.go`
- Create: `sdk/go/config_test.go`
- Create: `sdk/go/compatibility_test.go`
- Create: `sdk/go/raw_test.go`
- Create: `sdk/go/trace_test.go`

## Tasks

### Task 3.1: Implement JSON-RPC Envelope And Request IDs

- [ ] Create envelope types preserving top-level trace:

```go
package jsonrpc

import (
	"encoding/json"
	"github.com/openai/codex/sdk/go/protocol"
)

type Envelope struct {
	ID     *protocol.RequestID `json:"id,omitempty"`
	Method string              `json:"method,omitempty"`
	Params json.RawMessage     `json:"params,omitempty"`
	Result json.RawMessage     `json:"result,omitempty"`
	Error  *RPCError           `json:"error,omitempty"`
	Trace  json.RawMessage     `json:"trace,omitempty"`
}

type RPCError struct {
	Code    int64           `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}
```

- [ ] Add tests for string and integer ids, including integer reply echo and an `RPCError.Code` value that does not fit in 32 bits.
- [ ] Create `sdk/go/trace.go` with outbound per-call trace options and inbound handler trace metadata:

```go
package codex

import "context"

type TraceContext struct {
	TraceParent string `json:"traceparent,omitempty"`
	TraceState  string `json:"tracestate,omitempty"`
}

type CallOptions struct {
	Trace *TraceContext
}

type callOptionsKey struct{}

func WithCallOptions(ctx context.Context, opts CallOptions) context.Context {
	return context.WithValue(ctx, callOptionsKey{}, opts)
}

func TraceFromContext(ctx context.Context) (*TraceContext, bool) {
	opts, ok := ctx.Value(callOptionsKey{}).(CallOptions)
	return opts.Trace, ok && opts.Trace != nil
}
```

- [ ] Wire outbound `CallOptions.Trace` into the JSON-RPC envelope top-level `trace` field, never inside `params`.
- [ ] Attach inbound request trace data to handler context using `WithCallOptions` or an equivalent typed metadata path.
- [ ] Add tests proving outbound trace round-trips outside `params`, uses lowercase wire keys `traceparent`/`tracestate`, and inbound server-request trace is visible to handlers.

### Task 3.2: Implement Framed Reader And Serialized Writer

- [ ] Implement a reader that does not use `bufio.Scanner`.
- [ ] Enforce `MaxFrameBytes`.
- [ ] Implement a single serialized writer path.
- [ ] Tests:
  - frame above 64 KiB succeeds
  - frame at 16 MiB succeeds
  - frame over configured limit returns typed frame-size error
  - concurrent writes produce complete JSON lines without interleaving.

### Task 3.3: Implement Stdio Process Transport

- [ ] Keep the package boundary acyclic:
  - root package `github.com/openai/codex/sdk/go` may import `internal/jsonrpc`
  - `internal/jsonrpc` must not import the root package
  - the internal transport interface must live in `internal/jsonrpc` or another dependency-only internal package
  - root `codex.Transport` is adapted to that internal interface at the root package boundary before calling into `internal/jsonrpc`.
- [ ] Add an import-boundary test or script that fails if any `sdk/go/internal/...` package imports `github.com/openai/codex/sdk/go`.
- [ ] In `internal/jsonrpc/transport.go`, define the internal transport adapter with the same frame contract as public `codex.Transport` from Stage 0, without importing the root package:
  - `Receive(ctx)` returns one complete JSON-RPC object frame as `json.RawMessage`
  - `Send(ctx, frame)` accepts one complete JSON-RPC object frame without a trailing newline or content-length header
  - stdio transport owns newline framing at the process boundary
  - injected transports own their own external framing, if any, and are never asked to parse partial frames
  - the JSON-RPC client starts exactly one receive goroutine per transport
  - all outbound `Send` calls are serialized by the SDK writer path
  - `Close` is idempotent, unblocks any pending `Receive`, and causes later calls to return `ClosedError` or `context` cancellation.
- [ ] Add injected-transport tests for: complete-frame receive, serialized concurrent sends, receive unblock on close, close idempotence, context cancellation, and no newline/header requirement on frames passed to injected transports.
- [ ] Implement `stdio.go` to start `codex app-server --listen stdio://`.
- [ ] Implement runtime resolution before spawn with this exact precedence:
  1. If `ClientConfig.Transport` is non-nil, use the injected transport and do not resolve or spawn any process.
  2. Else if `ClientConfig.CodexPath` is non-empty, use that path.
  3. Else use `exec.LookPath("codex")`.
- [ ] Do not add a `RuntimeDiscovery` opt-in gate that changes the approved quickstart contract. `ClientConfig{}` must remain useful as the design specifies: a user with a compatible `codex` on `PATH` can call `NewClient(ctx, ClientConfig{})`. Safety comes from strict initialize digest/mode validation and compatibility-policy restrictions, not from disabling default discovery. README examples may prefer explicit `CodexPath` for deterministic tests and services, and Stage 6 positive real-runtime tests must still use `CODEX_EXEC_PATH`/`CodexPath` or injected transports so CI never accidentally proves an ambient machine binary.
- [ ] When spawning the runtime, build argv in this order:
  - validated arguments rendered only from typed `ClientConfig.Launch` allowlist before SDK-owned app-server arguments
  - one `--config key=value` pair for each validated non-secret `ConfigOverrides` entry, sorted by key for deterministic tests
  - `app-server`
  - no SDK-owned test-only app-server args in release/default runtime launches. Any startup bypass needed for SDK tests must use an injected transport or a separately reviewed debug/test fixture runtime; the public Go SDK must not smuggle hidden app-server args into the release-shaped `CODEX_EXEC_PATH app-server --listen stdio://` path.
  - `--listen`
  - `stdio://`.
- [ ] Do not promote the current debug-only app-server hooks into release-shipped behavior. Source anchors show the existing login issuer override is intentionally `#[cfg(debug_assertions)]` in `codex-rs/app-server/src/request_processors/account_processor.rs`, and the current app-server plugin/config test hooks are also debug-only or absent from the `codex app-server` CLI path. Keep that boundary: release/default `codex app-server --listen stdio://` must ignore or reject `CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG`, `CODEX_APP_SERVER_LOGIN_ISSUER`, any proposed `CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS`, any proposed `CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE`, and `--disable-plugin-startup-tasks-for-tests`.
- [ ] Stage 3 may add debug/test-fixture coverage for startup suppression, managed-config bypass, and mocked auth only if it remains unavailable in release/default binaries: either `#[cfg(debug_assertions)]`, Rust `#[cfg(test)]`, an injected Go transport, or a separate test-only fixture runtime with a distinct binary/arg path that cannot be confused with `CODEX_EXEC_PATH` release-readiness. Any such fixture must have source-anchored tests proving the release/default `codex app-server` path rejects or ignores the same env vars/hidden args even when launched directly by a user outside the Go SDK.
- [ ] Mocked auth/account integration must not require a release-shipped auth base-url env override. The Go SDK auth tests should use injected transport or the separately reviewed debug/test fixture runtime to cover device-code issuer, OAuth token exchange, agent-identity/JWKS, token usage, rate limits, and rate-limit reset-credit backend calls. Release-shaped real-runtime tests may cover unauthenticated account errors and config-driven Responses traffic, but they must not redirect ChatGPT/OAuth/backend/JWKS endpoints through a localhost/mock auth env hook.
- [ ] Add no additional release-shipped app-server startup/config/auth test hook names in Stage 3. If real-runtime tests need another hook, stop and update this plan with a source-anchored non-ambient capability gate that a direct user-launched `codex app-server` cannot activate by setting environment variables or hidden args.
- [ ] Do not expose free-form raw CLI args. Keep `ClientConfig.Launch` as a typed options namespace, but Stage 3's initial public launch allowlist is empty. Add a launch option only when this same stage defines a named Go type or constants for the allowed values, render rules, and tests proving unsupported strings cannot reach argv.
  Do not expose `Launch.ConfigProfile` unless this same stage first adds and tests source-anchored support for `codex --profile <name> app-server --listen stdio://`; current CLI support for `--profile` on other runtime commands is not enough for the Go SDK app-server path.
- [ ] Unknown launch option names, raw strings, unsupported future codex global flags, secret-like values, and any option that would render `--config`, `-c`, `app-server`, `--listen`, `stdio://`, `--help`, `--version`, or `--` must fail closed before spawn. Do not use a blacklist as the acceptance boundary.
- [ ] Apply `ClientConfig.CWD` to `exec.Cmd.Dir` when non-empty.
- [ ] Build the child environment from a sanitized parent environment, then merge public `ClientConfig.Env`. The sanitizer must remove known app-server test/debug hook envs before spawn, including `CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG`, `CODEX_APP_SERVER_LOGIN_ISSUER`, any proposed `CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS`, and any proposed `CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE`. Public `ClientConfig.Env` must reject those reserved names with a typed config error before spawn. Explicit non-reserved `Env` entries still override inherited variables with the same key.
- [ ] Return a typed `RuntimeNotFoundError` when no runtime is found. The error must include searched locations (`CodexPath` when supplied and a redacted `PATH` lookup summary for the default `codex` search) and a remediation hint, without leaking environment values.
- [ ] Validate injected `Transport` conflicts with every process-launch field before any runtime lookup: `CodexPath`, `Launch`, `CWD`, `Env`, and `ConfigOverrides`.
- [ ] Drain stderr concurrently into bounded redacted ring buffer.
- [ ] Validate non-secret `ConfigOverrides` before spawn. Reject any override whose key or value is secret-like using the exact conservative classifier from the spec: case-insensitive substrings `api_key`, `apikey`, `token`, `secret`, `password`, `credential`, `auth`, or `cookie`, plus any Codex config metadata that marks a field as secret. Rejection errors, logs, tests, stderr ring buffers, and config-validation traces must redact both key and value.
- [ ] Tests:
  - injected transport plus each process-launch field fails before runtime lookup or spawn
  - injected transport never spawns even when `PATH` contains a fake `codex`
  - resolver uses `CodexPath` before any PATH discovery
  - default process launch calls `exec.LookPath("codex")` only after injected transport and `CodexPath` are absent, starts a compatible PATH-discovered runtime under strict digest/mode validation, and reports PATH lookup details without environment values when no runtime is found
  - a PATH-discovered runtime with a missing, mismatched, or wrong-mode protocol digest fails closed under the zero-value strict compatibility policy before `initialized` is sent
  - missing runtime returns `RuntimeNotFoundError` with searched locations and remediation
  - spawned command argv preserves rendered allowlisted `Launch` options, deterministic non-secret `--config key=value`, then `app-server --listen stdio://`
  - unknown launch options, unsupported/raw future global flags, and `Launch.ConfigProfile` without proven app-server CLI support are rejected before spawn
  - any launch option that would render `--config`, `-c`, `app-server`, `--listen`, `stdio://`, `--help`, `--version`, or `--` is rejected before spawn and redacted when it contains secret-like material
  - `CWD` and merged non-reserved `Env` are applied to the child process without leaking values into errors
  - ambient parent env cannot activate app-server test/debug hooks: inherited `CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG=1`, `CODEX_APP_SERVER_LOGIN_ISSUER`, `CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS`, or `CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE=1` are scrubbed before release/default runtime launches
  - public `ClientConfig.Env` rejects reserved SDK test/control env names before spawn
  - config override validation rejects every listed secret-like substring in keys and values, rejects secret-marked Codex config metadata fields, and redacts both key and value
  - fake process writing more than a pipe buffer to stderr does not block stdout completion.

### Task 3.4: Implement Request Correlation And Close

- [ ] Implement waiter map keyed by `protocol.RequestID`.
- [ ] Context cancellation:
  - before write: no waiter remains
  - after write: local wait returns context error and late response is drained/discarded
  - close fails all waiters and streams.
- [ ] Tests cover cancellation matrix rows from the spec.

### Task 3.5: Validate Config Before Initialize

- [ ] Validate `ClientConfig.Compatibility` before process lookup, spawn, injected transport use, or `initialize`. Unknown integer values must return a typed `ConfigError` and must not write any JSON-RPC frame.
- [ ] Validate `ClientConfig.ProtocolMode` before process lookup, spawn, injected transport use, or `initialize`. Unknown integer values, for example `ProtocolMode(99)`, must return a typed `ConfigError`, must not select a stable/experimental digest, and must not write any JSON-RPC frame.
- [ ] Validate `ClientConfig.NotificationOptOuts` before `initialize`.
- [ ] In default `ClientModeHighLevel`, reject opt-outs for notifications required by lifecycle dependency metadata, including turn terminal notifications, process exit notifications, OAuth/login completion notifications, and any generated terminal cleanup trigger.
- [ ] Accept harmless opt-outs that do not affect enabled high-level workflows.
- [ ] In `ClientModeRawOnly`, accept otherwise-conflicting opt-outs but mark high-level workflows disabled in `Client.Metadata`.
- [ ] Tests:
  - invalid/unknown compatibility policy returns typed `ConfigError` before runtime lookup, spawn, injected transport use, or initialize write
  - invalid/unknown protocol mode returns typed `ConfigError` before runtime lookup, spawn, injected transport use, or initialize write
  - default-mode conflict returns typed `ConfigError` before any process spawn or initialize write
  - harmless opt-out initializes successfully
  - raw-only mode initializes with disabled workflow metadata
  - affected high-level APIs return typed configuration errors before starting work.

### Task 3.6: Implement Initialize Handshake

- [ ] `NewClient` sends generated `initialize`.
- [ ] Decode the raw JSON-RPC initialize result through a Go-internal compatibility envelope before policy evaluation. The envelope must require only the pre-change legacy core fields from `codex-rs/app-server-protocol/src/protocol/v1.rs` (`userAgent`, `codexHome`, `platformFamily`, `platformOs`), capture optional digest/mode fields as pointers or explicit presence flags, retain the raw result bytes, and expose a conversion path that re-decodes through generated `protocol.InitializeResponse` only when the current required digest/mode fields are present. This is the only allowed initialize-response decode exception.
- [ ] Verify returned digest for selected `ProtocolMode` after the compatibility envelope has established field presence. In strict/current success paths, re-decode or validate the raw result through generated `protocol.InitializeResponse`; in approved legacy/dev override paths, do not pretend the legacy payload satisfies the current generated type.
- [ ] Verify `activeProtocolMode`.
- [ ] Send generated `initialized` notification exactly once after successful initialize.
- [ ] Add a behavioral handshake test proving `NewClient` sends `initialized` through the generated `protocol.ClientNotification` type, not a hand-coded JSON object. The test must use an injected transport or mock server that records outbound frames after successful generated `initialize`.
- [ ] No public `Raw().Initialize`.
- [ ] Implement this exact `CompatibilityPolicy` matrix:
  - `CompatibilityStrict` is the zero-value production policy. It requires the selected runtime protocol digest to be present and equal to the generated SDK digest for `ClientConfig.ProtocolMode`; missing digest, empty digest, mismatched digest, or mismatched `activeProtocolMode` returns `CompatibilityError`, closes the transport/process, and does not send `initialized`.
  - `CompatibilityAllowProtocolDigestUnavailable` permits only a reviewed legacy/dev initialize fixture that omits the digest fields and omits `activeProtocolMode`, matching the pre-Stage-1 shape from `codex-rs/app-server-protocol/src/protocol/v1.rs`: `userAgent`, `codexHome`, `platformFamily`, and `platformOs`. The fixture must be decoded and validated through the Go-internal initialize compatibility envelope, not through generated `protocol.InitializeResponse`; tests must prove generated `protocol.InitializeResponse` still rejects the same missing-field payload. It is allowed only for injected test transports or an explicit `ClientConfig.CodexPath` runtime whose `InitializeResponse.userAgent` or version identifies a dev build such as `0.0.0` or `dev`; it must never accept an implicit `PATH` runtime or any release-like runtime. When this legacy shape is accepted, the SDK assumes the requested `ClientConfig.ProtocolMode` for local generated-method gating, sets `Metadata.CompatibilityOverrideActive=true`, and sets a human-readable `CompatibilityNote` naming the missing digest/mode condition and explicit dev/test source. If `activeProtocolMode` is present, it must match the requested mode. Any non-empty mismatched digest still fails closed.
  - `CompatibilityAllowDevBuild` permits a missing or mismatched digest only for reviewed dev-runtime fixtures: the runtime `InitializeResponse.userAgent` or version must identify a dev build such as `0.0.0` or `dev`, and the runtime must come from explicit `ClientConfig.CodexPath` or injected test transport, not implicit `PATH` lookup. If `activeProtocolMode` is missing in the dev fixture, the SDK assumes the requested `ClientConfig.ProtocolMode` and records that assumption in `Metadata.CompatibilityNote`; if present, it must match. On success it sets `Metadata.CompatibilityOverrideActive=true` and a `CompatibilityNote` that includes the policy name and the observed/expected digest labels without leaking environment values.
  - No compatibility override may accept a non-empty mismatched digest from an implicit `PATH` runtime or a runtime that looks like a release build.
- [ ] Cleanup test proves no process/goroutine/waiter leak after initialize failure.
- [ ] Compatibility tests:
  - strict policy fails closed for a realistic legacy initialize fixture containing only `userAgent`, `codexHome`, `platformFamily`, and `platformOs`, decoded through the Go-internal initialize compatibility envelope and not through generated `protocol.InitializeResponse`
  - generated `protocol.InitializeResponse` rejects that same legacy fixture with a typed missing-field decode error, proving the compatibility envelope does not weaken current generated protocol decoding
  - current successful initialize payloads re-decode or validate through generated `protocol.InitializeResponse` before `initialized` is sent
  - strict policy fails closed for missing digest before `initialized`
  - strict policy fails closed for mismatched digest before `initialized`
  - `CompatibilityAllowProtocolDigestUnavailable` succeeds for the realistic legacy initialize fixture only when it comes from injected test transport or explicit dev `CodexPath`, assumes the requested protocol mode, and sets `Metadata.CompatibilityOverrideActive` plus `CompatibilityNote`
  - `CompatibilityAllowProtocolDigestUnavailable` rejects the same legacy fixture from implicit `PATH` runtime or a release-like runtime identity
  - `CompatibilityAllowProtocolDigestUnavailable` still rejects a mismatched non-empty digest
  - `CompatibilityAllowDevBuild` succeeds for a reviewed explicit dev fixture with missing digest
  - `CompatibilityAllowDevBuild` succeeds for a reviewed explicit dev fixture with mismatched digest and sets warning metadata
  - `CompatibilityAllowDevBuild` succeeds for a reviewed explicit dev fixture that omits `activeProtocolMode` and records the requested-mode assumption in metadata
  - `CompatibilityAllowDevBuild` rejects mismatched digest from implicit `PATH` runtime
  - invalid/unknown policy fails fast in Task 3.5 before initialize.

### Task 3.7: Bind Root Raw Client

- [ ] Create `sdk/go/raw.go`:

```go
package codex

import "github.com/openai/codex/sdk/go/protocol"

// Raw returns the generated typed app-server client.
func (c *Client) Raw() *protocol.RawClient {
	return c.raw
}
```

- [ ] Add an unexported `raw *protocol.RawClient` field to `Client` and initialize it only after successful `initialize`.
- [ ] Implement the protocol sender interface on `Client`. The sender must:
  - route calls through the shared JSON-RPC client
  - apply generated experimental method/field validation before write
  - apply generated retry metadata
  - preserve request id and trace behavior
  - return typed SDK errors.
- [ ] Add compile tests that call a generated raw method through the public shape:

```go
func newInitializedTestClient(t *testing.T) *Client {
	t.Helper()
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: newMockInitializedTransport(t),
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := client.Close(); err != nil {
			t.Fatal(err)
		}
	})
	return client
}

func TestRawClientPublicShape(t *testing.T) {
	client := newInitializedTestClient(t)
	_, err := client.Raw().ThreadRead(context.Background(), protocol.ThreadReadParams{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
}
```

- [ ] Implement `newMockInitializedTransport(t)` in `raw_test.go` or a shared test helper. It must return a deterministic injected transport that serves `initialize`, the generated `initialized` notification path, and the raw method response used by `TestRawClientPublicShape`.
- [ ] Add a negative compile/runtime test proving `Initialize` is not exposed on `client.Raw()` while initialize params/response types are still generated in `protocol`.

### Task 3.8: Typed Errors And Metadata

- [ ] Expand `errors.go` with typed errors:
  - `ConfigError`
  - `TransportError`
  - `FrameSizeError`
  - `RPCError`
  - `CompatibilityError`
  - `ClosedError`
  - `UnsupportedError`
  - `OverflowError`
  - `DecodeError`
- [ ] Ensure all support `errors.As`.
- [ ] Ensure errors do not include secrets, raw env dumps, tokens, cookies, or auth headers.
- [ ] Populate `Client.Metadata`.

### Task 3.9: Verify And Commit

- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./...

cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just test -p codex-cli
just test -p codex-app-server
just test -p codex-login
just test -p codex-agent-identity
just fix -p codex-cli
just fix -p codex-app-server
just fix -p codex-login
just fix -p codex-agent-identity
just fmt
```

- [ ] Commit:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
git add sdk/go codex-rs/cli codex-rs/app-server codex-rs/login codex-rs/agent-identity
git commit -m "feat(go-sdk): add jsonrpc client core"
```

## Stage Review

Fresh blind engineering review and fresh blind product/security or release/ops review are mandatory. Include `go test ./...` output, focused test names, and source anchors proving the hidden app-server flag/env hooks remain test-only, fail closed in default/release-like launches, are not exposed through public Go SDK API/docs/examples, redact secret-bearing errors, and are covered by the same `CODEX_EXEC_PATH app-server --listen stdio://` smoke path used by integration tests.
