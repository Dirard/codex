package codex

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/openai/codex/sdk/go/images"
	"github.com/openai/codex/sdk/go/protocol"
)

// imagesAuthCacheTTL is how long ImagesAuth reuses a token before asking the app-server
// again, so a token Codex refreshed on its own is picked up.
const imagesAuthCacheTTL = 5 * time.Minute

// ImagesAuth returns credentials for an images.Client from the login of this app-server:
// the bearer token Codex uses (a ChatGPT access token or an API key) and, for ChatGPT, the
// chatgpt-account-id header. Codex keeps refreshing the token; after a 401 the images
// client asks the app-server to refresh it once (specs/go-sdk-images-client-2026-10-30.md,
// R1.2).
//
// It reads the token with the app-server's getAuthStatus request, a v1 compatibility
// method that the SDK does not expose otherwise.
func (c *Client) ImagesAuth() images.Auth {
	return &appServerImagesAuth{client: c, now: time.Now}
}

type appServerImagesAuth struct {
	client *Client
	now    func() time.Time

	mu        sync.Mutex
	token     string
	accountID string
	fetchedAt time.Time
}

type getAuthStatusParams struct {
	IncludeToken bool `json:"includeToken"`
	RefreshToken bool `json:"refreshToken"`
}

type getAuthStatusResponse struct {
	AuthMethod string `json:"authMethod"`
	AuthToken  string `json:"authToken"`
}

// Apply sets Authorization and, for a ChatGPT login, chatgpt-account-id.
func (a *appServerImagesAuth) Apply(ctx context.Context, req *http.Request) error {
	token, accountID, err := a.current(ctx, false)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", "Bearer "+token)
	if accountID != "" {
		req.Header.Set("chatgpt-account-id", accountID)
	}
	return nil
}

// Refresh asks the app-server to refresh its token.
func (a *appServerImagesAuth) Refresh(ctx context.Context) error {
	_, _, err := a.current(ctx, true)
	return err
}

func (a *appServerImagesAuth) current(ctx context.Context, refresh bool) (string, string, error) {
	a.mu.Lock()
	defer a.mu.Unlock()
	if !refresh && a.token != "" && a.now().Sub(a.fetchedAt) < imagesAuthCacheTTL {
		return a.token, a.accountID, nil
	}
	c := a.client
	if c == nil || c.rpc == nil {
		return "", "", &ClosedError{}
	}
	if err := c.terminationError(); err != nil {
		return "", "", err
	}
	var status getAuthStatusResponse
	if err := c.rpc.Call(ctx, "getAuthStatus", getAuthStatusParams{IncludeToken: true, RefreshToken: refresh}, &status, nil); err != nil {
		return "", "", fmt.Errorf("read the app-server login: %w", err)
	}
	if strings.TrimSpace(status.AuthToken) == "" {
		return "", "", errors.New("the app-server has no login with a token")
	}
	accountID := ""
	if strings.HasPrefix(strings.ToLower(status.AuthMethod), "chatgpt") {
		accountID = a.chatgptAccountID(ctx, status.AuthToken)
	}
	a.token, a.accountID, a.fetchedAt = status.AuthToken, accountID, a.now()
	return a.token, a.accountID, nil
}

// chatgptAccountID takes the workspace from account/read, else from the token's claims.
func (a *appServerImagesAuth) chatgptAccountID(ctx context.Context, token string) string {
	if account, err := a.client.Raw().AccountRead(ctx, protocol.GetAccountParams{}); err == nil {
		if routing, ok := account.WorkspaceRouting.Value(); ok && routing.ChatgptAccountID != "" {
			return routing.ChatgptAccountID
		}
	}
	return chatgptAccountIDFromToken(token)
}

// chatgptAccountIDFromToken reads https://api.openai.com/auth.chatgpt_account_id from the
// JWT payload, without verifying it (the backend does).
func chatgptAccountIDFromToken(token string) string {
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		return ""
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return ""
	}
	var claims struct {
		Auth struct {
			AccountID string `json:"chatgpt_account_id"`
		} `json:"https://api.openai.com/auth"`
	}
	if json.Unmarshal(payload, &claims) != nil {
		return ""
	}
	return claims.Auth.AccountID
}

// providerEntry is a [model_providers.<id>] table as config/read reports it.
type providerEntry struct {
	BaseURL            string            `json:"base_url"`
	EnvKey             string            `json:"env_key"`
	RequiresOpenAIAuth bool              `json:"requires_openai_auth"`
	HTTPHeaders        map[string]string `json:"http_headers"`
	EnvHTTPHeaders     map[string]string `json:"env_http_headers"`
}

const (
	chatgptImagesBaseURL  = "https://chatgpt.com/backend-api/codex"
	platformImagesBaseURL = "https://api.openai.com/v1"
)

// ImagesConfig builds an images.Config from the model provider this app-server runs with
// (config/read): its base URL, its static and environment HTTP headers, and its
// credentials — the provider's env_key read from this process's environment, otherwise the
// app-server login (ImagesAuth) for providers that require OpenAI auth and for the built-in
// openai provider. Set the remaining fields (timeout, retries, hooks) on the result
// (specs/go-sdk-images-client-2026-10-30.md, R1.3).
func (c *Client) ImagesConfig(ctx context.Context) (images.Config, error) {
	read, err := c.Raw().ConfigRead(ctx, protocol.ConfigReadParams{})
	if err != nil {
		return images.Config{}, fmt.Errorf("read the app-server config: %w", err)
	}
	providerID, _ := read.Config.ModelProvider.Value()
	if providerID == "" {
		providerID = "openai"
	}
	var providers map[string]providerEntry
	if raw, ok := read.Config.Additional["model_providers"]; ok {
		if err := json.Unmarshal(raw, &providers); err != nil {
			return images.Config{}, fmt.Errorf("decode model_providers: %w", err)
		}
	}
	entry, configured := providers[providerID]
	if !configured {
		if providerID != "openai" {
			return images.Config{}, fmt.Errorf("model provider %q is built in and has no images routes", providerID)
		}
		baseURL := platformImagesBaseURL
		if account, err := c.Raw().AccountRead(ctx, protocol.GetAccountParams{}); err == nil {
			if info, ok := account.Account.Value(); ok && info.TypeValue == "chatgpt" {
				baseURL = chatgptImagesBaseURL
			}
		}
		return images.Config{BaseURL: baseURL, Auth: c.ImagesAuth()}, nil
	}
	if strings.TrimSpace(entry.BaseURL) == "" {
		return images.Config{}, fmt.Errorf("model provider %q has no base_url", providerID)
	}
	cfg := images.Config{BaseURL: entry.BaseURL, Headers: http.Header{}}
	for name, value := range entry.HTTPHeaders {
		cfg.Headers.Set(name, value)
	}
	for name, variable := range entry.EnvHTTPHeaders {
		if value := os.Getenv(variable); value != "" {
			cfg.Headers.Set(name, value)
		}
	}
	switch {
	case entry.EnvKey != "":
		cfg.Auth = images.BearerFromEnv(entry.EnvKey)
	case entry.RequiresOpenAIAuth:
		cfg.Auth = c.ImagesAuth()
	}
	return cfg, nil
}
