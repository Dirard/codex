#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: stage-codex-runtime.sh --out <dir> [options]

Options:
  --bazel-target <triple>          Release target triple to stage.
  --helper-root <dir>              Stage 5G materialized helper root.
  --cargo-profile <profile>        dev or release. debug is not accepted.
  --release-package-archive        Stage through the package-builder archive path.
  --zstd-source <path>             Explicit materialized zstd executable.
  --github-env <file>              Append CODEX_EXEC_PATH for GitHub Actions.
  --print-shell-env                Print shell exports for local use.
  --verify-sandbox --exec-path <path>
                                  Verify an already staged runtime layout.
  -h, --help                       Show this help.
EOF
}

repo_root="${GITHUB_WORKSPACE:-}"
if [[ -z "$repo_root" ]]; then
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi

out=""
target=""
helper_root="${CODEX_PACKAGE_HELPER_ROOT:-}"
cargo_profile="dev"
github_env=""
print_shell_env=0
verify_sandbox=0
exec_path=""
release_package_archive=0
zstd_source=""
zstd_source_kind=""
windows_release_shaped_msvc=0
windows_msvc_host_platform=0
package_archive_gzip=""
package_archive_zstd=""
package_archive_sha256=""
package_archive_inventory_path=""
build_metadata_job=""
seed_provenance=""
bazel_compilation_mode="fastbuild"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      out="${2:?--out requires a value}"
      shift 2
      ;;
    --bazel-target)
      target="${2:?--bazel-target requires a value}"
      shift 2
      ;;
    --helper-root)
      helper_root="${2:?--helper-root requires a value}"
      shift 2
      ;;
    --cargo-profile)
      cargo_profile="${2:?--cargo-profile requires a value}"
      shift 2
      ;;
    --github-env)
      github_env="${2:?--github-env requires a value}"
      shift 2
      ;;
    --print-shell-env)
      print_shell_env=1
      shift
      ;;
    --verify-sandbox)
      verify_sandbox=1
      shift
      ;;
    --exec-path)
      exec_path="${2:?--exec-path requires a value}"
      shift 2
      ;;
    --release-package-archive)
      release_package_archive=1
      shift
      ;;
    --zstd-source)
      zstd_source="${2:?--zstd-source requires a value}"
      shift 2
      ;;
    --windows-release-shaped-msvc)
      windows_release_shaped_msvc=1
      shift
      ;;
    --windows-msvc-host-platform)
      windows_msvc_host_platform=1
      shift
      ;;
    --build-metadata-job)
      build_metadata_job="${2:?--build-metadata-job requires a value}"
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

if [[ "$cargo_profile" == "debug" ]]; then
  echo "--cargo-profile debug is not a workspace profile; use dev or release." >&2
  exit 1
fi
case "$cargo_profile" in
  dev|release)
    ;;
  *)
    echo "Unsupported --cargo-profile: $cargo_profile" >&2
    exit 1
    ;;
esac

if [[ "$release_package_archive" -eq 1 && "$cargo_profile" != "release" ]]; then
  echo "--release-package-archive requires --cargo-profile release." >&2
  exit 1
fi
if [[ "$cargo_profile" == "release" ]]; then
  bazel_compilation_mode="opt"
fi

default_target() {
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64|Linux-amd64) echo "x86_64-unknown-linux-musl" ;;
    Linux-aarch64|Linux-arm64) echo "aarch64-unknown-linux-musl" ;;
    Darwin-x86_64|Darwin-amd64) echo "x86_64-apple-darwin" ;;
    Darwin-aarch64|Darwin-arm64) echo "aarch64-apple-darwin" ;;
    MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) echo "x86_64-pc-windows-msvc" ;;
    MINGW*-aarch64|MSYS*-aarch64|CYGWIN*-aarch64|MINGW*-arm64|MSYS*-arm64|CYGWIN*-arm64) echo "aarch64-pc-windows-msvc" ;;
    *)
      echo "Unable to infer --bazel-target for $(uname -s)/$(uname -m)" >&2
      return 1
      ;;
  esac
}

target="${target:-$(default_target)}"

target_platform() {
  case "$1" in
    x86_64-unknown-linux-musl) echo "linux_amd64_musl" ;;
    aarch64-unknown-linux-musl) echo "linux_arm64_musl" ;;
    x86_64-apple-darwin) echo "macos_amd64" ;;
    aarch64-apple-darwin) echo "macos_arm64" ;;
    x86_64-pc-windows-msvc) echo "windows_amd64" ;;
    aarch64-pc-windows-msvc) echo "windows_arm64" ;;
    *)
      echo "Unsupported --bazel-target: $1" >&2
      return 1
      ;;
  esac
}

