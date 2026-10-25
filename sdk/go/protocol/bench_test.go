package protocol

import (
	"encoding/json"
	"strings"
	"testing"
)

// benchItemCompleted is an item/completed notification with an 11 KB agent message, the
// shape docs/fork/05-go-sdk.md R5 measured.
var benchItemCompleted = func() json.RawMessage {
	text, err := json.Marshal(strings.Repeat("The quick brown fox jumps over the lazy dog. ", 250))
	if err != nil {
		panic(err)
	}
	return json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","completedAtMs":5,"item":{"type":"agentMessage","id":"item-1","text":` + string(text) + `,"phase":"final_answer"}}`)
}()

var benchAgentDelta = json.RawMessage(`{"threadId":"thread-1","turnId":"turn-1","itemId":"item-1","delta":"The quick brown fox "}`)

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
		{"agent_message_delta", "item/agentMessage/delta", benchAgentDelta},
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
