package codex

import (
	"context"
	"sync"

	"github.com/openai/codex/sdk/go/protocol"
)

type ReviewStartOptions struct {
	ThreadID string
	Target   ReviewTarget
	Delivery ReviewDelivery
	// InterruptOnCancel interrupts the review turn on the runtime when the ctx of
	// Reviews.Start (after the request was sent), Wait or Events ends before the review
	// finished. By default cancellation only stops the local wait.
	InterruptOnCancel bool
}

type ReviewTarget = protocol.ReviewTarget
type ReviewDelivery = protocol.ReviewDelivery

const (
	ReviewDeliveryInline   ReviewDelivery = protocol.ReviewDeliveryInline
	ReviewDeliveryDetached ReviewDelivery = protocol.ReviewDeliveryDetached
)

// ReviewHandle follows one review turn; like TurnHandle, it records one outcome.
type ReviewHandle struct {
	client            *Client
	reviewThreadID    string
	turnID            string
	interruptOnCancel bool

	stateMu sync.Mutex
	state   *turnState
}

type ReviewResult = RunResult
type ReviewStream = NotificationStream

func UncommittedChangesReviewTarget() ReviewTarget {
	return ReviewTarget{TypeValue: "uncommittedChanges"}
}

func (c *ReviewsClient) Start(ctx context.Context, opts ReviewStartOptions) (*ReviewHandle, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("review start", "review/start"); err != nil {
		return nil, err
	}
	params := protocol.ReviewStartParams{ThreadID: opts.ThreadID, Target: opts.Target}
	if opts.Delivery != "" {
		params.Delivery = protocol.Some(opts.Delivery)
	}
	var response protocol.ReviewStartResponse
	var err error
	if opts.InterruptOnCancel {
		err = startTurnInterruptible(ctx, c.client, "review/start", params, &response, func(response *protocol.ReviewStartResponse) (string, string) {
			return response.ReviewThreadID, response.Turn.ID
		})
	} else {
		response, err = c.client.Raw().ReviewStart(ctx, params)
	}
	if err != nil {
		return nil, err
	}
	if response.ReviewThreadID == "" {
		return nil, &UnsupportedError{Reason: "review start response did not include reviewThreadId"}
	}
	if response.Turn.ID == "" {
		return nil, &UnsupportedError{Reason: "review start response did not include turn.id"}
	}
	return &ReviewHandle{
		client:            c.client,
		reviewThreadID:    response.ReviewThreadID,
		turnID:            response.Turn.ID,
		interruptOnCancel: opts.InterruptOnCancel,
	}, nil
}

// Events returns the review turn's events with the semantics of TurnHandle.Stream.
func (h *ReviewHandle) Events(ctx context.Context) (*ReviewStream, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("review events"); err != nil {
		return nil, err
	}
	return h.turnState().stream(ctx), nil
}

// Wait returns the review result with the semantics of TurnHandle.Wait.
func (h *ReviewHandle) Wait(ctx context.Context) (*ReviewResult, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("review events"); err != nil {
		return nil, err
	}
	return h.turnState().wait(ctx)
}

func (h *ReviewHandle) turnState() *turnState {
	h.stateMu.Lock()
	defer h.stateMu.Unlock()
	if h.state == nil {
		h.state = newTurnState(h.client, h.reviewThreadID, h.turnID, h.interruptOnCancel)
	}
	return h.state
}

func (h *ReviewHandle) ReviewThreadID() string {
	if h == nil {
		return ""
	}
	return h.reviewThreadID
}

func (h *ReviewHandle) TurnID() string {
	if h == nil {
		return ""
	}
	return h.turnID
}
