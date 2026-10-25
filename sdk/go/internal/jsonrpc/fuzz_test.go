package jsonrpc

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"sync"
	"testing"
	"time"
)

var envelopeSeeds = []string{
	`{"id":1,"method":"thread/start","params":{"model":"m"}}`,
	`{"id":"go-1","result":{"ok":true}}`,
	`{"id":"go-2","error":{"code":-32000,"message":"boom","data":{"x":1}}}`,
	`{"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","status":"completed"}},"trace":{"traceparent":"00-1-2-01"}}`,
	`{"id":null,"result":{}}`,
	`{"id":1.5,"result":{}}`,
	`{"id":9223372036854775808,"result":{}}`,
	`{"id":"x","error":{"code":null,"message":"m"}}`,
	`{"id":"x","result":{},"error":{"code":1,"message":"m"}}`,
	`{"method":""}`,
	`[]`,
	`null`,
	`{"id":{"nested":true},"method":"m"}`,
}

// FuzzEnvelopeDecode checks that decoding and validating any inbound envelope never
// panics, and that a valid envelope survives a round trip as a valid envelope.
func FuzzEnvelopeDecode(f *testing.F) {
	for _, seed := range envelopeSeeds {
		f.Add([]byte(seed))
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		var env Envelope
		if err := json.Unmarshal(data, &env); err != nil {
			return
		}
		if err := validateInboundEnvelope(env); err != nil {
			return
		}
		if env.ID != nil {
			if _, err := requestIDKey(*env.ID); err != nil {
				t.Fatalf("valid envelope has an unusable id: %v", err)
			}
		}
		encoded, err := json.Marshal(env)
		if err != nil {
			t.Fatalf("valid envelope does not encode: %v", err)
		}
		var again Envelope
		if err := json.Unmarshal(encoded, &again); err != nil {
			t.Fatalf("re-encoded envelope %s does not decode: %v", encoded, err)
		}
		if err := validateInboundEnvelope(again); err != nil {
			t.Fatalf("re-encoded envelope %s is invalid: %v", encoded, err)
		}
		if again.Method != env.Method || (again.ID == nil) != (env.ID == nil) {
			t.Fatalf("round trip changed %s into %s", data, encoded)
		}
	})
}

// FuzzStreamTransportFrames checks the NDJSON frame reader on arbitrary input: no frame
// exceeds the limit or contains a newline, and reading always ends.
func FuzzStreamTransportFrames(f *testing.F) {
	f.Add([]byte("{\"a\":1}\n{\"b\":2}\r\n"), uint16(64))
	f.Add([]byte("\n\n{}\n"), uint16(8))
	f.Add([]byte("{\"long\":\""+string(bytes.Repeat([]byte("x"), 5000))+"\"}\n"), uint16(4096))
	f.Add([]byte("{\"no_newline\":true}"), uint16(0))
	f.Add([]byte("  \r\n"), uint16(16))
	f.Fuzz(func(t *testing.T, data []byte, limit uint16) {
		transport := NewStreamTransport(io.NopCloser(bytes.NewReader(data)), nopWriteCloser{Writer: io.Discard}, int64(limit))
		ctx := context.Background()
		for range len(data) + 1 {
			frame, err := transport.Receive(ctx)
			if err != nil {
				var sizeErr *FrameSizeError
				if errors.As(err, &sizeErr) && (limit == 0 || sizeErr.Size <= int64(limit)) {
					t.Fatalf("frame size error %v under limit %d", sizeErr, limit)
				}
				if errors.Is(err, io.EOF) || errors.As(err, &sizeErr) || isFrameError(err) {
					return
				}
				var closed *ClosedError
				if errors.As(err, &closed) {
					return
				}
				t.Fatalf("unexpected receive error %T: %v", err, err)
			}
			if limit > 0 && int64(len(frame)) > int64(limit) {
				t.Fatalf("frame of %d bytes passed limit %d", len(frame), limit)
			}
			if bytes.ContainsRune(frame, '\n') {
				t.Fatalf("frame contains a newline: %q", frame)
			}
			if len(bytes.TrimSpace(frame)) == 0 {
				t.Fatalf("empty frame returned: %q", frame)
			}
		}
		t.Fatal("reader did not end")
	})
}

