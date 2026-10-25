//go:build codecdiff

package protocodex

// Differential check of the generated protocol codecs against the previous generated
// package (specs/go-sdk-decode-perf-2026-10-25.md, UC6). scripts/fork/go-codec-diff.sh
// extracts that package into legacyprotocol and writes codecDiffTypes; the tests here
// decode the same bytes with both packages and require the same outcome, the same value
// (including unexported Optional state) and the same marshaled bytes, and the same error
// text for a single fault in otherwise valid JSON.

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"math/rand/v2"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
	"unicode/utf8"
	"unsafe"

	"github.com/openai/codex/sdk/go/internal/protocodex/legacyprotocol"
	"github.com/openai/codex/sdk/go/protocol"
)

type codecDiffType struct {
	name   string
	fresh  func() any
	legacy func() any
}

// codecDiffStats counts what a run compared; mismatches fail the test.
type codecDiffStats struct {
	instances, mutations, accepted, rejected, errorTexts, marshals, marshalErrors, reuses, notifications int
}

func codecDiffSchemas(t testing.TB) (*SchemaBundle, map[string]Schema) {
	t.Helper()
	bundle, err := LoadSchemaBundle("schema-experimental")
	if err != nil {
		t.Fatal(err)
	}
	byName := map[string]Schema{}
	names := definitionNameMap(bundle)
	for _, key := range sortedDefinitionKeys(bundle) {
		if _, ok := byName[names[key]]; !ok {
			byName[names[key]] = bundle.Definitions[key]
		}
	}
	return bundle, byName
}

func codecDiffInstances() int {
	if value, err := strconv.Atoi(os.Getenv("CODEC_DIFF_INSTANCES")); err == nil && value > 0 {
		return value
	}
	return 12
}

func TestCodecMatchesLegacy(t *testing.T) {
	bundle, byName := codecDiffSchemas(t)
	var stats codecDiffStats
	withoutSchema := 0
	for index, typ := range codecDiffTypes {
		schema, ok := byName[typ.name]
		if !ok {
			withoutSchema++
		}
		rng := rand.New(rand.NewPCG(uint64(index), 0x5eed))
		gen := &instanceGen{bundle: bundle, rng: rng}
		for range codecDiffInstances() {
			var doc *jnode
			if ok {
				doc = gen.node(schema, 0)
			} else {
				doc = gen.anyValue(0)
			}
			stats.instances++
			input := doc.encode()
			valid := compareCodecs(t, typ, input, false, rng, &stats)
			for range 6 {
				mutated, single := mutateDocument(doc.clone(), rng, gen)
				stats.mutations++
				compareCodecs(t, typ, mutated, valid && single && json.Valid(mutated), rng, &stats)
				compareReuse(t, typ, input, mutated, &stats)
			}
		}
		if t.Failed() {
			t.Fatalf("stopping after the first mismatching type %s", typ.name)
		}
	}
	literals := protocolTestLiterals(t)
	for index, literal := range literals {
		rng := rand.New(rand.NewPCG(uint64(index), 0x11e))
		gen := &instanceGen{bundle: bundle, rng: rng}
		doc, err := parseJNode(literal)
		if err != nil {
			t.Fatalf("parse %q: %v", literal, err)
		}
		for _, typ := range codecDiffTypes {
			stats.instances++
			if !compareCodecs(t, typ, literal, false, rng, &stats) {
				continue
			}
			for range 6 {
				mutated, single := mutateDocument(doc.clone(), rng, gen)
				stats.mutations++
				compareCodecs(t, typ, mutated, single && json.Valid(mutated), rng, &stats)
			}
		}
	}
	t.Logf("%d JSON literals from the protocol tests decoded as every type", len(literals))
	compareNotificationDecoders(t, bundle, byName, &stats)
	t.Logf("legacy %s: %d types (%d without a schema definition), %d instances, %d mutations, %d accepted, %d rejected, %d single-fault error texts, %d marshals (%d errors), %d reused decodes, %d notification decodes",
		codecDiffLegacyRev, len(codecDiffTypes), withoutSchema, stats.instances, stats.mutations, stats.accepted, stats.rejected, stats.errorTexts, stats.marshals, stats.marshalErrors, stats.reuses, stats.notifications)
}

