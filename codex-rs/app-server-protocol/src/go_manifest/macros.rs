//! Fork: Go SDK manifest entries generated from the app-server protocol lists
//! (`docs/fork/05-go-sdk.md`).
//!
//! `fork_with_go_manifest!` (`protocol/fork_protocol.rs`) hands every protocol list of
//! `protocol/common.rs` to its upstream macro and to the matching `go_manifest_*_entries!`
//! macro below, so the upstream list macros stay as upstream wrote them. The matchers here
//! follow the upstream grammars: a grammar change upstream fails to compile instead of
//! letting the manifest drift. Lib.rs imports these macros with `#[macro_use]` before
//! `mod protocol`.

macro_rules! go_manifest_method_name {
    ($variant:ident => $wire:literal) => {
        $wire
    };
    (Initialize) => {
        "initialize"
    };
    (GetConversationSummary) => {
        "getConversationSummary"
    };
    (GitDiffToRemote) => {
        "gitDiffToRemote"
    };
    (GetAuthStatus) => {
        "getAuthStatus"
    };
    (ApplyPatchApproval) => {
        "applyPatchApproval"
    };
    (ExecCommandApproval) => {
        "execCommandApproval"
    };
    (Initialized) => {
        "initialized"
    };
    (AccountLoginCompleted) => {
        "account/login/completed"
    };
}

macro_rules! go_manifest_experimental_marker {
    (#[experimental($reason:expr)]) => {
        Some(crate::go_manifest::ExperimentalMarker {
            reason: $reason,
            field_paths: Vec::new(),
        })
    };
    () => {
        None
    };
}

macro_rules! go_manifest_inspect_params {
    (true) => {
        true
    };
    () => {
        false
    };
}

macro_rules! go_manifest_manual_payload_conversion {
    (manual) => {
        Some("manual response payload conversion")
    };
    () => {
        None
    };
}

macro_rules! go_manifest_serde_shape_requirement {
    (manual, $payload:ty) => {
        crate::go_manifest::SerdeShapeRequirement::ManualPayloadConversion
    };
    ($payload:ty) => {
        crate::go_manifest::serde_shape_requirement_for_type(stringify!($payload))
    };
}

macro_rules! go_manifest_identity_extractor {
    ($identity_name:literal, $field:ident, $optional:literal) => {
        crate::go_manifest::IdentityExtractor {
            identity_name: $identity_name,
            field_path: stringify!($field),
            optional: $optional,
            terminal_predicate: None,
        }
    };
}

