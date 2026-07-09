#!/usr/bin/env python3

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

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


if __name__ == "__main__":
    unittest.main()
