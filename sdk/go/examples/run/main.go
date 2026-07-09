package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
)

// codex-go-sdk-resource:Threads
// codex-go-sdk-docs:thread/start
// codex-go-sdk-resource:Turns
// codex-go-sdk-docs:turn/start
func runOnce(ctx context.Context, client *codex.Client) error {
	schema, err := codex.JSONSchema("summary", codex.ObjectSchema(map[string]codex.JSONSchemaSpec{
		"title": codex.StringSchema(),
	}, "title"))
	if err != nil {
		return err
	}
	thread, err := client.Threads.Start(ctx, codex.ThreadStartOptions{
		CWD:         ".",
		Permissions: "read-only",
	})
	if err != nil {
		return err
	}
	_, err = thread.Run(ctx, codex.Inputs(
		codex.Text("Summarize the repository layout."),
		codex.DataURL("data:image/png;base64,iVBORw0KGgo="),
		codex.LocalImage("./screenshot.png"),
	), codex.TurnOptions{
		OutputSchema: schema,
	})
	if err != nil {
		return err
	}
	turn, err := thread.Turn(ctx, codex.Text("Continue with one risk."), codex.TurnOptions{})
	if err != nil {
		return err
	}
	stream, err := turn.Stream(ctx)
	if err != nil {
		return err
	}
	return stream.Close()
}

func main() {}
