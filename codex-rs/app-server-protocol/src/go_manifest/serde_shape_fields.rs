use std::collections::BTreeSet;

use super::digest::reachable_schema_definition_keys;
use super::digest::schema_definition_rust_type_name;
use super::digest::schema_ref_for_definition_key;
use super::*;

pub(super) fn append_schema_sufficient_serde_shapes(protocol: &mut ProtocolModeManifest) {
    let mut existing = protocol
        .serde_shapes
        .iter()
        .map(|entry| entry.rust_type.clone())
        .collect::<BTreeSet<_>>();
    for schema_key in reachable_schema_definition_keys(protocol) {
        let rust_type = schema_definition_rust_type_name(&schema_key).to_string();
        if rust_type == "Option<()>" || existing.contains(&rust_type) {
            continue;
        }
        let schema_ref = Some(schema_ref_for_definition_key(&schema_key));
        let shape =
            if schema_reachable_serde_attribute_required_types().contains(&rust_type.as_str()) {
                manifest_required_serde_shape(rust_type.clone(), schema_ref)
            } else {
                schema_sufficient_serde_shape(rust_type.clone(), schema_ref)
            };
        protocol.serde_shapes.push(shape);
        existing.insert(rust_type);
    }
    protocol
        .serde_shapes
        .sort_by(|left, right| left.rust_type.cmp(&right.rust_type));
}

fn manifest_required_serde_shape(rust_type: String, schema_ref: Option<String>) -> SerdeShapeEntry {
    let fields = reviewed_manifest_required_fields(&rust_type);
    let variant_aliases = reviewed_manifest_required_variant_aliases(&rust_type);
    let review_note = if fields.is_empty() && variant_aliases.is_empty() {
        Some("reviewed manifest-required type with no field-level serde metadata")
    } else {
        None
    };
    SerdeShapeEntry {
        rust_type,
        schema_ref,
        metadata_status: SerdeMetadataStatus::ManifestRequired,
        schema_sufficient_proof: None,
        fields,
        variant_aliases,
        manual_payload_conversion: None,
        review_note,
    }
}

