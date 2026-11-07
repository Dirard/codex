use super::*;
use crate::meta::parse_script;
use pretty_assertions::assert_eq;

const SCRIPT: &str = "// comment\nexport const meta = { name: 'x', description: 'y' };\nthrow new Error('line 3');\n";

const ENGINE: WorkflowEngineInfo = WorkflowEngineInfo {
    max_concurrent_agents: 4,
    max_agents_per_run: 20,
    worktree_isolation: false,
    read_only_explorers: true,
};

/// UC2: script lines keep their numbers inside the cell.
#[test]
fn script_lines_keep_their_numbers() {
    let parsed = parse_script(SCRIPT).expect("script");
    let source = build_cell_source(&parsed.body, "{\"files\":[\"a\"]}", Some(10), &ENGINE);
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
    let source = build_cell_source("", "{\"q\":\"a'b\\\"c\"}", None, &ENGINE);
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
    assert!(!first.contains("__ENGINE_JSON__"));
    // Builtin-workflows R4.1: the frozen engine global is part of the one prelude line.
    assert!(
        first.contains(r#"const engine = deepFreeze(JSON.parse("{\"max_concurrent_agents\":4,\"max_agents_per_run\":20,\"worktree_isolation\":false,\"read_only_explorers\":true}"));"#),
        "{first}"
    );
    assert!(first.starts_with(
        "const { agent, parallel, pipeline, phase, log, workflow, budget, args, engine,"
    ));
    for tool in HOST_TOOLS {
        assert!(first.contains(&format!("host.{tool}")), "{tool}");
    }
    assert!(build_cell_source("", "1", Some(42), &ENGINE).contains("limit: 42"));
}

/// Builtin-workflows review B-1: `args` that contain the text of a prelude placeholder reach
/// the script as given and leave the other literals of the prelude intact.
#[test]
fn args_naming_placeholders_stay_intact() {
    let args_json = r#"{"q":"where is __ENGINE_JSON__ replaced? __BUDGET_LIMIT__ __MAX_FAN_OUT__ __ARGS_JSON__"}"#;
    let source = build_cell_source("", args_json, Some(7), &ENGINE);
    let first = source.lines().next().expect("first line");
    let args_literal = serde_json::to_string(args_json).expect("literal");
    assert!(
        first.contains(&format!(
            "const args = deepFreeze(JSON.parse({args_literal}));"
        )),
        "{first}"
    );
    assert!(
        first.contains(r#"const engine = deepFreeze(JSON.parse("{\"max_concurrent_agents\":4,\"max_agents_per_run\":20,\"worktree_isolation\":false,\"read_only_explorers\":true}"));"#),
        "{first}"
    );
    assert!(first.contains("limit: 7,"), "{first}");
    assert!(first.contains("at most 4096 thunks"), "{first}");
}

/// UC12, R12.1: a nested script's first line stays on line 1 of its function, and V8 names
/// it after the saved script.
#[test]
fn nested_source_keeps_script_lines() {
    let parsed = parse_script(SCRIPT).expect("script");
    let source = build_nested_source(&parsed.body, "verify");
    let lines: Vec<&str> = source.lines().collect();
    // Builtin-workflows R4.1, R4.4: the nested function takes `engine` and runs the body in
    // an inner function whose declarations shadow the parameters.
    assert_eq!(
        lines[0],
        "(async function (agent, parallel, pipeline, phase, log, workflow, budget, args, console, engine) {\"use strict\";return (async () => {// comment"
    );
    assert_eq!(lines[2], "throw new Error('line 3');");
    assert_eq!(&lines[3..], &["", "})();})", "//# sourceURL=verify.js"]);
    let prelude = build_cell_source("", "null", None, &ENGINE);
    assert!(prelude.contains("WorkflowNestingError"));
    assert!(prelude.contains("host.__wf_workflow({ kind: \"load\""));
    assert!(prelude.contains("scope.console, engine);"));
}

#[test]
fn errors_name_the_script() {
    assert_eq!(
        display_error("Error: x\n    at __wf_main (exec_main.mjs:3:7)"),
        "Error: x\n    at __wf_main (workflow.js:3:7)"
    );
}