func FuzzCodecMatchesLegacy(f *testing.F) {
	bundle, byName := codecDiffSchemas(f)
	for index, typ := range codecDiffTypes {
		schema, ok := byName[typ.name]
		if !ok {
			continue
		}
		gen := &instanceGen{bundle: bundle, rng: rand.New(rand.NewPCG(uint64(index), 1))}
		f.Add(uint16(index), gen.node(schema, 0).encode())
	}
	f.Fuzz(func(t *testing.T, index uint16, data []byte) {
		typ := codecDiffTypes[int(index)%len(codecDiffTypes)]
		var stats codecDiffStats
		compareCodecs(t, typ, data, false, rand.New(rand.NewPCG(uint64(len(data)), 2)), &stats)
	})
}

// compareCodecs decodes data with both packages, through json.Unmarshal and through the
// UnmarshalJSON method, and compares outcomes, values, marshaled bytes and (when the
// caller knows data has exactly one fault) error texts. It reports whether the legacy
// decoder accepted data.
func compareCodecs(t *testing.T, typ codecDiffType, data []byte, singleFault bool, rng *rand.Rand, stats *codecDiffStats) bool {
	t.Helper()
	fresh, legacy := typ.fresh(), typ.legacy()
	freshErr := json.Unmarshal(data, fresh)
	legacyErr := json.Unmarshal(data, legacy)
	if (freshErr == nil) != (legacyErr == nil) {
		t.Errorf("%s: json.Unmarshal(%q): new error %v, legacy error %v", typ.name, clip(data), freshErr, legacyErr)
		return legacyErr == nil
	}
	if legacyErr != nil {
		stats.rejected++
		if singleFault {
			stats.errorTexts++
			if freshErr.Error() != legacyText(legacyErr) {
				t.Errorf("%s: single-fault error text for %q:\n new:    %v\n legacy: %v", typ.name, clip(data), freshErr, legacyErr)
			}
		}
	} else {
		stats.accepted++
		if path, ok := equalValues(reflect.ValueOf(fresh).Elem(), reflect.ValueOf(legacy).Elem(), typ.name); !ok {
			t.Errorf("%s: decoded values differ at %s for %q", typ.name, path, clip(data))
			return true
		}
		compareMarshal(t, typ.name, fresh, legacy, data, stats)
		for range 3 {
			if !perturbLockstep(reflect.ValueOf(fresh).Elem(), reflect.ValueOf(legacy).Elem(), rng) {
				break
			}
			if path, ok := equalValues(reflect.ValueOf(fresh).Elem(), reflect.ValueOf(legacy).Elem(), typ.name); !ok {
				t.Fatalf("%s: lockstep perturbation diverged at %s", typ.name, path)
			}
			compareMarshal(t, typ.name, fresh, legacy, data, stats)
		}
	}
	directFresh, directLegacy := typ.fresh().(json.Unmarshaler), typ.legacy().(json.Unmarshaler)
	directFreshErr := directFresh.UnmarshalJSON(data)
	directLegacyErr := directLegacy.UnmarshalJSON(data)
	if (directFreshErr == nil) != (directLegacyErr == nil) {
		t.Errorf("%s: UnmarshalJSON(%q): new error %v, legacy error %v", typ.name, clip(data), directFreshErr, directLegacyErr)
	} else if directLegacyErr == nil {
		if path, ok := equalValues(reflect.ValueOf(directFresh).Elem(), reflect.ValueOf(directLegacy).Elem(), typ.name); !ok {
			t.Errorf("%s: UnmarshalJSON values differ at %s for %q", typ.name, path, clip(data))
		}
	} else if singleFault && directFreshErr.Error() != legacyText(directLegacyErr) {
		t.Errorf("%s: UnmarshalJSON single-fault error text for %q:\n new:    %v\n legacy: %v", typ.name, clip(data), directFreshErr, directLegacyErr)
	}
	return legacyErr == nil
}

// compareReuse decodes second into values that already hold first, as a caller reusing
// a variable does: fields second lacks keep what first decoded.
func compareReuse(t *testing.T, typ codecDiffType, first, second []byte, stats *codecDiffStats) {
	t.Helper()
	fresh, legacy := typ.fresh(), typ.legacy()
	if json.Unmarshal(first, fresh) != nil || json.Unmarshal(first, legacy) != nil {
		return
	}
	stats.reuses++
	freshErr := json.Unmarshal(second, fresh)
	legacyErr := json.Unmarshal(second, legacy)
	if (freshErr == nil) != (legacyErr == nil) {
		t.Errorf("%s: reused decode of %q: new error %v, legacy error %v", typ.name, clip(second), freshErr, legacyErr)
		return
	}
	if legacyErr == nil {
		if path, ok := equalValues(reflect.ValueOf(fresh).Elem(), reflect.ValueOf(legacy).Elem(), typ.name); !ok {
			t.Errorf("%s: reused decode values differ at %s: %q then %q", typ.name, path, clip(first), clip(second))
		}
	}
}

