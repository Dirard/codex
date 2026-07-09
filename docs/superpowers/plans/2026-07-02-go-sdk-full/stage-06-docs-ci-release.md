# Stage 6: Docs, Examples, CI, And Release Readiness

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Make the Go SDK discoverable, documented, reproducibly tested, and ready for future Go module releases.

**Architecture:** Docs and examples exercise the same public APIs as tests. CI builds a same-checkout app-server runtime and never relies on user auth state or arbitrary `PATH` discovery for positive integration tests.

**Tech Stack:** Go examples/tests, GitHub Actions SDK workflow, `just` commands, README.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:653-710`
- `.github/workflows/sdk.yml`
- `sdk/python/README.md` if present
- `sdk/typescript` docs/examples if present

## Files

- Create: `sdk/go/README.md`
- Create: `sdk/go/RELEASE.md`
- Create: `sdk/go/examples/run/main.go`
- Create: `sdk/go/examples/streaming/main.go`
- Create: `sdk/go/examples/login_account/main.go`
- Create: `sdk/go/examples/server_handlers/main.go`
- Create: `sdk/go/examples/resources/main.go`
- Create: `sdk/go/examples/reviews/main.go`
- Create: `sdk/go/examples/skills_hooks/main.go`
- Create: `sdk/go/examples/raw_protocol/main.go`
- Create: `sdk/go/examples/test_harness/main.go`
- Create: `sdk/go/integration_app_server_test.go`
- Create: `sdk/go/integration_auth_test.go`
- Create: `sdk/go/internal/testharness/mock_services.go`
- Create: `sdk/go/runtime_package_layout_test.go`
- Create: `sdk/go/runtime_staging_script_test.go`
- Create: `.github/scripts/stage-codex-runtime.sh`
- Create: `.github/scripts/stage-codex-runtime.ps1`
- Modify: `codex-rs/cli/BUILD.bazel` to add the `codex_go_sdk_runtime_layout` package-layout collector target consumed by the staging scripts.
- Modify: `bazel/platforms/release_binaries.bzl` only if needed to expose existing release binary outputs to the package-layout collector.
- Create: `bazel/platforms/go_sdk_runtime_layout.bzl` only if needed for the reviewed package-layout collector macro/rule used by `//codex-rs/cli:codex_go_sdk_runtime_layout`.
- Do not modify package-builder source files in Stage 6. Package-builder helper-source changes are owned by `stage-05g-package-source-hermeticity.md`; Stage 6 may read those files as source anchors only.
- Do not create or modify any other Bazel `.bzl` helper files in Stage 6. If the collector needs a different helper file, stop and split out a reviewed Bazel-helper stage before editing it.
- Modify: `.github/workflows/sdk.yml`
- Create: `.github/workflows/go-sdk-release-readiness.yml`
- Create: `.github/workflows/go-sdk-shipping-release-readiness.yml`
- Modify: `.github/workflows/rust-release.yml` to align `x86_64-apple-darwin` shipping build/package/finalize jobs with the same Intel/Rosetta runner proof required by Go SDK release-readiness.
- Modify: `.github/workflows/rust-release-windows.yml` so the published `codex-<target>.exe.zip` fails closed when `codex-command-runner.exe` or `codex-windows-sandbox-setup.exe` is missing, instead of falling back to a single-binary zip.
- Create/modify: Go drift-check scripts only if the generator command is not enough.

## Execution Split

Stage 6 depends on `stage-05g-package-source-hermeticity.md`. Do not start Stage 6 runtime packaging, staging scripts, CI, or Stage 7 final verification unless Stage 5G already proved no-network, release-owned helper payload sources for managed `rg` and zsh. A reviewed helper-root artifact contract is acceptable only when `CODEX_PACKAGE_HELPER_ROOT/<target>/codex-package-helpers.json` is present and `python3 -m codex_package.materialize_helpers --verify-only` validates the manifest, helper paths, byte sizes, and SHA-256 digests before the network-disabled staging segment begins. If Stage 5G blocked runtime staging, keep Stage 6 docs/examples work separate and do not claim real-runtime release readiness.

Stage 6 must execute as reviewed substages with separate review/commit boundaries. Do not combine docs/examples, runtime packaging, staging scripts, real-runtime tests, CI wiring, and release readiness into one implementation diff. Required split:

1. Docs/examples and coverage tests: README, examples, docs coverage, and example compile checks only.
2. Runtime package-layout source and collector: Bazel target, package-builder parity contract, helper-root manifest verification, no-network staging tests, and no workflow wiring.
3. Staging scripts: Unix/Git Bash and PowerShell scripts plus script tests, consuming only the reviewed collector output.
4. Real app-server integration tests: Go harness, child-env isolation, auth/mock services, required-test gate.
5. CI wiring: `.github/workflows/sdk.yml` only after the staging scripts and integration tests are reviewed.
6. Release readiness: release docs, release-readiness workflow, external-consumer tests, and tag-shape validation.

## Tasks

### Task 6.1: Write README

- [ ] `sdk/go/README.md` must cover:
  - install/import
  - minimum Go version, currently `go 1.25`
  - compatible Codex runtime setup
  - `ProtocolModeExperimental` zero-value default
  - `ProtocolModeStable` opt-out
  - default limits and overrides
  - non-secret config override policy
  - pre-release commit/tag pinning
  - future subdirectory tags such as `sdk/go/v1.2.3`
  - future v2+ import paths such as `github.com/openai/codex/sdk/go/v2`
  - runtime mismatch errors and remediation
  - auth
  - thread run
  - streaming
  - structured output with `TurnOptions.OutputSchema`
  - images
  - server handlers
  - resource workflows
  - MCP structured content
  - OS-specific resource behavior
  - raw typed client usage.

- [ ] README must not promise general thread/turn structured output unless app-server has a first-class output-schema/response-format field and tests.

### Task 6.2: Add Examples

- [ ] Add examples for:
  - sync-style `Run`
  - streaming
  - structured output using `TurnOptions.OutputSchema`
  - login/account
  - thread lifecycle
  - command/process streaming
  - filesystem watch
  - MCP flow
  - app listing
  - plugin flow
  - realtime
  - skills and hooks workflows
  - review start
  - config
  - models
  - environments
  - remote control
  - collaboration modes
  - external agents
  - memory
  - feedback
  - Windows sandbox unsupported handling
  - experimental features
  - permission profiles
  - server handler registration
  - raw typed method
  - test harness with injected transport or `CODEX_EXEC_PATH`.

- [ ] Examples may combine small groups in `examples/resources/main.go`, but each resource group must be explicitly exercised and named.
- [ ] Examples must satisfy the generated resource matrix `docsExampleOwner` rows from `sdk/go/resource_coverage_generated.go`; no SDK-public resource wrapper may be undocumented.

### Task 6.3: Add CI Jobs

- [ ] Before wiring workflow jobs, confirm Stage 5G is complete and add the Bazel runtime layout seed target family plus the helper-root merge contract:
  - create `//codex-rs/cli:codex_go_sdk_runtime_layout` in `codex-rs/cli/BUILD.bazel` as a filegroup over one seed target per release platform
  - each seed target must materialize only declared Bazel-owned runtime seed files: `codex-package.json` and `bin/codex[.exe]`. It must not read `CODEX_PACKAGE_HELPER_ROOT`, DotSlash manifests, package caches, network resources, or external helper payload directories as undeclared Bazel inputs.
  - the complete install-context package layout expected by `InstallContext::from_exe` is produced by `.github/scripts/stage-codex-runtime.sh` / `.ps1` after the staging script verifies the Stage 5G helper-root manifest and merges concrete helper payloads under `codex-resources/` and `codex-path/`. CI must not certify a Bazel-only synthetic layout that can diverge from the package shipped by the release workflows.
  - the seed target must include the same-checkout entrypoint and `codex-package.json`. The staging script must then add Linux `bwrap`, Windows helpers, `codex-path/<rg name>`, non-Windows `codex-resources/zsh/bin/zsh`, and any later V8/package inputs from Bazel build outputs, runfiles, checked-in files, or the Stage 5G helper-root artifact after `codex-package-helpers.json` verification. Managed `rg` and zsh must come from the release-owned helper source established by Stage 5G, not from DotSlash/package-cache during staging.
  - release-owned helper source anchors are `scripts/codex_package/rg`, `scripts/codex_package/codex-zsh`, `scripts/codex_package/ripgrep.py`, and `scripts/codex_package/zsh.py`, but Stage 6 consumes the already-reviewed Stage 5G materialized inputs rather than editing those sources. The app-server test fixture `codex-rs/app-server/tests/suite/zsh` is test-only and must not be used as the Go SDK release-shaped package source. It may be cited only as evidence that a separate test fixture exists.
  - it must not use DotSlash/package-builder/prebuilt downloads during Go SDK CI or Stage 7. If the Stage 5G no-network helper source or verified helper-root manifest is missing, stale, or incomplete, Stage 6 runtime packaging is blocked; do not add a fallback fetcher, ambient cache dependency, undeclared Bazel helper-root input, or placeholder helper directory.
  - this Bazel packaging/layout work is a reviewed first slice of Stage 6, before workflow wiring. Allowed write scope is `codex-rs/cli/BUILD.bazel`, `bazel/platforms/release_binaries.bzl`, and at most one new helper file `bazel/platforms/go_sdk_runtime_layout.bzl` containing only the package-layout collector macro/rule needed by `//codex-rs/cli:codex_go_sdk_runtime_layout`. If implementation needs any other Bazel/Starlark file, stop and create a separately reviewed Bazel-helper stage with exact file list, commit scope, package-layout tests, and no-network staging coverage. If implementation needs package-builder source-of-truth edits, return to Stage 5G instead of changing them in Stage 6.
  - verify locally on Linux/macOS with `bazel build //codex-rs/cli:codex_go_sdk_runtime_layout`; on native Windows, run the Stage 6 PowerShell bootstrap/export contract before any local Bazel command. CI/staging scripts must use the repo wrapper form `./.github/scripts/run-bazel-ci.sh --remote-download-toplevel -- build <args> -- //codex-rs/cli:codex_go_sdk_runtime_layout` for the seed target family, then merge only verified helper-root payloads.
