package codex

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestRawClientPublicShape(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	threadResponse := protocol.ThreadReadResponse{
		Thread: protocol.Thread{
			CliVersion:    "0.0.0-dev",
			CreatedAt:     1,
			Cwd:           protocol.AbsolutePathBuf("/tmp"),
			ID:            "thread-1",
			ModelProvider: "test",
			Preview:       "",
			SessionID:     "session-1",
			Source:        protocol.SessionSourceCli,
			Status:        protocol.ThreadStatus{TypeValue: "idle"},
			Turns:         []protocol.Turn{},
			UpdatedAt:     1,
		},
	}
	payload, err := json.Marshal(threadResponse)
	if err != nil {
		t.Fatal(err)
	}
	transport.responses["thread/read"] = payload
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	_, err = client.Raw().ThreadRead(context.Background(), protocol.ThreadReadParams{ThreadID: "thread-1"})
	if err != nil {
		t.Fatal(err)
	}
}

func TestRawStableModeRejectsExperimentalMethodBeforeWrite(t *testing.T) {
	transport := newScriptedInitializedTransport(t, stableInitializePayload())
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:    transport,
		ProtocolMode: ProtocolModeStable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	before := len(transport.sentFrames())
	params := protocol.NullableRemoteControlEnableParams(protocol.Some(protocol.RemoteControlEnableParams{}))
	_, err = client.Raw().RemoteControlEnable(context.Background(), params)
	if err == nil {
		t.Fatal("expected stable-mode experimental method error")
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("experimental method reached transport")
	}
}

func TestDirectClientCallUsesAuthoritativeMetadataForExperimentalGate(t *testing.T) {
	transport := newScriptedInitializedTransport(t, stableInitializePayload())
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:    transport,
		ProtocolMode: ProtocolModeStable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	params := protocol.NullableRemoteControlEnableParams(protocol.Some(protocol.RemoteControlEnableParams{}))
	err = client.Call(context.Background(), "remoteControl/enable", params, nil, protocol.MethodMetadata{})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("experimental method reached transport with empty metadata")
	}
}

func TestDirectClientCallUsesAuthoritativeMetadataForAdditionalContextBounds(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Limits: ClientLimits{
			MaxAdditionalContextEntries:    1,
			MaxAdditionalContextKeyBytes:   8,
			MaxAdditionalContextValueBytes: 3,
			MaxAdditionalContextTotalBytes: 8,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	params := protocol.TurnStartParams{
		ThreadID: "thread-1",
		AdditionalContext: protocol.Some(map[string]protocol.AdditionalContextEntry{
			"note": {Kind: protocol.AdditionalContextKindUntrusted, Value: "1234"},
		}),
	}
	err = client.Call(context.Background(), "turn/start", params, nil, protocol.MethodMetadata{})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("over-limit additionalContext reached transport with empty metadata")
	}
}

func TestRawClientUsesProtocolAdditionalContextCapsWhenLimitsAreRaised(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Limits: ClientLimits{
			MaxAdditionalContextEntries:    protocol.MaxAdditionalContextEntries + 1,
			MaxAdditionalContextKeyBytes:   protocol.MaxAdditionalContextKeyBytes + 1,
			MaxAdditionalContextValueBytes: protocol.MaxAdditionalContextValueBytes + 1,
			MaxAdditionalContextTotalBytes: protocol.MaxAdditionalContextTotalBytes + 1,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	additionalContext := protocol.Some(map[string]protocol.AdditionalContextEntry{
		"note": {
			Kind:  protocol.AdditionalContextKindUntrusted,
			Value: strings.Repeat("x", protocol.MaxAdditionalContextValueBytes+1),
		},
	})

	beforeTurnStart := len(transport.sentFrames())
	_, err = client.Raw().TurnStart(context.Background(), protocol.TurnStartParams{
		ThreadID:          "thread-1",
		AdditionalContext: additionalContext,
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) || !strings.Contains(configErr.Reason, "value") {
		t.Fatalf("turn/start err = %T %v, want additionalContext value ConfigError", err, err)
	}
	if len(transport.sentFrames()) != beforeTurnStart {
		t.Fatal("over-protocol-cap raw turn/start additionalContext reached transport")
	}

	beforeTurnSteer := len(transport.sentFrames())
	_, err = client.Raw().TurnSteer(context.Background(), protocol.TurnSteerParams{
		ThreadID:          "thread-1",
		ExpectedTurnID:    "turn-1",
		Input:             []protocol.UserInput{{TypeValue: "text", Text: protocol.SomeNonNull("continue")}},
		AdditionalContext: additionalContext,
	})
	if !errors.As(err, &configErr) || !strings.Contains(configErr.Reason, "value") {
		t.Fatalf("turn/steer err = %T %v, want additionalContext value ConfigError", err, err)
	}
	if len(transport.sentFrames()) != beforeTurnSteer {
		t.Fatal("over-protocol-cap raw turn/steer additionalContext reached transport")
	}
}

func TestDirectClientCallRejectsMismatchedMetadataBeforeWrite(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	err = client.Call(context.Background(), "thread/read", protocol.ThreadReadParams{ThreadID: "thread-1"}, nil, protocol.MethodMetadataByMethod["turn/start"])
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("mismatched metadata call reached transport")
	}
}

func TestDirectClientCallRejectsNonPublicMethodsBeforeWrite(t *testing.T) {
	tests := []struct {
		name   string
		method string
		params any
	}{
		{
			name:   "handshake only initialize",
			method: "initialize",
			params: protocol.InitializeParams{},
		},
		{
			name:   "compatibility only v1 method",
			method: "getConversationSummary",
		},
		{
			name:   "compatibility only git diff method",
			method: "gitDiffToRemote",
		},
		{
			name:   "compatibility only auth status method",
			method: "getAuthStatus",
		},
		{
			name:   "internal test-only experimental method",
			method: "mock/experimentalMethod",
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

			before := len(transport.sentFrames())
			err = client.Call(context.Background(), tt.method, tt.params, nil, protocol.MethodMetadata{})
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T, want *ConfigError", err)
			}
			if len(transport.sentFrames()) != before {
				t.Fatalf("non-public method %q reached transport", tt.method)
			}
		})
	}
}

