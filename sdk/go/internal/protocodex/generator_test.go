package protocodex

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestRenderMixedStringObjectUnionPreservesBothWireShapes(t *testing.T) {
	schema := Schema{OneOf: []Schema{
		{Type: "string", Enum: []json.RawMessage{json.RawMessage(`"never"`)}},
		{
			Type:       "object",
			Required:   []string{"granular"},
			Properties: map[string]Schema{"granular": {Type: "object"}},
		},
	}}
	rendered := renderDefinitionType("AskForApproval", "AskForApproval", schema, map[string]string{}, nil, nil, nil)
	for _, required := range []string{
		"StringValue string",
		`AskForApprovalNever = AskForApproval{StringValue: "never"}`,
		"var stringValue string",
		"return json.Marshal(v.StringValue)",
	} {
		if !strings.Contains(rendered, required) {
			t.Fatalf("mixed string/object union rendering missing %q\n%s", required, rendered)
		}
	}
}

func TestRenderTaggedObjectUnionRequiresOnlyTheDiscriminatorGlobally(t *testing.T) {
	schema := Schema{OneOf: []Schema{
		{
			Type:     "object",
			Required: []string{"type", "id", "text"},
			Properties: map[string]Schema{
				"type": {Type: "string", Enum: []json.RawMessage{json.RawMessage(`"agentMessage"`)}},
				"id":   {Type: "string"},
				"text": {Type: "string"},
			},
		},
		{
			Type:     "object",
			Required: []string{"type", "id", "kind"},
			Properties: map[string]Schema{
				"type": {Type: "string", Enum: []json.RawMessage{json.RawMessage(`"subAgentActivity"`)}},
				"id":   {Type: "string"},
				"kind": {Type: "string"},
			},
		},
	}}
	rendered := renderDefinitionType("ThreadItem", "ThreadItem", schema, map[string]string{}, nil, nil, nil)
	if !strings.Contains(rendered, "Kind OptionalNonNull[string]") {
		t.Fatalf("variant-only kind field must remain optional in the merged struct\n%s", rendered)
	}
	if strings.Contains(rendered, `if !ok { return DecodeError{Field: "kind", Reason: "missing required field"} }`) {
		t.Fatalf("variant-only kind field was required before discriminator dispatch\n%s", rendered)
	}
}

