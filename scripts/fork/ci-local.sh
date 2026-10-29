#!/usr/bin/env bash
# Fork CI checks for `codex/fork`.
#
# `.github/workflows/fork-ci.yml` runs one subcommand per CI step, and the same
# subcommands reproduce CI locally. Run from any directory:
#
#   scripts/fork/ci-local.sh <command> [options]
#
# Commands:
#   rust-fmt              cargo fmt --check (upstream rust-ci flags)
#   rust-clippy           strict clippy (--tests, -D warnings) for fork-touched crates
#   rust-helpers          build helper binaries the integration tests spawn
#   rust-test [OPTIONS]   build helpers, then nextest with an isolated HOME; fails
#                         when a passed test printed that a helper was missing
#       --partition SPEC    nextest partition, for example hash:1/3
#       -p PACKAGE          test this package instead of the defaults (repeatable)
#       -- ARGS...          extra arguments for `cargo nextest run` (select the
#                           profile with NEXTEST_PROFILE, not --profile)
#   rust-app-server-test [OPTIONS]
#                         rust-test for codex-app-server; its integration tests
#                         run one at a time in the default nextest profile
#       --partition SPEC    nextest partition, for example hash:1/4
#       -- ARGS...          extra arguments for `cargo nextest run`
#   sandbox-check         require working unprivileged user namespaces and bwrap
#   go-static             gofmt and go vet for sdk/go
#   go-test               go test -race ./... (real-runtime tests skip)
#   go-runtime-build      build codex, codex-app-server and write_go_sdk_manifest
#   go-generators         Go manifest, experimental schema and protocodex drift checks
#   go-real-app-server    TestRealAppServer* (strict handshake included) against
#                         codex; every test `go test -list` finds must pass
#   go-auth-fixture       TestAuthFixture* (Linux only) against codex-app-server;
#                         every test `go test -list` finds must pass
#   scripts               shellcheck and the ci-local.sh / verify-release.sh /
#                         release-package.sh / release.sh / upstream-drift.sh
#                         self-tests
#   fork-coverage UPSTREAM_REF
#                         fail when a package the fork changes since its merge
#                         base with UPSTREAM_REF lacks strict clippy or tests
#   all                   every command above except sandbox-check and
#                         fork-coverage, in CI order
#
# Environment:
#   RUSTY_V8_ARCHIVE, RUSTY_V8_SRC_BINDING_PATH
#                         rusty_v8 artifacts (see .github/actions/setup-rusty-v8);
#                         required for Code Mode crates.
#   CARGO_TARGET_DIR      honoured when locating built binaries.
#   CODEX_EXEC_PATH       overrides the codex binary used by go-real-app-server
#                         and the scripts self-test.
#   CODEX_GO_SDK_AUTH_FIXTURE_PATH
#                         overrides the codex-app-server binary used by
#                         go-auth-fixture.
#   NEXTEST_PROFILE, NEXTEST_RETRIES
#                         passed through to nextest (CI uses the default profile).
#   CI=true               a nextest version other than the pinned one is fatal.
#
# Tests run with HOME and XDG_CONFIG_HOME in a fresh temporary directory so host
# state such as ~/.agents/skills does not leak into snapshots. CARGO_HOME,
# RUSTUP_HOME and the Go caches stay pinned to their real locations; otherwise
# cargo and rustup would reinstall into the temporary HOME and rebuild target/.
#
# Requires bash >= 4.4 (on macOS, a Homebrew bash).

# Written for bash 3.2 too, so an old bash reaches this message.
if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 4))); then
  printf '%s: requires bash >= 4.4 (running %s); on macOS run it with a newer bash, for example Homebrew bash\n' \
    "${0##*/}" "${BASH_VERSION}" >&2
  exit 1
fi

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
rust_dir="${repo_root}/codex-rs"
go_dir="${repo_root}/sdk/go"

# Crates the fork changes relative to upstream. `fork-coverage UPSTREAM_REF`
# lists the packages missing here.
fork_clippy_packages=(
  app_test_support
  codex-agent-message-board-extension
  codex-analytics
  codex-api
  codex-app-server
  codex-app-server-daemon
  codex-app-server-protocol
  codex-app-server-transport
  codex-arg0
  codex-client
  codex-code-mode-host
  codex-code-mode-protocol
  codex-code-mode-runtime
  codex-config
  codex-core
  codex-exec
  codex-exec-server
  codex-extension-items
  codex-features
  codex-guardian-v2
  codex-history
  codex-linux-sandbox
  codex-mcp
  codex-models-manager
  codex-protocol
  codex-rollout
  codex-rollout-trace
  codex-thread-manager-sample
  codex-thread-store
  codex-tools
  codex-tui
  codex-utils-output-truncation
  codex-utils-string
  codex-workflow
  codex-worktree
  core_test_support
)

