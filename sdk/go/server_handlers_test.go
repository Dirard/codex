package codex

import (
	"context"
	"testing"

	"github.com/openai/codex/sdk/go/protocol"
)

func TestGeneratedMultiMethodHandlerFuncsSatisfyServerHandlers(t *testing.T) {
	handlers := ServerHandlers{
		Approvals: ApprovalsHandlerFuncs{
			ItemCommandExecutionRequestApproval: func(context.Context, protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
				return protocol.CommandExecutionRequestApprovalResponse{}, nil
			},
			ItemFileChangeRequestApproval: func(context.Context, protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error) {
				return protocol.FileChangeRequestApprovalResponse{}, nil
			},
		},
	}
	if handlers.Approvals == nil {
		t.Fatal("Approvals handler should be assignable")
	}
}
