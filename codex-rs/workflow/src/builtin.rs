//! Built-in workflow scripts compiled into the binary (`specs/builtin-workflows-2026-11-07.md`,
//! UC1, UC2, UC9).
//!
//! A built-in script is trusted code of the binary and the last source of the saved-script
//! lookup: a project or `$CODEX_HOME` script of the same name replaces it, and
//! `builtin-<name>` always runs it (UC1). Each one is validated like any script, never calls
//! `workflow()` and runs read-only explorers (R1.6, R1.11).

use serde_json::Map;
use serde_json::Value;

/// Prefix that runs a built-in script even when a file of the same name replaces it (R1.2).
pub const BUILTIN_PREFIX: &str = "builtin-";

/// One built-in script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltinWorkflow {
    /// File name without `.js`; equals `meta.name`.
    pub name: &'static str,
    pub source: &'static str,
    /// One line for the listing (UC2).
    pub summary: &'static str,
}

/// UC1: the built-in scripts, in listing order.
pub const BUILTIN_WORKFLOWS: &[BuiltinWorkflow] = &[
    BuiltinWorkflow {
        name: "review",
        source: include_str!("../builtin/review.js"),
        summary: "find bugs in changes, keep only findings independent skeptics fail to refute",
    },
    BuiltinWorkflow {
        name: "understand",
        source: include_str!("../builtin/understand.js"),
        summary: "map how code is organized and works",
    },
    BuiltinWorkflow {
        name: "research",
        source: include_str!("../builtin/research.js"),
        summary: "answer a question with evidence from code, tests, docs, history",
    },
];

/// UC2, R2.1, R2.2: the built-in scripts as the `name` parameter of the `workflow` tool lists
/// them; static text within 200 tokens, so the tool spec stays stable for prompt caching.
pub const BUILTIN_LISTING: &str = "Built-in scripts (a project or $CODEX_HOME script of the same name replaces one; `builtin-<name>` always runs the built-in; wrong `args` return the usage, no run):
- `review`: find bugs in changes, keep only findings independent skeptics fail to refute; args {effort: low|medium|high|xhigh|max, target, base, commit, uncommitted, paths, focus, max_findings}.
- `understand`: map how code is organized and works; args {question, paths, effort}.
- `research`: answer a question with evidence from code, tests, docs, history; args {question (required), paths, effort, web}.";

/// The built-in script `name` (without the `builtin-` prefix).
pub fn find_builtin(name: &str) -> Option<&'static BuiltinWorkflow> {
    BUILTIN_WORKFLOWS
        .iter()
        .find(|builtin| builtin.name == name)
}

/// Names of the built-in scripts, in listing order.
pub fn builtin_names() -> Vec<&'static str> {
    BUILTIN_WORKFLOWS
        .iter()
        .map(|builtin| builtin.name)
        .collect()
}

/// Effort levels of the built-in `review`, from the cheapest (UC5, UC9, UC10).
pub const REVIEW_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// Keys of the `review` args in the order of its `meta.args`, for a stable canonical text.
const REVIEW_ARG_ORDER: &[&str] = &[
    "effort",
    "target",
    "base",
    "commit",
    "uncommitted",
    "paths",
    "focus",
    "max_findings",
];

/// UC9, R9.4: the user turn that `/code-review` and `codex exec review --workflow` submit: it
/// names the `workflow` call with `name: "builtin-review"` and `args` as compact JSON and
/// tells the model how to report the run's result. The `builtin-` name keeps a project or
/// `$CODEX_HOME` script named `review` from taking the built-in's place (review B-8); a
/// model's own `{name: "review"}` call still reaches such a script. Keys of `args` keep the
/// order of `meta.args`, so the text does not depend on how JSON maps order keys.
pub fn review_turn_text(args: &Value) -> String {
    let args_json = match args {
        Value::Object(object) => ordered_object_json(object, REVIEW_ARG_ORDER),
        Value::Null => "{}".to_string(),
        other => other.to_string(),
    };
    format!(
        "Run the built-in code review workflow: call the `workflow` tool with {{\"name\":\"{BUILTIN_PREFIX}review\",\"args\":{args_json}}}; the `{BUILTIN_PREFIX}` name runs the script built into Codex even when the project or $CODEX_HOME has its own `review`. Do not review the code yourself. When the run's notification arrives, get the full result with `workflow_status` if the preview is cut, then report the findings as returned, most severe first: `[P1] <title> — <file>:<line>`, the failure scenario, and the verdict with its votes; then the summary, the omitted counts and the coverage notes. If the run fails or you do not have the `workflow` tool (it needs the workflows and builtin_workflows features and multi-agent v2), say so and stop."
    )
}

/// Compact JSON of `object` with the keys of `order` first, in that order, then the others
/// by name.
fn ordered_object_json(object: &Map<String, Value>, order: &[&str]) -> String {
    let mut keys: Vec<&String> = object.keys().collect();
    keys.sort_by_key(|key| {
        (
            order
                .iter()
                .position(|known| *known == key.as_str())
                .unwrap_or(order.len()),
            key.as_str(),
        )
    });
    let entries = keys
        .into_iter()
        .map(|key| format!("{}:{}", Value::String(key.clone()), object[key.as_str()]))
        .collect::<Vec<_>>();
    format!("{{{}}}", entries.join(","))
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
