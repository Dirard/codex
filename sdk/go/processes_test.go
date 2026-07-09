package codex

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestProcessSpawnFollowupsAndTerminalCleanupUseOwnedHandle(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	tty := true
	process, _, err := client.Processes.Spawn(context.Background(), ProcessSpawnOptions{
		Command: []string{"sleep", "1"},
		CWD:     "/repo",
		TTY:     &tty,
	})
	if err != nil {
		t.Fatal(err)
	}
	if process.ID() == "" {
		t.Fatal("process handle is empty")
	}
	params := requestParamsForMethod(t, transport, "process/spawn")
	assertRequestStringParam(t, params, "processHandle", process.ID())
	assertRequestStringParam(t, params, "cwd", "/repo")
	assertRequestStringSliceParam(t, params, "command", []string{"sleep", "1"})
	assertRequestBoolParam(t, params, "streamStdin", true)
	assertRequestBoolParam(t, params, "streamStdoutStderr", true)
	assertRequestBoolParam(t, params, "tty", true)

	stream, err := process.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	transport.deliverNotification("process/outputDelta", mustJSON(t, protocol.ProcessOutputDeltaNotification{
		ProcessHandle: process.ID(),
		DeltaBase64:   base64.StdEncoding.EncodeToString([]byte("running")),
		Stream:        protocol.ProcessOutputStreamStdout,
	}), nil)
	notification := nextTestNotification(t, stream)
	if notification.Method != "process/outputDelta" {
		t.Fatalf("method = %s, want process/outputDelta", notification.Method)
	}
	output, ok := notification.Payload.(protocol.ProcessOutputDeltaNotification)
	if !ok {
		t.Fatalf("payload = %T, want protocol.ProcessOutputDeltaNotification", notification.Payload)
	}
	if output.ProcessHandle != process.ID() {
		t.Fatalf("output process handle = %q, want %q", output.ProcessHandle, process.ID())
	}

	if err := process.WriteStdin(context.Background(), []byte("input")); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "process/writeStdin")
	assertRequestStringParam(t, params, "processHandle", process.ID())
	assertRequestStringParam(t, params, "deltaBase64", base64.StdEncoding.EncodeToString([]byte("input")))

	if err := process.CloseStdin(context.Background()); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "process/writeStdin")
	assertRequestStringParam(t, params, "processHandle", process.ID())
	assertRequestBoolParam(t, params, "closeStdin", true)

	if err := process.ResizePTY(context.Background(), TerminalSize{Rows: 24, Cols: 80}); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "process/resizePty")
	assertProcessResizeParams(t, params, process.ID(), TerminalSize{Rows: 24, Cols: 80})

	if err := process.Kill(context.Background()); err != nil {
		t.Fatal(err)
	}
	params = requestParamsForMethod(t, transport, "process/kill")
	assertRequestStringParam(t, params, "processHandle", process.ID())

	transport.deliverNotification("process/exited", mustJSON(t, protocol.ProcessExitedNotification{
		ProcessHandle: process.ID(),
		ExitCode:      0,
		Stdout:        "done",
	}), nil)
	notification = nextTestNotification(t, stream)
	if notification.Method != "process/exited" {
		t.Fatalf("method = %s, want process/exited", notification.Method)
	}
	exited, ok := notification.Payload.(protocol.ProcessExitedNotification)
	if !ok {
		t.Fatalf("payload = %T, want protocol.ProcessExitedNotification", notification.Payload)
	}
	if exited.ProcessHandle != process.ID() {
		t.Fatalf("exited process handle = %q, want %q", exited.ProcessHandle, process.ID())
	}
	expectClosedStream(t, stream)

	assertConflictError(t, process.WriteStdin(context.Background(), []byte("late")))
	assertConflictError(t, process.ResizePTY(context.Background(), TerminalSize{Rows: 10, Cols: 20}))
	assertConflictError(t, process.Kill(context.Background()))
}

func TestProcessSpawnDefaultsStreamingForHandle(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	process, _, err := client.Processes.Spawn(context.Background(), ProcessSpawnOptions{
		Command: []string{"echo", "ok"},
		CWD:     "/repo",
	})
	if err != nil {
		t.Fatal(err)
	}
	params := requestParamsForMethod(t, transport, "process/spawn")
	assertRequestStringParam(t, params, "processHandle", process.ID())
	assertRequestBoolParam(t, params, "streamStdin", true)
	assertRequestBoolParam(t, params, "streamStdoutStderr", true)
}

