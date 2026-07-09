package codex

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestInputHelpersMapToGeneratedUserInput(t *testing.T) {
	input := Inputs(
		Text("hello"),
		DataURL("data:image/png;base64,aW1hZ2U="),
		Skill("skill-name", "/tmp/skill"),
		Mention("file-name", "/tmp/file"),
	)
	wire, err := input.wire(ClientLimits{})
	if err != nil {
		t.Fatal(err)
	}
	want := []protocol.UserInput{
		{TypeValue: "text", Text: protocol.SomeNonNull("hello")},
		{TypeValue: "image", URL: protocol.SomeNonNull("data:image/png;base64,aW1hZ2U=")},
		{TypeValue: "skill", Name: protocol.SomeNonNull("skill-name"), Path: protocol.SomeNonNull("/tmp/skill")},
		{TypeValue: "mention", Name: protocol.SomeNonNull("file-name"), Path: protocol.SomeNonNull("/tmp/file")},
	}
	if len(wire) != len(want) {
		t.Fatalf("wire len = %d, want %d", len(wire), len(want))
	}
	for i := range want {
		if !reflect.DeepEqual(wire[i], want[i]) {
			t.Fatalf("wire[%d] = %#v, want %#v", i, wire[i], want[i])
		}
	}
}

func TestThreadTurnRejectsRemoteImageURLBeforeTransportWrite(t *testing.T) {
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{Transport: transport})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	waitForMethod(t, transport, "thread/start")
	framesBefore := len(transport.sentFrames())

	_, err = thread.Turn(context.Background(), ImageURL("https://example.test/image.png"), TurnOptions{})
	var unsupported *UnsupportedError
	if !errors.As(err, &unsupported) {
		t.Fatalf("err = %T %#v, want UnsupportedError", err, err)
	}
	if len(transport.sentFrames()) != framesBefore {
		t.Fatal("transport wrote after remote image URL validation failure")
	}
}

func TestLocalImageInputSizeLimits(t *testing.T) {
	dir := t.TempDir()
	below := writeLocalInput(t, filepath.Join(dir, "below.bin"), "12")
	exact := writeLocalInput(t, filepath.Join(dir, "exact.bin"), "123")
	over := writeLocalInput(t, filepath.Join(dir, "over.bin"), "1234")
	limits := ClientLimits{MaxLocalInputBytes: 3}

	if _, err := LocalImage(below).wire(limits); err != nil {
		t.Fatal(err)
	}
	if _, err := LocalImage(exact).wire(limits); err != nil {
		t.Fatal(err)
	}
	_, err := LocalImage(over).wire(limits)
	var sizeErr *LocalInputSizeError
	if !errors.As(err, &sizeErr) || sizeErr.Size != 4 || sizeErr.Limit != 3 {
		t.Fatalf("err = %T %#v, want LocalInputSizeError size 4 limit 3", err, err)
	}
}

func TestThreadTurnRejectsOversizedLocalImageBeforeTransportWrite(t *testing.T) {
	dir := t.TempDir()
	over := writeLocalInput(t, filepath.Join(dir, "over.bin"), "1234")
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Limits:    ClientLimits{MaxLocalInputBytes: 3},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	waitForMethod(t, transport, "thread/start")
	framesBefore := len(transport.sentFrames())

	_, err = thread.Turn(context.Background(), LocalImage(over), TurnOptions{})
	var sizeErr *LocalInputSizeError
	if !errors.As(err, &sizeErr) || sizeErr.Size != 4 || sizeErr.Limit != 3 {
		t.Fatalf("err = %T %#v, want LocalInputSizeError size 4 limit 3", err, err)
	}
	framesAfter := transport.sentFrames()
	if len(framesAfter) != framesBefore {
		t.Fatalf("transport wrote %d new frame(s) after local input validation failure", len(framesAfter)-framesBefore)
	}
	for _, frame := range framesAfter[framesBefore:] {
		if methodFromFrame(t, frame) == "turn/start" {
			t.Fatalf("turn/start frame was written after local input validation failure: %s", frame)
		}
	}
}

func TestThreadTurnRejectsOversizedLocalImageAfterStatFallbackBeforeTransportWrite(t *testing.T) {
	dir := t.TempDir()
	over := writeLocalInput(t, filepath.Join(dir, "over.bin"), "1234")
	originalStat := statLocalInput
	statLocalInput = func(string) (os.FileInfo, error) {
		return nil, os.ErrPermission
	}
	t.Cleanup(func() { statLocalInput = originalStat })
	transport := newWorkflowTransport(t)
	client, err := NewClient(context.Background(), ClientConfig{
		Transport: transport,
		Limits:    ClientLimits{MaxLocalInputBytes: 3},
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = client.Close() })
	thread, err := client.Threads.Start(context.Background(), ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	waitForMethod(t, transport, "thread/start")
	framesBefore := len(transport.sentFrames())

	_, err = thread.Turn(context.Background(), LocalImage(over), TurnOptions{})
	var sizeErr *LocalInputSizeError
	if !errors.As(err, &sizeErr) || sizeErr.Size != 4 || sizeErr.Limit != 3 {
		t.Fatalf("err = %T %#v, want LocalInputSizeError from bounded read fallback size 4 limit 3", err, err)
	}
	if len(transport.sentFrames()) != framesBefore {
		t.Fatal("transport wrote after local input bounded-read fallback failure")
	}
}

func writeLocalInput(t *testing.T, path string, contents string) string {
	t.Helper()
	if err := os.WriteFile(path, []byte(contents), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}