func TestGenerateWritesProtocolAndInventory(t *testing.T) {
	out := filepath.Join(t.TempDir(), "protocol")
	rootOut := t.TempDir()
	err := Generate(GenerateOptions{
		Mode:                   "both",
		StableSchemaRoot:       filepath.Join("..", "..", "..", "..", "codex-rs", "app-server-protocol", "schema"),
		ExperimentalSchemaRoot: "schema-experimental",
		ManifestPath:           testManifestPath(t),
		OutDir:                 out,
		RootOutDir:             rootOut,
	})
	if err != nil {
		t.Fatal(err)
	}

	types := readGeneratedFile(t, out, "types_generated.go")
	for _, required := range []string{
		"type AppScreenshot struct",
		"FileID",
		"Optional[string]",
		"UserPrompt",
		"type FileSystemSpecialPath struct",
		"type JSONRPCMessage struct",
		"NewJSONRPCRequestMessage",
		"func (v JSONRPCMessage) JSONRPCRequest()",
	} {
		if !strings.Contains(types, required) {
			t.Fatalf("types_generated.go missing %q\n%s", required, generatedExcerpt(types, "type AppScreenshot struct"))
		}
	}
	for _, forbidden := range []string{
		"type MockExperimentalMethodParams",
		"type MockExperimentalMethodResponse",
		"type GetConversationSummaryParams",
		"func (v ClientRequest) GetConversationSummaryParams()",
		"type GitDiffToRemoteParams",
		"func (v ClientRequest) GitDiffToRemoteParams()",
		"type GetAuthStatusParams",
		"func (v ClientRequest) GetAuthStatusParams()",
		"type ApplyPatchApprovalParams",
		"func (v ServerRequest) ApplyPatchApprovalParams()",
		"type ExecCommandApprovalParams",
		"func (v ServerRequest) ExecCommandApprovalParams()",
		"type JSONRPCMessage json.RawMessage",
	} {
		if strings.Contains(types, forbidden) {
			t.Fatalf("types_generated.go leaked non-public protocol surface %q", forbidden)
		}
	}

	rawClient := readGeneratedFile(t, out, "raw_client.go")
	if !strings.Contains(rawClient, "func (c RawClient) ThreadStart(") {
		t.Fatal("raw client missing ThreadStart")
	}
	if strings.Contains(rawClient, "func (c RawClient) Initialize(") {
		t.Fatal("raw client must not expose Initialize")
	}

	metadata := readGeneratedFile(t, out, "metadata.go")
	if !strings.Contains(metadata, "StableProtocolDigest") || !strings.Contains(metadata, "ExperimentalProtocolDigest") {
		t.Fatal("metadata missing digest constants")
	}
	for _, required := range []string{
		"MaxAdditionalContextEntries",
		"MaxAdditionalContextKeyBytes",
		"MaxAdditionalContextValueBytes",
		"MaxAdditionalContextTotalBytes",
	} {
		if !strings.Contains(metadata, required) {
			t.Fatalf("metadata.go missing %q", required)
		}
	}
	for _, required := range []string{
		"Retry: \"neverRetryAfterWrite\"",
		"ExperimentalFields: []ExperimentalFieldMetadata",
		"thread/start.allowProviderModelFallback",
	} {
		if !strings.Contains(metadata, required) {
			t.Fatalf("metadata.go missing %q", required)
		}
	}
	if strings.Contains(metadata, "mock/experimentalMethod") {
		t.Fatal("metadata.go leaked internal-test-only mock method")
	}

	clientNotifications := readGeneratedFile(t, out, "client_notifications.go")
	for _, required := range []string{
		"type ClientNotification struct",
		"type InitializedNotification struct",
		"NewInitializedNotification",
		"func (n ClientNotification) MarshalJSON()",
	} {
		if !strings.Contains(clientNotifications, required) {
			t.Fatalf("client_notifications.go missing %q", required)
		}
	}

	serverNotifications := readGeneratedFile(t, out, "server_notification_metadata.go")
	for _, required := range []string{
		"type ServerNotificationRoutingMetadata struct",
		"ServerNotificationRoutingByMethod",
		"func DecodeServerNotificationPayload(method string, params json.RawMessage) (any, error)",
		"case \"item/plan/delta\":",
		"var payload PlanDeltaNotification",
		"type RoutingLifecycleMetadata struct",
		"RoutingLifecycleByStartMethod",
		"RoutingKind: \"routed\"",
		"IdentityName: \"threadId\"",
		"\"command/exec\"",
		"ResourceDomain: \"commandExec\"",
		"WireIdentitySource: \"processId\"",
		"CleanupTriggers: []LifecycleTriggerMetadata",
		"Kind: \"jsonRpcResponse\"",
		"Method: \"command/exec\"",
		"\"process/spawn\"",
		"ResourceDomain: \"process\"",
		"Method: \"process/exited\"",
		"\"fs/watch\"",
		"ResourceDomain: \"fs\"",
		"Method: \"fs/unwatch\"",
		"NotificationOptOutDependencies: []string{\"fs/changed\"}",
	} {
		if !strings.Contains(serverNotifications, required) {
			t.Fatalf("server_notification_metadata.go missing %q", required)
		}
	}

	serverRequests := readGeneratedFile(t, out, "server_request_metadata.go")
	for _, required := range []string{
		"type ServerRequestMetadata struct",
		"\"currentTime/read\"",
		"Experimental: true",
	} {
		if !strings.Contains(serverRequests, required) {
			t.Fatalf("server_request_metadata.go missing %q", required)
		}
	}

	handlers := readGeneratedFile(t, rootOut, "handlers_generated.go")
	if !strings.Contains(handlers, "CurrentTime") {
		t.Fatal("handlers_generated.go missing current time handler metadata")
	}
	for _, required := range []string{
		"type generatedServerHandlerMetadataRow struct",
		"var generatedServerHandlerMetadata = []generatedServerHandlerMetadataRow",
	} {
		if !strings.Contains(handlers, required) {
			t.Fatalf("handlers_generated.go missing %q", required)
		}
	}
	for _, forbidden := range []string{
		"protocol.ApplyPatchApprovalParams",
		"protocol.ExecCommandApprovalParams",
		"type GeneratedServerHandlerMetadata struct",
	} {
		if strings.Contains(handlers, forbidden) {
			t.Fatalf("handlers_generated.go leaked compatibility-only protocol type %q", forbidden)
		}
	}

	coverage := readGeneratedFile(t, rootOut, "resource_coverage_generated.go")
	for _, required := range []string{
		"type generatedResourceCoverageRow struct",
		"var generatedResourceCoverage = []generatedResourceCoverageRow",
		"RawMethodName",
		"PublicSignature",
		"CompileCallsite",
		"SafeIntegrationReason",
		"ImplementationStatus",
		`Method: "app/list"`,
		`ImplementationStatus: "implemented-stage5f"`,
		`Method: "thread/start"`,
		`ImplementationStatus: "implemented-stage4"`,
	} {
		if !strings.Contains(coverage, required) {
			t.Fatalf("resource_coverage_generated.go missing %q", required)
		}
	}
	if strings.Contains(coverage, "type GeneratedResourceCoverage struct") {
		t.Fatal("resource_coverage_generated.go exported generated review coverage row type")
	}

	inventory := readGeneratedFile(t, rootOut, filepath.Join("internal", "protocodex", "current_protocol_inventory.generated.md"))
	if !strings.Contains(inventory, "thread/start") || !strings.Contains(inventory, "item/tool/call") || !strings.Contains(inventory, "initialized") {
		t.Fatal("generated inventory missing expected method or server request")
	}
	assertInventoryCoversManifest(t, inventory)
	for _, required := range []string{
		"serverNotifications=",
		"thread/started",
		"serverHandlers=",
		"item/tool/call(dynamic-tool-call)",
	} {
		if !strings.Contains(inventory, required) {
			t.Fatalf("generated inventory missing owner-level relation %q", required)
		}
	}
}

