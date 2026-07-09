# Stage 7: Final Verification And Handoff

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:verification-before-completion, then superpowers:requesting-code-review, then superpowers:finishing-a-development-branch.

**Goal:** Prove the full Go SDK is complete, reviewed, and ready for integration or PR.

**Architecture:** Final verification checks generated drift, Go behavior, Rust protocol compatibility, docs/examples, CI wiring, and clean worktree status.

**Tech Stack:** Go test, Rust `just`, Git, GitHub Actions config.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:675-790`
- Current `git diff`
- The current blind-review packet defined in `index.md`; final reviewers must not receive prior review outputs or repaired finding history.

## Tasks

### Task 7.1: Full Local Verification

- [ ] On Linux/macOS, or on Windows Git Bash only when the same Git Bash session obtains its environment from `.github/scripts/stage-codex-runtime.sh --print-shell-env` or is launched from a PowerShell process after `stage-codex-runtime.ps1 -ExportEnvironment | Invoke-Expression`, run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./...
```

- [ ] Run race tests for Go SDK concurrency-heavy code:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test -race ./...
```

- [ ] Run real app-server integration tests in non-skip mode on Unix or Git Bash:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
runtime_dir="$(mktemp -d)"
stage_runtime_args=(--out "${runtime_dir}" --print-shell-env)
expected_windows_bazel_target="${CODEX_GO_SDK_WINDOWS_TARGET:-x86_64-pc-windows-msvc}"
case "$(uname -s)" in
MINGW64_NT*|MSYS_NT*)
  stage_runtime_args+=(--bazel-target "${expected_windows_bazel_target}" --windows-release-shaped-msvc --windows-msvc-host-platform)
  ;;
esac
eval "$(.github/scripts/stage-codex-runtime.sh "${stage_runtime_args[@]}")"
test -n "${CODEX_EXEC_PATH}"
case "${CODEX_EXEC_PATH}" in
  */bin/codex|*/bin/codex.exe|*\\bin\\codex.exe) ;;
  *) echo "CODEX_EXEC_PATH must point inside the staged package bin directory: ${CODEX_EXEC_PATH}" >&2; exit 1 ;;
esac
test -f "${runtime_dir}/codex-package.json"
test -f "${runtime_dir}/codex-go-sdk-runtime-staging.json"
test -d "${runtime_dir}/codex-resources"
test -d "${runtime_dir}/codex-path"
case "$(uname -s)" in
MINGW64_NT*|MSYS_NT*)
  test -x "${runtime_dir}/codex-path/rg.exe"
  test -x "${runtime_dir}/codex-resources/codex-windows-sandbox-setup.exe"
  test -x "${runtime_dir}/codex-resources/codex-command-runner.exe"
  json_python=python3
  command -v "${json_python}" >/dev/null 2>&1 || json_python=python
  "${json_python}" - "${runtime_dir}/codex-go-sdk-runtime-staging.json" "${expected_windows_bazel_target}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    metadata = json.load(fh)
expected_bazel_target = sys.argv[2]
if metadata.get("bazelTarget") != expected_bazel_target:
    raise SystemExit(f"staged runtime did not use the expected MSVC Bazel target: {expected_bazel_target}")
if metadata.get("windowsReleaseShapedMsvc") is not True:
    raise SystemExit("staged runtime was not marked Windows release-shaped MSVC")
if metadata.get("windowsMsvcHostPlatform") is not True:
    raise SystemExit("staged runtime did not use the Windows MSVC host platform override")
PY
  ;;
*)
  test -x "${runtime_dir}/codex-path/rg"
  test -x "${runtime_dir}/codex-resources/zsh/bin/zsh"
  ;;
esac
if [[ "$(uname -s)" == "Linux" ]]; then
  test -x "${runtime_dir}/codex-resources/bwrap"
  .github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "${CODEX_EXEC_PATH}"
fi
unset CODEX_HOME CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS
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
```

- [ ] On native Windows PowerShell, use the supported Windows-local verifier instead of relying on Git Bash, and explicitly record that the local PowerShell packageArchive path remains blocked until implemented:

```powershell
# Run from the native Windows checkout root for this worktree.
$repo = (Get-Location).Path
Set-Location $repo
$runtimeDir = Join-Path $env:TEMP ("codex-go-sdk-runtime-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $runtimeDir | Out-Null
$expectedBazelTarget = if ($env:CODEX_GO_SDK_WINDOWS_TARGET) { $env:CODEX_GO_SDK_WINDOWS_TARGET } else { "x86_64-pc-windows-msvc" }
$zstdArgs = @()
if ($env:CODEX_GO_SDK_ZSTD_SOURCE) {
  if (-not (Test-Path $env:CODEX_GO_SDK_ZSTD_SOURCE)) { throw "CODEX_GO_SDK_ZSTD_SOURCE must point at the Stage 5G materialized zstd executable" }
  $zstdArgs = @("-ZstdSource", $env:CODEX_GO_SDK_ZSTD_SOURCE)
} elseif (-not (Get-Command zstd -ErrorAction SilentlyContinue)) {
  throw "zstd must be installed unless CODEX_GO_SDK_ZSTD_SOURCE points at the Stage 5G no-network source; DotSlash fallback is not accepted"
}
try {
  & .github\scripts\stage-codex-runtime.ps1 -Out $runtimeDir -BazelTarget $expectedBazelTarget -CargoProfile release -ReleasePackageArchive @zstdArgs -WindowsReleaseShapedMsvc -ExportEnvironment | Invoke-Expression
  throw "native Windows packageArchive verifier unexpectedly succeeded; update this Stage 7 gate to validate the implemented packageArchive evidence before using it"
} catch {
  if ($_.Exception.Message -notmatch "packageArchive staging is blocked until the native Windows app-server package archive lane is implemented") {
    throw
  }
}
Remove-Item -Recurse -Force $runtimeDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $runtimeDir | Out-Null
& .github\scripts\stage-codex-runtime.ps1 -Out $runtimeDir -BazelTarget $expectedBazelTarget -ExportEnvironment | Invoke-Expression
if (-not $env:CODEX_EXEC_PATH) { throw "CODEX_EXEC_PATH was not exported" }
if ($env:CODEX_EXEC_PATH -notmatch '[\\/]bin[\\/]codex\.exe$') { throw "CODEX_EXEC_PATH must point inside the staged package bin directory: $env:CODEX_EXEC_PATH" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-package.json'))) { throw "missing codex-package.json" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-go-sdk-runtime-staging.json'))) { throw "missing staging metadata" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-resources'))) { throw "missing codex-resources" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-path'))) { throw "missing codex-path" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-path\rg.exe'))) { throw "missing staged rg helper" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-resources\codex-windows-sandbox-setup.exe'))) { throw "missing staged Windows sandbox setup helper" }
if (-not (Test-Path (Join-Path $runtimeDir 'codex-resources\codex-command-runner.exe'))) { throw "missing staged command runner helper" }
$stagingMetadata = Get-Content (Join-Path $runtimeDir 'codex-go-sdk-runtime-staging.json') | ConvertFrom-Json
if ($stagingMetadata.runtimeSource -ne "bazelLayout") { throw "native Windows verifier did not stage Bazel layout runtime" }
if ($stagingMetadata.cargoProfile -ne "dev") { throw "native Windows verifier did not record dev cargoProfile for the supported local path" }
if ($stagingMetadata.bazelTarget -ne $expectedBazelTarget) { throw "staged runtime did not use the expected MSVC Bazel target $expectedBazelTarget" }
Remove-Item Env:CODEX_HOME -ErrorAction SilentlyContinue
Remove-Item Env:CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG -ErrorAction SilentlyContinue
Remove-Item Env:CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE -ErrorAction SilentlyContinue
Remove-Item Env:CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS -ErrorAction SilentlyContinue
$env:CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER = "1"
Set-Location sdk\go
$requiredTests = @(
  "TestRealAppServerInitializeStrictDigest",
  "TestRealAppServerRejectsDebugHookEnv",
  "TestRealAppServerDigestMismatch",
  "TestRealAppServerCompatibilityOverridePolicy",
  "TestRealAppServerThreadRunHappyPath",
  "TestRealAppServerConfigReadWrite",
  "TestRealAppServerFilesystemWatch",
  "TestRealAppServerCommandExecStreaming",
  "TestRealAppServerProcessLifecycle",
  "TestRealAppServerSafeResourceWorkflows",
  "TestRealAppServerRemoteControlWorkflow",
  "TestRealAppServerModelList",
  "TestRealAppServerProtocolModeExperimentalGate",
  "TestRealAppServerUnauthenticatedAccountRead"
)
foreach ($testName in $requiredTests) {
  $listed = go test ./... -list "^$testName$"
  if ($LASTEXITCODE -ne 0 -or $listed -notcontains $testName) { throw "required test not listed: $testName" }
  go test ./... -run "^$testName$"
  if ($LASTEXITCODE -ne 0) { throw "required test failed: $testName" }
}
```

