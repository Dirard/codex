//! Fork: `codex exec review --workflow[=EFFORT]` (`specs/builtin-workflows-2026-11-07.md`,
//! UC10).
//!
//! With `--workflow` the review target becomes the args of the built-in `review` workflow
//! and exec starts an ordinary user turn with the canonical review text that `/code-review`
//! also submits (R9.4), instead of `review/start`. The run's notification starts the turn
//! whose final message exec prints, as for any workflow run (`ExecWorkflowWait`, R10.4).

use clap::ValueEnum;
use codex_core::config::Config;
use codex_features::Feature;
use codex_protocol::user_input::UserInput;
use serde_json::Map;
use serde_json::Value;

use crate::InitialOperation;
use crate::cli::ReviewArgs;

/// Effort of the built-in `review` workflow (UC5 effort table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum ReviewWorkflowEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReviewWorkflowEffort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

/// UC10, R10.1, R10.2: the initial operation and summary of `codex exec review --workflow`.
/// The caller takes this path only with the flag, so without it the upstream review runs
/// unchanged (R10.3). Missing `workflows` or `builtin_workflows` features fail before a thread
/// starts; multi-agent v2 is not checked here, since the model's defaults may select it
/// (R10.2).
pub(crate) fn fork_review_workflow_operation(
    args: &ReviewArgs,
    config: &Config,
) -> anyhow::Result<(InitialOperation, String)> {
    let effort = args.fork_workflow.unwrap_or(ReviewWorkflowEffort::Medium);
    let missing: Vec<&str> = [Feature::Workflows, Feature::BuiltinWorkflows]
        .into_iter()
        .filter(|feature| !config.features.enabled(*feature))
        .map(Feature::key)
        .collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "`codex exec review --workflow` needs the features {}; enable them with --enable {}",
            missing.join(" and "),
            missing.join(" --enable ")
        );
    }
    let prompt = args
        .prompt
        .clone()
        .map(|prompt| crate::resolve_prompt(Some(prompt)).trim().to_string());
    let review_args = review_workflow_args(args, effort, prompt);
    let summary = format!(
        "code review workflow ({}): {}",
        effort.as_str(),
        review_target_summary(&review_args)
    );
    let text = codex_workflow::review_turn_text(&review_args);
    Ok((
        InitialOperation::UserTurn {
            items: vec![UserInput::Text {
                text,
                text_elements: Vec::new(),
            }],
            output_schema: None,
        },
        summary,
    ))
}

/// R10.1: `--uncommitted`, `--base`, `--commit` (without `--title`) and the prompt become the
/// args of the built-in `review`; none of them reviews the uncommitted changes.
fn review_workflow_args(
    args: &ReviewArgs,
    effort: ReviewWorkflowEffort,
    prompt: Option<String>,
) -> Value {
    let mut review = Map::new();
    review.insert(
        "effort".to_string(),
        Value::String(effort.as_str().to_string()),
    );
    if args.uncommitted {
        review.insert("uncommitted".to_string(), Value::Bool(true));
    }
    if let Some(base) = &args.base {
        review.insert("base".to_string(), Value::String(base.clone()));
    }
    if let Some(commit) = &args.commit {
        review.insert("commit".to_string(), Value::String(commit.clone()));
    }
    if let Some(prompt) = prompt.filter(|prompt| !prompt.is_empty()) {
        review.insert("target".to_string(), Value::String(prompt));
    }
    Value::Object(review)
}

fn review_target_summary(args: &Value) -> String {
    if let Some(base) = args.get("base").and_then(Value::as_str) {
        format!("changes against {base}")
    } else if let Some(commit) = args.get("commit").and_then(Value::as_str) {
        format!("commit {commit}")
    } else if let Some(target) = args.get("target").and_then(Value::as_str) {
        target.to_string()
    } else {
        "uncommitted changes".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[derive(Debug, Parser)]
    struct ReviewCli {
        #[clap(flatten)]
        args: ReviewArgs,
    }

    fn parse(arguments: &[&str]) -> ReviewArgs {
        ReviewCli::try_parse_from(std::iter::once("review").chain(arguments.iter().copied()))
            .expect("parse")
            .args
    }

    fn workflow_args(arguments: &[&str]) -> Option<Value> {
        let args = parse(arguments);
        args.fork_workflow
            .map(|effort| review_workflow_args(&args, effort, args.prompt.clone()))
    }

    /// R10.1, R10.3: the flag's value needs `=`; without a value the effort is medium; the
    /// prompt after `--workflow` stays the prompt; without the flag nothing changes.
    #[test]
    fn workflow_flag_maps_the_review_target() {
        assert_eq!(
            workflow_args(&["--workflow=high", "--base", "main"]),
            Some(json!({ "effort": "high", "base": "main" }))
        );
        assert_eq!(
            workflow_args(&["--workflow"]),
            Some(json!({ "effort": "medium" }))
        );
        assert_eq!(
            workflow_args(&["--workflow", "focus on auth"]),
            Some(json!({ "effort": "medium", "target": "focus on auth" }))
        );
        assert_eq!(
            workflow_args(&["--workflow=max", "--commit", "abc", "--title", "Fix"]),
            Some(json!({ "effort": "max", "commit": "abc" }))
        );
        assert_eq!(
            workflow_args(&["--uncommitted", "--workflow=low"]),
            Some(json!({ "effort": "low", "uncommitted": true }))
        );
        assert_eq!(workflow_args(&["--uncommitted"]), None);
        assert!(
            ReviewCli::try_parse_from(["review", "--workflow=huge"]).is_err(),
            "unknown effort"
        );
    }

    /// R10.2: without `workflows` or `builtin_workflows` the flag fails with the missing
    /// features; with both it starts a user turn with the canonical text.
    #[tokio::test]
    async fn workflow_flag_needs_both_features() {
        let home = tempfile::TempDir::new().expect("home");
        let mut config = codex_core::config::ConfigBuilder::default()
            .codex_home(home.path().to_path_buf())
            .fallback_cwd(Some(home.path().to_path_buf()))
            .build()
            .await
            .expect("config");
        let args = parse(&["--workflow=high", "--base", "main"]);
        let error = fork_review_workflow_operation(&args, &config)
            .err()
            .expect("features missing")
            .to_string();
        assert!(error.contains("workflows and builtin_workflows"), "{error}");
        config.features.enable(Feature::Workflows).expect("feature");
        let error = fork_review_workflow_operation(&args, &config)
            .err()
            .expect("builtin_workflows missing")
            .to_string();
        assert!(
            error.contains("needs the features builtin_workflows;"),
            "{error}"
        );
        config
            .features
            .enable(Feature::BuiltinWorkflows)
            .expect("feature");
        let (
            InitialOperation::UserTurn {
                items,
                output_schema,
            },
            summary,
        ) = fork_review_workflow_operation(&args, &config).expect("operation")
        else {
            panic!("a user turn");
        };
        assert_eq!(output_schema, None);
        assert_eq!(
            items,
            vec![UserInput::Text {
                text: codex_workflow::review_turn_text(
                    &json!({ "effort": "high", "base": "main" })
                ),
                text_elements: Vec::new(),
            }]
        );
        assert_eq!(summary, "code review workflow (high): changes against main");
    }
}
