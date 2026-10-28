#!/usr/bin/env bash
# Self-test for scripts/fork/release-package.sh on fake binaries.
#
#   scripts/fork/test-release-package.sh
#
# part: packs fake release directories (a shell-script codex answers
# --version; a stub strip records its calls) and checks the archive, the
# stripping per platform and the refusals. assemble: merges hand-made parts of
# all five targets, whose fake binaries carry valid ELF, Mach-O and PE headers,
# into packages that verify-release.sh --skip-runtime accepts, and checks the
# refusals for packaging mistakes (R1.4, R1.5, R2.1-R2.3).

# Written for bash 3.2 too, so an old bash reaches this message.
if ((BASH_VERSINFO[0] < 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] < 4))); then
  printf '%s: requires bash >= 4.4 (running %s)\n' "${0##*/}" "${BASH_VERSION}" >&2
  exit 1
fi

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
package="${repo_root}/scripts/fork/release-package.sh"
verify="${repo_root}/scripts/fork/verify-release.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/test-release-package.XXXXXX")"
trap 'chmod -R u+rwx "${work}" 2>/dev/null || true; rm -rf "${work}"' EXIT
umask 022

if [[ "$(tar --version 2>/dev/null || true)" != *"GNU tar"* ]]; then
  echo "${0##*/}: requires GNU tar, as on Linux" >&2
  exit 1
fi

version="1.2.3"
targets=(
  aarch64-apple-darwin
  x86_64-apple-darwin
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-gnu
  x86_64-pc-windows-msvc
)

# The bundles of the spec (R1.1), independent of release-package.sh.
bundle_files() {
  case "$2:$1" in
    primary:*linux*) echo "bwrap codex codex-responses-api-proxy" ;;
    primary:*windows*) echo "codex-command-runner.exe codex-responses-api-proxy.exe codex-windows-sandbox-setup.exe codex.exe" ;;
    primary:*) echo "codex codex-responses-api-proxy" ;;
    app-server:*windows*) echo "codex-app-server.exe codex-code-mode-host.exe" ;;
    app-server:*) echo "codex-app-server codex-code-mode-host" ;;
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

cat >"${work}/fake.py" <<'PY'
"""fake.py FORMAT PATH [TEXT]: a minimal executable file(1) recognises, ending in TEXT."""
import struct
import sys


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

fmt, path = sys.argv[1], sys.argv[2]
text = sys.argv[3].encode() if len(sys.argv) > 3 else b""
with open(path, "wb") as out:
    out.write(FORMATS[fmt]() + bytes(4096) + text)
PY

# fake TARGET PATH [TEXT]: a fake binary of the target's format.
fake() { python3 -I "${work}/fake.py" "$(format_for "$1")" "$2" "$(basename "$2") ${3-}"; }

passed=0
failures=0
ok() {
  echo "ok   $1"
  passed=$((passed + 1))
}
fail() {
  echo "FAIL $1" >&2
  failures=$((failures + 1))
}

# check NAME ok|fail PATTERN COMMAND...: runs COMMAND with the test TMPDIR and
# expects the status and an output line matching the extended regular
# expression PATTERN.
pkg_tmp="${work}/tmp"
mkdir -p "${pkg_tmp}"
check() {
  local name="$1" want="$2" pattern="$3"
  shift 3
  local status=0 got=ok
  TMPDIR="${pkg_tmp}" "$@" >"${work}/${name}.log" 2>&1 || status=$?
  [[ ${status} -eq 0 ]] || got=fail
  if [[ "${got}" == "${want}" ]] && grep -qE -- "${pattern}" "${work}/${name}.log"; then
    ok "${name}"
  else
    fail "${name}: status ${status}, want ${want} with /${pattern}/"
    sed 's/^/    /' "${work}/${name}.log" >&2
  fi
}

# expect NAME COMMAND...: records whether COMMAND succeeds.
expect() {
  local name="$1"
  shift
  if "$@"; then ok "${name}"; else fail "${name}"; fi
}

# --- bins --------------------------------------------------------------------

