use serde::Serialize;

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
    pub params_type: Option<String>,
    pub params_schema_ref: Option<String>,
    pub response_type: Option<String>,
    pub response_schema_ref: Option<String>,
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
    pub queue_key: Option<&'static str>,
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
    pub payload_type: Option<String>,
    pub payload_schema_ref: Option<String>,
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
    pub rust_type: String,
    pub schema_ref: Option<String>,
    pub metadata_status: SerdeMetadataStatus,
    pub schema_sufficient_proof: Option<SchemaSufficientProof>,
    pub fields: Vec<SerdeFieldEntry>,
    pub variant_aliases: Vec<SerdeVariantAliasEntry>,
    pub manual_payload_conversion: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_note: Option<&'static str>,
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
    pub discriminator: Option<ExperimentalVariantDiscriminator>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalVariantDiscriminator {
    pub field_path: &'static str,
    pub wire_value: &'static str,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitializeDigestSnapshot {
    pub stable_protocol_digest: &'static str,
    pub experimental_protocol_digest: &'static str,
    pub stable_schema_digest: &'static str,
    pub experimental_schema_digest: &'static str,
    pub stable_manifest_digest: &'static str,
    pub experimental_manifest_digest: &'static str,
}

pub fn initialize_digest_snapshot() -> InitializeDigestSnapshot {
    InitializeDigestSnapshot {
        stable_protocol_digest: "07ecf0eafba07f28ff19223bc46e761d500d30062b210d1bb0e8c6b77a958b37",
        experimental_protocol_digest: "7a6a74b4c2ce4311068fdc09aa97d38e0e46fca5dfa17e0d7b488153cd083a21",
        stable_schema_digest: "75288732abb09efc716701a0495e3321cf14c0c20e314c08a95302fcf6c9e28f",
        experimental_schema_digest: "4ac9803376dc348b284a630a582c845a996376914841cf34b5474297e0110c07",
        stable_manifest_digest: "0e8cbf742e065fca82dd66cf300ab0c4b7353beae077cc1e4dc324fc58cd3dce",
        experimental_manifest_digest: "872e0bdc9fee6c896182a9dd483c0cb03d9cb42102b848a8a4c4d3309c9a612a",
    }
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