Expected: every required test name exists and executes instead of skipping against the complete staged Bazel-built `codex` package layout produced by the stage-codex-runtime script family: `CODEX_EXEC_PATH` points at `bin/codex[.exe]`, package root contains `codex-package.json`, `codex-resources/`, and `codex-path/`, and `InstallContext::from_exe` can discover the package layout. Any missing `CODEX_EXEC_PATH`, missing package-root runtime resource such as bundled `codex-resources/*` or `codex-path/*`, missing mock Responses harness, accepted release-runtime auth/config/plugin test hook, skipped real-runtime test, or skipped `Thread.Run`/`turn/start` same-checkout runtime test is a failure when `CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1`. On Linux, sandbox readiness must be proven by `.github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "${CODEX_EXEC_PATH}" using the same system/bundled bubblewrap capability semantics as the runtime, not by PATH presence alone. On Windows, native local verification must use `.github/scripts/stage-codex-runtime.ps1` without Git Bash assumptions, Windows helper binaries must be discovered from `codex-resources/` under the staged package root, and the local PowerShell `-ReleasePackageArchive` path must be treated as an explicit blocked gap until implemented rather than as Windows packageArchive release-readiness evidence.

- [ ] Run the release-package archive real app-server smoke in non-skip mode once per claimed target, on the matching release runner/host shape for that target. Do not use an OS-family target loop as release evidence: each invocation must set exactly one `CODEX_GO_SDK_ARCHIVE_TARGET`, and the command must fail if the current machine cannot execute that target's staged runtime. Required invocations are:
  - `CODEX_GO_SDK_ARCHIVE_TARGET=x86_64-unknown-linux-musl` on `${repo}-linux-x64-xl`, with `uname -m` proving x86_64.
  - `CODEX_GO_SDK_ARCHIVE_TARGET=aarch64-unknown-linux-musl` on `${repo}-linux-arm64`, with `uname -m` proving arm64/aarch64.
  - `CODEX_GO_SDK_ARCHIVE_TARGET=aarch64-apple-darwin` on `macos-15-xlarge`, with `uname -m` proving arm64.
  - `CODEX_GO_SDK_ARCHIVE_TARGET=x86_64-apple-darwin` on `macos-15-large` with `uname -m == x86_64`, or on an approved Rosetta path that sets `CODEX_GO_SDK_X64_EXEC_PREFIX="arch -x86_64"` and runs staging plus smoke tests through that prefix.
  - `CODEX_GO_SDK_ARCHIVE_TARGET=x86_64-pc-windows-msvc` on `${repo}-windows-x64`, with the native host architecture proving AMD64/x64.
  - `CODEX_GO_SDK_ARCHIVE_TARGET=aarch64-pc-windows-msvc` on `${repo}-windows-arm64`, with the native host architecture proving ARM64.

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
: "${CODEX_GO_SDK_ARCHIVE_TARGET:?set to exactly one claimed release target for this runner}"
if [[ -n "${CODEX_GO_SDK_ZSTD_SOURCE:-}" ]]; then
  test -x "${CODEX_GO_SDK_ZSTD_SOURCE}" || { echo "CODEX_GO_SDK_ZSTD_SOURCE must point at the Stage 5G materialized zstd executable" >&2; exit 1; }
else
  command -v zstd >/dev/null 2>&1 || { echo "zstd must be installed unless CODEX_GO_SDK_ZSTD_SOURCE points at the Stage 5G no-network source; DotSlash fallback is not accepted" >&2; exit 1; }
fi
archive_target="${CODEX_GO_SDK_ARCHIVE_TARGET}"
run_prefix=()
case "${archive_target}" in
x86_64-unknown-linux-musl)
  [[ "$(uname -s)" == Linux ]] || { echo "${archive_target} must run on Linux" >&2; exit 1; }
  [[ "$(uname -m)" == x86_64 ]] || { echo "${archive_target} requires an x86_64 Linux runner" >&2; exit 1; }
  ;;
aarch64-unknown-linux-musl)
  [[ "$(uname -s)" == Linux ]] || { echo "${archive_target} must run on Linux" >&2; exit 1; }
  [[ "$(uname -m)" == aarch64 || "$(uname -m)" == arm64 ]] || { echo "${archive_target} requires an arm64 Linux runner" >&2; exit 1; }
  ;;
aarch64-apple-darwin)
  [[ "$(uname -s)" == Darwin ]] || { echo "${archive_target} must run on macOS" >&2; exit 1; }
  [[ "$(uname -m)" == arm64 || "$(uname -m)" == aarch64 ]] || { echo "${archive_target} requires an arm64 macOS runner" >&2; exit 1; }
  ;;
x86_64-apple-darwin)
  [[ "$(uname -s)" == Darwin ]] || { echo "${archive_target} must run on macOS" >&2; exit 1; }
  if [[ "$(uname -m)" == x86_64 ]]; then
    :
  else
    [[ "${CODEX_GO_SDK_X64_EXEC_PREFIX:-}" == "arch -x86_64" ]] || { echo "${archive_target} requires macos-15-large or CODEX_GO_SDK_X64_EXEC_PREFIX='arch -x86_64'" >&2; exit 1; }
    run_prefix=(arch -x86_64)
  fi
  ;;
x86_64-pc-windows-msvc)
  case "$(uname -s)" in MINGW64_NT*|MSYS_NT*) ;; *) echo "${archive_target} must run from Windows Git Bash" >&2; exit 1 ;; esac
  [[ "${PROCESSOR_ARCHITECTURE:-}" == AMD64 || "${PROCESSOR_ARCHITEW6432:-}" == AMD64 ]] || { echo "${archive_target} requires a native x64 Windows runner" >&2; exit 1; }
  ;;
aarch64-pc-windows-msvc)
  case "$(uname -s)" in MINGW64_NT*|MSYS_NT*) ;; *) echo "${archive_target} must run from Windows Git Bash" >&2; exit 1 ;; esac
  [[ "${PROCESSOR_ARCHITECTURE:-}" == ARM64 || "${PROCESSOR_ARCHITEW6432:-}" == ARM64 ]] || { echo "${archive_target} requires a native arm64 Windows runner" >&2; exit 1; }
  ;;
*)
  echo "unsupported CODEX_GO_SDK_ARCHIVE_TARGET=${archive_target}" >&2
  exit 1
  ;;
esac
for expected_archive_bazel_target in "${archive_target}"; do
  archive_runtime_dir="$(mktemp -d)"
  archive_runtime_args=(--out "${archive_runtime_dir}" --release-package-archive --cargo-profile release --print-shell-env --bazel-target "${expected_archive_bazel_target}")
  if [[ -n "${CODEX_GO_SDK_ZSTD_SOURCE:-}" ]]; then
    archive_runtime_args+=(--zstd-source "${CODEX_GO_SDK_ZSTD_SOURCE}")
  fi
  case "$(uname -s)" in
  MINGW64_NT*|MSYS_NT*)
    archive_runtime_args+=(--windows-release-shaped-msvc --windows-msvc-host-platform)
    ;;
  esac
  eval "$("${run_prefix[@]}" .github/scripts/stage-codex-runtime.sh "${archive_runtime_args[@]}")"
  test -n "${CODEX_EXEC_PATH}"
  test -f "${archive_runtime_dir}/codex-package.json"
  test -f "${archive_runtime_dir}/codex-go-sdk-runtime-staging.json"
  json_python=python3
  command -v "${json_python}" >/dev/null 2>&1 || json_python=python
  "${json_python}" - "${archive_runtime_dir}/codex-go-sdk-runtime-staging.json" "${expected_archive_bazel_target}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    metadata = json.load(fh)
expected_bazel_target = sys.argv[2]
if metadata.get("runtimeSource") != "packageArchive":
    raise SystemExit("archive verification did not stage packageArchive runtime")
if metadata.get("cargoProfile") != "release":
    raise SystemExit("archive verification did not record release cargoProfile")
if expected_bazel_target and metadata.get("bazelTarget") != expected_bazel_target:
    raise SystemExit(f"archive verification staged {metadata.get('bazelTarget')} instead of {expected_bazel_target}")
if metadata.get("zstdSource") not in {"preinstalled", "stage5gMaterialized"}:
    raise SystemExit("archive verification did not record the hermetic zstd source contract")
if "tar.zst" not in metadata.get("archiveFormats", []):
    raise SystemExit("archive verification did not record tar.zst archive validation")
if expected_bazel_target.endswith("-pc-windows-msvc"):
    if metadata.get("windowsReleaseShapedMsvc") is not True:
        raise SystemExit("archive Windows runtime was not marked release-shaped MSVC")
    if metadata.get("windowsMsvcHostPlatform") is not True:
        raise SystemExit("archive Windows runtime did not use the MSVC host-platform override")
PY
  test -d "${archive_runtime_dir}/codex-resources"
  test -d "${archive_runtime_dir}/codex-path"
  case "$(uname -s)" in
  MINGW64_NT*|MSYS_NT*)
    test -x "${archive_runtime_dir}/codex-path/rg.exe"
    test -x "${archive_runtime_dir}/codex-resources/codex-windows-sandbox-setup.exe"
    test -x "${archive_runtime_dir}/codex-resources/codex-command-runner.exe"
    ;;
  *)
    test -x "${archive_runtime_dir}/codex-path/rg"
    test -x "${archive_runtime_dir}/codex-resources/zsh/bin/zsh"
    if [[ "$(uname -s)" == "Linux" ]]; then
      test -x "${archive_runtime_dir}/codex-resources/bwrap"
      .github/scripts/stage-codex-runtime.sh --verify-sandbox --exec-path "${CODEX_EXEC_PATH}"
    fi
    ;;
  esac
  unset CODEX_HOME CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS
  export CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1
  (cd sdk/go && for test_name in \
    TestRealAppServerInitializeStrictDigest \
    TestRealAppServerRejectsDebugHookEnv \
    TestRealAppServerThreadRunHappyPath \
    TestRealAppServerCommandExecStreaming \
    TestRealAppServerProcessLifecycle \
    TestRealAppServerFilesystemWatch
  do
    "${run_prefix[@]}" go test ./... -list "^${test_name}$" | grep -Fx "${test_name}"
    "${run_prefix[@]}" go test ./... -run "^${test_name}$"
  done)
done
```

- [ ] On native Windows PowerShell, do not claim local packageArchive release-readiness until the PowerShell archive path is implemented. Record the explicit fail-closed blocker for exactly one matching MSVC target on the current runner; Windows packageArchive release-readiness must come from the downloaded SDK CI and shipping workflow artifacts in the later GitHub Actions evidence gate:

```powershell
# Run from the native Windows checkout root for this worktree.
$repo = (Get-Location).Path
Set-Location $repo
if (-not $env:CODEX_GO_SDK_ARCHIVE_TARGET) { throw "set CODEX_GO_SDK_ARCHIVE_TARGET to exactly one Windows MSVC target for this runner" }
$expectedBazelTarget = $env:CODEX_GO_SDK_ARCHIVE_TARGET
if ($expectedBazelTarget -eq "x86_64-pc-windows-msvc") {
  if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64" -and $env:PROCESSOR_ARCHITEW6432 -ne "AMD64") { throw "x86_64-pc-windows-msvc requires a native x64 Windows runner" }
} elseif ($expectedBazelTarget -eq "aarch64-pc-windows-msvc") {
  if ($env:PROCESSOR_ARCHITECTURE -ne "ARM64" -and $env:PROCESSOR_ARCHITEW6432 -ne "ARM64") { throw "aarch64-pc-windows-msvc requires a native ARM64 Windows runner" }
} else {
  throw "unsupported CODEX_GO_SDK_ARCHIVE_TARGET=$expectedBazelTarget"
}
$zstdArgs = @()
if ($env:CODEX_GO_SDK_ZSTD_SOURCE) {
  if (-not (Test-Path $env:CODEX_GO_SDK_ZSTD_SOURCE)) { throw "CODEX_GO_SDK_ZSTD_SOURCE must point at the Stage 5G materialized zstd executable" }
  $zstdArgs = @("-ZstdSource", $env:CODEX_GO_SDK_ZSTD_SOURCE)
} elseif (-not (Get-Command zstd -ErrorAction SilentlyContinue)) {
  throw "zstd must be installed unless CODEX_GO_SDK_ZSTD_SOURCE points at the Stage 5G no-network source; DotSlash fallback is not accepted"
}
$archiveRuntimeDir = Join-Path $env:TEMP ("codex-go-sdk-release-archive-blocked-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force $archiveRuntimeDir | Out-Null
try {
  & .github\scripts\stage-codex-runtime.ps1 -Out $archiveRuntimeDir -BazelTarget $expectedBazelTarget -CargoProfile release -ReleasePackageArchive @zstdArgs -WindowsReleaseShapedMsvc -ExportEnvironment | Invoke-Expression
  throw "native Windows packageArchive verifier unexpectedly succeeded; update this Stage 7 gate to validate implemented packageArchive evidence before using it"
} catch {
  if ($_.Exception.Message -notmatch "packageArchive staging is blocked until the native Windows app-server package archive lane is implemented") {
    throw
  }
}
```