# Every fork-touched package runs its tests here, except codex-app-server
# (rust-app-server-test) and untested_fork_packages; `fork-coverage` checks it.
default_test_packages=(
  codex-core
  codex-linux-sandbox
  codex-app-server-protocol
  codex-workflow
  codex-worktree
  codex-exec
  codex-utils-string
  app_test_support
  codex-agent-message-board-extension
  codex-analytics
  codex-api
  codex-app-server-daemon
  codex-app-server-transport
  codex-arg0
  codex-client
  codex-code-mode-host
  codex-code-mode-protocol
  codex-code-mode-runtime
  codex-config
  codex-exec-server
  codex-extension-items
  codex-features
  codex-guardian-v2
  codex-history
  codex-mcp
  codex-models-manager
  codex-protocol
  codex-rollout
  codex-rollout-trace
  codex-thread-manager-sample
  codex-thread-store
  codex-tools
  codex-tui
  codex-utils-output-truncation
  core_test_support
)

# Fork-touched packages whose tests do not run in fork CI yet, one per line with
# a comment giving the reason. fork-coverage accepts them and prints them.
untested_fork_packages=()

# Binaries that codex-core integration tests locate next to their test
# executables (codex_utils_cargo_bin::cargo_bin falls back to target/debug).
# Some tests skip instead of failing when one is missing, so rust-test also
# rejects the skip messages below. Inventory: specs/fork-ci-2026-10-25.md, UC3.
test_helper_bins=(codex codex-code-mode-host test_stdio_server test_streamable_http_server)

# A passed test whose output matches this extended regular expression ran
# without a helper binary: the skip text the tests print, or an error of
# codex_utils_cargo_bin::cargo_bin.
helper_missing_re='binary not available, skipping|could not locate binary|CARGO_BIN_EXE env var [^ ]+ resolved to'

# nextest version that CI installs (taiki-e/install-action in fork-ci.yml).
nextest_version=0.9.103

strict_handshake_test=TestRealAppServerInitializeStrictDigest

# Set by run_go_tests_require_pass: listed tests that reported SKIP.
go_tests_skipped=0

# Scratch directories removed on exit, whatever the outcome. Isolated HOMEs
# are not listed: run_isolated keeps a failed one for inspection.
cleanup_paths=()
trap 'rm -rf -- "${cleanup_paths[@]}"' EXIT

go_manifest_rel="internal/protocodex/manifest/app_server_protocol_manifest.json"
go_experimental_schema_rel="internal/protocodex/schema-experimental"

log() { printf '==> %s\n' "$*" >&2; }
die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed -E 's/^# ?//'
}

# Prints the absolute cargo target directory used by builds in codex-rs.
target_dir() {
  local dir="${CARGO_TARGET_DIR:-target}"
  if [[ "${dir}" != /* ]]; then
    dir="${rust_dir}/${dir}"
  fi
  printf '%s\n' "${dir}"
}

cargo_rs() {
  repair_rusty_v8_build_cache
  (cd "${rust_dir}" && cargo "$@")
}

# The v8 build script extracts librusty_v8.a into <target>/<profile>/gn_out, beside
# the profile's build/, .fingerprint/ and deps/ directories, which are the only ones
# rust-cache keeps. A restored cache then holds a finished build-script run without
# its library: cargo skips the script and linking v8 fails. Forget such runs so the
# script extracts RUSTY_V8_ARCHIVE again.
repair_rusty_v8_build_cache() {
  local target output dir profile
  target="$(target_dir)"
  for output in "${target}"/*/build/v8-*/output; do
    [[ -f "${output}" ]] || continue
    dir="$(sed -n 's/^cargo:rustc-link-search=\(native=\)\{0,1\}//p' "${output}" | head -n 1)"
    [[ -n "${dir}" && ! -f "${dir}/librusty_v8.a" ]] || continue
    profile="${output%/build/*}"
    log "rusty_v8: ${dir}/librusty_v8.a is missing; rerunning the v8 build script in ${profile##*/}"
    rm -rf "${profile}"/build/v8-* "${profile}"/.fingerprint/v8-*
  done
}

# Exports tool locations that would otherwise follow HOME.
pin_tool_homes() {
  export CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}"
  export RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}"
  if command -v go >/dev/null 2>&1; then
    GOPATH="$(go env GOPATH)"
    GOCACHE="$(go env GOCACHE)"
    GOMODCACHE="$(go env GOMODCACHE)"
    GOENV="$(go env GOENV)"
    export GOPATH GOCACHE GOMODCACHE GOENV
  fi
}

