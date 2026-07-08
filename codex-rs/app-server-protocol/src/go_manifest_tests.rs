use pretty_assertions::assert_eq;
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::protocol::common::ClientRequest;
use crate::protocol::common::ClientRequestSerializationScope;
use crate::protocol::common::FuzzyFileSearchSessionUpdateParams;
use crate::protocol::v1;
use crate::protocol::v2;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_absolute_path::test_support::PathBufExt;
use codex_utils_absolute_path::test_support::test_path_buf;

#[test]
fn go_sdk_manifest_contains_raw_response_item_completed() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let notification = manifest
        .experimental
        .server_notifications
        .iter()
        .find(|entry| entry.method == "rawResponseItem/completed")
        .unwrap();
    assert_eq!(
        notification.payload_type.as_deref(),
        Some("RawResponseItemCompletedNotification")
    );
    assert_eq!(
        notification.payload_schema_ref.as_deref(),
        Some("#/definitions/RawResponseItemCompletedNotification")
    );
    assert!(notification.schema_excluded_reason.is_some());
}

#[test]
fn go_sdk_manifest_includes_initialize_handshake_for_stable_and_experimental_modes() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    let stable_initialize = initialize_entry(&manifest.stable.client_requests, "stable");
    let experimental_initialize =
        initialize_entry(&manifest.experimental.client_requests, "experimental");

    assert_eq!(
        stable_initialize.sdk_visibility,
        crate::go_manifest::SdkVisibility::HandshakeOnly
    );
    assert_eq!(
        experimental_initialize.sdk_visibility,
        crate::go_manifest::SdkVisibility::HandshakeOnly
    );
    assert_eq!(
        experimental_initialize.response_type.as_deref(),
        Some("InitializeResponse")
    );
}

#[test]
fn notification_routing_global_fallback_uses_camel_case_fields() -> anyhow::Result<()> {
    let strategy = crate::go_manifest::NotificationRoutingStrategy::RoutedWithGlobalFallback {
        routes: vec![crate::go_manifest::RoutingRef {
            resource_domain: "thread",
            wire_identity_source: "threadId",
            identity_extractors: Vec::new(),
        }],
        missing_identity_reason: "legacy notification lacks a thread identity",
    };

    assert_eq!(
        serde_json::to_value(strategy)?,
        serde_json::json!({
            "kind": "routedWithGlobalFallback",
            "routes": [
                {
                    "resourceDomain": "thread",
                    "wireIdentitySource": "threadId",
                    "identityExtractors": [],
                }
            ],
            "missingIdentityReason": "legacy notification lacks a thread identity",
        })
    );

    Ok(())
}

#[test]
fn canonical_manifest_json_accepts_crlf_equivalent_manifest() -> anyhow::Result<()> {
    let manifest_json =
        crate::go_manifest::canonical_pretty_manifest_json(&crate::go_manifest::go_sdk_manifest())?;
    let crlf_manifest_json = manifest_json.replace('\n', "\r\n");

    assert_eq!(
        crate::go_manifest::canonical_manifest_json_from_str(&manifest_json)?,
        crate::go_manifest::canonical_manifest_json_from_str(&crlf_manifest_json)?
    );

    Ok(())
}

#[test]
fn canonical_manifest_json_detects_changed_manifest_field() -> anyhow::Result<()> {
    let manifest_json =
        crate::go_manifest::canonical_pretty_manifest_json(&crate::go_manifest::go_sdk_manifest())?;
    let changed_manifest_json = manifest_json.replacen(
        "\"manifestSchemaVersion\": 1",
        "\"manifestSchemaVersion\": 2",
        1,
    );

    assert_ne!(
        crate::go_manifest::canonical_manifest_json_from_str(&manifest_json)?,
        crate::go_manifest::canonical_manifest_json_from_str(&changed_manifest_json)?
    );

    Ok(())
}

#[test]
fn go_sdk_manifest_exports_macro_owned_protocol_inventory() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert!(
        manifest
            .experimental
            .client_requests
            .iter()
            .any(|entry| entry.method == "thread/resume")
    );
    assert!(
        manifest
            .experimental
            .server_requests
            .iter()
            .any(|entry| entry.method == "item/commandExecution/requestApproval")
    );
    assert!(
        manifest
            .experimental
            .server_notifications
            .iter()
            .any(|entry| entry.method == "turn/started")
    );
    assert!(
        manifest
            .experimental
            .client_notifications
            .iter()
            .any(|entry| entry.method == "initialized")
    );
}

#[test]
fn thread_or_path_scopes_keep_current_ordered_rust_precedence() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    for method in ["thread/resume", "thread/fork"] {
        let entry = manifest
            .experimental
            .client_requests
            .iter()
            .find(|entry| entry.method == method)
            .unwrap_or_else(|| panic!("{method} should be in the Go SDK manifest"));

        assert_eq!(
            entry.request_serialization_scopes,
            vec![
                crate::go_manifest::RequestSerializationScope {
                    kind: crate::go_manifest::RequestSerializationScopeKind::Thread,
                    queue_key: None,
                    precedence: 0,
                    condition: crate::go_manifest::RequestSerializationCondition::StringNonEmpty(
                        "thread_id"
                    ),
                    identity_extractors: vec![crate::go_manifest::IdentityExtractor {
                        identity_name: "threadId",
                        field_path: "thread_id",
                        optional: false,
                        terminal_predicate: None,
                    }],
                },
                crate::go_manifest::RequestSerializationScope {
                    kind: crate::go_manifest::RequestSerializationScopeKind::ThreadPath,
                    queue_key: None,
                    precedence: 1,
                    condition: crate::go_manifest::RequestSerializationCondition::All(&[
                        crate::go_manifest::RequestSerializationCondition::StringEmpty("thread_id"),
                        crate::go_manifest::RequestSerializationCondition::FieldPresent("path"),
                    ]),
                    identity_extractors: vec![crate::go_manifest::IdentityExtractor {
                        identity_name: "path",
                        field_path: "path",
                        optional: false,
                        terminal_predicate: None,
                    }],
                },
                crate::go_manifest::RequestSerializationScope {
                    kind: crate::go_manifest::RequestSerializationScopeKind::Thread,
                    queue_key: None,
                    precedence: 2,
                    condition: crate::go_manifest::RequestSerializationCondition::All(&[
                        crate::go_manifest::RequestSerializationCondition::StringEmpty("thread_id"),
                        crate::go_manifest::RequestSerializationCondition::FieldAbsent("path"),
                    ]),
                    identity_extractors: vec![crate::go_manifest::IdentityExtractor {
                        identity_name: "threadId",
                        field_path: "thread_id",
                        optional: false,
                        terminal_predicate: None,
                    }],
                },
            ],
            "{method} must mirror common.rs thread_or_path(params.thread_id, params.path)"
        );
    }
}

#[test]
fn manifest_exception_rows_cover_reviewed_non_public_protocol_entries() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert_eq!(
        client_request(&manifest.experimental.client_requests, "initialize").sdk_visibility,
        crate::go_manifest::SdkVisibility::HandshakeOnly
    );
    for method in ["getConversationSummary", "gitDiffToRemote", "getAuthStatus"] {
        let entry = client_request(&manifest.experimental.client_requests, method);
        assert_eq!(
            entry.sdk_visibility,
            crate::go_manifest::SdkVisibility::CompatibilityOnly
        );
        assert_eq!(entry.params_schema_ref, None);
        assert_eq!(entry.response_schema_ref, None);
        assert_eq!(
            entry.schema_excluded_reason,
            Some(
                "deprecated v1 compatibility request schemas are intentionally excluded from Go SDK schema inputs"
            )
        );
    }
    assert_eq!(
        client_request(
            &manifest.experimental.client_requests,
            "mock/experimentalMethod"
        )
        .sdk_visibility,
        crate::go_manifest::SdkVisibility::InternalTestOnly
    );

    for method in ["applyPatchApproval", "execCommandApproval"] {
        assert_eq!(
            server_request(&manifest.experimental.server_requests, method).sdk_visibility,
            crate::go_manifest::SdkVisibility::CompatibilityOnly
        );
    }

    let raw_completed = notification(
        &manifest.experimental.server_notifications,
        "rawResponseItem/completed",
    );
    assert_eq!(
        raw_completed.sdk_visibility,
        crate::go_manifest::SdkVisibility::GeneratedOnly
    );
    assert!(raw_completed.schema_excluded_reason.is_some());
    assert!(raw_completed.exception.is_some());
}

