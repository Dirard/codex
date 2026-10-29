package images

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand/v2"
	"net/http"
	"strings"
	"time"
)

// DefaultTimeout bounds one HTTP attempt.
const DefaultTimeout = 180 * time.Second

// maxErrorBody bounds how much of an error response is read.
const maxErrorBody = 1 << 20

// maxImageResponse bounds a successful response (n images of up to ~30 MiB base64 each).
const maxImageResponse = 512 << 20

// Config configures a Client.
type Config struct {
	// BaseURL is the root of the images routes, for example
	// "https://host/backend-api/codex" or "https://api.openai.com/v1".
	BaseURL string
	// Auth sets the credentials of each request; nil sends none.
	Auth Auth
	// HTTPClient sends the requests; nil uses a client without a total timeout (each
	// attempt is bounded by Timeout).
	HTTPClient *http.Client
	// Headers are added to every request.
	Headers http.Header
	// Timeout bounds one attempt; zero means DefaultTimeout.
	Timeout time.Duration
	// Retry is the retry policy of temporary failures; the zero value is DefaultRetryPolicy.
	Retry RetryPolicy
	// Hooks observe every attempt.
	Hooks Hooks
	// ResponsesFallback, when set, delivers partial images through a hosted image_generation
	// tool on {BaseURL}/responses when the images routes cannot stream. It spends tokens of
	// the host model and, behind codex-lb, the account's model quota.
	ResponsesFallback *ResponsesFallback
}

// ResponsesFallback configures the Responses path for partial images.
type ResponsesFallback struct {
	// Model is the host model that calls the image tool, for example "gpt-6.1-sol".
	Model string
	// Always uses the Responses path for every request with partial images, without first
	// trying the images route.
	Always bool
}

// RetryPolicy retries temporary failures (IsRetryable). Refusals, usage limits and invalid
// requests are never retried.
type RetryPolicy struct {
	// MaxAttempts counts the first attempt; 1 disables retries. Zero means 3.
	MaxAttempts int
	// BaseDelay and MaxDelay bound the exponential backoff with jitter; a Retry-After of
	// the server is honored up to MaxDelay. Zero means 1 s and 30 s.
	BaseDelay time.Duration
	MaxDelay  time.Duration
}

// DefaultRetryPolicy is used when Config.Retry is zero.
var DefaultRetryPolicy = RetryPolicy{MaxAttempts: 3, BaseDelay: time.Second, MaxDelay: 30 * time.Second}

// Hooks observe requests without their content: no prompt, image bytes or credentials.
type Hooks struct {
	OnRequest  func(RequestInfo)
	OnResponse func(ResponseInfo)
}

// RequestInfo describes one HTTP attempt.
type RequestInfo struct {
	// Operation is "generate" or "edit".
	Operation string
	// Path is the request path, for example "/backend-api/codex/images/generations".
	Path      string
	Attempt   int
	Streaming bool
}

// ResponseInfo describes the outcome of one HTTP attempt.
type ResponseInfo struct {
	RequestInfo
	// Status is 0 when no response arrived.
	Status            int
	Duration          time.Duration
	RequestID         string
	ImagegenRequestID string
	// RateLimitHeaders are the response headers about limits and retries
	// (x-codex-*, x-*-primary-*, x-*-secondary-*, Retry-After), lower-case.
	RateLimitHeaders map[string]string
	// Err is the attempt's error, or the error that ended a stream after its images
	// (Result.StreamError), which is not retried.
	Err error
}

// Client generates and edits images. It is safe for concurrent use.
type Client struct {
	cfg  Config
	http *http.Client
}

