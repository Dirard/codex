package codex

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestFatalDecodeFailureClosesStreamsAndClearsResourceRegistries(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	global, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	process := client.Processes.reserveProcess(ProcessSpawnOptions{})
	client.FileSystem.reserveWatch()
	client.FuzzyFileSearch.reserveSession()
	if _, err := client.Realtime.reserveSession("thread-1"); err != nil {
		t.Fatal(err)
	}

	transport.recv <- json.RawMessage(`{`)
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if notification, ok := global.Next(ctx); ok {
		t.Fatalf("unexpected notification after transport failure: %#v", notification)
	}
	var decodeErr *DecodeError
	if !errors.As(global.Err(), &decodeErr) {
		t.Fatalf("global stream err = %T(%v), want *DecodeError", global.Err(), global.Err())
	}

	processStream, err := process.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !errors.As(processStream.Err(), &decodeErr) {
		t.Fatalf("process stream err = %T(%v), want *DecodeError", processStream.Err(), processStream.Err())
	}
	late, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !errors.As(late.Err(), &decodeErr) {
		t.Fatalf("late stream err = %T(%v), want *DecodeError", late.Err(), late.Err())
	}
	if err := client.Call(context.Background(), "model/list", nil, nil, protocol.MethodMetadata{}); !errors.As(err, &decodeErr) {
		t.Fatalf("late call err = %T(%v), want *DecodeError", err, err)
	}

	client.Processes.mu.Lock()
	activeProcesses := len(client.Processes.activeProcesses)
	client.Processes.mu.Unlock()
	client.FileSystem.mu.Lock()
	activeWatches := len(client.FileSystem.activeWatches)
	client.FileSystem.mu.Unlock()
	client.FuzzyFileSearch.mu.Lock()
	activeSearches := len(client.FuzzyFileSearch.sessions)
	client.FuzzyFileSearch.mu.Unlock()
	client.Realtime.mu.Lock()
	activeRealtime := len(client.Realtime.activeByThread)
	client.Realtime.mu.Unlock()
	if activeProcesses+activeWatches+activeSearches+activeRealtime != 0 {
		t.Fatalf("active resources after transport failure: process=%d watch=%d search=%d realtime=%d", activeProcesses, activeWatches, activeSearches, activeRealtime)
	}
}

func TestInvalidJSONRPCEnvelopeSurfacesPublicDecodeError(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	global, err := client.Notifications(context.Background())
	if err != nil {
		t.Fatal(err)
	}

	transport.recv <- json.RawMessage(`{"id":"response-without-result"}`)
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if notification, ok := global.Next(ctx); ok {
		t.Fatalf("unexpected notification after invalid envelope: %#v", notification)
	}
	var decodeErr *DecodeError
	if !errors.As(global.Err(), &decodeErr) {
		t.Fatalf("global stream err = %T(%v), want *DecodeError", global.Err(), global.Err())
	}
}

func TestTransportTerminationRemainsTransportError(t *testing.T) {
	client := &Client{}
	err := client.handleUnexpectedTermination(io.EOF)
	var transportErr *TransportError
	if !errors.As(err, &transportErr) {
		t.Fatalf("err = %T(%v), want *TransportError", err, err)
	}
}

func TestClientConfigDefaults(t *testing.T) {
	limits, err := normalizeLimits(ClientLimits{})
	if err != nil {
		t.Fatal(err)
	}
	if limits.MaxFrameBytes != DefaultMaxFrameBytes {
		t.Fatalf("MaxFrameBytes = %d, want %d", limits.MaxFrameBytes, DefaultMaxFrameBytes)
	}
	if limits.HandlerTimeout != 60*time.Second {
		t.Fatalf("HandlerTimeout = %s, want 60s", limits.HandlerTimeout)
	}
	if ProtocolModeExperimental != 0 {
		t.Fatalf("ProtocolModeExperimental = %d, want zero-value", ProtocolModeExperimental)
	}
}

func TestClientConfigRejectsNegativeLimits(t *testing.T) {
	_, err := normalizeLimits(ClientLimits{MaxFrameBytes: -1})
	if err == nil {
		t.Fatal("expected error")
	}
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("error = %T, want *ConfigError", err)
	}
}

func TestClientConfigCapsAdditionalContextLimitOverrides(t *testing.T) {
	limits, err := normalizeLimits(ClientLimits{
		MaxAdditionalContextEntries:    protocol.MaxAdditionalContextEntries + 1,
		MaxAdditionalContextKeyBytes:   protocol.MaxAdditionalContextKeyBytes + 1,
		MaxAdditionalContextValueBytes: protocol.MaxAdditionalContextValueBytes + 1,
		MaxAdditionalContextTotalBytes: protocol.MaxAdditionalContextTotalBytes + 1,
	})
	if err != nil {
		t.Fatal(err)
	}
	if limits.MaxAdditionalContextEntries != protocol.MaxAdditionalContextEntries {
		t.Fatalf("MaxAdditionalContextEntries = %d, want protocol cap %d", limits.MaxAdditionalContextEntries, protocol.MaxAdditionalContextEntries)
	}
	if limits.MaxAdditionalContextKeyBytes != protocol.MaxAdditionalContextKeyBytes {
		t.Fatalf("MaxAdditionalContextKeyBytes = %d, want protocol cap %d", limits.MaxAdditionalContextKeyBytes, protocol.MaxAdditionalContextKeyBytes)
	}
	if limits.MaxAdditionalContextValueBytes != protocol.MaxAdditionalContextValueBytes {
		t.Fatalf("MaxAdditionalContextValueBytes = %d, want protocol cap %d", limits.MaxAdditionalContextValueBytes, protocol.MaxAdditionalContextValueBytes)
	}
	if limits.MaxAdditionalContextTotalBytes != protocol.MaxAdditionalContextTotalBytes {
		t.Fatalf("MaxAdditionalContextTotalBytes = %d, want protocol cap %d", limits.MaxAdditionalContextTotalBytes, protocol.MaxAdditionalContextTotalBytes)
	}
}
