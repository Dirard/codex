package codex

import (
	"bytes"
	"context"
	"encoding/json"
	"sync"
	"testing"
	"time"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

type scriptedTransport struct {
	t *testing.T

	recv chan json.RawMessage
	sent []json.RawMessage
	mu   sync.Mutex
	once sync.Once

	responses map[string]json.RawMessage

	initializedWasGenerated bool
}

func newScriptedInitializedTransport(t *testing.T, initializePayload json.RawMessage) *scriptedTransport {
	t.Helper()
	if initializePayload == nil {
		initializePayload = currentInitializePayload()
	}
	tr := &scriptedTransport{
		t:         t,
		recv:      make(chan json.RawMessage, 64),
		responses: map[string]json.RawMessage{"initialize": initializePayload},
	}
	return tr
}

func (t *scriptedTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	select {
	case frame, ok := <-t.recv:
		if !ok {
			return nil, &ClosedError{}
		}
		return frame, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (t *scriptedTransport) Send(ctx context.Context, frame json.RawMessage) error {
	t.mu.Lock()
	t.sent = append(t.sent, append(json.RawMessage(nil), frame...))
	t.mu.Unlock()
	var env jsonrpc.Envelope
	if err := json.Unmarshal(frame, &env); err != nil {
		return err
	}
	if env.Method == "initialized" {
		generated := protocol.NewInitializedNotification()
		t.initializedWasGenerated = env.Method == string(generated.Method) && optionalRawJSONEqual(env.Params, generated.Params)
		return nil
	}
	if env.ID == nil {
		return nil
	}
	result, ok := t.responses[env.Method]
	if !ok {
		result = json.RawMessage(`{}`)
	}
	reply := jsonrpc.Envelope{ID: env.ID, Result: result}
	data, err := json.Marshal(reply)
	if err != nil {
		return err
	}
	select {
	case t.recv <- data:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (t *scriptedTransport) Close() error {
	t.once.Do(func() { close(t.recv) })
	return nil
}

func optionalRawJSONEqual(actual json.RawMessage, expected json.RawMessage) bool {
	actual = bytes.TrimSpace(actual)
	expected = bytes.TrimSpace(expected)
	if len(actual) == 0 || bytes.Equal(actual, []byte("null")) {
		return len(expected) == 0 || bytes.Equal(expected, []byte("null"))
	}
	return bytes.Equal(actual, expected)
}

func (t *scriptedTransport) sentFrames() []json.RawMessage {
	t.mu.Lock()
	defer t.mu.Unlock()
	out := make([]json.RawMessage, len(t.sent))
	copy(out, t.sent)
	return out
}

func (t *scriptedTransport) lastFrame(tb testing.TB) json.RawMessage {
	tb.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		frames := t.sentFrames()
		if len(frames) > 0 {
			return frames[len(frames)-1]
		}
		time.Sleep(10 * time.Millisecond)
	}
	tb.Fatal("no frames sent")
	return nil
}

func (t *scriptedTransport) deliverServerRequest(method string, params json.RawMessage, trace json.RawMessage) {
	id := protocol.IntRequestID(99)
	env := jsonrpc.Envelope{ID: &id, Method: method, Params: params, Trace: trace}
	data, err := json.Marshal(env)
	if err != nil {
		t.t.Fatal(err)
	}
	t.recv <- data
}

func currentInitializePayload() json.RawMessage {
	return json.RawMessage(`{
		"userAgent":"codex-go-test dev 0.0.0",
		"codexHome":"/tmp/codex",
		"platformFamily":"unix",
		"platformOs":"linux",
		"stableProtocolDigest":"` + protocol.StableProtocolDigest + `",
		"experimentalProtocolDigest":"` + protocol.ExperimentalProtocolDigest + `",
		"stableSchemaDigest":"` + protocol.StableSchemaDigest + `",
		"experimentalSchemaDigest":"` + protocol.ExperimentalSchemaDigest + `",
		"stableManifestDigest":"` + protocol.StableManifestDigest + `",
		"experimentalManifestDigest":"` + protocol.ExperimentalManifestDigest + `",
		"activeProtocolMode":"experimental"
	}`)
}

func stableInitializePayload() json.RawMessage {
	return json.RawMessage(`{
		"userAgent":"codex-go-test dev 0.0.0",
		"codexHome":"/tmp/codex",
		"platformFamily":"unix",
		"platformOs":"linux",
		"stableProtocolDigest":"` + protocol.StableProtocolDigest + `",
		"experimentalProtocolDigest":"` + protocol.ExperimentalProtocolDigest + `",
		"stableSchemaDigest":"` + protocol.StableSchemaDigest + `",
		"experimentalSchemaDigest":"` + protocol.ExperimentalSchemaDigest + `",
		"stableManifestDigest":"` + protocol.StableManifestDigest + `",
		"experimentalManifestDigest":"` + protocol.ExperimentalManifestDigest + `",
		"activeProtocolMode":"stable"
	}`)
}

func legacyInitializePayload() json.RawMessage {
	return json.RawMessage(`{
		"userAgent":"codex-go-test dev 0.0.0",
		"codexHome":"/tmp/codex",
		"platformFamily":"unix",
		"platformOs":"linux"
	}`)
}

func assertMethod(t *testing.T, frame json.RawMessage, want string) {
	t.Helper()
	if got := methodFromFrame(t, frame); got != want {
		t.Fatalf("method = %s, want %s; frame = %s", got, want, frame)
	}
}

func methodFromFrame(t *testing.T, frame json.RawMessage) string {
	t.Helper()
	var object struct {
		Method string `json:"method"`
	}
	if err := json.Unmarshal(frame, &object); err != nil {
		t.Fatal(err)
	}
	return object.Method
}