#[test]
fn manifest_entries_have_protocol_owned_directions_and_reviewed_routing() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert!(
        manifest
            .experimental
            .client_requests
            .iter()
            .all(|entry| entry.direction == crate::go_manifest::ProtocolDirection::ClientToServer)
    );
    assert!(
        manifest
            .experimental
            .server_requests
            .iter()
            .all(|entry| entry.direction == crate::go_manifest::ProtocolDirection::ServerToClient)
    );
    assert!(
        manifest
            .experimental
            .server_notifications
            .iter()
            .all(|entry| entry.direction
                == crate::go_manifest::ProtocolDirection::ServerNotification)
    );
    assert!(
        manifest
            .experimental
            .client_notifications
            .iter()
            .all(|entry| entry.direction
                == crate::go_manifest::ProtocolDirection::ClientNotification)
    );

    for entry in &manifest.experimental.server_notifications {
        assert!(
            !matches!(
                entry.routing_strategy,
                crate::go_manifest::NotificationRoutingStrategy::InternalOnly { .. }
            ),
            "{} must be covered by the reviewed server notification routing seed",
            entry.method
        );
    }
}

#[test]
fn server_notification_routing_matches_reviewed_seed_examples() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert_routed_extractors(
        notification(&manifest.experimental.server_notifications, "turn/started"),
        &["threadId", "turn.id"],
    );
    assert_routed_extractors(
        notification(
            &manifest.experimental.server_notifications,
            "mcpServer/oauthLogin/completed",
        ),
        &["name", "threadId?"],
    );
    assert_routed_extractors(
        notification(
            &manifest.experimental.server_notifications,
            "remoteControl/status/changed",
        ),
        &["environmentId?", "installationId", "serverName"],
    );

    let warning = notification(&manifest.experimental.server_notifications, "warning");
    match &warning.routing_strategy {
        crate::go_manifest::NotificationRoutingStrategy::RoutedWithGlobalFallback {
            routes,
            missing_identity_reason,
        } => {
            assert_eq!(
                missing_identity_reason,
                &"warning notifications without threadId are displayed globally"
            );
            assert_eq!(
                extractor_paths(&routes[0].identity_extractors),
                vec!["threadId?"]
            );
        }
        other => panic!("warning should route with global fallback, got {other:?}"),
    }

    assert!(matches!(
        notification(
            &manifest.experimental.server_notifications,
            "skills/changed"
        )
        .routing_strategy,
        crate::go_manifest::NotificationRoutingStrategy::GlobalOnly { .. }
    ));
    assert_routed_extractors(
        notification(
            &manifest.experimental.server_notifications,
            "rawResponseItem/completed",
        ),
        &["threadId", "turnId"],
    );
}

#[test]
fn server_notification_experimental_markers_are_exported_from_attrs() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let process_exited = notification(
        &manifest.experimental.server_notifications,
        "process/exited",
    );

    assert_eq!(
        process_exited
            .experimental
            .as_ref()
            .map(|marker| marker.reason),
        Some("process/exited")
    );
}

#[test]
fn inspect_params_requests_export_field_level_experimental_metadata() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    for entry in manifest
        .experimental
        .client_requests
        .iter()
        .filter(|entry| entry.inspect_params)
    {
        assert!(
            !entry.experimental_fields.is_empty(),
            "{} has inspectParams and must export field-level experimental metadata",
            entry.method
        );
        assert!(
            entry
                .experimental_fields
                .iter()
                .any(|field| field.inspect_params),
            "{} field-level markers must include params provenance",
            entry.method
        );
    }

    let turn_start = client_request(&manifest.experimental.client_requests, "turn/start");
    assert!(
        turn_start
            .experimental_fields
            .iter()
            .any(|field| field.field_path == "additional_context"
                && field.reason == "turn/start.additionalContext"
                && field.containing_type == "TurnStartParams")
    );
}

#[test]
fn additional_context_entries_are_marked_as_bounded_model_context() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    for method in ["turn/start", "turn/steer"] {
        let entry = client_request(&manifest.experimental.client_requests, method);
        assert!(
            entry
                .bounded_model_context_fields
                .iter()
                .any(|field| field.field_path == "additional_context.*.value"
                    && field.limit_profile == "additionalContextValueBytes"),
            "{method} must keep AdditionalContext model-visible size limits in the manifest"
        );
    }
}

#[test]
fn no_params_requests_keep_clean_option_unit_metadata() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    for method in ["memory/reset", "account/logout"] {
        let entry = client_request(&manifest.experimental.client_requests, method);
        assert_eq!(entry.params_type.as_deref(), Some("Option<()>"));
        assert_eq!(entry.params_schema_ref, None);
    }
}

#[test]
fn global_serialization_scopes_keep_queue_keys() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert_global_scope(
        client_request(&manifest.experimental.client_requests, "memory/reset"),
        crate::go_manifest::RequestSerializationScopeKind::Global,
        "memory",
    );
    assert_global_scope(
        client_request(&manifest.experimental.client_requests, "account/logout"),
        crate::go_manifest::RequestSerializationScopeKind::Global,
        "account-auth",
    );
    assert_global_scope(
        client_request(
            &manifest.experimental.client_requests,
            "remoteControl/status/read",
        ),
        crate::go_manifest::RequestSerializationScopeKind::GlobalSharedRead,
        "remote-control",
    );
}

