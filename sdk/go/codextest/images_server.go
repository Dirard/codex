package codextest

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

// ImageServer is a fake images server for clients of the images package: each request to
// /images/generations, /images/edits or /responses gets the next scripted reply
// (specs/go-sdk-images-client-2026-10-30.md, R1.12). Use URL() as images.Config.BaseURL.
type ImageServer struct {
	t      testing.TB
	server *httptest.Server

	mu       sync.Mutex
	replies  []func(http.ResponseWriter)
	requests []ImageRequest
}

// ImageRequest is a recorded request: credentials are redacted and image data replaced by
// "<N bytes>".
type ImageRequest struct {
	Method string
	Path   string
	Header http.Header
	Body   map[string]any
}

// ImageResponse scripts a successful image reply.
type ImageResponse struct {
	// Images are the image bytes; one 1x1 PNG when empty.
	Images        [][]byte
	Quality       string
	Size          string
	Background    string
	OutputFormat  string
	RevisedPrompt string
	GenerationID  string
	// InputTokens and OutputTokens fill usage (text input, image output).
	InputTokens, OutputTokens int64
	RequestID                 string
	ImagegenRequestID         string
}

// OnePixelPNG is a valid 1x1 PNG for tests.
var OnePixelPNG = mustBase64("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==")

// NewImageServer starts a server that the test closes when it ends. An unscripted request
// fails the test and gets a 500.
func NewImageServer(t testing.TB) *ImageServer {
	t.Helper()
	s := &ImageServer{t: t}
	s.server = httptest.NewServer(http.HandlerFunc(s.serve))
	t.Cleanup(s.server.Close)
	return s
}

// URL is the base URL of the images routes.
func (s *ImageServer) URL() string { return s.server.URL }

// Requests returns the recorded requests in order.
func (s *ImageServer) Requests() []ImageRequest {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]ImageRequest(nil), s.requests...)
}

// RespondImage queues a JSON image response.
func (s *ImageServer) RespondImage(resp ImageResponse) {
	s.enqueue(func(w http.ResponseWriter) {
		writeImageHeaders(w, resp)
		data := make([]map[string]any, 0, len(resp.imagesOrDefault()))
		for _, img := range resp.imagesOrDefault() {
			entry := map[string]any{"b64_json": base64.StdEncoding.EncodeToString(img)}
			if resp.GenerationID != "" {
				entry["generation_id"] = resp.GenerationID
			}
			if resp.RevisedPrompt != "" {
				entry["revised_prompt"] = resp.RevisedPrompt
			}
			data = append(data, entry)
		}
		writeJSONStatus(w, http.StatusOK, map[string]any{
			"created": time.Now().Unix(), "data": data, "quality": resp.Quality, "size": resp.Size,
			"background": resp.Background, "output_format": resp.OutputFormat, "usage": resp.usage(),
		})
	})
}

// RespondPartialImages queues a streamed images response: one partial_image event per
// partial, then a completed event with the final image (as the OpenAI platform streams).
func (s *ImageServer) RespondPartialImages(partials [][]byte, final ImageResponse) {
	s.enqueue(func(w http.ResponseWriter) {
		writeImageHeaders(w, final)
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		for i, partial := range partials {
			writeEvent(w, "image_generation.partial_image", map[string]any{
				"type": "image_generation.partial_image", "partial_image_index": i, "b64_json": base64.StdEncoding.EncodeToString(partial),
			})
		}
		img := final.imagesOrDefault()[0]
		writeEvent(w, "image_generation.completed", map[string]any{
			"type": "image_generation.completed", "b64_json": base64.StdEncoding.EncodeToString(img), "created_at": time.Now().Unix(),
			"quality": final.Quality, "size": final.Size, "background": final.Background, "output_format": final.OutputFormat,
			"revised_prompt": final.RevisedPrompt, "generation_id": final.GenerationID, "usage": final.usage(),
		})
	})
}

