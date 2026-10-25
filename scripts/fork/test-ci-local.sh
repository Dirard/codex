#!/usr/bin/env bash
# Self-test for scripts/fork/ci-local.sh.
#
#   scripts/fork/test-ci-local.sh
#
# Runs the ci-local.sh branches that otherwise need a long Rust or Go run, or a
# CI runner: helper builds and the silent-skip guard of rust-test, the
# rust-app-server-test wrapper, the nextest version check, sandbox-check and
# the go-real-app-server and go-auth-fixture report checks. cargo, go, sysctl,
# unshare, bwrap and uname are stubs placed first on PATH, so the test is
# hermetic and builds nothing.

if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 4))); then
  printf '%s: requires bash >= 4.4 (running %s)\n' "${0##*/}" "${BASH_VERSION}" >&2
  exit 1
fi

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
ci_local="${repo_root}/scripts/fork/ci-local.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/test-ci-local.XXXXXX")"
trap 'rm -rf "${work}"' EXIT

mkdir -p "${work}/bin" "${work}/tmp"

# Stub cargo: build creates the requested --bin files, nextest writes the JUnit
# fixture where nextest would. With the fork-ci tool config (success output
# stored) it writes $STUB_STATE/junit.xml as is; without it, the testcases
# only, as nextest does for passing tests.
cat >"${work}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
state="${STUB_STATE:?}"
target="${CARGO_TARGET_DIR:?}"
printf 'cargo %s\n' "$*" >>"${state}/calls"
case "$1" in
  build)
    shift
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == --bin ]]; then
        mkdir -p "${target}/debug"
        printf '#!/bin/sh\n' >"${target}/debug/$2"
        chmod 0755 "${target}/debug/$2"
        shift 2
      else
        shift
      fi
    done
    ;;
  test) ;;
  nextest)
    if [[ "${2:-}" == --version ]]; then
      printf 'cargo-nextest %s (stub)\nrelease: %s\n' "$(cat "${state}/nextest-version")" "$(cat "${state}/nextest-version")"
      exit 0
    fi
    profile="${NEXTEST_PROFILE:-default}"
    tool_config=""
    args=("$@")
    for ((i = 0; i < ${#args[@]}; i++)); do
      if [[ "${args[i]}" == --tool-config-file ]]; then
        tool_config="${args[i + 1]#*:}"
      fi
    done
    if [[ ! -f "${state}/no-junit" ]]; then
      mkdir -p "${target}/nextest/${profile}"
      if [[ -n "${tool_config}" ]] &&
        grep -qx "\[profile\.${profile}\.junit\]" "${tool_config}" &&
        grep -qx 'store-success-output = true' "${tool_config}"; then
        cp "${state}/junit.xml" "${target}/nextest/${profile}/junit.xml"
      elif [[ "${profile}" == default ]]; then
        grep -E '<testsuites|<testsuite |<testcase|</testcase>|</testsuite>|</testsuites>' \
          "${state}/junit.xml" >"${target}/nextest/${profile}/junit.xml"
      fi
    fi
    exit "$(cat "${state}/nextest-status")"
    ;;
  *)
    echo "stub cargo: unexpected call: $*" >&2
    exit 99
    ;;
esac
STUB

# Stub go: `go env NAME` and `go test` (-list prints $STUB_STATE/go-list, a
# run records the CODEX_* environment as a `go-env` call line, prints
# $STUB_STATE/go-log and exits with $STUB_STATE/go-status).
cat >"${work}/bin/go" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
state="${STUB_STATE:?}"
printf 'go %s\n' "$*" >>"${state}/calls"
case "$1" in
  env) printf '%s\n' "${state}/go-env-$2" ;;
  test)
    if [[ " $* " == *" -list "* ]]; then
      cat "${state}/go-list"
      echo "ok   github.com/openai/codex/sdk/go 0.01s"
    else
      printf 'go-env %s\n' "$(env | grep -E '^CODEX_' | LC_ALL=C sort | tr '\n' ' ')" >>"${state}/calls"
      cat "${state}/go-log"
      exit "$(cat "${state}/go-status")"
    fi
    ;;
  *)
    echo "stub go: unexpected call: $*" >&2
    exit 99
    ;;
