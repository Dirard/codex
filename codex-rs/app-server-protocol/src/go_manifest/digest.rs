use anyhow::Context;
use serde_json::Map;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::*;

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

pub(super) fn empty_digest_set() -> DigestSet {
    DigestSet {
        protocol_digest: String::new(),
        schema_digest: String::new(),
        manifest_digest: String::new(),
    }
}

pub(crate) fn digest_set_for_manifest_mode(
    manifest: &GoSdkManifest,
    protocol_mode: ProtocolModeName,
) -> DigestSet {
    let projection = digest_input_projection(manifest, protocol_mode);
    digest_set_for_projection(projection)
}

pub(crate) fn digest_set_for_projection(projection: Value) -> DigestSet {
    let protocol_digest = digest_value(serde_json::json!({
        "manifestSchemaVersion": projection["manifestSchemaVersion"].clone(),
        "protocolMode": projection["protocolMode"].clone(),
        "schemaInputs": projection["schemaInputs"].clone(),
        "manifestInputs": projection["manifestInputs"].clone(),
        "requestSerializationInputs": projection["requestSerializationInputs"].clone(),
        "serdeShapeInputs": projection["serdeShapeInputs"].clone(),
        "visibilityInputs": projection["visibilityInputs"].clone(),
        "routingLifecycleInputs": projection["routingLifecycleInputs"].clone(),
        "experimentalFilterInputs": projection["experimentalFilterInputs"].clone(),
    }));
    let schema_digest = digest_value(serde_json::json!({
        "manifestSchemaVersion": projection["manifestSchemaVersion"].clone(),
        "protocolMode": projection["protocolMode"].clone(),
        "schemaInputs": projection["schemaInputs"].clone(),
        "serdeShapeInputs": projection["serdeShapeInputs"].clone(),
    }));
    let manifest_digest = digest_value(serde_json::json!({
        "manifestSchemaVersion": projection["manifestSchemaVersion"].clone(),
        "protocolMode": projection["protocolMode"].clone(),
        "manifestInputs": projection["manifestInputs"].clone(),
    }));

    DigestSet {
        protocol_digest,
        schema_digest,
        manifest_digest,
    }
}

pub(crate) fn digest_input_projection(
    manifest: &GoSdkManifest,
    protocol_mode: ProtocolModeName,
) -> Value {
    let mode = protocol_mode_manifest(manifest, protocol_mode);
    serde_json::json!({
        "manifestSchemaVersion": manifest.manifest_schema_version,
        "protocolMode": mode.protocol_mode,
        "schemaInputs": schema_digest_inputs(manifest, mode),
        "manifestInputs": manifest_digest_inputs(manifest, mode),
        "requestSerializationInputs": request_serialization_digest_inputs(mode),
        "serdeShapeInputs": mode.serde_shapes,
        "visibilityInputs": visibility_digest_inputs(mode),
        "routingLifecycleInputs": routing_lifecycle_digest_inputs(mode),
        "experimentalFilterInputs": experimental_filter_digest_inputs(mode),
    })
}

fn protocol_mode_manifest(
    manifest: &GoSdkManifest,
    protocol_mode: ProtocolModeName,
) -> &ProtocolModeManifest {
    match protocol_mode {
        ProtocolModeName::Stable => &manifest.stable,
        ProtocolModeName::Experimental => &manifest.experimental,
    }
}
fn schema_digest_inputs(manifest: &GoSdkManifest, mode: &ProtocolModeManifest) -> Value {
    serde_json::json!({
        "modelContextLimits": manifest.model_context_limits,
        "schemaDefinitions": schema_definition_digest_inputs(mode),
        "clientRequests": mode.client_requests.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "paramsSchemaRef": entry.params_schema_ref,
            "responseSchemaRef": entry.response_schema_ref,
            "boundedModelContextFields": entry.bounded_model_context_fields,
        })).collect::<Vec<_>>(),
        "serverRequests": mode.server_requests.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "paramsSchemaRef": entry.params_schema_ref,
            "responseSchemaRef": entry.response_schema_ref,
        })).collect::<Vec<_>>(),
        "serverNotifications": mode.server_notifications.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "payloadSchemaRef": entry.payload_schema_ref,
            "experimentalFields": entry.experimental_fields,
        })).collect::<Vec<_>>(),
        "clientNotifications": mode.client_notifications.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "payloadSchemaRef": entry.payload_schema_ref,
        })).collect::<Vec<_>>(),
    })
}

