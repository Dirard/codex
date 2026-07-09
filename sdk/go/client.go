package codex

import (
	"context"
	"encoding/json"

	"github.com/openai/codex/sdk/go/internal/jsonrpc"
	"github.com/openai/codex/sdk/go/protocol"
)

// Client owns a Codex app-server connection.
type Client struct {
	rpc                *jsonrpc.Client
	raw                *protocol.RawClient
	router             *notificationRouter
	metadata           Metadata
	configuredHandlers ServerHandlers
	limits             ClientLimits
	disabledStart      map[string]string
	rawOnly            bool

	Accounts             *AccountsClient
	Threads              *ThreadsClient
	Turns                *TurnsClient
	Realtime             *RealtimeClient
	Reviews              *ReviewsClient
	Models               *ModelsClient
	Config               *ConfigClient
	FileSystem           *FileSystemClient
	Commands             *CommandsClient
	Processes            *ProcessesClient
	Environments         *EnvironmentsClient
	Skills               *SkillsClient
	Hooks                *HooksClient
	Plugins              *PluginsClient
	Marketplace          *MarketplaceClient
	Apps                 *AppsClient
	MCP                  *MCPClient
	RemoteControl        *RemoteControlClient
	CollaborationModes   *CollaborationModesClient
	ExternalAgents       *ExternalAgentsClient
	FuzzyFileSearch      *FuzzyFileSearchClient
	Memory               *MemoryClient
	Feedback             *FeedbackClient
	WindowsSandbox       *WindowsSandboxClient
	ExperimentalFeatures *ExperimentalFeaturesClient
	PermissionProfiles   *PermissionProfilesClient
}

// NewClient starts or attaches to a Codex app-server connection.
func NewClient(ctx context.Context, cfg ClientConfig) (*Client, error) {
	return newClient(ctx, cfg)
}

func newClient(ctx context.Context, cfg ClientConfig) (*Client, error) {
	normalized, err := validateClientConfig(cfg)
	if err != nil {
		return nil, err
	}
	transport, runtimePath, source, err := resolveTransport(ctx, normalized)
	if err != nil {
		return nil, err
	}
	client := &Client{
		metadata: Metadata{
			RuntimePath:   runtimePath,
			ProtocolMode:  normalized.ProtocolMode,
			Compatibility: normalized.Compatibility,
		},
		configuredHandlers: normalized.Handlers,
		limits:             normalized.Limits,
		disabledStart:      disabledImplementedHighLevelStartMethods(normalized.NotificationOptOuts),
		rawOnly:            normalized.Mode == ClientModeRawOnly,
	}
	client.router = newNotificationRouter(normalized.Limits)
	rpc := jsonrpc.NewClientWithOptions(transport, client, jsonrpc.ClientOptions{
		HandlerConcurrency: normalized.Limits.HandlerConcurrency,
		HandlerQueue:       normalized.Limits.HandlerQueue,
		HandlerTimeout:     normalized.Limits.HandlerTimeout,
	})
	client.rpc = rpc
	if err := client.initialize(ctx, normalized, source); err != nil {
		_ = rpc.Close()
		return nil, err
	}
	raw := protocol.NewRawClient(client)
	client.raw = &raw
	client.initResourceClients()
	return client, nil
}

func (c *Client) Close() error {
	if c == nil || c.rpc == nil {
		return nil
	}
	if c.router != nil {
		c.router.close()
	}
	return c.rpc.Close()
}

func (c *Client) Metadata() Metadata {
	if c == nil {
		return Metadata{}
	}
	return c.metadata
}

func (c *Client) Notifications(ctx context.Context) (*NotificationStream, error) {
	if c == nil || c.router == nil {
		return nil, &ClosedError{}
	}
	stream := c.router.subscribeGlobal()
	closeStreamOnContext(ctx, stream)
	return stream, nil
}

func (c *Client) Call(ctx context.Context, method string, params any, result any, metadata protocol.MethodMetadata) error {
	if c == nil || c.rpc == nil {
		return &ClosedError{}
	}
	metadata, err := authoritativeMethodMetadata(method, metadata)
	if err != nil {
		return err
	}
	if err := c.validateMethodCall(metadata, params); err != nil {
		return err
	}
	var trace json.RawMessage
	if traceContext, ok := TraceFromContext(ctx); ok {
		data, err := json.Marshal(traceContext)
		if err != nil {
			return err
		}
		trace = data
	}
	return c.rpc.Call(ctx, method, params, result, trace)
}

