package images

import (
	"encoding/binary"
	"errors"
	"io"
	"net/http"
	"strings"
	"testing"
	"testing/iotest"
	"time"
)

func TestWebPSize(t *testing.T) {
	riff := func(chunk []byte) []byte {
		data := append([]byte("RIFF\x00\x00\x00\x00WEBP"), chunk...)
		return append(data, make([]byte, 32)...)
	}
	vp8x := append([]byte("VP8X\x0a\x00\x00\x00\x00\x00\x00\x00"), 0xff, 0x03, 0x00, 0x2b, 0x02, 0x00) // 1024 x 556
	if w, h := webpSize(riff(vp8x)); w != 1024 || h != 556 {
		t.Fatalf("VP8X = %dx%d", w, h)
	}
	vp8 := append([]byte("VP8 \x00\x00\x00\x00"), 0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a)
	vp8 = binary.LittleEndian.AppendUint16(vp8, 640)
	vp8 = binary.LittleEndian.AppendUint16(vp8, 480)
	if w, h := webpSize(riff(vp8)); w != 640 || h != 480 {
		t.Fatalf("VP8 = %dx%d", w, h)
	}
	bits := uint32(800-1) | uint32(600-1)<<14
	vp8l := append([]byte("VP8L\x00\x00\x00\x00\x2f"), binary.LittleEndian.AppendUint32(nil, bits)...)
	if w, h := webpSize(riff(vp8l)); w != 800 || h != 600 {
		t.Fatalf("VP8L = %dx%d", w, h)
	}
	if w, h := webpSize([]byte("not an image")); w != 0 || h != 0 {
		t.Fatalf("garbage = %dx%d", w, h)
	}
}

