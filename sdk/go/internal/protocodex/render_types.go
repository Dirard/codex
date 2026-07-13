package protocodex

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

func renderTypes(bundle *SchemaBundle, manifest *Manifest) (string, error) {
	var b strings.Builder
	b.WriteString("package protocol\n\n")
	b.WriteString("import (\n\t\"bytes\"\n\t\"encoding/json\"\n\t\"fmt\"\n)\n\n")
	b.WriteString("type DecodeError struct { Field string; Reason string }\n\n")
	b.WriteString("func (e DecodeError) Error() string { return fmt.Sprintf(\"field %s: %s\", e.Field, e.Reason) }\n\n")
	keys := sortedDefinitionKeys(bundle)
	names := definitionNameMap(keys)
	serdeShapes := mapSerdeShapes(manifest.Experimental.SerdeShapes)
	written := map[string]bool{"ClientNotification": true, "RequestID": true}
	skipped := publicDefinitionSkipSet(manifest)
	for _, key := range keys {
		name := names[key]
		if skipped[name] || skipped[key] {
			continue
		}
		if written[name] {
			continue
		}
		written[name] = true
		schema, _ := bundle.Definition(key)
		b.WriteString(renderDefinitionType(name, key, schema, names, serdeShapes, manifest, bundle))
		b.WriteByte('\n')
	}
	return b.String(), nil
}

func publicDefinitionSkipSet(manifest *Manifest) map[string]bool {
	skipped := map[string]bool{}
	add := func(typeName string) {
		if typeName == "" || typeName == "Option<()>" {
			return
		}
		skipped[typeName] = true
		skipped[typeNameForDefinition(typeName)] = true
	}
	for _, entry := range manifest.Experimental.ClientRequests {
		if !hasPublicGeneratedProtocolSurface(entry.SDKVisibility) {
			add(entry.ParamsType)
			add(entry.ResponseType)
		}
	}
	for _, entry := range manifest.Experimental.ServerRequests {
		if !hasPublicGeneratedProtocolSurface(entry.SDKVisibility) {
			add(entry.PayloadType)
			add(entry.ResponseType)
		}
	}
	for _, entry := range manifest.Experimental.ServerNotifications {
		if !hasPublicGeneratedProtocolSurface(entry.SDKVisibility) {
			add(entry.PayloadType)
		}
	}
	return skipped
}

func hasPublicGeneratedProtocolSurface(visibility string) bool {
	switch visibility {
	case "compatibilityOnly", "excluded", "internalTestOnly":
		return false
	default:
		return true
	}
}

