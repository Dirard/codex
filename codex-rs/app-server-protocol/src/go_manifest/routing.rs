use super::*;

pub(crate) fn notification_schema_excluded_reason(method: &'static str) -> Option<&'static str> {
    match method {
        "rawResponseItem/completed" => Some(
            "raw response item completion is stripped from the generated JSON ServerNotification method union",
        ),
        "rawResponse/completed" => Some(
            "raw response completion is stripped from the generated JSON ServerNotification method union",
        ),
        _ => None,
    }
}

pub(crate) fn notification_exception(method: &'static str) -> Option<ExceptionReview> {
    match method {
        "rawResponseItem/completed" | "rawResponse/completed" => Some(ExceptionReview {
            reason: "JSON ServerNotification method-union exclusion",
            owner: "app-server-protocol",
            review_note: "The payload schema exists, but the method-union exclusion must stay explicit for SDK generators.",
        }),
        _ => None,
    }
}

pub(crate) fn notification_routing_strategy(method: &'static str) -> NotificationRoutingStrategy {
    match method {
        "rawResponseItem/completed" | "rawResponse/completed" => {
            routed_notification(method, &[("threadId", false), ("turnId", false)])
        }
        "skills/changed" => global_notification("skills cache invalidation"),
        "account/updated" => global_notification("account cache invalidation"),
        "account/rateLimits/updated" => {
            global_notification("account rate-limit cache invalidation")
        }
        "app/list/updated" => global_notification("app list cache invalidation"),
        "deprecationNotice" => global_notification("deprecation display"),
        "configWarning" => global_notification("config warning display"),
        "windows/worldWritableWarning" => global_notification("warning display"),
        "windowsSandbox/setupCompleted" => {
            global_notification("terminal for Windows sandbox setup")
        }
        "warning" => NotificationRoutingStrategy::RoutedWithGlobalFallback {
            routes: vec![routing_ref(method, &[("threadId", true)])],
            missing_identity_reason: "warning notifications without threadId are displayed globally",
        },
        "error" => routed_notification(method, &[("threadId", false), ("turnId", false)]),
        "thread/started" => routed_notification(method, &[("thread.id", false)]),
        "thread/status/changed"
        | "thread/archived"
        | "thread/deleted"
        | "thread/unarchived"
        | "thread/closed"
        | "thread/name/updated"
        | "thread/goal/cleared"
        | "thread/settings/updated"
        | "guardianWarning"
        | "thread/realtime/started"
        | "thread/realtime/itemAdded"
        | "thread/realtime/outputAudio/delta"
        | "thread/realtime/sdp"
        | "thread/realtime/error"
        | "thread/realtime/closed" => routed_notification(method, &[("threadId", false)]),
        "thread/environment/connected" | "thread/environment/disconnected" => {
            routed_notification(method, &[("threadId", false)])
        }
        "thread/goal/updated" => {
            routed_notification(method, &[("threadId", false), ("turnId", true)])
        }
        "thread/tokenUsage/updated" => {
            routed_notification(method, &[("threadId", false), ("turnId", false)])
        }
        "turn/started" | "turn/completed" => {
            routed_notification(method, &[("threadId", false), ("turn.id", false)])
        }
        "hook/started" | "hook/completed" => routed_notification(
            method,
            &[("threadId", false), ("turnId", false), ("run.id", false)],
        ),
        "turn/diff/updated"
        | "turn/plan/updated"
        | "thread/compacted"
        | "model/rerouted"
        | "model/verification"
        | "turn/moderationMetadata"
        | "model/safetyBuffering/updated" => {
            routed_notification(method, &[("threadId", false), ("turnId", false)])
        }
        "item/started" | "item/completed" => routed_notification(
            method,
            &[("threadId", false), ("turnId", false), ("item.id", false)],
        ),
        "item/autoApprovalReview/started" | "item/autoApprovalReview/completed" => {
            routed_notification(
                method,
                &[
                    ("threadId", false),
                    ("turnId", false),
                    ("reviewId", false),
                    ("targetItemId", true),
                ],
            )
        }
        "item/agentMessage/delta"
        | "item/plan/delta"
        | "item/commandExecution/outputDelta"
        | "item/fileChange/outputDelta"
        | "item/fileChange/patchUpdated"
        | "item/mcpToolCall/progress" => routed_notification(
            method,
            &[("threadId", false), ("turnId", false), ("itemId", false)],
        ),
        "command/exec/outputDelta" => routed_notification(method, &[("processId", false)]),
        "process/outputDelta" | "process/exited" => {
            routed_notification(method, &[("processHandle", false)])
        }
        "item/commandExecution/terminalInteraction" => routed_notification(
            method,
            &[
                ("threadId", false),
                ("turnId", false),
                ("itemId", false),
                ("processId", false),
            ],
        ),
        "serverRequest/resolved" => {
            routed_notification(method, &[("threadId", false), ("requestId", false)])
        }
        "mcpServer/oauthLogin/completed" | "mcpServer/startupStatus/updated" => {
            routed_notification(method, &[("name", false), ("threadId", true)])
        }
        "remoteControl/status/changed" => routed_notification(
            method,
            &[
                ("environmentId", true),
                ("installationId", false),
                ("serverName", false),
            ],
        ),
        "externalAgentConfig/import/progress" | "externalAgentConfig/import/completed" => {
            routed_notification(method, &[("importId", false)])
        }
        "fs/changed" => routed_notification(method, &[("watchId", false)]),
        "item/reasoning/summaryTextDelta" | "item/reasoning/summaryPartAdded" => {
            routed_notification(
                method,
                &[
                    ("threadId", false),
                    ("turnId", false),
                    ("itemId", false),
                    ("summaryIndex", false),
                ],
            )
        }
        "item/reasoning/textDelta" => routed_notification(
            method,
            &[
                ("threadId", false),
                ("turnId", false),
                ("itemId", false),
                ("contentIndex", false),
            ],
        ),
        "fuzzyFileSearch/sessionUpdated" | "fuzzyFileSearch/sessionCompleted" => {
            routed_notification(method, &[("sessionId", false)])
        }
        "thread/realtime/transcript/delta" | "thread/realtime/transcript/done" => {
            routed_notification(method, &[("threadId", false), ("role", false)])
        }
        "account/login/completed" => NotificationRoutingStrategy::RoutedWithGlobalFallback {
            routes: vec![routing_ref(method, &[("loginId", true)])],
            missing_identity_reason: "login completion without loginId is delivered to account observers globally",
        },
        _ => NotificationRoutingStrategy::InternalOnly {
            reason: "notification routing must be reviewed before SDK exposure",
        },
    }
}

fn global_notification(reason: &'static str) -> NotificationRoutingStrategy {
    NotificationRoutingStrategy::GlobalOnly { reason }
}

fn routed_notification(
    method: &'static str,
    identities: &[(&'static str, bool)],
) -> NotificationRoutingStrategy {
    NotificationRoutingStrategy::Routed {
        routes: vec![routing_ref(method, identities)],
    }
}

fn routing_ref(method: &'static str, identities: &[(&'static str, bool)]) -> RoutingRef {
    RoutingRef {
        resource_domain: notification_resource_domain(method),
        wire_identity_source: method,
        identity_extractors: identities
            .iter()
            .map(|(field_path, optional)| IdentityExtractor {
                identity_name: field_path.rsplit('.').next().unwrap_or(field_path),
                field_path,
                optional: *optional,
                terminal_predicate: None,
            })
            .collect(),
    }
}

fn notification_resource_domain(method: &'static str) -> &'static str {
    method.split('/').next().unwrap_or(method)
}
