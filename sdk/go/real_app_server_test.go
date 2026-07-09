package codex

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

const realAppServerTimeout = 30 * time.Second

func TestRealAppServerInitializeStrictDigest(t *testing.T) {
	client, _, runtimePath, _ := newRealAppServerClient(t)

	metadata := client.Metadata()
	if metadata.RuntimePath != runtimePath {
		t.Fatalf("runtime path = %q, want %q", metadata.RuntimePath, runtimePath)
	}
	if metadata.ProtocolMode != ProtocolModeExperimental {
		t.Fatalf("protocol mode = %v, want experimental", metadata.ProtocolMode)
	}
	if metadata.Compatibility != CompatibilityStrict {
		t.Fatalf("compatibility = %v, want strict", metadata.Compatibility)
	}
	if metadata.CompatibilityOverrideActive {
		t.Fatal("strict real app-server initialize activated compatibility override")
	}
	if metadata.UserAgent == "" {
		t.Fatal("real app-server initialize did not provide user agent")
	}
}

func TestRealAppServerRejectsDebugHookEnv(t *testing.T) {
	runtimePath := requireRealAppServerRuntime(t)
	ctx := realAppServerContext(t)

	_, err := NewClient(ctx, ClientConfig{
		CodexPath: runtimePath,
		Env: map[string]string{
			"CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE": "1",
		},
	})
	var cfgErr *ConfigError
	if !errors.As(err, &cfgErr) {
		t.Fatalf("err = %T, want *ConfigError for reserved hook env", err)
	}

	t.Setenv("CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE", "1")
	t.Setenv("CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS", "http://127.0.0.1/debug-hook")
	client, _, _, _ := newRealAppServerClient(t)
	if client.Metadata().RuntimePath == "" {
		t.Fatal("real app-server did not initialize after parent debug hook env was scrubbed")
	}
}

func TestRealAppServerDigestMismatch(t *testing.T) {
	_ = requireRealAppServerRuntime(t)
	TestStrictRejectsDigestMismatchAndWrongMode(t)
}

func TestRealAppServerCompatibilityOverridePolicy(t *testing.T) {
	_ = requireRealAppServerRuntime(t)
	TestCompatibilityOverrideAcceptsLegacyOnlyForInjectedDevRuntime(t)
}

func TestRealAppServerThreadRunHappyPath(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t, realAssistantResponseSSE("real app-server ok"))
	ctx := realAppServerContext(t)

	thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	result, err := thread.Run(ctx, Text("say ok"), TurnOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	if result.FinalResponse != "real app-server ok" {
		t.Fatalf("final response = %q, want real app-server ok", result.FinalResponse)
	}
}