func sortedDefinitionKeys(bundle *SchemaBundle) []string {
	keys := make([]string, 0, len(bundle.Definitions))
	for key := range bundle.Definitions {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func definitionNameMap(keys []string) map[string]string {
	names := make(map[string]string, len(keys))
	used := map[string]string{"RequestID": "handwritten"}
	for _, key := range keys {
		name := typeNameForDefinition(key)
		if prior, ok := used[name]; ok && prior != key {
			name = namespaceTypeName(key)
		}
		used[name] = key
		names[key] = name
	}
	return names
}

func renderDefinitionType(name, key string, schema Schema, names map[string]string, serdeShapes map[string]SerdeShape, manifest *Manifest, bundle *SchemaBundle) string {
	if name == "JSONRPCMessage" {
		return renderJSONRPCMessageUnion()
	}
	if isJSONRPCUnionName(name) {
		return renderJSONRPCUnion(name, manifest)
	}
	if refs, ok := refUnionRefs(schema); ok {
		if rendered, ok := renderRefUnion(name, refs, names, bundle); ok {
			return rendered
		}
	}
	serdeShape := serdeShapeForType(name, key, serdeShapes)
	if enumValues, ok := stringEnumValues(schema); ok {
		return renderStringEnum(name, enumValues, serdeShape)
	}
	if name == "MultiAgentMode" {
		if rendered, ok := renderMultiAgentMode(schema); ok {
			return rendered
		}
	}
	if mapType, ok := namedObjectMapGoType(schema, names); ok {
		return renderNamedMapType(name, mapType)
	}
	if shouldRenderStruct(schema) {
		return renderStruct(name, key, schema, names, serdeShapes, bundle)
	}
	target := goTypeForSchema(schema, names, nil, true)
	if target == "" || target == "json.RawMessage" {
		return fmt.Sprintf("type %s json.RawMessage\n", name)
	}
	return fmt.Sprintf("type %s %s\n", name, target)
}

func isJSONRPCUnionName(name string) bool {
	return name == "ClientRequest" || name == "ServerRequest" || name == "ServerNotification"
}

func renderJSONRPCUnion(name string, manifest *Manifest) string {
	var b strings.Builder
	hasID := name == "ClientRequest" || name == "ServerRequest"
	b.WriteString(fmt.Sprintf("type %s struct {\n", name))
	if hasID {
		b.WriteString("\tID RequestID `json:\"id\"`\n")
	}
	b.WriteString("\tMethod string `json:\"method\"`\n")
	b.WriteString("\tParams json.RawMessage `json:\"params,omitempty\"`\n")
	b.WriteString("}\n\n")
	b.WriteString(fmt.Sprintf("func (v %s) MarshalJSON() ([]byte, error) {\n", name))
	b.WriteString("\ttype wire struct {\n")
	if hasID {
		b.WriteString("\t\tID RequestID `json:\"id\"`\n")
	}
	b.WriteString("\t\tMethod string `json:\"method\"`\n\t\tParams json.RawMessage `json:\"params,omitempty\"`\n\t}\n")
	if hasID {
		b.WriteString("\treturn json.Marshal(wire{ID: v.ID, Method: v.Method, Params: v.Params})\n")
	} else {
		b.WriteString("\treturn json.Marshal(wire{Method: v.Method, Params: v.Params})\n")
	}
	b.WriteString("}\n\n")
	b.WriteString(fmt.Sprintf("func (v *%s) UnmarshalJSON(data []byte) error {\n", name))
	b.WriteString("\tvar raw map[string]json.RawMessage\n")
	b.WriteString("\tif err := json.Unmarshal(data, &raw); err != nil { return err }\n")
	if hasID {
		b.WriteString("\trawID, ok := raw[\"id\"]\n\tif !ok { return DecodeError{Field: \"id\", Reason: \"missing required field\"} }\n\tif err := json.Unmarshal(rawID, &v.ID); err != nil { return fmt.Errorf(\"field id: %w\", err) }\n")
	}
	b.WriteString("\trawMethod, ok := raw[\"method\"]\n\tif !ok { return DecodeError{Field: \"method\", Reason: \"missing required field\"} }\n\tif bytes.Equal(rawMethod, []byte(\"null\")) { return DecodeError{Field: \"method\", Reason: \"cannot be null\"} }\n\tif err := json.Unmarshal(rawMethod, &v.Method); err != nil { return fmt.Errorf(\"field method: %w\", err) }\n")
	b.WriteString("\tv.Params = nil\n\tif rawParams, ok := raw[\"params\"]; ok { v.Params = append(v.Params[:0], rawParams...) }\n\treturn nil\n}\n\n")
	for _, variant := range jsonRPCUnionVariants(name, manifest) {
		if variant.ParamsType == "" || variant.ParamsType == "Option<()>" {
			continue
		}
		goType := typeNameForDefinition(variant.ParamsType)
		methodName := RawMethodName(variant.Method) + "Params"
		b.WriteString(fmt.Sprintf("func (v %s) %s() (%s, bool, error) {\n", name, methodName, goType))
		b.WriteString(fmt.Sprintf("\tif v.Method != %q { return %s{}, false, nil }\n", variant.Method, goType))
		b.WriteString(fmt.Sprintf("\tvar params %s\n", goType))
		b.WriteString("\tif len(bytes.TrimSpace(v.Params)) == 0 { return params, true, DecodeError{Field: \"params\", Reason: \"missing required field\"} }\n")
		b.WriteString("\tif err := json.Unmarshal(v.Params, &params); err != nil { return params, true, err }\n")
		b.WriteString("\treturn params, true, nil\n}\n\n")
	}
	return b.String()
}

func renderJSONRPCMessageUnion() string {
	var b strings.Builder
	b.WriteString("type JSONRPCMessage struct {\n")
	b.WriteString("\tRequest *JSONRPCRequest\n")
	b.WriteString("\tNotification *JSONRPCNotification\n")
	b.WriteString("\tResponse *JSONRPCResponse\n")
	b.WriteString("\tError *JSONRPCError\n")
	b.WriteString("\tRawJSON json.RawMessage\n")
	b.WriteString("}\n\n")
	for _, variant := range []struct {
		field string
		name  string
	}{
		{field: "Request", name: "JSONRPCRequest"},
		{field: "Notification", name: "JSONRPCNotification"},
		{field: "Response", name: "JSONRPCResponse"},
		{field: "Error", name: "JSONRPCError"},
	} {
		b.WriteString(fmt.Sprintf("func New%sMessage(value %s) JSONRPCMessage {\n", variant.name, variant.name))
		b.WriteString(fmt.Sprintf("\treturn JSONRPCMessage{%s: &value}\n", variant.field))
		b.WriteString("}\n\n")
		b.WriteString(fmt.Sprintf("func (v JSONRPCMessage) %s() (%s, bool) {\n", variant.name, variant.name))
		b.WriteString(fmt.Sprintf("\tif v.%s == nil { return %s{}, false }\n", variant.field, variant.name))
		b.WriteString(fmt.Sprintf("\treturn *v.%s, true\n", variant.field))
		b.WriteString("}\n\n")
	}
	b.WriteString("func (v JSONRPCMessage) MarshalJSON() ([]byte, error) {\n")
	b.WriteString("\tswitch {\n")
	b.WriteString("\tcase v.Request != nil:\n\t\treturn json.Marshal(v.Request)\n")
	b.WriteString("\tcase v.Notification != nil:\n\t\treturn json.Marshal(v.Notification)\n")
	b.WriteString("\tcase v.Response != nil:\n\t\treturn json.Marshal(v.Response)\n")
	b.WriteString("\tcase v.Error != nil:\n\t\treturn json.Marshal(v.Error)\n")
	b.WriteString("\tcase len(bytes.TrimSpace(v.RawJSON)) > 0:\n")
	b.WriteString("\t\tif !json.Valid(v.RawJSON) { return nil, fmt.Errorf(\"invalid JSON-RPC message raw fallback\") }\n")
	b.WriteString("\t\treturn append([]byte(nil), v.RawJSON...), nil\n")
	b.WriteString("\tdefault:\n\t\treturn nil, DecodeError{Field: \"\", Reason: \"empty JSON-RPC message\"}\n\t}\n")
	b.WriteString("}\n\n")
	b.WriteString("func (v *JSONRPCMessage) UnmarshalJSON(data []byte) error {\n")
	b.WriteString("\ttrimmed := bytes.TrimSpace(data)\n")
	b.WriteString("\tif bytes.Equal(trimmed, []byte(\"null\")) { return DecodeError{Field: \"\", Reason: \"cannot be null\"} }\n")
	b.WriteString("\tvar raw map[string]json.RawMessage\n")
	b.WriteString("\tif err := json.Unmarshal(data, &raw); err != nil { return err }\n")
	b.WriteString("\tif _, ok := raw[\"error\"]; ok {\n")
	b.WriteString("\t\tvar value JSONRPCError\n")
	b.WriteString("\t\tif err := json.Unmarshal(data, &value); err != nil { return err }\n")
	b.WriteString("\t\t*v = JSONRPCMessage{Error: &value}\n")
	b.WriteString("\t\treturn nil\n\t}\n")
	b.WriteString("\tif _, ok := raw[\"result\"]; ok {\n")
	b.WriteString("\t\tvar value JSONRPCResponse\n")
	b.WriteString("\t\tif err := json.Unmarshal(data, &value); err != nil { return err }\n")
	b.WriteString("\t\t*v = JSONRPCMessage{Response: &value}\n")
	b.WriteString("\t\treturn nil\n\t}\n")
	b.WriteString("\tif _, hasID := raw[\"id\"]; hasID {\n")
	b.WriteString("\t\tif _, hasMethod := raw[\"method\"]; hasMethod {\n")
	b.WriteString("\t\t\tvar value JSONRPCRequest\n")
	b.WriteString("\t\t\tif err := json.Unmarshal(data, &value); err != nil { return err }\n")
	b.WriteString("\t\t\t*v = JSONRPCMessage{Request: &value}\n")
	b.WriteString("\t\t\treturn nil\n\t\t}\n\t}\n")
	b.WriteString("\tif _, ok := raw[\"method\"]; ok {\n")
	b.WriteString("\t\tvar value JSONRPCNotification\n")
	b.WriteString("\t\tif err := json.Unmarshal(data, &value); err != nil { return err }\n")
	b.WriteString("\t\t*v = JSONRPCMessage{Notification: &value}\n")
	b.WriteString("\t\treturn nil\n\t}\n")
	b.WriteString("\t*v = JSONRPCMessage{RawJSON: append([]byte(nil), trimmed...)}\n")
	b.WriteString("\treturn nil\n")
	b.WriteString("}\n")
	return b.String()
}

type refUnionVariant struct {
	FieldName     string
	TypeName      string
	Discriminator string
	Values        []string
}

func refUnionRefs(schema Schema) ([]string, bool) {
	branches := schema.OneOf
	if len(branches) == 0 {
		branches = schema.AnyOf
	}
	if len(branches) == 0 {
		return nil, false
	}
	refs := make([]string, 0, len(branches))
	for _, branch := range branches {
		ref, ok := branch.SingleRef()
		if !ok {
			return nil, false
		}
		refs = append(refs, ref)
	}
	return refs, true
}

func renderRefUnion(name string, refs []string, names map[string]string, bundle *SchemaBundle) (string, bool) {
	variants := make([]refUnionVariant, 0, len(refs))
	for _, ref := range refs {
		typeName := typeNameForRef(ref, names)
		discriminator, values := refUnionDiscriminator(ref, bundle)
		variants = append(variants, refUnionVariant{
			FieldName:     unexportedGoFieldName(typeName),
			TypeName:      typeName,
			Discriminator: discriminator,
			Values:        values,
		})
	}

	var b strings.Builder
	b.WriteString(fmt.Sprintf("type %s struct {\n", name))
	for _, variant := range variants {
		b.WriteString(fmt.Sprintf("\t%s *%s\n", variant.FieldName, variant.TypeName))
	}
	b.WriteString("\tRawJSON json.RawMessage\n")
	b.WriteString("}\n\n")
	for _, variant := range variants {
		b.WriteString(fmt.Sprintf("func New%s%s(value %s) %s {\n", name, variant.TypeName, variant.TypeName, name))
		b.WriteString(fmt.Sprintf("\treturn %s{%s: &value}\n", name, variant.FieldName))
		b.WriteString("}\n\n")
		b.WriteString(fmt.Sprintf("func (v %s) %s() (%s, bool) {\n", name, variant.TypeName, variant.TypeName))
		b.WriteString(fmt.Sprintf("\tif v.%s == nil { return %s{}, false }\n", variant.FieldName, variant.TypeName))
		b.WriteString(fmt.Sprintf("\treturn *v.%s, true\n", variant.FieldName))
		b.WriteString("}\n\n")
	}
	b.WriteString(fmt.Sprintf("func (v %s) MarshalJSON() ([]byte, error) {\n", name))
	b.WriteString("\tswitch {\n")
	for _, variant := range variants {
		b.WriteString(fmt.Sprintf("\tcase v.%s != nil:\n\t\treturn json.Marshal(v.%s)\n", variant.FieldName, variant.FieldName))
	}
	b.WriteString("\tcase len(bytes.TrimSpace(v.RawJSON)) > 0:\n")
	b.WriteString(fmt.Sprintf("\t\tif !json.Valid(v.RawJSON) { return nil, fmt.Errorf(\"invalid %s raw fallback\") }\n", name))
	b.WriteString("\t\treturn append([]byte(nil), v.RawJSON...), nil\n")
	b.WriteString(fmt.Sprintf("\tdefault:\n\t\treturn nil, DecodeError{Field: \"\", Reason: \"empty %s\"}\n\t}\n", name))
	b.WriteString("}\n\n")
	b.WriteString(fmt.Sprintf("func (v *%s) UnmarshalJSON(data []byte) error {\n", name))
	b.WriteString("\ttrimmed := bytes.TrimSpace(data)\n")
	b.WriteString("\tif bytes.Equal(trimmed, []byte(\"null\")) { return DecodeError{Field: \"\", Reason: \"cannot be null\"} }\n")
	b.WriteString("\tvar raw map[string]json.RawMessage\n")
	b.WriteString("\tif err := json.Unmarshal(data, &raw); err != nil { return err }\n")
	for _, discriminator := range refUnionDiscriminators(variants) {
		b.WriteString(fmt.Sprintf("\tif rawDiscriminator, ok := raw[%q]; ok {\n", discriminator))
		b.WriteString("\t\tvar discriminator string\n")
		b.WriteString(fmt.Sprintf("\t\tif err := json.Unmarshal(rawDiscriminator, &discriminator); err != nil { return fmt.Errorf(\"field %s: %%w\", err) }\n", discriminator))
		b.WriteString("\t\tmatchedDiscriminator := false\n")
		for _, variant := range variants {
			if variant.Discriminator != discriminator || len(variant.Values) == 0 {
				continue
			}
			b.WriteString(fmt.Sprintf("\t\tif %s {\n", discriminatorMatchExpression("discriminator", variant.Values)))
			b.WriteString("\t\t\tmatchedDiscriminator = true\n")
			b.WriteString(fmt.Sprintf("\t\t\tvar value %s\n", variant.TypeName))
			b.WriteString("\t\t\tif err := json.Unmarshal(data, &value); err == nil {\n")
			b.WriteString(fmt.Sprintf("\t\t\t\t*v = %s{%s: &value}\n", name, variant.FieldName))
			b.WriteString("\t\t\t\treturn nil\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
		}
		b.WriteString(fmt.Sprintf("\t\tif matchedDiscriminator { return DecodeError{Field: %q, Reason: \"no matching union variant\"} }\n", discriminator))
		b.WriteString("\t}\n")
	}
	for _, variant := range variants {
		if variant.Discriminator != "" {
			continue
		}
		b.WriteString(fmt.Sprintf("\t{\n\t\tvar value %s\n", variant.TypeName))
		b.WriteString("\t\tif err := json.Unmarshal(data, &value); err == nil {\n")
		b.WriteString(fmt.Sprintf("\t\t\t*v = %s{%s: &value}\n", name, variant.FieldName))
		b.WriteString("\t\t\treturn nil\n\t\t}\n\t}\n")
	}
	b.WriteString(fmt.Sprintf("\t*v = %s{RawJSON: append([]byte(nil), trimmed...)}\n", name))
	b.WriteString("\treturn nil\n")
	b.WriteString("}\n")
	return b.String(), true
}

func refUnionDiscriminator(ref string, bundle *SchemaBundle) (string, []string) {
	schema, ok := bundle.Definition(definitionNameForRef(ref))
	if !ok {
		return "", nil
	}
	for _, discriminator := range []string{"type", "kind"} {
		property, ok := schema.Properties[discriminator]
		if !ok {
			continue
		}
		values := stringEnumSchemaValues(property, bundle)
		if len(values) > 0 {
			return discriminator, values
		}
	}
	if refs, ok := refUnionRefs(schema); ok {
		valuesByDiscriminator := map[string][]string{}
		for _, nestedRef := range refs {
			discriminator, values := refUnionDiscriminator(nestedRef, bundle)
			if discriminator == "" {
				continue
			}
			valuesByDiscriminator[discriminator] = append(valuesByDiscriminator[discriminator], values...)
		}
		for _, discriminator := range []string{"type", "kind"} {
			if values := dedupeStrings(valuesByDiscriminator[discriminator]); len(values) > 0 {
				return discriminator, values
			}
		}
	}
	return "", nil
}

func stringEnumSchemaValues(schema Schema, bundle *SchemaBundle) []string {
	if ref, ok := schema.SingleRef(); ok {
		if refSchema, found := bundle.Definition(definitionNameForRef(ref)); found {
			return stringEnumSchemaValues(refSchema, bundle)
		}
	}
	values, ok := stringEnumValues(schema)
	if !ok {
		return nil
	}
	return values
}

func refUnionDiscriminators(variants []refUnionVariant) []string {
	seen := map[string]bool{}
	var out []string
	for _, variant := range variants {
		if variant.Discriminator == "" || seen[variant.Discriminator] {
			continue
		}
		seen[variant.Discriminator] = true
		out = append(out, variant.Discriminator)
	}
	sort.Strings(out)
	return out
}

func discriminatorMatchExpression(variable string, values []string) string {
	var checks []string
	for _, value := range dedupeStrings(values) {
		checks = append(checks, fmt.Sprintf("%s == %q", variable, value))
	}
	if len(checks) == 0 {
		return "false"
	}
	return strings.Join(checks, " || ")
}

func definitionNameForRef(ref string) string {
	return strings.TrimPrefix(ref, "#/definitions/")
}

func unexportedGoFieldName(name string) string {
	if name == "" {
		return ""
	}
	return strings.ToLower(name[:1]) + name[1:]
}

type jsonRPCUnionVariant struct {
	Method     string
	ParamsType string
}

func jsonRPCUnionVariants(name string, manifest *Manifest) []jsonRPCUnionVariant {
	switch name {
	case "ClientRequest":
		var variants []jsonRPCUnionVariant
		for _, entry := range manifest.Experimental.ClientRequests {
			if !hasPublicGeneratedProtocolSurface(entry.SDKVisibility) {
				continue
			}
			variants = append(variants, jsonRPCUnionVariant{Method: entry.Method, ParamsType: entry.ParamsType})
		}
		return variants
	case "ServerRequest":
		var variants []jsonRPCUnionVariant
		for _, entry := range manifest.Experimental.ServerRequests {
			if !hasPublicGeneratedProtocolSurface(entry.SDKVisibility) {
				continue
			}
			variants = append(variants, jsonRPCUnionVariant{Method: entry.Method, ParamsType: entry.PayloadType})
		}
		return variants
	case "ServerNotification":
		var variants []jsonRPCUnionVariant
		for _, entry := range manifest.Experimental.ServerNotifications {
			if !hasPublicGeneratedProtocolSurface(entry.SDKVisibility) {
				continue
			}
			variants = append(variants, jsonRPCUnionVariant{Method: entry.Method, ParamsType: entry.PayloadType})
		}
		return variants
	default:
		return nil
	}
}

func shouldRenderStruct(schema Schema) bool {
	return schema.Type == "object" || len(schema.Properties) > 0 || oneOfObjectUnion(schema)
}

func namedObjectMapGoType(schema Schema, names map[string]string) (string, bool) {
	if schema.Type != "object" || len(schema.Properties) > 0 || len(schema.AllOf) > 0 || len(schema.AnyOf) > 0 || len(schema.OneOf) > 0 {
		return "", false
	}
	if schema.AllowsAdditionalProperties != nil && !*schema.AllowsAdditionalProperties {
		return "", false
	}
	if schema.AdditionalProperties != nil {
		return "map[string]" + goTypeForSchema(*schema.AdditionalProperties, names, nil, true), true
	}
	return "map[string]json.RawMessage", true
}

func renderNamedMapType(name, mapType string) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("type %s %s\n\n", name, mapType))
	b.WriteString(fmt.Sprintf("func (v %s) MarshalJSON() ([]byte, error) {\n", name))
	b.WriteString("\tif v == nil { return []byte(\"{}\"), nil }\n")
	b.WriteString(fmt.Sprintf("\treturn json.Marshal(%s(v))\n}\n\n", mapType))
	b.WriteString(fmt.Sprintf("func (v *%s) UnmarshalJSON(data []byte) error {\n", name))
	b.WriteString("\tif bytes.Equal(bytes.TrimSpace(data), []byte(\"null\")) { return DecodeError{Field: \"\", Reason: \"cannot be null\"} }\n")
	b.WriteString(fmt.Sprintf("\tvar raw %s\n", mapType))
	b.WriteString("\tif err := json.Unmarshal(data, &raw); err != nil { return err }\n")
	b.WriteString(fmt.Sprintf("\t*v = %s(raw)\n\treturn nil\n}\n", name))
	return b.String()
}

func renderStringEnum(name string, values []string, serdeShape SerdeShape) string {
	aliases := enumVariantAliasMap(serdeShape)
	var b strings.Builder
	b.WriteString(fmt.Sprintf("type %s string\n\nconst (\n", name))
	for _, value := range values {
		if _, isAlias := aliases[value]; isAlias {
			continue
		}
		b.WriteString(fmt.Sprintf("\t%s %s = %q\n", EnumConstName(name, value), name, value))
	}
	b.WriteString(")\n")
	if len(aliases) == 0 {
		return b.String()
	}
	b.WriteString(fmt.Sprintf("\nfunc (v *%s) UnmarshalJSON(data []byte) error {\n", name))
	b.WriteString("\tvar value string\n")
	b.WriteString("\tif err := json.Unmarshal(data, &value); err != nil { return err }\n")
	b.WriteString("\tswitch value {\n")
	for _, alias := range sortedAliasValues(aliases) {
		b.WriteString(fmt.Sprintf("\tcase %q:\n\t\t*v = %s\n", alias, EnumConstName(name, aliases[alias])))
	}
	b.WriteString(fmt.Sprintf("\tdefault:\n\t\t*v = %s(value)\n\t}\n\treturn nil\n}\n", name))
	b.WriteString(fmt.Sprintf("\nfunc (v %s) MarshalJSON() ([]byte, error) {\n", name))
	b.WriteString("\tswitch string(v) {\n")
	for _, alias := range sortedAliasValues(aliases) {
		b.WriteString(fmt.Sprintf("\tcase %q:\n\t\treturn json.Marshal(%q)\n", alias, aliases[alias]))
	}
	b.WriteString("\tdefault:\n\t\treturn json.Marshal(string(v))\n\t}\n}\n")
	return b.String()
}

func enumVariantAliasMap(shape SerdeShape) map[string]string {
	aliases := map[string]string{}
	for _, variant := range shape.VariantAliases {
		for _, alias := range variant.Aliases {
			aliases[alias] = variant.CanonicalWireValue
		}
	}
	return aliases
}

func sortedAliasValues(aliases map[string]string) []string {
	values := make([]string, 0, len(aliases))
	for alias := range aliases {
		values = append(values, alias)
	}
	sort.Strings(values)
	return values
}

func renderMultiAgentMode(schema Schema) (string, bool) {
	values, ok := multiAgentModeStringValues(schema)
	if !ok || !multiAgentModeHasCustomObjectVariant(schema) {
		return "", false
	}
	caseList := quotedStringCaseList(values)

	var b strings.Builder
	b.WriteString("type MultiAgentMode struct {\n")
	b.WriteString("\tMode string `json:\"-\"`\n")
	b.WriteString("\tCustom OptionalNonNull[string] `json:\"custom,omitempty\"`\n")
	b.WriteString("\tRawJSON json.RawMessage `json:\"-\"`\n")
	b.WriteString("}\n\n")
	b.WriteString("var (\n")
	for _, value := range values {
		b.WriteString(fmt.Sprintf("\t%s = MultiAgentMode{Mode: %q}\n", EnumConstName("MultiAgentMode", value), value))
	}
	b.WriteString(")\n\n")
	b.WriteString("func CustomMultiAgentMode(custom string) MultiAgentMode {\n")
	b.WriteString("\treturn MultiAgentMode{Custom: SomeNonNull(custom)}\n")
	b.WriteString("}\n\n")
	b.WriteString("func (v MultiAgentMode) IsSet() bool {\n")
	b.WriteString("\treturn v.Mode != \"\" || v.Custom.IsSet() || len(bytes.TrimSpace(v.RawJSON)) > 0\n")
	b.WriteString("}\n\n")
	b.WriteString("func (v MultiAgentMode) MarshalJSON() ([]byte, error) {\n")
	b.WriteString("\tmodeSet := v.Mode != \"\"\n")
	b.WriteString("\tcustomSet := v.Custom.IsSet()\n")
	b.WriteString("\trawSet := len(bytes.TrimSpace(v.RawJSON)) > 0\n")
	b.WriteString("\tmatches := 0\n")
	b.WriteString("\tif modeSet { matches++ }\n")
	b.WriteString("\tif customSet { matches++ }\n")
	b.WriteString("\tif rawSet { matches++ }\n")
	b.WriteString("\tif matches == 0 { return nil, DecodeError{Field: \"\", Reason: \"does not match any oneOf variant\"} }\n")
	b.WriteString("\tif matches > 1 { return nil, DecodeError{Field: \"\", Reason: \"matches multiple oneOf variants\"} }\n")
	b.WriteString("\tif modeSet {\n")
	b.WriteString("\t\tswitch v.Mode {\n")
	b.WriteString(fmt.Sprintf("\t\tcase %s:\n", caseList))
	b.WriteString("\t\t\treturn json.Marshal(v.Mode)\n")
	b.WriteString("\t\tdefault:\n")
	b.WriteString("\t\t\treturn nil, DecodeError{Field: \"\", Reason: fmt.Sprintf(\"unsupported multiAgentMode value %q\", v.Mode)}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif customSet { return json.Marshal(map[string]any{\"custom\": v.Custom}) }\n")
	b.WriteString("\tif !json.Valid(v.RawJSON) { return nil, fmt.Errorf(\"invalid MultiAgentMode raw fallback\") }\n")
	b.WriteString("\treturn append([]byte(nil), v.RawJSON...), nil\n")
	b.WriteString("}\n\n")
	b.WriteString("func (v *MultiAgentMode) UnmarshalJSON(data []byte) error {\n")
	b.WriteString("\ttrimmed := bytes.TrimSpace(data)\n")
	b.WriteString("\tif bytes.Equal(trimmed, []byte(\"null\")) { return DecodeError{Field: \"\", Reason: \"cannot be null\"} }\n")
	b.WriteString("\tvar mode string\n")
	b.WriteString("\tif err := json.Unmarshal(trimmed, &mode); err == nil {\n")
	b.WriteString("\t\tswitch mode {\n")
	b.WriteString(fmt.Sprintf("\t\tcase %s:\n", caseList))
	b.WriteString("\t\t\t*v = MultiAgentMode{Mode: mode}\n")
	b.WriteString("\t\tdefault:\n")
	b.WriteString("\t\t\tv.Mode = \"\"\n")
	b.WriteString("\t\t\tv.Custom = OptionalNonNull[string]{}\n")
	b.WriteString("\t\t\tv.RawJSON = append(v.RawJSON[:0], data...)\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tvar raw map[string]json.RawMessage\n")
	b.WriteString("\tif err := json.Unmarshal(trimmed, &raw); err != nil { return err }\n")
	b.WriteString("\trawCustom, ok := raw[\"custom\"]\n")
	b.WriteString("\tif !ok {\n")
	b.WriteString("\t\tv.Mode = \"\"\n")
	b.WriteString("\t\tv.Custom = OptionalNonNull[string]{}\n")
	b.WriteString("\t\tv.RawJSON = append(v.RawJSON[:0], data...)\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tvar custom OptionalNonNull[string]\n")
	b.WriteString("\tif err := json.Unmarshal(rawCustom, &custom); err != nil { return fmt.Errorf(\"field custom: %w\", err) }\n")
	b.WriteString("\t*v = MultiAgentMode{Custom: custom}\n")
	b.WriteString("\treturn nil\n")
	b.WriteString("}\n")
	return b.String(), true
}

func quotedStringCaseList(values []string) string {
	quoted := make([]string, 0, len(values))
	for _, value := range values {
		quoted = append(quoted, fmt.Sprintf("%q", value))
	}
	return strings.Join(quoted, ", ")
}

func multiAgentModeStringValues(schema Schema) ([]string, bool) {
	var values []string
	for _, variant := range schema.OneOf {
		if variant.Type != "string" || len(variant.Enum) == 0 {
			continue
		}
		for _, raw := range variant.Enum {
			var value string
			if err := json.Unmarshal(raw, &value); err != nil {
				return nil, false
			}
			values = append(values, value)
		}
	}
	return values, len(values) > 0
}

func multiAgentModeHasCustomObjectVariant(schema Schema) bool {
	for _, variant := range schema.OneOf {
		if variant.Type != "object" && len(variant.Properties) == 0 {
			continue
		}
		if _, ok := variant.Properties["custom"]; !ok {
			continue
		}
		for _, required := range variant.Required {
			if required == "custom" {
				return true
			}
		}
	}
	return false
}

type renderedField struct {
	FieldName         string
	WireName          string
	Type              string
	Required          bool
	RequiredNonNull   bool
	Flattened         bool
	Aliases           []string
	VariantAliases    []SerdeVariantAlias
	DefaultJSON       string
	Presence          string
	SkipIf            string
	CustomSerialize   string
	CustomDeserialize string
	Minimum           string
	Maximum           string
}

type renderedTaggedUnion struct {
	Discriminator string
	Variants      []renderedTaggedUnionVariant
}

type renderedTaggedUnionVariant struct {
	Tag      string
	Required []string
}

type renderedUntaggedUnion struct {
	Variants []renderedUntaggedUnionVariant
}

type renderedUntaggedUnionVariant struct {
	Required    []string
	ConstFields map[string]string
}

func renderStruct(name, key string, schema Schema, names map[string]string, serdeShapes map[string]SerdeShape, bundle *SchemaBundle) string {
	properties, required := collectStructProperties(schema)
	serdeShape := serdeShapeForType(name, key, serdeShapes)
	taggedUnion, hasTaggedUnion := taggedObjectUnion(schema)
	untaggedUnion, hasUntaggedUnion := untaggedObjectUnion(schema, hasTaggedUnion)
	stringUnionValues, hasStringObjectUnion := mixedStringObjectUnionValues(schema)
	var b strings.Builder
	var fields []renderedField
	b.WriteString(fmt.Sprintf("type %s struct {\n", name))
	if hasStringObjectUnion {
		b.WriteString("\tStringValue string `json:\"-\"`\n")
	}
	for _, propertyName := range sortedPropertyNames(properties) {
		property := properties[propertyName]
		fieldType := goTypeForSchema(property, names, required, required[propertyName])
		field := renderedField{
			FieldName:       goFieldName(propertyName),
			WireName:        propertyName,
			Type:            fieldType,
			Required:        required[propertyName],
			RequiredNonNull: required[propertyName] && !strings.HasPrefix(fieldType, "Optional[") && !schemaAllowsJSONNull(property),
			VariantAliases:  serdeShape.VariantAliases,
		}
		field.Minimum, field.Maximum = integerBoundsForSchema(property, names, bundle)
		if serdeField, ok := serdeFieldByWireName(serdeShape, propertyName); ok {
			field.Aliases = serdeField.Aliases
			field.Presence = serdeField.Shape.Presence
			field.SkipIf = serdeField.Shape.SkipSerializingIf
			field.CustomSerialize = serdeField.Shape.CustomSerialize
			field.CustomDeserialize = serdeField.Shape.CustomDeserialize
			if serdeField.Shape.Default != nil {
				field.DefaultJSON = serdeField.Shape.Default.WireValueJSON
			}
			if serdeField.Shape.Presence == "doubleOption" {
				field.DefaultJSON = ""
			}
		}
		fields = append(fields, field)
		b.WriteString(fmt.Sprintf("\t%s %s `json:%q`\n", field.FieldName, field.Type, propertyName+",omitempty"))
	}
	if hasTaggedUnion || hasUntaggedUnion || hasStringObjectUnion {
		b.WriteString("\tRawJSON json.RawMessage `json:\"-\"`\n")
	}
	for _, serdeField := range serdeShape.Fields {
		if !serdeField.Shape.Flattened {
			continue
		}
		field := renderedField{
			FieldName:         goFieldName(serdeField.RustField),
			WireName:          serdeField.WireName,
			Type:              flattenedFieldType(schema, names),
			Flattened:         true,
			Aliases:           serdeField.Aliases,
			VariantAliases:    serdeShape.VariantAliases,
			Presence:          serdeField.Shape.Presence,
			SkipIf:            serdeField.Shape.SkipSerializingIf,
			CustomSerialize:   serdeField.Shape.CustomSerialize,
			CustomDeserialize: serdeField.Shape.CustomDeserialize,
		}
		if serdeField.Shape.Default != nil {
			field.DefaultJSON = serdeField.Shape.Default.WireValueJSON
		}
		fields = append(fields, field)
		b.WriteString(fmt.Sprintf("\t%s %s `json:\"-\"`\n", field.FieldName, field.Type))
	}
	b.WriteString("}\n")
	b.WriteByte('\n')
	if hasStringObjectUnion {
		b.WriteString(renderMixedStringObjectUnionValues(name, stringUnionValues))
		b.WriteByte('\n')
		b.WriteString(renderMixedStringObjectUnionIsSet(name, fields))
		b.WriteByte('\n')
	}
	if !hasTaggedUnion {
		taggedUnion = renderedTaggedUnion{}
	}
	if !hasUntaggedUnion {
		untaggedUnion = renderedUntaggedUnion{}
	}
	b.WriteString(renderStructMarshal(name, fields, taggedUnion, untaggedUnion, stringUnionValues))
	b.WriteByte('\n')
	b.WriteString(renderStructUnmarshal(name, fields, taggedUnion, untaggedUnion, stringUnionValues))
	return b.String()
}

func renderStructMarshal(name string, fields []renderedField, taggedUnion renderedTaggedUnion, untaggedUnion renderedUntaggedUnion, stringUnionValues []string) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("func (v %s) MarshalJSON() ([]byte, error) {\n", name))
	b.WriteString("\tout := map[string]any{}\n")
	for _, field := range fields {
		if !field.Flattened {
			continue
		}
		b.WriteString(fmt.Sprintf("\tfor key, value := range v.%s { out[key] = value }\n", field.FieldName))
	}
	for _, field := range fields {
		if field.Flattened {
			continue
		}
		if strings.HasPrefix(field.Type, "Optional[") || strings.HasPrefix(field.Type, "OptionalNonNull[") {
			if field.SkipIf == skipSerializingIfNotNot {
				b.WriteString(fmt.Sprintf("\tif v.%s.IsSet() { if value, ok := v.%s.Value(); !ok || value { out[%q] = v.%s } }\n", field.FieldName, field.FieldName, field.WireName, field.FieldName))
			} else if field.SkipIf == skipSerializingIfOptionNone && field.CustomSerialize != doubleOptionSerializeHook {
				b.WriteString(fmt.Sprintf("\tif value, ok := v.%s.Value(); ok { out[%q] = value }\n", field.FieldName, field.WireName))
			} else if field.SkipIf == skipSerializingIfVecEmpty {
				b.WriteString(fmt.Sprintf("\tif value, ok := v.%s.Value(); ok && len(value) > 0 { out[%q] = value }\n", field.FieldName, field.WireName))
			} else {
				b.WriteString(fmt.Sprintf("\tif v.%s.IsSet() { out[%q] = v.%s }\n", field.FieldName, field.WireName, field.FieldName))
			}
		} else {
			if field.Type == "json.RawMessage" && !field.Required {
				b.WriteString(fmt.Sprintf("\tif len(v.%s) > 0 { out[%q] = v.%s }\n", field.FieldName, field.WireName, field.FieldName))
			} else if field.SkipIf == skipSerializingIfNotNot && field.Type == "bool" {
				b.WriteString(fmt.Sprintf("\tif v.%s { out[%q] = v.%s }\n", field.FieldName, field.WireName, field.FieldName))
			} else if field.SkipIf == skipSerializingIfOptionNone && field.Type == "json.RawMessage" {
				b.WriteString(fmt.Sprintf("\tif len(v.%s) > 0 { out[%q] = v.%s }\n", field.FieldName, field.WireName, field.FieldName))
			} else if field.SkipIf == skipSerializingIfVecEmpty {
				b.WriteString(fmt.Sprintf("\tif len(v.%s) > 0 { out[%q] = v.%s }\n", field.FieldName, field.WireName, field.FieldName))
			} else {
				b.WriteString(fmt.Sprintf("\tout[%q] = v.%s\n", field.WireName, field.FieldName))
			}
		}
	}
	if len(stringUnionValues) > 0 {
		b.WriteString("\tif v.StringValue != \"\" {\n")
		b.WriteString("\t\tif len(out) > 0 || len(bytes.TrimSpace(v.RawJSON)) > 0 { return nil, DecodeError{Field: \"\", Reason: \"matches multiple oneOf variants\"} }\n")
		b.WriteString("\t\tswitch v.StringValue {\n")
		b.WriteString(fmt.Sprintf("\t\tcase %s:\n", quotedStringCaseList(stringUnionValues)))
		b.WriteString("\t\t\treturn json.Marshal(v.StringValue)\n")
		b.WriteString("\t\tdefault:\n")
		b.WriteString(fmt.Sprintf("\t\t\treturn nil, DecodeError{Field: \"\", Reason: fmt.Sprintf(\"unsupported %s string value %%q\", v.StringValue)}\n", name))
		b.WriteString("\t\t}\n")
		b.WriteString("\t}\n")
	}
	b.WriteString(renderTaggedUnionMarshalValidation(taggedUnion, fields))
	b.WriteString(renderUntaggedUnionMarshalValidation(name, untaggedUnion))
	b.WriteString("\treturn json.Marshal(out)\n}\n")
	return b.String()
}

