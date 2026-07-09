package codex

import (
	"context"
	"errors"
	"testing"
)

func TestMemoryResetWrapper(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	failMethod(transport, "memory/reset")

	_, err = client.Memory.Reset(context.Background())
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "memory/reset")
}

func TestMemoryResetStableModeRejectsExperimentalMethodBeforeWrite(t *testing.T) {
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
	_, err = client.Memory.Reset(context.Background())
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T(%v), want *ConfigError", err, err)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("experimental memory reset reached transport in stable mode")
	}
}