- [ ] On Linux/macOS, or on Windows Git Bash only when the same Git Bash session obtains its environment from `.github/scripts/stage-codex-runtime.sh --print-shell-env` or is launched from a PowerShell process after `stage-codex-runtime.ps1 -ExportEnvironment | Invoke-Expression`, run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
cargo nextest --version
just test -p codex-app-server-protocol
just fmt-check
cd ..
bazel version
bazel build //codex-rs/cli:codex_go_sdk_runtime_layout
bazel test //codex-rs/app-server-protocol:app-server-protocol
```

- [ ] On native Windows PowerShell, do not use the bare Bazel commands above until the same local bootstrap contract as `.github/actions/setup-bazel-ci` has been applied. Run through the Stage 6 Windows-local script family or a shared bootstrap script extracted from it. Any Windows Stage 7 runtime/bootstrap staging that supports a release-readiness conclusion must use the release-shaped MSVC lane:

```powershell
# Run from the native Windows checkout root for this worktree.
$repo = (Get-Location).Path
Push-Location .\codex-rs
cargo nextest --version
just test -p codex-app-server-protocol
just fmt-check
Pop-Location
Set-Location $repo
$expectedBazelTarget = if ($env:CODEX_GO_SDK_WINDOWS_TARGET) { $env:CODEX_GO_SDK_WINDOWS_TARGET } else { "x86_64-pc-windows-msvc" }
$runtimeDir = Join-Path $env:TEMP ("codex-go-sdk-runtime-" + [guid]::NewGuid())
& .github\scripts\stage-codex-runtime.ps1 -Out $runtimeDir -BazelTarget $expectedBazelTarget -ExportEnvironment | Invoke-Expression
if (-not $env:CODEX_EXEC_PATH) { throw "CODEX_EXEC_PATH was not exported" }
$stagingMetadata = Get-Content (Join-Path $runtimeDir 'codex-go-sdk-runtime-staging.json') | ConvertFrom-Json
if ($stagingMetadata.runtimeSource -ne "bazelLayout") { throw "native Windows verifier did not stage Bazel layout runtime" }
if ($stagingMetadata.bazelTarget -ne $expectedBazelTarget) { throw "native Windows verifier staged the wrong MSVC target" }
bazel version
bazel test //codex-rs/app-server-protocol:app-server-protocol
```

Expected: `cargo-nextest` and Bazel are available; install repo-pinned `cargo-nextest` version `0.9.103` and the repo-supported Bazel/Bazelisk tool before continuing if either command is missing. Cargo/nextest and local Bazel both run the app-server-protocol manifest/digest tests on the current host, and `just fmt-check` proves formatting without mutating the final verification tree. The CI-only `.github/scripts/run-bazel-ci.sh` path is verified by Stage 6 workflow execution, not by local final verification unless the runner environment explicitly provides `RUNNER_OS`, Windows toolchain env, and `CODEX_BAZEL_WINDOWS_PATH`. Native Windows local verification must set short Bazel output/cache paths, materialize the Visual Studio/MSVC environment, compute the Bazel Windows PATH, and enable `core.longpaths` through the same contract as `setup-bazel-ci`/`stage-codex-runtime.ps1`; otherwise Windows Stage 7 is not accepted. The earlier native PowerShell packageArchive blocker is intentionally not release-readiness evidence and must be replaced by downloaded target-bound GitHub artifact evidence before any Windows packageArchive release claim. If any repair command such as `just fix` or `just fmt` is needed at this point, stop final verification, run the repair in the relevant stage, and restart Stage 7 from the beginning on the changed tree. Bazel runfiles/data drift or digest mismatch blocks final verification.

- [ ] If protocol/common/shared Rust changed and user approves full suite:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just test
```

### Task 7.2: Drift Verification

- [ ] Run stable and experimental schema drift checks without mutating checked-in fixtures:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just write-app-server-schema --check
cd ..
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
```

- [ ] Verify experimental schema fixtures in the Go SDK artifact root without mutating the stable app-server schema root:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
```

- [ ] Run Go generator check:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --check --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go run ./internal/cmd/protocodex --check --mode stable --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json
go run ./internal/cmd/protocodex --check --mode experimental --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json
go run ./internal/cmd/protocodex --check --mode both --stable-schema-root ../../codex-rs/app-server-protocol/schema --experimental-schema-root internal/protocodex/schema-experimental --manifest internal/protocodex/manifest/app_server_protocol_manifest.json --out protocol --root-out .
go test ./... -run 'TestResourceCoverage|TestResourceDocsCoverage|TestServerHandlerDocsCoverage'
go test ./... -run TestReleaseReadiness
```

- [ ] Re-run the Stage 5G package-source hermeticity owner tests before accepting release-readiness. Stage 5G owns these checks under `scripts/codex_package`; do not replace them with a `go test -run ...` placeholder unless matching Go tests are actually implemented and reviewed:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
python3 -m unittest scripts.codex_package.test_package_sources
python3 -m unittest discover scripts/codex_package
```

Expected: `scripts.codex_package.test_package_sources` proves strict package-source resolution for managed `rg`, zsh, Linux `bwrap`, Windows helpers, and archive `zstd`; release-workflow assertions reject ad hoc zsh `curl`, `facebook/install-dotslash`, package-assembly DotSlash resolution, `.github/workflows/zstd`, package-builder PATH fallback, missing helper producers, helper-root bypass, and bwrap digest drift; DotSlash parity proves `.github/dotslash-config.json` and `publish-dotslash` still match the shipping packageArchive names, helper artifact names, and in-archive paths; package-layout parity proves helper placement matches runtime lookup paths. The broader `discover` command must also pass so adjacent package builder tests remain green.

- [ ] Verify the non-publishing Go SDK release workflow exists and validates tag-shaped checkouts:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
test -f .github/workflows/go-sdk-release-readiness.yml
grep -F 'workflow_call:' .github/workflows/go-sdk-release-readiness.yml
grep -F 'checkout_ref:' .github/workflows/go-sdk-release-readiness.yml
grep -F 'validate_synthetic_tags:' .github/workflows/go-sdk-release-readiness.yml
grep -F "if: github.event_name == 'workflow_call' || github.repository == 'openai/codex' || github.event_name == 'workflow_dispatch'" .github/workflows/go-sdk-release-readiness.yml
grep -F 'workflow_dispatch:' .github/workflows/go-sdk-release-readiness.yml
grep -F 'sdk/go/v*' .github/workflows/go-sdk-release-readiness.yml
grep -F 'sdk/go/v1.99.0-go-sdk-ci' .github/workflows/go-sdk-release-readiness.yml
grep -F 'sdk/go/v1.99.1-go-sdk-ci-annotated' .github/workflows/go-sdk-release-readiness.yml
grep -F 'annotated_synthetic_tag="sdk/go/v1.99.1-go-sdk-ci-annotated"' .github/workflows/go-sdk-release-readiness.yml
grep -F -- '--git-dir="${bare_remote}"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'user.name=codex-go-sdk-ci' .github/workflows/go-sdk-release-readiness.yml
grep -F 'user.email=codex-go-sdk-ci@example.invalid' .github/workflows/go-sdk-release-readiness.yml
grep -F 'tag -a "${release_tag}" "${synthetic_tag_refs[$release_tag]}" -m "synthetic Go SDK annotated tag"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'rev-parse "${release_tag}^{commit}"' .github/workflows/go-sdk-release-readiness.yml
! grep -F 'git tag -a "sdk/go/v1.99.1-go-sdk-ci-annotated"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'sdk/go/v2.99.0-go-sdk-ci' .github/workflows/go-sdk-release-readiness.yml
grep -F 'sdk/go/v0.99.0-go-sdk-ci' .github/workflows/go-sdk-release-readiness.yml
grep -F 'go test ./...' .github/workflows/go-sdk-release-readiness.yml
grep -F 'github.com/openai/codex/sdk/go/v2' .github/workflows/go-sdk-release-readiness.yml
grep -F 'ref: ${{ inputs.checkout_ref || github.ref }}' .github/workflows/go-sdk-release-readiness.yml
! grep -F 'ref: ${{ inputs.checkout_ref || github.sha }}' .github/workflows/go-sdk-release-readiness.yml
grep -F 'git rev-parse "${GITHUB_REF}^{commit}"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'checkout_ref: ${{ github.event_name == '\''pull_request'\'' && github.event.pull_request.head.sha || github.sha }}' .github/workflows/sdk.yml
grep -F 'validate_synthetic_tags: true' .github/workflows/sdk.yml
grep -F 'module_version="$(git rev-parse HEAD)"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'module_import="$(awk '\''/^module / { print $2; exit }'\'' sdk/go/go.mod)"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'grep -Fx "module ${module_import}" sdk/go/go.mod' .github/workflows/go-sdk-release-readiness.yml
grep -F 'major_number="${major#v}"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'export GIT_CONFIG_GLOBAL="${git_config_global}"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'trace_dir="$(mktemp -d "${RUNNER_TEMP}/codex-go-sdk-release-trace.XXXXXX")"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'git config --global --add url."file://${bare_remote}".insteadOf "${rewrite}"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'git config --global --get-all url."file://${bare_remote}".insteadOf' .github/workflows/go-sdk-release-readiness.yml
grep -F 'go get "${module_import}@${module_version}"' .github/workflows/go-sdk-release-readiness.yml
grep -F '_ "${module_import}/protocol"' .github/workflows/go-sdk-release-readiness.yml
grep -F '_ "${release_import}/protocol"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'go -C sdk/go test ./...' .github/workflows/go-sdk-release-readiness.yml
grep -F 'grep -F "${bare_remote}" "${go_get_trace}"' .github/workflows/go-sdk-release-readiness.yml
grep -F 'uses: ./.github/actions/check-clean-worktree' .github/workflows/go-sdk-release-readiness.yml
! grep -F 'go mod edit -replace github.com/openai/codex/sdk/go' .github/workflows/go-sdk-release-readiness.yml
grep -F 'uses: ./.github/workflows/go-sdk-release-readiness.yml' .github/workflows/sdk.yml
grep -F 'bazel_target: x86_64-pc-windows-msvc' .github/workflows/sdk.yml
grep -F 'codex_go_sdk_runtime_layout' codex-rs/cli/BUILD.bazel
grep -F -- '--windows-release-shaped-msvc' .github/scripts/stage-codex-runtime.sh
grep -F -- '--windows-msvc-host-platform' .github/scripts/stage-codex-runtime.sh
grep -F -- '--windows-msvc-host-platform' .github/workflows/sdk.yml
grep -F 'codex-go-sdk-runtime-staging.json' .github/scripts/stage-codex-runtime.sh
grep -F 'codex-go-sdk-runtime-staging.json' .github/scripts/stage-codex-runtime.ps1
grep -F 'BazelTarget' .github/scripts/stage-codex-runtime.ps1
grep -F 'ReleasePackageArchive' .github/scripts/stage-codex-runtime.ps1
grep -F -- '--release-package-archive' .github/scripts/stage-codex-runtime.sh
grep -F -- '--release-package-archive' .github/workflows/sdk.yml
grep -F 'packageArchive' .github/scripts/stage-codex-runtime.sh
grep -F 'packageArchive' .github/scripts/stage-codex-runtime.ps1
grep -F 'runtimeSource' .github/scripts/stage-codex-runtime.sh
grep -F 'cargoProfile' .github/scripts/stage-codex-runtime.sh
grep -F 'zstdSource' .github/scripts/stage-codex-runtime.sh
grep -F 'archiveFormats' .github/scripts/stage-codex-runtime.sh
grep -F -- '--zstd-source' .github/scripts/stage-codex-runtime.sh
grep -F 'ZstdSource' .github/scripts/stage-codex-runtime.ps1
grep -F 'CODEX_GO_SDK_ZSTD_SOURCE' .github/workflows/sdk.yml
grep -F 'command -v zstd' .github/scripts/stage-codex-runtime.sh
grep -F 'Get-Command zstd' .github/scripts/stage-codex-runtime.ps1
grep -F 'command -v zstd' .github/workflows/sdk.yml
! grep -F '.github/workflows/zstd' .github/scripts/stage-codex-runtime.sh
! grep -F '.github/workflows/zstd' .github/scripts/stage-codex-runtime.ps1
if awk '
  /- name: Build Codex package archive/ { in_archive = 1 }
  in_archive && /- name: Build Python runtime wheel/ { in_archive = 0 }
  in_archive && /\.github\/workflows\/zstd/ { found = 1 }
  END { exit found ? 0 : 1 }