is_windows_target() {
  [[ "$target" == *windows* ]]
}

is_linux_target() {
  [[ "$target" == *linux* ]]
}

entrypoint_name() {
  if is_windows_target; then
    echo "codex.exe"
  else
    echo "codex"
  fi
}

code_mode_host_name() {
  if is_windows_target; then
    echo "codex-code-mode-host.exe"
  else
    echo "codex-code-mode-host"
  fi
}

rg_name() {
  if is_windows_target; then
    echo "rg.exe"
  else
    echo "rg"
  fi
}

verify_layout() {
  local root="$1"
  local entrypoint
  entrypoint="$(entrypoint_name)"
  for required in \
    "$root/codex-package.json" \
    "$root/bin/$entrypoint" \
    "$root/bin/$(code_mode_host_name)" \
    "$root/codex-path/$(rg_name)"; do
    if [[ ! -f "$required" ]]; then
      echo "Missing staged runtime file: $required" >&2
      return 1
    fi
  done
  for runtime_binary in \
    "$root/bin/$entrypoint" \
    "$root/bin/$(code_mode_host_name)"; do
    if [[ -L "$runtime_binary" ]]; then
      echo "Staged runtime binary must be a real executable, not a symlink: $runtime_binary" >&2
      return 1
    fi
  done
  if ! is_windows_target && [[ ! -f "$root/codex-resources/zsh/bin/zsh" ]]; then
    echo "Missing staged zsh helper: $root/codex-resources/zsh/bin/zsh" >&2
    return 1
  fi
  if is_linux_target && [[ ! -f "$root/codex-resources/bwrap" ]]; then
    echo "Missing staged bwrap helper: $root/codex-resources/bwrap" >&2
    return 1
  fi
  if is_windows_target; then
    for helper in codex-command-runner.exe codex-windows-sandbox-setup.exe; do
      if [[ ! -f "$root/codex-resources/$helper" ]]; then
        echo "Missing staged Windows helper: $root/codex-resources/$helper" >&2
        return 1
      fi
    done
  fi
}

verify_linux_sandbox() {
  local root="$1"
  local bwrap="$root/codex-resources/bwrap"
  if ! is_linux_target; then
    return 0
  fi
  if [[ ! -x "$bwrap" ]]; then
    echo "Linux sandbox smoke requires executable staged bwrap helper: $bwrap" >&2
    return 1
  fi
  "$bwrap" \
    --unshare-user \
    --unshare-ipc \
    --unshare-pid \
    --proc /proc \
    --dev /dev \
    --ro-bind / / \
    --tmpfs /tmp \
    --die-with-parent \
    /bin/sh -c 'test -d /proc/self && test -w /tmp' >/dev/null
}

if [[ "$verify_sandbox" -eq 1 ]]; then
  if [[ -z "$exec_path" ]]; then
    echo "--verify-sandbox requires --exec-path" >&2
    exit 1
  fi
  exec_dir="$(cd "$(dirname "$exec_path")" && pwd -P)"
  target="$(python3 - "$exec_dir/../codex-package.json" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["target"])
PY
)"
  verify_layout "$exec_dir/.."
  verify_linux_sandbox "$exec_dir/.."
  exit 0
fi

if [[ -z "$out" ]]; then
  usage >&2
  exit 1
fi
if [[ -z "$helper_root" ]]; then
  echo "--helper-root or CODEX_PACKAGE_HELPER_ROOT is required." >&2
  exit 1
fi

copy_helper() {
  local src="$1"
  local dest="$2"
  if [[ ! -f "$src" ]]; then
    echo "Missing materialized helper: $src" >&2
    exit 1
  fi
  if [[ ! -x "$src" ]]; then
    echo "Materialized helper is not executable: $src" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
  chmod 0755 "$dest" || true
}

verify_helper_root() {
  local manifest="${helper_root%/}/${target}/codex-package-helpers.json"
  if [[ ! -f "$manifest" ]]; then
    echo "Missing Stage 5G helper manifest: $manifest" >&2
    exit 1
  fi
  PYTHONPATH="$repo_root/scripts" python3 -m codex_package.materialize_helpers \
    --target "$target" \
    --output-root "$helper_root" \
    --verify-only >&2
}

