package main

import (
	"context"
	"fmt"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/protocol"
)

// codex-go-sdk-resource:Turns
// codex-go-sdk-docs:turn/steer
// codex-go-sdk-docs:turn/interrupt
// codex-go-sdk-docs:turn/settings/update
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
	if _, err := turn.UpdateSettings(ctx, protocol.TurnSettingsUpdateParams{}); err != nil {
		return err
	}
	if err := turn.Steer(ctx, codex.Text("Add one more constraint.")); err != nil {
		return err
	}
	return turn.Interrupt(ctx)
}

// printAnswer prints the answer as it streams, then waits for the turn's result. Wait
// still returns the result after the stream consumed every event, and ending ctx
// interrupts the turn because of InterruptOnCancel.
func printAnswer(ctx context.Context, thread *codex.Thread) (*codex.RunResult, error) {
	turn, err := thread.Turn(ctx, codex.Text("Explain the change."), codex.TurnOptions{InterruptOnCancel: true})
	if err != nil {
		return nil, err
	}
	stream, err := turn.Stream(ctx)
	if err != nil {
		return nil, err
	}
	defer stream.Close()
	for delta, err := range codex.Payloads[protocol.AgentMessageDeltaNotification](ctx, stream) {
		if err != nil {
			return nil, err
		}
		fmt.Print(delta.Delta)
	}
	return turn.Wait(ctx)
}

func main() {}
