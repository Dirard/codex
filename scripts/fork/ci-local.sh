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
#   sandbox-check         require working unprivileged user namespaces and bwrap
#   go-static             gofmt and go vet for sdk/go
#   go-test               go test -race ./... (real-runtime tests skip)
#   go-runtime-build      build codex and write_go_sdk_manifest
#   go-generators         Go manifest, experimental schema and protocodex drift checks
#   go-real-app-server    TestRealAppServer* (strict handshake included) against
#                         codex; every test `go test -list` finds must pass
#   scripts               shellcheck and the ci-local.sh / verify-release.sh /
#                         release.sh self-tests
#   all                   every command above except sandbox-check, in CI order
#
# Environment:
#   RUSTY_V8_ARCHIVE, RUSTY_V8_SRC_BINDING_PATH
#                         rusty_v8 artifacts (see .github/actions/setup-rusty-v8);
#                         required for Code Mode crates.
#   CARGO_TARGET_DIR      honoured when locating built binaries.
#   CODEX_EXEC_PATH       overrides the codex binary used by go-real-app-server
#                         and the scripts self-test.
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

# Crates the fork changes relative to upstream. Refresh after a rebase with
#   git diff --name-only <fork base> HEAD -- codex-rs
# and map the crate directories to package names.
fork_clippy_packages=(
  codex-agent-message-board-extension
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
  codex-exec-server
  codex-features
  codex-history
  codex-linux-sandbox
  codex-models-manager
  codex-protocol
  codex-rollout-trace
  codex-thread-manager-sample
  codex-tools
  codex-tui
  codex-utils-output-truncation
)

default_test_packages=(
  codex-core
  codex-linux-sandbox
  codex-app-server-protocol
)

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
  (cd "${rust_dir}" && cargo "$@")
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
  local tool_dir junit
  tool_dir="$(mktemp -d "${TMPDIR:-/tmp}/fork-ci-nextest.XXXXXX")"
  cleanup_paths+=("${tool_dir}")
  tool_dir="$(cd -- "${tool_dir}" && pwd)"
  printf '[profile.%s.junit]\npath = "junit.xml"\nstore-success-output = true\n' "${profile}" \
    >"${tool_dir}/fork-ci.toml"
  junit="$(target_dir)/nextest/${profile}/junit.xml"
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
  log "build codex and write_go_sdk_manifest"
  cargo_rs build --locked \
    -p codex-cli --bin codex \
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

cmd_go_real_app_server() {
  local codex
  codex="${CODEX_EXEC_PATH:-$(target_dir)/debug/codex}"
  require_built "${codex}"

  # `go test -run` exits 0 with "no tests to run", so the tests that must pass
  # come from `go test -list` of the same tree (-race reuses the test binary).
  local listed count
  listed="$(cd "${go_dir}" && go test -race -list '^TestRealAppServer' ./... |
    awk '/^TestRealAppServer[A-Za-z0-9_]*$/' | LC_ALL=C sort -u)"
  [[ -n "${listed}" ]] || die "go test -list found no TestRealAppServer tests in sdk/go"
  grep -qx "${strict_handshake_test}" <<<"${listed}" ||
    die "${strict_handshake_test} is not listed by go test -list in sdk/go"
  count="$(grep -c . <<<"${listed}")"

  local scratch
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/fork-ci-go-real.XXXXXX")"
  cleanup_paths+=("${scratch}")
  log "go test -race TestRealAppServer* (${count} tests, codex=${codex})"
  local status=0
  (
    cd "${go_dir}"
    run_isolated env -u CODEX_GO_SDK_AUTH_FIXTURE_PATH -u CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE \
      CODEX_EXEC_PATH="${codex}" \
      CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1 \
      go test -race -count=1 -v -run '^TestRealAppServer' ./...
  ) 2>&1 | tee "${scratch}/go-test.log" || status=$?
  [[ ${status} -eq 0 ]] || die "TestRealAppServer* failed (exit ${status})"

  # Every listed test must report PASS; SKIP is accepted only off Linux, where
  # Linux-only cases skip. The strict handshake must pass everywhere.
  local name missing=() skipped=0
  while IFS= read -r name; do
    if grep -qE "^--- PASS: ${name} \(" "${scratch}/go-test.log"; then
      continue
    elif [[ "$(uname -s)" != Linux && "${name}" != "${strict_handshake_test}" ]] &&
      grep -qE "^--- SKIP: ${name} \(" "${scratch}/go-test.log"; then
      skipped=$((skipped + 1))
    else
      missing+=("${name}")
    fi
  done <<<"${listed}"
  [[ ${#missing[@]} -eq 0 ]] || die "listed TestRealAppServer tests did not pass: ${missing[*]}"
  log "$((count - skipped)) of ${count} TestRealAppServer tests passed$([[ ${skipped} -eq 0 ]] || echo ", ${skipped} skipped off Linux")"
}

cmd_scripts() {
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
  log "ci-local.sh self-test (stub cargo, go, sysctl, unshare, bwrap)"
  "${repo_root}/scripts/fork/test-ci-local.sh"
  log "verify-release.sh self-test"
  "${selftest[@]}"
  log "release.sh self-test (stub gh)"
  "${repo_root}/scripts/fork/test-release.sh"
}

cmd_all() {
  cmd_rust_fmt
  cmd_rust_clippy
  cmd_rust_test
  cmd_go_static
  cmd_go_test
  cmd_go_runtime_build
  cmd_go_generators
  cmd_go_real_app_server
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
    sandbox-check) cmd_sandbox_check "$@" ;;
    go-static) cmd_go_static "$@" ;;
    go-test) cmd_go_test "$@" ;;
    go-runtime-build) cmd_go_runtime_build "$@" ;;
    go-generators) cmd_go_generators "$@" ;;
    go-real-app-server) cmd_go_real_app_server "$@" ;;
    scripts) cmd_scripts "$@" ;;
    all) cmd_all "$@" ;;
    -h | --help | help) usage ;;
    *)
      usage >&2
      die "unknown command: ${command}"
      ;;
  esac
}

main "$@"
