package protocol

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestOptionalNullableJSON(t *testing.T) {
	value := Some("value")
	encoded, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != `"value"` {
		t.Fatalf("encoded = %s, want string value", encoded)
	}

	encoded, err = json.Marshal(Null[string]())
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != "null" {
		t.Fatalf("encoded = %s, want null", encoded)
	}

	var decoded Optional[string]
	if err := json.Unmarshal([]byte(`"decoded"`), &decoded); err != nil {
		t.Fatal(err)
	}
	if got, ok := decoded.Value(); !ok || got != "decoded" {
		t.Fatalf("decoded Value() = %q, %v; want decoded, true", got, ok)
	}

	if err := json.Unmarshal([]byte(`null`), &decoded); err != nil {
		t.Fatal(err)
	}
	if !decoded.IsSet() || !decoded.IsNull() {
		t.Fatalf("decoded null IsSet=%v IsNull=%v, want true true", decoded.IsSet(), decoded.IsNull())
	}
}

func TestOptionalNullableUnsetMarshalFails(t *testing.T) {
	_, err := json.Marshal(Optional[string]{})
	if err == nil || !strings.Contains(err.Error(), "unset Optional") {
		t.Fatalf("err = %v, want unset Optional error", err)
	}
}

func TestOptionalNonNullJSON(t *testing.T) {
	encoded, err := json.Marshal(SomeNonNull(42))
	if err != nil {
		t.Fatal(err)
	}
	if string(encoded) != "42" {
		t.Fatalf("encoded = %s, want 42", encoded)
	}

	var decoded OptionalNonNull[int]
	if err := json.Unmarshal([]byte(`42`), &decoded); err != nil {
		t.Fatal(err)
	}
	if got, ok := decoded.Value(); !ok || got != 42 {
		t.Fatalf("decoded Value() = %d, %v; want 42, true", got, ok)
	}
}

func TestOptionalNonNullUnsetAndNullFail(t *testing.T) {
	_, err := json.Marshal(OptionalNonNull[int]{})
	if err == nil || !strings.Contains(err.Error(), "unset OptionalNonNull") {
		t.Fatalf("err = %v, want unset OptionalNonNull error", err)
	}

	var decoded OptionalNonNull[int]
	err = json.Unmarshal([]byte(`null`), &decoded)
	if err == nil || !strings.Contains(err.Error(), "cannot be null") {
		t.Fatalf("err = %v, want null rejection", err)
	}
}
