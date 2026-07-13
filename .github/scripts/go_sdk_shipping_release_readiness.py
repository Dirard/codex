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

LINUX_TARGETS = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
]

RUST_RELEASE_WORKFLOW = ".github/workflows/rust-release.yml"

REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS = [
    "TestRealAppServerInitializeStrictDigest",
    "TestRealAppServerRejectsDebugHookEnv",
    "TestRealAppServerThreadRunHappyPath",
    "TestRealAppServerCommandExecStreaming",
    "TestRealAppServerProcessLifecycle",
    "TestRealAppServerFilesystemWatch",
]

REQUIRED_LINUX_SANDBOX_SMOKE = {
    "verifier": ".github/scripts/stage-codex-runtime.sh",
    "arguments": ["--verify-sandbox", "--exec-path"],
    "bwrapPath": "codex-resources/bwrap",
    "result": "success",
}

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
        "artifactEvidenceShapeComplete": False,
        "evidenceKind": "sourcePreflightOnly",
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


def write_common_proofs(
    out_dir: Path,
    *,
    source_preflight_only: bool,
    not_release_ready: bool | None = None,
    evidence_kind: str | None = None,
) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "logs").mkdir(parents=True, exist_ok=True)

    if not_release_ready is None:
        not_release_ready = source_preflight_only
    readiness_line = f"notReleaseReady={str(not_release_ready).lower()}"
    evidence_kind_line = f"evidenceKind={evidence_kind}" if evidence_kind else None
    (out_dir / "workflow-reuse-proof.txt").write_text(
        "\n".join(
            [
                "workflowShape=thinWrapper",
                readiness_line,
                *([evidence_kind_line] if evidence_kind_line else []),
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
                *([evidence_kind_line] if evidence_kind_line else []),
                "",
            ]
        ),
        encoding="utf-8",
    )


def write_source_preflight(out_dir: Path) -> None:
    write_common_proofs(
        out_dir,
        source_preflight_only=True,
        not_release_ready=True,
        evidence_kind="sourcePreflightOnly",
    )
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
    write_executable(
        entrypoint_dir / f"codex-code-mode-host{executable_suffix(target)}"
    )
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
            "codex-code-mode-host.exe",
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


def unique_release_artifact(artifacts_dir: Path, name: str) -> Path:
    matches = sorted(path for path in artifacts_dir.rglob(name) if path.is_file())
    if len(matches) != 1:
        raise FileNotFoundError(
            f"expected exactly one release artifact named {name}, found {len(matches)}"
        )
    return matches[0]


def read_checksum_manifest(path: Path) -> dict[str, str]:
    entries: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        fields = line.split(maxsplit=1)
        if len(fields) != 2:
            raise RuntimeError(f"invalid checksum manifest line: {line!r}")
        digest, filename = fields
        filename = filename.lstrip("* ")
        if filename in entries:
            raise RuntimeError(f"duplicate checksum manifest entry for {filename}")
        entries[filename] = digest
    return entries


def release_archive_record(
    *,
    artifacts_dir: Path,
    out_dir: Path,
    checksum_entries: dict[str, str],
    target: str,
    artifact_name: str,
    archive_name: str,
    required_paths: list[str],
    inventory_name: str,
) -> dict[str, object]:
    archive = unique_release_artifact(artifacts_dir, archive_name)
    archive_sha256 = sha256_file(archive)
    if checksum_entries.get(archive_name) != archive_sha256:
        raise RuntimeError(f"release checksum manifest mismatch for {archive_name}")
    archive_paths = list_tar_zst(archive)
    require_members(archive_paths, required_paths, archive_name)
    inventory_path = out_dir / "inventories" / inventory_name
    write_inventory(inventory_path, archive_paths)
    return {
        "artifactName": artifact_name,
        "archiveFilename": archive_name,
        "sha256": archive_sha256,
        "checksumManifestPath": "checksums/codex-package_SHA256SUMS",
        "inventoryPath": relative_artifact_path(inventory_path, out_dir),
        "inventory": archive_paths,
        "target": target,
    }


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


