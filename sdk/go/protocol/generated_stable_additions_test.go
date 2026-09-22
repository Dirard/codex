package protocol

import (
	"encoding/json"
	"reflect"
	"testing"
)

func TestGeneratedPluginExtensionWireShapes(t *testing.T) {
	var extensions PluginExtensions
	if err := json.Unmarshal([]byte(`{}`), &extensions); err != nil {
		t.Fatal(err)
	}
	if !extensions.Entrypoints.IsSet() || !extensions.Entrypoints.IsNull() {
		t.Fatalf("default entrypoints = %#v, want set null", extensions.Entrypoints)
	}
	for name, field := range map[string]bool{
		"fileHandlers":           extensions.FileHandlers.IsSet(),
		"searchMentionProviders": extensions.SearchMentionProviders.IsSet(),
		"settings":               extensions.Settings.IsSet(),
		"settingsEntrypoints":    extensions.SettingsEntrypoints.IsSet(),
		"threadEntrypoints":      extensions.ThreadEntrypoints.IsSet(),
	} {
		if !field {
			t.Fatalf("default %s was not set to an empty array", name)
		}
	}

	var summary PluginSummary
	if err := json.Unmarshal([]byte(`{
		"authPolicy":"ON_USE","enabled":true,"id":"plugin","installPolicy":"NOT_AVAILABLE",
		"installed":true,"name":"Plugin","source":{"type":"remote"}
	}`), &summary); err != nil {
		t.Fatal(err)
	}
	if !summary.Extensions.IsSet() || !summary.Extensions.IsNull() {
		t.Fatalf("default summary extensions = %#v, want set null", summary.Extensions)
	}

	globalJSON := []byte(`{
		"type":"global",
		"appId":"app",
		"toolName":"tool.open",
		"title":"Open",
		"resourceUri":"ui://open",
		"icons":[{"src":"icon.svg"}],
		"quickAction":{"title":"Run","icons":[],"target":{"type":"tool","name":"tool.open","arguments":{"query":"x"}}}
	}`)
	var global PluginEntrypoint
	if err := json.Unmarshal(globalJSON, &global); err != nil {
		t.Fatal(err)
	}
	if global.TypeValue != "global" || global.QuickAction.IsNull() {
		t.Fatalf("global entrypoint = %#v", global)
	}
	encoded, err := json.Marshal(global)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, string(globalJSON))

	var settings PluginEntrypoint
	if err := json.Unmarshal([]byte(`{
		"type":"settings","appId":"app","toolName":"settings.open","title":"Settings",
		"resourceUri":"ui://settings","icons":[]
	}`), &settings); err != nil {
		t.Fatal(err)
	}
	encoded, err = json.Marshal(settings)
	if err != nil {
		t.Fatal(err)
	}
	if !containsJSONField(t, encoded, "searchTerms", []any{}) {
		t.Fatalf("encoded settings = %s; want default searchTerms array", encoded)
	}

	for _, payload := range []string{
		`{"type":"thread","appId":"app","toolName":"thread.open","title":"Thread","resourceUri":"ui://thread","icons":[]}`,
		`{"type":"file","appId":"app","toolName":"file.open","title":"File","resourceUri":"ui://file","icons":[],"extensions":[".txt"]}`,
	} {
		var entrypoint PluginEntrypoint
		if err := json.Unmarshal([]byte(payload), &entrypoint); err != nil {
			t.Fatal(err)
		}
		encoded, err = json.Marshal(entrypoint)
		if err != nil {
			t.Fatal(err)
		}
		assertJSONEqual(t, encoded, payload)
	}

	providerJSON := []byte(`{
		"appId":"app","toolName":"search","linkId":"link","title":"Search",
		"call":{"name":"search.call","arguments":{"query":"x"},"_meta":{"origin":"test"}}
	}`)
	var provider PluginSearchProvider
	if err := json.Unmarshal(providerJSON, &provider); err != nil {
		t.Fatal(err)
	}
	call, ok := provider.Call.Value()
	if !ok || call.Name != "search.call" || string(call.Meta) != `{"origin":"test"}` {
		t.Fatalf("search provider call = %#v", provider.Call)
	}
	encoded, err = json.Marshal(provider)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, string(providerJSON))

	futureJSON := []byte(`{"type":"future","appId":"app"}`)
	var future PluginEntrypoint
	if err := json.Unmarshal(futureJSON, &future); err != nil {
		t.Fatal(err)
	}
	if future.TypeValue != "future" || string(future.RawJSON) != string(futureJSON) {
		t.Fatalf("future entrypoint = %#v, raw = %s", future, future.RawJSON)
	}
}

