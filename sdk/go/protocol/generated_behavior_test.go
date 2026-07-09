package protocol

import (
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
)

func TestGeneratedStructUsesSerdeAliasesAndOmitsUnsetOptionals(t *testing.T) {
	var screenshot AppScreenshot
	if err := json.Unmarshal([]byte(`{"file_id":"file-1","user_prompt":"look"}`), &screenshot); err != nil {
		t.Fatal(err)
	}
	fileID, ok := screenshot.FileID.Value()
	if !ok || fileID != "file-1" {
		t.Fatalf("FileID = %q, %v; want file-1, true", fileID, ok)
	}
	if screenshot.UserPrompt != "look" {
		t.Fatalf("UserPrompt = %q, want look", screenshot.UserPrompt)
	}

	encoded, err := json.Marshal(AppScreenshot{UserPrompt: "look"})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(encoded), "fileId") {
		t.Fatalf("encoded unset optional field: %s", encoded)
	}
}

func TestGeneratedStructRejectsMissingRequiredField(t *testing.T) {
	var screenshot AppScreenshot
	err := json.Unmarshal([]byte(`{"fileId":"file-1"}`), &screenshot)
	if err == nil || !strings.Contains(err.Error(), "userPrompt") {
		t.Fatalf("err = %v, want missing userPrompt error", err)
	}
	var decodeErr DecodeError
	if !errors.As(err, &decodeErr) || decodeErr.Field != "userPrompt" {
		t.Fatalf("err = %v, want DecodeError for userPrompt", err)
	}
}

func TestGeneratedTaggedUnionUsesVariantAliases(t *testing.T) {
	var path FileSystemSpecialPath
	if err := json.Unmarshal([]byte(`{"kind":"current_working_directory"}`), &path); err != nil {
		t.Fatal(err)
	}
	if path.Kind != "project_roots" {
		t.Fatalf("Kind = %q, want project_roots", path.Kind)
	}
}

