package codex

import (
	"errors"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
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