// NewClient returns a client for cfg.
func NewClient(cfg Config) (*Client, error) {
	if strings.TrimSpace(cfg.BaseURL) == "" {
		return nil, errors.New("images: Config.BaseURL is required")
	}
	if !strings.HasPrefix(cfg.BaseURL, "http://") && !strings.HasPrefix(cfg.BaseURL, "https://") {
		return nil, fmt.Errorf("images: Config.BaseURL %q is not an http(s) URL", cfg.BaseURL)
	}
	cfg.BaseURL = strings.TrimRight(cfg.BaseURL, "/")
	if cfg.Timeout <= 0 {
		cfg.Timeout = DefaultTimeout
	}
	if cfg.Retry.MaxAttempts <= 0 {
		cfg.Retry.MaxAttempts = DefaultRetryPolicy.MaxAttempts
	}
	if cfg.Retry.BaseDelay <= 0 {
		cfg.Retry.BaseDelay = DefaultRetryPolicy.BaseDelay
	}
	if cfg.Retry.MaxDelay <= 0 {
		cfg.Retry.MaxDelay = DefaultRetryPolicy.MaxDelay
	}
	if cfg.ResponsesFallback != nil && strings.TrimSpace(cfg.ResponsesFallback.Model) == "" {
		return nil, errors.New("images: Config.ResponsesFallback.Model is required")
	}
	client := cfg.HTTPClient
	if client == nil {
		client = &http.Client{}
	}
	return &Client{cfg: cfg, http: client}, nil
}

// Generate creates images from a prompt (POST {BaseURL}/images/generations).
func (c *Client) Generate(ctx context.Context, req GenerateRequest) (*Result, error) {
	if strings.TrimSpace(req.Prompt) == "" {
		return nil, errors.New("images: GenerateRequest.Prompt is required")
	}
	if err := validateOptions(req.Options); err != nil {
		return nil, err
	}
	body := map[string]any{"prompt": req.Prompt}
	applyOptions(body, req.Options)
	return c.run(ctx, operation{name: "generate", path: "/images/generations", body: body, options: req.Options, prompt: req.Prompt})
}

// Edit changes images (POST {BaseURL}/images/edits). Images[0] is the base image.
func (c *Client) Edit(ctx context.Context, req EditRequest) (*Result, error) {
	if strings.TrimSpace(req.Prompt) == "" {
		return nil, errors.New("images: EditRequest.Prompt is required")
	}
	if len(req.Images) == 0 {
		return nil, errors.New("images: EditRequest.Images needs at least the base image")
	}
	if err := validateOptions(req.Options); err != nil {
		return nil, err
	}
	for i, in := range req.Images {
		if len(in.data) == 0 && in.url == "" && in.fileID == "" {
			return nil, fmt.Errorf("images: EditRequest.Images[%d] is empty", i)
		}
	}
	if req.Mask != nil {
		if err := validateMask(req.Images[0], *req.Mask); err != nil {
			return nil, err
		}
	}
	images := make([]map[string]string, len(req.Images))
	for i, in := range req.Images {
		images[i] = in.wire()
	}
	body := map[string]any{"prompt": req.Prompt, "images": images}
	if req.Mask != nil {
		body["mask"] = req.Mask.wire()
	}
	if req.InputFidelity != "" {
		body["input_fidelity"] = string(req.InputFidelity)
	}
	applyOptions(body, req.Options)
	return c.run(ctx, operation{name: "edit", path: "/images/edits", body: body, options: req.Options, prompt: req.Prompt, inputs: req.Images, mask: req.Mask, fidelity: req.InputFidelity})
}

func validateOptions(o Options) error {
	if o.N < 0 {
		return errors.New("images: Options.N must not be negative")
	}
	if o.PartialImages < 0 || o.PartialImages > 3 {
		return errors.New("images: Options.PartialImages must be between 0 and 3")
	}
	if o.OutputCompression != nil && (*o.OutputCompression < 0 || *o.OutputCompression > 100) {
		return errors.New("images: Options.OutputCompression must be between 0 and 100")
	}
	return nil
}

func applyOptions(body map[string]any, o Options) {
	body["model"] = firstNonEmpty(o.Model, DefaultModel)
	if o.Background != "" {
		body["background"] = string(o.Background)
	}
	if o.Quality != "" {
		body["quality"] = string(o.Quality)
	}
	if o.Size != "" {
		body["size"] = o.Size
	}
	if o.N > 0 {
		body["n"] = o.N
	}
	if o.OutputFormat != "" {
		body["output_format"] = string(o.OutputFormat)
	}
	if o.OutputCompression != nil {
		body["output_compression"] = *o.OutputCompression
	}
	if o.Moderation != "" {
		body["moderation"] = string(o.Moderation)
	}
}

// operation is one Generate or Edit call.
type operation struct {
	name     string
	path     string
	body     map[string]any
	options  Options
	prompt   string
	inputs   []InputImage
	mask     *InputImage
	fidelity InputFidelity
}