func isFrameError(err error) bool {
	return err != nil && err.Error() == "empty JSON-RPC frame"
}

// FuzzRingRedaction checks that the stderr tail never shows a line with a secret-like
// word, however the output is split into writes, and stays within its limit.
func FuzzRingRedaction(f *testing.F) {
	f.Add([]byte("panic: token=abc\nok line\n"), uint16(3), uint16(9), uint16(64))
	f.Add([]byte("Authorization: Bearer x\npartial secr"), uint16(20), uint16(30), uint16(8))
	f.Add([]byte("api_Key=1\n"), uint16(0), uint16(1), uint16(16))
	f.Add([]byte("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaapassword"), uint16(29), uint16(31), uint16(4))
	f.Fuzz(func(t *testing.T, data []byte, first uint16, second uint16, limit uint16) {
		ring := NewRing(int(limit))
		cuts := []int{int(first) % (len(data) + 1), int(second) % (len(data) + 1)}
		if cuts[0] > cuts[1] {
			cuts[0], cuts[1] = cuts[1], cuts[0]
		}
		for _, part := range [][]byte{data[:cuts[0]], data[cuts[0]:cuts[1]], data[cuts[1]:]} {
			if n, err := ring.Write(part); err != nil || n != len(part) {
				t.Fatalf("Write = %d, %v", n, err)
			}
		}
		tail := ring.String()
		if containsSensitiveText(tail) {
			t.Fatalf("stderr tail shows a secret-like word: %q", tail)
		}
		if len(tail) > int(limit) {
			t.Fatalf("stderr tail of %d bytes exceeds limit %d", len(tail), limit)
		}
	})
}

type fuzzTransport struct {
	frames chan json.RawMessage
	done   chan struct{}
	once   sync.Once
}

func (t *fuzzTransport) Receive(ctx context.Context) (json.RawMessage, error) {
	select {
	case frame := <-t.frames:
		return frame, nil
	case <-t.done:
		return nil, &ClosedError{}
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (t *fuzzTransport) Send(context.Context, json.RawMessage) error { return nil }

func (t *fuzzTransport) Close() error {
	t.once.Do(func() { close(t.done) })
	return nil
}

type fuzzHandler struct{}

func (fuzzHandler) HandleServerRequest(context.Context, string, json.RawMessage, json.RawMessage) (any, error) {
	return map[string]any{}, nil
}

func (fuzzHandler) HandleServerNotification(context.Context, string, json.RawMessage, json.RawMessage) error {
	return nil
}

// FuzzClientInboundFrame feeds one arbitrary inbound frame to a client with a pending call:
// the client must not panic, and it either keeps working or ends with an error.
func FuzzClientInboundFrame(f *testing.F) {
	for _, seed := range envelopeSeeds {
		f.Add([]byte(seed))
	}
	f.Add([]byte(`{"id":"go-1","result":"not an object"}`))
	f.Add([]byte(`{"id":"go-1","error":{"code":1,"message":"m"}}`))
	f.Add([]byte(`{"id":7,"method":"item/tool/call","params":{}}`))
	f.Fuzz(func(t *testing.T, frame []byte) {
		transport := &fuzzTransport{frames: make(chan json.RawMessage, 2), done: make(chan struct{})}
		client := NewClientWithOptions(transport, fuzzHandler{}, ClientOptions{HandlerTimeout: time.Second})
		defer client.Close()
		ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
		defer cancel()
		done, err := client.CallAsync(ctx, "thread/read", map[string]any{}, &map[string]any{}, nil)
		if err != nil {
			t.Fatal(err)
		}
		transport.frames <- append(json.RawMessage(nil), frame...)
		// A response to the pending call, unless the fuzzed frame already ended it or the
		// connection.
		transport.frames <- json.RawMessage(`{"id":"go-1","result":{}}`)
		select {
		case <-done:
		case <-time.After(2 * time.Second):
			t.Fatalf("pending call neither answered nor failed after frame %q", frame)
		}
	})
}
