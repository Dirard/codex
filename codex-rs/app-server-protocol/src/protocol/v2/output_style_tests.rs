use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

/// R4.4: parameters and responses use camelCase, `source` is lowercase and built-in styles
/// carry no `path`.
#[test]
fn output_style_payloads_serialize_in_camel_case() {
    assert_eq!(
        serde_json::to_value(ThreadOutputStyleListParams {
            thread_id: "thr-1".to_string(),
        })
        .expect("serializes"),
        json!({ "threadId": "thr-1" })
    );
    assert_eq!(
        serde_json::to_value(ThreadOutputStyleSetParams {
            thread_id: "thr-1".to_string(),
            name: "outcome-first".to_string(),
        })
        .expect("serializes"),
        json!({ "threadId": "thr-1", "name": "outcome-first" })
    );

    let path = if cfg!(windows) {
        "C:\\home\\output-styles\\terse.md"
    } else {
        "/home/output-styles/terse.md"
    };
    let response = ThreadOutputStyleListResponse {
        current: "terse".to_string(),
        styles: vec![
            OutputStyleInfo {
                name: "default".to_string(),
                description: "Standard.".to_string(),
                source: OutputStyleSource::Builtin,
                path: None,
            },
            OutputStyleInfo {
                name: "terse".to_string(),
                description: "Short.".to_string(),
                source: OutputStyleSource::User,
                path: Some(AbsolutePathBuf::from_absolute_path(path).expect("absolute path")),
            },
            OutputStyleInfo {
                name: "team".to_string(),
                description: "Team.".to_string(),
                source: OutputStyleSource::Project,
                path: None,
            },
        ],
        warnings: vec!["skipped".to_string()],
    };
    let value = serde_json::to_value(&response).expect("serializes");
    assert_eq!(
        value,
        json!({
            "current": "terse",
            "styles": [
                { "name": "default", "description": "Standard.", "source": "builtin" },
                { "name": "terse", "description": "Short.", "source": "user", "path": path },
                { "name": "team", "description": "Team.", "source": "project" },
            ],
            "warnings": ["skipped"],
        })
    );
    let parsed: ThreadOutputStyleListResponse =
        serde_json::from_value(value).expect("deserializes");
    assert_eq!(parsed, response);

    let set = ThreadOutputStyleSetResponse {
        style: response.styles[0].clone(),
    };
    assert_eq!(
        serde_json::to_value(set).expect("serializes"),
        json!({ "style": { "name": "default", "description": "Standard.", "source": "builtin" } })
    );
}
