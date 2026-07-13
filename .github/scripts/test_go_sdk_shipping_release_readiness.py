#!/usr/bin/env python3

import json
import shutil
import subprocess
import tarfile
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZipFile

import go_sdk_shipping_release_readiness


class GoSdkShippingReleaseReadinessTest(unittest.TestCase):
    def test_source_preflight_writes_blocking_metadata(self) -> None:
        with TemporaryDirectory() as temp_dir:
            out_dir = Path(temp_dir) / "shipping-release-readiness-metadata"

            go_sdk_shipping_release_readiness.write_source_preflight(out_dir)

            metadata_path = out_dir / "shipping-release-readiness.json"
            self.assertTrue(metadata_path.is_file())
            metadata = json.loads(metadata_path.read_text(encoding="utf-8"))

            self.assertEqual(metadata["workflowShape"], "thinWrapper")
            self.assertTrue(metadata["notReleaseReady"])
            self.assertFalse(metadata["artifactEvidenceShapeComplete"])
            self.assertEqual(metadata["evidenceKind"], "sourcePreflightOnly")
            self.assertFalse(metadata["workflowLocalDuplicateCommands"])
            self.assertEqual(metadata["targets"], {})
            self.assertEqual(
                metadata["targetRequirements"],
                go_sdk_shipping_release_readiness.EXPECTED_TARGETS,
            )
            self.assertIn(
                ".github/workflows/rust-release.yml",
                metadata["reusedWorkflows"],
            )
            self.assertIn(
                ".github/workflows/rust-release-windows.yml",
                metadata["reusedWorkflows"],
            )
            self.assertIn(
                ".github/scripts/write-codex-package-checksums.sh",
                metadata["reusedScripts"],
            )
            self.assertIn(
                "TestRealAppServerRejectsDebugHookEnv",
                metadata["requiredPackageArchiveSmokeTests"],
            )

            workflow_proof = (out_dir / metadata["workflowReuseProofPath"]).read_text(
                encoding="utf-8"
            )
            self.assertIn(".github/workflows/rust-release.yml", workflow_proof)
            self.assertIn("package-macos", workflow_proof)
            self.assertIn("codex-command-runner.exe", workflow_proof)

            duplicate_audit = (
                out_dir / metadata["duplicateCommandAuditPath"]
            ).read_text(encoding="utf-8")
            self.assertIn("workflowLocalDuplicateCommands=false", duplicate_audit)

            for log in metadata["boundedLogs"]:
                log_path = out_dir / log["path"]
                self.assertTrue(log_path.is_file())
                self.assertLessEqual(log_path.stat().st_size, log["maxBytes"])

    def test_collect_artifacts_blocks_implicit_real_mode(self) -> None:
        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            with self.assertRaisesRegex(
                SystemExit,
                "releaseArtifactEvidence mode requires explicit downloaded real release workflow evidence",
            ):
                go_sdk_shipping_release_readiness.collect_artifacts(
                    root / "artifacts",
                    root / "shipping-release-readiness-metadata",
                    fixture_substitutions=[],
                )

    def test_collect_artifacts_writes_stage7_target_metadata(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()

            self.write_shipping_artifacts(artifacts_dir, root)

            go_sdk_shipping_release_readiness.collect_artifacts(
                artifacts_dir,
                out_dir,
                fixture_substitutions=go_sdk_shipping_release_readiness.fixture_substitution_records(),
            )

            metadata = json.loads(
                (out_dir / "shipping-release-readiness.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertTrue(metadata["notReleaseReady"])
            self.assertTrue(metadata["artifactEvidenceShapeComplete"])
            self.assertEqual(metadata["evidenceKind"], "nonPublishingFixtureEvidence")
            self.assertIn("blocked", metadata["releaseReadinessImpact"])
            self.assertEqual(
                metadata["fixtureSubstitutions"],
                go_sdk_shipping_release_readiness.fixture_substitution_records(),
            )
            self.assertIn(
                "targetEvidenceAttached=true",
                (out_dir / metadata["duplicateCommandAuditPath"]).read_text(
                    encoding="utf-8"
                ),
            )
            self.assertIn(
                "notReleaseReady=true",
                (out_dir / "logs" / "source-preflight.log").read_text(
                    encoding="utf-8"
                ),
            )
            self.assertEqual(
                sorted(metadata["targets"]),
                sorted(go_sdk_shipping_release_readiness.EXPECTED_TARGETS),
            )
            linux_target = metadata["targets"]["x86_64-unknown-linux-musl"]
            self.assertTrue(linux_target["fixtureOnly"])
            self.assertFalse(linux_target["packageArchiveSmokeTestsRan"])
            self.assertEqual(
                linux_target["expectedPackageArchiveSmokeTests"],
                go_sdk_shipping_release_readiness.REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS,
            )
            self.assertNotIn("packageArchiveSmokeTests", linux_target)
            self.assertEqual(
                linux_target["jobName"],
                "Shipping package archive - x86_64-unknown-linux-musl",
            )
            self.assertIn("codex-resources/bwrap", linux_target["archivePaths"])
            self.assertIn(
                "codex-resources/bwrap",
                (out_dir / linux_target["archiveInventoryPath"]).read_text(
                    encoding="utf-8"
                ),
            )
            checksum = linux_target["packageArchiveChecksum"]
            checksum_manifest = (out_dir / checksum["manifestPath"]).read_text(
                encoding="utf-8"
            )
            self.assertIn(linux_target["archiveFilename"], checksum_manifest)
            self.assertIn(checksum["value"], checksum_manifest)

            macos_x64 = metadata["targets"]["x86_64-apple-darwin"]
            self.assertEqual(
                macos_x64["packageMacosJob"],
                "Package macOS artifacts - x86_64-apple-darwin",
            )
            self.assertEqual(
                macos_x64["finalizeMacosJob"],
                "Verify macOS artifacts - x86_64-apple-darwin",
            )
            self.assertEqual(macos_x64["architectureProof"]["unameMachine"], "x86_64")
            self.assertEqual(macos_x64["runnerLabel"], "macos-15-large")
            self.assertTrue(macos_x64["dmgArtifactNames"])
            self.assertTrue(macos_x64["directArtifactNames"])

            windows_target = metadata["targets"]["x86_64-pc-windows-msvc"]
            self.assertIn("codex-code-mode-host.exe", windows_target["publishedZipMembers"])
            self.assertIn("codex-command-runner.exe", windows_target["publishedZipMembers"])
            self.assertIn(
                "codex-windows-sandbox-setup.exe",
                (out_dir / windows_target["publishedZipInventoryPath"]).read_text(
                    encoding="utf-8"
                ),
            )
            self.assertTrue(metadata["dotslash"]["archiveParity"])
            self.assertEqual(
                metadata["dotslash"]["publishDotslashJob"],
                "Shipping DotSlash parity",
            )
            self.assertIn(
                "codex-windows-sandbox-setup",
                (out_dir / metadata["dotslash"]["archiveParityReportPath"]).read_text(
                    encoding="utf-8"
                ),
            )

    def test_collect_artifacts_rejects_missing_required_runtime_members(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()
            self.write_shipping_artifacts(
                artifacts_dir,
                root,
                omitted_members={
                    (
                        "x86_64-pc-windows-msvc",
                        "codex",
                        "bin/codex-code-mode-host.exe",
                    )
                },
            )

            with self.assertRaisesRegex(
                RuntimeError,
                "codex-package-x86_64-pc-windows-msvc.tar.zst.*bin/codex-code-mode-host.exe",
            ):
                go_sdk_shipping_release_readiness.collect_artifacts(
                    artifacts_dir,
                    out_dir,
                    fixture_substitutions=go_sdk_shipping_release_readiness.fixture_substitution_records(),
                )

    def test_collect_linux_release_artifacts_binds_archives_to_release_run(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()
            self.write_linux_release_artifacts(
                artifacts_dir,
                root,
                run_id="12345",
                run_attempt="2",
                commit_sha="a" * 40,
            )

            go_sdk_shipping_release_readiness.collect_linux_release_artifacts(
                artifacts_dir,
                out_dir,
                run_id="12345",
                run_attempt="2",
                commit_sha="a" * 40,
                release_tag="rust-v1.2.3",
                verification_job="go-sdk-linux-verification",
                verification_conclusion="success",
                verification_commit_sha="a" * 40,
            )

            metadata = json.loads(
                (out_dir / "shipping-release-readiness.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertFalse(metadata["notReleaseReady"])
            self.assertTrue(metadata["linuxReleaseReady"])
            self.assertEqual(metadata["evidenceKind"], "rustReleaseArtifactEvidence")
            self.assertEqual(metadata["source"]["workflowRunId"], "12345")
            self.assertEqual(metadata["source"]["workflowRunAttempt"], "2")
            self.assertEqual(metadata["source"]["commitSha"], "a" * 40)
            self.assertEqual(
                metadata["goSdkVerification"],
                {
                    "workflow": go_sdk_shipping_release_readiness.RUST_RELEASE_WORKFLOW,
                    "workflowRunId": "12345",
                    "workflowRunAttempt": "2",
                    "job": "go-sdk-linux-verification",
                    "conclusion": "success",
                    "commitSha": "a" * 40,
                },
            )
            self.assertEqual(
                sorted(metadata["targets"]),
                sorted(go_sdk_shipping_release_readiness.LINUX_TARGETS),
            )
            for target, target_metadata in metadata["targets"].items():
                self.assertEqual(
                    target_metadata["packageArchive"]["artifactName"], target
                )
                self.assertEqual(
                    target_metadata["appServerPackageArchive"]["artifactName"],
                    f"{target}-app-server",
                )
                self.assertEqual(
                    target_metadata["smokeEvidence"]["packageArchiveSha256"],
                    target_metadata["packageArchive"]["sha256"],
                )
                self.assertEqual(
                    target_metadata["smokeEvidence"]["sandboxSmoke"],
                    go_sdk_shipping_release_readiness.REQUIRED_LINUX_SANDBOX_SMOKE,
                )

            public_manifest = artifacts_dir / "codex-package_SHA256SUMS"
            final_evidence = root / "go-sdk-linux-release-readiness.json"
            go_sdk_shipping_release_readiness.finalize_linux_release_evidence(
                out_dir,
                public_manifest,
                final_evidence,
                run_id="12345",
                run_attempt="2",
                commit_sha="a" * 40,
                release_tag="rust-v1.2.3",
            )
            finalized = go_sdk_shipping_release_readiness.validate_linux_release_evidence(
                final_evidence,
                public_manifest,
                run_id="12345",
                run_attempt="2",
                commit_sha="a" * 40,
                release_tag="rust-v1.2.3",
                require_publication_finalization=True,
            )
            self.assertTrue(finalized["publicationFinalized"])
            self.assertEqual(
                finalized["publicChecksumManifest"]["sha256"],
                go_sdk_shipping_release_readiness.sha256_file(public_manifest),
            )
            for target in go_sdk_shipping_release_readiness.LINUX_TARGETS:
                for package_key in ("packageArchive", "appServerPackageArchive"):
                    self.assertEqual(
                        finalized["targets"][target][package_key]["checksumManifestPath"],
                        public_manifest.name,
                    )

    def test_finalize_linux_release_evidence_rejects_public_manifest_drift(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()
            self.write_linux_release_artifacts(
                artifacts_dir,
                root,
                run_id="12345",
                run_attempt="1",
                commit_sha="a" * 40,
            )
            go_sdk_shipping_release_readiness.collect_linux_release_artifacts(
                artifacts_dir,
                out_dir,
                run_id="12345",
                run_attempt="1",
                commit_sha="a" * 40,
                release_tag="rust-v1.2.3",
                verification_job="go-sdk-linux-verification",
                verification_conclusion="success",
                verification_commit_sha="a" * 40,
            )
            public_manifest = artifacts_dir / "codex-package_SHA256SUMS"
            public_manifest.write_text(
                public_manifest.read_text(encoding="utf-8").replace("a", "b", 1),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, "public checksum manifest"):
                go_sdk_shipping_release_readiness.finalize_linux_release_evidence(
                    out_dir,
                    public_manifest,
                    root / "go-sdk-linux-release-readiness.json",
                    run_id="12345",
                    run_attempt="1",
                    commit_sha="a" * 40,
                    release_tag="rust-v1.2.3",
                )

    def test_collect_linux_release_artifacts_rejects_foreign_smoke_evidence(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()
            self.write_linux_release_artifacts(
                artifacts_dir,
                root,
                run_id="12345",
                run_attempt="1",
                commit_sha="a" * 40,
            )
            evidence_path = next(artifacts_dir.rglob("go-sdk-release-smoke-*.json"))
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            evidence["workflowRunId"] = "foreign-run"
            evidence_path.write_text(
                json.dumps(evidence, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, "workflow run"):
                go_sdk_shipping_release_readiness.collect_linux_release_artifacts(
                    artifacts_dir,
                    out_dir,
                    run_id="12345",
                    run_attempt="1",
                    commit_sha="a" * 40,
                    release_tag="rust-v1.2.3",
                    verification_job="go-sdk-linux-verification",
                    verification_conclusion="success",
                    verification_commit_sha="a" * 40,
                )

    def test_collect_linux_release_artifacts_rejects_missing_sandbox_smoke(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()
            self.write_linux_release_artifacts(
                artifacts_dir,
                root,
                run_id="12345",
                run_attempt="1",
                commit_sha="a" * 40,
            )
            evidence_path = next(artifacts_dir.rglob("go-sdk-release-smoke-*.json"))
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            evidence.pop("sandboxSmoke")
            evidence_path.write_text(
                json.dumps(evidence, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(RuntimeError, "sandbox smoke evidence"):
                go_sdk_shipping_release_readiness.collect_linux_release_artifacts(
                    artifacts_dir,
                    out_dir,
                    run_id="12345",
                    run_attempt="1",
                    commit_sha="a" * 40,
                    release_tag="rust-v1.2.3",
                    verification_job="go-sdk-linux-verification",
                    verification_conclusion="success",
                    verification_commit_sha="a" * 40,
                )

    def test_build_fixture_artifacts_uses_shared_package_archive_script(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            work_dir = root / "work"
            out_dir = root / "shipping-release-readiness-metadata"
            repo_root = Path(__file__).resolve().parents[2]

            go_sdk_shipping_release_readiness.build_fixture_artifacts(
                repo_root,
                artifacts_dir,
                work_dir,
            )
            self.assertTrue(
                (
                    artifacts_dir
                    / "codex-package-x86_64-unknown-linux-musl.tar.zst"
                ).is_file()
            )
            self.assertTrue(
                (
                    artifacts_dir
                    / "codex-app-server-package-x86_64-pc-windows-msvc.tar.zst"
                ).is_file()
            )
            self.assertTrue(
                (artifacts_dir / "codex-x86_64-pc-windows-msvc.exe.zip").is_file()
            )
            self.assertTrue((artifacts_dir / "codex-x86_64-apple-darwin.dmg").is_file())

            go_sdk_shipping_release_readiness.collect_artifacts(
                artifacts_dir,
                out_dir,
                fixture_substitutions=go_sdk_shipping_release_readiness.fixture_substitution_records(),
            )
            metadata = json.loads(
                (out_dir / "shipping-release-readiness.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertTrue(metadata["notReleaseReady"])
            self.assertTrue(metadata["artifactEvidenceShapeComplete"])
            self.assertEqual(metadata["evidenceKind"], "nonPublishingFixtureEvidence")
            self.assertIn(
                "nonPublishingFixtureBinaries",
                {record["name"] for record in metadata["fixtureSubstitutions"]},
            )

    def write_shipping_artifacts(
        self,
        artifacts_dir: Path,
        root: Path,
        *,
        omitted_members: set[tuple[str, str, str]] | None = None,
    ) -> None:
        omitted_members = omitted_members or set()
        for target in go_sdk_shipping_release_readiness.EXPECTED_TARGETS:
            macos = "apple-darwin" in target
            windows = "windows" in target
            codex_paths = self.filtered_members(
                target,
                "codex",
                go_sdk_shipping_release_readiness.required_codex_package_paths(target),
                omitted_members,
            )
            app_server_paths = self.filtered_members(
                target,
                "app-server",
                go_sdk_shipping_release_readiness.required_app_server_package_paths(target),
                omitted_members,
            )

            self.write_tar_zst(
                artifacts_dir / f"codex-package-{target}.tar.zst",
                root,
                codex_paths,
            )
            self.write_tar_zst(
                artifacts_dir / f"codex-app-server-package-{target}.tar.zst",
                root,
                app_server_paths,
            )

            if macos:
                for artifact_name in [
                    f"codex-{target}.dmg",
                    f"codex-{target}",
                    f"codex-app-server-{target}",
                ]:
                    (artifacts_dir / artifact_name).write_text(
                        artifact_name,
                        encoding="utf-8",
                    )

            if windows:
                with ZipFile(artifacts_dir / f"codex-{target}.exe.zip", "w") as zip_file:
                    for member in self.filtered_members(
                        target,
                        "zip",
                        go_sdk_shipping_release_readiness.required_windows_zip_members(
                            target
                        ),
                        omitted_members,
                    ):
                        zip_file.writestr(member, member)

    def write_linux_release_artifacts(
        self,
        artifacts_dir: Path,
        root: Path,
        *,
        run_id: str,
        run_attempt: str,
        commit_sha: str,
    ) -> None:
        manifest_lines = []
        for target in go_sdk_shipping_release_readiness.LINUX_TARGETS:
            primary_dir = artifacts_dir / target
            app_server_dir = artifacts_dir / f"{target}-app-server"
            primary_dir.mkdir()
            app_server_dir.mkdir()
            codex_archive = primary_dir / f"codex-package-{target}.tar.zst"
            app_server_archive = (
                app_server_dir / f"codex-app-server-package-{target}.tar.zst"
            )
            self.write_tar_zst(
                codex_archive,
                root,
                go_sdk_shipping_release_readiness.required_codex_package_paths(target),
            )
            self.write_tar_zst(
                app_server_archive,
                root,
                go_sdk_shipping_release_readiness.required_app_server_package_paths(
                    target
                ),
            )
            codex_sha256 = go_sdk_shipping_release_readiness.sha256_file(codex_archive)
            app_server_sha256 = go_sdk_shipping_release_readiness.sha256_file(
                app_server_archive
            )
            manifest_lines.extend(
                [
                    f"{codex_sha256}  {codex_archive.name}",
                    f"{app_server_sha256}  {app_server_archive.name}",
                ]
            )
            architecture = "x86_64" if target.startswith("x86_64") else "aarch64"
            (primary_dir / f"go-sdk-release-smoke-{target}.json").write_text(
                json.dumps(
                    {
                        "sourceWorkflow": ".github/workflows/rust-release.yml",
                        "workflowRunId": run_id,
                        "workflowRunAttempt": run_attempt,
                        "commitSha": commit_sha,
                        "target": target,
                        "jobConclusion": "success",
                        "unameMachine": architecture,
                        "packageArchiveFilename": codex_archive.name,
                        "packageArchiveSha256": codex_sha256,
                        "smokeTests": go_sdk_shipping_release_readiness.REQUIRED_PACKAGE_ARCHIVE_SMOKE_TESTS,
                        "sandboxSmoke": go_sdk_shipping_release_readiness.REQUIRED_LINUX_SANDBOX_SMOKE,
                    },
                    indent=2,
                    sort_keys=True,
                )
                + "\n",
                encoding="utf-8",
            )
        (artifacts_dir / "codex-package_SHA256SUMS").write_text(
            "\n".join(sorted(manifest_lines)) + "\n",
            encoding="utf-8",
        )

    def filtered_members(
        self,
        target: str,
        artifact_kind: str,
        members: list[str],
        omitted_members: set[tuple[str, str, str]],
    ) -> list[str]:
        return [
            member
            for member in members
            if (target, artifact_kind, member) not in omitted_members
        ]

    def write_tar_zst(self, archive_path: Path, temp_root: Path, entries: list[str]) -> None:
        package_dir = temp_root / f"package-{archive_path.name}"
        tar_path = temp_root / f"{archive_path.name}.tar"
        package_dir.mkdir()
        for entry in entries:
            path = package_dir / entry
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(entry, encoding="utf-8")
        with tarfile.open(tar_path, "w") as tar_file:
            for entry in entries:
                tar_file.add(package_dir / entry, arcname=entry)
        subprocess.run(
            ["zstd", "-q", "-f", str(tar_path), "-o", str(archive_path)],
            check=True,
        )


if __name__ == "__main__":
    unittest.main()