fn schema_definition_digest_inputs(mode: &ProtocolModeManifest) -> Value {
    let definitions = schema_definitions(mode.protocol_mode);
    let mut schema_definitions = Map::new();
    for key in reachable_schema_definition_keys(mode) {
        let schema_ref = schema_ref_for_definition_key(&key);
        let Some(schema) = definitions.get(&key) else {
            panic!("Go SDK manifest schema ref {schema_ref} is missing from schema bundle");
        };
        let canonical_schema = crate::schema_fixtures::canonicalize_schema_json(schema);
        schema_definitions.insert(
            schema_ref.clone(),
            serde_json::json!({
                "schemaRef": schema_ref,
                "sha256": digest_value(canonical_schema.clone()),
                "schema": canonical_schema,
            }),
        );
    }
    Value::Object(schema_definitions)
}

pub(super) fn reachable_schema_rust_type_names(mode: &ProtocolModeManifest) -> BTreeSet<String> {
    reachable_schema_definition_keys(mode)
        .into_iter()
        .map(|key| schema_definition_rust_type_name(&key).to_string())
        .collect()
}

pub(super) fn reachable_schema_definition_keys(mode: &ProtocolModeManifest) -> BTreeSet<String> {
    let definitions = schema_definitions(mode.protocol_mode);
    let mut seen = BTreeSet::new();
    let mut pending = direct_schema_definition_keys(mode, definitions)
        .into_iter()
        .collect::<Vec<_>>();

    while let Some(key) = pending.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some(schema) = definitions.get(&key) else {
            let schema_ref = schema_ref_for_definition_key(&key);
            panic!("Go SDK manifest schema ref {schema_ref} is missing from schema bundle");
        };
        let mut refs = BTreeSet::new();
        collect_schema_definition_refs(schema, definitions, &mut refs);
        for referenced in refs {
            if !seen.contains(&referenced) {
                pending.push(referenced);
            }
        }
    }

    seen
}

fn direct_schema_definition_keys(
    mode: &ProtocolModeManifest,
    definitions: &BTreeMap<String, Value>,
) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for entry in mode
        .client_requests
        .iter()
        .chain(mode.server_requests.iter())
    {
        collect_schema_ref_key(&entry.params_schema_ref, definitions, &mut keys);
        collect_schema_ref_key(&entry.response_schema_ref, definitions, &mut keys);
    }
    for entry in mode
        .server_notifications
        .iter()
        .chain(mode.client_notifications.iter())
    {
        collect_schema_ref_key(&entry.payload_schema_ref, definitions, &mut keys);
    }
    keys
}

fn collect_schema_ref_key(
    schema_ref: &Option<String>,
    definitions: &BTreeMap<String, Value>,
    keys: &mut BTreeSet<String>,
) {
    if let Some(schema_ref) = schema_ref
        && let Some(key) = schema_definition_key_from_ref(schema_ref, definitions)
    {
        keys.insert(key);
    }
}

fn collect_schema_definition_refs(
    value: &Value,
    definitions: &BTreeMap<String, Value>,
    refs: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(obj) => {
            if let Some(Value::String(reference)) = obj.get("$ref")
                && let Some(key) = schema_definition_key_from_ref(reference, definitions)
            {
                refs.insert(key);
            }
            for child in obj.values() {
                collect_schema_definition_refs(child, definitions, refs);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_schema_definition_refs(child, definitions, refs);
            }
        }
        _ => {}
    }
}

