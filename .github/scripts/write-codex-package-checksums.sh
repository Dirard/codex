#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: write-codex-package-checksums.sh --dist <dir> [--manifest <path>]
EOF
}

dist_dir=""
manifest=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dist)
      dist_dir="${2:?--dist requires a value}"
      shift 2
      ;;
    --manifest)
      manifest="${2:?--manifest requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unexpected argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$dist_dir" ]]; then
  usage >&2
  exit 1
fi

if [[ -z "$manifest" ]]; then
  manifest="${dist_dir%/}/codex-package_SHA256SUMS"
fi

tmp_manifest="$(mktemp)"
find "$dist_dir" -type f \
  \( -name 'codex-package-*.tar.gz' \
    -o -name 'codex-package-*.tar.zst' \
    -o -name 'codex-app-server-package-*.tar.gz' \
    -o -name 'codex-app-server-package-*.tar.zst' \) \
  -print |
  sort |
  while IFS= read -r archive; do
    sha256sum "$archive" |
      awk -v name="$(basename "$archive")" '{ print $1 "  " name }'
  done > "$tmp_manifest"

if [[ ! -s "$tmp_manifest" ]]; then
  echo "No Codex package archives found for checksum manifest" >&2
  rm -f "$tmp_manifest"
  exit 1
fi

mkdir -p "$(dirname "$manifest")"
mv "$tmp_manifest" "$manifest"
cat "$manifest"
