package codex

import (
	"reflect"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestThreadAndTurnOptionCoverageAgainstGeneratedParams(t *testing.T) {
	assertGeneratedParamCoverage(t, "ThreadStartOptions", reflect.TypeOf(protocol.ThreadStartParams{}), map[string]string{
		"AllowProviderModelFallback": "ThreadStartOptions.AllowProviderModelFallback",
		"ApprovalPolicy":             "ThreadStartOptions.ApprovalPolicy",
		"ApprovalsReviewer":          "ThreadStartOptions.ApprovalsReviewer",
		"BaseInstructions":           "ThreadStartOptions.BaseInstructions",
		"Config":                     "ThreadStartOptions.Config",
		"Cwd":                        "ThreadStartOptions.CWD",
		"DeveloperInstructions":      "ThreadStartOptions.DeveloperInstructions",
		"DynamicTools":               "ThreadStartOptions.DynamicTools",
		"Environments":               "ThreadStartOptions.Environments",
		"Ephemeral":                  "ThreadStartOptions.Ephemeral",
		"ExperimentalRawEvents":      "ThreadStartOptions.ExperimentalRawEvents",
		"HistoryMode":                "ThreadStartOptions.HistoryMode",
		"MockExperimentalField":      "ThreadStartOptions.MockExperimentalField",
		"Model":                      "ThreadStartOptions.Model",
		"ModelProvider":              "ThreadStartOptions.ModelProvider",
		"MultiAgentMode":             "ThreadStartOptions.MultiAgentMode",
		"Permissions":                "ThreadStartOptions.Permissions",
		"Personality":                "ThreadStartOptions.Personality",
		"RuntimeWorkspaceRoots":      "ThreadStartOptions.RuntimeWorkspaceRoots",
		"Sandbox":                    "ThreadStartOptions.Sandbox",
		"SelectedCapabilityRoots":    "ThreadStartOptions.SelectedCapabilityRoots",
		"ServiceName":                "ThreadStartOptions.ServiceName",
		"ServiceTier":                "ThreadStartOptions.ServiceTier",
		"SessionStartSource":         "ThreadStartOptions.SessionStartSource",
		"ThreadSource":               "ThreadStartOptions.ThreadSource",
	}, nil)

	assertGeneratedParamCoverage(t, "TurnOptions", reflect.TypeOf(protocol.TurnStartParams{}), map[string]string{
		"AdditionalContext":          "TurnOptions.AdditionalContext",
		"ApprovalPolicy":             "TurnOptions.ApprovalPolicy",
		"ApprovalsReviewer":          "TurnOptions.ApprovalsReviewer",
		"ClientUserMessageID":        "TurnOptions.ClientUserMessageID",
		"CollaborationMode":          "TurnOptions.CollaborationMode",
		"Cwd":                        "TurnOptions.CWD",
		"Effort":                     "TurnOptions.Effort",
		"Environments":               "TurnOptions.Environments",
		"Model":                      "TurnOptions.Model",
		"MultiAgentMode":             "TurnOptions.MultiAgentMode",
		"OutputSchema":               "TurnOptions.OutputSchema",
		"Permissions":                "TurnOptions.Permissions",
		"Personality":                "TurnOptions.Personality",
		"ResponsesapiClientMetadata": "TurnOptions.ResponsesAPIClientMetadata",
		"RuntimeWorkspaceRoots":      "TurnOptions.RuntimeWorkspaceRoots",
		"SandboxPolicy":              "TurnOptions.SandboxPolicy",
		"ServiceTier":                "TurnOptions.ServiceTier",
		"Summary":                    "TurnOptions.Summary",
	}, map[string]string{
		"Input":    "SDK-owned required argument from Thread.Run/Thread.Turn input",
		"ThreadID": "SDK-owned from Thread.ID",
	})

	assertGeneratedParamCoverage(t, "SteerOptions", reflect.TypeOf(protocol.TurnSteerParams{}), map[string]string{
		"AdditionalContext":          "SteerOptions.AdditionalContext",
		"ClientUserMessageID":        "SteerOptions.ClientUserMessageID",
		"ResponsesapiClientMetadata": "SteerOptions.ResponsesAPIClientMetadata",
	}, map[string]string{
		"ExpectedTurnID": "SDK-owned from TurnHandle.ID",
		"Input":          "SDK-owned required argument from TurnHandle.Steer input",
		"ThreadID":       "SDK-owned from TurnHandle thread identity",
	})
}

func TestDeferredThreadNamespaceOptionCoverageIsDocumented(t *testing.T) {
	stage5Reason := "Stage 5 method-level resource matrix owns non-first-screen thread namespace wrappers"
	assertGeneratedParamCoverage(t, "ThreadResumeOptions deferred", reflect.TypeOf(protocol.ThreadResumeParams{}), nil, map[string]string{
		"ApprovalPolicy":        stage5Reason,
		"ApprovalsReviewer":     stage5Reason,
		"BaseInstructions":      stage5Reason,
		"Config":                stage5Reason,
		"Cwd":                   stage5Reason,
		"DeveloperInstructions": stage5Reason,
		"ExcludeTurns":          stage5Reason,
		"History":               stage5Reason,
		"InitialTurnsPage":      stage5Reason,
		"Model":                 stage5Reason,
		"ModelProvider":         stage5Reason,
		"Path":                  stage5Reason,
		"Permissions":           stage5Reason,
		"Personality":           stage5Reason,
		"RuntimeWorkspaceRoots": stage5Reason,
		"Sandbox":               stage5Reason,
		"ServiceTier":           stage5Reason,
		"ThreadID":              stage5Reason,
	})
	assertGeneratedParamCoverage(t, "ThreadForkOptions deferred", reflect.TypeOf(protocol.ThreadForkParams{}), nil, map[string]string{
		"ApprovalPolicy":        stage5Reason,
		"ApprovalsReviewer":     stage5Reason,
		"BaseInstructions":      stage5Reason,
		"Config":                stage5Reason,
		"Cwd":                   stage5Reason,
		"DeveloperInstructions": stage5Reason,
		"Ephemeral":             stage5Reason,
		"ExcludeTurns":          stage5Reason,
		"LastTurnID":            stage5Reason,
		"Model":                 stage5Reason,
		"ModelProvider":         stage5Reason,
		"Path":                  stage5Reason,
		"Permissions":           stage5Reason,
		"RuntimeWorkspaceRoots": stage5Reason,
		"Sandbox":               stage5Reason,
		"ServiceTier":           stage5Reason,
		"ThreadID":              stage5Reason,
		"ThreadSource":          stage5Reason,
	})
}

func assertGeneratedParamCoverage(t *testing.T, name string, paramsType reflect.Type, mappings map[string]string, omissions map[string]string) {
	t.Helper()
	for i := 0; i < paramsType.NumField(); i++ {
		field := paramsType.Field(i).Name
		if mappings[field] != "" {
			continue
		}
		if omissions[field] != "" {
			continue
		}
		t.Fatalf("%s missing mapping or omission for generated field %s", name, field)
	}
	for field, mapping := range mappings {
		if mapping == "" {
			t.Fatalf("%s has empty mapping for %s", name, field)
		}
		if _, ok := paramsType.FieldByName(field); !ok {
			t.Fatalf("%s mapping references non-generated field %s", name, field)
		}
	}
	for field, reason := range omissions {
		if reason == "" {
			t.Fatalf("%s has empty omission reason for %s", name, field)
		}
		if _, ok := paramsType.FieldByName(field); !ok {
			t.Fatalf("%s omission references non-generated field %s", name, field)
		}
	}
}
