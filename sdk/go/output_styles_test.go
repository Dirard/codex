package codex

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

// Fork: output styles (specs/output-styles-2026-11-07.md, R6.2): the experimental
// thread/outputStyle/* methods are raw-only; their params go out in camelCase and the
// answers decode, including the lowercase source and the path that only custom styles have.
func TestRawOutputStyleMethods(t *testing.T) {
	transport := newScriptedInitializedTransport(t, nil)
	transport.responses["thread/outputStyle/list"] = json.RawMessage(`{
		"current":"outcome-first",
		"styles":[
			{"name":"default","description":"Standard.","source":"builtin"},
			{"name":"terse","description":"Short.","source":"user","path":"/home/output-styles/terse.md"}
		],
		"warnings":["skipped output style file /home/output-styles/bad.md: missing YAML frontmatter delimited by ---"]
	}`)
	transport.responses["thread/outputStyle/set"] = json.RawMessage(`{
		"style":{"name":"learning","description":"Learn.","source":"builtin"}
	}`)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })

	listed, err := client.Raw().ThreadOutputStyleList(context.Background(), protocol.ThreadOutputStyleListParams{
		ThreadID: "thread-1",
	})
	if err != nil {
		t.Fatal(err)
	}
	assertOutputStyleJSON(t, requestParamsForMethod(t, transport, "thread/outputStyle/list"), `{"threadId":"thread-1"}`)
	if listed.Current != "outcome-first" || len(listed.Styles) != 2 || len(listed.Warnings) != 1 {
		t.Fatalf("list response = %+v", listed)
	}
	if listed.Styles[0].Source != protocol.OutputStyleSourceBuiltin || listed.Styles[1].Source != protocol.OutputStyleSourceUser {
		t.Fatalf("sources = %q, %q", listed.Styles[0].Source, listed.Styles[1].Source)
	}
	encoded, err := json.Marshal(listed.Styles[0])
	if err != nil {
		t.Fatal(err)
	}
	assertOutputStyleJSON(t, encoded, `{"name":"default","description":"Standard.","source":"builtin"}`)
	encoded, err = json.Marshal(listed.Styles[1])
	if err != nil {
		t.Fatal(err)
	}
	assertOutputStyleJSON(t, encoded, `{"name":"terse","description":"Short.","source":"user","path":"/home/output-styles/terse.md"}`)

	set, err := client.Raw().ThreadOutputStyleSet(context.Background(), protocol.ThreadOutputStyleSetParams{
		ThreadID: "thread-1",
		Name:     "Learning",
	})
	if err != nil {
		t.Fatal(err)
	}
	assertOutputStyleJSON(t, requestParamsForMethod(t, transport, "thread/outputStyle/set"), `{"threadId":"thread-1","name":"Learning"}`)
	if set.Style.Name != "learning" || set.Style.Source != protocol.OutputStyleSourceBuiltin {
		t.Fatalf("set response = %+v", set)
	}
}

func assertOutputStyleJSON(t *testing.T, got json.RawMessage, want string) {
	t.Helper()
	var gotValue, wantValue any
	if err := json.Unmarshal(got, &gotValue); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal([]byte(want), &wantValue); err != nil {
		t.Fatal(err)
	}
	gotJSON, _ := json.Marshal(gotValue)
	wantJSON, _ := json.Marshal(wantValue)
	if string(gotJSON) != string(wantJSON) {
		t.Fatalf("JSON = %s, want %s", gotJSON, wantJSON)
	}
}