func assertInventoryCoversManifest(t *testing.T, inventory string) {
	t.Helper()
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	for _, entry := range manifest.Experimental.ClientRequests {
		if !strings.Contains(inventory, "`"+entry.Method+"`") {
			t.Fatalf("inventory missing client method %q", entry.Method)
		}
	}
	for _, entry := range manifest.Experimental.ServerRequests {
		if !strings.Contains(inventory, "`"+entry.Method+"`") {
			t.Fatalf("inventory missing server request %q", entry.Method)
		}
	}
	for _, entry := range manifest.Experimental.ServerNotifications {
		if !strings.Contains(inventory, "`"+entry.Method+"`") {
			t.Fatalf("inventory missing server notification %q", entry.Method)
		}
	}
	for _, entry := range manifest.Experimental.ClientNotifications {
		if !strings.Contains(inventory, "`"+entry.Method+"`") {
			t.Fatalf("inventory missing client notification %q", entry.Method)
		}
	}
	for _, mapping := range resourceAPIMappings {
		if mapping.ResourceOwner == "" || !strings.Contains(inventory, "### "+mapping.ResourceOwner) {
			t.Fatalf("inventory missing resource owner section for %q", mapping.ResourceOwner)
		}
	}
	for _, mapping := range serverHandlerMappings {
		if mapping.HandlerOwner == "" || !strings.Contains(inventory, mapping.Capability) {
			t.Fatalf("inventory missing server handler capability for %q", mapping.Method)
		}
	}
}

func TestStableModeRejectsExperimentalMethodsAndFields(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	if err := validateModeGating("stable", manifest); err != nil {
		t.Fatal(err)
	}
	mutated := *manifest
	mutated.Stable.ClientRequests = append(mutated.Stable.ClientRequests, ClientRequestEntry{
		Direction:     "clientToServer",
		Method:        "thread/realtime/start",
		SDKVisibility: "public",
		Experimental:  []byte(`{"reason":"synthetic"}`),
	})
	err = validateModeGating("stable", &mutated)
	if err == nil || !strings.Contains(err.Error(), "experimental-only method") {
		t.Fatalf("err = %v, want stable experimental-only method rejection", err)
	}

	mutated = *manifest
	mutated.Stable.ServerRequests = append(mutated.Stable.ServerRequests, ServerRequestEntry{
		Direction:     "serverToClient",
		Method:        "currentTime/read",
		SDKVisibility: "public",
		Experimental:  []byte(`{"reason":"synthetic"}`),
	})
	err = validateModeGating("stable", &mutated)
	if err == nil || !strings.Contains(err.Error(), "stable server request includes method-level experimental-only method") {
		t.Fatalf("err = %v, want stable server experimental-only method rejection", err)
	}

	metadata := renderMetadata(manifest)
	for _, required := range []string{
		"FieldPath: \"allow_provider_model_fallback\"",
		"Reason: \"thread/start.allowProviderModelFallback\"",
	} {
		if !strings.Contains(metadata, required) {
			t.Fatalf("metadata missing stable experimental field gate %q", required)
		}
	}
}

func TestStableModeRejectsExperimentalSchemaRoot(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	experimental, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	err = validateModeSubset("stable", manifest, experimental, experimental)
	if err == nil || !strings.Contains(err.Error(), "strict subset") {
		t.Fatalf("err = %v, want stable schema strict subset rejection", err)
	}
}

func TestStableModeValidatesStableSchemaAgainstCanonicalManifest(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	stable, err := LoadSchemaBundle(filepath.Join("..", "..", "..", "..", "codex-rs", "app-server-protocol", "schema"))
	if err != nil {
		t.Fatal(err)
	}
	experimental, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	if err := validateModeSubset("stable", manifest, stable, experimental); err != nil {
		t.Fatal(err)
	}

	mutated := *manifest
	mutatedStable := manifest.Stable
	removedMethod := mutatedStable.ClientRequests[0].Method
	mutatedStable.ClientRequests = filterClientRequests(append([]ClientRequestEntry(nil), mutatedStable.ClientRequests...), removedMethod)
	mutated.Stable = mutatedStable
	err = validateModeSubset("stable", &mutated, stable, experimental)
	if err == nil || !strings.Contains(err.Error(), `schema ClientRequest method "`+removedMethod+`" missing from manifest`) {
		t.Fatalf("err = %v, want stable schema vs stable manifest drift rejection", err)
	}

	mutated = *manifest
	mutatedExperimental := manifest.Experimental
	mutatedExperimental.ServerNotifications = filterNotifications(mutatedExperimental.ServerNotifications, "process/outputDelta")
	mutated.Experimental = mutatedExperimental
	err = validateModeSubset("stable", &mutated, stable, experimental)
	if err == nil || !strings.Contains(err.Error(), `schema ServerNotification method "process/outputDelta" missing from manifest`) {
		t.Fatalf("err = %v, want stable schema coverage drift rejection", err)
	}

	stableInventory, err := extractProtocolSchemaInventory(stable)
	if err != nil {
		t.Fatal(err)
	}
	metadata := renderServerNotificationMetadata(manifest, stableInventory.ServerNotifications)
	processExited := generatedExcerpt(metadata, `"process/exited"`)
	if !strings.Contains(processExited, "Experimental: false") {
		t.Fatalf("process/exited metadata = %s, want stable schema notification", processExited)
	}
	rawResponseItemCompleted := generatedExcerpt(metadata, `"rawResponseItem/completed"`)
	if !strings.Contains(rawResponseItemCompleted, "Experimental: false") {
		t.Fatalf("rawResponseItem/completed metadata = %s, want stable manifest notification", rawResponseItemCompleted)
	}
}

