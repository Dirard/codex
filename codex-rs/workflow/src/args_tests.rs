use super::*;
use crate::meta::parse_script;
use pretty_assertions::assert_eq;
use serde_json::json;

const REVIEW_LIKE: &str = r#"export const meta = {
  name: "review",
  description: "d",
  args: {
    type: "object", additionalProperties: false,
    properties: {
      effort: { type: "string", enum: ["low", "medium", "high"], description: "breadth; default \"medium\"" },
      target: { type: "string", description: "what to review" },
      paths: { type: "array", items: { type: "string" }, description: "only these paths" },
      max_findings: { anyOf: [{ type: "integer", minimum: 1 }, { const: "all" }], description: "report cap" },
    },
  },
};
return args;
"#;

const RESEARCH_LIKE: &str = r#"export const meta = {
  name: "research", description: "d",
  args: { type: "object", required: ["question"], properties: { question: { type: "string", description: "the question" }, web: { type: "boolean" } } },
};
"#;

fn meta(script: &str) -> WorkflowMeta {
    parse_script(script).expect("script").meta
}

/// R3.1: optional properties, `anyOf` and `const` are accepted; keywords outside the subset,
/// a root that is not an object schema and an oversized schema are rejected.
#[test]
fn args_schemas_follow_the_subset() {
    check_args_schema(&meta(REVIEW_LIKE).args.expect("args").schema).expect("review args");
    check_args_schema(&json!({
        "type": "object",
        "required": ["q"],
        "properties": { "q": { "type": "string" }, "n": { "anyOf": [{ "type": "integer" }, { "const": "all" }] } }
    }))
    .expect("partial required");
    for (schema, expected) in [
        (
            json!({ "type": "object", "properties": { "a": { "type": "string", "oneOf": [] } } }),
            "keyword `oneOf`",
        ),
        (json!({ "type": "string" }), "root schema"),
        (json!({ "type": "object" }), "`properties`"),
        (
            json!({ "type": "object", "required": ["b"], "properties": { "a": { "type": "string" } } }),
            "`b`",
        ),
        (
            json!({ "type": "object", "additionalProperties": { "type": "string" }, "properties": {} }),
            "must be a boolean",
        ),
        (
            json!({ "type": "object", "properties": { "a": { "type": "string", "description": "x".repeat(70 * 1024) } } }),
            "the limit is",
        ),
    ] {
        let error = check_args_schema(&schema).expect_err(expected);
        assert!(error.contains(expected), "{expected}: {error}");
    }
}

/// R3.2: null and absent args are checked as `{}`; an extra property, a wrong `enum` value
/// and a wrong type fail with their paths and the usage; scripts without `meta.args` accept
/// anything.
#[test]
fn args_are_validated_against_meta_args() {
    let review = meta(REVIEW_LIKE);
    for ok in ["null", "{}", r#"{"effort":"high","max_findings":"all"}"#] {
        validate_args(&review, ok).unwrap_or_else(|error| panic!("{ok}: {error}"));
    }
    let error =
        validate_args(&review, r#"{"effort":"huge","extra":1,"paths":"a"}"#).expect_err("bad args");
    for part in [
        "args of workflow \"review\" do not match its `meta.args`: ",
        "$.effort: value is not one of the `enum` values",
        "$: unexpected property `extra`",
        "$.paths: expected array, got string",
        "\nargs of workflow \"review\":\n- effort: ",
    ] {
        assert!(error.contains(part), "{part:?} missing from {error}");
    }
    let error = validate_args(&review, r#"{"max_findings":0}"#).expect_err("cap below 1");
    assert!(
        error.contains("$.max_findings: value matches none of `anyOf`"),
        "{error}"
    );

    let research = meta(RESEARCH_LIKE);
    let error = validate_args(&research, "null").expect_err("question is required");
    assert!(
        error.contains("$: missing required property `question`"),
        "{error}"
    );
    validate_args(&research, r#"{"question":"why?"}"#).expect("question given");

    let plain = meta("export const meta = { name: 'plain', description: 'd' };\n");
    validate_args(&plain, r#"{"anything":[1,2]}"#).expect("no meta.args");
}

/// R3.4: the usage lists properties in the order of the script, names types, allowed values
/// and bounds, marks required ones and stays within 2000 characters.
#[test]
fn usage_lists_properties_in_script_order() {
    let review = meta(REVIEW_LIKE);
    assert_eq!(
        args_usage(review.args.as_ref().expect("args"), &review.name),
        "args of workflow \"review\":\n\
         - effort: \"low\" | \"medium\" | \"high\" — breadth; default \"medium\"\n\
         - target: string — what to review\n\
         - paths: array of string — only these paths\n\
         - max_findings: integer >= 1 | \"all\" — report cap"
    );
    let research = meta(RESEARCH_LIKE);
    assert_eq!(
        args_usage(research.args.as_ref().expect("args"), &research.name),
        "args of workflow \"research\":\n- question: string, required — the question\n- web: boolean"
    );
    let long = ArgsSchema::from(json!({
        "type": "object",
        "properties": (0..100)
            .map(|index| (format!("p{index:03}"), json!({ "type": "string", "description": "d".repeat(50) })))
            .collect::<serde_json::Map<String, Value>>(),
    }));
    let usage = args_usage(&long, "long");
    assert_eq!(usage.chars().count(), MAX_ARGS_USAGE_CHARS);
    assert!(usage.ends_with('…'));
    assert!(
        usage.contains("\n- p000: string"),
        "properties without an order sort by name"
    );
}