#[test]
fn manifest_digests_are_populated_and_sensitive_to_metadata_changes() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let digests = &manifest.experimental.digests;

    assert_digest_set_shape(digests);
    assert_digest_set_shape(&manifest.stable.digests);
    assert_ne!(
        manifest.stable.digests.protocol_digest,
        manifest.experimental.digests.protocol_digest
    );

    let mut changed_protocol = manifest.clone();
    client_request_mut(
        &mut changed_protocol.experimental.client_requests,
        "memory/reset",
    )
    .request_serialization_scopes[0]
        .queue_key = Some("memory-changed");
    let changed_protocol_digests = crate::go_manifest::digest_set_for_manifest_mode(
        &changed_protocol,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    assert_ne!(
        digests.protocol_digest,
        changed_protocol_digests.protocol_digest
    );
    assert_ne!(
        digests.manifest_digest,
        changed_protocol_digests.manifest_digest
    );

    let mut changed_schema = manifest.clone();
    serde_shape_mut(
        &mut changed_schema.experimental.serde_shapes,
        "AppScreenshot",
    )
    .fields[0]
        .aliases
        .push("legacy_file_id");
    let changed_schema_digests = crate::go_manifest::digest_set_for_manifest_mode(
        &changed_schema,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    assert_ne!(digests.schema_digest, changed_schema_digests.schema_digest);
    assert_ne!(
        digests.manifest_digest,
        changed_schema_digests.manifest_digest
    );

    let mut changed_experimental_fields = manifest.clone();
    client_request_mut(
        &mut changed_experimental_fields.experimental.client_requests,
        "thread/start",
    )
    .experimental_fields[0]
        .reason = "thread/start.changedExperimentalField";
    let changed_experimental_digests = crate::go_manifest::digest_set_for_manifest_mode(
        &changed_experimental_fields,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    assert_ne!(
        digests.protocol_digest,
        changed_experimental_digests.protocol_digest
    );
    assert_ne!(
        digests.manifest_digest,
        changed_experimental_digests.manifest_digest
    );
}

#[test]
fn manifest_digest_projection_includes_top_level_inputs_and_excludes_derived_digests() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let digests = &manifest.experimental.digests;

    let mut changed_version = manifest.clone();
    changed_version.manifest_schema_version += 1;
    let changed_version_digests = crate::go_manifest::digest_set_for_manifest_mode(
        &changed_version,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    assert_ne!(
        digests.protocol_digest,
        changed_version_digests.protocol_digest
    );
    assert_ne!(digests.schema_digest, changed_version_digests.schema_digest);
    assert_ne!(
        digests.manifest_digest,
        changed_version_digests.manifest_digest
    );

    let mut changed_limits = manifest.clone();
    changed_limits
        .model_context_limits
        .max_additional_context_total_bytes += 1;
    let changed_limits_digests = crate::go_manifest::digest_set_for_manifest_mode(
        &changed_limits,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    assert_ne!(digests.schema_digest, changed_limits_digests.schema_digest);
    assert_ne!(
        digests.manifest_digest,
        changed_limits_digests.manifest_digest
    );

    let mut changed_derived = manifest.clone();
    changed_derived.experimental.digests.protocol_digest = "derived-change".to_string();
    changed_derived.experimental.digests.schema_digest = "derived-change".to_string();
    changed_derived.experimental.digests.manifest_digest = "derived-change".to_string();
    let changed_derived_digests = crate::go_manifest::digest_set_for_manifest_mode(
        &changed_derived,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    assert_eq!(digests, &changed_derived_digests);
}

#[test]
fn schema_digest_inputs_include_canonical_schema_definition_content() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let projection = crate::go_manifest::digest_input_projection(
        &manifest,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    let definitions = projection["schemaInputs"]["schemaDefinitions"]
        .as_object()
        .expect("schemaInputs should contain schemaDefinitions");
    let thread_start = definitions
        .get("#/definitions/v2/ThreadStartParams")
        .expect("ThreadStartParams schema definition should be present");

    assert_eq!(
        thread_start["schemaRef"],
        serde_json::json!("#/definitions/v2/ThreadStartParams")
    );
    assert_digest_shape(
        thread_start["sha256"]
            .as_str()
            .expect("schema definition should include content hash"),
    );
    assert_eq!(
        thread_start["schema"]["title"],
        serde_json::json!("ThreadStartParams")
    );
    assert!(
        thread_start["schema"]
            .get("properties")
            .and_then(|properties| properties.get("dynamicTools"))
            .is_some(),
        "schema digest input must include ThreadStartParams body content"
    );
    assert!(
        definitions.contains_key("#/definitions/v2/DynamicToolSpec"),
        "transitively reachable nested schema definitions must be digest inputs"
    );
    assert!(
        !definitions.contains_key("#/definitions/InternalSessionSource"),
        "v2 SessionSource must not resolve to the unrelated top-level SessionSource definition"
    );
}

#[test]
fn schema_only_changes_do_not_change_manifest_digest() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let original = &manifest.experimental.digests;
    let mut projection = crate::go_manifest::digest_input_projection(
        &manifest,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    projection["schemaInputs"]["schemaDefinitions"]["#/definitions/v2/ThreadStartParams"]["schema"]
        ["description"] = serde_json::json!("changed schema body");

    let changed = crate::go_manifest::digest_set_for_projection(projection);
    assert_ne!(original.protocol_digest, changed.protocol_digest);
    assert_ne!(original.schema_digest, changed.schema_digest);
    assert_eq!(original.manifest_digest, changed.manifest_digest);
}

#[test]
fn initialize_digest_snapshot_matches_manifest_digests() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let snapshot = crate::go_manifest::initialize_digest_snapshot();

    assert_eq!(
        snapshot.stable_protocol_digest,
        manifest.stable.digests.protocol_digest
    );
    assert_eq!(
        snapshot.experimental_protocol_digest,
        manifest.experimental.digests.protocol_digest
    );
    assert_eq!(
        snapshot.stable_schema_digest,
        manifest.stable.digests.schema_digest
    );
    assert_eq!(
        snapshot.experimental_schema_digest,
        manifest.experimental.digests.schema_digest
    );
    assert_eq!(
        snapshot.stable_manifest_digest,
        manifest.stable.digests.manifest_digest
    );
    assert_eq!(
        snapshot.experimental_manifest_digest,
        manifest.experimental.digests.manifest_digest
    );
}

#[test]
#[should_panic(expected = "missing from schema bundle")]
fn missing_schema_definition_refs_fail_manifest_generation() {
    let mut manifest = crate::go_manifest::go_sdk_manifest();
    let request = manifest
        .experimental
        .client_requests
        .iter_mut()
        .find(|request| request.method == "thread/start")
        .expect("thread/start should be in manifest");
    request.params_schema_ref = Some("#/definitions/DefinitelyMissingForGoSdkTest".to_string());

    let _ = crate::go_manifest::digest_input_projection(
        &manifest,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
}

#[test]
fn stable_lifecycle_rows_only_reference_stable_methods() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let stable_methods = manifest_methods(&manifest.stable);

    assert!(
        !manifest
            .stable
            .routing_lifecycle
            .iter()
            .any(|entry| entry.start_method == "thread/realtime/start"),
        "stable manifest must not expose realtime lifecycle rows"
    );

    for entry in &manifest.stable.routing_lifecycle {
        assert!(
            stable_methods.contains(entry.start_method),
            "{} lifecycle start method {} is not stable",
            entry.resource_domain,
            entry.start_method
        );
        for method in lifecycle_methods(entry) {
            assert!(
                stable_methods.contains(method),
                "{} lifecycle references non-stable method {method}",
                entry.resource_domain
            );
        }
        for method in &entry.notification_opt_out_dependencies {
            assert!(
                stable_methods.contains(*method),
                "{} lifecycle opt-out dependency {method} is not stable",
                entry.resource_domain
            );
        }
    }
}

#[test]
fn schema_reachable_definitions_have_transitive_serde_shape_rows() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let projection = crate::go_manifest::digest_input_projection(
        &manifest,
        crate::go_manifest::ProtocolModeName::Experimental,
    );
    let schema_definition_names = projection["schemaInputs"]["schemaDefinitions"]
        .as_object()
        .expect("schemaInputs should contain schemaDefinitions")
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let serde_shape_names = manifest
        .experimental
        .serde_shapes
        .iter()
        .map(|entry| entry.rust_type.clone())
        .collect::<BTreeSet<_>>();

    for expected_nested in [
        "CapabilityRootLocation",
        "DynamicToolNamespaceTool",
        "DynamicToolSpec",
        "SelectedCapabilityRoot",
    ] {
        let expected_schema_ref = format!("#/definitions/v2/{expected_nested}");
        assert!(
            schema_definition_names.contains(&expected_schema_ref),
            "{expected_nested} should be reachable through schema refs"
        );
        assert!(
            serde_shape_names.contains(expected_nested),
            "{expected_nested} should have a transitive serde shape row"
        );
    }

    for schema_name in schema_definition_names {
        let rust_type = schema_ref_rust_type_name(&schema_name);
        assert!(
            serde_shape_names.contains(rust_type),
            "reachable schema definition {schema_name} lacks a serde shape row"
        );
    }
}

fn schema_ref_rust_type_name(schema_ref: &str) -> &str {
    schema_ref.rsplit('/').next().unwrap_or(schema_ref)
}

#[test]
fn schema_reachable_serde_attribute_types_are_manifest_required() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    for rust_type in crate::go_manifest::schema_reachable_serde_attribute_required_types() {
        let Some(shape) = manifest
            .experimental
            .serde_shapes
            .iter()
            .find(|shape| shape.rust_type == *rust_type)
        else {
            continue;
        };
        assert_eq!(
            shape.metadata_status,
            crate::go_manifest::SerdeMetadataStatus::ManifestRequired,
            "{rust_type} has Rust serde attributes that JSON Schema alone cannot prove"
        );
        assert!(
            shape.schema_sufficient_proof.is_none(),
            "{rust_type} must not claim schema-sufficient proof"
        );
    }
}

#[test]
fn config_layer_disabled_reason_skip_none_is_manifest_required() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let shape = serde_shape(&manifest.experimental.serde_shapes, "ConfigLayer");
    assert_eq!(
        shape.metadata_status,
        crate::go_manifest::SerdeMetadataStatus::ManifestRequired
    );
    assert_eq!(
        shape.fields,
        vec![crate::go_manifest::SerdeFieldEntry {
            rust_field: "disabled_reason",
            wire_name: "disabledReason",
            aliases: Vec::new(),
            shape: crate::go_manifest::SerdeFieldShape {
                presence: crate::go_manifest::SerdePresence::OptionalNullable,
                default: None,
                skip_serializing_if: Some("Option::is_none"),
                flattened: false,
                custom_serialize: None,
                custom_deserialize: None,
            },
        }]
    );
}

