package codex

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestInjectedTransportConflictsFailBeforeRuntimeLookup(t *testing.T) {
	t.Setenv("PATH", t.TempDir())
	_, err := NewClient(context.Background(), ClientConfig{
		Transport: newScriptedInitializedTransport(t, nil),
		CodexPath: "/must/not/spawn",
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
}

func TestInjectedTransportNeverSpawnsPathCodex(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "codex")
	if runtime.GOOS == "windows" {
		path += ".bat"
	}
	if err := os.WriteFile(path, []byte("#!/bin/sh\nexit 77\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: newScriptedInitializedTransport(t, nil),
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
}

func TestMissingRuntimeReturnsTypedError(t *testing.T) {
	t.Setenv("PATH", t.TempDir())
	_, err := NewClient(context.Background(), ClientConfig{})
	var missing *RuntimeNotFoundError
	if !errors.As(err, &missing) {
		t.Fatalf("err = %T, want *RuntimeNotFoundError", err)
	}
	if len(missing.Searched) == 0 || strings.Contains(err.Error(), os.Getenv("PATH")) {
		t.Fatalf("runtime error leaked PATH or lacked searched locations: %#v", missing)
	}
}

func TestMissingExplicitRuntimeReturnsTypedError(t *testing.T) {
	missingPath := filepath.Join(t.TempDir(), "missing-codex")
	_, err := NewClient(context.Background(), ClientConfig{CodexPath: missingPath})
	var missing *RuntimeNotFoundError
	if !errors.As(err, &missing) {
		t.Fatalf("err = %T, want *RuntimeNotFoundError", err)
	}
	if len(missing.Searched) != 1 || missing.Searched[0] != "ClientConfig.CodexPath" {
		t.Fatalf("searched = %#v", missing.Searched)
	}
	if strings.Contains(err.Error(), missingPath) {
		t.Fatalf("runtime error leaked explicit path: %v", err)
	}
}

func TestConfigOverridesRejectSecretBearingValuesBeforeRuntimeLookup(t *testing.T) {
	tests := []struct {
		name  string
		key   string
		value string
	}{
		{name: "secret substring", key: "api_key", value: "shhh"},
		{
			name:  "literal provider header",
			key:   "model_providers.custom.http_headers.X-Api-Key",
			value: `"opaque-material-42"`,
		},
		{
			name:  "literal telemetry header",
			key:   "otel.exporter.otlp-http.headers.X-Key",
			value: `"opaque-material-43"`,
		},
		{
			name:  "literal provider query parameter",
			key:   "model_providers.custom.query_params.x-key",
			value: `"opaque-material-44"`,
		},
		{
			name:  "dynamic environment map",
			key:   "mcp_servers.private.env.FOO",
			value: `"opaque-material-45"`,
		},
		{
			name:  "unsupported non-secret path",
			key:   "model_providers.custom.base_url",
			value: `"https://example.invalid/v1"`,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := NewClient(context.Background(), ClientConfig{
				CodexPath:       "/must/not/spawn",
				ConfigOverrides: map[string]string{tt.key: tt.value},
			})
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T, want *ConfigError", err)
			}
			if strings.Contains(err.Error(), tt.key) || strings.Contains(err.Error(), tt.value) {
				t.Fatalf("error leaked rejected key/value: %v", err)
			}
		})
	}
}

func TestRuntimeEnvScrubsAndRejectsReservedHookNames(t *testing.T) {
	reservedNames := []string{
		"CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG",
		"CODEX_APP_SERVER_MANAGED_CONFIG_PATH",
		"CODEX_APP_SERVER_LOGIN_ISSUER",
		"CODEX_APP_SERVER_AUTH_BASE_URL_FOR_TESTS",
		"CODEX_APP_SERVER_SDK_INTEGRATION_TEST_MODE",
		"CODEX_REFRESH_TOKEN_URL_OVERRIDE",
		"CODEX_REVOKE_TOKEN_URL_OVERRIDE",
		"CODEX_APP_SERVER_LOGIN_CLIENT_ID",
		"CODEX_APP_SERVER_DEV_OPEN_APP_URL",
		"CODEX_APP_SERVER_TEST_USER_CONFIG_FILE",
		"CODEX_AUTHAPI_BASE_URL",
		"CODEX_CODE_MODE_HOST_PATH",
		"CODEX_EXEC_SERVER_NOISE_AUTH_TOKEN",
		"CODEX_EXEC_SERVER_NOISE_CHATGPT_ACCOUNT_ID",
		"CODEX_EXEC_SERVER_NOISE_ENVIRONMENT_ID",
		"CODEX_EXEC_SERVER_NOISE_REGISTRY_URL",
		"CODEX_EXEC_SERVER_URL",
		"CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
		"CODEX_TEST_RATE_LIMIT_RESET_REQUEST_TIMEOUT_MS",
		"CODEX_TEST_ALLOW_HTTP_REMOTE_PLUGIN_BUNDLE_DOWNLOADS",
	}
	parent := []string{"PATH=/bin"}
	for _, name := range reservedNames {
		parent = append(parent, name+"=ambient")
	}
	env := buildRuntimeEnv(parent, map[string]string{"SAFE_FLAG": "ok"})
	childKeys := make(map[string]struct{}, len(env))
	for _, item := range env {
		key, _, ok := strings.Cut(item, "=")
		if ok {
			childKeys[key] = struct{}{}
		}
	}
	for _, name := range reservedNames {
		if _, ok := childKeys[name]; ok {
			t.Errorf("reserved env %s leaked into child env", name)
		}
	}
	if _, ok := childKeys["SAFE_FLAG"]; !ok {
		t.Fatal("safe override missing from child env")
	}
	for _, name := range reservedNames {
		t.Run(name, func(t *testing.T) {
			_, err := NewClient(context.Background(), ClientConfig{
				Env: map[string]string{name: "override"},
			})
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T, want *ConfigError", err)
			}
		})
	}
}

