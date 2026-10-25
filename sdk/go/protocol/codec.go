package protocol

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"reflect"
	"unicode"
	"unicode/utf16"
	"unicode/utf8"
)

// The generated UnmarshalJSON methods decode an object in two passes: objectScan walks
// the members once without copying values and records where each wire field is and
// whether it is null, and then a single encoding/json pass decodes the object into a
// method-free alias of the generated type. The serde checks (required fields, nulls,
// aliases and defaults, tagged-union dispatch) run on the scan. The result, the accepted
// inputs and the error for a single fault are those of decoding the object into
// map[string]json.RawMessage and then each field on its own, which the generated code did
// until the sdk/go codecs moved to this runtime (scripts/fork/go-codec-diff.sh compares
// the two).

// maxObjectFields bounds the wire fields of one generated object type; protocodex
// refuses to render a type with more.
const maxObjectFields = 64

type fieldFlags uint8

const (
	// fieldRequired rejects an object without the field (after aliases and defaults).
	fieldRequired fieldFlags = 1 << iota
	// fieldRejectNull rejects a JSON null.
	fieldRejectNull
	// fieldNonNullOptional rejects a JSON null the way OptionalNonNull does.
	fieldNonNullOptional
	// fieldCustom keeps the field out of the alias decode; the generated method decodes it.
	fieldCustom
)

// objectField describes one wire field of a generated object type.
type objectField struct {
	name  string
	flags fieldFlags
	// aliases are serde aliases tried in order when the canonical key is absent.
	aliases []string
	// defaultJSON is the serde default decoded when the key and its aliases are absent;
	// defaultTags (JSON-encoded discriminator values) limit it to some variants.
	defaultJSON string
	defaultTags []string
	// anyOfTags (JSON-encoded discriminator values) make the field a nested anyOf
	// alternative of those variants: it is reset and decoded ignoring decode errors.
	anyOfTags []string
	// variantAliases replace an exact JSON value before it is decoded.
	variantAliases []variantAlias
}

type variantAlias struct {
	alias, canonical string // JSON-encoded strings
}

// objectSpec describes a generated object type: its wire fields sorted by name, the
// index of the first wire field among the Go struct fields, and for tagged unions the
// discriminator field and the variants it names.
type objectSpec struct {
	fields      []objectField
	fieldOffset int
	tag         int
	tags        []string
	hasAliases  bool
}

