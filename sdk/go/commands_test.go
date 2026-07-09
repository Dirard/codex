package codex

import (
	"context"
	"encoding/base64"
	"errors"
	"runtime"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestCommandExecStreamsUntilJSONRPCResponseThenCloses(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	releaseResponse := transport.deferResponse("command/exec", mustJSON(t, protocol.CommandExecResponse{
		ExitCode: 0,
	}))
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.Commands.Exec(context.Background(), CommandExecOptions{
		Command: []string{"echo", "ok"},
	})
	if err != nil {
		t.Fatal(err)
	}

	waitForMethod(t, transport, "command/exec")
	params := requestParamsForMethod(t, transport, "command/exec")
	processID := requestStringParam(t, params, "processId")
	if processID == "" {
		t.Fatal("command processId is empty")
	}
	if handle.ID() != processID {
		t.Fatalf("handle ID = %q, want %q", handle.ID(), processID)
	}
	assertRequestStringSliceParam(t, params, "command", []string{"echo", "ok"})
	assertRequestBoolParam(t, params, "streamStdoutStderr", true)
	assertRequestBoolParam(t, params, "streamStdin", true)
	stream, err := handle.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("command/exec/outputDelta", mustJSON(t, protocol.CommandExecOutputDeltaNotification{
		ProcessID:   processID,
		DeltaBase64: base64.StdEncoding.EncodeToString([]byte("hi")),
		Stream:      protocol.CommandExecOutputStreamStdout,
	}), nil)
	notification := nextTestNotification(t, stream)
	if notification.Method != "command/exec/outputDelta" {
		t.Fatalf("method = %s, want command/exec/outputDelta", notification.Method)
	}
	payload, ok := notification.Payload.(protocol.CommandExecOutputDeltaNotification)
	if !ok {
		t.Fatalf("payload = %T, want protocol.CommandExecOutputDeltaNotification", notification.Payload)
	}
	if payload.ProcessID != processID || payload.DeltaBase64 != base64.StdEncoding.EncodeToString([]byte("hi")) {
		t.Fatalf("payload = %#v", payload)
	}

	if err := handle.Write(context.Background(), []byte("input")); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "command/exec/write")
	assertRequestStringParam(t, params, "processId", processID)
	assertRequestStringParam(t, params, "deltaBase64", base64.StdEncoding.EncodeToString([]byte("input")))

	if err := handle.CloseStdin(context.Background()); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "command/exec/write")
	assertRequestStringParam(t, params, "processId", processID)
	assertRequestBoolParam(t, params, "closeStdin", true)

	releaseResponse()
	response, err := handle.Wait(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if response.ExitCode != 0 || response.Stdout != "" {
		t.Fatalf("response = %#v", response)
	}
	expectClosedStream(t, stream)

	beforeWrite := methodCount(t, transport, "command/exec/write")
	assertConflictError(t, handle.Write(context.Background(), []byte("late")))
	assertConflictError(t, handle.Terminate(context.Background()))
	assertConflictError(t, handle.Resize(context.Background(), TerminalSize{Rows: 24, Cols: 80}))
	if got := methodCount(t, transport, "command/exec/write"); got != beforeWrite {
		t.Fatalf("command/exec/write sent %d times after stale write, want %d", got, beforeWrite)
	}
}

func TestCommandExecImmediateFollowupSendsStartBeforeWrite(t *testing.T) {
	previous := runtime.GOMAXPROCS(1)
	defer runtime.GOMAXPROCS(previous)

	transport := newScriptedInitializedTransport(t, nil)
	releaseResponse := transport.deferResponse("command/exec", mustJSON(t, protocol.CommandExecResponse{ExitCode: 0}))
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	handle, err := client.Commands.Exec(context.Background(), CommandExecOptions{
		Command: []string{"cat"},
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := handle.Write(context.Background(), []byte("input")); err != nil {
		t.Fatal(err)
	}
	waitForMethod(t, transport, "command/exec/write")
	assertMethodOrder(t, transport, "command/exec", "command/exec/write")
	releaseResponse()
	if _, err := handle.Wait(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestCommandExecRejectsSizeWithoutTTYBeforeSend(t *testing.T) {
	size := TerminalSize{Rows: 24, Cols: 80}
	ttyFalse := false
	tests := []struct {
		name string
		opts CommandExecOptions
	}{
		{
			name: "missing-tty",
			opts: CommandExecOptions{
				Command: []string{"echo", "ok"},
				Size:    &size,
			},
		},
		{
			name: "tty-false",
			opts: CommandExecOptions{
				Command: []string{"echo", "ok"},
				Size:    &size,
				TTY:     &ttyFalse,
			},
		},
		{
			name: "empty-command",
			opts: CommandExecOptions{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = client.Close() })

			_, err = client.Commands.Exec(context.Background(), tt.opts)
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T(%v), want *ConfigError", err, err)
			}
			if methodWasSent(t, transport, "command/exec") {
				t.Fatal("command/exec was sent for invalid options")
			}
		})
	}
}