func compareMarshal(t *testing.T, name string, fresh, legacy any, input []byte, stats *codecDiffStats) {
	t.Helper()
	stats.marshals++
	freshOut, freshErr := json.Marshal(fresh)
	legacyOut, legacyErr := json.Marshal(legacy)
	if (freshErr == nil) != (legacyErr == nil) {
		t.Errorf("%s: marshal error new %v, legacy %v (decoded from %q)", name, freshErr, legacyErr, clip(input))
		return
	}
	if legacyErr != nil {
		stats.marshalErrors++
		return
	}
	if !bytes.Equal(freshOut, legacyOut) {
		t.Errorf("%s: marshal differs (decoded from %q):\n new:    %s\n legacy: %s", name, clip(input), clip(freshOut), clip(legacyOut))
	}
}

// compareNotificationDecoders compares DecodeServerNotificationPayload of both packages
// for every notification method, on payload instances and their mutations, including
// whitespace around the payload that json.Unmarshal does and does not accept.
func compareNotificationDecoders(t *testing.T, bundle *SchemaBundle, byName map[string]Schema, stats *codecDiffStats) {
	manifest, err := LoadManifest(testManifestPath(t))
	if err != nil {
		t.Fatal(err)
	}
	for index, entry := range manifest.Experimental.ServerNotifications {
		if entry.PayloadType == "" {
			continue
		}
		rng := rand.New(rand.NewPCG(uint64(index), 0xfeed))
		gen := &instanceGen{bundle: bundle, rng: rng}
		schema, ok := byName[typeNameForDefinition(entry.PayloadType)]
		for range 6 {
			var doc *jnode
			if ok {
				doc = gen.node(schema, 0)
			} else {
				doc = gen.anyValue(0)
			}
			encoded := doc.encode()
			_, baseErr := legacyprotocol.DecodeServerNotificationPayload(entry.Method, encoded)
			inputs := [][]byte{encoded}
			singleFault := map[int]bool{}
			for range 4 {
				mutated, single := mutateDocument(doc.clone(), rng, gen)
				singleFault[len(inputs)] = baseErr == nil && single && json.Valid(mutated)
				inputs = append(inputs, mutated)
			}
			inputs = append(inputs, append([]byte(" \t\n"), encoded...), append(append([]byte("\v"), encoded...), '\f'), append(encoded, "\u00a0"...), nil)
			for inputIndex, input := range inputs {
				stats.notifications++
				freshValue, freshErr := protocol.DecodeServerNotificationPayload(entry.Method, input)
				legacyValue, legacyErr := legacyprotocol.DecodeServerNotificationPayload(entry.Method, input)
				if (freshErr == nil) != (legacyErr == nil) {
					t.Errorf("%s: DecodeServerNotificationPayload(%q): new error %v, legacy error %v", entry.Method, clip(input), freshErr, legacyErr)
					continue
				}
				if legacyErr != nil {
					if singleFault[inputIndex] && freshErr.Error() != legacyText(legacyErr) {
						t.Errorf("%s: error text for %q:\n new:    %v\n legacy: %v", entry.Method, clip(input), freshErr, legacyErr)
					}
					continue
				}
				if path, ok := equalValues(reflect.ValueOf(freshValue), reflect.ValueOf(legacyValue), entry.Method); !ok {
					t.Errorf("%s: decoded payloads differ at %s for %q", entry.Method, path, clip(input))
				}
			}
		}
	}
}

// protocolTestLiterals collects the JSON literals of the protocol package tests: they
// exercise serde aliases, variant aliases, legacy dynamic tools, flattened fields and
// defaults that schema instances do not produce.
func protocolTestLiterals(t *testing.T) [][]byte {
	t.Helper()
	paths, err := filepath.Glob(filepath.Join("..", "..", "protocol", "*_test.go"))
	if err != nil || len(paths) == 0 {
		t.Fatalf("protocol test files: %v", err)
	}
	seen := map[string]bool{}
	var out [][]byte
	for _, path := range paths {
		source, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		for _, match := range regexp.MustCompile("`([^`]*)`").FindAllSubmatch(source, -1) {
			literal := bytes.TrimSpace(match[1])
			if len(literal) == 0 || !json.Valid(literal) || seen[string(literal)] {
				continue
			}
			seen[string(literal)] = true
			out = append(out, literal)
		}
	}
	return out
}

