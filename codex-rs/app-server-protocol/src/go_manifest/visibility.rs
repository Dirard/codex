use super::serde_shape_fields::schema_reachable_serde_attribute_required_types;
use super::*;

pub(crate) fn manifest_type_name(raw_type: &'static str) -> Option<String> {
    let compact = raw_type
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if compact.ends_with("Option<()>") {
        return Some("Option<()>".to_string());
    }

    compact
        .rsplit("::")
        .next()
        .filter(|type_name| !type_name.is_empty())
        .map(str::to_owned)
}

pub(crate) fn manifest_schema_ref(raw_type: &'static str) -> Option<String> {
    let type_name = manifest_type_name(raw_type)?;
    if type_name == "Option<()>" {
        None
    } else {
        Some(format!("#/definitions/{type_name}"))
    }
}

pub(crate) fn request_manifest_schema_ref(
    direction: ProtocolDirection,
    method: &'static str,
    raw_type: &'static str,
) -> Option<String> {
    if request_schema_excluded_reason(direction, method).is_some() {
        None
    } else {
        manifest_schema_ref(raw_type)
    }
}

pub(crate) fn serde_shape_requirement_for_type(raw_type: &'static str) -> SerdeShapeRequirement {
    let type_name = manifest_type_name(raw_type);
    if type_name
        .as_deref()
        .is_some_and(|name| schema_reachable_serde_attribute_required_types().contains(&name))
    {
        return SerdeShapeRequirement::ManifestRequired;
    }

    match type_name.as_deref() {
        Some(
            "AppScreenshot"
            | "FileSystemSpecialPath"
            | "CollaborationModeMask"
            | "AnalyticsConfig"
            | "ThreadStartParams"
            | "ThreadStartResponse"
            | "ThreadResumeParams"
            | "ThreadResumeResponse"
            | "ThreadForkParams"
            | "ThreadForkResponse"
            | "CommandExecParams"
            | "LoginAccountParams"
            | "Config"
            | "ConfigReadParams"
            | "ConfigReadResponse"
            | "ConfigRequirements"
            | "ConfigRequirementsReadResponse"
            | "ConfigValueWriteParams"
            | "ConfigBatchWriteParams"
            | "ProcessSpawnParams"
            | "RemoteControlPairingStartParams"
            | "RemoteControlPairingStartResponse"
            | "ReviewStartResponse"
            | "CommandExecutionRequestApprovalParams",
        ) => SerdeShapeRequirement::ManifestRequired,
        _ => SerdeShapeRequirement::SchemaSufficient,
    }
}

pub(crate) fn request_sdk_visibility(
    direction: ProtocolDirection,
    method: &'static str,
) -> SdkVisibility {
    match (direction, method) {
        (ProtocolDirection::ClientToServer, "initialize") => SdkVisibility::HandshakeOnly,
        (
            ProtocolDirection::ClientToServer,
            "getConversationSummary" | "gitDiffToRemote" | "getAuthStatus",
        ) => SdkVisibility::CompatibilityOnly,
        (ProtocolDirection::ClientToServer, "mock/experimentalMethod") => {
            SdkVisibility::InternalTestOnly
        }
        (ProtocolDirection::ServerToClient, "applyPatchApproval" | "execCommandApproval") => {
            SdkVisibility::CompatibilityOnly
        }
        _ => SdkVisibility::Public,
    }
}

pub(crate) fn request_schema_excluded_reason(
    direction: ProtocolDirection,
    method: &'static str,
) -> Option<&'static str> {
    match (direction, method) {
        (
            ProtocolDirection::ClientToServer,
            "getConversationSummary" | "gitDiffToRemote" | "getAuthStatus",
        ) => Some(
            "deprecated v1 compatibility request schemas are intentionally excluded from Go SDK schema inputs",
        ),
        _ => None,
    }
}

pub(crate) fn request_exception(
    direction: ProtocolDirection,
    method: &'static str,
) -> Option<ExceptionReview> {
    match (direction, method) {
        (ProtocolDirection::ClientToServer, "initialize") => Some(ExceptionReview {
            reason: "legacy handshake request",
            owner: "app-server-protocol",
            review_note: "The Go SDK manifest keeps initialize explicit without treating it as a generated public request.",
        }),
        (
            ProtocolDirection::ClientToServer,
            "getConversationSummary" | "gitDiffToRemote" | "getAuthStatus",
        ) => Some(ExceptionReview {
            reason: "legacy compatibility request",
            owner: "app-server-protocol",
            review_note: "This v1 compatibility method remains generated for protocol parity but is not a public Go SDK workflow.",
        }),
        (ProtocolDirection::ClientToServer, "mock/experimentalMethod") => Some(ExceptionReview {
            reason: "internal test-only method",
            owner: "app-server-protocol",
            review_note: "The method validates experimental gating and must not become public SDK surface.",
        }),
        (ProtocolDirection::ServerToClient, "applyPatchApproval" | "execCommandApproval") => {
            Some(ExceptionReview {
                reason: "deprecated legacy server request",
                owner: "app-server-protocol",
                review_note: "The request remains available for legacy turn APIs and must be reviewed before any public SDK exposure.",
            })
        }
        _ => None,
    }
}

