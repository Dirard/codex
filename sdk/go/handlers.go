package codex

import (
	"context"
	"encoding/json"
)

// Handwritten server-handler conveniences live here. The ServerHandlers type
// and protocol-specific handler interfaces are generated in handlers_generated.go.

type UnknownServerRequest struct {
	Method string
	Params json.RawMessage
}

type UnknownServerRequestHandler interface {
	HandleUnknownServerRequest(ctx context.Context, request UnknownServerRequest) (any, error)
}

type UnknownServerRequestFunc func(ctx context.Context, request UnknownServerRequest) (any, error)

func (f UnknownServerRequestFunc) HandleUnknownServerRequest(ctx context.Context, request UnknownServerRequest) (any, error) {
	return f(ctx, request)
}