#[test]
fn command_exec_params_default_skip_false_fields_are_complete() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let shape = serde_shape(&manifest.experimental.serde_shapes, "CommandExecParams");
    let actual = shape
        .fields
        .iter()
        .map(|field| {
            (
                field.rust_field,
                field.wire_name,
                field.shape.default.as_ref().map(|default| {
                    let provider = match default.provider {
                        crate::go_manifest::SerdeDefaultProvider::SerdeDefault => "serdeDefault",
                        crate::go_manifest::SerdeDefaultProvider::Function(name) => name,
                        crate::go_manifest::SerdeDefaultProvider::TraitDefault(name) => name,
                    };
                    (provider, default.wire_value_json)
                }),
                field.shape.skip_serializing_if,
            )
        })
        .collect::<BTreeSet<_>>();
    let expected = [
        "tty",
        "stream_stdin",
        "stream_stdout_stderr",
        "disable_output_cap",
        "disable_timeout",
    ]
    .into_iter()
    .map(|rust_field| {
        let wire_name = match rust_field {
            "stream_stdin" => "streamStdin",
            "stream_stdout_stderr" => "streamStdoutStderr",
            "disable_output_cap" => "disableOutputCap",
            "disable_timeout" => "disableTimeout",
            _ => rust_field,
        };
        (
            rust_field,
            wire_name,
            Some(("serdeDefault", "false")),
            Some("std::ops::Not::not"),
        )
    })
    .collect::<BTreeSet<_>>();

    assert_eq!(actual, expected);
}

#[test]
fn manifest_required_serde_shapes_are_not_silent_empty_rows() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let reviewed_empty_rows = BTreeSet::from([
        "ConfigRequirements",
        "ConfigRequirementsReadResponse",
        "UserInput",
    ]);

    for shape in &manifest.experimental.serde_shapes {
        if shape.metadata_status != crate::go_manifest::SerdeMetadataStatus::ManifestRequired {
            continue;
        }
        let has_metadata = !shape.fields.is_empty()
            || !shape.variant_aliases.is_empty()
            || shape.manual_payload_conversion.is_some()
            || shape.review_note.is_some();
        assert!(
            has_metadata,
            "{} is manifest-required but has no reviewed serde metadata",
            shape.rust_type
        );
        if reviewed_empty_rows.contains(shape.rust_type.as_str()) {
            assert_eq!(
                shape.review_note,
                Some("reviewed manifest-required type with no field-level serde metadata")
            );
        }
    }
}

#[test]
fn auth_token_login_experimental_marker_uses_wire_discriminator() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let login = client_request(
        &manifest.experimental.client_requests,
        "account/login/start",
    );
    let field = login
        .experimental_fields
        .iter()
        .find(|field| field.reason == "account/login/start.chatgptAuthTokens")
        .unwrap();

    assert_eq!(field.field_path, "type");
    assert_eq!(
        field.discriminator,
        Some(crate::go_manifest::ExperimentalVariantDiscriminator {
            field_path: "type",
            wire_value: "chatgptAuthTokens",
        })
    );
}

#[test]
fn serde_shapes_and_lifecycle_entries_cover_stage_one_seed_rows() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert_shape_field_aliases(
        serde_shape(&manifest.experimental.serde_shapes, "AppScreenshot"),
        "file_id",
        &["file_id"],
    );
    assert_shape_field_aliases(
        serde_shape(&manifest.experimental.serde_shapes, "AppScreenshot"),
        "user_prompt",
        &["user_prompt"],
    );
    assert_variant_alias(
        serde_shape(&manifest.experimental.serde_shapes, "FileSystemSpecialPath"),
        "ProjectRoots",
        "project_roots",
        &["current_working_directory"],
    );
    assert_shape_field_presence(
        serde_shape(&manifest.experimental.serde_shapes, "CollaborationModeMask"),
        "reasoning_effort",
        crate::go_manifest::SerdePresence::DoubleOption,
    );
    assert!(
        serde_shape(&manifest.experimental.serde_shapes, "AnalyticsConfig").fields[0]
            .shape
            .flattened
    );
    assert_eq!(
        serde_shape(&manifest.experimental.serde_shapes, "CommandExecParams").fields[0]
            .shape
            .skip_serializing_if,
        Some("std::ops::Not::not")
    );
    assert!(
        serde_shape(
            &manifest.experimental.serde_shapes,
            "ConfigValueWriteParams"
        )
        .manual_payload_conversion
        .is_some()
    );
    assert_eq!(
        serde_shape(&manifest.experimental.serde_shapes, "ProcessSpawnParams").fields[3]
            .shape
            .presence,
        crate::go_manifest::SerdePresence::DoubleOption
    );
    assert_eq!(
        serde_shape(
            &manifest.experimental.serde_shapes,
            "CommandExecutionRequestApprovalParams"
        )
        .fields[3]
            .shape
            .skip_serializing_if,
        Some("Option::is_none")
    );

    for domain in [
        "thread",
        "turn",
        "accountLogin",
        "review",
        "remoteControlPairing",
        "commandExec",
        "process",
        "fsWatch",
        "mcpOauth",
        "fuzzyFileSearch",
        "realtime",
    ] {
        assert!(
            manifest
                .experimental
                .routing_lifecycle
                .iter()
                .any(|entry| entry.resource_domain == domain),
            "{domain} lifecycle entry should be present"
        );
    }
}

