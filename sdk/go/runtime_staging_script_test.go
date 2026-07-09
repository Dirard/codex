package codex

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestStageCodexRuntimeScriptStagesVerifiedHelperRoot(t *testing.T) {
	fixture := newLinuxStagingFixture(t, "out with spaces")
	cmd := exec.Command("bash", "-c", `
set -euo pipefail
eval "$("$STAGE_SCRIPT" --out "$STAGE_OUT" --bazel-target "$STAGE_TARGET" --helper-root "$STAGE_HELPER_ROOT" --print-shell-env)"
test "$CODEX_EXEC_PATH" = "$STAGE_OUT/bin/codex"
test "$CODEX_HOME" = "$STAGE_OUT/codex-home"
test "$CODEX_GO_SDK_RUNTIME_ROOT" = "$STAGE_OUT"
test "$CODEX_RUNTIME_METADATA_PATH" = "$STAGE_OUT/codex-go-sdk-runtime-staging.json"
case "${CODEX_PACKAGE_HELPER_ROOT-}" in
  "") ;;
  *) echo "CODEX_PACKAGE_HELPER_ROOT leaked into eval output" >&2; exit 1 ;;
esac
`)
	cmd.Env = append(
		os.Environ(),
		"CODEX_GO_SDK_TEST_LAYOUT_ROOT="+fixture.seedRoot,
		"GITHUB_WORKSPACE="+fixture.repoRoot,
		"STAGE_HELPER_ROOT="+fixture.helperRoot,
		"STAGE_OUT="+fixture.out,
		"STAGE_SCRIPT="+fixture.script,
		"STAGE_TARGET="+fixture.target,
	)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("stage-codex-runtime.sh failed: %v\n%s", err, output)
	}

	for _, required := range []string{
		"codex-package.json",
		"bin/codex",
		"codex-path/rg",
		"codex-resources/zsh/bin/zsh",
		"codex-resources/bwrap",
	} {
		if _, err := os.Stat(filepath.Join(fixture.out, filepath.FromSlash(required))); err != nil {
			t.Fatalf("missing staged file %s: %v", required, err)
		}
	}
	assertNotSymlink(t, filepath.Join(fixture.out, "bin", "codex"))

	metadata := readRuntimeStagingMetadata(t, fixture.out)
	if metadata.RuntimeSource != "bazelLayout" {
		t.Fatalf("runtimeSource = %q, want bazelLayout", metadata.RuntimeSource)
	}
	if metadata.CargoProfile != "dev" {
		t.Fatalf("cargoProfile = %q, want dev", metadata.CargoProfile)
	}
	if metadata.BazelTarget != fixture.target || metadata.LayoutTarget != fixture.target {
		t.Fatalf("metadata targets = %q/%q, want %q", metadata.BazelTarget, metadata.LayoutTarget, fixture.target)
	}
	if metadata.CodeExecPath != filepath.Join(fixture.out, "bin", "codex") {
		t.Fatalf("codeExecPath = %q, want staged codex path", metadata.CodeExecPath)
	}
	if len(metadata.ArchiveFormats) != 0 {
		t.Fatalf("archiveFormats = %v, want none for bazel layout staging", metadata.ArchiveFormats)
	}
	assertRuntimeHelperManifest(t, metadata, fixture.target, []string{"bwrap", "rg", "zsh"})
	if metadata.PackageArchive != nil {
		t.Fatalf("packageArchive = %#v, want nil for bazel layout staging", metadata.PackageArchive)
	}

	verifyCmd := exec.Command(
		"bash",
		fixture.script,
		"--verify-sandbox",
		"--exec-path",
		filepath.Join(fixture.out, "bin", "codex"),
	)
	verifyCmd.Env = append(os.Environ(), "GITHUB_WORKSPACE="+fixture.repoRoot)
	verifyOutput, err := verifyCmd.CombinedOutput()
	if err != nil {
		t.Fatalf("stage-codex-runtime.sh sandbox verification failed: %v\n%s", err, verifyOutput)
	}
}