func TestUsageLimitFromSecondaryWindowAndRetryAfterDate(t *testing.T) {
	header := http.Header{}
	header.Set("x-codex-active-limit", "image_gen")
	header.Set("x-image-gen-primary-used-percent", "40")
	header.Set("x-image-gen-primary-window-minutes", "300")
	header.Set("x-image-gen-secondary-used-percent", "100")
	header.Set("x-image-gen-secondary-window-minutes", "10080")
	header.Set("x-image-gen-secondary-reset-at", "1793900000")
	err := errorFromResponse(429, header, []byte(`{"error":{"type":"usage_limit_reached","plan_type":"pro"}}`))
	limit, ok := err.(*UsageLimitError)
	if !ok {
		t.Fatalf("err = %T %v", err, err)
	}
	if limit.LimitID != "image_gen" || limit.UsedPercent != 100 || limit.WindowMinutes != 10080 || !limit.ResetsAt.Equal(time.Unix(1793900000, 0)) || limit.PlanType != "pro" {
		t.Fatalf("limit = %+v", limit)
	}
	if limit.Primary == nil || limit.Primary.UsedPercent != 40 || limit.Secondary == nil {
		t.Fatalf("windows = %+v %+v", limit.Primary, limit.Secondary)
	}

	now := time.Date(2026, 10, 30, 12, 0, 0, 0, time.UTC)
	if got := parseRetryAfter(now.Add(90*time.Second).Format(http.TimeFormat), now); got != 90*time.Second {
		t.Fatalf("Retry-After date = %v", got)
	}
	if got := parseRetryAfter("2", now); got != 2*time.Second {
		t.Fatalf("Retry-After seconds = %v", got)
	}

	// codex-lb from go-v1.7.0: an image limit with its windows in x-codex-*, reset-after only.
	lb := http.Header{}
	lb.Set("x-codex-active-limit", "imagegen_premium")
	lb.Set("Retry-After", "3600")
	lb.Set("x-codex-primary-used-percent", "100")
	lb.Set("x-codex-primary-window-minutes", "1440")
	lb.Set("x-codex-primary-reset-after-seconds", "3600")
	before := time.Now()
	lbErr := errorFromResponse(429, lb, []byte(`{"error":{"type":"usage_limit_reached","code":"usage_limit_reached","message":"The image generation limit was reached"}}`))
	lbLimit, ok := lbErr.(*UsageLimitError)
	if !ok || lbLimit.LimitID != "imagegen_premium" || lbLimit.UsedPercent != 100 || lbLimit.WindowMinutes != 1440 {
		t.Fatalf("codex-lb limit = %+v", lbErr)
	}
	if lbLimit.ResetsAt.Before(before.Add(3599*time.Second)) || lbLimit.ResetsAt.After(time.Now().Add(3601*time.Second)) {
		t.Fatalf("codex-lb reset = %v", lbLimit.ResetsAt)
	}
	// codex-lb from go-v1.7.1 also forwards the limit's own windows; they win over the
	// x-image-gen-* and x-codex-* copies.
	lb.Set("x-imagegen-premium-primary-used-percent", "100")
	lb.Set("x-imagegen-premium-primary-window-minutes", "10080")
	lb.Set("x-imagegen-premium-primary-reset-at", "1793900000")
	lb.Set("x-image-gen-primary-used-percent", "20")
	own := errorFromResponse(429, lb, []byte(`{"error":{"type":"usage_limit_reached"}}`)).(*UsageLimitError)
	if own.WindowMinutes != 10080 || !own.ResetsAt.Equal(time.Unix(1793900000, 0)) || own.Primary == nil || own.Primary.WindowMinutes != 10080 {
		t.Fatalf("own windows = %+v", own)
	}
	// Only Retry-After: it still gives the reset time.
	onlyRetry := errorFromResponse(429, http.Header{"Retry-After": {"120"}, "X-Codex-Active-Limit": {"image_gen"}}, []byte(`{"error":{"type":"usage_limit_reached"}}`))
	if limit := onlyRetry.(*UsageLimitError); limit.ResetsAt.IsZero() || limit.Primary != nil {
		t.Fatalf("Retry-After only = %+v", limit)
	}

	// codex-lb's own 429 caps are temporary.
	capped := errorFromResponse(429, http.Header{"Retry-After": {"2"}}, []byte(`{"error":{"code":"account_stream_cap","type":"rate_limit_error","message":"busy"}}`))
	if !IsRetryable(capped) {
		t.Fatalf("codex-lb cap = %v, want retryable", capped)
	}
	quota := errorFromResponse(429, nil, []byte(`{"error":{"type":"insufficient_quota","message":"no credit"}}`))
	if limit, ok := quota.(*UsageLimitError); !ok || limit.Type != "insufficientQuota" || IsRetryable(quota) {
		t.Fatalf("quota = %v", quota)
	}
}

func TestResponsesStreamErrorAfterTheImageKeepsTheImage(t *testing.T) {
	png := "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="
	stream := "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"image_generation_call\",\"status\":\"completed\",\"result\":\"" + png + "\"}}\n\n" +
		"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"server_error\",\"message\":\"the model failed\"}}}\n\n"
	result, err := readResponsesStream(strings.NewReader(stream), nil)
	if err != nil || len(result.Images) != 1 || result.StreamError == nil || !IsRetryable(result.StreamError) {
		t.Fatalf("result %+v, err %v", result, err)
	}
	// Without an image the failure is returned, so the client may retry.
	if _, err := readResponsesStream(strings.NewReader(stream[strings.Index(stream, "\n\n")+2:]), nil); !IsRetryable(err) {
		t.Fatalf("err = %v, want a temporary error", err)
	}
	// A broken connection after the image keeps it too.
	cut := "event: image_generation.completed\ndata: {\"type\":\"image_generation.completed\",\"b64_json\":\"" + png + "\"}\n\n"
	got, err := readImageStream(io.MultiReader(strings.NewReader(cut), iotest.ErrReader(errors.New("connection reset"))), nil)
	if err != nil || len(got.Images) != 1 || !IsRetryable(got.StreamError) {
		t.Fatalf("cut stream: result %+v, err %v", got, err)
	}
}