fn reviewed_manifest_required_fields(rust_type: &str) -> Vec<SerdeFieldEntry> {
    match rust_type {
        "ActivePermissionProfile" => vec![default_null_field("extends", "extends")],
        "AdditionalFileSystemPermissions" => vec![
            optional_skip_none_field("glob_scan_max_depth", "globScanMaxDepth"),
            optional_skip_none_field("entries", "entries"),
        ],
        "AppInfo" => vec![
            default_bool_field("is_accessible", "isAccessible"),
            default_function_field(
                "is_enabled",
                "isEnabled",
                SerdePresence::OptionalNonNull,
                "default_enabled",
                "true",
            ),
            default_empty_vec_field("plugin_display_names", "pluginDisplayNames"),
        ],
        "AppsConfig" => vec![
            default_null_field("default", "_default"),
            default_flattened_object_field("apps", "*"),
        ],
        "AppsDefaultConfig" => vec![
            default_function_field(
                "enabled",
                "enabled",
                SerdePresence::OptionalNonNull,
                "default_enabled",
                "true",
            ),
            default_function_field(
                "destructive_enabled",
                "destructiveEnabled",
                SerdePresence::OptionalNonNull,
                "default_enabled",
                "true",
            ),
            default_function_field(
                "open_world_enabled",
                "openWorldEnabled",
                SerdePresence::OptionalNonNull,
                "default_enabled",
                "true",
            ),
        ],
        "AppsListParams" => vec![default_skip_false_field("force_refetch", "forceRefetch")],
        "CommandExecWriteParams" => vec![default_skip_false_field("close_stdin", "closeStdin")],
        "ConfigWarningNotification" => vec![
            optional_skip_none_field("path", "path"),
            optional_skip_none_field("range", "range"),
        ],
        "ExternalAgentConfigDetectParams" => {
            vec![default_skip_false_field("include_home", "includeHome")]
        }
        "FeedbackUploadParams" => vec![default_skip_false_field("include_logs", "includeLogs")],
        "FsCopyParams" => vec![default_skip_false_field("recursive", "recursive")],
        "GetAccountParams" => vec![default_skip_false_field("refresh_token", "refreshToken")],
        "HooksListParams" => vec![default_skip_empty_vec_field("cwds", "cwds")],
        "InitializeCapabilities" => vec![
            default_bool_field("experimental_api", "experimentalApi"),
            default_bool_field("request_attestation", "requestAttestation"),
            default_skip_false_field(
                "mcp_server_openai_form_elicitation",
                "mcpServerOpenaiFormElicitation",
            ),
        ],
        "InitializeParams" => vec![optional_skip_none_field("capabilities", "capabilities")],
        "McpServerElicitationRequestParams" => {
            vec![serde_field(
                "request",
                "*",
                &[],
                SerdeFieldShape {
                    presence: SerdePresence::Required,
                    default: None,
                    skip_serializing_if: None,
                    flattened: true,
                    custom_serialize: None,
                    custom_deserialize: None,
                },
            )]
        }
        "McpServerOauthLoginCompletedNotification" => {
            vec![optional_skip_none_field("error", "error")]
        }
        "McpServerOauthLoginParams" => vec![
            optional_skip_none_field("scopes", "scopes"),
            optional_skip_none_field("timeout_secs", "timeoutSecs"),
        ],
        "McpServerToolCallParams" => vec![
            optional_skip_none_field("arguments", "arguments"),
            optional_skip_none_field("meta", "_meta"),
        ],
        "McpServerToolCallResponse" => vec![
            optional_skip_none_field("structured_content", "structuredContent"),
            optional_skip_none_field("is_error", "isError"),
            optional_skip_none_field("meta", "_meta"),
        ],
        "MigrationDetails" => vec![
            default_empty_vec_field("plugins", "plugins"),
            default_empty_vec_field("skills", "skills"),
            default_empty_vec_field("sessions", "sessions"),
            default_empty_vec_field("mcp_servers", "mcpServers"),
            default_empty_vec_field("hooks", "hooks"),
            default_empty_vec_field("subagents", "subagents"),
            default_empty_vec_field("commands", "commands"),
        ],
        "Model" => vec![
            default_function_field(
                "input_modalities",
                "inputModalities",
                SerdePresence::OptionalNonNull,
                "default_input_modalities",
                "[\"text\"]",
            ),
            default_bool_field("supports_personality", "supportsPersonality"),
            default_empty_vec_field("additional_speed_tiers", "additionalSpeedTiers"),
            default_empty_vec_field("service_tiers", "serviceTiers"),
            default_null_field("default_service_tier", "defaultServiceTier"),
        ],
        "PermissionsRequestApprovalParams" => {
            vec![default_null_field("environment_id", "environmentId")]
        }
        "PermissionsRequestApprovalResponse" => vec![
            default_field("scope", "scope", SerdePresence::OptionalNonNull, "\"turn\""),
            optional_skip_none_field("strict_auto_review", "strictAutoReview"),
        ],
        "PluginInstalledResponse" => {
            vec![default_empty_vec_field(
                "marketplace_load_errors",
                "marketplaceLoadErrors",
            )]
        }
        "PluginListResponse" => vec![
            default_empty_vec_field("marketplace_load_errors", "marketplaceLoadErrors"),
            default_empty_vec_field("featured_plugin_ids", "featuredPluginIds"),
        ],
        "PluginShareContext" => vec![default_null_field("remote_version", "remoteVersion")],
        "PluginSummary" => vec![
            default_null_field("local_version", "localVersion"),
            default_field(
                "availability",
                "availability",
                SerdePresence::OptionalNonNull,
                "\"AVAILABLE\"",
            ),
            default_empty_vec_field("keywords", "keywords"),
        ],
        "ProcessWriteStdinParams" => vec![default_skip_false_field("close_stdin", "closeStdin")],
        "RemoteControlDisableParams" => vec![default_skip_false_field("ephemeral", "ephemeral")],
        "RemoteControlEnableParams" => vec![default_skip_false_field("ephemeral", "ephemeral")],
        "ReviewStartParams" => vec![default_null_field("delivery", "delivery")],
        "SandboxWorkspaceWrite" => vec![
            default_empty_vec_field("writable_roots", "writableRoots"),
            default_bool_field("network_access", "networkAccess"),
            default_bool_field("exclude_tmpdir_env_var", "excludeTmpdirEnvVar"),
            default_bool_field("exclude_slash_tmp", "excludeSlashTmp"),
        ],
        "SkillMetadata" => vec![
            optional_skip_none_field("short_description", "shortDescription"),
            optional_skip_none_field("interface", "interface"),
            optional_skip_none_field("dependencies", "dependencies"),
        ],
        "SkillToolDependency" => vec![
            serde_field(
                "type",
                "type",
                &[],
                SerdeFieldShape {
                    presence: SerdePresence::Required,
                    default: None,
                    skip_serializing_if: None,
                    flattened: false,
                    custom_serialize: None,
                    custom_deserialize: None,
                },
            ),
            optional_skip_none_field("description", "description"),
            optional_skip_none_field("transport", "transport"),
            optional_skip_none_field("command", "command"),
        ],
        "SkillsListParams" => vec![
            default_skip_empty_vec_field("cwds", "cwds"),
            default_skip_false_field("force_reload", "forceReload"),
        ],
        "Thread" => vec![default_field(
            "history_mode",
            "historyMode",
            SerdePresence::OptionalNonNull,
            "\"threaded\"",
        )],
        "ThreadListParams" => vec![default_skip_false_field(
            "use_state_db_only",
            "useStateDbOnly",
        )],
        "ThreadNameUpdatedNotification" => {
            vec![optional_skip_none_field("thread_name", "threadName")]
        }
        "ThreadReadParams" => vec![default_skip_false_field("include_turns", "includeTurns")],
        "ThreadRealtimeAppendTextParams" => vec![default_field(
            "role",
            "role",
            SerdePresence::OptionalNonNull,
            "\"user\"",
        )],
        "ThreadSettings" => vec![default_field(
            "multi_agent_mode",
            "multiAgentMode",
            SerdePresence::OptionalNonNull,
            "\"auto\"",
        )],
        "ToolRequestUserInputParams" => {
            vec![default_null_field("auto_resolution_ms", "autoResolutionMs")]
        }
        "Turn" => vec![default_field(
            "items_view",
            "itemsView",
            SerdePresence::OptionalNonNull,
            "\"visible\"",
        )],
        "TurnError" => vec![default_null_field(
            "additional_details",
            "additionalDetails",
        )],
        _ => Vec::new(),
    }
}