func TestStageCodexRuntimeScriptMaterializesSymlinkEntrypoint(t *testing.T) {
	fixture := newLinuxStagingFixture(t, "symlink out")
	realEntrypoint := filepath.Join(fixture.root, "bazel-out", "codex-real")
	writeExecutable(t, realEntrypoint, "#!/usr/bin/env sh\nexit 0\n")
	seedEntrypoint := filepath.Join(fixture.seedRoot, "bin", "codex")
	if err := os.Remove(seedEntrypoint); err != nil {
		t.Fatalf("remove seed entrypoint: %v", err)
	}
	if err := os.Symlink(realEntrypoint, seedEntrypoint); err != nil {
		t.Skipf("symlink fixture is not available on this platform: %v", err)
	}

	cmd := exec.Command(
		"bash",
		fixture.script,
		"--out",
		fixture.out,
		"--bazel-target",
		fixture.target,
		"--helper-root",
		fixture.helperRoot,
	)
	cmd.Env = append(
		os.Environ(),
		"CODEX_GO_SDK_TEST_LAYOUT_ROOT="+fixture.seedRoot,
		"GITHUB_WORKSPACE="+fixture.repoRoot,
	)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("stage-codex-runtime.sh failed: %v\n%s", err, output)
	}

	stagedEntrypoint := filepath.Join(fixture.out, "bin", "codex")
	assertNotSymlink(t, stagedEntrypoint)
	verifyCmd := exec.Command(
		"bash",
		fixture.script,
		"--verify-sandbox",
		"--exec-path",
		stagedEntrypoint,
	)
	verifyCmd.Env = append(os.Environ(), "GITHUB_WORKSPACE="+fixture.repoRoot)
	verifyOutput, err := verifyCmd.CombinedOutput()
	if err != nil {
		t.Fatalf("stage-codex-runtime.sh sandbox verification failed: %v\n%s", err, verifyOutput)
	}
}

func TestStageCodexRuntimeScriptRejectsMissingHelperManifest(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	root := t.TempDir()
	seedRoot := filepath.Join(root, "seed")
	helperRoot := filepath.Join(root, "helpers")
	out := filepath.Join(root, "out")

	writeExecutable(t, filepath.Join(seedRoot, "bin", "codex"), "#!/usr/bin/env sh\nexit 0\n")
	writeText(t, filepath.Join(seedRoot, "codex-package.json"), "{}\n")

	cmd := exec.Command(
		"bash",
		filepath.Join(repoRoot, ".github/scripts/stage-codex-runtime.sh"),
		"--out",
		out,
		"--bazel-target",
		"x86_64-unknown-linux-musl",
		"--helper-root",
		helperRoot,
	)
	cmd.Env = append(
		os.Environ(),
		"CODEX_GO_SDK_TEST_LAYOUT_ROOT="+seedRoot,
		"GITHUB_WORKSPACE="+repoRoot,
	)
	output, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("stage-codex-runtime.sh unexpectedly succeeded")
	}
	if !strings.Contains(string(output), "codex-package-helpers.json") {
		t.Fatalf("missing helper manifest error, got: %s", output)
	}
	if _, statErr := os.Stat(out); !os.IsNotExist(statErr) {
		t.Fatalf("staging output should not exist after helper verification failure: %v", statErr)
	}
}

func TestStageCodexRuntimeScriptRequiresReleaseProfileForPackageArchive(t *testing.T) {
	cmd := exec.Command(
		"bash",
		filepath.Join("..", "..", ".github/scripts/stage-codex-runtime.sh"),
		"--release-package-archive",
		"--cargo-profile",
		"dev",
	)
	output, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("stage-codex-runtime.sh unexpectedly accepted non-release package archive mode")
	}
	if !strings.Contains(string(output), "--release-package-archive requires --cargo-profile release") {
		t.Fatalf("missing release profile error, got: %s", output)
	}
}

func TestStageCodexRuntimePowerShellReleaseArchiveMarkers(t *testing.T) {
	content := readRepoText(t, ".github/scripts/stage-codex-runtime.ps1")
	for _, required := range []string{
		"[switch]$ExportEnvironment",
		"[switch]$BootstrapOnly",
		"[switch]$ReleasePackageArchive",
		"[string]$ZstdSource",
		"Get-Command zstd",
		"Set-Item -Path Env:CODEX_EXEC_PATH",
		"Initialize-WindowsBazelBootstrap",
		"BAZEL_OUTPUT_USER_ROOT",
		"BAZEL_REPO_CONTENTS_CACHE",
		"BAZEL_REPOSITORY_CACHE",
		"setup-msvc-env.ps1",
		"compute-bazel-windows-path.ps1",
		"core.longpaths",
		"$script:WindowsMsvcHostPlatform = $true",
		"--host_platform=//:local_windows_msvc",
		"windowsMsvcHostPlatform",
		"packageArchive",
		"requires -CargoProfile release",
		"native Windows app-server package archive lane",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("stage-codex-runtime.ps1 missing %q", required)
		}
	}
}