' .github/workflows/rust-release.yml; then
  echo "rust-release.yml package archive assembly must not invoke .github/workflows/zstd" >&2
  exit 1
fi
if awk '
  /- name: Build Codex package archives/ { in_archive = 1 }
  in_archive && /- name: Build Python runtime wheel/ { in_archive = 0 }
  in_archive && /\.github\/workflows\/zstd/ { found = 1 }
  END { exit found ? 0 : 1 }
' .github/workflows/rust-release-windows.yml; then
  echo "rust-release-windows.yml package archive assembly must not invoke .github/workflows/zstd" >&2
  exit 1
fi
if awk '
  /- name: Download packaged zsh manifest/ { in_zsh_download = 1 }
  in_zsh_download && /- name: Build Codex package archive/ { in_zsh_download = 0 }
  in_zsh_download && /curl -fsSL/ { found = 1 }
  END { exit found ? 0 : 1 }
' .github/workflows/rust-release.yml; then
  echo "rust-release.yml must not download codex-zsh with ad hoc curl during package assembly" >&2
  exit 1
fi
! grep -F 'facebook/install-dotslash' .github/workflows/rust-release-windows.yml
! grep -F 'falling back to single-binary zip' .github/workflows/rust-release-windows.yml
! grep -F 'warning: missing sandbox binaries' .github/workflows/rust-release-windows.yml
grep -F 'codex-command-runner.exe' .github/workflows/rust-release-windows.yml
grep -F 'codex-windows-sandbox-setup.exe' .github/workflows/rust-release-windows.yml
grep -F 'publish-dotslash:' .github/workflows/rust-release.yml
grep -F 'config: .github/dotslash-config.json' .github/workflows/rust-release.yml
grep -F 'def test_dotslash_release_archive_config_parity' scripts/codex_package/test_package_sources.py
grep -F 'go-sdk-shipping-release-readiness' .github/workflows/go-sdk-shipping-release-readiness.yml
grep -F '.github/workflows/rust-release.yml' .github/workflows/go-sdk-shipping-release-readiness.yml
grep -F '.github/workflows/rust-release-windows.yml' .github/workflows/go-sdk-shipping-release-readiness.yml
grep -F 'shipping-release-readiness-metadata' .github/workflows/go-sdk-shipping-release-readiness.yml
grep -F 'shipping-release-readiness.json' .github/workflows/go-sdk-shipping-release-readiness.yml
grep -E 'reusedWorkflows|reusedJobs|reusedScripts|workflowLocalDuplicateCommands' .github/workflows/go-sdk-shipping-release-readiness.yml
grep -E 'package-macos|finalize-macos|Build Codex package archive|Build Codex package archives|publish-dotslash|dotslash-config|codex-command-runner.exe|codex-windows-sandbox-setup.exe' .github/workflows/go-sdk-shipping-release-readiness.yml
awk '
  /- name: Test Go SDK release archive runtime/ { in_archive = 1 }
  in_archive && /- name: Rust protocol\/schema source-of-truth checks/ { in_archive = 0 }
  in_archive && /--cargo-profile release/ { has_release = 1 }
  in_archive && /--cargo-profile dev/ { has_dev = 1 }
  END { exit (has_release && !has_dev) ? 0 : 1 }
' .github/workflows/sdk.yml
grep -F 'Test Go SDK release archive runtime' .github/workflows/sdk.yml
awk '
  /- name: Test Go SDK release archive runtime/ { in_archive = 1 }
  in_archive && /- name: Rust protocol\/schema source-of-truth checks/ { in_archive = 0 }
  in_archive && /TestRealAppServerRejectsDebugHookEnv/ { found = 1 }
  END { exit found ? 0 : 1 }
' .github/workflows/sdk.yml
if awk '
  /- name: Test Go SDK release archive runtime/ { in_archive = 1 }
  in_archive && /- name: Rust protocol\/schema source-of-truth checks/ { in_archive = 0 }
  in_archive && /runner\.os != .macOS./ { found = 1 }
  END { exit found ? 0 : 1 }
' .github/workflows/sdk.yml; then
  echo "Go SDK packageArchive smoke must not skip macOS" >&2
  exit 1
fi
awk '
  /bazel_target: aarch64-apple-darwin/ { has_macos_arm64 = 1 }
  /bazel_target: x86_64-apple-darwin/ { has_macos_x64 = 1 }
  /- name: Test Go SDK release archive runtime/ { has_archive_step = 1 }
  END { exit (has_macos_arm64 && has_macos_x64 && has_archive_step) ? 0 : 1 }
' .github/workflows/sdk.yml
grep -F 'go-sdk-release-archive' .github/workflows/sdk.yml
grep -F 'Assert macOS x64 runner architecture' .github/workflows/sdk.yml
grep -F 'linux-x64-xl' .github/workflows/sdk.yml
grep -F 'linux-x64-xl' .github/workflows/rust-release.yml
grep -F 'runs_on: macos-15-large' .github/workflows/sdk.yml
grep -F 'test "$(uname -m)" = "x86_64"' .github/workflows/sdk.yml
grep -F 'macos-15-large' .github/workflows/rust-release.yml
grep -R -n 'TestMacosReleaseWorkflowRunnerShape' sdk/go .github/scripts .github/workflows
grep -R -n 'TestWindowsReleaseZipIncludesSandboxHelpers' sdk/go .github/scripts .github/workflows
if awk '
  /runner:|runs-on:/ { runner = $0 }
  /arch -x86_64/ { saw_arch = 1 }
  /target: x86_64-apple-darwin/ && runner ~ /macos-15-xlarge/ { found_xlarge = 1 }
  END { exit (found_xlarge && !saw_arch) ? 0 : 1 }
' .github/workflows/rust-release.yml; then
  echo "rust-release.yml must not attach x86_64-apple-darwin release rows to macos-15-xlarge without the Stage 5G source-parsing test proving an explicit x86_64 execution path" >&2
  exit 1
fi
if ! grep -E "package-macos:|finalize-macos:|matrix\\.target.*x86_64-apple-darwin|target: x86_64-apple-darwin|arch -x86_64" .github/workflows/rust-release.yml; then
  echo "rust-release.yml macOS build/sign/package/finalize jobs must be covered by TestMacosReleaseWorkflowRunnerShape" >&2
  exit 1
fi
grep -F 'WindowsReleaseShapedMsvc' .github/scripts/stage-codex-runtime.ps1
grep -F 'BootstrapOnly' .github/scripts/stage-codex-runtime.ps1
grep -F 'windowsMsvcHostPlatform' .github/scripts/stage-codex-runtime.ps1
grep -F 'x86_64-unknown-linux-musl' .github/workflows/sdk.yml
if awk '
  /- name: Test Go SDK release archive runtime/ { in_archive = 1 }
  in_archive && /- name: Rust protocol\/schema source-of-truth checks/ { in_archive = 0 }
  in_archive && /x86_64-unknown-linux-gnu/ { found = 1 }
  END { exit found ? 0 : 1 }
' .github/workflows/sdk.yml; then
  echo "Go SDK packageArchive smoke must not use the generic Linux GNU Bazel cache target" >&2
  exit 1
fi
grep -F 'aarch64-unknown-linux-musl' .github/workflows/sdk.yml
grep -F "runner.os == 'Linux'" .github/workflows/sdk.yml
! grep -F "matrix.name == 'linux'" .github/workflows/sdk.yml
grep -F 'aarch64-apple-darwin' .github/workflows/sdk.yml
grep -F 'x86_64-apple-darwin' .github/workflows/sdk.yml
grep -F 'x86_64-pc-windows-msvc' .github/workflows/sdk.yml
grep -F 'aarch64-pc-windows-msvc' .github/workflows/sdk.yml
grep -F '//codex-rs/cli:codex_go_sdk_runtime_layout' .github/scripts/stage-codex-runtime.sh
grep -F 'codex-resources/bwrap' .github/scripts/stage-codex-runtime.sh
grep -F 'codex-path/$(rg_name)' .github/scripts/stage-codex-runtime.sh
grep -F 'codex-resources/zsh/bin/zsh' .github/scripts/stage-codex-runtime.sh
grep -F -- '-ExportEnvironment' .github/scripts/stage-codex-runtime.ps1
grep -F 'Set-Item -Path Env:' .github/scripts/stage-codex-runtime.ps1
if grep -F 'CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE=1' .github/workflows/sdk.yml; then
  echo "workflow must not export SDK integration test mode at parent job scope" >&2
  exit 1