// parseJNode parses JSON into a jnode, keeping member order and duplicate keys.
func parseJNode(data []byte) (*jnode, error) {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	return parseJNodeValue(decoder)
}

func parseJNodeValue(decoder *json.Decoder) (*jnode, error) {
	token, err := decoder.Token()
	if err != nil {
		return nil, err
	}
	switch value := token.(type) {
	case json.Delim:
		if value == '{' {
			out := &jnode{kind: 'o'}
			for decoder.More() {
				key, err := decoder.Token()
				if err != nil {
					return nil, err
				}
				member, err := parseJNodeValue(decoder)
				if err != nil {
					return nil, err
				}
				out.members = append(out.members, jmember{key: key.(string), value: member})
			}
			_, err := decoder.Token()
			return out, err
		}
		out := &jnode{kind: 'a'}
		for decoder.More() {
			item, err := parseJNodeValue(decoder)
			if err != nil {
				return nil, err
			}
			out.items = append(out.items, item)
		}
		_, err := decoder.Token()
		return out, err
	case string:
		return rawNode(jsonQuote(value)), nil
	case json.Number:
		return rawNode(value.String()), nil
	case bool:
		return rawNode(strconv.FormatBool(value)), nil
	default:
		return rawNode("null"), nil
	}
}

// legacyText is the legacy error text with the package name of the new package, since
// encoding/json names the Go type in decode errors.
func legacyText(err error) string {
	return strings.ReplaceAll(err.Error(), "legacyprotocol.", "protocol.")
}

func clip(data []byte) string {
	if len(data) > 600 {
		return string(data[:600]) + "…"
	}
	return string(data)
}

// equalValues compares two values of structurally identical types from the two packages,
// unexported fields included.
func equalValues(a, b reflect.Value, path string) (string, bool) {
	if a.Kind() != b.Kind() {
		return path + " (kind)", false
	}
	switch a.Kind() {
	case reflect.Struct:
		if a.NumField() != b.NumField() {
			return path + " (field count)", false
		}
		for i := range a.NumField() {
			name := a.Type().Field(i).Name
			if name != b.Type().Field(i).Name {
				return path + "." + name + " (field name)", false
			}
			if p, ok := equalValues(a.Field(i), b.Field(i), path+"."+name); !ok {
				return p, false
			}
		}
		return "", true
	case reflect.Pointer, reflect.Interface:
		if a.IsNil() || b.IsNil() {
			return path + " (nil)", a.IsNil() == b.IsNil()
		}
		return equalValues(a.Elem(), b.Elem(), path)
	case reflect.Slice:
		if a.IsNil() != b.IsNil() {
			return path + " (nil slice)", false
		}
		if a.Len() != b.Len() {
			return path + " (len)", false
		}
		if a.Type().Elem().Kind() == reflect.Uint8 {
			return path, bytes.Equal(a.Bytes(), b.Bytes())
		}
		for i := range a.Len() {
			if p, ok := equalValues(a.Index(i), b.Index(i), fmt.Sprintf("%s[%d]", path, i)); !ok {
				return p, false
			}
		}
		return "", true
	case reflect.Map:
		if a.IsNil() != b.IsNil() {
			return path + " (nil map)", false
		}
		if a.Len() != b.Len() {
			return path + " (map len)", false
		}
		other := map[string]reflect.Value{}
		for iter := b.MapRange(); iter.Next(); {
			other[iter.Key().String()] = iter.Value()
		}
		for iter := a.MapRange(); iter.Next(); {
			key := iter.Key().String()
			value, ok := other[key]
			if !ok {
				return path + "[" + key + "] (missing key)", false
			}
			if p, ok := equalValues(iter.Value(), value, path+"["+key+"]"); !ok {
				return p, false
			}
		}
		return "", true
	case reflect.String:
		return path, a.String() == b.String()
	case reflect.Bool:
		return path, a.Bool() == b.Bool()
	case reflect.Int, reflect.Int8, reflect.Int16, reflect.Int32, reflect.Int64:
		return path, a.Int() == b.Int()
	case reflect.Uint, reflect.Uint8, reflect.Uint16, reflect.Uint32, reflect.Uint64:
		return path, a.Uint() == b.Uint()
	case reflect.Float32, reflect.Float64:
		return path, math.Float64bits(a.Float()) == math.Float64bits(b.Float())
	case reflect.Invalid:
		return path, !b.IsValid()
	default:
		return path + " (unsupported kind " + a.Kind().String() + ")", false
	}
}

