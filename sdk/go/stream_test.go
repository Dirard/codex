package codex

import (
	"context"
	"runtime"
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
		stream := newNotificationStream(1, nil)
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
