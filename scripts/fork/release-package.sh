#!/usr/bin/env bash
# Release packages of .github/workflows/fork-rust-release.yml, built from parts.
#
#   scripts/fork/release-package.sh bins --target TARGET --bundle BUNDLE
#   scripts/fork/release-package.sh part --version X.Y.Z --target TARGET
#                                        --bundle BUNDLE --release-dir DIR --out DIR
#   scripts/fork/release-package.sh assemble --version X.Y.Z --parts DIR --dist DIR
#                                            [--targets "TARGET ..."]
#
# Every target is built by two bundle jobs (R1.1): primary (codex,
# codex-responses-api-proxy; also bwrap on Linux, codex-command-runner and
# codex-windows-sandbox-setup on Windows) and app-server (codex-app-server,
# codex-code-mode-host).
#
# bins      prints the bundle's binaries (cargo names, without .exe) on one line.
# part      runs on the build runner after cargo build: copies the bundle's
#           binaries from DIR (target/<target>/release), strips them as the
#           packages always were (macOS strip -S -x; Linux strip --strip-debug
#           --strip-unneeded except bwrap, which the build already stripped to
#           export its digest; nothing on Windows), requires `codex --version` to
#           print `codex-cli X.Y.Z` in the primary bundle, and writes
#           OUT/codex-parts-<version>-<target>-<bundle>.tar: a ustar archive of
#           bin/ holding exactly the bundle's binaries as 0755 regular files
#           (R1.4, R1.5).
# assemble  runs in the verify job: merges the two parts of each target into
#           codex-package-<target>.tar.gz (.zip on Windows) in the empty dist
#           directory, with bin/ as drwxr-xr-x and -rwxr-xr-x binaries (R2.2).
#           It fails when a part is missing, a part holds anything but the
#           bundle's binaries, both parts hold the same file, the merged bin/
#           differs from the target's binaries, or the SHA-256 of a Linux
#           target's bwrap is not embedded in its codex and codex-app-server
#           (R2.1, R2.3). DIR must hold exactly the parts of the targets.
#           scripts/fork/verify-release.sh checks the packages next.
#
# part runs on Linux, macOS (GNU tar as gtar when installed, else bsdtar) and
# Windows (Git Bash) runners and keeps to bash 3.2 and portable tool options.
# assemble requires GNU tar, sha256sum and python3, as on the Linux verify
# runner.

set -euo pipefail

version_re='^[0-9]+\.[0-9]+\.[0-9]+(-(alpha(\.[0-9]+){0,2}|beta(\.[0-9]+)?))?$'
all_targets="aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu x86_64-pc-windows-msvc"
all_bundles="primary app-server"

# Staging directory of the current part or package, removed on exit.
stage=""
cleanup() {
  if [[ -n "${stage}" ]]; then
    chmod -R u+rwx -- "${stage}" 2>/dev/null || true
    rm -rf -- "${stage}"
  fi
}
trap cleanup EXIT

log() { printf '==> %s\n' "$*" >&2; }
die() {
  printf 'release-package: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,/^$/p' "${BASH_SOURCE[0]}" | sed -E 's/^# ?//'
}

check_target() {
  case " ${all_targets} " in
    *" $1 "*) ;;
    *) die "unknown target '$1' (want one of: ${all_targets})" ;;
  esac
}

check_bundle() {
  case " ${all_bundles} " in
    *" $1 "*) ;;
    *) die "unknown bundle '$1' (want one of: ${all_bundles})" ;;
  esac
}

check_version() {
  [[ "$1" =~ ${version_re} ]] || die "invalid release version '$1'"
}

# bundle_bins TARGET BUNDLE: the bundle's cargo binaries, space-separated.
bundle_bins() {
  case "$2" in
    primary)
      case "$1" in
        *-linux-*) echo "codex codex-responses-api-proxy bwrap" ;;
        *-windows-*) echo "codex codex-responses-api-proxy codex-command-runner codex-windows-sandbox-setup" ;;
        *) echo "codex codex-responses-api-proxy" ;;
      esac
      ;;
    app-server) echo "codex-app-server codex-code-mode-host" ;;
  esac
}

exe_suffix() {
  case "$1" in
    *-windows-*) echo ".exe" ;;
    *) echo "" ;;
  esac
}

