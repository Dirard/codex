# Stage 1: Rust Protocol Manifest And Compatibility Digests

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this stage task-by-task.

**Goal:** Make Rust export the complete Go SDK protocol manifest and runtime compatibility digests.

**Architecture:** Rust owns protocol truth. Go must consume generated manifest/schema/digest artifacts and must not parse Rust source or infer routing/serde behavior from JSON Schema alone.

**Tech Stack:** Rust, serde, schemars/ts-rs existing fixtures, app-server initialize flow, Bazel/Cargo-compatible tests.

---

## Required Reading

- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:183-347`
- `docs/superpowers/specs/2026-07-01-go-sdk-design.md:641-663`
- `codex-rs/app-server-protocol/src/protocol/common.rs`
- `codex-rs/app-server-protocol/src/protocol/v1.rs`
- `codex-rs/app-server-protocol/src/export.rs`
- `codex-rs/app-server-protocol/src/schema_fixtures.rs`
- `codex-rs/app-server/src/request_processors/initialize_processor.rs`

## Files

- Create: `codex-rs/app-server-protocol/src/go_manifest.rs`
- Create: `codex-rs/app-server-protocol/src/go_manifest_tests.rs`
- Create: `codex-rs/app-server-protocol/src/bin/write_go_sdk_manifest.rs`
- Create: `codex-rs/app-server-protocol/tests/write_schema_fixtures_check.rs`
- Modify: `codex-rs/app-server-protocol/src/bin/write_schema_fixtures.rs`
- Modify: `codex-rs/app-server-protocol/src/lib.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v1.rs`
- Modify: `codex-rs/app-server-protocol/src/export.rs`
- Modify: `codex-rs/app-server-protocol/src/schema_fixtures.rs`
- Modify: `codex-rs/app-server-protocol/BUILD.bazel`
- Modify/create: exact Bazel fixture/test-data entries in `codex-rs/app-server-protocol/BUILD.bazel` for `tests/write_schema_fixtures_check.rs`.
- Modify: `codex-rs/app-server/src/request_processors/initialize_processor.rs`
- Modify: `codex-rs/app-server/README.md`

## Execution Split

Stage 1 should still execute in coherent implementation slices so failures stay attributable, but fresh blind review runs only after the full Stage 1 checkpoint is implemented and locally verified. Do not dispatch fresh review after each substep or substage. Keep manifest inventory, serde/routing metadata, digest/initialize protocol changes, schema tooling, Bazel, and Python/TypeScript fallout distinguishable in the handoff evidence, but review them together as the complete Stage 1 result.

1. Manifest model and writer: `go_manifest.rs`, manifest tests, writer binary, `write_schema_fixtures.rs --check`, `tests/write_schema_fixtures_check.rs`, Bazel metadata/test data, and `lib.rs` exports.
2. Serde/routing inventory: `common.rs`, source extraction, method/request/notification metadata, and tests proving no generated protocol row is missing required ownership data.
3. Digest and initialize protocol contract: `v1.rs`, initialize processor, schema fixtures, `app-server/README.md`, and compatibility tests.
4. Cross-SDK generated fallout: Go schema fixture tree, Python/TypeScript generated schema fixture updates, Bazel metadata, and drift-check commands.

If a slice needs files outside its listed area, stop and update this plan with that exact file list before implementation continues. The next-stage gate is one fresh blind review cycle over the fully implemented Stage 1, not separate fresh blind cycles for the slices above.
- Create checked-in artifact: `sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json`
- Create checked-in artifact tree: `sdk/go/internal/protocodex/schema-experimental/json`
- Create checked-in artifact tree: `sdk/go/internal/protocodex/schema-experimental/typescript`
- Modify: relevant `BUILD.bazel` files so new Rust source files, bins, tests, and compile/test data are visible under both Cargo and Bazel.

## Tasks

### Task 1.1: Define Manifest Data Model

- [ ] Add `go_manifest` module to `codex-rs/app-server-protocol/src/lib.rs`:

```rust
pub mod go_manifest;

