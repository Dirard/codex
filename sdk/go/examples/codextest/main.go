// Command codextest shows code that uses the SDK and, in main_test.go, how to test it with
// the in-process fake app-server from package codextest.
package main

import (
	"context"
	"fmt"
	"os"

	codex "github.com/openai/codex/sdk/go"
)

// Answer asks Codex one question in a fresh thread. If ctx ends first, the turn is
// interrupted on the runtime instead of running on.
func Answer(ctx context.Context, client *codex.Client, question string) (string, error) {
	thread, err := client.Threads.Start(ctx, codex.ThreadStartOptions{Permissions: "read-only"})
	if err != nil {
		return "", err
	}
	result, err := thread.Run(ctx, codex.Text(question), codex.TurnOptions{InterruptOnCancel: true})
	if err != nil {
		return "", err
	}
	return result.FinalResponse, nil
}

func main() {
	codexPath := os.Getenv("CODEX_EXEC_PATH")
	if codexPath == "" {
		fmt.Fprintln(os.Stderr, "set CODEX_EXEC_PATH to a compatible codex runtime")
		os.Exit(2)
	}
	ctx := context.Background()
	client, err := codex.NewClient(ctx, codex.ClientConfig{CodexPath: codexPath})
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	defer client.Close()
	answer, err := Answer(ctx, client, "What is 2+2?")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println(answer)
}