// run sends op with retries and the streaming and Responses rules of Config.
func (c *Client) run(ctx context.Context, op operation) (*Result, error) {
	start := time.Now()
	attempts := 0
	streaming := op.options.PartialImages > 0
	if streaming && c.cfg.ResponsesFallback != nil && c.cfg.ResponsesFallback.Always {
		return c.runResponses(ctx, op, start, &attempts)
	}
	if streaming {
		op.body["stream"] = true
		op.body["partial_images"] = op.options.PartialImages
	}
	result, err := c.retry(ctx, &attempts, func(ctx context.Context, attempt int) (*Result, error) {
		return c.attemptImages(ctx, op, attempt, streaming)
	})
	if streaming && errors.Is(err, ErrStreamingUnsupported) && c.cfg.ResponsesFallback != nil {
		return c.runResponses(ctx, op, start, &attempts)
	}
	return finish(result, err, start, attempts)
}

func finish(result *Result, err error, start time.Time, attempts int) (*Result, error) {
	if err != nil {
		return nil, err
	}
	result.Attempts = attempts
	result.Duration = time.Since(start)
	return result, nil
}

// retry calls attempt until it succeeds, fails for good or runs out of attempts.
func (c *Client) retry(ctx context.Context, attempts *int, attempt func(context.Context, int) (*Result, error)) (*Result, error) {
	var lastErr error
	for n := 1; n <= c.cfg.Retry.MaxAttempts; n++ {
		*attempts = n
		result, err := attempt(ctx, n)
		if err == nil {
			return result, nil
		}
		lastErr = err
		if !IsRetryable(err) || n == c.cfg.Retry.MaxAttempts || ctx.Err() != nil {
			break
		}
		delay := c.backoff(n)
		var unavailable *UnavailableError
		if errors.As(err, &unavailable) && unavailable.RetryAfter > 0 {
			delay = min(unavailable.RetryAfter, c.cfg.Retry.MaxDelay)
		}
		timer := time.NewTimer(delay)
		select {
		case <-ctx.Done():
			timer.Stop()
			return nil, ctx.Err()
		case <-timer.C:
		}
	}
	return nil, lastErr
}

// backoff is the delay before attempt n+1: exponential with full jitter in [d/2, d].
func (c *Client) backoff(n int) time.Duration {
	delay := c.cfg.Retry.BaseDelay << (n - 1)
	if delay <= 0 || delay > c.cfg.Retry.MaxDelay {
		delay = c.cfg.Retry.MaxDelay
	}
	return delay/2 + rand.N(delay/2+1)
}

// attemptImages sends one request to an images route.
func (c *Client) attemptImages(ctx context.Context, op operation, attempt int, streaming bool) (*Result, error) {
	info := RequestInfo{Operation: op.name, Path: c.pathOf(op.path), Attempt: attempt, Streaming: streaming}
	resp, done, err := c.send(ctx, info, op.path, op.body, op.options.IdempotencyKey)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	var result *Result
	if resp.StatusCode/100 != 2 {
		err = errorFromResponse(resp.StatusCode, resp.Header, readLimited(resp.Body, maxErrorBody))
	} else if streaming && strings.HasPrefix(resp.Header.Get("Content-Type"), "text/event-stream") {
		result, err = readImageStream(resp.Body, op.options.OnPartialImage)
	} else {
		result, err = decodeImageResponse(resp.Body)
	}
	if result != nil {
		result.RequestID = requestIDFrom(resp.Header)
		result.ImagegenRequestID = resp.Header.Get("x-codex-imagegen-request-id")
	}
	done(resp, attemptError(result, err))
	return result, err
}

// attemptError is what the hooks see as an attempt's error: err, or the error that ended a
// stream after its images.
func attemptError(result *Result, err error) error {
	if err == nil && result != nil {
		return result.StreamError
	}
	return err
}

func (c *Client) pathOf(path string) string {
	base := c.cfg.BaseURL
	if i := strings.Index(base, "://"); i >= 0 {
		if j := strings.IndexByte(base[i+3:], '/'); j >= 0 {
			return base[i+3+j:] + path
		}
	}
	return path
}

