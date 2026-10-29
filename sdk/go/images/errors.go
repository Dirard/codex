package images

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// ErrStreamingUnsupported reports that the server does not stream image responses, so
// partial images cannot be delivered on the images routes. Config.ResponsesFallback
// delivers them through the Responses API instead.
var ErrStreamingUnsupported = errors.New("images: the server does not stream image responses")

// RateLimitWindow is one window of a usage limit.
type RateLimitWindow struct {
	UsedPercent   float64
	WindowMinutes int
	ResetsAt      time.Time
}

// UsageLimitError reports an exhausted usage limit (HTTP 429 usage_limit_reached and the
// like). It is also the failure of an imageGeneration turn item
// (codex.ImageGenerationFailure). It is never retried.
type UsageLimitError struct {
	// Type is "usageLimitExceeded" for a usage limit, "usageNotIncluded" when the plan has
	// no image generation, and "insufficientQuota" when a paid account ran out of quota.
	Type string
	// LimitID names the limit, for example "image_gen" or "imagegen_premium".
	LimitID string
	// ResetsAt is when the limit resets, in UTC; zero when unknown.
	ResetsAt time.Time
	// UsedPercent and WindowMinutes describe the exhausted window, when known.
	UsedPercent   float64
	WindowMinutes int
	// Primary and Secondary are the windows the server reported.
	Primary, Secondary *RateLimitWindow
	PlanType           string
	Message            string
	Status             int
	RequestID          string
}

func (e *UsageLimitError) Error() string {
	text := "images: usage limit"
	if e.LimitID != "" {
		text += " " + e.LimitID
	}
	if e.Type != "" && e.Type != usageLimitExceeded {
		text += " (" + e.Type + ")"
	}
	if !e.ResetsAt.IsZero() {
		text += ", resets at " + e.ResetsAt.Format(time.RFC3339)
	}
	if e.Message != "" {
		text += ": " + e.Message
	}
	return text
}

const usageLimitExceeded = "usageLimitExceeded"

// RefusalError reports that the backend refused the prompt or the images on content
// grounds. It is never retried.
type RefusalError struct {
	// Code is the backend's reason, for example "moderation_blocked", "invalid_prompt",
	// "cyber_policy" or "bio_policy".
	Code             string
	Message          string
	SafetyIdentifier string
	Status           int
	RequestID        string
}

func (e *RefusalError) Error() string {
	return fmt.Sprintf("images: refused (%s): %s", e.Code, e.Message)
}

// UnavailableError reports a temporary failure: a 5xx, an overloaded or rate-limited
// server (codex-lb 503 no_available_accounts, 429 rate_limit_error), or a transport error.
// It is retried.
type UnavailableError struct {
	Status     int
	Code       string
	Message    string
	RetryAfter time.Duration
	RequestID  string
	cause      error
}

func (e *UnavailableError) Error() string {
	if e.Status == 0 {
		return "images: unavailable: " + e.Message
	}
	text := fmt.Sprintf("images: unavailable (HTTP %d", e.Status)
	if e.Code != "" {
		text += " " + e.Code
	}
	text += ")"
	if e.Message != "" {
		text += ": " + e.Message
	}
	return text
}

func (e *UnavailableError) Unwrap() error { return e.cause }

// InvalidRequestError reports a request the backend rejected for its content or
// credentials (other 4xx). It is never retried.
type InvalidRequestError struct {
	Status    int
	Type      string
	Code      string
	Param     string
	Message   string
	RequestID string
	streaming bool
}

func (e *InvalidRequestError) Error() string {
	text := fmt.Sprintf("images: invalid request (HTTP %d", e.Status)
	if e.Code != "" {
		text += " " + e.Code
	}
	if e.Param != "" {
		text += " " + e.Param
	}
	return text + "): " + e.Message
}

// Is reports a streaming rejection as ErrStreamingUnsupported.
func (e *InvalidRequestError) Is(target error) bool {
	return target == ErrStreamingUnsupported && e.streaming
}

// IsRetryable reports whether err is temporary: an *UnavailableError, including a timed-out
// attempt. The caller's context ending, refusals, usage limits and invalid requests are not
// (the client returns the context's own error when the caller's ctx ends).
func IsRetryable(err error) bool {
	var unavailable *UnavailableError
	return errors.As(err, &unavailable)
}

// refusalCodes are the error codes of content refusals.
var refusalCodes = map[string]bool{
	"moderation_blocked":       true,
	"invalid_prompt":           true,
	"cyber_policy":             true,
	"bio_policy":               true,
	"content_policy_violation": true,
	"safety_violation":         true,
}

// apiErrorBody is the error envelope of OpenAI-style APIs and codex-lb.
type apiErrorBody struct {
	Error struct {
		Type               string          `json:"type"`
		Code               json.RawMessage `json:"code"`
		Message            string          `json:"message"`
		Param              string          `json:"param"`
		ResetsAt           *int64          `json:"resets_at"`
		PlanType           string          `json:"plan_type"`
		LimitWindowMinutes *int            `json:"limit_window_minutes"`
		SafetyIdentifier   string          `json:"safety_identifier"`
	} `json:"error"`
}

func (b apiErrorBody) code() string {
	var code string
	if json.Unmarshal(b.Error.Code, &code) == nil {
		return code
	}
	return ""
}

