use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn finding_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["items", "note"],
        "properties": {
            "note": {"type": ["string", "null"]},
            "items": {
                "type": "array",
                "maxItems": 3,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["file", "line", "severity"],
                    "properties": {
                        "file": {"type": "string", "minLength": 1},
                        "line": {"type": "integer", "minimum": 1},
                        "severity": {"enum": ["low", "high"]}
                    }
                }
            }
        }
    })
}

/// R3.5: strict-compatible schemas pass; each non-strict form is rejected with a reason.
#[test]
fn strict_compatibility_is_checked() {
    check_strict_compatible(&finding_schema()).expect("strict schema");

    let cases = [
        (
            json!({"type": "array", "items": {"type": "string"}}),
            "root schema",
        ),
        (
            json!({"type": "object", "properties": {"a": {"type": "string"}}, "required": ["a"]}),
            "additionalProperties: false",
        ),
        (
            json!({"type": "object", "additionalProperties": false, "properties": {"a": {"type": "string"}}}),
            "`a` is missing",
        ),
        (
            json!({"type": "object", "additionalProperties": false, "required": ["a"], "properties": {"a": {"type": "array"}}}),
            "needs `items`",
        ),
        (
            json!({"type": "object", "additionalProperties": false, "required": ["a"], "properties": {"a": {}}}),
            "needs `type`",
        ),
        (
            json!({"type": "object", "additionalProperties": false, "required": ["a"], "properties": {"a": {"$ref": "#/$defs/x"}}}),
            "`$ref` is not supported",
        ),
        (
            json!({"type": "object", "additionalProperties": false, "required": [], "properties": {}, "oneOf": []}),
            "`oneOf` is not supported",
        ),
    ];
    for (schema, expected) in cases {
        let error = check_strict_compatible(&schema).unwrap_err();
        assert!(error.contains(expected), "{schema}: {error}");
    }
}

/// R3.5, R3.6: values are validated with JSON paths in the errors.
#[test]
fn values_are_validated_against_the_schema() {
    let schema = finding_schema();
    validate_value(
        &schema,
        &json!({"note": null, "items": [{"file": "a.py", "line": 3, "severity": "high"}]}),
    )
    .expect("valid value");

    let errors = validate_value(
        &schema,
        &json!({"items": [{"file": "", "line": 0.5, "severity": "mid", "extra": 1}]}),
    )
    .unwrap_err();
    assert_eq!(
        errors,
        vec![
            "$: missing required property `note`".to_string(),
            "$.items[0]: unexpected property `extra`".to_string(),
            "$.items[0].file: has 0 characters; at least 1 required".to_string(),
            "$.items[0].line: expected integer, got number".to_string(),
            "$.items[0].severity: value is not one of the `enum` values".to_string(),
        ]
    );
    assert_eq!(
        validate_value(&schema, &json!([])).unwrap_err(),
        vec!["$: expected object, got array".to_string()]
    );
}

#[test]
fn final_messages_may_be_fenced() {
    assert_eq!(
        parse_final_message("```json\n{\"a\": 1}\n```").expect("json"),
        json!({"a": 1})
    );
    assert_eq!(parse_final_message(" [1] ").expect("json"), json!([1]));
    assert!(parse_final_message("not json").is_err());
}
