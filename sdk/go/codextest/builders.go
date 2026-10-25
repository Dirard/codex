package codextest

import "github.com/openai/codex/sdk/go/protocol"

// ThreadStartResponse returns a minimal valid thread/start response for an idle thread.
func ThreadStartResponse(threadID string) protocol.ThreadStartResponse {
	return protocol.ThreadStartResponse{
		ApprovalPolicy:    protocol.AskForApprovalNever,
		ApprovalsReviewer: protocol.ApprovalsReviewerAutoReview,
		Cwd:               protocol.AbsolutePathBuf(Home),
		Model:             "codextest-model",
		ModelProvider:     "codextest",
		Sandbox:           protocol.SandboxPolicy{TypeValue: "dangerFullAccess"},
		Thread: protocol.Thread{
			CliVersion:    "0.0.0-dev",
			CreatedAt:     1,
			Cwd:           protocol.AbsolutePathBuf(Home),
			ID:            threadID,
			ModelProvider: "codextest",
			ProjectID:     protocol.Null[string](),
			SessionID:     "codextest-session-" + threadID,
			Source:        protocol.SessionSourceCli,
			Status:        protocol.ThreadStatus{TypeValue: "idle"},
			Turns:         []protocol.Turn{},
			UpdatedAt:     1,
		},
	}
}

// TurnStartResponse returns a valid turn/start response for a turn in progress.
func TurnStartResponse(turnID string) protocol.TurnStartResponse {
	return protocol.TurnStartResponse{
		Turn: protocol.Turn{ID: turnID, Items: []protocol.ThreadItem{}, Status: protocol.TurnStatusInProgress},
	}
}

// AgentMessageCompleted returns an item/completed notification with the turn's final
// agent message.
func AgentMessageCompleted(threadID string, turnID string, itemID string, text string) protocol.ItemCompletedNotification {
	return protocol.ItemCompletedNotification{
		ThreadID: threadID,
		TurnID:   turnID,
		Item: protocol.ThreadItem{
			ID:        protocol.SomeNonNull(itemID),
			TypeValue: "agentMessage",
			Text:      protocol.SomeNonNull(text),
			Phase:     protocol.Some(protocol.MessagePhaseFinalAnswer),
		},
	}
}

// TurnCompleted returns a turn/completed notification with the given status.
func TurnCompleted(threadID string, turnID string, status protocol.TurnStatus) protocol.TurnCompletedNotification {
	return protocol.TurnCompletedNotification{
		ThreadID: threadID,
		Turn:     protocol.Turn{ID: turnID, Items: []protocol.ThreadItem{}, Status: status},
	}
}

// CompleteTurn sends the notifications of a turn that answered finalResponse: its final
// agent message and turn/completed.
func (s *Server) CompleteTurn(threadID string, turnID string, finalResponse string) error {
	if err := s.Notify("item/completed", AgentMessageCompleted(threadID, turnID, turnID+"-message", finalResponse)); err != nil {
		return err
	}
	return s.Notify("turn/completed", TurnCompleted(threadID, turnID, protocol.TurnStatusCompleted))
}
