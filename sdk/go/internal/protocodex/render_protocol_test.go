package protocodex

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestRenderClientNotificationsRejectsParamsBearingNotification(t *testing.T) {
	manifest := &Manifest{
		Experimental: ManifestMode{
			ClientNotifications: []NotificationEntry{
				{
					Direction:   "clientNotification",
					Method:      "initialized",
					PayloadType: "InitializedParams",
				},
			},
		},
	}
	schema := &SchemaBundle{
		Definitions: map[string]Schema{
			"ClientNotification": {
				OneOf: []Schema{
					{
						Type: "object",
						Properties: map[string]Schema{
							"method": {
								Type: "string",
								Enum: []json.RawMessage{json.RawMessage(`"initialized"`)},
							},
							"params": {
								Ref: "#/definitions/InitializedParams",
							},
						},
						Required: []string{"method", "params"},
					},
				},
			},
			"InitializedParams": {
				Type: "object",
				Properties: map[string]Schema{
					"ok": {Type: "boolean"},
				},
			},
		},
	}

	_, err := renderClientNotifications(manifest, schema)
	if err == nil || !strings.Contains(err.Error(), "params-bearing client notification") {
		t.Fatalf("err = %v, want params-bearing client notification rejection", err)
	}
}

func TestGeneratedOnlyServerNotificationsAreNotRendered(t *testing.T) {
	manifest := &Manifest{
		Experimental: ManifestMode{
			ServerNotifications: []NotificationEntry{
				{
					Method:        "rawResponse/completed",
					PayloadType:   "RawResponseCompletedNotification",
					SDKVisibility: "generatedOnly",
				},
				{
					Method:        "rawResponseItem/completed",
					PayloadType:   "RawResponseItemCompletedNotification",
					SDKVisibility: "generatedOnly",
				},
			},
		},
	}

	rendered := renderServerNotificationMetadata(manifest, nil)
	for _, method := range []string{"rawResponse/completed", "rawResponseItem/completed"} {
		if strings.Contains(rendered, method) {
			t.Fatalf("generated-only server notification %q leaked into generated metadata", method)
		}
	}
	if variants := jsonRPCUnionVariants("ServerNotification", manifest); len(variants) != 0 {
		t.Fatalf("generated-only server notification variants = %#v, want none", variants)
	}
}
