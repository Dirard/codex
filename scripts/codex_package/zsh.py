"""Fetch the patched zsh fork used by shell_zsh_fork."""

from pathlib import Path

from .dotslash import fetch_dotslash_executable
from .targets import REPO_ROOT
from .targets import TargetSpec
from .targets import resolve_input_path
from .targets import resolve_materialized_input_path


ZSH_MANIFEST = REPO_ROOT / "scripts" / "codex_package" / "codex-zsh"
ZSH_RESOURCE_PATH = Path("zsh") / "bin" / "zsh"


def resolve_zsh_bin(
    spec: TargetSpec,
    manifest_path: Path | None = None,
    zsh_bin: Path | None = None,
    *,
    require_materialized: bool = False,
) -> Path | None:
    if zsh_bin is not None:
        if require_materialized:
            return resolve_materialized_input_path(
                zsh_bin,
                "patched zsh executable",
                "--zsh-bin",
            )
        return resolve_input_path(zsh_bin, "patched zsh executable", "--zsh-bin")

    if require_materialized:
        if spec.is_windows:
            return None
        raise RuntimeError(
            "Stage 5G package source hermeticity requires --zsh-bin for "
            f"{spec.target}; DotSlash, package-cache, PATH, and network fallback "
            "are not allowed during package assembly."
        )

    return fetch_dotslash_executable(
        spec,
        manifest_path=manifest_path or ZSH_MANIFEST,
        artifact_label="codex-zsh",
        cache_key=f"{spec.target}-zsh",
        dest_name="zsh",
        missing_ok=True,
    )
