package codex

import (
	"os"
	"path/filepath"
	"regexp"
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

func TestRuntimeLayoutStagingScopeDoesNotAddWorkflowWiring(t *testing.T) {
	workflow := readRepoText(t, ".github/workflows/sdk.yml")
	if regexp.MustCompile(`(?m)^\s*go-sdk:`).FindString(workflow) != "" {
		t.Fatalf("runtime layout source slice must not wire the go-sdk CI job before staging scripts and integration tests are reviewed")
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
