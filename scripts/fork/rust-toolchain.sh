#!/usr/bin/env bash
# Prints the Rust toolchain channel pinned in codex-rs/rust-toolchain.toml.
#
#   scripts/fork/rust-toolchain.sh [TOOLCHAIN_FILE]
#
# .github/actions/fork-rust-toolchain installs this channel, so fork CI and the
# release builds use the toolchain that upstream pins instead of a copy of it.
# Written for bash 3.2 and POSIX awk: release builds run it on macOS and
# Windows runners too.

set -euo pipefail

file="${1:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)/codex-rs/rust-toolchain.toml}"
if [ ! -f "${file}" ]; then
  printf '%s: no toolchain file %s\n' "${0##*/}" "${file}" >&2
  exit 1
fi

# The `channel` key of the [toolchain] table, without quotes or a trailing comment.
channel="$(tr -d '\r' <"${file}" | awk '
  /^[[:space:]]*\[/ { table = $0; gsub(/[[:space:]]/, "", table); next }
  table == "[toolchain]" && /^[[:space:]]*channel[[:space:]]*=/ {
    value = $0
    sub(/^[^=]*=[[:space:]]*/, "", value)
    sub(/[[:space:]]*#.*$/, "", value)
    sub(/[[:space:]]+$/, "", value)
    if (value ~ /^"[^"]*"$/ || value ~ /^'\''[^'\'']*'\''$/) {
      print substr(value, 2, length(value) - 2)
    }
    exit
  }
')"

case "${channel}" in
  "" | *[!A-Za-z0-9._-]*)
    printf '%s: no valid [toolchain] channel in %s (got "%s")\n' "${0##*/}" "${file}" "${channel}" >&2
    exit 1
    ;;
esac
printf '%s\n' "${channel}"