seed_root_from_bazel() {
  local platform
  local label
  local metadata
  local bazel_host_platform_args=()
  local bazel_mode_args=()
  if [[ "$cargo_profile" == "release" ]]; then
    bazel_mode_args=(-c opt)
  fi
  platform="$(target_platform "$target")"
  label="//codex-rs/cli:codex_go_sdk_runtime_layout_${platform}"
  if [[ "$windows_msvc_host_platform" -eq 1 ]]; then
    if ! is_windows_target; then
      echo "--windows-msvc-host-platform requires a Windows target." >&2
      exit 1
    fi
    bazel_host_platform_args+=(--host_platform=//:local_windows_msvc)
  fi

  if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
    "$repo_root/.github/scripts/run-bazel-ci.sh" \
      --remote-download-toplevel \
      -- build "${bazel_mode_args[@]}" "${bazel_host_platform_args[@]}" -- "$label"
  else
    bazel build "${bazel_mode_args[@]}" "${bazel_host_platform_args[@]}" "$label"
  fi

  if [[ "${GITHUB_ACTIONS:-}" == "true" ]]; then
    metadata="$(
      "$repo_root/.github/scripts/run-bazel-ci.sh" \
        -- cquery "${bazel_mode_args[@]}" "${bazel_host_platform_args[@]}" --output=files "$label" \
        | grep '/codex-package.json$' \
        | head -n 1
    )"
  else
    metadata="$(
      bazel cquery "${bazel_mode_args[@]}" "${bazel_host_platform_args[@]}" --output=files "$label" \
        | grep '/codex-package.json$' \
        | head -n 1
    )"
  fi
  if [[ -z "$metadata" ]]; then
    echo "Unable to locate codex-package.json from Bazel target $label" >&2
    exit 1
  fi
  cd "$(dirname "$metadata")" && pwd -P
}

python_bin() {
  if command -v python3 >/dev/null 2>&1; then
    echo python3
  else
    echo python
  fi
}

reject_dotslash_zstd() {
  local candidate="$1"
  local candidate_dir
  local candidate_real
  local repo_zstd_dir
  local repo_zstd

  candidate_dir="$(cd "$(dirname "$candidate")" && pwd -P)"
  candidate_real="$candidate_dir/$(basename "$candidate")"
  repo_zstd_dir="${repo_root%/}/.github/workflows"
  repo_zstd="$repo_zstd_dir/zstd"
  if [[ "$candidate_real" == "$repo_zstd" ]]; then
    echo "Repo DotSlash zstd manifest is not allowed for package archives: $candidate_real" >&2
    exit 1
  fi
  if head -n 1 "$candidate_real" 2>/dev/null | grep -qi "dotslash"; then
    echo "DotSlash-backed zstd wrapper is not allowed for package archives: $candidate_real" >&2
    exit 1
  fi
}

preflight_zstd() {
  if [[ -n "$zstd_source" ]]; then
    local zstd_bind_dir
    local zstd_resolved
    local zstd_source_dir
    local zstd_source_real

    if [[ ! -x "$zstd_source" ]]; then
      echo "--zstd-source must point at an executable zstd binary: $zstd_source" >&2
      exit 1
    fi
    zstd_source_dir="$(cd "$(dirname "$zstd_source")" && pwd -P)"
    zstd_source_real="$zstd_source_dir/$(basename "$zstd_source")"
    reject_dotslash_zstd "$zstd_source_real"
    zstd_bind_dir="$(mktemp -d "${TMPDIR:-/tmp}/codex-go-sdk-zstd-source.XXXXXX")"
    if ! ln -s "$zstd_source_real" "$zstd_bind_dir/zstd" 2>/dev/null; then
      cp "$zstd_source_real" "$zstd_bind_dir/zstd"
      chmod +x "$zstd_bind_dir/zstd"
    fi
    PATH="$zstd_bind_dir:$PATH"
    export PATH
    zstd_resolved="$(command -v zstd || true)"
    if [[ "$zstd_resolved" != "$zstd_bind_dir/zstd" ]]; then
      echo "--zstd-source did not bind the executable zstd command: $zstd_resolved" >&2
      exit 1
    fi
    zstd_source_kind="stage5gMaterialized"
    return
  fi

  zstd_bin="$(command -v zstd || true)"
  if [[ -z "$zstd_bin" ]]; then
    echo "zstd is required for --release-package-archive unless --zstd-source points at a materialized executable." >&2
    exit 1
  fi
  zstd_dir="$(cd "$(dirname "$zstd_bin")" && pwd -P)"
  zstd_real="$zstd_dir/$(basename "$zstd_bin")"
  reject_dotslash_zstd "$zstd_real"
  zstd_source_kind="preinstalled"
}

