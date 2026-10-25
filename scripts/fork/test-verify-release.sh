#!/usr/bin/env bash
# Self-test for scripts/fork/verify-release.sh on a locally built fake layout.
#
#   scripts/fork/test-verify-release.sh [--codex PATH]
#
# Builds release packages for all five targets from fake binaries that carry
# valid ELF, Mach-O and PE headers, checks that verify-release.sh accepts them,
# and checks that it rejects common packaging mistakes. With --codex, the Linux
# x86_64 package carries that real binary, so the runtime checks (`codex
# --version` and the Go SDK strict handshake) run as well; the expected version
# is then read from codex-rs/Cargo.toml.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
verify="${repo_root}/scripts/fork/verify-release.sh"

codex=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --codex) codex="${2:?--codex requires a path}"; shift 2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

work="$(mktemp -d "${TMPDIR:-/tmp}/test-verify-release.XXXXXX")"
trap 'rm -rf "${work}"' EXIT
umask 022

runtime_args=(--skip-runtime)
version="1.2.3"
if [[ -n "${codex}" ]]; then
  [[ -x "${codex}" ]] || { echo "not executable: ${codex}" >&2; exit 2; }
  version="$(awk '
    /^\[workspace\.package\]/ { section = 1; next }
    /^\[/ { section = 0 }
    section && $1 == "version" { gsub(/"/, "", $3); print $3; exit }
  ' "${repo_root}/codex-rs/Cargo.toml")"
  [[ -n "${version}" ]] || { echo "cannot read workspace version" >&2; exit 2; }
  runtime_args=()
fi

cat >"${work}/fake.py" <<'PY'
"""Writes minimal executables whose headers file(1) recognises."""
import struct
import sys
import zipfile


def elf(machine):
    ident = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8)
    header = struct.pack("<HHIQQQIHHHHHH", 2, machine, 1, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0)
    return ident + header


def macho(cputype, cpusubtype):
    return struct.pack("<IiiIIIII", 0xFEEDFACF, cputype, cpusubtype, 2, 0, 0, 0, 0)


def pe_x86_64():
    dos = bytearray(64)
    dos[0:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3C, 64)
    coff = b"PE\0\0" + struct.pack("<HHIIIHH", 0x8664, 0, 0, 0, 0, 240, 0x0022)
    optional = bytearray(240)
    struct.pack_into("<H", optional, 0, 0x20B)
    struct.pack_into("<H", optional, 68, 3)
    return bytes(dos) + coff + bytes(optional)


FORMATS = {
    "elf-x86_64": lambda: elf(0x3E),
    "elf-aarch64": lambda: elf(0xB7),
    "macho-x86_64": lambda: macho(0x01000007, 3),
    "macho-arm64": lambda: macho(0x0100000C, 0),
    "pe-x86_64": pe_x86_64,
}

if sys.argv[1] == "zip":
    # zip ARCHIVE ROOT NAME...: store ROOT/NAME entries, directories with a slash.
    archive, root, names = sys.argv[2], sys.argv[3], sys.argv[4:]
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as out:
        for name in names:
            if name.endswith("/"):
                info = zipfile.ZipInfo(name)
                info.external_attr = (0o40755 << 16) | 0x10
                out.writestr(info, b"")
            else:
                out.write(f"{root}/{name}", name)
else:
    fmt, path = sys.argv[1], sys.argv[2]
    with open(path, "wb") as out:
        out.write(FORMATS[fmt]() + bytes(4096))
PY

targets=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
  x86_64-pc-windows-msvc
)

bins_for() {
  case "$1" in
    *apple-darwin) echo "codex codex-app-server codex-code-mode-host codex-responses-api-proxy" ;;
    *linux-gnu) echo "bwrap codex codex-app-server codex-code-mode-host codex-responses-api-proxy" ;;
    *windows-msvc) echo "codex.exe codex-app-server.exe codex-code-mode-host.exe codex-command-runner.exe codex-responses-api-proxy.exe codex-windows-sandbox-setup.exe" ;;
  esac
}