for target in "${targets[@]}"; do
  for bundle in primary app-server; do
    suffix=""
    [[ "${target}" != *windows* ]] || suffix=".exe"
    got="$("${package}" bins --target "${target}" --bundle "${bundle}" | tr ' ' '\n' | sed "s/\$/${suffix}/" |
      LC_ALL=C sort | xargs)"
    expect "bins-${target}-${bundle}" test "${got}" = "$(bundle_files "${target}" "${bundle}")"
  done
done
check bins-unknown-bundle fail "unknown bundle 'cli'" "${package}" bins --target x86_64-unknown-linux-gnu --bundle cli
check bins-unknown-target fail "unknown target 'riscv64gc-unknown-linux-gnu'" \
  "${package}" bins --target riscv64gc-unknown-linux-gnu --bundle primary

# --- part --------------------------------------------------------------------

# A stub strip that records its arguments and leaves the file as it is.
mkdir -p "${work}/stub"
cat >"${work}/stub/strip" <<STUB
#!/usr/bin/env bash
echo "strip \$*" >>"${work}/strip.calls"
STUB
chmod 0755 "${work}/stub/strip"
part_path="${work}/stub:${PATH}"

# release_dir NAME TARGET [CODEX_VERSION]: a fake target/<target>/release with
# every binary of both bundles, cargo by-products, and a codex that prints
# `codex-cli CODEX_VERSION` (default ${version}). Files are 0644 like a careless
# copy; part must still pack 0755.
release_dir() {
  local dir="${work}/release-$1" target="$2" codex_version="${3:-${version}}" file
  local codex="${dir}/codex"
  [[ "${target}" != *windows* ]] || codex="${dir}/codex.exe"
  mkdir -p "${dir}"
  for file in $(bundle_files "${target}" primary) $(bundle_files "${target}" app-server); do
    if [[ "${dir}/${file}" == "${codex}" ]]; then
      printf '#!/bin/sh\n# fake codex\necho "codex-cli %s"\n' "${codex_version}" >"${codex}"
    else
      fake "${target}" "${dir}/${file}"
      chmod 0644 "${dir}/${file}"
    fi
  done
  chmod 0755 "${codex}"
  mkdir -p "${dir}/deps"
  echo by-product >"${dir}/codex.d"
  echo by-product >"${dir}/deps/codex-0123"
  printf '%s\n' "${dir}"
}

# part_listing ARCHIVE: "mode owner name" per entry.
part_listing() { tar -tvf "$1" --numeric-owner | awk '{ print $1, $2, $NF }'; }

: >"${work}/strip.calls"
rel="$(release_dir linux x86_64-unknown-linux-gnu)"
PATH="${part_path}" check part-linux-primary ok '^==> codex --version: codex-cli 1\.2\.3$' \
  "${package}" part --version "${version}" --target x86_64-unknown-linux-gnu --bundle primary \
  --release-dir "${rel}" --out "${work}/parts-linux"
linux_part="${work}/parts-linux/codex-parts-${version}-x86_64-unknown-linux-gnu-primary.tar"
expect part-linux-primary-listing test "$(part_listing "${linux_part}" | xargs)" = \
  "drwxr-xr-x 0/0 bin/ -rwxr-xr-x 0/0 bin/bwrap -rwxr-xr-x 0/0 bin/codex -rwxr-xr-x 0/0 bin/codex-responses-api-proxy"
expect part-linux-strips-all-but-bwrap test "$(sed "s#${work}/[^ ]*/bin/##" "${work}/strip.calls" | LC_ALL=C sort | xargs)" = \
  "strip --strip-debug --strip-unneeded codex strip --strip-debug --strip-unneeded codex-responses-api-proxy"
mkdir -p "${work}/unpacked-linux"
tar -xf "${linux_part}" -C "${work}/unpacked-linux"
expect part-linux-content-unchanged cmp -s "${rel}/bwrap" "${work}/unpacked-linux/bin/bwrap"

: >"${work}/strip.calls"
rel="$(release_dir mac aarch64-apple-darwin)"
PATH="${part_path}" check part-macos-app-server ok "wrote .*codex-parts-${version}-aarch64-apple-darwin-app-server\.tar" \
  "${package}" part --version "${version}" --target aarch64-apple-darwin --bundle app-server \
  --release-dir "${rel}" --out "${work}/parts-mac"
expect part-macos-strip-s-x test "$(sed "s#${work}/[^ ]*/bin/##" "${work}/strip.calls" | LC_ALL=C sort | xargs)" = \
  "strip -S -x codex-app-server strip -S -x codex-code-mode-host"