stage_from_seed() {
  seed_root="${CODEX_GO_SDK_TEST_LAYOUT_ROOT:-}"
  if [[ -z "$seed_root" ]]; then
    seed_provenance="bazelOutput"
    seed_root="$(seed_root_from_bazel)"
  else
    seed_provenance="testFixture"
  fi
  if [[ ! -f "$seed_root/codex-package.json" ]]; then
    echo "Bazel runtime seed is missing codex-package.json: $seed_root" >&2
    exit 1
  fi

  rm -rf "$out"
  mkdir -p "$out"
  cp -R "$seed_root"/. "$out"/
  materialize_staged_binaries "$seed_root" "$out"
  merge_verified_helpers "$out"
  write_metadata "bazelLayout" "[]"
}

materialize_staged_binaries() {
  local seed_root="$1"
  local out_root="$2"
  materialize_staged_binary "$seed_root" "$out_root" "bin/$(entrypoint_name)" "entrypoint"
  materialize_staged_binary "$seed_root" "$out_root" "bin/$(code_mode_host_name)" "code-mode host"
}

materialize_staged_binary() {
  local seed_root="$1"
  local out_root="$2"
  local relative_path="$3"
  local description="$4"
  local seed_binary="$seed_root/$relative_path"
  local staged_binary="$out_root/$relative_path"

  if [[ ! -x "$seed_binary" ]]; then
    echo "Bazel runtime seed $description is not executable: $seed_binary" >&2
    exit 1
  fi
  rm -f "$staged_binary"
  cp -L "$seed_binary" "$staged_binary"
  chmod +x "$staged_binary"
  if [[ -L "$staged_binary" ]]; then
    echo "Staged runtime $description must be a real executable, not a symlink: $staged_binary" >&2
    exit 1
  fi
}

stage_from_archive() {
  local seed_root
  local archive_root
  local archive_dir
  local gzip_archive
  local zstd_archive
  local -a resource_args
  seed_root="${CODEX_GO_SDK_TEST_LAYOUT_ROOT:-}"
  if [[ -z "$seed_root" ]]; then
    seed_provenance="bazelOutput"
    seed_root="$(seed_root_from_bazel)"
  else
    seed_provenance="testFixture"
  fi
  if [[ ! -x "$seed_root/bin/$(entrypoint_name)" ]]; then
    echo "Package archive staging requires executable seed entrypoint: $seed_root/bin/$(entrypoint_name)" >&2
    exit 1
  fi
  if [[ ! -x "$seed_root/bin/$(code_mode_host_name)" ]]; then
    echo "Package archive staging requires executable code-mode host: $seed_root/bin/$(code_mode_host_name)" >&2
    exit 1
  fi

  preflight_zstd
  archive_root="$(mktemp -d "${TMPDIR:-/tmp}/codex-go-sdk-package-archive.XXXXXX")"
  archive_dir="$archive_root/archives"
  gzip_archive="$archive_dir/codex-package-${target}.tar.gz"
  zstd_archive="$archive_dir/codex-package-${target}.tar.zst"
  package_archive_gzip="$gzip_archive"
  package_archive_zstd="$zstd_archive"
  resource_args=(--rg-bin "$helper_target_root/$(rg_name)")
  if ! is_windows_target; then
    resource_args+=(--zsh-bin "$helper_target_root/zsh")
  fi
  if is_linux_target; then
    resource_args+=(--bwrap-bin "$helper_target_root/bwrap")
  fi
  if is_windows_target; then
    resource_args+=(
      --codex-command-runner-bin "$helper_target_root/codex-command-runner.exe"
      --codex-windows-sandbox-setup-bin "$helper_target_root/codex-windows-sandbox-setup.exe"
    )
  fi

  bash "$repo_root/.github/scripts/build-codex-package-archive.sh" \
    --target "$target" \
    --bundle primary \
    --entrypoint-dir "$seed_root/bin" \
    --archive-dir "$archive_dir" \
    --code-mode-host-bin "$seed_root/bin/$(code_mode_host_name)" \
    --require-materialized-helper-sources \
    "${resource_args[@]}" \
    >&2

  package_archive_sha256="$(sha256sum "$zstd_archive" | awk '{print $1}')"
  package_archive_inventory_path="$archive_root/package-archive-inventory.txt"
  zstd -dc "$zstd_archive" | tar -tf - | sed 's#^\./##' | sort -u >"$package_archive_inventory_path"
  if [[ ! -s "$package_archive_inventory_path" ]]; then
    echo "Package archive inventory is empty: $zstd_archive" >&2
    exit 1
  fi

  rm -rf "$out"
  mkdir -p "$out"
  zstd -dc "$zstd_archive" | tar -xf - -C "$out"
  verify_layout "$out"
  write_metadata "packageArchive" '["tar.gz","tar.zst"]'
}

