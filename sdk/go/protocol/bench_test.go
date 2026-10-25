package protocol

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"
)

func benchJSONString(text string) string {
	encoded, err := json.Marshal(text)
	if err != nil {
		panic(err)
	}
	return string(encoded)
}

// benchItemCompleted is an item/completed notification with an 11 KB agent message, the
// shape docs/fork/05-go-sdk.md R5 measured.
var benchItemCompleted = json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","completedAtMs":5,"item":{"type":"agentMessage","id":"item-1","text":` +
	benchJSONString(strings.Repeat("The quick brown fox jumps over the lazy dog. ", 250)) + `,"phase":"final_answer"}}`)

// benchCommandCompleted is a finished command execution whose aggregated output is 64 KB
// of log lines with escapes (tabs, quotes, newlines).
var benchCommandCompleted = json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","completedAtMs":5,"item":{"type":"commandExecution","id":"item-2",` +
	`"command":"go test ./...","cwd":"/home/user/project","processId":"proc-1","source":"agent","status":"completed",` +
	`"commandActions":[{"type":"unknown","command":"go test ./..."}],"exitCode":0,"durationMs":5321,"aggregatedOutput":` +
	benchJSONString(strings.Repeat("ok  \tgithub.com/example/project/pkg\t0.123s\t\"cached\"\n", 1200)) + `}}`)

// benchFileChangeCompleted is a finished patch with three file diffs of about 3 KB each.
var benchFileChangeCompleted = func() json.RawMessage {
	diff := strings.Repeat("@@ -1,3 +1,3 @@\n-old line with \"quotes\"\n+new line with <tags> & more\n context\n", 40)
	var changes []string
	for i := range 3 {
		changes = append(changes, fmt.Sprintf(`{"path":"pkg/file%d.go","kind":{"type":"update","move_path":null},"diff":%s}`, i, benchJSONString(diff)))
	}
	return json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","completedAtMs":5,"item":{"type":"fileChange","id":"item-3","status":"completed","changes":[` +
		strings.Join(changes, ",") + `]}}`)
}()

var benchAgentDelta = json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","delta":"The quick brown fox "}`)

var benchTokenUsage = json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","tokenUsage":{"total":{"inputTokens":120345,"cachedInputTokens":98000,"outputTokens":4567,"reasoningOutputTokens":1234,"totalTokens":124912},"last":{"inputTokens":2345,"cachedInputTokens":2000,"outputTokens":456,"reasoningOutputTokens":123,"totalTokens":2801},"modelContextWindow":272000}}`)

// plainItemCompleted is what a plain encoding/json struct decode of the same payload costs,
// the floor a generated decoder could approach.
type plainItemCompleted struct {
	ThreadID      string `json:"threadId"`
	TurnID        string `json:"turnId"`
	CompletedAtMs int64  `json:"completedAtMs"`
	Item          struct {
		Type  string `json:"type"`
		ID    string `json:"id"`
		Text  string `json:"text"`
		Phase string `json:"phase"`
	} `json:"item"`
}

func BenchmarkDecodeServerNotificationPayload(b *testing.B) {
	for _, bench := range []struct {
		name   string
		method string
		params json.RawMessage
	}{
		{"item_completed_11KB", "item/completed", benchItemCompleted},
		{"item_completed_command_64KB", "item/completed", benchCommandCompleted},
		{"item_completed_file_change", "item/completed", benchFileChangeCompleted},
		{"agent_message_delta", "item/agentMessage/delta", benchAgentDelta},
		{"thread_token_usage_updated", "thread/tokenUsage/updated", benchTokenUsage},
	} {
		b.Run(bench.name, func(b *testing.B) {
			b.ReportAllocs()
			b.SetBytes(int64(len(bench.params)))
			for b.Loop() {
				if _, err := DecodeServerNotificationPayload(bench.method, bench.params); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
	b.Run("item_completed_11KB_plain_struct_baseline", func(b *testing.B) {
		b.ReportAllocs()
		b.SetBytes(int64(len(benchItemCompleted)))
		for b.Loop() {
			var payload plainItemCompleted
			if err := json.Unmarshal(benchItemCompleted, &payload); err != nil {
				b.Fatal(err)
			}
		}
	})
}

func BenchmarkMarshal(b *testing.B) {
	var completed ItemCompletedNotification
	if err := json.Unmarshal(benchItemCompleted, &completed); err != nil {
		b.Fatal(err)
	}
	var command ItemCompletedNotification
	if err := json.Unmarshal(benchCommandCompleted, &command); err != nil {
		b.Fatal(err)
	}
	turnStart := TurnStartParams{
		ThreadID: "thread-1",
		Input: []UserInput{{
			TypeValue: "text",
			Text:      SomeNonNull(strings.Repeat("Please refactor the parser and keep the tests green. ", 20)),
		}},
		Model:  Some("gpt-5.5"),
		Effort: Some(ReasoningEffort("high")),
		Cwd:    Some("/home/user/project"),
	}
	for _, bench := range []struct {
		name  string
		value any
	}{
		{"turn_start_params", turnStart},
		{"item_completed_11KB", completed},
		{"item_completed_command_64KB", command},
	} {
		b.Run(bench.name, func(b *testing.B) {
			b.ReportAllocs()
			for b.Loop() {
				if _, err := json.Marshal(bench.value); err != nil {
					b.Fatal(err)
				}
			}
		})
	}
}
