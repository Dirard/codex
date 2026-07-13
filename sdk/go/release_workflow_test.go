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
		"target: x86_64-apple-darwin\n            bundle: primary\n            artifact_name: x86_64-apple-darwin\n            binaries: \"codex codex-code-mode-host codex-responses-api-proxy\"\n            build_dmg: \"true\"\n            runs_on: macos-15-large",
		"target: x86_64-apple-darwin\n            bundle: app-server\n            artifact_name: x86_64-apple-darwin-app-server\n            binaries: \"codex-app-server codex-code-mode-host\"\n            build_dmg: \"false\"\n            runs_on: macos-15-large",
		"target: x86_64-apple-darwin\n            bundle: primary\n            artifact_name: x86_64-apple-darwin\n            binaries: \"codex codex-code-mode-host codex-responses-api-proxy\"\n            verify_dmg: \"true\"\n            runs_on: macos-15-large",
		"target: x86_64-apple-darwin\n            bundle: app-server\n            artifact_name: x86_64-apple-darwin-app-server\n            binaries: \"codex-app-server codex-code-mode-host\"\n            verify_dmg: \"false\"\n            runs_on: macos-15-large",
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
		"codex-code-mode-host",
		"codex-windows-sandbox-setup",
		"Bundle required runtime binaries and sandbox helpers into the",
		`host_src="$dest/codex-code-mode-host-${target}.exe"`,
		`runner_src="$dest/codex-command-runner-${target}.exe"`,
		`setup_src="$dest/codex-windows-sandbox-setup-${target}.exe"`,
		`cp "$host_src" "$bundle_dir/codex-code-mode-host.exe"`,
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

func TestReleaseChecksumManifestCoversZstdPackageArchives(t *testing.T) {
	workflow := readRepoText(t, ".github/workflows/rust-release.yml")
	releaseJob := workflowJobSection(t, workflow, "release")
	checksumScript := readRepoText(t, ".github/scripts/write-codex-package-checksums.sh")

	for _, required := range []string{
		"Add Codex package checksum manifest",
		"bash .github/scripts/write-codex-package-checksums.sh",
		"--dist dist",
		"--manifest dist/codex-package_SHA256SUMS",
	} {
		if !strings.Contains(releaseJob, required) {
			t.Fatalf("rust-release.yml release job checksum manifest missing %q", required)
		}
	}

	for _, required := range []string{
		"codex-package_SHA256SUMS",
		"-name 'codex-package-*.tar.gz'",
		"-name 'codex-package-*.tar.zst'",
		"-name 'codex-app-server-package-*.tar.gz'",
		"-name 'codex-app-server-package-*.tar.zst'",
		`sha256sum "$archive"`,
		`awk -v name="$(basename "$archive")"`,
	} {
		if !strings.Contains(checksumScript, required) {
			t.Fatalf("write-codex-package-checksums.sh missing %q", required)
		}
	}
}