merge_verified_helpers() {
  local dest_root="$1"
  mkdir -p "$dest_root/codex-resources" "$dest_root/codex-path"
  copy_helper "$helper_target_root/$(rg_name)" "$dest_root/codex-path/$(rg_name)"
  if ! is_windows_target; then
    copy_helper "$helper_target_root/zsh" "$dest_root/codex-resources/zsh/bin/zsh"
  fi
  if is_linux_target; then
    copy_helper "$helper_target_root/bwrap" "$dest_root/codex-resources/bwrap"
  fi
  if is_windows_target; then
    copy_helper "$helper_target_root/codex-command-runner.exe" \
      "$dest_root/codex-resources/codex-command-runner.exe"
    copy_helper "$helper_target_root/codex-windows-sandbox-setup.exe" \
      "$dest_root/codex-resources/codex-windows-sandbox-setup.exe"
  fi
  verify_layout "$dest_root"
}

write_metadata() {
  local runtime_source="$1"
  local archive_formats_json="$2"
  local python
  python="$(python_bin)"
  "$python" - "$out/codex-go-sdk-runtime-staging.json" <<PY
import json
import os
import sys

metadata = {
    "archiveFormats": ${archive_formats_json},
    "bazelCompilationMode": "${bazel_compilation_mode}",
    "bazelTarget": "${target}",
    "buildMetadataJob": "${build_metadata_job}",
    "cargoProfile": "${cargo_profile}",
    "codeExecPath": os.path.abspath("${out}/bin/$(entrypoint_name)"),
    "layoutTarget": "${target}",
    "runtimeSource": "${runtime_source}",
    "seedProvenance": "${seed_provenance}",
    "windowsMsvcHostPlatform": bool(${windows_msvc_host_platform}),
    "windowsReleaseShapedMsvc": bool(${windows_release_shaped_msvc}),
    "zstdSource": "${zstd_source_kind}",
}
helper_manifest_path = os.path.abspath("${helper_target_root}/codex-package-helpers.json")
if os.path.isfile(helper_manifest_path):
    with open(helper_manifest_path, encoding="utf-8") as handle:
        helper_manifest = json.load(handle)
    helper_files = sorted(
        str(entry.get("relativePath", ""))
        for entry in (helper_manifest.get("helpers") or {}).values()
    )
    metadata["helperManifest"] = {
        "files": helper_files,
        "helpers": helper_manifest.get("helpers", {}),
        "path": helper_manifest_path,
        "schemaVersion": helper_manifest.get("schemaVersion"),
        "target": helper_manifest.get("target"),
    }
if "${runtime_source}" == "packageArchive":
    with open("${package_archive_inventory_path}", encoding="utf-8") as handle:
        package_archive_inventory = [line.strip() for line in handle if line.strip()]
    metadata["packageArchive"] = {
        "archiveFormats": ${archive_formats_json},
        "builder": ".github/scripts/build-codex-package-archive.sh",
        "gzipPath": os.path.abspath("${package_archive_gzip}"),
        "inventory": package_archive_inventory,
        "path": os.path.abspath("${package_archive_zstd}"),
        "sha256": "${package_archive_sha256}",
        "target": "${target}",
        "windowsMsvcHostPlatform": bool(${windows_msvc_host_platform}),
        "windowsReleaseShapedMsvc": bool(${windows_release_shaped_msvc}),
    }
with open(sys.argv[1], "w", encoding="utf-8") as handle:
    json.dump(metadata, handle, indent=2, sort_keys=True)
    handle.write("\\n")
PY
}

emit_environment() {
  exec_path="$out/bin/$(entrypoint_name)"
  code_home="$out/codex-home"
  mkdir -p "$code_home"
  if [[ -n "$github_env" ]]; then
    echo "CODEX_EXEC_PATH=$exec_path" >> "$github_env"
    echo "CODEX_RUNTIME_METADATA_PATH=$out/codex-go-sdk-runtime-staging.json" >> "$github_env"
  fi
  if [[ "$print_shell_env" -eq 1 ]]; then
    printf 'export CODEX_EXEC_PATH=%q\n' "$exec_path"
    printf 'export CODEX_HOME=%q\n' "$code_home"
    printf 'export CODEX_GO_SDK_RUNTIME_ROOT=%q\n' "$out"
    printf 'export CODEX_RUNTIME_METADATA_PATH=%q\n' "$out/codex-go-sdk-runtime-staging.json"
  fi
}

verify_helper_root

helper_target_root="${helper_root%/}/${target}"
if [[ "$release_package_archive" -eq 1 ]]; then
  stage_from_archive
else
  stage_from_seed
fi

emit_environment