format_for() {
  case "$1" in
    aarch64-apple-darwin) echo macho-arm64 ;;
    x86_64-apple-darwin) echo macho-x86_64 ;;
    aarch64-unknown-linux-gnu) echo elf-aarch64 ;;
    x86_64-unknown-linux-gnu) echo elf-x86_64 ;;
    x86_64-pc-windows-msvc) echo pe-x86_64 ;;
  esac
}

# make_package TARGET OUT_DIR [HOOK]: HOOK runs with the staged bin/ directory
# before archiving and can break the package.
make_package() {
  local target="$1" out="$2" hook="${3:-}"
  local stage="${work}/stage-${target}-${RANDOM}${RANDOM}"
  mkdir -p "${stage}/bin"
  local bin
  for bin in $(bins_for "${target}"); do
    if [[ "${target}" == x86_64-unknown-linux-gnu && "${bin}" == codex && -n "${codex}" ]]; then
      cp "${codex}" "${stage}/bin/codex"
    else
      python3 "${work}/fake.py" "$(format_for "${target}")" "${stage}/bin/${bin}"
    fi
    chmod 0755 "${stage}/bin/${bin}"
  done
  chmod 0755 "${stage}/bin"
  if [[ -n "${hook}" ]]; then
    "${hook}" "${stage}/bin"
  fi
  mkdir -p "${out}"
  if [[ "${target}" == *windows* ]]; then
    local names=(bin/)
    for bin in $(cd "${stage}" && find bin -mindepth 1 | LC_ALL=C sort); do
      names+=("${bin}")
    done
    rm -f "${out}/codex-package-${target}.zip"
    python3 "${work}/fake.py" zip "${out}/codex-package-${target}.zip" "${stage}" "${names[@]}"
  else
    rm -f "${out}/codex-package-${target}.tar.gz"
    tar -C "${stage}" -cf - bin | gzip -1 >"${out}/codex-package-${target}.tar.gz"
  fi
  rm -rf "${stage}"
}

passed=0
failures=0

# run_verify NAME DIST [verify args...]: stores output in ${work}/NAME.log.
run_verify() {
  local name="$1" dist="$2"
  shift 2
  local status=0
  "${verify}" --version "${version}" --dist "${dist}" "${runtime_args[@]}" "$@" \
    >"${work}/${name}.log" 2>&1 || status=$?
  return "${status}"
}

expect_ok() {
  local name="$1"
  shift
  if run_verify "${name}" "$@" && grep -q '^VERIFY_OK$' "${work}/${name}.log"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}: expected success" >&2
    sed 's/^/    /' "${work}/${name}.log" >&2
    failures=$((failures + 1))
  fi
}

expect_fail() {
  local name="$1" message="$2"
  shift 2
  if run_verify "${name}" "$@"; then
    echo "FAIL ${name}: expected failure containing '${message}'" >&2
    failures=$((failures + 1))
  elif grep -qF -- "${message}" "${work}/${name}.log"; then
    echo "ok   ${name}"
    passed=$((passed + 1))
  else
    echo "FAIL ${name}: missing '${message}' in output" >&2
    sed 's/^/    /' "${work}/${name}.log" >&2
    failures=$((failures + 1))
  fi
}

# copy_dist NAME: hard-links the good dist so cases only replace what they break.
copy_dist() {
  local dest="${work}/dist-$1"
  cp -al "${good}" "${dest}"
  rm -f "${dest}/SHA256SUMS"
  printf '%s\n' "${dest}"
}

good="${work}/dist-good"
for target in "${targets[@]}"; do
  make_package "${target}" "${good}"
done

expect_ok happy-path "${good}"
if [[ -n "${codex}" ]]; then
  if grep -q -- '--- PASS: TestRealAppServerInitializeStrictDigest' "${work}/happy-path.log"; then
    grep -E '^==> codex --version|^--- PASS' "${work}/happy-path.log"
    echo "ok   strict-handshake-ran"
    passed=$((passed + 1))
  else
    echo "FAIL strict-handshake-ran" >&2
    failures=$((failures + 1))
  fi
