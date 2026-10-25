package codex

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"time"

	"github.com/openai/codex/sdk/go/protocol"
)

// interruptOnCancelTimeout bounds the turn/interrupt request sent after a cancellation, and
// how long a cancelled start waits for the turn id it needs to interrupt.
const interruptOnCancelTimeout = 5 * time.Second

// turnState is the lifecycle of one turn as its handle observes it. The first Stream or
// Wait opens the handle's only router subscription; a pump goroutine folds its events into
// the run result and hands them to every open stream of the handle. The outcome is recorded
// once, so repeated or concurrent calls return it instead of subscribing again.
//
// Once the handle had a stream, events that arrive while none is open are kept in a
// bounded journal and handed to the next stream first, as the router's pending buffer did
// when every Stream call subscribed on its own.
//
// States: idle (no subscription) -> observing (pump running) -> finished (subscription
// closed). The result may be known before finished when it exceeds the result limits.
type turnState struct {
	client            *Client
	threadID          string
	turnID            string
	interruptOnCancel bool

	// deliverMu orders deliveries to streams: the pump holds it while it hands out an
	// event, and a new stream holds it while it replays the journal. Take it before mu.
	deliverMu sync.Mutex

	mu           sync.Mutex
	source       *NotificationStream
	streams      map[*NotificationStream]struct{}
	collector    runResultCollector
	ready        chan struct{}
	resultSet    bool
	result       *RunResult
	err          error
	finished     bool
	finishErr    error
	journalOn    bool
	journal      []Notification
	journalBytes int64
	journalLost  bool

	interruptOnce sync.Once
	interruptErr  error
}

func newTurnState(client *Client, threadID string, turnID string, interruptOnCancel bool) *turnState {
	return &turnState{
		client:            client,
		threadID:          threadID,
		turnID:            turnID,
		interruptOnCancel: interruptOnCancel,
		streams:           map[*NotificationStream]struct{}{},
		collector:         newRunResultCollector(threadID, turnID, client.limits),
		ready:             make(chan struct{}),
	}
}

// startLocked opens the subscription and starts the pump unless it already ran.
func (s *turnState) startLocked() {
	if s.source != nil || s.finished {
		return
	}
	s.source = s.client.router.subscribeTurn(s.threadID, s.turnID)
	go s.pump(s.source)
}

// pump observes the turn until its subscription closes: after turn/completed, a decode
// error for the turn, or the end of the client.
func (s *turnState) pump(source *NotificationStream) {
	for {
		notification, ok := source.Next(context.Background())
		if !ok {
			s.finish(source.Err())
			return
		}
		s.observe(notification)
	}
}

func (s *turnState) observe(notification Notification) {
	s.deliverMu.Lock()
	defer s.deliverMu.Unlock()
	s.mu.Lock()
	if !s.resultSet {
		if err := s.collector.observe(notification); err != nil {
			// The result is lost, but streams keep following the turn.
			s.setResultLocked(nil, err)
		}
	}
	if len(s.streams) == 0 {
		if s.journalOn {
			s.journalLocked(notification)
		}
		s.mu.Unlock()
		return
	}
	streams := make([]*NotificationStream, 0, len(s.streams))
	for stream := range s.streams {
		streams = append(streams, stream)
	}
	s.mu.Unlock()
	for _, stream := range streams {
		stream.sendAccepted(notification)
	}
}

// journalLocked keeps an event for the next stream, within what one stream can hold and
// PendingTurnQueue events; older events give way.
func (s *turnState) journalLocked(notification Notification) {
	limits := s.client.limits
	maxEvents := min(limits.PendingTurnQueue, limits.ResourceStreamQueue)
	s.journal = append(s.journal, notification)
	s.journalBytes += notificationRetainedBytes(notification)
	for len(s.journal) > 0 && (len(s.journal) > maxEvents || s.journalBytes > limits.ResourceStreamQueueBytes) {
		s.journalBytes -= notificationRetainedBytes(s.journal[0])
		s.journal[0] = Notification{}
		s.journal = s.journal[1:]
		s.journalLost = true
	}
}

func (s *turnState) finish(err error) {
	s.deliverMu.Lock()
	defer s.deliverMu.Unlock()
	s.mu.Lock()
	if s.finished {
		s.mu.Unlock()
		return
	}
	s.finished = true
	s.finishErr = err
	if !s.resultSet {
		if err != nil {
			s.setResultLocked(nil, err)
		} else {
			s.setResultLocked(s.collector.finish())
		}
	}
	streams := make([]*NotificationStream, 0, len(s.streams))
	for stream := range s.streams {
		streams = append(streams, stream)
	}
	s.streams = map[*NotificationStream]struct{}{}
	s.mu.Unlock()
	for _, stream := range streams {
		stream.closeWithError(err)
	}
}

func (s *turnState) setResultLocked(result *RunResult, err error) {
	s.result, s.err = result, err
	s.resultSet = true
	close(s.ready)
}

