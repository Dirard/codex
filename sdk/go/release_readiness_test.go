package codex

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestReleaseReadiness(t *testing.T) {
	readme := readRepoText(t, "sdk/go/README.md")
	release := readRepoText(t, "sdk/go/RELEASE.md")
	workflow := readRepoText(t, ".github/workflows/go-sdk-release-readiness.yml")
	sdkWorkflow := readRepoText(t, ".github/workflows/sdk.yml")

	for _, required := range []string{
		"github.com/openai/codex/sdk/go",
		"github.com/openai/codex/sdk/go/protocol",
		"github.com/openai/codex/sdk/go/v2",
		"sdk/go/vX.Y.Z",
	} {
		if !strings.Contains(readme, required) && !strings.Contains(release, required) {
			t.Fatalf("release docs missing %q", required)
		}
	}

	for _, required := range []string{
		"go get github.com/openai/codex/sdk/go@v1.2.3",
		"Do not delete, overwrite, force-push, or retag",
		"publish a higher patch version",
		"retract",
		"release note",
	} {
		if !strings.Contains(release, required) {
			t.Fatalf("sdk/go/RELEASE.md missing %q", required)
		}
	}

	stageScript := filepath.Join("..", "..", ".github", "scripts", "stage-codex-runtime.sh")
	info, err := os.Stat(stageScript)
	if err != nil {
		t.Fatalf("stat stage-codex-runtime.sh: %v", err)
	}
	if info.Mode()&0o111 == 0 {
		t.Fatalf("stage-codex-runtime.sh must be executable")
	}

	for _, required := range []string{
		"workflow_call:",
		"checkout_ref:",
		"required: true",
		"validate_synthetic_tags:",
		"default: true",
		"workflow_dispatch:",
		`- "sdk/go/v*"`,
		`github.event_name == 'workflow_call' || github.repository == 'openai/codex' || github.event_name == 'workflow_dispatch'`,
		"actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2",
		"fetch-depth: 0",
		"ref: ${{ inputs.checkout_ref || github.ref }}",
		"actions/setup-go@924ae3a1cded613372ab5595356fb5720e22ba16 # v6",
		`go-version: "1.25"`,
		`module_version="$(git rev-parse HEAD)"`,
		`module_import="$(awk '/^module / { print $2; exit }' sdk/go/go.mod)"`,
		`git rev-parse "${GITHUB_REF}^{commit}"`,
		`sdk/go/v0.99.0-go-sdk-ci`,
		`sdk/go/v1.99.0-go-sdk-ci`,
		`sdk/go/v1.99.1-go-sdk-ci-annotated`,
		`sdk/go/v2.99.0-go-sdk-ci`,
		`tag -a "${release_tag}"`,
		`rev-parse "${release_tag}^{commit}"`,
		`go -C sdk/go mod edit -module github.com/openai/codex/sdk/go/v2`,
		`go -C sdk/go test ./...`,
		`synthetic Go SDK v2 module path`,
		"GIT_CONFIG_GLOBAL",
		"git config --global --add",
		"git config --global --get-all",
		"GIT_TRACE=1",
		"file://${bare_remote}",
		"GIT_ALLOW_PROTOCOL=file:https:ssh",
		`go_get_trace="${trace_dir}/go-get-trace.log"`,
		`GIT_TRACE="${go_get_trace}"`,
		`grep -F "${bare_remote}" "${go_get_trace}"`,
		`go get "${module_import}@${module_version}"`,
		`release_go_get_trace="${trace_dir}/go-get-${release_version}.log"`,
		`GIT_TRACE="${release_go_get_trace}"`,
		`grep -F "${bare_remote}" "${release_go_get_trace}"`,
		`go get "${release_import}@${release_version}"`,
		`codex "${module_import}"`,
		`_ "${module_import}/protocol"`,
		`codex "${release_import}"`,
		`_ "${release_import}/protocol"`,
		"./.github/actions/check-clean-worktree",
	} {
		if !strings.Contains(workflow, required) {
			t.Fatalf("go-sdk-release-readiness workflow missing %q", required)
		}
	}

	for _, forbidden := range []string{
		"github.sha",
		"go mod edit -replace github.com/openai/codex/sdk/go",
		"secrets: inherit",
	} {
		if strings.Contains(workflow, forbidden) {
			t.Fatalf("go-sdk-release-readiness workflow must not contain %q", forbidden)
		}
	}

	for _, required := range []string{
		"  go-sdk-release-readiness:",
		"needs:",
		"- go-sdk",
		"uses: ./.github/workflows/go-sdk-release-readiness.yml",
		"checkout_ref: ${{ github.event_name == 'pull_request' && github.event.pull_request.head.sha || github.sha }}",
		"validate_synthetic_tags: true",
	} {
		if !strings.Contains(sdkWorkflow, required) {
			t.Fatalf("sdk.yml release-readiness caller missing %q", required)
		}
	}
	if strings.Contains(sdkWorkflow, "secrets: inherit") {
		t.Fatalf("sdk.yml release-readiness caller must not inherit secrets")
	}
}
