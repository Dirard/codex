package codex

import "context"

// Client owns a Codex app-server connection.
type Client struct {
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

func newClient(context.Context, ClientConfig) (*Client, error) {
	return nil, &ConfigError{Reason: "transport unavailable before jsonrpc client core is added"}
}
