use anyhow::Context;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GoSdkManifest {
    pub manifest_schema_version: u32,
    pub stable: ProtocolModeManifest,
    pub experimental: ProtocolModeManifest,
    pub model_context_limits: ModelContextLimits,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolModeManifest {
    pub protocol_mode: ProtocolModeName,
    pub client_requests: Vec<RequestManifestEntry>,
    pub server_requests: Vec<RequestManifestEntry>,
    pub server_notifications: Vec<NotificationManifestEntry>,
    pub client_notifications: Vec<NotificationManifestEntry>,
    pub serde_shapes: Vec<SerdeShapeEntry>,
    pub routing_lifecycle: Vec<RoutingLifecycleEntry>,
    pub digests: DigestSet,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolModeName {
    Stable,
    Experimental,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestManifestEntry {
    pub variant: &'static str,
    pub method: &'static str,
    pub direction: ProtocolDirection,
    pub request_serialization_scopes: Vec<RequestSerializationScope>,
    pub params_type: Option<&'static str>,
    pub params_schema_ref: Option<&'static str>,
    pub response_type: Option<&'static str>,
    pub response_schema_ref: Option<&'static str>,
    pub sdk_visibility: SdkVisibility,
    pub experimental: Option<ExperimentalMarker>,
    pub experimental_fields: Vec<ExperimentalFieldMarker>,
    pub bounded_model_context_fields: Vec<BoundedModelContextField>,
    pub inspect_params: bool,
    pub retry: RetryPolicy,
    pub manual_payload_conversion: Option<&'static str>,
    pub serde_shape_requirement: SerdeShapeRequirement,
    pub schema_excluded_reason: Option<&'static str>,
    pub exception: Option<ExceptionReview>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolDirection {
    ClientToServer,
    ServerToClient,
    ServerNotification,
    ClientNotification,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestSerializationScope {
    pub kind: RequestSerializationScopeKind,
    /// Lower values match first when more than one scope can match a request.
    pub precedence: u16,
    pub condition: RequestSerializationCondition,
    pub identity_extractors: Vec<IdentityExtractor>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RequestSerializationCondition {
    Always,
    FieldPresent(&'static str),
    FieldAbsent(&'static str),
    StringNonEmpty(&'static str),
    StringEmpty(&'static str),
    All(&'static [RequestSerializationCondition]),
    Any(&'static [RequestSerializationCondition]),
    Not(&'static RequestSerializationCondition),
    Variant(&'static str),
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RequestSerializationScopeKind {
    Global,
    GlobalSharedRead,
    Thread,
    ThreadPath,
    CommandExecProcess,
    Process,
    FuzzyFileSearchSession,
    FsWatch,
    McpOauth,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SerdeShapeRequirement {
    SchemaSufficient,
    ManifestRequired,
    ManualPayloadConversion,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SdkVisibility {
    Public,
    GeneratedOnly,
    CompatibilityOnly,
    InternalTestOnly,
    HandshakeOnly,
    Excluded,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RetryPolicy {
    NeverRetryAfterWrite,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationManifestEntry {
    pub variant: &'static str,
    pub method: &'static str,
    pub direction: ProtocolDirection,
    pub serde_shape_requirement: SerdeShapeRequirement,
    pub payload_type: Option<&'static str>,
    pub payload_schema_ref: Option<&'static str>,
    pub sdk_visibility: SdkVisibility,
    pub experimental: Option<ExperimentalMarker>,
    pub experimental_fields: Vec<ExperimentalFieldMarker>,
    pub routing_strategy: NotificationRoutingStrategy,
    pub manual_payload_conversion: Option<&'static str>,
    pub schema_excluded_reason: Option<&'static str>,
    pub exception: Option<ExceptionReview>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRef {
    pub resource_domain: &'static str,
    pub wire_identity_source: &'static str,
    pub identity_extractors: Vec<IdentityExtractor>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IdentityExtractor {
    pub identity_name: &'static str,
    pub field_path: &'static str,
    pub optional: bool,
    pub terminal_predicate: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum NotificationRoutingStrategy {
    Routed {
        routes: Vec<RoutingRef>,
    },
    RoutedWithGlobalFallback {
        routes: Vec<RoutingRef>,
        missing_identity_reason: &'static str,
    },
    GlobalOnly {
        reason: &'static str,
    },
    RawOnly {
        reason: &'static str,
    },
    InternalOnly {
        reason: &'static str,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingLifecycleEntry {
    pub resource_domain: &'static str,
    pub wire_identity_source: &'static str,
    pub start_method: &'static str,
    pub start_completion: WireCompletion,
    pub cleanup_triggers: Vec<CleanupTrigger>,
    pub notification_opt_out_dependencies: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum WireCompletion {
    JsonRpcResponse {
        method: &'static str,
    },
    TerminalNotification {
        method: &'static str,
        predicate: &'static str,
    },
    ExplicitMethodResponse {
        method: &'static str,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CleanupTrigger {
    JsonRpcResponse {
        method: &'static str,
    },
    TerminalNotification {
        method: &'static str,
        predicate: &'static str,
    },
    ExplicitMethodResponse {
        method: &'static str,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerdeShapeEntry {
    pub rust_type: &'static str,
    pub schema_ref: Option<&'static str>,
    pub metadata_status: SerdeMetadataStatus,
    pub schema_sufficient_proof: Option<SchemaSufficientProof>,
    pub fields: Vec<SerdeFieldEntry>,
    pub variant_aliases: Vec<SerdeVariantAliasEntry>,
    pub manual_payload_conversion: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSufficientProof {
    pub checked_required_fields: bool,
    pub checked_nullable_fields: bool,
    pub checked_additional_properties: bool,
    pub checked_enum_values: bool,
    pub checked_union_tags: bool,
    pub no_serde_aliases: bool,
    pub no_serde_defaults: bool,
    pub no_skip_serializing_if: bool,
    pub no_flatten: bool,
    pub no_custom_serde: bool,
    pub source_anchor: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerdeFieldEntry {
    pub rust_field: &'static str,
    pub wire_name: &'static str,
    pub aliases: Vec<&'static str>,
    pub shape: SerdeFieldShape,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerdeFieldShape {
    pub presence: SerdePresence,
    pub default: Option<SerdeDefault>,
    pub skip_serializing_if: Option<&'static str>,
    pub flattened: bool,
    pub custom_serialize: Option<&'static str>,
    pub custom_deserialize: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SerdePresence {
    Required,
    OptionalNonNull,
    OptionalNullable,
    DoubleOption,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerdeDefault {
    pub provider: SerdeDefaultProvider,
    pub wire_value_json: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SerdeDefaultProvider {
    SerdeDefault,
    Function(&'static str),
    TraitDefault(&'static str),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerdeVariantAliasEntry {
    pub rust_variant: &'static str,
    pub canonical_wire_value: &'static str,
    pub aliases: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalMarker {
    pub reason: &'static str,
    pub field_paths: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalFieldMarker {
    pub field_path: &'static str,
    pub reason: &'static str,
    pub inspect_params: bool,
    pub containing_type: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SerdeMetadataStatus {
    SchemaSufficient,
    ManifestRequired,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionReview {
    pub reason: &'static str,
    pub owner: &'static str,
    pub review_note: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DigestSet {
    pub protocol_digest: String,
    pub schema_digest: String,
    pub manifest_digest: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelContextLimits {
    pub max_additional_context_entries: u32,
    pub max_additional_context_key_bytes: u32,
    pub max_additional_context_value_bytes: u32,
    pub max_additional_context_total_bytes: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoundedModelContextField {
    pub method: &'static str,
    pub field_path: &'static str,
    pub limit_profile: &'static str,
}

pub fn go_sdk_manifest() -> GoSdkManifest {
    let mut stable = protocol_mode_manifest(ProtocolModeName::Stable);
    stable.client_requests.push(initialize_request_entry());

    GoSdkManifest {
        manifest_schema_version: 1,
        stable,
        experimental: ProtocolModeManifest {
            protocol_mode: ProtocolModeName::Experimental,
            client_requests: vec![initialize_request_entry()],
            server_requests: Vec::new(),
            server_notifications: vec![NotificationManifestEntry {
                variant: "RawResponseItemCompleted",
                method: "rawResponseItem/completed",
                direction: ProtocolDirection::ServerNotification,
                serde_shape_requirement: SerdeShapeRequirement::SchemaSufficient,
                payload_type: Some("RawResponseItemCompletedNotification"),
                payload_schema_ref: Some("#/definitions/RawResponseItemCompletedNotification"),
                sdk_visibility: SdkVisibility::GeneratedOnly,
                experimental: None,
                experimental_fields: Vec::new(),
                routing_strategy: NotificationRoutingStrategy::RawOnly {
                    reason: "raw response item stream marker",
                },
                manual_payload_conversion: None,
                schema_excluded_reason: Some(
                    "raw response item completion is stripped from the generated JSON ServerNotification method union",
                ),
                exception: Some(ExceptionReview {
                    reason: "JSON ServerNotification method-union exclusion",
                    owner: "app-server-protocol",
                    review_note: "The payload schema exists, but the method-union exclusion must stay explicit for SDK generators.",
                }),
            }],
            client_notifications: Vec::new(),
            serde_shapes: Vec::new(),
            routing_lifecycle: Vec::new(),
            digests: empty_digest_set(),
        },
        model_context_limits: ModelContextLimits {
            max_additional_context_entries: 8,
            max_additional_context_key_bytes: 128,
            max_additional_context_value_bytes: 1000,
            max_additional_context_total_bytes: 4096,
        },
    }
}

fn initialize_request_entry() -> RequestManifestEntry {
    RequestManifestEntry {
        variant: "Initialize",
        method: "initialize",
        direction: ProtocolDirection::ClientToServer,
        request_serialization_scopes: Vec::new(),
        params_type: Some("InitializeParams"),
        params_schema_ref: Some("#/definitions/InitializeParams"),
        response_type: Some("InitializeResponse"),
        response_schema_ref: Some("#/definitions/InitializeResponse"),
        sdk_visibility: SdkVisibility::HandshakeOnly,
        experimental: None,
        experimental_fields: Vec::new(),
        bounded_model_context_fields: Vec::new(),
        inspect_params: false,
        retry: RetryPolicy::NeverRetryAfterWrite,
        manual_payload_conversion: None,
        serde_shape_requirement: SerdeShapeRequirement::SchemaSufficient,
        schema_excluded_reason: None,
        exception: Some(ExceptionReview {
            reason: "legacy handshake request",
            owner: "app-server-protocol",
            review_note: "The Go SDK manifest keeps initialize explicit without treating it as a generated public request.",
        }),
    }
}

pub fn canonical_pretty_manifest_json(manifest: &GoSdkManifest) -> anyhow::Result<String> {
    let value = serde_json::to_value(manifest).context("serialize Go SDK manifest")?;
    let value = sort_object_keys(value);
    let mut json = serde_json::to_string_pretty(&value).context("format Go SDK manifest JSON")?;
    json.push('\n');
    Ok(json)
}

pub fn canonical_manifest_json_from_str(json: &str) -> anyhow::Result<Value> {
    let value: Value = serde_json::from_str(json).context("parse Go SDK manifest JSON")?;
    Ok(sort_object_keys(value))
}

fn protocol_mode_manifest(protocol_mode: ProtocolModeName) -> ProtocolModeManifest {
    ProtocolModeManifest {
        protocol_mode,
        client_requests: Vec::new(),
        server_requests: Vec::new(),
        server_notifications: Vec::new(),
        client_notifications: Vec::new(),
        serde_shapes: Vec::new(),
        routing_lifecycle: Vec::new(),
        digests: empty_digest_set(),
    }
}

fn empty_digest_set() -> DigestSet {
    DigestSet {
        protocol_digest: String::new(),
        schema_digest: String::new(),
        manifest_digest: String::new(),
    }
}

fn sort_object_keys(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(sort_object_keys).collect()),
        Value::Object(map) => {
            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_object_keys(value));
            }
            Value::Object(sorted)
        }
        other => other,
    }
}