// send performs one HTTP POST bounded by Config.Timeout. It reports the request to the
// hooks; done reports the response once the caller has read it.
func (c *Client) send(ctx context.Context, info RequestInfo, path string, body any, idempotencyKey string) (*http.Response, func(*http.Response, error), error) {
	payload, err := json.Marshal(body)
	if err != nil {
		return nil, nil, fmt.Errorf("images: encode request: %w", err)
	}
	attemptCtx, cancel := context.WithTimeout(ctx, c.cfg.Timeout)
	request, err := http.NewRequestWithContext(attemptCtx, http.MethodPost, c.cfg.BaseURL+path, bytes.NewReader(payload))
	if err != nil {
		cancel()
		return nil, nil, fmt.Errorf("images: build request: %w", err)
	}
	for name, values := range c.cfg.Headers {
		for _, value := range values {
			request.Header.Add(name, value)
		}
	}
	request.Header.Set("Content-Type", "application/json")
	if info.Streaming {
		request.Header.Set("Accept", "text/event-stream")
	} else {
		request.Header.Set("Accept", "application/json")
	}
	if idempotencyKey != "" {
		request.Header.Set("Idempotency-Key", idempotencyKey)
	}
	if c.cfg.Auth != nil {
		if err := c.cfg.Auth.Apply(attemptCtx, request); err != nil {
			cancel()
			return nil, nil, fmt.Errorf("images: credentials: %w", err)
		}
	}
	if c.cfg.Hooks.OnRequest != nil {
		c.cfg.Hooks.OnRequest(info)
	}
	start := time.Now()
	response, err := c.http.Do(request)
	if err == nil && response.StatusCode == http.StatusUnauthorized && c.cfg.Auth != nil {
		if refresher, ok := c.cfg.Auth.(Refresher); ok {
			_ = response.Body.Close()
			response, err = c.resend(attemptCtx, request, payload, refresher)
		}
	}
	report := func(resp *http.Response, outcome error) {
		defer cancel()
		if c.cfg.Hooks.OnResponse == nil {
			return
		}
		out := ResponseInfo{RequestInfo: info, Duration: time.Since(start), Err: outcome}
		if resp != nil {
			out.Status = resp.StatusCode
			out.RequestID = requestIDFrom(resp.Header)
			out.ImagegenRequestID = resp.Header.Get("x-codex-imagegen-request-id")
			out.RateLimitHeaders = rateLimitHeaders(resp.Header)
		}
		c.cfg.Hooks.OnResponse(out)
	}
	if err != nil {
		outcome := transportError(ctx, err)
		report(nil, outcome)
		return nil, nil, outcome
	}
	return response, report, nil
}

// resend repeats a request once after the credentials were refreshed.
func (c *Client) resend(ctx context.Context, original *http.Request, payload []byte, refresher Refresher) (*http.Response, error) {
	if err := refresher.Refresh(ctx); err != nil {
		return nil, fmt.Errorf("images: refresh credentials: %w", err)
	}
	request := original.Clone(ctx)
	request.Body = io.NopCloser(bytes.NewReader(payload))
	request.ContentLength = int64(len(payload))
	if err := c.cfg.Auth.Apply(ctx, request); err != nil {
		return nil, fmt.Errorf("images: credentials: %w", err)
	}
	return c.http.Do(request)
}

// transportError classifies a failed round trip: the caller's ctx ends the call, anything
// else (connection, per-attempt timeout) is temporary.
func transportError(ctx context.Context, err error) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	return &UnavailableError{Message: err.Error(), cause: err}
}

func rateLimitHeaders(header http.Header) map[string]string {
	out := map[string]string{}
	for name, values := range header {
		lower := strings.ToLower(name)
		if lower == "x-codex-imagegen-request-id" {
			continue
		}
		if lower == "retry-after" || strings.HasPrefix(lower, "x-codex-") ||
			(strings.HasPrefix(lower, "x-") && (strings.Contains(lower, "-primary-") || strings.Contains(lower, "-secondary-"))) ||
			strings.HasPrefix(lower, "x-ratelimit-") {
			if len(values) > 0 {
				out[lower] = values[0]
			}
		}
	}
	return out
}

func readLimited(r io.Reader, limit int64) []byte {
	data, _ := io.ReadAll(io.LimitReader(r, limit))
	return data
}
