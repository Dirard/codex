#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: build-codex-package-archive.sh \
  --target <rust-target> \
  --bundle <primary|app-server> \
  --entrypoint-dir <dir> \
  --archive-dir <dir> \
  --rg-bin <path> \
  [--require-materialized-helper-sources] \
  [--bwrap-bin <path>] \
  [--code-mode-host-bin <path>] \
  [--zsh-manifest <path>] \
  [--zsh-bin <path>] \
  [--codex-command-runner-bin <path>] \
  [--codex-windows-sandbox-setup-bin <path>] \
  [--target-suffixed-entrypoint]
EOF
}

target=""
bundle=""
entrypoint_dir=""
archive_dir=""
target_suffixed_entrypoint="false"
resource_args=()
rg_bin_provided="false"
zsh_bin_provided="false"
bwrap_bin_provided="false"
code_mode_host_bin_provided="false"
command_runner_bin_provided="false"
sandbox_setup_bin_provided="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target="${2:?--target requires a value}"
      shift 2
      ;;
    --bundle)
      bundle="${2:?--bundle requires a value}"
      shift 2
      ;;
    --entrypoint-dir)
      entrypoint_dir="${2:?--entrypoint-dir requires a value}"
      shift 2
      ;;
    --archive-dir)
      archive_dir="${2:?--archive-dir requires a value}"
      shift 2
      ;;
    --rg-bin)
      resource_args+=(--rg-bin "${2:?--rg-bin requires a value}")
      rg_bin_provided="true"
      shift 2
      ;;
    --require-materialized-helper-sources)
      shift
      ;;
    --bwrap-bin)
      resource_args+=(--bwrap-bin "${2:?--bwrap-bin requires a value}")
      bwrap_bin_provided="true"
      shift 2
      ;;
    --code-mode-host-bin)
      resource_args+=(--code-mode-host-bin "${2:?--code-mode-host-bin requires a value}")
      code_mode_host_bin_provided="true"
      shift 2
      ;;
    --zsh-manifest)
      resource_args+=(--zsh-manifest "${2:?--zsh-manifest requires a value}")
      shift 2
      ;;
    --zsh-bin)
      resource_args+=(--zsh-bin "${2:?--zsh-bin requires a value}")
      zsh_bin_provided="true"
      shift 2
      ;;
    --codex-command-runner-bin)
      resource_args+=(
        --codex-command-runner-bin
        "${2:?--codex-command-runner-bin requires a value}"
      )
      command_runner_bin_provided="true"
      shift 2
      ;;
    --codex-windows-sandbox-setup-bin)
      resource_args+=(
        --codex-windows-sandbox-setup-bin
        "${2:?--codex-windows-sandbox-setup-bin requires a value}"
      )
      sandbox_setup_bin_provided="true"
      shift 2
      ;;
    --target-suffixed-entrypoint)
      target_suffixed_entrypoint="true"
      shift
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

if [[ -z "$target" || -z "$bundle" || -z "$entrypoint_dir" || -z "$archive_dir" ]]; then
  usage >&2
  exit 1
fi

if [[ "$rg_bin_provided" == "false" ]]; then
  echo "Stage 5G package source hermeticity requires --rg-bin." >&2
  exit 1
fi

case "$bundle" in
  primary)
    variant="codex"
    entrypoint="codex"
    archive_stem="codex-package"
    ;;
  app-server)
    variant="codex-app-server"
    entrypoint="codex-app-server"
    archive_stem="codex-app-server-package"
    ;;
  *)
    echo "No Codex package variant for bundle: $bundle" >&2
    exit 1
    ;;
esac

exe_suffix=""
case "$target" in
  *windows*)
    exe_suffix=".exe"
    ;;
esac

code_mode_host_bin="${entrypoint_dir%/}/codex-code-mode-host${exe_suffix}"
if [[ "$code_mode_host_bin_provided" == "false" && -f "$code_mode_host_bin" ]]; then
  resource_args+=(--code-mode-host-bin "$code_mode_host_bin")
fi