# bundle_files TARGET BUNDLE: the bundle's file names in bin/, sorted.
bundle_files() {
  local bin suffix
  suffix="$(exe_suffix "$1")"
  for bin in $(bundle_bins "$1" "$2"); do
    printf '%s%s\n' "${bin}" "${suffix}"
  done | LC_ALL=C sort
}

part_name() {
  printf 'codex-parts-%s-%s-%s.tar\n' "$1" "$2" "$3"
}

package_name() {
  case "$1" in
    *-windows-*) printf 'codex-package-%s.zip\n' "$1" ;;
    *) printf 'codex-package-%s.tar.gz\n' "$1" ;;
  esac
}

# The tar every command uses: GNU tar where there is one (gtar on the macOS
# images), otherwise the system tar (bsdtar on macOS).
tar_cmd=tar

# is_gnu_tar [TAR]: a plain capture, not a pipe into grep -q, whose early exit
# could fail the pipeline and read as "not GNU tar".
is_gnu_tar() {
  local banner
  banner="$("${1:-${tar_cmd}}" --version 2>/dev/null || true)"
  [[ "${banner}" == *"GNU tar"* ]]
}

pick_tar() {
  if ! is_gnu_tar tar && command -v gtar >/dev/null 2>&1 && is_gnu_tar gtar; then
    tar_cmd=gtar
  fi
}

# Absolute form of an existing directory. Git Bash prints /d/a/... for
# D:\a\..., which GNU tar does not mistake for a remote host:file archive.
abs_dir() {
  (cd -- "$1" && pwd)
}

# part_files LABEL ARCHIVE: requires every entry of the tar ARCHIVE to be the
# bin/ directory (drwxr-xr-x) or a 0755 regular file bin/NAME with a safe NAME,
# and prints the NAMEs, sorted. ARCHIVE is a path without a drive colon.
part_files() {
  local label="$1" archive="$2" listing bad
  listing="$("${tar_cmd}" -tvf "${archive}")" || die "${label}: cannot list ${archive}"
  # GNU tar and bsdtar both print the mode first and the name last; a symlink
  # or hard link line ends in its target and fails the mode check.
  bad="$(awk '
    $NF == "bin/" { if ($1 != "drwxr-xr-x") print; next }
    $1 != "-rwxr-xr-x" || $NF !~ /^bin\/[A-Za-z0-9][A-Za-z0-9._-]*$/ { print }
  ' <<<"${listing}")"
  [[ -z "${bad}" ]] || die "${label}: unexpected entries (want bin/ and 0755 regular files bin/NAME):
${bad}"
  awk '$NF != "bin/" { sub(/^bin\//, "", $NF); print $NF }' <<<"${listing}" | LC_ALL=C sort
}

cmd_bins() {
  local target="" bundle=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --target) target="${2:?--target requires a value}"; shift 2 ;;
      --bundle) bundle="${2:?--bundle requires a value}"; shift 2 ;;
      *) die "unknown bins option: $1" ;;
    esac
  done
  check_target "${target}"
  check_bundle "${bundle}"
  bundle_bins "${target}" "${bundle}"
}

