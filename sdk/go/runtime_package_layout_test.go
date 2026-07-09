package codex

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

var runtimeLayoutTargets = map[string]string{
	"linux_arm64_musl": "aarch64-unknown-linux-musl",
	"linux_amd64_musl": "x86_64-unknown-linux-musl",
	"macos_amd64":      "x86_64-apple-darwin",
	"macos_arm64":      "aarch64-apple-darwin",
	"windows_amd64":    "x86_64-pc-windows-msvc",
	"windows_arm64":    "aarch64-pc-windows-msvc",
}

func TestRuntimeLayoutBazelSeedTargets(t *testing.T) {
	build := readRepoText(t, "codex-rs/cli/BUILD.bazel")
	rule := readRepoText(t, "bazel/platforms/go_sdk_runtime_layout.bzl")
	releaseBinaries := readRepoText(t, "bazel/platforms/release_binaries.bzl")

	for _, required := range []string{
		`load("//bazel/platforms:go_sdk_runtime_layout.bzl", "go_sdk_runtime_layouts")`,
		`go_sdk_runtime_layouts(`,
		`name = "codex_go_sdk_runtime_layout"`,
		`binary = "codex"`,
		`filegroup_name = "codex_code_mode_host_release_binaries"`,
		`name = "codex_code_mode_host"`,
		`target = "//codex-rs/code-mode-host:codex-code-mode-host"`,
		`code_mode_host_binary = "codex_code_mode_host"`,
	} {
		if !strings.Contains(build, required) {
			t.Fatalf("codex-rs/cli/BUILD.bazel missing %q", required)
		}
	}

	for platform, target := range runtimeLayoutTargets {
		if !strings.Contains(releaseBinaries, `"`+platform+`": "`+target+`"`) {
			t.Fatalf("release_binaries.bzl missing platform target mapping %s -> %s", platform, target)
		}
	}

	for _, required := range []string{
		`target = PLATFORM_TARGETS[platform]`,
		`codex = ":" + binary + "_" + platform`,
		`code_mode_host = ":" + code_mode_host_binary + "_" + platform`,
		`code_mode_host_name = "codex-code-mode-host"`,
		`code_mode_host_name = "codex-code-mode-host.exe"`,
		`ctx.label.name + "/bin/" + ctx.attr.code_mode_host_name`,
		`target_file = ctx.executable.code_mode_host`,
		`files = depset([entrypoint, code_mode_host, metadata])`,
		`runfiles = ctx.runfiles(files = [entrypoint, code_mode_host, metadata])`,
		`ctx.actions.symlink(`,
		`ctx.actions.write(`,
		`"entrypoint": "bin/%s"`,
		`"resourcesDir": "codex-resources"`,
		`"pathDir": "codex-path"`,
		`"variant": "codex"`,
	} {
		if !strings.Contains(rule, required) {
			t.Fatalf("go_sdk_runtime_layout.bzl missing %q", required)
		}
	}
}

func TestRuntimeLayoutBazelSeedDoesNotResolveHelpers(t *testing.T) {
	rule := readRepoText(t, "bazel/platforms/go_sdk_runtime_layout.bzl")

	for _, forbidden := range []string{
		"CODEX_PACKAGE_HELPER_ROOT",
		"codex-package-helpers.json",
		"materialize_helpers",
		"DotSlash",
		"dotslash",
		"download_archive",
		"package-cache",
		"run_shell",
		"curl ",
		"wget ",
		"http://",
		"https://",
	} {
		if strings.Contains(rule, forbidden) {
			t.Fatalf("Bazel layout seed must not resolve external helpers or fetch network resources; found %q", forbidden)
		}
	}
}