esac
STUB

# Stub sysctl: `sysctl -n KEY` reads $STUB_STATE/sysctl/KEY; a missing file is
# a missing key, reported the way procps reports it.
cat >"${work}/bin/sysctl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
state="${STUB_STATE:?}"
[[ "$1" == -n ]] || { echo "stub sysctl: unexpected call: $*" >&2; exit 99; }
file="${state}/sysctl/$2"
if [[ -f "${file}" ]]; then
  cat "${file}"
else
  echo "sysctl: cannot stat /proc/sys/${2//.//}: No such file or directory" >&2
  exit 255
fi
STUB

for tool in unshare bwrap; do
  cat >"${work}/bin/${tool}" <<STUB
#!/usr/bin/env bash
printf '${tool} %s\n' "\$*" >>"\${STUB_STATE:?}/calls"
exit "\$(cat "\${STUB_STATE}/${tool}-status")"
STUB
done

# Stub uname: `uname -s` prints $STUB_UNAME (Linux by default).
real_uname="$(command -v uname)"
cat >"${work}/bin/uname" <<STUB
#!/usr/bin/env bash
if [[ "\${1:-}" == -s ]]; then
  printf '%s\n' "\${STUB_UNAME:-Linux}"
else
  exec "${real_uname}" "\$@"
fi
STUB
chmod 0755 "${work}/bin/"*
export PATH="${work}/bin:${PATH}"
export TMPDIR="${work}/tmp"
unset CI NEXTEST_PROFILE GITHUB_STEP_SUMMARY CODEX_EXEC_PATH CODEX_GO_SDK_AUTH_FIXTURE_PATH STUB_UNAME

passed=0
failures=0

# new_state: fresh stub state with passing defaults; prints its directory.
new_state() {
  local dir
  dir="$(mktemp -d "${work}/state.XXXXXX")"
  : >"${dir}/calls"
  echo 0.9.103 >"${dir}/nextest-version"
  echo 0 >"${dir}/nextest-status"
  echo 0 >"${dir}/go-status"
  echo 0 >"${dir}/unshare-status"
  echo 0 >"${dir}/bwrap-status"
  mkdir -p "${dir}/sysctl" "${dir}/target"
  echo 1 >"${dir}/sysctl/kernel.unprivileged_userns_clone"
  echo 0 >"${dir}/sysctl/kernel.apparmor_restrict_unprivileged_userns"
  junit "${dir}" plain-test ""
  printf '%s\n' "${dir}"
}

# junit STATE (NAME OUTPUT)...: writes a nextest-shaped JUnit report of
# passing tests in suite codex-core::all; OUTPUT goes to <system-err>.
junit() {
  local state="$1"
  shift
  {
    printf '<?xml version="1.0" encoding="UTF-8"?>\n'
    printf '<testsuites name="nextest-run" tests="%s" failures="0" errors="0">\n' "$(($# / 2))"
    printf '    <testsuite name="codex-core::all" tests="%s" disabled="0" errors="0" failures="0">\n' "$(($# / 2))"
    while [[ $# -gt 0 ]]; do
      printf '        <testcase name="%s" classname="codex-core::all" timestamp="2026-10-25T00:00:00.000+00:00" time="0.010">\n' "$1"
      printf '            <system-out>\nrunning 1 test\ntest %s ... ok\n</system-out>\n' "$1"
      printf '            <system-err>%s\n</system-err>\n' "$2"
      printf '        </testcase>\n'
      shift 2
    done
    printf '    </testsuite>\n</testsuites>\n'
  } >"${state}/junit.xml"
}

# check NAME ok|fail PATTERN COMMAND...: runs COMMAND with the current stub
# state, expects the status and an output line matching the extended regular
# expression PATTERN.
check() {
  local name="$1" want="$2" pattern="$3"
  shift 3
  local status=0
  CARGO_TARGET_DIR="${STUB_STATE}/target" "$@" >"${work}/${name}.log" 2>&1 || status=$?
  local got=ok
  [[ ${status} -eq 0 ]] || got=fail
  if [[ "${got}" == "${want}" ]] && grep -qE -- "${pattern}" "${work}/${name}.log"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}: status ${status}, want ${want} with /${pattern}/" >&2
    sed 's/^/    /' "${work}/${name}.log" >&2
    failures=$((failures + 1))
  fi
}

# expect NAME CONDITION...: records a check of a shell condition.
expect() {
  local name="$1"
  shift
  if "$@"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}" >&2
    failures=$((failures + 1))
  fi
}

