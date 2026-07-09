package codex

import (
	"io"
	"os"
	"strings"

	"github.com/openai/codex/sdk/go/protocol"
)

var statLocalInput = os.Stat

var openLocalInput = func(path string) (io.ReadCloser, error) {
	return os.Open(path)
}

type Input struct {
	items []inputItem
}

type inputItem struct {
	kind string
	text string
	url  string
	name string
	path string
}

type LocalInputSizeError struct {
	Path  string
	Limit int64
	Size  int64
}

func (e *LocalInputSizeError) Error() string {
	return "codex sdk local input exceeds configured byte limit"
}

func Text(text string) Input {
	return Input{items: []inputItem{{kind: "text", text: text}}}
}

func ImageURL(url string) Input {
	return Input{items: []inputItem{{kind: "image", url: url}}}
}

func DataURL(url string) Input {
	return ImageURL(url)
}

func LocalImage(path string) Input {
	return Input{items: []inputItem{{kind: "localImage", path: path}}}
}

func Skill(name string, path string) Input {
	return Input{items: []inputItem{{kind: "skill", name: name, path: path}}}
}

func Mention(name string, path string) Input {
	return Input{items: []inputItem{{kind: "mention", name: name, path: path}}}
}

func Inputs(items ...Input) Input {
	var out Input
	for _, item := range items {
		out.items = append(out.items, item.items...)
	}
	return out
}

func (i Input) wire(limits ClientLimits) ([]protocol.UserInput, error) {
	out := make([]protocol.UserInput, 0, len(i.items))
	for _, item := range i.items {
		switch item.kind {
		case "text":
			out = append(out, protocol.UserInput{TypeValue: "text", Text: protocol.SomeNonNull(item.text)})
		case "image":
			if err := validateImageURL(item.url); err != nil {
				return nil, err
			}
			out = append(out, protocol.UserInput{TypeValue: "image", URL: protocol.SomeNonNull(item.url)})
		case "localImage":
			if err := validateLocalInputSize(item.path, limits.MaxLocalInputBytes); err != nil {
				return nil, err
			}
			out = append(out, protocol.UserInput{TypeValue: "localImage", Path: protocol.SomeNonNull(item.path)})
		case "skill":
			out = append(out, protocol.UserInput{TypeValue: "skill", Name: protocol.SomeNonNull(item.name), Path: protocol.SomeNonNull(item.path)})
		case "mention":
			out = append(out, protocol.UserInput{TypeValue: "mention", Name: protocol.SomeNonNull(item.name), Path: protocol.SomeNonNull(item.path)})
		default:
			return nil, &ConfigError{Reason: "unsupported input item kind"}
		}
	}
	return out, nil
}

func validateImageURL(url string) error {
	scheme, _, ok := strings.Cut(url, ":")
	if ok && (strings.EqualFold(scheme, "http") || strings.EqualFold(scheme, "https")) {
		return &UnsupportedError{Reason: "remote image URLs are not supported; use DataURL with an inline data URL instead"}
	}
	return nil
}

func validateLocalInputSize(path string, limit int64) error {
	info, err := statLocalInput(path)
	if err == nil && info.Size() > limit {
		return &LocalInputSizeError{Path: path, Limit: limit, Size: info.Size()}
	}
	file, err := openLocalInput(path)
	if err != nil {
		return err
	}
	defer file.Close()
	reader := &io.LimitedReader{R: file, N: limit + 1}
	n, err := io.Copy(io.Discard, reader)
	if err != nil {
		return err
	}
	if n > limit {
		return &LocalInputSizeError{Path: path, Limit: limit, Size: n}
	}
	return nil
}