func TestStageCodexRuntimeScriptThreadsWindowsMsvcHostPlatform(t *testing.T) {
	content := readRepoText(t, ".github/scripts/stage-codex-runtime.sh")
	for _, required := range []string{
		"bazel_host_platform_args+=(--host_platform=//:local_windows_msvc)",
		`-- build "${bazel_host_platform_args[@]}" -- "$label"`,
		`-- cquery "${bazel_host_platform_args[@]}" --output=files "$label"`,
		`bazel build "${bazel_host_platform_args[@]}" "$label"`,
		`bazel cquery "${bazel_host_platform_args[@]}" --output=files "$label"`,
		"--windows-msvc-host-platform requires a Windows target",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("stage-codex-runtime.sh missing MSVC host-platform wiring %q", required)
		}
	}
}

func TestStageCodexRuntimeScriptStagesReleasePackageArchive(t *testing.T) {
	zstdBin, err := exec.LookPath("zstd")
	if err != nil {
		t.Skip("zstd is not installed")
	}
	fixture := newLinuxStagingFixture(t, "archive out with spaces")
	zstdSource := filepath.Join(fixture.root, "tools", "zstd")
	writeExecutable(t, zstdSource, "#!/bin/sh\nexec \""+zstdBin+"\" \"$@\"\n")

	cmd := exec.Command(
		"bash",
		fixture.script,
		"--out",
		fixture.out,
		"--bazel-target",
		fixture.target,
		"--helper-root",
		fixture.helperRoot,
		"--cargo-profile",
		"release",
		"--release-package-archive",
		"--zstd-source",
		zstdSource,
	)
	cmd.Env = append(
		os.Environ(),
		"CODEX_GO_SDK_TEST_LAYOUT_ROOT="+fixture.seedRoot,
		"GITHUB_WORKSPACE="+fixture.repoRoot,
	)
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("stage-codex-runtime.sh package archive mode failed: %v\n%s", err, output)
	}
	scriptContent := readRepoText(t, ".github/scripts/stage-codex-runtime.sh")
	if !strings.Contains(scriptContent, `zstd -dc "$zstd_archive" | tar -xf - -C "$out"`) {
		t.Fatalf("packageArchive staging must extract the tar.zst archive")
	}
	if strings.Contains(scriptContent, `tar -xzf "$gzip_archive" -C "$out"`) {
		t.Fatalf("packageArchive staging must not use the supplemental gzip archive")
	}

	for _, required := range []string{
		"codex-package.json",
		"bin/codex",
		"codex-path/rg",
		"codex-resources/zsh/bin/zsh",
		"codex-resources/bwrap",
	} {
		if _, err := os.Stat(filepath.Join(fixture.out, filepath.FromSlash(required))); err != nil {
			t.Fatalf("missing package-archive staged file %s: %v", required, err)
		}
	}

	metadata := readRuntimeStagingMetadata(t, fixture.out)
	if metadata.RuntimeSource != "packageArchive" {
		t.Fatalf("runtimeSource = %q, want packageArchive", metadata.RuntimeSource)
	}
	if metadata.CargoProfile != "release" {
		t.Fatalf("cargoProfile = %q, want release", metadata.CargoProfile)
	}
	if metadata.ZstdSource != "stage5gMaterialized" {
		t.Fatalf("zstdSource = %q, want stage5gMaterialized", metadata.ZstdSource)
	}
	if strings.Join(metadata.ArchiveFormats, ",") != "tar.gz,tar.zst" {
		t.Fatalf("archiveFormats = %v, want tar.gz and tar.zst", metadata.ArchiveFormats)
	}
	if metadata.PackageArchive == nil {
		t.Fatalf("packageArchive metadata missing")
	}
	if metadata.PackageArchive.Target != fixture.target {
		t.Fatalf("packageArchive target = %q, want %q", metadata.PackageArchive.Target, fixture.target)
	}
	if !strings.HasSuffix(metadata.PackageArchive.Path, ".tar.zst") {
		t.Fatalf("packageArchive path = %q, want tar.zst", metadata.PackageArchive.Path)
	}
	if !strings.HasSuffix(metadata.PackageArchive.GzipPath, ".tar.gz") {
		t.Fatalf("packageArchive gzipPath = %q, want tar.gz", metadata.PackageArchive.GzipPath)
	}
	if strings.Join(metadata.PackageArchive.ArchiveFormats, ",") != "tar.gz,tar.zst" {
		t.Fatalf("packageArchive archiveFormats = %v, want tar.gz and tar.zst", metadata.PackageArchive.ArchiveFormats)
	}
	assertRuntimeHelperManifest(t, metadata, fixture.target, []string{"bwrap", "rg", "zsh"})
}