func (spec *objectSpec) lookup(name []byte) int {
	lo, hi := 0, len(spec.fields)
	for lo < hi {
		mid := int(uint(lo+hi) >> 1)
		if spec.fields[mid].name < string(name) {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	if lo < len(spec.fields) && spec.fields[lo].name == string(name) {
		return lo
	}
	return -1
}

func (spec *objectSpec) index(name string) int {
	return spec.lookup([]byte(name))
}

func (spec *objectSpec) isAlias(name []byte) bool {
	if !spec.hasAliases {
		return false
	}
	for i := range spec.fields {
		for _, alias := range spec.fields[i].aliases {
			if alias == string(name) {
				return true
			}
		}
	}
	return false
}

// foldMatches reports whether a key that is not a wire name could still reach a struct
// field: encoding/json matches field names case-insensitively, and the check also
// ignores '_' and '-' so that a decoder matching names more loosely never sees the key.
func (spec *objectSpec) foldMatches(name []byte) bool {
	for i := range spec.fields {
		if looseEqualFold(name, spec.fields[i].name) {
			return true
		}
	}
	return false
}

func looseEqualFold(a []byte, b string) bool {
	for {
		for len(a) > 0 && (a[0] == '_' || a[0] == '-') {
			a = a[1:]
		}
		for len(b) > 0 && (b[0] == '_' || b[0] == '-') {
			b = b[1:]
		}
		if len(a) == 0 || len(b) == 0 {
			return len(a) == 0 && len(b) == 0
		}
		ra, na := utf8.DecodeRune(a)
		rb, nb := utf8.DecodeRuneInString(b)
		if ra != rb && !equalFoldRune(ra, rb) {
			return false
		}
		a, b = a[na:], b[nb:]
	}
}

// equalFoldRune matches runes under Unicode simple case folding, as strings.EqualFold.
func equalFoldRune(a, b rune) bool {
	for r := unicode.SimpleFold(a); r != a; r = unicode.SimpleFold(r) {
		if r == b {
			return true
		}
	}
	return false
}

type valueSpan struct {
	start, end int32
}

type aliasHit struct {
	name  string
	value valueSpan
}

// objectScan is one pass over a JSON object. It lives on the stack of the generated
// UnmarshalJSON method that owns it.
type objectScan struct {
	spec *objectSpec
	data []byte
	// values holds the last value of every wire field's canonical key.
	values  [maxObjectFields]valueSpan
	present uint64
	null    uint64
	// excluded marks fields kept out of the alias decode: custom-decoded fields, nested
	// anyOf alternatives and values taken from an alias, a default or a variant alias.
	excluded  uint64
	aliasHits []aliasHit
	// clean means the alias decode needs a copy of the object without duplicate,
	// case-folded or excluded members.
	clean   bool
	unknown bool
	tag     string
}

var jsonNull = []byte("null")

// decodeObject is the whole UnmarshalJSON of a generated object type without union
// checks, integer bounds or custom fields: begin, then finish.
func decodeObject(data []byte, spec *objectSpec, alias any) error {
	var s objectScan
	if _, err := s.begin(data, spec); err != nil {
		return err
	}
	return s.finish(alias)
}

// begin is the workflow step "scan the object": it trims data, rejects a top-level null
// and records presence and nullability of every wire field. For tagged unions it checks
// and decodes the discriminator and reports whether it names a known variant; an unknown
// value is left to the caller's raw fallback after validating the JSON.
func (s *objectScan) begin(data []byte, spec *objectSpec) (bool, error) {
	s.spec = spec
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, jsonNull) {
		return false, DecodeError{Field: "", Reason: "cannot be null"}
	}
	s.data = trimmed
	if !s.scan() {
		return false, objectError(trimmed)
	}
	if spec.tags == nil {
		return true, nil
	}
	field := &spec.fields[spec.tag]
	if s.present&(1<<spec.tag) == 0 {
		return false, DecodeError{Field: field.name, Reason: "missing required field"}
	}
	if s.null&(1<<spec.tag) != 0 {
		return false, DecodeError{Field: field.name, Reason: "cannot be null"}
	}
	raw := s.raw(spec.tag)
	for _, alias := range field.variantAliases {
		if string(raw) == alias.alias {
			raw = []byte(alias.canonical)
			break
		}
	}
	tag, ok := decodeJSONString(raw)
	if !ok {
		if err := json.Unmarshal(raw, &tag); err != nil {
			return false, fmt.Errorf("field %s: %w", field.name, err)
		}
	}
	s.tag = tag
	for _, known := range spec.tags {
		if known == tag {
			return true, nil
		}
	}
	if !json.Valid(trimmed) {
		return false, objectError(trimmed)
	}
	return false, nil
}

// finish is the workflow steps "check required and null fields", "decode into the alias"
// and "apply substitutions": it checks the fields in wire-name order, decodes the object
// into alias (a pointer to a method-free conversion of the generated type) with one
// encoding/json pass, and then decodes the fields that came from serde aliases, defaults
// or variant aliases and the nested anyOf alternatives.
func (s *objectScan) finish(alias any) error {
	spec := s.spec
	for i := range spec.fields {
		field := &spec.fields[i]
		bit := uint64(1) << i
		if s.present&bit == 0 && field.flags&fieldRequired == 0 && field.defaultJSON == "" && field.aliases == nil && field.anyOfTags == nil {
			continue
		}
		anyOf := s.anyOf(i)
		raw, ok, substituted := s.fieldValue(i)
		if substituted || anyOf || field.flags&fieldCustom != 0 {
			s.excluded |= bit
			if s.present&bit != 0 {
				s.clean = true
			}
		}
		if !ok {
			if field.flags&fieldRequired != 0 {
				return DecodeError{Field: field.name, Reason: "missing required field"}
			}
			continue
		}
		if bytes.Equal(raw, jsonNull) {
			if field.flags&fieldRejectNull != 0 {
				return DecodeError{Field: field.name, Reason: "cannot be null"}
			}
			if field.flags&fieldNonNullOptional != 0 && !anyOf && field.flags&fieldCustom == 0 {
				return fmt.Errorf("field %s: %w", field.name, errOptionalNonNullNull)
			}
		}
	}
	src := s.data
	if s.clean {
		if !json.Valid(src) {
			return objectError(src)
		}
		src = s.cleaned()
	}
	if err := json.Unmarshal(src, alias); err != nil {
		if !json.Valid(src) {
			return err
		}
		return s.diagnose(alias, err)
	}
	if s.excluded == 0 {
		return nil
	}
	target := reflect.ValueOf(alias).Elem()
	for i := range spec.fields {
		field := &spec.fields[i]
		if s.excluded&(1<<i) == 0 || field.flags&fieldCustom != 0 {
			continue
		}
		value := target.Field(spec.fieldOffset + i)
		raw, ok, _ := s.fieldValue(i)
		if s.anyOf(i) {
			value.SetZero()
			if ok {
				decoded := reflect.New(value.Type())
				if json.Unmarshal(raw, decoded.Interface()) == nil {
					value.Set(decoded.Elem())
				}
			}
			continue
		}
		if !ok {
			continue
		}
		if err := json.Unmarshal(raw, value.Addr().Interface()); err != nil {
			return fmt.Errorf("field %s: %w", field.name, err)
		}
	}
	return nil
}

// fieldValue returns the raw JSON the per-field decoder used for field i: the canonical
// member, else the first present serde alias, else the serde default, then rewritten by
// a variant alias. substituted reports that it is not the canonical member as written.
func (s *objectScan) fieldValue(i int) (raw []byte, ok, substituted bool) {
	field := &s.spec.fields[i]
	if s.present&(1<<i) != 0 {
		raw, ok = s.raw(i), true
	} else {
		for _, alias := range field.aliases {
			if raw, ok = s.member(alias); ok {
				break
			}
		}
		if !ok && field.defaultJSON != "" && (len(field.defaultTags) == 0 || s.tagRawIn(field.defaultTags)) {
			raw, ok = []byte(field.defaultJSON), true
		}
		if !ok {
			return nil, false, false
		}
		substituted = true
	}
	for _, alias := range field.variantAliases {
		if string(raw) == alias.alias {
			return []byte(alias.canonical), true, true
		}
	}
	return raw, true, substituted
}

// member returns the last value of the member named name, a wire field or an alias.
func (s *objectScan) member(name string) ([]byte, bool) {
	if i := s.spec.index(name); i >= 0 {
		if s.present&(1<<i) == 0 {
			return nil, false
		}
		return s.raw(i), true
	}
	for k := len(s.aliasHits) - 1; k >= 0; k-- {
		if s.aliasHits[k].name == name {
			span := s.aliasHits[k].value
			return s.data[span.start:span.end], true
		}
	}
	return nil, false
}

func (s *objectScan) raw(i int) []byte {
	span := s.values[i]
	return s.data[span.start:span.end]
}

func (s *objectScan) anyOf(i int) bool {
	tags := s.spec.fields[i].anyOfTags
	return len(tags) > 0 && s.tagRawIn(tags)
}

// tagRawIn compares the raw JSON of the discriminator with JSON-encoded tags: variant
// conditions of fields match the discriminator as written, not its decoded value.
func (s *objectScan) tagRawIn(tags []string) bool {
	if s.spec.tags == nil || s.present&(1<<s.spec.tag) == 0 {
		return false
	}
	raw := s.raw(s.spec.tag)
	for _, tag := range tags {
		if string(raw) == tag {
			return true
		}
	}
	return false
}

// has reports whether the canonical key of the named wire field is present.
func (s *objectScan) has(name string) bool {
	i := s.spec.index(name)
	return i >= 0 && s.present&(1<<i) != 0
}

// isNull reports whether the canonical key of the named wire field is present and null.
func (s *objectScan) isNull(name string) bool {
	i := s.spec.index(name)
	return i >= 0 && s.present&(1<<i) != 0 && s.null&(1<<i) != 0
}

// nonNull reports whether the canonical key of the named wire field is present and not null.
func (s *objectScan) nonNull(name string) bool {
	i := s.spec.index(name)
	return i >= 0 && s.present&(1<<i) != 0 && s.null&(1<<i) == 0
}

// rawEquals reports whether the canonical key of the named wire field is present with
// exactly the JSON value want.
func (s *objectScan) rawEquals(name, want string) bool {
	i := s.spec.index(name)
	return i >= 0 && s.present&(1<<i) != 0 && string(s.raw(i)) == want
}

// value returns the raw JSON of the named wire field after aliases and defaults.
func (s *objectScan) value(name string) ([]byte, bool) {
	i := s.spec.index(name)
	if i < 0 {
		return nil, false
	}
	raw, ok, _ := s.fieldValue(i)
	return raw, ok
}

// diagnose re-decodes each canonical field on its own, in wire-name order, so that a
// failed alias decode reports the same "field <name>: ..." error as the per-field decoder.
func (s *objectScan) diagnose(alias any, err error) error {
	target := reflect.ValueOf(alias).Elem()
	for i := range s.spec.fields {
		bit := uint64(1) << i
		if s.present&bit == 0 || s.excluded&bit != 0 {
			continue
		}
		value := target.Field(s.spec.fieldOffset + i)
		if fieldErr := json.Unmarshal(s.raw(i), value.Addr().Interface()); fieldErr != nil {
			return fmt.Errorf("field %s: %w", s.spec.fields[i].name, fieldErr)
		}
	}
	return err
}

// objectError returns the error for data that is not a JSON object: the error of
// decoding it into map[string]json.RawMessage.
func objectError(data []byte) error {
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}
	return fmt.Errorf("protocol: internal error: object scan rejected a valid JSON object")
}

