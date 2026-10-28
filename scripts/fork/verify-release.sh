#!/usr/bin/env bash
# Verify fork-rust-release packages before they are attached to a release.
#
#   scripts/fork/verify-release.sh --version X.Y.Z --dist DIR [options]
#
# DIR holds the packages produced by .github/workflows/fork-rust-release.yml
# (codex-package-<target>.tar.gz, or .zip for Windows) and nothing else except
# an optional SHA256SUMS.
#
# Options:
#   --targets "T ..."   targets that must be present (default: all five)
#   --work-dir DIR      empty extraction directory to keep (default: a temporary
#                       one, removed on exit)
#   --go-sdk DIR        Go SDK module for the strict handshake (default: sdk/go of
#                       this checkout; it must come from the release commit)
#   --skip-runtime      do not run the Linux x86_64 package
#
# Checks, in order, failing on the first problem:
#   1. DIR contains exactly one package per expected target.
#   2. Each archive passes its integrity test.
#   3. Each archive contains exactly bin/ and the expected binaries: regular
#      files only, safe names, tar modes drwxr-xr-x / -rwxr-xr-x (0755).
#   4. file(1) reports the expected format and architecture for every binary.
#   5. SHA256SUMS lists every package: written when absent, compared when present.
#   6. On an x86_64 Linux host: `codex --version` prints `codex-cli X.Y.Z`, and
#      the Go SDK strict initialize handshake passes against that binary: its
#      output must show `--- PASS: TestRealAppServerInitializeStrictDigest`,
#      because `go test -run` also exits 0 when no test matches.
#
# Prints VERIFY_OK on success. Appends a summary to $GITHUB_STEP_SUMMARY when set.
# Requires Linux tools: bash >= 4.4, GNU-compatible coreutils (GNU or uutils:
# stat -c) and findutils (GNU or bfs: find -printf).

# Written for bash 3.2 too, so an old bash reaches this message.
if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 4))); then
  printf '%s: requires bash >= 4.4 (running %s); run it on Linux\n' "${0##*/}" "${BASH_VERSION}" >&2
  exit 1
fi

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

# Temporary directories removed on exit, whatever the outcome. A broken archive
# can extract directories without search permission, so restore it first.
cleanup_paths=()
cleanup() {
  [[ ${#cleanup_paths[@]} -gt 0 ]] || return 0
  chmod -R u+rwx -- "${cleanup_paths[@]}" 2>/dev/null || true
  rm -rf -- "${cleanup_paths[@]}"
}
trap cleanup EXIT

all_targets=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
  x86_64-pc-windows-msvc
)

log() { printf '==> %s\n' "$*" >&2; }
die() {
  printf 'verify-release: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed -E 's/^# ?//'
}

strict_handshake_test=TestRealAppServerInitializeStrictDigest

# Requires the GNU-compatible options used below; BSD and macOS tools lack them.
require_tool_features() {
  local probe
  probe="$(mktemp -d "${TMPDIR:-/tmp}/verify-release-probe.XXXXXX")"
  cleanup_paths+=("${probe}")
  printf 'x' >"${probe}/f"
  [[ "$(find "${probe}" -mindepth 1 -maxdepth 1 -name f -printf '%f\n' 2>/dev/null)" == f ]] ||
    die "requires GNU-compatible findutils (find -printf): GNU findutils or bfs, as on Linux"
  [[ "$(stat -c %s "${probe}/f" 2>/dev/null)" == 1 ]] ||
    die "requires GNU-compatible coreutils (stat -c): GNU or uutils coreutils, as on Linux"
}

# Binaries packaged for a target: both bundles of scripts/fork/release-package.sh,
# which assembles the packages that fork-rust-release.yml verifies.
expected_bins() {
  local target="$1"
  local bins=(codex codex-app-server codex-code-mode-host codex-responses-api-proxy)
  case "${target}" in
    *linux*) bins+=(bwrap) ;;
    *windows*)
      bins+=(codex-command-runner codex-windows-sandbox-setup)
      bins=("${bins[@]/%/.exe}")
      ;;
  esac
  printf '%s\n' "${bins[@]}" | LC_ALL=C sort
}

# Extended regular expression that `file -b` output must match.
expected_file_type() {
  case "$1" in
    aarch64-apple-darwin) printf '%s\n' '^Mach-O 64-bit arm64 executable' ;;
    x86_64-apple-darwin) printf '%s\n' '^Mach-O 64-bit x86_64 executable' ;;
    aarch64-unknown-linux-gnu) printf '%s\n' '^ELF 64-bit LSB [^,]*, ARM aarch64,' ;;
    x86_64-unknown-linux-gnu) printf '%s\n' '^ELF 64-bit LSB [^,]*, x86-64,' ;;
    x86_64-pc-windows-msvc) printf '%s\n' '^PE32\+ executable.*x86-64' ;;
    *) return 1 ;;
  esac
}