- [ ] Modify `.github/workflows/sdk.yml`:
  - add a `go-sdk` job with `strategy.fail-fast: false`
  - run the job on release-shaped Linux/macOS/Windows runner shapes matching the shipping release pool: `${repo}-linux-x64-xl` for `x86_64-unknown-linux-musl`, `${repo}-linux-arm64` for `aarch64-unknown-linux-musl`, `macos-15-xlarge` for arm64, `macos-15-large` for x64, `${repo}-windows-x64`, and `${repo}-windows-arm64`
  - set job env `CARGO_NET_GIT_FETCH_WITH_CLI: "true"`
  - install the repo-pinned Rust toolchain before any `cargo build` or `cargo run`
  - install `just` using the repo's existing `taiki-e/install-action` pattern
  - install `cargo-nextest` using the repo's existing `taiki-e/install-action` pattern and pinned version `0.9.103` before any `just test`
  - set up Bazel with `./.github/actions/setup-bazel-ci` immediately after checkout and before `actions/setup-go`, `dtolnay/rust-toolchain`, or any Bazel command; this preserves toolchain PATH on Windows because `setup-bazel-ci` exports the Visual Studio developer shell PATH
  - install Linux native build dependencies `bubblewrap`, `pkg-config`, and `libcap-dev` on Linux runners as the system-sandbox fallback, while still accepting a valid bundled `codex-resources/bwrap` helper next to the staged runtime as equivalent sandbox readiness
  - use repo-approved pinned SHA form for `actions/setup-go` with `go-version: '1.25'` and cache dependency paths `sdk/go/go.mod` plus `sdk/go/go.sum`
  - build and stage the compatible same-checkout app-server runtime through `.github/scripts/stage-codex-runtime.sh`; the helper must produce a complete adjacent runtime layout by merging the Stage 6-owned `//codex-rs/cli:codex_go_sdk_runtime_layout` seed target with a verified Stage 5G helper-root artifact, and the final staged layout must have either shared-source or tested parity with the real package-builder release layout, not only a copied `codex` binary
  - the staging helper must build `//codex-rs/cli:codex_go_sdk_runtime_layout` with the same Bazel wrapper, `--remote-download-toplevel`, `cquery --output=files`, and stable staging path pattern already used by the checked-in SDK workflow; `cargo build` may remain a developer convenience check but must not be the release-confidence runtime artifact for Go integration tests
  - the staging helper must copy or materialize every resource that the runtime resolves from the package root recognized by `InstallContext::from_exe`, including `codex-resources/*` such as bundled `zsh` and Linux `bwrap`, `codex-path/*` such as bundled `rg`, and `codex-package.json`; if the current Bazel output only exposes a seed or Stage 5G did not create materialized helper inputs, the stage must fail until the seed plus verified helper-root manifest can produce the complete layout
  - before wiring CI, add the single supported local-only package resource source: a Bazel/runfiles runtime seed target named `//codex-rs/cli:codex_go_sdk_runtime_layout` plus the Stage 5G verified helper-root artifact. The target must materialize the entrypoint and `codex-package.json` from same-checkout build outputs, while the staging scripts must merge managed `rg`, zsh, Linux `bwrap`, Windows helpers, and any later package inputs only after `codex-package-helpers.json` verification. The Go SDK staging scripts must consume this seed-plus-manifest route and must not expose a second local-prebuilt/package-builder route for CI or Stage 7. Any future alternative staging architecture must be a separate reviewed stage before it can replace this route.
  - runtime staging for CI and Stage 7 must be network-hermetic: it may use only the `//codex-rs/cli:codex_go_sdk_runtime_layout` seed output, verified Stage 5G helper-root artifact, same-checkout build outputs, Bazel-materialized runfiles, checked-in files, or a fail-fast preinstalled `zstd` prerequisite for archive writing. It must not fetch DotSlash, V8, `rg`, zsh, `zstd`, helper archives, or package resources during Go SDK CI or final verification. The stage must add a no-network script test with downloads disabled and an empty DotSlash/package cache proving staging succeeds from the Bazel seed plus verified helper root and fails if any required layout input is missing.
  - on Windows, use an explicit release-shaped MSVC staging lane for the main Go SDK real-runtime validation: build/stage `codex.exe` and helper binaries with the requested MSVC target (`x86_64-pc-windows-msvc` or `aarch64-pc-windows-msvc`), matching the repo's current package/release artifact convention in `.github/dotslash-config.json` and Windows release workflows. A separate Bazel/gnullvm compatibility lane may remain for Bazel-specific coverage, but it must not be the only Windows real-runtime/package validation. The MSVC lane must stage `codex.exe`, `codex-windows-sandbox-setup.exe`, and `codex-command-runner.exe` under the install-context package layout for the same requested target, and Go SDK tests must prove helper discovery through the staged package root computed from `CODEX_EXEC_PATH`/`InstallContext`, not through Cargo test binary env vars.
  - the Bazel layout collector is a fast preflight and same-checkout staging path, not the only release-confidence artifact. For every supported OS in the Go SDK matrix, CI must also run a verifier bound to the real shipping package-builder archive path. Current `.github/workflows/rust-release.yml` builds package archives in both the generic build job and the `package-macos` job, and `.github/workflows/rust-release-windows.yml` builds Windows package archives, so Linux, macOS, and Windows targets must all build or consume the matching package-builder archive, using the Stage 5G materialized helper inputs, unpack that archive, derive `CODEX_EXEC_PATH` from the unpacked `bin/codex[.exe]`, and run a required SDK smoke suite against that archive runtime. macOS release readiness must also prove the shipping DMG/direct dist path through the same `package-macos`/`finalize-macos` runner shape or through a non-publishing workflow that reuses the same commands, artifacts, and notarization/stapling verification shape with real signing disabled only by an explicit reviewed fixture. The packageArchive lane remains mandatory while `package-macos` builds archives, and the DMG lane is mandatory for any claimed macOS release-ready verdict. The archive lane must use the same release profile as `.github/scripts/build-codex-package-archive.sh`; `--release-package-archive` must reject `--cargo-profile dev` unless the mode is changed to consume prebuilt release artifacts directly. The archive lane must preflight `zstd` or consume a Stage 5G no-network `zstd` input before package assembly, and must fail before `.github/scripts/build-codex-package-archive.sh` can prepend `.github/workflows` to `PATH` or invoke `.github/workflows/zstd`/DotSlash. Layout parity alone is not sufficient release-readiness evidence. The same archive outputs must be checked against `.github/dotslash-config.json` and the `publish-dotslash` job so DotSlash regex/path drift cannot pass after package archive changes.
  - macOS x64 release-readiness must execute the staged x64 runtime on an Intel runner. Use `macos-15-large` for the `x86_64-apple-darwin` Go SDK matrix leg and add a `uname -m == x86_64` self-check before staging. If the repository cannot use an Intel macOS larger runner, the stage must instead add an explicit Rosetta prerequisite plus `arch -x86_64` self-check and run all x64 staging/smoke commands under `arch -x86_64`; otherwise `x86_64-apple-darwin` must be marked not release-ready.
  - align the shipping `.github/workflows/rust-release.yml` macOS x64 path with the same proof. Build, sign, `package-macos`, and finalize/notarize jobs for `x86_64-apple-darwin` must use `macos-15-large` or an explicit Rosetta `arch -x86_64` execution path for the commands that build/sign/package the x64 binary and package archive. The existing `macos-15-xlarge` arm64 runner may remain for `aarch64-apple-darwin`, but no `x86_64-apple-darwin` release row in `build`, `sign-macos-binaries`, `package-macos`, or later macOS finalize/notarize jobs may rely on `macos-15-xlarge` alone. Add a source-parsing release workflow test such as `TestMacosReleaseWorkflowRunnerShape` that fails on any x64 macOS release row attached to `macos-15-xlarge` without an explicit `arch -x86_64` command path.
  - export absolute `CODEX_EXEC_PATH`
  - on Windows, export native Windows-form `CODEX_EXEC_PATH` before `go test`; do not export helper binary env paths, because helpers must be resolved from `codex-resources/` under the staged package root
  - do not globally export child-runtime control env (`CODEX_HOME`, `CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG`, `CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE`, or auth override envs) in CI. Real-runtime tests must create isolated `CODEX_HOME` values through normal public SDK launch config, scrub or reject app-server debug/test hook envs before spawn, and prove release/default `codex app-server` cannot be steered by those hook envs so missing SDK threading cannot be masked by ambient parent process state.
  - run `go test ./...` from `sdk/go`
  - run `go test -race ./...` on the Linux matrix leg
  - run Rust protocol/schema source-of-truth checks before Go drift checks
  - run Go generator drift check
  - run schema-experimental full tree drift check
  - run manifest drift check
  - run resource coverage and docs/example coverage tests
  - add a secretless `go-sdk-release-readiness` job that calls `.github/workflows/go-sdk-release-readiness.yml` with explicit `checkout_ref` and synthetic release tag validation enabled. Blocking PR CI must validate commit-SHA external-consumer behavior and current `v0`/`v1` tag/import-path behavior against the reviewed head checkout before any real tag is created. Synthetic `v2+` validation is a separate future-major policy smoke test against a throwaway rewritten commit, not evidence that the reviewed head checkout is already a valid `v2` module. Do not pass `secrets: inherit` unless a concrete secret consumer is added with a reviewed justification and least-privilege permissions.
  - keep Python and TypeScript SDK jobs.

