package codex

import (
	"context"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestFeedbackUploadWrapperAndRetryMetadata(t *testing.T) {
	if got := protocol.MethodMetadataByMethod["feedback/upload"].Retry; got != "neverRetryAfterWrite" {
		t.Fatalf("feedback/upload retry metadata = %q, want neverRetryAfterWrite", got)
	}

	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	failMethod(transport, "feedback/upload")

	_, err = client.Feedback.Upload(context.Background(), protocol.FeedbackUploadParams{
		Classification: "bug",
		ThreadID:       protocol.Some("thread-1"),
	})
	var rpcErr *RPCError
	if !errors.As(err, &rpcErr) {
		t.Fatalf("err = %T(%v), want *RPCError", err, err)
	}
	assertMethod(t, transport.lastFrame(t), "feedback/upload")
	if got := methodFrameCount(t, transport, "feedback/upload"); got != 1 {
		t.Fatalf("feedback/upload frames = %d, want 1", got)
	}
}
