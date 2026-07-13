package codex

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

const docsCoverageWindowLines = 40

var docsOwnerFiles = map[string]string{
	"README accounts":         "README.md",
	"README config":           "README.md",
	"README thread lifecycle": "README.md",
	"examples/login_account":  "examples/login_account/main.go",
	"examples/raw_protocol":   "examples/raw_protocol/main.go",
	"examples/resources":      "examples/resources/main.go",
	"examples/reviews":        "examples/reviews/main.go",
	"examples/run":            "examples/run/main.go",
	"examples/skills_hooks":   "examples/skills_hooks/main.go",
	"examples/streaming":      "examples/streaming/main.go",
}

var serverHandlerDocsOwnerFiles = map[string]string{
	"README server handlers":              "README.md",
	"README experimental server handlers": "README.md",
	"examples/server_handlers":            "examples/server_handlers/main.go",
}

var publicDocsFiles = map[string]string{
	"README":                 "README.md",
	"examples/login_account": "examples/login_account/main.go",
	"examples/raw_protocol":  "examples/raw_protocol/main.go",
	"examples/resources":     "examples/resources/main.go",
	"examples/reviews":       "examples/reviews/main.go",
	"examples/run":           "examples/run/main.go",
	"examples/server":        "examples/server_handlers/main.go",
	"examples/skills_hooks":  "examples/skills_hooks/main.go",
	"examples/streaming":     "examples/streaming/main.go",
	"examples/test_harness":  "examples/test_harness/main.go",
}

var callsiteTokenPattern = regexp.MustCompile(`(?:[A-Za-z_][A-Za-z0-9_]*\.)+[A-Za-z_][A-Za-z0-9_]*\(`)

func TestResourceDocsCoverage(t *testing.T) {
	contents := readDocsTargets(t, docsOwnerFiles)
	for _, row := range generatedResourceCoverage {
		if row.SDKVisibility != "public" {
			continue
		}
		target, ok := docsOwnerFiles[row.DocsExampleOwner]
		if !ok {
			t.Fatalf("%s has unsupported docs/example owner %q", row.Method, row.DocsExampleOwner)
		}
		assertAdjacentDocsCoverage(
			t,
			target,
			"codex-go-sdk-docs:"+row.Method,
			"codex-go-sdk-resource:"+row.ResourceOwner,
			resourceDocsEvidenceTokens(row),
			contents[target],
		)
	}
}

func TestServerHandlerDocsCoverage(t *testing.T) {
	contents := readDocsTargets(t, serverHandlerDocsOwnerFiles)
	publicDocs := strings.Join(mapValues(readDocsTargets(t, publicDocsFiles)), "\n")
	for _, row := range protocol.ServerRequestMetadataByMethod {
		assertTestOwnerExists(t, row.Method, row.UnitTestOwner)
		switch row.Visibility {
		case "sdk-public", "experimental-public":
			target, ok := serverHandlerDocsOwnerFiles[row.DocsExampleOwner]
			if !ok {
				t.Fatalf("%s has unsupported server handler docs owner %q", row.Method, row.DocsExampleOwner)
			}
			assertAdjacentHandlerDocsCoverage(
				t,
				target,
				"codex-go-sdk-handler-docs:"+row.Method,
				row.Capability,
				[]string{"protocol." + row.ParamsType, "protocol." + row.ResponseType},
				contents[target],
			)
		case "compatibility-only":
			if row.GeneratedOnlyException == "" || row.ReviewNote == "" {
				t.Fatalf("%s compatibility-only handler missing internal exception/review note metadata", row.Method)
			}
			if strings.Contains(publicDocs, "codex-go-sdk-handler-docs:"+row.Method) ||
				strings.Contains(publicDocs, row.Method) ||
				strings.Contains(publicDocs, row.Capability) {
				t.Fatalf("%s compatibility-only handler is documented in public docs/examples", row.Method)
			}
		default:
			t.Fatalf("%s has unknown server handler visibility %q", row.Method, row.Visibility)
		}
	}
}

