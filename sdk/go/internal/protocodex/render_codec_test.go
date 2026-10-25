package protocodex

import (
	"fmt"
	"strings"
	"testing"
)

func TestValidateObjectCodecFailsClosed(t *testing.T) {
	tooMany := objectCodec{name: "Wide"}
	for i := range runtimeMaxObjectFields + 1 {
		tooMany.fields = append(tooMany.fields, renderedField{FieldName: fmt.Sprintf("F%d", i), WireName: fmt.Sprintf("f%02d", i), Type: "string", Required: true})
	}
	for _, tc := range []struct {
		name  string
		codec objectCodec
		want  string
	}{
		{"too many fields", tooMany, "supports 64"},
		{"invalid tag name", objectCodec{name: "Quoted", fields: []renderedField{{FieldName: "A", WireName: `a"b`, Type: "string"}}}, "not a valid encoding/json tag name"},
		{"dash tag name", objectCodec{name: "Dash", fields: []renderedField{{FieldName: "A", WireName: "-", Type: "string"}}}, "not a valid encoding/json tag name"},
		{"bounded nested anyOf", objectCodec{name: "AnyOf", fields: []renderedField{{FieldName: "N", WireName: "n", Type: "int64", Required: true, Minimum: "1", NestedAnyOfTags: []string{"image"}}}}, "nested anyOf"},
		{"bounded without presence", objectCodec{name: "Bounded", fields: []renderedField{{FieldName: "N", WireName: "n", Type: "int64", Maximum: "9"}}}, "neither required nor defaulted"},
		{"tagged required without field", objectCodec{
			name:        "Tagged",
			fields:      []renderedField{{FieldName: "TypeValue", WireName: "type", Type: "string", Required: true}},
			taggedUnion: renderedTaggedUnion{Discriminator: "type", Variants: []renderedTaggedUnionVariant{{Tag: "a", Required: []string{"type", "missing"}}}},
		}, `requires "missing"`},
		{"untagged required without field", objectCodec{
			name:          "Untagged",
			fields:        []renderedField{{FieldName: "A", WireName: "a", Type: "OptionalNonNull[string]"}},
			untaggedUnion: renderedUntaggedUnion{Variants: []renderedUntaggedUnionVariant{{Required: []string{"b"}}}},
		}, `requires "b"`},
	} {
		t.Run(tc.name, func(t *testing.T) {
			err := validateObjectCodec(tc.codec)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("validateObjectCodec = %v, want an error containing %q", err, tc.want)
			}
		})
	}
	ok := objectCodec{name: "Fine", fields: []renderedField{
		{FieldName: "N", WireName: "n", Type: "int64", Required: true, Minimum: "0"},
		{FieldName: "D", WireName: "d", Type: "int64", DefaultJSON: "1", Maximum: "9"},
		{FieldName: "O", WireName: "o", Type: "OptionalNonNull[int64]", Minimum: "0", NestedAnyOfTags: []string{"x"}},
		{FieldName: "S", WireName: "$schema", Type: "OptionalNonNull[string]"},
	}}
	if err := validateObjectCodec(ok); err != nil {
		t.Fatalf("validateObjectCodec(supported type) = %v", err)
	}
}

func TestRenderObjectSpecCarriesSerdeShape(t *testing.T) {
	codec := objectCodec{
		name: "FileSystemSpecialPath",
		fields: []renderedField{
			{FieldName: "FileID", WireName: "fileId", Type: "OptionalNonNull[string]", Aliases: []string{"file_id"}},
			{FieldName: "Kind", WireName: "kind", Type: "string", Required: true, RequiredNonNull: true, VariantAliases: []SerdeVariantAlias{{CanonicalWireValue: "project_roots", Aliases: []string{"current_working_directory"}}}},
			{FieldName: "Tools", WireName: "tools", Type: "Optional[[]DynamicToolSpec]", CustomDeserialize: dynamicToolSpecsDeserializeHook},
			{FieldName: "Verbose", WireName: "verbose", Type: "bool", DefaultJSON: "false", VariantTags: []string{"root"}},
			{FieldName: "Extra", WireName: "extra", Type: "map[string]json.RawMessage", Flattened: true},
		},
		taggedUnion:       renderedTaggedUnion{Discriminator: "kind", Variants: []renderedTaggedUnionVariant{{Tag: "root", Required: []string{"kind"}}}},
		stringUnionValues: []string{"never"},
	}
	rendered := renderObjectSpec(codec)
	for _, required := range []string{
		"var fileSystemSpecialPathSpec = objectSpec{",
		`{name: "fileId", flags: fieldNonNullOptional, aliases: []string{"file_id"}},`,
		`{name: "kind", flags: fieldRequired | fieldRejectNull, variantAliases: []variantAlias{{alias: "\"current_working_directory\"", canonical: "\"project_roots\""}}},`,
		`{name: "tools", flags: fieldCustom},`,
		`{name: "verbose", defaultJSON: "false", defaultTags: []string{"\"root\""}},`,
		"fieldOffset: 1,",
		"tag: 1,",
		`tags: []string{"root"},`,
		"hasAliases: true,",
	} {
		if !strings.Contains(rendered, required) {
			t.Fatalf("objectSpec missing %q\n%s", required, rendered)
		}
	}
	if strings.Contains(rendered, `"extra"`) {
		t.Fatalf("flattened field rendered as a wire field\n%s", rendered)
	}
	unmarshal := renderObjectUnmarshal(codec)
	for _, required := range []string{
		"known, err := s.begin(data, &fileSystemSpecialPathSpec)",
		"v.Kind = s.tag",
		`if raw, ok := s.value("tools"); ok {`,
		"normalizeDynamicToolSpecsJSON(raw)",
		"v.Extra = s.unknownMembers()",
		"v.StringValue = \"\"",
	} {
		if !strings.Contains(unmarshal, required) {
			t.Fatalf("UnmarshalJSON missing %q\n%s", required, unmarshal)
		}
	}
	if !codec.marshalsThroughMap() {
		t.Fatal("a flattened mixed union must keep the map marshaler")
	}
}

