package jsonrpc

import (
	"strings"
	"testing"
)

func TestRingRedactsSensitiveRecordsAcrossWriteBoundaries(t *testing.T) {
	markers := []string{"api_key", "apikey", "token", "secret", "password", "credential", "auth", "cookie"}
	for _, marker := range markers {
		for split := 1; split < len(marker); split++ {
			t.Run(marker+"/split-"+string(rune('0'+split)), func(t *testing.T) {
				ring := NewRing(64)
				_, _ = ring.Write([]byte("prefix " + marker[:split]))
				_, _ = ring.Write([]byte(marker[split:] + "=value\n"))
				_, _ = ring.Write([]byte("safe\n"))
				if got := ring.String(); got != "[redacted]\nsafe\n" {
					t.Fatalf("ring = %q", got)
				}
			})
		}
	}
}

func TestRingRedactsSplitMarkerNearLimit(t *testing.T) {
	ring := NewRing(16)
	_, _ = ring.Write([]byte(strings.Repeat("x", 64) + "to"))
	_, _ = ring.Write([]byte("ken=value\n"))
	_, _ = ring.Write([]byte("safe\n"))
	if got := ring.String(); got != "[redacted]\nsafe\n" {
		t.Fatalf("ring = %q", got)
	}
}