- [ ] Positive integration tests must not discover arbitrary `codex` from `PATH`.
- [ ] PATH lookup tests must use controlled fake/missing/stale binaries.
- [ ] Create `.github/scripts/stage-codex-runtime.sh` before wiring the Go SDK workflow. Contract:
  - accepts `--out <dir>`, optional `--bazel-target <triple>`, optional `--cargo-profile <profile>` defaulting to the real workspace profile `dev`, `--windows-release-shaped-msvc` for the required Windows release-shaped staging lane, `--windows-msvc-host-platform` for the required Windows wrapper host-platform override, `--release-package-archive` for building and unpacking the current package-builder archive path instead of the Bazel collector, optional `--build-metadata-job <name>`, `--github-env <file>` for GitHub Actions, `--print-shell-env` for Unix/Git Bash local Stage 7 verification, and `--verify-sandbox --exec-path <path>` for verifying an already staged runtime with a bounded Linux `codex-resources/bwrap` smoke instead of a file-presence-only check. `debug` must not be accepted as a Cargo profile unless the script implements and tests it as an explicit internal alias to a real workspace profile.
  - in CI, invokes `./.github/scripts/run-bazel-ci.sh --remote-download-toplevel -- build <args> -- //codex-rs/cli:codex_go_sdk_runtime_layout`, matching the current `.github/workflows/sdk.yml` wrapper shape where wrapper args, Bazel command/args, and Bazel targets are separated by two `--` delimiters; every Windows release-shaped staging invocation must pass the wrapper flag `--windows-msvc-host-platform` or the equivalent Bazel arg `--host_platform=//:local_windows_msvc` before the target delimiter. Locally on Unix, uses `bazel build //codex-rs/cli:codex_go_sdk_runtime_layout` unless an explicit CI wrapper environment is present; locally on Windows, performs the same bootstrap obligations as `.github/actions/setup-bazel-ci` before Bazel use: short `BAZEL_OUTPUT_USER_ROOT`, `BAZEL_REPO_CONTENTS_CACHE`, Visual Studio/MSVC environment materialization through `VsDevCmd.bat`, computed Bazel Windows PATH, and `git config --global core.longpaths true`
  - resolves the exact `//codex-rs/cli:codex_go_sdk_runtime_layout` tree output with the same wrapper/local Bazel path used for the build and never guesses from convenience symlinks
  - stages an install-context-compatible package layout under `--out`, with package root `--out`, metadata file `--out/codex-package.json`, executable at `--out/bin/codex` or `--out/bin/codex.exe`, resources under `--out/codex-resources/`, and managed PATH helpers under `--out/codex-path/`; `CODEX_EXEC_PATH` must point to `--out/bin/codex[.exe]`. Default staging must copy the Bazel seed output of `//codex-rs/cli:codex_go_sdk_runtime_layout` and merge only verified Stage 5G helper-root payloads; it must not use an ad hoc flat directory and not a package-builder/prebuilt fallback. The `--release-package-archive` mode must build or consume the current release package-builder archive for the same Linux/macOS/Windows target, unpack it into the same install-context layout, require `--cargo-profile release` when it builds the archive, reject `dev`, and emit metadata that identifies the source as `packageArchive` and `cargoProfile` as `release`.
  - never downloads runtime/package resources during CI or Stage 7 verification. The helper must fail if the Bazel seed target is unavailable, incomplete, not parity-checked against the package-builder layout, the helper-root manifest is missing/stale, or any required input resolves through DotSlash/network/package-cache lookup. Add script tests with network disabled and an empty DotSlash/package cache so accidental live fetches fail before CI; these tests must also prove concrete managed `rg` and non-Windows `zsh` payloads are present from explicit release-owned verified helper-root inputs or that staging is blocked by a prerequisite packaging-source gap before release-shaped staging is accepted.
  - on Linux, separately asserts release-shaped package completeness and runtime sandbox readiness. Package completeness always requires executable bundled `--out/codex-resources/bwrap` in the staged layout, even when system `bwrap` is available. Runtime readiness then uses the same capability semantics as `codex-rs/linux-sandbox/src/launcher.rs`: a system `bwrap` must be executable and its `--help` output must advertise `--perms`, with `--argv0` support recorded for the runtime compatibility path; otherwise the executable bundled helper is used. The helper must also provide `--verify-sandbox --exec-path <path>` so CI and Stage 7 can derive the package root from `CODEX_EXEC_PATH` and re-run both the bundled-helper layout assertion and the runtime readiness probe, or perform an equivalent bounded command/process sandbox smoke through the staged runtime.
  - on Windows, `--windows-release-shaped-msvc` must stage the full package for the requested `--bazel-target`, supporting both `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` for `codex.exe` and helper binaries. A separate compatibility invocation may accept `--bazel-target x86_64-pc-windows-gnullvm`, but that gnullvm path is not sufficient for release-shaped Go SDK CI. The required MSVC lane must stage helpers in `--out/codex-resources/` exactly where the runtime expects them, emit native Windows-form `CODEX_EXEC_PATH` pointing at `--out/bin/codex.exe`, and verify the helper paths by deriving the package root from `CODEX_EXEC_PATH`. It must not emit or depend on Cargo test binary env vars for Go SDK runtime checks.
  - update the shipping Windows release workflow so the published main `codex-<target>.exe.zip` includes `codex-command-runner.exe` and `codex-windows-sandbox-setup.exe` for both Windows MSVC targets, and fails the release job if either helper is absent. Warning-only fallback to a single-binary zip is not release-ready. Add a release-workflow assertion such as `TestWindowsReleaseZipIncludesSandboxHelpers` that rejects the old fallback text, verifies helper files are copied into the bundle zip with their runtime names, and covers both `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`.
  - add a DotSlash release-output parity assertion such as `TestDotSlashReleaseArchiveConfigParity` that verifies `.github/dotslash-config.json` still matches every claimed Linux/macOS/Windows `codex`, `codex-app-server`, `codex-responses-api-proxy`, Linux `bwrap`, Windows `codex-command-runner`, and Windows `codex-windows-sandbox-setup` release artifact filename and configured path, and verifies the shipping `publish-dotslash` job still publishes from that config. The gate must fail on helper-output regex/path drift too; package-archive parity alone is not enough.
  - update the shipping release checksum/provenance path so the public checksum manifest covers the `.tar.zst` package archives consumed by DotSlash/packageArchive validation, not only `.tar.gz`. The metadata for each target must record the checksum algorithm, checksum value, public manifest name/path, and whether provenance came from the real release job or the non-publishing shipping-readiness wrapper. A wrapper-local checksum artifact is useful diagnostic evidence, but it is not sufficient release-readiness proof unless the same `.tar.zst` checksum is present in the public release checksum manifest. If a `.tar.zst` checksum exception is ever intentionally accepted, it must be explicit in `shipping-release-readiness.json` with a reviewed rationale and Stage 7 must treat the target as not release-ready unless that exception is approved in the plan.
  - add the non-publishing shipping release-readiness workflow `.github/workflows/go-sdk-shipping-release-readiness.yml`. This exact workflow is required because Stage 7 validates its file shape, run ID, and downloaded metadata artifact; a real `rust-release`/`rust-release-windows` tag run may be additional evidence but is not an alternate completion path for this plan. The workflow must be a thin wrapper around the shipping release implementation, not a bespoke parallel release. Preferred shape is reusable `workflow_call` jobs or checked-in shared scripts used by `.github/workflows/rust-release.yml` and `.github/workflows/rust-release-windows.yml`; if GitHub Actions prevents direct job reuse, first extract the exact package archive, `package-macos`, `finalize-macos`, Windows zip-helper bundling, checksum/provenance manifest generation, and `publish-dotslash` logic into shared scripts and call those scripts from both workflows. Any fixture substitution, such as disabled signing/notarization credentials, must be listed in an explicit `fixtureSubstitutions` metadata array with `name`, `replaces`, `reason`, `reviewEvidence`, and `releaseReadinessImpact`; a claimed lane must fail if it is served by workflow-local duplicate commands instead of reused release logic. The workflow must exercise the same release-shaped lanes needed for Go SDK confidence without publishing: Linux package archive assembly, `package-macos` archive plus DMG/direct artifact build/finalize shape for both macOS targets, Windows package archive assembly plus published zip helper bundling for both MSVC targets, public checksum/provenance manifest generation for `.tar.zst` package archives, and DotSlash config consumption. It must upload a bounded artifact named `shipping-release-readiness-metadata` containing `shipping-release-readiness.json` plus bounded logs, `workflowReuseProofPath`, `duplicateCommandAuditPath`, downloaded archive member inventory files, downloaded checksum manifest copies, Windows published zip inventory files, and a DotSlash parity report that Stage 7 can inspect. The workflow reuse proof file must identify the real release workflow files, reused jobs/scripts, and shared-script checksums, and the duplicate-command audit must contain an explicit `workflowLocalDuplicateCommands=false` or equivalent no-duplicates marker. The metadata must include `boundedLogs` entries with each log path, `maxBytes`, redaction policy, and retention policy so Stage 7 can prove the files are present and bounded. That metadata must include reused workflow files, reused job/script names, target-specific successful `jobName` or `packageArchiveJob` plus `jobConclusion: "success"` for every claimed target with no skipped or `notReleaseReady` target lane, target-specific `codex-package-*.tar.zst` and `codex-app-server-package-*.tar.zst` archive filenames, in-archive executable paths for both packages, runtime helper paths inside the `codex-package` archive (`codex-path/rg[.exe]`, non-Windows `codex-resources/zsh/bin/zsh`, Linux `codex-resources/bwrap`, Windows `codex-resources/codex-command-runner.exe`, and Windows `codex-resources/codex-windows-sandbox-setup.exe`), `archiveInventoryPath` for the target `codex-package` archive, `appServerPackageArchive.archiveInventoryPath`, public `.tar.zst` checksum manifest records plus downloaded `manifestPath` files for both packages, DMG/direct artifact names, Windows zip contents plus `publishedZipInventoryPath`, runner labels, target triples, architecture proof for macOS x64, successful `packageMacosJob`, `finalizeMacosJob`, and `dotslash.publishDotslashJob` names, DotSlash config parity results plus `archiveParityReportPath` for all published entries, and the full required packageArchive SDK smoke test list that ran: `TestRealAppServerInitializeStrictDigest`, `TestRealAppServerRejectsDebugHookEnv`, `TestRealAppServerThreadRunHappyPath`, `TestRealAppServerCommandExecStreaming`, `TestRealAppServerProcessLifecycle`, and `TestRealAppServerFilesystemWatch`.
  - `--print-shell-env` must print POSIX-shell-escaped `export KEY=VALUE` lines safe for `eval` in bash/Git Bash, including native Windows paths containing backslashes, colons, and spaces. Add a script test that stages a path with spaces and verifies `eval "$(.github/scripts/stage-codex-runtime.sh --print-shell-env ...)"` preserves `CODEX_EXEC_PATH`, `CODEX_HOME`, and package-root paths without introducing helper binary env vars.
  - before `--release-package-archive` enters package assembly, the shell and PowerShell entrypoints must prove archive tooling is hermetic using the exact Stage 5G contract. If Stage 5G chose a fail-fast prerequisite and no explicit zstd source argument is provided, require `command -v zstd` or `Get-Command zstd` and exit with a clear error when absent. If Stage 5G chose a checked-in/no-network zstd input, accept only an explicit `--zstd-source <path>` or `-ZstdSource <path>` pointing at that materialized executable and do not require or read `zstd` from `PATH`. In either case, script tests must fail if the mode reaches `.github/workflows/zstd`, DotSlash, or the PATH-prepending fallback in `.github/scripts/build-codex-package-archive.sh`.
  - create `.github/scripts/stage-codex-runtime.ps1` as the supported Windows-local entrypoint for Stage 7. It must share the same staging/package validation behavior as the shell entrypoint, expose `-BazelTarget` as the PowerShell equivalent of `--bazel-target`, expose `-WindowsReleaseShapedMsvc` as the PowerShell equivalent of `--windows-release-shaped-msvc` plus the required Windows MSVC host-platform override, expose `-ReleasePackageArchive` as the PowerShell equivalent of `--release-package-archive`, and expose `-ZstdSource` as the PowerShell equivalent of `--zstd-source`. `-ExportEnvironment` is the single supported handoff contract for later native PowerShell commands: it must recompute/validate the setup-bazel-ci-equivalent Windows bootstrap inputs, including short `BAZEL_OUTPUT_USER_ROOT`, `BAZEL_REPO_CONTENTS_CACHE`, Visual Studio/MSVC environment materialization through `VsDevCmd.bat`, computed Bazel Windows PATH, and `git config --global core.longpaths true`, then print only PowerShell-safe `Set-Item -Path Env:KEY -Value '...'` assignments for the full Bazel/MSVC/staged-runtime environment. It must not rely on inherited state from an earlier script process. `-BootstrapOnly` may exist only as a side-effect/preflight mode with no env printing and no runtime staging, and Stage 7 acceptance must still work with a single `& .github\scripts\stage-codex-runtime.ps1 ... -ExportEnvironment | Invoke-Expression` invocation. If a Git Bash follow-on check is supported on native Windows, it must either evaluate `.github/scripts/stage-codex-runtime.sh --print-shell-env` inside that same Git Bash session or launch Git Bash from the PowerShell process after `-ExportEnvironment | Invoke-Expression`; no check may depend on ambient bootstrap state from a previous process. The script must not require dot-sourcing. Include a parity test proving both entrypoints produce the same package layout for the same target/profile and a PowerShell export test proving paths with spaces round-trip through `Invoke-Expression`.
  - emit `--out/codex-go-sdk-runtime-staging.json` with bounded non-secret staging metadata including `layoutTarget`, `bazelTarget`, `cargoProfile`, `runtimeSource` (`bazelLayout` or `packageArchive`), `zstdSource` (`preinstalled` or `stage5gMaterialized`) for archive mode, `archiveFormats` containing `tar.zst` for archive mode, `windowsReleaseShapedMsvc`, and `windowsMsvcHostPlatform`. Stage 7 and script tests must assert that Windows release-shaped runs record `bazelTarget` equal to the requested MSVC target, `windowsReleaseShapedMsvc: true`, and `windowsMsvcHostPlatform: true`, so final verification cannot accidentally validate an ambient/default compatibility target.
  - emits no secret values, returns non-zero if any required layout/resource/helper is missing, and is the only script family used by Go SDK CI and Stage 7 for real-runtime staging.