fi
grep -F 'reserved env leaked into child env' sdk/go/config_test.go
grep -F 'parent debug hook env was scrubbed' sdk/go/real_app_server_test.go
```

- [ ] Verify real GitHub Actions release-readiness evidence. This gate is mandatory after local source checks; local `grep`/`awk`/unit tests are not enough to claim Linux/macOS/Windows release readiness. The executor must provide run IDs for successful runs on the reviewed commit:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
: "${CODEX_GO_SDK_CI_RUN_ID:?set to the successful sdk.yml run id for the reviewed commit}"
: "${CODEX_GO_SDK_RELEASE_READINESS_RUN_ID:?set to the successful go-sdk-release-readiness.yml run id for the reviewed commit}"
: "${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID:?set to the successful shipping release-readiness run id for the reviewed commit}"
expected_sha="$(git rev-parse HEAD)"
verify_run() {
  run_id="$1"
  expected_workflow="$2"
  json_path=".verification/github-actions-${run_id}.json"
  log_path=".verification/github-actions-${run_id}.log"
  mkdir -p .verification
  gh run view "${run_id}" --json headSha,status,conclusion,workflowName,event,jobs >"${json_path}"
  gh run view "${run_id}" --log >"${log_path}"
  python3 - "${json_path}" "${expected_sha}" "${expected_workflow}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    run = json.load(fh)
if run.get("headSha") != sys.argv[2]:
    raise SystemExit(f"run {sys.argv[1]} is for {run.get('headSha')} not reviewed commit {sys.argv[2]}")
if run.get("workflowName") != sys.argv[3]:
    raise SystemExit(f"run {sys.argv[1]} workflow {run.get('workflowName')} != {sys.argv[3]}")
if run.get("status") != "completed" or run.get("conclusion") != "success":
    raise SystemExit(f"run {sys.argv[1]} did not complete successfully")
jobs = run.get("jobs") or []
failed = [job for job in jobs if job.get("conclusion") not in {None, "success", "skipped"}]
if failed:
    raise SystemExit(f"run {sys.argv[1]} has failed jobs: {failed}")
PY
}
verify_run "${CODEX_GO_SDK_CI_RUN_ID}" "sdk"
verify_run "${CODEX_GO_SDK_RELEASE_READINESS_RUN_ID}" "go-sdk-release-readiness"
verify_run "${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}" "go-sdk-shipping-release-readiness"
sdk_artifacts_dir=".verification/sdk-ci-release-artifacts"
rm -rf "${sdk_artifacts_dir}"
mkdir -p "${sdk_artifacts_dir}"
gh run download "${CODEX_GO_SDK_CI_RUN_ID}" --name go-sdk-ci-release-evidence --dir "${sdk_artifacts_dir}"
sdk_metadata="${sdk_artifacts_dir}/go-sdk-ci-release-evidence/go-sdk-ci-release-evidence.json"
if ! test -f "${sdk_metadata}"; then
  sdk_metadata="${sdk_artifacts_dir}/go-sdk-ci-release-evidence.json"
fi
test -f "${sdk_metadata}"
python3 - "${sdk_metadata}" ".verification/github-actions-${CODEX_GO_SDK_CI_RUN_ID}.json" <<'PY'
import json
import sys
from pathlib import Path

expected_targets = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]
required_smokes = {
    "TestRealAppServerInitializeStrictDigest",
    "TestRealAppServerRejectsDebugHookEnv",
    "TestRealAppServerThreadRunHappyPath",
    "TestRealAppServerCommandExecStreaming",
    "TestRealAppServerProcessLifecycle",
    "TestRealAppServerFilesystemWatch",
}
metadata_path = Path(sys.argv[1])
artifact_dir = metadata_path.parent
with open(metadata_path, encoding="utf-8") as fh:
    metadata = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    run = json.load(fh)
jobs_by_name = {job.get("name"): job for job in (run.get("jobs") or []) if job.get("name")}

def require_job_success(job_name, label):
    job = jobs_by_name.get(job_name)
    if not job or job.get("conclusion") != "success":
        raise SystemExit(f"{label} did not succeed: {job_name!r}")

def require_artifact_file(relative_path, label):
    if not relative_path:
        raise SystemExit(f"{label} missing artifact file path")
    path = Path(relative_path)
    if path.is_absolute() or ".." in path.parts:
        raise SystemExit(f"{label} artifact path must stay inside go-sdk-ci-release-evidence")
    candidate = (artifact_dir / path).resolve()
    artifact_root = artifact_dir.resolve()
    try:
        candidate.relative_to(artifact_root)
    except ValueError as exc:
        raise SystemExit(f"{label} artifact path escapes the SDK CI evidence artifact") from exc
    if not candidate.is_file():
        raise SystemExit(f"{label} artifact file is missing: {relative_path}")
    return candidate

def require_artifact_text(relative_path, label):
    return require_artifact_file(relative_path, label).read_text(encoding="utf-8")
targets = metadata.get("targets") or {}
missing_targets = [target for target in expected_targets if target not in targets]
if missing_targets:
    raise SystemExit(f"sdk.yml evidence missing targets: {missing_targets}")
for target in expected_targets:
    target_metadata = targets[target]
    job_name = target_metadata.get("jobName") or target_metadata.get("packageArchiveJob")
    if not job_name:
        raise SystemExit(f"{target} SDK CI evidence missing jobName")
    job = jobs_by_name.get(job_name)
    if not job or job.get("conclusion") != "success":
        raise SystemExit(f"{target} SDK CI job did not succeed: {job_name!r}")
    if target_metadata.get("jobConclusion") != "success":
        raise SystemExit(f"{target} SDK CI evidence lacks successful jobConclusion")
    helper_root_evidence = target_metadata.get("helperRootEvidence") or {}
    if helper_root_evidence.get("source") == "producerModeMaterializeHelpers":
        raise SystemExit(f"{target} SDK CI helper root used producer-mode materialization and is not release-ready")
    if target_metadata.get("skipped") or target_metadata.get("notReleaseReady"):
        raise SystemExit(f"{target} SDK CI evidence was skipped or marked not release-ready")
    if target_metadata.get("bazelTarget") != target:
        raise SystemExit(f"{target} SDK CI evidence has wrong bazelTarget {target_metadata.get('bazelTarget')!r}")
    if target_metadata.get("runtimeSource") != "packageArchive":
        raise SystemExit(f"{target} SDK CI evidence did not use packageArchive runtimeSource")
    if target_metadata.get("cargoProfile") != "release":
        raise SystemExit(f"{target} SDK CI evidence did not use release cargoProfile")
    verifier = target_metadata.get("packageArchiveVerifier")
    if verifier not in {True, "success"}:
        raise SystemExit(f"{target} SDK CI evidence missing successful packageArchive verifier")
    if not target_metadata.get("runnerLabel"):
        raise SystemExit(f"{target} SDK CI evidence missing runnerLabel")
    staging = target_metadata.get("stagingMetadata") or {}
    if staging.get("runtimeSource") != "packageArchive" or staging.get("cargoProfile") != "release" or staging.get("bazelTarget") != target:
        raise SystemExit(f"{target} SDK CI evidence has incomplete staging metadata")
    if target == "x86_64-apple-darwin":
        arch_proof = target_metadata.get("architectureProof") or staging.get("architectureProof") or {}
        arch_proof_text = require_artifact_text(target_metadata.get("architectureProofPath"), "SDK CI macOS x64 architecture proof")
        label = target_metadata.get("runnerLabel") or ""
        command = arch_proof.get("command") or ""
        uname_machine = arch_proof.get("unameMachine")
        if "macos-15-large" not in label and "arch -x86_64" not in command:
            raise SystemExit("SDK CI macOS x64 lane lacks Intel runner label or Rosetta arch proof")
        if uname_machine != "x86_64" and "arch -x86_64" not in command:
            raise SystemExit("SDK CI macOS x64 lane lacks x86_64 runtime execution proof")
        if "x86_64" not in arch_proof_text and "arch -x86_64" not in arch_proof_text:
            raise SystemExit("SDK CI macOS x64 downloaded architecture proof lacks x86_64 evidence")
    if "windows" in target:
        if staging.get("windowsReleaseShapedMsvc") is not True or staging.get("windowsMsvcHostPlatform") is not True:
            raise SystemExit(f"{target} SDK CI evidence lacks Windows MSVC release-shaped host proof")
        windows_proof_text = require_artifact_text(target_metadata.get("windowsHostProofPath"), f"{target} SDK CI Windows host proof")
        for needle in [target, "windowsReleaseShapedMsvc=true", "windowsMsvcHostPlatform=true"]:
            if needle not in windows_proof_text:
                raise SystemExit(f"{target} SDK CI downloaded Windows host proof missing {needle}")
    smoke_tests = set(target_metadata.get("packageArchiveSmokeTests") or [])
    missing_smokes = sorted(required_smokes - smoke_tests)
    if missing_smokes:
        raise SystemExit(f"{target} SDK CI packageArchive smoke suite missing {missing_smokes}")
    bounded_logs = target_metadata.get("boundedLogs") or []
    if not bounded_logs:
        raise SystemExit(f"{target} SDK CI evidence missing bounded log references")
    for index, log in enumerate(bounded_logs):
        relative_path = Path(log.get("path") or "")
        if not log.get("maxBytes") or relative_path.is_absolute() or ".." in relative_path.parts:
            raise SystemExit(f"{target} SDK CI boundedLogs[{index}] has invalid path or maxBytes")
        log_path = (artifact_dir / relative_path).resolve()
        artifact_root = artifact_dir.resolve()
        try:
            log_path.relative_to(artifact_root)
        except ValueError as exc:
            raise SystemExit(f"{target} SDK CI boundedLogs[{index}] escapes the evidence artifact") from exc
        if not log_path.is_file():
            raise SystemExit(f"{target} SDK CI bounded log file is missing: {relative_path}")
        if log_path.stat().st_size > int(log["maxBytes"]):
            raise SystemExit(f"{target} SDK CI bounded log exceeds maxBytes: {relative_path}")
PY
shipping_artifacts_dir=".verification/shipping-release-artifacts"
rm -rf "${shipping_artifacts_dir}"
mkdir -p "${shipping_artifacts_dir}"
gh run download "${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}" --dir "${shipping_artifacts_dir}"
shipping_metadata="${shipping_artifacts_dir}/shipping-release-readiness-metadata/shipping-release-readiness.json"
test -f "${shipping_metadata}"
python3 - "${shipping_metadata}" ".verification/github-actions-${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}.json" <<'PY'
import json
import sys
from pathlib import Path

expected_targets = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]
required_smokes = {
    "TestRealAppServerInitializeStrictDigest",
    "TestRealAppServerRejectsDebugHookEnv",
    "TestRealAppServerThreadRunHappyPath",
    "TestRealAppServerCommandExecStreaming",
    "TestRealAppServerProcessLifecycle",
    "TestRealAppServerFilesystemWatch",
}
expected_dotslash_entries = {
    "codex",
    "codex-app-server",
    "codex-responses-api-proxy",
    "bwrap",
    "codex-command-runner",
    "codex-windows-sandbox-setup",
}
metadata_path = Path(sys.argv[1])
artifact_dir = metadata_path.parent
with open(metadata_path, encoding="utf-8") as fh:
    metadata = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    run = json.load(fh)
jobs_by_name = {job.get("name"): job for job in (run.get("jobs") or []) if job.get("name")}

def require_job_success(job_name, label):
    job = jobs_by_name.get(job_name)
    if not job or job.get("conclusion") != "success":
        raise SystemExit(f"{label} did not succeed: {job_name!r}")

def require_artifact_file(relative_path, label):
    if not relative_path:
        raise SystemExit(f"{label} missing artifact file path")
    path = Path(relative_path)
    if path.is_absolute() or ".." in path.parts:
        raise SystemExit(f"{label} artifact path must stay inside shipping-release-readiness-metadata")
    candidate = (artifact_dir / path).resolve()
    artifact_root = artifact_dir.resolve()
    try:
        candidate.relative_to(artifact_root)
    except ValueError as exc:
        raise SystemExit(f"{label} artifact path escapes the metadata artifact") from exc
    if not candidate.is_file():
        raise SystemExit(f"{label} artifact file is missing: {relative_path}")
    return candidate

def require_artifact_text(relative_path, label):
    return require_artifact_file(relative_path, label).read_text(encoding="utf-8")

shape = metadata.get("workflowShape")
if shape != "thinWrapper":
    raise SystemExit(f"shipping workflow shape must be thinWrapper from go-sdk-shipping-release-readiness.yml, got {shape!r}")
workflows = set(metadata.get("reusedWorkflows") or [])
for workflow in [".github/workflows/rust-release.yml", ".github/workflows/rust-release-windows.yml"]:
    if workflow not in workflows:
        raise SystemExit(f"shipping metadata missing reused workflow {workflow}")
workflow_reuse_proof = require_artifact_text(metadata.get("workflowReuseProofPath"), "shipping workflow reuse proof")
for workflow in [".github/workflows/rust-release.yml", ".github/workflows/rust-release-windows.yml"]:
    if workflow not in workflow_reuse_proof:
        raise SystemExit(f"shipping workflow reuse proof missing {workflow}")
if metadata.get("workflowLocalDuplicateCommands"):
    raise SystemExit("shipping readiness used workflow-local duplicate release commands")
duplicate_audit = require_artifact_text(metadata.get("duplicateCommandAuditPath"), "shipping duplicate command audit")
if "workflowLocalDuplicateCommands=false" not in duplicate_audit and "no workflow-local duplicate commands" not in duplicate_audit:
    raise SystemExit("shipping duplicate command audit does not prove no workflow-local duplicate commands")
if "fixtureSubstitutions" not in metadata:
    raise SystemExit("shipping metadata missing explicit fixtureSubstitutions list")
fixture_substitutions = metadata.get("fixtureSubstitutions")
if not isinstance(fixture_substitutions, list):
    raise SystemExit("shipping metadata fixtureSubstitutions must be a list")
fixture_mode = bool(fixture_substitutions)
evidence_kind = metadata.get("evidenceKind")
release_impact = str(metadata.get("releaseReadinessImpact") or "")
if fixture_mode:
    if metadata.get("notReleaseReady") is not True:
        raise SystemExit("non-publishing fixture evidence must keep top-level notReleaseReady=true")
    if metadata.get("artifactEvidenceShapeComplete") is not True:
        raise SystemExit("non-publishing fixture evidence must only claim artifact evidence shape completeness")
    if evidence_kind != "nonPublishingFixtureEvidence":
        raise SystemExit("non-publishing fixture evidence must use evidenceKind=nonPublishingFixtureEvidence")
    if "fixture" not in release_impact.lower() or "blocked" not in release_impact.lower():
        raise SystemExit("non-publishing fixture evidence must say final release readiness remains blocked")
else:
    if metadata.get("notReleaseReady") is True:
        raise SystemExit("real shipping evidence must not keep the top-level notReleaseReady marker")
    if metadata.get("artifactEvidenceShapeComplete") is not True:
        raise SystemExit("real shipping evidence must prove complete artifact evidence shape")
    if evidence_kind != "releaseArtifactEvidence":
        raise SystemExit("real shipping evidence must use evidenceKind=releaseArtifactEvidence")
for index, substitution in enumerate(fixture_substitutions):
    if not isinstance(substitution, dict):
        raise SystemExit(f"fixtureSubstitutions[{index}] is not an object")
    missing = [
        key for key in ["name", "replaces", "reason", "reviewEvidence", "releaseReadinessImpact"]
        if not substitution.get(key)
    ]
    if missing:
        raise SystemExit(f"fixtureSubstitutions[{index}] missing required fields: {missing}")
bounded_logs = metadata.get("boundedLogs")
if not isinstance(bounded_logs, list) or not bounded_logs:
    raise SystemExit("shipping metadata missing boundedLogs entries")
for index, log in enumerate(bounded_logs):
    if not isinstance(log, dict):
        raise SystemExit(f"boundedLogs[{index}] is not an object")
    missing = [
        key for key in ["path", "maxBytes", "redaction", "retention"]
        if not log.get(key)
    ]
    if missing:
        raise SystemExit(f"boundedLogs[{index}] missing required fields: {missing}")
    relative_path = Path(log["path"])
    if relative_path.is_absolute() or ".." in relative_path.parts:
        raise SystemExit(f"boundedLogs[{index}] path must stay inside shipping-release-readiness-metadata")
    log_path = (artifact_dir / relative_path).resolve()
    artifact_root = artifact_dir.resolve()
    try:
        log_path.relative_to(artifact_root)
    except ValueError as exc:
        raise SystemExit(f"boundedLogs[{index}] escapes the metadata artifact") from exc
    if not log_path.is_file():
        raise SystemExit(f"boundedLogs[{index}] file is missing: {relative_path}")
    max_bytes = int(log["maxBytes"])
    if max_bytes <= 0:
        raise SystemExit(f"boundedLogs[{index}] maxBytes must be positive")
    if log_path.stat().st_size > max_bytes:
        raise SystemExit(f"boundedLogs[{index}] exceeds maxBytes: {relative_path}")
reused_text = "\n".join((metadata.get("reusedJobs") or []) + (metadata.get("reusedScripts") or []))
for needle in ["package-macos", "finalize-macos", "publish-dotslash", "build-codex-package-archive", "rust-release-windows"]:
    if needle not in reused_text:
        raise SystemExit(f"shipping metadata missing reused release job/script marker {needle}")
    if needle not in workflow_reuse_proof:
        raise SystemExit(f"shipping workflow reuse proof missing release marker {needle}")

targets = metadata.get("targets") or {}
missing_targets = [target for target in expected_targets if target not in targets]
if missing_targets:
    raise SystemExit(f"shipping metadata missing targets: {missing_targets}")
required_provenance_source = "shippingReadinessWrapper" if fixture_mode else "realReleaseWorkflow"
for target in expected_targets:
    target_metadata = targets[target]
    job_name = target_metadata.get("jobName") or target_metadata.get("packageArchiveJob")
    if not job_name:
        raise SystemExit(f"{target} metadata missing target job name")
    require_job_success(job_name, f"{target} shipping target job")
    if target_metadata.get("jobConclusion") != "success":
        raise SystemExit(f"{target} shipping metadata lacks successful jobConclusion")
    if target_metadata.get("skipped") or target_metadata.get("notReleaseReady"):
        raise SystemExit(f"{target} was skipped or marked not release-ready")
    archive_name = target_metadata.get("archiveFilename") or ""
    if target not in archive_name or not archive_name.startswith("codex-package-"):
        raise SystemExit(f"{target} archive filename is not target-specific package archive: {archive_name!r}")
    if not archive_name.endswith(".tar.zst"):
        raise SystemExit(f"{target} packageArchive evidence must name the .tar.zst archive, got {archive_name!r}")
    checksum = target_metadata.get("packageArchiveChecksum") or {}
    if checksum.get("algorithm") != "sha256" or not checksum.get("value"):
        raise SystemExit(f"{target} metadata missing sha256 checksum for .tar.zst packageArchive")
    if not checksum.get("manifest"):
        raise SystemExit(f"{target} metadata missing public checksum manifest proof")
    checksum_manifest = require_artifact_text(checksum.get("manifestPath"), f"{target} public checksum manifest")
    if archive_name not in checksum_manifest or checksum.get("value") not in checksum_manifest:
        raise SystemExit(f"{target} public checksum manifest does not contain the package archive checksum")
    provenance = target_metadata.get("packageArchiveProvenance") or {}
    if provenance.get("source") != required_provenance_source:
        raise SystemExit(f"{target} metadata packageArchive provenance must be {required_provenance_source}")
    if target_metadata.get("packageArchiveChecksumException"):
        raise SystemExit(f"{target} has a .tar.zst checksum exception and is not release-ready")
    archive_paths = set(target_metadata.get("archivePaths") or [])
    archive_inventory = require_artifact_text(target_metadata.get("archiveInventoryPath"), f"{target} codex packageArchive inventory")
    exe_path = "bin/codex.exe" if "windows" in target else "bin/codex"
    for required_path in [exe_path, "codex-package.json"]:
        if required_path not in archive_paths:
            raise SystemExit(f"{target} archive is missing {required_path}")
        if required_path not in archive_inventory:
            raise SystemExit(f"{target} downloaded archive inventory is missing {required_path}")
    helper_paths = set()
    if "windows" in target:
        helper_paths.update({
            "codex-path/rg.exe",
            "codex-resources/codex-command-runner.exe",
            "codex-resources/codex-windows-sandbox-setup.exe",
        })
    else:
        helper_paths.update({
            "codex-path/rg",
            "codex-resources/zsh/bin/zsh",
        })
        if "linux" in target:
            helper_paths.add("codex-resources/bwrap")
    missing_helper_paths = sorted(helper_paths - archive_paths)
    if missing_helper_paths:
        raise SystemExit(f"{target} codex packageArchive missing runtime helper paths: {missing_helper_paths}")
    missing_inventory_helper_paths = sorted(path for path in helper_paths if path not in archive_inventory)
    if missing_inventory_helper_paths:
        raise SystemExit(f"{target} downloaded archive inventory missing runtime helper paths: {missing_inventory_helper_paths}")
    app_server_archive = target_metadata.get("appServerPackageArchive") or {}
    app_server_archive_name = app_server_archive.get("archiveFilename") or ""
    if target not in app_server_archive_name or not app_server_archive_name.startswith("codex-app-server-package-") or not app_server_archive_name.endswith(".tar.zst"):
        raise SystemExit(f"{target} app-server packageArchive evidence must name the target-specific .tar.zst archive, got {app_server_archive_name!r}")
    app_server_paths = set(app_server_archive.get("archivePaths") or [])
    app_server_inventory = require_artifact_text(app_server_archive.get("archiveInventoryPath"), f"{target} app-server packageArchive inventory")
    app_server_exe_path = "bin/codex-app-server.exe" if "windows" in target else "bin/codex-app-server"
    if app_server_exe_path not in app_server_paths:
        raise SystemExit(f"{target} app-server archive is missing {app_server_exe_path}")
    if app_server_exe_path not in app_server_inventory:
        raise SystemExit(f"{target} downloaded app-server archive inventory is missing {app_server_exe_path}")
    app_server_checksum = app_server_archive.get("checksum") or {}
    if app_server_checksum.get("algorithm") != "sha256" or not app_server_checksum.get("value"):
        raise SystemExit(f"{target} metadata missing sha256 checksum for app-server .tar.zst packageArchive")
    if not app_server_checksum.get("manifest"):
        raise SystemExit(f"{target} metadata missing app-server public checksum manifest proof")
    app_server_manifest = require_artifact_text(app_server_checksum.get("manifestPath"), f"{target} app-server public checksum manifest")
    if app_server_archive_name not in app_server_manifest or app_server_checksum.get("value") not in app_server_manifest:
        raise SystemExit(f"{target} public checksum manifest does not contain the app-server archive checksum")
    app_server_provenance = app_server_archive.get("provenance") or {}
    if app_server_provenance.get("source") != required_provenance_source:
        raise SystemExit(f"{target} metadata app-server packageArchive provenance must be {required_provenance_source}")
    if app_server_archive.get("checksumException"):
        raise SystemExit(f"{target} app-server packageArchive has a checksum exception and is not release-ready")
    if not target_metadata.get("runnerLabel"):
        raise SystemExit(f"{target} metadata missing runnerLabel")
    smoke_key = "expectedPackageArchiveSmokeTests" if fixture_mode else "packageArchiveSmokeTests"
    if fixture_mode:
        if target_metadata.get("fixtureOnly") is not True:
            raise SystemExit(f"{target} fixture shipping metadata must set fixtureOnly=true")
        if target_metadata.get("packageArchiveSmokeTestsRan") is not False:
            raise SystemExit(f"{target} fixture shipping metadata must not claim packageArchive smoke tests ran")
        if "packageArchiveSmokeTests" in target_metadata:
            raise SystemExit(f"{target} fixture shipping metadata must not publish packageArchiveSmokeTests as ran evidence")
    smoke_tests = set(target_metadata.get(smoke_key) or [])
    missing_smokes = sorted(required_smokes - smoke_tests)
    if missing_smokes:
        raise SystemExit(f"{target} packageArchive smoke suite expectation missing {missing_smokes}")
    if "apple-darwin" in target:
        for key in ["packageMacosJob", "finalizeMacosJob"]:
            job_name = target_metadata.get(key)
            if not job_name:
                raise SystemExit(f"{target} metadata missing {key}")
            require_job_success(job_name, f"{target} {key}")
        dmg_names = target_metadata.get("dmgArtifactNames") or []
        direct_names = target_metadata.get("directArtifactNames") or []
        if not dmg_names or not direct_names:
            raise SystemExit(f"{target} metadata missing DMG/direct artifact names")
        if target == "x86_64-apple-darwin":
            arch_proof = target_metadata.get("architectureProof") or {}
            label = target_metadata.get("runnerLabel") or ""
            command = arch_proof.get("command") or ""
            uname_machine = arch_proof.get("unameMachine")
            if "macos-15-large" not in label and "arch -x86_64" not in command:
                raise SystemExit("macOS x64 lane lacks Intel runner label or Rosetta arch proof")
            if uname_machine != "x86_64" and "arch -x86_64" not in command:
                raise SystemExit("macOS x64 lane lacks x86_64 runtime execution proof")
    if "windows" in target:
        zip_members = set(target_metadata.get("publishedZipMembers") or [])
        zip_inventory = require_artifact_text(target_metadata.get("publishedZipInventoryPath"), f"{target} published Windows zip inventory")
        for helper in ["codex-command-runner.exe", "codex-windows-sandbox-setup.exe"]:
            if helper not in zip_members:
                raise SystemExit(f"{target} published zip missing {helper}")
            if helper not in zip_inventory:
                raise SystemExit(f"{target} downloaded Windows zip inventory missing {helper}")
        if not target_metadata.get("publishedZipName"):
            raise SystemExit(f"{target} metadata missing publishedZipName")

dotslash = metadata.get("dotslash") or {}
if dotslash.get("configPath") != ".github/dotslash-config.json":
    raise SystemExit("DotSlash metadata did not use .github/dotslash-config.json")
if not fixture_mode:
    dotslash_provenance = dotslash.get("provenance") or {}
    if dotslash_provenance.get("source") != "realReleaseWorkflow":
        raise SystemExit("real shipping DotSlash metadata must come from realReleaseWorkflow provenance")
if not dotslash.get("publishDotslashJob"):
    raise SystemExit("DotSlash metadata missing publish-dotslash job proof")
require_job_success(dotslash.get("publishDotslashJob"), "DotSlash publish job")
if not dotslash.get("archiveParity"):
    raise SystemExit("DotSlash archive parity failed or was not recorded")
dotslash_report = require_artifact_text(dotslash.get("archiveParityReportPath"), "DotSlash archive parity report")
dotslash_entries = set(dotslash.get("matchedEntries") or dotslash.get("entries") or [])
missing_dotslash_entries = sorted(expected_dotslash_entries - dotslash_entries)
if missing_dotslash_entries:
    raise SystemExit(f"DotSlash metadata missing published entries: {missing_dotslash_entries}")
for entry in expected_dotslash_entries:
    if entry not in dotslash_report:
        raise SystemExit(f"DotSlash archive parity report missing entry {entry}")
dotslash_targets = set(dotslash.get("matchedTargets") or [])
missing_dotslash = [target for target in expected_targets if target not in dotslash_targets]
if missing_dotslash:
    raise SystemExit(f"DotSlash metadata missing matched targets: {missing_dotslash}")
for target in expected_targets:
    if target not in dotslash_report:
        raise SystemExit(f"DotSlash archive parity report missing target {target}")
PY
for target in \
  x86_64-unknown-linux-musl \
  aarch64-unknown-linux-musl \
  aarch64-apple-darwin \
  x86_64-apple-darwin \
  x86_64-pc-windows-msvc \
  aarch64-pc-windows-msvc
do
  grep -R "${target}" .verification/github-actions-"${CODEX_GO_SDK_CI_RUN_ID}".log
done
grep -R 'runtimeSource.*packageArchive\|packageArchive' .verification/github-actions-"${CODEX_GO_SDK_CI_RUN_ID}".log
grep -R 'TestRealAppServerInitializeStrictDigest' .verification/github-actions-"${CODEX_GO_SDK_CI_RUN_ID}".log
grep -R 'TestRealAppServerRejectsDebugHookEnv' .verification/github-actions-"${CODEX_GO_SDK_CI_RUN_ID}".log
grep -R 'TestReleaseReadiness' .verification/github-actions-"${CODEX_GO_SDK_RELEASE_READINESS_RUN_ID}".log
grep -R 'package-macos\|finalize-macos\|DMG\|dmg' .verification/github-actions-"${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}".log
grep -R 'codex-command-runner.exe\|codex-windows-sandbox-setup.exe' .verification/github-actions-"${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}".log
grep -R 'sha256\|SHA256\|tar.zst\|codex-package_SHA256SUMS' .verification/github-actions-"${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}".log
grep -R 'publish-dotslash\|dotslash-config.json\|test_dotslash_release_archive_config_parity\|codex-responses-api-proxy\|bwrap\|codex-command-runner\|codex-windows-sandbox-setup' .verification/github-actions-"${CODEX_GO_SDK_SHIPPING_RELEASE_RUN_ID}".log
rm -rf .verification
```