fn reviewed_manifest_required_variant_aliases(rust_type: &str) -> Vec<SerdeVariantAliasEntry> {
    match rust_type {
        "Account" => vec![
            SerdeVariantAliasEntry {
                rust_variant: "ApiKey",
                canonical_wire_value: "apiKey",
                aliases: Vec::new(),
            },
            SerdeVariantAliasEntry {
                rust_variant: "Chatgpt",
                canonical_wire_value: "chatgpt",
                aliases: Vec::new(),
            },
            SerdeVariantAliasEntry {
                rust_variant: "AmazonBedrock",
                canonical_wire_value: "amazonBedrock",
                aliases: Vec::new(),
            },
        ],
        "ApprovalsReviewer" => vec![
            SerdeVariantAliasEntry {
                rust_variant: "User",
                canonical_wire_value: "user",
                aliases: Vec::new(),
            },
            SerdeVariantAliasEntry {
                rust_variant: "AutoReview",
                canonical_wire_value: "auto_review",
                aliases: vec!["guardian_subagent"],
            },
        ],
        "AskForApproval" => vec![SerdeVariantAliasEntry {
            rust_variant: "UnlessTrusted",
            canonical_wire_value: "untrusted",
            aliases: Vec::new(),
        }],
        "PluginAvailability" => vec![
            SerdeVariantAliasEntry {
                rust_variant: "Available",
                canonical_wire_value: "AVAILABLE",
                aliases: vec!["ENABLED"],
            },
            SerdeVariantAliasEntry {
                rust_variant: "DisabledByAdmin",
                canonical_wire_value: "DISABLED_BY_ADMIN",
                aliases: Vec::new(),
            },
        ],
        _ => Vec::new(),
    }
}

fn schema_sufficient_serde_shape(rust_type: String, schema_ref: Option<String>) -> SerdeShapeEntry {
    SerdeShapeEntry {
        rust_type,
        schema_ref,
        metadata_status: SerdeMetadataStatus::SchemaSufficient,
        schema_sufficient_proof: Some(SchemaSufficientProof {
            checked_required_fields: true,
            checked_nullable_fields: true,
            checked_additional_properties: true,
            checked_enum_values: true,
            checked_union_tags: true,
            no_serde_aliases: true,
            no_serde_defaults: true,
            no_skip_serializing_if: true,
            no_flatten: true,
            no_custom_serde: true,
            source_anchor: "generated JSON schema definition plus reviewed manifest-required override list",
        }),
        fields: Vec::new(),
        variant_aliases: Vec::new(),
        manual_payload_conversion: None,
        review_note: None,
    }
}

