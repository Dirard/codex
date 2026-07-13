package codex

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"

	internaljsonrpc "github.com/openai/codex/sdk/go/internal/jsonrpc"
)

type authFixtureRequest struct {
	key           string
	backend       bool
	authenticated bool
}

type authHTTPFixture struct {
	server *httptest.Server

	mu       sync.Mutex
	requests []authFixtureRequest
}

func newAuthHTTPFixture(t *testing.T) *authHTTPFixture {
	t.Helper()
	fixture := &authHTTPFixture{}
	fixture.server = httptest.NewServer(http.HandlerFunc(fixture.handle))
	t.Cleanup(fixture.server.Close)
	return fixture
}

func (f *authHTTPFixture) URL() string {
	return f.server.URL
}

func (f *authHTTPFixture) handle(w http.ResponseWriter, r *http.Request) {
	isBackend := strings.HasPrefix(r.URL.Path, "/api/codex/")
	isAuth := strings.HasPrefix(r.URL.Path, "/api/accounts/") || r.URL.Path == "/oauth/token"
	if isBackend || isAuth {
		f.mu.Lock()
		f.requests = append(f.requests, authFixtureRequest{
			key:           r.Method + " " + r.URL.Path,
			backend:       isBackend,
			authenticated: strings.HasPrefix(r.Header.Get("Authorization"), "Bearer ") && r.Header.Get("ChatGPT-Account-ID") == "account-123",
		})
		f.mu.Unlock()
	}

	w.Header().Set("Content-Type", "application/json")
	switch {
	case r.Method == http.MethodPost && r.URL.Path == "/api/accounts/deviceauth/usercode":
		writeAuthFixtureJSON(w, map[string]any{
			"device_auth_id": "fixture-device-auth",
			"user_code":      "TEST-CODE",
			"interval":       "0",
		})
	case r.Method == http.MethodPost && r.URL.Path == "/api/accounts/deviceauth/token":
		writeAuthFixtureJSON(w, map[string]any{
			"authorization_code": "fixture-authorization-code",
			"code_challenge":     "fixture-code-challenge",
			"code_verifier":      "fixture-code-verifier",
		})
	case r.Method == http.MethodPost && r.URL.Path == "/oauth/token":
		writeAuthFixtureJSON(w, map[string]any{
			"id_token":      authFixtureIDToken(),
			"access_token":  "fixture-access-token",
			"refresh_token": "fixture-refresh-token",
		})
	case r.Method == http.MethodGet && r.URL.Path == "/api/codex/profiles/me":
		writeAuthFixtureJSON(w, map[string]any{
			"stats": map[string]any{
				"lifetime_tokens":          123,
				"peak_daily_tokens":        45,
				"longest_running_turn_sec": 6,
				"current_streak_days":      2,
				"longest_streak_days":      3,
				"daily_usage_buckets":      []any{},
			},
		})
	case r.Method == http.MethodGet && r.URL.Path == "/api/codex/usage":
		writeAuthFixtureJSON(w, map[string]any{
			"plan_type": "pro",
			"rate_limit": map[string]any{
				"allowed":       true,
				"limit_reached": false,
				"primary_window": map[string]any{
					"used_percent":         1,
					"limit_window_seconds": 300,
					"reset_after_seconds":  60,
					"reset_at":             1735689600,
				},
			},
		})
	case r.Method == http.MethodGet && r.URL.Path == "/api/codex/rate-limit-reset-credits":
		writeAuthFixtureJSON(w, map[string]any{
			"credits":            []any{},
			"available_count":    0,
			"total_earned_count": 0,
		})
	case r.Method == http.MethodPost && r.URL.Path == "/api/codex/rate-limit-reset-credits/consume":
		writeAuthFixtureJSON(w, map[string]any{
			"code":          "nothing_to_reset",
			"windows_reset": 0,
		})
	case r.Method == http.MethodGet && r.URL.Path == "/v1/models":
		writeAuthFixtureJSON(w, map[string]any{
			"object": "list",
			"data": []any{map[string]any{
				"id":       "mock-model",
				"object":   "model",
				"created":  0,
				"owned_by": "codex-go-sdk-auth-fixture",
			}},
		})
	default:
		http.NotFound(w, r)
	}
}

func writeAuthFixtureJSON(w http.ResponseWriter, value any) {
	_ = json.NewEncoder(w).Encode(value)
}