package_name() {
  if [[ "$1" == *windows* ]]; then
    printf 'codex-package-%s.zip\n' "$1"
  else
    printf 'codex-package-%s.tar.gz\n' "$1"
  fi
}

# Verifies one archive's listing and extracts it into $2.
check_and_extract() {
  local target="$1" dest="$2" pkg="$3"
  local entries expected
  expected="$(expected_bins "${target}")"
  mkdir -p "${dest}"

  if [[ "${pkg}" == *.zip ]]; then
    unzip -tq "${pkg}" >/dev/null || die "${target}: zip integrity test failed"
    entries="$(unzip -Z1 "${pkg}")"
  else
    gzip -t "${pkg}" || die "${target}: gzip integrity test failed"
    entries="$(tar -tzf "${pkg}")" || die "${target}: tar listing failed"
    # Every entry must be the bin/ directory or a 0755 regular file inside it.
    local bad
    bad="$(tar -tvzf "${pkg}" | awk '
      $NF == "bin/" { if ($1 != "drwxr-xr-x") print; next }
      { if ($1 != "-rwxr-xr-x") print }
    ')"
    [[ -z "${bad}" ]] || die "${target}: unexpected tar entry type or mode (want 0755 regular files):
${bad}"
  fi

  local entry names=()
  while IFS= read -r entry; do
    [[ -n "${entry}" ]] || continue
    if [[ "${entry}" == "bin/" ]]; then
      continue
    fi
    [[ "${entry}" =~ ^bin/[A-Za-z0-9][A-Za-z0-9._-]*$ ]] ||
      die "${target}: unexpected archive entry ${entry}"
    names+=("${entry#bin/}")
  done <<<"${entries}"
  local actual
  actual="$(printf '%s\n' "${names[@]}" | LC_ALL=C sort)"
  [[ "${actual}" == "${expected}" ]] || die "${target}: layout mismatch
expected:
${expected}
actual:
${actual}"

  if [[ "${pkg}" == *.zip ]]; then
    unzip -oq "${pkg}" -d "${dest}"
  else
    tar -xzf "${pkg}" -C "${dest}"
  fi

  local bin path kind pattern
  pattern="$(expected_file_type "${target}")"
  while IFS= read -r bin; do
    path="${dest}/bin/${bin}"
    [[ -f "${path}" && ! -L "${path}" ]] || die "${target}: ${bin} is not a regular file after extraction"
    kind="$(file -b "${path}")"
    [[ "${kind}" =~ ${pattern} ]] || die "${target}: ${bin} has unexpected type: ${kind}"
  done <<<"${expected}"
}

