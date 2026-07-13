package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

type ReviewStartOptions struct {
	ThreadID string
	Target   ReviewTarget
	Delivery ReviewDelivery
}

type ReviewTarget = protocol.ReviewTarget
type ReviewDelivery = protocol.ReviewDelivery

const (
	ReviewDeliveryInline   ReviewDelivery = protocol.ReviewDeliveryInline
	ReviewDeliveryDetached ReviewDelivery = protocol.ReviewDeliveryDetached
)

type ReviewHandle struct {
	client         *Client
	reviewThreadID string
	turnID         string
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
	response, err := c.client.Raw().ReviewStart(ctx, params)
	if err != nil {
		return nil, err
	}
	if response.ReviewThreadID == "" {
		return nil, &UnsupportedError{Reason: "review start response did not include reviewThreadId"}
	}
	if response.Turn.ID == "" {
		return nil, &UnsupportedError{Reason: "review start response did not include turn.id"}
	}
	return &ReviewHandle{client: c.client, reviewThreadID: response.ReviewThreadID, turnID: response.Turn.ID}, nil
}

func (h *ReviewHandle) Events(ctx context.Context) (*ReviewStream, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("review events"); err != nil {
		return nil, err
	}
	stream := h.client.router.subscribeTurn(h.reviewThreadID, h.turnID)
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (h *ReviewHandle) Wait(ctx context.Context) (*ReviewResult, error) {
	stream, err := h.Events(ctx)
	if err != nil {
		return nil, err
	}
	defer stream.Close()
	return collectRunResultForThread(ctx, h.reviewThreadID, h.turnID, stream, h.client.limits)
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
