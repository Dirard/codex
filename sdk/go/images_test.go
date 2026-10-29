package codex_test

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"testing"
	"time"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/codextest"
	"github.com/openai/codex/sdk/go/protocol"
)

// onePixelPNG is a valid 1x1 PNG.
var onePixelPNG = []byte{
	0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
	0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
	0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
	0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
	0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
}

func decodeItem(t *testing.T, raw string) protocol.ThreadItem {
	t.Helper()
	var item protocol.ThreadItem
	if err := json.Unmarshal([]byte(raw), &item); err != nil {
		t.Fatalf("decode item: %v", err)
	}
	return item
}

func TestGeneratedImageFromItemDecodesAnInlineImageAsCodexSendsIt(t *testing.T) {
	item := decodeItem(t, `{
		"type": "imageGeneration",
		"id": "ig_1",
		"status": "completed",
		"revisedPrompt": "a red circle on white",
		"result": "`+base64.StdEncoding.EncodeToString(onePixelPNG)+`",
		"transparentBackground": false,
		"failure": null,
		"savedPath": "/home/user/.codex/generated_images/thread/ig_1.png",
		"inputImages": [{"path": "/work/ref.png", "sha256": "abc"}],
		"generationId": "gen_1"
	}`)

	img, ok, err := codex.GeneratedImageFromItem(item)
	if err != nil || !ok {
		t.Fatalf("GeneratedImageFromItem = ok %v, err %v", ok, err)
	}
	if !bytes.Equal(img.Data, onePixelPNG) {
		t.Fatalf("Data = %x, want the PNG", img.Data)
	}
	if got := img.MediaType(); got != "image/png" {
		t.Fatalf("MediaType = %q", got)
	}
	if img.ItemID != "ig_1" || img.Status != codex.ImageGenerationStatusCompleted || !img.Completed() {
		t.Fatalf("id/status = %q/%q", img.ItemID, img.Status)
	}
	if img.RevisedPrompt != "a red circle on white" || img.GenerationID != "gen_1" {
		t.Fatalf("prompt/generation = %q/%q", img.RevisedPrompt, img.GenerationID)
	}
	if img.SavedPath != "/home/user/.codex/generated_images/thread/ig_1.png" || img.FileID != "" {
		t.Fatalf("savedPath/fileId = %q/%q", img.SavedPath, img.FileID)
	}
	if transparent, set := img.TransparentBackground.Value(); !set || transparent {
		t.Fatalf("TransparentBackground = %v, %v", transparent, set)
	}
	if len(img.InputImages) != 1 || img.InputImages[0].Path != "/work/ref.png" || img.InputImages[0].Sha256 != "abc" {
		t.Fatalf("InputImages = %+v", img.InputImages)
	}
	if img.Failure != nil {
		t.Fatalf("Failure = %+v", img.Failure)
	}
}

func TestGeneratedImageFromItemHandlesFileReferencesStartsAndOtherItems(t *testing.T) {
	file := decodeItem(t, `{"type":"imageGeneration","id":"ig_2","status":"completed","revisedPrompt":null,"fileId":"file_9","failure":null,"generationId":null}`)
	img, ok, err := codex.GeneratedImageFromItem(file)
	if err != nil || !ok || img.FileID != "file_9" || img.Data != nil || img.MediaType() != "" {
		t.Fatalf("file reference = %+v, ok %v, err %v", img, ok, err)
	}

	started := decodeItem(t, `{"type":"imageGeneration","id":"ig_3","status":"in_progress","revisedPrompt":null,"result":"","failure":null,"generationId":null}`)
	img, ok, err = codex.GeneratedImageFromItem(started)
	if err != nil || !ok || img.Status != codex.ImageGenerationStatusInProgress || img.Data != nil || img.Completed() {
		t.Fatalf("started = %+v, ok %v, err %v", img, ok, err)
	}

	message := protocol.ThreadItem{TypeValue: "agentMessage", ID: protocol.SomeNonNull("m1"), Text: protocol.SomeNonNull("hi")}
	if _, ok, err := codex.GeneratedImageFromItem(message); ok || err != nil {
		t.Fatalf("agent message = ok %v, err %v", ok, err)
	}
}

func TestGeneratedImageFromItemReportsUsageLimitFailures(t *testing.T) {
	failed := decodeItem(t, `{"type":"imageGeneration","id":"ig_4","status":"failed","revisedPrompt":null,"result":"",
		"failure":{"type":"usageLimitExceeded","limitId":"imagegen_premium","resetsAt":1793300000},"generationId":null}`)
	img, ok, err := codex.GeneratedImageFromItem(failed)
	if err != nil || !ok {
		t.Fatalf("ok %v, err %v", ok, err)
	}
	if img.Status != codex.ImageGenerationStatusFailed || img.Failure == nil {
		t.Fatalf("status/failure = %q/%+v", img.Status, img.Failure)
	}
	want := codex.ImageGenerationFailure{Type: "usageLimitExceeded", LimitID: "imagegen_premium", ResetsAt: time.Unix(1793300000, 0).UTC()}
	if *img.Failure != want {
		t.Fatalf("Failure = %+v, want %+v", *img.Failure, want)
	}

	unknownReset := decodeItem(t, `{"type":"imageGeneration","id":"ig_5","status":"failed","revisedPrompt":null,"result":"",
		"failure":{"type":"usageLimitExceeded","limitId":"image_gen","resetsAt":null},"generationId":null}`)
	img, _, err = codex.GeneratedImageFromItem(unknownReset)
	if err != nil || img.Failure == nil || !img.Failure.ResetsAt.IsZero() || img.Failure.LimitID != "image_gen" {
		t.Fatalf("unknown reset = %+v, err %v", img.Failure, err)
	}
}