func TestRawStableModeHonorsExperimentalDiscriminators(t *testing.T) {
	transport := newScriptedInitializedTransport(t, stableInitializePayload())
	transport.responses["account/login/start"] = json.RawMessage(`{"type":"apiKey"}`)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:    transport,
		ProtocolMode: ProtocolModeStable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	_, err = client.Raw().AccountLoginStart(context.Background(), protocol.LoginAccountParams{
		TypeValue: "apiKey",
		APIKey:    protocol.SomeNonNull("test-key"),
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(transport.sentFrames()) != before+1 {
		t.Fatal("stable apiKey login did not reach transport")
	}

	before = len(transport.sentFrames())
	_, err = client.Raw().AccountLoginStart(context.Background(), protocol.LoginAccountParams{
		TypeValue:        "chatgptAuthTokens",
		AccessToken:      protocol.SomeNonNull("test-token"),
		ChatgptAccountID: protocol.SomeNonNull("account-1"),
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("experimental discriminator variant reached transport")
	}
}

func TestRawStableModeRejectsPresenceBasedExperimentalFieldBeforeWrite(t *testing.T) {
	transport := newScriptedInitializedTransport(t, stableInitializePayload())
	client, err := NewClient(context.Background(), ClientConfig{
		Transport:    transport,
		ProtocolMode: ProtocolModeStable,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	params := protocol.TurnStartParams{
		ThreadID: "thread-1",
		AdditionalContext: protocol.Some(map[string]protocol.AdditionalContextEntry{
			"note": {Kind: protocol.AdditionalContextKindUntrusted, Value: "bounded"},
		}),
	}
	err = client.Call(context.Background(), "turn/start", params, nil, protocol.MethodMetadataByMethod["turn/start"])
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("presence-based experimental field reached transport")
	}
}

func TestRawInitializeIsNotExposedButTypesExist(t *testing.T) {
	raw := protocol.NewRawClient(noopSender{})
	_ = raw
	_ = protocol.InitializeParams{}
	_ = protocol.InitializeResponse{}
}

func TestRawTurnAdditionalContextLimits(t *testing.T) {
	tests := []struct {
		name    string
		limits  ClientLimits
		context map[string]protocol.AdditionalContextEntry
		wantErr string
	}{
		{
			name: "below limits",
			limits: ClientLimits{
				MaxAdditionalContextEntries:    2,
				MaxAdditionalContextKeyBytes:   8,
				MaxAdditionalContextValueBytes: 8,
				MaxAdditionalContextTotalBytes: 20,
			},
			context: map[string]protocol.AdditionalContextEntry{
				"a": {Kind: protocol.AdditionalContextKindUntrusted, Value: "123"},
			},
		},
		{
			name: "at limits",
			limits: ClientLimits{
				MaxAdditionalContextEntries:    2,
				MaxAdditionalContextKeyBytes:   4,
				MaxAdditionalContextValueBytes: 4,
				MaxAdditionalContextTotalBytes: 12,
			},
			context: map[string]protocol.AdditionalContextEntry{
				"key1": {Kind: protocol.AdditionalContextKindUntrusted, Value: "val1"},
				"k2":   {Kind: protocol.AdditionalContextKindApplication, Value: "v2"},
			},
		},
		{
			name: "entry count over",
			limits: ClientLimits{
				MaxAdditionalContextEntries:    1,
				MaxAdditionalContextKeyBytes:   8,
				MaxAdditionalContextValueBytes: 8,
				MaxAdditionalContextTotalBytes: 20,
			},
			context: map[string]protocol.AdditionalContextEntry{
				"a": {Kind: protocol.AdditionalContextKindUntrusted, Value: "1"},
				"b": {Kind: protocol.AdditionalContextKindApplication, Value: "2"},
			},
			wantErr: "entry count",
		},
		{
			name: "key bytes over",
			limits: ClientLimits{
				MaxAdditionalContextEntries:    2,
				MaxAdditionalContextKeyBytes:   3,
				MaxAdditionalContextValueBytes: 8,
				MaxAdditionalContextTotalBytes: 20,
			},
			context: map[string]protocol.AdditionalContextEntry{
				"key4": {Kind: protocol.AdditionalContextKindUntrusted, Value: "1"},
			},
			wantErr: "key",
		},
		{
			name: "value bytes over",
			limits: ClientLimits{
				MaxAdditionalContextEntries:    2,
				MaxAdditionalContextKeyBytes:   8,
				MaxAdditionalContextValueBytes: 3,
				MaxAdditionalContextTotalBytes: 20,
			},
			context: map[string]protocol.AdditionalContextEntry{
				"a": {Kind: protocol.AdditionalContextKindUntrusted, Value: "1234"},
			},
			wantErr: "value",
		},
		{
			name: "total bytes over",
			limits: ClientLimits{
				MaxAdditionalContextEntries:    2,
				MaxAdditionalContextKeyBytes:   8,
				MaxAdditionalContextValueBytes: 8,
				MaxAdditionalContextTotalBytes: 5,
			},
			context: map[string]protocol.AdditionalContextEntry{
				"aa": {Kind: protocol.AdditionalContextKindUntrusted, Value: "22"},
				"bb": {Kind: protocol.AdditionalContextKindApplication, Value: "33"},
			},
			wantErr: "total",
		},
	}

	for _, tt := range tests {
		for _, method := range []string{"turn/start", "turn/steer"} {
			t.Run(tt.name+"/"+method, func(t *testing.T) {
				transport := newScriptedInitializedTransport(t, nil)
				client, err := NewClient(context.Background(), ClientConfig{
					Transport: transport,
					Limits:    tt.limits,
				})
				if err != nil {
					t.Fatal(err)
				}
				t.Cleanup(func() { _ = client.Close() })

				before := len(transport.sentFrames())
				err = client.Call(context.Background(), method, rawTurnParams(method, tt.context), nil, protocol.MethodMetadataByMethod[method])
				if tt.wantErr == "" {
					if err != nil {
						t.Fatal(err)
					}
					if len(transport.sentFrames()) != before+1 {
						t.Fatal("bounded additionalContext call did not reach transport")
					}
					return
				}
				var configErr *ConfigError
				if !errors.As(err, &configErr) {
					t.Fatalf("err = %T, want *ConfigError", err)
				}
				if !strings.Contains(configErr.Reason, tt.wantErr) {
					t.Fatalf("reason = %q, want containing %q", configErr.Reason, tt.wantErr)
				}
				if len(transport.sentFrames()) != before {
					t.Fatal("over-limit additionalContext reached transport")
				}
			})
		}
	}
}

func rawTurnParams(method string, additionalContext map[string]protocol.AdditionalContextEntry) any {
	switch method {
	case "turn/start":
		return protocol.TurnStartParams{
			ThreadID:          "thread-1",
			Input:             []protocol.UserInput{},
			AdditionalContext: protocol.Some(additionalContext),
		}
	case "turn/steer":
		return protocol.TurnSteerParams{
			ThreadID:          "thread-1",
			ExpectedTurnID:    "turn-1",
			Input:             []protocol.UserInput{},
			AdditionalContext: protocol.Some(additionalContext),
		}
	default:
		panic("unsupported raw turn method")
	}
}

func TestInjectedTransportAdapterRejectsOversizedOutboundFrame(t *testing.T) {
	adapter := publicTransportAdapter{
		transport:     newScriptedInitializedTransport(t, nil),
		maxFrameBytes: 5,
	}
	err := adapter.Send(context.Background(), json.RawMessage(`{"too":"large"}`))
	var sizeErr *FrameSizeError
	if !errors.As(err, &sizeErr) {
		t.Fatalf("err = %T, want *FrameSizeError", err)
	}
}

type noopSender struct{}

func (noopSender) Call(context.Context, string, any, any, protocol.MethodMetadata) error {
	return nil
}