func TestReservedRuntimeEnvTracksRustAuthAndDebugHooks(t *testing.T) {
	type hookSource struct {
		path     string
		exact    map[string]struct{}
		prefixes []string
	}
	sources := []hookSource{
		{
			path: "codex-rs/login/src/auth/manager.rs",
			exact: map[string]struct{}{
				"CODEX_REFRESH_TOKEN_URL_OVERRIDE":   {},
				"CODEX_REVOKE_TOKEN_URL_OVERRIDE":    {},
				"CODEX_APP_SERVER_LOGIN_CLIENT_ID":   {},
			},
		},
		{
			path:  "codex-rs/login/src/auth/personal_access_token.rs",
			exact: map[string]struct{}{"CODEX_AUTHAPI_BASE_URL": {}},
		},
		{path: "codex-rs/app-server/src/lib.rs", prefixes: []string{"CODEX_APP_SERVER_TEST_"}},
		{path: "codex-rs/app-server/src/request_processors/account_processor.rs", prefixes: []string{"CODEX_APP_SERVER_"}},
		{path: "codex-rs/app-server/src/request_processors/account_processor/rate_limit_resets.rs", prefixes: []string{"CODEX_TEST_"}},
		{path: "codex-rs/core-plugins/src/remote_bundle.rs", prefixes: []string{"CODEX_TEST_"}},
		{
			path:  "codex-rs/code-mode/src/remote_session.rs",
			exact: map[string]struct{}{"CODEX_CODE_MODE_HOST_PATH": {}},
		},
		{path: "codex-rs/exec-server/src/environment.rs", prefixes: []string{"CODEX_EXEC_SERVER_"}},
		{
			path:  "codex-rs/login/src/auth/default_client.rs",
			exact: map[string]struct{}{"CODEX_INTERNAL_ORIGINATOR_OVERRIDE": {}},
		},
	}
	envNamePattern := regexp.MustCompile(`"(CODEX_[A-Z0-9_]+)"`)
	for _, source := range sources {
		data, err := os.ReadFile(filepath.Join("..", "..", filepath.FromSlash(source.path)))
		if err != nil {
			t.Fatalf("read %s: %v", source.path, err)
		}
		for _, groups := range envNamePattern.FindAllStringSubmatch(string(data), -1) {
			match := groups[1]
			_, selected := source.exact[match]
			for _, prefix := range source.prefixes {
				selected = selected || strings.HasPrefix(match, prefix)
			}
			if selected && !isReservedEnv(match) {
				t.Errorf("Rust control hook %s from %s is not reserved by the Go SDK", match, source.path)
			}
		}
	}
}

func TestRuntimeEnvWindowsCaseInsensitiveOverride(t *testing.T) {
	env := buildRuntimeEnvForOS([]string{
		"Path=parent",
		"TEMP=temp",
	}, map[string]string{
		"PATH": "child",
	}, "windows")
	var pathEntries []string
	for _, item := range env {
		key, _, ok := strings.Cut(item, "=")
		if ok && strings.EqualFold(key, "PATH") {
			pathEntries = append(pathEntries, item)
		}
	}
	if len(pathEntries) != 1 || pathEntries[0] != "PATH=child" {
		t.Fatalf("PATH entries = %#v, want only explicit override", pathEntries)
	}
}