fn schema_definition_key_from_ref(
    reference: &str,
    definitions: &BTreeMap<String, Value>,
) -> Option<String> {
    let raw_key = reference.strip_prefix("#/definitions/")?;
    let mut parts = raw_key.split('/');
    let first = parts.next().unwrap_or(raw_key);
    if matches!(first, "v1" | "v2") {
        let name = parts.next().unwrap_or_default();
        if name.is_empty() {
            return None;
        }
        return Some(format!("{first}/{name}"));
    }
    if raw_key.is_empty() {
        return None;
    }
    if definitions.contains_key(raw_key) {
        return Some(raw_key.to_string());
    }

    let namespace_matches = ["v1", "v2"]
        .into_iter()
        .map(|namespace| format!("{namespace}/{raw_key}"))
        .filter(|key| definitions.contains_key(key))
        .collect::<Vec<_>>();
    match namespace_matches.as_slice() {
        [] => Some(raw_key.to_string()),
        [key] => Some(key.clone()),
        _ => panic!("Go SDK manifest schema ref {reference} is ambiguous across schema namespaces"),
    }
}

pub(super) fn schema_ref_for_definition_key(key: &str) -> String {
    format!("#/definitions/{key}")
}

pub(super) fn schema_definition_rust_type_name(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

fn schema_definitions(protocol_mode: ProtocolModeName) -> &'static BTreeMap<String, Value> {
    static STABLE_SCHEMA_DEFINITIONS: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    static EXPERIMENTAL_SCHEMA_DEFINITIONS: OnceLock<BTreeMap<String, Value>> = OnceLock::new();

    match protocol_mode {
        ProtocolModeName::Stable => {
            STABLE_SCHEMA_DEFINITIONS.get_or_init(|| build_schema_definition_map(protocol_mode))
        }
        ProtocolModeName::Experimental => EXPERIMENTAL_SCHEMA_DEFINITIONS
            .get_or_init(|| build_schema_definition_map(protocol_mode)),
    }
}

fn build_schema_definition_map(protocol_mode: ProtocolModeName) -> BTreeMap<String, Value> {
    let bundle = schema_bundle(protocol_mode);
    let Some(definitions) = bundle.get("definitions").and_then(Value::as_object) else {
        panic!("app-server schema bundle should contain definitions");
    };
    let mut merged = BTreeMap::new();
    for (name, schema) in definitions {
        if matches!(name.as_str(), "v1" | "v2") {
            let Some(namespace_definitions) = schema.as_object() else {
                panic!("app-server schema namespace {name} should contain definitions");
            };
            for (nested_name, nested_schema) in namespace_definitions {
                insert_schema_definition(
                    &mut merged,
                    format!("{name}/{nested_name}"),
                    nested_schema.clone(),
                );
            }
            continue;
        }

        insert_schema_definition(&mut merged, name.clone(), schema.clone());

        if let Some(nested) = schema.get("definitions").and_then(Value::as_object) {
            for (nested_name, nested_schema) in nested {
                insert_schema_definition(&mut merged, nested_name.clone(), nested_schema.clone());
            }
        }
    }
    merged
}

fn insert_schema_definition(definitions: &mut BTreeMap<String, Value>, key: String, schema: Value) {
    if let Some(existing) = definitions.get(&key) {
        if existing == &schema {
            return;
        }
        panic!("Go SDK manifest schema definition {key} has conflicting schemas");
    }
    definitions.insert(key, schema);
}

fn schema_bundle(protocol_mode: ProtocolModeName) -> &'static Value {
    static STABLE_SCHEMA_BUNDLE: OnceLock<Value> = OnceLock::new();
    static EXPERIMENTAL_SCHEMA_BUNDLE: OnceLock<Value> = OnceLock::new();

    match protocol_mode {
        ProtocolModeName::Stable => STABLE_SCHEMA_BUNDLE.get_or_init(|| {
            match crate::export::build_json_schema_bundle(/*experimental_api*/ false) {
                Ok(bundle) => bundle,
                Err(error) => {
                    panic!("build stable app-server schema bundle for Go SDK manifest: {error}")
                }
            }
        }),
        ProtocolModeName::Experimental => EXPERIMENTAL_SCHEMA_BUNDLE.get_or_init(|| {
            match crate::export::build_json_schema_bundle(/*experimental_api*/ true) {
                Ok(bundle) => bundle,
                Err(error) => {
                    panic!(
                        "build experimental app-server schema bundle for Go SDK manifest: {error}"
                    )
                }
            }
        }),
    }
}

