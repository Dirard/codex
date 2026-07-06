package codex

// ServerHandlers groups handlers for app-server requests sent to the SDK client.
type ServerHandlers struct {
	Approvals ApprovalHandler
}

// ApprovalHandler handles approval requests.
type ApprovalHandler interface {
	HandleApproval() error
}
