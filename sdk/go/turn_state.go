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
// Wait opens the handle's only router subscription. The router hands the subscription's
// events to the state synchronously and in delivery order (deliver, end): they are folded
// into the run result and handed to every open stream of the handle, without a queue or a
// goroutine of the state's own, so a slow stream fails only itself. The outcome is recorded
// once, so repeated or concurrent calls return it instead of subscribing again.
//
// Once the handle had a stream, or a Wait of it returned because its ctx ended, events
// that arrive while no stream is open are kept for the next stream: within what one stream
// holds, within the client's pending budget, which the router's pending buffer shares, and
// for at most LifecycleInactivityTimeout. A stream that follows a loss gets the kept events
// and then *OverflowError.
//
// States: idle (no subscription) -> observing (subscribed) -> finished (subscription
// closed, or the handle released). The result may be known before finished when it exceeds
// the result limits.
type turnState struct {
	client            *Client
	threadID          string
	turnID            string
	interruptOnCancel bool

	startOnce sync.Once

	// deliverMu orders deliveries to streams: deliver holds it while it hands out an event,
	// and a new stream holds it while it replays the kept events. Take it before mu.
	deliverMu sync.Mutex

	mu        sync.Mutex
	source    *NotificationStream
	streams   map[*NotificationStream]struct{}
	collector runResultCollector
	ready     chan struct{}
	resultSet bool
	result    *RunResult
	err       error
	finished  bool
	finishErr error
	kept      keptEvents

	interruptMu       sync.Mutex
	interrupted       bool
	interruptInFlight chan struct{}
	interruptErr      error
}