#[test]
fn field_level_experimental_inventory_covers_current_client_and_server_payloads() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "thread/start"),
        &[
            (
                "allow_provider_model_fallback",
                "thread/start.allowProviderModelFallback",
                "ThreadStartParams",
                true,
            ),
            (
                "runtime_workspace_roots",
                "thread/start.runtimeWorkspaceRoots",
                "ThreadStartParams",
                true,
            ),
            ("approval_policy", "nested", "ThreadStartParams", true),
            (
                "permissions",
                "thread/start.permissions",
                "ThreadStartParams",
                true,
            ),
            (
                "multi_agent_mode",
                "thread/start.multiAgentMode",
                "ThreadStartParams",
                true,
            ),
            (
                "history_mode",
                "thread/start.historyMode",
                "ThreadStartParams",
                true,
            ),
            (
                "environments",
                "thread/start.environments",
                "ThreadStartParams",
                true,
            ),
            (
                "dynamic_tools",
                "thread/start.dynamicTools",
                "ThreadStartParams",
                true,
            ),
            (
                "selected_capability_roots",
                "thread/start.selectedCapabilityRoots",
                "ThreadStartParams",
                true,
            ),
            (
                "mock_experimental_field",
                "thread/start.mockExperimentalField",
                "ThreadStartParams",
                true,
            ),
            (
                "experimental_raw_events",
                "thread/start.experimentalRawEvents",
                "ThreadStartParams",
                true,
            ),
            (
                "runtime_workspace_roots",
                "thread/start.runtimeWorkspaceRoots",
                "ThreadStartResponse",
                false,
            ),
            ("approval_policy", "nested", "ThreadStartResponse", false),
            (
                "active_permission_profile",
                "thread/start.activePermissionProfile",
                "ThreadStartResponse",
                false,
            ),
            (
                "multi_agent_mode",
                "thread/start.multiAgentMode",
                "ThreadStartResponse",
                false,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "thread/resume"),
        &[
            (
                "history",
                "thread/resume.history",
                "ThreadResumeParams",
                true,
            ),
            ("path", "thread/resume.path", "ThreadResumeParams", true),
            (
                "runtime_workspace_roots",
                "thread/resume.runtimeWorkspaceRoots",
                "ThreadResumeParams",
                true,
            ),
            ("approval_policy", "nested", "ThreadResumeParams", true),
            (
                "permissions",
                "thread/resume.permissions",
                "ThreadResumeParams",
                true,
            ),
            (
                "exclude_turns",
                "thread/resume.excludeTurns",
                "ThreadResumeParams",
                true,
            ),
            (
                "initial_turns_page",
                "thread/resume.initialTurnsPage",
                "ThreadResumeParams",
                true,
            ),
            (
                "runtime_workspace_roots",
                "thread/resume.runtimeWorkspaceRoots",
                "ThreadResumeResponse",
                false,
            ),
            ("approval_policy", "nested", "ThreadResumeResponse", false),
            (
                "active_permission_profile",
                "thread/resume.activePermissionProfile",
                "ThreadResumeResponse",
                false,
            ),
            (
                "multi_agent_mode",
                "thread/resume.multiAgentMode",
                "ThreadResumeResponse",
                false,
            ),
            (
                "initial_turns_page",
                "thread/resume.initialTurnsPage",
                "ThreadResumeResponse",
                false,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "thread/fork"),
        &[
            ("path", "thread/fork.path", "ThreadForkParams", true),
            (
                "runtime_workspace_roots",
                "thread/fork.runtimeWorkspaceRoots",
                "ThreadForkParams",
                true,
            ),
            ("approval_policy", "nested", "ThreadForkParams", true),
            (
                "permissions",
                "thread/fork.permissions",
                "ThreadForkParams",
                true,
            ),
            (
                "exclude_turns",
                "thread/fork.excludeTurns",
                "ThreadForkParams",
                true,
            ),
            (
                "runtime_workspace_roots",
                "thread/fork.runtimeWorkspaceRoots",
                "ThreadForkResponse",
                false,
            ),
            ("approval_policy", "nested", "ThreadForkResponse", false),
            (
                "active_permission_profile",
                "thread/fork.activePermissionProfile",
                "ThreadForkResponse",
                false,
            ),
            (
                "multi_agent_mode",
                "thread/fork.multiAgentMode",
                "ThreadForkResponse",
                false,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(
            &manifest.experimental.client_requests,
            "thread/settings/update",
        ),
        &[
            (
                "approval_policy",
                "nested",
                "ThreadSettingsUpdateParams",
                true,
            ),
            (
                "permissions",
                "thread/settings/update.permissions",
                "ThreadSettingsUpdateParams",
                true,
            ),
            (
                "collaboration_mode",
                "thread/settings/update.collaborationMode",
                "ThreadSettingsUpdateParams",
                true,
            ),
            (
                "multi_agent_mode",
                "thread/settings/update.multiAgentMode",
                "ThreadSettingsUpdateParams",
                true,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "thread/list"),
        &[
            (
                "parent_thread_id",
                "thread/list.parentThreadId",
                "ThreadListParams",
                true,
            ),
            (
                "ancestor_thread_id",
                "thread/list.ancestorThreadId",
                "ThreadListParams",
                true,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "turn/start"),
        &[
            (
                "responsesapi_client_metadata",
                "turn/start.responsesapiClientMetadata",
                "TurnStartParams",
                true,
            ),
            (
                "additional_context",
                "turn/start.additionalContext",
                "TurnStartParams",
                true,
            ),
            (
                "environments",
                "turn/start.environments",
                "TurnStartParams",
                true,
            ),
            (
                "runtime_workspace_roots",
                "turn/start.runtimeWorkspaceRoots",
                "TurnStartParams",
                true,
            ),
            ("approval_policy", "nested", "TurnStartParams", true),
            (
                "permissions",
                "turn/start.permissions",
                "TurnStartParams",
                true,
            ),
            (
                "collaboration_mode",
                "turn/start.collaborationMode",
                "TurnStartParams",
                true,
            ),
            (
                "multi_agent_mode",
                "turn/start.multiAgentMode",
                "TurnStartParams",
                true,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "turn/steer"),
        &[
            (
                "responsesapi_client_metadata",
                "turn/steer.responsesapiClientMetadata",
                "TurnSteerParams",
                true,
            ),
            (
                "additional_context",
                "turn/steer.additionalContext",
                "TurnSteerParams",
                true,
            ),
        ],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "command/exec"),
        &[(
            "permission_profile",
            "command/exec.permissionProfile",
            "CommandExecParams",
            true,
        )],
    );

    assert_experimental_fields_exact(
        client_request(&manifest.experimental.client_requests, "config/read"),
        &[
            ("config", "nested", "ConfigReadResponse", false),
            ("config.approval_policy", "nested", "Config", false),
            (
                "config.approvals_reviewer",
                "config/read.approvalsReviewer",
                "Config",
                false,
            ),
            ("config.apps", "config/read.apps", "Config", false),
        ],
    );

    assert_experimental_fields_exact(
        client_request(
            &manifest.experimental.client_requests,
            "configRequirements/read",
        ),
        &[
            (
                "requirements",
                "nested",
                "ConfigRequirementsReadResponse",
                false,
            ),
            (
                "requirements.allowed_approval_policies",
                "nested",
                "ConfigRequirements",
                false,
            ),
            (
                "requirements.allowed_approvals_reviewers",
                "configRequirements/read.allowedApprovalsReviewers",
                "ConfigRequirements",
                false,
            ),
            (
                "requirements.hooks",
                "configRequirements/read.hooks",
                "ConfigRequirements",
                false,
            ),
            (
                "requirements.network",
                "configRequirements/read.network",
                "ConfigRequirements",
                false,
            ),
        ],
    );

    let login = client_request(
        &manifest.experimental.client_requests,
        "account/login/start",
    );
    assert_experimental_fields_exact(
        login,
        &[(
            "type",
            "account/login/start.chatgptAuthTokens",
            "LoginAccountParams",
            true,
        )],
    );

    let approval = server_request(
        &manifest.experimental.server_requests,
        "item/commandExecution/requestApproval",
    );
    assert_experimental_fields_exact(
        approval,
        &[
            (
                "additional_permissions",
                "item/commandExecution/requestApproval.additionalPermissions",
                "CommandExecutionRequestApprovalParams",
                true,
            ),
            (
                "available_decisions",
                "item/commandExecution/requestApproval.availableDecisions",
                "CommandExecutionRequestApprovalParams",
                true,
            ),
        ],
    );
}

#[test]
fn manifest_required_payloads_have_matching_entry_requirements() {
    let manifest = crate::go_manifest::go_sdk_manifest();

    for entry in manifest
        .experimental
        .client_requests
        .iter()
        .chain(manifest.experimental.server_requests.iter())
    {
        if entry.schema_excluded_reason.is_some() {
            assert_eq!(entry.params_schema_ref, None);
            assert_eq!(entry.response_schema_ref, None);
            continue;
        }
        if let Some(params_type) = entry.params_type.as_deref() {
            assert_serde_shape_row_is_complete(&manifest.experimental.serde_shapes, params_type);
            assert_manifest_required_entry_shape_matches(
                &manifest.experimental.serde_shapes,
                params_type,
                entry.serde_shape_requirement,
                entry.method,
            );
        }

        if let Some(response_type) = entry.response_type.as_deref() {
            assert_serde_shape_row_is_complete(&manifest.experimental.serde_shapes, response_type);
            assert_manifest_shape_is_present_when_required(
                &manifest.experimental.serde_shapes,
                response_type,
                entry.method,
            );
        }
    }

    for entry in manifest
        .experimental
        .server_notifications
        .iter()
        .chain(manifest.experimental.client_notifications.iter())
    {
        if let Some(payload_type) = entry.payload_type.as_deref() {
            assert_serde_shape_row_is_complete(&manifest.experimental.serde_shapes, payload_type);
            assert_manifest_required_entry_shape_matches(
                &manifest.experimental.serde_shapes,
                payload_type,
                entry.serde_shape_requirement,
                entry.method,
            );
        }
    }
}

#[test]
fn runtime_serialization_scopes_match_manifest_representatives() {
    assert_runtime_none_matches_manifest(
        "initialize",
        ClientRequest::Initialize {
            request_id: request_id(),
            params: v1::InitializeParams {
                client_info: v1::ClientInfo {
                    name: "test".to_string(),
                    title: None,
                    version: "0.1.0".to_string(),
                },
                capabilities: None,
            },
        },
    );

    assert_runtime_scope_matches_manifest(
        "memory/reset",
        ClientRequest::MemoryReset {
            request_id: request_id(),
            params: None,
        },
        ClientRequestSerializationScope::Global("memory"),
        crate::go_manifest::RequestSerializationScopeKind::Global,
        Some("memory"),
        &[],
    );

    assert_runtime_scope_matches_manifest(
        "remoteControl/status/read",
        ClientRequest::RemoteControlStatusRead {
            request_id: request_id(),
            params: None,
        },
        ClientRequestSerializationScope::GlobalSharedRead("remote-control"),
        crate::go_manifest::RequestSerializationScopeKind::GlobalSharedRead,
        Some("remote-control"),
        &[],
    );

    assert_runtime_scope_matches_manifest(
        "thread/resume",
        ClientRequest::ThreadResume {
            request_id: request_id(),
            params: v2::ThreadResumeParams {
                thread_id: "thread-1".to_string(),
                ..Default::default()
            },
        },
        ClientRequestSerializationScope::Thread {
            thread_id: "thread-1".to_string(),
        },
        crate::go_manifest::RequestSerializationScopeKind::Thread,
        None,
        &["thread_id"],
    );

    assert_runtime_scope_matches_manifest(
        "thread/resume",
        ClientRequest::ThreadResume {
            request_id: request_id(),
            params: v2::ThreadResumeParams {
                thread_id: String::new(),
                path: Some(PathBuf::from("/tmp/resume-thread.jsonl")),
                ..Default::default()
            },
        },
        ClientRequestSerializationScope::ThreadPath {
            path: PathBuf::from("/tmp/resume-thread.jsonl"),
        },
        crate::go_manifest::RequestSerializationScopeKind::ThreadPath,
        None,
        &["path"],
    );

    assert_runtime_scope_matches_manifest(
        "command/exec",
        ClientRequest::OneOffCommandExec {
            request_id: request_id(),
            params: command_exec_params(Some("proc-1")),
        },
        ClientRequestSerializationScope::CommandExecProcess {
            process_id: "proc-1".to_string(),
        },
        crate::go_manifest::RequestSerializationScopeKind::CommandExecProcess,
        None,
        &["process_id?"],
    );

    assert_runtime_scope_matches_manifest(
        "process/spawn",
        ClientRequest::ProcessSpawn {
            request_id: request_id(),
            params: v2::ProcessSpawnParams {
                command: vec!["true".to_string()],
                process_handle: "process-1".to_string(),
                cwd: absolute_path("/tmp/repo"),
                tty: false,
                stream_stdin: false,
                stream_stdout_stderr: false,
                output_bytes_cap: None,
                timeout_ms: None,
                env: None,
                size: None,
            },
        },
        ClientRequestSerializationScope::Process {
            process_handle: "process-1".to_string(),
        },
        crate::go_manifest::RequestSerializationScopeKind::Process,
        None,
        &["process_handle"],
    );

    assert_runtime_scope_matches_manifest(
        "fuzzyFileSearch/sessionUpdate",
        ClientRequest::FuzzyFileSearchSessionUpdate {
            request_id: request_id(),
            params: FuzzyFileSearchSessionUpdateParams {
                session_id: "search-1".to_string(),
                query: "lib".to_string(),
            },
        },
        ClientRequestSerializationScope::FuzzyFileSearchSession {
            session_id: "search-1".to_string(),
        },
        crate::go_manifest::RequestSerializationScopeKind::FuzzyFileSearchSession,
        None,
        &["session_id"],
    );

    assert_runtime_scope_matches_manifest(
        "fs/watch",
        ClientRequest::FsWatch {
            request_id: request_id(),
            params: v2::FsWatchParams {
                watch_id: "watch-1".to_string(),
                path: absolute_path("/tmp/repo"),
            },
        },
        ClientRequestSerializationScope::FsWatch {
            watch_id: "watch-1".to_string(),
        },
        crate::go_manifest::RequestSerializationScopeKind::FsWatch,
        None,
        &["watch_id"],
    );

    assert_runtime_scope_matches_manifest(
        "mcpServer/oauth/login",
        ClientRequest::McpServerOauthLogin {
            request_id: request_id(),
            params: v2::McpServerOauthLoginParams {
                name: "server-1".to_string(),
                thread_id: None,
                scopes: None,
                timeout_secs: None,
            },
        },
        ClientRequestSerializationScope::McpOauth {
            server_name: "server-1".to_string(),
        },
        crate::go_manifest::RequestSerializationScopeKind::McpOauth,
        None,
        &["name"],
    );
}

#[test]
fn server_notification_routing_matches_reviewed_seed_table() {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let expected = expected_server_notification_routes();

    assert_eq!(
        manifest.experimental.server_notifications.len(),
        expected.len(),
        "routing seed must enumerate every current ServerNotification"
    );

    for entry in &manifest.experimental.server_notifications {
        let expected_routing = expected
            .iter()
            .find(|(method, _)| method == &entry.method)
            .unwrap_or_else(|| panic!("{} must be in the reviewed routing seed", entry.method));
        assert_expected_routing(entry, &expected_routing.1);
    }
}

fn initialize_entry<'a>(
    entries: &'a [crate::go_manifest::RequestManifestEntry],
    mode_name: &str,
) -> &'a crate::go_manifest::RequestManifestEntry {
    entries
        .iter()
        .find(|entry| entry.method == "initialize")
        .unwrap_or_else(|| panic!("{mode_name} manifest should include initialize"))
}

fn client_request<'a>(
    entries: &'a [crate::go_manifest::RequestManifestEntry],
    method: &str,
) -> &'a crate::go_manifest::RequestManifestEntry {
    entries
        .iter()
        .find(|entry| entry.method == method)
        .unwrap_or_else(|| panic!("{method} client request should be in the Go SDK manifest"))
}