func TestStageCodexRuntimeScriptsUseVerifiedHelpersOnly(t *testing.T) {
	for _, path := range []string{
		".github/scripts/stage-codex-runtime.sh",
		".github/scripts/stage-codex-runtime.ps1",
	} {
		content := readRepoText(t, path)
		for _, required := range []string{
			"codex_package.materialize_helpers",
			"--verify-only",
			"codex-package-helpers.json",
		} {
			if !strings.Contains(content, required) {
				t.Fatalf("%s missing %q", path, required)
			}
		}
		for _, forbidden := range []string{
			".github/workflows/zstd",
			"fetch_dotslash",
			"download_archive",
			"curl ",
			"wget ",
			"build-codex-package-archive.sh",
		} {
			if strings.Contains(content, forbidden) {
				t.Fatalf("%s must not fetch helpers during staging; found %q", path, forbidden)
			}
		}
	}
}

type linuxStagingFixture struct {
	repoRoot   string
	root       string
	seedRoot   string
	helperRoot string
	out        string
	target     string
	script     string
}

type runtimeStagingMetadata struct {
	ArchiveFormats           []string               `json:"archiveFormats"`
	BazelTarget              string                 `json:"bazelTarget"`
	CargoProfile             string                 `json:"cargoProfile"`
	CodeExecPath             string                 `json:"codeExecPath"`
	HelperManifest           *runtimeHelperManifest `json:"helperManifest"`
	LayoutTarget             string                 `json:"layoutTarget"`
	PackageArchive           *runtimePackageArchive `json:"packageArchive"`
	RuntimeSource            string                 `json:"runtimeSource"`
	WindowsMsvcHostPlatform  bool                   `json:"windowsMsvcHostPlatform"`
	WindowsReleaseShapedMsvc bool                   `json:"windowsReleaseShapedMsvc"`
	ZstdSource               string                 `json:"zstdSource"`
}

type runtimeHelperManifest struct {
	Files  []string `json:"files"`
	Path   string   `json:"path"`
	Target string   `json:"target"`
}

type runtimePackageArchive struct {
	ArchiveFormats           []string `json:"archiveFormats"`
	GzipPath                 string   `json:"gzipPath"`
	Path                     string   `json:"path"`
	Target                   string   `json:"target"`
	WindowsMsvcHostPlatform  bool     `json:"windowsMsvcHostPlatform"`
	WindowsReleaseShapedMsvc bool     `json:"windowsReleaseShapedMsvc"`
}

func newLinuxStagingFixture(t *testing.T, outName string) linuxStagingFixture {
	t.Helper()
	repoRoot := filepath.Join("..", "..")
	root := t.TempDir()
	seedRoot := filepath.Join(root, "seed")
	helperRoot := filepath.Join(root, "helpers")
	target := "x86_64-unknown-linux-musl"

	writeLinuxSeed(t, seedRoot, target)
	writeLinuxHelpers(t, helperRoot, target)

	return linuxStagingFixture{
		repoRoot:   repoRoot,
		root:       root,
		seedRoot:   seedRoot,
		helperRoot: helperRoot,
		out:        filepath.Join(root, outName),
		target:     target,
		script:     filepath.Join(repoRoot, ".github/scripts/stage-codex-runtime.sh"),
	}
}