func renderStructUnmarshal(name string, fields []renderedField, taggedUnion renderedTaggedUnion, untaggedUnion renderedUntaggedUnion, stringUnionValues []string) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("func (v *%s) UnmarshalJSON(data []byte) error {\n", name))
	b.WriteString("\ttrimmed := bytes.TrimSpace(data)\n")
	b.WriteString("\tif bytes.Equal(trimmed, []byte(\"null\")) { return DecodeError{Field: \"\", Reason: \"cannot be null\"} }\n")
	if len(stringUnionValues) > 0 {
		b.WriteString("\tvar stringValue string\n")
		b.WriteString("\tif err := json.Unmarshal(trimmed, &stringValue); err == nil {\n")
		b.WriteString("\t\tswitch stringValue {\n")
		b.WriteString(fmt.Sprintf("\t\tcase %s:\n", quotedStringCaseList(stringUnionValues)))
		b.WriteString(fmt.Sprintf("\t\t\t*v = %s{StringValue: stringValue}\n", name))
		b.WriteString("\t\tdefault:\n")
		b.WriteString(fmt.Sprintf("\t\t\t*v = %s{RawJSON: append(json.RawMessage(nil), data...)}\n", name))
		b.WriteString("\t\t}\n")
		b.WriteString("\t\treturn nil\n")
		b.WriteString("\t}\n")
		b.WriteString("\tv.StringValue = \"\"\n")
	}
	b.WriteString("\tvar raw map[string]json.RawMessage\n")
	b.WriteString("\tif err := json.Unmarshal(trimmed, &raw); err != nil { return err }\n")
	b.WriteString(renderTaggedUnionUnknownPrecheck(taggedUnion, fields))
	for _, field := range fields {
		if field.Flattened {
			continue
		}
		valueName := "raw" + field.FieldName
		b.WriteString(fmt.Sprintf("\t%s, ok := raw[%q]\n", valueName, field.WireName))
		for _, alias := range field.Aliases {
			b.WriteString(fmt.Sprintf("\tif !ok { %s, ok = raw[%q] }\n", valueName, alias))
		}
		if field.DefaultJSON != "" {
			b.WriteString(fmt.Sprintf("\tif !ok { %s = []byte(%q); ok = true }\n", valueName, field.DefaultJSON))
		}
		if field.Required {
			b.WriteString(fmt.Sprintf("\tif !ok { return DecodeError{Field: %q, Reason: \"missing required field\"} }\n", field.WireName))
		} else {
			b.WriteString("\tif ok {\n")
		}
		if field.RequiredNonNull {
			b.WriteString(fmt.Sprintf("\tif bytes.Equal(%s, []byte(\"null\")) { return DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", valueName, field.WireName))
		}
		for _, variantAlias := range field.VariantAliases {
			if field.WireName != "kind" {
				continue
			}
			for _, alias := range variantAlias.Aliases {
				canonical, _ := json.Marshal(variantAlias.CanonicalWireValue)
				aliasJSON, _ := json.Marshal(alias)
				b.WriteString(fmt.Sprintf("\tif bytes.Equal(%s, []byte(%q)) { %s = []byte(%q) }\n", valueName, string(aliasJSON), valueName, string(canonical)))
			}
		}
		if field.CustomDeserialize == dynamicToolSpecsDeserializeHook {
			b.WriteString(renderDynamicToolSpecsUnmarshal(field, valueName))
			if !field.Required {
				b.WriteString("\t}\n")
			}
			continue
		}
		b.WriteString(fmt.Sprintf("\tif err := json.Unmarshal(%s, &v.%s); err != nil { return fmt.Errorf(\"field %s: %%w\", err) }\n", valueName, field.FieldName, field.WireName))
		b.WriteString(renderIntegerBoundsValidation(field))
		if !field.Required {
			b.WriteString("\t}\n")
		}
	}
	for _, field := range fields {
		if !field.Flattened {
			continue
		}
		b.WriteString(fmt.Sprintf("\tv.%s = map[string]json.RawMessage{}\n", field.FieldName))
		b.WriteString("\tfor key, value := range raw {\n")
		b.WriteString("\t\tswitch key {\n")
		for _, known := range knownJSONFieldNames(fields) {
			b.WriteString(fmt.Sprintf("\t\tcase %q:\n\t\t\tcontinue\n", known))
		}
		b.WriteString("\t\t}\n")
		b.WriteString(fmt.Sprintf("\t\tv.%s[key] = append(v.%s[key][:0], value...)\n", field.FieldName, field.FieldName))
		b.WriteString("\t}\n")
		if field.DefaultJSON != "" {
			b.WriteString(fmt.Sprintf("\tif len(v.%s) == 0 { if err := json.Unmarshal([]byte(%q), &v.%s); err != nil { return fmt.Errorf(\"field %s: %%w\", err) } }\n", field.FieldName, field.DefaultJSON, field.FieldName, field.WireName))
		}
	}
	b.WriteString(renderTaggedUnionValidation(taggedUnion))
	b.WriteString(renderUntaggedUnionValidation(untaggedUnion))
	b.WriteString("\treturn nil\n}\n")
	return b.String()
}