fn client_request_mut<'a>(
    entries: &'a mut [crate::go_manifest::RequestManifestEntry],
    method: &str,
) -> &'a mut crate::go_manifest::RequestManifestEntry {
    entries
        .iter_mut()
        .find(|entry| entry.method == method)
        .unwrap_or_else(|| panic!("{method} client request should be in the Go SDK manifest"))
}

fn server_request<'a>(
    entries: &'a [crate::go_manifest::RequestManifestEntry],
    method: &str,
) -> &'a crate::go_manifest::RequestManifestEntry {
    entries
        .iter()
        .find(|entry| entry.method == method)
        .unwrap_or_else(|| panic!("{method} server request should be in the Go SDK manifest"))
}

fn notification<'a>(
    entries: &'a [crate::go_manifest::NotificationManifestEntry],
    method: &str,
) -> &'a crate::go_manifest::NotificationManifestEntry {
    entries
        .iter()
        .find(|entry| entry.method == method)
        .unwrap_or_else(|| panic!("{method} notification should be in the Go SDK manifest"))
}

fn assert_routed_extractors(
    entry: &crate::go_manifest::NotificationManifestEntry,
    expected_paths: &[&str],
) {
    match &entry.routing_strategy {
        crate::go_manifest::NotificationRoutingStrategy::Routed { routes } => {
            assert_eq!(
                extractor_paths(&routes[0].identity_extractors),
                expected_paths,
                "{} route extractors should match the reviewed seed",
                entry.method
            );
        }
        other => panic!("{} should be routed, got {other:?}", entry.method),
    }
}

fn extractor_paths(extractors: &[crate::go_manifest::IdentityExtractor]) -> Vec<String> {
    extractors
        .iter()
        .map(|extractor| {
            if extractor.optional {
                format!("{}?", extractor.field_path)
            } else {
                extractor.field_path.to_string()
            }
        })
        .collect()
}