func TestGeneratedTaggedUnionPreservesUnknownDiscriminator(t *testing.T) {
	var path FileSystemSpecialPath
	if err := json.Unmarshal([]byte(`{"kind":"future_path","path":{"nested":true}}`), &path); err != nil {
		t.Fatal(err)
	}
	if path.Kind != "future_path" {
		t.Fatalf("Kind = %q, want future_path", path.Kind)
	}
	if string(path.RawJSON) != `{"kind":"future_path","path":{"nested":true}}` {
		t.Fatalf("RawJSON = %s, want original payload", path.RawJSON)
	}
	encoded, err := json.Marshal(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"kind":"future_path","path":{"nested":true}}` {
		t.Fatalf("encoded = %s, want original payload", encoded)
	}
}

func TestGeneratedTaggedUnionRequiresVariantFields(t *testing.T) {
	var tool DynamicToolSpec
	err := json.Unmarshal([]byte(`{"type":"function"}`), &tool)
	var decodeErr DecodeError
	if !errors.As(err, &decodeErr) || !isOneOf(decodeErr.Field, "description", "inputSchema", "name") {
		t.Fatalf("err = %v, want DecodeError for a missing function variant field", err)
	}
}

func TestGeneratedTaggedUnionMarshalRejectsInvalidVariant(t *testing.T) {
	_, err := json.Marshal(DynamicToolSpec{TypeValue: "function"})
	var decodeErr DecodeError
	if !errors.As(err, &decodeErr) || !isOneOf(decodeErr.Field, "description", "inputSchema", "name") {
		t.Fatalf("err = %v, want DecodeError for a missing function variant field", err)
	}

	_, err = json.Marshal(DynamicToolSpec{TypeValue: "future"})
	if !errors.As(err, &decodeErr) || decodeErr.Field != "type" {
		t.Fatalf("err = %v, want DecodeError for unsupported type", err)
	}
}

func TestGeneratedUntaggedObjectUnionPreservesUnknownVariant(t *testing.T) {
	payload := []byte(`{"serverName":"server-1","threadId":"thread-1","mode":"future","message":"Try this","future":{"nested":true}}`)
	var unknown McpServerElicitationRequestParams
	if err := json.Unmarshal(payload, &unknown); err != nil {
		t.Fatal(err)
	}
	if string(unknown.RawJSON) != string(payload) {
		t.Fatalf("RawJSON = %s, want original payload", unknown.RawJSON)
	}
	encoded, err := json.Marshal(unknown)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != string(payload) {
		t.Fatalf("encoded = %s, want original payload", encoded)
	}

	var invalidKnownVariant McpServerElicitationRequestParams
	err = json.Unmarshal([]byte(`{"serverName":"server-1","threadId":"thread-1","mode":"form","message":"Fill this in"}`), &invalidKnownVariant)
	var decodeErr DecodeError
	if !errors.As(err, &decodeErr) || !strings.Contains(decodeErr.Reason, "oneOf") {
		t.Fatalf("err = %v, want oneOf variant mismatch for malformed known variant", err)
	}

	var form McpServerElicitationRequestParams
	if err := json.Unmarshal([]byte(`{
		"serverName":"server-1",
		"threadId":"thread-1",
		"mode":"form",
		"message":"Fill this in",
		"requestedSchema":{"properties":{},"type":"object"}
	}`), &form); err != nil {
		t.Fatal(err)
	}
	mode, ok := form.Mode.Value()
	if !ok || mode != "form" {
		t.Fatalf("Mode = %q, %v; want form, true", mode, ok)
	}
}

func TestGeneratedMultiAgentModeSupportsBuiltInAndCustomUnion(t *testing.T) {
	encoded, err := json.Marshal(MultiAgentModeProactive)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"proactive"` {
		t.Fatalf("encoded proactive mode = %s, want proactive string", encoded)
	}

	var explicit MultiAgentMode
	if err := json.Unmarshal([]byte(`"explicitRequestOnly"`), &explicit); err != nil {
		t.Fatal(err)
	}
	if explicit.Mode != "explicitRequestOnly" {
		t.Fatalf("Mode = %q, want explicitRequestOnly", explicit.Mode)
	}

	custom := CustomMultiAgentMode("review-only")
	encoded, err = json.Marshal(custom)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"custom":"review-only"}` {
		t.Fatalf("encoded custom mode = %s, want custom object", encoded)
	}

	var decodedCustom MultiAgentMode
	if err := json.Unmarshal([]byte(`{"custom":"review-only"}`), &decodedCustom); err != nil {
		t.Fatal(err)
	}
	customValue, ok := decodedCustom.Custom.Value()
	if !ok || customValue != "review-only" {
		t.Fatalf("Custom = %q, %v; want review-only, true", customValue, ok)
	}

	var future MultiAgentMode
	if err := json.Unmarshal([]byte(`"futureMode"`), &future); err != nil {
		t.Fatal(err)
	}
	encoded, err = json.Marshal(future)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"futureMode"` {
		t.Fatalf("encoded future mode = %s, want raw string", encoded)
	}
}

func TestGeneratedObjectStructRejectsTopLevelNull(t *testing.T) {
	var empty AttestationGenerateParams
	err := json.Unmarshal([]byte(`null`), &empty)
	if err == nil || !strings.Contains(err.Error(), "cannot be null") {
		t.Fatalf("err = %v, want null object rejection", err)
	}

	var optionalOnly AppsListParams
	err = json.Unmarshal([]byte(` null `), &optionalOnly)
	if err == nil || !strings.Contains(err.Error(), "cannot be null") {
		t.Fatalf("err = %v, want null object rejection", err)
	}
}

