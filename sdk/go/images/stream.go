package images

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
	"time"
)

// readSSE calls handle for each server-sent event of r (event name and joined data lines).
func readSSE(r io.Reader, handle func(event string, data []byte) error) error {
	reader := bufio.NewReaderSize(r, 64<<10)
	var event string
	var data bytes.Buffer
	dispatch := func() error {
		if data.Len() == 0 {
			event = ""
			return nil
		}
		err := handle(event, bytes.TrimSuffix(data.Bytes(), []byte("\n")))
		event = ""
		data.Reset()
		return err
	}
	for {
		line, err := reader.ReadString('\n')
		if len(line) > 0 {
			line = strings.TrimRight(line, "\r\n")
			switch {
			case line == "":
				if derr := dispatch(); derr != nil {
					return derr
				}
			case strings.HasPrefix(line, ":"):
			case strings.HasPrefix(line, "event:"):
				event = strings.TrimSpace(strings.TrimPrefix(line, "event:"))
			case strings.HasPrefix(line, "data:"):
				data.WriteString(strings.TrimPrefix(strings.TrimPrefix(line, "data:"), " "))
				data.WriteByte('\n')
			}
		}
		if err == io.EOF {
			return dispatch()
		}
		if err != nil {
			return &UnavailableError{Message: "read stream: " + err.Error(), cause: err}
		}
	}
}

// imageStreamEvent is an event of the images routes with stream: true.
type imageStreamEvent struct {
	Type              string           `json:"type"`
	B64JSON           string           `json:"b64_json"`
	PartialImageIndex int              `json:"partial_image_index"`
	CreatedAt         int64            `json:"created_at"`
	Size              string           `json:"size"`
	Quality           string           `json:"quality"`
	Background        string           `json:"background"`
	OutputFormat      string           `json:"output_format"`
	RevisedPrompt     string           `json:"revised_prompt"`
	GenerationID      string           `json:"generation_id"`
	Usage             *wireUsage       `json:"usage"`
	Error             *streamErrorBody `json:"error"`
}

// readImageStream reads the events of a streamed images response.
func readImageStream(r io.Reader, onPartial func(PartialImage)) (*Result, error) {
	result := &Result{}
	err := readSSE(r, func(event string, data []byte) error {
		var ev imageStreamEvent
		if err := json.Unmarshal(data, &ev); err != nil {
			return fmt.Errorf("images: decode stream event %q: %w", event, err)
		}
		kind := firstNonEmpty(ev.Type, event)
		switch {
		case strings.HasSuffix(kind, ".partial_image"):
			if onPartial != nil {
				img, err := imageFromBase64(ev.B64JSON)
				if err != nil {
					return fmt.Errorf("images: partial image %d: %w", ev.PartialImageIndex, err)
				}
				onPartial(PartialImage{Index: ev.PartialImageIndex, Data: img.Data, MediaType: img.MediaType})
			}
		case strings.HasSuffix(kind, ".completed"):
			img, err := imageFromBase64(ev.B64JSON)
			if err != nil {
				return fmt.Errorf("images: completed image: %w", err)
			}
			img.RevisedPrompt = ev.RevisedPrompt
			img.GenerationID = ev.GenerationID
			result.Images = append(result.Images, img)
			result.Quality = Quality(ev.Quality)
			result.Size = ev.Size
			result.Background = Background(ev.Background)
			result.OutputFormat = OutputFormat(ev.OutputFormat)
			if ev.CreatedAt > 0 {
				result.Created = time.Unix(ev.CreatedAt, 0).UTC()
			}
			if ev.Usage != nil {
				result.Usage = ev.Usage.usage()
			}
		case kind == "error" || ev.Error != nil:
			return streamError(ev.Error)
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	if len(result.Images) == 0 {
		return nil, &UnavailableError{Message: "the image stream ended without an image"}
	}
	return result, nil
}

// streamErrorBody is an error reported inside an event stream.
type streamErrorBody struct {
	Type    string          `json:"type"`
	Code    json.RawMessage `json:"code"`
	Message string          `json:"message"`
	Param   string          `json:"param"`
}

func streamError(e *streamErrorBody) error {
	if e == nil {
		return &UnavailableError{Message: "the server reported a stream error"}
	}
	body, _ := json.Marshal(map[string]any{"error": e})
	return errorFromResponse(statusForStreamError(e.Type, e.Code), nil, body)
}

// statusForStreamError maps an in-stream error to the HTTP status it would have had, so
// the same classification applies.
func statusForStreamError(kind string, rawCode json.RawMessage) int {
	var code string
	_ = json.Unmarshal(rawCode, &code)
	switch {
	case kind == "usage_limit_reached" || code == "usage_limit_reached" || code == "rate_limit_exceeded":
		return 429
	case refusalCodes[code] || kind == "invalid_request_error":
		return 400
	case kind == "server_error" || code == "server_error" || code == "" && kind == "":
		return 500
	}
	return 400
}

var errNoImageCall = errors.New("images: the Responses stream ended without an image_generation_call")
