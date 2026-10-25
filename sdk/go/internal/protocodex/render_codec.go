package protocodex

import (
	"encoding/json"
	"fmt"
	"strings"
	"unicode"
)

// The generated object codecs are thin: UnmarshalJSON hands the object to the decoder
// runtime in protocol/codec.go together with an objectSpec table rendered here, and
// MarshalJSON marshals a method-free alias of the struct whose field tags (omitzero,
// omitempty) express the serde skip rules. Only the checks the runtime cannot express in
// a table — union variants, integer bounds, custom hooks — are rendered as code.

// runtimeMaxObjectFields mirrors maxObjectFields in protocol/codec.go.
const runtimeMaxObjectFields = 64

// objectCodec is what renderStruct knows about one generated object type.
type objectCodec struct {
	name              string
	fields            []renderedField
	taggedUnion       renderedTaggedUnion
	untaggedUnion     renderedUntaggedUnion
	stringUnionValues []string
}

func (c objectCodec) wireFields() []renderedField {
	var out []renderedField
	for _, field := range c.fields {
		if !field.Flattened {
			out = append(out, field)
		}
	}
	return out
}

func (c objectCodec) fieldOffset() int {
	if len(c.stringUnionValues) > 0 {
		return 1 // StringValue comes first
	}
	return 0
}

func (c objectCodec) hasNestedAnyOf() bool {
	for _, variant := range c.taggedUnion.Variants {
		if len(variant.AnyOfAlternatives) > 0 {
			return true
		}
	}
	return false
}

func (c objectCodec) hasFlattened() bool {
	return len(c.wireFields()) != len(c.fields)
}

// marshalsThroughMap reports whether MarshalJSON keeps building a map[string]any: the
// marshal checks of untagged and mixed unions and the flattened fields read the map.
func (c objectCodec) marshalsThroughMap() bool {
	return len(c.untaggedUnion.Variants) > 0 || len(c.stringUnionValues) > 0 || c.hasFlattened()
}

func objectSpecName(name string) string {
	return lowerCamel(name) + "Spec"
}

// lowerCamel lowercases a leading initialism the way Go names do: JSONRPCError becomes
// jsonrpcError and URL becomes url.
func lowerCamel(name string) string {
	runes := []rune(name)
	for i := range runes {
		if !unicode.IsUpper(runes[i]) {
			break
		}
		if i > 0 && i+1 < len(runes) && unicode.IsLower(runes[i+1]) {
			break
		}
		runes[i] = unicode.ToLower(runes[i])
	}
	return string(runes)
}

func isOptionalGoType(goType string) bool {
	return strings.HasPrefix(goType, "Optional[") || strings.HasPrefix(goType, "OptionalNonNull[")
}

// wireTag is the struct tag of a generated field: its wire name, and the omit option
// that reproduces the serde skip rule when the struct is marshaled through an alias.
func wireTag(field renderedField) string {
	option := ""
	switch {
	case isOptionalGoType(field.Type):
		option = ",omitzero"
	case field.Type == "json.RawMessage" && !field.Required:
		option = ",omitempty"
	case field.SkipIf == skipSerializingIfNotNot && field.Type == "bool":
		option = ",omitempty"
	case field.SkipIf == skipSerializingIfOptionNone && field.Type == "json.RawMessage":
		option = ",omitempty"
	case field.SkipIf == skipSerializingIfVecEmpty:
		option = ",omitempty"
	}
	return fmt.Sprintf("`json:%q`", field.WireName+option)
}