fn assert_global_scope(
    entry: &crate::go_manifest::RequestManifestEntry,
    kind: crate::go_manifest::RequestSerializationScopeKind,
    queue_key: &str,
) {
    assert_eq!(entry.request_serialization_scopes.len(), 1);
    let scope = &entry.request_serialization_scopes[0];
    assert_eq!(scope.kind, kind);
    assert_eq!(scope.queue_key, Some(queue_key));
}

fn assert_digest_set_shape(digests: &crate::go_manifest::DigestSet) {
    assert_digest_shape(&digests.protocol_digest);
    assert_digest_shape(&digests.schema_digest);
    assert_digest_shape(&digests.manifest_digest);
}

fn assert_digest_shape(digest: &str) {
    assert_eq!(digest.len(), 64);
    assert!(
        digest
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
        "{digest} must be lowercase SHA-256 hex"
    );
}

fn assert_experimental_fields_exact(
    entry: &crate::go_manifest::RequestManifestEntry,
    expected: &[(&'static str, &'static str, &'static str, bool)],
) {
    let mut actual = entry
        .experimental_fields
        .iter()
        .map(|field| {
            (
                field.field_path,
                field.reason,
                field.containing_type,
                field.inspect_params,
            )
        })
        .collect::<Vec<_>>();
    actual.sort();

    let mut expected = expected.to_vec();
    expected.sort();

    assert_eq!(
        actual, expected,
        "{} field-level experimental inventory drifted",
        entry.method
    );
}

fn assert_serde_shape_row_is_complete(
    shapes: &[crate::go_manifest::SerdeShapeEntry],
    rust_type: &str,
) {
    if rust_type == "Option<()>" {
        return;
    }

    let shape = serde_shape(shapes, rust_type);
    if shape.metadata_status == crate::go_manifest::SerdeMetadataStatus::SchemaSufficient {
        let proof = shape
            .schema_sufficient_proof
            .as_ref()
            .unwrap_or_else(|| panic!("{rust_type} schema-sufficient shape needs proof"));
        assert!(proof.checked_required_fields);
        assert!(proof.checked_nullable_fields);
        assert!(proof.checked_additional_properties);
        assert!(proof.checked_enum_values);
        assert!(proof.checked_union_tags);
        assert!(proof.no_serde_aliases);
        assert!(proof.no_serde_defaults);
        assert!(proof.no_skip_serializing_if);
        assert!(proof.no_flatten);
        assert!(proof.no_custom_serde);
        assert!(!proof.source_anchor.is_empty());
    }
}

fn assert_manifest_required_entry_shape_matches(
    shapes: &[crate::go_manifest::SerdeShapeEntry],
    rust_type: &str,
    requirement: crate::go_manifest::SerdeShapeRequirement,
    method: &str,
) {
    let Some(shape) = shapes.iter().find(|shape| shape.rust_type == rust_type) else {
        return;
    };

    if shape.metadata_status == crate::go_manifest::SerdeMetadataStatus::ManifestRequired {
        assert!(
            matches!(
                requirement,
                crate::go_manifest::SerdeShapeRequirement::ManifestRequired
                    | crate::go_manifest::SerdeShapeRequirement::ManualPayloadConversion
            ),
            "{method} references manifest-required {rust_type} but marks it {requirement:?}"
        );
    }
}

fn assert_manifest_shape_is_present_when_required(
    shapes: &[crate::go_manifest::SerdeShapeEntry],
    rust_type: &str,
    method: &str,
) {
    if is_known_manifest_required_type(rust_type) {
        assert!(
            shapes.iter().any(|shape| shape.rust_type == rust_type),
            "{method} references manifest-required {rust_type} without a serde shape row"
        );
    }
}

fn is_known_manifest_required_type(rust_type: &str) -> bool {
    matches!(
        rust_type,
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
            | "CommandExecutionRequestApprovalParams"
    )
}

fn assert_runtime_none_matches_manifest(method: &str, request: ClientRequest) {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let entry = client_request(&manifest.experimental.client_requests, method);

    assert_eq!(
        request.serialization_scope(),
        None,
        "{method} runtime scope"
    );
    assert!(
        entry.request_serialization_scopes.is_empty(),
        "{method} manifest should not declare serialization scopes"
    );
}

fn assert_runtime_scope_matches_manifest(
    method: &str,
    request: ClientRequest,
    expected_runtime_scope: ClientRequestSerializationScope,
    expected_manifest_kind: crate::go_manifest::RequestSerializationScopeKind,
    expected_queue_key: Option<&'static str>,
    expected_paths: &[&str],
) {
    let manifest = crate::go_manifest::go_sdk_manifest();
    let entry = client_request(&manifest.experimental.client_requests, method);
    let expected_paths = expected_paths
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();

    assert_eq!(
        request.serialization_scope(),
        Some(expected_runtime_scope),
        "{method} runtime scope"
    );
    assert!(
        entry.request_serialization_scopes.iter().any(|scope| {
            scope.kind == expected_manifest_kind
                && scope.queue_key == expected_queue_key
                && extractor_paths(&scope.identity_extractors) == expected_paths
        }),
        "{method} manifest scopes should include the runtime helper branch"
    );
}

fn manifest_methods(manifest: &crate::go_manifest::ProtocolModeManifest) -> BTreeSet<&'static str> {
    manifest
        .client_requests
        .iter()
        .map(|entry| entry.method)
        .chain(manifest.server_requests.iter().map(|entry| entry.method))
        .chain(
            manifest
                .server_notifications
                .iter()
                .map(|entry| entry.method),
        )
        .chain(
            manifest
                .client_notifications
                .iter()
                .map(|entry| entry.method),
        )
        .collect()
}

fn lifecycle_methods(entry: &crate::go_manifest::RoutingLifecycleEntry) -> BTreeSet<&'static str> {
    let mut methods = BTreeSet::new();
    match entry.start_completion {
        crate::go_manifest::WireCompletion::JsonRpcResponse { method }
        | crate::go_manifest::WireCompletion::TerminalNotification { method, .. }
        | crate::go_manifest::WireCompletion::ExplicitMethodResponse { method } => {
            methods.insert(method);
        }
    }
    for trigger in &entry.cleanup_triggers {
        match trigger {
            crate::go_manifest::CleanupTrigger::JsonRpcResponse { method }
            | crate::go_manifest::CleanupTrigger::TerminalNotification { method, .. }
            | crate::go_manifest::CleanupTrigger::ExplicitMethodResponse { method } => {
                methods.insert(*method);
            }
        }
    }
    methods
}

fn request_id() -> crate::RequestId {
    crate::RequestId::Integer(1)
}

fn absolute_path(path: &str) -> AbsolutePathBuf {
    let path = format!("/{}", path.trim_start_matches('/'));
    test_path_buf(&path).abs()
}

fn command_exec_params(process_id: Option<&str>) -> v2::CommandExecParams {
    v2::CommandExecParams {
        command: vec!["true".to_string()],
        process_id: process_id.map(str::to_string),
        tty: false,
        stream_stdin: false,
        stream_stdout_stderr: false,
        output_bytes_cap: None,
        disable_output_cap: false,
        disable_timeout: false,
        timeout_ms: None,
        cwd: None,
        env: None,
        size: None,
        sandbox_policy: None,
        permission_profile: None,
    }
}

fn serde_shape<'a>(
    entries: &'a [crate::go_manifest::SerdeShapeEntry],
    rust_type: &str,
) -> &'a crate::go_manifest::SerdeShapeEntry {
    entries
        .iter()
        .find(|entry| entry.rust_type == rust_type)
        .unwrap_or_else(|| panic!("{rust_type} serde shape should be present"))
}

fn serde_shape_mut<'a>(
    entries: &'a mut [crate::go_manifest::SerdeShapeEntry],
    rust_type: &str,
) -> &'a mut crate::go_manifest::SerdeShapeEntry {
    entries
        .iter_mut()
        .find(|entry| entry.rust_type == rust_type)
        .unwrap_or_else(|| panic!("{rust_type} serde shape should be present"))
}

fn assert_shape_field_aliases(
    shape: &crate::go_manifest::SerdeShapeEntry,
    rust_field: &str,
    aliases: &[&str],
) {
    let field = shape
        .fields
        .iter()
        .find(|field| field.rust_field == rust_field)
        .unwrap_or_else(|| panic!("{rust_field} field should be present"));
    assert_eq!(field.aliases, aliases);
}