#[cfg(test)]
#[path = "go_manifest_tests.rs"]
mod go_manifest_tests;
```

- [ ] Create `codex-rs/app-server-protocol/src/go_manifest.rs` with serializable manifest structs. Include these public structs exactly enough for tests and generator consumption:

```rust
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
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum NotificationRoutingStrategy {
    Routed { routes: Vec<RoutingRef> },
    RoutedWithGlobalFallback { routes: Vec<RoutingRef>, missing_identity_reason: &'static str },
    GlobalOnly { reason: &'static str },
    RawOnly { reason: &'static str },
    InternalOnly { reason: &'static str },
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
    JsonRpcResponse { method: &'static str },
    TerminalNotification { method: &'static str, predicate: &'static str },
    ExplicitMethodResponse { method: &'static str },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CleanupTrigger {
    JsonRpcResponse { method: &'static str },
    TerminalNotification { method: &'static str, predicate: &'static str },
    ExplicitMethodResponse { method: &'static str },
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
```

- [ ] Add model-context limit structs to the manifest model. At minimum include:

```rust
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
```

The limits must be Rust/app-server-owned and low enough that no single `AdditionalContextEntry.value` can cross the repository's 10K-token model-visible item cap under a conservative bytes-to-token estimate. If any default can plausibly cross 1K tokens, Stage 1 must call that out for manual review before Stage 4 exposes the field.

### Task 1.2: Export Manifest From Existing Macro Inventory

- [ ] Extend the Rust protocol macro inventory in `common.rs` or adjacent helper modules so every `ClientRequest`, `ServerRequest`, `ServerNotification`, and `ClientNotification` entry has:
  - wire method
  - protocol direction (`clientToServer`, `serverToClient`, `serverNotification`, or `clientNotification`)
  - request serialization scopes for every client-to-server request, mirroring `ClientRequestSerializationScope` exactly (`global`, `globalSharedRead`, `thread`, `threadPath`, `commandExecProcess`, `process`, `fuzzyFileSearchSession`, `fsWatch`, `mcpOauth`) plus conditional discriminator and identity extractor field paths. Requests with Rust `serialization: None` must have an empty `requestSerializationScopes` array. Requests whose Rust serialization can resolve to multiple scope kinds, such as `thread_or_path`, must emit one conditional scope per branch rather than collapsing to a single kind.
  - serde shape requirement (`schemaSufficient`, `manifestRequired`, or `manualPayloadConversion`) for params/payload encoding decisions
  - params/payload type
  - params/payload schema reference when present
  - response type for requests
  - response schema reference for requests
  - `sdk_visibility`
  - method/notification experimental marker
  - field-level `experimental_fields` with field path, reason, containing type, and whether params inspection is required
  - `inspect_params` when field-level experimental gating is required
  - retry policy
  - notification routing strategy for every `ServerNotification`
  - manual payload conversion reason when schema/serde metadata is intentionally insufficient
  - schema-excluded reason when needed
  - exception review row when non-public.

- [ ] Add explicit exception rows for:
  - `initialize` as `handshakeOnly`
  - `getConversationSummary`, `gitDiffToRemote`, `getAuthStatus` as `compatibilityOnly`
  - `mock/experimentalMethod` as `internalTestOnly`
  - deprecated legacy server requests such as `applyPatchApproval` and `execCommandApproval`
  - schema-excluded `rawResponseItem/completed`.

- [ ] Add request serialization scope tests that fail if any current `ClientRequest` with Rust `serialization:` metadata is missing `requestSerializationScopes`, if a scope kind is not one of the current `ClientRequestSerializationScope` variants, if branch `precedence` is missing or duplicated within one method, if an extractor field path does not resolve against the params type, or if changing a Rust serialization helper such as `thread_id`, `thread_or_path`, `process_handle`, `fs_watch_id`, or `mcp_oauth` does not change the manifest digest.
- [ ] Add explicit ordered-decision golden tests for `thread/resume` and `thread/fork`: each method must encode the exact current Rust `thread_or_path(params.thread_id, params.path)` precedence from `codex-rs/app-server-protocol/src/protocol/common.rs`: first `Thread` when `thread_id` is non-empty, then `ThreadPath` when `thread_id` is empty and `path` is present, then the fallback `Thread` branch when both `thread_id` is empty and `path` is absent. The manifest must use `StringNonEmpty`, `StringEmpty`, `FieldPresent`, `FieldAbsent`, and `All`/`Not` or an equivalent Rust-owned ordered decision table, not plain `FieldPresent(thread_id)` for string semantics. Tests must cover `thread_id+path`, empty `thread_id+path`, and neither field for both methods, fail if either branch is missing, fail if precedence changes without manifest digest change, and explicitly resolve any doc/source conflict in favor of the current Rust helper or by changing the Rust helper plus schema/docs in the same stage.

- [ ] Add `pub fn go_sdk_manifest() -> GoSdkManifest` to `go_manifest.rs`.

- [ ] Write manifest tests in `go_manifest_tests.rs`:

```rust
#[test]
fn go_sdk_manifest_contains_raw_response_item_completed() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let has_notification = manifest.experimental.server_notifications.iter().any(|entry| {
        entry.method == "rawResponseItem/completed"
            && entry.schema_excluded_reason.is_some()
    });
    assert!(has_notification);
}

#[test]
fn go_sdk_manifest_classifies_initialize_as_handshake_only() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let initialize = manifest.experimental.client_requests.iter().find(|entry| entry.method == "initialize").unwrap();
    assert_eq!(initialize.sdk_visibility, crate::go_manifest::SdkVisibility::HandshakeOnly);
}
```

- [ ] Verify the sibling test file is registered by running the exact test names:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just test -p codex-app-server-protocol go_sdk_manifest_contains_raw_response_item_completed
just test -p codex-app-server-protocol go_sdk_manifest_classifies_initialize_as_handshake_only
```

### Task 1.3: Add Serde Shape And Routing Fixtures

- [ ] Add Rust-owned serde shape entries for:
  - `AppScreenshot.fileId` alias `file_id`
  - `AppScreenshot.userPrompt` alias `user_prompt`
  - `FileSystemSpecialPath.ProjectRoots` alias `current_working_directory`
  - representative double-option, flattened map, default/skip bool, renamed enum, and manual conversion payloads from current protocol types.

- [ ] Add Rust-owned routing/lifecycle entries for every SDK-neutral wire lifecycle primitive used by a high-level Go SDK handle or notification-backed workflow:
  - thread/turn notifications
  - login/OAuth notifications
  - `command/exec` cleanup by JSON-RPC response
  - `process/spawn` cleanup by `process/exited`
  - filesystem watch wire cleanup by `fs/unwatch` response
  - MCP OAuth handles, including `mcpServer/oauthLogin/completed`
  - fuzzy file search sessions, including `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, `fuzzyFileSearch/sessionStop`, and terminal `fuzzyFileSearch/sessionCompleted`
  - realtime sessions and review handles when the reviewed resource mapping exposes them as handle-start or notification-backed workflows.
- [ ] Every `handle-start` or notification-backed workflow named by `resourceAPIMappings` must have either a matching Rust-owned `RoutingLifecycleEntry` or an explicit reviewed `NoWireLifecycle` exception row with reason, owner, and test owner. Do not let Stage 4 or Stage 5 invent Go-only lifecycle cleanup for wire-terminal workflows that are visible in the manifest or appendix.
- [ ] Every `ServerNotification` must have exactly one explicit routing strategy:
  - `Routed` with one or more `RoutingRef` rows and one or more `IdentityExtractor` rows per route
  - `RoutedWithGlobalFallback` with one or more `RoutingRef` rows plus a `missingIdentityReason`, only when the Rust payload identity field is actually optional and the reviewed table says missing identity is global
  - `GlobalOnly` with a reason
  - `RawOnly` with a reason
  - `InternalOnly` with a reason.
- [ ] Populate current routing metadata from the reviewed table in `docs/superpowers/plans/2026-07-02-go-sdk-full/appendix-current-protocol-inventory.md#server-notification-routing-review-seed`. Do not infer notification routes from method prefixes or payload field names without matching the reviewed table.
- [ ] `Routed` and `RoutedWithGlobalFallback` notifications must support multiple identities in the same payload, such as thread plus turn. Required Rust fields must be modeled as required extractors; optional Rust fields must either stay routed-optional within a route or use `RoutedWithGlobalFallback` with a reviewed global fallback reason. Do not collapse multi-identity payloads to a single `RoutingRef`.
- [ ] Each `RoutingLifecycleEntry` must include `resourceDomain`, `wireIdentitySource`, `startMethod`, `startCompletion`, typed Rust cleanup triggers, and notification opt-out dependencies. Do not put Go-owned triggers such as `handleClose`, `clientClose`, `contextCancel`, `timeout`, or `overflow` in the Rust manifest.

- [ ] Add tests that fail when aliases or lifecycle entries are absent. Lifecycle tests must cover at least turn/thread, login/OAuth, MCP OAuth, process, command, filesystem watch, fuzzy file search, realtime, and review handle rows when those workflows are exposed by the reviewed mapping, plus any explicit `NoWireLifecycle` exceptions.
- [ ] Add manifest completeness tests that iterate all request and notification entries and fail unless:
  - `direction` matches the owning protocol enum (`ClientRequest` is `clientToServer`; `ServerRequest` is `serverToClient`; `ServerNotification` is `serverNotification`; `ClientNotification` is `clientNotification`)
  - every `ClientRequest` entry has explicit `requestSerializationScopes`; each scope `kind` exactly matches one current Rust `ClientRequestSerializationScope` variant, each condition is explicit, and identity extractor paths resolve against the request params type
  - notification entries do not use `requestSerializationScopes`; notification identity, terminal cleanup, and opt-out dependencies are represented only by routing/lifecycle metadata
  - every request, response, notification, and server-request payload type has an explicit `serdeShapeRequirement`
  - `manualPayloadConversion` metadata is required only when `serdeShapeRequirement == ManualPayloadConversion`; tests must fail if manual conversion is modeled as a request serialization scope
  - every `inspectParams` request has at least one matching `experimentalFields` row
  - every `experimentalFields` path resolves to the params/payload type named by that entry
  - every `ServerNotification` has `routingStrategy`
  - every routed notification has non-empty routes and non-empty identity extractors
  - every identity extractor field path resolves against the notification payload type
  - every optional identity extractor has an explicit behavior: route omission within a still-routed notification, or `RoutedWithGlobalFallback` with a non-empty `missingIdentityReason`; required Rust fields must never be marked optional in the manifest
  - every current `ServerNotification` method exactly matches the reviewed routing/lifecycle table in the appendix, including optionality, global/internal classification, and terminal cleanup contribution
  - every terminal notification used by lifecycle cleanup has an explicit terminal predicate or cleanup contribution
  - every global/raw/internal notification has a non-empty reason
  - every public or generated-only entry has schema refs unless an explicit schema-excluded exception permits omission.
- [ ] Add exhaustive serde coverage tests. The tests must build the transitive closure of every JSON Schema definition reachable from every request params type, request response type, notification payload type, server request params type, and server request response type used by the Go SDK manifest. Each reachable Rust/schema type must have either:
  - `SerdeMetadataStatus::SchemaSufficient` with non-empty `schemaSufficientProof` proving JSON Schema fully preserves wire shape for that type
  - or `SerdeMetadataStatus::ManifestRequired` with explicit metadata for nullable/double-option, default values and providers, `skip_serializing_if` predicates, flatten, field aliases, variant aliases, rename policy, custom serialize/deserialize hooks, manual conversion, experimental field paths, and generated validation requirement.
- [ ] The serde test and Go generator must fail when `metadataStatus == SchemaSufficient` and `schemaSufficientProof` is absent or does not explicitly rule out aliases, defaults, skip predicates, flatten, custom serde hooks, and schema-lost union/nullability shape.
- [ ] The transitive serde test must fail when a nested reachable type is absent from `serdeShapes`; top-level payload coverage is not sufficient.
- [ ] Add golden fixtures for default-true fields, default collection fields, nested aliases, flattened maps, and custom serialize/deserialize paths. Include source anchors for at least `Config` defaults, thread fields that deserialize through custom helpers, and plugin/config nested alias or flattened-map shapes.
- [ ] Keep representative golden fixtures for specific known tricky shapes, but do not treat representative fixtures as the acceptance boundary.

### Task 1.4: Write Checked-In Manifest And Schema Inputs

- [ ] Add manifest canonical JSON helpers in `go_manifest.rs` before creating the writer binary:
  - `canonical_pretty_manifest_json(&GoSdkManifest) -> anyhow::Result<String>` writes sorted, deterministic pretty JSON with LF endings
  - `canonical_manifest_json_from_str(&str) -> anyhow::Result<serde_json::Value>` parses JSON and recursively sorts object keys so check mode is semantic and line-ending independent.
- [ ] Create `codex-rs/app-server-protocol/src/bin/write_go_sdk_manifest.rs`:

```rust
use std::fs;
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    output: PathBuf,

    #[arg(long)]
    check: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let manifest = codex_app_server_protocol::go_manifest::go_sdk_manifest();
    let json = codex_app_server_protocol::go_manifest::canonical_pretty_manifest_json(&manifest)?;
    if args.check {
        let existing = fs::read_to_string(&args.output)?;
        let existing = codex_app_server_protocol::go_manifest::canonical_manifest_json_from_str(&existing)?;
        let generated = codex_app_server_protocol::go_manifest::canonical_manifest_json_from_str(&json)?;
        anyhow::ensure!(existing == generated, "Go SDK manifest drift: {}", args.output.display());
        return Ok(());
    }
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, json)?;
    Ok(())
}
```

- [ ] Wire the binary into Cargo/Bazel in the same style as `write_schema_fixtures`.
- [ ] `write_go_sdk_manifest --check` must compare parsed canonical JSON, not raw bytes or platform-dependent line endings. Add a CRLF fixture test proving a semantically identical checked-in manifest does not produce false drift, and a changed field does.
- [ ] Extend `codex-rs/app-server-protocol/src/bin/write_schema_fixtures.rs` with a `--check` flag before using check-mode commands. In check mode it must generate into a temporary directory, compare the generated `json` and `typescript` trees against `--schema-root`, and fail with a clear drift error without mutating the checked-in tree. JSON files must be compared as parsed canonical JSON; TypeScript files must be compared after LF normalization. Add CRLF fixture coverage and at least one changed-content fixture proving real drift still fails.
- [ ] From repo root, generate the checked-in Go SDK manifest:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
```

- [ ] Generate the checked-in experimental schema fixture tree from Rust truth:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --schema-root sdk/go/internal/protocodex/schema-experimental
```

- [ ] Add check-mode commands that compare checked-in artifacts without mutating them:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --check --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
```

- [ ] Verify there are no unexpected changed paths before committing. This pre-commit gate must allow Stage 1 protocol/app-server files, generated Go manifest/schema artifacts, required Python/TypeScript SDK fallout from the shared `InitializeResponse` contract, and lockfiles only:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
unexpected="$(git status --short --untracked-files=all | awk '$2 !~ /^(codex-rs\/app-server\/README\.md$|codex-rs\/app-server-protocol(\/|$)|codex-rs\/app-server(\/|$)|sdk\/go\/internal\/protocodex(\/|$)|sdk\/python(\/|$)|sdk\/typescript(\/|$)|MODULE\.bazel\.lock$|codex-rs\/Cargo\.lock$)/ { print }')"
test -z "${unexpected}" || { printf '%s\n' "${unexpected}"; exit 1; }
```

### Task 1.5: Implement Compatibility Digests

- [ ] Extend the deterministic canonical JSON helpers in `go_manifest.rs` for digest input construction.
- [ ] Define the canonical digest object with exactly these top-level fields:
  - `manifestSchemaVersion`
  - `protocolMode`
  - `schemaInputs`
  - `manifestInputs`
  - `requestSerializationInputs`
  - `serdeShapeInputs`
  - `visibilityInputs`
  - `routingLifecycleInputs`
  - `experimentalFilterInputs`.
- [ ] Define a digest-input projection for `GoSdkManifest` before computing any digest. The projection must omit `ProtocolModeManifest.digests`, `DigestSet`, and every digest/checksum field derived from the manifest itself. Compute all digests from that projection, then fill `stable.digests` and `experimental.digests` only after the digest inputs are finalized. The projection must still include every non-derived manifest section that affects generated Go protocol behavior.
- [ ] Implement schema-aware canonicalization for JSON Schema inputs. Object keys must be sorted, insignificant whitespace removed, and raw platform-dependent fixture bytes must not be digest inputs. Preserve array order by default. The only Rust-owned validation-order-insensitive array keywords that may be sorted are:
  - `required`
  - `enum`, only after every element is a scalar JSON string/number/bool/null
  - `type`, only when encoded as an array of primitive JSON Schema type names.
  Do not sort `oneOf`, `anyOf`, `allOf`, `prefixItems`, `items`, `examples`, `default`, or unknown array-valued keywords.
- [ ] Compute stable and experimental digest sets over:
  - selected schema input canonical hashes
  - manifest sections
  - request serialization scope metadata and identity extractors
  - serde shape metadata
  - visibility metadata
  - routing/lifecycle metadata
  - experimental filtering mode.
- [ ] Ensure digest inputs do not include local paths, timestamps, host OS, Go generated output, or Go package names.
- [ ] Add tests:

```rust
#[test]
fn stable_and_experimental_protocol_digests_are_distinct() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    assert_ne!(
        manifest.stable.digests.protocol_digest,
        manifest.experimental.digests.protocol_digest
    );
}

#[test]
fn manifest_digest_is_lowercase_sha256_hex() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let digest = &manifest.experimental.digests.manifest_digest;
    assert_eq!(digest.len(), 64);
    assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
}
```
- [ ] Add contract tests for:
  - required canonical digest object fields
  - schema-aware canonicalization of equivalent schema JSON
  - reorderings of allowlisted arrays keep the same digest
  - reorderings of non-allowlisted arrays such as `oneOf`, `anyOf`, and `allOf` change the digest
  - mutating only embedded `DigestSet` fields does not change the digest-input projection
  - mutating a non-derived manifest field still changes the relevant digest after digests are filled back into the checked-in manifest
  - Cargo and Bazel digest parity
  - Linux/macOS/Windows equivalent schema generation producing the same digest
  - changing any selected schema/manifest/request-serialization/serde/visibility/routing/experimental input changes the relevant digest
  - digest generation failing if it attempts to read `sdk/go` generated output.
- [ ] Update `codex-rs/app-server-protocol/BUILD.bazel` or adjacent Bazel metadata for every new Rust file, binary, fixture, schema/manifest input, and test data path needed by the digest tests. The Bazel target must not depend on generated `sdk/go` output.
- [ ] Prove the digest tests run under both build systems before Stage 1 review:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
cargo nextest --version
just test -p codex-app-server-protocol stable_and_experimental_protocol_digests_are_distinct
just test -p codex-app-server-protocol manifest_digest_is_lowercase_sha256_hex

cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
bazel version
bazel test //codex-rs/app-server-protocol:app-server-protocol-unit-tests
```

Native Windows local verification must not run the bare Bazel commands above directly. Use the same bootstrap contract as `.github/actions/setup-bazel-ci`: short `BAZEL_OUTPUT_USER_ROOT`, `BAZEL_REPO_CONTENTS_CACHE`, Visual Studio/MSVC environment materialization through `VsDevCmd.bat`, computed Bazel Windows `PATH`, and `git config --global core.longpaths true`. If that bootstrap helper is not yet implemented locally in Stage 1, scope these exact Bazel commands to Unix/Git Bash and keep native Windows validation blocked until Stage 6's `stage-codex-runtime.ps1` bootstrap path exists and passes the Stage 7 Windows check.

Expected: `cargo-nextest` and Bazel are available; install repo-pinned `cargo-nextest` version `0.9.103` and the repo-supported Bazel/Bazelisk tool before continuing if either command is missing. The Cargo commands prove targeted digest tests, and the local Bazel command proves the same crate tests and runfiles/data visibility under the current host Bazel setup. CI wrapper parity is verified by the Stage 6 workflow, not by locally invoking `.github/scripts/run-bazel-ci.sh` without its CI environment. A missing Bazel target, missing runfiles data, or digest mismatch under Bazel blocks Stage 1.

### Task 1.6: Expose Digests In The Active Initialize Surface Without Expanding v1

- [ ] Do not add required fields to `codex-rs/app-server-protocol/src/protocol/v1.rs` `InitializeResponse`. That type remains the legacy initialize response shape because repo policy says active API development must not add new surface area to v1. Stage 1 must fail review if the implementation adds these fields directly to `v1::InitializeResponse`.
- [ ] Create the current initialize response type in an active/shared protocol surface instead: prefer a new unversioned/shared response type owned from `codex-rs/app-server-protocol/src/protocol/common.rs` or an active v2/shared module that `common.rs` can use for the public `Initialize` response. Keep `v1::InitializeParams` only if the request shape remains unchanged, and update `client_request_definitions!` so `Initialize` no longer returns `v1::InitializeResponse`.
- [ ] Add required camelCase fields to the active/current initialize response type:
  - `stableProtocolDigest`
  - `experimentalProtocolDigest`
  - `stableSchemaDigest`
  - `experimentalSchemaDigest`
  - `stableManifestDigest`
  - `experimentalManifestDigest`
  - `activeProtocolMode`.
- [ ] Keep the active Rust initialize response schema strict and current-only after these fields are added. Do not make the new digest/mode fields optional to support legacy Go SDK compatibility tests, and do not add a second public app-server initialize response variant. Legacy/dev compatibility is owned by Stage 3's Go-internal raw initialize compatibility envelope before current generated decode.
- [ ] Modify `initialize_processor.rs` to populate fields from embedded/Rust-owned digest constants derived from `go_sdk_manifest()` and `InitializeCapabilities.experimentalApi`. Runtime initialize must not read schema fixture directories, checked-in manifest JSON, TypeScript output, `sdk/go` generated files, Bazel runfiles, or the source tree to compute these values. Build/test-time generators may read source fixtures, but the app-server binary must carry the selected digest values in code or build-time embedded data.
- [ ] Add a Stage 1 app-server smoke test that starts initialize from a temporary cwd with schema fixture directories and `sdk/go` output absent from the runtime filesystem, then proves all initialize digest fields and `activeProtocolMode` are still present. Include a Bazel variant or runfiles-stripped fixture so a source-tree read cannot pass locally and fail in staged CI.
- [ ] Update `codex-rs/app-server/README.md`, app-server schema fixtures, TypeScript exports, and any generated protocol docs/tests affected by moving `Initialize` from the v1 response type to the active/shared current response type. The handoff must list every non-Go consumer touched by this public app-server contract change.
- [ ] Inspect/update Python SDK initialize handling and tests so the added required fields do not break startup. At minimum, update or add tests near `sdk/python/src/openai_codex/client.py` and existing app-server run/streaming/login tests to prove unknown/added initialize fields are tolerated or surfaced safely.
- [ ] Inspect/update TypeScript SDK generated protocol expectations or tests affected by `InitializeResponse` schema changes. Regenerate TypeScript fixtures as part of the same change and verify existing TypeScript SDK checks still pass or are updated intentionally.
- [ ] Run the same-stage cross-SDK fallout checks below. Do not defer these if the package manager is available; a failure here blocks Stage 1 because `InitializeResponse` is shared app-server protocol surface:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/python
uv run --group test pytest tests/test_app_server_run.py tests/test_app_server_streaming.py tests/test_app_server_login.py tests/test_client_rpc_methods.py
uv run --group format ruff check src tests

cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/sdk/typescript
pnpm install --frozen-lockfile
pnpm run build
pnpm run lint
pnpm test
```

- [ ] Record exact Python and TypeScript command output status in the Stage 1 review handoff. If `uv` or `pnpm` is unavailable, install/use the repository-pinned tool before marking the stage complete; do not silently skip cross-SDK checks.

Current Stage 1 TypeScript fallout evidence (2026-07-08):

- Source inspection found no `InitializeResponse`, `protocolDigest`, `schemaDigest`, `manifestDigest`, or `modelContextLimits` consumer under `sdk/typescript`.
- The tracked workspace package files were unchanged while investigating this gate: `git status --short -- package.json pnpm-lock.yaml pnpm-workspace.yaml sdk/typescript/package.json` returned no paths before the TypeScript checks.
- Using the repo-pinned package manager through Corepack (`pnpm@10.33.0`), `CI=true corepack pnpm install --frozen-lockfile --ignore-scripts` completed successfully from `sdk/typescript`.
- With `CODEX_EXEC_PATH=/home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs/target/debug/codex`, `corepack pnpm run build`, `corepack pnpm run lint`, and `corepack pnpm test` completed successfully from `sdk/typescript`; Jest reported 4 passed suites and 37 passed tests.

### Task 1.7: Regenerate And Verify

- [ ] Run:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full/codex-rs
just write-app-server-schema
cd ..
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --schema-root sdk/go/internal/protocodex/schema-experimental
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_go_sdk_manifest -- --check --output sdk/go/internal/protocodex/manifest/app_server_protocol_manifest.json
cargo run --manifest-path codex-rs/Cargo.toml -p codex-app-server-protocol --bin write_schema_fixtures -- --experimental --check --schema-root sdk/go/internal/protocodex/schema-experimental
cd codex-rs
just test -p codex-app-server-protocol
just test -p codex-app-server
just fix -p codex-app-server-protocol
just fix -p codex-app-server
just fmt
cd ..
bazel version
bazel test //codex-rs/app-server-protocol:app-server-protocol-unit-tests
```

- [ ] If Rust dependencies changed, run from repo root:

```bash
just bazel-lock-update
```

- [ ] Commit per reviewed substage. Do not squash these before fresh review:

```bash
git add codex-rs/app-server-protocol/src/go_manifest.rs codex-rs/app-server-protocol/src/go_manifest_tests.rs codex-rs/app-server-protocol/src/bin/write_go_sdk_manifest.rs codex-rs/app-server-protocol/src/bin/write_schema_fixtures.rs codex-rs/app-server-protocol/tests/write_schema_fixtures_check.rs codex-rs/app-server-protocol/src/lib.rs
git add codex-rs/app-server-protocol/BUILD.bazel
git commit -m "feat(app-server): add Go SDK protocol manifest writer"

git add codex-rs/app-server-protocol/src/protocol/common.rs codex-rs/app-server-protocol/src/export.rs
git commit -m "feat(app-server): export Go SDK protocol routing metadata"

git add codex-rs/app-server-protocol/src/protocol/common.rs codex-rs/app-server-protocol/src/protocol/v2 codex-rs/app-server-protocol/src/schema_fixtures.rs codex-rs/app-server/src/request_processors/initialize_processor.rs codex-rs/app-server/README.md
git commit -m "feat(app-server): expose protocol compatibility digests"

git add sdk/go/internal/protocodex sdk/python sdk/typescript
git add codex-rs/app-server-protocol/BUILD.bazel codex-rs/app-server-protocol/tests/write_schema_fixtures_check.rs
git add MODULE.bazel.lock codex-rs/Cargo.lock
git commit -m "chore(sdk): refresh protocol schema fixtures"
```

- [ ] Verify the stage commit left the worktree clean, including untracked generated files:

```bash
cd /home/dirard/dev/ai-apps/codex/.worktrees/codex-go-sdk-full
status="$(git status --porcelain=v1 --untracked-files=normal --ignore-submodules=none)"
test -z "${status}" || { printf '%s\n' "${status}"; exit 1; }
```

## Stage Review

Fresh blind engineering review is mandatory. Product review is required because this stage changes the cross-SDK initialize contract.
