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

type ThreadResumeOptions struct {
	ThreadID              string
	Model                 string
	ModelProvider         string
	ServiceTier           string
	CWD                   string
	Path                  string
	RuntimeWorkspaceRoots []protocol.AbsolutePathBuf
	ApprovalPolicy        protocol.AskForApproval
	ApprovalsReviewer     protocol.ApprovalsReviewer
	Sandbox               protocol.SandboxMode
	Permissions           string
	Config                map[string]json.RawMessage
	BaseInstructions      string
	DeveloperInstructions string
	Personality           protocol.Personality
	ExcludeTurns          *bool
	History               []protocol.ResponseItem
	InitialTurnsPage      *protocol.ThreadResumeInitialTurnsPageParams
}

type ThreadForkOptions struct {
	ThreadID              string
	Model                 string
	ModelProvider         string
	ServiceTier           string
	CWD                   string
	Path                  string
	RuntimeWorkspaceRoots []protocol.AbsolutePathBuf
	ApprovalPolicy        protocol.AskForApproval
	ApprovalsReviewer     protocol.ApprovalsReviewer
	Sandbox               protocol.SandboxMode
	Permissions           string
	Config                map[string]json.RawMessage
	BaseInstructions      string
	DeveloperInstructions string
	Ephemeral             *bool
	ExcludeTurns          *bool
	LastTurnID            string
	ThreadSource          protocol.ThreadSource
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

func (c *ThreadsClient) Resume(ctx context.Context, opts ThreadResumeOptions) (*Thread, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("thread resume", "thread/resume"); err != nil {
		return nil, err
	}
	response, err := c.client.Raw().ThreadResume(ctx, threadResumeParams(opts))
	if err != nil {
		return nil, err
	}
	return &Thread{client: c.client, id: response.Thread.ID}, nil
}

func (c *ThreadsClient) Fork(ctx context.Context, opts ThreadForkOptions) (*Thread, error) {
	if c == nil || c.client == nil {
		return nil, &ClosedError{}
	}
	if err := c.client.ensureHighLevelWorkflowEnabled("thread fork", "thread/fork"); err != nil {
		return nil, err
	}
	response, err := c.client.Raw().ThreadFork(ctx, threadForkParams(opts))
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

func (c *ThreadsClient) Archive(ctx context.Context, params protocol.ThreadArchiveParams) (protocol.ThreadArchiveResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadArchiveResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadArchive(ctx, params)
}

func (c *ThreadsClient) Delete(ctx context.Context, params protocol.ThreadDeleteParams) (protocol.ThreadDeleteResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadDeleteResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadDelete(ctx, params)
}

func (t *Thread) Unsubscribe(ctx context.Context) error {
	if t == nil || t.client == nil {
		return &ClosedError{}
	}
	if err := t.client.ensureHighLevelEnabled("thread unsubscribe"); err != nil {
		return err
	}
	_, err := t.client.Raw().ThreadUnsubscribe(ctx, protocol.ThreadUnsubscribeParams{ThreadID: t.id})
	return err
}

func (c *ThreadsClient) SetName(ctx context.Context, params protocol.ThreadSetNameParams) (protocol.ThreadSetNameResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadSetNameResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadNameSet(ctx, params)
}

func (c *ThreadsClient) SetGoal(ctx context.Context, params protocol.ThreadGoalSetParams) (protocol.ThreadGoalSetResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadGoalSetResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadGoalSet(ctx, params)
}

func (c *ThreadsClient) GetGoal(ctx context.Context, params protocol.ThreadGoalGetParams) (protocol.ThreadGoalGetResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadGoalGetResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadGoalGet(ctx, params)
}

func (c *ThreadsClient) ClearGoal(ctx context.Context, params protocol.ThreadGoalClearParams) (protocol.ThreadGoalClearResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadGoalClearResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadGoalClear(ctx, params)
}

func (c *ThreadsClient) UpdateMetadata(ctx context.Context, params protocol.ThreadMetadataUpdateParams) (protocol.ThreadMetadataUpdateResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadMetadataUpdateResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadMetadataUpdate(ctx, params)
}

func (c *ThreadsClient) Unarchive(ctx context.Context, params protocol.ThreadUnarchiveParams) (protocol.ThreadUnarchiveResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadUnarchiveResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadUnarchive(ctx, params)
}

func (c *ThreadsClient) StartCompaction(ctx context.Context, params protocol.ThreadCompactStartParams) (protocol.ThreadCompactStartResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadCompactStartResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadCompactStart(ctx, params)
}

func (c *ThreadsClient) ShellCommand(ctx context.Context, params protocol.ThreadShellCommandParams) (protocol.ThreadShellCommandResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadShellCommandResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadShellCommand(ctx, params)
}

func (c *ThreadsClient) ApproveGuardianDeniedAction(ctx context.Context, params protocol.ThreadApproveGuardianDeniedActionParams) (protocol.ThreadApproveGuardianDeniedActionResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadApproveGuardianDeniedActionResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadApproveGuardianDeniedAction(ctx, params)
}

func (c *ThreadsClient) Rollback(ctx context.Context, params protocol.ThreadRollbackParams) (protocol.ThreadRollbackResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadRollbackResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadRollback(ctx, params)
}

func (c *ThreadsClient) List(ctx context.Context, params protocol.ThreadListParams) (protocol.ThreadListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadList(ctx, params)
}

func (c *ThreadsClient) ListLoaded(ctx context.Context, params protocol.ThreadLoadedListParams) (protocol.ThreadLoadedListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadLoadedListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadLoadedList(ctx, params)
}

func (c *ThreadsClient) Read(ctx context.Context, params protocol.ThreadReadParams) (protocol.ThreadReadResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadReadResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadRead(ctx, params)
}

func (c *ThreadsClient) InjectItems(ctx context.Context, params protocol.ThreadInjectItemsParams) (protocol.ThreadInjectItemsResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadInjectItemsResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadInjectItems(ctx, params)
}

func (t *Thread) IncrementElicitation(ctx context.Context) (protocol.ThreadIncrementElicitationResponse, error) {
	if t == nil || t.client == nil {
		return protocol.ThreadIncrementElicitationResponse{}, &ClosedError{}
	}
	if err := t.client.ensureHighLevelEnabled("thread increment elicitation"); err != nil {
		return protocol.ThreadIncrementElicitationResponse{}, err
	}
	return t.client.Raw().ThreadIncrementElicitation(ctx, protocol.ThreadIncrementElicitationParams{ThreadID: t.id})
}

func (c *ThreadsClient) IncrementElicitation(ctx context.Context, threadID string) (protocol.ThreadIncrementElicitationResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadIncrementElicitationResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadIncrementElicitation(ctx, protocol.ThreadIncrementElicitationParams{ThreadID: threadID})
}

func (t *Thread) DecrementElicitation(ctx context.Context) (protocol.ThreadDecrementElicitationResponse, error) {
	if t == nil || t.client == nil {
		return protocol.ThreadDecrementElicitationResponse{}, &ClosedError{}
	}
	if err := t.client.ensureHighLevelEnabled("thread decrement elicitation"); err != nil {
		return protocol.ThreadDecrementElicitationResponse{}, err
	}
	return t.client.Raw().ThreadDecrementElicitation(ctx, protocol.ThreadDecrementElicitationParams{ThreadID: t.id})
}

func (c *ThreadsClient) DecrementElicitation(ctx context.Context, threadID string) (protocol.ThreadDecrementElicitationResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadDecrementElicitationResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadDecrementElicitation(ctx, protocol.ThreadDecrementElicitationParams{ThreadID: threadID})
}

func (c *ThreadsClient) UpdateSettings(ctx context.Context, params protocol.ThreadSettingsUpdateParams) (protocol.ThreadSettingsUpdateResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadSettingsUpdateResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadSettingsUpdate(ctx, params)
}

func (c *ThreadsClient) SetMemoryMode(ctx context.Context, params protocol.ThreadMemoryModeSetParams) (protocol.ThreadMemoryModeSetResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadMemoryModeSetResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadMemoryModeSet(ctx, params)
}

func (c *ThreadsClient) CleanBackgroundTerminals(ctx context.Context, params protocol.ThreadBackgroundTerminalsCleanParams) (protocol.ThreadBackgroundTerminalsCleanResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadBackgroundTerminalsCleanResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadBackgroundTerminalsClean(ctx, params)
}

func (c *ThreadsClient) ListBackgroundTerminals(ctx context.Context, params protocol.ThreadBackgroundTerminalsListParams) (protocol.ThreadBackgroundTerminalsListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadBackgroundTerminalsListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadBackgroundTerminalsList(ctx, params)
}

func (c *ThreadsClient) TerminateBackgroundTerminal(ctx context.Context, params protocol.ThreadBackgroundTerminalsTerminateParams) (protocol.ThreadBackgroundTerminalsTerminateResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadBackgroundTerminalsTerminateResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadBackgroundTerminalsTerminate(ctx, params)
}

func (c *ThreadsClient) Search(ctx context.Context, params protocol.ThreadSearchParams) (protocol.ThreadSearchResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadSearchResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadSearch(ctx, params)
}

func (c *ThreadsClient) ListTurns(ctx context.Context, params protocol.ThreadTurnsListParams) (protocol.ThreadTurnsListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadTurnsListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadTurnsList(ctx, params)
}

func (c *ThreadsClient) ListItems(ctx context.Context, params protocol.ThreadItemsListParams) (protocol.ThreadItemsListResponse, error) {
	if c == nil || c.client == nil {
		return protocol.ThreadItemsListResponse{}, &ClosedError{}
	}
	return c.client.Raw().ThreadItemsList(ctx, params)
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

func threadResumeParams(opts ThreadResumeOptions) protocol.ThreadResumeParams {
	params := protocol.ThreadResumeParams{ThreadID: opts.ThreadID}
	if opts.Model != "" {
		params.Model = protocol.Some(opts.Model)
	}
	if opts.ModelProvider != "" {
		params.ModelProvider = protocol.Some(opts.ModelProvider)
	}
	if opts.ServiceTier != "" {
		params.ServiceTier = protocol.Some(opts.ServiceTier)
	}
	if opts.CWD != "" {
		params.Cwd = protocol.Some(opts.CWD)
	}
	if opts.Path != "" {
		params.Path = protocol.Some(opts.Path)
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
	if opts.BaseInstructions != "" {
		params.BaseInstructions = protocol.Some(opts.BaseInstructions)
	}
	if opts.DeveloperInstructions != "" {
		params.DeveloperInstructions = protocol.Some(opts.DeveloperInstructions)
	}
	if opts.Personality != "" {
		params.Personality = protocol.Some(opts.Personality)
	}
	if opts.ExcludeTurns != nil {
		params.ExcludeTurns = protocol.SomeNonNull(*opts.ExcludeTurns)
	}
	if len(opts.History) > 0 {
		params.History = protocol.Some(opts.History)
	}
	if opts.InitialTurnsPage != nil {
		params.InitialTurnsPage = protocol.Some(*opts.InitialTurnsPage)
	}
	return params
}

func threadForkParams(opts ThreadForkOptions) protocol.ThreadForkParams {
	params := protocol.ThreadForkParams{ThreadID: opts.ThreadID}
	if opts.Model != "" {
		params.Model = protocol.Some(opts.Model)
	}
	if opts.ModelProvider != "" {
		params.ModelProvider = protocol.Some(opts.ModelProvider)
	}
	if opts.ServiceTier != "" {
		params.ServiceTier = protocol.Some(opts.ServiceTier)
	}
	if opts.CWD != "" {
		params.Cwd = protocol.Some(opts.CWD)
	}
	if opts.Path != "" {
		params.Path = protocol.Some(opts.Path)
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
	if opts.BaseInstructions != "" {
		params.BaseInstructions = protocol.Some(opts.BaseInstructions)
	}
	if opts.DeveloperInstructions != "" {
		params.DeveloperInstructions = protocol.Some(opts.DeveloperInstructions)
	}
	if opts.Ephemeral != nil {
		params.Ephemeral = protocol.SomeNonNull(*opts.Ephemeral)
	}
	if opts.ExcludeTurns != nil {
		params.ExcludeTurns = protocol.SomeNonNull(*opts.ExcludeTurns)
	}
	if opts.LastTurnID != "" {
		params.LastTurnID = protocol.Some(opts.LastTurnID)
	}
	if opts.ThreadSource != "" {
		params.ThreadSource = protocol.Some(opts.ThreadSource)
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
