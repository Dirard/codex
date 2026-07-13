package codex

import (
	"context"
	"encoding/json"
	"errors"
	"reflect"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestInitializeStrictSuccessSendsGeneratedInitialized(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	sent := transport.sentFrames()
	if len(sent) < 2 {
		t.Fatalf("sent frames = %d", len(sent))
	}
	assertMethod(t, sent[0], "initialize")
	assertMethod(t, sent[1], "initialized")
	if !transport.initializedWasGenerated {
		t.Fatal("initialized fixture did not verify generated notification path")
	}
}

func TestInitializeAdvertisesMcpElicitationCapabilityOnlyWithHandler(t *testing.T) {
	withoutHandler := newScriptedInitializedTransport(t, nil)
	withoutClient, err := NewClient(context.Background(), ClientConfig{Transport: withoutHandler})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = withoutClient.Close() })
	assertInitializeMcpElicitationCapability(t, withoutHandler.sentFrames()[0], false)

	withHandler := newScriptedInitializedTransport(t, nil)
	withClient, err := NewClient(context.Background(), ClientConfig{
		Transport: withHandler,
		Handlers: ServerHandlers{
			MCPElicitation: MCPElicitationMcpServerElicitationRequestFunc(func(context.Context, protocol.McpServerElicitationRequestParams) (protocol.McpServerElicitationRequestResponse, error) {
				return protocol.McpServerElicitationRequestResponse{}, nil
			}),
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = withClient.Close() })
	assertInitializeMcpElicitationCapability(t, withHandler.sentFrames()[0], true)
}

func TestStrictRejectsLegacyInitializeBeforeInitialized(t *testing.T) {
	transport := newScriptedInitializedTransport(t, legacyInitializePayload())
	_, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	var compatErr *CompatibilityError
	if !errors.As(err, &compatErr) {
		t.Fatalf("err = %T, want *CompatibilityError", err)
	}
	requiredOverride := CompatibilityAllowProtocolDigestUnavailable
	want := &CompatibilityError{
		Reason:           "initialize response missing selected protocol digest",
		ExpectedDigest:   protocol.ExperimentalProtocolDigest,
		ExpectedMode:     ProtocolModeExperimental,
		RuntimeVersion:   "0.0.0",
		UserAgent:        "codex-go-test dev 0.0.0",
		RequiredOverride: &requiredOverride,
	}
	if !reflect.DeepEqual(compatErr, want) {
		t.Fatalf("compatibility error = %#v, want %#v", compatErr, want)
	}
	for _, frame := range transport.sentFrames() {
		if methodFromFrame(t, frame) == "initialized" {
			t.Fatal("initialized sent after incompatible legacy initialize")
		}
	}

	var generated protocol.InitializeResponse
	if err := json.Unmarshal(legacyInitializePayload(), &generated); err == nil {
		t.Fatal("generated InitializeResponse accepted legacy payload")
	}
}

func TestCompatibilityOverrideAcceptsLegacyOnlyForInjectedDevRuntime(t *testing.T) {
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:     newScriptedInitializedTransport(t, legacyInitializePayload()),
		Compatibility: CompatibilityAllowProtocolDigestUnavailable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	if !client.Metadata().CompatibilityOverrideActive {
		t.Fatal("compatibility override metadata not set")
	}
}

func TestCompatibilityOverridesRejectMalformedCurrentInitialize(t *testing.T) {
	payload := malformedCurrentInitializePayload(t)
	var generated protocol.InitializeResponse
	if err := json.Unmarshal(payload, &generated); err == nil {
		t.Fatal("generated InitializeResponse accepted malformed current payload")
	}

	for _, tt := range []struct {
		name   string
		policy CompatibilityPolicy
	}{
		{name: "digest unavailable", policy: CompatibilityAllowProtocolDigestUnavailable},
		{name: "dev build", policy: CompatibilityAllowDevBuild},
	} {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, payload)
			_, err := NewClient(context.Background(), ClientConfig{
				Transport:     transport,
				Compatibility: tt.policy,
			})
			if err == nil {
				t.Fatal("expected malformed current initialize to fail")
			}
			for _, frame := range transport.sentFrames() {
				if methodFromFrame(t, frame) == "initialized" {
					t.Fatal("initialized sent after malformed current initialize")
				}
			}
		})
	}
}

