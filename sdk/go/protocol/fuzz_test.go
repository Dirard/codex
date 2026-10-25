package protocol

import (
	"bytes"
	"encoding/json"
	"sort"
	"testing"
)

// notificationSeeds are realistic payloads for the most frequent notifications; every
// known method is also seeded with an empty object.
var notificationSeeds = map[string][]string{
	"item/completed": {
		`{"threadId":"t","turnId":"u","completedAtMs":5,"item":{"type":"agentMessage","id":"i","text":"done","phase":"final_answer"}}`,
		`{"threadId":"t","turnId":"u","completedAtMs":5,"item":{"type":"commandExecution","id":"i","command":"ls","cwd":"/tmp","status":"completed","commandActions":[],"aggregatedOutput":"a\nb","exitCode":0,"durationMs":3}}`,
		`{"threadId":"t","turnId":"u","completedAtMs":5,"item":{"type":"reasoning","id":"i","summary":["s"],"content":[]}}`,
		`{"threadId":"t","turnId":"u","completedAtMs":5,"item":{"type":"futureItem","id":"i","extra":{"a":[1,2]}}}`,
	},
	"item/started":            {`{"threadId":"t","turnId":"u","startedAtMs":4,"item":{"type":"userMessage","id":"i","content":[{"type":"text","text":"hi","text_elements":[]}]}}`},
	"item/agentMessage/delta": {`{"threadId":"t","turnId":"u","itemId":"i","delta":"he"}`},
	"turn/started":            {`{"threadId":"t","turn":{"id":"u","items":[],"status":"inProgress"}}`},
	"turn/completed": {
		`{"threadId":"t","turn":{"id":"u","items":[],"status":"completed","startedAt":1,"completedAt":2,"durationMs":1}}`,
		`{"threadId":"t","turn":{"id":"u","items":[],"status":"failed","error":{"message":"m","codexErrorInfo":"other"}}}`,
	},
	"thread/tokenUsage/updated": {`{"threadId":"t","turnId":"u","tokenUsage":{"last":{"totalTokens":1,"inputTokens":1,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0},"total":{"totalTokens":2,"inputTokens":2,"cachedInputTokens":0,"outputTokens":0,"reasoningOutputTokens":0}}}`},
	"process/outputDelta":       {`{"processHandle":"p","stream":"stdout","deltaBase64":"aGk=","capReached":false}`},
	"process/exited":            {`{"processHandle":"p","exitCode":0,"stdout":"","stderr":"","stdoutCapReached":false,"stderrCapReached":false}`},
	"account/login/completed":   {`{"loginId":"l","success":true,"error":null}`},
	"error":                     {`{"threadId":"t","turnId":"u","willRetry":false,"error":{"message":"m"}}`},
}

func addNotificationSeeds(f *testing.F) {
	methods := make([]string, 0, len(ServerNotificationRoutingByMethod))
	for method := range ServerNotificationRoutingByMethod {
		methods = append(methods, method)
	}
	sort.Strings(methods)
	for _, method := range methods {
		f.Add(method, []byte(`{}`))
		for _, seed := range notificationSeeds[method] {
			f.Add(method, []byte(seed))
		}
	}
	f.Add("unknown/method", []byte(`{"any":true}`))
}

// The realistic seeds must decode, or the fuzzer starts from rejected inputs only.
func TestNotificationFuzzSeedsDecode(t *testing.T) {
	for method, seeds := range notificationSeeds {
		for _, seed := range seeds {
			if _, err := DecodeServerNotificationPayload(method, json.RawMessage(seed)); err != nil {
				t.Errorf("%s seed %s: %v", method, seed, err)
			}
		}
	}
}

// FuzzDecodeServerNotificationPayload checks the generated notification decoders: no
// panic, and a payload that decodes encodes and decodes again to the same encoding.
func FuzzDecodeServerNotificationPayload(f *testing.F) {
	addNotificationSeeds(f)
	f.Fuzz(func(t *testing.T, method string, params []byte) {
		payload, err := DecodeServerNotificationPayload(method, params)
		if err != nil {
			return
		}
		if _, known := ServerNotificationRoutingByMethod[method]; !known {
			return
		}
		encoded, err := json.Marshal(payload)
		if err != nil {
			t.Fatalf("%s payload decoded from %s does not encode: %v", method, params, err)
		}
		again, err := DecodeServerNotificationPayload(method, encoded)
		if err != nil {
			t.Fatalf("%s payload re-encoded as %s does not decode: %v", method, encoded, err)
		}
		reencoded, err := json.Marshal(again)
		if err != nil {
			t.Fatalf("%s payload does not encode a second time: %v", method, err)
		}
		if !bytes.Equal(encoded, reencoded) {
			t.Fatalf("%s round trip is not stable:\n%s\n%s", method, encoded, reencoded)
		}
	})
}

// FuzzThreadItemUnmarshal checks the largest generated union the same way.
func FuzzThreadItemUnmarshal(f *testing.F) {
	for _, seed := range notificationSeeds["item/completed"] {
		var envelope struct {
			Item json.RawMessage `json:"item"`
		}
		if err := json.Unmarshal([]byte(seed), &envelope); err != nil {
			f.Fatal(err)
		}
		f.Add([]byte(envelope.Item))
	}
	f.Add([]byte(`{"type":"agentMessage","id":"i","text":"x"}`))
	f.Add([]byte(`{"type":"fileChange","id":"i","changes":[],"status":"completed"}`))
	f.Add([]byte(`{"type":7}`))
	f.Fuzz(func(t *testing.T, data []byte) {
		var item ThreadItem
		if err := json.Unmarshal(data, &item); err != nil {
			return
		}
		encoded, err := json.Marshal(item)
		if err != nil {
			t.Fatalf("item decoded from %s does not encode: %v", data, err)
		}
		var again ThreadItem
		if err := json.Unmarshal(encoded, &again); err != nil {
			t.Fatalf("item re-encoded as %s does not decode: %v", encoded, err)
		}
		reencoded, err := json.Marshal(again)
		if err != nil || !bytes.Equal(encoded, reencoded) {
			t.Fatalf("item round trip is not stable (%v):\n%s\n%s", err, encoded, reencoded)
		}
	})
}

// FuzzRequestIDUnmarshal checks that request ids are strings or integers that round-trip.
func FuzzRequestIDUnmarshal(f *testing.F) {
	for _, seed := range []string{`"req-1"`, `42`, `-7`, `1.0`, `1e3`, `null`, `true`, `{}`, `"a" "b"`, `9223372036854775807`, `-9223372036854775809`} {
		f.Add([]byte(seed))
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		var id RequestID
		if err := json.Unmarshal(data, &id); err != nil {
			return
		}
		encoded, err := json.Marshal(id)
		if err != nil {
			t.Fatalf("id decoded from %s does not encode: %v", data, err)
		}
		var again RequestID
		if err := json.Unmarshal(encoded, &again); err != nil || again != id {
			t.Fatalf("id %s round trip = %#v, %v; want %#v", data, again, err, id)
		}
	})
}