pub(crate) fn schema_reachable_serde_attribute_required_types() -> &'static [&'static str] {
    &[
        "Account",
        "ActivePermissionProfile",
        "AdditionalFileSystemPermissions",
        "AppInfo",
        "ApprovalsReviewer",
        "AppsConfig",
        "AppsDefaultConfig",
        "AppsListParams",
        "AskForApproval",
        "CommandExecWriteParams",
        "ConfigLayer",
        "ConfigWarningNotification",
        "ExternalAgentConfigDetectParams",
        "FeedbackUploadParams",
        "FsCopyParams",
        "GetAccountParams",
        "HookRunSummary",
        "HooksListParams",
        "InitializeCapabilities",
        "InitializeParams",
        "McpServerElicitationRequestParams",
        "McpServerOauthLoginCompletedNotification",
        "McpServerOauthLoginParams",
        "McpServerToolCallParams",
        "McpServerToolCallResponse",
        "MigrationDetails",
        "Model",
        "PermissionsRequestApprovalParams",
        "PermissionsRequestApprovalResponse",
        "PluginAvailability",
        "PluginInstalledResponse",
        "PluginListResponse",
        "PluginShareContext",
        "PluginSummary",
        "ProcessWriteStdinParams",
        "RemoteControlDisableParams",
        "RemoteControlEnableParams",
        "ReviewStartParams",
        "SandboxPolicy",
        "SandboxWorkspaceWrite",
        "SkillMetadata",
        "SkillToolDependency",
        "SkillsListParams",
        "Thread",
        "ThreadItem",
        "ThreadListParams",
        "ThreadNameUpdatedNotification",
        "ThreadReadParams",
        "ThreadRealtimeAppendTextParams",
        "ThreadSettings",
        "ToolRequestUserInputParams",
        "Turn",
        "TurnError",
        "UserInput",
    ]
}

pub(super) fn serde_field(
    rust_field: &'static str,
    wire_name: &'static str,
    aliases: &[&'static str],
    shape: SerdeFieldShape,
) -> SerdeFieldEntry {
    SerdeFieldEntry {
        rust_field,
        wire_name,
        aliases: aliases.to_vec(),
        shape,
    }
}

pub(super) fn optional_skip_none_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence: SerdePresence::OptionalNullable,
            default: None,
            skip_serializing_if: Some("Option::is_none"),
            flattened: false,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn default_bool_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    default_field(
        rust_field,
        wire_name,
        SerdePresence::OptionalNonNull,
        "false",
    )
}

pub(super) fn default_null_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    default_field(
        rust_field,
        wire_name,
        SerdePresence::OptionalNullable,
        "null",
    )
}

pub(super) fn default_empty_vec_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    default_field(rust_field, wire_name, SerdePresence::OptionalNonNull, "[]")
}

pub(super) fn default_field(
    rust_field: &'static str,
    wire_name: &'static str,
    presence: SerdePresence,
    wire_value_json: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::SerdeDefault,
                wire_value_json,
            }),
            skip_serializing_if: None,
            flattened: false,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn default_function_field(
    rust_field: &'static str,
    wire_name: &'static str,
    presence: SerdePresence,
    function_name: &'static str,
    wire_value_json: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::Function(function_name),
                wire_value_json,
            }),
            skip_serializing_if: None,
            flattened: false,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn default_skip_empty_vec_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence: SerdePresence::OptionalNonNull,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::SerdeDefault,
                wire_value_json: "[]",
            }),
            skip_serializing_if: Some("Vec::is_empty"),
            flattened: false,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn default_flattened_object_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence: SerdePresence::OptionalNonNull,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::SerdeDefault,
                wire_value_json: "{}",
            }),
            skip_serializing_if: None,
            flattened: true,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn default_skip_false_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence: SerdePresence::OptionalNonNull,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::SerdeDefault,
                wire_value_json: "false",
            }),
            skip_serializing_if: Some("std::ops::Not::not"),
            flattened: false,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn thread_response_serde_fields() -> Vec<SerdeFieldEntry> {
    vec![
        default_field(
            "runtime_workspace_roots",
            "runtimeWorkspaceRoots",
            SerdePresence::Required,
            "[]",
        ),
        default_field(
            "instruction_sources",
            "instructionSources",
            SerdePresence::Required,
            "[]",
        ),
        default_field(
            "active_permission_profile",
            "activePermissionProfile",
            SerdePresence::OptionalNullable,
            "null",
        ),
        default_field(
            "multi_agent_mode",
            "multiAgentMode",
            SerdePresence::Required,
            "\"explicitRequestOnly\"",
        ),
    ]
}

pub(super) fn default_skip_none_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence: SerdePresence::OptionalNullable,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::SerdeDefault,
                wire_value_json: "null",
            }),
            skip_serializing_if: Some("Option::is_none"),
            flattened: false,
            custom_serialize: None,
            custom_deserialize: None,
        },
    )
}

pub(super) fn double_option_field(
    rust_field: &'static str,
    wire_name: &'static str,
) -> SerdeFieldEntry {
    serde_field(
        rust_field,
        wire_name,
        &[],
        SerdeFieldShape {
            presence: SerdePresence::DoubleOption,
            default: Some(SerdeDefault {
                provider: SerdeDefaultProvider::SerdeDefault,
                wire_value_json: "null",
            }),
            skip_serializing_if: Some("Option::is_none"),
            flattened: false,
            custom_serialize: Some("crate::protocol::serde_helpers::serialize_double_option"),
            custom_deserialize: Some("crate::protocol::serde_helpers::deserialize_double_option"),
        },
    )
}
