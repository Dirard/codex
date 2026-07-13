package codex

import (
	"encoding/json"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
)

// ConfigError reports invalid SDK configuration before startup.
type ConfigError struct{ Reason string }

func (e *ConfigError) Error() string { return "codex sdk config error: " + e.Reason }

type TransportError struct{ Reason string }

func (e *TransportError) Error() string { return "codex sdk transport error: " + e.Reason }

type CompatibilityError struct {
	Reason           string
	ExpectedDigest   string
	FoundDigest      string
	ExpectedMode     ProtocolMode
	FoundMode        *ProtocolMode
	RuntimePath      string
	RuntimeVersion   string
	UserAgent        string
	RequiredOverride *CompatibilityPolicy
}

func (e *CompatibilityError) Error() string { return "codex sdk compatibility error: " + e.Reason }

type RuntimeNotFoundError struct {
	Searched []string
	Hint     string
}

func (e *RuntimeNotFoundError) Error() string {
	return "codex runtime not found: " + e.Hint
}

type UnsupportedError struct{ Reason string }

func (e *UnsupportedError) Error() string { return "codex sdk unsupported: " + e.Reason }

func (e *UnsupportedError) SafeJSONRPCMessage() string { return e.Error() }

type ConflictError struct{ Reason string }

func (e *ConflictError) Error() string { return "codex sdk conflict: " + e.Reason }

type OverflowError struct{ Reason string }

func (e *OverflowError) Error() string { return "codex sdk overflow: " + e.Reason }

type DecodeError struct {
	Reason string
	cause  error
}

func (e *DecodeError) Error() string { return "codex sdk decode error: " + e.Reason }

func (e *DecodeError) Unwrap() error {
	if e == nil {
		return nil
	}
	return e.cause
}

type FrameSizeError = jsonrpc.FrameSizeError
type ClosedError = jsonrpc.ClosedError
type RPCError = jsonrpc.RPCError

type rpcErrorPayload struct {
	Code    int64           `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
}
