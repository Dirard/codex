package codex

import (
	"encoding/json"
	"testing"
)

// referenceStringAt is the lookup the router did before paramIndex: decode the params into
// map[string]any and walk the dot-separated path.
func referenceStringAt(params []byte, path string) (string, bool) {
	var raw map[string]any
	if len(params) > 0 {
		_ = json.Unmarshal(params, &raw)
	}
	if len(raw) == 0 || path == "" {
		return "", false
	}
	var current any = raw
	start := 0
	for i := 0; i <= len(path); i++ {
		if i < len(path) && path[i] != '.' {
			continue
		}
		object, ok := current.(map[string]any)
		if !ok {
			return "", false
		}
		current, ok = object[path[start:i]]
		if !ok {
			return "", false
		}
		start = i + 1
	}
	value, ok := current.(string)
	return value, ok
}

var paramIndexCases = []struct {
	params string
	path   string
}{
	{`{"threadId":"t","turn":{"id":"u","items":[{"id":"x"}]}}`, "turn.id"},
	{`{"threadId":"t","threadId":"last"}`, "threadId"},
	{`{"threadId":null}`, "threadId"},
	{`{"threadId":7}`, "threadId"},
	{`{"turn":"u"}`, "turn.id"},
	{`{"turn":null}`, "turn.id"},
	{`{"threadId":"escaped key"}`, "threadId"},
	{`{"threadId":"esc\"aped\\né"}`, "threadId"},
	{"{\"threadId\":\"bad utf8 \xff\"}", "threadId"},
	{"{\"thread\xffId\":\"bad utf8 key\"}", "thread�Id"},
	{`{"THREADID":"case"}`, "threadId"},
	{`{"a":{"":{"b":"empty segment"}}}`, "a..b"},
	{`{"":"empty key"}`, "."},
	{`{"a":{"b":{"c":"deep"}}}`, "a.b.c"},
	{`{"a":[{"b":"array"}]}`, "a.b"},
	{` { "threadId" : "spaces" , "x" : [ 1 , { "y" : "}" } ] } `, "threadId"},
	{`[{"threadId":"t"}]`, "threadId"},
	{`{"threadId":"t"`, "threadId"},
	{`{}`, "threadId"},
	{``, "threadId"},
	{`{"threadId":"t"}`, ""},
	{`{"item":{"text":"{\"id\":\"inner\"}","id":"outer"}}`, "item.id"},
}

func TestParamIndexMatchesMapDecode(t *testing.T) {
	for _, tc := range paramIndexCases {
		wantValue, wantOK := referenceStringAt([]byte(tc.params), tc.path)
		gotValue, gotOK := newParamIndex([]byte(tc.params)).stringAt(tc.path)
		if gotValue != wantValue || gotOK != wantOK {
			t.Errorf("%s at %q = %q, %v; want %q, %v", tc.params, tc.path, gotValue, gotOK, wantValue, wantOK)
		}
	}
}

// FuzzParamIndexMatchesMapDecode checks the router's identity lookup against the
// map[string]any decode it replaces, for any params and path.
func FuzzParamIndexMatchesMapDecode(f *testing.F) {
	for _, tc := range paramIndexCases {
		f.Add([]byte(tc.params), tc.path)
	}
	for _, seeds := range routerFuzzSeeds {
		for _, seed := range seeds {
			for _, path := range []string{"threadId", "turnId", "turn.id", "item.id", "loginId", "processHandle"} {
				f.Add([]byte(seed), path)
			}
		}
	}
	f.Fuzz(func(t *testing.T, params []byte, path string) {
		wantValue, wantOK := referenceStringAt(params, path)
		gotValue, gotOK := newParamIndex(params).stringAt(path)
		if gotValue != wantValue || gotOK != wantOK {
			t.Fatalf("%q at %q = %q, %v; want %q, %v", params, path, gotValue, gotOK, wantValue, wantOK)
		}
	})
}