// perturbLockstep applies one random change to the same place of both values: an
// Optional becomes unset, null or set, a raw message or string gets a new value, a slice
// or map becomes nil or empty. It reports whether it found a place to change.
func perturbLockstep(a, b reflect.Value, rng *rand.Rand) bool {
	var slots [][2]reflect.Value
	collectSlots(a, b, &slots, 0)
	if len(slots) == 0 {
		return false
	}
	slot := slots[rng.IntN(len(slots))]
	choice := rng.IntN(1 << 16)
	applyPerturbation(writable(slot[0]), choice)
	applyPerturbation(writable(slot[1]), choice)
	return true
}

func collectSlots(a, b reflect.Value, slots *[][2]reflect.Value, depth int) {
	if depth > 8 || !a.CanAddr() {
		return
	}
	*slots = append(*slots, [2]reflect.Value{a, b})
	switch a.Kind() {
	case reflect.Struct:
		if isOptionalType(a.Type()) {
			if a.FieldByName("set").Bool() {
				collectSlots(a.FieldByName("value"), b.FieldByName("value"), slots, depth+1)
			}
			return
		}
		for i := range a.NumField() {
			collectSlots(a.Field(i), b.Field(i), slots, depth+1)
		}
	case reflect.Slice:
		if a.Type().Elem().Kind() == reflect.Uint8 {
			return
		}
		for i := range a.Len() {
			collectSlots(a.Index(i), b.Index(i), slots, depth+1)
		}
	case reflect.Pointer:
		if !a.IsNil() {
			collectSlots(a.Elem(), b.Elem(), slots, depth+1)
		}
	}
}

func isOptionalType(typ reflect.Type) bool {
	return typ.Kind() == reflect.Struct && (strings.HasPrefix(typ.Name(), "Optional[") || strings.HasPrefix(typ.Name(), "OptionalNonNull["))
}

func writable(v reflect.Value) reflect.Value {
	return reflect.NewAt(v.Type(), unsafe.Pointer(v.UnsafeAddr())).Elem()
}

var perturbRawMessages = []string{"", "null", " null ", "{}", "[]", `"x"`, "1", `{"a":[1,{"b":null}]}`, "{", "true"}

var perturbStrings = []string{"", "x", "agentMessage", "imageGeneration", "commandExecution", "image", "text", "unknown", "never", "granular", "<tag>&amp;", "névé", "\u2028", "\x00", "\xff", strings.Repeat("long ", 30)}

func applyPerturbation(v reflect.Value, choice int) {
	switch {
	case isOptionalType(v.Type()):
		set := writable(v.FieldByName("set"))
		null := v.FieldByName("null")
		switch choice % 4 {
		case 0:
			v.SetZero()
		case 1:
			if null.IsValid() {
				writable(v.FieldByName("value")).SetZero()
				set.SetBool(true)
				writable(null).SetBool(true)
			} else {
				v.SetZero()
			}
		default:
			set.SetBool(true)
			if null.IsValid() {
				writable(null).SetBool(false)
			}
		}
	case v.Kind() == reflect.Slice && v.Type().Elem().Kind() == reflect.Uint8:
		raw := perturbRawMessages[choice%len(perturbRawMessages)]
		if raw == "" && choice%3 == 0 {
			v.SetZero()
		} else {
			v.SetBytes([]byte(raw))
		}
	case v.Kind() == reflect.String:
		v.SetString(perturbStrings[choice%len(perturbStrings)])
	case v.Kind() == reflect.Bool:
		v.SetBool(!v.Bool())
	case v.Kind() >= reflect.Int && v.Kind() <= reflect.Int64:
		values := []int64{0, -1, 1, math.MaxInt32, math.MinInt32, math.MaxInt64}
		v.SetInt(values[choice%len(values)])
	case v.Kind() >= reflect.Uint && v.Kind() <= reflect.Uint64:
		values := []uint64{0, 1, math.MaxUint32, math.MaxUint64}
		v.SetUint(values[choice%len(values)])
	case v.Kind() == reflect.Float64 || v.Kind() == reflect.Float32:
		values := []float64{0, -1.5, 1e300, math.NaN(), math.Inf(1)}
		v.SetFloat(values[choice%len(values)])
	case v.Kind() == reflect.Slice || v.Kind() == reflect.Map:
		if choice%2 == 0 {
			v.SetZero()
		} else if v.Kind() == reflect.Slice {
			v.Set(reflect.MakeSlice(v.Type(), 0, 0))
		} else {
			v.Set(reflect.MakeMap(v.Type()))
		}
	case v.Kind() == reflect.Pointer:
		v.SetZero()
	case v.Kind() == reflect.Struct && choice%5 == 0:
		v.SetZero()
	}
}