func TestGeneratedClientNotificationInitializedWireShape(t *testing.T) {
	encoded, err := json.Marshal(NewInitializedNotification())
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"method":"initialized"}` {
		t.Fatalf("encoded = %s, want initialized notification", encoded)
	}
}

func TestGeneratedClientNotificationRejectsUnknownMethod(t *testing.T) {
	_, err := json.Marshal(ClientNotification{Method: ClientNotificationMethod("future/missing")})
	if err == nil || !strings.Contains(err.Error(), "unsupported client notification method") {
		t.Fatalf("err = %v, want unsupported client notification method", err)
	}
}

func TestGeneratedStructAppliesSerdeDefaultsOnMissingFields(t *testing.T) {
	var info AppInfo
	if err := json.Unmarshal([]byte(`{"id":"app-1","name":"App One"}`), &info); err != nil {
		t.Fatal(err)
	}
	enabled, ok := info.IsEnabled.Value()
	if !ok || !enabled {
		t.Fatalf("IsEnabled = %v, %v; want default true", enabled, ok)
	}
	pluginDisplayNames, ok := info.PluginDisplayNames.Value()
	if !ok || len(pluginDisplayNames) != 0 {
		t.Fatalf("PluginDisplayNames = %#v, %v; want default empty list", pluginDisplayNames, ok)
	}
}

func TestGeneratedStructPreservesFlattenedAdditionalFields(t *testing.T) {
	var config AnalyticsConfig
	if err := json.Unmarshal([]byte(`{"enabled":true,"customFlag":"kept","nested":{"value":1}}`), &config); err != nil {
		t.Fatal(err)
	}
	enabled, ok := config.Enabled.Value()
	if !ok || !enabled {
		t.Fatalf("Enabled = %v, %v; want true, true", enabled, ok)
	}
	if string(config.Additional["customFlag"]) != `"kept"` {
		t.Fatalf("Additional customFlag = %s, want kept", config.Additional["customFlag"])
	}
	if string(config.Additional["nested"]) != `{"value":1}` {
		t.Fatalf("Additional nested = %s, want nested object", config.Additional["nested"])
	}

	encoded, err := json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	encodedText := string(encoded)
	for _, required := range []string{`"enabled":true`, `"customFlag":"kept"`, `"nested":{"value":1}`} {
		if !strings.Contains(encodedText, required) {
			t.Fatalf("encoded = %s, missing %s", encodedText, required)
		}
	}
}

func TestGeneratedStructHonorsSkipSerializingIfFalse(t *testing.T) {
	params := ProcessSpawnParams{
		Command:       []string{"bash", "-lc", "true"},
		Cwd:           AbsolutePathBuf("/tmp"),
		ProcessHandle: "proc-1",
		StreamStdin:   SomeNonNull(true),
		Tty:           SomeNonNull(false),
	}
	encoded, err := json.Marshal(params)
	if err != nil {
		t.Fatal(err)
	}
	encodedText := string(encoded)
	if strings.Contains(encodedText, `"tty"`) {
		t.Fatalf("encoded skip-serializing false field: %s", encodedText)
	}
	if !strings.Contains(encodedText, `"streamStdin":true`) {
		t.Fatalf("encoded = %s, want streamStdin true", encodedText)
	}
}

func TestGeneratedStructHonorsOptionAndVecSkipSerializingIf(t *testing.T) {
	toolCall := McpServerToolCallParams{
		Server:   "server-1",
		ThreadID: "thread-1",
		Tool:     "tool-1",
	}
	encoded, err := json.Marshal(toolCall)
	if err != nil {
		t.Fatal(err)
	}
	encodedText := string(encoded)
	for _, omitted := range []string{`"_meta"`, `"arguments"`} {
		if strings.Contains(encodedText, omitted) {
			t.Fatalf("encoded omitted Option::is_none field %s: %s", omitted, encodedText)
		}
	}

	skills := SkillsListParams{Cwds: SomeNonNull([]string{})}
	encoded, err = json.Marshal(skills)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(encoded), `"cwds"`) {
		t.Fatalf("encoded empty Vec::is_empty field: %s", encoded)
	}

	hooks := HooksListParams{Cwds: SomeNonNull([]string{"/tmp"})}
	encoded, err = json.Marshal(hooks)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(encoded), `"cwds":["/tmp"]`) {
		t.Fatalf("encoded = %s, want non-empty cwds", encoded)
	}
}

func TestGeneratedOpenObjectPreservesAppToolsConfig(t *testing.T) {
	var config AppConfig
	if err := json.Unmarshal([]byte(`{"tools":{"search":{"enabled":true},"shell":{"approval":"never"}}}`), &config); err != nil {
		t.Fatal(err)
	}
	tools, ok := config.Tools.Value()
	if !ok {
		t.Fatal("tools should be present")
	}
	if string(tools["search"]) != `{"enabled":true}` {
		t.Fatalf("search tool = %s, want preserved raw object", tools["search"])
	}
	if string(tools["shell"]) != `{"approval":"never"}` {
		t.Fatalf("shell tool = %s, want preserved raw object", tools["shell"])
	}

	encoded, err := json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	var roundTrip map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &roundTrip); err != nil {
		t.Fatal(err)
	}
	var roundTripTools map[string]json.RawMessage
	if err := json.Unmarshal(roundTrip["tools"], &roundTripTools); err != nil {
		t.Fatal(err)
	}
	if string(roundTripTools["search"]) != `{"enabled":true}` || string(roundTripTools["shell"]) != `{"approval":"never"}` {
		t.Fatalf("round-trip tools = %s", roundTrip["tools"])
	}

	var nullTools AppToolsConfig
	err = json.Unmarshal([]byte(`null`), &nullTools)
	if err == nil || !strings.Contains(err.Error(), "cannot be null") {
		t.Fatalf("err = %v, want null object rejection", err)
	}
}

func TestGeneratedOptionalRawMessagesOmitUnsetFields(t *testing.T) {
	notification := JSONRPCNotification{Method: "initialized"}
	encoded, err := json.Marshal(notification)
	if err != nil {
		t.Fatal(err)
	}
	encodedText := string(encoded)
	if strings.Contains(encodedText, `"params"`) {
		t.Fatalf("encoded unset raw params: %s", encodedText)
	}

	errorObject := JSONRPCErrorError{Code: -32603, Message: "internal"}
	encoded, err = json.Marshal(errorObject)
	if err != nil {
		t.Fatal(err)
	}
	encodedText = string(encoded)
	if strings.Contains(encodedText, `"data"`) {
		t.Fatalf("encoded unset raw data: %s", encodedText)
	}
}

func TestGeneratedDoubleOptionDistinguishesOmittedNullAndValue(t *testing.T) {
	var omitted ProcessSpawnParams
	if err := json.Unmarshal([]byte(`{"command":["bash"],"cwd":"/tmp","processHandle":"proc-1"}`), &omitted); err != nil {
		t.Fatal(err)
	}
	if omitted.OutputBytesCap.IsSet() {
		t.Fatal("omitted outputBytesCap should remain unset")
	}
	encodedOmitted, err := json.Marshal(omitted)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(encodedOmitted), "outputBytesCap") {
		t.Fatalf("encoded omitted double-option field: %s", encodedOmitted)
	}

	var explicitNull ProcessSpawnParams
	if err := json.Unmarshal([]byte(`{"command":["bash"],"cwd":"/tmp","processHandle":"proc-1","outputBytesCap":null}`), &explicitNull); err != nil {
		t.Fatal(err)
	}
	if !explicitNull.OutputBytesCap.IsNull() {
		t.Fatal("explicit null outputBytesCap should be set null")
	}
	encodedNull, err := json.Marshal(explicitNull)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(encodedNull), `"outputBytesCap":null`) {
		t.Fatalf("encoded = %s, want explicit null outputBytesCap", encodedNull)
	}

	var explicitValue ProcessSpawnParams
	if err := json.Unmarshal([]byte(`{"command":["bash"],"cwd":"/tmp","processHandle":"proc-1","outputBytesCap":1024}`), &explicitValue); err != nil {
		t.Fatal(err)
	}
	value, ok := explicitValue.OutputBytesCap.Value()
	if !ok || value != 1024 {
		t.Fatalf("OutputBytesCap = %d, %v; want 1024, true", value, ok)
	}
}

func TestGeneratedUnsignedIntegerFormatsRejectNegativeAndKeepUint64Range(t *testing.T) {
	var negative McpElicitationTitledMultiSelectEnumSchema
	err := json.Unmarshal([]byte(`{"items":{"anyOf":[]},"maxItems":-1,"type":"array"}`), &negative)
	if err == nil || !strings.Contains(err.Error(), "maxItems") {
		t.Fatalf("err = %v, want maxItems negative rejection", err)
	}

	var max McpElicitationTitledMultiSelectEnumSchema
	if err := json.Unmarshal([]byte(`{"items":{"anyOf":[]},"maxItems":18446744073709551615,"type":"array"}`), &max); err != nil {
		t.Fatal(err)
	}
	value, ok := max.MaxItems.Value()
	if !ok || value != ^uint64(0) {
		t.Fatalf("MaxItems = %d, %v; want max uint64, true", value, ok)
	}
}

func TestGeneratedIntegerBoundsRejectSchemaOutOfRangeValues(t *testing.T) {
	var invalid AdditionalFileSystemPermissions
	err := json.Unmarshal([]byte(`{"globScanMaxDepth":0}`), &invalid)
	if err == nil || !strings.Contains(err.Error(), "globScanMaxDepth") {
		t.Fatalf("err = %v, want globScanMaxDepth minimum rejection", err)
	}

	var valid AdditionalFileSystemPermissions
	if err := json.Unmarshal([]byte(`{"globScanMaxDepth":1}`), &valid); err != nil {
		t.Fatal(err)
	}
	value, ok := valid.GlobScanMaxDepth.Value()
	if !ok || value != 1 {
		t.Fatalf("GlobScanMaxDepth = %d, %v; want 1, true", value, ok)
	}
}

func TestGeneratedThreadStartParamsNormalizesLegacyDynamicTools(t *testing.T) {
	var params ThreadStartParams
	err := json.Unmarshal([]byte(`{
		"dynamicTools": [
			{
				"namespace": "workspace",
				"name": "search",
				"description": "Search files",
				"inputSchema": {"type":"object"},
				"exposeToContext": false
			}
		]
	}`), &params)
	if err != nil {
		t.Fatal(err)
	}
	tools, ok := params.DynamicTools.Value()
	if !ok || len(tools) != 1 {
		t.Fatalf("DynamicTools = %#v, %v; want one normalized tool", tools, ok)
	}
	if tools[0].TypeValue != "namespace" {
		t.Fatalf("tool type = %q, want namespace", tools[0].TypeValue)
	}
	namespaceName, ok := tools[0].Name.Value()
	if !ok || namespaceName != "workspace" {
		t.Fatalf("namespace name = %q, %v; want workspace, true", namespaceName, ok)
	}
	namespaceTools, ok := tools[0].Tools.Value()
	if !ok || len(namespaceTools) != 1 {
		t.Fatalf("namespace tools = %#v, %v; want one function", namespaceTools, ok)
	}
	if namespaceTools[0].TypeValue != "function" {
		t.Fatalf("namespace tool type = %q, want function", namespaceTools[0].TypeValue)
	}
	toolName, ok := namespaceTools[0].Name.Value()
	if !ok || toolName != "search" {
		t.Fatalf("function name = %q, %v; want search, true", toolName, ok)
	}
	deferLoading, ok := namespaceTools[0].DeferLoading.Value()
	if !ok || !deferLoading {
		t.Fatalf("deferLoading = %v, %v; want true, true", deferLoading, ok)
	}
}

func TestGeneratedNamedScalarDefinitionsDecodeAsJSONScalars(t *testing.T) {
	var response InitializeResponse
	err := json.Unmarshal([]byte(`{
		"activeProtocolMode":"experimental",
		"codexHome":"/tmp/codex",
		"experimentalManifestDigest":"em",
		"experimentalProtocolDigest":"ep",
		"experimentalSchemaDigest":"es",
		"platformFamily":"unix",
		"platformOs":"linux",
		"stableManifestDigest":"sm",
		"stableProtocolDigest":"sp",
		"stableSchemaDigest":"ss",
		"userAgent":"codex-test"
	}`), &response)
	if err != nil {
		t.Fatal(err)
	}
	if string(response.CodexHome) != "/tmp/codex" {
		t.Fatalf("CodexHome = %q, want /tmp/codex", response.CodexHome)
	}

	encoded, err := json.Marshal(response.CodexHome)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"/tmp/codex"` {
		t.Fatalf("encoded CodexHome = %s, want JSON string", encoded)
	}
}