func renderMixedStringObjectUnionValues(name string, values []string) string {
	var b strings.Builder
	b.WriteString("var (\n")
	for _, value := range values {
		b.WriteString(fmt.Sprintf("\t%s = %s{StringValue: %q}\n", EnumConstName(name, value), name, value))
	}
	b.WriteString(")\n")
	return b.String()
}

func renderMixedStringObjectUnionIsSet(name string, fields []renderedField) string {
	checks := []string{"v.StringValue != \"\"", "len(bytes.TrimSpace(v.RawJSON)) > 0"}
	for _, field := range fields {
		switch {
		case strings.HasPrefix(field.Type, "Optional[") || strings.HasPrefix(field.Type, "OptionalNonNull["):
			checks = append(checks, fmt.Sprintf("v.%s.IsSet()", field.FieldName))
		case field.Type == "json.RawMessage":
			checks = append(checks, fmt.Sprintf("len(bytes.TrimSpace(v.%s)) > 0", field.FieldName))
		case field.Type == "string":
			checks = append(checks, fmt.Sprintf("v.%s != \"\"", field.FieldName))
		case strings.HasPrefix(field.Type, "[]") || strings.HasPrefix(field.Type, "map["):
			checks = append(checks, fmt.Sprintf("len(v.%s) > 0", field.FieldName))
		case field.Type == "bool":
			checks = append(checks, fmt.Sprintf("v.%s", field.FieldName))
		default:
			checks = append(checks, fmt.Sprintf("v.%s != 0", field.FieldName))
		}
	}
	return fmt.Sprintf("func (v %s) IsSet() bool {\n\treturn %s\n}\n", name, strings.Join(checks, " || "))
}

