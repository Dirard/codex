package codex

import (
	"context"
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

type streamEvent struct {
	notification Notification
	err          error
}

type NotificationStream struct {
	ch        chan streamEvent
	mu        sync.Mutex
	closed    bool
	closeOnce sync.Once
	done      chan struct{}
	errMu     sync.Mutex
	err       error
	onClose   func()
	filter    func(Notification) bool
}

func newNotificationStream(size int, onClose func()) *NotificationStream {
	if size <= 0 {
		size = 1
	}
	return &NotificationStream{
		ch:      make(chan streamEvent, size),
		done:    make(chan struct{}),
		onClose: onClose,
	}
}

func newFilteredNotificationStream(size int, onClose func(), filter func(Notification) bool) *NotificationStream {
	stream := newNotificationStream(size, onClose)
	stream.filter = filter
	return stream
}

func (s *NotificationStream) Events() <-chan Notification {
	out := make(chan Notification)
	go func() {
		defer close(out)
		for {
			notification, ok := s.recv(context.Background())
			if !ok {
				return
			}
			select {
			case out <- notification:
			case <-s.done:
				return
			}
		}
	}()
	return out
}

func (s *NotificationStream) Next(ctx context.Context) (Notification, bool) {
	return s.recv(ctx)
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
	select {
	case s.ch <- streamEvent{notification: notification}:
		s.mu.Unlock()
		return true
	default:
		s.mu.Unlock()
		s.closeWithError(&OverflowError{Reason: "notification stream queue overflow"})
		return false
	}
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
	case item, ok := <-s.ch:
		if !ok {
			return Notification{}, false
		}
		if item.err != nil {
			s.closeWithError(item.err)
			return Notification{}, false
		}
		return item.notification, true
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
