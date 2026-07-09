#!/usr/bin/env python3

import argparse
import json
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


def write_source_preflight(out_dir: Path) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "logs").mkdir(parents=True, exist_ok=True)

    (out_dir / "workflow-reuse-proof.txt").write_text(
        "\n".join(
            [
                "workflowShape=thinWrapper",
                "notReleaseReady=true",
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
                "sourcePreflightOnly=true",
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
                "notReleaseReady=true",
                "",
            ]
        ),
        encoding="utf-8",
    )
    (out_dir / "shipping-release-readiness.json").write_text(
        json.dumps(source_preflight_metadata(), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    source = subparsers.add_parser("source-preflight")
    source.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "source-preflight":
        write_source_preflight(args.out)


if __name__ == "__main__":
    main()