expect part-macos-app-server-listing test \
  "$(part_listing "${work}/parts-mac/codex-parts-${version}-aarch64-apple-darwin-app-server.tar" | xargs)" = \
  "drwxr-xr-x 0/0 bin/ -rwxr-xr-x 0/0 bin/codex-app-server -rwxr-xr-x 0/0 bin/codex-code-mode-host"

: >"${work}/strip.calls"
rel="$(release_dir windows x86_64-pc-windows-msvc)"
PATH="${part_path}" check part-windows-primary ok '^==> codex --version: codex-cli 1\.2\.3$' \
  "${package}" part --version "${version}" --target x86_64-pc-windows-msvc --bundle primary \
  --release-dir "${rel}" --out "${work}/parts-windows"
expect part-windows-no-strip test ! -s "${work}/strip.calls"
expect part-windows-primary-names test \
  "$(tar -tf "${work}/parts-windows/codex-parts-${version}-x86_64-pc-windows-msvc-primary.tar" | xargs)" = \
  "bin/ bin/codex-command-runner.exe bin/codex-responses-api-proxy.exe bin/codex-windows-sandbox-setup.exe bin/codex.exe"

rel="$(release_dir old-version x86_64-unknown-linux-gnu 1.2.2)"
PATH="${part_path}" check part-version-mismatch fail "codex --version printed 'codex-cli 1\.2\.2', want 'codex-cli 1\.2\.3'" \
  "${package}" part --version "${version}" --target x86_64-unknown-linux-gnu --bundle primary \
  --release-dir "${rel}" --out "${work}/parts-old-version"
expect part-version-mismatch-no-archive test ! -e "${work}/parts-old-version/codex-parts-${version}-x86_64-unknown-linux-gnu-primary.tar"

rel="$(release_dir missing x86_64-apple-darwin)"
rm "${rel}/codex-code-mode-host"
PATH="${part_path}" check part-missing-binary fail 'x86_64-apple-darwin app-server: missing binary .*/codex-code-mode-host$' \
  "${package}" part --version "${version}" --target x86_64-apple-darwin --bundle app-server \
  --release-dir "${rel}" --out "${work}/parts-missing"

# Without GNU tar (bsdtar on a macOS runner without gtar) part relies on the
# 0755 modes it set: this tar rejects the GNU-only options and does not call
# itself GNU tar.
mkdir -p "${work}/plain-tar"
cat >"${work}/plain-tar/tar" <<STUB
#!/usr/bin/env bash
if [[ "\${1-}" == --version ]]; then echo "bsdtar 3.7.4 - libarchive 3.7.4"; exit 0; fi
for arg in "\$@"; do
  case "\${arg}" in
    --owner=* | --group=* | --numeric-owner | --mode=* | --sort=*) echo "tar: Option \${arg} is not supported" >&2; exit 1 ;;
  esac
done
exec "$(command -v tar)" "\$@"
STUB
chmod 0755 "${work}/plain-tar/tar"
rel="$(release_dir plain-tar x86_64-apple-darwin)"
PATH="${work}/plain-tar:${part_path}" check part-without-gnu-tar ok '^==> codex --version: codex-cli 1\.2\.3$' \
  "${package}" part --version "${version}" --target x86_64-apple-darwin --bundle primary \
  --release-dir "${rel}" --out "${work}/parts-plain-tar"
expect part-without-gnu-tar-listing test \
  "$(tar -tvf "${work}/parts-plain-tar/codex-parts-${version}-x86_64-apple-darwin-primary.tar" |
    awk '{ print $NF, $1 }' | LC_ALL=C sort | xargs)" = \
  "bin/ drwxr-xr-x bin/codex -rwxr-xr-x bin/codex-responses-api-proxy -rwxr-xr-x"

PATH="${part_path}" check part-existing-archive fail 'already exists' \
  "${package}" part --version "${version}" --target x86_64-unknown-linux-gnu --bundle primary \
  --release-dir "${work}/release-linux" --out "${work}/parts-linux"

# --- assemble ----------------------------------------------------------------