func TestRenderObjectMarshalNormalizesSkipRules(t *testing.T) {
	codec := objectCodec{name: "Skips", fields: []renderedField{
		{FieldName: "Content", WireName: "content", Type: "json.RawMessage", Required: true, RequiredNonNull: true},
		{FieldName: "DoubleOption", WireName: "doubleOption", Type: "Optional[string]", SkipIf: skipSerializingIfOptionNone, CustomSerialize: doubleOptionSerializeHook},
		{FieldName: "Enabled", WireName: "enabled", Type: "Optional[bool]", SkipIf: skipSerializingIfNotNot},
		{FieldName: "Flag", WireName: "flag", Type: "bool", SkipIf: skipSerializingIfNotNot},
		{FieldName: "Items", WireName: "items", Type: "OptionalNonNull[[]string]", SkipIf: skipSerializingIfVecEmpty},
		{FieldName: "Name", WireName: "name", Type: "Optional[string]", SkipIf: skipSerializingIfOptionNone},
		{FieldName: "Raw", WireName: "raw", Type: "json.RawMessage"},
		{FieldName: "Tags", WireName: "tags", Type: "[]string", SkipIf: skipSerializingIfVecEmpty},
		{FieldName: "Text", WireName: "text", Type: "OptionalNonNull[string]", SkipIf: skipSerializingIfOptionNone},
	}}
	rendered := renderObjectMarshal(codec)
	for _, required := range []string{
		`if bytes.Equal(v.Content, []byte("null")) { return nil, DecodeError{Field: "content", Reason: "cannot be null"} }`,
		"w.Enabled = optionalWithoutFalse(w.Enabled)",
		"w.Items = nonNullWithoutEmpty(w.Items)",
		"w.Name = w.Name.withoutNull()",
		"return json.Marshal(w)",
	} {
		if !strings.Contains(rendered, required) {
			t.Fatalf("MarshalJSON missing %q\n%s", required, rendered)
		}
	}
	for _, forbidden := range []string{"w.DoubleOption", "w.Text", "w.Flag", "w.Tags", "map[string]any"} {
		if strings.Contains(rendered, forbidden) {
			t.Fatalf("MarshalJSON has %q\n%s", forbidden, rendered)
		}
	}
	for field, want := range map[string]string{
		"content": "`json:\"content\"`",
		"enabled": "`json:\"enabled,omitzero\"`",
		"flag":    "`json:\"flag,omitempty\"`",
		"raw":     "`json:\"raw,omitempty\"`",
		"tags":    "`json:\"tags,omitempty\"`",
		"text":    "`json:\"text,omitzero\"`",
	} {
		for _, f := range codec.fields {
			if f.WireName == field {
				if got := wireTag(f); got != want {
					t.Errorf("wireTag(%s) = %s, want %s", field, got, want)
				}
			}
		}
	}
}

func TestLowerCamelLowersLeadingInitialisms(t *testing.T) {
	for name, want := range map[string]string{
		"ThreadItem":   "threadItem",
		"JSONRPCError": "jsonrpcError",
		"URL":          "url",
		"MCPServer":    "mcpServer",
		"V2RequestID":  "v2RequestID",
	} {
		if got := lowerCamel(name); got != want {
			t.Errorf("lowerCamel(%q) = %q, want %q", name, got, want)
		}
	}
}

func TestServerNotificationPayloadsUseDirectObjectDecode(t *testing.T) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	rendered := renderServerNotificationMetadata(manifest, map[string]bool{"ItemCompletedNotification": true})
	if !strings.Contains(rendered, "var payload ItemCompletedNotification\n\t\tif err := decodePayload(params, &payload); err != nil {") {
		t.Fatalf("item/completed payload is not decoded directly\n%s", generatedExcerpt(rendered, "ItemCompletedNotification"))
	}
	if !strings.Contains(rendered, "var payload ErrorNotification\n\t\tif err := json.Unmarshal(params, &payload); err != nil {") {
		t.Fatalf("payload outside the object set lost json.Unmarshal\n%s", generatedExcerpt(rendered, "ErrorNotification"))
	}
}