// stream hands out a stream of the turn's events: first the events kept while no stream
// of the handle was open, then live ones. After the turn finished, the stream closes with
// the turn's terminal error once it got the kept events; if kept events were dropped, it
// closes with *OverflowError instead.
func (s *turnState) stream(ctx context.Context) *NotificationStream {
	limits := s.client.limits
	var stream *NotificationStream
	stream = newNotificationStream(limits.ResourceStreamQueue, limits.ResourceStreamQueueBytes, func() {
		s.removeStream(stream)
	})
	s.deliverMu.Lock()
	s.mu.Lock()
	journal, lost := s.journal, s.journalLost
	s.journal, s.journalBytes, s.journalLost = nil, 0, false
	s.journalOn = true
	finished, finishErr := s.finished, s.finishErr
	if !finished {
		s.streams[stream] = struct{}{}
		s.startLocked()
	}
	s.mu.Unlock()
	for _, notification := range journal {
		if !stream.sendAccepted(notification) {
			break
		}
	}
	switch {
	case lost:
		stream.closeWithError(&OverflowError{Reason: "turn events kept while no stream was open exceeded the pending limits"})
	case finished:
		stream.closeWithError(finishErr)
	}
	s.deliverMu.Unlock()
	if !finished {
		s.watchStreamContext(ctx, stream)
	}
	return stream
}

func (s *turnState) removeStream(stream *NotificationStream) {
	s.mu.Lock()
	delete(s.streams, stream)
	s.mu.Unlock()
}

// watchStreamContext closes the stream when ctx ends and, with InterruptOnCancel, stops
// the turn when the stream ended because of that cancellation rather than Close.
func (s *turnState) watchStreamContext(ctx context.Context, stream *NotificationStream) {
	if ctx == nil || ctx.Done() == nil {
		return
	}
	go func() {
		select {
		case <-ctx.Done():
			stream.closeWithError(ctx.Err())
		case <-stream.done:
		}
		if ctx.Err() == nil || !isContextError(stream.Err()) {
			return
		}
		_ = s.interruptForCancel(ctx)
	}()
}

// wait returns the turn's outcome. When ctx ends first it returns the context error and
// leaves the turn observed, so a later wait still gets the outcome.
func (s *turnState) wait(ctx context.Context) (*RunResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	s.mu.Lock()
	s.startLocked()
	ready := s.ready
	s.mu.Unlock()
	select {
	case <-ready:
		return s.outcome()
	case <-ctx.Done():
	}
	select {
	case <-ready:
		return s.outcome()
	default:
	}
	err := ctx.Err()
	if interruptErr := s.interruptForCancel(ctx); interruptErr != nil {
		err = errors.Join(err, interruptErr)
	}
	return nil, err
}

func (s *turnState) outcome() (*RunResult, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.result.clone(), s.err
}

// release ends an observation nobody else can see, such as the one behind Thread.Run.
func (s *turnState) release() {
	s.mu.Lock()
	source := s.source
	idle := len(s.streams) == 0
	s.mu.Unlock()
	if idle && source != nil {
		_ = source.Close()
	}
}

// interruptForCancel sends turn/interrupt once per turn when InterruptOnCancel is set and
// the turn has not finished.
func (s *turnState) interruptForCancel(ctx context.Context) error {
	if !s.interruptOnCancel {
		return nil
	}
	s.mu.Lock()
	finished := s.finished
	s.mu.Unlock()
	if finished {
		return nil
	}
	s.interruptOnce.Do(func() {
		s.interruptErr = s.client.interruptTurn(ctx, s.threadID, s.turnID)
	})
	return s.interruptErr
}

// interruptTurn asks the runtime to stop a turn on behalf of a cancelled ctx: the request
// keeps the ctx values but not its cancellation.
func (c *Client) interruptTurn(ctx context.Context, threadID string, turnID string) error {
	if ctx == nil {
		ctx = context.Background()
	}
	interruptCtx, cancel := context.WithTimeout(context.WithoutCancel(ctx), interruptOnCancelTimeout)
	defer cancel()
	_, err := c.Raw().TurnInterrupt(interruptCtx, protocol.TurnInterruptParams{ThreadID: threadID, TurnID: turnID})
	if err != nil {
		return fmt.Errorf("interrupt turn %s after cancellation: %w", turnID, err)
	}
	return nil
}

// startTurnInterruptible sends a request that starts a turn. When ctx ends before the
// response, it waits a bounded time for the turn id and interrupts that turn, so a
// cancelled start does not leave a turn running.
func startTurnInterruptible[R any](ctx context.Context, c *Client, method string, params any, response *R, turnOf func(*R) (threadID string, turnID string)) error {
	done, err := c.callAsync(ctx, method, params, response, protocol.MethodMetadataByMethod[method])
	if err != nil {
		return err
	}
	select {
	case err := <-done:
		return err
	case <-ctx.Done():
	}
	select {
	case err := <-done:
		return err
	default:
	}
	timer := time.NewTimer(interruptOnCancelTimeout)
	defer timer.Stop()
	var interruptErr error
	select {
	case err := <-done:
		if err == nil {
			threadID, turnID := turnOf(response)
			if turnID != "" {
				interruptErr = c.interruptTurn(ctx, threadID, turnID)
			}
		}
	case <-timer.C:
		interruptErr = fmt.Errorf("interrupt after cancellation: no %s response within %s", method, interruptOnCancelTimeout)
	}
	return errors.Join(ctx.Err(), interruptErr)
}

func isContextError(err error) bool {
	return errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded)
}