# Runs a command with HOME and XDG_CONFIG_HOME in a fresh temporary directory.
# The directory is removed on success and kept for inspection on failure.
run_isolated() {
  pin_tool_homes
  local home status=0
  home="$(mktemp -d "${TMPDIR:-/tmp}/fork-ci-home.XXXXXX")"
  mkdir -p "${home}/.config"
  log "isolated HOME=${home}"
  env -u CODEX_HOME -u XDG_DATA_HOME -u XDG_STATE_HOME -u XDG_CACHE_HOME \
    HOME="${home}" XDG_CONFIG_HOME="${home}/.config" "$@" || status=$?
  if [[ ${status} -eq 0 ]]; then
    rm -rf "${home}"
  else
    log "kept isolated HOME ${home} for inspection"
  fi
  return "${status}"
}

require_built() {
  local path
  for path in "$@"; do
    [[ -x "${path}" && ! -d "${path}" ]] || die "missing executable ${path}"
  done
}

cmd_rust_fmt() {
  log "cargo fmt --check"
  cargo_rs fmt -- --config imports_granularity=Item --check
}

cmd_rust_clippy() {
  local args=() package
  for package in "${fork_clippy_packages[@]}"; do
    args+=(-p "${package}")
  done
  log "cargo clippy --tests (${#fork_clippy_packages[@]} fork-touched packages)"
  cargo_rs clippy --locked --tests "${args[@]}" -- -D warnings
}

cmd_rust_helpers() {
  log "build test helpers: ${test_helper_bins[*]}"
  cargo_rs build --locked \
    -p codex-cli --bin codex \
    -p codex-code-mode-host --bin codex-code-mode-host \
    -p codex-rmcp-client --bin test_stdio_server --bin test_streamable_http_server
  local bin dir
  dir="$(target_dir)/debug"
  for bin in "${test_helper_bins[@]}"; do
    require_built "${dir}/${bin}"
  done
}

# Requires the nextest version CI pins: fatal in CI, a warning elsewhere.
check_nextest_version() {
  local out found
  out="$(cargo_rs nextest --version 2>/dev/null)" ||
    die "cargo nextest is not installed (CI uses ${nextest_version})"
  found="$(awk '$1 == "cargo-nextest" { print $2; exit }' <<<"${out}")"
  [[ "${found}" == "${nextest_version}" ]] && return 0
  if [[ "${CI:-}" == "true" ]]; then
    die "nextest ${found}, want ${nextest_version}: pin it with tool: nextest@${nextest_version} in fork-ci.yml"
  fi
  log "warning: nextest ${found}, want ${nextest_version}; CI runs ${nextest_version}"
}

# Prints "kind<TAB>test<TAB>message" for every line of a passed or failed
# test's captured output in a nextest JUnit report that names a missing helper
# (kind "helper") or is a skip message (kind "skip").
junit_skip_lines() {
  awk -v helper_re="${helper_missing_re}" '
    function attr(line, key,    pattern) {
      pattern = " " key "=\"[^\"]*\""
      if (!match(line, pattern)) return ""
      return substr(line, RSTART + length(key) + 3, RLENGTH - length(key) - 4)
    }
    /<testcase / { current = attr($0, "classname") " " attr($0, "name"); next }
    /<\/testcase>/ { current = ""; next }
    current == "" { next }
    {
      line = $0
      gsub(/<\/?system-(out|err)>/, "", line)
      gsub(/&quot;/, "\"", line); gsub(/&apos;/, "'\''", line)
      gsub(/&lt;/, "<", line); gsub(/&gt;/, ">", line); gsub(/&amp;/, "\\&", line)
      sub(/^[ \t]+/, "", line)
      if (line ~ /^test .* \.\.\. ok$/) next
      # Skip messages of the tests start with "skipping" or say "skipping
      # test"; log lines that mention skipping something else do not count.
      if (line ~ helper_re) kind = "helper"
      else if (tolower(line) ~ /^skipping |skipping test/) kind = "skip"
      else next
      print kind "\t" current "\t" substr(line, 1, 240)
    }
  ' "$1" | LC_ALL=C sort -u
}

