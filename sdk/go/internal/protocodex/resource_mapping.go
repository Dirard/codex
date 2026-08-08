package protocodex

var serverHandlerMappings = []ServerHandlerMapping{
	{Method: "account/chatgptAuthTokens/refresh", HandlerOwner: "ServerHandlers.ChatGPTTokenRefresh", Visibility: "sdk-public", Capability: "chatgpt-token-refresh"},
	{Method: "applyPatchApproval", HandlerOwner: "internal compatibility dispatch/decode only; no public handler field", Visibility: "compatibility-only", Capability: "legacy-apply-patch-approval"},
	{Method: "attestation/generate", HandlerOwner: "ServerHandlers.Attestation", Visibility: "sdk-public", Capability: "attestation-generate"},
	{Method: "execCommandApproval", HandlerOwner: "internal compatibility dispatch/decode only; no public handler field", Visibility: "compatibility-only", Capability: "legacy-exec-command-approval"},
	{Method: "item/commandExecution/requestApproval", HandlerOwner: "ServerHandlers.Approvals", Visibility: "sdk-public", Capability: "command-execution-approval"},
	{Method: "item/fileChange/requestApproval", HandlerOwner: "ServerHandlers.Approvals", Visibility: "sdk-public", Capability: "file-change-approval"},
	{Method: "item/tool/requestUserInput", HandlerOwner: "ServerHandlers.UserInput", Visibility: "sdk-public", Capability: "tool-user-input"},
	{Method: "item/permissions/requestApproval", HandlerOwner: "ServerHandlers.Permissions", Visibility: "sdk-public", Capability: "permission-approval"},
	{Method: "item/tool/call", HandlerOwner: "ServerHandlers.DynamicTools", Visibility: "sdk-public", Capability: "dynamic-tool-call"},
	{Method: "mcpServer/elicitation/request", HandlerOwner: "ServerHandlers.MCPElicitation", Visibility: "sdk-public", Capability: "mcp-elicitation"},
	{Method: "currentTime/read", HandlerOwner: "ServerHandlers.CurrentTime", Visibility: "experimental-public", Capability: "current-time-read"},
}
