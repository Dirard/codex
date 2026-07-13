package codex

import (
	"context"
	"errors"
	"runtime"
	"slices"
	"testing"
	"time"
)

func TestTurnHandleStreamHasTurnStreamAPISignature(t *testing.T) {
	var _ interface {
		Stream(context.Context) (*TurnStream, error)
	} = (*TurnHandle)(nil)
}

func TestNotificationStreamSendAndCloseCanRace(t *testing.T) {
	for range 1000 {
		stream := newNotificationStream(1, DefaultResourceStreamQueueBytes, nil)
		done := make(chan struct{})
		go func() {
			defer close(done)
			_ = stream.send(Notification{Method: "test/event"})
		}()
		_ = stream.Close()
		select {
		case <-done:
		case <-time.After(2 * time.Second):
			t.Fatal("send did not return after close")
		}
	}
}

func TestNotificationStreamEventsDrainsBufferedNotificationsAfterClose(t *testing.T) {
	stream := newNotificationStream(2, DefaultResourceStreamQueueBytes, nil)
	for _, method := range []string{"test/first", "test/terminal"} {
		if !stream.send(Notification{Method: method}) {
			t.Fatalf("send(%q) failed", method)
		}
	}
	if err := stream.Close(); err != nil {
		t.Fatal(err)
	}

	events := stream.Events()
	time.Sleep(20 * time.Millisecond)
	var methods []string
	for notification := range events {
		methods = append(methods, notification.Method)
	}
	if got, want := methods, []string{"test/first", "test/terminal"}; !slices.Equal(got, want) {
		t.Fatalf("methods = %v, want %v", got, want)
	}
}

func TestNotificationStreamQueueBytesAreBoundedAndReleased(t *testing.T) {
	notification := Notification{
		Method:    "test/event",
		RawParams: make([]byte, 256),
	}
	byteLimit := notificationRetainedBytes(notification)
	stream := newNotificationStream(4, byteLimit, nil)
	if !stream.send(notification) {
		t.Fatalf("first send failed: %v", stream.Err())
	}
	if stream.send(notification) {
		t.Fatal("second send succeeded beyond the byte budget")
	}
	var overflowErr *OverflowError
	if !errors.As(stream.Err(), &overflowErr) {
		t.Fatalf("stream error = %T %[1]v, want *OverflowError", stream.Err())
	}

	stream = newNotificationStream(4, byteLimit, nil)
	if !stream.send(notification) {
		t.Fatalf("send before receive failed: %v", stream.Err())
	}
	if _, ok := stream.Next(context.Background()); !ok {
		t.Fatalf("receive failed: %v", stream.Err())
	}
	if !stream.send(notification) {
		t.Fatalf("send after receive failed: %v", stream.Err())
	}
	_ = stream.Close()

	stream = newNotificationStream(4, byteLimit, nil)
	if !stream.send(notification) {
		t.Fatalf("send before Events receive failed: %v", stream.Err())
	}
	if _, ok := <-stream.Events(); !ok {
		t.Fatalf("Events receive failed: %v", stream.Err())
	}
	if !stream.send(notification) {
		t.Fatalf("send after Events receive failed: %v", stream.Err())
	}
	_ = stream.Close()
}

func TestTurnStreamWithBackgroundContextDoesNotLeakWatcher(t *testing.T) {
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
	handle, err := thread.Turn(context.Background(), Text("hello"), TurnOptions{})
	if err != nil {
		t.Fatal(err)
	}

	before := runtime.NumGoroutine()
	for range 20 {
		stream, err := handle.Stream(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		_ = stream.Close()
	}
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		runtime.GC()
		if runtime.NumGoroutine() <= before+2 {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("goroutines after background streams = %d, before = %d", runtime.NumGoroutine(), before)
}
