"""Manifest and verification helpers for materialized package payloads."""

import hashlib
import json
from pathlib import Path
from pathlib import PurePosixPath

from .targets import TargetSpec
from .targets import is_executable


HELPER_MANIFEST_NAME = "codex-package-helpers.json"
HELPER_MANIFEST_SCHEMA_VERSION = 1
HELPER_MANIFEST_GENERATOR = "codex_package.materialize_helpers"


def expected_helper_payloads(spec: TargetSpec) -> dict[str, str]:
    helpers = {"rg": spec.rg_name}
    if not spec.is_windows:
        helpers["zsh"] = "zsh"
    if spec.is_linux:
        helpers["bwrap"] = "bwrap"
    if spec.is_windows:
        helpers["codex-command-runner"] = "codex-command-runner.exe"
        helpers["codex-windows-sandbox-setup"] = "codex-windows-sandbox-setup.exe"
    return helpers


def write_helper_manifest(spec: TargetSpec, target_root: Path) -> Path:
    manifest = {
        "schemaVersion": HELPER_MANIFEST_SCHEMA_VERSION,
        "generatedBy": HELPER_MANIFEST_GENERATOR,
        "target": spec.target,
        "helpers": {
            name: helper_manifest_entry(target_root, relative_path)
            for name, relative_path in expected_helper_payloads(spec).items()
        },
    }
    manifest_path = target_root / HELPER_MANIFEST_NAME
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest_path


def verify_helper_manifest(spec: TargetSpec, target_root: Path) -> dict:
    manifest_path = target_root / HELPER_MANIFEST_NAME
    if not manifest_path.is_file():
        raise RuntimeError(f"Missing package helper manifest: {manifest_path}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    expect_field(
        manifest,
        "schemaVersion",
        HELPER_MANIFEST_SCHEMA_VERSION,
        manifest_path,
    )
    expect_field(manifest, "generatedBy", HELPER_MANIFEST_GENERATOR, manifest_path)
    expect_field(manifest, "target", spec.target, manifest_path)

    helpers = manifest.get("helpers")
    if not isinstance(helpers, dict):
        raise RuntimeError(f"{manifest_path} must contain a helpers object")

    expected = expected_helper_payloads(spec)
    if set(helpers) != set(expected):
        raise RuntimeError(
            f"{manifest_path} helper set {sorted(helpers)} does not match "
            f"expected {sorted(expected)}"
        )

    for helper_name, relative_path in expected.items():
        verify_helper_entry(
            target_root,
            manifest_path,
            helper_name,
            helpers[helper_name],
            relative_path,
        )
    return manifest


def helper_manifest_entry(target_root: Path, relative_path: str) -> dict:
    path = target_root / relative_path
    if not path.is_file():
        raise RuntimeError(f"Materialized helper is missing: {path}")
    if not is_executable(path):
        raise RuntimeError(f"Materialized helper is not executable: {path}")
    return {
        "relativePath": relative_path,
        "sha256": sha256_file(path),
        "sizeBytes": path.stat().st_size,
    }


def verify_helper_entry(
    target_root: Path,
    manifest_path: Path,
    helper_name: str,
    entry: object,
    expected_relative_path: str,
) -> None:
    if not isinstance(entry, dict):
        raise RuntimeError(f"{manifest_path} helper {helper_name!r} must be an object")

    relative_path = str(entry.get("relativePath", ""))
    if relative_path != expected_relative_path:
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} path {relative_path!r} "
            f"does not match expected {expected_relative_path!r}"
        )

    helper_path = resolve_manifest_relative_path(
        target_root,
        manifest_path,
        helper_name,
        relative_path,
    )
    if not helper_path.is_file():
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} is missing: {helper_path}"
        )
    if not is_executable(helper_path):
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} is not executable: {helper_path}"
        )

    actual_size = helper_path.stat().st_size
    if entry.get("sizeBytes") != actual_size:
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} has size {actual_size}, "
            f"expected {entry.get('sizeBytes')}"
        )

    actual_sha256 = sha256_file(helper_path)
    if entry.get("sha256") != actual_sha256:
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} has sha256 "
            f"{actual_sha256}, expected {entry.get('sha256')}"
        )


def resolve_manifest_relative_path(
    target_root: Path,
    manifest_path: Path,
    helper_name: str,
    relative_path: str,
) -> Path:
    if not relative_path:
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} has an empty relative path"
        )
    if "\\" in relative_path:
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} must use POSIX paths: "
            f"{relative_path!r}"
        )
    parsed = PurePosixPath(relative_path)
    if parsed.is_absolute() or ".." in parsed.parts:
        raise RuntimeError(
            f"{manifest_path} helper {helper_name!r} must stay under "
            f"the target helper root: {relative_path!r}"
        )
    return target_root.joinpath(*parsed.parts)


def expect_field(
    manifest: dict,
    field_name: str,
    expected_value: object,
    manifest_path: Path,
) -> None:
    actual_value = manifest.get(field_name)
    if actual_value != expected_value:
        raise RuntimeError(
            f"{manifest_path} field {field_name!r} is {actual_value!r}, "
            f"expected {expected_value!r}"
        )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()