// keptEvents are the events a handle keeps for its next stream.
type keptEvents struct {
	// on is set once the handle had a stream or a Wait of it gave up on its ctx.
	on     bool
	events []Notification
	bytes  int64
	// lost records that kept events were dropped since the last stream took them.
	lost bool
	// timer drops the kept events after LifecycleInactivityTimeout; generation tells a
	// stale timer from the current one.
	timer      *time.Timer
	generation uint64
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

// start opens the handle's subscription once. It runs without the state's locks: the
// router replays the turn's pending events to deliver during the subscription.
func (s *turnState) start() {
	s.startOnce.Do(func() {
		s.mu.Lock()
		finished := s.finished
		s.mu.Unlock()
		if finished {
			return
		}
		source := s.client.router.subscribeTurnSink(s.threadID, s.turnID, s)
		s.mu.Lock()
		s.source = source
		finished = s.finished
		s.mu.Unlock()
		if finished {
			// Release ran while the subscription opened; closing an ended one is a no-op.
			_ = source.Close()
		}
	})
}

// deliver folds one event of the turn into the result and hands it to the open streams, or
// keeps it for the next stream. The router calls it in delivery order; it never blocks.
func (s *turnState) deliver(notification Notification) {
	s.deliverMu.Lock()
	defer s.deliverMu.Unlock()
	s.mu.Lock()
	if s.finished {
		s.mu.Unlock()
		return
	}
	if !s.resultSet {
		if err := s.collector.observe(notification); err != nil {
			// The result is lost, but streams keep following the turn.
			s.setResultLocked(nil, err)
		}
	}
	if len(s.streams) == 0 {
		if s.kept.on {
			s.keepLocked(notification)
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

// end records the outcome when the subscription closed: after turn/completed, a decode
// error for the turn, or the end of the client.
func (s *turnState) end(err error) {
	s.finish(err)
}

// keepLocked keeps an event for the next stream. Older events give way to stay within
// what one stream holds; when the client's pending budget is used up, the handle drops its
// kept events and keeps only what follows, so the kept events stay the newest ones.
func (s *turnState) keepLocked(notification Notification) {
	limits := s.client.limits
	maxEvents := min(limits.PendingTurnQueue, limits.ResourceStreamQueue)
	size := notificationRetainedBytes(notification)
	for len(s.kept.events) > 0 && (len(s.kept.events) >= maxEvents || s.kept.bytes+size > limits.ResourceStreamQueueBytes) {
		s.kept.bytes -= notificationRetainedBytes(s.kept.events[0])
		s.client.router.releaseKept(notificationRetainedBytes(s.kept.events[0]))
		s.kept.events[0] = Notification{}
		s.kept.events = s.kept.events[1:]
		s.kept.lost = true
	}
	if size > limits.ResourceStreamQueueBytes {
		s.kept.lost = true
		return
	}
	if !s.client.router.reserveKept(size) {
		s.dropKeptLocked()
		s.kept.lost = true
		if !s.client.router.reserveKept(size) {
			return
		}
	}
	s.kept.events = append(s.kept.events, notification)
	s.kept.bytes += size
	if s.kept.timer == nil {
		generation := s.kept.generation
		s.kept.timer = time.AfterFunc(limits.LifecycleInactivityTimeout, func() { s.expireKept(generation) })
	}
}

// expireKept drops kept events nobody took within LifecycleInactivityTimeout; the next
// stream reports the loss.
func (s *turnState) expireKept(generation uint64) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.kept.generation != generation || len(s.kept.events) == 0 {
		return
	}
	s.dropKeptLocked()
	s.kept.lost = true
}

// takeKeptLocked hands the kept events to a new stream and returns their budget.
func (s *turnState) takeKeptLocked() ([]Notification, bool) {
	events, lost := s.kept.events, s.kept.lost
	s.client.router.releaseKept(s.kept.bytes)
	s.resetKeptLocked()
	s.kept.lost = false
	return events, lost
}

// dropKeptLocked forgets the kept events and returns their budget; it leaves lost alone.
func (s *turnState) dropKeptLocked() {
	s.client.router.releaseKept(s.kept.bytes)
	clear(s.kept.events)
	s.resetKeptLocked()
}

func (s *turnState) resetKeptLocked() {
	if s.kept.timer != nil {
		s.kept.timer.Stop()
		s.kept.timer = nil
	}
	s.kept.generation++
	s.kept.events, s.kept.bytes = nil, 0
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
	if s.client.router.isClosed() {
		// The client ended: like the router's pending events, kept events go with it.
		s.dropKeptLocked()
		s.kept.lost = false
	}
	streams := s.takeStreamsLocked()
	s.mu.Unlock()
	for _, stream := range streams {
		stream.closeWithError(err)
	}
}

func (s *turnState) takeStreamsLocked() []*NotificationStream {
	streams := make([]*NotificationStream, 0, len(s.streams))
	for stream := range s.streams {
		streams = append(streams, stream)
	}
	s.streams = map[*NotificationStream]struct{}{}
	return streams
}

func (s *turnState) setResultLocked(result *RunResult, err error) {
	s.result, s.err = result, err
	if !s.resultSet {
		s.resultSet = true
		close(s.ready)
	}
}

// release stops following the turn: it drops the subscription, the kept events and the
// result, closes the open streams and fails later and pending calls with *ClosedError. It
// sends nothing to the runtime.
func (s *turnState) release() {
	releaseErr := &ClosedError{Reason: "turn handle released"}
	s.deliverMu.Lock()
	s.mu.Lock()
	s.finished = true
	s.finishErr = releaseErr
	s.setResultLocked(nil, releaseErr)
	s.collector = runResultCollector{}
	s.dropKeptLocked()
	s.kept.on, s.kept.lost = false, false
	source := s.source
	streams := s.takeStreamsLocked()
	s.mu.Unlock()
	s.deliverMu.Unlock()
	if source != nil {
		_ = source.Close()
	}
	for _, stream := range streams {
		stream.closeWithError(releaseErr)
	}
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
	kept, lost := s.takeKeptLocked()
	finished, finishErr := s.finished, s.finishErr
	if !finished {
		s.kept.on = true
		s.streams[stream] = struct{}{}
	}
	s.mu.Unlock()
	for _, notification := range kept {
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
		s.start()
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
// the turn when the stream ended because of that cancellation rather than Close. A failed
// interrupt is sent again by the next cancellation of the handle.
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
// leaves the turn observed, so a later wait still gets the outcome; with keepForStreams the
// events that arrive from then on are kept for the handle's next stream.
func (s *turnState) wait(ctx context.Context, keepForStreams bool) (*RunResult, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	s.start()
	select {
	case <-s.ready:
		return s.outcome()
	case <-ctx.Done():
	}
	if s.isReady() {
		return s.outcome()
	}
	if keepForStreams {
		s.mu.Lock()
		if !s.finished {
			s.kept.on = true
		}
		s.mu.Unlock()
	}
	err := ctx.Err()
	if interruptErr := s.interruptForCancel(ctx); interruptErr != nil {
		err = errors.Join(err, interruptErr)
	}
	if s.isReady() {
		// The turn finished while it was being interrupted.
		return s.outcome()
	}
	return nil, err
}

func (s *turnState) isReady() bool {
	select {
	case <-s.ready:
		return true
	default:
		return false
	}
}

func (s *turnState) outcome() (*RunResult, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.result.clone(), s.err
}

// turnEnded reports whether the turn finished or its turn/completed was already folded.
func (s *turnState) turnEnded() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.finished || s.collector.completed
}

// interruptForCancel sends turn/interrupt when InterruptOnCancel is set and the turn has not
// ended. One request is in flight at a time and concurrent callers share its result; once an
// interrupt succeeded no further one is sent, while a failed one is sent again by the next
// call.
func (s *turnState) interruptForCancel(ctx context.Context) error {
	if !s.interruptOnCancel || s.turnEnded() {
		return nil
	}
	s.interruptMu.Lock()
	if s.interrupted {
		s.interruptMu.Unlock()
		return nil
	}
	if inFlight := s.interruptInFlight; inFlight != nil {
		s.interruptMu.Unlock()
		<-inFlight
		s.interruptMu.Lock()
		defer s.interruptMu.Unlock()
		if s.interrupted {
			return nil
		}
		return s.interruptErr
	}
	inFlight := make(chan struct{})
	s.interruptInFlight = inFlight
	s.interruptMu.Unlock()

	err := s.client.interruptTurn(ctx, s.threadID, s.turnID)

	s.interruptMu.Lock()
	s.interruptInFlight = nil
	s.interrupted = err == nil
	s.interruptErr = err
	close(inFlight)
	s.interruptMu.Unlock()
	return err
}

// markInterrupted records an interrupt the caller sent itself, so a later cancellation does
// not interrupt the turn again.
func (s *turnState) markInterrupted() {
	s.interruptMu.Lock()
	s.interrupted = true
	s.interruptErr = nil
	s.interruptMu.Unlock()
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
// response, it waits up to interruptOnCancelTimeout for the turn id and then interrupts that
// turn, which may take up to interruptOnCancelTimeout more, so a cancelled start does not
// leave a turn running. A late error response is returned with the ctx error.
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
	var lateErr error
	select {
	case err := <-done:
		if err != nil {
			lateErr = err
			break
		}
		threadID, turnID := turnOf(response)
		if turnID != "" {
			lateErr = c.interruptTurn(ctx, threadID, turnID)
		}
	case <-timer.C:
		lateErr = fmt.Errorf("interrupt after cancellation: no %s response within %s", method, interruptOnCancelTimeout)
	}
	return errors.Join(ctx.Err(), lateErr)
}

func isContextError(err error) bool {
	return errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded)
}
