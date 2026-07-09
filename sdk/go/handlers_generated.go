package codex

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/openai/codex/sdk/go/protocol"
)

type ServerHandlers struct {
	Approvals           ApprovalsHandler
	Attestation         AttestationHandler
	ChatGPTTokenRefresh ChatGPTTokenRefreshHandler
	CurrentTime         CurrentTimeHandler
	DynamicTools        DynamicToolsHandler
	MCPElicitation      MCPElicitationHandler
	Permissions         PermissionsHandler
	UserInput           UserInputHandler
	Unknown             UnknownServerRequestHandler
}

type ApprovalsHandler interface {
	HandleItemCommandExecutionRequestApproval(ctx context.Context, params protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error)
	HandleItemFileChangeRequestApproval(ctx context.Context, params protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error)
}

type ApprovalsHandlerFuncs struct {
	ItemCommandExecutionRequestApproval func(ctx context.Context, params protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error)
	ItemFileChangeRequestApproval       func(ctx context.Context, params protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error)
}

func (f ApprovalsHandlerFuncs) HandleItemCommandExecutionRequestApproval(ctx context.Context, params protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
	var zero protocol.CommandExecutionRequestApprovalResponse
	if f.ItemCommandExecutionRequestApproval == nil {
		return zero, fmt.Errorf("server handler %q is not configured", "item/commandExecution/requestApproval")
	}
	return f.ItemCommandExecutionRequestApproval(ctx, params)
}

func (f ApprovalsHandlerFuncs) HandleItemFileChangeRequestApproval(ctx context.Context, params protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error) {
	var zero protocol.FileChangeRequestApprovalResponse
	if f.ItemFileChangeRequestApproval == nil {
		return zero, fmt.Errorf("server handler %q is not configured", "item/fileChange/requestApproval")
	}
	return f.ItemFileChangeRequestApproval(ctx, params)
}

type ApprovalsItemCommandExecutionRequestApprovalFunc func(ctx context.Context, params protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error)

func (f ApprovalsItemCommandExecutionRequestApprovalFunc) HandleItemCommandExecutionRequestApproval(ctx context.Context, params protocol.CommandExecutionRequestApprovalParams) (protocol.CommandExecutionRequestApprovalResponse, error) {
	return f(ctx, params)
}

type ApprovalsItemFileChangeRequestApprovalFunc func(ctx context.Context, params protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error)

func (f ApprovalsItemFileChangeRequestApprovalFunc) HandleItemFileChangeRequestApproval(ctx context.Context, params protocol.FileChangeRequestApprovalParams) (protocol.FileChangeRequestApprovalResponse, error) {
	return f(ctx, params)
}

type AttestationHandler interface {
	HandleAttestationGenerate(ctx context.Context, params protocol.AttestationGenerateParams) (protocol.AttestationGenerateResponse, error)
}

type AttestationAttestationGenerateFunc func(ctx context.Context, params protocol.AttestationGenerateParams) (protocol.AttestationGenerateResponse, error)

func (f AttestationAttestationGenerateFunc) HandleAttestationGenerate(ctx context.Context, params protocol.AttestationGenerateParams) (protocol.AttestationGenerateResponse, error) {
	return f(ctx, params)
}

type ChatGPTTokenRefreshHandler interface {
	HandleAccountChatgptAuthTokensRefresh(ctx context.Context, params protocol.ChatgptAuthTokensRefreshParams) (protocol.ChatgptAuthTokensRefreshResponse, error)
}

type ChatGPTTokenRefreshAccountChatgptAuthTokensRefreshFunc func(ctx context.Context, params protocol.ChatgptAuthTokensRefreshParams) (protocol.ChatgptAuthTokensRefreshResponse, error)

func (f ChatGPTTokenRefreshAccountChatgptAuthTokensRefreshFunc) HandleAccountChatgptAuthTokensRefresh(ctx context.Context, params protocol.ChatgptAuthTokensRefreshParams) (protocol.ChatgptAuthTokensRefreshResponse, error) {
	return f(ctx, params)
}

type CurrentTimeHandler interface {
	HandleCurrentTimeRead(ctx context.Context, params protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error)
}

type CurrentTimeCurrentTimeReadFunc func(ctx context.Context, params protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error)

func (f CurrentTimeCurrentTimeReadFunc) HandleCurrentTimeRead(ctx context.Context, params protocol.CurrentTimeReadParams) (protocol.CurrentTimeReadResponse, error) {
	return f(ctx, params)
}

type DynamicToolsHandler interface {
	HandleItemToolCall(ctx context.Context, params protocol.DynamicToolCallParams) (protocol.DynamicToolCallResponse, error)
}

type DynamicToolsItemToolCallFunc func(ctx context.Context, params protocol.DynamicToolCallParams) (protocol.DynamicToolCallResponse, error)

