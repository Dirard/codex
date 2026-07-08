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
