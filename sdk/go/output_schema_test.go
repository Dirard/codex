package codex

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"
)

func TestJSONSchemaSendsSchemaDirectlyInTurnStartRequest(t *testing.T) {
	schema, err := JSONSchema("answer", ObjectSchema(map[string]JSONSchemaSpec{
		"value": StringSchema(),
	}, "value"))
	if err != nil {
		t.Fatal(err)
	}
	want := json.RawMessage(`{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}`)
	got := sendOutputSchema(t, schema)
	if !jsonEqual(got, want) {
		t.Fatalf("turn/start outputSchema = %s, want %s", got, want)
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
		Type                 string `json:"type"`
		Required             []string
		AdditionalProperties *bool `json:"additionalProperties"`
		Properties           map[string]struct {
			Type string `json:"type"`
		} `json:"properties"`
	}
	if err := json.Unmarshal(schema.rawJSON(), &raw); err != nil {
		t.Fatal(err)
	}
	if raw.Type != "object" || raw.Properties["value"].Type != "string" {
		t.Fatalf("schema = %#v", raw)
	}
	if len(raw.Required) != 1 || raw.Required[0] != "value" {
		t.Fatalf("required = %#v", raw.Required)
	}
	if raw.AdditionalProperties == nil || *raw.AdditionalProperties {
		t.Fatalf("additionalProperties = %#v, want false", raw.AdditionalProperties)
	}
}

func TestRawOutputSchemaSendsExactSchemaInTurnStartRequest(t *testing.T) {
	want := json.RawMessage(`{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false}`)
	got := sendOutputSchema(t, RawOutputSchema(want))
	if !jsonEqual(got, want) {
		t.Fatalf("turn/start outputSchema = %s, want %s", got, want)
	}
}

func sendOutputSchema(t *testing.T, schema OutputSchema) json.RawMessage {
	t.Helper()
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := thread.Turn(context.Background(), Text("hello"), TurnOptions{OutputSchema: schema}); err != nil {
		t.Fatal(err)
	}

	var params struct {
		OutputSchema json.RawMessage `json:"outputSchema"`
	}
	if err := json.Unmarshal(requestParamsForMethod(t, transport, "turn/start"), &params); err != nil {
		t.Fatal(err)
	}
	return params.OutputSchema
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
