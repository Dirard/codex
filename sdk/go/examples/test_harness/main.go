package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"

	codex "github.com/openai/codex/sdk/go"
)

type injectedTransport struct{}

func (injectedTransport) Receive(context.Context) (json.RawMessage, error) {
	return nil, io.EOF
}

func (injectedTransport) Send(context.Context, json.RawMessage) error {
	return nil
}

func (injectedTransport) Close() error {
	return nil
}

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

func newInjectedTransportClient(ctx context.Context) (*codex.Client, error) {
	return codex.NewClient(ctx, codex.ClientConfig{Transport: injectedTransport{}})
}

func main() {}