func TestRequiredExamplesExist(t *testing.T) {
	for _, file := range []string{
		"examples/run/main.go",
		"examples/streaming/main.go",
		"examples/login_account/main.go",
		"examples/server_handlers/main.go",
		"examples/resources/main.go",
		"examples/reviews/main.go",
		"examples/skills_hooks/main.go",
		"examples/raw_protocol/main.go",
		"examples/test_harness/main.go",
	} {
		data, err := os.ReadFile(file)
		if err != nil {
			t.Fatalf("read %s: %v", file, err)
		}
		if !strings.Contains(string(data), "package main") {
			t.Fatalf("%s is not a compileable Go example", file)
		}
	}
}

func TestTestHarnessExampleUsesCODEXExecPath(t *testing.T) {
	content := readDocsFile(t, "examples/test_harness/main.go")
	for _, required := range []string{
		"CODEX_EXEC_PATH",
		"os.Getenv(",
		"codexHome string",
		"CodexPath: codexPath",
		`"CODEX_HOME": codexHome`,
		"Transport: injectedTransport{}",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("examples/test_harness/main.go missing test harness evidence %q", required)
		}
	}
	if strings.Contains(content, "/tmp/codex-go-sdk-home") {
		t.Fatalf("examples/test_harness/main.go must not document a shared fixed CODEX_HOME")
	}
}

func TestLoginAccountExampleAvoidsAPIKeyLiterals(t *testing.T) {
	content := readDocsFile(t, "examples/login_account/main.go")
	if strings.Contains(content, "sk-") {
		t.Fatalf("examples/login_account/main.go must not contain API-key-shaped literals")
	}
	if !strings.Contains(content, "LoginWithAPIKey(ctx, apiKey)") {
		t.Fatalf("examples/login_account/main.go should pass an API key supplied by the caller")
	}
	if !strings.Contains(content, "LoginWithAmazonBedrock(ctx, apiKey, bedrockRegion)") {
		t.Fatalf("examples/login_account/main.go should show typed Amazon Bedrock login")
	}
}

func TestWindowsSandboxExampleHandlesUnsupportedPlatform(t *testing.T) {
	content := readDocsFile(t, "examples/resources/main.go")
	for _, required := range []string{
		"WindowsSandboxReadinessUnsupportedPlatform",
		"UnsupportedPlatformError",
		"errors.As(",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("examples/resources/main.go missing Windows sandbox unsupported-platform evidence %q", required)
		}
	}
}

func TestServerHandlerExampleRegistersHandlers(t *testing.T) {
	for file, required := range map[string][]string{
		"README.md": {
			"ClientConfig",
			"Handlers:  handlers",
		},
		"examples/server_handlers/main.go": {
			"codex.ClientConfig",
			"Handlers:  handlers()",
		},
	} {
		content := readDocsFile(t, file)
		for _, token := range required {
			if !strings.Contains(content, token) {
				t.Fatalf("%s missing server handler registration evidence %q", file, token)
			}
		}
	}
}

func TestREADMERequiredWorkflowLinks(t *testing.T) {
	content := readDocsFile(t, "README.md")
	for _, required := range []string{
		"examples/run",
		"examples/streaming",
		"examples/login_account",
		"examples/server_handlers",
		"examples/resources",
		"examples/reviews",
		"examples/skills_hooks",
		"examples/raw_protocol",
		"examples/test_harness",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("README.md missing workflow example link %q", required)
		}
	}
}

func TestREADMEClientLimitDefaults(t *testing.T) {
	content := readDocsFile(t, "README.md")
	for _, required := range []string{
		"MaxFrameBytes",
		"16 MiB",
		"MaxLocalInputBytes",
		"MaxAdditionalContextEntries",
		"8",
		"MaxAdditionalContextKeyBytes",
		"128",
		"MaxAdditionalContextValueBytes",
		"1000",
		"MaxAdditionalContextTotalBytes",
		"4096",
		"ResourceStreamQueue",
		"256",
		"ResourceStreamQueueBytes",
		"PendingTurnQueue",
		"512",
		"PendingTurnMap",
		"PendingNotificationBytes",
		"GlobalSubscriberQueue",
		"GlobalSubscriberQueueBytes",
		"64 MiB",
		"HandlerConcurrency",
		"HandlerQueue",
		"HandlerTimeout",
		"60s",
		"StderrRingBytes",
		"64 KiB",
		"LifecycleInactivityTimeout",
		"5m",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("README.md missing documented ClientLimits default %q", required)
		}
	}
}