# make_part TARGET BUNDLE OUT [HOOK]: a part with fake binaries of the target's
# format; Linux codex and codex-app-server embed the digest of the Linux bwrap
# of that target. HOOK runs with the staged bin/ directory before archiving.
make_part() {
  local target="$1" bundle="$2" out="$3" hook="${4:-}"
  local stage="${work}/stage-${target}-${bundle}-${RANDOM}${RANDOM}" file digest=""
  mkdir -p "${stage}/bin" "${out}"
  if [[ "${target}" == *linux* ]]; then
    fake "${target}" "${work}/bwrap-${target}"
    digest="$(sha256sum "${work}/bwrap-${target}" | cut -d' ' -f1)"
  fi
  for file in $(bundle_files "${target}" "${bundle}"); do
    case "${file}" in
      bwrap) cp "${work}/bwrap-${target}" "${stage}/bin/bwrap" ;;
      codex | codex-app-server) fake "${target}" "${stage}/bin/${file}" "${digest}" ;;
      *) fake "${target}" "${stage}/bin/${file}" ;;
    esac
    chmod 0755 "${stage}/bin/${file}"
  done
  chmod 0755 "${stage}/bin"
  if [[ -n "${hook}" ]]; then
    "${hook}" "${stage}/bin" "${target}"
  fi
  tar --format=ustar --owner=0 --group=0 --numeric-owner -C "${stage}" \
    -cf "${out}/codex-parts-${version}-${target}-${bundle}.tar" bin
  rm -rf "${stage}"
}

good_parts="${work}/parts-good"
for target in "${targets[@]}"; do
  make_part "${target}" primary "${good_parts}"
  make_part "${target}" app-server "${good_parts}"
done

# variant_parts NAME: hard-linked copy of the good parts.
variant_parts() {
  cp -al "${good_parts}" "${work}/parts-$1"
  printf '%s\n' "${work}/parts-$1"
}

assemble=("${package}" assemble --version "${version}")

check assemble-all-targets ok 'assembled .*codex-package-x86_64-pc-windows-msvc\.zip$' \
  "${assemble[@]}" --parts "${good_parts}" --dist "${work}/dist-good"
expect assemble-bwrap-digest-checked grep -qE \
  '^==> x86_64-unknown-linux-gnu: codex and codex-app-server embed the bwrap SHA-256 [0-9a-f]{64}$' \
  "${work}/assemble-all-targets.log"
check assembled-packages-verify ok '^VERIFY_OK$' \
  "${verify}" --version "${version}" --dist "${work}/dist-good" --skip-runtime
expect assembled-tar-layout test \
  "$(tar -tvzf "${work}/dist-good/codex-package-x86_64-unknown-linux-gnu.tar.gz" --numeric-owner | awk '{ print $1, $2, $NF }' | xargs)" = \
  "drwxr-xr-x 0/0 bin/ -rwxr-xr-x 0/0 bin/bwrap -rwxr-xr-x 0/0 bin/codex -rwxr-xr-x 0/0 bin/codex-app-server -rwxr-xr-x 0/0 bin/codex-code-mode-host -rwxr-xr-x 0/0 bin/codex-responses-api-proxy"
expect assembled-zip-layout test \
  "$(zipinfo "${work}/dist-good/codex-package-x86_64-pc-windows-msvc.zip" | awk '/^[d-]/ { print $1, $3, $NF }' | xargs)" = \
  "drwxr-xr-x unx bin/ -rwxr-xr-x unx bin/codex-app-server.exe -rwxr-xr-x unx bin/codex-code-mode-host.exe -rwxr-xr-x unx bin/codex-command-runner.exe -rwxr-xr-x unx bin/codex-responses-api-proxy.exe -rwxr-xr-x unx bin/codex-windows-sandbox-setup.exe -rwxr-xr-x unx bin/codex.exe"