func TestSchemaInventoryCoversManifestAllDirections(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	schema, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	if err := validateProtocolSchemaManifestMode("experimental", manifest.Experimental, schema); err != nil {
		t.Fatal(err)
	}
	inventory, err := extractProtocolSchemaInventory(schema)
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []struct {
		methods map[string]protocolSchemaVariant
		method  string
	}{
		{inventory.ClientRequests, "thread/start"},
		{inventory.ServerRequests, "item/tool/call"},
		{inventory.ServerNotifications, "account/updated"},
		{inventory.ClientNotifications, "initialized"},
	} {
		if _, ok := required.methods[required.method]; !ok {
			t.Fatalf("schema inventory missing %q", required.method)
		}
	}
}

func TestSchemaInventoryRejectsManifestDrift(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	schema, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	mutated := manifest.Experimental
	mutated.ClientRequests = filterClientRequests(mutated.ClientRequests, "thread/start")
	err = validateProtocolSchemaManifestMode("experimental", mutated, schema)
	if err == nil || !strings.Contains(err.Error(), `schema ClientRequest method "thread/start" missing from manifest`) {
		t.Fatalf("err = %v, want schema-backed drift rejection", err)
	}
}

func TestManifestSchemaRefsAreFailClosed(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	schema, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	if err := validateManifestSchemaRefs("experimental", manifest.Experimental, schema); err != nil {
		t.Fatal(err)
	}

	mutated := manifest.Experimental
	mutated.ClientRequests = replaceClientRequest(mutated.ClientRequests, "thread/start", func(entry ClientRequestEntry) ClientRequestEntry {
		entry.ResponseSchemaRef = "#/definitions/DefinitelyMissingResponse"
		return entry
	})
	err = validateManifestSchemaRefs("experimental", mutated, schema)
	if err == nil || !strings.Contains(err.Error(), "responseSchemaRef") || !strings.Contains(err.Error(), "DefinitelyMissingResponse") {
		t.Fatalf("err = %v, want missing responseSchemaRef rejection", err)
	}

	mutated = manifest.Experimental
	mutated.ClientRequests = replaceClientRequest(mutated.ClientRequests, "thread/start", func(entry ClientRequestEntry) ClientRequestEntry {
		marker := "crate::protocol::manual_payload_conversion"
		entry.ManualPayloadConversion = &marker
		return entry
	})
	err = validateManifestSchemaRefs("experimental", mutated, schema)
	if err == nil || !strings.Contains(err.Error(), "manualPayloadConversion") {
		t.Fatalf("err = %v, want unsupported manualPayloadConversion rejection", err)
	}
}

func TestMethodMetadataPreservesManifestProtocolRefs(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}

	metadata := renderMetadata(manifest)
	threadStart := generatedExcerpt(metadata, `"thread/start"`)
	for _, required := range []string{
		`ParamsType: "ThreadStartParams"`,
		`ParamsSchemaRef: "#/definitions/ThreadStartParams"`,
		`ResponseType: "ThreadStartResponse"`,
		`ResponseSchemaRef: "#/definitions/ThreadStartResponse"`,
		`InspectParams: true`,
		`ManualPayloadConversion: ""`,
	} {
		if !strings.Contains(threadStart, required) {
			t.Fatalf("thread/start metadata = %s, missing %q", threadStart, required)
		}
	}

	configValueWrite := generatedExcerpt(metadata, `"config/value/write"`)
	if !strings.Contains(configValueWrite, `ManualPayloadConversion: "manual response payload conversion"`) {
		t.Fatalf("config/value/write metadata = %s, missing manualPayloadConversion marker", configValueWrite)
	}

	turnStart := generatedExcerpt(metadata, `"turn/start"`)
	for _, required := range []string{
		`BoundedModelContextFields: []BoundedModelContextFieldMetadata`,
		`FieldPath: "additional_context.*.value"`,
		`LimitProfile: "additionalContextValueBytes"`,
	} {
		if !strings.Contains(turnStart, required) {
			t.Fatalf("turn/start metadata = %s, missing %q", turnStart, required)
		}
	}
}

