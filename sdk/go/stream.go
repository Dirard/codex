package codex

import (
	"context"
	"iter"
	"sync"
)

type Notification struct {
	Method    string
	Payload   any
	RawParams []byte
	Trace     []byte
}

type UnknownNotification struct {
	Method string
	Params []byte
	Trace  []byte
}

// MalformedNotification is delivered to global streams when a known notification fails to
// decode. Streams routed to it are closed with Err; the connection stays open.
type MalformedNotification struct {
	Method string
	Params []byte
	Trace  []byte
	Err    *DecodeError
}

type NotificationStream struct {
	ch             chan Notification
	mu             sync.Mutex
	closed         bool
	maxQueuedBytes int64
	queuedBytes    int64
	queuedSizes    []int64
	closeOnce      sync.Once
	done           chan struct{}
	errMu          sync.Mutex
	err            error
	onClose        func()
	filter         func(Notification) bool
}

func newNotificationStream(size int, maxQueuedBytes int64, onClose func()) *NotificationStream {
	if size <= 0 {
		size = 1
	}
	if maxQueuedBytes <= 0 {
		maxQueuedBytes = DefaultResourceStreamQueueBytes
	}
	return &NotificationStream{
		ch:             make(chan Notification, size),
		maxQueuedBytes: maxQueuedBytes,
		done:           make(chan struct{}),
		onClose:        onClose,
	}
}

func newFilteredNotificationStream(size int, maxQueuedBytes int64, onClose func(), filter func(Notification) bool) *NotificationStream {
	stream := newNotificationStream(size, maxQueuedBytes, onClose)
	stream.filter = filter
	return stream
}

func (s *NotificationStream) Events() <-chan Notification {
	return s.ch
}

func (s *NotificationStream) Next(ctx context.Context) (Notification, bool) {
	return s.recv(ctx)
}

// All returns an iterator over the stream's notifications in delivery order. Each
// notification comes with a nil error. If the stream closes with an error, including the
// ctx error when ctx ends, the iterator yields that error once with a zero Notification
// and stops; a clean close (for example after a turn completed) just ends the loop.
// Breaking out of the loop leaves the stream open.
//
//	for notification, err := range stream.All(ctx) {
//		if err != nil {
//			return err
//		}
//		_ = notification
//	}
func (s *NotificationStream) All(ctx context.Context) iter.Seq2[Notification, error] {
	return func(yield func(Notification, error) bool) {
		if s == nil {
			yield(Notification{}, &ClosedError{})
			return
		}
		for {
			notification, ok := s.Next(ctx)
			if !ok {
				if err := s.Err(); err != nil {
					yield(Notification{}, err)
				}
				return
			}
			if !yield(notification, nil) {
				return
			}
		}
	}
}

// Payloads returns an iterator over the payloads of type T in stream, skipping other
// notifications, with the error semantics of NotificationStream.All.
//
//	for delta, err := range codex.Payloads[protocol.AgentMessageDeltaNotification](ctx, stream) {
//		if err != nil {
//			return err
//		}
//		fmt.Print(delta.Delta)
//	}
func Payloads[T any](ctx context.Context, stream *NotificationStream) iter.Seq2[T, error] {
	return func(yield func(T, error) bool) {
		for notification, err := range stream.All(ctx) {
			if err != nil {
				var zero T
				yield(zero, err)
				return
			}
			payload, ok := notification.Payload.(T)
			if !ok {
				continue
			}
			if !yield(payload, nil) {
				return
			}
		}
	}
}

func (s *NotificationStream) Err() error {
	s.errMu.Lock()
	defer s.errMu.Unlock()
	return s.err
}

func (s *NotificationStream) Close() error {
	s.closeWithError(nil)
	return nil
}

func (s *NotificationStream) send(notification Notification) bool {
	if !s.accepts(notification) {
		return true
	}
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return true
	}
	s.reconcileQueuedBytesLocked()
	retainedBytes := notificationRetainedBytes(notification)
	if retainedBytes > s.maxQueuedBytes-s.queuedBytes {
		s.mu.Unlock()
		s.closeWithError(&OverflowError{Reason: "notification stream byte budget exceeded"})
		return false
	}
	select {
	case s.ch <- notification:
		s.queuedBytes += retainedBytes
		s.queuedSizes = append(s.queuedSizes, retainedBytes)
		s.mu.Unlock()
		return true
	default:
		s.mu.Unlock()
		s.closeWithError(&OverflowError{Reason: "notification stream queue overflow"})
		return false
	}
}

func notificationRetainedBytes(notification Notification) int64 {
	const estimatedNotificationOverhead int64 = 256
	encodedBytes := int64(len(notification.Method) + len(notification.RawParams) + len(notification.Trace))
	return estimatedNotificationOverhead + 2*encodedBytes
}

func (s *NotificationStream) reconcileQueuedBytesLocked() {
	consumed := len(s.queuedSizes) - len(s.ch)
	if consumed <= 0 {
		return
	}
	for _, retainedBytes := range s.queuedSizes[:consumed] {
		s.queuedBytes -= retainedBytes
	}
	s.queuedSizes = s.queuedSizes[consumed:]
}

func (s *NotificationStream) accepts(notification Notification) bool {
	if s == nil || s.filter == nil {
		return true
	}
	return s.filter(notification)
}

func (s *NotificationStream) closeWithError(err error) {
	s.closeOnce.Do(func() {
		s.errMu.Lock()
		s.err = err
		s.errMu.Unlock()
		s.mu.Lock()
		s.closed = true
		close(s.done)
		close(s.ch)
		s.mu.Unlock()
		if s.onClose != nil {
			s.onClose()
		}
	})
}

func (s *NotificationStream) recv(ctx context.Context) (Notification, bool) {
	if ctx == nil {
		ctx = context.Background()
	}
	select {
	case notification, ok := <-s.ch:
		if !ok {
			return Notification{}, false
		}
		s.mu.Lock()
		s.reconcileQueuedBytesLocked()
		s.mu.Unlock()
		return notification, true
	case <-ctx.Done():
		s.closeWithError(ctx.Err())
		return Notification{}, false
	}
}

func closeStreamOnContext(ctx context.Context, stream *NotificationStream) {
	if ctx == nil || stream == nil || ctx.Done() == nil {
		return
	}
	go func() {
		select {
		case <-ctx.Done():
			stream.closeWithError(ctx.Err())
		case <-stream.done:
		}
	}()
}