func TestGeneratedJSONRPCUnionPreservesRawParamsAndTypedDecode(t *testing.T) {
	var request ClientRequest
	if err := json.Unmarshal([]byte(`{"id":1,"method":"thread/start","params":{"model":"gpt-5","workingDirectory":"/tmp"}}`), &request); err != nil {
		t.Fatal(err)
	}
	if request.Method != "thread/start" || !strings.Contains(string(request.Params), `"model"`) {
		t.Fatalf("request = %#v, params = %s", request, request.Params)
	}
	params, ok, err := request.ThreadStartParams()
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("ThreadStartParams accessor did not match thread/start")
	}
	model, ok := params.Model.Value()
	if !ok || model != "gpt-5" {
		t.Fatalf("model = %q, %v; want gpt-5", model, ok)
	}

	var future ClientRequest
	if err := json.Unmarshal([]byte(`{"id":"future","method":"future/method","params":{"x":1}}`), &future); err != nil {
		t.Fatal(err)
	}
	if future.Method != "future/method" || string(future.Params) != `{"x":1}` {
		t.Fatalf("future = %#v params=%s", future, future.Params)
	}
}

func TestGeneratedJSONRPCUnionAllowsNoParamClientRequest(t *testing.T) {
	var omitted ClientRequest
	if err := json.Unmarshal([]byte(`{"id":1,"method":"memory/reset"}`), &omitted); err != nil {
		t.Fatal(err)
	}
	if omitted.Method != "memory/reset" || len(omitted.Params) != 0 {
		t.Fatalf("omitted = %#v params=%s", omitted, omitted.Params)
	}
	encoded, err := json.Marshal(omitted)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(encoded), `"params"`) {
		t.Fatalf("encoded omitted no-param request with params: %s", encoded)
	}

	var explicitNull ClientRequest
	if err := json.Unmarshal([]byte(`{"id":1,"method":"memory/reset","params":null}`), &explicitNull); err != nil {
		t.Fatal(err)
	}
	if string(explicitNull.Params) != "null" {
		t.Fatalf("explicit null params = %s, want null", explicitNull.Params)
	}
	encoded, err = json.Marshal(explicitNull)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(encoded), `"params":null`) {
		t.Fatalf("encoded explicit-null no-param request = %s, want params:null", encoded)
	}
}

