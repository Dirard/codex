package codex

import (
	"context"
	"strings"
	"testing"
)

type expectedResourceSignals struct {
	notifications []string
	handlers      []string
}

var expectedResourceSignalsByOwner = map[string]expectedResourceSignals{
	"Accounts": {
		notifications: []string{"account/login/completed", "account/rateLimits/updated", "account/updated"},
		handlers:      []string{"account/chatgptAuthTokens/refresh(chatgpt-token-refresh)", "attestation/generate(attestation-generate)"},
	},
	"Apps":                 {notifications: []string{"app/list/updated"}},
	"CollaborationModes":   {},
	"Commands":             {notifications: []string{"command/exec/outputDelta"}},
	"Config":               {notifications: []string{"configWarning"}},
	"Environments":         {},
	"ExperimentalFeatures": {},
	"ExternalAgents": {
		notifications: []string{"externalAgentConfig/import/completed", "externalAgentConfig/import/progress"},
	},
	"Feedback":        {},
	"FileSystem":      {notifications: []string{"fs/changed"}},
	"FuzzyFileSearch": {notifications: []string{"fuzzyFileSearch/sessionCompleted", "fuzzyFileSearch/sessionUpdated"}},
	"Hooks":           {notifications: []string{"hook/completed", "hook/started"}},
	"MCP": {
		notifications: []string{"mcpServer/oauthLogin/completed", "mcpServer/startupStatus/updated"},
		handlers:      []string{"mcpServer/elicitation/request(mcp-elicitation)"},
	},
	"Marketplace": {},
	"Memory":      {},
	"Models": {
		notifications: []string{"model/rerouted", "model/safetyBuffering/updated", "model/verification"},
	},
	"PermissionProfiles": {},
	"Plugins":            {},
	"Processes":          {notifications: []string{"process/exited", "process/outputDelta"}},
	"Realtime": {
		notifications: []string{
			"thread/realtime/closed",
			"thread/realtime/error",
			"thread/realtime/itemAdded",
			"thread/realtime/outputAudio/delta",
			"thread/realtime/sdp",
			"thread/realtime/started",
			"thread/realtime/transcript/delta",
			"thread/realtime/transcript/done",
		},
	},
	"RemoteControl": {notifications: []string{"remoteControl/status/changed"}},
	"Reviews": {
		notifications: []string{
			"error",
			"guardianWarning",
			"item/agentMessage/delta",
			"item/commandExecution/outputDelta",
			"item/commandExecution/terminalInteraction",
			"item/completed",
			"item/fileChange/outputDelta",
			"item/fileChange/patchUpdated",
			"item/mcpToolCall/progress",
			"item/plan/delta",
			"item/reasoning/summaryPartAdded",
			"item/reasoning/summaryTextDelta",
			"item/reasoning/textDelta",
			"item/started",
			"rawResponseItem/completed",
			"serverRequest/resolved",
			"turn/completed",
			"turn/diff/updated",
			"turn/moderationMetadata",
			"turn/plan/updated",
			"turn/started",
			"warning",
		},
	},
	"Skills": {notifications: []string{"skills/changed"}},
	"Threads": {
		notifications: []string{
			"deprecationNotice",
			"error",
			"guardianWarning",
			"serverRequest/resolved",
			"thread/archived",
			"thread/closed",
			"thread/compacted",
			"thread/deleted",
			"thread/goal/cleared",
			"thread/goal/updated",
			"thread/name/updated",
			"thread/realtime/closed",
			"thread/realtime/error",
			"thread/realtime/itemAdded",
			"thread/realtime/outputAudio/delta",
			"thread/realtime/sdp",
			"thread/realtime/started",
			"thread/realtime/transcript/delta",
			"thread/realtime/transcript/done",
			"thread/settings/updated",
			"thread/started",
			"thread/status/changed",
			"thread/tokenUsage/updated",
			"thread/unarchived",
			"warning",
		},
		handlers: []string{"currentTime/read(current-time-read)"},
	},
	"Turns": {
		notifications: []string{
			"error",
			"guardianWarning",
			"item/agentMessage/delta",
			"item/autoApprovalReview/completed",
			"item/autoApprovalReview/started",
			"item/commandExecution/outputDelta",
			"item/commandExecution/terminalInteraction",
			"item/completed",
			"item/fileChange/outputDelta",
			"item/fileChange/patchUpdated",
			"item/mcpToolCall/progress",
			"item/plan/delta",
			"item/reasoning/summaryPartAdded",
			"item/reasoning/summaryTextDelta",
			"item/reasoning/textDelta",
			"item/started",
			"rawResponseItem/completed",
			"serverRequest/resolved",
			"turn/completed",
			"turn/diff/updated",
			"turn/moderationMetadata",
			"turn/plan/updated",
			"turn/started",
			"warning",
		},
		handlers: []string{
			"item/commandExecution/requestApproval(command-execution-approval)",
			"item/fileChange/requestApproval(file-change-approval)",
			"item/permissions/requestApproval(permission-approval)",
			"item/tool/call(dynamic-tool-call)",
			"item/tool/requestUserInput(tool-user-input)",
		},
	},
	"WindowsSandbox": {notifications: []string{"windows/worldWritableWarning", "windowsSandbox/setupCompleted"}},
}

