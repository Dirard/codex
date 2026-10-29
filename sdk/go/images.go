package codex

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// Statuses of an imageGeneration item as Codex sends them. They differ from the
// command-execution statuses that share the ThreadItem.Status field; an item that only
// started may also carry an empty status.
const (
	ImageGenerationStatusInProgress = "in_progress"
	ImageGenerationStatusCompleted  = "completed"
	ImageGenerationStatusFailed     = "failed"
)

// ErrGeneratedImageUnavailable reports that a generated image has neither inline bytes nor
// a saved file to read: only an attachment-store file ID, which the app-server protocol
// cannot download.
var ErrGeneratedImageUnavailable = errors.New("codex sdk: generated image bytes are not available")

// GeneratedImage is one image the model generated in a turn: an imageGeneration item of
// the turn (specs/go-sdk-generated-images-2026-10-29.md, use case 1).
type GeneratedImage struct {
	// ItemID is the thread item ID (the tool call ID).
	ItemID string
	// Status is the status as Codex sent it: ImageGenerationStatusInProgress,
	// ImageGenerationStatusCompleted, ImageGenerationStatusFailed, or empty for an item
	// that only started.
	Status string
	// RevisedPrompt is the prompt the image was generated from.
	RevisedPrompt string
	// Data holds the decoded image bytes when the item carries them inline. It is empty
	// while the image is in progress, when it failed, when it is a file reference, and
	// when the app-server omits media from notifications.
	Data []byte
	// FileID is the attachment-store reference of an image an external store keeps.
	FileID string
	// SavedPath is the file Codex wrote on the app-server host, when it saved one.
	SavedPath string
	// TransparentBackground reports whether the image has a transparent background,
	// when Codex knows it.
	TransparentBackground protocol.Optional[bool]
	// GenerationID is the image backend's generation ID, when it sent one.
	GenerationID string
	// InputImages are the local files the image was edited from.
	InputImages []protocol.ImageGenerationInputImage
	// Failure explains a failed generation when Codex knows the reason.
	Failure *ImageGenerationFailure
}

// ImageGenerationFailure explains why an image was not generated.
type ImageGenerationFailure struct {
	// Type is the failure kind, for example "usageLimitExceeded".
	Type string
	// LimitID names the exhausted limit, for example "imagegen_premium".
	LimitID string
	// ResetsAt is when the limit resets, in UTC; zero when the server did not say.
	ResetsAt time.Time
}

// Completed reports whether the image finished successfully.
func (img GeneratedImage) Completed() bool {
	return img.Status == ImageGenerationStatusCompleted
}

// MediaType returns the media type sniffed from Data, for example "image/png", or ""
// without inline bytes.
func (img GeneratedImage) MediaType() string {
	if len(img.Data) == 0 {
		return ""
	}
	return http.DetectContentType(img.Data)
}

// GeneratedImageFromItem decodes an imageGeneration thread item, from RunResult.Items or an
// item/started or item/completed notification. It returns false without an error for any
// other item type, and an error when the inline image is not valid base64.
func GeneratedImageFromItem(item protocol.ThreadItem) (GeneratedImage, bool, error) {
	if item.TypeValue != "imageGeneration" {
		return GeneratedImage{}, false, nil
	}
	id, _ := item.ID.Value()
	status, _ := item.Status.Value()
	img := GeneratedImage{
		ItemID:                id,
		Status:                string(status),
		TransparentBackground: item.TransparentBackground,
	}
	if prompt, ok := item.RevisedPrompt.Value(); ok {
		img.RevisedPrompt = prompt
	}
	if fileID, ok := item.FileID.Value(); ok {
		img.FileID = fileID
	}
	if path, ok := item.SavedPath.Value(); ok {
		img.SavedPath = string(path)
	}
	if generationID, ok := item.GenerationID.Value(); ok {
		img.GenerationID = generationID
	}
	if inputs, ok := item.InputImages.Value(); ok && len(inputs) > 0 {
		img.InputImages = append([]protocol.ImageGenerationInputImage(nil), inputs...)
	}
	if failure, ok := item.Failure.Value(); ok {
		img.Failure = imageGenerationFailure(failure)
	}
	data, err := decodeInlineImage(item.Result)
	if err != nil {
		return GeneratedImage{}, true, &DecodeError{Reason: "imageGeneration item " + id + ": " + err.Error(), cause: err}
	}
	img.Data = data
	return img, true, nil
}

// GeneratedImages returns the images among items, in item order.
func GeneratedImages(items []protocol.ThreadItem) ([]GeneratedImage, error) {
	var images []GeneratedImage
	for _, item := range items {
		img, ok, err := GeneratedImageFromItem(item)
		if err != nil {
			return nil, err
		}
		if ok {
			images = append(images, img)
		}
	}
	return images, nil
}

// GeneratedImages returns the images the model generated in this turn, in item order.
// Each call decodes fresh copies, so changing the returned bytes does not change r.
func (r *RunResult) GeneratedImages() ([]GeneratedImage, error) {
	if r == nil {
		return nil, nil
	}
	return GeneratedImages(r.Items)
}

// ReadGeneratedImage returns the bytes of img: its inline Data when present, otherwise the
// file at SavedPath read through fs/readFile on the app-server host, otherwise
// ErrGeneratedImageUnavailable.
func (c *FileSystemClient) ReadGeneratedImage(ctx context.Context, img GeneratedImage) ([]byte, error) {
	if len(img.Data) > 0 {
		return bytes.Clone(img.Data), nil
	}
	if img.SavedPath == "" {
		return nil, ErrGeneratedImageUnavailable
	}
	response, err := c.ReadFile(ctx, protocol.FsReadFileParams{Path: protocol.AbsolutePathBuf(img.SavedPath)})
	if err != nil {
		return nil, err
	}
	data, err := base64.StdEncoding.DecodeString(response.DataBase64)
	if err != nil {
		return nil, &DecodeError{Reason: "fs/readFile " + img.SavedPath + ": invalid base64", cause: err}
	}
	return data, nil
}

func decodeInlineImage(raw json.RawMessage) ([]byte, error) {
	if len(raw) == 0 || bytes.Equal(bytes.TrimSpace(raw), []byte("null")) {
		return nil, nil
	}
	var encoded string
	if err := json.Unmarshal(raw, &encoded); err != nil {
		return nil, errors.New("result is not a JSON string")
	}
	if encoded == "" {
		return nil, nil
	}
	data, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		return nil, errors.New("result is not valid base64")
	}
	return data, nil
}

func imageGenerationFailure(failure protocol.ImageGenerationFailure) *ImageGenerationFailure {
	out := &ImageGenerationFailure{Type: failure.TypeValue}
	if limitID, ok := failure.LimitID.Value(); ok {
		out.LimitID = limitID
	}
	if resetsAt, ok := failure.ResetsAt.Value(); ok {
		out.ResetsAt = time.Unix(resetsAt, 0).UTC()
	}
	return out
}
