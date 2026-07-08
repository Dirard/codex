package jsonrpc

import (
	"encoding/json"

	"github.com/openai/codex/sdk/go/protocol"
)

// Envelope is the app-server JSON-RPC object shape. App-server stdio omits a
// jsonrpc version field, and trace is a top-level peer of params/result.
type Envelope struct {
	ID     *protocol.RequestID `json:"id,omitempty"`
	Method string              `json:"method,omitempty"`
	Params json.RawMessage     `json:"params,omitempty"`
	Result json.RawMessage     `json:"result,omitempty"`
	Error  *RPCError           `json:"error,omitempty"`
	Trace  json.RawMessage     `json:"trace,omitempty"`
}

// RPCError is the JSON-RPC error object. Code is int64 to preserve server
// values outside the int32 range.
type RPCError struct {
	Code    int64           `json:"code"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data,omitempty"`
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
