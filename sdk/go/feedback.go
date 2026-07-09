package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

func (c *FeedbackClient) Upload(ctx context.Context, params protocol.FeedbackUploadParams) (protocol.FeedbackUploadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.FeedbackUploadResponse{}, &ClosedError{}
	}
	return c.client.Raw().FeedbackUpload(ctx, params)
}