- [ ] Add the Go SDK job in this concrete shape, preserving the repository's pinned-action policy when replacing the example action tags with approved SHAs:

```yaml
  go-sdk:
    strategy:
      fail-fast: false
      matrix:
        include:
          - name: linux-musl
            bazel_target: x86_64-unknown-linux-musl
            runs_on:
              group: ${{ github.event.repository.name }}-runners
              labels: ${{ github.event.repository.name }}-linux-x64-xl
          - name: linux-arm64-musl
            bazel_target: aarch64-unknown-linux-musl
            runs_on:
              group: ${{ github.event.repository.name }}-runners
              labels: ${{ github.event.repository.name }}-linux-arm64
          - name: macos-arm64
            bazel_target: aarch64-apple-darwin
            runs_on: macos-15-xlarge
          - name: macos-x64
            bazel_target: x86_64-apple-darwin
            runs_on: macos-15-large
          - name: windows
            bazel_target: x86_64-pc-windows-msvc
            release_shape: true
            runs_on:
              group: ${{ github.event.repository.name }}-runners
              labels: ${{ github.event.repository.name }}-windows-x64
          - name: windows-arm64
            bazel_target: aarch64-pc-windows-msvc
            release_shape: true
            runs_on:
              group: ${{ github.event.repository.name }}-runners
              labels: ${{ github.event.repository.name }}-windows-arm64
    runs-on: ${{ matrix.runs_on }}
    name: Go SDK on ${{ matrix.name }} (${{ matrix.bazel_target }})
    environment:
      name: bazel
      deployment: false
    env:
      CARGO_NET_GIT_FETCH_WITH_CLI: "true"
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
        with:
          ref: ${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}
          persist-credentials: false
      - name: Set up Bazel CI
        id: setup_bazel
        uses: ./.github/actions/setup-bazel-ci
        with:
          target: ${{ matrix.bazel_target }}
      - uses: dtolnay/rust-toolchain@e081816240890017053eacbb1bdf337761dc5582 # 1.95.0
      - uses: taiki-e/install-action@44c6d64aa62cd779e873306675c7a58e86d6d532 # v2.62.49
        with:
          tool: just
      - uses: taiki-e/install-action@44c6d64aa62cd779e873306675c7a58e86d6d532 # v2.62.49
        with:
          tool: nextest
          version: 0.9.103
      - name: Install Linux build dependencies
        if: runner.os == 'Linux'
        shell: bash
        run: |
          set -euo pipefail
          sudo apt-get update -y
          sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends bubblewrap pkg-config libcap-dev
      - uses: actions/setup-go@924ae3a1cded613372ab5595356fb5720e22ba16 # v6
        with:
          go-version: '1.25'
          cache-dependency-path: |
            sdk/go/go.mod
            sdk/go/go.sum
      - name: Verify macOS x64 runner
        if: matrix.bazel_target == 'x86_64-apple-darwin'
        shell: bash
        run: |
          set -euo pipefail
          test "$(uname -m)" = "x86_64"
      - name: Build and stage same-checkout codex runtime
        env:
          BUILDBUDDY_API_KEY: ${{ secrets.BUILDBUDDY_API_KEY }}
        shell: bash
        run: |
          set -euo pipefail
          release_shape_args=()
          if [[ "${{ matrix.release_shape || false }}" == "true" ]]; then
            release_shape_args+=(--windows-release-shaped-msvc --windows-msvc-host-platform)
          fi
          ./.github/scripts/stage-codex-runtime.sh \
            --out "${RUNNER_TEMP}/codex-go-sdk-runtime" \
            --bazel-target "${{ matrix.bazel_target }}" \
            "${release_shape_args[@]}" \
            --cargo-profile dev \
            --build-metadata-job go-sdk \
            --github-env "${GITHUB_ENV}"
      - name: Test Go SDK
        shell: bash
        run: |
          set -euo pipefail
          if [[ "${RUNNER_OS}" == "Linux" ]]; then
            ./.github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "${CODEX_EXEC_PATH}"
          fi
          export CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1
          cd sdk/go
          required_tests=(
            TestRealAppServerInitializeStrictDigest
            TestRealAppServerRejectsDebugHookEnv
            TestRealAppServerDigestMismatch
            TestRealAppServerCompatibilityOverridePolicy
            TestRealAppServerThreadRunHappyPath
            TestRealAppServerConfigReadWrite
            TestRealAppServerFilesystemWatch
            TestRealAppServerCommandExecStreaming
            TestRealAppServerProcessLifecycle
            TestRealAppServerSafeResourceWorkflows
            TestRealAppServerRemoteControlWorkflow
            TestRealAppServerModelList
            TestRealAppServerProtocolModeExperimentalGate
            TestRealAppServerUnauthenticatedAccountRead
          )
          for test_name in "${required_tests[@]}"; do
            go test ./... -list "^${test_name}$" | grep -Fx "${test_name}"
            go test ./... -run "^${test_name}$"
          done
          go test ./...
      - name: Race-test Go SDK
        if: runner.os == 'Linux'
        shell: bash
        run: |
          set -euo pipefail
          export CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1
          if [[ "${RUNNER_OS}" == "Linux" ]]; then
            ./.github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "${CODEX_EXEC_PATH}"
          fi
          cd sdk/go
          go test -race ./...
      - name: Test Go SDK release archive runtime
        shell: bash
        run: |
          set -euo pipefail
          zstd_args=()
          if [[ -n "${CODEX_GO_SDK_ZSTD_SOURCE:-}" ]]; then
            test -x "${CODEX_GO_SDK_ZSTD_SOURCE}" || { echo "CODEX_GO_SDK_ZSTD_SOURCE must point at the Stage 5G materialized zstd executable" >&2; exit 1; }
            zstd_args+=(--zstd-source "${CODEX_GO_SDK_ZSTD_SOURCE}")
          else
            command -v zstd >/dev/null 2>&1 || { echo "zstd is required for Go SDK release-archive validation unless CODEX_GO_SDK_ZSTD_SOURCE points at the Stage 5G no-network source; DotSlash fallback is not accepted" >&2; exit 1; }
          fi
          release_shape_args=()
          if [[ "${{ matrix.release_shape || false }}" == "true" ]]; then
            release_shape_args+=(--windows-release-shaped-msvc --windows-msvc-host-platform)
          fi
          archive_runtime_dir="${RUNNER_TEMP}/codex-go-sdk-release-archive-runtime"
          eval "$(.github/scripts/stage-codex-runtime.sh \
            --out "${archive_runtime_dir}" \
            --bazel-target "${{ matrix.bazel_target }}" \
            "${release_shape_args[@]}" \
            "${zstd_args[@]}" \
            --cargo-profile release \
            --release-package-archive \
            --build-metadata-job go-sdk-release-archive \
            --print-shell-env)"
          test -n "${CODEX_EXEC_PATH}"
          case "${CODEX_EXEC_PATH}" in
            "${archive_runtime_dir}"/bin/codex|"${archive_runtime_dir}"/bin/codex.exe|*"\\codex-go-sdk-release-archive-runtime\\bin\\codex.exe") ;;
            *) echo "archive CODEX_EXEC_PATH must point inside ${archive_runtime_dir}/bin: ${CODEX_EXEC_PATH}" >&2; exit 1 ;;
          esac
          json_python=python3
          command -v "${json_python}" >/dev/null 2>&1 || json_python=python
          "${json_python}" - "${RUNNER_TEMP}/codex-go-sdk-release-archive-runtime/codex-go-sdk-runtime-staging.json" "${{ matrix.bazel_target }}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    metadata = json.load(fh)
expected_bazel_target = sys.argv[2]
if metadata.get("runtimeSource") != "packageArchive":
    raise SystemExit("archive lane did not stage packageArchive runtime")
if metadata.get("cargoProfile") != "release":
    raise SystemExit("archive lane did not record release cargoProfile")
if metadata.get("bazelTarget") != expected_bazel_target:
    raise SystemExit(f"archive lane staged {metadata.get('bazelTarget')} instead of {expected_bazel_target}")
if metadata.get("zstdSource") not in {"preinstalled", "stage5gMaterialized"}:
    raise SystemExit("archive lane did not record the hermetic zstd source contract")
if "tar.zst" not in metadata.get("archiveFormats", []):
    raise SystemExit("archive lane did not record tar.zst archive validation")
if expected_bazel_target.endswith("-pc-windows-msvc"):
    if metadata.get("windowsReleaseShapedMsvc") is not True:
        raise SystemExit("archive Windows runtime was not marked release-shaped MSVC")
    if metadata.get("windowsMsvcHostPlatform") is not True:
        raise SystemExit("archive Windows runtime did not use the MSVC host-platform override")
PY
          test -d "${archive_runtime_dir}/codex-resources"
          test -d "${archive_runtime_dir}/codex-path"
          case "${RUNNER_OS}" in
            Windows)
              test -x "${archive_runtime_dir}/codex-path/rg.exe"
              test -x "${archive_runtime_dir}/codex-resources/codex-windows-sandbox-setup.exe"
              test -x "${archive_runtime_dir}/codex-resources/codex-command-runner.exe"
              ;;
            *)
              test -x "${archive_runtime_dir}/codex-path/rg"
              test -x "${archive_runtime_dir}/codex-resources/zsh/bin/zsh"
              if [[ "${RUNNER_OS}" == "Linux" ]]; then
                test -x "${archive_runtime_dir}/codex-resources/bwrap"
                ./.github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "${CODEX_EXEC_PATH}"
              fi
              ;;
          esac
          unset CODEX_HOME CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS
          export CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1
          cd sdk/go
          archive_tests=(
            TestRealAppServerInitializeStrictDigest
            TestRealAppServerRejectsDebugHookEnv
            TestRealAppServerThreadRunHappyPath
            TestRealAppServerCommandExecStreaming
            TestRealAppServerProcessLifecycle
            TestRealAppServerFilesystemWatch
          )
          for test_name in "${archive_tests[@]}"; do
            go test ./... -list "^${test_name}$" | grep -Fx "${test_name}"
            go test ./... -run "^${test_name}$"
          done
      - name: Rust protocol/schema source-of-truth checks
        env:
          BUILDBUDDY_API_KEY: ${{ secrets.BUILDBUDDY_API_KEY }}
        shell: bash
        run: |
          set -euo pipefail
          cd codex-rs
          just write-app-server-schema --check
          cd ..
          cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
          cd codex-rs
          just test -p codex-app-server-protocol
          cd ..
          ./.github/scripts/run-bazel-ci.sh \
            -- \
            test \
            --build_metadata=COMMIT_SHA=${GITHUB_SHA} \
            --build_metadata=TAG_job=go-sdk \
            -- \
            //codex-rs/app-server-protocol:app-server-protocol
          cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --check --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
          cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
      - name: Go SDK drift checks
        shell: bash
        run: |
          set -euo pipefail
          cd sdk/go
          go run ./internal/cmd/protocodex --check --mode stable --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json
          go run ./internal/cmd/protocodex --check --mode experimental --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json
          go run ./internal/cmd/protocodex --check --mode both --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
          go test ./... -run 'TestResourceCoverage|TestResourceDocsCoverage|TestServerHandlerDocsCoverage'
          go test ./... -run TestReleaseReadiness
      - name: Check for a clean worktree
        if: always() && !cancelled()
        uses: ./.github/actions/check-clean-worktree

  go-sdk-release-readiness:
    name: Go SDK release readiness
    uses: ./.github/workflows/go-sdk-release-readiness.yml
    with:
      checkout_ref: ${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}
      validate_synthetic_tags: true
```