macro_rules! go_manifest_request_serialization_scopes {
    (None) => {
        Vec::new()
    };
    (global($key:literal)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::Global,
            queue_key: Some($key),
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: Vec::new(),
        }]
    };
    (global_shared_read($key:literal)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::GlobalSharedRead,
            queue_key: Some($key),
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: Vec::new(),
        }]
    };
    (thread_id($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::Thread,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: vec![go_manifest_identity_extractor!("threadId", $field, false)],
        }]
    };
    (optional_thread_id($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::Thread,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::FieldPresent(stringify!(
                $field
            )),
            identity_extractors: vec![go_manifest_identity_extractor!("threadId", $field, true)],
        }]
    };
    (thread_or_path($params:ident . $thread_field:ident, $params2:ident . $path_field:ident)) => {
        vec![
            crate::go_manifest::RequestSerializationScope {
                kind: crate::go_manifest::RequestSerializationScopeKind::Thread,
                queue_key: None,
                precedence: 0,
                condition: crate::go_manifest::RequestSerializationCondition::StringNonEmpty(
                    stringify!($thread_field),
                ),
                identity_extractors: vec![go_manifest_identity_extractor!(
                    "threadId",
                    $thread_field,
                    false
                )],
            },
            crate::go_manifest::RequestSerializationScope {
                kind: crate::go_manifest::RequestSerializationScopeKind::ThreadPath,
                queue_key: None,
                precedence: 1,
                condition: crate::go_manifest::RequestSerializationCondition::All(&[
                    crate::go_manifest::RequestSerializationCondition::StringEmpty(stringify!(
                        $thread_field
                    )),
                    crate::go_manifest::RequestSerializationCondition::FieldPresent(stringify!(
                        $path_field
                    )),
                ]),
                identity_extractors: vec![go_manifest_identity_extractor!(
                    "path",
                    $path_field,
                    false
                )],
            },
            crate::go_manifest::RequestSerializationScope {
                kind: crate::go_manifest::RequestSerializationScopeKind::Thread,
                queue_key: None,
                precedence: 2,
                condition: crate::go_manifest::RequestSerializationCondition::All(&[
                    crate::go_manifest::RequestSerializationCondition::StringEmpty(stringify!(
                        $thread_field
                    )),
                    crate::go_manifest::RequestSerializationCondition::FieldAbsent(stringify!(
                        $path_field
                    )),
                ]),
                identity_extractors: vec![go_manifest_identity_extractor!(
                    "threadId",
                    $thread_field,
                    false
                )],
            },
        ]
    };
    (optional_command_process_id($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::CommandExecProcess,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::FieldPresent(stringify!(
                $field
            )),
            identity_extractors: vec![go_manifest_identity_extractor!("processId", $field, true)],
        }]
    };
    (command_process_id($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::CommandExecProcess,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: vec![go_manifest_identity_extractor!("processId", $field, false)],
        }]
    };
    (process_handle($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::Process,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: vec![go_manifest_identity_extractor!(
                "processHandle",
                $field,
                false
            )],
        }]
    };
    (fuzzy_session_id($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::FuzzyFileSearchSession,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: vec![go_manifest_identity_extractor!("sessionId", $field, false)],
        }]
    };
    (fs_watch_id($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::FsWatch,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: vec![go_manifest_identity_extractor!("watchId", $field, false)],
        }]
    };
    (mcp_oauth_server($params:ident . $field:ident)) => {
        vec![crate::go_manifest::RequestSerializationScope {
            kind: crate::go_manifest::RequestSerializationScopeKind::McpOauth,
            queue_key: None,
            precedence: 0,
            condition: crate::go_manifest::RequestSerializationCondition::Always,
            identity_extractors: vec![go_manifest_identity_extractor!("serverName", $field, false)],
        }]
    };
}

macro_rules! go_manifest_optional_payload_type {
    () => {
        None
    };
    ($payload:ty) => {
        crate::go_manifest::manifest_type_name(stringify!($payload))
    };
}

macro_rules! go_manifest_optional_payload_schema_ref {
    () => {
        None
    };
    ($payload:ty) => {
        crate::go_manifest::manifest_schema_ref(stringify!($payload))
    };
}

macro_rules! go_manifest_optional_payload_serde_shape_requirement {
    () => {
        crate::go_manifest::SerdeShapeRequirement::SchemaSufficient
    };
    ($payload:ty) => {
        crate::go_manifest::serde_shape_requirement_for_type(stringify!($payload))
    };
}

