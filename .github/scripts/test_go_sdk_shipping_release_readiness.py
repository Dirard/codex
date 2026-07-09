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

    def test_collect_artifacts_writes_stage7_target_metadata(self) -> None:
        if shutil.which("tar") is None or shutil.which("zstd") is None:
            self.skipTest("tar and zstd are required for package archive fixtures")

        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            artifacts_dir = root / "artifacts"
            out_dir = root / "shipping-release-readiness-metadata"
            artifacts_dir.mkdir()

            for target in go_sdk_shipping_release_readiness.EXPECTED_TARGETS:
                windows = "windows" in target
                linux = "linux" in target
                macos = "apple-darwin" in target
                exe = ".exe" if windows else ""
                codex_paths = ["codex-package.json", f"bin/codex{exe}"]
                app_server_paths = [f"bin/codex-app-server{exe}"]
                if windows:
                    codex_paths.extend(
                        [
                            "codex-path/rg.exe",
                            "codex-resources/codex-command-runner.exe",
                            "codex-resources/codex-windows-sandbox-setup.exe",
                        ]
                    )
                else:
                    codex_paths.extend(
                        [
                            "codex-path/rg",
                            "codex-resources/zsh/bin/zsh",
                        ]
                    )
                    if linux:
                        codex_paths.append("codex-resources/bwrap")

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
                        for member in [
                            f"codex-{target}.exe",
                            "codex-command-runner.exe",
                            "codex-windows-sandbox-setup.exe",
                        ]:
                            zip_file.writestr(member, member)

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
            self.assertFalse(metadata["notReleaseReady"])
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
                "notReleaseReady=false",
                (out_dir / "logs" / "source-preflight.log").read_text(
                    encoding="utf-8"
                ),
            )
            self.assertEqual(
                sorted(metadata["targets"]),
                sorted(go_sdk_shipping_release_readiness.EXPECTED_TARGETS),
            )
            linux_target = metadata["targets"]["x86_64-unknown-linux-musl"]
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
            self.assertFalse(metadata["notReleaseReady"])
            self.assertIn(
                "nonPublishingFixtureBinaries",
                {record["name"] for record in metadata["fixtureSubstitutions"]},
            )

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