func TestServerMetadataPreservesManifestProtocolRefs(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	stable, err := LoadSchemaBundle(filepath.Join("..", "..", "..", "..", "codex-rs", "app-server-protocol", "schema"))
	if err != nil {
		t.Fatal(err)
	}
	stableInventory, err := extractProtocolSchemaInventory(stable)
	if err != nil {
		t.Fatal(err)
	}

	serverRequests := renderServerRequestMetadata(manifest)
	elicitation := generatedExcerpt(serverRequests, `"mcpServer/elicitation/request"`)
	for _, required := range []string{
		`ParamsType: "McpServerElicitationRequestParams"`,
		`ParamsSchemaRef: "#/definitions/McpServerElicitationRequestParams"`,
		`ResponseType: "McpServerElicitationRequestResponse"`,
		`ResponseSchemaRef: "#/definitions/McpServerElicitationRequestResponse"`,
		`ManualPayloadConversion: ""`,
	} {
		if !strings.Contains(elicitation, required) {
			t.Fatalf("mcpServer/elicitation/request metadata = %s, missing %q", elicitation, required)
		}
	}

	notifications := renderServerNotificationMetadata(manifest, stableInventory.ServerNotifications)
	threadStarted := generatedExcerpt(notifications, `"thread/started"`)
	for _, required := range []string{
		`PayloadType: "ThreadStartedNotification"`,
		`PayloadSchemaRef: "#/definitions/ThreadStartedNotification"`,
		`ManualPayloadConversion: ""`,
	} {
		if !strings.Contains(threadStarted, required) {
			t.Fatalf("thread/started metadata = %s, missing %q", threadStarted, required)
		}
	}
}

func TestSchemaSufficientProofIsRequired(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mutated := manifest.Experimental
	mutated.SerdeShapes = append([]SerdeShape(nil), mutated.SerdeShapes...)
	for index := range mutated.SerdeShapes {
		if mutated.SerdeShapes[index].MetadataStatus == "schemaSufficient" {
			mutated.SerdeShapes[index].SchemaSufficientProof = nil
			err = validateSerdeShapeProofs("experimental", mutated)
			if err == nil || !strings.Contains(err.Error(), "without schemaSufficientProof") {
				t.Fatalf("err = %v, want missing schemaSufficientProof rejection", err)
			}
			return
		}
	}
	t.Fatal("test manifest has no schemaSufficient serde shape")
}

func TestReachableSerdeShapeCoverageIsRequired(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	schema, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	mutated := manifest.Experimental
	mutated.SerdeShapes = filterSerdeShapes(mutated.SerdeShapes, "AppScreenshot")
	err = validateReachableSerdeShapeCoverage("experimental", mutated, schema)
	if err == nil || !strings.Contains(err.Error(), "AppScreenshot") {
		t.Fatalf("err = %v, want missing reachable AppScreenshot serde shape rejection", err)
	}
}

func TestSchemaExcludedCompatibilityTypesDoNotRequireSchemaDefinitions(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	schema, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	var entry *ClientRequestEntry
	for index := range manifest.Experimental.ClientRequests {
		if manifest.Experimental.ClientRequests[index].Method == "getConversationSummary" {
			entry = &manifest.Experimental.ClientRequests[index]
			break
		}
	}
	if entry == nil {
		t.Fatal("getConversationSummary should be in manifest")
	}
	if entry.SchemaExcludedReason == "" {
		t.Fatal("getConversationSummary should record why v1 schemas are excluded")
	}
	if entry.ParamsSchemaRef != "" || entry.ResponseSchemaRef != "" {
		t.Fatalf("schema refs = (%q, %q), want excluded refs", entry.ParamsSchemaRef, entry.ResponseSchemaRef)
	}
	if _, err := reachableSchemaDefinitions(manifest.Experimental, schema); err != nil {
		t.Fatalf("reachable schema definitions rejected excluded v1 compatibility schemas: %v", err)
	}
}

func TestUnknownCustomSerdeHookIsRejected(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mutated := manifest.Experimental
	mutated.SerdeShapes = append([]SerdeShape(nil), mutated.SerdeShapes...)
	mutated.SerdeShapes[0].Fields = append([]SerdeField(nil), mutated.SerdeShapes[0].Fields...)
	mutated.SerdeShapes[0].Fields = append(mutated.SerdeShapes[0].Fields, SerdeField{
		WireName:  "custom",
		RustField: "custom",
		Shape: SerdeFieldShape{
			CustomDeserialize: "crate::unsupported::deserialize_custom",
		},
	})
	err = validateCustomSerdeHooks("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "unsupported customDeserialize hook") {
		t.Fatalf("err = %v, want unsupported customDeserialize hook rejection", err)
	}
}

func TestUnknownSkipSerializingIfIsRejected(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mutated := manifest.Experimental
	mutated.SerdeShapes = append([]SerdeShape(nil), mutated.SerdeShapes...)
	mutated.SerdeShapes[0].Fields = append([]SerdeField(nil), mutated.SerdeShapes[0].Fields...)
	mutated.SerdeShapes[0].Fields = append(mutated.SerdeShapes[0].Fields, SerdeField{
		WireName:  "custom",
		RustField: "custom",
		Shape: SerdeFieldShape{
			SkipSerializingIf: "crate::unsupported::skip_custom",
		},
	})
	err = validateCustomSerdeHooks("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "unsupported skipSerializingIf predicate") {
		t.Fatalf("err = %v, want unsupported skipSerializingIf rejection", err)
	}
}