// jnode is an ordered JSON document, so that mutations can duplicate keys, change their
// spelling and keep the order they were written in.
type jnode struct {
	kind    byte // 'o' object, 'a' array, 'r' raw literal
	members []jmember
	items   []*jnode
	raw     string
}

type jmember struct {
	key        string
	encodedKey string // overrides the JSON encoding of key when set
	value      *jnode
}

func rawNode(raw string) *jnode { return &jnode{kind: 'r', raw: raw} }

func (n *jnode) clone() *jnode {
	out := &jnode{kind: n.kind, raw: n.raw}
	for _, member := range n.members {
		out.members = append(out.members, jmember{key: member.key, encodedKey: member.encodedKey, value: member.value.clone()})
	}
	for _, item := range n.items {
		out.items = append(out.items, item.clone())
	}
	return out
}

func (n *jnode) encode() []byte {
	var b bytes.Buffer
	n.write(&b)
	return b.Bytes()
}

func (n *jnode) write(b *bytes.Buffer) {
	switch n.kind {
	case 'o':
		b.WriteByte('{')
		for i, member := range n.members {
			if i > 0 {
				b.WriteByte(',')
			}
			if member.encodedKey != "" {
				b.WriteString(member.encodedKey)
			} else {
				b.WriteString(jsonQuote(member.key))
			}
			b.WriteByte(':')
			member.value.write(b)
		}
		b.WriteByte('}')
	case 'a':
		b.WriteByte('[')
		for i, item := range n.items {
			if i > 0 {
				b.WriteByte(',')
			}
			item.write(b)
		}
		b.WriteByte(']')
	default:
		b.WriteString(n.raw)
	}
}

func jsonQuote(s string) string {
	encoded, _ := json.Marshal(s)
	return string(encoded)
}

// instanceGen builds JSON instances of a schema: every union branch, optional fields
// present or absent, nullable fields sometimes null, integers inside their bounds.
type instanceGen struct {
	bundle *SchemaBundle
	rng    *rand.Rand
}

var instanceStrings = []string{"", "x", "thread-1", "a \"quoted\" \\ value", "tab\tnew\nline", "névé ❄", "<tag>&", "\u2028", "😀", "unknown"}

func (g *instanceGen) node(schema Schema, depth int) *jnode {
	if depth > 7 {
		return rawNode("null")
	}
	if schema.BooleanSchema != nil {
		return g.anyValue(depth)
	}
	if ref, ok := schema.SingleRef(); ok {
		definition, found := g.bundle.Definition(definitionNameForRef(ref))
		if !found {
			return g.anyValue(depth)
		}
		return g.node(definition, depth+1)
	}
	if len(schema.Enum) > 0 {
		return rawNode(string(schema.Enum[g.rng.IntN(len(schema.Enum))]))
	}
	if len(schema.OneOf) > 0 {
		return g.node(schema.OneOf[g.rng.IntN(len(schema.OneOf))], depth+1)
	}
	if len(schema.AnyOf) > 0 && len(schema.Properties) == 0 {
		return g.node(schema.AnyOf[g.rng.IntN(len(schema.AnyOf))], depth+1)
	}
	if len(schema.AllOf) == 1 {
		return g.node(schema.AllOf[0], depth+1)
	}
	types := schema.Types
	if schema.Type != "" {
		types = []string{schema.Type}
	}
	if len(types) == 0 {
		if len(schema.Properties) > 0 || schema.AdditionalProperties != nil {
			types = []string{"object"}
		} else {
			return g.anyValue(depth)
		}
	}
	typ := types[g.rng.IntN(len(types))]
	if hasType(types, "null") && len(types) > 1 && typ == "null" && g.rng.IntN(3) != 0 {
		typ = nonNullType(types)
	}
	switch typ {
	case "null":
		return rawNode("null")
	case "string":
		return rawNode(jsonQuote(instanceStrings[g.rng.IntN(len(instanceStrings))]))
	case "boolean":
		return rawNode(strconv.FormatBool(g.rng.IntN(2) == 0))
	case "integer":
		return rawNode(g.integer(schema))
	case "number":
		return rawNode([]string{"0", "1.5", "-2", "1e3", "12345.678"}[g.rng.IntN(5)])
	case "array":
		out := &jnode{kind: 'a'}
		for range g.rng.IntN(3) {
			if schema.Items != nil {
				out.items = append(out.items, g.node(*schema.Items, depth+1))
			} else {
				out.items = append(out.items, g.anyValue(depth+1))
			}
		}
		return out
	default:
		return g.object(schema, depth)
	}
}

