//! Fork: selection views of the `/workflows` screen (`specs/dynamic-workflow-2026-10-25.md`,
//! UC 8.1, R8.6.2 to R8.6.5).
//!
//! Both views are plain `ListSelectionView`s. The run list keeps itself open when a row opens a
//! run, so the run view stacks above it and Esc returns to the list. Builders take the clock as
//! `now_ms` so tests render fixed durations.

use std::collections::BTreeSet;

use codex_app_server_protocol::WorkflowAgent;
use codex_app_server_protocol::WorkflowAgentState;
use codex_app_server_protocol::WorkflowRun;
use codex_app_server_protocol::WorkflowRunStatus;
use codex_protocol::ThreadId;
use crossterm::event::KeyCode;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;

use super::WorkflowsEvent;
use crate::app_event::AppEvent;
use crate::bottom_pane::SelectionAction;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionSecondaryAction;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;
use crate::key_hint;
use crate::status::format_tokens_compact;
use crate::status_indicator_widget::fmt_elapsed_compact;
use crate::style::StatusTone;
use crate::style::accent_style;
use crate::style::status_style;

pub(crate) const RUN_LIST_VIEW_ID: &str = "workflow-runs";
pub(crate) const RUN_VIEW_ID: &str = "workflow-run";

/// Log lines of the run view header (Q39 of the spec).
const LOG_LINES: usize = 5;
/// Width of the header labels such as `Phases` and `Tokens`.
const LABEL_WIDTH: usize = 9;

/// A row of the run view, used to keep the selection when the view is rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunViewRow {
    Cancel,
    Agent(u64),
    NoAgents,
}

/// R8.6.2: runs newest first; equal start times keep run id order.
pub(crate) fn sorted_runs<'a>(
    runs: impl IntoIterator<Item = &'a WorkflowRun>,
) -> Vec<&'a WorkflowRun> {
    let mut runs: Vec<&WorkflowRun> = runs.into_iter().collect();
    runs.sort_by(|left, right| {
        right
            .started_at_ms
            .cmp(&left.started_at_ms)
            .then_with(|| left.run_id.cmp(&right.run_id))
    });
    runs
}

/// UC 8.1, "список run'ов": one row per run in the given order; Enter opens the run and `x`
/// cancels an active one (R8.6.2, R8.6.5).
pub(crate) fn run_list_params(
    thread_id: ThreadId,
    runs: &[&WorkflowRun],
    cancelling: &BTreeSet<String>,
    now_ms: i64,
    selected: Option<usize>,
) -> SelectionViewParams {
    let items = runs
        .iter()
        .map(|run| run_list_item(thread_id, run, cancelling.contains(&run.run_id), now_ms))
        .collect();
    SelectionViewParams {
        view_id: Some(RUN_LIST_VIEW_ID),
        title: Some("Workflows".to_string()),
        subtitle: Some("Runs of this thread, newest first.".to_string()),
        footer_hint: Some(standard_popup_hint_line()),
        items,
        initial_selected_idx: selected,
        ..SelectionViewParams::picker()
    }
}

fn run_list_item(
    thread_id: ThreadId,
    run: &WorkflowRun,
    cancelling: bool,
    now_ms: i64,
) -> SelectionItem {
    let open_run_id = run.run_id.clone();
    let open: SelectionAction = Box::new(move |tx| {
        tx.send(AppEvent::Workflows(WorkflowsEvent::OpenRun {
            thread_id,
            run_id: open_run_id.clone(),
        }));
    });
    let secondary_action = (!run.status.is_terminal() && !cancelling).then(|| {
        let run_id = run.run_id.clone();
        SelectionSecondaryAction {
            key: key_hint::plain(KeyCode::Char('x')),
            action: Box::new(move |tx| {
                tx.send(AppEvent::Workflows(WorkflowsEvent::Cancel {
                    thread_id,
                    run_id: run_id.clone(),
                }));
            }),
            footer_hint: active_run_hint(),
        }
    });
    SelectionItem {
        name: one_line(&run.name),
        name_prefix_spans: vec![run_mark(run.status), " ".into()],
        description: Some(run_row_description(run, cancelling, now_ms)),
        actions: vec![open],
        secondary_action,
        dismiss_on_select: false,
        dismiss_parent_on_child_accept: true,
        ..Default::default()
    }
}

