# Stage 5A: Resource Foundation And Matrix Gates

> Execute this substage as its own commit and fresh blind review before any resource bundle implementation.

## Scope

- `sdk/go/resources.go`
- `sdk/go/client.go`
- `sdk/go/resource_coverage_test.go`
- `sdk/go/resource_callsite_test.go`
- generated coverage/callsite files owned by Stage 2
- `sdk/go/internal/protocodex/resource_mapping.go`

## Tasks

- [ ] Populate every exported root resource field in `Client`.
- [ ] Add non-nil initialization tests for every resource field listed in `stage-05-resource-clients.md`.
- [ ] Implement generated matrix assertions for every SDK-public row: owner, raw method, wrapper or generated-only reason, public signature, compile callsite, unit test, integration decision, docs/example owner, notifications, and server-handler links.
- [ ] Add compile-test callsites for every SDK-public wrapper signature. Reflection-only method-name checks are not sufficient.
- [ ] Fail if any row has placeholder wrapper, placeholder test owner, placeholder docs owner, missing integration decision, or an unsafe/not-applicable reason that does not name the concrete limitation.
- [ ] Fail if `internalTestOnly`, `compatibilityOnly`, `handshakeOnly`, or `excluded` entries appear in root resource APIs or docs/example owners.

## Verification

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'TestResourceCoverage|TestResourceCallsites|TestClientResourceFields'
go test ./...
```

## Review Gate

- Commit this substage separately.
- Commit scope must include `sdk/go/client.go` if `NewClient` is where exported root resource fields are initialized; do not satisfy `TestClientResourceFields` by drifting initialization into a later bundle.
- Run fresh blind engineering/product review before Stage 5B starts.
