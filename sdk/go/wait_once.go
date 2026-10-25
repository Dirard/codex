package codex

import (
	"context"
	"errors"
	"sync"
)

// memoWait gives a handle's Wait one outcome. One call waits at a time while the others
// wait for its outcome or their turn. An error caused by the end of the caller's ctx, or one
// that run wraps in *retryableError, is not an outcome, so the next call resumes the wait.
// Every call after the outcome returns it.
type memoWait[T any] struct {
	mu    sync.Mutex
	slot  chan struct{}
	done  chan struct{}
	value T
	err   error
}

func (m *memoWait[T]) channels() (slot chan struct{}, done chan struct{}) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.done == nil {
		m.slot = make(chan struct{}, 1)
		m.done = make(chan struct{})
	}
	return m.slot, m.done
}

// wait runs run unless the outcome is already known. The value and error written before
// done is closed are visible to every caller that observed done.
func (m *memoWait[T]) wait(ctx context.Context, run func(context.Context) (T, error)) (T, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	slot, done := m.channels()
	select {
	case <-done:
		return m.value, m.err
	default:
	}
	select {
	case slot <- struct{}{}:
	case <-done:
		return m.value, m.err
	case <-ctx.Done():
		var zero T
		return zero, ctx.Err()
	}
	defer func() { <-slot }()
	select {
	case <-done:
		return m.value, m.err
	default:
	}
	value, err := run(ctx)
	var retry *retryableError
	if errors.As(err, &retry) {
		return value, retry.err
	}
	if err != nil && ctx.Err() != nil && isContextError(err) {
		return value, err
	}
	m.value, m.err = value, err
	close(done)
	return value, err
}

// retryableError marks an error of a memoWait run that is not the handle's outcome: the
// caller gets the wrapped error and the next Wait runs again.
type retryableError struct {
	err error
}

func (e *retryableError) Error() string { return e.err.Error() }

func (e *retryableError) Unwrap() error { return e.err }
