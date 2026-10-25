package protocol

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

// scanTestSpec has wire names that need escapes, non-ASCII and case-folding care.
var scanTestSpec = objectSpec{
	fields: []objectField{
		{name: "a"},
		{name: "fileId", aliases: []string{"file_id"}},
		{name: "kind"},
		{name: "névé"},
		{name: "threadId"},
		{name: "type"},
	},
	hasAliases: true,
}

var scanTestObjects = []string{
	`{}`,
	` { } `,
	`{"a":1}`,
	`{"a":1,"a":2}`,
	`{"a":null}`,
	`{"a":{"b":[1,"}",{"c":"]"}]},"type":"x"}`,
	`{"a":"quote \" inside","type":"back\\\\slash\\\\"}`,
	`{"a":"\\\"","threadId":"t"}`,
	`{"threadId":"escaped key"}`,
	`{"threadid":"folded","THREADID":"upper","threadId":"exact"}`,
	`{"thread_id":"delimiters"}`,
	`{"névé":"non-ASCII key","nÉvé":"folded non-ASCII"}`,
	"{\"\xffa\":1,\"a\xff\":2}",
	`{"file_id":"alias","fileId":"canonical"}`,
	`{"file_id":"alias only"}`,
	`{"unknown":[true,false,null,-1.5e+10],"kind":"k"}`,
	"{\n\t\"a\" : 1 ,\r\n \"kind\" :\"k\"\n}",
	`{"a":"\u0000\ud800"}`,
}

var scanTestInvalid = []string{
	``,
	`null`,
	`[]`,
	`"text"`,
	`1`,
	`{`,
	`{"a"}`,
	`{"a":}`,
	`{"a":1,}`,
	`{"a":1} {}`,
	`{"a":1}}`,
	`{"a" 1}`,
	`{"a":tru}`,
	`{"a":"unterminated}`,
	`{"a":[1,2}`,
	"{\"a\x01\":1}",
	`{a:1}`,
}

func TestObjectScanMatchesMapDecode(t *testing.T) {
	for _, input := range scanTestObjects {
		checkObjectScanMatchesMapDecode(t, []byte(input))
	}
	for _, input := range scanTestInvalid {
		checkObjectScanMatchesMapDecode(t, []byte(input))
	}
}

func FuzzObjectScanMatchesMapDecode(f *testing.F) {
	for _, input := range scanTestObjects {
		f.Add([]byte(input))
	}
	for _, input := range scanTestInvalid {
		f.Add([]byte(input))
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		checkObjectScanMatchesMapDecode(t, data)
	})
}

// checkObjectScanMatchesMapDecode requires the scan of every valid JSON object to agree
// with decoding it into map[string]json.RawMessage, and objectError to report the map
// decode's error for every input the scan rejects.
func checkObjectScanMatchesMapDecode(t *testing.T, data []byte) {
	t.Helper()
	trimmed := bytes.TrimSpace(data)
	if bytes.Equal(trimmed, jsonNull) {
		return // begin rejects a top-level null before scanning
	}
	var raw map[string]json.RawMessage
	mapErr := json.Unmarshal(trimmed, &raw)
	s := objectScan{spec: &scanTestSpec, data: trimmed}
	if !s.scan() {
		if mapErr == nil {
			t.Fatalf("scan rejected %q, which decodes into a map", data)
		}
		if err := objectError(trimmed); err == nil || err.Error() != mapErr.Error() {
			t.Fatalf("objectError(%q) = %v, want %v", data, err, mapErr)
		}
		return
	}
	if mapErr != nil {
		if json.Valid(trimmed) {
			t.Fatalf("scan accepted %q; valid JSON the map decode rejects: %v", data, mapErr)
		}
		return
	}
	folded := false
	for key := range raw {
		if scanTestSpec.lookup([]byte(key)) < 0 && scanTestSpec.foldMatches([]byte(key)) {
			folded = true
		}
	}
	for i, field := range scanTestSpec.fields {
		value, ok := raw[field.name]
		if got := s.present&(1<<i) != 0; got != ok {
			t.Fatalf("%q: present[%s] = %v, want %v", data, field.name, got, ok)
		}
		if !ok {
			continue
		}
		if got := s.raw(i); !bytes.Equal(got, value) {
			t.Fatalf("%q: value[%s] = %q, want %q", data, field.name, got, value)
		}
		if got, want := s.null&(1<<i) != 0, bytes.Equal(value, jsonNull); got != want {
			t.Fatalf("%q: null[%s] = %v, want %v", data, field.name, got, want)
		}
		if member, ok := s.member(field.name); !ok || !bytes.Equal(member, value) {
			t.Fatalf("%q: member(%s) = %q, want %q", data, field.name, member, value)
		}
	}
	for _, alias := range []string{"file_id"} {
		value, ok := raw[alias]
		member, found := s.member(alias)
		if found != ok || !bytes.Equal(member, value) {
			t.Fatalf("%q: member(%s) = %q %v, want %q %v", data, alias, member, found, value, ok)
		}
	}
	if folded && !s.clean {
		t.Fatalf("%q: a case-folded key did not ask for a cleaned copy", data)
	}
	wantCleaned := map[string]json.RawMessage{}
	wantUnknown := map[string]json.RawMessage{}
	for key, value := range raw {
		switch {
		case scanTestSpec.lookup([]byte(key)) >= 0:
			wantCleaned[key] = value
		case key != "file_id":
			wantUnknown[key] = value
		}
	}
	var cleaned map[string]json.RawMessage
	if err := json.Unmarshal(s.cleaned(), &cleaned); err != nil {
		t.Fatalf("%q: cleaned copy %q is invalid: %v", data, s.cleaned(), err)
	}
	if !equalRawMaps(cleaned, wantCleaned) {
		t.Fatalf("%q: cleaned copy %q, want the wire fields %v", data, s.cleaned(), wantCleaned)
	}
	if unknown := s.unknownMembers(); !equalRawMaps(unknown, wantUnknown) {
		t.Fatalf("%q: unknownMembers() = %v, want %v", data, unknown, wantUnknown)
	}
}

