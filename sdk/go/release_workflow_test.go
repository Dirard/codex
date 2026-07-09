package codex

import (
	"strings"
	"testing"
)

func TestMacosReleaseWorkflowRunnerShape(t *testing.T) {
	workflow := readRepoText(t, ".github/workflows/rust-release.yml")

	for _, required := range []string{
		"runner: macos-15-large\n            target: x86_64-apple-darwin\n            bundle: primary",
		"runner: macos-15-large\n            target: x86_64-apple-darwin\n            bundle: app-server",
		"package-macos:",
		"finalize-macos:",
		"runs-on: ${{ matrix.runs_on }}",
		"Verify macOS x64 runner",
		`test "$(uname -m)" = "x86_64"`,
		"target: x86_64-apple-darwin\n            bundle: primary\n            artifact_name: x86_64-apple-darwin\n            binaries: \"codex codex-responses-api-proxy\"\n            build_dmg: \"true\"\n            runs_on: macos-15-large",
		"target: x86_64-apple-darwin\n            bundle: app-server\n            artifact_name: x86_64-apple-darwin-app-server\n            binaries: \"codex-app-server\"\n            build_dmg: \"false\"\n            runs_on: macos-15-large",
		"target: x86_64-apple-darwin\n            bundle: primary\n            artifact_name: x86_64-apple-darwin\n            binaries: \"codex codex-responses-api-proxy\"\n            verify_dmg: \"true\"\n            runs_on: macos-15-large",
		"target: x86_64-apple-darwin\n            bundle: app-server\n            artifact_name: x86_64-apple-darwin-app-server\n            binaries: \"codex-app-server\"\n            verify_dmg: \"false\"\n            runs_on: macos-15-large",
	} {
		if !strings.Contains(workflow, required) {
			t.Fatalf("rust-release.yml missing %q", required)
		}
	}

	if strings.Count(workflow, "Verify macOS x64 runner") < 3 {
		t.Fatalf("rust-release.yml must verify macOS x64 runner architecture in build, package, and final verification jobs")
	}
	if strings.Contains(workflow, "runner: macos-15-xlarge\n            target: x86_64-apple-darwin") {
		t.Fatalf("rust-release.yml must not build x86_64-apple-darwin on macos-15-xlarge")
	}

	for _, job := range []string{"package-macos", "finalize-macos"} {
		section := workflowJobSection(t, workflow, job)
		if !strings.Contains(section, "runs-on: ${{ matrix.runs_on }}") {
			t.Fatalf("%s must run on matrix.runs_on", job)
		}
		for _, row := range strings.Split(section, "\n          - target: ") {
			if strings.Contains(row, "x86_64-apple-darwin") && strings.Contains(row, "runs_on: macos-15-xlarge") {
				t.Fatalf("%s must not run x86_64-apple-darwin rows on macos-15-xlarge", job)
			}
		}
	}
}

func TestWindowsReleaseZipIncludesSandboxHelpers(t *testing.T) {
	workflow := readRepoText(t, ".github/workflows/rust-release-windows.yml")

	for _, required := range []string{
		"x86_64-pc-windows-msvc",
		"aarch64-pc-windows-msvc",
		"codex-command-runner",
		"codex-windows-sandbox-setup",
		"Bundle the sandbox helper binaries into the main codex zip",
		`runner_src="$dest/codex-command-runner-${target}.exe"`,
		`setup_src="$dest/codex-windows-sandbox-setup-${target}.exe"`,
		`cp "$runner_src" "$bundle_dir/codex-command-runner.exe"`,
		`cp "$setup_src" "$bundle_dir/codex-windows-sandbox-setup.exe"`,
		`7z a "$repo_root/$dest/${base}.zip" .`,
	} {
		if !strings.Contains(workflow, required) {
			t.Fatalf("rust-release-windows.yml missing %q", required)
		}
	}

	for _, forbidden := range []string{
		"falling back to single-binary zip",
		"warning: missing sandbox binaries",
	} {
		if strings.Contains(workflow, forbidden) {
			t.Fatalf("rust-release-windows.yml must fail closed instead of allowing %q", forbidden)
		}
	}
}

func workflowJobSection(t *testing.T, workflow string, job string) string {
	t.Helper()

	lines := strings.Split(workflow, "\n")
	start := -1
	for i, line := range lines {
		if line == "  "+job+":" {
			start = i
			break
		}
	}
	if start == -1 {
		t.Fatalf("workflow missing job %s", job)
	}

	end := len(lines)
	for i := start + 1; i < len(lines); i++ {
		line := lines[i]
		if strings.HasPrefix(line, "  ") && len(line) > 2 && line[2] != ' ' {
			end = i
			break
		}
	}
	return strings.Join(lines[start:end], "\n")
}