def target_exe_suffix(target: str) -> str:
    return ".exe" if is_windows_target(target) else ""


def required_codex_package_paths(target: str) -> list[str]:
    exe = target_exe_suffix(target)
    paths = [
        "codex-package.json",
        f"bin/codex{exe}",
        f"bin/codex-code-mode-host{exe}",
        f"codex-path/rg{exe}",
    ]
    if is_windows_target(target):
        paths.extend(
            [
                "codex-resources/codex-command-runner.exe",
                "codex-resources/codex-windows-sandbox-setup.exe",
            ]
        )
    else:
        paths.append("codex-resources/zsh/bin/zsh")
        if is_linux_target(target):
            paths.append("codex-resources/bwrap")
    return paths


def required_app_server_package_paths(target: str) -> list[str]:
    exe = target_exe_suffix(target)
    return [
        f"bin/codex-app-server{exe}",
        f"bin/codex-code-mode-host{exe}",
    ]


def required_windows_zip_members(target: str) -> list[str]:
    return [
        f"codex-{target}.exe",
        "codex-code-mode-host.exe",
        "codex-command-runner.exe",
        "codex-windows-sandbox-setup.exe",
    ]


def require_members(paths: list[str], required_paths: list[str], artifact_name: str) -> None:
    missing = sorted(set(required_paths) - set(paths))
    if missing:
        raise RuntimeError(
            f"{artifact_name} is missing required runtime members: "
            + ", ".join(missing)
        )


