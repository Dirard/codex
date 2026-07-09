package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
)

// codex-go-sdk-resource:Turns
// codex-go-sdk-docs:turn/steer
// codex-go-sdk-docs:turn/interrupt
func streamTurn(ctx context.Context, thread *codex.Thread) error {
	turn, err := thread.Turn(ctx, codex.Text("Start a streamed answer."), codex.TurnOptions{})
	if err != nil {
		return err
	}
	stream, err := turn.Stream(ctx)
	if err != nil {
		return err
	}
	defer stream.Close()
	if err := turn.Steer(ctx, codex.Text("Add one more constraint.")); err != nil {
		return err
	}
	return turn.Interrupt(ctx)
}

func main() {}