fn active_run_hint() -> Line<'static> {
    Line::from(vec![
        key_hint::plain(KeyCode::Enter).into(),
        " open · ".into(),
        key_hint::plain(KeyCode::Char('x')).into(),
        " cancel run · ".into(),
        key_hint::plain(KeyCode::Esc).into(),
        " close".into(),
    ])
}

fn run_row_description(run: &WorkflowRun, cancelling: bool, now_ms: i64) -> String {
    let mut parts = vec![run.run_id.clone(), status_text(run.status, cancelling)];
    if !run.status.is_terminal()
        && let Some(phase) = run.current_phase.as_deref()
    {
        parts.push(one_line(phase));
    }
    parts.push(agent_summary(run));
    parts.push(run_duration(run, now_ms));
    parts.join(" · ")
}

/// `3 running, 5 done` for an active run, `12 agents (1 null)` for a finished one.
fn agent_summary(run: &WorkflowRun) -> String {
    let counts = &run.agent_counts;
    if !run.status.is_terminal() {
        return format!(
            "{} running, {} done",
            counts.running + counts.awaiting_approval,
            counts.completed + counts.null_results
        );
    }
    let total = counts.queued
        + counts.running
        + counts.awaiting_approval
        + counts.completed
        + counts.null_results;
    let mut summary = format!("{total} {}", plural(total, "agent", "agents"));
    if counts.null_results > 0 {
        summary.push_str(&format!(" ({} null)", counts.null_results));
    }
    summary
}

/// UC 8.1, "карточка run'а": header with phases, agents, tokens, resume, error, result and
/// log, a cancel row for an active run and one row per `agent()` call (R8.6.3 to R8.6.5).
pub(crate) fn run_view_params(
    thread_id: ThreadId,
    run: &WorkflowRun,
    cancelling: bool,
    now_ms: i64,
    selected: Option<usize>,
) -> SelectionViewParams {
    let mut items = Vec::new();
    for row in run_view_rows(run) {
        items.push(match row {
            RunViewRow::Cancel => cancel_item(thread_id, run, cancelling),
            RunViewRow::Agent(seq) => {
                let agent = run.agents.iter().flatten().find(|agent| agent.seq == seq);
                match agent {
                    Some(agent) => agent_item(agent),
                    None => continue,
                }
            }
            RunViewRow::NoAgents => SelectionItem {
                name: if run.status.is_terminal() {
                    "No agents".to_string()
                } else {
                    "No agents yet".to_string()
                },
                is_disabled: true,
                ..Default::default()
            },
        });
    }
    // The title goes into the header: `ListSelectionView` puts its own title below the header.
    let mut header = vec![
        Line::from(format!("Workflow {}", one_line(&run.name)).bold()),
        Line::from(run_subtitle(run, cancelling, now_ms).dim()),
        Line::default(),
    ];
    header.extend(run_header_lines(run));
    SelectionViewParams {
        view_id: Some(RUN_VIEW_ID),
        header: Box::new(Paragraph::new(header).wrap(Wrap { trim: false })),
        footer_hint: Some(standard_popup_hint_line()),
        items,
        initial_selected_idx: selected,
        ..SelectionViewParams::picker()
    }
}

/// `wf-1a2b3c4d · running · 1m 20s`.
fn run_subtitle(run: &WorkflowRun, cancelling: bool, now_ms: i64) -> String {
    format!(
        "{} · {} · {}",
        run.run_id,
        status_text(run.status, cancelling),
        run_duration(run, now_ms)
    )
}

/// Rows of the run view in display order.
pub(crate) fn run_view_rows(run: &WorkflowRun) -> Vec<RunViewRow> {
    let mut rows = Vec::new();
    if !run.status.is_terminal() {
        rows.push(RunViewRow::Cancel);
    }
    let agents = run.agents.as_deref().unwrap_or_default();
    if agents.is_empty() {
        rows.push(RunViewRow::NoAgents);
    }
    rows.extend(agents.iter().map(|agent| RunViewRow::Agent(agent.seq)));
    rows
}