func (g *instanceGen) object(schema Schema, depth int) *jnode {
	out := &jnode{kind: 'o'}
	required := map[string]bool{}
	for _, name := range schema.Required {
		required[name] = true
	}
	names := make([]string, 0, len(schema.Properties))
	for name := range schema.Properties {
		names = append(names, name)
	}
	sort.Strings(names)
	g.rng.Shuffle(len(names), func(i, j int) { names[i], names[j] = names[j], names[i] })
	for _, name := range names {
		if !required[name] && g.rng.IntN(5) < 2 {
			continue
		}
		out.members = append(out.members, jmember{key: name, value: g.node(schema.Properties[name], depth+1)})
	}
	for _, alternative := range schema.AnyOf {
		if g.rng.IntN(2) == 0 {
			for _, name := range alternative.Required {
				if property, ok := alternative.Properties[name]; ok {
					out.members = append(out.members, jmember{key: name, value: g.node(property, depth+1)})
				}
			}
		}
	}
	if schema.AdditionalProperties != nil && len(schema.Properties) == 0 {
		for i := range g.rng.IntN(3) {
			out.members = append(out.members, jmember{key: fmt.Sprintf("key%d", i), value: g.node(*schema.AdditionalProperties, depth+1)})
		}
	}
	return out
}

func (g *instanceGen) integer(schema Schema) string {
	low, high := int64(-1000), int64(100000)
	switch schema.Format {
	case "uint", "uint64", "uint32", "uint16", "uint8":
		low = 0
	}
	if schema.Minimum != nil {
		if value, err := schema.Minimum.Int64(); err == nil {
			low = value
		}
	}
	if schema.Maximum != nil {
		if value, err := schema.Maximum.Int64(); err == nil && value >= low {
			high = value
		}
	}
	switch g.rng.IntN(4) {
	case 0:
		return strconv.FormatInt(low, 10)
	case 1:
		return strconv.FormatInt(high, 10)
	default:
		span := high - low
		if span <= 0 || span > 1<<40 {
			span = 1 << 20
		}
		return strconv.FormatInt(low+g.rng.Int64N(span+1), 10)
	}
}

func (g *instanceGen) anyValue(depth int) *jnode {
	switch g.rng.IntN(6) {
	case 0:
		return rawNode("null")
	case 1:
		return rawNode(jsonQuote(instanceStrings[g.rng.IntN(len(instanceStrings))]))
	case 2:
		return rawNode("42")
	case 3:
		return rawNode("true")
	case 4:
		if depth < 4 {
			return &jnode{kind: 'a', items: []*jnode{g.anyValue(depth + 1)}}
		}
		return rawNode("[]")
	default:
		if depth < 4 {
			return &jnode{kind: 'o', members: []jmember{{key: "k", value: g.anyValue(depth + 1)}}}
		}
		return rawNode("{}")
	}
}

// mutateDocument applies one mutation to a random object of doc, and sometimes a second
// one, and reports whether the result has a single deliberate change (so a rejection has
// a single fault).
func mutateDocument(doc *jnode, rng *rand.Rand, gen *instanceGen) ([]byte, bool) {
	if rng.IntN(4) == 0 {
		applyMutation(doc, rng, gen)
		data, _ := applyMutation(doc, rng, gen)
		return data, false
	}
	return applyMutation(doc, rng, gen)
}

