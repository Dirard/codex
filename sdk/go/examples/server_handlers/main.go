package main

import (
	"context"

	codex "github.com/openai/codex/sdk/go"
	"github.com/openai/codex/sdk/go/protocol"
)

// codex-go-sdk-handler-docs:account/chatgptAuthTokens/refresh chatgpt-token-refresh
// codex-go-sdk-handler-docs:attestation/generate attestation-generate
// codex-go-sdk-handler-docs:item/commandExecution/requestApproval command-execution-approval
// codex-go-sdk-handler-docs:item/fileChange/requestApproval file-change-approval
// codex-go-sdk-handler-docs:item/tool/requestUserInput tool-user-input
// codex-go-sdk-handler-docs:item/permissions/requestApproval permission-approval
// codex-go-sdk-handler-docs:item/tool/call dynamic-tool-call
// codex-go-sdk-handler-docs:mcpServer/elicitation/request mcp-elicitation
// codex-go-sdk-handler-docs:currentTime/read current-time-read
func handlers() codex.ServerHandlers {
	return codex.ServerHandlers{
		Approvals: codex.ApprovalsHandlerFuncs{
			ItemCommandExecutionRequestApproval: func(context.Context, protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
				return protocol.CommandExecutionRequestApprovalResponse{}, nil
			},
			ItemFileChangeRequestApproval: func(context.Context, protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error) {
				return protocol.FileChangeRequestApprovalResponse{}, nil
			},
		},
		Attestation: codex.AttestationAttestationGenerateFunc(func(context.Context, protocol.AttestationGenerateParams) (protocol.AttestationGenerateResponse, error) {
			return protocol.AttestationGenerateResponse{}, nil
		}),
		ChatGPTTokenRefresh: codex.ChatGPTTokenRefreshAccountChatgptAuthTokensRefreshFunc(func(context.Context, protocol.ChatgptAuthTokensRefreshParams) (protocol.ChatgptAuthTokensRefreshResponse, error) {
			return protocol.ChatgptAuthTokensRefreshResponse{}, nil
		}),
		CurrentTime: codex.CurrentTimeCurrentTimeReadFunc(func(context.Context, protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error) {
			return protocol.CurrentTimeReadResponse{}, nil
		}),
		DynamicTools: codex.DynamicToolsItemToolCallFunc(func(context.Context, protocol.DynamicToolCallParams) (protocol.DynamicToolCallResponse, error) {
			return protocol.DynamicToolCallResponse{}, nil
		}),
		MCPElicitation: codex.MCPElicitationMcpServerElicitationRequestFunc(func(context.Context, protocol.McpServerElicitationRequestParams) (protocol.McpServerElicitationRequestResponse, error) {
			return protocol.McpServerElicitationRequestResponse{}, nil
		}),
		Permissions: codex.PermissionsItemPermissionsRequestApprovalFunc(func(context.Context, protocol.PermissionsRequestApprovalParams) (protocol.PermissionsRequestApprovalResponse, error) {
			return protocol.PermissionsRequestApprovalResponse{}, nil
		}),
		UserInput: codex.UserInputItemToolRequestUserInputFunc(func(context.Context, protocol.ToolRequestUserInputParams) (protocol.ToolRequestUserInputResponse, error) {
			return protocol.ToolRequestUserInputResponse{}, nil
		}),
	}
}

func newClientWithHandlers(ctx context.Context, codexPath string) (*codex.Client, error) {
	return codex.NewClient(ctx, codex.ClientConfig{
		CodexPath: codexPath,
		Handlers:  handlers(),
	})
}

func main() {}
