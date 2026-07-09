# Stage 5: Full Resource Clients

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Expose every SDK-public app-server namespace through ergonomic `Client` resource fields while preserving generated raw access.

**Architecture:** The generated method-level resource matrix is the acceptance contract and is produced from schema, Rust manifest, and the reviewed Go-owned `resourceAPIMappings` input. Every SDK-public method must have a root resource wrapper or an explicit generated-only raw-protocol row with a reason, unit test, safe integration test decision, and docs/example owner.

**Tech Stack:** Go root `codex` package, generated `protocol`, generated `resource_coverage_generated.go`, mock app-server harness.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:81-129`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:456-499`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:686-710`
- `docs/superpowers/plans/2026-07-02-go-sdk-full/appendix-current-protocol-inventory.md`
- Generated `sdk/go/resource_coverage_generated.go`
- Generated `sdk/go/internal/protocodex/current_protocol_inventory.generated.md`
- Generated method manifest and Go-owned `sdk/go/internal/protocodex/resource_mapping.go` from Stage 2.
- `codex-rs/app-server/src/request_processors/*`

## Files

- Execute the substage files below; each file owns its write set, tests, commit, and fresh review gate:
  - `stage-05a-resource-foundation.md`
  - `stage-05b-threads-turns-realtime-skills-hooks.md`
  - `stage-05c-config-filesystem-command-process.md`
  - `stage-05d-mcp-plugins-marketplace-apps.md`
  - `stage-05e-account-review-model-environment-control.md`
  - `stage-05f-search-memory-feedback-platform.md`

## Tasks

Stage 5 is not one implementation unit. Do not implement resource clients as a single broad change. The execution authority is the substage file for each bundle; each substage must land as its own commit and pass fresh blind review before the next bundle starts.

### Task 5.1: Initialize Resource Fields

- [ ] `NewClient` must populate every exported resource-client field listed in the design.
- [ ] Add compile/runtime tests that every exported resource field is non-nil after successful initialization:
  - `Accounts`
  - `Threads`
  - `Turns`
  - `Realtime`
  - `Reviews`
  - `Models`
  - `Config`
  - `FileSystem`
  - `Commands`
  - `Processes`
  - `Environments`
  - `Skills`
  - `Hooks`
  - `Plugins`
  - `Marketplace`
  - `Apps`
  - `MCP`
  - `RemoteControl`
  - `CollaborationModes`
  - `ExternalAgents`
  - `FuzzyFileSearch`
  - `Memory`
  - `Feedback`
  - `WindowsSandbox`
  - `ExperimentalFeatures`
  - `PermissionProfiles`.

### Task 5.2: Enforce Method-Level Resource Matrix

- [ ] Before implementing wrappers, compare `sdk/go/internal/protocodex/current_protocol_inventory.generated.md` with `appendix-current-protocol-inventory.md`, the Stage 2 schema/manifest extraction report, and `resourceAPIMappings`. The generated inventory must enumerate every current stable and experimental schema/manifest method with manifest-derived visibility and response type plus mapping-derived resource owner, wrapper, exact public wrapper signature or strict signature convention, compile-test callsite, tests, docs, notifications, and server-handler links. Any omission from schema/manifest extraction or missing mapping row blocks Stage 5 until the manifest, generator, mapping, or appendix is corrected and fresh-reviewed.
- [ ] Add resource matrix assertions for current known SDK-public/experimental coverage required by the design:
  - `thread/realtime/start` under `Realtime`
  - `thread/settings/update` under `Threads`
  - `memory/reset` under `Memory`
  - `collaborationMode/list` under `CollaborationModes`
  - `process/spawn` under `Processes`
  - `fuzzyFileSearch` plus `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop` under `FuzzyFileSearch`.
- [ ] The matrix check must fail if a row has a placeholder wrapper, placeholder docs owner, placeholder test owner, or a not-applicable integration reason that does not name the concrete safety issue.
- [ ] Implement `sdk/go/resource_coverage_test.go`. It must iterate generated resource coverage rows and fail unless every SDK-public row has:
  - wire method
  - resource owner field
  - generated raw method name
  - root wrapper method name or explicit generated-only raw-protocol row
  - exact public wrapper signature, or one of these strict conventions:
    - thin resource method: `Method(ctx context.Context, params protocol.XParams) (protocol.YResponse, error)`
    - no-param thin resource method: `Method(ctx context.Context) (protocol.YResponse, error)`
    - high-level workflow: named option/result types documented in Stage 4 or the matrix row
    - handle method: receiver-bound method that owns/injects handle identity and accepts only operation-specific data/options plus typed result; public handle follow-up methods must not require callers to re-supply `thread_id`, `turn_id`, `process_id`, `process_handle`, `session_id`, `watch_id`, `login_id`, or similar identity fields already owned by the handle
  - compile-test callsite exercising the public signature
  - unit test name
  - safe integration test name or explicit unsafe/not-applicable reason
  - docs/example owner.
- [ ] Add a second test that reflects over root resource clients and proves every non-exception SDK-public wrapper named in the matrix exists with the expected exported method name.
- [ ] Add `sdk/go/resource_callsite_test.go` or generated compile-test files that call every SDK-public wrapper using the matrix `publicSignature`/`compileCallsite`. Reflection-only method-name checks are insufficient for Stage 5 acceptance.
- [ ] Add handle-identity tests for command, process, realtime, fuzzy search session, filesystem watch, login, thread, and turn follow-up wrappers. Each test must prove the handle injects its own identity into the generated protocol params and either omits identity from the public options type or rejects conflicting advanced params with a typed error before sending. Raw generated params that expose identity remain available only through `client.Raw()`.
- [ ] Validate that every SDK-public row has a non-empty `docsExampleOwner` value and carry those owners into Stage 6. Do not create or require `sdk/go/README.md` or example files in Stage 5; Stage 6 owns docs/example creation, existence checks, and content checks.
- [ ] Assert `internalTestOnly`, `compatibilityOnly`, `handshakeOnly`, and `excluded` entries are absent from root resource APIs and docs.

### Task 5.3: Execute Resource Bundle Substages

- [ ] Execute `stage-05a-resource-foundation.md`.
- [ ] Execute `stage-05b-threads-turns-realtime-skills-hooks.md`.
- [ ] Execute `stage-05c-config-filesystem-command-process.md`.
- [ ] Execute `stage-05d-mcp-plugins-marketplace-apps.md`.
- [ ] Execute `stage-05e-account-review-model-environment-control.md`.
- [ ] Execute `stage-05f-search-memory-feedback-platform.md`.
- [ ] Each substage must have its own commit, verification output, and fresh blind review. Do not proceed to the next substage with unresolved P0/P1/substantial P2 findings.

### Task 5.4: Verify Aggregate Resource Coverage

- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./...
```

- [ ] Confirm resource matrix coverage:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run 'TestResourceCoverage'
```

- [ ] Confirm every Stage 5 substage commit is present. Do not squash the substage commits before Stage 6 review unless a fresh blind review verifies that the resulting combined diff remains reviewable and no resource bundle coverage was lost.

## Stage Review

Fresh blind engineering and product reviews are mandatory. Product review must specifically confirm the method-level matrix proves full SDK scope, not a raw generated client with a few wrappers.