calls_contain() { grep -qE -- "$1" "${STUB_STATE}/calls"; }
calls_lack() { ! grep -qE -- "$1" "${STUB_STATE}/calls"; }
calls_empty() { [[ ! -s "${STUB_STATE}/calls" ]]; }

# --- rust-test: helpers and the silent-skip guard -------------------------

STUB_STATE="$(new_state)"
export STUB_STATE
check rust-test-clean ok 'nextest run' "${ci_local}" rust-test -p codex-core
expect helpers-include-streamable-http calls_contain '^cargo build .*-p codex-rmcp-client --bin test_stdio_server --bin test_streamable_http_server'
expect nextest-gets-tool-config calls_contain '^cargo nextest run .*--tool-config-file fork-ci:'

STUB_STATE="$(new_state)"
junit "${STUB_STATE}" suite::rmcp_client::streamable_http_with_oauth_round_trip \
  'test_streamable_http_server binary not available, skipping test: could not locate binary &quot;test_streamable_http_server&quot;' \
  suite::other::passes ""
check helper-skip-fails fail 'codex-core::all suite::rmcp_client::streamable_http_with_oauth_round_trip' \
  "${ci_local}" rust-test -p codex-core

STUB_STATE="$(new_state)"
junit "${STUB_STATE}" suite::exec::uses_helper \
  'helper lookup: CARGO_BIN_EXE env var CARGO_BIN_EXE_x resolved to &quot;/nope/x&quot;, but it does not exist'
check cargo-bin-error-fails fail 'suite::exec::uses_helper' "${ci_local}" rust-test -p codex-core

STUB_STATE="$(new_state)"
junit "${STUB_STATE}" suite::sandbox::bwrap_probe 'skipping bwrap test: bwrap sandbox prerequisites are unavailable'
GITHUB_STEP_SUMMARY="${STUB_STATE}/summary.md" \
  check plain-skip-listed ok 'skip message.*suite::sandbox::bwrap_probe|suite::sandbox::bwrap_probe.*skipping' \
  "${ci_local}" rust-test -p codex-core
expect plain-skip-in-summary grep -q 'suite::sandbox::bwrap_probe' "${STUB_STATE}/summary.md"

STUB_STATE="$(new_state)"
mkdir -p "${STUB_STATE}/target/nextest/default"
cp "${STUB_STATE}/junit.xml" "${STUB_STATE}/target/nextest/default/junit.xml"
touch "${STUB_STATE}/no-junit"
check stale-or-missing-junit-fails fail 'no JUnit report' "${ci_local}" rust-test -p codex-core

STUB_STATE="$(new_state)"
touch "${STUB_STATE}/no-junit"
echo 101 >"${STUB_STATE}/nextest-status"
check nextest-failed-before-junit fail 'nextest failed \(exit 101\) before writing' \
  "${ci_local}" rust-test -p codex-core

STUB_STATE="$(new_state)"
check profile-argument-rejected fail 'NEXTEST_PROFILE' "${ci_local}" rust-test -p codex-core -- --profile ci

STUB_STATE="$(new_state)"
junit "${STUB_STATE}" suite::rmcp_client::plugin_helper 'test_stdio_server binary not available, skipping test: x'
echo 100 >"${STUB_STATE}/nextest-status"
check nextest-failure-and-helper-skip fail 'suite::rmcp_client::plugin_helper' "${ci_local}" rust-test -p codex-core

