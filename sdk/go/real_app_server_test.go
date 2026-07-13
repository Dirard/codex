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

	internaljsonrpc "github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

const realAppServerTimeout = 30 * time.Second

func TestRealAppServerLinuxUsesWorkspaceWriteSandbox(t *testing.T) {
	if runtime.GOOS != "linux" {
		t.Skip("Linux-only sandbox policy")
	}
	codexHome := t.TempDir()
	writeRealAppServerConfig(t, codexHome, "http://127.0.0.1")
	content, err := os.ReadFile(filepath.Join(codexHome, "config.toml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(content), `sandbox_mode = "workspace-write"`) {
		t.Fatalf("config.toml does not enable workspace-write sandbox: %s", content)
	}
}

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

	t.Run("SDK rejects reserved child environment", func(t *testing.T) {
		for name := range reservedRuntimeEnv {
			t.Run(name, func(t *testing.T) {
				ctx := realAppServerContext(t)
				_, err := NewClient(ctx, ClientConfig{
					CodexPath: runtimePath,
					Env:       map[string]string{name: "override"},
				})
				var cfgErr *ConfigError
				if !errors.As(err, &cfgErr) {
					t.Fatalf("err = %T, want *ConfigError for reserved hook env", err)
				}
			})
		}
	})

	t.Run("release process ignores managed-config debug path", func(t *testing.T) {
		codexHome := t.TempDir()
		fixture := newRealResponsesFixture(t)
		writeRealAppServerConfig(t, codexHome, fixture.URL())
		invalidManagedConfig := filepath.Join(t.TempDir(), "invalid-managed-config.toml")
		if err := os.WriteFile(invalidManagedConfig, []byte("invalid = [\n"), 0o600); err != nil {
			t.Fatal(err)
		}
		ctx := realAppServerContext(t)
		transport, err := internaljsonrpc.StartStdio(ctx, internaljsonrpc.StdioOptions{
			Path: runtimePath,
			Args: []string{"app-server", "--listen", "stdio://"},
			Dir:  t.TempDir(),
			Env: append(realAppServerDirectEnvironment(codexHome),
				"CODEX_APP_SERVER_MANAGED_CONFIG_PATH="+invalidManagedConfig,
				"CODEX_APP_SERVER_LOGIN_ISSUER="+fixture.URL(),
				"CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE=1",
				"CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS="+fixture.URL(),
			),
			MaxFrameBytes: DefaultMaxFrameBytes,
			StderrBytes:   DefaultStderrRingBytes,
		})
		if err != nil {
			t.Fatal(err)
		}
		client, err := NewClient(ctx, ClientConfig{Transport: transport})
		if err != nil {
			_ = transport.Close()
			t.Fatalf("release app-server honored a debug-only environment hook: %v; stderr: %s", err, transport.StderrTail())
		}
		t.Cleanup(func() { _ = client.Close() })
	})

	t.Run("release process rejects hidden plugin-startup bypass", func(t *testing.T) {
		ctx := realAppServerContext(t)
		transport, err := internaljsonrpc.StartStdio(ctx, internaljsonrpc.StdioOptions{
			Path: runtimePath,
			Args: []string{
				"app-server",
				"--listen",
				"stdio://",
				"--disable-plugin-startup-tasks-for-tests",
			},
			Env:           realAppServerDirectEnvironment(t.TempDir()),
			MaxFrameBytes: DefaultMaxFrameBytes,
			StderrBytes:   DefaultStderrRingBytes,
		})
		if err != nil {
			t.Fatal(err)
		}
		_, initErr := NewClient(ctx, ClientConfig{Transport: transport})
		_ = transport.Close()
		if initErr == nil {
			t.Fatal("release app-server accepted the hidden plugin-startup test argument")
		}
		if stderr := transport.StderrTail(); !strings.Contains(stderr, "disable-plugin-startup-tasks-for-tests") {
			t.Fatalf("release app-server rejected hidden argument without naming it; stderr: %s", stderr)
		}
	})
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
		var generatedDecodeErr protocol.DecodeError
		if errors.As(err, &generatedDecodeErr) {
			t.Fatalf("thread run generated decode failure at field %q", generatedDecodeErr.Field)
		}
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
	if _, err := client.Config.BatchWrite(ctx, protocol.ConfigBatchWriteParams{
		Edits: []protocol.ConfigEdit{{
			KeyPath:       "model",
			MergeStrategy: protocol.MergeStrategyReplace,
			Value:         json.RawMessage(`"mock-model"`),
		}},
	}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Config.ReadRequirements(ctx); err != nil {
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
	client, _, _, workdir := newRealAppServerClient(t, realAssistantResponseSSE("review fixture ok"))
	ctx := realAppServerContext(t)

	if _, err := client.Threads.List(ctx, protocol.ThreadListParams{}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.FileSystem.GetMetadata(ctx, protocol.FsGetMetadataParams{Path: protocol.AbsolutePathBuf(workdir)}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Skills.List(ctx, protocol.SkillsListParams{}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Hooks.List(ctx, protocol.HooksListParams{}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Apps.List(ctx, protocol.AppsListParams{}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.MCP.ListStatus(ctx, protocol.ListMcpServerStatusParams{}); err != nil {
		t.Fatal(err)
	}
	if _, err := client.Plugins.List(ctx, protocol.PluginListParams{}); err != nil {
		t.Fatal(err)
	}
	assertManifestBackedNotApplicable(t, []string{
		"marketplace/add",
		"marketplace/remove",
		"marketplace/upgrade",
	}, "the manifest exposes only side-effecting marketplace add, remove, and upgrade methods; Linux release tests do not mutate marketplace configuration")
	thread, err := client.Threads.Start(ctx, ThreadStartOptions{CWD: workdir})
	if err != nil {
		t.Fatal(err)
	}
	review, err := client.Reviews.Start(ctx, ReviewStartOptions{
		ThreadID: thread.ID(),
		Target: protocol.ReviewTarget{
			TypeValue:    "custom",
			Instructions: protocol.SomeNonNull("Review the fixture without changing files."),
		},
		Delivery: ReviewDeliveryDetached,
	})
	if err != nil {
		t.Fatal(err)
	}
	result, err := review.Wait(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if result.FinalResponse != "review fixture ok" {
		t.Fatalf("review final response = %q, want review fixture ok", result.FinalResponse)
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
	assertManifestBackedNotApplicable(t, []string{
		"remoteControl/pairing/start",
		"remoteControl/pairing/status",
		"remoteControl/client/list",
		"remoteControl/client/revoke",
	}, "paired remote-control service or session is unavailable in the hermetic app-server fixture; package tests cover the external-session variants")
}

func TestRealAppServerModelList(t *testing.T) {
	client, _, _, _ := newRealAppServerClient(t)
	ctx := realAppServerContext(t)

	response, err := client.Models.List(ctx, protocol.ModelListParams{})
	if err != nil {
		t.Fatal(err)
	}
	if len(response.Data) == 0 {
		t.Fatal("model/list returned an empty catalog")
	}
	seen := make(map[string]struct{}, len(response.Data))
	for _, model := range response.Data {
		if model.ID == "" {
			t.Fatal("model/list returned a model with an empty id")
		}
		if _, ok := seen[model.ID]; ok {
			t.Fatalf("model/list returned duplicate model id %q", model.ID)
		}
		seen[model.ID] = struct{}{}
	}
}

func TestRealAppServerProtocolModeExperimentalGate(t *testing.T) {
	stableClient, _, _, stableWorkdir := newRealAppServerClientWithConfig(t, nil, ClientConfig{
		ProtocolMode: ProtocolModeStable,
	})
	ctx := realAppServerContext(t)

	_, err := stableClient.Threads.Start(ctx, ThreadStartOptions{
		CWD:                   stableWorkdir,
		MockExperimentalField: "stable-must-reject",
	})
	var cfgErr *ConfigError
	if !errors.As(err, &cfgErr) {
		t.Fatalf("err = %T, want *ConfigError for experimental field in stable mode", err)
	}

	experimentalClient, _, _, experimentalWorkdir := newRealAppServerClientWithConfig(t, nil, ClientConfig{
		ProtocolMode: ProtocolModeExperimental,
	})
	thread, err := experimentalClient.Threads.Start(ctx, ThreadStartOptions{
		CWD:                   experimentalWorkdir,
		MockExperimentalField: "experimental-must-accept",
	})
	if err != nil {
		t.Fatal(err)
	}
	if thread.ID() == "" {
		t.Fatal("experimental thread/start returned an empty thread id")
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
	if response.RequiresOpenaiAuth {
		t.Fatal("unauthenticated custom provider unexpectedly requires OpenAI auth")
	}
}

func assertManifestBackedNotApplicable(t *testing.T, methods []string, reason string) {
	t.Helper()
	want := make(map[string]struct{}, len(methods))
	for _, method := range methods {
		want[method] = struct{}{}
	}
	for _, row := range generatedResourceCoverage {
		if _, ok := want[row.Method]; !ok {
			continue
		}
		if row.SafeIntegrationOwner != "" || row.SafeIntegrationReason != reason {
			t.Fatalf("%s integration coverage = owner %q reason %q", row.Method, row.SafeIntegrationOwner, row.SafeIntegrationReason)
		}
		delete(want, row.Method)
	}
	if len(want) != 0 {
		t.Fatalf("generated resource coverage is missing not-applicable methods: %v", want)
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

	ctx := realAppServerContext(t)
	client, err := NewClient(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	return client, fixture, runtimePath, workdir
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

func realAppServerDirectEnvironment(codexHome string) []string {
	env := make([]string, 0, 10)
	for _, key := range []string{"PATH", "TMPDIR", "LANG", "LC_ALL", "TZ"} {
		if value, ok := os.LookupEnv(key); ok {
			env = append(env, key+"="+value)
		}
	}
	return append(env,
		"HOME="+codexHome,
		"CODEX_HOME="+codexHome,
		"RUST_LOG=error",
	)
}

func writeRealAppServerConfig(t *testing.T, codexHome string, serverURL string) {
	t.Helper()
	content := fmt.Sprintf(`
model = "mock-model"
approval_policy = "never"
sandbox_mode = %q
chatgpt_base_url = %q
model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for Go SDK tests"
base_url = %q
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
`, realAppServerSandboxMode(), serverURL, serverURL+"/v1")
	if err := os.WriteFile(filepath.Join(codexHome, "config.toml"), []byte(strings.TrimSpace(content)+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
}

func realAppServerSandboxMode() string {
	if runtime.GOOS == "linux" {
		return "workspace-write"
	}
	return "danger-full-access"
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
