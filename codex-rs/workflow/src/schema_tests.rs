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

/// R3.12: only client errors that name the schema or the response format count as the
/// provider refusing native structured output.
#[test]
fn schema_format_rejections_are_recognized() {
    let rejected = [
        (
            Some(400),
            false,
            "unexpected status 400 Bad Request: Invalid schema for response_format 'codex_output_schema'",
        ),
        (
            Some(400),
            false,
            "Unsupported parameter: 'text.format' is not supported with this model.",
        ),
        (Some(422), false, "json_schema is not supported"),
        (
            None,
            true,
            "Structured outputs are not available for this provider",
        ),
        // A plain HTTP 400 reaches the agent as the provider's error body.
        (
            None,
            false,
            r#"{"error":{"message":"json_schema is not supported","type":"invalid_request_error"}}"#,
        ),
        // The provider names the answer format as the rejected parameter.
        (
            None,
            false,
            r#"{"error":{"message":"Invalid schema for response_format 'codex_output_schema': json_schema is not supported by this provider","type":"invalid_request_error","param":"text.format","code":"invalid_json_schema"}}"#,
        ),
        (
            None,
            false,
            r#"{"error":{"message":"This model does not support it.","type":"invalid_request_error","param":"text.format.schema","code":"unsupported_parameter"}}"#,
        ),
        (
            Some(400),
            false,
            r#"unexpected status 400 Bad Request: {"error":{"message":"Unsupported value","type":"invalid_request_error","param":"response_format","code":"unsupported_value"}}"#,
        ),
    ];
    for (status, bad_request, message) in rejected {
        assert!(
            is_schema_format_rejection(status, bad_request, message),
            "{message}"
        );
    }
    let kept = [
        (
            Some(400),
            false,
            "unexpected status 400 Bad Request: context length exceeded",
        ),
        (Some(500), false, "internal error while validating schema"),
        (Some(429), false, "rate limit: too many schema requests"),
        (None, false, "the model stopped: schema"),
        (
            None,
            false,
            r#"{"error":{"message":"context too long","type":"invalid_request_error"}}"#,
        ),
        // The schema of a tool, not of the answer format.
        (
            None,
            false,
            r#"{"error":{"message":"Invalid schema for function 'mcp__docs__search': In context=(), 'additionalProperties' is required to be supplied and to be false.","type":"invalid_request_error","param":"tools[3].parameters","code":"invalid_function_parameters"}}"#,
        ),
        (
            Some(400),
            false,
            "unexpected status 400 Bad Request: Invalid schema for function 'lookup': json_schema keyword 'patternProperties' is not supported",
        ),
        // A parameter other than the answer format, even with schema words in the text.
        (
            None,
            false,
            r#"{"error":{"message":"Invalid value: the json_schema of input[2] is malformed","type":"invalid_request_error","param":"input","code":"invalid_value"}}"#,
        ),
        // "structured" about something else.
        (
            Some(400),
            false,
            "unexpected status 400 Bad Request: the input contains structured content that is not supported",
        ),
        (
            None,
            true,
            "Bad request: structured data in the message is too large",
        ),
    ];
    for (status, bad_request, message) in kept {
        assert!(
            !is_schema_format_rejection(status, bad_request, message),
            "{message}"
        );
    }
    let schema = json!({"type": "object"});
    assert!(prompt_schema_suffix(&schema).ends_with("JSON schema:\n{\"type\":\"object\"}"));
    assert!(native_rejected_follow_up(&schema).starts_with("The model provider rejected"));
}