func writeLinuxSeed(t *testing.T, seedRoot string, target string) {
	t.Helper()
	writeExecutable(t, filepath.Join(seedRoot, "bin", "codex"), "#!/usr/bin/env sh\nexit 0\n")
	writeText(t, filepath.Join(seedRoot, "codex-package.json"), `{
  "entrypoint": "bin/codex",
  "layoutVersion": 1,
  "pathDir": "codex-path",
  "resourcesDir": "codex-resources",
  "target": "`+target+`",
  "variant": "codex",
  "version": "0.0.0"
}
`)
}

func writeLinuxHelpers(t *testing.T, helperRoot string, target string) {
	t.Helper()
	helperTargetRoot := filepath.Join(helperRoot, target)
	helperFiles := map[string]string{
		"rg":    "#!/usr/bin/env sh\necho rg\n",
		"zsh":   "#!/usr/bin/env sh\necho zsh\n",
		"bwrap": "#!/usr/bin/env sh\necho bwrap\n",
	}
	for name, content := range helperFiles {
		writeExecutable(t, filepath.Join(helperTargetRoot, name), content)
	}
	writeHelperManifest(t, helperTargetRoot, target, helperFiles)
}

func readRuntimeStagingMetadata(t *testing.T, out string) runtimeStagingMetadata {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(out, "codex-go-sdk-runtime-staging.json"))
	if err != nil {
		t.Fatalf("read runtime staging metadata: %v", err)
	}
	var metadata runtimeStagingMetadata
	if err := json.Unmarshal(data, &metadata); err != nil {
		t.Fatalf("unmarshal runtime staging metadata: %v", err)
	}
	return metadata
}

func assertRuntimeHelperManifest(t *testing.T, metadata runtimeStagingMetadata, target string, files []string) {
	t.Helper()
	if metadata.HelperManifest == nil {
		t.Fatalf("helperManifest metadata missing")
	}
	if metadata.HelperManifest.Target != target {
		t.Fatalf("helperManifest target = %q, want %q", metadata.HelperManifest.Target, target)
	}
	if strings.Join(metadata.HelperManifest.Files, ",") != strings.Join(files, ",") {
		t.Fatalf("helperManifest files = %v, want %v", metadata.HelperManifest.Files, files)
	}
	if !strings.HasSuffix(metadata.HelperManifest.Path, filepath.Join(target, "codex-package-helpers.json")) {
		t.Fatalf("helperManifest path = %q, want target manifest path", metadata.HelperManifest.Path)
	}
}

func assertNotSymlink(t *testing.T, path string) {
	t.Helper()
	info, err := os.Lstat(path)
	if err != nil {
		t.Fatalf("lstat staged entrypoint: %v", err)
	}
	if info.Mode()&os.ModeSymlink != 0 {
		t.Fatalf("staged entrypoint must be a real executable, not a symlink: %s", path)
	}
}

func writeHelperManifest(t *testing.T, targetRoot string, target string, helpers map[string]string) {
	t.Helper()
	type helperEntry struct {
		RelativePath string `json:"relativePath"`
		SHA256       string `json:"sha256"`
		SizeBytes    int64  `json:"sizeBytes"`
	}
	entries := map[string]helperEntry{}
	for name := range helpers {
		path := filepath.Join(targetRoot, name)
		data, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read helper %s: %v", name, err)
		}
		sum := sha256.Sum256(data)
		entries[name] = helperEntry{
			RelativePath: name,
			SHA256:       hex.EncodeToString(sum[:]),
			SizeBytes:    int64(len(data)),
		}
	}
	manifest := map[string]any{
		"generatedBy":   "codex_package.materialize_helpers",
		"helpers":       entries,
		"schemaVersion": 1,
		"target":        target,
	}
	data, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatalf("marshal helper manifest: %v", err)
	}
	writeText(t, filepath.Join(targetRoot, "codex-package-helpers.json"), string(data)+"\n")
}

func writeExecutable(t *testing.T, path string, content string) {
	t.Helper()
	writeText(t, path, content)
	if err := os.Chmod(path, 0o755); err != nil {
		t.Fatalf("chmod %s: %v", path, err)
	}
}

func writeText(t *testing.T, path string, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir %s: %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}