STUB_STATE="$(new_state)"
junit "${STUB_STATE}" suite::rmcp_client::plugin_helper 'test_stdio_server binary not available, skipping test: x'
NEXTEST_PROFILE=local check local-profile-guarded fail 'suite::rmcp_client::plugin_helper' \
  "${ci_local}" rust-test -p codex-core

STUB_STATE="$(new_state)"
check app-server-test-runs ok 'nextest run' "${ci_local}" rust-app-server-test --partition hash:2/4
expect app-server-test-builds-tests calls_contain '^cargo test --locked --no-run -p codex-app-server$'
expect app-server-test-partition calls_contain '^cargo nextest run .*-p codex-app-server .*--partition hash:2/4'
expect app-server-test-helpers calls_contain '^cargo build .*-p codex-cli --bin codex -p codex-code-mode-host'

STUB_STATE="$(new_state)"
check app-server-test-rejects-package fail 'tests codex-app-server only' \
  "${ci_local}" rust-app-server-test -p codex-core
expect app-server-test-rejected-before-build calls_empty

STUB_STATE="$(new_state)"
junit "${STUB_STATE}" suite::v2::executor_mcp::uses_codex 'helper lookup: could not locate binary &quot;codex&quot;'
check app-server-test-helper-guard fail 'suite::v2::executor_mcp::uses_codex' \
  "${ci_local}" rust-app-server-test

STUB_STATE="$(new_state)"
echo 0.9.111 >"${STUB_STATE}/nextest-version"
CI=true check nextest-version-ci fail 'nextest 0\.9\.111, want 0\.9\.103' "${ci_local}" rust-test -p codex-core
STUB_STATE="$(new_state)"
echo 0.9.111 >"${STUB_STATE}/nextest-version"
check nextest-version-local ok 'warning: .*nextest 0\.9\.111, want 0\.9\.103' "${ci_local}" rust-test -p codex-core

# --- sandbox-check --------------------------------------------------------

STUB_STATE="$(new_state)"
check sandbox-ok ok 'user namespaces and bubblewrap work' "${ci_local}" sandbox-check
expect sandbox-probes-run calls_contain '^unshare --user --map-root-user true$'
expect sandbox-bwrap-probe-runs calls_contain '^bwrap --unshare-user --ro-bind / / true$'

STUB_STATE="$(new_state)"
echo 1 >"${STUB_STATE}/sysctl/kernel.apparmor_restrict_unprivileged_userns"
check sandbox-apparmor-restricted fail 'kernel\.apparmor_restrict_unprivileged_userns is 1, want 0' "${ci_local}" sandbox-check

STUB_STATE="$(new_state)"
echo 0 >"${STUB_STATE}/sysctl/kernel.unprivileged_userns_clone"
check sandbox-userns-clone-off fail 'kernel\.unprivileged_userns_clone is 0, want 1' "${ci_local}" sandbox-check

STUB_STATE="$(new_state)"
echo 1 >"${STUB_STATE}/unshare-status"
check sandbox-unshare-fails fail 'unshare --user --map-root-user true failed' "${ci_local}" sandbox-check

STUB_STATE="$(new_state)"
echo 1 >"${STUB_STATE}/bwrap-status"
check sandbox-bwrap-fails fail 'bwrap --unshare-user --ro-bind / / true failed' "${ci_local}" sandbox-check

STUB_STATE="$(new_state)"
rm "${STUB_STATE}/sysctl/"*
check sandbox-keys-absent ok 'user namespaces and bubblewrap work' "${ci_local}" sandbox-check

# --- go-real-app-server ---------------------------------------------------

strict=TestRealAppServerInitializeStrictDigest
go_state() {
  STUB_STATE="$(new_state)"
  printf '#!/bin/sh\n' >"${STUB_STATE}/codex"
  chmod 0755 "${STUB_STATE}/codex"
  printf '%s\n' "$@" >"${STUB_STATE}/go-list"
  local name
  : >"${STUB_STATE}/go-log"
  for name in "$@"; do
    printf '=== RUN   %s\n--- PASS: %s (0.10s)\n' "${name}" "${name}" >>"${STUB_STATE}/go-log"
  done
  printf 'PASS\nok   github.com/openai/codex/sdk/go 1.0s\n' >>"${STUB_STATE}/go-log"
}