// validateObjectCodec rejects types the decoder runtime cannot represent, so that a
// schema change fails generation instead of rendering a decoder that differs from serde.
func validateObjectCodec(c objectCodec) error {
	wireFields := c.wireFields()
	if len(wireFields) > runtimeMaxObjectFields {
		return fmt.Errorf("%s has %d wire fields; the protocol decoder runtime supports %d (maxObjectFields in protocol/codec.go)", c.name, len(wireFields), runtimeMaxObjectFields)
	}
	byWireName := map[string]renderedField{}
	for _, field := range wireFields {
		if !validJSONTagName(field.WireName) {
			return fmt.Errorf("%s field %q is not a valid encoding/json tag name", c.name, field.WireName)
		}
		byWireName[field.WireName] = field
		bounded := field.Minimum != "" || field.Maximum != ""
		if !bounded || isOptionalGoType(field.Type) {
			continue
		}
		if len(field.NestedAnyOfTags) > 0 {
			return fmt.Errorf("%s field %q has integer bounds on a non-optional nested anyOf alternative", c.name, field.WireName)
		}
		if !field.Required && field.DefaultJSON == "" {
			return fmt.Errorf("%s field %q has integer bounds but is neither required nor defaulted", c.name, field.WireName)
		}
	}
	for _, variant := range c.taggedUnion.Variants {
		for _, required := range variant.Required {
			if _, ok := byWireName[required]; !ok {
				return fmt.Errorf("%s variant %q requires %q, which is not a struct field", c.name, variant.Tag, required)
			}
		}
	}
	for _, variant := range c.untaggedUnion.Variants {
		for _, required := range variant.Required {
			if _, ok := byWireName[required]; !ok {
				return fmt.Errorf("%s untagged variant requires %q, which is not a struct field", c.name, required)
			}
		}
	}
	return nil
}

// validJSONTagName mirrors encoding/json's isValidTag: a name it rejects would make the
// struct field marshal under its Go name.
func validJSONTagName(name string) bool {
	if name == "" || name == "-" {
		return false
	}
	for _, c := range name {
		switch {
		case strings.ContainsRune("!#$%&()*+-./:;<=>?@[]^_{|}~ ", c):
		case !unicode.IsLetter(c) && !unicode.IsDigit(c):
			return false
		}
	}
	return true
}

func jsonStringLiterals(values []string) []string {
	out := make([]string, 0, len(values))
	for _, value := range values {
		encoded, _ := json.Marshal(value)
		out = append(out, string(encoded))
	}
	return out
}

func goStringSlice(values []string) string {
	quoted := make([]string, 0, len(values))
	for _, value := range values {
		quoted = append(quoted, fmt.Sprintf("%q", value))
	}
	return "[]string{" + strings.Join(quoted, ", ") + "}"
}

// renderObjectSpec renders the objectSpec table of one generated object type.
func renderObjectSpec(c objectCodec) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("var %s = objectSpec{\n\tfields: []objectField{\n", objectSpecName(c.name)))
	hasAliases := false
	tagIndex := -1
	for i, field := range c.wireFields() {
		var parts []string
		parts = append(parts, fmt.Sprintf("name: %q", field.WireName))
		var flags []string
		if field.Required {
			flags = append(flags, "fieldRequired")
		}
		if field.RequiredNonNull {
			flags = append(flags, "fieldRejectNull")
		}
		if strings.HasPrefix(field.Type, "OptionalNonNull[") {
			flags = append(flags, "fieldNonNullOptional")
		}
		if field.CustomDeserialize == dynamicToolSpecsDeserializeHook {
			flags = append(flags, "fieldCustom")
		}
		if len(flags) > 0 {
			parts = append(parts, "flags: "+strings.Join(flags, " | "))
		}
		if len(field.Aliases) > 0 {
			hasAliases = true
			parts = append(parts, "aliases: "+goStringSlice(field.Aliases))
		}
		if field.DefaultJSON != "" {
			parts = append(parts, fmt.Sprintf("defaultJSON: %q", field.DefaultJSON))
			if len(field.VariantTags) > 0 {
				parts = append(parts, "defaultTags: "+goStringSlice(jsonStringLiterals(field.VariantTags)))
			}
		}
		if len(field.NestedAnyOfTags) > 0 {
			parts = append(parts, "anyOfTags: "+goStringSlice(jsonStringLiterals(field.NestedAnyOfTags)))
		}
		if aliases := kindVariantAliases(field); len(aliases) > 0 {
			parts = append(parts, "variantAliases: []variantAlias{"+strings.Join(aliases, ", ")+"}")
		}
		if field.WireName == c.taggedUnion.Discriminator {
			tagIndex = i
		}
		b.WriteString("\t\t{" + strings.Join(parts, ", ") + "},\n")
	}
	b.WriteString("\t},\n")
	if offset := c.fieldOffset(); offset != 0 {
		b.WriteString(fmt.Sprintf("\tfieldOffset: %d,\n", offset))
	}
	if c.taggedUnion.Discriminator != "" {
		tags := make([]string, 0, len(c.taggedUnion.Variants))
		for _, variant := range c.taggedUnion.Variants {
			tags = append(tags, variant.Tag)
		}
		b.WriteString(fmt.Sprintf("\ttag: %d,\n\ttags: %s,\n", tagIndex, goStringSlice(tags)))
	}
	if hasAliases {
		b.WriteString("\thasAliases: true,\n")
	}
	b.WriteString("}\n")
	return b.String()
}