/// The experimental marker of a notification variant, read from its attributes in any order.
macro_rules! go_manifest_experimental_marker_from_attrs {
    () => {
        None
    };
    ([experimental($reason:expr)] $($rest:tt)*) => {
        go_manifest_experimental_marker!(#[experimental($reason)])
    };
    ([$($other:tt)*] $($rest:tt)*) => {
        go_manifest_experimental_marker_from_attrs!($($rest)*)
    };
}

/// Generates `go_sdk_client_request_manifest_entries()` from the list given to
/// `client_request_definitions!`; the matcher is the upstream one.
macro_rules! go_manifest_client_request_entries {
    (
        $(
            $(#[experimental($reason:expr)])?
            $(#[doc = $variant_doc:literal])*
            $variant:ident => $wire:literal {
                params: $(#[$params_meta:meta])* $params:ty,
                $(inspect_params: $inspect_params:tt,)?
                serialization: $serialization:ident $( ( $($serialization_args:tt)* ) )?,
                $(manual_payload_conversion: $manual_payload_conversion:ident,)?
                response: $response:ty,
            }
        ),* $(,)?
    ) => {
        #[allow(clippy::vec_init_then_push)]
        pub(crate) fn go_sdk_client_request_manifest_entries(
        ) -> Vec<crate::go_manifest::RequestManifestEntry> {
            let mut entries = Vec::new();
            $(
                let method = $wire;
                entries.push(crate::go_manifest::RequestManifestEntry {
                    variant: stringify!($variant),
                    method,
                    direction: crate::go_manifest::ProtocolDirection::ClientToServer,
                    request_serialization_scopes: go_manifest_request_serialization_scopes!(
                        $serialization $( ( $($serialization_args)* ) )?
                    ),
                    params_type: crate::go_manifest::manifest_type_name(stringify!($params)),
                    params_schema_ref: crate::go_manifest::request_manifest_schema_ref(
                        crate::go_manifest::ProtocolDirection::ClientToServer,
                        method,
                        stringify!($params),
                    ),
                    response_type: crate::go_manifest::manifest_type_name(stringify!($response)),
                    response_schema_ref: crate::go_manifest::request_manifest_schema_ref(
                        crate::go_manifest::ProtocolDirection::ClientToServer,
                        method,
                        stringify!($response),
                    ),
                    sdk_visibility: crate::go_manifest::request_sdk_visibility(
                        crate::go_manifest::ProtocolDirection::ClientToServer,
                        method,
                    ),
                    experimental: go_manifest_experimental_marker!($(#[experimental($reason)])?),
                    experimental_fields: crate::go_manifest::request_experimental_fields(method),
                    bounded_model_context_fields: crate::go_manifest::request_bounded_model_context_fields(method),
                    inspect_params: go_manifest_inspect_params!($($inspect_params)?),
                    retry: crate::go_manifest::RetryPolicy::NeverRetryAfterWrite,
                    manual_payload_conversion: go_manifest_manual_payload_conversion!(
                        $($manual_payload_conversion)?
                    ),
                    serde_shape_requirement: go_manifest_serde_shape_requirement!(
                        $($manual_payload_conversion,)?
                        $params
                    ),
                    schema_excluded_reason: crate::go_manifest::request_schema_excluded_reason(
                        crate::go_manifest::ProtocolDirection::ClientToServer,
                        method,
                    ),
                    exception: crate::go_manifest::request_exception(
                        crate::go_manifest::ProtocolDirection::ClientToServer,
                        method,
                    ),
                });
            )*
            entries
        }
    };
}

/// Generates `go_sdk_server_request_manifest_entries()` from the list given to
/// `server_request_definitions!`; the matcher is the upstream one.
macro_rules! go_manifest_server_request_entries {
    (
        $(
            $(#[experimental($reason:expr)])?
            $(#[doc = $variant_doc:literal])*
            $variant:ident $(=> $wire:literal)? {
                params: $params:ty,
                response: $response:ty,
            }
        ),* $(,)?
    ) => {
        #[allow(clippy::vec_init_then_push)]
        pub(crate) fn go_sdk_server_request_manifest_entries(
        ) -> Vec<crate::go_manifest::RequestManifestEntry> {
            let mut entries = Vec::new();
            $(
                let method = go_manifest_method_name!($variant $(=> $wire)?);
                entries.push(crate::go_manifest::RequestManifestEntry {
                    variant: stringify!($variant),
                    method,
                    direction: crate::go_manifest::ProtocolDirection::ServerToClient,
                    request_serialization_scopes: Vec::new(),
                    params_type: crate::go_manifest::manifest_type_name(stringify!($params)),
                    params_schema_ref: crate::go_manifest::request_manifest_schema_ref(
                        crate::go_manifest::ProtocolDirection::ServerToClient,
                        method,
                        stringify!($params),
                    ),
                    response_type: crate::go_manifest::manifest_type_name(stringify!($response)),
                    response_schema_ref: crate::go_manifest::request_manifest_schema_ref(
                        crate::go_manifest::ProtocolDirection::ServerToClient,
                        method,
                        stringify!($response),
                    ),
                    sdk_visibility: crate::go_manifest::request_sdk_visibility(
                        crate::go_manifest::ProtocolDirection::ServerToClient,
                        method,
                    ),
                    experimental: go_manifest_experimental_marker!($(#[experimental($reason)])?),
                    experimental_fields: crate::go_manifest::server_request_experimental_fields(method),
                    bounded_model_context_fields: Vec::new(),
                    inspect_params: false,
                    retry: crate::go_manifest::RetryPolicy::NeverRetryAfterWrite,
                    manual_payload_conversion: None,
                    serde_shape_requirement: crate::go_manifest::serde_shape_requirement_for_type(
                        stringify!($params),
                    ),
                    schema_excluded_reason: crate::go_manifest::request_schema_excluded_reason(
                        crate::go_manifest::ProtocolDirection::ServerToClient,
                        method,
                    ),
                    exception: crate::go_manifest::request_exception(
                        crate::go_manifest::ProtocolDirection::ServerToClient,
                        method,
                    ),
                });
            )*
            entries
        }
    };
}

/// Generates `go_sdk_server_notification_manifest_entries()` from the list given to
/// `server_notification_definitions!`; the matcher accepts the upstream variant attributes.
macro_rules! go_manifest_server_notification_entries {
    (
        $(
            $(#[$($variant_attr:tt)*])*
            $variant:ident $(=> $wire:literal)? ( $payload:ty )
        ),* $(,)?
    ) => {
        #[allow(clippy::vec_init_then_push)]
        pub(crate) fn go_sdk_server_notification_manifest_entries(
        ) -> Vec<crate::go_manifest::NotificationManifestEntry> {
            let mut entries = Vec::new();
            $(
                let method = go_manifest_method_name!($variant $(=> $wire)?);
                entries.push(crate::go_manifest::NotificationManifestEntry {
                    variant: stringify!($variant),
                    method,
                    direction: crate::go_manifest::ProtocolDirection::ServerNotification,
                    serde_shape_requirement: crate::go_manifest::serde_shape_requirement_for_type(
                        stringify!($payload),
                    ),
                    payload_type: crate::go_manifest::manifest_type_name(stringify!($payload)),
                    payload_schema_ref: crate::go_manifest::manifest_schema_ref(stringify!($payload)),
                    sdk_visibility: crate::go_manifest::notification_sdk_visibility(method),
                    experimental: go_manifest_experimental_marker_from_attrs!(
                        $([$($variant_attr)*])*
                    ),
                    experimental_fields: crate::go_manifest::notification_experimental_fields(method),
                    routing_strategy: crate::go_manifest::notification_routing_strategy(method),
                    manual_payload_conversion: None,
                    schema_excluded_reason: crate::go_manifest::notification_schema_excluded_reason(method),
                    exception: crate::go_manifest::notification_exception(method),
                });
            )*
            entries
        }
    };
}

/// Generates `go_sdk_client_notification_manifest_entries()` from the list given to
/// `client_notification_definitions!`; the matcher is the upstream one.
macro_rules! go_manifest_client_notification_entries {
    (
        $(
            $(#[$variant_meta:meta])*
            $variant:ident $( ( $payload:ty ) )?
        ),* $(,)?
    ) => {
        #[allow(clippy::vec_init_then_push)]
        pub(crate) fn go_sdk_client_notification_manifest_entries(
        ) -> Vec<crate::go_manifest::NotificationManifestEntry> {
            let mut entries = Vec::new();
            $(
                let method = go_manifest_method_name!($variant);
                entries.push(crate::go_manifest::NotificationManifestEntry {
                    variant: stringify!($variant),
                    method,
                    direction: crate::go_manifest::ProtocolDirection::ClientNotification,
                    serde_shape_requirement: go_manifest_optional_payload_serde_shape_requirement!(
                        $($payload)?
                    ),
                    payload_type: go_manifest_optional_payload_type!($($payload)?),
                    payload_schema_ref: go_manifest_optional_payload_schema_ref!($($payload)?),
                    sdk_visibility: crate::go_manifest::SdkVisibility::Public,
                    experimental: None,
                    experimental_fields: Vec::new(),
                    routing_strategy: crate::go_manifest::NotificationRoutingStrategy::GlobalOnly {
                        reason: "client notification is sent to the server and is not routed by server-side notification dispatch",
                    },
                    manual_payload_conversion: None,
                    schema_excluded_reason: None,
                    exception: None,
                });
            )*
            entries
        }
    };
}
