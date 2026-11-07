use super::*;
use crate::args::check_args_schema;
use crate::literal::parse_literal;
use crate::meta::MAX_PHASES;
use crate::meta::MAX_SCRIPT_BYTES;
use crate::meta::parse_script;
use crate::scan::Token;
use crate::scan::TokenKind;
use crate::scan::scan;
use crate::schema::check_strict_compatible;
use codex_utils_string::approx_token_count;
use pretty_assertions::assert_eq;
use serde_json::json;

fn tokens(builtin: &BuiltinWorkflow) -> Vec<Token<'static>> {
    scan(builtin.source).unwrap_or_else(|error| panic!("{}: {error:?}", builtin.name))
}

/// The tokens between the parenthesis that opens at `open` and its match.
fn call_arguments<'a>(tokens: &'a [Token<'static>], open: usize) -> &'a [Token<'static>] {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token.kind != TokenKind::Punct {
            continue;
        }
        match token.text {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return &tokens[open + 1..index];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced call");
}

/// Builtin-workflows UC1/R1.6: every built-in script passes static validation, its
/// `meta.name` is its table name, its `meta.args` passes `check_args_schema`, it declares at
/// most 32 phases and stays below the script size limit.
#[test]
fn every_builtin_parses() {
    assert_eq!(builtin_names(), vec!["review", "understand", "research"]);
    for builtin in BUILTIN_WORKFLOWS {
        let parsed = parse_script(builtin.source)
            .unwrap_or_else(|error| panic!("{}: {error}", builtin.name));
        assert_eq!(parsed.meta.name, builtin.name);
        let args = parsed
            .meta
            .args
            .unwrap_or_else(|| panic!("{} declares meta.args", builtin.name));
        check_args_schema(&args.schema).unwrap_or_else(|error| panic!("{}: {error}", builtin.name));
        assert!(!parsed.meta.phases.is_empty(), "{}", builtin.name);
        assert!(parsed.meta.phases.len() <= MAX_PHASES, "{}", builtin.name);
        assert!(builtin.source.len() < MAX_SCRIPT_BYTES, "{}", builtin.name);
        assert!(!builtin.summary.is_empty(), "{}", builtin.name);
        assert_eq!(find_builtin(builtin.name), Some(builtin));
    }
    assert_eq!(find_builtin("migrate"), None, "migrate is wave 2");
    assert_eq!(find_builtin("builtin-review"), None);
}

/// Builtin-workflows UC1/R1.9: each script declares its agent schemas in one top-level pure
/// literal `const SCHEMAS = {…};`, and every schema in it is strict-compatible.
#[test]
fn builtin_schemas_are_strict() {
    for builtin in BUILTIN_WORKFLOWS {
        let tokens = tokens(builtin);
        let start = tokens
            .windows(4)
            .position(|window| {
                window[0].is_ident("const")
                    && window[1].is_ident("SCHEMAS")
                    && window[2].is_punct("=")
                    && window[3].is_punct("{")
            })
            .unwrap_or_else(|| panic!("{} declares `const SCHEMAS = {{`", builtin.name));
        let open = start + 3;
        let body = call_arguments(&tokens, open);
        let close = tokens[open + 1 + body.len()];
        let literal = &builtin.source[tokens[open].start..close.end()];
        let schemas = parse_literal(literal).unwrap_or_else(|error| {
            panic!("{}: SCHEMAS is not a literal: {error:?}", builtin.name)
        });
        let schemas = schemas.as_object().expect("SCHEMAS is an object");
        assert!(!schemas.is_empty(), "{}", builtin.name);
        for (key, schema) in schemas {
            check_strict_compatible(schema)
                .unwrap_or_else(|error| panic!("{}: SCHEMAS.{key}: {error}", builtin.name));
        }
        // Agent calls use these schemas only.
        for (index, token) in tokens.iter().enumerate() {
            if token.is_ident("schema") && tokens[index + 1].is_punct(":") {
                assert!(
                    tokens[index + 2].is_ident("SCHEMAS") || tokens[index + 2].is_ident("schema"),
                    "{}: line {} passes a schema outside SCHEMAS",
                    builtin.name,
                    token.line
                );
            }
        }
    }
}

/// Builtin-workflows UC1/R1.10, R1.11: every `agent(` call's task is a template that starts
/// with the tag `[codex-<name>:`; no call sets `model`, `effort` or `isolation` or spreads
/// options, and every call runs an explorer.
#[test]
fn builtin_prompts_are_tagged() {
    for builtin in BUILTIN_WORKFLOWS {
        let tokens = tokens(builtin);
        let mut calls = 0;
        for (index, token) in tokens.iter().enumerate() {
            let is_call = token.is_ident("agent")
                && tokens.get(index + 1).is_some_and(|next| next.is_punct("("))
                && !index
                    .checked_sub(1)
                    .is_some_and(|previous| tokens[previous].is_member_access());
            if !is_call {
                continue;
            }
            calls += 1;
            let arguments = call_arguments(&tokens, index + 1);
            let task = arguments.first().expect("agent() has a task");
            assert!(
                task.kind == TokenKind::Template
                    && task.text.starts_with(&format!("[codex-{}:", builtin.name)),
                "{}: the task on line {} must start with `[codex-{}:`",
                builtin.name,
                token.line,
                builtin.name
            );
            for forbidden in ["model", "effort", "isolation"] {
                assert!(
                    !arguments
                        .iter()
                        .any(|argument| argument.is_ident(forbidden)),
                    "{}: agent() on line {} sets `{forbidden}`",
                    builtin.name,
                    token.line
                );
            }
            assert!(
                !arguments.iter().any(|argument| argument.is_punct("...")),
                "{}: agent() on line {} spreads options",
                builtin.name,
                token.line
            );
            assert!(
                arguments
                    .windows(3)
                    .any(|window| window[0].is_ident("agent_type")
                        && window[1].is_punct(":")
                        && window[2].text == "\"explorer\""),
                "{}: agent() on line {} must run an explorer",
                builtin.name,
                token.line
            );
        }
        assert_eq!(
            calls, 1,
            "{}: one agent() call site, in `ask`",
            builtin.name
        );
        // Every tag of the script names the script.
        for token in tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Template)
        {
            if let Some(rest) = token.text.strip_prefix("[codex-") {
                assert!(
                    rest.starts_with(&format!("{}:", builtin.name)),
                    "{}: tag {} on line {}",
                    builtin.name,
                    token.text,
                    token.line
                );
            }
        }
    }
}

