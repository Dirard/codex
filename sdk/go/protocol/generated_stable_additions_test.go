package protocol

import (
	"encoding/json"
	"testing"
)

func TestGeneratedPluginSummaryDefaults(t *testing.T) {
	var summary PluginSummary
	if err := json.Unmarshal([]byte(`{
		"authPolicy":"ON_USE","enabled":true,"id":"plugin","installPolicy":"NOT_AVAILABLE",
		"installed":true,"name":"Plugin","source":{"type":"remote"}
	}`), &summary); err != nil {
		t.Fatal(err)
	}
	if !summary.LocalVersion.IsNull() {
		t.Fatalf("default local version = %#v, want null", summary.LocalVersion)
	}
	if availability, ok := summary.Availability.Value(); !ok || availability != PluginAvailabilityAvailable {
		t.Fatalf("default availability = %#v, want available", summary.Availability)
	}
	if keywords, ok := summary.Keywords.Value(); !ok || len(keywords) != 0 {
		t.Fatalf("default keywords = %#v, want empty", summary.Keywords)
	}
}

func TestThreadItemsListCursorStringAndAnchor(t *testing.T) {
	var anchor ThreadItemsListAnchor
	if err := json.Unmarshal([]byte(`{"type":"item","itemId":"item-1"}`), &anchor); err != nil {
		t.Fatal(err)
	}
	var reused ThreadItemsListCursor
	for _, tt := range []struct {
		name   string
		cursor ThreadItemsListCursor
		wire   string
	}{
		{"opaque", NewThreadItemsListCursorString("page-2"), `"page-2"`},
		{"anchor", NewThreadItemsListCursorThreadItemsListAnchor(anchor), `{"type":"item","itemId":"item-1"}`},
		{"opaque again", NewThreadItemsListCursorString("page-3"), `"page-3"`},
	} {
		t.Run(tt.name, func(t *testing.T) {
			encoded, err := json.Marshal(ThreadItemsListParams{ThreadID: "thread-1", Cursor: Some(tt.cursor)})
			if err != nil {
				t.Fatal(err)
			}
			assertJSONEqual(t, encoded, `{"threadId":"thread-1","cursor":`+tt.wire+`}`)
			if err := json.Unmarshal([]byte(tt.wire), &reused); err != nil {
				t.Fatal(err)
			}
			encoded, err = json.Marshal(reused)
			if err != nil {
				t.Fatal(err)
			}
			assertJSONEqual(t, encoded, tt.wire)
		})
	}
	for _, invalid := range []string{`42`, `{"type":"item"}`} {
		if err := json.Unmarshal([]byte(invalid), &reused); err == nil {
			t.Fatalf("invalid cursor accepted: %s", invalid)
		}
	}
	encoded, err := json.Marshal(ThreadItemsListParams{ThreadID: "thread-1", Cursor: Null[ThreadItemsListCursor]()})
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{"threadId":"thread-1","cursor":null}`)
}

func TestRealtimeBackendReasoningStatusDefaultAndOptIn(t *testing.T) {
	params := ThreadRealtimeStartParams{BackendReasoningStatus: SomeNonNull(true)}
	if err := json.Unmarshal([]byte(`{"threadId":"thread-1","outputModality":"audio"}`), &params); err != nil {
		t.Fatal(err)
	}
	if value, ok := params.BackendReasoningStatus.Value(); !ok || value {
		t.Fatalf("default backendReasoningStatus = %#v, want false", params.BackendReasoningStatus)
	}
	encoded, err := json.Marshal(params)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{"threadId":"thread-1","outputModality":"audio"}`)
	params.BackendReasoningStatus = SomeNonNull(true)
	encoded, err = json.Marshal(params)
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{"threadId":"thread-1","outputModality":"audio","backendReasoningStatus":true}`)
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

func TestGeneratedInitializeCapabilitiesGatewayOauthDefault(t *testing.T) {
	reused := InitializeCapabilities{ExplicitGatewayOauth: SomeNonNull(true)}
	if err := json.Unmarshal([]byte(`{}`), &reused); err != nil {
		t.Fatal(err)
	}
	value, ok := reused.ExplicitGatewayOauth.Value()
	if !ok || value {
		t.Fatalf("ExplicitGatewayOauth = %#v, want default false", reused.ExplicitGatewayOauth)
	}
	encoded, err := json.Marshal(reused)
	if err != nil {
		t.Fatal(err)
	}
	var omitted map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &omitted); err != nil {
		t.Fatal(err)
	}
	if _, exists := omitted["explicitGatewayOauth"]; exists {
		t.Fatalf("encoded default false field: %s", encoded)
	}

	encoded, err = json.Marshal(InitializeCapabilities{ExplicitGatewayOauth: SomeNonNull(true)})
	if err != nil {
		t.Fatal(err)
	}
	assertJSONEqual(t, encoded, `{"explicitGatewayOauth":true}`)
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

func asDecodeError(err error, target *DecodeError) bool {
	if decodeErr, ok := err.(DecodeError); ok {
		*target = decodeErr
		return true
	}
	return false
}
