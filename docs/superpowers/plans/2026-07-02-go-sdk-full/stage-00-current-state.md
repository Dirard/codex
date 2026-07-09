# Stage 0: Repository Bootstrap And Baseline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Create the Go SDK skeleton and capture a clean baseline before protocol/generator work begins.

**Architecture:** This stage adds only scaffolding, baseline tests, and command wiring. No protocol behavior should be hand-implemented here beyond compile-safe stubs that later stages replace.

**Tech Stack:** Go 1.25 module, standard `testing`, repository `just` commands.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:1-129`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:675-710`
- `.github/workflows/sdk.yml`
- `sdk/python/pyproject.toml` and `sdk/typescript/package.json` for SDK layout conventions.

## Files

- Create: `sdk/go/go.mod`
- Create: `sdk/go/client.go`
- Create: `sdk/go/config.go`
- Create: `sdk/go/errors.go`
- Create: `sdk/go/handlers.go`
- Create: `sdk/go/limits.go`
- Create: `sdk/go/metadata.go`
- Create: `sdk/go/resources.go`
- Create: `sdk/go/client_test.go`
- Create: `sdk/go/internal/jsonrpc/doc.go`
- Create: `sdk/go/protocol/doc.go`
- Create: `sdk/go/internal/cmd/protocodex/main.go`
- Create: `sdk/go/internal/protocodex/doc.go`

## Tasks

### Task 0.1: Add Go Module Skeleton

- [ ] Create `sdk/go/go.mod`:

```go
module github.com/openai/codex/sdk/go

go 1.25
```

- [ ] Create `sdk/go/client.go`:

```go
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
```

- [ ] Create `sdk/go/config.go`:

```go
package codex

import (
	"context"
	"encoding/json"
	"time"
)

// ProtocolMode selects the app-server protocol surface used during initialize.
type ProtocolMode int

const (
	ProtocolModeExperimental ProtocolMode = iota
	ProtocolModeStable
)

// ClientConfig configures SDK startup and runtime behavior.
type ClientConfig struct {
	CodexPath        string
	Launch          LaunchOptions
	CWD              string
	Env              map[string]string
	ConfigOverrides  map[string]string
	ClientName       string
	ClientVersion    string
	ProtocolMode     ProtocolMode
	Mode             ClientMode
	Limits           ClientLimits
	Handlers         ServerHandlers
	Compatibility    CompatibilityPolicy
	Transport        Transport
	NotificationOptOuts NotificationOptOuts
}

// LaunchOptions is reserved for typed codex global launch knobs that may appear
// before the app-server subcommand. Stage 3 starts with no public launch
// options; unknown raw CLI flags are not accepted through the public SDK surface.
type LaunchOptions struct{}

// CompatibilityPolicy controls runtime protocol mismatch behavior.
type CompatibilityPolicy int

const (
	// CompatibilityStrict is the zero-value production policy. It requires the
	// runtime initialize response to include the selected protocol digest and for
	// that digest to match the generated SDK digest.
	CompatibilityStrict CompatibilityPolicy = iota
	// CompatibilityAllowDevBuild permits missing or mismatched digests only for
	// explicit, reviewed dev-runtime fixtures, including legacy initialize
	// responses without activeProtocolMode. It must never silently accept a
	// PATH-discovered release runtime with a mismatched digest.
	CompatibilityAllowDevBuild
	// CompatibilityAllowProtocolDigestUnavailable permits legacy initialize
	// responses with no digest fields and no activeProtocolMode only from
	// injected test transports or explicit dev CodexPath launches, and still
	// rejects any non-empty mismatched digest.
	CompatibilityAllowProtocolDigestUnavailable
)

// Transport is an injected JSON-RPC frame transport used by tests and embedding hosts.
//
// Receive blocks until it returns one complete JSON-RPC envelope. Send writes one
// complete JSON-RPC envelope. Implementations own their external framing, if any:
// callers pass raw JSON objects without a trailing newline or content-length
// header. The SDK owns JSON-RPC request correlation, generated method dispatch,
// and serialized calls into Send.
type Transport interface {
	Receive(ctx context.Context) (json.RawMessage, error)
	Send(ctx context.Context, frame json.RawMessage) error
	Close() error
}

// NotificationOptOuts lists app-server notifications the caller opts out of.
type NotificationOptOuts struct {
	Methods []string
}

// ClientMode controls high-level workflow availability.
type ClientMode int

const (
	ClientModeHighLevel ClientMode = iota
	ClientModeRawOnly
)

// ClientLimits contains bounded runtime limits. Zero means SDK default.
type ClientLimits struct {
	MaxFrameBytes                 int64
	MaxLocalInputBytes            int64
	MaxAdditionalContextEntries   int
	MaxAdditionalContextKeyBytes  int64
	MaxAdditionalContextValueBytes int64
	MaxAdditionalContextTotalBytes int64
	ResourceStreamQueue           int
	PendingTurnQueue              int
	PendingTurnMap                int
	GlobalSubscriberQueue         int
	HandlerConcurrency           int
	HandlerQueue                 int
	HandlerTimeout               time.Duration
	StderrRingBytes              int
	LifecycleInactivityTimeout    time.Duration
}
```