func (f DynamicToolsItemToolCallFunc) HandleItemToolCall(ctx context.Context, params protocol.DynamicToolCallParams) (protocol.DynamicToolCallResponse, error) {
	return f(ctx, params)
}

type MCPElicitationHandler interface {
	HandleMcpServerElicitationRequest(ctx context.Context, params protocol.McpServerElicitationRequestParams) (protocol.McpServerElicitationRequestResponse, error)
}

type MCPElicitationMcpServerElicitationRequestFunc func(ctx context.Context, params protocol.McpServerElicitationRequestParams) (protocol.McpServerElicitationRequestResponse, error)

func (f MCPElicitationMcpServerElicitationRequestFunc) HandleMcpServerElicitationRequest(ctx context.Context, params protocol.McpServerElicitationRequestParams) (protocol.McpServerElicitationRequestResponse, error) {
	return f(ctx, params)
}

type PermissionsHandler interface {
	HandleItemPermissionsRequestApproval(ctx context.Context, params protocol.PermissionsRequestApprovalParams) (protocol.PermissionsRequestApprovalResponse, error)
}

type PermissionsItemPermissionsRequestApprovalFunc func(ctx context.Context, params protocol.PermissionsRequestApprovalParams) (protocol.PermissionsRequestApprovalResponse, error)

func (f PermissionsItemPermissionsRequestApprovalFunc) HandleItemPermissionsRequestApproval(ctx context.Context, params protocol.PermissionsRequestApprovalParams) (protocol.PermissionsRequestApprovalResponse, error) {
	return f(ctx, params)
}

type UserInputHandler interface {
	HandleItemToolRequestUserInput(ctx context.Context, params protocol.ToolRequestUserInputParams) (protocol.ToolRequestUserInputResponse, error)
}

type UserInputItemToolRequestUserInputFunc func(ctx context.Context, params protocol.ToolRequestUserInputParams) (protocol.ToolRequestUserInputResponse, error)

func (f UserInputItemToolRequestUserInputFunc) HandleItemToolRequestUserInput(ctx context.Context, params protocol.ToolRequestUserInputParams) (protocol.ToolRequestUserInputResponse, error) {
	return f(ctx, params)
}

func (h ServerHandlers) DispatchServerRequest(ctx context.Context, method string, params json.RawMessage) (any, error) {
	switch method {
	case "account/chatgptAuthTokens/refresh":
		decoded, err := decodeAccountChatgptAuthTokensRefreshServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.ChatGPTTokenRefresh == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.ChatGPTTokenRefresh.HandleAccountChatgptAuthTokensRefresh(ctx, decoded)
	case "applyPatchApproval":
		if _, err := decodeApplyPatchApprovalServerRequest(params); err != nil {
			return nil, err
		}
		return nil, fmt.Errorf("server request %q has no public handler: applyPatchApproval", method)
	case "attestation/generate":
		decoded, err := decodeAttestationGenerateServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.Attestation == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.Attestation.HandleAttestationGenerate(ctx, decoded)
	case "execCommandApproval":
		if _, err := decodeExecCommandApprovalServerRequest(params); err != nil {
			return nil, err
		}
		return nil, fmt.Errorf("server request %q has no public handler: execCommandApproval", method)
	case "item/commandExecution/requestApproval":
		decoded, err := decodeItemCommandExecutionRequestApprovalServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.Approvals == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.Approvals.HandleItemCommandExecutionRequestApproval(ctx, decoded)
	case "item/fileChange/requestApproval":
		decoded, err := decodeItemFileChangeRequestApprovalServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.Approvals == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.Approvals.HandleItemFileChangeRequestApproval(ctx, decoded)
	case "item/tool/requestUserInput":
		decoded, err := decodeItemToolRequestUserInputServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.UserInput == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.UserInput.HandleItemToolRequestUserInput(ctx, decoded)
	case "item/permissions/requestApproval":
		decoded, err := decodeItemPermissionsRequestApprovalServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.Permissions == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.Permissions.HandleItemPermissionsRequestApproval(ctx, decoded)
	case "item/tool/call":
		decoded, err := decodeItemToolCallServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.DynamicTools == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.DynamicTools.HandleItemToolCall(ctx, decoded)
	case "mcpServer/elicitation/request":
		decoded, err := decodeMcpServerElicitationRequestServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.MCPElicitation == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.MCPElicitation.HandleMcpServerElicitationRequest(ctx, decoded)
	case "currentTime/read":
		decoded, err := decodeCurrentTimeReadServerRequest(params)
		if err != nil {
			return nil, err
		}
		if h.CurrentTime == nil {
			return nil, &UnsupportedError{Reason: fmt.Sprintf("server handler %q is not configured", method)}
		}
		return h.CurrentTime.HandleCurrentTimeRead(ctx, decoded)
	default:
		if h.Unknown != nil {
			return h.Unknown.HandleUnknownServerRequest(ctx, UnknownServerRequest{Method: method, Params: append(json.RawMessage(nil), params...)})
		}
		return nil, &UnsupportedError{Reason: fmt.Sprintf("unsupported server request method %q", method)}
	}
}

