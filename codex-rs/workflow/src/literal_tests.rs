use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn accepts_javascript_object_syntax() {
    let value = parse_literal(
        "{\n  name: 'find-bugs', // comment\n  \"description\": `Fan-out\\nsearch`,\n  phases: ['Find', \"Verify\",],\n  n: -1.5e3, ok: true, none: null, nested: { 1: [0x0] },\n}",
    );
    // Hexadecimal numbers are not part of the supported subset.
    assert!(value.is_err());

    let value = parse_literal(
        "{\n  name: 'find-bugs', // comment\n  \"description\": `Fan-out\\nsearch`,\n  phases: ['Find', \"Verify\",],\n  /* block */ n: -1.5e3, ok: true, none: null, nested: { 1: [10] },\n}",
    )
    .expect("parse");
    assert_eq!(
        value,
        json!({
            "name": "find-bugs",
            "description": "Fan-out\nsearch",
            "phases": ["Find", "Verify"],
            "n": -1500.0,
            "ok": true,
            "none": null,
            "nested": {"1": [10]},
        })
    );
}

#[test]
fn rejects_expressions() {
    let error = parse_literal("{ name: compute() }").unwrap_err();
    assert_eq!(error.offset, 8);
    assert!(error.message.contains("`compute` is not a literal"));
    assert!(parse_literal("{ a: `x${y}` }").is_err());
    assert!(parse_literal("{ a: 1 } + 1").is_err());
    assert!(parse_literal("{ a: 1, a: 2 }").is_err());
    assert!(parse_literal("[1,,2]").is_err());
}

#[test]
fn decodes_escapes() {
    assert_eq!(
        parse_literal(r#"'\u0041\u{1F600}\x41\'\\'"#).expect("parse"),
        json!("A\u{1F600}A'\\")
    );
    assert_eq!(
        parse_literal(r#""\uD83D\uDE00""#).expect("parse"),
        json!("\u{1F600}")
    );
}
