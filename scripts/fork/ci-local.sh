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
#   rust-test [OPTIONS]   build helpers, then nextest with an isolated HOME
#       --partition SPEC    nextest partition, for example hash:1/3
#       -p PACKAGE          test this package instead of the defaults (repeatable)
#       -- ARGS...          extra arguments for `cargo nextest run`
#   go-static             gofmt and go vet for sdk/go
#   go-test               go test -race ./... (real-runtime tests skip)
#   go-runtime-build      build codex and write_go_sdk_manifest
#   go-generators         Go manifest, experimental schema and protocodex drift checks
#   go-real-app-server    TestRealAppServer* (strict handshake included) against codex
#   scripts               shellcheck and the verify-release.sh / release.sh self-tests
#   all                   every command above, in CI order
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
#
# Tests run with HOME and XDG_CONFIG_HOME in a fresh temporary directory so host
# state such as ~/.agents/skills does not leak into snapshots. CARGO_HOME,
# RUSTUP_HOME and the Go caches stay pinned to their real locations; otherwise
# cargo and rustup would reinstall into the temporary HOME and rebuild target/.

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
test_helper_bins=(codex codex-code-mode-host test_stdio_server)

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
    -p codex-rmcp-client --bin test_stdio_server
  local bin dir
  dir="$(target_dir)/debug"
  for bin in "${test_helper_bins[@]}"; do
    require_built "${dir}/${bin}"
  done
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

  local package_args=() package
  for package in "${packages[@]}"; do
    package_args+=(-p "${package}")
  done

  cmd_rust_helpers
  log "build test binaries: ${packages[*]}"
  cargo_rs test --locked --no-run "${package_args[@]}"

  local nextest_args=(nextest run --locked --no-fail-fast "${package_args[@]}")
  if [[ -n "${partition}" ]]; then
    nextest_args+=(--partition "${partition}")
  fi
  log "cargo ${nextest_args[*]} ${extra[*]}"
  (
    cd "${rust_dir}"
    export RUST_MIN_STACK=8388608 INSTA_UPDATE=no
    run_isolated cargo "${nextest_args[@]}" "${extra[@]}"
  )
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
  rm -rf "${scratch}"

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
  log "go test -race TestRealAppServer* (codex=${codex})"
  (
    cd "${go_dir}"
    run_isolated env -u CODEX_GO_SDK_AUTH_FIXTURE_PATH -u CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE \
      CODEX_EXEC_PATH="${codex}" \
      CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1 \
      go test -race -count=1 -v -run '^TestRealAppServer' ./...
  )
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