- [ ] Create `sdk/go/handlers.go`:

```go
package codex

// ServerHandlers groups handlers for app-server requests sent to the SDK client.
type ServerHandlers struct {
	Approvals ApprovalHandler
}

// ApprovalHandler handles approval requests.
type ApprovalHandler interface {
	HandleApproval() error
}
```

- [ ] Create `sdk/go/resources.go`:

```go
package codex

type AccountsClient struct{}
type ThreadsClient struct{}
type TurnsClient struct{}
type RealtimeClient struct{}
type ReviewsClient struct{}
type ModelsClient struct{}
type ConfigClient struct{}
type FileSystemClient struct{}
type CommandsClient struct{}
type ProcessesClient struct{}
type EnvironmentsClient struct{}
type SkillsClient struct{}
type HooksClient struct{}
type PluginsClient struct{}
type MarketplaceClient struct{}
type AppsClient struct{}
type MCPClient struct{}
type RemoteControlClient struct{}
type CollaborationModesClient struct{}
type ExternalAgentsClient struct{}
type FuzzyFileSearchClient struct{}
type MemoryClient struct{}
type FeedbackClient struct{}
type WindowsSandboxClient struct{}
type ExperimentalFeaturesClient struct{}
type PermissionProfilesClient struct{}
```

- [ ] Create `sdk/go/limits.go`:

```go
package codex

import "time"

const (
	DefaultMaxFrameBytes          int64 = 16 * 1024 * 1024
	DefaultMaxLocalInputBytes     int64 = 16 * 1024 * 1024
	DefaultMaxAdditionalContextEntries        = 16
	DefaultMaxAdditionalContextKeyBytes int64 = 256
	DefaultMaxAdditionalContextValueBytes int64 = 8 * 1024
	DefaultMaxAdditionalContextTotalBytes int64 = 64 * 1024
	DefaultResourceStreamQueue          = 256
	DefaultPendingTurnQueue             = 512
	DefaultPendingTurnMap               = 128
	DefaultGlobalSubscriberQueue        = 512
	DefaultHandlerConcurrency           = 16
	DefaultHandlerQueue                 = 256
	DefaultStderrRingBytes              = 64 * 1024
)

const (
	DefaultHandlerTimeout            = 60 * time.Second
	DefaultLifecycleInactivityTimeout = 5 * time.Minute
)

func normalizeLimits(l ClientLimits) (ClientLimits, error) {
	if l.MaxFrameBytes < 0 ||
		l.MaxLocalInputBytes < 0 ||
		l.MaxAdditionalContextEntries < 0 ||
		l.MaxAdditionalContextKeyBytes < 0 ||
		l.MaxAdditionalContextValueBytes < 0 ||
		l.MaxAdditionalContextTotalBytes < 0 ||
		l.ResourceStreamQueue < 0 ||
		l.PendingTurnQueue < 0 ||
		l.PendingTurnMap < 0 ||
		l.GlobalSubscriberQueue < 0 ||
		l.HandlerConcurrency < 0 ||
		l.HandlerQueue < 0 ||
		l.HandlerTimeout < 0 ||
		l.StderrRingBytes < 0 ||
		l.LifecycleInactivityTimeout < 0 {
		return ClientLimits{}, &ConfigError{Reason: "limits must be zero for defaults or positive overrides"}
	}
	if l.MaxFrameBytes == 0 {
		l.MaxFrameBytes = DefaultMaxFrameBytes
	}
	if l.MaxLocalInputBytes == 0 {
		l.MaxLocalInputBytes = DefaultMaxLocalInputBytes
	}
	if l.MaxAdditionalContextEntries == 0 {
		l.MaxAdditionalContextEntries = DefaultMaxAdditionalContextEntries
	}
	if l.MaxAdditionalContextKeyBytes == 0 {
		l.MaxAdditionalContextKeyBytes = DefaultMaxAdditionalContextKeyBytes
	}
	if l.MaxAdditionalContextValueBytes == 0 {
		l.MaxAdditionalContextValueBytes = DefaultMaxAdditionalContextValueBytes
	}
	if l.MaxAdditionalContextTotalBytes == 0 {
		l.MaxAdditionalContextTotalBytes = DefaultMaxAdditionalContextTotalBytes
	}
	if l.ResourceStreamQueue == 0 {
		l.ResourceStreamQueue = DefaultResourceStreamQueue
	}
	if l.PendingTurnQueue == 0 {
		l.PendingTurnQueue = DefaultPendingTurnQueue
	}
	if l.PendingTurnMap == 0 {
		l.PendingTurnMap = DefaultPendingTurnMap
	}
	if l.GlobalSubscriberQueue == 0 {
		l.GlobalSubscriberQueue = DefaultGlobalSubscriberQueue
	}
	if l.HandlerConcurrency == 0 {
		l.HandlerConcurrency = DefaultHandlerConcurrency
	}
	if l.HandlerQueue == 0 {
		l.HandlerQueue = DefaultHandlerQueue
	}
	if l.HandlerTimeout == 0 {
		l.HandlerTimeout = DefaultHandlerTimeout
	}
	if l.StderrRingBytes == 0 {
		l.StderrRingBytes = DefaultStderrRingBytes
	}
	if l.LifecycleInactivityTimeout == 0 {
		l.LifecycleInactivityTimeout = DefaultLifecycleInactivityTimeout
	}
	return l, nil
}
```