func TestClientResourceFields(t *testing.T) {
	client, err := NewClient(context.Background(), ClientConfig{Transport: newScriptedInitializedTransport(t, nil)})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	resources := map[string]any{
		"Accounts":             client.Accounts,
		"Threads":              client.Threads,
		"Turns":                client.Turns,
		"Realtime":             client.Realtime,
		"Reviews":              client.Reviews,
		"Models":               client.Models,
		"Config":               client.Config,
		"FileSystem":           client.FileSystem,
		"Commands":             client.Commands,
		"Processes":            client.Processes,
		"Environments":         client.Environments,
		"Skills":               client.Skills,
		"Hooks":                client.Hooks,
		"Plugins":              client.Plugins,
		"Marketplace":          client.Marketplace,
		"Apps":                 client.Apps,
		"MCP":                  client.MCP,
		"RemoteControl":        client.RemoteControl,
		"CollaborationModes":   client.CollaborationModes,
		"ExternalAgents":       client.ExternalAgents,
		"FuzzyFileSearch":      client.FuzzyFileSearch,
		"Memory":               client.Memory,
		"Feedback":             client.Feedback,
		"WindowsSandbox":       client.WindowsSandbox,
		"ExperimentalFeatures": client.ExperimentalFeatures,
		"PermissionProfiles":   client.PermissionProfiles,
	}
	for name, resource := range resources {
		if resource == nil {
			t.Fatalf("%s resource client is nil", name)
		}
	}
}

func TestResourceCoverage(t *testing.T) {
	if len(generatedResourceCoverage) == 0 {
		t.Fatal("generated resource coverage is empty")
	}
	rowsByMethod := map[string]generatedResourceCoverageRow{}
	for _, row := range generatedResourceCoverage {
		if row.Method == "" {
			t.Fatal("resource coverage row has empty method")
		}
		if _, ok := rowsByMethod[row.Method]; ok {
			t.Fatalf("duplicate resource coverage row for %q", row.Method)
		}
		rowsByMethod[row.Method] = row
		if row.SDKVisibility == "public" {
			assertPublicResourceCoverageRow(t, row)
		} else {
			assertGeneratedOnlyResourceCoverageRow(t, row)
		}
	}
	for method, owner := range map[string]string{
		"thread/realtime/start":         "Realtime",
		"thread/settings/update":        "Threads",
		"memory/reset":                  "Memory",
		"collaborationMode/list":        "CollaborationModes",
		"process/spawn":                 "Processes",
		"fuzzyFileSearch":               "FuzzyFileSearch",
		"fuzzyFileSearch/sessionStart":  "FuzzyFileSearch",
		"fuzzyFileSearch/sessionUpdate": "FuzzyFileSearch",
		"fuzzyFileSearch/sessionStop":   "FuzzyFileSearch",
	} {
		row, ok := rowsByMethod[method]
		if !ok {
			t.Fatalf("resource coverage missing required method %q", method)
		}
		if row.ResourceOwner != owner {
			t.Fatalf("%s resource owner = %q, want %q", method, row.ResourceOwner, owner)
		}
	}
}

func assertPublicResourceCoverageRow(t *testing.T, row generatedResourceCoverageRow) {
	t.Helper()
	for field, value := range map[string]string{
		"implementation status": row.ImplementationStatus,
		"resource owner":        row.ResourceOwner,
		"raw method":            row.RawMethodName,
		"wrapper":               row.WrapperName,
		"wrapper file":          row.WrapperFile,
		"signature convention":  row.SignatureConventionID,
		"compile callsite":      row.CompileCallsite,
		"unit test owner":       row.UnitTestOwner,
		"docs/example owner":    row.DocsExampleOwner,
		"review note":           row.ReviewNote,
	} {
		assertCoverageValue(t, row.Method, field, value)
	}
	if row.PublicSignature == "" && row.SignatureConventionID == "" {
		t.Fatalf("%s has no public signature or signature convention", row.Method)
	}
	if !allowedSignatureConvention(row.SignatureConventionID) {
		t.Fatalf("%s signature convention = %q", row.Method, row.SignatureConventionID)
	}
	assertThinConventionHasExplicitSignatureForPositionalArgs(t, row)
	assertResourceSignals(t, row)
	if row.SafeIntegrationOwner == "" && row.SafeIntegrationReason == "" {
		t.Fatalf("%s has no safe integration owner or reason", row.Method)
	}
	if row.SafeIntegrationReason != "" {
		assertCoverageValue(t, row.Method, "safe integration reason", row.SafeIntegrationReason)
		assertSpecificSafetyReason(t, row.Method, row.SafeIntegrationReason)
	}
	if row.GeneratedOnlyException != "" {
		t.Fatalf("%s is public but has generated-only exception %q", row.Method, row.GeneratedOnlyException)
	}
}