func TestRuntimeLayoutPlanRequiresManifestMerge(t *testing.T) {
	stage6 := readRepoText(t, "docs/superpowers/plans/2026-07-02-go-sdk-full/stage-06-docs-ci-release.md")
	readme := readRepoText(t, "scripts/codex_package/README.md")

	for _, required := range []string{
		"helper-root manifest verification",
		"codex-package-helpers.json",
		"python3 -m codex_package.materialize_helpers --verify-only",
		"not from DotSlash/package-cache during staging",
	} {
		if !strings.Contains(stage6, required) {
			t.Fatalf("stage-06 plan missing runtime manifest requirement %q", required)
		}
	}
	for _, required := range []string{
		"pre-produced artifact",
		"--verify-only",
		"must not call the materializer, DotSlash, package-cache lookup",
	} {
		if !strings.Contains(readme, required) {
			t.Fatalf("package README missing helper-root consumer contract %q", required)
		}
	}
}

func TestRuntimeLayoutGoSDKWorkflowWiring(t *testing.T) {
	workflow := readRepoText(t, ".github/workflows/sdk.yml")

	for _, required := range []string{
		"  go-sdk:",
		`name: Go SDK - ${{ matrix.name }}`,
		"runs-on: ${{ matrix.runs_on }}",
		"fail-fast: false",
		"linux-musl",
		"x86_64-unknown-linux-musl",
		"linux-arm64-musl",
		"aarch64-unknown-linux-musl",
		"macos-arm64",
		"aarch64-apple-darwin",
		"macos-x64",
		"x86_64-apple-darwin",
		"windows",
		"x86_64-pc-windows-msvc",
		"windows-arm64",
		"aarch64-pc-windows-msvc",
		"${{ github.event.repository.name }}-linux-x64-xl",
		"${{ github.event.repository.name }}-linux-arm64",
		"macos-15-xlarge",
		"macos-15-large",
		"${{ github.event.repository.name }}-windows-x64",
		"${{ github.event.repository.name }}-windows-arm64",
		"runner_label:",
		"actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2",
		"./.github/actions/setup-bazel-ci",
		"actions/setup-go@924ae3a1cded613372ab5595356fb5720e22ba16 # v6",
		`go-version: "1.25"`,
		"dtolnay/rust-toolchain@e081816240890017053eacbb1bdf337761dc5582 # 1.95.0",
		`toolchain: "1.95.0"`,
		"./.github/actions/setup-msvc-env",
		"taiki-e/install-action@44c6d64aa62cd779e873306675c7a58e86d6d532 # v2.62.49",
		"tool: just,nextest@0.9.103",
		"bubblewrap pkg-config libcap-dev",
		"Assert macOS x64 runner architecture",
		`test "$(uname -m)" = "x86_64"`,
		"Materialize package helper root",
		"CODEX_PACKAGE_HELPER_ROOT",
		"python3 -m codex_package.materialize_helpers",
		"--verify-only",
		"--bwrap-bin",
		"--codex-command-runner-bin",
		"--codex-windows-sandbox-setup-bin",
		".github/scripts/stage-codex-runtime.sh",
		`--bazel-target "${{ matrix.bazel_target }}"`,
		"--cargo-profile dev",
		"--build-metadata-job go-sdk",
		`--github-env "${GITHUB_ENV}"`,
		"--windows-release-shaped-msvc",
		"--windows-msvc-host-platform",
		`CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER: "1"`,
		"TestRealAppServerInitializeStrictDigest",
		"TestRealAppServerRejectsDebugHookEnv",
		"TestRealAppServerDigestMismatch",
		"TestRealAppServerCompatibilityOverridePolicy",
		"TestRealAppServerThreadRunHappyPath",
		"TestRealAppServerConfigReadWrite",
		"TestRealAppServerFilesystemWatch",
		"TestRealAppServerCommandExecStreaming",
		"TestRealAppServerProcessLifecycle",
		"TestRealAppServerSafeResourceWorkflows",
		"TestRealAppServerRemoteControlWorkflow",
		"TestRealAppServerModelList",
		"TestRealAppServerProtocolModeExperimentalGate",
		"TestRealAppServerUnauthenticatedAccountRead",
		"go test -race ./...",
		"--release-package-archive",
		"--cargo-profile release",
		"--build-metadata-job go-sdk-release-archive",
		"--verify-sandbox",
		`--exec-path "${CODEX_EXEC_PATH:?}"`,
		"CODEX_RUNTIME_METADATA_PATH",
		"CODEX_GO_SDK_HELPER_ROOT_SOURCE=producer-mode-blocked",
		"unset CODEX_HOME",
		"CODEX_GO_SDK_ZSTD_SOURCE",
		"--zstd-source",
		"DotSlash fallback is not accepted",
		"release-archive-smoke.log",
		"release_shape_args+=(--windows-release-shaped-msvc --windows-msvc-host-platform)",
		`metadata.get("bazelTarget") != "${{ matrix.bazel_target }}"`,
		`metadata.get("helperManifest")`,
		`metadata.get("packageArchive")`,
		`metadata.get("runtimeSource") != "packageArchive"`,
		`metadata.get("cargoProfile") != "release"`,
		`required_helpers = {"rg.exe" if "windows" in target else "rg"}`,
		`go test ./... -list "^${test_name}$" | grep -Fx "$test_name"`,
		"ran_smoke_tests",
		"helperRootEvidence",
		"producerModeMaterializeHelpers",
		"notReleaseReadyTargets",
		"SDK CI target evidence is uploaded for audit only",
		"windowsReleaseShapedMsvc",
		"go-sdk-ci-release-evidence",
		"target-evidence.json",
		"go-sdk-ci-release-evidence.json",
		"packageArchiveSmokeTests",
		`"packageArchiveVerifier": "success"`,
		"stagingMetadata",
		"boundedLogs",
		"architectureProofPath",
		"windowsHostProofPath",
		`path: go-sdk-ci-release-evidence/`,
		"actions/upload-artifact@bbbca2ddaa5d8feaa63e36b76fdaad77386f024f # v7.0.0",
		"actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1",
		"notReleaseReady",
		"just write-app-server-schema --check",
		"write_schema_fixtures",
		"just test -p codex-app-server-protocol",
		"//codex-rs/app-server-protocol:app-server-protocol",
		"go run ./internal/cmd/protocodex --check --mode stable",
		"go run ./internal/cmd/protocodex --check --mode experimental",
		"go run ./internal/cmd/protocodex --check --mode both",
		"--stable-schema-root ../../codex-rs/app-server-protocol/schema",
		"--experimental-schema-root internal/protocodex/schema-experimental",
		"--out protocol",
		"--root-out .",
		"TestResourceCoverage|TestResourceDocsCoverage|TestServerHandlerDocsCoverage",
	} {
		if !strings.Contains(workflow, required) {
			t.Fatalf("sdk.yml go-sdk job missing %q", required)
		}
	}

	stepOrder := []string{
		"  go-sdk:",
		"Checkout repository",
		"Set up Bazel CI",
		"Setup Go",
		"Setup Rust",
		"Materialize package helper root",
		"Stage Go SDK runtime",
		"Test Go SDK against real app-server",
		"Test Go SDK release archive runtime",
		"Check Rust protocol schema drift",
		"Check Go SDK generated protocol drift",
		"Check for a clean worktree",
	}
	previous := -1
	for _, marker := range stepOrder {
		next := strings.Index(workflow[previous+1:], marker)
		if next == -1 {
			t.Fatalf("sdk.yml go-sdk job missing ordered marker %q", marker)
		}
		current := previous + 1 + next
		if current <= previous {
			t.Fatalf("sdk.yml go-sdk marker %q appears out of order", marker)
		}
		previous = current
	}

	for _, forbidden := range []string{
		`echo "CODEX_HOME=`,
		"CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG=",
		"CODEX_APP_SERVER_MANAGED_CONFIG_PATH=",
		"CODEX_APP_SERVER_LOGIN_ISSUER=",
		"CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS=",
		"CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE=",
		".github/workflows/zstd",
		"TestSandboxPolicy",
		"--schema-root internal/protocodex/schema",
	} {
		if strings.Contains(workflow, forbidden) {
			t.Fatalf("sdk.yml go-sdk job contains forbidden runtime staging source %q", forbidden)
		}
	}
}

func readRepoText(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "..", filepath.FromSlash(path)))
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(data)
}