func authFixtureIDToken() string {
	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"none","typ":"JWT"}`))
	payload := base64.RawURLEncoding.EncodeToString([]byte(`{"email":"fixture@example.test","https://api.openai.com/auth":{"chatgpt_plan_type":"pro","chatgpt_user_id":"user-123","chatgpt_account_id":"account-123"}}`))
	signature := base64.RawURLEncoding.EncodeToString([]byte("fixture-signature"))
	return header + "." + payload + "." + signature
}

func (f *authHTTPFixture) assertNoAuthOrBackendRequests(t *testing.T) {
	t.Helper()
	f.mu.Lock()
	defer f.mu.Unlock()
	if len(f.requests) != 0 {
		t.Fatalf("unexpected auth/backend requests: %v", authFixtureRequestKeys(f.requests))
	}
}

func (f *authHTTPFixture) assertRequested(t *testing.T, expected map[string]int) {
	t.Helper()
	f.mu.Lock()
	defer f.mu.Unlock()
	counts := make(map[string]int)
	for _, request := range f.requests {
		counts[request.key]++
	}
	for key, want := range expected {
		if got := counts[key]; got != want {
			t.Fatalf("request %s count = %d, want %d; all requests: %v", key, got, want, authFixtureRequestKeys(f.requests))
		}
	}
}

func (f *authHTTPFixture) assertBackendRequestsAuthenticated(t *testing.T) {
	t.Helper()
	f.mu.Lock()
	defer f.mu.Unlock()
	found := false
	for _, request := range f.requests {
		if !request.backend {
			continue
		}
		found = true
		if !request.authenticated {
			t.Fatalf("backend request %s omitted fixture authentication headers", request.key)
		}
	}
	if !found {
		t.Fatal("auth fixture did not receive a backend request")
	}
}

func authFixtureRequestKeys(requests []authFixtureRequest) []string {
	keys := make([]string, 0, len(requests))
	for _, request := range requests {
		keys = append(keys, request.key)
	}
	return keys
}

func newAuthFixtureClient(t *testing.T) (*Client, *authHTTPFixture, string) {
	t.Helper()
	runtimePath := requireAuthFixtureRuntime(t)
	fixture := newAuthHTTPFixture(t)
	codexHome := t.TempDir()
	writeAuthFixtureConfig(t, codexHome, fixture.URL())

	transport, err := internaljsonrpc.StartStdio(authFixtureContext(t), internaljsonrpc.StdioOptions{
		Path: runtimePath,
		Args: []string{
			"--listen",
			"stdio://",
			"--disable-plugin-startup-tasks-for-tests",
		},
		Env:           authFixtureEnvironment(codexHome, fixture.URL()),
		MaxFrameBytes: 4 << 20,
		StderrBytes:   64 << 10,
	})
	if err != nil {
		t.Fatal(err)
	}
	client, err := NewClient(authFixtureContext(t), ClientConfig{Transport: transport})
	if err != nil {
		_ = transport.Close()
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	return client, fixture, codexHome
}

func requireAuthFixtureRuntime(t *testing.T) string {
	t.Helper()
	if runtime.GOOS != "linux" {
		if os.Getenv("CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE") == "1" {
			t.Fatal("the Go SDK auth fixture is Linux-only")
		}
		t.Skip("the Go SDK auth fixture is Linux-only")
	}
	path := os.Getenv("CODEX_GO_SDK_AUTH_FIXTURE_PATH")
	if path == "" {
		if os.Getenv("CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE") == "1" {
			t.Fatal("CODEX_GO_SDK_AUTH_FIXTURE_PATH is required when CODEX_GO_SDK_REQUIRE_AUTH_FIXTURE=1")
		}
		t.Skip("CODEX_GO_SDK_AUTH_FIXTURE_PATH is not set")
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("CODEX_GO_SDK_AUTH_FIXTURE_PATH %q is not usable: %v", path, err)
	}
	if info.IsDir() {
		t.Fatalf("CODEX_GO_SDK_AUTH_FIXTURE_PATH %q is a directory", path)
	}
	if releasePath := os.Getenv("CODEX_EXEC_PATH"); releasePath != "" {
		fixturePath, fixtureErr := filepath.EvalSymlinks(path)
		releasePath, releaseErr := filepath.EvalSymlinks(releasePath)
		if fixtureErr == nil && releaseErr == nil && fixturePath == releasePath {
			t.Fatal("auth fixture path must be distinct from release CODEX_EXEC_PATH")
		}
	}
	return path
}

func authFixtureEnvironment(codexHome string, issuer string) []string {
	env := make([]string, 0, 12)
	for _, key := range []string{"PATH", "TMPDIR", "LANG", "LC_ALL", "TZ"} {
		if value, ok := os.LookupEnv(key); ok {
			env = append(env, key+"="+value)
		}
	}
	return append(env,
		"HOME="+codexHome,
		"CODEX_HOME="+codexHome,
		"CODEX_APP_SERVER_DISABLE_MANAGED_CONFIG=1",
		"CODEX_APP_SERVER_LOGIN_ISSUER="+issuer,
		"RUST_LOG=error",
	)
}

func writeAuthFixtureConfig(t *testing.T, codexHome string, serverURL string) {
	t.Helper()
	content := fmt.Sprintf(`
model = "mock-model"
model_provider = "mock_provider"
approval_policy = "never"
sandbox_mode = "read-only"
chatgpt_base_url = %q
cli_auth_credentials_store = "file"

[model_providers.mock_provider]
name = "Mock provider for Go SDK auth tests"
base_url = %q
requires_openai_auth = true
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[features]
shell_snapshot = false
`, serverURL, serverURL+"/v1")
	if err := os.WriteFile(filepath.Join(codexHome, "config.toml"), []byte(strings.TrimSpace(content)+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
}