func applyMutation(doc *jnode, rng *rand.Rand, gen *instanceGen) ([]byte, bool) {
	var objects []*jnode
	collectObjects(doc, &objects)
	switch op := rng.IntN(20); {
	case op == 0:
		return append(append([]byte(" \n\t"), doc.encode()...), " \r\n"...), true
	case op == 1:
		encoded := doc.encode()
		return encoded[:rng.IntN(len(encoded)+1)], false
	case op == 2:
		return []byte([]string{"null", "[]", `"text"`, "1", "{}", ""}[rng.IntN(6)]), true
	case op == 3:
		return append([]byte("\v"), doc.encode()...), false
	case len(objects) == 0:
		return doc.encode(), true
	}
	object := objects[rng.IntN(len(objects))]
	if len(object.members) == 0 {
		object.members = append(object.members, jmember{key: "unknownField", value: gen.anyValue(3)})
		return doc.encode(), true
	}
	at := rng.IntN(len(object.members))
	member := &object.members[at]
	switch rng.IntN(15) {
	case 13:
		member.value = rawNode([]string{"tru", "nul", "1.", "--1", "01"}[rng.IntN(5)])
	case 14:
		member.value = rawNode([]string{`"bad \q escape"`, `"\u12"`, "\"raw\ttab\"", `"\ud800"`}[rng.IntN(4)])
	case 0:
		object.members = append(object.members[:at], object.members[at+1:]...)
	case 1:
		member.value = rawNode("null")
	case 2:
		member.value = typeChanged(member.value, rng)
	case 3:
		duplicate := jmember{key: member.key, value: gen.anyValue(3)}
		if rng.IntN(2) == 0 {
			object.members = append(object.members, duplicate)
		} else {
			object.members = append([]jmember{duplicate}, object.members...)
		}
	case 4:
		member.key = strings.ToUpper(member.key)
	case 5:
		member.key = snakeCase(member.key)
	case 6:
		object.members = append(object.members, jmember{key: "unknownField", value: gen.anyValue(3)})
	case 7:
		member.encodedKey = escapeFirstRune(member.key)
	case 8:
		member.value = rawNode("\"bad \xff utf8\"")
	case 9:
		member.value = rawNode([]string{"-1", "4294967296", "18446744073709551616", "1.5", "1e400", "-9223372036854775809"}[rng.IntN(6)])
	case 10:
		if member.key == "type" || member.key == "kind" {
			member.value = rawNode(jsonQuote("variantFromTheFuture"))
		} else {
			member.value = rawNode(jsonQuote(instanceStrings[rng.IntN(len(instanceStrings))]))
		}
	case 11:
		if len(member.key) > 0 {
			member.key = strings.ToLower(member.key[:1]) + strings.ToUpper(member.key[1:])
		}
	default:
		object.members[at], object.members[len(object.members)-1] = object.members[len(object.members)-1], object.members[at]
		duplicate := jmember{key: object.members[len(object.members)-1].key, value: object.members[len(object.members)-1].value.clone()}
		object.members = append(object.members, duplicate)
	}
	return doc.encode(), true
}

func collectObjects(n *jnode, out *[]*jnode) {
	switch n.kind {
	case 'o':
		*out = append(*out, n)
		for _, member := range n.members {
			collectObjects(member.value, out)
		}
	case 'a':
		for _, item := range n.items {
			collectObjects(item, out)
		}
	}
}

func typeChanged(n *jnode, rng *rand.Rand) *jnode {
	switch n.kind {
	case 'o':
		return &jnode{kind: 'a'}
	case 'a':
		return rawNode(`"array became a string"`)
	}
	if strings.HasPrefix(n.raw, `"`) {
		return rawNode([]string{"7", "{}", "false"}[rng.IntN(3)])
	}
	return rawNode(`"7"`)
}

func snakeCase(key string) string {
	var b strings.Builder
	for i, r := range key {
		if i > 0 && r >= 'A' && r <= 'Z' {
			b.WriteByte('_')
			r += 'a' - 'A'
		}
		b.WriteRune(r)
	}
	return b.String()
}

func escapeFirstRune(key string) string {
	if key == "" {
		return `""`
	}
	r, size := utf8.DecodeRuneInString(key)
	if r > 0xffff {
		return jsonQuote(key)
	}
	return `"` + fmt.Sprintf(`\u%04x`, r) + strings.Trim(jsonQuote(key[size:]), `"`) + `"`
}