- [ ] CI must run the Rust protocol/schema source-of-truth gate before Go drift checks. These commands depend on Stage 1 having added `write_go_sdk_manifest` and `write_schema_fixtures --check`:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just write-app-server-schema --check
cd ..
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
cd codex-rs
just test -p codex-app-server-protocol
cd ..
./.github/scripts/run-bazel-ci.sh \
  -- \
  test \
  --build_metadata=COMMIT_SHA=${GITHUB_SHA} \
  --build_metadata=TAG_job=go-sdk \
  -- \
  //codex-rs/app-server-protocol:app-server-protocol
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --check --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
```

Expected: Cargo and Bazel both run the app-server-protocol manifest/digest tests. The job must install `nextest` before `just test`, must run `setup-bazel-ci` immediately after checkout and before Go/Rust toolchain setup, and must run Bazel through the Bazel wrapper. A missing Bazel target, missing Bazel data/runfiles entry, missing nextest binary, PATH/toolchain loss after Windows `setup-bazel-ci`, or digest mismatch under Bazel blocks the CI gate.

- [ ] CI drift commands must use the checked-in artifact paths:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cd sdk/go
go run ./internal/cmd/protocodex --check --mode stable --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json
go run ./internal/cmd/protocodex --check --mode experimental --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json
go run ./internal/cmd/protocodex --check --mode both --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
go test ./... -run 'TestResourceCoverage|TestResourceDocsCoverage|TestServerHandlerDocsCoverage'
go test ./... -run TestReleaseReadiness
```

- [ ] Create `TestResourceDocsCoverage` in Stage 6, after README/examples exist. It must iterate generated matrix rows and fail unless every SDK-public `docsExampleOwner` points to an existing README section or example file and that target contains a reviewed coverage marker for the row. The marker must include the resource group name plus either the wrapper name, the generated matrix compile callsite, or an explicit `codex-go-sdk-docs:<wire-method>` marker adjacent to a real usage snippet. Existence of `examples/resources/main.go` alone must not satisfy coverage for app/plugin/MCP/remote-control/etc. It must also verify examples compile under `go test ./...` and the structured-output example exercises `TurnOptions.OutputSchema`.
- [ ] Create `TestServerHandlerDocsCoverage` in Stage 6. It must iterate generated `ServerRequestMetadata` rows and fail unless every SDK-public handler has a `docsExampleOwner` pointing to an existing README section or example file and that target contains the handler capability name plus either the generated handler registration callsite or an explicit `codex-go-sdk-handler-docs:<wire-method>` marker adjacent to a real handler snippet. Compatibility-only server requests must instead have an internal exception/review note and tests proving they are absent from public README sections, public examples, and first-class handler registration. Public coverage must include ChatGPT token refresh, permissions, dynamic tool calls, user input, MCP elicitation, attestation, and any current-time/current equivalent that appears in the manifest; deprecated compatibility approvals must be covered only by internal manifest/dispatch tests.

### Task 6.4: Add Real App-Server Integration Tests