case "$target" in
  *windows*)
    ;;
  *)
    if [[ "$zsh_bin_provided" == "false" ]]; then
      echo "Stage 5G package source hermeticity requires --zsh-bin for $target." >&2
      exit 1
    fi
    ;;
esac

entrypoint_name="$entrypoint"
if [[ "$target_suffixed_entrypoint" == "true" ]]; then
  entrypoint_name="${entrypoint_name}-${target}"
fi

case "$target" in
  *linux*)
    bwrap_bin="${entrypoint_dir%/}/bwrap"
    if [[ "$bwrap_bin_provided" == "false" ]]; then
      if [[ ! -x "$bwrap_bin" ]]; then
        echo "Stage 5G package source hermeticity requires bwrap at $bwrap_bin or --bwrap-bin for $target." >&2
        exit 1
      fi
      resource_args+=(--bwrap-bin "$bwrap_bin")
    fi
    ;;
  *windows*)
    command_runner_bin="${entrypoint_dir%/}/codex-command-runner.exe"
    sandbox_setup_bin="${entrypoint_dir%/}/codex-windows-sandbox-setup.exe"
    if [[ "$command_runner_bin_provided" == "false" ]]; then
      if [[ ! -f "$command_runner_bin" ]]; then
        echo "Stage 5G package source hermeticity requires codex-command-runner.exe at $command_runner_bin or --codex-command-runner-bin for $target." >&2
        exit 1
      fi
      resource_args+=(--codex-command-runner-bin "$command_runner_bin")
    fi
    if [[ "$sandbox_setup_bin_provided" == "false" ]]; then
      if [[ ! -f "$sandbox_setup_bin" ]]; then
        echo "Stage 5G package source hermeticity requires codex-windows-sandbox-setup.exe at $sandbox_setup_bin or --codex-windows-sandbox-setup-bin for $target." >&2
        exit 1
      fi
      resource_args+=(--codex-windows-sandbox-setup-bin "$sandbox_setup_bin")
    fi
    ;;
esac

repo_root="${GITHUB_WORKSPACE:-}"
if [[ -z "$repo_root" ]]; then
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fi

zstd_bin="$(command -v zstd || true)"
if [[ -z "$zstd_bin" ]]; then
  echo "zstd is required before package archive assembly starts." >&2
  exit 1
fi
zstd_dir="$(cd "$(dirname "$zstd_bin")" && pwd -P)"
zstd_real="${zstd_dir}/$(basename "$zstd_bin")"
repo_zstd="${repo_root%/}/.github/workflows/zstd"
if [[ "$zstd_real" == "$repo_zstd" ]]; then
  echo "Repo DotSlash zstd manifest is not allowed for package archives: $zstd_real" >&2
  exit 1
fi
if [[ "$zstd_real" == "${repo_root%/}/"* && -f "$zstd_real" ]] && head -n 1 "$zstd_real" | grep -qi 'dotslash'; then
  echo "Repo DotSlash zstd manifest is not allowed for package archives: $zstd_real" >&2
  exit 1
fi

if command -v python3 >/dev/null 2>&1; then
  python_bin="python3"
else
  python_bin="python"
fi

mkdir -p "$archive_dir"
package_dir="${RUNNER_TEMP:-/tmp}/${archive_stem}-${target}"
gzip_archive_path="${archive_dir}/${archive_stem}-${target}.tar.gz"
zstd_archive_path="${archive_dir}/${archive_stem}-${target}.tar.zst"
rm -rf "$package_dir"

python_args=(
  "${repo_root}/scripts/build_codex_package.py"
  --target "$target"
  --variant "$variant"
  --entrypoint-bin "${entrypoint_dir%/}/${entrypoint_name}${exe_suffix}"
  --cargo-profile release
  --require-materialized-helper-sources
  --package-dir "$package_dir"
  --archive-output "$gzip_archive_path"
  --archive-output "$zstd_archive_path"
)
if ((${#resource_args[@]} > 0)); then
  python_args+=("${resource_args[@]}")
fi
python_args+=(--force)

"$python_bin" "${python_args[@]}"