// RespondStreamWithoutFrames queues a stream as codex-lb (from go-v1.7.0) sends it for
// subscription accounts: a keepalive event, then the completed image, no partial frames and
// no [DONE].
func (s *ImageServer) RespondStreamWithoutFrames(final ImageResponse) {
	s.enqueue(func(w http.ResponseWriter) {
		writeImageHeaders(w, final)
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		writeEvent(w, "keepalive", map[string]any{"type": "keepalive"})
		img := final.imagesOrDefault()[0]
		writeEvent(w, "image_generation.completed", map[string]any{
			"type": "image_generation.completed", "b64_json": base64.StdEncoding.EncodeToString(img), "created_at": time.Now().Unix(),
			"generation_id": final.GenerationID, "quality": final.Quality, "size": final.Size, "background": final.Background,
			"output_format": final.OutputFormat, "usage": final.usage(),
		})
	})
}

// RespondStreamError queues a stream that ends with an error event instead of an image, as
// codex-lb sends {"type":"error","error":{"code":"invalid_upstream_response",…}}.
func (s *ImageServer) RespondStreamError(code string, message string) {
	s.enqueue(func(w http.ResponseWriter) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		writeEvent(w, "keepalive", map[string]any{"type": "keepalive"})
		writeEvent(w, "error", map[string]any{"type": "error", "error": map[string]any{"code": code, "message": message}})
	})
}

// RespondResponsesImage queues a streamed /responses reply of a hosted image_generation
// call: partial images, the completed image_generation_call, then response.completed.
func (s *ImageServer) RespondResponsesImage(partials [][]byte, final ImageResponse) {
	s.enqueue(func(w http.ResponseWriter) {
		writeImageHeaders(w, final)
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		for i, partial := range partials {
			writeEvent(w, "response.image_generation_call.partial_image", map[string]any{
				"type": "response.image_generation_call.partial_image", "partial_image_index": i,
				"partial_image_b64": base64.StdEncoding.EncodeToString(partial), "item_id": "ig_1",
			})
		}
		img := final.imagesOrDefault()[0]
		writeEvent(w, "response.output_item.done", map[string]any{
			"type": "response.output_item.done", "item": map[string]any{
				"type": "image_generation_call", "id": "ig_1", "status": "completed", "result": base64.StdEncoding.EncodeToString(img),
				"revised_prompt": final.RevisedPrompt, "quality": final.Quality, "size": final.Size,
				"background": final.Background, "output_format": final.OutputFormat,
			},
		})
		writeEvent(w, "response.completed", map[string]any{
			"type": "response.completed", "response": map[string]any{"usage": final.usage()},
		})
	})
}

// RespondUsageLimit queues a 429 usage_limit_reached for limitID with the reset time and
// the primary window in the x-codex-active-limit and x-<limit>-primary-* headers.
func (s *ImageServer) RespondUsageLimit(limitID string, resetsAt time.Time, usedPercent float64, windowMinutes int) {
	s.enqueue(func(w http.ResponseWriter) {
		prefix := "x-" + strings.ReplaceAll(strings.ToLower(limitID), "_", "-") + "-primary-"
		w.Header().Set("x-codex-active-limit", limitID)
		w.Header().Set(prefix+"used-percent", strconv.FormatFloat(usedPercent, 'f', -1, 64))
		w.Header().Set(prefix+"window-minutes", strconv.Itoa(windowMinutes))
		body := map[string]any{"type": "usage_limit_reached", "message": "The usage limit has been reached"}
		if !resetsAt.IsZero() {
			w.Header().Set(prefix+"reset-at", strconv.FormatInt(resetsAt.Unix(), 10))
			body["resets_at"] = resetsAt.Unix()
		}
		writeJSONStatus(w, http.StatusTooManyRequests, map[string]any{"error": body})
	})
}

// RespondRefusal queues a 400 content refusal with code (for example "moderation_blocked").
func (s *ImageServer) RespondRefusal(code string, message string) {
	s.enqueue(func(w http.ResponseWriter) {
		writeJSONStatus(w, http.StatusBadRequest, map[string]any{"error": map[string]any{
			"type": "invalid_request_error", "code": code, "message": message, "safety_identifier": "safety-test",
		}})
	})
}

// RespondUnavailable queues a temporary failure (for example 503 no_available_accounts),
// with Retry-After when retryAfter is positive.
func (s *ImageServer) RespondUnavailable(status int, code string, retryAfter time.Duration) {
	s.enqueue(func(w http.ResponseWriter) {
		if retryAfter > 0 {
			w.Header().Set("Retry-After", strconv.FormatFloat(retryAfter.Seconds(), 'f', -1, 64))
		}
		writeJSONStatus(w, status, map[string]any{"error": map[string]any{"code": code, "message": "temporarily unavailable"}})
	})
}