// errorFromResponse turns a non-2xx response into a typed error.
func errorFromResponse(status int, header http.Header, body []byte) error {
	var parsed apiErrorBody
	_ = json.Unmarshal(body, &parsed)
	code := parsed.code()
	message := parsed.Error.Message
	if message == "" {
		message = strings.TrimSpace(string(body))
		if len(message) > 300 {
			message = message[:300] + "…"
		}
	}
	requestID := requestIDFrom(header)
	retryAfter := parseRetryAfter(header.Get("Retry-After"), time.Now())

	if status == http.StatusTooManyRequests {
		switch {
		case parsed.Error.Type == "usage_limit_reached" || code == "usage_limit_reached":
			return usageLimitFrom(status, header, parsed, message, requestID)
		case parsed.Error.Type == "usage_not_included" || code == "usage_not_included":
			return &UsageLimitError{Type: "usageNotIncluded", Message: message, Status: status, RequestID: requestID, PlanType: parsed.Error.PlanType}
		case parsed.Error.Type == "insufficient_quota" || code == "insufficient_quota" || code == "credit_balance_exhausted" ||
			code == "organization_spend_limit_exceeded" || code == "project_spend_limit_exceeded" || code == "organization_usage_limit_exceeded":
			return &UsageLimitError{Type: "insufficientQuota", Message: message, Status: status, RequestID: requestID}
		}
		return &UnavailableError{Status: status, Code: firstNonEmpty(code, parsed.Error.Type), Message: message, RetryAfter: retryAfter, RequestID: requestID}
	}
	if status >= 500 || status == http.StatusRequestTimeout {
		return &UnavailableError{Status: status, Code: firstNonEmpty(code, parsed.Error.Type), Message: message, RetryAfter: retryAfter, RequestID: requestID}
	}
	if refusalCodes[code] {
		return &RefusalError{Code: code, Message: message, SafetyIdentifier: parsed.Error.SafetyIdentifier, Status: status, RequestID: requestID}
	}
	return &InvalidRequestError{
		Status: status, Type: parsed.Error.Type, Code: code, Param: parsed.Error.Param, Message: message, RequestID: requestID,
		streaming: strings.Contains(strings.ToLower(message), "streaming image responses are not supported") ||
			(parsed.Error.Param == "stream" && status == http.StatusBadRequest),
	}
}

func usageLimitFrom(status int, header http.Header, parsed apiErrorBody, message string, requestID string) *UsageLimitError {
	out := &UsageLimitError{
		Type:      usageLimitExceeded,
		LimitID:   strings.TrimSpace(header.Get("x-codex-active-limit")),
		Message:   message,
		Status:    status,
		RequestID: requestID,
		PlanType:  parsed.Error.PlanType,
	}
	prefix := "x-" + strings.ReplaceAll(strings.ToLower(firstNonEmpty(out.LimitID, "codex")), "_", "-")
	out.Primary = rateLimitWindow(header, prefix+"-primary")
	out.Secondary = rateLimitWindow(header, prefix+"-secondary")
	if parsed.Error.ResetsAt != nil {
		out.ResetsAt = time.Unix(*parsed.Error.ResetsAt, 0).UTC()
	}
	// The exhausted window: the one at 100 %, else the primary.
	exhausted := out.Primary
	for _, window := range []*RateLimitWindow{out.Primary, out.Secondary} {
		if window != nil && window.UsedPercent >= 100 && (exhausted == nil || exhausted.UsedPercent < 100 || window.ResetsAt.After(exhausted.ResetsAt)) {
			exhausted = window
		}
	}
	if exhausted != nil {
		out.UsedPercent = exhausted.UsedPercent
		out.WindowMinutes = exhausted.WindowMinutes
		if out.ResetsAt.IsZero() {
			out.ResetsAt = exhausted.ResetsAt
		}
	}
	if out.WindowMinutes == 0 && parsed.Error.LimitWindowMinutes != nil {
		out.WindowMinutes = *parsed.Error.LimitWindowMinutes
	}
	return out
}

func rateLimitWindow(header http.Header, prefix string) *RateLimitWindow {
	used, okUsed := parseFloat(header.Get(prefix + "-used-percent"))
	minutes, okMinutes := parseInt(header.Get(prefix + "-window-minutes"))
	resetAt, okReset := parseInt(header.Get(prefix + "-reset-at"))
	if !okUsed && !okMinutes && !okReset {
		return nil
	}
	window := &RateLimitWindow{UsedPercent: used, WindowMinutes: int(minutes)}
	if okReset {
		window.ResetsAt = time.Unix(resetAt, 0).UTC()
	}
	return window
}

func parseFloat(value string) (float64, bool) {
	f, err := strconv.ParseFloat(strings.TrimSpace(value), 64)
	return f, err == nil
}

func parseInt(value string) (int64, bool) {
	i, err := strconv.ParseInt(strings.TrimSpace(value), 10, 64)
	return i, err == nil
}

// parseRetryAfter reads Retry-After as seconds or an HTTP date.
func parseRetryAfter(value string, now time.Time) time.Duration {
	value = strings.TrimSpace(value)
	if value == "" {
		return 0
	}
	if seconds, err := strconv.ParseFloat(value, 64); err == nil && seconds >= 0 {
		return time.Duration(seconds * float64(time.Second))
	}
	if at, err := http.ParseTime(value); err == nil && at.After(now) {
		return at.Sub(now)
	}
	return 0
}

func requestIDFrom(header http.Header) string {
	return firstNonEmpty(header.Get("x-request-id"), header.Get("x-oai-request-id"))
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}