func (c *Client) callAsync(ctx context.Context, method string, params any, result any, metadata protocol.MethodMetadata) (<-chan error, error) {
	if c == nil || c.rpc == nil {
		return nil, &ClosedError{}
	}
	metadata, err := authoritativeMethodMetadata(method, metadata)
	if err != nil {
		return nil, err
	}
	if err := c.validateMethodCall(metadata, params); err != nil {
		return nil, err
	}
	var trace json.RawMessage
	if traceContext, ok := TraceFromContext(ctx); ok {
		data, err := json.Marshal(traceContext)
		if err != nil {
			return nil, err
		}
		trace = data
	}
	return c.rpc.CallAsync(ctx, method, params, result, trace)
}

func authoritativeMethodMetadata(method string, supplied protocol.MethodMetadata) (protocol.MethodMetadata, error) {
	metadata, ok := protocol.MethodMetadataByMethod[method]
	if !ok {
		return protocol.MethodMetadata{}, &ConfigError{Reason: "unknown method metadata: " + method}
	}
	if supplied.Method != "" && supplied.Method != method {
		return protocol.MethodMetadata{}, &ConfigError{Reason: "method metadata mismatch: " + supplied.Method + " for " + method}
	}
	if metadata.Visibility != "public" {
		return protocol.MethodMetadata{}, &ConfigError{Reason: "method is not public in the Go SDK: " + method}
	}
	return metadata, nil
}

func (c *Client) HandleServerRequest(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) (any, error) {
	if len(trace) > 0 {
		var traceContext TraceContext
		if err := json.Unmarshal(trace, &traceContext); err == nil {
			ctx = WithCallOptions(ctx, CallOptions{Trace: &traceContext})
		}
	}
	return c.serverHandlers().DispatchServerRequest(ctx, method, params)
}

func (c *Client) HandleServerNotification(ctx context.Context, method string, params json.RawMessage, trace json.RawMessage) {
	if c == nil || c.router == nil {
		return
	}
	c.router.route(ctx, method, params, trace)
	c.observeRealtimeLifecycle(method, params)
	c.observeProcessLifecycle(method, params)
}

func (c *Client) serverHandlers() ServerHandlers {
	if c == nil {
		return ServerHandlers{}
	}
	return c.configuredHandlers
}

func (c *Client) initResourceClients() {
	c.Accounts = &AccountsClient{client: c}
	c.Threads = &ThreadsClient{client: c}
	c.Turns = &TurnsClient{client: c}
	c.Realtime = &RealtimeClient{client: c}
	c.Reviews = &ReviewsClient{client: c}
	c.Models = &ModelsClient{client: c}
	c.Config = &ConfigClient{client: c}
	c.FileSystem = &FileSystemClient{client: c}
	c.Commands = &CommandsClient{client: c}
	c.Processes = &ProcessesClient{client: c}
	c.Environments = &EnvironmentsClient{client: c}
	c.Skills = &SkillsClient{client: c}
	c.Hooks = &HooksClient{client: c}
	c.Plugins = &PluginsClient{client: c}
	c.Marketplace = &MarketplaceClient{client: c}
	c.Apps = &AppsClient{client: c}
	c.MCP = &MCPClient{client: c}
	c.RemoteControl = &RemoteControlClient{client: c}
	c.CollaborationModes = &CollaborationModesClient{client: c}
	c.ExternalAgents = &ExternalAgentsClient{client: c}
	c.FuzzyFileSearch = &FuzzyFileSearchClient{client: c}
	c.Memory = &MemoryClient{client: c}
	c.Feedback = &FeedbackClient{client: c}
	c.WindowsSandbox = &WindowsSandboxClient{client: c}
	c.ExperimentalFeatures = &ExperimentalFeaturesClient{client: c}
	c.PermissionProfiles = &PermissionProfilesClient{client: c}
}

func (c *Client) ensureHighLevelEnabled(workflow string) error {
	if c == nil {
		return &ClosedError{}
	}
	if c.rawOnly {
		return &ConfigError{Reason: workflow + " is disabled in raw-only client mode"}
	}
	return nil
}

func (c *Client) ensureHighLevelWorkflowEnabled(workflow string, startMethod string) error {
	if err := c.ensureHighLevelEnabled(workflow); err != nil {
		return err
	}
	if dependency, ok := c.disabledStart[startMethod]; ok {
		return &ConfigError{Reason: workflow + " requires opted-out notifications: " + dependency}
	}
	return nil
}