// RespondInvalidRequest queues a 400 invalid request about param.
func (s *ImageServer) RespondInvalidRequest(param string, message string) {
	s.enqueue(func(w http.ResponseWriter) {
		writeJSONStatus(w, http.StatusBadRequest, map[string]any{"error": map[string]any{
			"type": "invalid_request_error", "code": "invalid_request", "param": param, "message": message,
		}})
	})
}

// RespondStreamingUnsupported queues the 400 the ChatGPT images route returns for stream:
// true.
func (s *ImageServer) RespondStreamingUnsupported() {
	s.RespondInvalidRequest("stream", "Streaming image responses are not supported")
}

// RespondUnauthorized queues a 401.
func (s *ImageServer) RespondUnauthorized() {
	s.enqueue(func(w http.ResponseWriter) {
		writeJSONStatus(w, http.StatusUnauthorized, map[string]any{"error": map[string]any{"code": "invalid_api_key", "message": "unauthorized"}})
	})
}

func (s *ImageServer) enqueue(reply func(http.ResponseWriter)) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.replies = append(s.replies, reply)
}

func (s *ImageServer) serve(w http.ResponseWriter, r *http.Request) {
	body, _ := io.ReadAll(r.Body)
	record := ImageRequest{Method: r.Method, Path: r.URL.Path, Header: r.Header.Clone()}
	for _, name := range []string{"Authorization", "Chatgpt-Account-Id", "Cookie"} {
		if record.Header.Get(name) != "" {
			record.Header.Set(name, "<redacted>")
		}
	}
	var decoded map[string]any
	if json.Unmarshal(body, &decoded) == nil {
		record.Body = redactImages(decoded).(map[string]any)
	}
	s.mu.Lock()
	s.requests = append(s.requests, record)
	var reply func(http.ResponseWriter)
	if len(s.replies) > 0 {
		reply, s.replies = s.replies[0], s.replies[1:]
	}
	s.mu.Unlock()
	if reply == nil {
		s.t.Errorf("codextest: unscripted image request %s %s", r.Method, r.URL.Path)
		writeJSONStatus(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "codextest: unscripted request"}})
		return
	}
	reply(w)
}

// redactImages replaces data URLs and base64 image fields with their decoded size.
func redactImages(v any) any {
	switch value := v.(type) {
	case map[string]any:
		out := make(map[string]any, len(value))
		for key, item := range value {
			out[key] = redactImages(item)
		}
		return out
	case []any:
		out := make([]any, len(value))
		for i, item := range value {
			out[i] = redactImages(item)
		}
		return out
	case string:
		if rest, ok := strings.CutPrefix(value, "data:"); ok {
			if _, encoded, found := strings.Cut(rest, ";base64,"); found {
				return fmt.Sprintf("<%d bytes>", base64.StdEncoding.DecodedLen(len(encoded)))
			}
		}
		return value
	}
	return v
}

func (r ImageResponse) imagesOrDefault() [][]byte {
	if len(r.Images) == 0 {
		return [][]byte{OnePixelPNG}
	}
	return r.Images
}

func (r ImageResponse) usage() map[string]any {
	return map[string]any{
		"input_tokens": r.InputTokens, "output_tokens": r.OutputTokens, "total_tokens": r.InputTokens + r.OutputTokens,
		"input_tokens_details":  map[string]any{"text_tokens": r.InputTokens, "image_tokens": 0},
		"output_tokens_details": map[string]any{"text_tokens": 0, "image_tokens": r.OutputTokens},
	}
}

func writeImageHeaders(w http.ResponseWriter, r ImageResponse) {
	if r.RequestID != "" {
		w.Header().Set("X-Request-Id", r.RequestID)
	}
	if r.ImagegenRequestID != "" {
		w.Header().Set("X-Codex-Imagegen-Request-Id", r.ImagegenRequestID)
	}
}

func writeJSONStatus(w http.ResponseWriter, status int, body any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(body)
}

func writeEvent(w http.ResponseWriter, event string, data any) {
	encoded, _ := json.Marshal(data)
	_, _ = fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event, encoded)
	if flusher, ok := w.(http.Flusher); ok {
		flusher.Flush()
	}
}

func mustBase64(encoded string) []byte {
	data, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		panic(err)
	}
	return data
}
