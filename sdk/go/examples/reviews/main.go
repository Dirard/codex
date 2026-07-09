package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
)

// codex-go-sdk-resource:Reviews
// codex-go-sdk-docs:review/start
func startReview(ctx context.Context, client *codex.Client, thread *codex.Thread) error {
	review, err := client.Reviews.Start(ctx, codex.ReviewStartOptions{
		ThreadID: thread.ID(),
		Target:   codex.UncommittedChangesReviewTarget(),
	})
	if err != nil {
		return err
	}
	_, err = review.Wait(ctx)
	return err
}

func main() {}