func TestShippingReleaseReadinessWorkflowArtifactMetadata(t *testing.T) {
	workflow := readRepoText(t, ".github/workflows/go-sdk-shipping-release-readiness.yml")
	sdkWorkflow := readRepoText(t, ".github/workflows/sdk.yml")
	rustReleaseWorkflow := readRepoText(t, ".github/workflows/rust-release.yml")

	for _, required := range []string{
		"name: go-sdk-shipping-release-readiness",
		"workflow_call:",
		"checkout_ref:",
		"evidence_source:",
		"ci-preflight",
		"rust-release",
		"Download actual Rust release Linux artifacts",
		`pattern: "{x86_64,aarch64}-unknown-linux-musl{,-app-server}"`,
		"source-preflight",
		"collect-linux-release",
		"go_sdk_shipping_release_readiness.py",
		"codex-package_SHA256SUMS",
		"write-codex-package-checksums.sh",
		"github.run_id",
		"github.run_attempt",
		"github.ref_name",
		"shipping-release-readiness-metadata",
		"actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2",
		"actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1",
		"persist-credentials: false",
	} {
		if !strings.Contains(workflow, required) {
			t.Fatalf("go-sdk-shipping-release-readiness workflow source anchors missing %q", required)
		}
	}

	for _, required := range []string{
		"go-sdk-linux-helper-roots:",
		"labels: ${{ matrix.runner_label }}",
		"Upload release-owned helper root",
		"Download pre-produced Linux helper root",
		"go-sdk-package-helper-root-${TARGET}.tar.gz",
		"tar -xzf \"$helper_archive\"",
		"CODEX_GO_SDK_HELPER_ROOT_SOURCE=release-owned-artifact",
		"source\": \"preProducedStage5GArtifact",
		"write-codex-package-checksums.sh",
		"packageArchiveArtifact",
		"seedProvenance",
		"bazelCompilationMode",
		"Verify Linux sandbox readiness",
		"go-sdk-linux-shipping-release-readiness:",
		"always() && !cancelled()",
		"uses: ./.github/workflows/go-sdk-shipping-release-readiness.yml",
		"evidence_source: ci-preflight",
	} {
		if !strings.Contains(sdkWorkflow, required) {
			t.Fatalf("sdk workflow Linux release evidence missing %q", required)
		}
	}

	for _, required := range []string{
		"go-sdk-linux-verification:",
		"commit_sha: ${{ steps.validate-tag.outputs.commit_sha }}",
		`commit_sha="$(git rev-parse "${GITHUB_SHA}^{commit}")"`,
		"ref: ${{ needs.tag-check.outputs.commit_sha }}",
		"Run full Linux Go SDK verification",
		"Check Rust-owned Go SDK protocol sources",
		"write_go_sdk_manifest",
		"just write-app-server-schema --check",
		"write_schema_fixtures",
		"just test -p codex-app-server-protocol",
		"go test ./... -count=1",
		"go test -race ./... -count=1",
		"go vet ./...",
		"Run Go SDK smoke tests against Linux release package",
		"stage-codex-runtime.sh\" --verify-sandbox --exec-path \"$runtime_path\"",
		"sandboxSmoke",
		"cache-dependency-path: sdk/go/go.mod",
		"go-sdk-release-smoke-${TARGET}.json",
		"CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1",
		"test ./... -list",
		"for smoke_test in",
		"go-sdk-linux-shipping-release-readiness:",
		"needs.go-sdk-linux-verification.result == 'success'",
		"verification_job: go-sdk-linux-verification",
		"verification_conclusion: ${{ needs.go-sdk-linux-verification.result }}",
		"checkout_ref: ${{ needs.tag-check.outputs.commit_sha }}",
		"verification_commit_sha: ${{ needs.tag-check.outputs.commit_sha }}",
		"evidence_source: rust-release",
		"needs.go-sdk-linux-shipping-release-readiness.result == 'success'",
		"Download preliminary Linux release readiness metadata",
		"finalize-linux-release",
		"dist/go-sdk-linux-release-readiness.json",
	} {
		if !strings.Contains(rustReleaseWorkflow, required) {
			t.Fatalf("rust-release Linux Go SDK gate missing %q", required)
		}
	}
	if strings.Contains(rustReleaseWorkflow, "cache-dependency-path: sdk/go/go.sum") {
		t.Fatal("rust-release Linux Go setup must not use the absent sdk/go/go.sum cache dependency")
	}

	for _, forbidden := range []string{
		"workflow_dispatch:",
		"build-fixture-artifacts",
		"--fixture-substitutions",
		"nonPublishingFixtureBinaries",
		"disabledSigningAndNotarization",
		"aarch64-apple-darwin",
		"x86_64-apple-darwin",
		"pc-windows-msvc",
		"secrets: inherit",
		"contents: write",
		"softprops/action-gh-release",
		"dotslash-publish-release",
		"GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}",
	} {
		if strings.Contains(workflow, forbidden) {
			t.Fatalf("go-sdk-shipping-release-readiness workflow must stay validation-only and secretless; found %q", forbidden)
		}
	}
}

func TestStageSevenLinuxOnlyValidationBranchesBeforeCrossPlatformEvidence(t *testing.T) {
	verification := readRepoText(t, "docs/superpowers/plans/2026-07-02-go-sdk-full/stage-07-final-verification.md")
	releaseReadinessRequirement := strings.Index(verification, `${CODEX_GO_SDK_RELEASE_READINESS_RUN_ID:?set to the successful go-sdk-release-readiness.yml run id for the reviewed commit}`)
	releaseReadinessValidation := strings.Index(verification, `verify_run "${CODEX_GO_SDK_RELEASE_READINESS_RUN_ID}" "go-sdk-release-readiness"`)
	linuxBranch := strings.Index(verification, `if [[ "${linux_only}" == "true" ]]; then`)
	legacyValidation := strings.Index(verification, `python3 - "${sdk_metadata}"`)
	if releaseReadinessRequirement == -1 || releaseReadinessValidation == -1 || linuxBranch == -1 || legacyValidation == -1 {
		t.Fatal("stage 7 must require module release readiness and contain Linux-only and legacy SDK evidence validation branches")
	}
	if releaseReadinessRequirement > linuxBranch || releaseReadinessValidation > linuxBranch {
		t.Fatal("stage 7 must validate Go module release readiness before choosing platform evidence")
	}
	if !strings.Contains(verification, "go-sdk-synthetic-tag-validation=v0,v1,annotated-v1,v2") {
		t.Fatal("stage 7 must require immutable log evidence that all synthetic Go module tag lanes ran")
	}
	if linuxBranch > legacyValidation {
		t.Fatal("stage 7 must choose Linux-only validation before evaluating cross-platform SDK evidence")
	}
	branchBody := verification[linuxBranch:legacyValidation]
	if !strings.Contains(branchBody, "validate-linux-release") ||
		!strings.Contains(branchBody, "else\nverify_run \"${CODEX_GO_SDK_CI_RUN_ID}\" \"blocking-ci\"") {
		t.Fatal("stage 7 Linux-only branch must validate published Linux evidence before the legacy branch")
	}
	if !strings.Contains(verification, `verify_run "${CODEX_GO_SDK_CI_RUN_ID}" "blocking-ci"`) {
		t.Fatal("stage 7 must validate the blocking-ci caller run that owns reusable sdk jobs")
	}
	for _, linuxJob := range []string{"sdk / Go SDK - linux-musl", "sdk / Go SDK - linux-arm64-musl"} {
		if !strings.Contains(verification, linuxJob) {
			t.Fatalf("stage 7 must require successful nested Linux SDK job %q", linuxJob)
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