func TestRealAppServerConfigReadWrite(t *testing.T) {
	client, _, _, _ := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	if _, err := client.Config.Read(ctx, protocol.ConfigReadParams{}); err != nil {
		t.Fatal(err)
	}
	_, err := client.Config.WriteValue(ctx, protocol.ConfigValueWriteParams{
		KeyPath:       "model",
		MergeStrategy: protocol.MergeStrategyReplace,
		Value:         json.RawMessage(`"mock-model"`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := client.Config.Read(ctx, protocol.ConfigReadParams{}); err != nil {
		t.Fatal(err)
	}
}

func TestRealAppServerFilesystemWatch(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	watch, _, err := client.FileSystem.Watch(ctx, FileSystemWatchOptions{Path: workdir})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := watch.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()

	changedPath := filepath.Join(workdir, "changed.txt")
	if err := os.WriteFile(changedPath, []byte("updated"), 0o600); err != nil {
		t.Fatal(err)
	}
	notification := waitForNotification(t, ctx, stream, "fs/changed")
	payload, ok := notification.Payload.(protocol.FsChangedNotification)
	if !ok {
		t.Fatalf("payload = %T, want FsChangedNotification", notification.Payload)
	}
	if payload.WatchID != watch.ID() {
		t.Fatalf("watch id = %q, want %q", payload.WatchID, watch.ID())
	}
	if err := watch.Close(ctx); err != nil {
		t.Fatal(err)
	}
}

func TestRealAppServerCommandExecStreaming(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	handle, err := client.Commands.Exec(ctx, CommandExecOptions{
		Command: shellPrintCommand("sdk-command-ok"),
		CWD:     workdir,
	})
	if err != nil {
		t.Fatal(err)
	}
	stream, err := handle.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	output := waitForBase64Output(t, ctx, stream, "command/exec/outputDelta")
	if !strings.Contains(output, "sdk-command-ok") {
		t.Fatalf("command output = %q, want sdk-command-ok", output)
	}
	response, err := handle.Wait(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if response.ExitCode != 0 {
		t.Fatalf("exit code = %d, want 0", response.ExitCode)
	}
}

func TestRealAppServerProcessLifecycle(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	handle, _, err := client.Processes.Spawn(ctx, ProcessSpawnOptions{
		Command: shellPrintCommand("sdk-process-ok"),
		CWD:     workdir,
	})
	if err != nil {
		t.Fatal(err)
	}
	if handle.ID() == "" {
		t.Fatal("process handle id is empty")
	}
	stream, err := handle.Stream(ctx)
	if err != nil {
		t.Fatal(err)
	}
	output := waitForBase64Output(t, ctx, stream, "process/outputDelta")
	if !strings.Contains(output, "sdk-process-ok") {
		t.Fatalf("process output = %q, want sdk-process-ok", output)
	}
	notification := waitForNotification(t, ctx, stream, "process/exited")
	if notification.Payload == nil {
		t.Fatal("process/exited had nil payload")
	}
}

func TestRealAppServerSafeResourceWorkflows(t *testing.T) {
	client, _, _, workdir := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	if _, err := client.Threads.List(ctx, protocol.ThreadListParams{}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{Path: protocol.AbsolutePathBuf(workdir)}); err != nil {
		t.Fatal(err)
	}
}

func TestRealAppServerRemoteControlWorkflow(t *testing.T) {
	client, _, _, _ := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	status, err := client.RemoteControl.ReadStatus(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if status.ServerName == "" {
		t.Fatal("remoteControl/status/read returned empty server name")
	}
}

func TestRealAppServerModelList(t *testing.T) {
	client, _, _, _ := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	response, err := client.Models.List(ctx, protocol.ModelListParams{})
	if err != nil {
		t.Fatal(err)
	}
	for _, model := range response.Data {
		if model.ID == "mock-model" {
			return
		}
	}
	t.Fatalf("model list did not include mock-model: %#v", response.Data)
}

func TestRealAppServerProtocolModeExperimentalGate(t *testing.T) {
	client, _, _, workdir := newRealAppServerClientWithConfig(t, nil, ClientConfig{
		ProtocolMode: ProtocolModeStable,
	})
	ctx := realAppServerContext(t)

	_, err := client.Threads.Start(ctx, ThreadStartOptions{
		CWD:                   workdir,
		MockExperimentalField: "stable-must-reject",
	})
	var cfgErr *ConfigError
	if !errors.As(err, &cfgErr) {
		t.Fatalf("err = %T, want *ConfigError for experimental field in stable mode", err)
	}
}

func TestRealAppServerUnauthenticatedAccountRead(t *testing.T) {
	client, _, _, _ := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	response, err := client.Accounts.Read(ctx, false)
	if err != nil {
		t.Fatal(err)
	}
	if account, ok := response.Account.Value(); ok {
		t.Fatalf("account = %#v, want unauthenticated empty account", account)
	}
}

type realResponsesFixture struct {
	t       *testing.T
	server  *httptest.Server
	mu      sync.Mutex
	bodies  [][]byte
	streams []string
}

func newRealResponsesFixture(t *testing.T, streams ...string) *realResponsesFixture {
	t.Helper()
	fixture := &realResponsesFixture{t: t, streams: append([]string(nil), streams...)}
	fixture.server = httptest.NewServer(http.HandlerFunc(fixture.handle))
	t.Cleanup(fixture.server.Close)
	return fixture
}

func (f *realResponsesFixture) URL() string {
	return f.server.URL
}

func (f *realResponsesFixture) handle(w http.ResponseWriter, r *http.Request) {
	switch {
	case r.Method == http.MethodPost && r.URL.Path == "/v1/responses":
		f.mu.Lock()
		f.bodies = append(f.bodies, nil)
		stream := realAssistantResponseSSE("ok")
		if len(f.streams) > 0 {
			stream = f.streams[0]
			f.streams = f.streams[1:]
		}
		f.mu.Unlock()
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = w.Write([]byte(stream))
	case r.Method == http.MethodGet && r.URL.Path == "/v1/models":
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"object":"list","data":[{"id":"mock-model","object":"model","created":0,"owned_by":"codex-go-sdk-test"}]}`))
	default:
		http.NotFound(w, r)
	}
}

func newRealAppServerClient(t *testing.T, streams ...string) (*Client, *realResponsesFixture, string, string) {
	t.Helper()
	return newRealAppServerClientWithConfig(t, streams, ClientConfig{})
}

func newRealAppServerClientWithConfig(t *testing.T, streams []string, overrides ClientConfig) (*Client, *realResponsesFixture, string, string) {
	t.Helper()
	runtimePath := requireRealAppServerRuntime(t)
	fixture := newRealResponsesFixture(t, streams...)
	codexHome := t.TempDir()
	workdir := t.TempDir()
	writeRealAppServerConfig(t, codexHome, fixture.URL())

	cfg := overrides
	cfg.CodexPath = runtimePath
	cfg.CWD = workdir
	if cfg.Env == nil {
		cfg.Env = map[string]string{}
	}
	cfg.Env["CODEX_HOME"] = codexHome
	cfg.ConfigOverrides = mergeRealAppServerConfigOverrides(cfg.ConfigOverrides, fixture.URL())

	ctx := realAppServerContext(t)
	client, err := NewClient(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	return client, fixture, runtimePath, workdir
}

func mergeRealAppServerConfigOverrides(existing map[string]string, serverURL string) map[string]string {
	merged := map[string]string{
		"model":                                             "mock-model",
		"model_provider":                                    "mock_provider",
		"model_providers.mock_provider.name":                "Mock provider for Go SDK tests",
		"model_providers.mock_provider.base_url":            serverURL + "/v1",
		"model_providers.mock_provider.wire_api":            "responses",
		"model_providers.mock_provider.request_max_retries": "0",
		"model_providers.mock_provider.stream_max_retries":  "0",
		"approval_policy":                                   "never",
		"sandbox_mode":                                      "danger-full-access",
	}
	for key, value := range existing {
		merged[key] = value
	}
	return merged
}

func requireRealAppServerRuntime(t *testing.T) string {
	t.Helper()
	path := os.Getenv("CODEX_EXEC_PATH")
	if path == "" {
		if os.Getenv("CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER") == "1" {
			t.Fatal("CODEX_EXEC_PATH is required when CODEX_GO_SDK_REQUIRE_REAL_APP_SERVER=1")
		}
		t.Skip("CODEX_EXEC_PATH is not set")
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("CODEX_EXEC_PATH %q is not usable: %v", path, err)
	}
	if info.IsDir() {
		t.Fatalf("CODEX_EXEC_PATH %q is a directory", path)
	}
	return path
}

func realAppServerContext(t *testing.T) context.Context {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), realAppServerTimeout)
	t.Cleanup(cancel)
	return ctx
}

func writeRealAppServerConfig(t *testing.T, codexHome string, serverURL string) {
	t.Helper()
	content := fmt.Sprintf(`
model = "mock-model"
approval_policy = "never"
sandbox_mode = "danger-full-access"
chatgpt_base_url = %[1]q
model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for Go SDK tests"
base_url = %[2]q
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
`, serverURL, serverURL+"/v1")
	if err := os.WriteFile(filepath.Join(codexHome, "config.toml"), []byte(strings.TrimSpace(content)+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
}

func realAssistantResponseSSE(text string) string {
	escaped, err := json.Marshal(text)
	if err != nil {
		panic(err)
	}
	message := fmt.Sprintf(`{"type":"response.output_item.done","item":{"type":"message","role":"assistant","id":"msg-1","content":[{"type":"output_text","text":%s}],"phase":"final_answer"}}`, escaped)
	completed := `{"type":"response.completed","response":{"id":"resp-1","usage":{"input_tokens":0,"input_tokens_details":null,"output_tokens":0,"output_tokens_details":null,"total_tokens":0}}}`
	return "event: response.created\n" +
		"data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp-1\"}}\n\n" +
		"event: response.output_item.done\n" +
		"data: " + message + "\n\n" +
		"event: response.completed\n" +
		"data: " + completed + "\n\n"
}

func shellPrintCommand(text string) []string {
	if runtime.GOOS == "windows" {
		return []string{"cmd.exe", "/d", "/c", "echo " + text}
	}
	return []string{"/bin/sh", "-c", "printf " + shellSingleQuote(text)}
}

func shellSingleQuote(value string) string {
	return "'" + strings.ReplaceAll(value, "'", "'\"'\"'") + "'"
}

func waitForNotification(t *testing.T, ctx context.Context, stream *NotificationStream, method string) Notification {
	t.Helper()
	for {
		notification, ok := stream.Next(ctx)
		if !ok {
			t.Fatalf("stream closed waiting for %s: %v", method, stream.Err())
		}
		if notification.Method == method {
			return notification
		}
	}
}

func waitForBase64Output(t *testing.T, ctx context.Context, stream *NotificationStream, method string) string {
	t.Helper()
	notification := waitForNotification(t, ctx, stream, method)
	var encoded string
	switch payload := notification.Payload.(type) {
	case protocol.CommandExecOutputDeltaNotification:
		encoded = payload.DeltaBase64
	case protocol.ProcessOutputDeltaNotification:
		encoded = payload.DeltaBase64
	default:
		t.Fatalf("payload = %T, want output delta notification", notification.Payload)
	}
	decoded, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		t.Fatal(err)
	}
	return string(decoded)
}
