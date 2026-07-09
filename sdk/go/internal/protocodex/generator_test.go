package protocodex

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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
		"DecodeServerNotificationPayload",
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

func TestGoMappingRowsMatchReviewedAppendixSeed(t *testing.T) {
	rows := loadAppendixMappingRows(t)
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	for _, mapping := range resourceAPIMappings {
		row, ok := rows.resource[mapping.Method]
		if !ok {
			t.Fatalf("appendix resource seed missing %q", mapping.Method)
		}
		assertResourceMappingMatchesAppendixRow(t, mapping, row)
	}
	for _, mapping := range serverHandlerMappings {
		row, ok := rows.handlers[mapping.Method]
		if !ok {
			t.Fatalf("appendix server handler seed missing %q", mapping.Method)
		}
		assertServerHandlerMappingMatchesAppendixRow(t, mapping, row)
	}
	seenNotifications := map[string]struct{}{}
	for _, entry := range manifest.Experimental.ServerNotifications {
		row, ok := rows.notifications[entry.Method]
		if !ok {
			t.Fatalf("appendix server notification routing seed missing %q", entry.Method)
		}
		assertServerNotificationRoutingMatchesAppendixRow(t, entry, row)
		seenNotifications[entry.Method] = struct{}{}
	}
	for method := range rows.notifications {
		if _, ok := seenNotifications[method]; !ok {
			t.Fatalf("appendix server notification routing seed has stale method %q", method)
		}
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

type appendixMappingRows struct {
	resource      map[string][]string
	handlers      map[string][]string
	notifications map[string][]string
}

func loadAppendixMappingRows(t *testing.T) appendixMappingRows {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "..", "..", "..", "docs", "superpowers", "plans", "2026-07-02-go-sdk-full", "appendix-current-protocol-inventory.md"))
	if err != nil {
		t.Fatal(err)
	}
	rows := appendixMappingRows{
		resource:      map[string][]string{},
		handlers:      map[string][]string{},
		notifications: map[string][]string{},
	}
	section := ""
	for _, line := range strings.Split(string(data), "\n") {
		switch line {
		case "### Stable Client-To-Server Methods", "### Experimental Client-To-Server Methods", "### Client-To-Server Non-Public Exceptions":
			section = "resource"
			continue
		case "## Reviewed Server Handler Mapping Seed":
			section = "handlers"
			continue
		case "## Server Notification Routing Review Seed":
			section = "notifications"
			continue
		}
		if strings.HasPrefix(line, "## ") && line != "## Reviewed Server Handler Mapping Seed" && line != "## Server Notification Routing Review Seed" {
			section = ""
		}
		if section == "" || !strings.HasPrefix(line, "|") || strings.Contains(line, "---") || strings.Contains(line, "Wire method") {
			continue
		}
		cells := markdownTableCells(line)
		if len(cells) == 0 {
			continue
		}
		switch section {
		case "resource":
			if len(cells) != 7 && len(cells) != 11 {
				t.Fatalf("unexpected appendix resource row shape for %q: %#v", cells[0], cells)
			}
			rows.resource[cells[0]] = cells
		case "handlers":
			if len(cells) != 8 {
				t.Fatalf("unexpected appendix handler row shape for %q: %#v", cells[0], cells)
			}
			rows.handlers[cells[0]] = cells
		case "notifications":
			if len(cells) != 3 {
				t.Fatalf("unexpected appendix server notification row shape for %q: %#v", cells[0], cells)
			}
			rows.notifications[cells[0]] = cells
		}
	}
	return rows
}

func assertResourceMappingMatchesAppendixRow(t *testing.T, mapping ResourceAPIMapping, row []string) {
	t.Helper()
	switch len(row) {
	case 11:
		assertEqualAppendixValue(t, mapping.Method, row[0], "method")
		assertEqualAppendixValue(t, mapping.ResourceOwner, row[2], "resource owner")
		assertEqualAppendixValue(t, mapping.WrapperFile, row[3], "wrapper file")
		if mapping.SignatureConventionID != "internal-test-only" {
			assertEqualAppendixValue(t, mapping.WrapperName, row[4], "wrapper name")
		}
		assertSignatureConventionMatchesAppendix(t, mapping, row[5])
		assertEqualAppendixValue(t, mapping.CompileCallsite, row[6], "compile callsite")
		assertEqualAppendixValue(t, mapping.UnitTestOwner, row[7], "unit test owner")
		assertSafeIntegrationMatchesAppendix(t, mapping, row[8])
		assertEqualAppendixValue(t, mapping.DocsExampleOwner, row[9], "docs/example owner")
		assertEqualAppendixValue(t, mapping.ReviewNote, row[10], "review note")
	case 7:
		assertEqualAppendixValue(t, mapping.Method, row[0], "method")
		assertEqualAppendixValue(t, mapping.SignatureConventionID, row[1], "visibility")
		assertEqualAppendixValue(t, mapping.ResourceOwner, row[2], "resource owner")
		assertEqualAppendixValue(t, mapping.GeneratedOnlyException, row[3], "exception")
		assertEqualAppendixValue(t, mapping.UnitTestOwner, row[4], "unit test owner")
		assertEqualAppendixValue(t, mapping.DocsExampleOwner, row[5], "docs/example owner")
		assertEqualAppendixValue(t, mapping.ReviewNote, row[6], "review note")
	default:
		t.Fatalf("unexpected resource row for %q: %#v", mapping.Method, row)
	}
}

