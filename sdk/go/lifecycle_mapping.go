package codex

import "github.com/openai/codex/sdk/go/protocol"

type goLifecycleTrigger string

const (
	goLifecycleHandleClose   goLifecycleTrigger = "handleClose"
	goLifecycleClientClose   goLifecycleTrigger = "clientClose"
	goLifecycleContextCancel goLifecycleTrigger = "contextCancel"
	goLifecycleTimeout       goLifecycleTrigger = "timeout"
	goLifecycleOverflow      goLifecycleTrigger = "overflow"
)

type lifecycleMapping struct {
	HandleKind     string
	ResourceDomain string
	StartMethod    string
	GoTriggers     []goLifecycleTrigger
}

var lifecycleMappings = []lifecycleMapping{
	{
		HandleKind:     "thread",
		ResourceDomain: "thread",
		StartMethod:    "thread/start",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "turn",
		ResourceDomain: "turn",
		StartMethod:    "turn/start",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "accountLogin",
		ResourceDomain: "accountLogin",
		StartMethod:    "account/login/start",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "review",
		ResourceDomain: "review",
		StartMethod:    "review/start",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "remoteControlPairing",
		ResourceDomain: "remoteControlPairing",
		StartMethod:    "remoteControl/pairing/start",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "mcpOauth",
		ResourceDomain: "mcpServer",
		StartMethod:    "mcpServer/oauth/login",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "commandExec",
		ResourceDomain: "commandExec",
		StartMethod:    "command/exec",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "process",
		ResourceDomain: "process",
		StartMethod:    "process/spawn",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "fsWatch",
		ResourceDomain: "fs",
		StartMethod:    "fs/watch",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "fuzzyFileSearch",
		ResourceDomain: "fuzzyFileSearch",
		StartMethod:    "fuzzyFileSearch/sessionStart",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
	{
		HandleKind:     "realtime",
		ResourceDomain: "realtime",
		StartMethod:    "thread/realtime/start",
		GoTriggers:     []goLifecycleTrigger{goLifecycleHandleClose, goLifecycleClientClose, goLifecycleContextCancel, goLifecycleTimeout, goLifecycleOverflow},
	},
}

func lifecycleMappingByStartMethod(startMethod string) (lifecycleMapping, bool) {
	for _, mapping := range lifecycleMappings {
		if mapping.StartMethod == startMethod {
			return mapping, true
		}
	}
	return lifecycleMapping{}, false
}

func rustLifecycleByStartMethod(startMethod string) (protocol.RoutingLifecycleMetadata, bool) {
	lifecycle, ok := protocol.RoutingLifecycleByStartMethod[startMethod]
	return lifecycle, ok
}