func TestRequestSerializationScopeConditionsAreFailClosed(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	mutated := manifest.Experimental
	mutated.ClientRequests = replaceClientRequest(mutated.ClientRequests, "thread/resume", func(entry ClientRequestEntry) ClientRequestEntry {
		entry.RequestSerializationScopes = append([]RequestSerializationScope(nil), entry.RequestSerializationScopes...)
		entry.RequestSerializationScopes[0].Condition = []byte(`{"unknown":"thread_id"}`)
		return entry
	})
	err = validateRequestSerializationScopes("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "unknown request serialization condition") {
		t.Fatalf("err = %v, want unknown condition rejection", err)
	}

	mutated = manifest.Experimental
	mutated.ClientRequests = replaceClientRequest(mutated.ClientRequests, "thread/resume", func(entry ClientRequestEntry) ClientRequestEntry {
		entry.RequestSerializationScopes = append([]RequestSerializationScope(nil), entry.RequestSerializationScopes...)
		entry.RequestSerializationScopes[0].Condition = []byte(`{"stringNonEmpty":"thread_id"}`)
		entry.RequestSerializationScopes[1].Condition = []byte(`{"fieldPresent":"thread_id"}`)
		return entry
	})
	err = validateRequestSerializationScopes("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "overlapping request serialization scope condition") {
		t.Fatalf("err = %v, want overlapping condition rejection", err)
	}
}

func TestManifestDigestMetadataIsFailClosed(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}

	mutated := manifest.Experimental
	mutated.Digests = copyDigestMap(mutated.Digests)
	delete(mutated.Digests, "protocolDigest")
	err = validateManifestDigests("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "protocolDigest") {
		t.Fatalf("err = %v, want missing protocolDigest rejection", err)
	}

	mutated = manifest.Experimental
	mutated.Digests = copyDigestMap(mutated.Digests)
	mutated.Digests["schemaDigest"] = ""
	err = validateManifestDigests("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "schemaDigest") {
		t.Fatalf("err = %v, want empty schemaDigest rejection", err)
	}
}

func TestManifestSchemaVersionIsFailClosed(t *testing.T) {
	data, err := os.ReadFile(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name   string
		mutate func(map[string]any)
	}{
		{name: "missing", mutate: func(manifest map[string]any) { delete(manifest, "manifestSchemaVersion") }},
		{name: "zero", mutate: func(manifest map[string]any) { manifest["manifestSchemaVersion"] = 0 }},
		{name: "future", mutate: func(manifest map[string]any) { manifest["manifestSchemaVersion"] = 2 }},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var manifest map[string]any
			if err := json.Unmarshal(data, &manifest); err != nil {
				t.Fatal(err)
			}
			tt.mutate(manifest)
			mutated, err := json.Marshal(manifest)
			if err != nil {
				t.Fatal(err)
			}
			path := filepath.Join(t.TempDir(), "manifest.json")
			if err := os.WriteFile(path, mutated, 0o600); err != nil {
				t.Fatal(err)
			}
			_, err = LoadManifest(path)
			if err == nil || !strings.Contains(err.Error(), "manifestSchemaVersion") {
				t.Fatalf("err = %v, want manifestSchemaVersion rejection", err)
			}
		})
	}
}