// scan walks the members of s.data. It accepts every valid JSON object and may accept
// some invalid ones, which the alias decode or json.Valid rejects afterwards.
func (s *objectScan) scan() bool {
	data := s.data
	if len(data) < 2 || data[0] != '{' {
		return false
	}
	i := skipJSONSpace(data, 1)
	if i < len(data) && data[i] == '}' {
		return i == len(data)-1
	}
	for {
		keyStart := i
		keyEnd, ok := scanJSONString(data, i)
		if !ok {
			return false
		}
		i = skipJSONSpace(data, keyEnd)
		if i >= len(data) || data[i] != ':' {
			return false
		}
		i = skipJSONSpace(data, i+1)
		valueStart := i
		valueEnd, ok := skipJSONValue(data, i)
		if !ok {
			return false
		}
		if !s.addMember(data[keyStart:keyEnd], valueStart, valueEnd) {
			return false
		}
		i = skipJSONSpace(data, valueEnd)
		if i >= len(data) {
			return false
		}
		switch data[i] {
		case ',':
			i = skipJSONSpace(data, i+1)
		case '}':
			return i == len(data)-1
		default:
			return false
		}
	}
}

func (s *objectScan) addMember(quotedKey []byte, valueStart, valueEnd int) bool {
	key, ok := jsonKey(quotedKey)
	if !ok {
		return false
	}
	span := valueSpan{start: int32(valueStart), end: int32(valueEnd)}
	if i := s.spec.lookup(key); i >= 0 {
		bit := uint64(1) << i
		if s.present&bit != 0 {
			s.clean = true
		}
		s.present |= bit
		s.values[i] = span
		if s.data[valueStart] == 'n' {
			s.null |= bit
		} else {
			s.null &^= bit
		}
		return true
	}
	s.unknown = true
	if s.spec.isAlias(key) {
		s.aliasHits = append(s.aliasHits, aliasHit{name: string(key), value: span})
	}
	if s.spec.foldMatches(key) {
		s.clean = true
	}
	return true
}