func TestDigestUnavailableOverrideRejectsDigestWithoutActiveMode(t *testing.T) {
	payload := currentInitializePayloadWithoutActiveMode(t)
	transport := newScriptedInitializedTransport(t, payload)
	_, err := NewClient(context.Background(), ClientConfig{
		Transport:     transport,
		Compatibility: CompatibilityAllowProtocolDigestUnavailable,
	})
	if err == nil {
		t.Fatal("expected digest-present initialize without activeProtocolMode to fail")
	}
	for _, frame := range transport.sentFrames() {
		if methodFromFrame(t, frame) == "initialized" {
			t.Fatal("initialized sent after digest-present initialize without activeProtocolMode")
		}
	}
}

func TestDigestUnavailableOverrideRejectsNonSelectedDigest(t *testing.T) {
	tests := []struct {
		name        string
		mode        ProtocolMode
		digestField string
		digestValue string
	}{
		{
			name:        "experimental mode rejects stable digest",
			mode:        ProtocolModeExperimental,
			digestField: "stableProtocolDigest",
			digestValue: "mismatch",
		},
		{
			name:        "stable mode rejects experimental digest",
			mode:        ProtocolModeStable,
			digestField: "experimentalProtocolDigest",
			digestValue: "mismatch",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			payload := legacyInitializePayloadWithField(t, tt.digestField, tt.digestValue)
			transport := newScriptedInitializedTransport(t, payload)
			_, err := NewClient(context.Background(), ClientConfig{
				Transport:     transport,
				ProtocolMode:  tt.mode,
				Compatibility: CompatibilityAllowProtocolDigestUnavailable,
			})
			if err == nil {
				t.Fatal("expected non-selected digest to fail")
			}
			for _, frame := range transport.sentFrames() {
				if methodFromFrame(t, frame) == "initialized" {
					t.Fatal("initialized sent after non-selected digest")
				}
			}
		})
	}
}

func TestInitializeCompatibilityEnvelopeRejectsMalformedOptionalDigest(t *testing.T) {
	var object map[string]any
	if err := json.Unmarshal(currentInitializePayload(), &object); err != nil {
		t.Fatal(err)
	}
	object["experimentalProtocolDigest"] = 123
	payload, err := json.Marshal(object)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := decodeInitializeCompatibility(payload); err == nil {
		t.Fatal("malformed optional digest decoded as absent")
	}
}

