package images

import (
	"bytes"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"net/http"
	"time"
)

// wireImageResponse is the JSON of /images/generations and /images/edits.
type wireImageResponse struct {
	Created          int64       `json:"created"`
	Background       string      `json:"background"`
	Data             []wireImage `json:"data"`
	OutputFormat     string      `json:"output_format"`
	Quality          string      `json:"quality"`
	Size             string      `json:"size"`
	SafetyIdentifier string      `json:"safety_identifier"`
	Usage            *wireUsage  `json:"usage"`
}

type wireImage struct {
	B64JSON       string `json:"b64_json"`
	URL           string `json:"url"`
	RevisedPrompt string `json:"revised_prompt"`
	GenerationID  string `json:"generation_id"`
}

type wireUsage struct {
	InputTokens        int64 `json:"input_tokens"`
	OutputTokens       int64 `json:"output_tokens"`
	TotalTokens        int64 `json:"total_tokens"`
	InputTokensDetails struct {
		TextTokens  int64 `json:"text_tokens"`
		ImageTokens int64 `json:"image_tokens"`
	} `json:"input_tokens_details"`
	OutputTokensDetails struct {
		TextTokens  int64 `json:"text_tokens"`
		ImageTokens int64 `json:"image_tokens"`
	} `json:"output_tokens_details"`
}

func (u *wireUsage) usage() Usage {
	if u == nil {
		return Usage{}
	}
	total := u.TotalTokens
	if total == 0 {
		total = u.InputTokens + u.OutputTokens
	}
	return Usage{
		InputTokens: u.InputTokens, InputTextTokens: u.InputTokensDetails.TextTokens, InputImageTokens: u.InputTokensDetails.ImageTokens,
		OutputTokens: u.OutputTokens, OutputTextTokens: u.OutputTokensDetails.TextTokens, OutputImageTokens: u.OutputTokensDetails.ImageTokens,
		TotalTokens: total,
	}
}

func decodeImageResponse(r io.Reader) (*Result, error) {
	data, err := io.ReadAll(io.LimitReader(r, maxImageResponse+1))
	if err != nil {
		return nil, &UnavailableError{Message: "read response: " + err.Error(), cause: err}
	}
	if len(data) > maxImageResponse {
		return nil, errors.New("images: response is larger than 512 MiB")
	}
	var wire wireImageResponse
	if err := json.Unmarshal(data, &wire); err != nil {
		return nil, fmt.Errorf("images: decode response: %w", err)
	}
	return wire.result()
}

func (w wireImageResponse) result() (*Result, error) {
	result := &Result{
		Quality:          Quality(w.Quality),
		Size:             w.Size,
		Background:       Background(w.Background),
		OutputFormat:     OutputFormat(w.OutputFormat),
		Usage:            w.Usage.usage(),
		SafetyIdentifier: w.SafetyIdentifier,
	}
	if w.Created > 0 {
		result.Created = time.Unix(w.Created, 0).UTC()
	}
	for i, item := range w.Data {
		if item.B64JSON == "" {
			if item.URL != "" {
				return nil, fmt.Errorf("images: image %d came as a URL; this client reads b64_json only", i)
			}
			return nil, fmt.Errorf("images: image %d has no data", i)
		}
		img, err := imageFromBase64(item.B64JSON)
		if err != nil {
			return nil, fmt.Errorf("images: image %d: %w", i, err)
		}
		img.RevisedPrompt = item.RevisedPrompt
		img.GenerationID = item.GenerationID
		result.Images = append(result.Images, img)
	}
	if len(result.Images) == 0 {
		return nil, errors.New("images: response has no images")
	}
	return result, nil
}

func imageFromBase64(encoded string) (Image, error) {
	data, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		return Image{}, errors.New("invalid base64")
	}
	return imageFromBytes(data), nil
}

func imageFromBytes(data []byte) Image {
	width, height := imageSize(data)
	return Image{Data: data, MediaType: http.DetectContentType(data), Width: width, Height: height}
}

// ImageFromBytes describes image bytes: media type and, for PNG, JPEG, GIF and WebP,
// width and height.
func ImageFromBytes(data []byte) Image { return imageFromBytes(data) }

// imageSize reads the dimensions of PNG, JPEG, GIF and WebP data; 0, 0 when unknown.
func imageSize(data []byte) (int, int) {
	if config, _, err := image.DecodeConfig(bytes.NewReader(data)); err == nil {
		return config.Width, config.Height
	}
	return webpSize(data)
}

// webpSize parses the WebP header (VP8X, VP8 and VP8L chunks).
func webpSize(data []byte) (int, int) {
	if len(data) < 30 || string(data[0:4]) != "RIFF" || string(data[8:12]) != "WEBP" {
		return 0, 0
	}
	chunk := data[12:]
	switch string(chunk[0:4]) {
	case "VP8X":
		w := 1 + (int(chunk[12]) | int(chunk[13])<<8 | int(chunk[14])<<16)
		h := 1 + (int(chunk[15]) | int(chunk[16])<<8 | int(chunk[17])<<16)
		return w, h
	case "VP8 ":
		if len(chunk) < 18 {
			return 0, 0
		}
		w := int(binary.LittleEndian.Uint16(chunk[14:16]) & 0x3fff)
		h := int(binary.LittleEndian.Uint16(chunk[16:18]) & 0x3fff)
		return w, h
	case "VP8L":
		if len(chunk) < 13 || chunk[8] != 0x2f {
			return 0, 0
		}
		bits := binary.LittleEndian.Uint32(chunk[9:13])
		return int(bits&0x3fff) + 1, int((bits>>14)&0x3fff) + 1
	}
	return 0, 0
}

// validateMask rejects a mask whose size differs from the base image or that has no alpha
// channel, when both images are decodable PNG, JPEG or GIF.
func validateMask(base InputImage, mask InputImage) error {
	maskData, ok := mask.bytes()
	if !ok {
		return nil
	}
	maskConfig, maskFormat, err := image.DecodeConfig(bytes.NewReader(maskData))
	if err != nil {
		return nil
	}
	if !hasAlpha(maskData, maskFormat) {
		return errors.New("images: EditRequest.Mask must have an alpha channel (a PNG with transparency)")
	}
	baseData, ok := base.bytes()
	if !ok {
		return nil
	}
	baseWidth, baseHeight := imageSize(baseData)
	if baseWidth == 0 {
		return nil
	}
	if maskConfig.Width != baseWidth || maskConfig.Height != baseHeight {
		return fmt.Errorf("images: EditRequest.Mask is %dx%d, the base image is %dx%d", maskConfig.Width, maskConfig.Height, baseWidth, baseHeight)
	}
	return nil
}

// hasAlpha reports whether a decodable image can carry transparency: a PNG with an alpha
// color type or a tRNS chunk, or a GIF. JPEG has no alpha.
func hasAlpha(data []byte, format string) bool {
	switch format {
	case "png":
		if len(data) < 26 {
			return false
		}
		switch colorType := data[25]; colorType {
		case 4, 6: // gray+alpha, RGBA
			return true
		}
		return bytes.Contains(data, []byte("tRNS"))
	case "gif":
		return true
	}
	return false
}
