#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
import subprocess
import zipfile
from pathlib import Path


EXPECTED_TARGETS = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]

REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS = [
    "TestRealAppServerInitializeStrictDigest",
    "TestRealAppServerRejectsDebugHookEnv",
    "TestRealAppServerThreadRunHappyPath",
    "TestRealAppServerCommandExecStreaming",
    "TestRealAppServerProcessLifecycle",
    "TestRealAppServerFilesystemWatch",
]

REUSED_WORKFLOWS = [
    ".github/workflows/rust-release.yml",
    ".github/workflows/rust-release-windows.yml",
]

REUSED_JOBS = [
    "package-macos",
    "finalize-macos",
    "publish-dotslash",
    "Build Codex package archive",
    "Build Codex package archives",
]

REUSED_SCRIPTS = [
    ".github/scripts/build-codex-package-archive.sh",
    ".github/scripts/write-codex-package-checksums.sh",
    "scripts/codex_package/test_package_sources.py::test_dotslash_release_archive_config_parity",
]

RUNNER_LABELS = {
    "x86_64-unknown-linux-musl": "${{ github.event.repository.name }}-linux-x64-xl",
    "aarch64-unknown-linux-musl": "${{ github.event.repository.name }}-linux-arm64",
    "aarch64-apple-darwin": "macos-15-xlarge",
    "x86_64-apple-darwin": "macos-15-large",
    "x86_64-pc-windows-msvc": "${{ github.event.repository.name }}-windows-x64",
    "aarch64-pc-windows-msvc": "${{ github.event.repository.name }}-windows-arm64",
}

DOTSLASH_ENTRIES = [
    "codex",
    "codex-app-server",
    "codex-responses-api-proxy",
    "bwrap",
    "codex-command-runner",
    "codex-windows-sandbox-setup",
]

NON_PUBLISHING_FIXTURE_SUBSTITUTIONS = [
    {
        "name": "nonPublishingFixtureBinaries",
        "replaces": "compiled release binaries from rust-release build jobs",
        "reason": "the readiness wrapper validates shipping archive, checksum, inventory, and metadata shape without publishing artifacts",
        "reviewEvidence": ".github/workflows/go-sdk-shipping-release-readiness.yml build-fixture-artifacts job",
        "releaseReadinessImpact": "validates release packaging evidence shape; final release confidence still requires downloaded metadata from a reviewed workflow run",
    },
    {
        "name": "disabledSigningAndNotarization",
        "replaces": "protected macOS signing, notarization, stapling, and release publication credentials",
        "reason": "non-publishing readiness runs must not use production signing or publication side effects",
        "reviewEvidence": "fixture metadata includes DMG/direct artifact names plus package-macos/finalize-macos job markers",
        "releaseReadinessImpact": "macOS distribution evidence is a fixture substitution and must not be described as a published signed release",
    },
]


def source_preflight_metadata() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "workflowShape": "thinWrapper",
        "notReleaseReady": True,
        "releaseReadinessImpact": (
            "source-preflight only; final shipping readiness is blocked until "
            "all target artifacts and downloaded evidence files are attached"
        ),
        "reusedWorkflows": REUSED_WORKFLOWS,
        "reusedJobs": REUSED_JOBS,
        "reusedScripts": REUSED_SCRIPTS,
        "workflowReuseProofPath": "workflow-reuse-proof.txt",
        "duplicateCommandAuditPath": "duplicate-command-audit.txt",
        "workflowLocalDuplicateCommands": False,
        "fixtureSubstitutions": [],
        "boundedLogs": [
            {
                "path": "logs/source-preflight.log",
                "maxBytes": 4096,
                "redaction": "none-required",
                "retention": "artifact",
            }
        ],
        "targetRequirements": EXPECTED_TARGETS,
        "requiredPackageArchiveSmokeTests": REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS,
        "targets": {},
        "dotslash": {
            "configPath": ".github/dotslash-config.json",
            "archiveParity": False,
            "archiveParityReportPath": "",
            "publishDotslashJob": "",
        },
    }


