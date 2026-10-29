package images

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"time"
)

// responsesInstructions asks the host model to call the image tool with the prompt as is.
const responsesInstructions = "Generate the image the user describes by calling the image_generation tool once, " +
	"with the user's prompt unchanged. Do not answer with text."

// runResponses generates through a hosted image_generation tool on {BaseURL}/responses,
// delivering partial images (specs/go-sdk-images-client-2026-10-30.md, R1.6).
func (c *Client) runResponses(ctx context.Context, op operation, start time.Time, attempts *int) (*Result, error) {
	body := c.responsesBody(op)
	result, err := c.retry(ctx, attempts, func(ctx context.Context, attempt int) (*Result, error) {
		info := RequestInfo{Operation: op.name, Path: c.pathOf("/responses"), Attempt: attempt, Streaming: true}
		resp, done, err := c.send(ctx, info, "/responses", body, op.options.IdempotencyKey)
		if err != nil {
			return nil, err
		}
		defer resp.Body.Close()
		var result *Result
		if resp.StatusCode/100 != 2 {
			err = errorFromResponse(resp.StatusCode, resp.Header, readLimited(resp.Body, maxErrorBody))
		} else {
			result, err = readResponsesStream(resp.Body, op.options.OnPartialImage)
		}
		if result != nil {
			result.RequestID = requestIDFrom(resp.Header)
			result.ImagegenRequestID = resp.Header.Get("x-codex-imagegen-request-id")
			result.ViaResponses = true
		}
		done(resp, attemptError(result, err))
		return result, err
	})
	return finish(result, err, start, *attempts)
}

func (c *Client) responsesBody(op operation) map[string]any {
	content := []map[string]any{{"type": "input_text", "text": op.prompt}}
	for _, in := range op.inputs {
		ref := in.wire()
		part := map[string]any{"type": "input_image"}
		if id, ok := ref["file_id"]; ok {
			part["file_id"] = id
		} else {
			part["image_url"] = ref["image_url"]
		}
		content = append(content, part)
	}
	tool := map[string]any{"type": "image_generation", "partial_images": op.options.PartialImages}
	if op.options.Model != "" {
		tool["model"] = op.options.Model
	}
	for key, value := range map[string]string{
		"background": string(op.options.Background), "quality": string(op.options.Quality), "size": op.options.Size,
		"output_format": string(op.options.OutputFormat), "moderation": string(op.options.Moderation), "input_fidelity": string(op.fidelity),
	} {
		if value != "" {
			tool[key] = value
		}
	}
	if op.options.OutputCompression != nil {
		tool["output_compression"] = *op.options.OutputCompression
	}
	if op.mask != nil {
		tool["input_image_mask"] = op.mask.wire()
	}
	return map[string]any{
		"model":        c.cfg.ResponsesFallback.Model,
		"instructions": responsesInstructions,
		"input":        []map[string]any{{"type": "message", "role": "user", "content": content}},
		"tools":        []map[string]any{tool},
		"tool_choice":  map[string]string{"type": "image_generation"},
		"stream":       true,
		"store":        false,
	}
}

// responsesEvent is the subset of Responses stream events the image path reads.
type responsesEvent struct {
	Type              string `json:"type"`
	PartialImageB64   string `json:"partial_image_b64"`
	PartialImageIndex int    `json:"partial_image_index"`
	Item              *struct {
		Type          string `json:"type"`
		Status        string `json:"status"`
		Result        string `json:"result"`
		RevisedPrompt string `json:"revised_prompt"`
		Background    string `json:"background"`
		Quality       string `json:"quality"`
		Size          string `json:"size"`
		OutputFormat  string `json:"output_format"`
	} `json:"item"`
	Response *struct {
		Usage *wireUsage `json:"usage"`
		Error *struct {
			Code    json.RawMessage `json:"code"`
			Message string          `json:"message"`
		} `json:"error"`
	} `json:"response"`
	Code    json.RawMessage  `json:"code"`
	Message string           `json:"message"`
	Error   *streamErrorBody `json:"error"`
}

func readResponsesStream(r io.Reader, onPartial func(PartialImage)) (*Result, error) {
	result := &Result{}
	err := readSSE(r, func(event string, data []byte) error {
		var ev responsesEvent
		if err := json.Unmarshal(data, &ev); err != nil {
			return fmt.Errorf("images: decode Responses event %q: %w", event, err)
		}
		switch kind := firstNonEmpty(ev.Type, event); {
		case kind == "response.image_generation_call.partial_image":
			if onPartial != nil {
				img, err := imageFromBase64(ev.PartialImageB64)
				if err != nil {
					return fmt.Errorf("images: partial image %d: %w", ev.PartialImageIndex, err)
				}
				onPartial(PartialImage{Index: ev.PartialImageIndex, Data: img.Data, MediaType: img.MediaType})
			}
			result.PartialImages++
		case kind == "response.output_item.done" && ev.Item != nil && ev.Item.Type == "image_generation_call":
			if ev.Item.Result == "" {
				return &UnavailableError{Message: "the image_generation_call ended with status " + ev.Item.Status + " and no image"}
			}
			img, err := imageFromBase64(ev.Item.Result)
			if err != nil {
				return fmt.Errorf("images: image_generation_call: %w", err)
			}
			img.RevisedPrompt = ev.Item.RevisedPrompt
			result.Images = append(result.Images, img)
			result.Quality = Quality(ev.Item.Quality)
			result.Size = ev.Item.Size
			result.Background = Background(ev.Item.Background)
			result.OutputFormat = OutputFormat(ev.Item.OutputFormat)
		case kind == "response.completed":
			if ev.Response != nil && ev.Response.Usage != nil {
				result.Usage = ev.Response.Usage.usage()
			}
		case kind == "response.failed" || kind == "response.incomplete":
			if ev.Response != nil && ev.Response.Error != nil {
				return streamError(flatStreamError(ev.Response.Error.Code, ev.Response.Error.Message, ""))
			}
			return &UnavailableError{Message: "the Responses stream ended with " + kind}
		case kind == "error" || strings.HasSuffix(kind, ".failed"):
			if ev.Error != nil {
				return streamError(ev.Error)
			}
			return streamError(flatStreamError(ev.Code, ev.Message, ""))
		}
		return nil
	})
	if err != nil {
		return keepDeliveredImages(result, err)
	}
	if len(result.Images) == 0 {
		return nil, errNoImageCall
	}
	return result, nil
}
