package codex

import (
	"context"
	"encoding/json"

	"github.com/openai/codex/sdk/go/protocol"
)

type ThreadStartOptions struct {
	Model                      string
	ModelProvider              string
	AllowProviderModelFallback *bool
	ServiceTier                string
	CWD                        string
	RuntimeWorkspaceRoots      []protocol.AbsolutePathBuf
	ApprovalPolicy             protocol.AskForApproval
	ApprovalsReviewer          protocol.ApprovalsReviewer
	Sandbox                    protocol.SandboxMode
	Permissions                string
	Config                     map[string]json.RawMessage
	ServiceName                string
	BaseInstructions           string
	DeveloperInstructions      string
	Personality                protocol.Personality
	MultiAgentMode             protocol.MultiAgentMode
	Ephemeral                  *bool
	HistoryMode                protocol.ThreadHistoryMode
	SessionStartSource         protocol.ThreadStartSource
	ThreadSource               protocol.ThreadSource
	Environments               []protocol.TurnEnvironmentParams
	DynamicTools               []protocol.DynamicToolSpec
	SelectedCapabilityRoots    []protocol.SelectedCapabilityRoot
	MockExperimentalField      string
	ExperimentalRawEvents      *bool
}

type TurnOptions struct {
	ClientUserMessageID        string
	ResponsesAPIClientMetadata map[string]string
	AdditionalContext          map[string]protocol.AdditionalContextEntry
	Environments               []protocol.TurnEnvironmentParams
	CWD                        string
	RuntimeWorkspaceRoots      []protocol.AbsolutePathBuf
	ApprovalPolicy             protocol.AskForApproval
	ApprovalsReviewer          protocol.ApprovalsReviewer
	SandboxPolicy              protocol.SandboxPolicy
	Permissions                string
	Model                      string
	ServiceTier                string
	Effort                     protocol.ReasoningEffort
	Summary                    protocol.ReasoningSummary
	Personality                protocol.Personality
	OutputSchema               OutputSchema
	CollaborationMode          protocol.CollaborationMode
	MultiAgentMode             protocol.MultiAgentMode
}

type SteerOptions struct {
	ClientUserMessageID        string
	ResponsesAPIClientMetadata map[string]string
	AdditionalContext          map[string]protocol.AdditionalContextEntry
}

type Thread struct {
	client *Client
	id     string
}

func (c *ThreadsClient) Start(ctx context.Context, opts ThreadStartOptions) (*Thread, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("thread start", "thread/start"); err != nil {
		return nil, err
	}
	params := threadStartParams(opts)
	response, err := c.client.Raw().ThreadStart(ctx, params)
	if err != nil {
		return nil, err
	}
	return &Thread{client: c.client, id: response.Thread.ID}, nil
}

func (t *Thread) ID() string {
	if t == nil {
		return ""
	}
	return t.id
}

func (t *Thread) Run(ctx context.Context, input Input, opts TurnOptions) (*RunResult, error) {
	handle, err := t.Turn(ctx, input, opts)
	if err != nil {
		return nil, err
	}
	stream, err := handle.Stream(ctx)
	if err != nil {
		return nil, err
	}
	defer stream.Close()
	return collectRunResult(ctx, handle.id, stream)
}

func (t *Thread) Turn(ctx context.Context, input Input, opts TurnOptions) (*TurnHandle, error) {
	if t == nil || t.client == nil {
		return nil, &ClosedError{}
	}
	if err := t.client.ensureHighLevelWorkflowEnabled("turn start", "turn/start"); err != nil {
		return nil, err
	}
	wireInput, err := input.wire(t.client.limits)
	if err != nil {
		return nil, err
	}
	params := turnStartParams(t.id, wireInput, opts)
	response, err := t.client.Raw().TurnStart(ctx, params)
	if err != nil {
		return nil, err
	}
	return &TurnHandle{client: t.client, threadID: t.id, id: response.Turn.ID}, nil
}