func TestGeneratedJSONRPCUnionRequiresMethodDiscriminator(t *testing.T) {
	var request ClientRequest
	err := json.Unmarshal([]byte(`{"id":1,"params":{}}`), &request)
	var decodeErr DecodeError
	if !errors.As(err, &decodeErr) || decodeErr.Field != "method" {
		t.Fatalf("err = %v, want DecodeError for method", err)
	}
}

func TestGeneratedJSONRPCMessageUsesTypedVariantsAndRawFallback(t *testing.T) {
	request := NewJSONRPCRequestMessage(JSONRPCRequest{
		ID:     IntRequestID(1),
		Method: "thread/start",
		Params: json.RawMessage(`{"threadId":"thread-1"}`),
	})
	encoded, err := json.Marshal(request)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"id":1,"method":"thread/start","params":{"threadId":"thread-1"}}` {
		t.Fatalf("encoded request = %s", encoded)
	}

	var decoded JSONRPCMessage
	if err := json.Unmarshal(encoded, &decoded); err != nil {
		t.Fatal(err)
	}
	gotRequest, ok := decoded.JSONRPCRequest()
	if !ok {
		t.Fatal("decoded message is not a JSON-RPC request")
	}
	if gotRequest.Method != "thread/start" || string(gotRequest.Params) != `{"threadId":"thread-1"}` {
		t.Fatalf("request = %#v", gotRequest)
	}

	for _, tc := range []struct {
		name           string
		payload        string
		exactRoundTrip bool
		checkKind      func(JSONRPCMessage) bool
	}{
		{
			name:    "notification",
			payload: `{"method":"initialized"}`,
			checkKind: func(message JSONRPCMessage) bool {
				notification, ok := message.JSONRPCNotification()
				return ok && notification.Method == "initialized"
			},
		},
		{
			name:    "response",
			payload: `{"id":1,"result":{"ok":true}}`,
			checkKind: func(message JSONRPCMessage) bool {
				response, ok := message.JSONRPCResponse()
				return ok && string(response.Result) == `{"ok":true}`
			},
		},
		{
			name:    "error",
			payload: `{"id":1,"error":{"code":-32603,"message":"internal"}}`,
			checkKind: func(message JSONRPCMessage) bool {
				responseError, ok := message.JSONRPCError()
				return ok && responseError.Error.Code == -32603
			},
		},
		{
			name:           "raw fallback",
			payload:        `{"jsonrpc":"2.0","future":true}`,
			exactRoundTrip: true,
			checkKind: func(message JSONRPCMessage) bool {
				return string(message.RawJSON) == `{"jsonrpc":"2.0","future":true}`
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var message JSONRPCMessage
			if err := json.Unmarshal([]byte(tc.payload), &message); err != nil {
				t.Fatal(err)
			}
			if !tc.checkKind(message) {
				t.Fatalf("message %#v did not match %s", message, tc.name)
			}
			encoded, err := json.Marshal(message)
			if err != nil {
				t.Fatal(err)
			}
			if tc.exactRoundTrip {
				if string(encoded) != tc.payload {
					t.Fatalf("encoded = %s, want %s", encoded, tc.payload)
				}
			} else {
				assertJSONEqual(t, encoded, tc.payload)
			}
		})
	}
}

func TestGeneratedJSONRPCResponseAllowsNullResult(t *testing.T) {
	var response JSONRPCResponse
	if err := json.Unmarshal([]byte(`{"id":1,"result":null}`), &response); err != nil {
		t.Fatal(err)
	}
	if string(response.Result) != `null` {
		t.Fatalf("Result = %s, want null", response.Result)
	}
	encoded, err := json.Marshal(response)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{"id":1,"result":null}`)
}