func TestStrictRejectsDigestMismatchAndWrongMode(t *testing.T) {
	payload := currentInitializePayload()
	var object map[string]any
	if err := json.Unmarshal(payload, &object); err != nil {
		t.Fatal(err)
	}
	object["experimentalProtocolDigest"] = "mismatch"
	payload, _ = json.Marshal(object)
	_, err := NewClient(context.Background(), ClientConfig{Transport: newScriptedInitializedTransport(t, payload)})
	var compatErr *CompatibilityError
	if !errors.As(err, &compatErr) {
		t.Fatalf("err = %T, want *CompatibilityError", err)
	}
	foundExperimentalMode := ProtocolModeExperimental
	requiredOverride := CompatibilityAllowDevBuild
	want := &CompatibilityError{
		Reason:           "initialize response protocol digest mismatch",
		ExpectedDigest:   protocol.ExperimentalProtocolDigest,
		FoundDigest:      "mismatch",
		ExpectedMode:     ProtocolModeExperimental,
		FoundMode:        &foundExperimentalMode,
		RuntimeVersion:   "0.0.0",
		UserAgent:        "codex-go-test dev 0.0.0",
		RequiredOverride: &requiredOverride,
	}
	if !reflect.DeepEqual(compatErr, want) {
		t.Fatalf("digest mismatch compatibility error = %#v, want %#v", compatErr, want)
	}

	payload = currentInitializePayload()
	if err := json.Unmarshal(payload, &object); err != nil {
		t.Fatal(err)
	}
	object["activeProtocolMode"] = "stable"
	payload, _ = json.Marshal(object)
	_, err = NewClient(context.Background(), ClientConfig{Transport: newScriptedInitializedTransport(t, payload)})
	if !errors.As(err, &compatErr) {
		t.Fatalf("err = %T, want *CompatibilityError", err)
	}
	foundStableMode := ProtocolModeStable
	want = &CompatibilityError{
		Reason:         "initialize response activeProtocolMode mismatch",
		ExpectedDigest: protocol.ExperimentalProtocolDigest,
		FoundDigest:    protocol.ExperimentalProtocolDigest,
		ExpectedMode:   ProtocolModeExperimental,
		FoundMode:      &foundStableMode,
		RuntimeVersion: "0.0.0",
		UserAgent:      "codex-go-test dev 0.0.0",
	}
	if !reflect.DeepEqual(compatErr, want) {
		t.Fatalf("mode mismatch compatibility error = %#v, want %#v", compatErr, want)
	}
}

func assertInitializeMcpElicitationCapability(t *testing.T, frame json.RawMessage, want bool) {
	t.Helper()
	var envelope struct {
		Method string          `json:"method"`
		Params json.RawMessage `json:"params"`
	}
	if err := json.Unmarshal(frame, &envelope); err != nil {
		t.Fatal(err)
	}
	if envelope.Method != "initialize" {
		t.Fatalf("method = %q, want initialize", envelope.Method)
	}
	var params struct {
		Capabilities json.RawMessage `json:"capabilities"`
	}
	if err := json.Unmarshal(envelope.Params, &params); err != nil {
		t.Fatal(err)
	}
	var capabilities map[string]json.RawMessage
	if err := json.Unmarshal(params.Capabilities, &capabilities); err != nil {
		t.Fatal(err)
	}
	got, ok := capabilities["mcpServerOpenaiFormElicitation"]
	if ok != want {
		t.Fatalf("mcpServerOpenaiFormElicitation present = %v, want %v", ok, want)
	}
	if want && string(got) != "true" {
		t.Fatalf("mcpServerOpenaiFormElicitation = %s, want true", got)
	}
}

func malformedCurrentInitializePayload(t *testing.T) json.RawMessage {
	t.Helper()
	var object map[string]any
	if err := json.Unmarshal(currentInitializePayload(), &object); err != nil {
		t.Fatal(err)
	}
	delete(object, "stableSchemaDigest")
	delete(object, "experimentalSchemaDigest")
	delete(object, "stableManifestDigest")
	delete(object, "experimentalManifestDigest")
	payload, err := json.Marshal(object)
	if err != nil {
		t.Fatal(err)
	}
	return payload
}

func currentInitializePayloadWithoutActiveMode(t *testing.T) json.RawMessage {
	t.Helper()
	var object map[string]any
	if err := json.Unmarshal(currentInitializePayload(), &object); err != nil {
		t.Fatal(err)
	}
	delete(object, "activeProtocolMode")
	payload, err := json.Marshal(object)
	if err != nil {
		t.Fatal(err)
	}
	return payload
}

func legacyInitializePayloadWithField(t *testing.T, field string, value any) json.RawMessage {
	t.Helper()
	var object map[string]any
	if err := json.Unmarshal(legacyInitializePayload(), &object); err != nil {
		t.Fatal(err)
	}
	object[field] = value
	payload, err := json.Marshal(object)
	if err != nil {
		t.Fatal(err)
	}
	return payload
}