go_state "${strict}" TestRealAppServerTurn
CODEX_EXEC_PATH="${STUB_STATE}/codex" check go-real-all-pass ok '2 of 2 TestRealAppServer tests passed' \
  "${ci_local}" go-real-app-server

go_state
: >"${STUB_STATE}/go-list"
printf 'testing: warning: no tests to run\nPASS\n' >"${STUB_STATE}/go-log"
CODEX_EXEC_PATH="${STUB_STATE}/codex" check go-real-none-listed fail 'no TestRealAppServer tests' \
  "${ci_local}" go-real-app-server

go_state TestRealAppServerTurn
CODEX_EXEC_PATH="${STUB_STATE}/codex" check go-real-strict-not-listed fail "${strict} is not listed" \
  "${ci_local}" go-real-app-server

go_state "${strict}" TestRealAppServerTurn
printf '%s\n' "${strict}" TestRealAppServerTurn TestRealAppServerSilent >"${STUB_STATE}/go-list"
CODEX_EXEC_PATH="${STUB_STATE}/codex" check go-real-test-without-pass fail 'did not pass: TestRealAppServerSilent' \
  "${ci_local}" go-real-app-server

go_state "${strict}" TestRealAppServerTurn
sed -i 's/--- PASS: TestRealAppServerTurn /--- SKIP: TestRealAppServerTurn /' "${STUB_STATE}/go-log"
CODEX_EXEC_PATH="${STUB_STATE}/codex" STUB_UNAME=Darwin \
  check go-real-skip-off-linux ok '1 of 2 TestRealAppServer tests passed, 1 skipped off Linux' \
  "${ci_local}" go-real-app-server
CODEX_EXEC_PATH="${STUB_STATE}/codex" \
  check go-real-skip-on-linux fail 'did not pass: TestRealAppServerTurn' "${ci_local}" go-real-app-server

go_state "${strict}" TestRealAppServerTurn
sed -i "s/--- PASS: ${strict} /--- SKIP: ${strict} /" "${STUB_STATE}/go-log"
CODEX_EXEC_PATH="${STUB_STATE}/codex" STUB_UNAME=Darwin \
  check go-real-strict-skip-off-linux fail "did not pass: ${strict}" "${ci_local}" go-real-app-server

# --- go-auth-fixture ------------------------------------------------------

# auth_state NAME...: go_state plus a codex-app-server at the default path.
auth_state() {
  go_state "$@"
  mkdir -p "${STUB_STATE}/target/debug"
  printf '#!/bin/sh\n' >"${STUB_STATE}/target/debug/codex-app-server"
  chmod 0755 "${STUB_STATE}/target/debug/codex-app-server"
}

auth_state TestAuthFixtureDeviceCodeFlow TestAuthFixtureUsageAndRateLimitRead
CODEX_EXEC_PATH="${STUB_STATE}/codex" check go-auth-all-pass ok '2 of 2 TestAuthFixture tests passed' \
  "${ci_local}" go-auth-fixture
expect go-auth-default-binary calls_contain "^go-env .*CODEX_GO_SDK_AUTH_FIXTURE_PATH=${STUB_STATE}/target/debug/codex-app-server "
expect go-auth-required calls_contain '^go-env .*CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE=1 '
expect go-auth-without-exec-path calls_lack '^go-env .*CODEX_EXEC_PATH='
expect go-auth-runs-prefix calls_contain "^go test -race -count=1 -v -run \^TestAuthFixture \./\.\.\.$"

