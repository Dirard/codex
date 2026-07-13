package jsonrpc

import (
	"bytes"
	"encoding/json"

	"github.com/openai/codex/sdk/go/protocol"
)

// Envelope is the app-server JSON-RPC object shape. App-server stdio omits a
// jsonrpc version field, and trace is a top-level peer of params/result.
type Envelope struct {
	ID        *protocol.RequestID `json:"id,omitempty"`
	Method    string              `json:"method,omitempty"`
	Params    json.RawMessage     `json:"params,omitempty"`
	Result    json.RawMessage     `json:"result,omitempty"`
	Error     *RPCError           `json:"error,omitempty"`
	Trace     json.RawMessage     `json:"trace,omitempty"`
	idPresent bool
}

func (e *Envelope) UnmarshalJSON(data []byte) error {
	type wireEnvelope Envelope
	var decoded wireEnvelope
	if err := json.Unmarshal(data, &decoded); err != nil {
		return err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	*e = Envelope(decoded)
	_, e.idPresent = fields["id"]
	return nil
}

// DecodeError reports an invalid inbound JSON-RPC envelope shape.
type DecodeError struct {
	Reason string
}

func (e *DecodeError) Error() string {
	if e == nil || e.Reason == "" {
		return "invalid JSON-RPC envelope"
	}
	return "invalid JSON-RPC envelope: " + e.Reason
}

// RPCError is the JSON-RPC error object. Code is int64 to preserve server
// values outside the int32 range.
type RPCError struct {
	Code           int64           `json:"code"`
	Message        string          `json:"message"`
	Data           json.RawMessage `json:"data,omitempty"`
	codePresent    bool
	messagePresent bool
}

func (e *RPCError) UnmarshalJSON(data []byte) error {
	type wireRPCError RPCError
	var decoded wireRPCError
	if err := json.Unmarshal(data, &decoded); err != nil {
		return err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	*e = RPCError(decoded)
	code, codePresent := fields["code"]
	message, messagePresent := fields["message"]
	if codePresent && bytes.Equal(bytes.TrimSpace(code), []byte("null")) {
		return &DecodeError{Reason: "error code must be a number"}
	}
	if messagePresent && bytes.Equal(bytes.TrimSpace(message), []byte("null")) {
		return &DecodeError{Reason: "error message must be a string"}
	}
	e.codePresent = codePresent
	e.messagePresent = messagePresent
	return nil
}

func (e *RPCError) Error() string {
	if e == nil {
		return "codex jsonrpc error: <nil>"
	}
	return "codex jsonrpc error: " + e.Message
}

func requestIDKey(id protocol.RequestID) (string, error) {
	data, err := json.Marshal(id)
	if err != nil {
		return "", err
	}
	return string(data), nil
}