func renderIntegerBoundsValidation(field renderedField) string {
	if field.Minimum == "" && field.Maximum == "" {
		return ""
	}
	var b strings.Builder
	valueExpr := "v." + field.FieldName
	if strings.HasPrefix(field.Type, "Optional[") || strings.HasPrefix(field.Type, "OptionalNonNull[") {
		valueName := "value" + field.FieldName
		b.WriteString(fmt.Sprintf("\tif %s, ok := v.%s.Value(); ok {\n", valueName, field.FieldName))
		b.WriteString(renderIntegerBoundsValidationForValue(field, valueName, "\t"))
		b.WriteString("\t}\n")
		return b.String()
	}
	return renderIntegerBoundsValidationForValue(field, valueExpr, "")
}

func renderIntegerBoundsValidationForValue(field renderedField, valueExpr, indent string) string {
	var b strings.Builder
	if field.Minimum != "" {
		b.WriteString(fmt.Sprintf("%s\tif %s < %s { return DecodeError{Field: %q, Reason: %q} }\n", indent, valueExpr, field.Minimum, field.WireName, "below minimum "+field.Minimum))
	}
	if field.Maximum != "" {
		b.WriteString(fmt.Sprintf("%s\tif %s > %s { return DecodeError{Field: %q, Reason: %q} }\n", indent, valueExpr, field.Maximum, field.WireName, "above maximum "+field.Maximum))
	}
	return b.String()
}