cmd_part() {
  local version="" target="" bundle="" release_dir="" out=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --target) target="${2:?--target requires a value}"; shift 2 ;;
      --bundle) bundle="${2:?--bundle requires a value}"; shift 2 ;;
      --release-dir) release_dir="${2:?--release-dir requires a directory}"; shift 2 ;;
      --out) out="${2:?--out requires a directory}"; shift 2 ;;
      *) die "unknown part option: $1" ;;
    esac
  done
  check_version "${version}"
  check_target "${target}"
  check_bundle "${bundle}"
  [[ -d "${release_dir}" ]] || die "missing release directory '${release_dir}'"
  [[ -n "${out}" ]] || die "--out is required"
  release_dir="$(abs_dir "${release_dir}")"
  mkdir -p -- "${out}"
  out="$(abs_dir "${out}")"

  local label="${target} ${bundle}" suffix name
  suffix="$(exe_suffix "${target}")"
  name="$(part_name "${version}" "${target}" "${bundle}")"
  [[ ! -e "${out}/${name}" ]] || die "${label}: ${out}/${name} already exists"

  stage="$(mktemp -d "${TMPDIR:-/tmp}/release-part.XXXXXX")"
  mkdir "${stage}/bin"
  local bin src dest
  for bin in $(bundle_bins "${target}" "${bundle}"); do
    src="${release_dir}/${bin}${suffix}"
    [[ -f "${src}" && ! -L "${src}" ]] || die "${label}: missing binary ${src}"
    dest="${stage}/bin/${bin}${suffix}"
    cp -- "${src}" "${dest}"
    chmod 0755 "${dest}"
    case "${target}" in
      *-apple-darwin) strip -S -x "${dest}" ;;
      *-linux-*)
        # bwrap was stripped before its digest went into CODEX_BWRAP_SHA256.
        if [[ "${bin}" != bwrap ]]; then
          strip --strip-debug --strip-unneeded "${dest}"
        fi
        ;;
    esac
  done
  chmod 0755 "${stage}/bin"

  if [[ "${bundle}" == primary ]]; then
    local printed
    printed="$("${stage}/bin/codex${suffix}" --version)" || die "${label}: codex --version failed"
    [[ "${printed}" == "codex-cli ${version}" ]] ||
      die "${label}: codex --version printed '${printed}', want 'codex-cli ${version}'"
    log "codex --version: ${printed}"
  fi

  # ustar keeps macOS extended attributes out, and COPYFILE_DISABLE keeps
  # bsdtar from adding AppleDouble ._ entries. GNU tar (Linux, Git Bash, gtar)
  # also records 0755 and root ownership regardless of how the file system
  # reports them. The archive is written inside the stage with a relative name.
  pick_tar
  local tar_create="${tar_cmd} --format=ustar"
  if is_gnu_tar; then
    tar_create="${tar_create} --owner=0 --group=0 --numeric-owner --mode=0755 --sort=name"
  fi
  # shellcheck disable=SC2086 # tar_create holds options without spaces
  (cd "${stage}" && COPYFILE_DISABLE=1 ${tar_create} -cf part.tar bin) ||
    die "${label}: cannot write the part archive"

  local got want
  got="$(cd "${stage}" && part_files "${label}" part.tar)"
  want="$(bundle_files "${target}" "${bundle}")"
  [[ "${got}" == "${want}" ]] || die "${label}: the part holds
${got}
want
${want}"
  mv -- "${stage}/part.tar" "${out}/${name}"
  log "wrote ${out}/${name}"
}

# assemble_target VERSION TARGET PARTS DIST: one package from two parts.
assemble_target() {
  local version="$1" target="$2" parts="$3" dist="$4"
  local primary app_server overlap want got
  primary="$(part_files "${target}" "${parts}/$(part_name "${version}" "${target}" primary)")"
  app_server="$(part_files "${target}" "${parts}/$(part_name "${version}" "${target}" app-server)")"

  overlap="$(LC_ALL=C comm -12 <(printf '%s\n' "${primary}") <(printf '%s\n' "${app_server}"))"
  [[ -z "${overlap}" ]] || die "${target}: both parts contain ${overlap//$'\n'/, }"
  want="$(bundle_files "${target}" primary)"
  [[ "${primary}" == "${want}" ]] || die "${target}: layout mismatch in the primary part
expected:
${want}
actual:
${primary}"
  want="$(bundle_files "${target}" app-server)"
  [[ "${app_server}" == "${want}" ]] || die "${target}: layout mismatch in the app-server part
expected:
${want}
actual:
${app_server}"

  stage="$(mktemp -d "${TMPDIR:-/tmp}/release-package.XXXXXX")"
  local bundle
  for bundle in ${all_bundles}; do
    "${tar_cmd}" -xf "${parts}/$(part_name "${version}" "${target}" "${bundle}")" -C "${stage}" \
      --no-same-owner --no-same-permissions || die "${target}: cannot extract the ${bundle} part"
  done
  chmod 0755 "${stage}/bin"
  find "${stage}/bin" -mindepth 1 -type f -exec chmod 0755 {} +

  want="$( (bundle_files "${target}" primary && bundle_files "${target}" app-server) | LC_ALL=C sort)"
  got="$(cd "${stage}" && find . -mindepth 1 -printf '%y %P\n' | LC_ALL=C sort)"
  [[ "${got}" == "$(printf 'd bin\n' && awk '{ print "f bin/" $0 }' <<<"${want}")" ]] ||
    die "${target}: layout mismatch in the merged bin/
expected:
${want}
actual:
${got}"

  if [[ "${target}" == *-linux-* ]]; then
    # The build embeds CODEX_BWRAP_SHA256 as a string (option_env!) in both
    # binaries; each bundle job built its own bwrap.
    local digest bin
    digest="$(sha256sum "${stage}/bin/bwrap" | cut -d' ' -f1)"
    for bin in codex codex-app-server; do
      LC_ALL=C grep -qaF -- "${digest}" "${stage}/bin/${bin}" ||
        die "${target}: ${bin} does not embed the SHA-256 of the packaged bwrap (${digest}); its bundle built a different bwrap"
    done
    log "${target}: codex and codex-app-server embed the bwrap SHA-256 ${digest}"
  fi

  local archive
  archive="${dist}/$(package_name "${target}")"
  if [[ "${target}" == *-windows-* ]]; then
    python3 -I - "${archive}" "${stage}" <<'PY' || die "${target}: cannot write ${archive}"
"""Zips ROOT/bin as bin/ with Unix 0755 modes, entries sorted."""
import os
import sys
import zipfile

archive, root = sys.argv[1], sys.argv[2]
bin_dir = os.path.join(root, "bin")
with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED, strict_timestamps=False) as out:
    out.write(bin_dir, "bin/")
    for name in sorted(os.listdir(bin_dir)):
        out.write(os.path.join(bin_dir, name), "bin/" + name)
