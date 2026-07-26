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
        stable_protocol_digest: "c66cdf6b3c0e5d68a30097ccf412b185f7d1807ab0d6b14ce899f155ea5938d4",
        experimental_protocol_digest: "172b3da414d9d353a68d25cb3478f07d94793338658efde9e982559e6959f401",
        stable_schema_digest: "2d8afa5cfebeef8b643c8fab5a644f87868471087bcb96e663cb48796d1b2b7e",
        experimental_schema_digest: "295817bd68d73ab4a041ad5c748e951e1c99516524b41469c8d72929ea65657f",
        stable_manifest_digest: "c53c8cd075ad9f8325fc7a84bac50c834d44b95c095225c59e3c1a7e56341565",
        experimental_manifest_digest: "8e2805a85483d565cb9e3021d682837bd0eee968007624ac7e90f53f2596df80",
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