// members calls visit for every member of the valid object s.data with its unquoted key.
func (s *objectScan) members(visit func(key []byte, quotedKey, value []byte, valueStart int)) {
	data := s.data
	i := skipJSONSpace(data, 1)
	for i < len(data) && data[i] != '}' {
		keyStart := i
		keyEnd, _ := scanJSONString(data, i)
		i = skipJSONSpace(data, keyEnd)
		i = skipJSONSpace(data, i+1)
		valueStart := i
		valueEnd, _ := skipJSONValue(data, i)
		key, _ := jsonKey(data[keyStart:keyEnd])
		visit(key, data[keyStart:keyEnd], data[valueStart:valueEnd], valueStart)
		i = skipJSONSpace(data, valueEnd)
		if i < len(data) && data[i] == ',' {
			i = skipJSONSpace(data, i+1)
		}
	}
}

// cleaned copies the object keeping only the last member of every wire field that the
// alias decode should see, so encoding/json neither merges duplicate keys nor folds the
// case of a key onto a field.
func (s *objectScan) cleaned() []byte {
	out := make([]byte, 0, len(s.data))
	out = append(out, '{')
	s.members(func(key, quotedKey, value []byte, valueStart int) {
		i := s.spec.lookup(key)
		if i < 0 || s.excluded&(1<<i) != 0 || int(s.values[i].start) != valueStart {
			return
		}
		if len(out) > 1 {
			out = append(out, ',')
		}
		out = append(out, quotedKey...)
		out = append(out, ':')
		out = append(out, value...)
	})
	return append(out, '}')
}