func TestGeneratedMcpElicitationPrimitiveSchemaUsesTypedUnionAndRawFallback(t *testing.T) {
	var primitive McpElicitationPrimitiveSchema
	if err := json.Unmarshal([]byte(`{"type":"boolean","title":"Enabled"}`), &primitive); err != nil {
		t.Fatal(err)
	}
	booleanSchema, ok := primitive.McpElicitationBooleanSchema()
	if !ok {
		t.Fatalf("primitive = %#v, want boolean schema variant", primitive)
	}
	title, ok := booleanSchema.Title.Value()
	if !ok || title != "Enabled" {
		t.Fatalf("Title = %q, %v; want Enabled, true", title, ok)
	}
	encoded, err := json.Marshal(primitive)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{"type":"boolean","title":"Enabled"}`)

	var future McpElicitationPrimitiveSchema
	if err := json.Unmarshal([]byte(`{"type":"future","x":1}`), &future); err != nil {
		t.Fatal(err)
	}
	if string(future.RawJSON) != `{"type":"future","x":1}` {
		t.Fatalf("RawJSON = %s, want future payload", future.RawJSON)
	}
	encoded, err = json.Marshal(future)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `{"type":"future","x":1}` {
		t.Fatalf("encoded = %s, want future payload", encoded)
	}
}

func TestGeneratedStringEnumAppliesSerdeVariantAliases(t *testing.T) {
	var reviewer ApprovalsReviewer
	if err := json.Unmarshal([]byte(`"guardian_subagent"`), &reviewer); err != nil {
		t.Fatal(err)
	}
	if reviewer != ApprovalsReviewerAutoReview {
		t.Fatalf("reviewer = %q, want %q", reviewer, ApprovalsReviewerAutoReview)
	}
	encoded, err := json.Marshal(reviewer)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"auto_review"` {
		t.Fatalf("encoded decoded alias = %s, want canonical auto_review", encoded)
	}

	encoded, err = json.Marshal(ApprovalsReviewer("guardian_subagent"))
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"auto_review"` {
		t.Fatalf("encoded direct alias = %s, want canonical auto_review", encoded)
	}

	var future ApprovalsReviewer
	if err := json.Unmarshal([]byte(`"future_reviewer"`), &future); err != nil {
		t.Fatal(err)
	}
	if future != ApprovalsReviewer("future_reviewer") {
		t.Fatalf("future = %q, want preserved unknown value", future)
	}
	encoded, err = json.Marshal(future)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"future_reviewer"` {
		t.Fatalf("encoded future = %s, want preserved unknown value", encoded)
	}
}

func isOneOf(value string, options ...string) bool {
	for _, option := range options {
		if value == option {
			return true
		}
	}
	return false
}

func assertJSONEqual(t *testing.T, got []byte, want string) {
	t.Helper()
	var gotValue any
	if err := json.Unmarshal(got, &gotValue); err != nil {
		t.Fatal(err)
	}
	var wantValue any
	if err := json.Unmarshal([]byte(want), &wantValue); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(gotValue, wantValue) {
		t.Fatalf("encoded = %s, want JSON-equivalent %s", got, want)
	}
}