func TestManifestV1SemanticMetadataIsFailClosed(t *testing.T) {
	data, err := os.ReadFile(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	tests := []struct {
		name    string
		wantErr string
		mutate  func(map[string]any)
	}{
		{
			name:    "protocol mode",
			wantErr: "protocolMode",
			mutate: func(manifest map[string]any) {
				manifest["experimental"].(map[string]any)["protocolMode"] = "future"
			},
		},
		{
			name:    "SDK visibility",
			wantErr: "sdkVisibility",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				mode["clientRequests"].([]any)[0].(map[string]any)["sdkVisibility"] = "future"
			},
		},
		{
			name:    "serde shape requirement",
			wantErr: "serdeShapeRequirement",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				mode["clientRequests"].([]any)[0].(map[string]any)["serdeShapeRequirement"] = "future"
			},
		},
		{
			name:    "retry policy",
			wantErr: "retry",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				mode["clientRequests"].([]any)[0].(map[string]any)["retry"] = "retryAfterWrite"
			},
		},
		{
			name:    "empty exception review",
			wantErr: "exception",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				mode["clientRequests"].([]any)[0].(map[string]any)["exception"] = map[string]any{}
			},
		},
		{
			name:    "incomplete experimental marker",
			wantErr: "experimental",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				mode["clientRequests"].([]any)[0].(map[string]any)["experimental"] = map[string]any{"reason": "future"}
			},
		},
		{
			name:    "incomplete experimental discriminator",
			wantErr: "discriminator",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				for _, collection := range []string{"clientRequests", "serverRequests", "serverNotifications", "clientNotifications"} {
					for _, rawEntry := range mode[collection].([]any) {
						fields := rawEntry.(map[string]any)["experimentalFields"].([]any)
						if len(fields) == 0 {
							continue
						}
						fields[0].(map[string]any)["discriminator"] = map[string]any{}
						return
					}
				}
				t.Fatal("canonical manifest has no experimental field")
			},
		},
		{
			name:    "unknown serde presence",
			wantErr: "presence",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				shapes := mode["serdeShapes"].([]any)
				for _, rawShape := range shapes {
					fields := rawShape.(map[string]any)["fields"].([]any)
					if len(fields) == 0 {
						continue
					}
					fields[0].(map[string]any)["shape"].(map[string]any)["presence"] = "future"
					return
				}
				t.Fatal("canonical manifest has no serde field")
			},
		},
		{
			name:    "incomplete default provider",
			wantErr: "provider",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				shapes := mode["serdeShapes"].([]any)
				for _, rawShape := range shapes {
					for _, rawField := range rawShape.(map[string]any)["fields"].([]any) {
						shape := rawField.(map[string]any)["shape"].(map[string]any)
						if shape["default"] == nil {
							continue
						}
						shape["default"].(map[string]any)["provider"] = map[string]any{}
						return
					}
				}
				t.Fatal("canonical manifest has no serde default")
			},
		},
		{
			name:    "manual conversion mismatch",
			wantErr: "manualPayloadConversion",
			mutate: func(manifest map[string]any) {
				mode := manifest["experimental"].(map[string]any)
				entry := mode["clientRequests"].([]any)[0].(map[string]any)
				entry["serdeShapeRequirement"] = "schemaSufficient"
				entry["manualPayloadConversion"] = "manual response payload conversion"
			},
		},
		{
			name:    "zero context limit",
			wantErr: "maxAdditionalContextEntries",
			mutate: func(manifest map[string]any) {
				manifest["modelContextLimits"].(map[string]any)["maxAdditionalContextEntries"] = 0
			},
		},
		{
			name:    "negative context limit",
			wantErr: "maxAdditionalContextValueBytes",
			mutate: func(manifest map[string]any) {
				manifest["modelContextLimits"].(map[string]any)["maxAdditionalContextValueBytes"] = -1
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var manifest map[string]any
			if err := json.Unmarshal(data, &manifest); err != nil {
				t.Fatal(err)
			}
			tt.mutate(manifest)
			mutated, err := json.Marshal(manifest)
			if err != nil {
				t.Fatal(err)
			}
			path := filepath.Join(t.TempDir(), "manifest.json")
			if err := os.WriteFile(path, mutated, 0o600); err != nil {
				t.Fatal(err)
			}
			_, err = LoadManifest(path)
			if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
				t.Fatalf("err = %v, want rejection containing %q", err, tt.wantErr)
			}
		})
	}
}

func TestManifestV1RejectsUnknownNestedField(t *testing.T) {
	data, err := os.ReadFile(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	var manifest map[string]any
	if err := json.Unmarshal(data, &manifest); err != nil {
		t.Fatal(err)
	}
	experimental := manifest["experimental"].(map[string]any)
	notifications := experimental["serverNotifications"].([]any)
	notification := notifications[0].(map[string]any)
	notification["unknownV1Field"] = true
	mutated, err := json.Marshal(manifest)
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "manifest.json")
	if err := os.WriteFile(path, mutated, 0o600); err != nil {
		t.Fatal(err)
	}
	_, err = LoadManifest(path)
	if err == nil || !strings.Contains(err.Error(), "unknownV1Field") {
		t.Fatalf("err = %v, want unknown nested field rejection", err)
	}
}

func TestManifestV1InformationalMetadataIsRepresented(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	if manifest.Experimental.ClientRequests[0].Variant == "" || manifest.Experimental.ServerRequests[0].Variant == "" {
		t.Fatal("request variant metadata was not decoded")
	}
	if manifest.Experimental.ServerNotifications[0].Variant == "" {
		t.Fatal("notification variant metadata was not decoded")
	}
	var sawRoutingReason bool
	var sawMissingIdentityReason bool
	for _, entry := range manifest.Experimental.ServerNotifications {
		sawRoutingReason = sawRoutingReason || entry.RoutingStrategy.Reason != ""
		sawMissingIdentityReason = sawMissingIdentityReason || entry.RoutingStrategy.MissingIdentityReason != ""
	}
	if !sawRoutingReason || !sawMissingIdentityReason {
		t.Fatal("routing reason metadata was not decoded")
	}
	var sawReviewNote bool
	for _, shape := range manifest.Experimental.SerdeShapes {
		sawReviewNote = sawReviewNote || shape.ReviewNote != nil
	}
	if !sawReviewNote {
		t.Fatal("serde shape review metadata was not decoded")
	}
}