// kindVariantAliases renders the serde variant aliases rewritten before decoding a
// field named kind, as JSON-encoded alias and canonical values.
func kindVariantAliases(field renderedField) []string {
	if field.WireName != "kind" {
		return nil
	}
	var out []string
	for _, variantAlias := range field.VariantAliases {
		canonical, _ := json.Marshal(variantAlias.CanonicalWireValue)
		for _, alias := range variantAlias.Aliases {
			aliasJSON, _ := json.Marshal(alias)
			out = append(out, fmt.Sprintf("{alias: %q, canonical: %q}", string(aliasJSON), string(canonical)))
		}
	}
	return out
}

func renderObjectUnmarshal(c objectCodec) string {
	var b strings.Builder
	name := c.name
	specName := objectSpecName(name)
	b.WriteString(fmt.Sprintf("func (v *%s) UnmarshalJSON(data []byte) error {\n", name))
	if len(c.stringUnionValues) > 0 {
		b.WriteString("\ttrimmed := bytes.TrimSpace(data)\n")
		b.WriteString("\tif bytes.Equal(trimmed, []byte(\"null\")) { return DecodeError{Field: \"\", Reason: \"cannot be null\"} }\n")
		b.WriteString("\tvar stringValue string\n")
		b.WriteString("\tif err := json.Unmarshal(trimmed, &stringValue); err == nil {\n")
		b.WriteString("\t\tswitch stringValue {\n")
		b.WriteString(fmt.Sprintf("\t\tcase %s:\n", quotedStringCaseList(c.stringUnionValues)))
		b.WriteString(fmt.Sprintf("\t\t\t*v = %s{StringValue: stringValue}\n", name))
		b.WriteString("\t\tdefault:\n")
		b.WriteString(fmt.Sprintf("\t\t\t*v = %s{RawJSON: append(json.RawMessage(nil), data...)}\n", name))
		b.WriteString("\t\t}\n")
		b.WriteString("\t\treturn nil\n")
		b.WriteString("\t}\n")
		b.WriteString("\tv.StringValue = \"\"\n")
	}
	b.WriteString(fmt.Sprintf("\ttype wire %s\n", name))
	var after strings.Builder
	for _, field := range c.wireFields() {
		if field.CustomDeserialize == dynamicToolSpecsDeserializeHook {
			after.WriteString(renderDynamicToolSpecsUnmarshal(field))
		}
	}
	for _, field := range c.wireFields() {
		after.WriteString(renderIntegerBoundsValidation(field))
	}
	for _, field := range c.fields {
		if !field.Flattened {
			continue
		}
		after.WriteString(fmt.Sprintf("\tv.%s = s.unknownMembers()\n", field.FieldName))
		if field.DefaultJSON != "" {
			after.WriteString(fmt.Sprintf("\tif len(v.%s) == 0 { if err := json.Unmarshal([]byte(%q), &v.%s); err != nil { return fmt.Errorf(\"field %s: %%w\", err) } }\n", field.FieldName, field.DefaultJSON, field.FieldName, field.WireName))
		}
	}
	after.WriteString(renderTaggedUnionValidation(c.taggedUnion, c.fields))
	after.WriteString(renderUntaggedUnionValidation(c.untaggedUnion))
	if after.Len() == 0 {
		b.WriteString(fmt.Sprintf("\treturn decodeObject(data, &%s, (*wire)(v))\n}\n", specName))
		return b.String()
	}
	b.WriteString("\tvar s objectScan\n")
	if c.taggedUnion.Discriminator != "" {
		discriminator := goFieldName(c.taggedUnion.Discriminator)
		for _, field := range c.fields {
			if field.WireName == c.taggedUnion.Discriminator {
				discriminator = field.FieldName
			}
		}
		b.WriteString(fmt.Sprintf("\tknown, err := s.begin(data, &%s)\n", specName))
		b.WriteString("\tif err != nil { return err }\n")
		b.WriteString(fmt.Sprintf("\tif !known {\n\t\tv.%s = s.tag\n\t\tv.RawJSON = append(v.RawJSON[:0], data...)\n\t\treturn nil\n\t}\n", discriminator))
	} else {
		b.WriteString(fmt.Sprintf("\tif _, err := s.begin(data, &%s); err != nil { return err }\n", specName))
	}
	b.WriteString("\tif err := s.finish((*wire)(v)); err != nil { return err }\n")
	b.WriteString(after.String())
	b.WriteString("\treturn nil\n}\n")
	return b.String()
}

