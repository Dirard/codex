use super::*;

pub(super) fn go_sdk_routing_lifecycle_entries() -> Vec<RoutingLifecycleEntry> {
    vec![
        RoutingLifecycleEntry {
            resource_domain: "thread",
            wire_identity_source: "thread.id",
            start_method: "thread/start",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "thread/start",
            },
            cleanup_triggers: vec![
                CleanupTrigger::TerminalNotification {
                    method: "thread/closed",
                    predicate: "threadId matches thread.id",
                },
                CleanupTrigger::TerminalNotification {
                    method: "thread/deleted",
                    predicate: "threadId matches thread.id",
                },
            ],
            notification_opt_out_dependencies: vec!["thread/closed", "thread/deleted"],
        },
        RoutingLifecycleEntry {
            resource_domain: "turn",
            wire_identity_source: "turn.id",
            start_method: "turn/start",
            start_completion: WireCompletion::TerminalNotification {
                method: "turn/started",
                predicate: "threadId and turn.id are present",
            },
            cleanup_triggers: vec![CleanupTrigger::TerminalNotification {
                method: "turn/completed",
                predicate: "threadId and turn.id match",
            }],
            notification_opt_out_dependencies: vec!["turn/started", "turn/completed"],
        },
        RoutingLifecycleEntry {
            resource_domain: "accountLogin",
            wire_identity_source: "loginId",
            start_method: "account/login/start",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "account/login/start",
            },
            cleanup_triggers: vec![CleanupTrigger::TerminalNotification {
                method: "account/login/completed",
                predicate: "success or error completed login flow",
            }],
            notification_opt_out_dependencies: vec!["account/login/completed"],
        },
        RoutingLifecycleEntry {
            resource_domain: "review",
            wire_identity_source: "reviewThreadId + turn.id",
            start_method: "review/start",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "review/start",
            },
            cleanup_triggers: vec![CleanupTrigger::TerminalNotification {
                method: "turn/completed",
                predicate: "threadId matches reviewThreadId and turn.id matches review turn",
            }],
            notification_opt_out_dependencies: vec!["turn/started", "turn/completed"],
        },
        RoutingLifecycleEntry {
            resource_domain: "remoteControlPairing",
            wire_identity_source: "pairingCode or manualPairingCode",
            start_method: "remoteControl/pairing/start",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "remoteControl/pairing/start",
            },
            cleanup_triggers: vec![CleanupTrigger::ExplicitMethodResponse {
                method: "remoteControl/pairing/status",
            }],
            notification_opt_out_dependencies: Vec::new(),
        },
        RoutingLifecycleEntry {
            resource_domain: "commandExec",
            wire_identity_source: "processId",
            start_method: "command/exec",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "command/exec",
            },
            cleanup_triggers: vec![CleanupTrigger::JsonRpcResponse {
                method: "command/exec",
            }],
            notification_opt_out_dependencies: vec!["command/exec/outputDelta"],
        },
        RoutingLifecycleEntry {
            resource_domain: "process",
            wire_identity_source: "processHandle",
            start_method: "process/spawn",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "process/spawn",
            },
            cleanup_triggers: vec![CleanupTrigger::TerminalNotification {
                method: "process/exited",
                predicate: "processHandle matches spawned process",
            }],
            notification_opt_out_dependencies: vec!["process/outputDelta", "process/exited"],
        },
        RoutingLifecycleEntry {
            resource_domain: "fs",
            wire_identity_source: "watchId",
            start_method: "fs/watch",
            start_completion: WireCompletion::JsonRpcResponse { method: "fs/watch" },
            cleanup_triggers: vec![CleanupTrigger::ExplicitMethodResponse {
                method: "fs/unwatch",
            }],
            notification_opt_out_dependencies: vec!["fs/changed"],
        },
        RoutingLifecycleEntry {
            resource_domain: "mcpServer",
            wire_identity_source: "name",
            start_method: "mcpServer/oauth/login",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "mcpServer/oauth/login",
            },
            cleanup_triggers: vec![CleanupTrigger::TerminalNotification {
                method: "mcpServer/oauthLogin/completed",
                predicate: "name matches OAuth server",
            }],
            notification_opt_out_dependencies: vec!["mcpServer/oauthLogin/completed"],
        },
        RoutingLifecycleEntry {
            resource_domain: "fuzzyFileSearch",
            wire_identity_source: "sessionId",
            start_method: "fuzzyFileSearch/sessionStart",
            start_completion: WireCompletion::JsonRpcResponse {
                method: "fuzzyFileSearch/sessionStart",
            },
            cleanup_triggers: vec![CleanupTrigger::ExplicitMethodResponse {
                method: "fuzzyFileSearch/sessionStop",
            }],
            notification_opt_out_dependencies: vec![
                "fuzzyFileSearch/sessionUpdated",
                "fuzzyFileSearch/sessionCompleted",
            ],
        },
        RoutingLifecycleEntry {
            resource_domain: "realtime",
            wire_identity_source: "threadId",
            start_method: "thread/realtime/start",
            start_completion: WireCompletion::TerminalNotification {
                method: "thread/realtime/started",
                predicate: "threadId matches realtime thread",
            },
            cleanup_triggers: vec![
                CleanupTrigger::ExplicitMethodResponse {
                    method: "thread/realtime/stop",
                },
                CleanupTrigger::TerminalNotification {
                    method: "thread/realtime/closed",
                    predicate: "threadId matches realtime thread",
                },
            ],
            notification_opt_out_dependencies: vec![
                "thread/realtime/started",
                "thread/realtime/itemAdded",
                "thread/realtime/transcript/delta",
                "thread/realtime/transcript/done",
                "thread/realtime/outputAudio/delta",
                "thread/realtime/sdp",
                "thread/realtime/error",
                "thread/realtime/closed",
            ],
        },
    ]
}
