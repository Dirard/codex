package codex

import (
	"context"
	"encoding/json"
	"time"
)

// ProtocolMode selects the app-server protocol surface used during initialize.
type ProtocolMode int

const (
	ProtocolModeExperimental ProtocolMode = iota
	ProtocolModeStable
)

// ClientConfig configures SDK startup and runtime behavior.
type ClientConfig struct {
	CodexPath           string
	Launch              LaunchOptions
	CWD                 string
	Env                 map[string]string
	ConfigOverrides     map[string]string
	ClientName          string
	ClientVersion       string
	ProtocolMode        ProtocolMode
	Mode                ClientMode
	Limits              ClientLimits
	Handlers            ServerHandlers
	Compatibility       CompatibilityPolicy
	Transport           Transport
	NotificationOptOuts NotificationOptOuts
}

// LaunchOptions is reserved for typed codex global launch knobs that may appear
// before the app-server subcommand. Stage 3 starts with no public launch
// options; unknown raw CLI flags are not accepted through the public SDK surface.
type LaunchOptions struct{}

// CompatibilityPolicy controls runtime protocol mismatch behavior.
type CompatibilityPolicy int

const (
	// CompatibilityStrict is the zero-value production policy. It requires the
	// runtime initialize response to include the selected protocol digest and for
	// that digest to match the generated SDK digest.
	CompatibilityStrict CompatibilityPolicy = iota
	// CompatibilityAllowDevBuild permits missing or mismatched digests only for
	// explicit, reviewed dev-runtime fixtures, including legacy initialize
	// responses without activeProtocolMode. It must never silently accept a
	// PATH-discovered release runtime with a mismatched digest.
	CompatibilityAllowDevBuild
	// CompatibilityAllowProtocolDigestUnavailable permits legacy initialize
	// responses with no digest fields and no activeProtocolMode only from
	// injected test transports or explicit dev CodexPath launches, and still
	// rejects any non-empty mismatched digest.
	CompatibilityAllowProtocolDigestUnavailable
)

// Transport is an injected JSON-RPC frame transport used by tests and embedding hosts.
//
// Receive blocks until it returns one complete JSON-RPC envelope. Send writes one
// complete JSON-RPC envelope. Implementations own their external framing, if any:
// callers pass raw JSON objects without a trailing newline or content-length
// header. The SDK owns JSON-RPC request correlation, generated method dispatch,
// and serialized calls into Send.
type Transport interface {
	Receive(ctx context.Context) (json.RawMessage, error)
	Send(ctx context.Context, frame json.RawMessage) error
	Close() error
}

// NotificationOptOuts lists app-server notifications the caller opts out of.
type NotificationOptOuts struct {
	Methods []string
}

// ClientMode controls high-level workflow availability.
type ClientMode int

const (
	ClientModeHighLevel ClientMode = iota
	ClientModeRawOnly
)

// ClientLimits contains bounded runtime limits. Zero means SDK default.
type ClientLimits struct {
	MaxFrameBytes                  int64
	MaxLocalInputBytes             int64
	MaxAdditionalContextEntries    int
	MaxAdditionalContextKeyBytes   int64
	MaxAdditionalContextValueBytes int64
	MaxAdditionalContextTotalBytes int64
	ResourceStreamQueue            int
	PendingTurnQueue               int
	PendingTurnMap                 int
	GlobalSubscriberQueue          int
	HandlerConcurrency             int
	HandlerQueue                   int
	HandlerTimeout                 time.Duration
	StderrRingBytes                int
	LifecycleInactivityTimeout     time.Duration
}
