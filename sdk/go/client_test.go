package codex

import (
	"errors"
	"testing"
	"time"
)

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