PY
  else
    "${tar_cmd}" -C "${stage}" --sort=name --owner=0 --group=0 --numeric-owner -czf "${archive}" bin ||
      die "${target}: cannot write ${archive}"
  fi
  rm -rf -- "${stage}"
  stage=""
  log "assembled ${archive}"
}

cmd_assemble() {
  local version="" parts="" dist="" targets="${all_targets}"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version) version="${2:?--version requires a value}"; shift 2 ;;
      --parts) parts="${2:?--parts requires a directory}"; shift 2 ;;
      --dist) dist="${2:?--dist requires a directory}"; shift 2 ;;
      --targets) targets="${2:?--targets requires a list}"; shift 2 ;;
      *) die "unknown assemble option: $1" ;;
    esac
  done
  check_version "${version}"
  [[ -n "${targets// /}" ]] || die "no targets given"
  local target
  for target in ${targets}; do
    check_target "${target}"
  done
  pick_tar
  is_gnu_tar || die "assemble requires GNU tar"
  command -v python3 >/dev/null 2>&1 || die "assemble requires python3"
  [[ -d "${parts}" ]] || die "missing parts directory '${parts}'"
  parts="$(abs_dir "${parts}")"
  [[ -n "${dist}" ]] || die "--dist is required"
  mkdir -p -- "${dist}"
  dist="$(abs_dir "${dist}")"
  [[ -z "$(find "${dist}" -mindepth 1 -maxdepth 1 -print -quit)" ]] || die "dist directory ${dist} is not empty"

  # Exactly the two parts of every target (R2.1).
  local bundle name expected=()
  for target in ${targets}; do
    for bundle in ${all_bundles}; do
      name="$(part_name "${version}" "${target}" "${bundle}")"
      [[ -f "${parts}/${name}" && ! -L "${parts}/${name}" ]] || die "${target}: missing ${bundle} part ${name}"
      expected+=("${name}")
    done
  done
  local extra
  extra="$(LC_ALL=C comm -23 \
    <(find "${parts}" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort) \
    <(printf '%s\n' "${expected[@]}" | LC_ALL=C sort))"
  [[ -z "${extra}" ]] || die "unexpected files in ${parts}: ${extra//$'\n'/, }"

  for target in ${targets}; do
    log "assemble ${target}"
    assemble_target "${version}" "${target}" "${parts}" "${dist}"
  done
}

main() {
  [[ $# -ge 1 ]] || {
    usage >&2
    exit 2
  }
  local command="$1"
  shift
  case "${command}" in
    bins) cmd_bins "$@" ;;
    part) cmd_part "$@" ;;
    assemble) cmd_assemble "$@" ;;
    -h | --help | help) usage ;;
    *)
      usage >&2
      die "unknown command: ${command}"
      ;;
  esac
}

main "$@"