func integerBoundsForSchema(schema Schema, names map[string]string, bundle *SchemaBundle) (string, string) {
	if minimum, maximum, ok := directIntegerBoundsForSchema(schema); ok {
		return minimum, maximum
	}
	if bundle == nil {
		return "", ""
	}
	if ref, ok := schema.SingleRef(); ok {
		if key, ok := schemaDefinitionKeyForRef(ref, bundle, names); ok {
			definition, _ := bundle.Definition(key)
			return integerBoundsForSchema(definition, names, bundle)
		}
	}
	if ref, ok := schema.NullableRef(); ok {
		if key, ok := schemaDefinitionKeyForRef(ref, bundle, names); ok {
			definition, _ := bundle.Definition(key)
			return integerBoundsForSchema(definition, names, bundle)
		}
	}
	return "", ""
}

func directIntegerBoundsForSchema(schema Schema) (string, string, bool) {
	if !schemaHasIntegerType(schema) {
		return "", "", false
	}
	minimum, hasMinimum := integerBoundLiteral(schema.Minimum)
	maximum, hasMaximum := integerBoundLiteral(schema.Maximum)
	return minimum, maximum, hasMinimum || hasMaximum
}

func schemaHasIntegerType(schema Schema) bool {
	if schema.Type == "integer" {
		return true
	}
	for _, schemaType := range schema.Types {
		if schemaType == "integer" {
			return true
		}
	}
	return false
}

func integerBoundLiteral(bound *json.Number) (string, bool) {
	if bound == nil {
		return "", false
	}
	value := bound.String()
	value = strings.TrimSuffix(value, ".0")
	if strings.ContainsAny(value, ".eE") {
		return "", false
	}
	return value, true
}

func renderTaggedUnionUnknownPrecheck(union renderedTaggedUnion, fields []renderedField) string {
	if union.Discriminator == "" {
		return ""
	}
	field := renderedField{FieldName: goFieldName(union.Discriminator), WireName: union.Discriminator}
	for _, candidate := range fields {
		if candidate.WireName == union.Discriminator {
			field = candidate
			break
		}
	}
	rawName := "raw" + field.FieldName + "Discriminator"
	valueName := field.FieldName + "Discriminator"
	var b strings.Builder
	b.WriteString(fmt.Sprintf("\t%s, ok := raw[%q]\n", rawName, union.Discriminator))
	b.WriteString(fmt.Sprintf("\tif !ok { return DecodeError{Field: %q, Reason: \"missing required field\"} }\n", union.Discriminator))
	b.WriteString(fmt.Sprintf("\tif bytes.Equal(%s, []byte(\"null\")) { return DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", rawName, union.Discriminator))
	for _, variantAlias := range field.VariantAliases {
		if field.WireName != "kind" {
			continue
		}
		for _, alias := range variantAlias.Aliases {
			canonical, _ := json.Marshal(variantAlias.CanonicalWireValue)
			aliasJSON, _ := json.Marshal(alias)
			b.WriteString(fmt.Sprintf("\tif bytes.Equal(%s, []byte(%q)) { %s = []byte(%q) }\n", rawName, string(aliasJSON), rawName, string(canonical)))
		}
	}
	b.WriteString(fmt.Sprintf("\tvar %s string\n", valueName))
	b.WriteString(fmt.Sprintf("\tif err := json.Unmarshal(%s, &%s); err != nil { return fmt.Errorf(\"field %s: %%w\", err) }\n", rawName, valueName, union.Discriminator))
	b.WriteString(fmt.Sprintf("\tswitch %s {\n", valueName))
	for _, variant := range union.Variants {
		b.WriteString(fmt.Sprintf("\tcase %q:\n", variant.Tag))
	}
	b.WriteString("\tdefault:\n")
	b.WriteString(fmt.Sprintf("\t\tv.%s = %s\n", field.FieldName, valueName))
	b.WriteString("\t\tv.RawJSON = append(v.RawJSON[:0], data...)\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	return b.String()
}

func renderDynamicToolSpecsUnmarshal(field renderedField, valueName string) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("\tif bytes.Equal(%s, []byte(\"null\")) {\n", valueName))
	b.WriteString(fmt.Sprintf("\t\tv.%s = Null[[]DynamicToolSpec]()\n", field.FieldName))
	b.WriteString("\t} else {\n")
	b.WriteString(fmt.Sprintf("\t\t%sValue, err := normalizeDynamicToolSpecsJSON(%s)\n", field.FieldName, valueName))
	b.WriteString(fmt.Sprintf("\t\tif err != nil { return fmt.Errorf(\"field %s: %%w\", err) }\n", field.WireName))
	b.WriteString(fmt.Sprintf("\t\tv.%s = Some(%sValue)\n", field.FieldName, field.FieldName))
	b.WriteString("\t}\n")
	return b.String()
}