# Members are the part files, byte for byte.
mkdir -p "${work}/members/part" "${work}/members/package"
tar -xf "${good_parts}/codex-parts-${version}-aarch64-apple-darwin-primary.tar" -C "${work}/members/part"
tar -xf "${good_parts}/codex-parts-${version}-aarch64-apple-darwin-app-server.tar" -C "${work}/members/part"
tar -xzf "${work}/dist-good/codex-package-aarch64-apple-darwin.tar.gz" -C "${work}/members/package"
expect assembled-members-identical diff -r "${work}/members/part" "${work}/members/package"
mkdir -p "${work}/members/zip-part" "${work}/members/zip-package"
tar -xf "${good_parts}/codex-parts-${version}-x86_64-pc-windows-msvc-primary.tar" -C "${work}/members/zip-part"
tar -xf "${good_parts}/codex-parts-${version}-x86_64-pc-windows-msvc-app-server.tar" -C "${work}/members/zip-part"
unzip -q "${work}/dist-good/codex-package-x86_64-pc-windows-msvc.zip" -d "${work}/members/zip-package"
expect assembled-zip-members-identical diff -r "${work}/members/zip-part" "${work}/members/zip-package"

subset="${work}/parts-subset"
mkdir -p "${subset}"
ln "${good_parts}/codex-parts-${version}-x86_64-pc-windows-msvc-"* "${subset}/"
check assemble-target-subset ok 'assembled .*codex-package-x86_64-pc-windows-msvc\.zip$' \
  "${assemble[@]}" --parts "${subset}" --dist "${work}/dist-subset" --targets x86_64-pc-windows-msvc

# part output feeds assemble: the Linux primary part from the part section with
# an app-server part whose codex-app-server embeds that part's bwrap digest.
roundtrip="${work}/parts-roundtrip"
mkdir -p "${roundtrip}" "${work}/roundtrip-app/bin"
roundtrip_digest="$(sha256sum "${work}/unpacked-linux/bin/bwrap" | cut -d' ' -f1)"
fake x86_64-unknown-linux-gnu "${work}/roundtrip-app/bin/codex-app-server" "${roundtrip_digest}"
fake x86_64-unknown-linux-gnu "${work}/roundtrip-app/bin/codex-code-mode-host"
chmod 0755 "${work}/roundtrip-app/bin" "${work}/roundtrip-app/bin/"*
tar --format=ustar -C "${work}/roundtrip-app" \
  -cf "${roundtrip}/codex-parts-${version}-x86_64-unknown-linux-gnu-app-server.tar" bin
printf '# %s\n' "${roundtrip_digest}" >>"${work}/release-linux/codex"
PATH="${part_path}" check part-roundtrip-primary ok 'wrote ' \
  "${package}" part --version "${version}" --target x86_64-unknown-linux-gnu --bundle primary \
  --release-dir "${work}/release-linux" --out "${roundtrip}"
check assemble-from-part-output ok 'embed the bwrap SHA-256' \
  "${assemble[@]}" --parts "${roundtrip}" --dist "${work}/dist-roundtrip" --targets x86_64-unknown-linux-gnu

# Rejected: the four mistakes of the spec, then more.
parts="$(variant_parts missing-part)"
rm "${parts}/codex-parts-${version}-x86_64-apple-darwin-app-server.tar"
check assemble-missing-part fail "^release-package: x86_64-apple-darwin: missing app-server part codex-parts-${version}-x86_64-apple-darwin-app-server\.tar$" \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-missing-part"

hook_add_codex() { python3 -I "${work}/fake.py" "$(format_for "$2")" "$1/codex" && chmod 0755 "$1/codex"; }
parts="$(variant_parts both-parts)"
rm "${parts}/codex-parts-${version}-aarch64-apple-darwin-app-server.tar"
make_part aarch64-apple-darwin app-server "${parts}" hook_add_codex
check assemble-file-in-both-parts fail '^release-package: aarch64-apple-darwin: both parts contain codex$' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-both-parts"

hook_extra_file() { cp "$1/codex" "$1/codex-extra" && chmod 0755 "$1/codex-extra"; }
parts="$(variant_parts extra-file)"
rm "${parts}/codex-parts-${version}-aarch64-unknown-linux-gnu-primary.tar"
make_part aarch64-unknown-linux-gnu primary "${parts}" hook_extra_file
check assemble-extra-file fail '^release-package: aarch64-unknown-linux-gnu: layout mismatch in the primary part$' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-extra-file"