func renderDynamicToolSpecsUnmarshal(field renderedField) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("\tif raw, ok := s.value(%q); ok {\n", field.WireName))
	b.WriteString("\t\tif bytes.Equal(raw, []byte(\"null\")) {\n")
	b.WriteString(fmt.Sprintf("\t\t\tv.%s = Null[[]DynamicToolSpec]()\n", field.FieldName))
	b.WriteString("\t\t} else {\n")
	b.WriteString("\t\t\tvalue, err := normalizeDynamicToolSpecsJSON(raw)\n")
	b.WriteString(fmt.Sprintf("\t\t\tif err != nil { return fmt.Errorf(\"field %s: %%w\", err) }\n", field.WireName))
	b.WriteString(fmt.Sprintf("\t\t\tv.%s = Some(value)\n", field.FieldName))
	b.WriteString("\t\t}\n\t}\n")
	return b.String()
}

func renderTaggedUnionValidation(union renderedTaggedUnion, fields []renderedField) string {
	if union.Discriminator == "" {
		return ""
	}
	fieldsByWireName := map[string]renderedField{}
	for _, field := range fields {
		fieldsByWireName[field.WireName] = field
	}
	var b strings.Builder
	b.WriteString("\tv.RawJSON = nil\n")
	b.WriteString(fmt.Sprintf("\tswitch v.%s {\n", goFieldName(union.Discriminator)))
	for variantIndex, variant := range union.Variants {
		b.WriteString(fmt.Sprintf("\tcase %q:\n", variant.Tag))
		for _, required := range variant.Required {
			if required == union.Discriminator {
				continue
			}
			reason := fmt.Sprintf("missing required field for %s %s", union.Discriminator, variant.Tag)
			b.WriteString(fmt.Sprintf("\t\tif !s.has(%q) { return DecodeError{Field: %q, Reason: %q} }\n", required, required, reason))
			if !variant.Nullable[required] {
				b.WriteString(fmt.Sprintf("\t\tif s.isNull(%q) { return DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", required, required))
			}
		}
		if len(variant.AnyOfAlternatives) == 0 {
			continue
		}
		matchName := fmt.Sprintf("taggedAnyOfVariant%dMatches", variantIndex)
		var alternatives []string
		for _, alternative := range variant.AnyOfAlternatives {
			var checks []string
			for _, required := range sortedStrings(alternative.Required) {
				field, ok := fieldsByWireName[required]
				if !ok {
					continue
				}
				switch {
				case isOptionalGoType(field.Type):
					checks = append(checks, fmt.Sprintf("v.%s.IsSet()", field.FieldName))
				case alternative.Nullable[required]:
					checks = append(checks, fmt.Sprintf("s.has(%q)", required))
				default:
					checks = append(checks, fmt.Sprintf("s.nonNull(%q)", required))
				}
			}
			if len(checks) == 0 {
				checks = []string{"true"}
			}
			alternatives = append(alternatives, "("+strings.Join(checks, " && ")+")")
		}
		b.WriteString(fmt.Sprintf("\t\t%s := %s\n", matchName, strings.Join(alternatives, " || ")))
		b.WriteString(fmt.Sprintf("\t\tif !%s { return DecodeError{Field: \"\", Reason: %q} }\n", matchName, fmt.Sprintf("does not match any anyOf variant for %s %s", union.Discriminator, variant.Tag)))
	}
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\tv.RawJSON = append(v.RawJSON[:0], data...)\n")
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
	for _, variant := range union.Variants {
		var checks []string
		for _, required := range sortedStrings(variant.Required) {
			if variant.Nullable[required] {
				checks = append(checks, fmt.Sprintf("s.has(%q)", required))
			} else {
				checks = append(checks, fmt.Sprintf("s.nonNull(%q)", required))
			}
		}
		var constChecks []string
		for _, fieldName := range sortedMapKeys(variant.ConstFields) {
			encoded, _ := json.Marshal(variant.ConstFields[fieldName])
			constChecks = append(constChecks, fmt.Sprintf("s.rawEquals(%q, %q)", fieldName, string(encoded)))
		}
		if len(constChecks) > 0 {
			b.WriteString(fmt.Sprintf("\tif %s { untaggedOneOfKnownVariantMatches = true }\n", strings.Join(constChecks, " && ")))
		}
		all := append(checks, constChecks...)
		if len(all) == 0 {
			all = []string{"true"}
		}
		b.WriteString(fmt.Sprintf("\tif %s { untaggedOneOfMatches++ }\n", strings.Join(all, " && ")))
	}
	b.WriteString("\tif untaggedOneOfMatches == 0 {\n")
	b.WriteString("\t\tif !untaggedOneOfKnownVariantMatches { v.RawJSON = append(v.RawJSON[:0], data...); return nil }\n")
	b.WriteString("\t\treturn DecodeError{Field: \"\", Reason: \"does not match any oneOf variant\"}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif untaggedOneOfMatches > 1 { return DecodeError{Field: \"\", Reason: \"matches multiple oneOf variants\"} }\n")
	b.WriteString("\tv.RawJSON = nil\n")
	return b.String()
}