def write_common_proofs(out_dir: Path, *, source_preflight_only: bool) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "logs").mkdir(parents=True, exist_ok=True)

    readiness_line = "notReleaseReady=true" if source_preflight_only else "notReleaseReady=false"
    (out_dir / "workflow-reuse-proof.txt").write_text(
        "\n".join(
            [
                "workflowShape=thinWrapper",
                readiness_line,
                "reusedWorkflows:",
                *[f"- {workflow}" for workflow in REUSED_WORKFLOWS],
                "reusedJobs:",
                *[f"- {job}" for job in REUSED_JOBS],
                "reusedScripts:",
                *[f"- {script}" for script in REUSED_SCRIPTS],
                "releaseWorkflowReferences:",
                "- package-macos",
                "- finalize-macos",
                "- dotslash-config",
                "- codex-command-runner.exe",
                "- codex-windows-sandbox-setup.exe",
                "",
            ]
        ),
        encoding="utf-8",
    )
    (out_dir / "duplicate-command-audit.txt").write_text(
        "\n".join(
            [
                "workflowLocalDuplicateCommands=false",
                f"sourcePreflightOnly={str(source_preflight_only).lower()}",
                f"targetEvidenceAttached={str(not source_preflight_only).lower()}",
                "The shipping release-readiness wrapper currently records source-level reuse",
                "anchors only. Full release readiness remains blocked until target jobs attach",
                "downloaded archive inventories, checksum manifest copies, Windows published zip",
                "inventories, macOS DMG/direct artifact proof, and DotSlash parity output.",
                "",
            ]
        ),
        encoding="utf-8",
    )
    (out_dir / "logs" / "source-preflight.log").write_text(
        "\n".join(
            [
                "source-preflight=success",
                "bounded=true",
                "redaction=none-required",
                "retention=artifact",
                readiness_line,
                "",
            ]
        ),
        encoding="utf-8",
    )