fn cancel_item(thread_id: ThreadId, run: &WorkflowRun, cancelling: bool) -> SelectionItem {
    if cancelling {
        return SelectionItem {
            name: "Cancelling…".to_string(),
            description: Some("waiting for its agents to stop".to_string()),
            is_disabled: true,
            ..Default::default()
        };
    }
    let run_id = run.run_id.clone();
    SelectionItem {
        name: "Cancel run".to_string(),
        description: Some("interrupt its agents and stop the script".to_string()),
        actions: vec![Box::new(move |tx| {
            tx.send(AppEvent::Workflows(WorkflowsEvent::Cancel {
                thread_id,
                run_id: run_id.clone(),
            }));
        })],
        dismiss_on_select: false,
        ..Default::default()
    }
}

/// R8.6.4: an agent row opens the agent's transcript; a call without a thread is disabled.
fn agent_item(agent: &WorkflowAgent) -> SelectionItem {
    let name = match agent.label.as_deref().map(one_line) {
        Some(label) if !label.is_empty() => format!("#{} {label}", agent.seq),
        _ => format!("#{}", agent.seq),
    };
    let target = agent
        .thread_id
        .as_deref()
        .and_then(|thread_id| ThreadId::from_string(thread_id).ok());
    let actions: Vec<SelectionAction> = match target {
        Some(thread_id) => vec![Box::new(move |tx| {
            tx.send(AppEvent::SelectAgentThread(thread_id));
        })],
        None => Vec::new(),
    };
    SelectionItem {
        name,
        name_prefix_spans: vec![agent_mark(agent.state), " ".into()],
        description: Some(agent_description(agent)),
        is_disabled: target.is_none(),
        actions,
        dismiss_on_select: true,
        ..Default::default()
    }
}

fn agent_description(agent: &WorkflowAgent) -> String {
    let mut parts = Vec::new();
    if let Some(phase) = agent.phase.as_deref() {
        parts.push(one_line(phase));
    }
    parts.push(match agent.state {
        WorkflowAgentState::Queued => "queued".to_string(),
        WorkflowAgentState::Running => "running".to_string(),
        WorkflowAgentState::AwaitingApproval => "awaiting approval".to_string(),
        WorkflowAgentState::Completed => "done".to_string(),
        WorkflowAgentState::Null => match agent.reason.as_deref() {
            Some(reason) => format!("null: {}", one_line(reason)),
            None => "null".to_string(),
        },
    });
    if agent.cached {
        parts.push("from journal".to_string());
    } else if agent.charged_tokens > 0 {
        parts.push(format!("{} tokens", tokens(agent.charged_tokens)));
    }
    parts.join(" · ")
}

fn run_header_lines(run: &WorkflowRun) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let description = one_line(&run.description);
    if !description.is_empty() {
        lines.push(Line::from(description).dim());
    }
    if !run.phases.is_empty() {
        lines.push(labeled("Phases", phases_text(run)));
    }
    lines.push(labeled("Agents", agents_text(run)));
    lines.push(labeled("Tokens", tokens_text(run)));
    if let Some(resume) = &run.resume {
        lines.push(labeled(
            "Resumed",
            format!(
                "from {}: {} of {} journaled results reused, {} unused",
                resume.from, resume.served, resume.source_results, resume.unused
            ),
        ));
    }
    if let Some(error) = run.error.as_deref().and_then(first_line) {
        lines.push(labeled("Error", error));
    }
    if let Some(result) = run.result_preview.as_deref().and_then(first_line) {
        lines.push(labeled("Result", result));
    }
    let skip = run.logs_tail.len().saturating_sub(LOG_LINES);
    for (index, log) in run.logs_tail.iter().skip(skip).enumerate() {
        lines.push(labeled(if index == 0 { "Log" } else { "" }, one_line(log)));
    }
    lines
}

fn labeled(label: &str, text: String) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<LABEL_WIDTH$}")).dim(),
        Span::from(text),
    ])
}