func TestProcessSpawnStreamDrainsFastExitBeforeFirstStream(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	process, _, err := client.Processes.Spawn(context.Background(), ProcessSpawnOptions{
		Command: []string{"true"},
		CWD:     "/repo",
	})
	if err != nil {
		t.Fatal(err)
	}

	client.HandleServerNotification(context.Background(), "process/exited", mustJSON(t, protocol.ProcessExitedNotification{
		ProcessHandle: process.ID(),
		ExitCode:      0,
		Stdout:        "done",
	}), nil)

	stream, err := process.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	notification := nextTestNotification(t, stream)
	if notification.Method != "process/exited" {
		t.Fatalf("method = %s, want process/exited", notification.Method)
	}
	exited, ok := notification.Payload.(protocol.ProcessExitedNotification)
	if !ok {
		t.Fatalf("payload = %T, want protocol.ProcessExitedNotification", notification.Payload)
	}
	if exited.ProcessHandle != process.ID() || exited.ExitCode != 0 {
		t.Fatalf("exited payload = %#v", exited)
	}
	expectClosedStream(t, stream)
	assertConflictError(t, process.Kill(context.Background()))
}

func TestProcessSpawnRejectsInvalidOptionsBeforeSend(t *testing.T) {
	size := TerminalSize{Rows: 24, Cols: 80}
	ttyFalse := false
	streamFalse := false
	tests := []struct {
		name string
		opts ProcessSpawnOptions
	}{
		{
			name: "missing-cwd",
			opts: ProcessSpawnOptions{Command: []string{"sleep", "1"}},
		},
		{
			name: "relative-cwd",
			opts: ProcessSpawnOptions{Command: []string{"sleep", "1"}, CWD: "repo"},
		},
		{
			name: "empty-command",
			opts: ProcessSpawnOptions{CWD: "/repo"},
		},
		{
			name: "stream-stdin-false",
			opts: ProcessSpawnOptions{Command: []string{"sleep", "1"}, CWD: "/repo", StreamStdin: &streamFalse},
		},
		{
			name: "stream-output-false",
			opts: ProcessSpawnOptions{Command: []string{"sleep", "1"}, CWD: "/repo", StreamStdoutStderr: &streamFalse},
		},
		{
			name: "size-missing-tty",
			opts: ProcessSpawnOptions{Command: []string{"sleep", "1"}, CWD: "/repo", Size: &size},
		},
		{
			name: "size-tty-false",
			opts: ProcessSpawnOptions{Command: []string{"sleep", "1"}, CWD: "/repo", Size: &size, TTY: &ttyFalse},
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

			_, _, err = client.Processes.Spawn(context.Background(), tt.opts)
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T(%v), want *ConfigError", err, err)
			}
			if methodWasSent(t, transport, "process/spawn") {
				t.Fatal("process/spawn was sent for invalid options")
			}
		})
	}
}

func TestProcessResizeRequiresTTYBeforeSend(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	process, _, err := client.Processes.Spawn(context.Background(), ProcessSpawnOptions{
		Command: []string{"sleep", "1"},
		CWD:     "/repo",
	})
	if err != nil {
		t.Fatal(err)
	}
	before := methodCount(t, transport, "process/resizePty")
	var configErr *ConfigError
	if err := process.ResizePTY(context.Background(), TerminalSize{Rows: 24, Cols: 80}); !errors.As(err, &configErr) {
		t.Fatalf("err = %T(%v), want *ConfigError", err, err)
	}
	if got := methodCount(t, transport, "process/resizePty"); got != before {
		t.Fatalf("process/resizePty sent %d times after invalid resize, want %d", got, before)
	}
}

func assertProcessResizeParams(t *testing.T, params json.RawMessage, wantHandle string, wantSize TerminalSize) {
	t.Helper()
	var raw struct {
		ProcessHandle string `json:"processHandle"`
		Size          struct {
			Rows uint16 `json:"rows"`
			Cols uint16 `json:"cols"`
		} `json:"size"`
	}
	if err := json.Unmarshal(params, &raw); err != nil {
		t.Fatal(err)
	}
	if raw.ProcessHandle != wantHandle {
		t.Fatalf("processHandle = %q, want %q", raw.ProcessHandle, wantHandle)
	}
	if raw.Size.Rows != wantSize.Rows || raw.Size.Cols != wantSize.Cols {
		t.Fatalf("size = %#v, want %#v", raw.Size, wantSize)
	}
}
