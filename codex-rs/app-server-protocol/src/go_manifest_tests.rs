use pretty_assertions::assert_eq;

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
        notification.payload_type,
        Some("RawResponseItemCompletedNotification")
    );
    assert_eq!(
        notification.payload_schema_ref,
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

fn initialize_entry<'a>(
    entries: &'a [crate::go_manifest::RequestManifestEntry],
    mode_name: &str,
) -> &'a crate::go_manifest::RequestManifestEntry {
    entries
        .iter()
        .find(|entry| entry.method == "initialize")
        .unwrap_or_else(|| panic!("{mode_name} manifest should include initialize"))
}
