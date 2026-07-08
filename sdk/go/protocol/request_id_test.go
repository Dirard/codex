package protocol

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestRequestIDAcceptsStringsAndIntegers(t *testing.T) {
	tests := []struct {
		name      string
		input     string
		canonical string
	}{
		{name: "string", input: `"req-1"`, canonical: `"req-1"`},
		{name: "positive integer", input: `42`, canonical: `42`},
		{name: "negative integer", input: `-7`, canonical: `-7`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var id RequestID
			if err := json.Unmarshal([]byte(tt.input), &id); err != nil {
				t.Fatal(err)
			}
			encoded, err := json.Marshal(id)
			if err != nil {
				t.Fatal(err)
			}
			if string(encoded) != tt.canonical {
				t.Fatalf("encoded = %s, want %s", encoded, tt.canonical)
			}
		})
	}
}

func TestRequestIDRejectsInvalidJSONShapes(t *testing.T) {
	tests := []string{
		`null`,
		`true`,
		`1.2`,
		`1e3`,
		`{}`,
		`[]`,
		`"a" "b"`,
	}

	for _, input := range tests {
		t.Run(input, func(t *testing.T) {
			var id RequestID
			err := json.Unmarshal([]byte(input), &id)
			if err == nil {
				t.Fatalf("expected error for %s", input)
			}
		})
	}
}

func TestRequestIDUnsetMarshalFails(t *testing.T) {
	_, err := json.Marshal(RequestID{})
	if err == nil || !strings.Contains(err.Error(), "unset request id") {
		t.Fatalf("err = %v, want unset request id error", err)
	}
}