func threadStartParams(opts ThreadStartOptions) protocol.ThreadStartParams {
	params := protocol.ThreadStartParams{}
	if opts.Model != "" {
		params.Model = protocol.Some(opts.Model)
	}
	if opts.ModelProvider != "" {
		params.ModelProvider = protocol.Some(opts.ModelProvider)
	}
	if opts.AllowProviderModelFallback != nil {
		params.AllowProviderModelFallback = protocol.SomeNonNull(*opts.AllowProviderModelFallback)
	}
	if opts.ServiceTier != "" {
		params.ServiceTier = protocol.Some(opts.ServiceTier)
	}
	if opts.CWD != "" {
		params.Cwd = protocol.Some(opts.CWD)
	}
	if len(opts.RuntimeWorkspaceRoots) > 0 {
		params.RuntimeWorkspaceRoots = protocol.Some(opts.RuntimeWorkspaceRoots)
	}
	if opts.ApprovalPolicy.Granular.IsSet() {
		params.ApprovalPolicy = protocol.Some(opts.ApprovalPolicy)
	}
	if opts.ApprovalsReviewer != "" {
		params.ApprovalsReviewer = protocol.Some(opts.ApprovalsReviewer)
	}
	if opts.Sandbox != "" {
		params.Sandbox = protocol.Some(opts.Sandbox)
	}
	if opts.Permissions != "" {
		params.Permissions = protocol.Some(opts.Permissions)
	}
	if len(opts.Config) > 0 {
		params.Config = protocol.Some(opts.Config)
	}
	if opts.ServiceName != "" {
		params.ServiceName = protocol.Some(opts.ServiceName)
	}
	if opts.BaseInstructions != "" {
		params.BaseInstructions = protocol.Some(opts.BaseInstructions)
	}
	if opts.DeveloperInstructions != "" {
		params.DeveloperInstructions = protocol.Some(opts.DeveloperInstructions)
	}
	if opts.Personality != "" {
		params.Personality = protocol.Some(opts.Personality)
	}
	if opts.MultiAgentMode != "" {
		params.MultiAgentMode = protocol.Some(opts.MultiAgentMode)
	}
	if opts.Ephemeral != nil {
		params.Ephemeral = protocol.Some(*opts.Ephemeral)
	}
	if opts.HistoryMode != "" {
		params.HistoryMode = protocol.Some(opts.HistoryMode)
	}
	if opts.SessionStartSource != "" {
		params.SessionStartSource = protocol.Some(opts.SessionStartSource)
	}
	if opts.ThreadSource != "" {
		params.ThreadSource = protocol.Some(opts.ThreadSource)
	}
	if len(opts.Environments) > 0 {
		params.Environments = protocol.Some(opts.Environments)
	}
	if len(opts.DynamicTools) > 0 {
		params.DynamicTools = protocol.Some(opts.DynamicTools)
	}
	if len(opts.SelectedCapabilityRoots) > 0 {
		params.SelectedCapabilityRoots = protocol.Some(opts.SelectedCapabilityRoots)
	}
	if opts.MockExperimentalField != "" {
		params.MockExperimentalField = protocol.Some(opts.MockExperimentalField)
	}
	if opts.ExperimentalRawEvents != nil {
		params.ExperimentalRawEvents = protocol.SomeNonNull(*opts.ExperimentalRawEvents)
	}
	return params
}

func turnStartParams(threadID string, input []protocol.UserInput, opts TurnOptions) protocol.TurnStartParams {
	params := protocol.TurnStartParams{ThreadID: threadID, Input: input}
	applyTurnOptions(&params, opts)
	return params
}

func applyTurnOptions(params *protocol.TurnStartParams, opts TurnOptions) {
	if opts.ClientUserMessageID != "" {
		params.ClientUserMessageID = protocol.Some(opts.ClientUserMessageID)
	}
	if len(opts.ResponsesAPIClientMetadata) > 0 {
		params.ResponsesapiClientMetadata = protocol.Some(opts.ResponsesAPIClientMetadata)
	}
	if len(opts.AdditionalContext) > 0 {
		params.AdditionalContext = protocol.Some(opts.AdditionalContext)
	}
	if len(opts.Environments) > 0 {
		params.Environments = protocol.Some(opts.Environments)
	}
	if opts.CWD != "" {
		params.Cwd = protocol.Some(opts.CWD)
	}
	if len(opts.RuntimeWorkspaceRoots) > 0 {
		params.RuntimeWorkspaceRoots = protocol.Some(opts.RuntimeWorkspaceRoots)
	}
	if opts.ApprovalPolicy.Granular.IsSet() {
		params.ApprovalPolicy = protocol.Some(opts.ApprovalPolicy)
	}
	if opts.ApprovalsReviewer != "" {
		params.ApprovalsReviewer = protocol.Some(opts.ApprovalsReviewer)
	}
	if opts.SandboxPolicy.TypeValue != "" {
		params.SandboxPolicy = protocol.Some(opts.SandboxPolicy)
	}
	if opts.Permissions != "" {
		params.Permissions = protocol.Some(opts.Permissions)
	}
	if opts.Model != "" {
		params.Model = protocol.Some(opts.Model)
	}
	if opts.ServiceTier != "" {
		params.ServiceTier = protocol.Some(opts.ServiceTier)
	}
	if opts.Effort != "" {
		params.Effort = protocol.Some(opts.Effort)
	}
	if len(opts.Summary) > 0 {
		params.Summary = protocol.Some(opts.Summary)
	}
	if opts.Personality != "" {
		params.Personality = protocol.Some(opts.Personality)
	}
	if len(opts.OutputSchema.raw) > 0 {
		params.OutputSchema = opts.OutputSchema.rawJSON()
	}
	if opts.CollaborationMode.Mode != "" {
		params.CollaborationMode = protocol.Some(opts.CollaborationMode)
	}
	if opts.MultiAgentMode != "" {
		params.MultiAgentMode = protocol.Some(opts.MultiAgentMode)
	}
}

func applySteerOptions(params *protocol.TurnSteerParams, opts SteerOptions) {
	if opts.ClientUserMessageID != "" {
		params.ClientUserMessageID = protocol.Some(opts.ClientUserMessageID)
	}
	if len(opts.ResponsesAPIClientMetadata) > 0 {
		params.ResponsesapiClientMetadata = protocol.Some(opts.ResponsesAPIClientMetadata)
	}
	if len(opts.AdditionalContext) > 0 {
		params.AdditionalContext = protocol.Some(opts.AdditionalContext)
	}
}