func TestREADMEPublicAPIBoundary(t *testing.T) {
	content := readDocsFile(t, "README.md")
	for _, required := range []string{
		"root `codex` package",
		"`protocol` package",
		"protocol digest",
		"`internal/`",
		"not public API",
	} {
		if !strings.Contains(content, required) {
			t.Fatalf("README.md missing public API boundary text %q", required)
		}
	}
}

func TestStructuredOutputExampleUsesOutputSchema(t *testing.T) {
	data, err := os.ReadFile(filepath.FromSlash("examples/run/main.go"))
	if err != nil {
		t.Fatalf("read examples/run/main.go: %v", err)
	}
	content := string(data)
	for _, required := range []string{"OutputSchema:", "codex.JSONSchema", "codex.ObjectSchema"} {
		if !strings.Contains(content, required) {
			t.Fatalf("examples/run/main.go missing structured output usage %q", required)
		}
	}
}

func assertAdjacentDocsCoverage(t *testing.T, file string, marker string, resourceMarker string, evidenceTokens []string, content string) {
	t.Helper()
	for _, window := range markerWindows(content, marker) {
		if strings.Contains(window, resourceMarker) && containsAll(window, evidenceTokens) {
			return
		}
	}
	t.Fatalf("%s missing adjacent docs coverage for %q with resource marker %q and evidence %v", file, marker, resourceMarker, evidenceTokens)
}

func assertAdjacentHandlerDocsCoverage(t *testing.T, file string, marker string, capability string, evidenceTokens []string, content string) {
	t.Helper()
	for _, window := range markerWindows(content, marker) {
		if strings.Contains(window, capability) && containsAll(window, evidenceTokens) {
			return
		}
	}
	t.Fatalf("%s missing adjacent handler docs coverage for %q with capability %q and evidence %v", file, marker, capability, evidenceTokens)
}

func readDocsTargets(t *testing.T, owners map[string]string) map[string]string {
	t.Helper()
	contents := map[string]string{}
	for _, file := range owners {
		if _, ok := contents[file]; ok {
			continue
		}
		contents[file] = readDocsFile(t, file)
	}
	return contents
}

func readDocsFile(t *testing.T, file string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.FromSlash(file))
	if err != nil {
		t.Fatalf("read %s: %v", file, err)
	}
	return string(data)
}

func mapValues(values map[string]string) []string {
	out := make([]string, 0, len(values))
	for _, value := range values {
		out = append(out, value)
	}
	return out
}

func markerWindows(content string, marker string) []string {
	lines := strings.Split(content, "\n")
	var windows []string
	for index, line := range lines {
		if !strings.Contains(line, marker) {
			continue
		}
		start := max(0, index-docsCoverageWindowLines)
		end := min(len(lines), index+docsCoverageWindowLines+1)
		windows = append(windows, strings.Join(lines[start:end], "\n"))
	}
	return windows
}

func resourceDocsEvidenceTokens(row generatedResourceCoverageRow) []string {
	callsite := row.CompileCallsite
	if compiled, ok := compiledResourceCallsites[row.Method]; ok {
		callsite = compiled.callsite
	}
	tokens := docsCallsiteTokens(callsite)
	if len(tokens) > 0 {
		return tokens
	}
	return []string{row.WrapperName}
}

func docsCallsiteTokens(callsite string) []string {
	matches := callsiteTokenPattern.FindAllString(callsite, -1)
	tokens := make([]string, 0, len(matches))
	for _, match := range matches {
		token := strings.TrimSuffix(match, "(") + "("
		if strings.HasPrefix(token, "codex.") ||
			strings.HasPrefix(token, "protocol.") ||
			strings.HasSuffix(token, ".ID(") {
			continue
		}
		tokens = append(tokens, token)
	}
	return tokens
}

func containsAll(content string, tokens []string) bool {
	for _, token := range tokens {
		if token == "" || !strings.Contains(content, token) {
			return false
		}
	}
	return true
}