- [ ] Create `sdk/go/errors.go`:

```go
package codex

// ConfigError reports invalid SDK configuration before startup.
type ConfigError struct {
	Reason string
}

func (e *ConfigError) Error() string {
	return "codex sdk config error: " + e.Reason
}
```

- [ ] Create `sdk/go/metadata.go`:

```go
package codex

// Metadata describes the connected runtime and effective SDK configuration.
type Metadata struct {
	RuntimePath                 string
	RuntimeVersion              string
	UserAgent                   string
	ProtocolMode                ProtocolMode
	Compatibility               CompatibilityPolicy
	CompatibilityOverrideActive bool
	CompatibilityNote           string
}
```

- [ ] Create `sdk/go/protocol/doc.go`:

```go
// Package protocol contains generated Codex app-server protocol types.
package protocol
```

- [ ] Create `sdk/go/internal/jsonrpc/doc.go`:

```go
// Package jsonrpc implements Codex app-server newline-delimited JSON-RPC transport.
package jsonrpc
```

- [ ] Create `sdk/go/internal/protocodex/doc.go`:

```go
// Package protocodex contains helpers for the Go app-server protocol generator.
package protocodex
```

- [ ] Create `sdk/go/internal/cmd/protocodex/main.go`:

```go
package main

import (
	"fmt"
	"os"
)

func main() {
	fmt.Fprintln(os.Stderr, "protocodex generator command is unavailable before generator support is added")
	os.Exit(2)
}
```

### Task 0.2: Add Compile Baseline Tests

- [ ] Create `sdk/go/client_test.go`:

```go
package codex

import (
	"errors"
	"testing"
	"time"
)

func TestClientConfigDefaults(t *testing.T) {
	limits, err := normalizeLimits(ClientLimits{})
	if err != nil {
		t.Fatal(err)
	}
	if limits.MaxFrameBytes != DefaultMaxFrameBytes {
		t.Fatalf("MaxFrameBytes = %d, want %d", limits.MaxFrameBytes, DefaultMaxFrameBytes)
	}
	if limits.HandlerTimeout != 60*time.Second {
		t.Fatalf("HandlerTimeout = %s, want 60s", limits.HandlerTimeout)
	}
	if ProtocolModeExperimental != 0 {
		t.Fatalf("ProtocolModeExperimental = %d, want zero-value", ProtocolModeExperimental)
	}
}

func TestClientConfigRejectsNegativeLimits(t *testing.T) {
	_, err := normalizeLimits(ClientLimits{MaxFrameBytes: -1})
	if err == nil {
		t.Fatal("expected error")
	}
	var configErr *ConfigError
	if !errors.As(err, &configErr) {
		t.Fatalf("error = %T, want *ConfigError", err)
	}
}
```

- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/go
go test ./...
```

Expected: `sdk/go` compiles, `codex` package tests pass, generator command compiles but is not invoked.

### Task 0.3: Commit Stage 0

- [ ] Verify there are no unexpected changed paths before committing. This pre-commit gate must allow the Stage 0 `sdk/go` skeleton and fail on anything else:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
unexpected="$(git status --short --untracked-files=all | awk '$2 !~ /^sdk\/go(\/|$)/ { print }')"
test -z "${unexpected}" || { printf '%s\n' "${unexpected}"; exit 1; }
```

- [ ] Commit:

```bash
git add sdk/go
git commit -m "feat(go-sdk): add module skeleton"
```

- [ ] Verify the stage commit left the worktree clean, including untracked generated files:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
status="$(git status --porcelain=v1 --untracked-files=normal --ignore-submodules=none)"
test -z "${status}" || { printf '%s\n' "${status}"; exit 1; }
```

## Stage Review

Run a fresh blind engineering review with:

- spec: `docs/superpowers/specs/2026-07-01-go-sdk-design.md`
- plan stage: this file
- diff: stage 0 commit
- command output: `go test ./...`