func TestGeneratedNullableStableAdditions(t *testing.T) {
	target := McpResourceReadParams{
		Server: "codex_apps",
		Uri:    "app://resource",
		Target: Some(McpResourceReadTarget{
			ConnectorID: "connector",
			LinkID:      Null[string](),
		}),
	}
	encoded, err := json.Marshal(target)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{
		"server":"codex_apps","uri":"app://resource",
		"target":{"connectorId":"connector","linkId":null}
	}`)

	var missingLink McpResourceReadTarget
	err = json.Unmarshal([]byte(`{"connectorId":"connector"}`), &missingLink)
	var decodeErr DecodeError
	if !asDecodeError(err, &decodeErr) || decodeErr.Field != "linkId" {
		t.Fatalf("err = %T(%v), want missing linkId DecodeError", err, err)
	}

	var status McpServerStatus
	if err := json.Unmarshal([]byte(`{
		"authStatus":"none","name":"server","resourceTemplates":[],"resources":[],
		"tools":{},"httpOrigin":null
	}`), &status); err != nil {
		t.Fatal(err)
	}
	if !status.HttpOrigin.IsSet() || !status.HttpOrigin.IsNull() {
		t.Fatalf("HttpOrigin = %#v, want set null", status.HttpOrigin)
	}

	var item ThreadItemEntry
	if err := json.Unmarshal([]byte(`{
		"item":{"type":"contextCompaction","id":"item-1"},
		"turnId":"turn-1","startedAtMs":1,"completedAtMs":null
	}`), &item); err != nil {
		t.Fatal(err)
	}
	if started, ok := item.StartedAtMs.Value(); !ok || started != 1 {
		t.Fatalf("StartedAtMs = %#v", item.StartedAtMs)
	}
	if !item.CompletedAtMs.IsSet() || !item.CompletedAtMs.IsNull() {
		t.Fatalf("CompletedAtMs = %#v, want set null", item.CompletedAtMs)
	}
}

func TestGeneratedGatewayOAuthNotificationWireShape(t *testing.T) {
	payload := []byte(`{
		"authUrl":null,"providerId":"gateway","status":"notReady","error":null
	}`)
	var notification GatewayOAuthChangedNotification
	if err := json.Unmarshal(payload, &notification); err != nil {
		t.Fatal(err)
	}
	if notification.ProviderID != "gateway" || notification.Status != GatewayOAuthStatusNotReady {
		t.Fatalf("notification = %#v", notification)
	}
	if !notification.AuthURL.IsSet() || !notification.AuthURL.IsNull() ||
		!notification.Error.IsSet() || !notification.Error.IsNull() {
		t.Fatalf("nullable fields = %#v/%#v", notification.AuthURL, notification.Error)
	}

	encoded, err := json.Marshal(notification)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, string(payload))
}

func containsJSONField(t *testing.T, data json.RawMessage, field string, want any) bool {
	t.Helper()
	var object map[string]json.RawMessage
	if err := json.Unmarshal(data, &object); err != nil {
		t.Fatal(err)
	}
	var got any
	if err := json.Unmarshal(object[field], &got); err != nil {
		t.Fatal(err)
	}
	return reflect.DeepEqual(got, want)
}

func asDecodeError(err error, target *DecodeError) bool {
	if decodeErr, ok := err.(DecodeError); ok {
		*target = decodeErr
		return true
	}
	return false
}
