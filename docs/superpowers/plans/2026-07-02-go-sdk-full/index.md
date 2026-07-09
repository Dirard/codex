# Go SDK Full Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` before executing any stage. Each stage also requires a fresh blind review before the next stage starts.

## Goal

Implement a best-in-class, full-featured Go SDK for Codex app-server in this repository. The SDK must provide both:

- generated typed protocol parity for the current app-server surface
- ergonomic high-level Go workflows for real users across threads, turns, realtime, reviews, config, models, environments, remote control, collaboration modes, external agents, memory, feedback, Windows sandbox, experimental features, permission profiles, MCP, apps, plugins, marketplace, login/account, and server-to-client handler workflows.

This is not an MVP and not a raw JSON-RPC wrapper.

## Authority Order

1. User goal in this thread: full Go SDK, best option, no MVP.
2. Design spec: `docs/superpowers/specs/2026-07-01-go-sdk-design.md`.
3. This index for execution order, global invariants, and acceptance gates.
4. Stage files below for concrete implementation steps.
5. `appendix-current-protocol-inventory.md` as the reviewed seed for current method/resource/server-handler/notification ownership. Generated manifests must become authoritative during execution and fail closed on drift.
6. Live Rust protocol/schema sources under `codex-rs/app-server-protocol`.

If artifacts conflict, stop and repair the plan before implementation rather than improvising in code.

## Stage Map

| Stage | File | Depends on | Purpose | Blocks next stage until |
| --- | --- | --- | --- | --- |
| 0 | `stage-00-current-state.md` | approved design | Go module skeleton, public surface placeholders, clean baseline | `go test ./...` baseline and stage review pass |
| 1 | `stage-01-rust-manifest-digests.md` | stage 0 | Rust-owned manifest, serde/routing metadata, protocol digests, initialize compatibility | Cargo and Bazel wrapper tests pass; manifest/digest drift checks pass |
| 2 | `stage-02-go-generator-protocol.md` | stages 0-1 | Go generator, generated protocol, mapping matrices, schema/manifest drift checks | generated protocol compiles and matrix coverage is exhaustive |
| 3 | `stage-03-transport-client-core.md` | stages 0-2 | JSON-RPC transport, stdio runtime startup, injected transport, initialize, compatibility, raw client | core/race tests pass and import boundaries are proven |
| 4 | `stage-04-routing-handlers-workflows.md` | stages 0-3 | notification routing, lifecycle cleanup, server request handlers, high-level thread/turn workflows | workflow, handler, opt-out, and cleanup tests pass |
| 5 | `stage-05-resource-clients.md` plus `stage-05a` through `stage-05f` | stages 0-4 | complete resource clients from generated matrix in reviewed bundles | every SDK-public matrix row has wrapper, compile callsite, tests, docs owner, per-bundle commit, and per-bundle fresh review |
| 5G | `stage-05g-package-source-hermeticity.md` | stages 0-5 | release-owned no-network helper sources for runtime package layout | `rg` and zsh package inputs are materialized without DotSlash/package-cache/network during Go SDK CI and Stage 7, or Stage 6 runtime staging is explicitly blocked |
| 6 | `stage-06-docs-ci-release.md` | stages 0-5G | README/examples, CI, real app-server integration, release-readiness workflow | docs coverage, CI snippets, real runtime tests, and release readiness pass |
| 7 | `stage-07-final-verification.md` | stages 0-6 | final full verification and handoff | all required checks pass or blocker is reported with source evidence |

## Global Invariants