# Rejects a JUnit report in which a test ran without a helper binary, and lists
# every other test that printed a skip message (log and job summary).
check_junit_skips() {
  local junit="$1" lines helper skip
  [[ -f "${junit}" ]] ||
    die "no JUnit report at ${junit}; the silent-skip guard needs the [profile.<name>.junit] path junit.xml"
  lines="$(junit_skip_lines "${junit}")"
  skip="$(awk -F '\t' '$1 == "skip" { print "    " $2 ": " $3 }' <<<"${lines}")"
  helper="$(awk -F '\t' '$1 == "helper" { print "    " $2 ": " $3 }' <<<"${lines}")"
  if [[ -n "${skip}" ]]; then
    log "other skip messages in test output (not fatal): $(grep -c . <<<"${skip}")"
    printf '%s\n' "${skip}" >&2
  else
    log "no other skip messages in test output"
  fi
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      printf '### Skip messages in nextest output\n\n'
      if [[ -n "${helper}${skip}" ]]; then
        # shellcheck disable=SC2016 # backticks are a Markdown code fence
        printf '```\n%s\n```\n' "$(printf '%s\n%s\n' "${helper}" "${skip}" | sed '/^$/d' | head -n 200)"
      else
        printf 'None.\n'
      fi
    } >>"${GITHUB_STEP_SUMMARY}"
  fi
  if [[ -n "${helper}" ]]; then
    printf 'error: tests passed without running because a helper binary was missing:\n%s\n' "${helper}" >&2
    printf 'error: build the helper in cmd_rust_helpers (scripts/fork/ci-local.sh)\n' >&2
    return 1
  fi
}