func TestExplicitRuntimePathPrecedesPathLookupAndLaunchesWithExpectedShape(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell fake runtime is POSIX-only")
	}
	recordDir := t.TempDir()
	explicitRuntime := writeFakeRuntime(t, recordDir)
	pathDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(pathDir, "codex"), []byte("#!/bin/sh\nexit 77\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", pathDir)
	client, err := NewClient(context.Background(), ClientConfig{
		CodexPath: explicitRuntime,
		CWD:       recordDir,
		Env:       map[string]string{"RECORD_DIR": recordDir, "SAFE_FLAG": "safe"},
		ConfigOverrides: map[string]string{
			"model":        `"gpt-5"`,
			"sandbox_mode": `"read-only"`,
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	assertFile(t, filepath.Join(recordDir, "cwd"), recordDir+"\n")
	assertFile(t, filepath.Join(recordDir, "safe_flag"), "safe\n")
	assertFile(t, filepath.Join(recordDir, "args"), "--config\nmodel=\"gpt-5\"\n--config\nsandbox_mode=\"read-only\"\napp-server\n--listen\nstdio://\n")
}

func TestPathDiscoveredRuntimeStartsUnderStrictDigestValidation(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell fake runtime is POSIX-only")
	}
	recordDir := t.TempDir()
	runtimePath := writeFakeRuntime(t, recordDir)
	pathDir := t.TempDir()
	if err := os.Symlink(runtimePath, filepath.Join(pathDir, "codex")); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", pathDir)
	client, err := NewClient(context.Background(), ClientConfig{
		Env: map[string]string{"RECORD_DIR": recordDir},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	if client.Metadata().RuntimePath != filepath.Join(pathDir, "codex") {
		t.Fatalf("runtime path = %s", client.Metadata().RuntimePath)
	}
}

func TestStartupContextCancellationAfterNewClientDoesNotKillRuntime(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("shell fake runtime is POSIX-only")
	}
	recordDir := t.TempDir()
	runtimePath := writeFakeRuntime(t, recordDir)
	ctx, cancel := context.WithCancel(context.Background())
	client, err := NewClient(ctx, ClientConfig{
		CodexPath: runtimePath,
		Env:       map[string]string{"RECORD_DIR": recordDir},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	cancel()
	time.Sleep(50 * time.Millisecond)

	if _, err := client.Raw().MemoryReset(context.Background()); err != nil {
		t.Fatalf("runtime died after startup context cancellation: %v", err)
	}
}

func TestNotificationOptOutValidation(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	_, err := NewClient(context.Background(), ClientConfig{
		Transport:           transport,
		NotificationOptOuts: NotificationOptOuts{Methods: []string{"turn/completed"}},
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != 0 {
		t.Fatal("initialize was sent after default-mode notification opt-out conflict")
	}

	rawOnlyClient, err := NewClient(context.Background(), ClientConfig{
		Transport:           newScriptedInitializedTransport(t, nil),
		Mode:                ClientModeRawOnly,
		NotificationOptOuts: NotificationOptOuts{Methods: []string{"turn/completed"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = rawOnlyClient.Close() })
	if len(rawOnlyClient.Metadata().DisabledHighLevelWorkflows) == 0 {
		t.Fatal("raw-only metadata did not expose disabled workflows")
	}
	wantDisabled := []string{
		"account/browser-login disabled in raw-only mode",
		"account/device-code-login disabled in raw-only mode",
		"command/exec disabled in raw-only mode",
		"fs/watch disabled in raw-only mode",
		"fuzzyFileSearch/sessionStart disabled in raw-only mode",
		"mcpServer/oauth/login disabled in raw-only mode",
		"process/spawn disabled in raw-only mode",
		"realtime/start disabled in raw-only mode",
		"remoteControl/pairing/start disabled in raw-only mode",
		"review/start requires turn/completed",
		"thread/fork disabled in raw-only mode",
		"thread/resume disabled in raw-only mode",
		"thread/start disabled in raw-only mode",
		"turn/start requires turn/completed",
	}
	if got := rawOnlyClient.Metadata().DisabledHighLevelWorkflows; !reflect.DeepEqual(got, wantDisabled) {
		t.Fatalf("raw-only disabled workflows = %#v, want %#v", got, wantDisabled)
	}
}

func TestMetadataReturnsIndependentDisabledWorkflowSlice(t *testing.T) {
	client := &Client{metadata: Metadata{
		DisabledHighLevelWorkflows: []string{"turn/start requires turn/completed"},
	}}

	first := client.Metadata()
	first.DisabledHighLevelWorkflows[0] = "mutated"
	second := client.Metadata()
	if got := second.DisabledHighLevelWorkflows; !reflect.DeepEqual(got, []string{"turn/start requires turn/completed"}) {
		t.Fatalf("metadata disabled workflows = %#v, want independent snapshot", got)
	}
}

func TestRealtimeNotificationOptOutDisablesHighLevelWorkflow(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	_, err := NewClient(context.Background(), ClientConfig{
		Transport:           transport,
		NotificationOptOuts: NotificationOptOuts{Methods: []string{"thread/realtime/closed"}},
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != 0 {
		t.Fatal("initialize was sent after realtime notification opt-out conflict")
	}

	rawOnlyClient, err := NewClient(context.Background(), ClientConfig{
		Transport:           newScriptedInitializedTransport(t, nil),
		Mode:                ClientModeRawOnly,
		NotificationOptOuts: NotificationOptOuts{Methods: []string{"thread/realtime/closed"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = rawOnlyClient.Close() })
	if !containsString(rawOnlyClient.Metadata().DisabledHighLevelWorkflows, "realtime/start requires thread/realtime/closed") {
		t.Fatalf("raw-only disabled workflows = %#v, want realtime dependency", rawOnlyClient.Metadata().DisabledHighLevelWorkflows)
	}
}

func TestNotificationOptOutsDisableHighLevelWorkflows(t *testing.T) {
	tests := []struct {
		name     string
		optOut   string
		workflow string
	}{
		{
			name:     "command",
			optOut:   "command/exec/outputDelta",
			workflow: "command/exec requires command/exec/outputDelta",
		},
		{
			name:     "filesystem",
			optOut:   "fs/changed",
			workflow: "fs/watch requires fs/changed",
		},
		{
			name:     "process",
			optOut:   "process/exited",
			workflow: "process/spawn requires process/exited",
		},
		{
			name:     "fuzzy search updates",
			optOut:   "fuzzyFileSearch/sessionUpdated",
			workflow: "fuzzyFileSearch/sessionStart requires fuzzyFileSearch/sessionUpdated",
		},
		{
			name:     "fuzzy search completed",
			optOut:   "fuzzyFileSearch/sessionCompleted",
			workflow: "fuzzyFileSearch/sessionStart requires fuzzyFileSearch/sessionCompleted",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			transport := newScriptedInitializedTransport(t, nil)
			_, err := NewClient(context.Background(), ClientConfig{
				Transport:           transport,
				NotificationOptOuts: NotificationOptOuts{Methods: []string{tt.optOut}},
			})
			var configErr *ConfigError
			if !errors.As(err, &configErr) {
				t.Fatalf("err = %T, want *ConfigError", err)
			}
			if len(transport.sentFrames()) != 0 {
				t.Fatalf("initialize was sent after %s notification opt-out conflict", tt.name)
			}

			rawOnlyClient, err := NewClient(context.Background(), ClientConfig{
				Transport:           newScriptedInitializedTransport(t, nil),
				Mode:                ClientModeRawOnly,
				NotificationOptOuts: NotificationOptOuts{Methods: []string{tt.optOut}},
			})
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() { _ = rawOnlyClient.Close() })
			if !containsString(rawOnlyClient.Metadata().DisabledHighLevelWorkflows, tt.workflow) {
				t.Fatalf("raw-only disabled workflows = %#v, want %s", rawOnlyClient.Metadata().DisabledHighLevelWorkflows, tt.workflow)
			}
		})
	}
}

func TestNotificationOptOutsRejectUnknownMethodBeforeInitialize(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	_, err := NewClient(context.Background(), ClientConfig{
		Transport:           transport,
		NotificationOptOuts: NotificationOptOuts{Methods: []string{"future/typo"}},
	})
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("err = %T, want *ConfigError", err)
	}
	if len(transport.sentFrames()) != 0 {
		t.Fatal("initialize was sent after unknown notification opt-out")
	}
}

func containsString(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

func writeFakeRuntime(t *testing.T, recordDir string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "fake-codex")
	script := fmt.Sprintf(`#!/bin/sh
set -eu
record_dir="${RECORD_DIR:-%s}"
pwd > "$record_dir/cwd"
printf '%%s\n' "$@" > "$record_dir/args"
printf '%%s\n' "${SAFE_FLAG:-}" > "$record_dir/safe_flag"
head -c 131072 /dev/zero >&2 || true
IFS= read -r line
printf '%%s\n' '{"id":"go-1","result":{"userAgent":"codex-go-test dev 0.0.0","codexHome":"/tmp/codex","platformFamily":"unix","platformOs":"linux","stableProtocolDigest":"%s","experimentalProtocolDigest":"%s","stableSchemaDigest":"%s","experimentalSchemaDigest":"%s","stableManifestDigest":"%s","experimentalManifestDigest":"%s","activeProtocolMode":"experimental"}}'
IFS= read -r line || true
IFS= read -r line || true
if [ -n "${line:-}" ]; then
  printf '%%s\n' '{"id":"go-2","result":{}}'
fi
sleep 5
`, recordDir, protocol.StableProtocolDigest, protocol.ExperimentalProtocolDigest, protocol.StableSchemaDigest, protocol.ExperimentalSchemaDigest, protocol.StableManifestDigest, protocol.ExperimentalManifestDigest)
	if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

func assertFile(t *testing.T, path string, want string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != want {
		t.Fatalf("%s = %q, want %q", path, data, want)
	}
}