/// `✓ Find 3/3 · ▸ Verify 2/5 · Synthesize`: finished and started agents per phase.
fn phases_text(run: &WorkflowRun) -> String {
    run.phases
        .iter()
        .map(|phase| {
            let current = !run.status.is_terminal()
                && run.current_phase.as_deref() == Some(phase.title.as_str());
            let mark = if current {
                "▸ "
            } else if phase.started > 0 && phase.finished == phase.started {
                "✓ "
            } else {
                ""
            };
            let counts = if phase.started > 0 {
                format!(" {}/{}", phase.finished, phase.started)
            } else {
                String::new()
            };
            format!("{mark}{}{counts}", one_line(&phase.title))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn agents_text(run: &WorkflowRun) -> String {
    let counts = &run.agent_counts;
    let mut parts = Vec::new();
    for (count, label) in [
        (counts.queued, "queued"),
        (counts.running, "running"),
        (counts.awaiting_approval, "awaiting approval"),
        (counts.completed, "done"),
    ] {
        if count > 0 {
            parts.push(format!("{count} {label}"));
        }
    }
    if counts.null_results > 0 {
        let reasons = run
            .null_reasons
            .iter()
            .map(|(reason, count)| format!("{} {count}", one_line(reason)))
            .collect::<Vec<_>>();
        if reasons.is_empty() {
            parts.push(format!("{} null", counts.null_results));
        } else {
            parts.push(format!(
                "{} null ({})",
                counts.null_results,
                reasons.join(", ")
            ));
        }
    }
    if parts.is_empty() {
        return "none yet".to_string();
    }
    parts.join(" · ")
}

fn tokens_text(run: &WorkflowRun) -> String {
    let mut text = format!(
        "{} charged · {} cached",
        tokens(run.charged_tokens),
        tokens(run.cached_tokens)
    );
    if let Some(budget) = run.budget_tokens {
        text.push_str(&format!(" · budget {}", tokens(budget)));
    }
    text
}

fn tokens(value: u64) -> String {
    format_tokens_compact(i64::try_from(value).unwrap_or(i64::MAX))
}

/// Run duration; an active run counts up to `now_ms`.
pub(crate) fn run_duration(run: &WorkflowRun, now_ms: i64) -> String {
    let end = run.finished_at_ms.unwrap_or(now_ms);
    let elapsed_ms = u64::try_from(end.saturating_sub(run.started_at_ms)).unwrap_or(0);
    fmt_elapsed_compact(elapsed_ms / 1000)
}

pub(crate) fn status_text(status: WorkflowRunStatus, cancelling: bool) -> String {
    if cancelling && !status.is_terminal() {
        return "cancelling…".to_string();
    }
    match status {
        WorkflowRunStatus::Running => "running",
        WorkflowRunStatus::Completed => "completed",
        WorkflowRunStatus::Failed => "failed",
        WorkflowRunStatus::Cancelled => "cancelled",
        WorkflowRunStatus::Interrupted => "interrupted",
    }
    .to_string()
}

/// Status mark of a run (spec data `status_marks`).
pub(crate) fn run_mark(status: WorkflowRunStatus) -> Span<'static> {
    match status {
        WorkflowRunStatus::Running => Span::styled("●", accent_style()),
        WorkflowRunStatus::Completed => Span::styled("✓", status_style(StatusTone::Success)),
        WorkflowRunStatus::Failed => Span::styled("✗", status_style(StatusTone::Failure)),
        WorkflowRunStatus::Cancelled => "■".dim(),
        WorkflowRunStatus::Interrupted => Span::styled("!", status_style(StatusTone::Attention)),
    }
}

fn agent_mark(state: WorkflowAgentState) -> Span<'static> {
    match state {
        WorkflowAgentState::Queued => "○".dim(),
        WorkflowAgentState::Running => Span::styled("●", accent_style()),
        WorkflowAgentState::AwaitingApproval => {
            Span::styled("?", status_style(StatusTone::Attention))
        }
        WorkflowAgentState::Completed => Span::styled("✓", status_style(StatusTone::Success)),
        WorkflowAgentState::Null => Span::styled("✗", status_style(StatusTone::Failure)),
    }
}

/// Script-provided text (names, phases, labels, logs) on one line.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_line(text: &str) -> Option<String> {
    text.lines().map(one_line).find(|line| !line.is_empty())
}

fn plural<'a>(count: u64, one: &'a str, many: &'a str) -> &'a str {
    if count == 1 { one } else { many }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
