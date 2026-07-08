package codex

import "context"

type TraceContext struct {
	TraceParent string `json:"traceparent,omitempty"`
	TraceState  string `json:"tracestate,omitempty"`
}

type CallOptions struct {
	Trace *TraceContext
}

type callOptionsKey struct{}

func WithCallOptions(ctx context.Context, opts CallOptions) context.Context {
	return context.WithValue(ctx, callOptionsKey{}, opts)
}

func TraceFromContext(ctx context.Context) (*TraceContext, bool) {
	opts, ok := ctx.Value(callOptionsKey{}).(CallOptions)
	return opts.Trace, ok && opts.Trace != nil
}