func TestGeneratedImageFromItemRejectsInvalidBase64(t *testing.T) {
	item := decodeItem(t, `{"type":"imageGeneration","id":"ig_6","status":"completed","revisedPrompt":null,"result":"not base64!","failure":null,"generationId":null}`)
	_, ok, err := codex.GeneratedImageFromItem(item)
	var decodeErr *codex.DecodeError
	if !ok || !errors.As(err, &decodeErr) || !bytes.Contains([]byte(err.Error()), []byte("ig_6")) {
		t.Fatalf("ok %v, err %v", ok, err)
	}
	if _, err := codex.GeneratedImages([]protocol.ThreadItem{item}); err == nil {
		t.Fatal("GeneratedImages accepted invalid base64")
	}
}

func TestRunResultGeneratedImagesReturnsTheTurnImages(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	srv := codextest.New(t)
	srv.Respond("thread/start", codextest.ThreadStartResponse("thread-1"))
	srv.Handle("turn/start", func(ctx context.Context, call *codextest.Call) (any, error) {
		var params protocol.TurnStartParams
		if err := call.Decode(&params); err != nil {
			return nil, err
		}
		_ = call.NotifyAfter("item/completed", codextest.ImageGenerationCompleted(params.ThreadID, "turn-1", "ig_1", onePixelPNG, "a red circle", "/codex/generated_images/ig_1.png"))
		_ = call.NotifyAfter("item/completed", codextest.ImageGenerationFailed(params.ThreadID, "turn-1", "ig_2", "imagegen_premium", 0))
		_ = call.NotifyAfter("item/completed", codextest.AgentMessageCompleted(params.ThreadID, "turn-1", "m1", "Done."))
		_ = call.NotifyAfter("turn/completed", codextest.TurnCompleted(params.ThreadID, "turn-1", protocol.TurnStatusCompleted))
		return codextest.TurnStartResponse("turn-1"), nil
	})
	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()
	thread, err := client.Threads.Start(ctx, codex.ThreadStartOptions{})
	if err != nil {
		t.Fatal(err)
	}
	result, err := thread.Run(ctx, codex.Text("Draw a red circle"), codex.TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}

	images, err := result.GeneratedImages()
	if err != nil {
		t.Fatal(err)
	}
	if len(images) != 2 || images[0].ItemID != "ig_1" || images[1].ItemID != "ig_2" {
		t.Fatalf("images = %+v", images)
	}
	if !bytes.Equal(images[0].Data, onePixelPNG) || images[0].RevisedPrompt != "a red circle" || images[0].SavedPath != "/codex/generated_images/ig_1.png" {
		t.Fatalf("first image = %+v", images[0])
	}
	if images[1].Completed() || images[1].Failure == nil || images[1].Failure.LimitID != "imagegen_premium" || !images[1].Failure.ResetsAt.IsZero() {
		t.Fatalf("second image = %+v", images[1])
	}
	images[0].Data[0] = 0
	again, err := result.GeneratedImages()
	if err != nil || !bytes.Equal(again[0].Data, onePixelPNG) {
		t.Fatalf("a changed image leaked into the run result: %v", err)
	}
	if result.FinalResponse != "Done." {
		t.Fatalf("FinalResponse = %q", result.FinalResponse)
	}
}

func TestReadGeneratedImageUsesInlineBytesThenTheSavedFile(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	srv := codextest.New(t)
	srv.Handle("fs/readFile", func(ctx context.Context, call *codextest.Call) (any, error) {
		var params protocol.FsReadFileParams
		if err := call.Decode(&params); err != nil {
			return nil, err
		}
		if params.Path != "/codex/generated_images/ig_1.png" {
			t.Errorf("fs/readFile path = %q", params.Path)
		}
		return protocol.FsReadFileResponse{DataBase64: base64.StdEncoding.EncodeToString(onePixelPNG)}, nil
	})
	client, err := codex.NewClient(ctx, codex.ClientConfig{Transport: srv})
	if err != nil {
		t.Fatal(err)
	}
	defer client.Close()

	inline, err := client.FileSystem.ReadGeneratedImage(ctx, codex.GeneratedImage{Data: onePixelPNG, SavedPath: "/elsewhere.png"})
	if err != nil || !bytes.Equal(inline, onePixelPNG) {
		t.Fatalf("inline = %x, %v", inline, err)
	}
	if n := len(requestsFor(srv, "fs/readFile")); n != 0 {
		t.Fatalf("inline bytes made %d fs/readFile requests", n)
	}

	saved, err := client.FileSystem.ReadGeneratedImage(ctx, codex.GeneratedImage{SavedPath: "/codex/generated_images/ig_1.png"})
	if err != nil || !bytes.Equal(saved, onePixelPNG) {
		t.Fatalf("saved = %x, %v", saved, err)
	}
	if n := len(requestsFor(srv, "fs/readFile")); n != 1 {
		t.Fatalf("fs/readFile requests = %d, want 1", n)
	}

	if _, err := client.FileSystem.ReadGeneratedImage(ctx, codex.GeneratedImage{FileID: "file_9"}); !errors.Is(err, codex.ErrGeneratedImageUnavailable) {
		t.Fatalf("file reference err = %v", err)
	}
}

func requestsFor(srv *codextest.Server, method string) []codextest.Message {
	var out []codextest.Message
	for _, request := range srv.Requests() {
		if request.Method == method {
			out = append(out, request)
		}
	}
	return out
}