// unknownMembers collects the members whose keys are neither wire names nor serde
// aliases, the last value of a key winning, for flattened additional fields.
func (s *objectScan) unknownMembers() map[string]json.RawMessage {
	out := map[string]json.RawMessage{}
	if !s.unknown {
		return out
	}
	s.members(func(key, _, value []byte, _ int) {
		if s.spec.lookup(key) < 0 && !s.spec.isAlias(key) {
			out[string(key)] = append(json.RawMessage(nil), value...)
		}
	})
	return out
}

// jsonKey unquotes a member key the way encoding/json does: escapes are decoded and
// invalid UTF-8 becomes U+FFFD. A key without escapes or non-ASCII bytes is returned as
// a subslice of quoted.
func jsonKey(quoted []byte) ([]byte, bool) {
	inner := quoted[1 : len(quoted)-1]
	for _, c := range inner {
		if c == '\\' || c >= utf8.RuneSelf || c < 0x20 {
			var key string
			if err := json.Unmarshal(quoted, &key); err != nil {
				return nil, false
			}
			return []byte(key), true
		}
	}
	return inner, true
}

func skipJSONSpace(data []byte, i int) int {
	for i < len(data) {
		switch data[i] {
		case ' ', '\t', '\n', '\r':
			i++
		default:
			return i
		}
	}
	return i
}

// scanJSONString returns the index after the string that starts at data[i]: the first
// quote preceded by an even number of backslashes.
func scanJSONString(data []byte, i int) (int, bool) {
	if i >= len(data) || data[i] != '"' {
		return 0, false
	}
	j := i + 1
	for {
		k := bytes.IndexByte(data[j:], '"')
		if k < 0 {
			return 0, false
		}
		q := j + k
		backslashes := 0
		for p := q - 1; p > i && data[p] == '\\'; p-- {
			backslashes++
		}
		if backslashes%2 == 0 {
			return q + 1, true
		}
		j = q + 1
	}
}

// skipJSONValue returns the index after the value that starts at data[i].
func skipJSONValue(data []byte, i int) (int, bool) {
	if i >= len(data) {
		return 0, false
	}
	switch data[i] {
	case '"':
		return scanJSONString(data, i)
	case '{', '[':
		depth := 0
		for j := i; j < len(data); j++ {
			switch data[j] {
			case '"':
				end, ok := scanJSONString(data, j)
				if !ok {
					return 0, false
				}
				j = end - 1
			case '{', '[':
				depth++
			case '}', ']':
				depth--
				if depth == 0 {
					return j + 1, true
				}
			}
		}
		return 0, false
	default:
		j := i
		for j < len(data) {
			switch data[j] {
			case ',', '}', ']', ':', ' ', '\t', '\n', '\r':
				return j, j > i
			}
			j++
		}
		return j, j > i
	}
}

const (
	swarOnes  = 0x0101010101010101
	swarHighs = 0x8080808080808080
)

// decodeJSONString returns the value of the JSON string literal data exactly as
// encoding/json decodes it — escapes resolved, unpaired surrogates and invalid UTF-8
// replaced by U+FFFD — and false when data is not a valid JSON string literal, which
// callers leave to json.Unmarshal. A literal without escapes, control characters or
// non-ASCII bytes is checked eight bytes at a time.
func decodeJSONString(data []byte) (string, bool) {
	if len(data) < 2 || data[0] != '"' || data[len(data)-1] != '"' {
		return "", false
	}
	inner := data[1 : len(data)-1]
	// (x - 0x01…) &^ x has a high bit set in some byte exactly when x has a zero byte, and
	// (x - 0x20…) &^ x when x has a byte below 0x20; x & 0x80… finds non-ASCII bytes.
	i := 0
	for ; i+8 <= len(inner); i += 8 {
		w := binary.LittleEndian.Uint64(inner[i:])
		quote := w ^ (swarOnes * '"')
		backslash := w ^ (swarOnes * '\\')
		if ((w-swarOnes*0x20)&^w|(quote-swarOnes)&^quote|(backslash-swarOnes)&^backslash|w)&swarHighs != 0 {
			break
		}
	}
	for ; i < len(inner); i++ {
		if c := inner[i]; c < 0x20 || c == '"' || c == '\\' || c >= utf8.RuneSelf {
			break
		}
	}
	if i == len(inner) {
		return string(inner), true
	}
	return unquoteJSONString(inner, i)
}