func assertGeneratedOnlyResourceCoverageRow(t *testing.T, row generatedResourceCoverageRow) {
	t.Helper()
	for field, value := range map[string]string{
		"visibility":               row.SDKVisibility,
		"resource owner":           row.ResourceOwner,
		"generated-only exception": row.GeneratedOnlyException,
		"unit test owner":          row.UnitTestOwner,
		"docs/example owner":       row.DocsExampleOwner,
		"review note":              row.ReviewNote,
	} {
		assertCoverageValue(t, row.Method, field, value)
	}
	for field, value := range map[string]string{
		"wrapper":          row.WrapperName,
		"public signature": row.PublicSignature,
		"compile callsite": row.CompileCallsite,
	} {
		if value != "" {
			t.Fatalf("%s generated-only %s = %q, want empty", row.Method, field, value)
		}
	}
	if row.WrapperFile != "" && !strings.HasSuffix(row.WrapperFile, "_test.go") {
		t.Fatalf("%s generated-only wrapper file = %q, want empty or test-only", row.Method, row.WrapperFile)
	}
	lowerDocsOwner := strings.ToLower(row.DocsExampleOwner)
	if strings.Contains(row.DocsExampleOwner, "examples/") || strings.Contains(lowerDocsOwner, "readme") {
		t.Fatalf("%s generated-only docs/example owner looks public: %q", row.Method, row.DocsExampleOwner)
	}
	if len(row.ServerNotificationMethods) > 0 {
		t.Fatalf("%s generated-only server notification methods = %q, want empty", row.Method, row.ServerNotificationMethods)
	}
	if len(row.ServerHandlerCapabilities) > 0 {
		t.Fatalf("%s generated-only server handler capabilities = %q, want empty", row.Method, row.ServerHandlerCapabilities)
	}
}

func assertCoverageValue(t *testing.T, method string, field string, value string) {
	t.Helper()
	if strings.TrimSpace(value) == "" {
		t.Fatalf("%s has empty %s", method, field)
	}
	lower := strings.ToLower(value)
	for _, forbidden := range []string{"placeholder", "todo", "tbd", "fixme"} {
		if strings.Contains(lower, forbidden) {
			t.Fatalf("%s %s contains placeholder marker %q: %q", method, field, forbidden, value)
		}
	}
}

func allowedSignatureConvention(convention string) bool {
	switch convention {
	case "thin", "high-level", "handle-start", "handle-followup":
		return true
	default:
		return false
	}
}

func assertSpecificSafetyReason(t *testing.T, method string, reason string) {
	t.Helper()
	lower := strings.ToLower(strings.TrimSpace(reason))
	switch lower {
	case "unsafe", "not applicable", "not-applicable", "n/a", "none":
		t.Fatalf("%s safe integration reason is not specific: %q", method, reason)
	}
	for _, forbidden := range []string{"row required", "not-applicable", "not applicable"} {
		if strings.Contains(lower, forbidden) {
			t.Fatalf("%s safe integration reason has unresolved wording %q: %q", method, forbidden, reason)
		}
	}
}

func assertThinConventionHasExplicitSignatureForPositionalArgs(t *testing.T, row generatedResourceCoverageRow) {
	t.Helper()
	if row.SignatureConventionID != "thin" || row.PublicSignature != "" {
		return
	}
	if strings.HasSuffix(row.CompileCallsite, "(ctx)") || strings.Contains(row.CompileCallsite, "(ctx, protocol.") {
		return
	}
	t.Fatalf("%s thin row has positional args without explicit public signature: %q", row.Method, row.CompileCallsite)
}

func assertResourceSignals(t *testing.T, row generatedResourceCoverageRow) {
	t.Helper()
	expected, ok := expectedResourceSignalsByOwner[row.ResourceOwner]
	if !ok {
		t.Fatalf("%s has resource owner %q without explicit signal expectation", row.Method, row.ResourceOwner)
	}
	assertStringSliceEqual(t, row.Method, "server notification methods", row.ServerNotificationMethods, expected.notifications)
	assertStringSliceEqual(t, row.Method, "server handler capabilities", row.ServerHandlerCapabilities, expected.handlers)
}

func assertStringSliceEqual(t *testing.T, method string, field string, got []string, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("%s %s = %q, want %q", method, field, got, want)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Fatalf("%s %s = %q, want %q", method, field, got, want)
		}
	}
}