cmd_rust_test() {
  local partition="" packages=() extra=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --partition)
        [[ $# -ge 2 ]] || die "--partition requires a value"
        partition="$2"
        shift 2
        ;;
      -p)
        [[ $# -ge 2 ]] || die "-p requires a package"
        packages+=("$2")
        shift 2
        ;;
      --)
        shift
        extra=("$@")
        break
        ;;
      *) die "unknown rust-test option: $1" ;;
    esac
  done
  if [[ ${#packages[@]} -eq 0 ]]; then
    packages=("${default_test_packages[@]}")
  fi
  if [[ -n "${partition}" && ! "${partition}" =~ ^(hash|count):[1-9][0-9]*/[1-9][0-9]*$ ]]; then
    die "invalid partition ${partition}; expected hash:K/N or count:K/N"
  fi
  local profile="${NEXTEST_PROFILE:-default}" arg
  [[ "${profile}" =~ ^[A-Za-z0-9_-]+$ ]] || die "invalid NEXTEST_PROFILE '${profile}'"
  for arg in "${extra[@]}"; do
    case "${arg}" in
      --profile | --profile=* | -P*)
        die "select the nextest profile with NEXTEST_PROFILE, not ${arg}: the silent-skip guard reads that profile's JUnit report"
        ;;
    esac
  done

  local package_args=() package
  for package in "${packages[@]}"; do
    package_args+=(-p "${package}")
  done

  cmd_rust_helpers
  check_nextest_version
  log "build test binaries: ${packages[*]}"
  cargo_rs test --locked --no-run "${package_args[@]}"

  # Keep the output of passed tests in the JUnit report, so skip messages are
  # visible. A tool config sits below .config/nextest.toml, which stays as
  # upstream has it, and also gives a JUnit report to profiles without one.
  # nextest keeps its store under <workspace>/target/nextest even when
  # CARGO_TARGET_DIR points elsewhere, so the store is pinned next to the
  # target directory the guard reads.
  local tool_dir junit store
  tool_dir="$(mktemp -d "${TMPDIR:-/tmp}/fork-ci-nextest.XXXXXX")"
  cleanup_paths+=("${tool_dir}")
  tool_dir="$(cd -- "${tool_dir}" && pwd)"
  store="$(target_dir)/nextest"
  [[ "${store}" != *"'"* ]] || die "target directory must not contain a single quote: ${store}"
  printf "[store]\ndir = '%s'\n\n[profile.%s.junit]\npath = \"junit.xml\"\nstore-success-output = true\n" \
    "${store}" "${profile}" >"${tool_dir}/fork-ci.toml"
  junit="${store}/${profile}/junit.xml"
  rm -f -- "${junit}"

  local nextest_args=(nextest run --locked --no-fail-fast "${package_args[@]}"
    --tool-config-file "fork-ci:${tool_dir}/fork-ci.toml")
  if [[ -n "${partition}" ]]; then
    nextest_args+=(--partition "${partition}")
  fi
  log "cargo ${nextest_args[*]} ${extra[*]}"
  local status=0
  (
    cd "${rust_dir}"
    export RUST_MIN_STACK=8388608 INSTA_UPDATE=no
    # As in fork-ci.yml: error values must not capture backtraces, because tests
    # compare their Debug output; panics still honour RUST_BACKTRACE.
    export RUST_LIB_BACKTRACE=0
    repair_rusty_v8_build_cache
    run_isolated cargo "${nextest_args[@]}" "${extra[@]}"
  ) || status=$?

  if [[ ${status} -ne 0 && ! -f "${junit}" ]]; then
    log "nextest failed (exit ${status}) before writing ${junit}"
    return "${status}"
  fi
  log "silent-skip guard: ${junit}"
  local guard=0
  check_junit_skips "${junit}" || guard=$?
  [[ ${status} -eq 0 ]] || return "${status}"
  [[ ${guard} -eq 0 ]] || exit 1
}

cmd_rust_app_server_test() {
  local arg
  for arg in "$@"; do
    [[ "${arg}" != -- ]] || break
    [[ "${arg}" != -p ]] || die "rust-app-server-test tests codex-app-server only; use rust-test -p for other packages"
  done
  cmd_rust_test -p codex-app-server "$@"
}

# Reads a kernel parameter that must have the given value when it exists.
check_kernel_param() {
  local key="$1" want="$2" file value
  file="/proc/sys/${key//.//}"
  if command -v sysctl >/dev/null 2>&1; then
    if ! value="$(sysctl -n "${key}" 2>&1)"; then
      [[ "${value}" == *"No such file or directory"* ]] || die "cannot read ${key}: ${value}"
      log "${key}: not present on this kernel"
      return 0
    fi
  elif [[ -e "${file}" ]]; then
    value="$(<"${file}")"
  else
    log "${key}: not present on this kernel"
    return 0
  fi
  [[ "${value}" == "${want}" ]] || die "${key} is ${value}, want ${want}"
  log "${key} = ${value}"
}

# Postcondition of the runner's sandbox preparation: bubblewrap sandbox tests
# can run instead of skipping. Probes alone are not enough: the system unshare
# and bwrap can pass under the AppArmor restriction, so the parameters are
# checked first.
cmd_sandbox_check() {
  check_kernel_param kernel.unprivileged_userns_clone 1
  check_kernel_param kernel.apparmor_restrict_unprivileged_userns 0
  command -v unshare >/dev/null 2>&1 || die "unshare is not installed"
  command -v bwrap >/dev/null 2>&1 || die "bwrap is not installed (apt-get install bubblewrap)"
  unshare --user --map-root-user true ||
    die "unshare --user --map-root-user true failed: unprivileged user namespaces are unavailable"
  bwrap --unshare-user --ro-bind / / true ||
    die "bwrap --unshare-user --ro-bind / / true failed: bubblewrap cannot create a sandbox"
  log "user namespaces and bubblewrap work"
}

cmd_go_static() {
  log "gofmt -l sdk/go"
  local unformatted
  unformatted="$(cd "${go_dir}" && gofmt -l .)"
  if [[ -n "${unformatted}" ]]; then
    printf '%s\n' "${unformatted}" >&2
    die "gofmt reports unformatted files (run gofmt -w in sdk/go)"
  fi
  log "go vet ./..."
  (cd "${go_dir}" && go vet ./...)
}

cmd_go_test() {
  log "go test -race ./... (real-runtime tests skip without CODEX_EXEC_PATH)"
  (
    cd "${go_dir}"
    run_isolated env -u CODEX_EXEC_PATH -u CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER \
      -u CODEX_GO_SDK_AUTH_FIXTURE_PATH -u CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE \
      go test -race -count=1 ./...
  )
}

cmd_go_runtime_build() {
  log "build codex, codex-app-server and write_go_sdk_manifest"
  cargo_rs build --locked \
    -p codex-cli --bin codex \
    -p codex-app-server --bin codex-app-server \
    -p codex-app-server-protocol --bin write_go_sdk_manifest
}

# Prints how to regenerate the Go SDK manifest, schema copy and bindings.
go_regen_hint() {
  cat <<HINT
regenerate the Go SDK from the repository root:
  (cd codex-rs && cargo run -p codex-app-server-protocol --bin write_go_sdk_manifest -- \\
     --output ../sdk/go/${go_manifest_rel} \\
     --experimental-schema-output ../sdk/go/${go_experimental_schema_rel})
  (cd sdk/go && go run ./internal/cmd/protocodex --mode both \\
     --stable-schema-root ../../codex-rs/app-server-protocol/schema \\
     --experimental-schema-root ${go_experimental_schema_rel} \\
     --manifest ${go_manifest_rel} --out protocol --root-out .)
HINT
}

cmd_go_generators() {
  local regen_hint
  regen_hint="$(go_regen_hint)"
  local writer=(run --locked --quiet -p codex-app-server-protocol --bin write_go_sdk_manifest --)

  log "write_go_sdk_manifest --check"
  cargo_rs "${writer[@]}" --output "${go_dir}/${go_manifest_rel}" --check ||
    die "Go SDK manifest drift; ${regen_hint}"

  # --check compares only the manifest; regenerate both outputs into a scratch
  # directory to also catch a stale experimental schema copy.
  local scratch
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/fork-ci-go-manifest.XXXXXX")"
  cleanup_paths+=("${scratch}")
  log "regenerate manifest and experimental schema into ${scratch}"
  cargo_rs "${writer[@]}" --output "${scratch}/manifest.json" \
    --experimental-schema-output "${scratch}/schema-experimental"
  if ! cmp -s "${scratch}/manifest.json" "${go_dir}/${go_manifest_rel}"; then
    die "regenerated manifest bytes differ from sdk/go/${go_manifest_rel}; ${regen_hint}"
  fi
  local drift
  if ! drift="$(diff -rq "${scratch}/schema-experimental" "${go_dir}/${go_experimental_schema_rel}")"; then
    printf '%s\n' "${drift}" | head -n 50 >&2
    die "experimental schema drift in sdk/go/${go_experimental_schema_rel}; ${regen_hint}"
  fi

  log "protocodex --check --mode both"
  (
    cd "${go_dir}"
    go run ./internal/cmd/protocodex --check --mode both \
      --stable-schema-root ../../codex-rs/app-server-protocol/schema \
      --experimental-schema-root "${go_experimental_schema_rel}" \
      --manifest "${go_manifest_rel}" \
      --out protocol \
      --root-out .
  ) || die "generated Go protocol drift; ${regen_hint}"
}

# Prints the sdk/go tests that `go test -list` finds for ^PREFIX, sorted.
# `go test -run` exits 0 with "no tests to run", so this list, not the run,
# names the tests that must pass (-race reuses the test binary of the run).
list_go_tests() {
  local prefix="$1"
  (cd "${go_dir}" && go test -race -list "^${prefix}" ./...) |
    awk -v prefix="${prefix}" 'index($0, prefix) == 1 && /^[A-Za-z0-9_]+$/' |
    LC_ALL=C sort -u
}

# Runs ^PREFIX tests of sdk/go with -race under an isolated HOME and
# `env ENV_ARG...`, then requires `--- PASS` for every test in LISTED; a test in
# SKIPPABLE may report `--- SKIP` instead (both newline-separated). Sets
# go_tests_skipped.
run_go_tests_require_pass() {
  local prefix="$1" listed="$2" skippable="$3"
  shift 3
  local scratch status=0
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/fork-ci-go-test.XXXXXX")"
  cleanup_paths+=("${scratch}")
  (
    cd "${go_dir}"
    run_isolated env "$@" go test -race -count=1 -v -run "^${prefix}" ./...
  ) 2>&1 | tee "${scratch}/go-test.log" || status=$?
  [[ ${status} -eq 0 ]] || die "${prefix}* failed (exit ${status})"

  local name missing=()
  go_tests_skipped=0
  while IFS= read -r name; do
    if grep -qE "^--- PASS: ${name} \(" "${scratch}/go-test.log"; then
      continue
    elif grep -qx "${name}" <<<"${skippable}" &&
      grep -qE "^--- SKIP: ${name} \(" "${scratch}/go-test.log"; then
      go_tests_skipped=$((go_tests_skipped + 1))
    else
      missing+=("${name}")
    fi
  done <<<"${listed}"
  [[ ${#missing[@]} -eq 0 ]] || die "listed ${prefix} tests did not pass: ${missing[*]}"
}

cmd_go_real_app_server() {
  local codex
  codex="${CODEX_EXEC_PATH:-$(target_dir)/debug/codex}"
  require_built "${codex}"

  local listed count skippable=""
  listed="$(list_go_tests TestRealAppServer)"
  [[ -n "${listed}" ]] || die "go test -list found no TestRealAppServer tests in sdk/go"
  grep -qx "${strict_handshake_test}" <<<"${listed}" ||
    die "${strict_handshake_test} is not listed by go test -list in sdk/go"
  count="$(grep -c . <<<"${listed}")"
  # Linux-only cases skip elsewhere; the strict handshake must pass everywhere.
  if [[ "$(uname -s)" != Linux ]]; then
    skippable="$(grep -vx "${strict_handshake_test}" <<<"${listed}" || true)"
  fi

  log "go test -race TestRealAppServer* (${count} tests, codex=${codex})"
  run_go_tests_require_pass TestRealAppServer "${listed}" "${skippable}" \
    -u CODEX_GO_SDK_AUTH_FIXTURE_PATH -u CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE \
    CODEX_EXEC_PATH="${codex}" \
    CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1
  log "$((count - go_tests_skipped)) of ${count} TestRealAppServer tests passed$([[ ${go_tests_skipped} -eq 0 ]] || echo ", ${go_tests_skipped} skipped off Linux")"
}

# The auth fixture drives codex-app-server against a local HTTP fixture of the
# ChatGPT auth and backend endpoints; it is Linux-only.
cmd_go_auth_fixture() {
  [[ "$(uname -s)" == Linux ]] || die "go-auth-fixture: the Go SDK auth fixture is Linux-only"
  local app_server
  app_server="${CODEX_GO_SDK_AUTH_FIXTURE_PATH:-$(target_dir)/debug/codex-app-server}"
  require_built "${app_server}"

  local listed count
  listed="$(list_go_tests TestAuthFixture)"
  [[ -n "${listed}" ]] || die "go test -list found no TestAuthFixture tests in sdk/go"
  count="$(grep -c . <<<"${listed}")"

  log "go test -race TestAuthFixture* (${count} tests, codex-app-server=${app_server})"
  run_go_tests_require_pass TestAuthFixture "${listed}" "" \
    -u CODEX_EXEC_PATH -u CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER \
    CODEX_GO_SDK_AUTH_FIXTURE_PATH="${app_server}" \
    CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE=1
  log "${count} of ${count} TestAuthFixture tests passed"
}

# Package names of the Cargo manifests under codex-rs, as "<dir>\t<name>" lines.
workspace_package_dirs() {
  local manifest name
  while IFS= read -r manifest; do
    name="$(awk '
      /^\[package\]/ { in_package = 1; next }
      /^\[/ { in_package = 0 }
      in_package && /^name[[:space:]]*=/ {
        sub(/^name[[:space:]]*=[[:space:]]*"/, ""); sub(/".*/, ""); print; exit
      }' "${repo_root}/${manifest}")"
    [[ -n "${name}" ]] && printf '%s\t%s\n' "${manifest%/Cargo.toml}" "${name}"
  done < <(git -C "${repo_root}" ls-files -- 'codex-rs/*Cargo.toml')
}

# Fails when a package that the fork changes relative to UPSTREAM_REF is missing
# from fork_clippy_packages, or runs no tests in fork CI and is not listed in
# untested_fork_packages.
cmd_fork_coverage() {
  [[ $# -eq 1 ]] || die "usage: ${0##*/} fork-coverage UPSTREAM_REF"
  local upstream="$1" base
  base="$(git -C "${repo_root}" merge-base HEAD "${upstream}")" ||
    die "HEAD and ${upstream} have no common history"

  local -A dir_package=() touched=() tested=() clippy=() untested=()
  local dir name file package
  while IFS=$'\t' read -r dir name; do
    dir_package["${dir}"]="${name}"
  done < <(workspace_package_dirs)
  while IFS= read -r file; do
    dir="${file%/*}"
    while [[ "${dir}" == codex-rs/* ]]; do
      if [[ -n "${dir_package[${dir}]:-}" ]]; then
        touched["${dir_package[${dir}]}"]=1
        break
      fi
      dir="${dir%/*}"
    done
  done < <(git -C "${repo_root}" diff --name-only "${base}" HEAD -- codex-rs)

  for package in "${default_test_packages[@]}" codex-app-server; do tested["${package}"]=1; done
  for package in "${fork_clippy_packages[@]}"; do clippy["${package}"]=1; done
  for package in "${untested_fork_packages[@]}"; do untested["${package}"]=1; done

  local missing_tests=() missing_clippy=() accepted=()
  for package in $(printf '%s\n' "${!touched[@]}" | LC_ALL=C sort); do
    if [[ -z "${tested[${package}]:-}" ]]; then
      if [[ -n "${untested[${package}]:-}" ]]; then
        accepted+=("${package}")
      else
        missing_tests+=("${package}")
      fi
    fi
    [[ -n "${clippy[${package}]:-}" ]] || missing_clippy+=("${package}")
  done

  log "fork-coverage: ${#touched[@]} packages changed since $(git -C "${repo_root}" rev-parse --short "${base}")"
  if [[ ${#accepted[@]} -gt 0 ]]; then
    log "not tested in fork CI (untested_fork_packages): ${accepted[*]}"
  fi
  local status=0
  if [[ ${#missing_tests[@]} -gt 0 ]]; then
    printf 'error: fork-touched packages without tests in fork CI: %s\n' "${missing_tests[*]}" >&2
    printf '       add them to default_test_packages or untested_fork_packages in %s\n' "${BASH_SOURCE[0]}" >&2
    status=1
  fi
  if [[ ${#missing_clippy[@]} -gt 0 ]]; then
    printf 'error: fork-touched packages without strict clippy: %s\n' "${missing_clippy[*]}" >&2
    printf '       add them to fork_clippy_packages in %s\n' "${BASH_SOURCE[0]}" >&2
    status=1
  fi
  [[ ${status} -eq 0 ]] && log "fork-coverage: every fork-touched package is covered"
  return "${status}"
}

cmd_scripts() {
  # CI runs these scripts by path, and a checkout with core.fileMode=false
  # commits new files without the executable bit.
  local not_executable
  not_executable="$(git -C "${repo_root}" ls-files -s -- 'scripts/fork/*.sh' | awk '$1 != "100755" { print $4 }')"
  [[ -z "${not_executable}" ]] ||
    die "scripts/fork scripts must be executable in git (git update-index --chmod=+x): ${not_executable//$'\n'/ }"

  if command -v shellcheck >/dev/null 2>&1; then
    log "shellcheck scripts/fork/*.sh"
    shellcheck "${repo_root}"/scripts/fork/*.sh
  elif [[ "${CI:-}" == "true" ]]; then
    die "shellcheck is required in CI"
  else
    log "shellcheck not installed; skipping lint"
  fi

  local selftest=("${repo_root}/scripts/fork/test-verify-release.sh")
  local codex
  codex="${CODEX_EXEC_PATH:-$(target_dir)/debug/codex}"
  if [[ -x "${codex}" && ! -d "${codex}" ]]; then
    selftest+=(--codex "${codex}")
  else
    log "no built codex at ${codex}; the self-test skips the runtime checks"
  fi
  log "ci-local.sh self-test (stub cargo, go, sysctl, unshare, bwrap, uname)"
  "${repo_root}/scripts/fork/test-ci-local.sh"
  log "verify-release.sh self-test"
  "${selftest[@]}"
  log "release-package.sh self-test (fake binaries, stub strip)"
  "${repo_root}/scripts/fork/test-release-package.sh"
  log "release.sh self-test (stub gh)"
  "${repo_root}/scripts/fork/test-release.sh"
  log "upstream-drift.sh self-test (throwaway repository)"
  "${repo_root}/scripts/fork/test-upstream-drift.sh"
}

cmd_all() {
  cmd_rust_fmt
  cmd_rust_clippy
  cmd_rust_test
  cmd_rust_app_server_test
  cmd_go_static
  cmd_go_test
  cmd_go_runtime_build
  cmd_go_generators
  cmd_go_real_app_server
  cmd_go_auth_fixture
  cmd_scripts
}

main() {
  [[ $# -ge 1 ]] || {
    usage >&2
    exit 2
  }
  local command="$1"
  shift
  case "${command}" in
    rust-fmt) cmd_rust_fmt "$@" ;;
    rust-clippy) cmd_rust_clippy "$@" ;;
    rust-helpers) cmd_rust_helpers "$@" ;;
    rust-test) cmd_rust_test "$@" ;;
    rust-app-server-test) cmd_rust_app_server_test "$@" ;;
    sandbox-check) cmd_sandbox_check "$@" ;;
    go-static) cmd_go_static "$@" ;;
    go-test) cmd_go_test "$@" ;;
    go-runtime-build) cmd_go_runtime_build "$@" ;;
    go-generators) cmd_go_generators "$@" ;;
    go-real-app-server) cmd_go_real_app_server "$@" ;;
    go-auth-fixture) cmd_go_auth_fixture "$@" ;;
    scripts) cmd_scripts "$@" ;;
    fork-coverage) cmd_fork_coverage "$@" ;;
    all) cmd_all "$@" ;;
    -h | --help | help) usage ;;
    *)
      usage >&2
      die "unknown command: ${command}"
      ;;
  esac
}

main "$@"