func decodeAccountChatgptAuthTokensRefreshServerRequest(params json.RawMessage) (protocol.ChatgptAuthTokensRefreshParams, error) {
	var decoded protocol.ChatgptAuthTokensRefreshParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeApplyPatchApprovalServerRequest(params json.RawMessage) (json.RawMessage, error) {
	if !json.Valid(params) {
		return nil, fmt.Errorf("invalid JSON params")
	}
	return params, nil
}

func decodeAttestationGenerateServerRequest(params json.RawMessage) (protocol.AttestationGenerateParams, error) {
	var decoded protocol.AttestationGenerateParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeExecCommandApprovalServerRequest(params json.RawMessage) (json.RawMessage, error) {
	if !json.Valid(params) {
		return nil, fmt.Errorf("invalid JSON params")
	}
	return params, nil
}

func decodeItemCommandExecutionRequestApprovalServerRequest(params json.RawMessage) (protocol.CommandExecutionRequestApprovalParams, error) {
	var decoded protocol.CommandExecutionRequestApprovalParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeItemFileChangeRequestApprovalServerRequest(params json.RawMessage) (protocol.FileChangeRequestApprovalParams, error) {
	var decoded protocol.FileChangeRequestApprovalParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeItemToolRequestUserInputServerRequest(params json.RawMessage) (protocol.ToolRequestUserInputParams, error) {
	var decoded protocol.ToolRequestUserInputParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeItemPermissionsRequestApprovalServerRequest(params json.RawMessage) (protocol.PermissionsRequestApprovalParams, error) {
	var decoded protocol.PermissionsRequestApprovalParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeItemToolCallServerRequest(params json.RawMessage) (protocol.DynamicToolCallParams, error) {
	var decoded protocol.DynamicToolCallParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeMcpServerElicitationRequestServerRequest(params json.RawMessage) (protocol.McpServerElicitationRequestParams, error) {
	var decoded protocol.McpServerElicitationRequestParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

func decodeCurrentTimeReadServerRequest(params json.RawMessage) (protocol.CurrentTimeReadParams, error) {
	var decoded protocol.CurrentTimeReadParams
	if err := json.Unmarshal(params, &decoded); err != nil {
		return decoded, err
	}
	return decoded, nil
}

type generatedServerHandlerMetadataRow struct {
	Method       string
	Visibility   string
	Capability   string
	HandlerOwner string
}

var generatedServerHandlerMetadata = []generatedServerHandlerMetadataRow{
	{Method: "account/chatgptAuthTokens/refresh", Visibility: "sdk-public", Capability: "chatgpt-token-refresh", HandlerOwner: "ServerHandlers.ChatGPTTokenRefresh"},
	{Method: "applyPatchApproval", Visibility: "compatibility-only", Capability: "legacy-apply-patch-approval", HandlerOwner: "internal compatibility dispatch/decode only; no public handler field"},
	{Method: "attestation/generate", Visibility: "sdk-public", Capability: "attestation-generate", HandlerOwner: "ServerHandlers.Attestation"},
	{Method: "execCommandApproval", Visibility: "compatibility-only", Capability: "legacy-exec-command-approval", HandlerOwner: "internal compatibility dispatch/decode only; no public handler field"},
	{Method: "item/commandExecution/requestApproval", Visibility: "sdk-public", Capability: "command-execution-approval", HandlerOwner: "ServerHandlers.Approvals"},
	{Method: "item/fileChange/requestApproval", Visibility: "sdk-public", Capability: "file-change-approval", HandlerOwner: "ServerHandlers.Approvals"},
	{Method: "item/tool/requestUserInput", Visibility: "sdk-public", Capability: "tool-user-input", HandlerOwner: "ServerHandlers.UserInput"},
	{Method: "item/permissions/requestApproval", Visibility: "sdk-public", Capability: "permission-approval", HandlerOwner: "ServerHandlers.Permissions"},
	{Method: "item/tool/call", Visibility: "sdk-public", Capability: "dynamic-tool-call", HandlerOwner: "ServerHandlers.DynamicTools"},
	{Method: "mcpServer/elicitation/request", Visibility: "sdk-public", Capability: "mcp-elicitation", HandlerOwner: "ServerHandlers.MCPElicitation"},
	{Method: "currentTime/read", Visibility: "experimental-public", Capability: "current-time-read", HandlerOwner: "ServerHandlers.CurrentTime"},
}