auth_state TestAuthFixtureDeviceCodeFlow
printf '#!/bin/sh\n' >"${STUB_STATE}/fixture-app-server"
chmod 0755 "${STUB_STATE}/fixture-app-server"
CODEX_GO_SDK_AUTH_FIXTURE_PATH="${STUB_STATE}/fixture-app-server" \
  check go-auth-env-binary ok '1 of 1 TestAuthFixture tests passed' "${ci_local}" go-auth-fixture
expect go-auth-env-binary-used calls_contain "^go-env .*CODEX_GO_SDK_AUTH_FIXTURE_PATH=${STUB_STATE}/fixture-app-server "

auth_state TestAuthFixtureDeviceCodeFlow
rm "${STUB_STATE}/target/debug/codex-app-server"
check go-auth-missing-binary fail 'missing executable .*/codex-app-server' "${ci_local}" go-auth-fixture

auth_state
: >"${STUB_STATE}/go-list"
printf 'testing: warning: no tests to run\nPASS\n' >"${STUB_STATE}/go-log"
check go-auth-none-listed fail 'no TestAuthFixture tests' "${ci_local}" go-auth-fixture

auth_state TestAuthFixtureDeviceCodeFlow TestAuthFixtureUsageAndRateLimitRead
sed -i 's/--- PASS: TestAuthFixtureUsageAndRateLimitRead /--- SKIP: TestAuthFixtureUsageAndRateLimitRead /' \
  "${STUB_STATE}/go-log"
check go-auth-skip-fails fail 'did not pass: TestAuthFixtureUsageAndRateLimitRead' "${ci_local}" go-auth-fixture

auth_state TestAuthFixtureDeviceCodeFlow
echo 1 >"${STUB_STATE}/go-status"
check go-auth-run-fails fail 'TestAuthFixture\* failed \(exit 1\)' "${ci_local}" go-auth-fixture

auth_state TestAuthFixtureDeviceCodeFlow
STUB_UNAME=Darwin check go-auth-linux-only fail 'Linux-only' "${ci_local}" go-auth-fixture
expect go-auth-linux-only-runs-nothing calls_lack '^go test'

# --- rusty_v8: a cached build-script run without its static library --------

STUB_STATE="$(new_state)"
export STUB_STATE
v8_target="${STUB_STATE}/target"
mkdir -p "${v8_target}/debug/build/v8-run1" "${v8_target}/debug/build/v8-script1" \
  "${v8_target}/debug/.fingerprint/v8-run1" "${v8_target}/debug/.fingerprint/codex-core-1"
printf 'cargo:rustc-link-lib=static=rusty_v8\ncargo:rustc-link-search=%s\n' \
  "${v8_target}/debug/gn_out/obj" >"${v8_target}/debug/build/v8-run1/output"
check rusty-v8-missing-lib-repaired ok 'librusty_v8\.a is missing' "${ci_local}" rust-helpers
expect rusty-v8-build-dirs-removed test ! -e "${v8_target}/debug/build/v8-run1"
expect rusty-v8-fingerprint-removed test ! -e "${v8_target}/debug/.fingerprint/v8-run1"
expect rusty-v8-other-fingerprints-kept test -d "${v8_target}/debug/.fingerprint/codex-core-1"

STUB_STATE="$(new_state)"
export STUB_STATE
v8_target="${STUB_STATE}/target"
mkdir -p "${v8_target}/debug/build/v8-run1" "${v8_target}/debug/gn_out/obj" "${v8_target}/debug/.fingerprint/v8-run1"
: >"${v8_target}/debug/gn_out/obj/librusty_v8.a"
printf 'cargo:rustc-link-search=native=%s\n' "${v8_target}/debug/gn_out/obj" \
  >"${v8_target}/debug/build/v8-run1/output"
check rusty-v8-present-lib-kept ok 'build test helpers' "${ci_local}" rust-helpers
expect rusty-v8-present-build-kept test -f "${v8_target}/debug/build/v8-run1/output"
expect rusty-v8-present-fingerprint-kept test -d "${v8_target}/debug/.fingerprint/v8-run1"

echo "ci-local.sh self-test: ${passed} passed, ${failures} failed"
[[ ${failures} -eq 0 ]]