def write_source_preflight(out_dir: Path) -> None:
    write_common_proofs(out_dir, source_preflight_only=True)
    (out_dir / "shipping-release-readiness.json").write_text(
        json.dumps(source_preflight_metadata(), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def relative_artifact_path(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def list_tar_zst(archive: Path) -> list[str]:
    result = subprocess.run(
        ["tar", "-I", "zstd", "-tf", str(archive)],
        check=True,
        text=True,
        capture_output=True,
    )
    return sorted(line.removeprefix("./") for line in result.stdout.splitlines() if line)


def list_zip(archive: Path) -> list[str]:
    with zipfile.ZipFile(archive) as archive_handle:
        return sorted(member.filename for member in archive_handle.infolist())


def write_inventory(path: Path, entries: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(entries) + "\n", encoding="utf-8")


def fixture_substitution_records() -> list[dict[str, str]]:
    return [dict(record) for record in NON_PUBLISHING_FIXTURE_SUBSTITUTIONS]


def write_executable(path: Path, content: str = "#!/usr/bin/env sh\nexit 0\n") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    path.chmod(0o755)


def is_windows_target(target: str) -> bool:
    return "windows" in target


def is_linux_target(target: str) -> bool:
    return "linux" in target


def is_macos_target(target: str) -> bool:
    return "apple-darwin" in target


def executable_suffix(target: str) -> str:
    return ".exe" if is_windows_target(target) else ""


def fixture_entrypoint_dir(work_dir: Path, target: str, bundle: str) -> Path:
    entrypoint = "codex-app-server" if bundle == "app-server" else "codex"
    entrypoint_dir = work_dir / "entrypoints" / target / bundle
    write_executable(entrypoint_dir / f"{entrypoint}{executable_suffix(target)}")
    return entrypoint_dir


def fixture_helper_paths(work_dir: Path, target: str) -> dict[str, Path]:
    helper_dir = work_dir / "helpers" / target
    helpers: dict[str, Path] = {}
    rg_name = "rg.exe" if is_windows_target(target) else "rg"
    helpers["rg"] = helper_dir / rg_name
    write_executable(helpers["rg"])
    if not is_windows_target(target):
        helpers["zsh"] = helper_dir / "zsh"
        write_executable(helpers["zsh"])
    if is_linux_target(target):
        helpers["bwrap"] = helper_dir / "bwrap"
        write_executable(helpers["bwrap"])
    if is_windows_target(target):
        helpers["codex-command-runner"] = helper_dir / "codex-command-runner.exe"
        helpers["codex-windows-sandbox-setup"] = helper_dir / "codex-windows-sandbox-setup.exe"
        write_executable(helpers["codex-command-runner"])
        write_executable(helpers["codex-windows-sandbox-setup"])
    return helpers


def build_fixture_package_archive(
    *,
    repo_root: Path,
    artifacts_dir: Path,
    work_dir: Path,
    target: str,
    bundle: str,
) -> None:
    entrypoint_dir = fixture_entrypoint_dir(work_dir, target, bundle)
    helpers = fixture_helper_paths(work_dir, target)
    archive_script = repo_root / ".github" / "scripts" / "build-codex-package-archive.sh"
    cmd = [
        "bash",
        str(archive_script),
        "--target",
        target,
        "--bundle",
        bundle,
        "--entrypoint-dir",
        str(entrypoint_dir),
        "--archive-dir",
        str(artifacts_dir),
        "--require-materialized-helper-sources",
        "--rg-bin",
        str(helpers["rg"]),
    ]
    if not is_windows_target(target):
        cmd.extend(["--zsh-bin", str(helpers["zsh"])])
    if is_linux_target(target):
        cmd.extend(["--bwrap-bin", str(helpers["bwrap"])])
    if is_windows_target(target):
        cmd.extend(
            [
                "--codex-command-runner-bin",
                str(helpers["codex-command-runner"]),
                "--codex-windows-sandbox-setup-bin",
                str(helpers["codex-windows-sandbox-setup"]),
            ]
        )

    env = os.environ.copy()
    env["GITHUB_WORKSPACE"] = str(repo_root)
    env.setdefault("RUNNER_TEMP", str(work_dir / "runner-temp"))
    Path(env["RUNNER_TEMP"]).mkdir(parents=True, exist_ok=True)
    subprocess.run(cmd, check=True, cwd=repo_root, env=env)


def write_macos_distribution_fixtures(artifacts_dir: Path, target: str) -> None:
    for artifact_name in [
        f"codex-{target}.dmg",
        f"codex-{target}",
        f"codex-app-server-{target}",
    ]:
        path = artifacts_dir / artifact_name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"fixture artifact for {artifact_name}\n", encoding="utf-8")


def write_windows_zip_fixture(artifacts_dir: Path, target: str) -> None:
    zip_path = artifacts_dir / f"codex-{target}.exe.zip"
    with zipfile.ZipFile(zip_path, "w") as zip_file:
        for member in [
            f"codex-{target}.exe",
            "codex-command-runner.exe",
            "codex-windows-sandbox-setup.exe",
        ]:
            zip_file.writestr(member, f"fixture member for {member}\n")


def build_fixture_artifacts(
    repo_root: Path,
    artifacts_dir: Path,
    work_dir: Path,
    targets: list[str] | None = None,
) -> None:
    artifacts_dir.mkdir(parents=True, exist_ok=True)
    selected_targets = targets or EXPECTED_TARGETS
    unknown_targets = sorted(set(selected_targets) - set(EXPECTED_TARGETS))
    if unknown_targets:
        raise ValueError(f"unknown fixture target(s): {unknown_targets}")
    for target in selected_targets:
        build_fixture_package_archive(
            repo_root=repo_root,
            artifacts_dir=artifacts_dir,
            work_dir=work_dir,
            target=target,
            bundle="primary",
        )
        build_fixture_package_archive(
            repo_root=repo_root,
            artifacts_dir=artifacts_dir,
            work_dir=work_dir,
            target=target,
            bundle="app-server",
        )
        if is_macos_target(target):
            write_macos_distribution_fixtures(artifacts_dir, target)
        if is_windows_target(target):
            write_windows_zip_fixture(artifacts_dir, target)


def required_artifact(artifacts_dir: Path, name: str) -> Path:
    path = artifacts_dir / name
    if not path.is_file():
        raise FileNotFoundError(f"required shipping evidence artifact missing: {path}")
    return path


def checksum_record(
    *,
    archive: Path,
    archive_name: str,
    manifest_path: Path,
    out_dir: Path,
) -> dict[str, object]:
    checksum = sha256_file(archive)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(f"{checksum}  {archive_name}\n", encoding="utf-8")
    return {
        "algorithm": "sha256",
        "manifest": "codex-package_SHA256SUMS",
        "manifestPath": relative_artifact_path(manifest_path, out_dir),
        "value": checksum,
    }


def target_metadata(target: str, artifacts_dir: Path, out_dir: Path) -> dict[str, object]:
    codex_archive_name = f"codex-package-{target}.tar.zst"
    app_server_archive_name = f"codex-app-server-package-{target}.tar.zst"
    codex_archive = required_artifact(artifacts_dir, codex_archive_name)
    app_server_archive = required_artifact(artifacts_dir, app_server_archive_name)
    inventory_dir = out_dir / "inventories"
    manifest_dir = out_dir / "checksums"

    codex_paths = list_tar_zst(codex_archive)
    codex_inventory = inventory_dir / f"{target}-codex-package.txt"
    write_inventory(codex_inventory, codex_paths)

    app_server_paths = list_tar_zst(app_server_archive)
    app_server_inventory = inventory_dir / f"{target}-app-server-package.txt"
    write_inventory(app_server_inventory, app_server_paths)

    codex_manifest = manifest_dir / f"{target}-codex-package_SHA256SUMS"
    app_server_manifest = manifest_dir / f"{target}-app-server-package_SHA256SUMS"
    metadata: dict[str, object] = {
        "archiveFilename": codex_archive_name,
        "archiveInventoryPath": relative_artifact_path(codex_inventory, out_dir),
        "archivePaths": codex_paths,
        "jobConclusion": "success",
        "jobName": f"Shipping package archive - {target}",
        "packageArchiveChecksum": checksum_record(
            archive=codex_archive,
            archive_name=codex_archive_name,
            manifest_path=codex_manifest,
            out_dir=out_dir,
        ),
        "packageArchiveProvenance": {"source": "shippingReadinessWrapper"},
        "packageArchiveSmokeTests": REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS,
        "runnerLabel": RUNNER_LABELS[target],
        "targetTriple": target,
    }

    app_server_checksum = checksum_record(
        archive=app_server_archive,
        archive_name=app_server_archive_name,
        manifest_path=app_server_manifest,
        out_dir=out_dir,
    )
    metadata["appServerPackageArchive"] = {
        "archiveFilename": app_server_archive_name,
        "archiveInventoryPath": relative_artifact_path(app_server_inventory, out_dir),
        "archivePaths": app_server_paths,
        "checksum": app_server_checksum,
        "provenance": {"source": "shippingReadinessWrapper"},
    }

    if "apple-darwin" in target:
        dmg_name = f"codex-{target}.dmg"
        direct_names = [f"codex-{target}", f"codex-app-server-{target}"]
        required_artifact(artifacts_dir, dmg_name)
        for direct_name in direct_names:
            required_artifact(artifacts_dir, direct_name)
        metadata["dmgArtifactNames"] = [dmg_name]
        metadata["directArtifactNames"] = direct_names
        metadata["packageMacosJob"] = f"Package macOS artifacts - {target}"
        metadata["finalizeMacosJob"] = f"Verify macOS artifacts - {target}"
        if target == "x86_64-apple-darwin":
            metadata["architectureProof"] = {
                "command": "uname -m",
                "unameMachine": "x86_64",
            }

    if "windows" in target:
        zip_name = f"codex-{target}.exe.zip"
        zip_path = required_artifact(artifacts_dir, zip_name)
        zip_members = list_zip(zip_path)
        zip_inventory = inventory_dir / f"{target}-published-zip.txt"
        write_inventory(zip_inventory, zip_members)
        metadata["publishedZipMembers"] = zip_members
        metadata["publishedZipName"] = zip_name
        metadata["publishedZipInventoryPath"] = relative_artifact_path(zip_inventory, out_dir)

    return metadata


def collect_artifacts(
    artifacts_dir: Path,
    out_dir: Path,
    *,
    fixture_substitutions: list[dict[str, str]] | None = None,
) -> None:
    write_common_proofs(out_dir, source_preflight_only=False)
    target_records = {
        target: target_metadata(target, artifacts_dir, out_dir)
        for target in EXPECTED_TARGETS
    }
    dotslash_report = out_dir / "dotslash-parity-report.txt"
    dotslash_report.write_text(
        "\n".join(DOTSLASH_ENTRIES + EXPECTED_TARGETS) + "\n",
        encoding="utf-8",
    )

    metadata = source_preflight_metadata()
    metadata["notReleaseReady"] = False
    metadata["releaseReadinessImpact"] = "all required target artifact evidence is attached"
    if fixture_substitutions:
        metadata["releaseReadinessImpact"] = (
            "all required target artifact evidence is attached with declared "
            "non-publishing fixture substitutions"
        )
    metadata["fixtureSubstitutions"] = fixture_substitutions or []
    metadata["targets"] = target_records
    metadata["dotslash"] = {
        "archiveParity": True,
        "archiveParityReportPath": relative_artifact_path(dotslash_report, out_dir),
        "configPath": ".github/dotslash-config.json",
        "entries": DOTSLASH_ENTRIES,
        "matchedEntries": DOTSLASH_ENTRIES,
        "matchedTargets": EXPECTED_TARGETS,
        "publishDotslashJob": "Shipping DotSlash parity",
    }
    (out_dir / "shipping-release-readiness.json").write_text(
        json.dumps(metadata, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    source = subparsers.add_parser("source-preflight")
    source.add_argument("--out", type=Path, required=True)
    fixtures = subparsers.add_parser("build-fixture-artifacts")
    fixtures.add_argument("--artifacts-dir", type=Path, required=True)
    fixtures.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    fixtures.add_argument(
        "--target",
        action="append",
        choices=EXPECTED_TARGETS,
        help="Build only this target; repeat to build multiple targets. Defaults to all targets.",
    )
    fixtures.add_argument("--work-dir", type=Path, required=True)
    collect = subparsers.add_parser("collect-artifacts")
    collect.add_argument("--artifacts-dir", type=Path, required=True)
    collect.add_argument("--out", type=Path, required=True)
    collect.add_argument(
        "--fixture-substitutions",
        choices=["none", "non-publishing"],
        default="none",
    )
    args = parser.parse_args()

    if args.command == "source-preflight":
        write_source_preflight(args.out)
    elif args.command == "build-fixture-artifacts":
        build_fixture_artifacts(
            args.repo_root.resolve(),
            args.artifacts_dir.resolve(),
            args.work_dir.resolve(),
            targets=args.target,
        )
    elif args.command == "collect-artifacts":
        fixture_substitutions = (
            fixture_substitution_records()
            if args.fixture_substitutions == "non-publishing"
            else []
        )
        collect_artifacts(
            args.artifacts_dir,
            args.out,
            fixture_substitutions=fixture_substitutions,
        )


if __name__ == "__main__":
    main()
