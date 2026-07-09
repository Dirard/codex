#!/usr/bin/env python3

import hashlib
import json
import os
import re
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from codex_package.archive import write_archive
from codex_package.layout import build_package_dir
from codex_package.layout import ZSH_RESOURCE_PATH
from codex_package.materialize_helpers import materialize_manifest_helper
from codex_package.targets import PACKAGE_VARIANTS
from codex_package.targets import TARGET_SPECS
from codex_package.targets import PackageInputs
from codex_package.ripgrep import resolve_rg_source
from codex_package.zsh import resolve_zsh_bin


REPO_ROOT = Path(__file__).resolve().parents[2]
PACKAGE_PLATFORMS = {
    "linux-aarch64": "aarch64-unknown-linux-musl",
    "linux-x86_64": "x86_64-unknown-linux-musl",
    "macos-aarch64": "aarch64-apple-darwin",
    "macos-x86_64": "x86_64-apple-darwin",
    "windows-aarch64": "aarch64-pc-windows-msvc",
    "windows-x86_64": "x86_64-pc-windows-msvc",
}
HELPER_OUTPUTS = {
    "codex-responses-api-proxy": set(PACKAGE_PLATFORMS),
    "bwrap": {"linux-aarch64", "linux-x86_64"},
    "codex-command-runner": {"windows-aarch64", "windows-x86_64"},
    "codex-windows-sandbox-setup": {"windows-aarch64", "windows-x86_64"},
}