def target_metadata(target: str, artifacts_dir: Path, out_dir: Path) -> dict[str, object]:
    codex_archive_name = f"codex-package-{target}.tar.zst"
    app_server_archive_name = f"codex-app-server-package-{target}.tar.zst"
    codex_archive = required_artifact(artifacts_dir, codex_archive_name)
    app_server_archive = required_artifact(artifacts_dir, app_server_archive_name)
    inventory_dir = out_dir / "inventories"
    manifest_dir = out_dir / "checksums"

    codex_paths = list_tar_zst(codex_archive)
    require_members(codex_paths, required_codex_package_paths(target), codex_archive_name)
    codex_inventory = inventory_dir / f"{target}-codex-package.txt"
    write_inventory(codex_inventory, codex_paths)

    app_server_paths = list_tar_zst(app_server_archive)
    require_members(
        app_server_paths,
        required_app_server_package_paths(target),
        app_server_archive_name,
    )
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
        require_members(zip_members, required_windows_zip_members(target), zip_name)
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
    if not fixture_substitutions:
        raise SystemExit(
            "collect-artifacts releaseArtifactEvidence mode requires explicit "
            "downloaded real release workflow evidence; the current wrapper only "
            "supports --fixture-substitutions non-publishing"
        )
    fixture_mode = bool(fixture_substitutions)
    evidence_kind = (
        "nonPublishingFixtureEvidence" if fixture_mode else "releaseArtifactEvidence"
    )
    write_common_proofs(
        out_dir,
        source_preflight_only=False,
        not_release_ready=fixture_mode,
        evidence_kind=evidence_kind,
    )
    target_records = {
        target: target_metadata(target, artifacts_dir, out_dir)
        for target in EXPECTED_TARGETS
    }
    if fixture_mode:
        for target_record in target_records.values():
            target_record["fixtureOnly"] = True
            target_record["packageArchiveSmokeTestsRan"] = False
            target_record["expectedPackageArchiveSmokeTests"] = target_record.pop(
                "packageArchiveSmokeTests"
            )
    dotslash_report = out_dir / "dotslash-parity-report.txt"
    dotslash_report.write_text(
        "\n".join(DOTSLASH_ENTRIES + EXPECTED_TARGETS) + "\n",
        encoding="utf-8",
    )

    metadata = source_preflight_metadata()
    metadata["artifactEvidenceShapeComplete"] = True
    metadata["evidenceKind"] = evidence_kind
    metadata["notReleaseReady"] = fixture_mode
    if fixture_mode:
        metadata["releaseReadinessImpact"] = (
            "artifact evidence shape is complete with declared non-publishing "
            "fixture substitutions; final release readiness is blocked until "
            "real non-fixture shipping evidence is attached"
        )
    else:
        metadata["releaseReadinessImpact"] = (
            "all required target artifact evidence is attached"
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


def collect_linux_release_artifacts(
    artifacts_dir: Path,
    out_dir: Path,
    *,
    run_id: str,
    run_attempt: str,
    commit_sha: str,
    release_tag: str,
    verification_job: str,
    verification_conclusion: str,
    verification_commit_sha: str,
) -> None:
    if not run_id.isdigit() or not run_attempt.isdigit():
        raise RuntimeError("release workflow run id and attempt must be numeric")
    if len(commit_sha) != 40 or any(char not in "0123456789abcdef" for char in commit_sha):
        raise RuntimeError("release commit sha must be a lowercase 40-character hex digest")
    if not release_tag.startswith("rust-v"):
        raise RuntimeError("release tag must use the rust-v prefix")
    if verification_job != "go-sdk-linux-verification":
        raise RuntimeError("Linux release evidence has unexpected Go SDK verification job")
    if verification_conclusion != "success":
        raise RuntimeError("Linux release evidence requires successful Go SDK verification")
    if verification_commit_sha != commit_sha:
        raise RuntimeError("Go SDK verification commit does not match release commit")

    checksum_source = unique_release_artifact(
        artifacts_dir, "codex-package_SHA256SUMS"
    )
    checksum_entries = read_checksum_manifest(checksum_source)
    checksum_output = out_dir / "checksums" / "codex-package_SHA256SUMS"
    checksum_output.parent.mkdir(parents=True, exist_ok=True)
    checksum_output.write_bytes(checksum_source.read_bytes())

    targets: dict[str, object] = {}
    expected_architectures = {
        "x86_64-unknown-linux-musl": {"x86_64"},
        "aarch64-unknown-linux-musl": {"aarch64", "arm64"},
    }
    for target in LINUX_TARGETS:
        package_archive = release_archive_record(
            artifacts_dir=artifacts_dir,
            out_dir=out_dir,
            checksum_entries=checksum_entries,
            target=target,
            artifact_name=target,
            archive_name=f"codex-package-{target}.tar.zst",
            required_paths=required_codex_package_paths(target),
            inventory_name=f"{target}-codex-package.txt",
        )
        app_server_archive = release_archive_record(
            artifacts_dir=artifacts_dir,
            out_dir=out_dir,
            checksum_entries=checksum_entries,
            target=target,
            artifact_name=f"{target}-app-server",
            archive_name=f"codex-app-server-package-{target}.tar.zst",
            required_paths=required_app_server_package_paths(target),
            inventory_name=f"{target}-app-server-package.txt",
        )
        smoke_path = unique_release_artifact(
            artifacts_dir, f"go-sdk-release-smoke-{target}.json"
        )
        smoke = json.loads(smoke_path.read_text(encoding="utf-8"))
        if smoke.get("sourceWorkflow") != RUST_RELEASE_WORKFLOW:
            raise RuntimeError(f"smoke evidence source workflow mismatch for {target}")
        if smoke.get("workflowRunId") != run_id:
            raise RuntimeError(f"smoke evidence workflow run mismatch for {target}")
        if smoke.get("workflowRunAttempt") != run_attempt:
            raise RuntimeError(f"smoke evidence workflow run attempt mismatch for {target}")
        if smoke.get("commitSha") != commit_sha:
            raise RuntimeError(f"smoke evidence commit mismatch for {target}")
        if smoke.get("target") != target or smoke.get("jobConclusion") != "success":
            raise RuntimeError(f"smoke evidence target or conclusion mismatch for {target}")
        if smoke.get("unameMachine") not in expected_architectures[target]:
            raise RuntimeError(f"smoke evidence architecture mismatch for {target}")
        if smoke.get("packageArchiveFilename") != package_archive["archiveFilename"]:
            raise RuntimeError(f"smoke evidence archive filename mismatch for {target}")
        if smoke.get("packageArchiveSha256") != package_archive["sha256"]:
            raise RuntimeError(f"smoke evidence archive digest mismatch for {target}")
        if smoke.get("sandboxSmoke") != REQUIRED_LINUX_SANDBOX_SMOKE:
            raise RuntimeError(f"sandbox smoke evidence mismatch for {target}")
        missing_smoke_tests = sorted(
            set(REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS)
            - set(smoke.get("smokeTests") or [])
        )
        if missing_smoke_tests:
            raise RuntimeError(
                f"smoke evidence missing tests {missing_smoke_tests} for {target}"
            )
        targets[target] = {
            "packageArchive": package_archive,
            "appServerPackageArchive": app_server_archive,
            "smokeEvidence": smoke,
        }

    out_dir.mkdir(parents=True, exist_ok=True)
    metadata = {
        "schemaVersion": 1,
        "workflowShape": "actualRustReleaseArtifactGate",
        "workflowLocalDuplicateCommands": False,
        "notReleaseReady": False,
        "linuxReleaseReady": True,
        "evidenceKind": "rustReleaseArtifactEvidence",
        "source": {
            "workflow": RUST_RELEASE_WORKFLOW,
            "workflowRunId": run_id,
            "workflowRunAttempt": run_attempt,
            "commitSha": commit_sha,
            "releaseTag": release_tag,
            "artifactScope": "sameWorkflowRun",
        },
        "goSdkVerification": {
            "workflow": RUST_RELEASE_WORKFLOW,
            "workflowRunId": run_id,
            "workflowRunAttempt": run_attempt,
            "job": verification_job,
            "conclusion": verification_conclusion,
            "commitSha": verification_commit_sha,
        },
        "reusedScripts": [
            ".github/scripts/build-codex-package-archive.sh",
            ".github/scripts/write-codex-package-checksums.sh",
            ".github/scripts/go_sdk_shipping_release_readiness.py",
            ".github/scripts/stage-codex-runtime.sh",
        ],
        "targets": targets,
    }
    (out_dir / "shipping-release-readiness.json").write_text(
        json.dumps(metadata, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def validate_linux_release_evidence(
    metadata_path: Path,
    public_checksum_manifest: Path,
    *,
    run_id: str,
    run_attempt: str,
    commit_sha: str,
    release_tag: str,
    require_publication_finalization: bool,
) -> dict[str, object]:
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    expected_kind = (
        "rustReleasePublishedArtifactEvidence"
        if require_publication_finalization
        else "rustReleaseArtifactEvidence"
    )
    if metadata.get("evidenceKind") != expected_kind:
        raise RuntimeError(f"Linux release evidence kind must be {expected_kind}")
    if metadata.get("notReleaseReady") is not False:
        raise RuntimeError("Linux release evidence must not be marked notReleaseReady")
    if metadata.get("linuxReleaseReady") is not True:
        raise RuntimeError("Linux release evidence must set linuxReleaseReady")

    source = metadata.get("source") or {}
    expected_source = {
        "workflow": RUST_RELEASE_WORKFLOW,
        "workflowRunId": run_id,
        "workflowRunAttempt": run_attempt,
        "commitSha": commit_sha,
        "releaseTag": release_tag,
        "artifactScope": "sameWorkflowRun",
    }
    if source != expected_source:
        raise RuntimeError("Linux release evidence source provenance mismatch")

    expected_verification = {
        "workflow": RUST_RELEASE_WORKFLOW,
        "workflowRunId": run_id,
        "workflowRunAttempt": run_attempt,
        "job": "go-sdk-linux-verification",
        "conclusion": "success",
        "commitSha": commit_sha,
    }
    if metadata.get("goSdkVerification") != expected_verification:
        raise RuntimeError("Linux release Go SDK verification provenance mismatch")

    public_entries = read_checksum_manifest(public_checksum_manifest)
    targets = metadata.get("targets") or {}
    if set(targets) != set(LINUX_TARGETS):
        raise RuntimeError("Linux release evidence target set mismatch")
    for target in LINUX_TARGETS:
        target_metadata = targets[target]
        archive_specs = [
            (
                "packageArchive",
                target,
                f"codex-package-{target}.tar.zst",
                required_codex_package_paths(target),
            ),
            (
                "appServerPackageArchive",
                f"{target}-app-server",
                f"codex-app-server-package-{target}.tar.zst",
                required_app_server_package_paths(target),
            ),
        ]
        for field, artifact_name, archive_name, required_paths in archive_specs:
            archive = target_metadata.get(field) or {}
            if archive.get("artifactName") != artifact_name:
                raise RuntimeError(f"{target} {field} artifact provenance mismatch")
            if archive.get("archiveFilename") != archive_name:
                raise RuntimeError(f"{target} {field} filename mismatch")
            digest = archive.get("sha256")
            if not digest or public_entries.get(archive_name) != digest:
                raise RuntimeError(
                    f"public checksum manifest mismatch for {archive_name}"
                )
            if (
                require_publication_finalization
                and archive.get("checksumManifestPath")
                != public_checksum_manifest.name
            ):
                raise RuntimeError(
                    f"{target} {field} does not reference the published checksum manifest"
                )
            require_members(
                list(archive.get("inventory") or []), required_paths, archive_name
            )

        smoke = target_metadata.get("smokeEvidence") or {}
        package_archive = target_metadata["packageArchive"]
        if smoke.get("sourceWorkflow") != RUST_RELEASE_WORKFLOW:
            raise RuntimeError(f"{target} smoke source workflow mismatch")
        if smoke.get("workflowRunId") != run_id:
            raise RuntimeError(f"{target} smoke workflow run mismatch")
        if smoke.get("workflowRunAttempt") != run_attempt:
            raise RuntimeError(f"{target} smoke workflow run attempt mismatch")
        if smoke.get("commitSha") != commit_sha or smoke.get("target") != target:
            raise RuntimeError(f"{target} smoke commit or target mismatch")
        if smoke.get("jobConclusion") != "success":
            raise RuntimeError(f"{target} smoke job did not succeed")
        if smoke.get("packageArchiveFilename") != package_archive["archiveFilename"]:
            raise RuntimeError(f"{target} smoke archive filename mismatch")
        if smoke.get("packageArchiveSha256") != package_archive["sha256"]:
            raise RuntimeError(f"{target} smoke archive digest mismatch")
        if smoke.get("sandboxSmoke") != REQUIRED_LINUX_SANDBOX_SMOKE:
            raise RuntimeError(f"{target} sandbox smoke evidence mismatch")
        if set(smoke.get("smokeTests") or []) != set(
            REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS
        ):
            raise RuntimeError(f"{target} smoke test evidence mismatch")

    if require_publication_finalization:
        if metadata.get("publicationFinalized") is not True:
            raise RuntimeError("Linux release evidence is not publication-finalized")
        public_manifest = metadata.get("publicChecksumManifest") or {}
        if public_manifest.get("filename") != public_checksum_manifest.name:
            raise RuntimeError("public checksum manifest filename mismatch")
        if public_manifest.get("sha256") != sha256_file(public_checksum_manifest):
            raise RuntimeError("public checksum manifest digest mismatch")
    return metadata


def finalize_linux_release_evidence(
    preliminary_metadata_dir: Path,
    public_checksum_manifest: Path,
    output_path: Path,
    *,
    run_id: str,
    run_attempt: str,
    commit_sha: str,
    release_tag: str,
) -> None:
    preliminary_path = unique_release_artifact(
        preliminary_metadata_dir, "shipping-release-readiness.json"
    )
    metadata = validate_linux_release_evidence(
        preliminary_path,
        public_checksum_manifest,
        run_id=run_id,
        run_attempt=run_attempt,
        commit_sha=commit_sha,
        release_tag=release_tag,
        require_publication_finalization=False,
    )
    metadata["evidenceKind"] = "rustReleasePublishedArtifactEvidence"
    metadata["publicationFinalized"] = True
    for target in LINUX_TARGETS:
        target_metadata = metadata["targets"][target]
        for field in ("packageArchive", "appServerPackageArchive"):
            target_metadata[field]["checksumManifestPath"] = (
                public_checksum_manifest.name
            )
    metadata["publicChecksumManifest"] = {
        "filename": public_checksum_manifest.name,
        "sha256": sha256_file(public_checksum_manifest),
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(metadata, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    validate_linux_release_evidence(
        output_path,
        public_checksum_manifest,
        run_id=run_id,
        run_attempt=run_attempt,
        commit_sha=commit_sha,
        release_tag=release_tag,
        require_publication_finalization=True,
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
        choices=["non-publishing"],
        required=True,
    )
    linux_release = subparsers.add_parser("collect-linux-release")
    linux_release.add_argument("--artifacts-dir", type=Path, required=True)
    linux_release.add_argument("--out", type=Path, required=True)
    linux_release.add_argument("--run-id", required=True)
    linux_release.add_argument("--run-attempt", required=True)
    linux_release.add_argument("--commit-sha", required=True)
    linux_release.add_argument("--release-tag", required=True)
    linux_release.add_argument("--verification-job", required=True)
    linux_release.add_argument("--verification-conclusion", required=True)
    linux_release.add_argument("--verification-commit-sha", required=True)
    finalize_release = subparsers.add_parser("finalize-linux-release")
    finalize_release.add_argument("--metadata-dir", type=Path, required=True)
    finalize_release.add_argument(
        "--public-checksum-manifest", type=Path, required=True
    )
    finalize_release.add_argument("--out", type=Path, required=True)
    finalize_release.add_argument("--run-id", required=True)
    finalize_release.add_argument("--run-attempt", required=True)
    finalize_release.add_argument("--commit-sha", required=True)
    finalize_release.add_argument("--release-tag", required=True)
    validate_release = subparsers.add_parser("validate-linux-release")
    validate_release.add_argument("--metadata", type=Path, required=True)
    validate_release.add_argument(
        "--public-checksum-manifest", type=Path, required=True
    )
    validate_release.add_argument("--run-id", required=True)
    validate_release.add_argument("--run-attempt", required=True)
    validate_release.add_argument("--commit-sha", required=True)
    validate_release.add_argument("--release-tag", required=True)
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
        collect_artifacts(
            args.artifacts_dir,
            args.out,
            fixture_substitutions=fixture_substitution_records(),
        )
    elif args.command == "collect-linux-release":
        collect_linux_release_artifacts(
            args.artifacts_dir,
            args.out,
            run_id=args.run_id,
            run_attempt=args.run_attempt,
            commit_sha=args.commit_sha,
            release_tag=args.release_tag,
            verification_job=args.verification_job,
            verification_conclusion=args.verification_conclusion,
            verification_commit_sha=args.verification_commit_sha,
        )
    elif args.command == "finalize-linux-release":
        finalize_linux_release_evidence(
            args.metadata_dir,
            args.public_checksum_manifest,
            args.out,
            run_id=args.run_id,
            run_attempt=args.run_attempt,
            commit_sha=args.commit_sha,
            release_tag=args.release_tag,
        )
    elif args.command == "validate-linux-release":
        validate_linux_release_evidence(
            args.metadata,
            args.public_checksum_manifest,
            run_id=args.run_id,
            run_attempt=args.run_attempt,
            commit_sha=args.commit_sha,
            release_tag=args.release_tag,
            require_publication_finalization=True,
        )


if __name__ == "__main__":
    main()