/// Builtin-workflows UC1/R1.6: built-in scripts never call `workflow(`, so each one also
/// runs as a nested workflow.
#[test]
fn builtins_never_nest() {
    for builtin in BUILTIN_WORKFLOWS {
        let tokens = tokens(builtin);
        for (index, token) in tokens.iter().enumerate() {
            let called = token.is_ident("workflow")
                && tokens.get(index + 1).is_some_and(|next| next.is_punct("("));
            assert!(
                !called,
                "{}: workflow() on line {}",
                builtin.name, token.line
            );
        }
    }
}

/// Builtin-workflows R2.1, R2.2: the listing stays within 200 tokens and names every
/// built-in script exactly once.
#[test]
fn listing_names_every_builtin_once() {
    assert!(
        approx_token_count(BUILTIN_LISTING) <= 200,
        "{} tokens",
        approx_token_count(BUILTIN_LISTING)
    );
    let listed: Vec<&str> = BUILTIN_LISTING
        .lines()
        .filter_map(|line| line.strip_prefix("- `"))
        .filter_map(|line| line.split('`').next())
        .collect();
    assert_eq!(listed, builtin_names());
    for builtin in BUILTIN_WORKFLOWS {
        assert!(
            BUILTIN_LISTING.contains(builtin.summary),
            "{}: the listing carries its summary",
            builtin.name
        );
    }
    for part in [
        "a project or $CODEX_HOME script of the same name replaces one",
        "`builtin-<name>` always runs the built-in",
        "wrong `args` return the usage, no run",
    ] {
        assert!(BUILTIN_LISTING.contains(part), "{part}");
    }
}

/// Builtin-workflows R9.4: the canonical review turn names the `builtin-review` call with
/// compact args in the order of `meta.args`, and the reporting instructions.
#[test]
fn review_turn_text_names_the_call_and_the_report() {
    let text = review_turn_text(&json!({ "target": "main", "effort": "max" }));
    assert!(
        text.contains(r#"call the `workflow` tool with {"name":"builtin-review","args":{"effort":"max","target":"main"}}; the `builtin-` name runs the script built into Codex even when the project or $CODEX_HOME has its own `review`."#),
        "{text}"
    );
    for part in [
        "Do not review the code yourself.",
        "get the full result with `workflow_status` if the preview is cut",
        "most severe first: `[P1] <title> — <file>:<line>`, the failure scenario, and the verdict with its votes",
        "then the summary, the omitted counts and the coverage notes",
        "If the run fails or you do not have the `workflow` tool",
        "say so and stop.",
    ] {
        assert!(text.contains(part), "{part:?} missing from {text}");
    }
    for empty in [json!({}), serde_json::Value::Null] {
        assert!(
            review_turn_text(&empty).contains(r#"{"name":"builtin-review","args":{}}"#),
            "{empty}"
        );
    }
    assert!(
        review_turn_text(&json!({ "zeta": 1, "uncommitted": true, "base": "main" }))
            .contains(r#""args":{"base":"main","uncommitted":true,"zeta":1}"#)
    );
}

/// Builtin-workflows review B-8: `/code-review` and `codex exec review --workflow` ask for the
/// built-in by its `builtin-` name, so a project or `$CODEX_HOME` script named `review` cannot
/// take their place; only a model's own `{name: "review"}` call reaches such a script.
#[test]
fn review_turn_text_runs_the_builtin_by_its_prefix() {
    let text = review_turn_text(&json!({ "effort": "high" }));
    assert!(
        text.contains(&format!(
            r#"{{"name":"{BUILTIN_PREFIX}review","args":{{"effort":"high"}}}}"#
        )),
        "{text}"
    );
    assert!(!text.contains(r#""name":"review""#), "{text}");
}