class Stage5GPackageSourceContractTest(unittest.TestCase):
    def test_strict_resolvers_require_explicit_helpers_without_fetching(self) -> None:
        spec = TARGET_SPECS["x86_64-unknown-linux-musl"]

        with patch("codex_package.ripgrep.fetch_rg") as fetch_rg:
            fetch_rg.side_effect = AssertionError("must not fetch rg")
            with self.assertRaisesRegex(RuntimeError, "--rg-bin"):
                resolve_rg_source(spec, None, require_materialized=True)
            fetch_rg.assert_not_called()

        with patch("codex_package.zsh.fetch_dotslash_executable") as fetch_zsh:
            fetch_zsh.side_effect = AssertionError("must not fetch zsh")
            with self.assertRaisesRegex(RuntimeError, "--zsh-bin"):
                resolve_zsh_bin(spec, require_materialized=True)
            fetch_zsh.assert_not_called()

    def test_strict_resolvers_accept_materialized_helpers_without_fetching(self) -> None:
        spec = TARGET_SPECS["x86_64-unknown-linux-musl"]
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            rg_bin = touch_executable(root / "rg")
            zsh_bin = touch_executable(root / "zsh")

            with patch("codex_package.ripgrep.fetch_rg") as fetch_rg:
                fetch_rg.side_effect = AssertionError("must not fetch rg")
                self.assertEqual(
                    resolve_rg_source(spec, rg_bin, require_materialized=True),
                    rg_bin.resolve(),
                )
                fetch_rg.assert_not_called()

            with patch("codex_package.zsh.fetch_dotslash_executable") as fetch_zsh:
                fetch_zsh.side_effect = AssertionError("must not fetch zsh")
                self.assertEqual(
                    resolve_zsh_bin(
                        spec,
                        zsh_bin=zsh_bin,
                        require_materialized=True,
                    ),
                    zsh_bin.resolve(),
                )
                fetch_zsh.assert_not_called()

    def test_strict_resolvers_reject_dotslash_wrapper_helpers(self) -> None:
        spec = TARGET_SPECS["x86_64-unknown-linux-musl"]
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            rg_wrapper = touch_executable(
                root / "rg",
                "#!/usr/bin/env dotslash\n{}\n",
            )
            zsh_wrapper = touch_executable(
                root / "zsh",
                "#!/usr/bin/env dotslash\n{}\n",
            )

            with self.assertRaisesRegex(RuntimeError, "DotSlash"):
                resolve_rg_source(spec, rg_wrapper, require_materialized=True)
            with self.assertRaisesRegex(RuntimeError, "DotSlash"):
                resolve_zsh_bin(
                    spec,
                    zsh_bin=zsh_wrapper,
                    require_materialized=True,
                )

    def test_strict_cli_rejects_dotslash_wrappers_for_resource_helpers(
        self,
    ) -> None:
        script = REPO_ROOT / "scripts/build_codex_package.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            cases = [
                (
                    "linux-bwrap",
                    "x86_64-unknown-linux-musl",
                    "codex",
                    [
                        ("--rg-bin", "rg"),
                        ("--zsh-bin", "zsh"),
                        ("--bwrap-bin", "dotslash-bwrap"),
                    ],
                    "DotSlash",
                ),
                (
                    "windows-command-runner",
                    "x86_64-pc-windows-msvc",
                    "codex.exe",
                    [
                        ("--rg-bin", "rg.exe"),
                        ("--codex-command-runner-bin", "dotslash-runner.exe"),
                        (
                            "--codex-windows-sandbox-setup-bin",
                            "codex-windows-sandbox-setup.exe",
                        ),
                    ],
                    "DotSlash",
                ),
                (
                    "windows-sandbox-setup",
                    "x86_64-pc-windows-msvc",
                    "codex.exe",
                    [
                        ("--rg-bin", "rg.exe"),
                        ("--codex-command-runner-bin", "codex-command-runner.exe"),
                        (
                            "--codex-windows-sandbox-setup-bin",
                            "dotslash-sandbox-setup.exe",
                        ),
                    ],
                    "DotSlash",
                ),
            ]
            for case_name, target, entrypoint_name, helper_args, expected_error in cases:
                with self.subTest(case=case_name):
                    case_root = root / case_name
                    case_root.mkdir()
                    package_dir = case_root / "package"
                    command = [
                        sys.executable,
                        str(script),
                        "--target",
                        target,
                        "--variant",
                        "codex",
                        "--entrypoint-bin",
                        str(touch_executable(case_root / entrypoint_name)),
                        "--require-materialized-helper-sources",
                        "--package-dir",
                        str(package_dir),
                    ]
                    for flag_name, file_name in helper_args:
                        text = "#!/usr/bin/env sh\nexit 0\n"
                        if "dotslash" in file_name:
                            text = "#!/usr/bin/env dotslash\n{}\n"
                        command.extend(
                            [
                                flag_name,
                                str(touch_executable(case_root / file_name, text)),
                            ]
                        )

                    result = subprocess.run(
                        command,
                        cwd=REPO_ROOT,
                        text=True,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        check=False,
                    )

                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(expected_error, result.stderr)
                    self.assertFalse(package_dir.exists(), package_dir)

    def test_package_archive_script_blocks_missing_windows_helpers_before_python(self) -> None:
        script = REPO_ROOT / ".github/scripts/build-codex-package-archive.sh"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            entrypoint_dir = root / "release"
            archive_dir = root / "archives"
            fake_bin_dir = root / "bin"
            entrypoint_dir.mkdir()
            fake_bin_dir.mkdir()
            touch_executable(entrypoint_dir / "codex.exe")
            rg_bin = touch_executable(root / "rg.exe")
            fake_python = touch_executable(fake_bin_dir / "python3", "#!/usr/bin/env sh\nexit 99\n")

            env = os.environ.copy()
            env["GITHUB_WORKSPACE"] = str(REPO_ROOT)
            env["RUNNER_TEMP"] = str(root)
            env["PATH"] = f"{fake_bin_dir}{os.pathsep}{env['PATH']}"
            result = subprocess.run(
                [
                    "bash",
                    str(script),
                    "--target",
                    "x86_64-pc-windows-msvc",
                    "--bundle",
                    "primary",
                    "--entrypoint-dir",
                    str(entrypoint_dir),
                    "--archive-dir",
                    str(archive_dir),
                    "--require-materialized-helper-sources",
                    "--rg-bin",
                    str(rg_bin),
                ],
                cwd=REPO_ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertNotEqual(result.returncode, 99, fake_python)
        self.assertIn("codex-command-runner.exe", result.stderr)

    def test_strict_cli_blocks_source_built_helpers_before_cargo(self) -> None:
        script = REPO_ROOT / "scripts/build_codex_package.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            fake_cargo = touch_executable(root / "cargo", "#!/usr/bin/env sh\nexit 99\n")

            for target, entrypoint_name, rg_name, zsh_name, missing_flag in [
                (
                    "x86_64-unknown-linux-musl",
                    "codex",
                    "rg",
                    "zsh",
                    "--bwrap-bin",
                ),
                (
                    "x86_64-pc-windows-msvc",
                    "codex.exe",
                    "rg.exe",
                    None,
                    "--codex-command-runner-bin",
                ),
            ]:
                with self.subTest(target=target):
                    case_root = root / target
                    case_root.mkdir()
                    package_dir = case_root / "package"
                    command = [
                        sys.executable,
                        str(script),
                        "--target",
                        target,
                        "--variant",
                        "codex",
                        "--entrypoint-bin",
                        str(touch_executable(case_root / entrypoint_name)),
                        "--rg-bin",
                        str(touch_executable(case_root / rg_name)),
                        "--require-materialized-helper-sources",
                        "--cargo",
                        str(fake_cargo),
                        "--package-dir",
                        str(package_dir),
                    ]
                    if zsh_name is not None:
                        command.extend(
                            [
                                "--zsh-bin",
                                str(touch_executable(case_root / zsh_name)),
                            ]
                        )

                    result = subprocess.run(
                        command,
                        cwd=REPO_ROOT,
                        text=True,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        check=False,
                    )

                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotEqual(result.returncode, 99, result.stderr)
                    self.assertIn(missing_flag, result.stderr)
                    self.assertFalse(package_dir.exists(), package_dir)

    def test_strict_cli_blocks_missing_rg_and_zsh_before_cargo(self) -> None:
        script = REPO_ROOT / "scripts/build_codex_package.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            fake_cargo = touch_executable(root / "cargo", "#!/usr/bin/env sh\nexit 99\n")
            cases = [
                ("missing-rg", ["--zsh-bin", "zsh", "--bwrap-bin", "bwrap"], "--rg-bin"),
                ("missing-zsh", ["--rg-bin", "rg", "--bwrap-bin", "bwrap"], "--zsh-bin"),
            ]
            for case_name, provided_helpers, missing_flag in cases:
                with self.subTest(case=case_name):
                    case_root = root / case_name
                    case_root.mkdir()
                    package_dir = case_root / "package"
                    command = [
                        sys.executable,
                        str(script),
                        "--target",
                        "x86_64-unknown-linux-musl",
                        "--variant",
                        "codex",
                        "--require-materialized-helper-sources",
                        "--cargo",
                        str(fake_cargo),
                        "--package-dir",
                        str(package_dir),
                    ]
                    for flag_name, file_name in zip(
                        provided_helpers[0::2],
                        provided_helpers[1::2],
                        strict=True,
                    ):
                        command.extend(
                            [flag_name, str(touch_executable(case_root / file_name))]
                        )

                    result = subprocess.run(
                        command,
                        cwd=REPO_ROOT,
                        text=True,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        check=False,
                    )

                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotEqual(result.returncode, 99, result.stderr)
                    self.assertIn(missing_flag, result.stderr)
                    self.assertFalse(package_dir.exists(), package_dir)

    def test_strict_cli_preflights_zstd_before_package_or_archive_writes(self) -> None:
        script = REPO_ROOT / "scripts/build_codex_package.py"
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            fake_path = root / "bin"
            fake_path.mkdir()
            package_dir = root / "package"
            gzip_archive = root / "archives" / "codex-package.tar.gz"
            zstd_archive = root / "archives" / "codex-package.tar.zst"
            env = os.environ.copy()
            env["PATH"] = str(fake_path)

            result = subprocess.run(
                [
                    sys.executable,
                    str(script),
                    "--target",
                    "x86_64-unknown-linux-musl",
                    "--variant",
                    "codex",
                    "--entrypoint-bin",
                    str(touch_executable(root / "codex")),
                    "--bwrap-bin",
                    str(touch_executable(root / "bwrap")),
                    "--rg-bin",
                    str(touch_executable(root / "rg")),
                    "--zsh-bin",
                    str(touch_executable(root / "zsh")),
                    "--require-materialized-helper-sources",
                    "--package-dir",
                    str(package_dir),
                    "--archive-output",
                    str(gzip_archive),
                    "--archive-output",
                    str(zstd_archive),
                    "--force",
                ],
                cwd=REPO_ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("zstd is required", result.stderr)
        self.assertFalse(package_dir.exists(), package_dir)
        self.assertFalse(gzip_archive.exists(), gzip_archive)
        self.assertFalse(zstd_archive.exists(), zstd_archive)

    def test_package_archive_script_never_prepends_workflow_zstd(self) -> None:
        script = read_text(".github/scripts/build-codex-package-archive.sh")

        self.assertNotIn(".github/workflows:${PATH}", script)
        self.assertIn("Repo DotSlash zstd manifest is not allowed", script)

    def test_rust_release_package_archives_require_materialized_helpers(self) -> None:
        workflow = read_text(".github/workflows/rust-release.yml")

        self.assertNotIn("Download packaged zsh manifest", workflow)
        self.assertNotIn("codex-zsh\" \\", workflow)
        self.assertNotIn("curl -fsSL \\", workflow)
        self.assertGreaterEqual(workflow.count("--require-materialized-helper-sources"), 2)
        self.assertGreaterEqual(workflow.count("--rg-bin"), 2)
        self.assertGreaterEqual(workflow.count("--zsh-bin"), 2)
        self.assertIn('binaries: "codex-app-server bwrap"', workflow)
        self.assertIn("CODEX_PACKAGE_BWRAP_BIN", workflow)
        self.assertIn("--bwrap-bin", workflow)

    def test_linux_release_package_bwrap_matches_embedded_digest(self) -> None:
        workflow = read_text(".github/workflows/rust-release.yml")

        self.assertIn('package_bwrap_digest="$(sha256sum "$bwrap_bin"', workflow)
        self.assertIn(
            '[[ "$package_bwrap_digest" != "$CODEX_BWRAP_SHA256" ]]',
            workflow,
        )
        self.assertLess(
            workflow.index('package_bwrap_digest="$(sha256sum "$bwrap_bin"'),
            workflow.index('--bwrap-bin "$CODEX_PACKAGE_BWRAP_BIN"'),
        )

    def test_windows_release_package_archives_require_materialized_helpers(self) -> None:
        workflow = read_text(".github/workflows/rust-release-windows.yml")
        readme = read_text("scripts/codex_package/README.md")

        self.assertNotIn("Install DotSlash", workflow)
        self.assertNotIn("facebook/install-dotslash", workflow)
        self.assertNotIn(".github/workflows/zstd", workflow)
        self.assertNotIn("falling back to single-binary zip", workflow)
        self.assertIn("--require-materialized-helper-sources", workflow)
        self.assertIn("--rg-bin", workflow)
        self.assertIn("--codex-command-runner-bin", workflow)
        self.assertIn("--codex-windows-sandbox-setup-bin", workflow)
        self.assertIn(
            'command_runner_bin="${helper_root}/codex-command-runner.exe"',
            workflow,
        )
        self.assertIn(
            'sandbox_setup_bin="${helper_root}/codex-windows-sandbox-setup.exe"',
            workflow,
        )
        self.assertNotIn('command_runner_bin="target/${target}/release', workflow)
        self.assertNotIn('sandbox_setup_bin="target/${target}/release', workflow)
        self.assertIn(
            "${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-command-runner.exe",
            readme,
        )
        self.assertIn(
            "${CODEX_PACKAGE_HELPER_ROOT}/<target>/codex-windows-sandbox-setup.exe",
            readme,
        )

    def test_release_workflows_define_materialized_helper_producers(self) -> None:
        rust_workflow = read_text(".github/workflows/rust-release.yml")
        windows_workflow = read_text(".github/workflows/rust-release-windows.yml")
        producer = "codex_package.materialize_helpers"
        helper_root_env = (
            'CODEX_PACKAGE_HELPER_ROOT="${RUNNER_TEMP}/codex-package-helpers"'
        )

        for workflow in [rust_workflow, windows_workflow]:
            with self.subTest(workflow=workflow.splitlines()[0]):
                self.assertIn("Materialize package helpers", workflow)
                self.assertIn(producer, workflow)
                self.assertIn(helper_root_env, workflow)
                self.assertLess(
                    workflow.index("Materialize package helpers"),
                    workflow.index("Resolve materialized package helpers"),
                )

    def test_materialize_manifest_helper_extracts_verified_payload(self) -> None:
        spec = TARGET_SPECS["x86_64-unknown-linux-musl"]
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            payload_root = root / "payload"
            (payload_root / "ripgrep-15.1.0").mkdir(parents=True)
            helper = touch_executable(payload_root / "ripgrep-15.1.0" / "rg")
            archive_path = root / "rg.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                archive.add(helper, arcname="ripgrep-15.1.0/rg")
            manifest_path = root / "rg-manifest"
            manifest_path.write_text(
                "#!/usr/bin/env dotslash\n"
                + json.dumps(
                    {
                        "name": "rg",
                        "platforms": {
                            "linux-x86_64": {
                                "size": archive_path.stat().st_size,
                                "hash": "sha256",
                                "digest": sha256_file(archive_path),
                                "format": "tar.gz",
                                "path": "ripgrep-15.1.0/rg",
                                "providers": [{"url": archive_path.as_uri()}],
                            },
                        },
                    }
                ),
                encoding="utf-8",
            )

            materialized = materialize_manifest_helper(
                spec,
                manifest_path=manifest_path,
                artifact_label="ripgrep",
                dest_dir=root / "helpers",
                dest_name="rg",
            )

            self.assertEqual(materialized, root / "helpers" / "rg")
            self.assertTrue(materialized.is_file())
            self.assertTrue(os.access(materialized, os.X_OK))

    def test_dotslash_release_archive_config_parity(self) -> None:
        config = json.loads(read_text(".github/dotslash-config.json"))
        workflow = read_text(".github/workflows/rust-release.yml")

        self.assertIn("config: .github/dotslash-config.json", workflow)

        outputs = config["outputs"]
        self.assertEqual(
            set(outputs),
            {
                "codex",
                "codex-app-server",
                "codex-responses-api-proxy",
                "bwrap",
                "codex-command-runner",
                "codex-windows-sandbox-setup",
            },
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            package_script = read_text(".github/scripts/build-codex-package-archive.sh")
            for output_name, variant_name, bundle_name in [
                ("codex", "codex", "primary"),
                ("codex-app-server", "codex-app-server", "app-server"),
            ]:
                actual_platforms = outputs[output_name]["platforms"]
                self.assertEqual(set(actual_platforms), set(PACKAGE_PLATFORMS))
                for platform_name, target in PACKAGE_PLATFORMS.items():
                    spec = TARGET_SPECS[target]
                    archive_name = package_archive_name_from_script(
                        bundle_name,
                        target,
                        package_script,
                    )
                    configured = actual_platforms[platform_name]
                    self.assertRegex(archive_name, configured["regex"])

                    package_dir = root / output_name / target
                    build_fixture_package(package_dir, variant_name, spec)
                    archive_path = root / f"{archive_name.removesuffix('.tar.zst')}.tar.gz"
                    write_archive(package_dir, archive_path, force=False)
                    with tarfile.open(archive_path, "r:gz") as archive:
                        self.assertIn(configured["path"], archive.getnames())

            windows_workflow = read_text(".github/workflows/rust-release-windows.yml")
            for output_name, expected_platforms in HELPER_OUTPUTS.items():
                actual_platforms = outputs[output_name]["platforms"]
                self.assertEqual(set(actual_platforms), expected_platforms, output_name)
                for platform_name in expected_platforms:
                    target = PACKAGE_PLATFORMS[platform_name]
                    archive_name = helper_archive_name_from_workflow(
                        output_name,
                        platform_name,
                        target,
                        rust_release_workflow=workflow,
                        windows_release_workflow=windows_workflow,
                    )
                    configured = actual_platforms[platform_name]
                    self.assertRegex(archive_name, configured["regex"])
                    self.assertEqual(
                        configured["path"],
                        helper_payload_path_from_workflow(
                            output_name,
                            platform_name,
                            target,
                            rust_release_workflow=workflow,
                            windows_release_workflow=windows_workflow,
                        ),
                    )
                    self.assertIsNotNone(re.compile(configured["regex"]))

    def test_package_layout_helper_paths_match_runtime_contract(self) -> None:
        install_context = read_text("codex-rs/install-context/src/lib.rs")
        self.assertIn("path_dir.join(default_rg_command())", install_context)
        self.assertIn('PathBuf::from("rg")', install_context)
        self.assertIn('PathBuf::from("rg.exe")', install_context)
        self.assertIn(
            'PathBuf::from(ZSH_DIRNAME).join(BIN_DIRNAME).join("zsh")',
            install_context,
        )
        self.assertEqual(
            Path("codex-resources") / ZSH_RESOURCE_PATH,
            Path("codex-resources/zsh/bin/zsh"),
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for target in [
                "x86_64-unknown-linux-musl",
                "x86_64-apple-darwin",
                "x86_64-pc-windows-msvc",
            ]:
                with self.subTest(target=target):
                    spec = TARGET_SPECS[target]
                    package_dir = root / target
                    build_fixture_package(package_dir, "codex", spec)
                    self.assertTrue((package_dir / "codex-path" / spec.rg_name).is_file())
                    if spec.is_linux:
                        self.assertTrue(
                            (package_dir / "codex-resources/bwrap").is_file()
                        )
                    if not spec.is_windows:
                        self.assertTrue(
                            (
                                package_dir
                                / "codex-resources"
                                / ZSH_RESOURCE_PATH
                            ).is_file()
                        )
                    if spec.is_windows:
                        self.assertTrue(
                            (
                                package_dir
                                / "codex-resources/codex-command-runner.exe"
                            ).is_file()
                        )
                        self.assertTrue(
                            (
                                package_dir
                                / "codex-resources/codex-windows-sandbox-setup.exe"
                            ).is_file()
                        )


def read_text(relative_path: str) -> str:
    return (REPO_ROOT / relative_path).read_text(encoding="utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package_archive_name_from_script(bundle: str, target: str, script: str) -> str:
    pattern = re.compile(
        rf"^\s*{re.escape(bundle)}\)\n"
        r"(?P<body>.*?)^\s*;;$",
        re.MULTILINE | re.DOTALL,
    )
    match = pattern.search(script)
    if match is None:
        raise AssertionError(f"missing package archive bundle case: {bundle}")
    archive_stem = re.search(r'archive_stem="([^"]+)"', match.group("body"))
    if archive_stem is None:
        raise AssertionError(f"missing package archive stem for bundle: {bundle}")
    require_contains(
        script,
        'zstd_archive_path="${archive_dir}/${archive_stem}-${target}.tar.zst"',
    )
    return f"{archive_stem.group(1)}-{target}.tar.zst"


def helper_archive_name_from_workflow(
    output_name: str,
    platform_name: str,
    target: str,
    *,
    rust_release_workflow: str,
    windows_release_workflow: str,
) -> str:
    staged_name, _payload_path = helper_workflow_production(
        output_name,
        platform_name,
        target,
        rust_release_workflow=rust_release_workflow,
        windows_release_workflow=windows_release_workflow,
    )
    return f"{staged_name}.zst"


def helper_payload_path_from_workflow(
    output_name: str,
    platform_name: str,
    target: str,
    *,
    rust_release_workflow: str,
    windows_release_workflow: str,
) -> str:
    _staged_name, payload_path = helper_workflow_production(
        output_name,
        platform_name,
        target,
        rust_release_workflow=rust_release_workflow,
        windows_release_workflow=windows_release_workflow,
    )
    return payload_path


def helper_workflow_production(
    output_name: str,
    platform_name: str,
    target: str,
    *,
    rust_release_workflow: str,
    windows_release_workflow: str,
) -> tuple[str, str]:
    if platform_name.startswith("windows"):
        return windows_helper_workflow_production(
            output_name,
            target,
            windows_release_workflow,
        )

    return unix_helper_workflow_production(output_name, target, rust_release_workflow)


def unix_helper_workflow_production(
    output_name: str,
    target: str,
    workflow: str,
) -> tuple[str, str]:
    require_matrix_binary(workflow, target, output_name)
    require_contains(
        workflow,
        'cp "target/${{ matrix.target }}/release/${binary}" '
        '"$dest/${binary}-${{ matrix.target }}"',
    )
    require_contains(workflow, 'zstd -T0 -19 --rm "$dest/$base"')
    return render_workflow_template("${binary}-${{ matrix.target }}", output_name, target), output_name


def windows_helper_workflow_production(
    output_name: str,
    target: str,
    workflow: str,
) -> tuple[str, str]:
    require_matrix_binary(workflow, target, output_name)
    require_contains(workflow, 'cp "target/${{ matrix.target }}/release/${binary}.exe" \\')
    require_contains(workflow, '"$dest/${binary}-${{ matrix.target }}.exe"')
    require_contains(workflow, 'zstd -T0 -19 "$dest/$base"')
    return (
        render_workflow_template(
            "${binary}-${{ matrix.target }}.exe",
            output_name,
            target,
        ),
        render_workflow_template("${binary}.exe", output_name, target),
    )


def require_matrix_binary(workflow: str, target: str, binary: str) -> None:
    target_marker = f"target: {target}"
    for match in re.finditer(re.escape(target_marker), workflow):
        block = workflow[match.start() : match.start() + 600]
        binaries = re.search(r'binaries:\s+"([^"]+)"', block)
        if binaries and binary in binaries.group(1).split():
            return
    raise AssertionError(f"{binary} is not produced for {target}")


def require_contains(text: str, needle: str) -> None:
    if needle not in text:
        raise AssertionError(f"missing workflow production fragment: {needle}")


def render_workflow_template(template: str, binary: str, target: str) -> str:
    return (
        template.replace("${binary}", binary)
        .replace("${{ matrix.target }}", target)
        .replace("${target}", target)
    )


def touch_executable(path: Path, text: str = "#!/usr/bin/env sh\nexit 0\n") -> Path:
    path.write_text(text, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return path


def build_fixture_package(package_dir: Path, variant_name: str, spec) -> None:
    root = package_dir.parent / f"{package_dir.name}-inputs"
    root.mkdir(parents=True)
    variant = PACKAGE_VARIANTS[variant_name]
    inputs = PackageInputs(
        entrypoint_bin=touch_executable(root / variant.entrypoint_name(spec)),
        rg_bin=touch_executable(root / spec.rg_name),
        zsh_bin=None if spec.is_windows else touch_executable(root / "zsh"),
        bwrap_bin=touch_executable(root / "bwrap") if spec.is_linux else None,
        codex_command_runner_bin=touch_executable(root / "codex-command-runner.exe")
        if spec.is_windows
        else None,
        codex_windows_sandbox_setup_bin=touch_executable(
            root / "codex-windows-sandbox-setup.exe"
        )
        if spec.is_windows
        else None,
    )
    package_dir.mkdir(parents=True)
    build_package_dir(package_dir, "0.0.0", variant, spec, inputs)


if __name__ == "__main__":
    unittest.main()