Expected: all three run IDs point to successful completed runs for the reviewed commit. The `sdk.yml` run uploads `go-sdk-ci-release-evidence/go-sdk-ci-release-evidence.json`, and Stage 7 cross-checks that metadata against `gh run view --json jobs` so every claimed release target has a successful, non-skipped SDK CI job plus target-bound packageArchive verifier, release cargo profile, staging metadata, runner label, bounded log, no skipped/not-release-ready marker, no `helperRootEvidence.source=producerModeMaterializeHelpers` block, macOS x64 architecture proof with downloaded proof-file evidence, Windows MSVC release-shaped host proof with downloaded proof-file evidence, and required smoke-test evidence that was listed before it ran. The shipping release-readiness run uploads `shipping-release-readiness-metadata/shipping-release-readiness.json`, and Stage 7 cross-checks that artifact against the shipping run jobs for downloaded real release workflow/script reuse proof, downloaded duplicate-command audit proof, successful critical reused jobs (`packageMacosJob`, `finalizeMacosJob`, and `publishDotslashJob`), explicit reviewed `fixtureSubstitutions`, top-level readiness semantics (`nonPublishingFixtureEvidence` must keep `notReleaseReady=true`, while real `releaseArtifactEvidence` must be non-fixture evidence with `notReleaseReady=false`), present bounded log files, target-specific successful job names/conclusions, target-specific `codex-package-*.tar.zst` and `codex-app-server-package-*.tar.zst` archive filenames, downloaded archive member inventories, downloaded public checksum manifest copies, mode-matched provenance records (`shippingReadinessWrapper` only for fixture evidence and `realReleaseWorkflow` for non-fixture evidence), in-archive executable paths, runtime helper paths inside the `codex-package` archives, and either the real full required packageArchive smoke suite or fixture-only `expectedPackageArchiveSmokeTests` with `packageArchiveSmokeTestsRan=false`, plus macOS `package-macos`/`finalize-macos` DMG/direct artifact names, macOS x64 runner/architecture proof, Windows package archive plus downloaded published zip helper inventory, and `publish-dotslash` consumption/parity for every entry in `.github/dotslash-config.json` with a downloaded parity report. `sdk.yml`, release-readiness, and shipping logs are supplemental anchors only; if GitHub Actions access, run IDs, downloaded artifacts, required metadata fields, or required downloaded evidence files are unavailable, Stage 7 is blocked rather than downgraded to local YAML parsing, metadata-only proof, or log-only greps. The command removes `.verification` after consuming the evidence so the subsequent clean-worktree gate is not failed by its own downloaded artifacts.