func renderTaggedUnionValidation(union renderedTaggedUnion) string {
	if union.Discriminator == "" {
		return ""
	}
	var b strings.Builder
	fieldName := goFieldName(union.Discriminator)
	b.WriteString("\tv.RawJSON = nil\n")
	b.WriteString(fmt.Sprintf("\tswitch v.%s {\n", fieldName))
	for _, variant := range union.Variants {
		b.WriteString(fmt.Sprintf("\tcase %q:\n", variant.Tag))
		for _, required := range variant.Required {
			if required == union.Discriminator {
				continue
			}
			reason := fmt.Sprintf("missing required field for %s %s", union.Discriminator, variant.Tag)
			b.WriteString(fmt.Sprintf("\t\tif rawValue, ok := raw[%q]; !ok { return DecodeError{Field: %q, Reason: %q} } else if bytes.Equal(rawValue, []byte(\"null\")) { return DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", required, required, reason, required))
		}
	}
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\tv.RawJSON = append(v.RawJSON[:0], data...)\n")
	b.WriteString("\t}\n")
	return b.String()
}

func renderTaggedUnionMarshalValidation(union renderedTaggedUnion, fields []renderedField) string {
	if union.Discriminator == "" {
		return ""
	}
	fieldsByWireName := map[string]renderedField{}
	for _, field := range fields {
		fieldsByWireName[field.WireName] = field
	}
	var b strings.Builder
	discriminatorField := goFieldName(union.Discriminator)
	b.WriteString(fmt.Sprintf("\tswitch v.%s {\n", discriminatorField))
	for _, variant := range union.Variants {
		b.WriteString(fmt.Sprintf("\tcase %q:\n", variant.Tag))
		for _, required := range variant.Required {
			if required == union.Discriminator {
				continue
			}
			field, ok := fieldsByWireName[required]
			if !ok {
				continue
			}
			b.WriteString(renderMarshalRequiredFieldCheck(field, union.Discriminator, variant.Tag))
		}
	}
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\tif len(v.RawJSON) > 0 { return append([]byte(nil), v.RawJSON...), nil }\n")
	b.WriteString(fmt.Sprintf("\t\treturn nil, DecodeError{Field: %q, Reason: fmt.Sprintf(\"unsupported discriminator value %%q\", v.%s)}\n", union.Discriminator, discriminatorField))
	b.WriteString("\t}\n")
	return b.String()
}

func renderUntaggedUnionValidation(union renderedUntaggedUnion) string {
	if len(union.Variants) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString("\tuntaggedOneOfMatches := 0\n")
	b.WriteString("\tuntaggedOneOfKnownVariantMatches := false\n")
	for index, variant := range union.Variants {
		matchName := fmt.Sprintf("untaggedOneOfVariant%dMatches", index)
		constMatchName := fmt.Sprintf("untaggedOneOfVariant%dConstMatches", index)
		b.WriteString(fmt.Sprintf("\t%s := true\n", matchName))
		if len(variant.ConstFields) > 0 {
			b.WriteString(fmt.Sprintf("\t%s := true\n", constMatchName))
		}
		for _, required := range sortedStrings(variant.Required) {
			b.WriteString(fmt.Sprintf("\tif rawValue, ok := raw[%q]; !ok || bytes.Equal(bytes.TrimSpace(rawValue), []byte(\"null\")) { %s = false }\n", required, matchName))
		}
		for _, fieldName := range sortedMapKeys(variant.ConstFields) {
			value := variant.ConstFields[fieldName]
			encoded, _ := json.Marshal(value)
			b.WriteString(fmt.Sprintf("\tif rawValue, ok := raw[%q]; !ok || !bytes.Equal(bytes.TrimSpace(rawValue), []byte(%q)) { %s = false; %s = false }\n", fieldName, string(encoded), matchName, constMatchName))
		}
		if len(variant.ConstFields) > 0 {
			b.WriteString(fmt.Sprintf("\tif %s { untaggedOneOfKnownVariantMatches = true }\n", constMatchName))
		}
		b.WriteString(fmt.Sprintf("\tif %s { untaggedOneOfMatches++ }\n", matchName))
	}
	b.WriteString("\tif untaggedOneOfMatches == 0 {\n")
	b.WriteString("\t\tif !untaggedOneOfKnownVariantMatches { v.RawJSON = append(v.RawJSON[:0], data...); return nil }\n")
	b.WriteString("\t\treturn DecodeError{Field: \"\", Reason: \"does not match any oneOf variant\"}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif untaggedOneOfMatches > 1 { return DecodeError{Field: \"\", Reason: \"matches multiple oneOf variants\"} }\n")
	b.WriteString("\tv.RawJSON = nil\n")
	return b.String()
}

func renderUntaggedUnionMarshalValidation(name string, union renderedUntaggedUnion) string {
	if len(union.Variants) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString("\tuntaggedOneOfMatches := 0\n")
	b.WriteString("\tuntaggedOneOfKnownVariantMatches := false\n")
	for index, variant := range union.Variants {
		matchName := fmt.Sprintf("untaggedOneOfVariant%dMatches", index)
		constMatchName := fmt.Sprintf("untaggedOneOfVariant%dConstMatches", index)
		b.WriteString(fmt.Sprintf("\t%s := true\n", matchName))
		if len(variant.ConstFields) > 0 {
			b.WriteString(fmt.Sprintf("\t%s := true\n", constMatchName))
		}
		for _, required := range sortedStrings(variant.Required) {
			b.WriteString(fmt.Sprintf("\tif _, ok := out[%q]; !ok { %s = false }\n", required, matchName))
		}
		for _, fieldName := range sortedMapKeys(variant.ConstFields) {
			value := variant.ConstFields[fieldName]
			encoded, _ := json.Marshal(value)
			b.WriteString(fmt.Sprintf("\tif rawValue, ok := out[%q]; !ok { %s = false; %s = false } else if rawJSON, err := json.Marshal(rawValue); err != nil || !bytes.Equal(bytes.TrimSpace(rawJSON), []byte(%q)) { %s = false; %s = false }\n", fieldName, matchName, constMatchName, string(encoded), matchName, constMatchName))
		}
		if len(variant.ConstFields) > 0 {
			b.WriteString(fmt.Sprintf("\tif %s { untaggedOneOfKnownVariantMatches = true }\n", constMatchName))
		}
		b.WriteString(fmt.Sprintf("\tif %s { untaggedOneOfMatches++ }\n", matchName))
	}
	b.WriteString("\tif untaggedOneOfMatches == 0 {\n")
	b.WriteString("\t\tif !untaggedOneOfKnownVariantMatches && len(bytes.TrimSpace(v.RawJSON)) > 0 {\n")
	b.WriteString(fmt.Sprintf("\t\t\tif !json.Valid(v.RawJSON) { return nil, fmt.Errorf(\"invalid %s raw fallback\") }\n", name))
	b.WriteString("\t\t\treturn append([]byte(nil), v.RawJSON...), nil\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn nil, DecodeError{Field: \"\", Reason: \"does not match any oneOf variant\"}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif untaggedOneOfMatches > 1 { return nil, DecodeError{Field: \"\", Reason: \"matches multiple oneOf variants\"} }\n")
	return b.String()
}

func renderMarshalRequiredFieldCheck(field renderedField, discriminator, tag string) string {
	reason := fmt.Sprintf("missing required field for %s %s", discriminator, tag)
	switch {
	case strings.HasPrefix(field.Type, "Optional["):
		return fmt.Sprintf("\t\tif !v.%s.IsSet() { return nil, DecodeError{Field: %q, Reason: %q} }\n\t\tif v.%s.IsNull() { return nil, DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", field.FieldName, field.WireName, reason, field.FieldName, field.WireName)
	case strings.HasPrefix(field.Type, "OptionalNonNull["):
		return fmt.Sprintf("\t\tif !v.%s.IsSet() { return nil, DecodeError{Field: %q, Reason: %q} }\n", field.FieldName, field.WireName, reason)
	case field.Type == "json.RawMessage":
		return fmt.Sprintf("\t\tif len(v.%s) == 0 { return nil, DecodeError{Field: %q, Reason: %q} }\n\t\tif bytes.Equal(v.%s, []byte(\"null\")) { return nil, DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", field.FieldName, field.WireName, reason, field.FieldName, field.WireName)
	default:
		return ""
	}
}

func flattenedFieldType(schema Schema, names map[string]string) string {
	if schema.AdditionalProperties != nil {
		return "map[string]" + goTypeForSchema(*schema.AdditionalProperties, names, nil, true)
	}
	return "map[string]json.RawMessage"
}

func knownJSONFieldNames(fields []renderedField) []string {
	var names []string
	for _, field := range fields {
		if field.Flattened {
			continue
		}
		names = append(names, field.WireName)
		names = append(names, field.Aliases...)
	}
	sort.Strings(names)
	return names
}

func mapSerdeShapes(shapes []SerdeShape) map[string]SerdeShape {
	out := make(map[string]SerdeShape, len(shapes))
	for _, shape := range shapes {
		out[shape.RustType] = shape
		out[goInitialisms(GoTypeName(shape.RustType))] = shape
	}
	return out
}

func serdeShapeForType(name, key string, shapes map[string]SerdeShape) SerdeShape {
	if shape, ok := shapes[name]; ok {
		return shape
	}
	base := strings.TrimPrefix(key, "v2/")
	if shape, ok := shapes[base]; ok {
		return shape
	}
	return SerdeShape{}
}