pub(crate) fn request_experimental_fields(method: &'static str) -> Vec<ExperimentalFieldMarker> {
    let mut fields = request_param_experimental_fields(method);
    fields.extend(request_response_experimental_fields(method));
    fields
}

fn request_param_experimental_fields(method: &'static str) -> Vec<ExperimentalFieldMarker> {
    match method {
        "thread/start" => experimental_fields(
            "ThreadStartParams",
            &[
                (
                    "allow_provider_model_fallback",
                    "thread/start.allowProviderModelFallback",
                ),
                (
                    "runtime_workspace_roots",
                    "thread/start.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                ("permissions", "thread/start.permissions"),
                ("multi_agent_mode", "thread/start.multiAgentMode"),
                ("history_mode", "thread/start.historyMode"),
                ("environments", "thread/start.environments"),
                ("dynamic_tools", "thread/start.dynamicTools"),
                (
                    "selected_capability_roots",
                    "thread/start.selectedCapabilityRoots",
                ),
                (
                    "mock_experimental_field",
                    "thread/start.mockExperimentalField",
                ),
                (
                    "experimental_raw_events",
                    "thread/start.experimentalRawEvents",
                ),
            ],
        ),
        "thread/resume" => experimental_fields(
            "ThreadResumeParams",
            &[
                ("history", "thread/resume.history"),
                ("path", "thread/resume.path"),
                (
                    "runtime_workspace_roots",
                    "thread/resume.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                ("permissions", "thread/resume.permissions"),
                ("exclude_turns", "thread/resume.excludeTurns"),
                ("initial_turns_page", "thread/resume.initialTurnsPage"),
            ],
        ),
        "thread/fork" => experimental_fields(
            "ThreadForkParams",
            &[
                ("path", "thread/fork.path"),
                (
                    "runtime_workspace_roots",
                    "thread/fork.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                ("permissions", "thread/fork.permissions"),
                ("exclude_turns", "thread/fork.excludeTurns"),
            ],
        ),
        "thread/settings/update" => experimental_fields(
            "ThreadSettingsUpdateParams",
            &[
                ("approval_policy", "nested"),
                ("permissions", "thread/settings/update.permissions"),
                (
                    "collaboration_mode",
                    "thread/settings/update.collaborationMode",
                ),
                ("multi_agent_mode", "thread/settings/update.multiAgentMode"),
            ],
        ),
        "thread/list" => experimental_fields(
            "ThreadListParams",
            &[
                ("parent_thread_id", "thread/list.parentThreadId"),
                ("ancestor_thread_id", "thread/list.ancestorThreadId"),
            ],
        ),
        "turn/start" => experimental_fields(
            "TurnStartParams",
            &[
                (
                    "responsesapi_client_metadata",
                    "turn/start.responsesapiClientMetadata",
                ),
                ("additional_context", "turn/start.additionalContext"),
                ("environments", "turn/start.environments"),
                (
                    "runtime_workspace_roots",
                    "turn/start.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                ("permissions", "turn/start.permissions"),
                ("collaboration_mode", "turn/start.collaborationMode"),
                ("multi_agent_mode", "turn/start.multiAgentMode"),
            ],
        ),
        "turn/steer" => experimental_fields(
            "TurnSteerParams",
            &[
                (
                    "responsesapi_client_metadata",
                    "turn/steer.responsesapiClientMetadata",
                ),
                ("additional_context", "turn/steer.additionalContext"),
            ],
        ),
        "account/login/start" => vec![ExperimentalFieldMarker {
            field_path: "type",
            reason: "account/login/start.chatgptAuthTokens",
            inspect_params: true,
            containing_type: "LoginAccountParams",
            discriminator: Some(ExperimentalVariantDiscriminator {
                field_path: "type",
                wire_value: "chatgptAuthTokens",
            }),
        }],
        "command/exec" => experimental_fields(
            "CommandExecParams",
            &[("permission_profile", "command/exec.permissionProfile")],
        ),
        _ => Vec::new(),
    }
}

fn request_response_experimental_fields(method: &'static str) -> Vec<ExperimentalFieldMarker> {
    match method {
        "thread/start" => experimental_response_fields(
            "ThreadStartResponse",
            &[
                (
                    "runtime_workspace_roots",
                    "thread/start.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                (
                    "active_permission_profile",
                    "thread/start.activePermissionProfile",
                ),
                ("multi_agent_mode", "thread/start.multiAgentMode"),
            ],
        ),
        "thread/resume" => experimental_response_fields(
            "ThreadResumeResponse",
            &[
                (
                    "runtime_workspace_roots",
                    "thread/resume.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                (
                    "active_permission_profile",
                    "thread/resume.activePermissionProfile",
                ),
                ("multi_agent_mode", "thread/resume.multiAgentMode"),
                ("initial_turns_page", "thread/resume.initialTurnsPage"),
            ],
        ),
        "thread/fork" => experimental_response_fields(
            "ThreadForkResponse",
            &[
                (
                    "runtime_workspace_roots",
                    "thread/fork.runtimeWorkspaceRoots",
                ),
                ("approval_policy", "nested"),
                (
                    "active_permission_profile",
                    "thread/fork.activePermissionProfile",
                ),
                ("multi_agent_mode", "thread/fork.multiAgentMode"),
            ],
        ),
        "config/read" => {
            let mut fields =
                experimental_response_fields("ConfigReadResponse", &[("config", "nested")]);
            fields.extend(experimental_response_fields(
                "Config",
                &[
                    ("config.approval_policy", "nested"),
                    ("config.approvals_reviewer", "config/read.approvalsReviewer"),
                    ("config.apps", "config/read.apps"),
                ],
            ));
            fields
        }
        "configRequirements/read" => {
            let mut fields = experimental_response_fields(
                "ConfigRequirementsReadResponse",
                &[("requirements", "nested")],
            );
            fields.extend(experimental_response_fields(
                "ConfigRequirements",
                &[
                    ("requirements.allowed_approval_policies", "nested"),
                    (
                        "requirements.allowed_approvals_reviewers",
                        "configRequirements/read.allowedApprovalsReviewers",
                    ),
                    ("requirements.hooks", "configRequirements/read.hooks"),
                    ("requirements.network", "configRequirements/read.network"),
                ],
            ));
            fields
        }
        _ => Vec::new(),
    }
}

pub(crate) fn server_request_experimental_fields(
    method: &'static str,
) -> Vec<ExperimentalFieldMarker> {
    match method {
        "item/commandExecution/requestApproval" => experimental_fields(
            "CommandExecutionRequestApprovalParams",
            &[
                (
                    "additional_permissions",
                    "item/commandExecution/requestApproval.additionalPermissions",
                ),
                (
                    "available_decisions",
                    "item/commandExecution/requestApproval.availableDecisions",
                ),
            ],
        ),
        _ => Vec::new(),
    }
}

pub(crate) fn notification_experimental_fields(
    _method: &'static str,
) -> Vec<ExperimentalFieldMarker> {
    Vec::new()
}

pub(crate) fn request_bounded_model_context_fields(
    method: &'static str,
) -> Vec<BoundedModelContextField> {
    match method {
        "turn/start" | "turn/steer" => vec![BoundedModelContextField {
            method,
            field_path: "additional_context.*.value",
            limit_profile: "additionalContextValueBytes",
        }],
        _ => Vec::new(),
    }
}

pub(crate) fn notification_sdk_visibility(method: &'static str) -> SdkVisibility {
    match method {
        "rawResponseItem/completed" => SdkVisibility::GeneratedOnly,
        _ => SdkVisibility::Public,
    }
}

fn experimental_fields(
    containing_type: &'static str,
    fields: &[(&'static str, &'static str)],
) -> Vec<ExperimentalFieldMarker> {
    fields
        .iter()
        .map(|(field_path, reason)| ExperimentalFieldMarker {
            field_path,
            reason,
            inspect_params: true,
            containing_type,
            discriminator: None,
        })
        .collect()
}

fn experimental_response_fields(
    containing_type: &'static str,
    fields: &[(&'static str, &'static str)],
) -> Vec<ExperimentalFieldMarker> {
    fields
        .iter()
        .map(|(field_path, reason)| ExperimentalFieldMarker {
            field_path,
            reason,
            inspect_params: false,
            containing_type,
            discriminator: None,
        })
        .collect()
}
