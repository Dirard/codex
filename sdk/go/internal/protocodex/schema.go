package protocodex

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

type SchemaBundle struct {
	Definitions   map[string]Schema
	V2Definitions map[string]Schema
}

type Schema struct {
	BooleanSchema              *bool
	Ref                        string
	Type                       string
	Types                      []string
	Format                     string
	Title                      string
	Description                string
	Properties                 map[string]Schema
	Required                   []string
	Items                      *Schema
	AdditionalProperties       *Schema
	AllowsAdditionalProperties *bool
	AllOf                      []Schema
	AnyOf                      []Schema
	OneOf                      []Schema
	Enum                       []json.RawMessage
	Minimum                    *json.Number
	Maximum                    *json.Number
}

func LoadSchemaBundle(root string) (*SchemaBundle, error) {
	definitions, err := loadSchemaDefinitions(filepath.Join(root, "json", "codex_app_server_protocol.schemas.json"))
	if err != nil {
		return nil, err
	}
	v2Definitions, err := loadSchemaDefinitions(filepath.Join(root, "json", "codex_app_server_protocol.v2.schemas.json"))
	if err != nil {
		return nil, err
	}
	return &SchemaBundle{Definitions: definitions, V2Definitions: v2Definitions}, nil
}

func loadSchemaDefinitions(path string) (map[string]Schema, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var rawRoot struct {
		Definitions map[string]json.RawMessage `json:"definitions"`
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	if err := decoder.Decode(&rawRoot); err != nil {
		return nil, err
	}
	definitions := make(map[string]Schema)
	for name, raw := range rawRoot.Definitions {
		if nested, ok := nestedDefinitionNamespace(raw); ok {
			for nestedName, nestedRaw := range nested {
				schema, err := decodeSchema(nestedRaw)
				if err != nil {
					return nil, fmt.Errorf("decode definition %s/%s: %w", name, nestedName, err)
				}
				definitions[name+"/"+nestedName] = schema
			}
			continue
		}
		schema, err := decodeSchema(raw)
		if err != nil {
			return nil, fmt.Errorf("decode definition %s: %w", name, err)
		}
		definitions[name] = schema
	}
	return definitions, nil
}

func (b *SchemaBundle) Definition(name string) (Schema, bool) {
	if b == nil {
		return Schema{}, false
	}
	schema, ok := b.Definitions[name]
	return schema, ok
}

func (b *SchemaBundle) V2Definition(name string) (Schema, bool) {
	if b == nil {
		return Schema{}, false
	}
	schema, ok := b.V2Definitions[name]
	return schema, ok
}

func (s Schema) SingleRef() (string, bool) {
	if s.Ref != "" {
		return s.Ref, true
	}
	if len(s.AllOf) == 1 && s.AllOf[0].Ref != "" {
		return s.AllOf[0].Ref, true
	}
	return "", false
}

func (s Schema) NullableRef() (string, bool) {
	if len(s.AnyOf) != 2 {
		return "", false
	}
	var ref string
	var hasNull bool
	for _, branch := range s.AnyOf {
		if branch.Type == "null" {
			hasNull = true
			continue
		}
		if branchRef, ok := branch.SingleRef(); ok {
			ref = branchRef
		}
	}
	return ref, ref != "" && hasNull
}

func nestedDefinitionNamespace(raw json.RawMessage) (map[string]json.RawMessage, bool) {
	var obj map[string]json.RawMessage
	if err := json.Unmarshal(raw, &obj); err != nil {
		return nil, false
	}
	for _, schemaKey := range []string{"$ref", "type", "properties", "required", "items", "additionalProperties", "allOf", "anyOf", "oneOf", "enum"} {
		if _, ok := obj[schemaKey]; ok {
			return nil, false
		}
	}
	return obj, true
}

func decodeSchema(raw json.RawMessage) (Schema, error) {
	var schema Schema
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	if err := decoder.Decode(&schema); err != nil {
		return Schema{}, err
	}
	return schema, nil
}

func (s *Schema) UnmarshalJSON(data []byte) error {
	var booleanSchema bool
	if err := json.Unmarshal(data, &booleanSchema); err == nil {
		s.BooleanSchema = &booleanSchema
		return nil
	}

	var raw struct {
		Ref                  string            `json:"$ref"`
		Type                 json.RawMessage   `json:"type"`
		Format               string            `json:"format"`
		Title                string            `json:"title"`
		Description          string            `json:"description"`
		Properties           map[string]Schema `json:"properties"`
		Required             []string          `json:"required"`
		Items                *Schema           `json:"items"`
		AdditionalProperties json.RawMessage   `json:"additionalProperties"`
		AllOf                []Schema          `json:"allOf"`
		AnyOf                []Schema          `json:"anyOf"`
		OneOf                []Schema          `json:"oneOf"`
		Enum                 []json.RawMessage `json:"enum"`
		Minimum              *json.Number      `json:"minimum"`
		Maximum              *json.Number      `json:"maximum"`
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	if err := decoder.Decode(&raw); err != nil {
		return err
	}
	s.Ref = raw.Ref
	s.Format = raw.Format
	s.Title = raw.Title
	s.Description = raw.Description
	s.Properties = raw.Properties
	s.Required = raw.Required
	s.Items = raw.Items
	s.AllOf = raw.AllOf
	s.AnyOf = raw.AnyOf
	s.OneOf = raw.OneOf
	s.Enum = raw.Enum
	s.Minimum = raw.Minimum
	s.Maximum = raw.Maximum

	if len(raw.Type) > 0 {
		var single string
		if err := json.Unmarshal(raw.Type, &single); err == nil {
			s.Type = single
			s.Types = []string{single}
		} else {
			var many []string
			if err := json.Unmarshal(raw.Type, &many); err != nil {
				return fmt.Errorf("decode schema type: %w", err)
			}
			s.Types = many
			if len(many) == 1 {
				s.Type = many[0]
			}
		}
	}

	if len(raw.AdditionalProperties) > 0 {
		var allowed bool
		if err := json.Unmarshal(raw.AdditionalProperties, &allowed); err == nil {
			s.AllowsAdditionalProperties = &allowed
		} else {
			additional, err := decodeSchema(raw.AdditionalProperties)
			if err != nil {
				return fmt.Errorf("decode additionalProperties: %w", err)
			}
			s.AdditionalProperties = &additional
		}
	}
	return nil
}
