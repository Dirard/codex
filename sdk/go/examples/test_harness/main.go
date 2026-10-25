package main

import (
	"context"
	"fmt"
	"os"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/codextest"
)

func newHarnessClient(ctx context.Context, codexHome string) (*codex.Client, error) {
	codexPath := os.Getenv("CODEX_EXEC_PATH")
	if codexPath == "" {
		return nil, fmt.Errorf("CODEX_EXEC_PATH must point at the app-server runtime")
	}
	if codexHome == "" {
		return nil, fmt.Errorf("isolated CODEX_HOME is required")
	}
	return codex.NewClient(ctx, codex.ClientConfig{
		CodexPath: codexPath,
		Env:       map[string]string{"CODEX_HOME": codexHome},
	})
}

// newInjectedTransportClient runs the client against the in-process fake app-server; see
// examples/codextest for a scripted turn.
func newInjectedTransportClient(ctx context.Context) (*codex.Client, *codextest.Server, error) {
	srv := codextest.NewServer()
	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
	if err != nil {
		_ = srv.Close()
		return nil, nil, err
	}
	return client, srv, nil
}

func main() {}
