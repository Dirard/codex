package codex

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestOutputSchemaMapsExactlyToTurnStartParams(t *testing.T) {
	schema, err := JSONSchema("answer", ObjectSchema(map[string]JSONSchemaSpec{
		"value": StringSchema(),
	}, "value"))
	if err != nil {
		t.Fatal(err)
	}
	params := turnStartParams("thread-1", []protocol.UserInput{}, TurnOptions{OutputSchema: schema})
	if !jsonEqual(params.OutputSchema, schema.rawJSON()) {
		t.Fatalf("OutputSchema = %s, want %s", params.OutputSchema, schema.rawJSON())
	}
}

func TestObjectSchemaBuildsTypedJSONSchema(t *testing.T) {
	schema, err := JSONSchema("answer", ObjectSchema(map[string]JSONSchemaSpec{
		"value": StringSchema(),
	}, "value"))
	if err != nil {
		t.Fatal(err)
	}

	var raw struct {
		Name   string `json:"name"`
		Strict bool   `json:"strict"`
		Schema struct {
			Type       string `json:"type"`
			Required   []string
			Properties map[string]struct {
				Type string `json:"type"`
			} `json:"properties"`
		} `json:"schema"`
	}
	if err := json.Unmarshal(schema.rawJSON(), &raw); err != nil {
		t.Fatal(err)
	}
	if raw.Name != "answer" || !raw.Strict || raw.Schema.Type != "object" || raw.Schema.Properties["value"].Type != "string" {
		t.Fatalf("schema = %#v", raw)
	}
	if len(raw.Schema.Required) != 1 || raw.Schema.Required[0] != "value" {
		t.Fatalf("required = %#v", raw.Schema.Required)
	}
}

func jsonEqual(left json.RawMessage, right json.RawMessage) bool {
	var leftValue any
	var rightValue any
	if err := json.Unmarshal(left, &leftValue); err != nil {
		return false
	}
	if err := json.Unmarshal(right, &rightValue); err != nil {
		return false
	}
	leftData, _ := json.Marshal(leftValue)
	rightData, _ := json.Marshal(rightValue)
	return bytes.Equal(leftData, rightData)
}
