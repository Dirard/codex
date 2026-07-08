package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

type RequestID struct {
	stringValue string
	intValue    int64
	kind        requestIDKind
}

type requestIDKind uint8

const (
	requestIDUnset requestIDKind = iota
	requestIDString
	requestIDInt
)

func StringRequestID(value string) RequestID {
	return RequestID{stringValue: value, kind: requestIDString}
}

func IntRequestID(value int64) RequestID {
	return RequestID{intValue: value, kind: requestIDInt}
}

func (id RequestID) MarshalJSON() ([]byte, error) {
	switch id.kind {
	case requestIDString:
		return json.Marshal(id.stringValue)
	case requestIDInt:
		return json.Marshal(id.intValue)
	default:
		return nil, fmt.Errorf("unset request id")
	}
}

func (id *RequestID) UnmarshalJSON(data []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	var value any
	if err := decoder.Decode(&value); err != nil {
		return err
	}
	if err := decoder.Decode(&struct{}{}); err != io.EOF {
		return fmt.Errorf("request id must be a single JSON string or integer")
	}
	switch value := value.(type) {
	case string:
		*id = StringRequestID(value)
		return nil
	case json.Number:
		if strings.ContainsAny(value.String(), ".eE") {
			return fmt.Errorf("request id integer must not be a floating-point number")
		}
		n, err := value.Int64()
		if err != nil {
			return fmt.Errorf("request id integer out of range: %w", err)
		}
		*id = IntRequestID(n)
		return nil
	default:
		return fmt.Errorf("request id must be string or integer")
	}
}