func TestRoutingLifecycleMetadataIsFailClosed(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	if err := validateRoutingLifecycle("experimental", manifest.Experimental); err != nil {
		t.Fatal(err)
	}

	mutated := manifest.Experimental
	mutated.RoutingLifecycle = replaceRoutingLifecycle(mutated.RoutingLifecycle, "command/exec", func(entry RoutingLifecycle) RoutingLifecycle {
		entry.WireIdentitySource = ""
		return entry
	})
	err = validateRoutingLifecycle("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "wireIdentitySource") {
		t.Fatalf("err = %v, want missing wireIdentitySource rejection", err)
	}

	mutated = manifest.Experimental
	mutated.RoutingLifecycle = replaceRoutingLifecycle(mutated.RoutingLifecycle, "fs/watch", func(entry RoutingLifecycle) RoutingLifecycle {
		entry.CleanupTriggers = nil
		return entry
	})
	err = validateRoutingLifecycle("experimental", mutated)
	if err == nil || !strings.Contains(err.Error(), "cleanupTriggers") {
		t.Fatalf("err = %v, want missing cleanupTriggers rejection", err)
	}
}

func TestGlobalNotificationsMappedToResourceOwners(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	notificationsByOwner := serverNotificationMethodsByOwner(manifest)
	for _, required := range []struct {
		owner  string
		method string
	}{
		{"Accounts", "account/updated"},
		{"Accounts", "account/rateLimits/updated"},
		{"Apps", "app/list/updated"},
		{"Realtime", "thread/realtime/started"},
		{"Realtime", "thread/realtime/sdp"},
		{"Reviews", "item/completed"},
		{"Reviews", "turn/completed"},
		{"Skills", "skills/changed"},
		{"WindowsSandbox", "windowsSandbox/setupCompleted"},
	} {
		if !containsString(notificationsByOwner[required.owner], required.method) {
			t.Fatalf("%s notifications = %#v, missing %s", required.owner, notificationsByOwner[required.owner], required.method)
		}
	}
}

func TestStage4ImplementedResourceCoverageUsesCurrentAPINames(t *testing.T) {
	for _, mapping := range resourceAPIMappings {
		if resourceImplementationStatus(mapping) != "implemented-stage4" {
			continue
		}
		text := mapping.WrapperName + " " + mapping.CompileCallsite
		for _, forbidden := range []string{
			"Accounts.LoginStart",
			"DeviceCodeLoginOptions",
			"ReadRateLimits",
			"ReadUsage",
			"Turns.Start",
			"PermissionProfile(",
			"ServerName:",
		} {
			if strings.Contains(text, forbidden) {
				t.Fatalf("%s implemented-stage4 mapping uses stale API name %q: %s", mapping.Method, forbidden, text)
			}
		}
	}
}

func TestStage4ImplementedResourceCoverageUsesExistingTestOwners(t *testing.T) {
	for _, mapping := range resourceAPIMappings {
		if resourceImplementationStatus(mapping) != "implemented-stage4" {
			continue
		}
		if mapping.UnitTestOwner == "" {
			t.Fatalf("%s implemented-stage4 mapping has empty unitTest owner", mapping.Method)
		}
		path := filepath.Join("..", "..", mapping.UnitTestOwner)
		if _, err := os.Stat(path); err != nil {
			t.Fatalf("%s implemented-stage4 unitTest owner %q does not exist: %v", mapping.Method, mapping.UnitTestOwner, err)
		}
	}
}

func filterClientRequests(entries []ClientRequestEntry, method string) []ClientRequestEntry {
	out := entries[:0]
	for _, entry := range entries {
		if entry.Method != method {
			out = append(out, entry)
		}
	}
	return out
}

func filterNotifications(entries []NotificationEntry, method string) []NotificationEntry {
	out := make([]NotificationEntry, 0, len(entries))
	for _, entry := range entries {
		if entry.Method != method {
			out = append(out, entry)
		}
	}
	return out
}

func replaceClientRequest(entries []ClientRequestEntry, method string, replace func(ClientRequestEntry) ClientRequestEntry) []ClientRequestEntry {
	out := append([]ClientRequestEntry(nil), entries...)
	for index, entry := range out {
		if entry.Method == method {
			out[index] = replace(entry)
		}
	}
	return out
}

func filterSerdeShapes(entries []SerdeShape, rustType string) []SerdeShape {
	out := make([]SerdeShape, 0, len(entries))
	for _, entry := range entries {
		if entry.RustType != rustType {
			out = append(out, entry)
		}
	}
	return out
}

func containsString(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func copyDigestMap(in map[string]string) map[string]string {
	out := make(map[string]string, len(in))
	for key, value := range in {
		out[key] = value
	}
	return out
}

func replaceRoutingLifecycle(entries []RoutingLifecycle, startMethod string, replace func(RoutingLifecycle) RoutingLifecycle) []RoutingLifecycle {
	out := append([]RoutingLifecycle(nil), entries...)
	for index, entry := range out {
		if entry.StartMethod == startMethod {
			out[index] = replace(entry)
		}
	}
	return out
}

func generatedExcerpt(text, marker string) string {
	start := strings.Index(text, marker)
	if start < 0 {
		return ""
	}
	end := start + 900
	if end > len(text) {
		end = len(text)
	}
	return text[start:end]
}

func readGeneratedFile(t *testing.T, root, name string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(root, name))
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}
