// Package images generates and edits images over HTTP, without an app-server process
// (specs/go-sdk-images-client-2026-10-30.md).
//
// The same Client talks to any server with the OpenAI images routes under one base URL:
// codex-lb (`https://host/backend-api/codex` or `https://host/v1`), the ChatGPT Codex backend
// (`https://chatgpt.com/backend-api/codex`) and the OpenAI platform (`https://api.openai.com/v1`).
// What each backend honors differs; see the Go SDK README.
package images

import (
	"encoding/base64"
	"net/http"
	"strings"
	"time"
)

// DefaultModel is the image model Codex itself uses.
const DefaultModel = "gpt-image-2"

// Background of the generated image. Unknown future values pass through unchanged.
type Background string

const (
	BackgroundAuto        Background = "auto"
	BackgroundOpaque      Background = "opaque"
	BackgroundTransparent Background = "transparent"
)

// Quality of the generated image. Unknown future values pass through unchanged.
type Quality string

const (
	QualityAuto   Quality = "auto"
	QualityLow    Quality = "low"
	QualityMedium Quality = "medium"
	QualityHigh   Quality = "high"
)

// OutputFormat of the generated image file.
type OutputFormat string

const (
	OutputFormatPNG  OutputFormat = "png"
	OutputFormatJPEG OutputFormat = "jpeg"
	OutputFormatWebP OutputFormat = "webp"
)

// Moderation strictness of the image backend.
type Moderation string

const (
	ModerationAuto Moderation = "auto"
	ModerationLow  Moderation = "low"
)

// InputFidelity is how closely an edit keeps the input images.
type InputFidelity string

const (
	InputFidelityLow  InputFidelity = "low"
	InputFidelityHigh InputFidelity = "high"
)

// SizeAuto lets the backend choose the size. Other sizes are "WIDTHxHEIGHT".
const SizeAuto = "auto"

// Options are the settings shared by Generate and Edit. Zero values are left out of the
// request, so the backend applies its defaults; Model defaults to DefaultModel.
type Options struct {
	Model             string
	Background        Background
	Quality           Quality
	Size              string
	N                 int
	OutputFormat      OutputFormat
	OutputCompression *int
	Moderation        Moderation

	// PartialImages (1–3) asks for draft frames, delivered to OnPartialImage in index order
	// before the call returns. See Config.ResponsesFallback for servers that cannot stream.
	PartialImages  int
	OnPartialImage func(PartialImage)

	// IdempotencyKey is sent as the Idempotency-Key header when set.
	IdempotencyKey string
}

// GenerateRequest generates images from a prompt.
type GenerateRequest struct {
	Prompt string
	Options
}

// EditRequest edits images: Images[0] is the base image, the rest are references.
type EditRequest struct {
	Prompt        string
	Images        []InputImage
	Mask          *InputImage
	InputFidelity InputFidelity
	Options
}

// InputImage is an image sent to Edit: raw bytes, an image URL (including data URLs) or a
// file ID. Build it with ImageBytes, ImageURL or ImageFileID.
type InputImage struct {
	data   []byte
	url    string
	fileID string
}

// ImageBytes is an input image given by its file bytes (PNG, JPEG, WebP or GIF).
func ImageBytes(data []byte) InputImage { return InputImage{data: data} }

// ImageURL is an input image given by an https or data URL.
func ImageURL(url string) InputImage { return InputImage{url: url} }

// ImageFileID is an input image given by a file ID of the backend.
func ImageFileID(fileID string) InputImage { return InputImage{fileID: fileID} }

// bytes returns the image bytes when the input carries them: raw bytes or a base64 data URL.
func (in InputImage) bytes() ([]byte, bool) {
	if len(in.data) > 0 {
		return in.data, true
	}
	if rest, ok := strings.CutPrefix(in.url, "data:"); ok {
		if _, encoded, found := strings.Cut(rest, ";base64,"); found {
			data, err := base64.StdEncoding.DecodeString(encoded)
			return data, err == nil
		}
	}
	return nil, false
}

// wire returns the JSON reference of the input: {"image_url": …} or {"file_id": …}.
func (in InputImage) wire() map[string]string {
	switch {
	case in.fileID != "":
		return map[string]string{"file_id": in.fileID}
	case len(in.data) > 0:
		return map[string]string{"image_url": dataURL(in.data)}
	default:
		return map[string]string{"image_url": in.url}
	}
}

func dataURL(data []byte) string {
	return "data:" + http.DetectContentType(data) + ";base64," + base64.StdEncoding.EncodeToString(data)
}

// PartialImage is a draft frame of an image being generated.
type PartialImage struct {
	Index     int
	Data      []byte
	MediaType string
}

// Image is one generated image. The same type describes images of turn items
// (codex.GeneratedImage.Image).
type Image struct {
	Data          []byte
	MediaType     string
	Width, Height int
	RevisedPrompt string
	GenerationID  string
}

// Usage counts the tokens of a request. Through the Responses fallback it also counts the
// tokens of the host model.
type Usage struct {
	InputTokens       int64
	InputTextTokens   int64
	InputImageTokens  int64
	OutputTokens      int64
	OutputTextTokens  int64
	OutputImageTokens int64
	TotalTokens       int64
}

// Result is the answer of Generate or Edit.
type Result struct {
	Images []Image
	// Created is when the backend created the images, when it said.
	Created time.Time
	// Quality, Size, Background and OutputFormat are what the backend actually produced,
	// which may differ from the request.
	Quality      Quality
	Size         string
	Background   Background
	OutputFormat OutputFormat
	Usage        Usage
	// SafetyIdentifier is the backend's safety tracking ID, when it sent one.
	SafetyIdentifier string
	// RequestID is x-request-id; ImagegenRequestID is x-codex-imagegen-request-id.
	RequestID         string
	ImagegenRequestID string
	// Attempts counts the HTTP attempts, retries included; Duration spans all of them.
	Attempts int
	Duration time.Duration
	// ViaResponses reports that the images came through the Responses fallback.
	ViaResponses bool
}

// Image returns the first image, or the zero Image when there is none.
func (r *Result) Image() Image {
	if r == nil || len(r.Images) == 0 {
		return Image{}
	}
	return r.Images[0]
}
