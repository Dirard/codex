package codex

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestWindowsSandboxUnsupportedPlatformStatusAndError(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	before := len(transport.sentFrames())
	readiness, err := client.WindowsSandbox.Readiness(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if readiness.Status != WindowsSandboxReadinessUnsupportedPlatform {
		t.Fatalf("status = %q, want %q", readiness.Status, WindowsSandboxReadinessUnsupportedPlatform)
	}
	if readiness.PlatformOS != "linux" {
		t.Fatalf("platform OS = %q, want linux", readiness.PlatformOS)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("unsupported windows sandbox readiness reached transport")
	}

	_, err = client.WindowsSandbox.SetupStart(context.Background(), protocol.WindowsSandboxSetupStartParams{
		Mode: protocol.WindowsSandboxSetupModeUnelevated,
	})
	var platformErr *UnsupportedPlatformError
	if !errors.As(err, &platformErr) {
		t.Fatalf("err = %T(%v), want *UnsupportedPlatformError", err, err)
	}
	if platformErr.PlatformOS != "linux" {
		t.Fatalf("platform error OS = %q, want linux", platformErr.PlatformOS)
	}
	if len(transport.sentFrames()) != before {
		t.Fatal("unsupported windows sandbox setup reached transport")
	}
}

func TestWindowsSandboxCallsRuntimeOnWindowsPlatform(t *testing.T) {
	transport := newScriptedInitializedTransport(t, initializePayloadWithPlatformOS("windows"))
	transport.responses["windowsSandbox/readiness"] = []byte(`{"status":"ready"}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	readiness, err := client.WindowsSandbox.Readiness(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if readiness.Status != WindowsSandboxReadinessReady {
		t.Fatalf("status = %q, want %q", readiness.Status, WindowsSandboxReadinessReady)
	}
	assertMethod(t, transport.lastFrame(t), "windowsSandbox/readiness")

	failMethod(transport, "windowsSandbox/setupStart")
	_, err = client.WindowsSandbox.SetupStart(context.Background(), protocol.WindowsSandboxSetupStartParams{
		Mode: protocol.WindowsSandboxSetupModeElevated,
	})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	setupFrame := transport.lastFrame(t)
	assertMethod(t, setupFrame, "windowsSandbox/setupStart")
	assertRequestStringParam(t, paramsFromFrame(t, setupFrame), "mode", "elevated")
}

func initializePayloadWithPlatformOS(platformOS string) json.RawMessage {
	return json.RawMessage(bytes.ReplaceAll(currentInitializePayload(), []byte(`"platformOs":"linux"`), []byte(`"platformOs":"`+platformOS+`"`)))
}