func assertServerHandlerMappingMatchesAppendixRow(t *testing.T, mapping ServerHandlerMapping, row []string) {
	t.Helper()
	assertEqualAppendixValue(t, mapping.Method, row[0], "method")
	assertEqualAppendixValue(t, mapping.HandlerOwner, row[2], "handler owner")
	assertEqualAppendixValue(t, mapping.Visibility, row[3], "visibility")
	assertEqualAppendixValue(t, mapping.Capability, row[4], "capability")
	assertEqualAppendixValue(t, mapping.UnitTestOwner, row[5], "unit test owner")
	assertEqualAppendixValue(t, mapping.DocsExampleOwner, row[6], "docs/example owner")
	assertEqualAppendixValue(t, mapping.ReviewNote, row[7], "review note")
}

func assertServerNotificationRoutingMatchesAppendixRow(t *testing.T, entry NotificationEntry, row []string) {
	t.Helper()
	assertEqualAppendixValue(t, entry.Method, row[0], "method")
	want := row[1]
	if before, _, ok := strings.Cut(want, ";"); ok {
		want = before
	}
	assertEqualAppendixValue(t, appendixRoutingStrategy(entry), want, "routing strategy")
}

func appendixRoutingStrategy(entry NotificationEntry) string {
	prefix := ""
	if entry.SDKVisibility != "public" && entry.SDKVisibility != "experimental-public" {
		prefix = "internal "
	}
	switch entry.RoutingStrategy.Kind {
	case "globalOnly":
		return prefix + "global"
	case "routed", "routedWithGlobalFallback":
		routeName := "route"
		if entry.RoutingStrategy.Kind == "routedWithGlobalFallback" {
			routeName = "routeWithGlobalFallback"
		}
		var routes []string
		for _, route := range entry.RoutingStrategy.Routes {
			var fields []string
			for _, extractor := range route.IdentityExtractors {
				field := extractor.FieldPath
				if extractor.Optional {
					field += "?"
				}
				fields = append(fields, field)
			}
			routes = append(routes, routeName+"("+strings.Join(fields, ", ")+")")
		}
		return prefix + strings.Join(routes, "; ")
	default:
		return prefix + entry.RoutingStrategy.Kind
	}
}

func assertSafeIntegrationMatchesAppendix(t *testing.T, mapping ResourceAPIMapping, value string) {
	t.Helper()
	if strings.HasPrefix(value, "reason: ") {
		assertEqualAppendixValue(t, mapping.SafeIntegrationReason, strings.TrimPrefix(value, "reason: "), "safe integration reason")
		return
	}
	assertEqualAppendixValue(t, mapping.SafeIntegrationOwner, value, "safe integration owner")
}

func assertSignatureConventionMatchesAppendix(t *testing.T, mapping ResourceAPIMapping, value string) {
	t.Helper()
	convention := value
	publicSignature := ""
	if before, after, ok := strings.Cut(value, ";"); ok {
		convention = strings.TrimSpace(before)
		for _, part := range strings.Split(after, ";") {
			part = strings.TrimSpace(part)
			signature, ok := strings.CutPrefix(part, "public signature:")
			if !ok {
				t.Fatalf("unknown signature convention appendix suffix for %q: %q", mapping.Method, part)
			}
			publicSignature = strings.TrimSpace(signature)
		}
	}
	assertEqualAppendixValue(t, mapping.SignatureConventionID, convention, "signature convention")
	assertEqualAppendixValue(t, mapping.PublicSignature, publicSignature, "public signature")
}

func assertEqualAppendixValue(t *testing.T, got, want, label string) {
	t.Helper()
	if normalizeAppendixCell(got) != normalizeAppendixCell(want) {
		t.Fatalf("%s = %q, want appendix seed %q", label, got, want)
	}
}

func markdownTableCells(line string) []string {
	line = strings.Trim(line, "|")
	parts := strings.Split(line, "|")
	cells := make([]string, 0, len(parts))
	for _, part := range parts {
		cells = append(cells, normalizeAppendixCell(part))
	}
	return cells
}

func normalizeAppendixCell(value string) string {
	value = strings.TrimSpace(value)
	value = strings.ReplaceAll(value, "`", "")
	value = strings.Join(strings.Fields(value), " ")
	if value == "none" {
		return ""
	}
	return value
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
