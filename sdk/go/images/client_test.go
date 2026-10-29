package images_test

import (
	"bytes"
	"context"
	"errors"
	"image"
	"image/color"
	"image/png"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/codextest"
	"github.com/openai/codex/sdk/go/images"
)

func newClient(t *testing.T, cfg images.Config) *images.Client {
	t.Helper()
	client, err := images.NewClient(cfg)
	if err != nil {
		t.Fatal(err)
	}
	return client
}

func pngOf(t *testing.T, w, h int, transparent bool) []byte {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	for x := 0; x < w; x++ {
		for y := 0; y < h; y++ {
			img.Set(x, y, color.RGBA{R: 200, A: 255})
		}
	}
	if transparent {
		img.Set(0, 0, color.RGBA{})
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

func TestGenerateSendsTypedFieldsAndDecodesTheResult(t *testing.T) {
	srv := codextest.NewImageServer(t)
	picture := pngOf(t, 3, 2, false)
	srv.RespondImage(codextest.ImageResponse{
		Images: [][]byte{picture}, Quality: "low", Size: "1254x1254", Background: "opaque", OutputFormat: "png",
		GenerationID: "gen_1", InputTokens: 12, OutputTokens: 515, RequestID: "req_1", ImagegenRequestID: "ig_req_1",
	})
	client := newClient(t, images.Config{BaseURL: srv.URL() + "/backend-api/codex", Auth: images.BearerToken("secret")})
	compression := 80
	result, err := client.Generate(context.Background(), images.GenerateRequest{
		Prompt: "a red apple",
		Options: images.Options{
			Quality: images.QualityHigh, Size: "1024x1024", Background: images.BackgroundTransparent, N: 1,
			OutputFormat: images.OutputFormatWebP, OutputCompression: &compression, Moderation: images.ModerationLow,
			IdempotencyKey: "job-7",
		},
	})
	if err != nil {
		t.Fatal(err)
	}

	requests := srv.Requests()
	if len(requests) != 1 || requests[0].Path != "/backend-api/codex/images/generations" {
		t.Fatalf("requests = %+v", requests)
	}
	body := requests[0].Body
	want := map[string]any{
		"prompt": "a red apple", "model": images.DefaultModel, "quality": "high", "size": "1024x1024", "background": "transparent",
		"n": float64(1), "output_format": "webp", "output_compression": float64(80), "moderation": "low",
	}
	for key, value := range want {
		if body[key] != value {
			t.Errorf("body[%s] = %v, want %v", key, body[key], value)
		}
	}
	if requests[0].Header.Get("Authorization") != "<redacted>" || requests[0].Header.Get("Idempotency-Key") != "job-7" {
		t.Errorf("headers = %v", requests[0].Header)
	}

	img := result.Image()
	if !bytes.Equal(img.Data, picture) || img.MediaType != "image/png" || img.Width != 3 || img.Height != 2 || img.GenerationID != "gen_1" {
		t.Fatalf("image = %+v", img)
	}
	if result.Quality != images.QualityLow || result.Size != "1254x1254" || result.Background != images.BackgroundOpaque || result.OutputFormat != images.OutputFormatPNG {
		t.Fatalf("actual settings = %q %q %q %q", result.Quality, result.Size, result.Background, result.OutputFormat)
	}
	if result.Usage != (images.Usage{InputTokens: 12, InputTextTokens: 12, OutputTokens: 515, OutputImageTokens: 515, TotalTokens: 527}) {
		t.Fatalf("usage = %+v", result.Usage)
	}
	if result.RequestID != "req_1" || result.ImagegenRequestID != "ig_req_1" || result.Attempts != 1 || result.Duration <= 0 || result.Created.IsZero() {
		t.Fatalf("result meta = %+v", result)
	}
}

func TestEditSendsInputsInOrderAndChecksTheMask(t *testing.T) {
	srv := codextest.NewImageServer(t)
	client := newClient(t, images.Config{BaseURL: srv.URL()})
	base := pngOf(t, 4, 4, false)
	ctx := context.Background()

	small := pngOf(t, 2, 2, true)
	if _, err := client.Edit(ctx, images.EditRequest{Prompt: "p", Images: []images.InputImage{images.ImageBytes(base)}, Mask: ptr(images.ImageBytes(small))}); err == nil || !strings.Contains(err.Error(), "2x2") {
		t.Fatalf("small mask err = %v", err)
	}
	opaque := pngOf(t, 4, 4, false)
	if _, err := client.Edit(ctx, images.EditRequest{Prompt: "p", Images: []images.InputImage{images.ImageBytes(base)}, Mask: ptr(images.ImageBytes(opaque))}); err == nil || !strings.Contains(err.Error(), "alpha") {
		t.Fatalf("opaque mask err = %v", err)
	}
	if n := len(srv.Requests()); n != 0 {
		t.Fatalf("rejected masks sent %d requests", n)
	}

	srv.RespondImage(codextest.ImageResponse{})
	mask := pngOf(t, 4, 4, true)
	_, err := client.Edit(ctx, images.EditRequest{
		Prompt:        "make it blue",
		Images:        []images.InputImage{images.ImageBytes(base), images.ImageURL("https://example.com/ref.png"), images.ImageFileID("file_1")},
		Mask:          ptr(images.ImageBytes(mask)),
		InputFidelity: images.InputFidelityHigh,
	})
	if err != nil {
		t.Fatal(err)
	}
	body := srv.Requests()[0].Body
	inputs := body["images"].([]any)
	if len(inputs) != 3 {
		t.Fatalf("images = %v", inputs)
	}
	if got := inputs[0].(map[string]any)["image_url"]; !strings.HasPrefix(got.(string), "<") {
		t.Errorf("base image = %v, want a redacted data URL", got)
	}
	if inputs[1].(map[string]any)["image_url"] != "https://example.com/ref.png" || inputs[2].(map[string]any)["file_id"] != "file_1" {
		t.Errorf("references = %v", inputs[1:])
	}
	if body["input_fidelity"] != "high" || body["mask"] == nil || srv.Requests()[0].Path != "/images/edits" {
		t.Errorf("body = %v", body)
	}
}

func TestPartialImagesStreamFromTheImagesRoute(t *testing.T) {
	srv := codextest.NewImageServer(t)
	frames := [][]byte{pngOf(t, 1, 1, false), pngOf(t, 2, 1, false)}
	final := pngOf(t, 2, 2, false)
	srv.RespondPartialImages(frames, codextest.ImageResponse{Images: [][]byte{final}, Quality: "high", Size: "1024x1024", OutputTokens: 9})
	client := newClient(t, images.Config{BaseURL: srv.URL()})
	var got []images.PartialImage
	result, err := client.Generate(context.Background(), images.GenerateRequest{
		Prompt:  "p",
		Options: images.Options{PartialImages: 2, OnPartialImage: func(p images.PartialImage) { got = append(got, p) }},
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 2 || got[0].Index != 0 || got[1].Index != 1 || !bytes.Equal(got[1].Data, frames[1]) || got[0].MediaType != "image/png" {
		t.Fatalf("partials = %+v", got)
	}
	if !bytes.Equal(result.Image().Data, final) || result.Quality != images.QualityHigh || result.Usage.OutputTokens != 9 || result.ViaResponses {
		t.Fatalf("result = %+v", result)
	}
	body := srv.Requests()[0].Body
	if body["stream"] != true || body["partial_images"] != float64(2) {
		t.Fatalf("body = %v", body)
	}
}

func TestPartialImagesWithoutStreamingFailOrUseTheResponsesFallback(t *testing.T) {
	srv := codextest.NewImageServer(t)
	srv.RespondStreamingUnsupported()
	client := newClient(t, images.Config{BaseURL: srv.URL()})
	_, err := client.Generate(context.Background(), images.GenerateRequest{Prompt: "p", Options: images.Options{PartialImages: 1, OnPartialImage: func(images.PartialImage) {}}})
	if !errors.Is(err, images.ErrStreamingUnsupported) {
		t.Fatalf("err = %v, want ErrStreamingUnsupported", err)
	}

	srv.RespondStreamingUnsupported()
	frame, final := pngOf(t, 1, 1, false), pngOf(t, 2, 2, false)
	srv.RespondResponsesImage([][]byte{frame}, codextest.ImageResponse{Images: [][]byte{final}, Quality: "low", Size: "1254x1254", RevisedPrompt: "a red apple, studio light", OutputTokens: 600})
	client = newClient(t, images.Config{BaseURL: srv.URL(), ResponsesFallback: &images.ResponsesFallback{Model: "gpt-6.1-sol"}})
	var partials []images.PartialImage
	result, err := client.Generate(context.Background(), images.GenerateRequest{
		Prompt:  "a red apple",
		Options: images.Options{PartialImages: 1, Quality: images.QualityHigh, OnPartialImage: func(p images.PartialImage) { partials = append(partials, p) }},
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(partials) != 1 || !bytes.Equal(partials[0].Data, frame) {
		t.Fatalf("partials = %+v", partials)
	}
	if !result.ViaResponses || !bytes.Equal(result.Image().Data, final) || result.Image().RevisedPrompt != "a red apple, studio light" || result.Usage.OutputTokens != 600 {
		t.Fatalf("result = %+v", result)
	}
	requests := srv.Requests()
	responses := requests[len(requests)-1]
	if responses.Path != "/responses" || responses.Body["model"] != "gpt-6.1-sol" || responses.Body["stream"] != true {
		t.Fatalf("responses request = %+v", responses)
	}
	tool := responses.Body["tools"].([]any)[0].(map[string]any)
	if tool["type"] != "image_generation" || tool["partial_images"] != float64(1) || tool["quality"] != "high" {
		t.Fatalf("tool = %v", tool)
	}
}

func TestErrorsAreTyped(t *testing.T) {
	srv := codextest.NewImageServer(t)
	client := newClient(t, images.Config{BaseURL: srv.URL(), Retry: images.RetryPolicy{MaxAttempts: 1}})
	ctx := context.Background()
	reset := time.Unix(1793394000, 0).UTC()

	srv.RespondUsageLimit("imagegen_premium", reset, 100, 1440)
	_, err := client.Generate(ctx, images.GenerateRequest{Prompt: "p"})
	var limit *images.UsageLimitError
	if !errors.As(err, &limit) {
		t.Fatalf("err = %v, want UsageLimitError", err)
	}
	if limit.LimitID != "imagegen_premium" || !limit.ResetsAt.Equal(reset) || limit.UsedPercent != 100 || limit.WindowMinutes != 1440 ||
		limit.Primary == nil || limit.Type != "usageLimitExceeded" || images.IsRetryable(err) {
		t.Fatalf("limit = %+v", limit)
	}

	srv.RespondRefusal("moderation_blocked", "Your request was rejected by the safety system.")
	_, err = client.Generate(ctx, images.GenerateRequest{Prompt: "p"})
	var refusal *images.RefusalError
	if !errors.As(err, &refusal) || refusal.Code != "moderation_blocked" || refusal.SafetyIdentifier != "safety-test" || images.IsRetryable(err) {
		t.Fatalf("refusal err = %v", err)
	}

	srv.RespondUnavailable(503, "no_available_accounts", 2*time.Second)
	_, err = client.Generate(ctx, images.GenerateRequest{Prompt: "p"})
	var unavailable *images.UnavailableError
	if !errors.As(err, &unavailable) || unavailable.Status != 503 || unavailable.Code != "no_available_accounts" || unavailable.RetryAfter != 2*time.Second || !images.IsRetryable(err) {
		t.Fatalf("unavailable err = %v", err)
	}

	srv.RespondInvalidRequest("size", "Invalid size")
	_, err = client.Generate(ctx, images.GenerateRequest{Prompt: "p"})
	var invalid *images.InvalidRequestError
	if !errors.As(err, &invalid) || invalid.Param != "size" || invalid.Status != 400 || images.IsRetryable(err) || errors.Is(err, images.ErrStreamingUnsupported) {
		t.Fatalf("invalid err = %v", err)
	}
}

func TestRetriesOnlyTemporaryFailures(t *testing.T) {
	srv := codextest.NewImageServer(t)
	client := newClient(t, images.Config{BaseURL: srv.URL(), Retry: images.RetryPolicy{MaxAttempts: 3, BaseDelay: time.Millisecond, MaxDelay: 50 * time.Millisecond}})
	ctx := context.Background()

	srv.RespondUnavailable(503, "no_available_accounts", 10*time.Millisecond)
	srv.RespondUnavailable(502, "", 0)
	srv.RespondImage(codextest.ImageResponse{})
	result, err := client.Generate(ctx, images.GenerateRequest{Prompt: "p"})
	if err != nil || result.Attempts != 3 {
		t.Fatalf("result = %+v, err = %v", result, err)
	}

	srv.RespondRefusal("invalid_prompt", "no")
	if _, err := client.Generate(ctx, images.GenerateRequest{Prompt: "p"}); err == nil {
		t.Fatal("refusal succeeded")
	}
	srv.RespondUsageLimit("image_gen", time.Time{}, 100, 60)
	if _, err := client.Generate(ctx, images.GenerateRequest{Prompt: "p"}); err == nil {
		t.Fatal("usage limit succeeded")
	}
	if n := len(srv.Requests()); n != 5 {
		t.Fatalf("requests = %d, want 3 + 1 + 1 (refusal and limit are not retried)", n)
	}

	// Cancelling ctx ends the wait before a retry.
	slow := newClient(t, images.Config{BaseURL: srv.URL(), Retry: images.RetryPolicy{MaxAttempts: 3, BaseDelay: time.Hour, MaxDelay: time.Hour}})
	srv.RespondUnavailable(503, "", 0)
	cancelCtx, cancel := context.WithTimeout(ctx, 50*time.Millisecond)
	defer cancel()
	if _, err := slow.Generate(cancelCtx, images.GenerateRequest{Prompt: "p"}); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("cancelled err = %v", err)
	}
}

func TestEachAttemptHasItsOwnTimeout(t *testing.T) {
	var calls int
	var mu sync.Mutex
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		calls++
		n := calls
		mu.Unlock()
		if n == 1 {
			select {
			case <-r.Context().Done():
			case <-time.After(2 * time.Second):
			}
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"created":1,"data":[{"b64_json":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="}]}`))
	}))
	defer server.Close()
	client := newClient(t, images.Config{BaseURL: server.URL, Timeout: 50 * time.Millisecond, Retry: images.RetryPolicy{MaxAttempts: 2, BaseDelay: time.Millisecond}})
	result, err := client.Generate(context.Background(), images.GenerateRequest{Prompt: "p"})
	if err != nil || result.Attempts != 2 {
		t.Fatalf("result = %+v, err = %v", result, err)
	}
}

func TestHooksSeeEveryAttemptWithoutContent(t *testing.T) {
	srv := codextest.NewImageServer(t)
	var mu sync.Mutex
	var requests []images.RequestInfo
	var responses []images.ResponseInfo
	client := newClient(t, images.Config{
		BaseURL: srv.URL(), Auth: images.BearerToken("secret"),
		Retry: images.RetryPolicy{MaxAttempts: 2, BaseDelay: time.Millisecond},
		Hooks: images.Hooks{
			OnRequest:  func(info images.RequestInfo) { mu.Lock(); requests = append(requests, info); mu.Unlock() },
			OnResponse: func(info images.ResponseInfo) { mu.Lock(); responses = append(responses, info); mu.Unlock() },
		},
	})
	srv.RespondUsageLimit("imagegen_premium", time.Unix(1793394000, 0), 100, 1440)
	_, _ = client.Generate(context.Background(), images.GenerateRequest{Prompt: "secret prompt"})
	if len(requests) != 1 || len(responses) != 1 {
		t.Fatalf("hooks = %d requests, %d responses", len(requests), len(responses))
	}
	got := responses[0]
	if got.Operation != "generate" || got.Status != 429 || got.Attempt != 1 || got.Duration <= 0 || got.Err == nil {
		t.Fatalf("response info = %+v", got)
	}
	if got.RateLimitHeaders["x-codex-active-limit"] != "imagegen_premium" || got.RateLimitHeaders["x-imagegen-premium-primary-used-percent"] != "100" {
		t.Fatalf("rate-limit headers = %v", got.RateLimitHeaders)
	}
	for name := range got.RateLimitHeaders {
		if strings.Contains(name, "authorization") || name == "x-codex-imagegen-request-id" {
			t.Fatalf("hook saw %s", name)
		}
	}
}

type refreshingAuth struct {
	mu        sync.Mutex
	token     string
	refreshes int
}

func (a *refreshingAuth) Apply(_ context.Context, req *http.Request) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	req.Header.Set("Authorization", "Bearer "+a.token)
	return nil
}

func (a *refreshingAuth) Refresh(context.Context) error {
	a.mu.Lock()
	defer a.mu.Unlock()
	a.refreshes++
	a.token = "fresh"
	return nil
}

func TestAuthHeadersAndRefreshAfter401(t *testing.T) {
	var seen []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = append(seen, r.Header.Get("Authorization"))
		if r.Header.Get("Authorization") != "Bearer fresh" {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"data":[{"b64_json":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="}]}`))
	}))
	defer server.Close()
	auth := &refreshingAuth{token: "stale"}
	client := newClient(t, images.Config{BaseURL: server.URL, Auth: auth})
	if _, err := client.Generate(context.Background(), images.GenerateRequest{Prompt: "p"}); err != nil {
		t.Fatal(err)
	}
	if auth.refreshes != 1 || len(seen) != 2 || seen[0] != "Bearer stale" || seen[1] != "Bearer fresh" {
		t.Fatalf("refreshes = %d, seen = %v", auth.refreshes, seen)
	}

	t.Setenv("CODEXTEST_IMAGES_KEY", "fresh")
	envClient := newClient(t, images.Config{BaseURL: server.URL, Auth: images.BearerFromEnv("CODEXTEST_IMAGES_KEY")})
	if _, err := envClient.Generate(context.Background(), images.GenerateRequest{Prompt: "p"}); err != nil {
		t.Fatal(err)
	}
	t.Setenv("CODEXTEST_IMAGES_KEY", "")
	if _, err := envClient.Generate(context.Background(), images.GenerateRequest{Prompt: "p"}); err == nil || !strings.Contains(err.Error(), "CODEXTEST_IMAGES_KEY") {
		t.Fatalf("empty env err = %v", err)
	}
}

func TestRequestValidation(t *testing.T) {
	client := newClient(t, images.Config{BaseURL: "https://example.invalid"})
	ctx := context.Background()
	for name, call := range map[string]func() error{
		"empty prompt":       func() error { _, err := client.Generate(ctx, images.GenerateRequest{}); return err },
		"edit without image": func() error { _, err := client.Edit(ctx, images.EditRequest{Prompt: "p"}); return err },
		"too many partials": func() error {
			_, err := client.Generate(ctx, images.GenerateRequest{Prompt: "p", Options: images.Options{PartialImages: 4}})
			return err
		},
	} {
		if call() == nil {
			t.Errorf("%s: accepted", name)
		}
	}
	if _, err := images.NewClient(images.Config{BaseURL: "ftp://x"}); err == nil {
		t.Error("non-http base URL accepted")
	}
	if _, err := images.NewClient(images.Config{BaseURL: "https://x", ResponsesFallback: &images.ResponsesFallback{}}); err == nil {
		t.Error("fallback without model accepted")
	}
}

func ptr[T any](v T) *T { return &v }
