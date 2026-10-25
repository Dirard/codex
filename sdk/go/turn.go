package codex

import (
	"context"

	"github.com/openai/codex/sdk/go/protocol"
)

// TurnHandle follows one turn. Its first Stream or Wait subscribes to the turn's events;
// the outcome is recorded once, so repeated and concurrent calls return the same result.
type TurnHandle struct {
	client   *Client
	threadID string
	id       string
	state    *turnState
}

func newTurnHandle(client *Client, threadID string, turnID string, interruptOnCancel bool) *TurnHandle {
	return &TurnHandle{
		client:   client,
		threadID: threadID,
		id:       turnID,
		state:    newTurnState(client, threadID, turnID, interruptOnCancel),
	}
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

// Stream returns the turn's events. The handle's first Stream or Wait also gets the events
// buffered since the turn started, and a Stream opened after the handle's earlier streams
// closed first gets the events that arrived meanwhile (up to the smaller of
// PendingTurnQueue and ResourceStreamQueue; beyond that the newest, then an
// *OverflowError). ctx bounds the stream's lifetime. After the turn finished, the stream
// closes with the turn's terminal error, which is nil after turn/completed, once it
// delivered those events.
func (h *TurnHandle) Stream(ctx context.Context) (*TurnStream, error) {
	if h == nil || h.client == nil || h.client.router == nil || h.state == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("turn stream"); err != nil {
		return nil, err
	}
	return h.state.stream(ctx), nil
}

// Wait returns the turn's result once turn/completed arrives, even when a stream of the
// handle already consumed its events. Every call gets the same result or error; each gets
// its own Items slice, while values nested in the items are shared and must be treated as
// read-only. When ctx ends first, Wait returns its error and the handle keeps following
// the turn, so a later Wait still returns the result.
func (h *TurnHandle) Wait(ctx context.Context) (*RunResult, error) {
	if h == nil || h.client == nil || h.client.router == nil || h.state == nil {
		return nil, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("turn wait"); err != nil {
		return nil, err
	}
	return h.state.wait(ctx)
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

func (h *TurnHandle) UpdateSettings(ctx context.Context, params protocol.TurnSettingsUpdateParams) (protocol.TurnSettingsUpdateResponse, error) {
	if h == nil || h.client == nil {
		return protocol.TurnSettingsUpdateResponse{}, &ClosedError{}
	}
	if err := h.client.ensureHighLevelEnabled("turn settings update"); err != nil {
		return protocol.TurnSettingsUpdateResponse{}, err
	}
	params.ThreadID = h.threadID
	params.TurnID = h.id
	return h.client.Raw().TurnSettingsUpdate(ctx, params)
}

// runResultCollector folds the notifications of one turn into its RunResult.
type runResultCollector struct {
	threadID  string
	turnID    string
	limits    ClientLimits
	result    RunResult
	items     []protocol.ThreadItem
	itemBytes int64
	completed bool
}

func newRunResultCollector(threadID string, turnID string, limits ClientLimits) runResultCollector {
	return runResultCollector{threadID: threadID, turnID: turnID, limits: limits, result: RunResult{TurnID: turnID}}
}

// observe records one notification and fails when the result exceeds the configured limits.
func (c *runResultCollector) observe(notification Notification) error {
	switch payload := notification.Payload.(type) {
	case protocol.ItemCompletedNotification:
		if payload.TurnID != c.turnID || !threadMatches(c.threadID, payload.ThreadID) {
			return nil
		}
		return appendRunResultItems(&c.items, &c.itemBytes, []protocol.ThreadItem{payload.Item}, len(notification.RawParams), c.limits)
	case protocol.TurnCompletedNotification:
		if payload.Turn.ID != c.turnID || !threadMatches(c.threadID, payload.ThreadID) {
			return nil
		}
		c.completed = true
		c.result.Status = payload.Turn.Status
		c.result.Error = payload.Turn.Error
		c.result.StartedAt = payload.Turn.StartedAt
		c.result.CompletedAt = payload.Turn.CompletedAt
		c.result.DurationMs = payload.Turn.DurationMs
		if len(c.items) == 0 && len(payload.Turn.Items) > 0 {
			return appendRunResultItems(&c.items, &c.itemBytes, payload.Turn.Items, len(notification.RawParams), c.limits)
		}
	case protocol.ThreadTokenUsageUpdatedNotification:
		if payload.TurnID != c.turnID || !threadMatches(c.threadID, payload.ThreadID) {
			return nil
		}
		c.result.TokenUsage = protocol.Some(payload.TokenUsage)
	}
	return nil
}

// finish returns the result once the stream ended; a turn without turn/completed has none.
func (c *runResultCollector) finish() (*RunResult, error) {
	if !c.completed {
		return nil, &DecodeError{Reason: "turn stream closed before turn/completed"}
	}
	result := c.result
	result.Items = c.items
	result.FinalResponse = finalResponseFromItems(c.items)
	return &result, nil
}

// clone copies a result so callers that share one outcome cannot alias its items.
func (r *RunResult) clone() *RunResult {
	if r == nil {
		return nil
	}
	copied := *r
	copied.Items = append([]protocol.ThreadItem(nil), r.Items...)
	return &copied
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
