package codex_test

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"testing"
	"time"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/codextest"
	"github.com/openai/codex/sdk/go/images"
	"github.com/openai/codex/sdk/go/protocol"
)

func fakeJWT(t *testing.T, accountID string) string {
	t.Helper()
	payload, err := json.Marshal(map[string]any{"https://api.openai.com/auth": map[string]any{"chatgpt_account_id": accountID}})
	if err != nil {
		t.Fatal(err)
	}
	enc := base64.RawURLEncoding
	return enc.EncodeToString([]byte(`{"alg":"none"}`)) + "." + enc.EncodeToString(payload) + ".sig"
}

func TestImagesAuthUsesTheAppServerLoginAndRefreshes(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	srv := codextest.New(t)
	stale, fresh := fakeJWT(t, "acct-1"), fakeJWT(t, "acct-1")+"x"
	var refreshes []bool
	srv.Handle("getAuthStatus", func(ctx context.Context, call *codextest.Call) (any, error) {
		var params struct {
			IncludeToken bool `json:"includeToken"`
			RefreshToken bool `json:"refreshToken"`
		}
		if err := call.Decode(&params); err != nil {
			return nil, err
		}
		if !params.IncludeToken {
			t.Errorf("getAuthStatus without includeToken")
		}
		refreshes = append(refreshes, params.RefreshToken)
		token := stale
		if params.RefreshToken {
			token = fresh
		}
		return map[string]any{"authMethod": "chatgpt", "authToken": token, "requiresOpenaiAuth": true}, nil
	})
	srv.Respond("account/read", map[string]any{"account": map[string]any{"type": "chatgpt", "email": "a@example.com", "planType": "pro"}, "requiresOpenaiAuth": true})
	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()

	auth := client.ImagesAuth()
	req, _ := http.NewRequest(http.MethodPost, "https://example.invalid/images/generations", nil)
	if err := auth.Apply(ctx, req); err != nil {
		t.Fatal(err)
	}
	if req.Header.Get("Authorization") != "Bearer "+stale || req.Header.Get("chatgpt-account-id") != "acct-1" {
		t.Fatalf("headers = %v", req.Header)
	}
	// A second request within the cache window does not ask the app-server again.
	if err := auth.Apply(ctx, req); err != nil || len(refreshes) != 1 {
		t.Fatalf("second Apply: err %v, getAuthStatus calls %d", err, len(refreshes))
	}
	refresher, ok := auth.(images.Refresher)
	if !ok {
		t.Fatal("ImagesAuth does not refresh")
	}
	if err := refresher.Refresh(ctx); err != nil {
		t.Fatal(err)
	}
	if err := auth.Apply(ctx, req); err != nil || req.Header.Get("Authorization") != "Bearer "+fresh {
		t.Fatalf("after refresh: err %v, headers %v", err, req.Header)
	}
	if len(refreshes) != 2 || refreshes[0] || !refreshes[1] {
		t.Fatalf("getAuthStatus refreshToken flags = %v", refreshes)
	}
}

func TestImagesConfigFromAKeyProvider(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	imageServer := codextest.NewImageServer(t)
	srv := codextest.New(t)
	srv.Respond("config/read", map[string]any{
		"config": map[string]any{
			"model_provider": "codex-lb",
			"model_providers": map[string]any{"codex-lb": map[string]any{
				"name": "openai", "base_url": imageServer.URL() + "/backend-api/codex", "env_key": "CODEXTEST_LB_KEY",
				"http_headers": map[string]any{"X-Team": "canon"}, "env_http_headers": map[string]any{"X-Region": "CODEXTEST_REGION"},
			}},
		},
		"origins": map[string]any{},
	})
	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()
	t.Setenv("CODEXTEST_LB_KEY", "lb-secret")
	t.Setenv("CODEXTEST_REGION", "eu")

	cfg, err := client.ImagesConfig(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.BaseURL != imageServer.URL()+"/backend-api/codex" || cfg.Headers.Get("X-Team") != "canon" || cfg.Headers.Get("X-Region") != "eu" {
		t.Fatalf("config = %+v", cfg)
	}
	imagesClient, err := images.NewClient(cfg)
	if err != nil {
		t.Fatal(err)
	}
	imageServer.RespondImage(codextest.ImageResponse{})
	if _, err := imagesClient.Generate(ctx, images.GenerateRequest{Prompt: "p"}); err != nil {
		t.Fatal(err)
	}
	request := imageServer.Requests()[0]
	if request.Path != "/backend-api/codex/images/generations" || request.Header.Get("Authorization") != "<redacted>" || request.Header.Get("X-Team") != "canon" {
		t.Fatalf("request = %+v", request)
	}
}

func TestGeneratedImageSharesTheImagesTypes(t *testing.T) {
	item := protocol.ThreadItem{
		ID:            protocol.SomeNonNull("ig_1"),
		TypeValue:     "imageGeneration",
		Status:        protocol.SomeNonNull(protocol.CommandExecutionStatus("completed")),
		RevisedPrompt: protocol.Some("a red circle"),
		GenerationID:  protocol.Some("gen_1"),
		Result:        json.RawMessage(`"` + base64.StdEncoding.EncodeToString(codextest.OnePixelPNG) + `"`),
	}
	img, ok, err := codex.GeneratedImageFromItem(item)
	if err != nil || !ok {
		t.Fatalf("ok %v err %v", ok, err)
	}
	shared := img.Image()
	if !bytes.Equal(shared.Data, codextest.OnePixelPNG) || shared.MediaType != "image/png" || shared.Width != 1 || shared.Height != 1 ||
		shared.RevisedPrompt != "a red circle" || shared.GenerationID != "gen_1" {
		t.Fatalf("Image() = %+v", shared)
	}
	shared.Data[0] = 0
	if img.Data[0] == 0 {
		t.Fatal("Image() shares the item's bytes")
	}
	if (codex.GeneratedImage{}).Image().Data != nil {
		t.Fatal("Image() of an image without bytes is not zero")
	}

	// The turn-item failure and the images client error are one type.
	var failure *codex.ImageGenerationFailure = &images.UsageLimitError{Type: "usageLimitExceeded", LimitID: "imagegen_premium"}
	var asError error = failure
	if asError.Error() == "" {
		t.Fatal("UsageLimitError has no message")
	}
}