- [ ] Resolve ignored plan/spec artifacts before the final clean gate. For this plan bundle, execute exactly one reviewed path and record it in the final verification notes: force-add `docs/superpowers/plans/2026-07-02-go-sdk-full` if the plan bundle is part of the deliverable, move/remove the ignored plan bundle from the final release-readiness worktree after handoff if it is not part of the deliverable, or provide a committed reviewed allowlist file through `CODEX_GO_SDK_IGNORED_PLAN_ALLOWLIST` containing the exact `git status --porcelain` ignored lines that are intentionally accepted. Do not leave `!! docs/superpowers/` as an unexamined side effect.
- [ ] Check clean generated output with the same fail-fast gate as CI, including untracked generated files and unresolved ignored plan/spec artifacts:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
status="$(git status --porcelain=v1 --untracked-files=normal --ignore-submodules=none)"
ignored_plan_status="$(git status --porcelain=v1 --ignored=matching --untracked-files=normal --ignore-submodules=none docs/superpowers/plans/2026-07-02-go-sdk-full)"
if test -n "${CODEX_GO_SDK_IGNORED_PLAN_ALLOWLIST:-}"; then
  test -f "${CODEX_GO_SDK_IGNORED_PLAN_ALLOWLIST}"
  ignored_plan_status="$(printf '%s\n' "${ignored_plan_status}" | grep -Fvx -f "${CODEX_GO_SDK_IGNORED_PLAN_ALLOWLIST}" || true)"
fi
if test -n "${status}" || test -n "${ignored_plan_status}"; then
  printf '%s\n' "${status}" "${ignored_plan_status}" | sed '/^$/d'
  exit 1