// renderObjectMarshal marshals a method-free alias of the struct: the field tags omit
// unset optionals and empty values, and the few serde skip rules a tag cannot express
// normalize a copy first.
func renderObjectMarshal(c objectCodec) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("func (v %s) MarshalJSON() ([]byte, error) {\n", c.name))
	var normalize []string
	for _, field := range c.wireFields() {
		if field.Type == "json.RawMessage" && field.RequiredNonNull {
			b.WriteString(fmt.Sprintf("\tif bytes.Equal(v.%s, []byte(\"null\")) { return nil, DecodeError{Field: %q, Reason: \"cannot be null\"} }\n", field.FieldName, field.WireName))
		}
		if line := marshalNormalization(field); line != "" {
			normalize = append(normalize, line)
		}
	}
	if !c.hasNestedAnyOf() {
		b.WriteString(renderTaggedUnionMarshalValidation(c.taggedUnion, c.fields))
	}
	b.WriteString(fmt.Sprintf("\ttype wire %s\n", c.name))
	if len(normalize) == 0 && !c.hasNestedAnyOf() {
		b.WriteString("\treturn json.Marshal(wire(v))\n}\n")
		return b.String()
	}
	b.WriteString("\tw := wire(v)\n")
	for _, line := range normalize {
		b.WriteString(line)
	}
	if c.hasNestedAnyOf() {
		b.WriteString(renderTaggedUnionAliasMarshalValidation(c.taggedUnion, c.fields))
	}
	b.WriteString("\treturn json.Marshal(w)\n}\n")
	return b.String()
}

