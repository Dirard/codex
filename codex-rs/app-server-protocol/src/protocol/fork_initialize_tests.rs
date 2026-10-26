use super::*;
use anyhow::Result;
use codex_utils_absolute_path::test_support::PathBufExt;
use codex_utils_absolute_path::test_support::test_path_buf;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn initialize_response_serializes_protocol_digests() -> Result<()> {
    let response = InitializeResponse {
        user_agent: "codex-test".to_string(),
        codex_home: test_path_buf("/tmp/codex-home").abs(),
        platform_family: "unix".to_string(),
        platform_os: "linux".to_string(),
        stable_protocol_digest: "a".repeat(64),
        experimental_protocol_digest: "b".repeat(64),
        stable_schema_digest: "c".repeat(64),
        experimental_schema_digest: "d".repeat(64),
        stable_manifest_digest: "e".repeat(64),
        experimental_manifest_digest: "f".repeat(64),
        active_protocol_mode: ActiveProtocolMode::Experimental,
    };

    assert_eq!(
        json!({
            "userAgent": "codex-test",
            "codexHome": "/tmp/codex-home",
            "platformFamily": "unix",
            "platformOs": "linux",
            "stableProtocolDigest": "a".repeat(64),
            "experimentalProtocolDigest": "b".repeat(64),
            "stableSchemaDigest": "c".repeat(64),
            "experimentalSchemaDigest": "d".repeat(64),
            "stableManifestDigest": "e".repeat(64),
            "experimentalManifestDigest": "f".repeat(64),
            "activeProtocolMode": "experimental"
        }),
        serde_json::to_value(&response)?,
    );
    Ok(())
}