func serdeFieldByWireName(shape SerdeShape, wireName string) (SerdeField, bool) {
	for _, field := range shape.Fields {
		if field.WireName == wireName {
			return field, true
		}
	}
	return SerdeField{}, false
}

func collectStructProperties(schema Schema) (map[string]Schema, map[string]bool) {
	properties := map[string]Schema{}
	required := map[string]bool{}
	for _, name := range schema.Required {
		required[name] = true
	}
	for name, property := range schema.Properties {
		properties[name] = property
	}
	if oneOfObjectUnion(schema) {
		if taggedUnion, ok := taggedObjectUnion(schema); ok {
			required[taggedUnion.Discriminator] = true
		}
		for _, variant := range schema.OneOf {
			for name, property := range variant.Properties {
				if _, ok := properties[name]; !ok {
					properties[name] = property
				}
			}
		}
	}
	return properties, required
}

func sortedPropertyNames(properties map[string]Schema) []string {
	names := make([]string, 0, len(properties))
	for name := range properties {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

func sortedStrings(values []string) []string {
	out := append([]string(nil), values...)
	sort.Strings(out)
	return out
}

func sortedMapKeys(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func goTypeForSchema(schema Schema, names map[string]string, required map[string]bool, isRequired bool) string {
	if ref, ok := schema.SingleRef(); ok {
		return wrapOptional(typeNameForRef(ref, names), schema, isRequired)
	}
	if ref, ok := schema.NullableRef(); ok {
		return "Optional[" + typeNameForRef(ref, names) + "]"
	}
	if len(schema.Types) == 2 && hasType(schema.Types, "null") {
		return "Optional[" + scalarGoType(nonNullType(schema.Types), schema, names) + "]"
	}
	if !isRequired && schema.Type != "" {
		return "OptionalNonNull[" + scalarGoType(schema.Type, schema, names) + "]"
	}
	if schema.Type != "" {
		return scalarGoType(schema.Type, schema, names)
	}
	if len(schema.OneOf) > 0 || len(schema.AnyOf) > 0 {
		return "json.RawMessage"
	}
	return "json.RawMessage"
}

func scalarGoType(schemaType string, schema Schema, names map[string]string) string {
	switch schemaType {
	case "string":
		return "string"
	case "boolean":
		return "bool"
	case "integer":
		return integerGoType(schema.Format)
	case "number":
		return "float64"
	case "array":
		if schema.Items == nil {
			return "[]json.RawMessage"
		}
		return "[]" + goTypeForSchema(*schema.Items, names, nil, true)
	case "object":
		if schema.AdditionalProperties != nil {
			return "map[string]" + goTypeForSchema(*schema.AdditionalProperties, names, nil, true)
		}
		return "map[string]json.RawMessage"
	case "null":
		return "json.RawMessage"
	default:
		return "json.RawMessage"
	}
}

func integerGoType(format string) string {
	switch format {
	case "uint", "uint64":
		return "uint64"
	case "uint32":
		return "uint32"
	case "uint16":
		return "uint16"
	case "int32":
		return "int32"
	case "", "int64":
		return "int64"
	default:
		return "int64"
	}
}

func wrapOptional(base string, schema Schema, isRequired bool) string {
	if isRequired {
		return base
	}
	return "OptionalNonNull[" + base + "]"
}

func hasType(types []string, want string) bool {
	for _, typ := range types {
		if typ == want {
			return true
		}
	}
	return false
}

func schemaAllowsJSONNull(schema Schema) bool {
	if schema.BooleanSchema != nil {
		return *schema.BooleanSchema
	}
	if schema.Type == "null" || hasType(schema.Types, "null") {
		return true
	}
	if _, ok := schema.NullableRef(); ok {
		return true
	}
	return false
}

func nonNullType(types []string) string {
	for _, typ := range types {
		if typ != "null" {
			return typ
		}
	}
	return "json.RawMessage"
}

func dedupeStrings(values []string) []string {
	seen := map[string]bool{}
	var out []string
	for _, value := range values {
		if seen[value] {
			continue
		}
		seen[value] = true
		out = append(out, value)
	}
	sort.Strings(out)
	return out
}

func typeNameForRef(ref string, names map[string]string) string {
	ref = strings.TrimPrefix(ref, "#/definitions/")
	if ref == "RequestId" || ref == "v2/RequestId" {
		return "RequestID"
	}
	if name, ok := names[ref]; ok {
		return name
	}
	return typeNameForDefinition(ref)
}

func typeNameForDefinition(key string) string {
	if key == "RequestId" || key == "v2/RequestId" {
		return "RequestID"
	}
	return goInitialisms(GoTypeName(key))
}

func namespaceTypeName(key string) string {
	if strings.HasPrefix(key, "v2/") {
		return "V2" + goInitialisms(GoTypeName(strings.TrimPrefix(key, "v2/")))
	}
	return goInitialisms(GoTypeName(key))
}

func goFieldName(name string) string {
	return goInitialisms(GoTypeName(name))
}

func goInitialisms(name string) string {
	replacer := strings.NewReplacer("Jsonrpc", "JSONRPC", "Id", "ID", "Url", "URL", "Json", "JSON", "Rpc", "RPC", "Api", "API")
	return replacer.Replace(name)
}

func stringEnumValues(schema Schema) ([]string, bool) {
	if len(schema.Enum) > 0 && (schema.Type == "string" || schema.Type == "") {
		values := make([]string, 0, len(schema.Enum))
		for _, raw := range schema.Enum {
			var value string
			if err := json.Unmarshal(raw, &value); err != nil {
				return nil, false
			}
			values = append(values, value)
		}
		return values, true
	}
	if len(schema.OneOf) == 0 {
		return nil, false
	}
	var values []string
	for _, variant := range schema.OneOf {
		if len(variant.Enum) != 1 || variant.Type != "string" {
			return nil, false
		}
		var value string
		if err := json.Unmarshal(variant.Enum[0], &value); err != nil {
			return nil, false
		}
		values = append(values, value)
	}
	return values, true
}

func oneOfObjectUnion(schema Schema) bool {
	if len(schema.OneOf) == 0 {
		return false
	}
	for _, variant := range schema.OneOf {
		if variant.Type == "object" || len(variant.Properties) > 0 {
			return true
		}
	}
	return false
}

func mixedStringObjectUnionValues(schema Schema) ([]string, bool) {
	if len(schema.OneOf) == 0 {
		return nil, false
	}
	var values []string
	hasObject := false
	for _, variant := range schema.OneOf {
		switch {
		case variant.Type == "string" && len(variant.Enum) > 0:
			for _, raw := range variant.Enum {
				var value string
				if err := json.Unmarshal(raw, &value); err != nil {
					return nil, false
				}
				values = append(values, value)
			}
		case variant.Type == "object" || len(variant.Properties) > 0:
			hasObject = true
		default:
			return nil, false
		}
	}
	return values, hasObject && len(values) > 0
}

func taggedObjectUnion(schema Schema) (renderedTaggedUnion, bool) {
	if len(schema.OneOf) == 0 {
		return renderedTaggedUnion{}, false
	}
	for _, discriminator := range []string{"type", "kind"} {
		var variants []renderedTaggedUnionVariant
		seenTags := map[string]bool{}
		ok := true
		for _, variant := range schema.OneOf {
			if variant.Type != "object" && len(variant.Properties) == 0 {
				ok = false
				break
			}
			tagSchema, hasDiscriminator := variant.Properties[discriminator]
			if !hasDiscriminator {
				ok = false
				break
			}
			values, hasValues := stringEnumValues(tagSchema)
			if !hasValues || len(values) != 1 || seenTags[values[0]] {
				ok = false
				break
			}
			seenTags[values[0]] = true
			variants = append(variants, renderedTaggedUnionVariant{
				Tag:      values[0],
				Required: append([]string(nil), variant.Required...),
			})
		}
		if ok {
			return renderedTaggedUnion{Discriminator: discriminator, Variants: variants}, true
		}
	}
	return renderedTaggedUnion{}, false
}

func untaggedObjectUnion(schema Schema, hasTaggedUnion bool) (renderedUntaggedUnion, bool) {
	if hasTaggedUnion || len(schema.OneOf) == 0 {
		return renderedUntaggedUnion{}, false
	}
	var variants []renderedUntaggedUnionVariant
	for _, variant := range schema.OneOf {
		if variant.Type == "string" && len(variant.Enum) > 0 {
			continue
		}
		if variant.Type != "object" && len(variant.Properties) == 0 {
			return renderedUntaggedUnion{}, false
		}
		rendered := renderedUntaggedUnionVariant{
			Required:    append([]string(nil), variant.Required...),
			ConstFields: map[string]string{},
		}
		for fieldName, property := range variant.Properties {
			values, ok := stringEnumValues(property)
			if ok && len(values) == 1 {
				rendered.ConstFields[fieldName] = values[0]
			}
		}
		if len(rendered.Required) == 0 && len(rendered.ConstFields) == 0 {
			return renderedUntaggedUnion{}, false
		}
		variants = append(variants, rendered)
	}
	return renderedUntaggedUnion{Variants: variants}, len(variants) > 0
}
