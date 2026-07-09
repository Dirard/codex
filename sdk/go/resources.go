package codex

import "sync"

type AccountsClient struct{ client *Client }
type ThreadsClient struct{ client *Client }
type TurnsClient struct{ client *Client }
type RealtimeClient struct {
	client *Client

	mu             sync.Mutex
	activeByThread map[string]*RealtimeSession
	nextSessionID  uint64
}
type ReviewsClient struct{ client *Client }
type ModelsClient struct{ client *Client }
type ConfigClient struct{ client *Client }
type FileSystemClient struct {
	client *Client

	mu            sync.Mutex
	activeWatches map[string]*FileSystemWatchHandle
	nextWatchID   uint64
}
type CommandsClient struct {
	client *Client

	mu            sync.Mutex
	nextProcessID uint64
}
type ProcessesClient struct {
	client *Client

	mu              sync.Mutex
	activeProcesses map[string]*ProcessHandle
	nextHandleID    uint64
}
type EnvironmentsClient struct{ client *Client }
type SkillsClient struct{ client *Client }
type HooksClient struct{ client *Client }
type PluginsClient struct{ client *Client }
type MarketplaceClient struct{ client *Client }
type AppsClient struct{ client *Client }
type MCPClient struct{ client *Client }
type RemoteControlClient struct{ client *Client }
type CollaborationModesClient struct{ client *Client }
type ExternalAgentsClient struct{ client *Client }
type FuzzyFileSearchClient struct {
	client *Client

	mu            sync.Mutex
	sessions      map[string]*FuzzySearchSession
	sessionPrefix string
	nextSessionID uint64
}
type MemoryClient struct{ client *Client }
type FeedbackClient struct{ client *Client }
type WindowsSandboxClient struct{ client *Client }
type ExperimentalFeaturesClient struct{ client *Client }
type PermissionProfilesClient struct{ client *Client }