- No Go-only app-server API surface. Shared compatibility fields or protocol surfaces must be intentional app-server changes.
- Rust manifest and schema/digest exports are the source of truth. Go generated output must never become an input to Rust digests.
- Schema-insufficient serde shape, request serialization scope, notification routing, terminal lifecycle cleanup, schema-excluded notifications, and deprecated compatibility requests must fail closed.
- Public high-level APIs use root SDK option/input/result types. Raw generated protocol params stay reachable through generated/raw APIs only.
- `internal/jsonrpc` must not import root package `github.com/openai/codex/sdk/go`; avoid import cycles by keeping internal dependency direction explicit.
- Real app-server tests must be hermetic: pinned `CODEX_EXEC_PATH`, isolated `CODEX_HOME` created through normal public SDK launch/config flow, release-owned package helper payloads, and mock Responses services owned by the Go test process. Release-shaped `CODEX_EXEC_PATH app-server --listen stdio://` coverage must not depend on managed-config bypass envs, plugin-startup suppression args, auth/base-url hooks, or other hidden app-server debug/test hooks; those bypasses belong only to injected transports or a distinct debug/test fixture runtime that cannot be confused with the release/default path.
- Tests must never discover `codex` from `PATH` for positive real-runtime coverage.
- Windows, macOS, and Linux remain supported unless a workflow is explicitly OS-specific and documented in the matrix.
- Do not weaken auth, secret handling, permission, or runtime compatibility checks to make tests pass.
- Stage 5 must not be implemented as one broad resource-client change. Execute `stage-05a-resource-foundation.md`, `stage-05b-threads-turns-realtime-skills-hooks.md`, `stage-05c-config-filesystem-command-process.md`, `stage-05d-mcp-plugins-marketplace-apps.md`, `stage-05e-account-review-model-environment-control.md`, and `stage-05f-search-memory-feedback-platform.md` as separate commits with fresh blind review after each bundle. Execute `stage-05g-package-source-hermeticity.md` after Stage 5 resource bundles and before Stage 6; Stage 6 cannot start until Stage 5G either proves no-network helper sources or explicitly blocks runtime staging.

## Acceptance Gates

- `sdk/go` builds and `go test ./...` passes.
- Generated stable and experimental protocol outputs are checked in and drift-checked.
- Manifest and schema digests are deterministic across Cargo/Bazel paths and exposed in initialize compatibility.
- Every current SDK-public client method has resource owner, wrapper, compile callsite, unit test, integration coverage or reviewed not-applicable reason, and docs/example owner.
- Every current server request has generated dispatch plus public/internal/compatibility visibility classification, tests, and docs/example handling.
- Every current server notification has reviewed routing/lifecycle/global/internal metadata and opt-out safety tests.
- Real app-server integration tests run in non-skip mode when `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1`.
- Release-readiness validates an external consumer through VCS/module resolution without `replace`, including v0/v1 and v2+ semantic import paths.
- Final verification uses repository-supported `just` plus local Bazel commands for developer-side checks, while CI/release-confidence gates use `.github/scripts/run-bazel-ci.sh` only inside workflows that set up its required CI environment.

## Review Loop

After every stage and after any repair that changes behavior, architecture, test strategy, security, data handling, scope, or public contract:

1. Close prior reviewers.
2. Dispatch fresh blind product/engineering/release or QA reviewers as appropriate.
3. Provide only the user goal, approved design/spec, current plan or diff, acceptance criteria, relevant files, and current check outputs.
4. Repair all P0/P1/substantial P2 findings.
5. Repeat until a fresh blind iteration returns no blocking findings.

P3 and minor P2 issues may be fixed immediately or recorded as follow-up only when they do not change behavior, architecture, security, public API contract, test validity, or product value.

## Stop Conditions

Stop and repair the plan before coding if:

- manifest/schema/source-of-truth ownership is ambiguous
- a stage asks for an impossible current app-server workflow
- public API examples do not compile as written
- CI/release checks can validate the wrong checkout or use network/ambient state instead of hermetic test state
- a stage requires an unreviewed app-server API change
- generated coverage can pass while omitting a current protocol method, notification, server request, or serde shape

## Handoff Checklist

Every implementation handoff must include:

- this index plus the relevant stage file
- the design spec
- the appendix inventory and any generated matrix artifacts already produced
- out-of-scope constraints from the design
- exact checks required for the stage
- current known environment readiness, including tool versions and missing tools
- instruction to stop and ask/repair if source anchors disagree with the plan