fn manifest_digest_inputs(manifest: &GoSdkManifest, mode: &ProtocolModeManifest) -> Value {
    serde_json::json!({
        "manifestSchemaVersion": manifest.manifest_schema_version,
        "modelContextLimits": manifest.model_context_limits,
        "clientRequests": mode.client_requests,
        "serverRequests": mode.server_requests,
        "serverNotifications": mode.server_notifications,
        "clientNotifications": mode.client_notifications,
        "serdeShapes": mode.serde_shapes,
        "routingLifecycle": mode.routing_lifecycle,
    })
}

fn request_serialization_digest_inputs(mode: &ProtocolModeManifest) -> Value {
    serde_json::json!(
        mode.client_requests
            .iter()
            .map(|entry| serde_json::json!({
                "method": entry.method,
                "requestSerializationScopes": entry.request_serialization_scopes,
            }))
            .collect::<Vec<_>>()
    )
}

fn visibility_digest_inputs(mode: &ProtocolModeManifest) -> Value {
    serde_json::json!({
        "clientRequests": mode.client_requests.iter().map(request_visibility_digest_input).collect::<Vec<_>>(),
        "serverRequests": mode.server_requests.iter().map(request_visibility_digest_input).collect::<Vec<_>>(),
        "serverNotifications": mode.server_notifications.iter().map(notification_visibility_digest_input).collect::<Vec<_>>(),
        "clientNotifications": mode.client_notifications.iter().map(notification_visibility_digest_input).collect::<Vec<_>>(),
    })
}

fn request_visibility_digest_input(entry: &RequestManifestEntry) -> Value {
    serde_json::json!({
        "method": entry.method,
        "sdkVisibility": entry.sdk_visibility,
        "experimental": entry.experimental,
        "experimentalFields": entry.experimental_fields,
        "inspectParams": entry.inspect_params,
        "exception": entry.exception,
        "schemaExcludedReason": entry.schema_excluded_reason,
    })
}

fn notification_visibility_digest_input(entry: &NotificationManifestEntry) -> Value {
    serde_json::json!({
        "method": entry.method,
        "sdkVisibility": entry.sdk_visibility,
        "experimental": entry.experimental,
        "experimentalFields": entry.experimental_fields,
        "exception": entry.exception,
        "schemaExcludedReason": entry.schema_excluded_reason,
    })
}

fn routing_lifecycle_digest_inputs(mode: &ProtocolModeManifest) -> Value {
    serde_json::json!({
        "routingLifecycle": mode.routing_lifecycle,
        "serverNotifications": mode.server_notifications.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "routingStrategy": entry.routing_strategy,
        })).collect::<Vec<_>>(),
    })
}

fn experimental_filter_digest_inputs(mode: &ProtocolModeManifest) -> Value {
    serde_json::json!({
        "protocolMode": mode.protocol_mode,
        "clientRequests": mode.client_requests.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "experimental": entry.experimental,
            "experimentalFields": entry.experimental_fields,
        })).collect::<Vec<_>>(),
        "serverRequests": mode.server_requests.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "experimental": entry.experimental,
            "experimentalFields": entry.experimental_fields,
        })).collect::<Vec<_>>(),
        "serverNotifications": mode.server_notifications.iter().map(|entry| serde_json::json!({
            "method": entry.method,
            "experimental": entry.experimental,
            "experimentalFields": entry.experimental_fields,
        })).collect::<Vec<_>>(),
    })
}

fn digest_value(value: Value) -> String {
    let canonical_value = sort_object_keys(value);
    let digest_input = match serde_json::to_string(&canonical_value) {
        Ok(json) => json,
        Err(error) => error.to_string(),
    };
    sha256_hex(digest_input.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

pub(super) fn sort_object_keys(value: Value) -> Value {
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
