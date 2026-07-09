"""Materialize release-owned package helper payloads for strict assembly."""

import argparse
import shutil
import stat
from pathlib import Path

from .dotslash import archive_filename
from .dotslash import artifact_for_target
from .dotslash import download_archive
from .dotslash import extract_archive_member
from .dotslash import verify_archive
from .helper_manifest import write_helper_manifest
from .helper_manifest import verify_helper_manifest
from .ripgrep import RG_MANIFEST
from .targets import TARGET_SPECS
from .targets import TargetSpec
from .targets import resolve_materialized_input_path
from .zsh import ZSH_MANIFEST


def main() -> int:
    args = parse_args()
    spec = TARGET_SPECS[args.target]
    target_root = args.output_root / spec.target
    if args.verify_only:
        verify_helper_manifest(spec, target_root)
        print(f"Verified materialized package helpers at {target_root}")
        return 0

    target_root = materialize_target_helpers(
        spec,
        output_root=args.output_root,
        bwrap_bin=args.bwrap_bin,
        codex_command_runner_bin=args.codex_command_runner_bin,
        codex_windows_sandbox_setup_bin=args.codex_windows_sandbox_setup_bin,
    )
    print(f"Materialized package helpers at {target_root}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Materialize package helper payloads before strict assembly.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--target", required=True, choices=sorted(TARGET_SPECS))
    parser.add_argument("--output-root", required=True, type=Path)
    parser.add_argument("--bwrap-bin", type=Path)
    parser.add_argument("--codex-command-runner-bin", type=Path)
    parser.add_argument("--codex-windows-sandbox-setup-bin", type=Path)
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help=(
            "Verify an existing materialized helper root and its manifest "
            "without downloading, copying, or resolving helpers."
        ),
    )
    return parser.parse_args()


def materialize_target_helpers(
    spec: TargetSpec,
    *,
    output_root: Path,
    bwrap_bin: Path | None = None,
    codex_command_runner_bin: Path | None = None,
    codex_windows_sandbox_setup_bin: Path | None = None,
) -> Path:
    target_root = output_root / spec.target
    target_root.mkdir(parents=True, exist_ok=True)

    materialize_manifest_helper(
        spec,
        manifest_path=RG_MANIFEST,
        artifact_label="ripgrep",
        dest_dir=target_root,
        dest_name=spec.rg_name,
    )
    if not spec.is_windows:
        materialize_manifest_helper(
            spec,
            manifest_path=ZSH_MANIFEST,
            artifact_label="codex-zsh",
            dest_dir=target_root,
            dest_name="zsh",
        )
    if spec.is_linux:
        if bwrap_bin is None:
            raise RuntimeError(f"--bwrap-bin is required for {spec.target}")
        copy_materialized_helper(
            bwrap_bin,
            dest_dir=target_root,
            dest_name="bwrap",
            description="Linux bwrap executable",
            flag_name="--bwrap-bin",
        )
    if spec.is_windows:
        if codex_command_runner_bin is None:
            raise RuntimeError(
                f"--codex-command-runner-bin is required for {spec.target}"
            )
        if codex_windows_sandbox_setup_bin is None:
            raise RuntimeError(
                f"--codex-windows-sandbox-setup-bin is required for {spec.target}"
            )
        copy_materialized_helper(
            codex_command_runner_bin,
            dest_dir=target_root,
            dest_name="codex-command-runner.exe",
            description="Windows codex-command-runner.exe executable",
            flag_name="--codex-command-runner-bin",
        )
        copy_materialized_helper(
            codex_windows_sandbox_setup_bin,
            dest_dir=target_root,
            dest_name="codex-windows-sandbox-setup.exe",
            description="Windows codex-windows-sandbox-setup.exe executable",
            flag_name="--codex-windows-sandbox-setup-bin",
        )
    write_helper_manifest(spec, target_root)
    return target_root


def materialize_manifest_helper(
    spec: TargetSpec,
    *,
    manifest_path: Path,
    artifact_label: str,
    dest_dir: Path,
    dest_name: str,
) -> Path:
    artifact = artifact_for_target(
        spec,
        manifest_path,
        artifact_label=artifact_label,
    )
    archive_dir = dest_dir / "_archives"
    archive_path = archive_dir / archive_filename(artifact.url)
    download_archive(artifact.url, archive_path)
    verify_archive(archive_path, artifact, artifact_label)

    dest = dest_dir / dest_name
    extract_archive_member(archive_path, artifact, dest, artifact_label)
    mark_executable(dest)
    return dest


def copy_materialized_helper(
    source_path: Path,
    *,
    dest_dir: Path,
    dest_name: str,
    description: str,
    flag_name: str,
) -> Path:
    source = resolve_materialized_input_path(source_path, description, flag_name)
    dest = dest_dir / dest_name
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, dest)
    mark_executable(dest)
    return dest


def mark_executable(path: Path) -> None:
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


if __name__ == "__main__":
    raise SystemExit(main())
