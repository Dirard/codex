package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

type TurnHandle struct {
	client   *Client
	threadID string
	id       string
}

type RunResult struct {
	TurnID        string
	Status        protocol.TurnStatus
	Error         protocol.Optional[protocol.TurnError]
	StartedAt     protocol.Optional[int64]
	CompletedAt   protocol.Optional[int64]
	DurationMs    protocol.Optional[int64]
	FinalResponse string
	Items         []protocol.ThreadItem
	TokenUsage    protocol.Optional[protocol.ThreadTokenUsage]
}

type TurnStream = NotificationStream

func (h *TurnHandle) ID() string {
	if h == nil {
		return ""
	}
	return h.id
}

func (h *TurnHandle) Stream(ctx context.Context) (*TurnStream, error) {
	if h == nil || h.client == nil || h.client.router == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("turn stream"); err != nil {
		return nil, err
	}
	stream := h.client.router.subscribeTurn(h.threadID, h.id)
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (h *TurnHandle) Steer(ctx context.Context, input Input, opts ...SteerOptions) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("turn steer"); err != nil {
		return err
	}
	wireInput, err := input.wire(h.client.limits)
	if err != nil {
		return err
	}
	params := protocol.TurnSteerParams{
		ThreadID:       h.threadID,
		ExpectedTurnID: h.id,
		Input:          wireInput,
	}
	if len(opts) > 0 {
		applySteerOptions(&params, opts[0])
	}
	_, err = h.client.Raw().TurnSteer(ctx, params)
	return err
}

func (h *TurnHandle) Interrupt(ctx context.Context) error {
	if h == nil || h.client == nil {
		return &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("turn interrupt"); err != nil {
		return err
	}
	_, err := h.client.Raw().TurnInterrupt(ctx, protocol.TurnInterruptParams{
		ThreadID: h.threadID,
		TurnID:   h.id,
	})
	return err
}

func collectRunResult(ctx context.Context, turnID string, stream *NotificationStream, limits ClientLimits) (*RunResult, error) {
	return collectRunResultForThread(ctx, "", turnID, stream, limits)
}

func collectRunResultForThread(ctx context.Context, threadID string, turnID string, stream *NotificationStream, limits ClientLimits) (*RunResult, error) {
	result := &RunResult{TurnID: turnID}
	var items []protocol.ThreadItem
	var itemBytes int64
	completed := false
	for {
		notification, ok := stream.Next(ctx)
		if !ok {
			if err := stream.Err(); err != nil {
				return nil, err
			}
			if !completed {
				return nil, &DecodeError{Reason: "turn stream closed before turn/completed"}
			}
			result.Items = items
			result.FinalResponse = finalResponseFromItems(items)
			return result, nil
		}
		switch payload := notification.Payload.(type) {
		case protocol.ItemCompletedNotification:
			if payload.TurnID != turnID || !threadMatches(threadID, payload.ThreadID) {
				continue
			}
			if err := appendRunResultItems(&items, &itemBytes, []protocol.ThreadItem{payload.Item}, len(notification.RawParams), limits); err != nil {
				return nil, err
			}
		case protocol.TurnCompletedNotification:
			if payload.Turn.ID != turnID || !threadMatches(threadID, payload.ThreadID) {
				continue
			}
			completed = true
			result.Status = payload.Turn.Status
			result.Error = payload.Turn.Error
			result.StartedAt = payload.Turn.StartedAt
			result.CompletedAt = payload.Turn.CompletedAt
			result.DurationMs = payload.Turn.DurationMs
			if len(items) == 0 && len(payload.Turn.Items) > 0 {
				if err := appendRunResultItems(&items, &itemBytes, payload.Turn.Items, len(notification.RawParams), limits); err != nil {
					return nil, err
				}
			}
		case protocol.ThreadTokenUsageUpdatedNotification:
			if payload.TurnID != turnID || !threadMatches(threadID, payload.ThreadID) {
				continue
			}
			result.TokenUsage = protocol.Some(payload.TokenUsage)
		}
	}
}

func appendRunResultItems(items *[]protocol.ThreadItem, encodedBytes *int64, additional []protocol.ThreadItem, additionalBytes int, limits ClientLimits) error {
	if len(additional) > limits.MaxRunResultItems-len(*items) {
		return &OverflowError{Reason: "run result item count exceeded configured limit"}
	}
	if int64(additionalBytes) > limits.MaxRunResultBytes-*encodedBytes {
		return &OverflowError{Reason: "run result item bytes exceeded configured limit"}
	}
	*items = append(*items, additional...)
	*encodedBytes += int64(additionalBytes)
	return nil
}

func threadMatches(expected string, actual string) bool {
	return expected == "" || actual == expected
}

func finalResponseFromItems(items []protocol.ThreadItem) string {
	var fallback string
	fallbackSet := false
	for i := len(items) - 1; i >= 0; i-- {
		item := items[i]
		if item.TypeValue != "agentMessage" {
			continue
		}
		text, ok := item.Text.Value()
		if !ok {
			continue
		}
		phase, hasPhase := item.Phase.Value()
		if hasPhase {
			if phase == protocol.MessagePhaseFinalAnswer {
				return text
			}
			continue
		}
		if !fallbackSet {
			fallback = text
			fallbackSet = true
		}
	}
	return fallback
}