fi
```

Expected: the command exits 0 with no output, including no unresolved ignored `docs/superpowers/plans/2026-07-02-go-sdk-full` plan-bundle status. Any expected plan/spec artifact must be committed, force-added intentionally if ignored, removed from the final worktree before release-readiness sign-off, or matched by an explicit reviewed allowlist file that this gate consumes through `CODEX_GO_SDK_IGNORED_PLAN_ALLOWLIST`; otherwise final acceptance is blocked instead of claimed clean.

### Task 7.3: Acceptance Checklist

- [ ] Verify `sdk/go/go.mod` declares Go 1.25.
- [ ] Verify `ClientConfig{}` selects `ProtocolModeExperimental`.
- [ ] Verify `ProtocolModeStable` rejects experimental methods/fields before write.
- [ ] Verify `Client` has non-nil exported resource fields.
- [ ] Verify `Raw()` exposes generated typed methods only for SDK-public client-to-server rows; handshake-only `initialize`, compatibility-only, internal-test-only, and excluded rows must be absent from public `Raw()`.
- [ ] Verify every SDK-public generated method has a method-level resource matrix row with resource owner, wrapper or explicit generated-only raw-protocol row, unit test, safe integration decision, and docs/example owner derived from the reviewed Go-owned `resourceAPIMappings` input.
- [ ] Verify Stage 5 was executed through `stage-05a` through `stage-05f` bundle commits with fresh review after each bundle, or that any squashed result received an additional fresh blind review proving every bundle remains reviewable and complete.
- [ ] Verify `sdk/go/internal/protocodex/current_protocol_inventory.generated.md` exists, is generated from manifest/schema extraction plus `resourceAPIMappings` and `serverHandlerMappings`, and covers every current stable and experimental method/request/notification. The appendix comparison is a seed check only; final acceptance depends on schema/manifest extraction and mapping coverage.
- [ ] Verify known current experimental entries are present and marked experimental: `thread/realtime/start`, `thread/settings/update`, `memory/reset`, `collaborationMode/list`, `process/spawn`, and `fuzzyFileSearch/sessionStart`.
- [ ] Verify `Threads`, `Turns`, `Realtime`, `Skills`, and `Hooks` are covered beyond the high-level quickstart helpers.
- [ ] Verify server request handlers cover SDK-public requests and compatibility dispatch covers non-public server requests without public handler fields, public examples, or public README usage for compatibility-only rows.
- [ ] Verify `ClientNotification` coverage includes generated `initialized`, selected schema source validation, and drift failure for future uncovered client notifications.
- [ ] Verify `rawResponseItem/completed` is covered despite schema fixture exclusion.
- [ ] Verify serde metadata is exhaustive for every Go SDK request/response/notification payload or has an explicit schema-sufficient proof; representative golden fixtures are not enough.
- [ ] Verify top-level `trace` has outbound per-call options, inbound handler propagation, lowercase `traceparent`/`tracestate` wire keys, and round-trip tests outside `params`.
- [ ] Verify local image/file input helpers enforce `ClientConfig.Limits.MaxLocalInputBytes` before transport write with below-limit, at-limit, and over-limit tests proving no unbounded read.
- [ ] Verify model-visible `AdditionalContext` is bounded end-to-end before it is exposed publicly: server-owned manifest/protocol limits exist, Go `ClientLimits` normalizes them, high-level `Thread.Run`/`Thread.Turn`/`TurnHandle.Steer` and generated/raw `turn/start`/`turn/steer` reject over-limit additional context before JSON-RPC write, and Rust app-server rejects over-limit raw requests before `map_additional_context` builds core context. Verification must cover below-limit, at-limit, over-limit entry count, key bytes, value bytes, total bytes, and a proof that no single item can exceed the repo's 10K-token model-context cap; any default that can plausibly cross 1K tokens requires explicit manual review evidence.
- [ ] Verify notification opt-out conflicts fail before initialize in high-level mode and raw-only mode disables affected high-level workflows with typed errors.
- [ ] Verify unknown server notifications are preserved for raw/global subscribers as `UnknownNotification` with method, raw params, and top-level trace while known notifications still route by manifest metadata.
- [ ] Verify unknown `ProtocolMode` values fail with typed `ConfigError` before runtime lookup, spawn, injected transport use, or initialize write.
- [ ] Verify default runtime launch follows the approved resolver order `Transport -> CodexPath -> exec.LookPath("codex")`: a compatible PATH-discovered runtime works for `ClientConfig{}` under strict digest/mode validation, a missing PATH runtime returns `RuntimeNotFoundError` without leaking environment values, and any PATH-discovered runtime with missing, mismatched, or wrong-mode protocol digests fails closed before `initialized`. Positive release/CI real-runtime tests must still use explicit `CodexPath`/`CODEX_EXEC_PATH` or injected transports so CI never accidentally proves an ambient machine binary.
- [ ] Verify `CompatibilityAllowProtocolDigestUnavailable` accepts missing runtime digests only for injected test transports or explicit development `CodexPath` launches, and rejects implicit `PATH` or release-like runtimes.
- [ ] Verify initialize compatibility uses a Go-internal raw compatibility envelope before policy evaluation, while generated `protocol.InitializeResponse` remains current-only and rejects the same legacy missing-digest payload with a typed missing-field decode error.
- [ ] Verify release/default app-server rejects or ignores debug/test hook envs and hidden startup args. Any startup suppression, managed-config bypass, or mocked-auth coverage must be debug/test-fixture or injected-transport only, not behavior reachable from a release-shaped `CODEX_EXEC_PATH app-server` launch.
- [ ] Verify config override secret rejection tests pass.
- [ ] Verify login/account integration tests use mocks only through injected transport or a separate debug/test fixture. The production `CODEX_EXEC_PATH app-server` path must not receive or accept a mock auth base-url env hook; release-runtime coverage must instead prove unauthenticated account errors are typed, while Responses traffic is redirected only by the isolated `CODEX_HOME/config.toml` contract using `model_providers.<id>.base_url`, `openai_base_url`, and `wire_api = "responses"`, with no new Responses/auth base-url env hook.
- [ ] Verify Stage 5G produced a release-owned no-network helper source or reviewed helper-root artifact manifest for managed `rg`, zsh, and archive `zstd`, or that archive staging has a clear fail-fast preinstalled-`zstd` prerequisite before package assembly. Any helper-root artifact must include `codex-package-helpers.json` and Stage 7 must verify it with `python3 -m codex_package.materialize_helpers --verify-only` before the network-disabled staging segment. If Stage 5G marked runtime staging blocked, Stage 7 must stop before claiming real-runtime release readiness.
- [ ] Verify real app-server integration tests consume `CODEX_EXEC_PATH`, start from a clean parent env with no ambient `CODEX_HOME`, `CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG`, `CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE`, or auth override envs, create isolated `CODEX_HOME` values inside the Go test harness, reject reserved hook envs before spawn, and cover initialize digest plus safe representative workflows without release-shipped startup/auth/config bypasses.
- [ ] Verify final staging checks assert concrete helper payloads where `InstallContext` resolves them for Bazel-layout and Linux/macOS/Windows packageArchive lanes: managed `rg` in `codex-path`, non-Windows `zsh/bin/zsh` in `codex-resources`, Linux `bwrap`, Windows sandbox setup, and Windows command runner. Empty `codex-resources/` or `codex-path/` directories are not sufficient final verification evidence. Downloaded shipping metadata must record those helper paths inside each target `codex-package-*.tar.zst` archive, not only in local logs. Linux packageArchive verification must rerun `stage-codex-runtime.sh --verify-sandbox --exec-path "$CODEX_EXEC_PATH"`, and macOS packageArchive verification must assert shipping metadata `runtimeSource: packageArchive`, `cargoProfile: release`, and the target `bazelTarget` from the `package-macos` archive lane before running the same required SDK smoke tests, including `TestRealAppServerRejectsDebugHookEnv`, against the staged macOS release binary. Claimed packageArchive release readiness must include public checksum manifest-backed `.tar.zst` sha256/provenance records and in-archive executable path proof for both `codex-package` and `codex-app-server-package` archives for every target, and claimed macOS release readiness must also include downloaded artifact-level DMG/direct evidence from the shipping release-readiness metadata, not only matching workflow source or logs.
- [ ] Verify each release-shaped CI target runs the Bazel runtime-layout staging lane plus the matching shipping package-builder archive verifier for that OS: Linux, macOS, and Windows all use metadata `runtimeSource: packageArchive`, `cargoProfile: release`, and `bazelTarget` equal to that matrix lane's target. The packageArchive loops must enumerate `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc` explicitly with no empty/default target. Windows packageArchive metadata must also record `windowsReleaseShapedMsvc: true` and `windowsMsvcHostPlatform: true`.
- [ ] Verify `RemoteControlPairingHandle` owns pairing start/status/wait/cleanup where supported, and docs/examples do not present pairing as a thin raw params helper.
- [ ] Verify realtime sessions enforce one active SDK session per thread until every realtime notification has a Codex-owned session identity, with tests proving no cross-delivery.
- [ ] Verify `.github/workflows/go-sdk-release-readiness.yml` validates `sdk/go/v*` tags, is callable through `workflow_call` from `.github/workflows/sdk.yml` with an explicit `checkout_ref`, falls back to `github.ref` rather than `github.sha` for direct tag/manual validation, peels `${GITHUB_REF}^{commit}` for pushed tags, covers an annotated synthetic `sdk/go/v1.*` tag created in the temporary bare remote with explicit git identity and skipped by the lightweight tag loop, does not receive `secrets: inherit`, validates synthetic `sdk/go/v0.*` and `sdk/go/v1.*` tags against the reviewed head checkout during PR CI, labels synthetic `sdk/go/v2.*` as a throwaway rewritten-tree future-major policy smoke rather than reviewed-head checkout proof, runs `go test ./...`, runs `TestReleaseReadiness`, proves an external consumer can import and compile the module path derived from `sdk/go/go.mod` or tag semantic import version through Go VCS/module resolution without `replace`, and finishes with the shared clean-worktree action.
- [ ] Verify Windows/macOS/Linux CI jobs are present with release-shaped targets (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc`), verify `x86_64-unknown-linux-musl` runs on the same shipping release runner pool `${repo}-linux-x64-xl`, verify both Go SDK CI and shipping `.github/workflows/rust-release.yml` execute `x86_64-apple-darwin` on an Intel macOS runner such as `macos-15-large` or an explicit Rosetta `arch -x86_64` path, verify downloaded shipping metadata contains macOS x64 runner/architecture proof and macOS DMG/direct artifact names for claimed macOS release readiness, verify the published Windows `codex-<target>.exe.zip` fails closed unless it includes `codex-command-runner.exe` and `codex-windows-sandbox-setup.exe`, verify downloaded DotSlash metadata proves `publish-dotslash` consumes `.github/dotslash-config.json` against every published entry (`codex`, `codex-app-server`, `codex-responses-api-proxy`, Linux `bwrap`, Windows `codex-command-runner`, and Windows `codex-windows-sandbox-setup`), verify `.tar.zst` package archives have checksum/provenance records, and verify Windows release-shaped staging is target-aware for both `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc`; otherwise final acceptance is explicitly blocked.
- [ ] Verify docs cover each resource group and do not promise unsupported structured output.

### Task 7.4: Final Fresh Blind Review

- [ ] Dispatch engineering reviewer with:
  - user goal
  - design/spec
  - this plan bundle
  - full diff
  - verification outputs.
- [ ] Dispatch product owner with same materials and docs/examples focus.
- [ ] Dispatch release/ops owner with CI/release focus.
- [ ] Repair all valid blocking findings.
- [ ] Repeat fresh blind review until blocking findings = 0.

### Task 7.5: Finish Branch

- [ ] Use `superpowers:finishing-a-development-branch`.
- [ ] Present options:
  - keep branch for more work
  - open draft PR
  - open ready PR
  - merge if project policy allows.

## Final Output Requirements

The final user-facing answer must include:

- Files changed at a high level.
- Verification commands run and pass/fail status.
- Any skipped commands and why.
- Whether fresh blind blocking findings are zero.
- PR/branch status if requested.