- [ ] Create `sdk/go/internal/testharness/mock_services.go` with in-process `httptest.Server` helpers for Responses and auth. Integration tests must pass mock endpoint URLs through typed `ClientConfig`/test harness fields, not ambient global env, and must close servers in cleanup.
- [ ] Route real app-server Responses traffic to the mock Responses server only through an isolated temporary `CODEX_HOME/config.toml`, following the existing Rust test pattern in `codex-rs/app-server/tests/common/config.rs`: write `model_providers.<id>.base_url = "<mock>/v1"`, `wire_api = "responses"`, `request_max_retries = 0`, `stream_max_retries = 0`, `supports_websockets = false`, and `openai_base_url = "<mock>/v1"` when the selected provider is `openai`. Do not add a new Responses base-url env hook in Stage 6. If the existing config keys are insufficient, stop and repair Stage 3 with an exact hook name, gate semantics, and Rust tests before continuing.
- [ ] Create `sdk/go/integration_app_server_test.go`. Tests must skip with a clear message when `CODEX_EXEC_PATH` is unset, and must never discover `codex` from `PATH` for positive integration coverage.
- [ ] Add `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1` behavior to the integration test harness: when set, missing `CODEX_EXEC_PATH`, missing mock endpoints, or skipped real-runtime integration tests must fail the test run instead of skipping.
- [ ] Add a single helper, for example `newRealRuntimeClientConfig(t, services)`, that reads `CODEX_EXEC_PATH`, requires it when `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1`, sets `ClientConfig.CodexPath` to that exact path, and fails if the positive integration path would fall back to `PATH`.
- [ ] Each test must create an isolated temporary `CODEX_HOME` and pass it through `ClientConfig.Env` or the process environment configured for the owned runtime.
- [ ] Real app-server integration configs must not depend on release-shipped startup/auth/config test hooks. The helper must start from a clean parent env, pass only normal public SDK launch config plus isolated `CODEX_HOME`, and prove public `ClientConfig.Env` rejects known app-server debug/test hook env names before spawn. If a workflow is not deterministic without a test hook, cover it through injected transport or the separate debug/test fixture in `integration_auth_test.go`, not through release-readiness.
- [ ] Add a helper-level test proving positive real-runtime tests cannot construct a runtime client without `CodexPath` and isolated `CODEX_HOME`, and cannot accidentally pass reserved hook envs such as `CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG`, `CODEX_APP_SERVER_LOGIN_ISSUER`, `CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS`, or `CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE`.
- [ ] Add real stdio app-server tests with these exact function names:
  - `TestRealAppServerInitializeStrictDigest`: `initialize` strict digest success against same-checkout runtime
  - `TestRealAppServerRejectsDebugHookEnv`: release-shaped same-checkout runtime smoke proving known debug/test hook envs and hidden startup args cannot redirect auth, bypass managed config, or skip plugin startup tasks on the default `codex app-server --listen stdio://` path
  - `TestRealAppServerDigestMismatch`: digest mismatch/missing digest failure using a controlled fake runtime
  - `TestRealAppServerCompatibilityOverridePolicy`: controlled fake/dev runtime coverage for the exact Stage 3 compatibility matrix, including realistic legacy initialize fixture handling with no digest/mode fields, strict missing-digest failure, strict mismatch failure, unavailable-digest override metadata, dev-build mismatch override metadata, release-runtime mismatch rejection, and unknown policy fail-fast behavior
  - `TestRealAppServerThreadRunHappyPath`: mandatory `Thread.Run`/`turn/start` happy path against the same-checkout runtime using in-process mocked Responses; final acceptance is blocked if this test skips or falls back to a fake transport
  - `TestRealAppServerConfigReadWrite`: matrix-backed `config/read`, `config/value/write`, `config/batchWrite`, and `configRequirements/read` with a temporary `CODEX_HOME`; no `config/list` wrapper unless the manifest exposes one
  - `TestRealAppServerFilesystemWatch`: filesystem watch start/event/unwatch using a temporary directory
  - `TestRealAppServerCommandExecStreaming`: command exec streaming with a harmless local command
  - `TestRealAppServerProcessLifecycle`: process spawn/write/terminate stream cleanup with a harmless local process
  - `TestRealAppServerSafeResourceWorkflows`: skills/app/MCP/plugin/marketplace/review read-only or mocked workflows where current app-server supports them; any not-applicable row must name the current manifest limitation
  - `TestRealAppServerRemoteControlWorkflow`: remote-control pairing/session workflow or a matrix-backed not-applicable reason when it cannot be safely exercised in CI
  - `TestRealAppServerModelList`: model list or another safe read-only resource workflow
  - `TestRealAppServerProtocolModeExperimentalGate`: stable-mode rejection for a known experimental method and experimental-mode acceptance using a mocked safe method path
  - `TestRealAppServerUnauthenticatedAccountRead`: release runtime returns the typed unauthenticated account error without live auth/backend traffic or localhost auth override.
- [ ] The CI workflow must run a required-test gate before the broad `go test ./...`: each exact real-runtime test name above must be present in `go test ./... -list`, then executed individually with `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1`. A renamed, build-tag-excluded, skipped, or deleted required real-runtime test must fail CI before the broad package test can hide it.
- [ ] Create `sdk/go/integration_auth_test.go` for login/account integration using mocked auth/Responses endpoints only, but keep it out of the release-shaped runtime proof. These tests must use injected transport or the separately reviewed debug/test fixture runtime from Stage 3, not the release `CODEX_EXEC_PATH app-server` path and not a release-shipped auth base-url env override. The mock server must cover the current production paths exercised by these tests: device-code `/api/accounts` user-code and polling endpoints, `/oauth/token` exchange/refresh paths needed by `complete_device_code_login`, agent-identity auth API/JWKS endpoints, account usage/rate-limit backend endpoints reached through `BackendClient::from_auth(chatgpt_base_url, auth)`, and rate-limit reset-credit endpoints. Do not use live ChatGPT/auth services or user credentials.
  - `TestAuthFixtureUnauthenticatedAccountRead`
  - `TestAuthFixtureFakeAPIKeyLogin`
  - `TestAuthFixtureDeviceCodeFlow`
  - `TestAuthFixtureUsageAndRateLimitRead`.
- [ ] CI must export only `CODEX_EXEC_PATH` and `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1` at the parent job level for real-runtime tests. The test harness must create isolated `CODEX_HOME` values, reject app-server hook envs before release runtime spawn, and prove no auth/mock env hook is accepted by release/default runtime. On Linux, sandbox readiness must be proven by `.github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "$CODEX_EXEC_PATH"` or by an equivalent bounded sandbox smoke through the staged runtime before sandboxed command/process tests run; PATH presence alone is not sufficient. On Windows tests must derive `codex-windows-sandbox-setup.exe` and `codex-command-runner.exe` from the staged package root rather than from Cargo env vars.
- [ ] CI must set `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1`; mock Responses endpoints for thread/run traffic are created by the in-process Go test harness through isolated `CODEX_HOME/config.toml`, so no separate CI service startup step is required. Mock auth endpoints are used only by the auth fixture tests, not by release-readiness.

### Task 6.5: Add Release Readiness Checks

- [ ] Create `sdk/go/RELEASE.md` with Go SDK release guidance: module path, version/tag format, v0/v1 import path, future v2+ import path, non-publishing release validation command, and bad-tag remediation. Remediation must state that published Go module tags are not overwritten or retagged in place; a bad published tag is handled by blocking the release, publishing a higher patch version with an appropriate `retract` directive or release note when needed, and documenting the superseding version.
- [ ] Add `sdk/go/release_readiness_test.go` with `TestReleaseReadiness`. The test must inspect checked-in README, examples, and `sdk/go/RELEASE.md`, and fail unless:
  - v0/v1 import examples use `github.com/openai/codex/sdk/go`
  - future v2 examples use `github.com/openai/codex/sdk/go/v2`
  - `sdk/go/RELEASE.md` mentions prefixed tags `sdk/go/vX.Y.Z`
  - `sdk/go/RELEASE.md` documents bad-tag remediation without deleting, overwriting, or retagging an already published Go module version.
- [ ] Create `.github/workflows/go-sdk-release-readiness.yml` as a non-publishing release validation path:

```yaml
name: go-sdk-release-readiness

on:
  workflow_call:
    inputs:
      checkout_ref:
        required: true
        type: string
      validate_synthetic_tags:
        required: false
        type: boolean
        default: true
  workflow_dispatch:
    inputs:
      checkout_ref:
        description: "Optional checkout ref for manual validation"
        required: false
        type: string
  push:
    tags:
      - "sdk/go/v*"

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: false

jobs:
  validate-go-sdk-release:
    if: github.event_name == 'workflow_call' || github.repository == 'openai/codex' || github.event_name == 'workflow_dispatch'
    runs-on:
      group: ${{ github.event.repository.name }}-runners
      labels: ${{ github.event.repository.name }}-linux-x64
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
        with:
          persist-credentials: false
          fetch-depth: 0
          ref: ${{ inputs.checkout_ref || github.ref }}
      - uses: actions/setup-go@924ae3a1cded613372ab5595356fb5720e22ba16 # v6
        with:
          go-version: '1.25'
          cache-dependency-path: |
            sdk/go/go.mod
            sdk/go/go.sum
      - name: Validate tag and module
        shell: bash
        run: |
          set -euo pipefail
          ref_name="${GITHUB_REF_NAME:-manual}"
          module_version="$(git rev-parse HEAD)"
          module_import="$(awk '/^module / { print $2; exit }' sdk/go/go.mod)"
          [[ -n "${module_import}" ]]
          if [[ "${GITHUB_EVENT_NAME}" == "push" ]]; then
            checked_out_commit="$(git rev-parse HEAD)"
            tagged_commit="$(git rev-parse "${GITHUB_REF}^{commit}")"
            test "${checked_out_commit}" = "${tagged_commit}"
          fi
          release_tags=()
          declare -A synthetic_tag_refs=()
          if [[ "${GITHUB_EVENT_NAME}" == "push" ]]; then
            [[ "${ref_name}" =~ ^sdk/go/v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]
            module_version="${ref_name#sdk/go/}"
            major="${module_version%%.*}"
            major_number="${major#v}"
            if (( major_number >= 2 )); then
              module_import="github.com/openai/codex/sdk/go/${major}"
            fi
            release_tags=("${ref_name}")
          elif [[ "${{ inputs.validate_synthetic_tags || false }}" == "true" ]]; then
            annotated_synthetic_tag="sdk/go/v1.99.1-go-sdk-ci-annotated"
            release_tags=("sdk/go/v0.99.0-go-sdk-ci" "sdk/go/v1.99.0-go-sdk-ci" "${annotated_synthetic_tag}" "sdk/go/v2.99.0-go-sdk-ci")
            synthetic_tag_refs["sdk/go/v0.99.0-go-sdk-ci"]="$(git rev-parse HEAD)"
            synthetic_tag_refs["sdk/go/v1.99.0-go-sdk-ci"]="$(git rev-parse HEAD)"
            synthetic_tag_refs["${annotated_synthetic_tag}"]="$(git rev-parse HEAD)"
          fi
          grep -Fx "module ${module_import}" sdk/go/go.mod
          cd sdk/go
          go test ./...
          go test ./... -run TestReleaseReadiness
          bare_remote="${RUNNER_TEMP}/codex-go-sdk-release.git"
          git clone --bare "${GITHUB_WORKSPACE}" "${bare_remote}"
          if [[ -n "${synthetic_tag_refs[sdk/go/v1.99.0-go-sdk-ci]:-}" ]]; then
            synthetic_v2_worktree="${RUNNER_TEMP}/codex-go-sdk-synthetic-v2"
            git clone "${bare_remote}" "${synthetic_v2_worktree}"
            (
              cd "${synthetic_v2_worktree}"
              go -C sdk/go mod edit -module github.com/openai/codex/sdk/go/v2
              self_imports="${RUNNER_TEMP}/codex-go-sdk-v2-self-imports"
              if grep -RIl 'github.com/openai/codex/sdk/go' sdk/go --include='*.go' > "${self_imports}"; then
                xargs -r perl -0pi -e 's#github\.com/openai/codex/sdk/go"#github.com/openai/codex/sdk/go/v2"#g; s#github\.com/openai/codex/sdk/go/(?!v[0-9]+(?:/|"))#github.com/openai/codex/sdk/go/v2/#g' < "${self_imports}"
              fi
              stale_v2_imports="${RUNNER_TEMP}/codex-go-sdk-v2-stale-imports"
              find sdk/go -name '*.go' -print0 | xargs -0 perl -ne 'print "$ARGV:$.:$_" if /github\.com\/openai\/codex\/sdk\/go(?:"|\/(?!v[0-9]+(?:\/|")))/' > "${stale_v2_imports}"
              test ! -s "${stale_v2_imports}" || { cat "${stale_v2_imports}"; exit 1; }
              go -C sdk/go test ./...
              git add sdk/go/go.mod
              git add sdk/go
              git -c user.name=codex-go-sdk-ci -c user.email=codex-go-sdk-ci@example.invalid commit -m "synthetic Go SDK v2 module path"
              git push "${bare_remote}" HEAD:refs/heads/synthetic-go-sdk-v2
              synthetic_v2_ref="$(git rev-parse HEAD)"
              printf '%s\n' "${synthetic_v2_ref}" > "${RUNNER_TEMP}/codex-go-sdk-synthetic-v2-ref"
            )
            synthetic_tag_refs["sdk/go/v2.99.0-go-sdk-ci"]="$(cat "${RUNNER_TEMP}/codex-go-sdk-synthetic-v2-ref")"
          fi
          for release_tag in "${release_tags[@]}"; do
            if [[ "${GITHUB_EVENT_NAME}" == "push" ]]; then
              git --git-dir="${bare_remote}" show-ref --verify "refs/tags/${release_tag}"
            elif [[ "${release_tag}" == "${annotated_synthetic_tag:-}" ]]; then
              git --git-dir="${bare_remote}" \
                -c user.name=codex-go-sdk-ci \
                -c user.email=codex-go-sdk-ci@example.invalid \
                tag -a "${release_tag}" "${synthetic_tag_refs[${release_tag}]}" -m "synthetic Go SDK annotated tag"
              peeled_commit="$(git --git-dir="${bare_remote}" rev-parse "${release_tag}^{commit}")"
              test "${peeled_commit}" = "${synthetic_tag_refs[${release_tag}]}"
            else
              git --git-dir="${bare_remote}" tag "${release_tag}" "${synthetic_tag_refs[${release_tag}]}"
            fi
          done
          git_config_global="${RUNNER_TEMP}/codex-go-sdk-release.gitconfig"
          : > "${git_config_global}"
          export GIT_CONFIG_GLOBAL="${git_config_global}"
          trace_dir="$(mktemp -d "${RUNNER_TEMP}/codex-go-sdk-release-trace.XXXXXX")"
          rewrites=(
            "https://github.com/openai/codex.git"
            "https://github.com/openai/codex"
            "ssh://git@github.com/openai/codex.git"
            "ssh://git@github.com/openai/codex"
            "git@github.com:openai/codex.git"
            "git@github.com:openai/codex"
          )
          for rewrite in "${rewrites[@]}"; do
            git config --global --add url."file://${bare_remote}".insteadOf "${rewrite}"
          done
          for rewrite in "${rewrites[@]}"; do
            git config --global --get-all url."file://${bare_remote}".insteadOf | grep -Fx "${rewrite}"
          done
          GIT_TRACE=1 git ls-remote https://github.com/openai/codex HEAD > /dev/null 2> "${trace_dir}/rewrite-trace.log"
          grep -F "file://${bare_remote}" "${trace_dir}/rewrite-trace.log"
          export GOPRIVATE=github.com/openai/codex
          export GONOSUMDB=github.com/openai/codex
          export GIT_ALLOW_PROTOCOL=file:https:ssh
          export GOMODCACHE="$(mktemp -d)"
          consumer_dir="$(mktemp -d)"
          cd "${consumer_dir}"
          go mod init example.com/codex-go-sdk-consumer
          GIT_TRACE=1 go get "${module_import}@${module_version}" 2> "${trace_dir}/go-get-trace.log"
          grep -F "file://${bare_remote}" "${trace_dir}/go-get-trace.log"
          cat > main.go <<GO
          package main

          import (
            codex "${module_import}"
            _ "${module_import}/protocol"
          )

          func main() {
            _ = codex.ClientConfig{}
          }
          GO
          go mod tidy
          go test ./...
          for release_tag in "${release_tags[@]}"; do
            release_version="${release_tag#sdk/go/}"
            release_major="${release_version%%.*}"
            release_import="$(awk '/^module / { print $2; exit }' "${GITHUB_WORKSPACE}/sdk/go/go.mod")"
            release_major_number="${release_major#v}"
            if (( release_major_number >= 2 )); then
              release_import="github.com/openai/codex/sdk/go/${release_major}"
            fi
            git --git-dir="${bare_remote}" show "${release_tag}:sdk/go/go.mod" | grep -Fx "module ${release_import}"
            release_consumer_dir="$(mktemp -d)"
            cd "${release_consumer_dir}"
            go mod init example.com/codex-go-sdk-release-consumer
            GIT_TRACE=1 go get "${release_import}@${release_version}" 2> "${trace_dir}/go-get-${release_version}.log"
            grep -F "file://${bare_remote}" "${trace_dir}/go-get-${release_version}.log"
            cat > main.go <<GO
            package main

            import (
              codex "${release_import}"
              _ "${release_import}/protocol"
            )

            func main() {
              _ = codex.ClientConfig{}
            }
            GO
            go mod tidy
            go test ./...
          done
      - name: Check for a clean worktree
        if: always() && !cancelled()
        uses: ./.github/actions/check-clean-worktree
```