hook_no_digest() { python3 -I "${work}/fake.py" "$(format_for "$2")" "$1/codex-app-server" other-bwrap; }
parts="$(variant_parts no-digest)"
rm "${parts}/codex-parts-${version}-x86_64-unknown-linux-gnu-app-server.tar"
make_part x86_64-unknown-linux-gnu app-server "${parts}" hook_no_digest
check assemble-bwrap-digest-missing fail \
  '^release-package: x86_64-unknown-linux-gnu: codex-app-server does not embed the SHA-256 of the packaged bwrap \([0-9a-f]{64}\)' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-no-digest"

hook_other_bwrap() { python3 -I "${work}/fake.py" "$(format_for "$2")" "$1/bwrap" rebuilt; }
parts="$(variant_parts other-bwrap)"
rm "${parts}/codex-parts-${version}-aarch64-unknown-linux-gnu-primary.tar"
make_part aarch64-unknown-linux-gnu primary "${parts}" hook_other_bwrap
check assemble-bwrap-rebuilt fail '^release-package: aarch64-unknown-linux-gnu: codex does not embed the SHA-256 of the packaged bwrap' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-other-bwrap"

hook_group_writable() { chmod 0775 "$1/codex"; }
parts="$(variant_parts mode)"
rm "${parts}/codex-parts-${version}-aarch64-apple-darwin-primary.tar"
make_part aarch64-apple-darwin primary "${parts}" hook_group_writable
check assemble-mode-0775 fail '^release-package: aarch64-apple-darwin: unexpected entries' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-mode"

hook_symlink() { rm "$1/codex-code-mode-host" && ln -s codex-app-server "$1/codex-code-mode-host"; }
parts="$(variant_parts symlink)"
rm "${parts}/codex-parts-${version}-x86_64-apple-darwin-app-server.tar"
make_part x86_64-apple-darwin app-server "${parts}" hook_symlink
check assemble-symlink fail '^release-package: x86_64-apple-darwin: unexpected entries' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-symlink"

# GNU tar strips ".." on create, so Python writes the entry that escapes bin/.
parts="$(variant_parts traversal)"
rm "${parts}/codex-parts-${version}-x86_64-pc-windows-msvc-primary.tar"
mkdir -p "${work}/traversal-stage/bin"
for file in $(bundle_files x86_64-pc-windows-msvc primary); do
  fake x86_64-pc-windows-msvc "${work}/traversal-stage/bin/${file}"
done
chmod 0755 "${work}/traversal-stage/bin" "${work}/traversal-stage/bin/"*
python3 -I - "${parts}/codex-parts-${version}-x86_64-pc-windows-msvc-primary.tar" "${work}/traversal-stage" <<'PY'
import os
import sys
import tarfile

out, root = sys.argv[1], sys.argv[2]
with tarfile.open(out, "w", format=tarfile.USTAR_FORMAT) as tar:
    tar.add(os.path.join(root, "bin"), "bin", recursive=False)
    for name in sorted(os.listdir(os.path.join(root, "bin"))):
        tar.add(os.path.join(root, "bin", name), "bin/" + name)
    tar.add(os.path.join(root, "bin", "codex.exe"), "bin/../escape.exe")
PY
check assemble-unsafe-entry fail '^release-package: x86_64-pc-windows-msvc: unexpected entries' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-traversal"

parts="$(variant_parts stray)"
echo notes >"${parts}/notes.txt"
check assemble-unexpected-file fail '^release-package: unexpected files in .*: notes\.txt$' \
  "${assemble[@]}" --parts "${parts}" --dist "${work}/dist-stray"

mkdir -p "${work}/dist-busy"
echo old >"${work}/dist-busy/codex-package-x86_64-unknown-linux-gnu.tar.gz"
check assemble-dist-not-empty fail 'is not empty' \
  "${assemble[@]}" --parts "${good_parts}" --dist "${work}/dist-busy"

check assemble-unknown-target fail "unknown target 'riscv64gc-unknown-linux-gnu'" \
  "${assemble[@]}" --parts "${good_parts}" --dist "${work}/dist-unknown" --targets riscv64gc-unknown-linux-gnu

# part and assemble remove their staging directories in every case above.
leftovers="$(find "${pkg_tmp}" -mindepth 1 -maxdepth 1)"
if [[ -z "${leftovers}" ]]; then ok temp-dirs-removed; else fail "temp-dirs-removed: ${leftovers}"; fi

echo "release-package self-test: ${passed} passed, ${failures} failed"
[[ ${failures} -eq 0 ]]