func equalRawMaps(a, b map[string]json.RawMessage) bool {
	if len(a) != len(b) {
		return false
	}
	for key, value := range a {
		other, ok := b[key]
		if !ok || !bytes.Equal(value, other) {
			return false
		}
	}
	return true
}

func TestLooseEqualFoldMatchesEqualFold(t *testing.T) {
	for _, tc := range []struct {
		key, name string
		want      bool
	}{
		{"threadId", "threadId", true},
		{"THREADID", "threadId", true},
		{"thread_id", "threadId", true},
		{"-thread-Id-", "threadId", true},
		{"threadIdx", "threadId", false},
		{"kelvinK", "kelvink", true},
		{"ſession", "session", true},
		{"névé", "NÉVÉ", true},
		{"", "a", false},
	} {
		if got := looseEqualFold([]byte(tc.key), tc.name); got != tc.want {
			t.Errorf("looseEqualFold(%q, %q) = %v, want %v", tc.key, tc.name, got, tc.want)
		}
		if strings.EqualFold(tc.key, tc.name) && !looseEqualFold([]byte(tc.key), tc.name) {
			t.Errorf("looseEqualFold(%q, %q) misses a strings.EqualFold match", tc.key, tc.name)
		}
	}
}

func FuzzLooseEqualFoldCoversEqualFold(f *testing.F) {
	f.Add("threadId", "THREAD_ID")
	f.Add("K", "k")
	f.Fuzz(func(t *testing.T, key, name string) {
		if strings.EqualFold(key, name) && !looseEqualFold([]byte(key), name) {
			t.Fatalf("looseEqualFold(%q, %q) misses a strings.EqualFold match", key, name)
		}
	})
}

func TestDecodeJSONStringMatchesUnmarshal(t *testing.T) {
	for _, input := range []string{
		`""`, `"a"`, `"12345678"`, `"123456789"`, `"névé snow"`, `"a\"b"`, `"a\\b"`, `"a\nb"`, `"\/\b\f\r\t"`,
		`"\u0041\u00e9\u20ac"`, `"\ud83d\ude00"`, `"\ud800"`, `"\udc00x"`, `"\ud800\u0041"`, `"\ud800\ud800"`, `"\u12"`, `"\uZZZZ"`,
		`"\'"`, `"\x"`, `"trailing\"`, "\"a\x01b\"", "\"\xff\"", "\"\xed\xa0\x80\"", "\"\xef\xbf\xbd\"", `"unterminated`, `x"`, `"`,
		`"` + strings.Repeat("abcdefgh", 9) + `"`, `"` + strings.Repeat("abcdefgh", 9) + `\"x"`, "\"" + strings.Repeat("a", 15) + "\x1f\"",
		"\"tab\tin\"", `"a" "b"`, `"a" `, `"line\nline\n` + strings.Repeat("é", 20) + `"`,
	} {
		checkDecodeJSONString(t, []byte(input))
	}
}

func FuzzDecodeJSONStringMatchesUnmarshal(f *testing.F) {
	f.Add([]byte(`"plain"`))
	f.Add([]byte(`"esc\"aped\n\u00e9\ud83d\ude00"`))
	f.Add([]byte("\"\xe2\x82\""))
	f.Fuzz(func(t *testing.T, data []byte) {
		checkDecodeJSONString(t, data)
	})
}

// checkDecodeJSONString requires decodeJSONString to agree with json.Unmarshal whenever
// it accepts data, and to accept every single string literal json.Unmarshal accepts.
func checkDecodeJSONString(t *testing.T, data []byte) {
	t.Helper()
	got, ok := decodeJSONString(data)
	var want string
	err := json.Unmarshal(data, &want)
	if ok && (err != nil || got != want) {
		t.Fatalf("decodeJSONString(%q) = %q, json.Unmarshal = %q, %v", data, got, want, err)
	}
	literal := len(data) >= 2 && data[0] == '"' && data[len(data)-1] == '"'
	if !ok && err == nil && literal {
		t.Fatalf("decodeJSONString(%q) rejected a literal json.Unmarshal decodes to %q", data, want)
	}
}

func TestOptionalStringFastPathMatchesUnmarshal(t *testing.T) {
	for _, input := range []string{`"plain"`, `"escaped"`, "\"\xff\"", `"a\"b"`} {
		var fast Optional[string]
		if err := fast.UnmarshalJSON([]byte(input)); err != nil {
			t.Fatalf("Optional[string].UnmarshalJSON(%s): %v", input, err)
		}
		var want string
		if err := json.Unmarshal([]byte(input), &want); err != nil {
			t.Fatal(err)
		}
		if got, ok := fast.Value(); !ok || got != want {
			t.Fatalf("Optional[string] from %s = %q, want %q", input, got, want)
		}
		var nonNull OptionalNonNull[string]
		if err := nonNull.UnmarshalJSON([]byte(input)); err != nil {
			t.Fatal(err)
		}
		if got, _ := nonNull.Value(); got != want {
			t.Fatalf("OptionalNonNull[string] from %s = %q, want %q", input, got, want)
		}
	}
}
