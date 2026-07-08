package protocol

import (
	"encoding/json"
	"fmt"
)

type ClientNotificationMethod string

const (
	ClientNotificationInitialized ClientNotificationMethod = "initialized"
)

type ClientNotification struct {
	Method ClientNotificationMethod `json:"method"`
	Params json.RawMessage          `json:"params,omitempty"`
}

type InitializedNotification struct{}

func NewInitializedNotification() ClientNotification {
	return InitializedNotification{}.ClientNotification()
}

func (InitializedNotification) ClientNotification() ClientNotification {
	return ClientNotification{Method: ClientNotificationInitialized}
}

func (n ClientNotification) MarshalJSON() ([]byte, error) {
	switch n.Method {
	case ClientNotificationInitialized:
	default:
		return nil, fmt.Errorf("unsupported client notification method %q", n.Method)
	}
	type wire struct {
		Method ClientNotificationMethod `json:"method"`
		Params json.RawMessage          `json:"params,omitempty"`
	}
	return json.Marshal(wire{Method: n.Method, Params: n.Params})
}
