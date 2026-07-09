package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/protocol"
)

// codex-go-sdk-resource:Threads
// codex-go-sdk-docs:thread/read
func rawThreadRead(ctx context.Context, client *codex.Client) error {
	if _, err := client.Threads.Read(ctx, protocol.ThreadReadParams{}); err != nil {
		return err
	}
	_, err := client.Raw().ThreadRead(ctx, protocol.ThreadReadParams{})
	return err
}

func main() {}
