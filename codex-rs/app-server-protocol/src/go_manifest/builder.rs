use std::collections::BTreeSet;

use super::digest::digest_set_for_manifest_mode;
use super::digest::empty_digest_set;
use super::digest::reachable_schema_rust_type_names;
use super::lifecycle::go_sdk_routing_lifecycle_entries;
use super::serde_shape_fields::append_schema_sufficient_serde_shapes;
use super::serde_shapes::go_sdk_serde_shapes;
use super::*;
use crate::protocol::v2::MAX_ADDITIONAL_CONTEXT_ENTRIES;
use crate::protocol::v2::MAX_ADDITIONAL_CONTEXT_KEY_BYTES;
use crate::protocol::v2::MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES;
use crate::protocol::v2::MAX_ADDITIONAL_CONTEXT_VALUE_BYTES;

pub fn go_sdk_manifest() -> GoSdkManifest {
    let mut experimental = ProtocolModeManifest {
        protocol_mode: ProtocolModeName::Experimental,
        client_requests: crate::protocol::common::go_sdk_client_request_manifest_entries(),
        server_requests: crate::protocol::common::go_sdk_server_request_manifest_entries(),
        server_notifications: crate::protocol::common::go_sdk_server_notification_manifest_entries(
        ),
        client_notifications: crate::protocol::common::go_sdk_client_notification_manifest_entries(
        ),
        serde_shapes: go_sdk_serde_shapes(),
        routing_lifecycle: go_sdk_routing_lifecycle_entries(),
        digests: empty_digest_set(),
    };
    append_schema_sufficient_serde_shapes(&mut experimental);
    let stable = stable_manifest_from_experimental(&experimental);

    let mut manifest = GoSdkManifest {
        manifest_schema_version: 1,
        stable,
        experimental,
        model_context_limits: ModelContextLimits {
            max_additional_context_entries: MAX_ADDITIONAL_CONTEXT_ENTRIES as u32,
            max_additional_context_key_bytes: MAX_ADDITIONAL_CONTEXT_KEY_BYTES as u32,
            max_additional_context_value_bytes: MAX_ADDITIONAL_CONTEXT_VALUE_BYTES as u32,
            max_additional_context_total_bytes: MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES as u32,
        },
    };
    manifest.experimental.digests =
        digest_set_for_manifest_mode(&manifest, ProtocolModeName::Experimental);
    manifest.stable.digests = digest_set_for_manifest_mode(&manifest, ProtocolModeName::Stable);
    manifest
}
fn stable_manifest_from_experimental(experimental: &ProtocolModeManifest) -> ProtocolModeManifest {
    let mut stable = ProtocolModeManifest {
        protocol_mode: ProtocolModeName::Stable,
        client_requests: experimental
            .client_requests
            .iter()
            .filter(|entry| entry.experimental.is_none())
            .cloned()
            .collect(),
        server_requests: experimental
            .server_requests
            .iter()
            .filter(|entry| entry.experimental.is_none())
            .cloned()
            .collect(),
        server_notifications: experimental
            .server_notifications
            .iter()
            .filter(|entry| entry.experimental.is_none())
            .cloned()
            .collect(),
        client_notifications: experimental
            .client_notifications
            .iter()
            .filter(|entry| entry.experimental.is_none())
            .cloned()
            .collect(),
        serde_shapes: Vec::new(),
        routing_lifecycle: Vec::new(),
        digests: empty_digest_set(),
    };
    stable.serde_shapes = filter_serde_shapes_for_mode(&stable, &experimental.serde_shapes);
    stable.routing_lifecycle =
        filter_routing_lifecycle_for_mode(&stable, &experimental.routing_lifecycle);
    stable
}

fn filter_serde_shapes_for_mode(
    mode: &ProtocolModeManifest,
    serde_shapes: &[SerdeShapeEntry],
) -> Vec<SerdeShapeEntry> {
    let reachable = reachable_schema_rust_type_names(mode);
    serde_shapes
        .iter()
        .filter(|entry| reachable.contains(&entry.rust_type))
        .cloned()
        .collect()
}

fn filter_routing_lifecycle_for_mode(
    mode: &ProtocolModeManifest,
    routing_lifecycle: &[RoutingLifecycleEntry],
) -> Vec<RoutingLifecycleEntry> {
    let methods = manifest_method_set(mode);
    routing_lifecycle
        .iter()
        .filter(|entry| {
            routing_lifecycle_entry_methods(entry).all(|method| methods.contains(method))
        })
        .cloned()
        .collect()
}

fn manifest_method_set(mode: &ProtocolModeManifest) -> BTreeSet<&'static str> {
    mode.client_requests
        .iter()
        .map(|entry| entry.method)
        .chain(mode.server_requests.iter().map(|entry| entry.method))
        .chain(mode.server_notifications.iter().map(|entry| entry.method))
        .chain(mode.client_notifications.iter().map(|entry| entry.method))
        .collect()
}

fn routing_lifecycle_entry_methods(
    entry: &RoutingLifecycleEntry,
) -> impl Iterator<Item = &'static str> + '_ {
    std::iter::once(entry.start_method)
        .chain(std::iter::once(wire_completion_method(
            &entry.start_completion,
        )))
        .chain(entry.cleanup_triggers.iter().map(cleanup_trigger_method))
        .chain(entry.notification_opt_out_dependencies.iter().copied())
}

fn wire_completion_method(completion: &WireCompletion) -> &'static str {
    match completion {
        WireCompletion::JsonRpcResponse { method }
        | WireCompletion::TerminalNotification { method, .. }
        | WireCompletion::ExplicitMethodResponse { method } => method,
    }
}

fn cleanup_trigger_method(trigger: &CleanupTrigger) -> &'static str {
    match trigger {
        CleanupTrigger::JsonRpcResponse { method }
        | CleanupTrigger::TerminalNotification { method, .. }
        | CleanupTrigger::ExplicitMethodResponse { method } => method,
    }
}