- [ ] The release-readiness workflow is validation-only and secretless by default. It must not publish artifacts or persistent tags, but it must always run when called by blocking CI through `workflow_call`, be runnable by `workflow_dispatch`, and automatically validate real `sdk/go/v*` tags. Repository guards may apply only to direct `push`/manual events, not to `workflow_call`. Its checkout must use `inputs.checkout_ref || github.ref`, not `github.sha`, so tag pushes, including annotated tags, validate the actual ref; direct push validation must compare checked-out `HEAD` to `git rev-parse "${GITHUB_REF}^{commit}"`. Non-tag validation must use `git rev-parse HEAD` from the checked-out repository rather than the default `${GITHUB_SHA}` environment. Its external-consumer test must not use `replace`; it must fetch the derived Go module import path through VCS/module resolution against a tag-faithful git remote and strip `sdk/go/` from release tags only for the Go module version query. The generated external consumer must import both the root module and `${module_import}/protocol` or `${release_import}/protocol`, so release readiness proves exported subpackages compile in addition to the root package. During `workflow_call`, `validate_synthetic_tags=true` must create synthetic `sdk/go/v0.*` and `sdk/go/v1.*` tags inside the temporary bare remote pointing at the reviewed head checkout; the annotated `sdk/go/v1.*` tag must be created only in that bare remote with `git --git-dir="${bare_remote}" -c user.name=... -c user.email=... tag -a`, peeled to its commit with `git --git-dir="${bare_remote}" rev-parse "${release_tag}^{commit}"`, and skipped by the generic lightweight-tag creation path. Then the workflow must run the same external-consumer `go get`, import, `go mod tidy`, and `go test ./...` checks for those current-major tag-shaped branches. Synthetic `sdk/go/v2.*` and later checks are allowed only as future-major policy smoke tests: they must point at a throwaway bare-remote commit whose `sdk/go/go.mod` module path and module-internal Go imports already include the semantic import suffix, and that rewritten `sdk/go` module must pass `go test ./...` before it is committed and tagged. The workflow and docs must label this as synthetic rewritten-tree validation, not reviewed-head checkout validation, so CI does not pretend the current `v0/v1` module is already a valid `v2` release. Tags `sdk/go/v0.*` and `sdk/go/v1.*` must use `github.com/openai/codex/sdk/go`; tags `sdk/go/v2.*` and later must use the corresponding semantic import path such as `github.com/openai/codex/sdk/go/v2`. `TestReleaseReadiness` must assert non-tag, v0 tag, v1 tag, annotated v1 tag peeling, synthetic v2+ policy smoke, protocol subpackage imports, no `secrets: inherit` on the caller, and synthetic v2 self-test before release validation can pass.
- [ ] Wire the release-readiness gate into CI and final verification with:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./... -run TestReleaseReadiness
```

- [ ] The release-readiness test must also assert `.github/workflows/go-sdk-release-readiness.yml` exists, `.github/scripts/stage-codex-runtime.sh` exists and is executable, the workflow contains `workflow_call`, required input `checkout_ref`, optional/defaulted input `validate_synthetic_tags`, `workflow_dispatch`, and `sdk/go/v*` tag triggers, the release-readiness job condition allows `github.event_name == 'workflow_call'` without a repository guard, checks out `inputs.checkout_ref || github.ref` and contains no checkout fallback to `github.sha`, the caller job in `.github/workflows/sdk.yml` passes `checkout_ref: ${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}`, enables synthetic tag validation, and does not pass `secrets: inherit`, derives `module_version` from `git rev-parse HEAD` for non-tag runs, derives non-tag `module_import` from checked-in `sdk/go/go.mod`, peels pushed tags with `git rev-parse "${GITHUB_REF}^{commit}"` before comparing to the checked-out commit, creates synthetic `sdk/go/v0.*` and `sdk/go/v1.*` tags in the temporary bare remote for `workflow_call` against the reviewed head checkout, creates the annotated synthetic `sdk/go/v1.*` tag only in the bare remote with explicit `user.name`/`user.email`, peels it to its commit, and skips it in the generic lightweight-tag loop, creates synthetic `sdk/go/v2.*` only as a labeled throwaway rewritten-tree policy smoke, rewrites or rejects stale v2 module-internal imports before tagging the synthetic v2 commit, runs `go -C sdk/go test ./...` in the rewritten synthetic v2 worktree before tagging it, derives v0/v1 versus v2+ tag imports and asserts they match the tagged `go.mod`, stores `GIT_CONFIG_GLOBAL` and trace logs under `${RUNNER_TEMP}`, uses `git config --global --add` plus `--get-all` assertions for every supported GitHub URL rewrite, verifies `go get` reads the local bare remote through `GIT_TRACE` for non-tag and synthetic/real tag paths, compiles an external consumer importing the derived module path and its `/protocol` subpackage for every release tag, contains no `go mod edit -replace github.com/openai/codex/sdk/go`, and ends with the shared clean-worktree action.
- [ ] The Go SDK CI workflow must cover every release package target that can be claimed as Go SDK release-ready: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc`. The `x86_64-apple-darwin` lane must run on an Intel macOS label such as `macos-15-large` with a `uname -m` self-check, or it must run an explicit Rosetta `arch -x86_64` validation path; `macos-15-xlarge` alone is arm64 and cannot prove x64 runtime execution. If runner capacity blocks a target, the workflow and Stage 7 checklist must explicitly mark that target as not release-ready instead of silently inheriting confidence from a different architecture.
- [ ] The release-package archive lane must run with `--cargo-profile release` for every claimed Linux, macOS, and Windows target, fail fast unless `zstd` is available from an explicit Stage 5G-approved no-network `CODEX_GO_SDK_ZSTD_SOURCE`/`--zstd-source` value or a preinstalled runner binary, and the staging metadata for that lane must record `runtimeSource: "packageArchive"`, `cargoProfile: "release"`, `zstdSource`, `archiveFormats` containing `tar.zst`, and `bazelTarget` equal to the matrix lane target. Each matrix lane must emit target-bound, non-secret SDK CI evidence, and a required aggregation/upload step must publish `go-sdk-ci-release-evidence/go-sdk-ci-release-evidence.json` with one `targets.<triple>` object per claimed target containing the GitHub Actions `jobName`, `jobConclusion: "success"`, `bazelTarget`, `runnerLabel`, `runtimeSource: "packageArchive"`, `cargoProfile: "release"`, packageArchive verifier status, bounded log references, the exact required packageArchive smoke tests that were first listed with `go test -list` and then ran, sanitized staging metadata, macOS x64 `architectureProof` plus downloaded `architectureProofPath` (`unameMachine` or `arch -x86_64` command plus runner label), and Windows `windowsReleaseShapedMsvc: true` plus `windowsMsvcHostPlatform: true` with downloaded `windowsHostProofPath` for both MSVC targets. Final release-readiness requires no `skipped` or `notReleaseReady` marker; while Go SDK CI still creates helper roots with producer-mode `materialize_helpers` instead of consuming a pre-produced Stage 5G helper-root artifact and running `--verify-only`, the target evidence must carry `helperRootEvidence.source=producerModeMaterializeHelpers` plus `notReleaseReady=true`, the aggregate evidence must still upload with top-level `notReleaseReady=true` and `notReleaseReadyTargets` for auditability, and Stage 7 must block release-readiness instead of overclaiming hermetic helper proof. `TestRuntimePackageLayoutParity` or a dedicated staging-script test must fail if `--release-package-archive --cargo-profile dev` is accepted, if any current shipping target lacks a packageArchive verifier run, if `.github/dotslash-config.json` no longer matches every shipping release artifact name/path published by `publish-dotslash` (`codex`, `codex-app-server`, `codex-responses-api-proxy`, Linux `bwrap`, Windows `codex-command-runner`, and Windows `codex-windows-sandbox-setup`), if the `.tar.zst` package archives lack checksum/provenance metadata in the release-readiness artifact, if `TestRealAppServerRejectsDebugHookEnv` is absent from the packageArchive smoke suite, if the archive-lane tests inherit `CODEX_HOME` or app-server debug hook envs from staging, or if archive staging reaches `.github/workflows/zstd`, DotSlash, or the package-builder PATH fallback. macOS DMG/direct artifact checks are mandatory for claimed macOS release readiness and must not replace packageArchive while `package-macos` invokes `.github/scripts/build-codex-package-archive.sh`.
- [ ] Final release-readiness sign-off must record successful GitHub Actions run IDs for the real `.github/workflows/sdk.yml` matrix, `.github/workflows/go-sdk-release-readiness.yml`, and the real or non-publishing shipping release-readiness workflow on the reviewed commit. Stage 7 must prove every claimed release target has a successful, non-skipped GitHub Actions job in both `sdk.yml` and shipping release-readiness evidence, using downloaded target-bound metadata artifacts rather than aggregate log greps. Stage 7 must download and validate `go-sdk-ci-release-evidence/go-sdk-ci-release-evidence.json` from the `sdk.yml` run for target-specific packageArchive verifier, release cargo profile, staging metadata, runner labels, job success, no skipped/not-release-ready marker, macOS x64 architecture proof plus downloaded proof file, Windows MSVC release-shaped host flags plus downloaded host proof file, and required smoke-test evidence. Stage 7 must also download the shipping release-readiness artifacts and validate `shipping-release-readiness-metadata/shipping-release-readiness.json`, including downloaded workflow reuse proof, duplicate-command audit proof, successful reused critical job conclusions, explicit `fixtureSubstitutions`, bounded log files, target-specific successful job names/conclusions, target-specific `codex-package-*.tar.zst` and `codex-app-server-package-*.tar.zst` archive filenames, target-specific in-archive executable path sets for both packages, downloaded archive member inventory files, runtime helper paths inside the target `codex-package` archive, public `.tar.zst` checksum manifest records and downloaded checksum manifest copies for both packages, runner labels, macOS x64 architecture proof, macOS DMG/direct artifact names, Windows published zip members plus downloaded zip inventory files, DotSlash config parity and downloaded parity report for every entry in `.github/dotslash-config.json`, and the exact SDK packageArchive smoke tests that ran. Local source parsing, local smoke commands, self-reported metadata fields without matching downloaded evidence files, and log-only `grep` checks are preflight only; they are not sufficient to claim Linux/macOS/Windows release readiness.

### Task 6.6: Verify And Commit

- [ ] Run on Linux/macOS, or in Git Bash only after evaluating the staging script's exported environment in the same shell:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./...
go test ./... -run TestReleaseReadiness
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
bazel build //codex-rs/cli:codex_go_sdk_runtime_layout
```

Native Windows PowerShell local verification must not run a bare Bazel command before bootstrap. Until the native PowerShell packageArchive lane is implemented, record that `-ReleasePackageArchive` fails closed, then use the supported non-archive Stage 6 Windows-local script contract for bootstrap/local checks. Do not treat the blocked packageArchive call as Windows release-readiness evidence; Windows packageArchive readiness must come from downloaded target-bound SDK CI and shipping artifacts:

```powershell
Set-Location C:\path\to\codex-go-sdk-full
$expectedBazelTarget = if ($env:CODEX_GO_SDK_WINDOWS_TARGET) { $env:CODEX_GO_SDK_WINDOWS_TARGET } else { "x86_64-pc-windows-msvc" }
try {
  & .github\scripts\stage-codex-runtime.ps1 -Out "$env:TEMP\codex-go-sdk-runtime-stage6-blocked" -BazelTarget $expectedBazelTarget -CargoProfile release -ReleasePackageArchive -WindowsReleaseShapedMsvc -ExportEnvironment | Invoke-Expression
  throw "native Windows packageArchive verifier unexpectedly succeeded; update Stage 6/7 to validate implemented packageArchive evidence before using it"
} catch {
  if ($_.Exception.Message -notmatch "packageArchive staging is blocked until the native Windows app-server package archive lane is implemented") { throw }
}
& .github\scripts\stage-codex-runtime.ps1 -Out "$env:TEMP\codex-go-sdk-runtime-stage6" -BazelTarget $expectedBazelTarget -ExportEnvironment | Invoke-Expression
bazel build //codex-rs/cli:codex_go_sdk_runtime_layout
```

- [ ] Run protocol gates if this stage touches generated artifacts:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just write-app-server-schema --check
just test -p codex-app-server-protocol
just fix -p codex-app-server-protocol
just fmt
cd ..
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
```

- [ ] Commit per reviewed substage. Do not squash these before fresh review, because each substage has different failure modes:

```bash
git add sdk/go/README.md sdk/go/examples sdk/go/*coverage*_test.go
git commit -m "docs(go-sdk): add examples and coverage gates"

git add codex-rs/cli/BUILD.bazel bazel/platforms/release_binaries.bzl sdk/go/runtime_package_layout_test.go
test ! -f bazel/platforms/go_sdk_runtime_layout.bzl || git add bazel/platforms/go_sdk_runtime_layout.bzl
git commit -m "build(go-sdk): stage release-shaped app-server runtime"

git add .github/scripts/stage-codex-runtime.sh .github/scripts/stage-codex-runtime.ps1 sdk/go/runtime_staging_script_test.go
git commit -m "build(go-sdk): add runtime staging scripts"

git add sdk/go/integration_app_server_test.go sdk/go/integration_auth_test.go sdk/go/internal/testharness/mock_services.go
git commit -m "test(go-sdk): cover real app-server integration"

git add .github/workflows/sdk.yml
git commit -m "ci(go-sdk): add real runtime SDK job"

git add sdk/go/RELEASE.md sdk/go/release_readiness_test.go .github/workflows/go-sdk-release-readiness.yml .github/workflows/go-sdk-shipping-release-readiness.yml .github/workflows/rust-release.yml .github/workflows/rust-release-windows.yml
git commit -m "ci(go-sdk): add release readiness checks"
```

## Stage Review

Fresh blind engineering, product, and release/ops reviews are mandatory.
