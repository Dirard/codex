//! Fork: `/subagents` rows of workflow agents grouped under their run
//! (`specs/dynamic-workflow-2026-10-25.md`, UC 8.1, R8.6.7).
//!
//! The upstream picker shows `visible_threads()` one row per thread and maps a row number to a
//! thread positionally. `PickerLayout` is the display order with run rows inserted; it exists
//! only when the picker holds workflow agents, so without them the upstream path runs
//! unchanged.

use std::collections::HashSet;

use ratatui::text::Span;

use crate::bottom_pane::SelectionItem;

/// A workflow agent, named `wf_<run>_<seq>[_<label>]` by `codex_workflow::child_agent_name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowChild {
    /// Run id, `wf-` and eight hex digits.
    pub(crate) run_id: String,
    /// Call number of the agent in its run.
    pub(crate) seq: u64,
    pub(crate) label: Option<String>,
}

impl WorkflowChild {
    /// Row name under the run row: `#<seq> <label>`.
    pub(crate) fn row_name(&self) -> String {
        match self.label.as_deref() {
            Some(label) => format!("#{} {label}", self.seq),
            None => format!("#{}", self.seq),
        }
    }
}

/// The workflow agent named by the last segment of `agent_path`, if it is one.
pub(crate) fn parse_workflow_child(agent_path: &str) -> Option<WorkflowChild> {
    let segment = agent_path.trim().rsplit('/').next()?;
    let rest = segment.strip_prefix("wf_")?;
    let (run, rest) = rest.split_once('_')?;
    if run.len() != 8
        || !run
            .chars()
            .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
    {
        return None;
    }
    let (seq, label) = match rest.split_once('_') {
        Some((seq, label)) => (seq, Some(label)),
        None => (rest, None),
    };
    if seq.is_empty() || !seq.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if label.is_some_and(str::is_empty) {
        return None;
    }
    Some(WorkflowChild {
        run_id: format!("wf-{run}"),
        seq: seq.parse().ok()?,
        label: label.map(str::to_string),
    })
}

/// One row of the grouped picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PickerRow {
    /// The thread at this index of `visible_threads()`.
    Thread(usize),
    /// A run and how many of its agents the picker holds.
    Run { run_id: String, children: usize },
}

/// Display order of a picker that holds workflow agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickerLayout {
    rows: Vec<PickerRow>,
    children: Vec<Option<WorkflowChild>>,
}

impl PickerLayout {
    /// Groups the workflow agents among `agent_paths` (in `visible_threads()` order) under a
    /// run row placed where the run's first agent was; other rows keep their order. Returns
    /// `None` when no path names a workflow agent.
    pub(crate) fn build<'a>(
        agent_paths: impl IntoIterator<Item = Option<&'a str>>,
    ) -> Option<Self> {
        let children: Vec<Option<WorkflowChild>> = agent_paths
            .into_iter()
            .map(|path| path.and_then(parse_workflow_child))
            .collect();
        if children.iter().all(Option::is_none) {
            return None;
        }
        let mut rows = Vec::with_capacity(children.len());
        let mut placed = HashSet::new();
        for (index, child) in children.iter().enumerate() {
            let Some(child) = child else {
                rows.push(PickerRow::Thread(index));
                continue;
            };
            if !placed.insert(child.run_id.as_str()) {
                continue;
            }
            let members: Vec<usize> = children
                .iter()
                .enumerate()
                .filter(|(_, other)| {
                    other
                        .as_ref()
                        .is_some_and(|other| other.run_id == child.run_id)
                })
                .map(|(member, _)| member)
                .collect();
            rows.push(PickerRow::Run {
                run_id: child.run_id.clone(),
                children: members.len(),
            });
            rows.extend(members.into_iter().map(PickerRow::Thread));
        }
        Some(Self { rows, children })
    }

    #[cfg(test)]
    pub(crate) fn rows(&self) -> &[PickerRow] {
        &self.rows
    }

    /// The `visible_threads()` index shown at row `display`; `None` for a run row.
    pub(crate) fn visible_index(&self, display: usize) -> Option<usize> {
        match self.rows.get(display)? {
            PickerRow::Thread(index) => Some(*index),
            PickerRow::Run { .. } => None,
        }
    }

    /// The row that shows `visible_threads()[visible]`.
    pub(crate) fn display_index(&self, visible: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| *row == PickerRow::Thread(visible))
    }

    /// The run shown at row `display`; `None` for a thread row.
    pub(crate) fn run_at(&self, display: usize) -> Option<&str> {
        match self.rows.get(display)? {
            PickerRow::Run { run_id, .. } => Some(run_id),
            PickerRow::Thread(_) => None,
        }
    }

    /// The row of run `run_id`, if the picker still holds one of its agents.
    pub(crate) fn run_row(&self, run_id: &str) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, PickerRow::Run { run_id: id, .. } if id == run_id))
    }

    /// Reorders `items` (one per visible thread, in `visible_threads()` order) into display
    /// order: run rows come from `run_row`, workflow agents are indented and named
    /// `#<seq> <label>`.
    pub(crate) fn group_items(
        &self,
        items: Vec<SelectionItem>,
        mut run_row: impl FnMut(&str, usize) -> SelectionItem,
    ) -> Vec<SelectionItem> {
        let mut slots: Vec<Option<SelectionItem>> = items.into_iter().map(Some).collect();
        let mut grouped = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            match row {
                PickerRow::Thread(index) => {
                    let Some(mut item) = slots.get_mut(*index).and_then(Option::take) else {
                        continue;
                    };
                    if let Some(child) = self.children.get(*index).and_then(Option::as_ref) {
                        item.name = child.row_name();
                        let mut prefix: Vec<Span<'static>> = vec!["  ".into()];
                        prefix.append(&mut item.name_prefix_spans);
                        item.name_prefix_spans = prefix;
                    }
                    grouped.push(item);
                }
                PickerRow::Run { run_id, children } => grouped.push(run_row(run_id, *children)),
            }
        }
        grouped
    }
}

#[cfg(test)]
#[path = "picker_tests.rs"]
mod tests;