// renderTaggedUnionAliasMarshalValidation is renderTaggedUnionMarshalValidation for a
// tagged union with nested anyOf alternatives marshaled through the normalized alias w:
// an alternative's field matches when w emits it and it does not marshal to null, which
// is what the map marshaler checked on the map.
func renderTaggedUnionAliasMarshalValidation(union renderedTaggedUnion, fields []renderedField) string {
	fieldsByWireName := map[string]renderedField{}
	for _, field := range fields {
		fieldsByWireName[field.WireName] = field
	}
	var b strings.Builder
	b.WriteString(fmt.Sprintf("\tswitch v.%s {\n", goFieldName(union.Discriminator)))
	for variantIndex, variant := range union.Variants {
		b.WriteString(fmt.Sprintf("\tcase %q:\n", variant.Tag))
		for _, required := range variant.Required {
			if required == union.Discriminator {
				continue
			}
			field, ok := fieldsByWireName[required]
			if !ok {
				continue
			}
			b.WriteString(renderMarshalRequiredFieldCheck(field, union.Discriminator, variant.Tag, variant.Nullable[required]))
		}
		if len(variant.AnyOfAlternatives) == 0 {
			continue
		}
		var alternatives []string
		for _, alternative := range variant.AnyOfAlternatives {
			var checks []string
			for _, required := range sortedStrings(alternative.Required) {
				field, ok := fieldsByWireName[required]
				if !ok || field.Flattened {
					checks = append(checks, "false")
					continue
				}
				if emitted := emittedExpr(field); emitted != "true" {
					checks = append(checks, emitted)
				}
				checks = append(checks, fmt.Sprintf("marshalsNonNull(w.%s)", field.FieldName))
			}
			if len(checks) == 0 {
				checks = []string{"true"}
			}
			alternatives = append(alternatives, "("+strings.Join(checks, " && ")+")")
		}
		matchName := fmt.Sprintf("taggedAnyOfVariant%dMatches", variantIndex)
		b.WriteString(fmt.Sprintf("\t\t%s := %s\n", matchName, strings.Join(alternatives, " || ")))
		b.WriteString(fmt.Sprintf("\t\tif !%s { return nil, DecodeError{Field: \"\", Reason: %q} }\n", matchName, fmt.Sprintf("does not match any anyOf variant for %s %s", union.Discriminator, variant.Tag)))
	}
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\tif len(v.RawJSON) > 0 { return append([]byte(nil), v.RawJSON...), nil }\n")
	b.WriteString(fmt.Sprintf("\t\treturn nil, DecodeError{Field: %q, Reason: fmt.Sprintf(\"unsupported discriminator value %%q\", v.%s)}\n", union.Discriminator, goFieldName(union.Discriminator)))
	b.WriteString("\t}\n")
	return b.String()
}

// emittedExpr is the Go condition under which json.Marshal emits a field of the
// normalized alias w, following wireTag.
func emittedExpr(field renderedField) string {
	tag := wireTag(field)
	switch {
	case strings.Contains(tag, ",omitzero"):
		return fmt.Sprintf("w.%s.IsSet()", field.FieldName)
	case strings.Contains(tag, ",omitempty") && field.Type == "bool":
		return "w." + field.FieldName
	case strings.Contains(tag, ",omitempty"):
		return fmt.Sprintf("len(w.%s) > 0", field.FieldName)
	default:
		return "true"
	}
}

// marshalNormalization renders the statement that makes an optional field's zero value
// match its serde skip rule before the alias is marshaled.
func marshalNormalization(field renderedField) string {
	nullable := strings.HasPrefix(field.Type, "Optional[")
	switch {
	case !isOptionalGoType(field.Type):
		return ""
	case field.SkipIf == skipSerializingIfNotNot:
		if nullable {
			return fmt.Sprintf("\tw.%s = optionalWithoutFalse(w.%s)\n", field.FieldName, field.FieldName)
		}
		return fmt.Sprintf("\tw.%s = nonNullWithoutFalse(w.%s)\n", field.FieldName, field.FieldName)
	case field.SkipIf == skipSerializingIfOptionNone && field.CustomSerialize != doubleOptionSerializeHook:
		if nullable {
			return fmt.Sprintf("\tw.%s = w.%s.withoutNull()\n", field.FieldName, field.FieldName)
		}
		return ""
	case field.SkipIf == skipSerializingIfVecEmpty:
		if nullable {
			return fmt.Sprintf("\tw.%s = optionalWithoutEmpty(w.%s)\n", field.FieldName, field.FieldName)
		}
		return fmt.Sprintf("\tw.%s = nonNullWithoutEmpty(w.%s)\n", field.FieldName, field.FieldName)
	default:
		return ""
	}
}