main() {
  local version="" dist="" work_dir="" go_sdk="${repo_root}/sdk/go" skip_runtime=0
  local targets=("${all_targets[@]}")
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --dist) dist="${2:?--dist requires a directory}"; shift 2 ;;
      --targets)
        read -r -a targets <<<"${2:?--targets requires a list}"
        shift 2
        ;;
      --work-dir) work_dir="${2:?--work-dir requires a directory}"; shift 2 ;;
      --go-sdk) go_sdk="${2:?--go-sdk requires a directory}"; shift 2 ;;
      --skip-runtime) skip_runtime=1; shift ;;
      -h | --help) usage; exit 0 ;;
      *) usage >&2; die "unknown option: $1" ;;
    esac
  done
  [[ -n "${version}" && -n "${dist}" ]] || {
    usage >&2
    die "--version and --dist are required"
  }
  [[ "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-(alpha(\.[0-9]+){0,2}|beta(\.[0-9]+)?))?$ ]] ||
    die "invalid version ${version}"
  [[ -d "${dist}" ]] || die "missing dist directory ${dist}"
  dist="$(cd -- "${dist}" && pwd)"
  [[ ${#targets[@]} -gt 0 ]] || die "no targets given"
  require_tool_features

  local target
  for target in "${targets[@]}"; do
    expected_file_type "${target}" >/dev/null || die "unknown target ${target}"
  done

  # 1. Exact package set.
  local expected_files actual_files
  expected_files="$(for target in "${targets[@]}"; do package_name "${target}"; done | LC_ALL=C sort)"
  actual_files="$(find "${dist}" -mindepth 1 -maxdepth 1 ! -name SHA256SUMS -printf '%f\n' | LC_ALL=C sort)"
  [[ "${actual_files}" == "${expected_files}" ]] || die "package set mismatch in ${dist}
expected:
${expected_files}
actual:
${actual_files}"
  local name
  while IFS= read -r name; do
    [[ -f "${dist}/${name}" && ! -L "${dist}/${name}" ]] || die "${name} is not a regular file"
  done <<<"${expected_files}"

  if [[ -z "${work_dir}" ]]; then
    # Extracted binaries are large; pass --work-dir to keep them for inspection.
    work_dir="$(mktemp -d "${TMPDIR:-/tmp}/verify-release.XXXXXX")"
    cleanup_paths+=("${work_dir}")
  else
    mkdir -p "${work_dir}"
    work_dir="$(cd -- "${work_dir}" && pwd)"
    [[ -z "$(find "${work_dir}" -mindepth 1 -maxdepth 1 -print -quit)" ]] ||
      die "work dir ${work_dir} is not empty"
  fi

  # 2-4. Integrity, layout, modes and architectures.
  for target in "${targets[@]}"; do
    log "check ${target}"
    check_and_extract "${target}" "${work_dir}/${target}" "${dist}/$(package_name "${target}")"
  done

  # 5. SHA256SUMS.
  local sums
  sums="$(cd "${dist}" && while IFS= read -r name; do sha256sum -- "${name}"; done <<<"${expected_files}")"
  if [[ -e "${dist}/SHA256SUMS" ]]; then
    [[ "$(cat "${dist}/SHA256SUMS")" == "${sums}" ]] ||
      die "existing ${dist}/SHA256SUMS does not match the packages"
    log "SHA256SUMS matches"
  else
    printf '%s\n' "${sums}" >"${dist}/SHA256SUMS"
    log "wrote ${dist}/SHA256SUMS"
  fi

  # 6. Runtime smoke on the matching host.
  local runtime="skipped"
  if [[ ${skip_runtime} -eq 0 && "$(uname -s)/$(uname -m)" == "Linux/x86_64" ]] &&
    printf '%s\n' "${targets[@]}" | grep -qx x86_64-unknown-linux-gnu; then
    local codex="${work_dir}/x86_64-unknown-linux-gnu/bin/codex" actual_version
    actual_version="$("${codex}" --version)" || die "codex --version failed"
    [[ "${actual_version}" == "codex-cli ${version}" ]] ||
      die "codex --version printed '${actual_version}', want 'codex-cli ${version}'"
    log "codex --version: ${actual_version}"

    [[ -f "${go_sdk}/go.mod" ]] || die "missing Go SDK at ${go_sdk}"
    local home
    home="$(mktemp -d "${TMPDIR:-/tmp}/verify-release-home.XXXXXX")"
    cleanup_paths+=("${home}")
    mkdir -p "${home}/.config" "${home}/codex-home"
    log "strict initialize handshake (Go SDK ${go_sdk})"
    (
      cd "${go_sdk}"
      GOCACHE="$(go env GOCACHE)" GOPATH="$(go env GOPATH)" GOMODCACHE="$(go env GOMODCACHE)" \
        HOME="${home}" XDG_CONFIG_HOME="${home}/.config" CODEX_HOME="${home}/codex-home" \
        CODEX_EXEC_PATH="${codex}" CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1 \
        go test . -run "^${strict_handshake_test}\$" -count=1 -v
    ) 2>&1 | tee "${home}/go-test.log" || die "strict initialize handshake failed"
    grep -qE "^--- PASS: ${strict_handshake_test} \\(" "${home}/go-test.log" ||
      die "strict handshake did not run: go test printed no '--- PASS: ${strict_handshake_test}' (renamed, or moved out of the root package of ${go_sdk}?)"
    runtime="codex --version and strict handshake (--- PASS: ${strict_handshake_test}) passed"
  fi

  log "packages"
  while IFS= read -r name; do
    printf '%s\t%s\n' "$(stat -c %s "${dist}/${name}")" "${name}"
  done <<<"${expected_files}"
  printf '%s\n' "${sums}"

  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      printf '### Verified codex %s packages\n\n' "${version}"
      printf '| Package | Bytes | SHA-256 |\n|---|---:|---|\n'
      local hash file
      while read -r hash file; do
        # shellcheck disable=SC2016 # backticks are Markdown code spans
        printf '| `%s` | %s | `%s` |\n' "${file}" "$(stat -c %s "${dist}/${file}")" "${hash}"
      done <<<"${sums}"
      printf '\nRuntime: %s.\n' "${runtime}"
    } >>"${GITHUB_STEP_SUMMARY}"
  fi
  echo VERIFY_OK
}

main "$@"
