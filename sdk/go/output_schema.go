package codex

import "encoding/json"

type JSONSchemaSpec struct {
	Type                 string                    `json:"type,omitempty"`
	Description          string                    `json:"description,omitempty"`
	Properties           map[string]JSONSchemaSpec `json:"properties,omitempty"`
	Required             []string                  `json:"required,omitempty"`
	Items                *JSONSchemaSpec           `json:"items,omitempty"`
	Enum                 []string                  `json:"enum,omitempty"`
	AdditionalProperties *bool                     `json:"additionalProperties,omitempty"`
}

type OutputSchema struct {
	raw json.RawMessage
}

func JSONSchema(name string, schema JSONSchemaSpec) (OutputSchema, error) {
	payload := map[string]any{
		"name":   name,
		"schema": schema,
		"strict": true,
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return OutputSchema{}, err
	}
	return OutputSchema{raw: data}, nil
}

func ObjectSchema(properties map[string]JSONSchemaSpec, required ...string) JSONSchemaSpec {
	return JSONSchemaSpec{
		Type:       "object",
		Properties: cloneJSONSchemaProperties(properties),
		Required:   append([]string(nil), required...),
	}
}

func StringSchema() JSONSchemaSpec {
	return JSONSchemaSpec{Type: "string"}
}

func NumberSchema() JSONSchemaSpec {
	return JSONSchemaSpec{Type: "number"}
}

func BooleanSchema() JSONSchemaSpec {
	return JSONSchemaSpec{Type: "boolean"}
}

func ArraySchema(items JSONSchemaSpec) JSONSchemaSpec {
	itemCopy := items
	return JSONSchemaSpec{Type: "array", Items: &itemCopy}
}

func RawOutputSchema(data json.RawMessage) OutputSchema {
	return OutputSchema{raw: append(json.RawMessage(nil), data...)}
}

func (s OutputSchema) rawJSON() json.RawMessage {
	return append(json.RawMessage(nil), s.raw...)
}

func cloneJSONSchemaProperties(properties map[string]JSONSchemaSpec) map[string]JSONSchemaSpec {
	if len(properties) == 0 {
		return nil
	}
	out := make(map[string]JSONSchemaSpec, len(properties))
	for name, schema := range properties {
		out[name] = schema
	}
	return out
}