fn assert_shape_field_presence(
    shape: &crate::go_manifest::SerdeShapeEntry,
    rust_field: &str,
    presence: crate::go_manifest::SerdePresence,
) {
    let field = shape
        .fields
        .iter()
        .find(|field| field.rust_field == rust_field)
        .unwrap_or_else(|| panic!("{rust_field} field should be present"));
    assert_eq!(field.shape.presence, presence);
}

fn assert_variant_alias(
    shape: &crate::go_manifest::SerdeShapeEntry,
    rust_variant: &str,
    canonical_wire_value: &str,
    aliases: &[&str],
) {
    let alias = shape
        .variant_aliases
        .iter()
        .find(|alias| alias.rust_variant == rust_variant)
        .unwrap_or_else(|| panic!("{rust_variant} variant alias should be present"));
    assert_eq!(alias.canonical_wire_value, canonical_wire_value);
    assert_eq!(alias.aliases, aliases);
}

enum ExpectedRouting {
    Routed(&'static [&'static str]),
    RoutedWithGlobalFallback(&'static [&'static str]),
    Global,
}

fn expected_server_notification_routes() -> Vec<(&'static str, ExpectedRouting)> {
    vec![
        ("error", ExpectedRouting::Routed(&["threadId", "turnId"])),
        ("thread/started", ExpectedRouting::Routed(&["thread.id"])),
        (
            "thread/status/changed",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        ("thread/archived", ExpectedRouting::Routed(&["threadId"])),
        ("thread/deleted", ExpectedRouting::Routed(&["threadId"])),
        ("thread/unarchived", ExpectedRouting::Routed(&["threadId"])),
        ("thread/closed", ExpectedRouting::Routed(&["threadId"])),
        ("skills/changed", ExpectedRouting::Global),
        (
            "thread/name/updated",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/goal/updated",
            ExpectedRouting::Routed(&["threadId", "turnId?"]),
        ),
        (
            "thread/goal/cleared",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/settings/updated",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/tokenUsage/updated",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "turn/started",
            ExpectedRouting::Routed(&["threadId", "turn.id"]),
        ),
        (
            "hook/started",
            ExpectedRouting::Routed(&["threadId", "turnId", "run.id"]),
        ),
        (
            "turn/completed",
            ExpectedRouting::Routed(&["threadId", "turn.id"]),
        ),
        (
            "hook/completed",
            ExpectedRouting::Routed(&["threadId", "turnId", "run.id"]),
        ),
        (
            "turn/diff/updated",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "turn/plan/updated",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "item/started",
            ExpectedRouting::Routed(&["threadId", "turnId", "item.id"]),
        ),
        (
            "item/autoApprovalReview/started",
            ExpectedRouting::Routed(&["threadId", "turnId", "reviewId", "targetItemId?"]),
        ),
        (
            "item/autoApprovalReview/completed",
            ExpectedRouting::Routed(&["threadId", "turnId", "reviewId", "targetItemId?"]),
        ),
        (
            "item/completed",
            ExpectedRouting::Routed(&["threadId", "turnId", "item.id"]),
        ),
        (
            "rawResponseItem/completed",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "item/agentMessage/delta",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId"]),
        ),
        (
            "item/plan/delta",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId"]),
        ),
        (
            "command/exec/outputDelta",
            ExpectedRouting::Routed(&["processId"]),
        ),
        (
            "process/outputDelta",
            ExpectedRouting::Routed(&["processHandle"]),
        ),
        (
            "process/exited",
            ExpectedRouting::Routed(&["processHandle"]),
        ),
        (
            "item/commandExecution/outputDelta",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId"]),
        ),
        (
            "item/commandExecution/terminalInteraction",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId", "processId"]),
        ),
        (
            "item/fileChange/outputDelta",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId"]),
        ),
        (
            "item/fileChange/patchUpdated",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId"]),
        ),
        (
            "serverRequest/resolved",
            ExpectedRouting::Routed(&["threadId", "requestId"]),
        ),
        (
            "item/mcpToolCall/progress",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId"]),
        ),
        (
            "mcpServer/oauthLogin/completed",
            ExpectedRouting::Routed(&["name", "threadId?"]),
        ),
        (
            "mcpServer/startupStatus/updated",
            ExpectedRouting::Routed(&["name", "threadId?"]),
        ),
        ("account/updated", ExpectedRouting::Global),
        ("account/rateLimits/updated", ExpectedRouting::Global),
        ("app/list/updated", ExpectedRouting::Global),
        (
            "remoteControl/status/changed",
            ExpectedRouting::Routed(&["environmentId?", "installationId", "serverName"]),
        ),
        (
            "externalAgentConfig/import/progress",
            ExpectedRouting::Routed(&["importId"]),
        ),
        (
            "externalAgentConfig/import/completed",
            ExpectedRouting::Routed(&["importId"]),
        ),
        ("fs/changed", ExpectedRouting::Routed(&["watchId"])),
        (
            "item/reasoning/summaryTextDelta",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId", "summaryIndex"]),
        ),
        (
            "item/reasoning/summaryPartAdded",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId", "summaryIndex"]),
        ),
        (
            "item/reasoning/textDelta",
            ExpectedRouting::Routed(&["threadId", "turnId", "itemId", "contentIndex"]),
        ),
        (
            "thread/compacted",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "model/rerouted",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "model/verification",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "turn/moderationMetadata",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "model/safetyBuffering/updated",
            ExpectedRouting::Routed(&["threadId", "turnId"]),
        ),
        (
            "warning",
            ExpectedRouting::RoutedWithGlobalFallback(&["threadId?"]),
        ),
        ("guardianWarning", ExpectedRouting::Routed(&["threadId"])),
        ("deprecationNotice", ExpectedRouting::Global),
        ("configWarning", ExpectedRouting::Global),
        (
            "fuzzyFileSearch/sessionUpdated",
            ExpectedRouting::Routed(&["sessionId"]),
        ),
        (
            "fuzzyFileSearch/sessionCompleted",
            ExpectedRouting::Routed(&["sessionId"]),
        ),
        (
            "thread/realtime/started",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/realtime/itemAdded",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/realtime/transcript/delta",
            ExpectedRouting::Routed(&["threadId", "role"]),
        ),
        (
            "thread/realtime/transcript/done",
            ExpectedRouting::Routed(&["threadId", "role"]),
        ),
        (
            "thread/realtime/outputAudio/delta",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/realtime/sdp",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/realtime/error",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        (
            "thread/realtime/closed",
            ExpectedRouting::Routed(&["threadId"]),
        ),
        ("windows/worldWritableWarning", ExpectedRouting::Global),
        ("windowsSandbox/setupCompleted", ExpectedRouting::Global),
        (
            "account/login/completed",
            ExpectedRouting::RoutedWithGlobalFallback(&["loginId?"]),
        ),
    ]
}

fn assert_expected_routing(
    entry: &crate::go_manifest::NotificationManifestEntry,
    expected: &ExpectedRouting,
) {
    match expected {
        ExpectedRouting::Routed(expected_paths) => {
            assert_routed_extractors(entry, expected_paths);
        }
        ExpectedRouting::RoutedWithGlobalFallback(expected_paths) => {
            match &entry.routing_strategy {
                crate::go_manifest::NotificationRoutingStrategy::RoutedWithGlobalFallback {
                    routes,
                    missing_identity_reason,
                } => {
                    assert!(!missing_identity_reason.is_empty());
                    assert_eq!(
                        extractor_paths(&routes[0].identity_extractors),
                        *expected_paths
                    );
                }
                other => panic!(
                    "{} should route with global fallback, got {other:?}",
                    entry.method
                ),
            }
        }
        ExpectedRouting::Global => {
            assert!(
                matches!(
                    entry.routing_strategy,
                    crate::go_manifest::NotificationRoutingStrategy::GlobalOnly { .. }
                ),
                "{} should be global-only",
                entry.method
            );
        }
    }
}
