package codex

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestRemoteControlThinWrappers(t *testing.T) {
	tests := []struct {
		name   string
		method string
		call   func(context.Context, *Client) error
	}{
		{
			name:   "enable",
			method: "remoteControl/enable",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.RemoteControl.Enable(ctx, protocol.NullableRemoteControlEnableParams{})
				return err
			},
		},
		{
			name:   "disable",
			method: "remoteControl/disable",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.RemoteControl.Disable(ctx, protocol.NullableRemoteControlDisableParams{})
				return err
			},
		},
		{
			name:   "read-status",
			method: "remoteControl/status/read",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.RemoteControl.ReadStatus(ctx)
				return err
			},
		},
		{
			name:   "list-clients",
			method: "remoteControl/client/list",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.RemoteControl.ListClients(ctx, protocol.RemoteControlClientsListParams{EnvironmentID: "env-1"})
				return err
			},
		},
		{
			name:   "revoke-client",
			method: "remoteControl/client/revoke",
			call: func(ctx context.Context, client *Client) error {
				_, err := client.RemoteControl.RevokeClient(ctx, protocol.RemoteControlClientsRevokeParams{
					ClientID:      "client-1",
					EnvironmentID: "env-1",
				})
				return err
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })
			failMethod(transport, tt.method)

			err = tt.call(context.Background(), client)
			var rpcErr *RPCError
			if !errors.As(err, &rpcErr) {
				t.Fatalf("err = %T(%v), want *RPCError", err, err)
			}
			assertMethod(t, transport.lastFrame(t), tt.method)
		})
	}
}

func TestRemoteControlPairingHandleInjectsPairingCodes(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["remoteControl/pairing/start"] = mustJSON(t, protocol.RemoteControlPairingStartResponse{
		EnvironmentID:     "env-1",
		ExpiresAt:         123,
		ManualPairingCode: protocol.Some("manual-code"),
		PairingCode:       "pair-code",
	})
	transport.responses["remoteControl/pairing/status"] = mustJSON(t, protocol.RemoteControlPairingStatusResponse{Claimed: false})
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	pairing, start, err := client.RemoteControl.StartPairing(context.Background(), RemoteControlPairingOptions{ManualCode: true})
	if err != nil {
		t.Fatal(err)
	}
	if pairing.ID() != "pair-code" || pairing.ManualPairingCode() != "manual-code" || pairing.EnvironmentID() != "env-1" || pairing.ExpiresAt() != 123 {
		t.Fatalf("pairing handle = %#v start = %#v", pairing, start)
	}
	assertRequestBoolParam(t, requestParamsForMethod(t, transport, "remoteControl/pairing/start"), "manualCode", true)

	status, err := pairing.Status(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if status.Claimed {
		t.Fatalf("status = %#v, want unclaimed", status)
	}
	params := requestParamsForMethod(t, transport, "remoteControl/pairing/status")
	assertRequestStringParam(t, params, "pairingCode", "pair-code")
	assertRequestParamAbsent(t, params, "manualPairingCode")

	transport.responses["remoteControl/pairing/status"] = mustJSON(t, protocol.RemoteControlPairingStatusResponse{Claimed: true})
	status, err = pairing.Wait(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !status.Claimed {
		t.Fatalf("wait status = %#v, want claimed", status)
	}

	status, err = client.RemoteControl.PairingStatus(context.Background(), pairing.ID())
	if err != nil {
		t.Fatal(err)
	}
	if !status.Claimed {
		t.Fatalf("root status = %#v, want claimed", status)
	}
	assertRequestStringParam(t, requestParamsForMethod(t, transport, "remoteControl/pairing/status"), "pairingCode", "pair-code")
}

func TestRemoteControlPairingStatusRejectsEmptyCodeBeforeSend(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	_, err = client.RemoteControl.PairingStatus(context.Background(), "")
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T(%v), want *ConfigError", err, err)
	}
	if methodWasSent(t, transport, "remoteControl/pairing/status") {
		t.Fatal("remoteControl/pairing/status was sent with empty pairing code")
	}
}

func TestRemoteControlStatusChangedNotificationRoutes(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	stream, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	transport.deliverNotification("remoteControl/status/changed", mustJSON(t, protocol.RemoteControlStatusChangedNotification{
		InstallationID: "installation-1",
		ServerName:     "server-1",
		Status:         protocol.RemoteControlConnectionStatusConnected,
	}), nil)
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	notification, ok := stream.Next(ctx)
	if !ok {
		t.Fatalf("stream closed before remoteControl/status/changed: %v", stream.Err())
	}
	if notification.Method != "remoteControl/status/changed" {
		t.Fatalf("notification method = %q, want remoteControl/status/changed", notification.Method)
	}
	payload, ok := notification.Payload.(protocol.RemoteControlStatusChangedNotification)
	if !ok {
		t.Fatalf("notification payload = %T, want protocol.RemoteControlStatusChangedNotification", notification.Payload)
	}
	if payload.InstallationID != "installation-1" || payload.ServerName != "server-1" {
		data, _ := json.Marshal(payload)
		t.Fatalf("notification payload = %s", data)
	}
}