fi
if (cd "${good}" && sha256sum --quiet -c SHA256SUMS); then
  echo "ok   sha256sums-valid"
  passed=$((passed + 1))
else
  echo "FAIL sha256sums-valid" >&2
  failures=$((failures + 1))
fi
expect_ok existing-sums-match "${good}"

subset="${work}/dist-subset"
mkdir -p "${subset}"
ln "${good}/codex-package-aarch64-apple-darwin.tar.gz" "${subset}/"
ln "${good}/codex-package-x86_64-pc-windows-msvc.zip" "${subset}/"
expect_ok target-subset "${subset}" --targets "aarch64-apple-darwin x86_64-pc-windows-msvc"

hook_group_writable() { chmod 0775 "$1/codex"; }
hook_extra_binary() { cp "$1/codex" "$1/extra"; }
hook_missing_binary() { rm "$1/codex-responses-api-proxy"; }
hook_symlink() { rm "$1/bwrap" && ln -s codex "$1/bwrap"; }
hook_wrong_arch() { python3 "${work}/fake.py" macho-arm64 "$1/codex"; }
hook_unsafe_name() { cp "$1/codex.exe" "$1/-rf.exe"; }

dist="$(copy_dist mode)"
make_package aarch64-apple-darwin "${dist}" hook_group_writable
expect_fail mode-0775 "unexpected tar entry type or mode" "${dist}"

dist="$(copy_dist extra)"
make_package x86_64-apple-darwin "${dist}" hook_extra_binary
expect_fail extra-binary "layout mismatch" "${dist}"

dist="$(copy_dist missing)"
make_package aarch64-unknown-linux-gnu "${dist}" hook_missing_binary
expect_fail missing-binary "layout mismatch" "${dist}"

dist="$(copy_dist symlink)"
make_package aarch64-unknown-linux-gnu "${dist}" hook_symlink
expect_fail symlink "unexpected tar entry type or mode" "${dist}"

dist="$(copy_dist arch)"
make_package x86_64-apple-darwin "${dist}" hook_wrong_arch
expect_fail wrong-arch "has unexpected type" "${dist}"

dist="$(copy_dist unsafe)"
make_package x86_64-pc-windows-msvc "${dist}" hook_unsafe_name
expect_fail unsafe-zip-entry "unexpected archive entry" "${dist}"

dist="$(copy_dist missing-package)"
rm "${dist}/codex-package-x86_64-pc-windows-msvc.zip"
expect_fail missing-package "package set mismatch" "${dist}"

dist="$(copy_dist extra-file)"
echo notes >"${dist}/notes.txt"
expect_fail extra-file "package set mismatch" "${dist}"

dist="$(copy_dist corrupt)"
rm "${dist}/codex-package-x86_64-apple-darwin.tar.gz"
head -c 100 "${good}/codex-package-x86_64-apple-darwin.tar.gz" >"${dist}/codex-package-x86_64-apple-darwin.tar.gz"
expect_fail corrupt-archive "gzip integrity test failed" "${dist}"

dist="$(copy_dist bad-sums)"
sed 's/^./0/' "${good}/SHA256SUMS" >"${dist}/SHA256SUMS"
expect_fail bad-sums "does not match the packages" "${dist}"

if [[ -n "${codex}" ]]; then
  dist="$(copy_dist wrong-version)"
  # Keep the wrong version valid so only the runtime check can reject it.
  version_saved="${version}"
  version="0.0.1"
  expect_fail wrong-version "want 'codex-cli 0.0.1'" "${dist}"
  version="${version_saved}"
fi

echo "verify-release self-test: ${passed} passed, ${failures} failed (runtime checks: $([[ -n "${codex}" ]] && echo on || echo off))"
[[ ${failures} -eq 0 ]]
