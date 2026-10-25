use super::*;
use crate::meta::parse_script;
use pretty_assertions::assert_eq;

const SCRIPT: &str = "// comment\nexport const meta = { name: 'x', description: 'y' };\nthrow new Error('line 3');\n";

/// UC2: script lines keep their numbers inside the cell.
#[test]
fn script_lines_keep_their_numbers() {
    let parsed = parse_script(SCRIPT).expect("script");
    let source = build_cell_source(&parsed.body, "{\"files\":[\"a\"]}", Some(10));
    let lines: Vec<&str> = source.lines().collect();
    assert!(lines[0].starts_with("const { agent, parallel, pipeline"));
    assert!(lines[0].ends_with("async function __wf_main() {// comment"));
    assert_eq!(
        lines[1],
        "       const meta = { name: 'x', description: 'y' };"
    );
    assert_eq!(lines[2], "throw new Error('line 3');");
    assert_eq!(
        &lines[3..],
        &["", "}", "await __wf_main().then(__wf_finish, __wf_fail);"]
    );
}

#[test]
fn prelude_embeds_args_budget_and_limits() {
    let source = build_cell_source("", "{\"q\":\"a'b\\\"c\"}", None);
    let first = source.lines().next().expect("first line");
    assert!(
        first.contains(r#"JSON.parse("{\"q\":\"a'b\\\"c\"}")"#),
        "{first}"
    );
    assert!(first.contains("limit: null"));
    assert!(first.contains("at most 4096 thunks"));
    assert!(!first.contains("__MAX_FAN_OUT__"));
    assert!(!first.contains("__BUDGET_LIMIT__"));
    assert!(!first.contains("__ARGS_JSON__"));
    for tool in HOST_TOOLS {
        assert!(first.contains(&format!("host.{tool}")), "{tool}");
    }
    assert!(build_cell_source("", "1", Some(42)).contains("limit: 42"));
}

#[test]
fn errors_name_the_script() {
    assert_eq!(
        display_error("Error: x\n    at __wf_main (exec_main.mjs:3:7)"),
        "Error: x\n    at __wf_main (workflow.js:3:7)"
    );
}