// unquoteJSONString decodes the inside of a string literal whose first i bytes are plain
// ASCII, following encoding/json's unquote and rejecting what its scanner rejects.
func unquoteJSONString(inner []byte, i int) (string, bool) {
	out := make([]byte, i, len(inner)+utf8.UTFMax)
	copy(out, inner[:i])
	for i < len(inner) {
		c := inner[i]
		switch {
		case c == '\\':
			if i+1 >= len(inner) {
				return "", false
			}
			switch inner[i+1] {
			case '"', '\\', '/':
				out = append(out, inner[i+1])
			case 'b':
				out = append(out, '\b')
			case 'f':
				out = append(out, '\f')
			case 'n':
				out = append(out, '\n')
			case 'r':
				out = append(out, '\r')
			case 't':
				out = append(out, '\t')
			case 'u':
				r, ok := jsonHex4(inner[i+2:])
				if !ok {
					return "", false
				}
				i += 6
				if utf16.IsSurrogate(r) {
					if low, ok := jsonLowSurrogate(inner[i:]); ok {
						if decoded := utf16.DecodeRune(r, low); decoded != unicode.ReplacementChar {
							out = utf8.AppendRune(out, decoded)
							i += 6
							continue
						}
					}
					r = unicode.ReplacementChar
				}
				out = utf8.AppendRune(out, r)
				continue
			default:
				return "", false
			}
			i += 2
		case c == '"' || c < 0x20:
			return "", false
		case c < utf8.RuneSelf:
			out = append(out, c)
			i++
		default:
			r, size := utf8.DecodeRune(inner[i:])
			out = utf8.AppendRune(out, r)
			i += size
		}
	}
	return string(out), true
}

func jsonHex4(data []byte) (rune, bool) {
	if len(data) < 4 {
		return 0, false
	}
	var r rune
	for _, c := range data[:4] {
		switch {
		case '0' <= c && c <= '9':
			c -= '0'
		case 'a' <= c && c <= 'f':
			c = c - 'a' + 10
		case 'A' <= c && c <= 'F':
			c = c - 'A' + 10
		default:
			return 0, false
		}
		r = r*16 + rune(c)
	}
	return r, true
}

// jsonLowSurrogate reads the \uXXXX escape at the start of data, if there is a valid one.
func jsonLowSurrogate(data []byte) (rune, bool) {
	if len(data) < 6 || data[0] != '\\' || data[1] != 'u' {
		return 0, false
	}
	return jsonHex4(data[2:])
}

// decodePayload decodes a notification payload exactly as json.Unmarshal(params, v) does,
// calling the generated decoder directly instead of letting encoding/json validate and
// skip the payload first: the decoder validates it itself. Bytes around params that
// bytes.TrimSpace would drop but JSON does not allow still go through json.Unmarshal.
func decodePayload(params []byte, v json.Unmarshaler) error {
	if len(bytes.TrimSpace(params)) != len(trimJSONSpace(params)) {
		return json.Unmarshal(params, v)
	}
	return v.UnmarshalJSON(params)
}

func trimJSONSpace(data []byte) []byte {
	start := skipJSONSpace(data, 0)
	end := len(data)
	for end > start {
		switch data[end-1] {
		case ' ', '\t', '\n', '\r':
			end--
			continue
		}
		break
	}
	return data[start:end]
}

// marshalsNonNull reports whether value marshals to something other than null, the check
// the marshal of a nested anyOf alternative makes on each of its required fields.
func marshalsNonNull(value any) bool {
	raw, err := json.Marshal(value)
	return err == nil && !bytes.Equal(bytes.TrimSpace(raw), jsonNull)
}
