//! World state section listing a thread's active workflow runs (UC11, R11.2).
//!
//! The section re-renders when the set of runs, their status or their phase changes and
//! after compaction drops its fragment, so the model keeps knowing which runs will notify it.
//! Agent counts are left to `workflow_status`: they change on every agent and would add a
//! fragment to the context on most steps.

use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_text;
use serde::Deserialize;
use serde::Serialize;

use super::registry::WorkflowRegistry;
use crate::context::ContextualUserFragment;
use crate::context::world_state::PreviousSectionState;
use crate::context::world_state::SectionTransition;
use crate::context::world_state::WorldStateSection;
use crate::context::world_state::WorldStateUpdate;
use crate::session::session::Session;

/// Token bound of the rendered section (R11.2).
const MAX_SECTION_TOKENS: usize = 300;
const OPEN_MARKER: &str = "<workflow_runs>";
const CLOSE_MARKER: &str = "</workflow_runs>";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct WorkflowRunLine {
    run_id: String,
    name: String,
    status: String,
    phase: Option<String>,
}

/// Active runs of a thread as the model sees them.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct WorkflowRunsState {
    runs: Vec<WorkflowRunLine>,
}

impl WorkflowRunsState {
    /// UC11: the non-terminal runs of `session`.
    pub(crate) fn for_session(session: &Session) -> Self {
        let runs = WorkflowRegistry::existing(session)
            .map(|registry| {
                registry
                    .all()
                    .iter()
                    .map(|run| run.snapshot(/*include_agents*/ false))
                    .filter(|snapshot| !snapshot.status.is_terminal())
                    .map(|snapshot| WorkflowRunLine {
                        run_id: snapshot.run_id,
                        name: snapshot.name,
                        status: snapshot.status.as_str().to_string(),
                        phase: snapshot.current_phase,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { runs }
    }

    fn render(&self) -> String {
        if self.runs.is_empty() {
            return "No workflow runs are active.".to_string();
        }
        let mut text = String::from(
            "Active workflow runs (each sends a workflow notification when it ends; do not poll):",
        );
        for run in &self.runs {
            text.push_str(&format!(
                "\n- {} \"{}\": {}",
                run.run_id, run.name, run.status
            ));
            if let Some(phase) = &run.phase {
                text.push_str(&format!(", phase {phase}"));
            }
        }
        truncate_text(&text, TruncationPolicy::Tokens(MAX_SECTION_TOKENS))
    }
}

struct WorkflowRunsFragment {
    body: String,
}

impl ContextualUserFragment for WorkflowRunsFragment {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("workflow.active_runs".to_string())
    }

    fn role(&self) -> &'static str {
        "developer"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        (OPEN_MARKER, CLOSE_MARKER)
    }

    fn body(&self) -> ContentItem {
        input_text(format!("\n{}\n", self.body))
    }
}

impl WorldStateSection for WorkflowRunsState {
    const ID: &'static str = "workflow_runs";
    type Snapshot = Self;

    fn has_retained_fragment_matcher() -> bool {
        true
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        role == "developer" && text.trim_start().starts_with(OPEN_MARKER)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> SectionTransition<Self::Snapshot> {
        let changed = match previous {
            PreviousSectionState::Known(previous) => previous != self,
            // Nothing to announce until a run exists; after compaction, announce again.
            PreviousSectionState::Absent => !self.runs.is_empty(),
            PreviousSectionState::Unknown => true,
        };
        if !changed {
            return (Some(self.clone()), Vec::new());
        }
        (
            Some(self.clone()),
            vec![WorldStateUpdate::fragment(WorkflowRunsFragment {
                body: self.render(),
            })],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn state(phase: Option<&str>) -> WorkflowRunsState {
        WorkflowRunsState {
            runs: vec![WorkflowRunLine {
                run_id: "wf-0000abcd".to_string(),
                name: "find-bugs".to_string(),
                status: "running".to_string(),
                phase: phase.map(str::to_string),
            }],
        }
    }

    #[test]
    fn renders_only_on_change() {
        let current = state(Some("Verify"));
        let (_, updates) = current.render_diff(PreviousSectionState::Absent);
        assert_eq!(updates.len(), 1);
        let (_, updates) = current.render_diff(PreviousSectionState::Known(&current));
        assert!(updates.is_empty());
        let (_, updates) = current.render_diff(PreviousSectionState::Known(&state(Some("Find"))));
        assert_eq!(updates.len(), 1);
        let (_, updates) = current.render_diff(PreviousSectionState::Unknown);
        assert_eq!(updates.len(), 1);
        let (_, updates) = WorkflowRunsState::default().render_diff(PreviousSectionState::Absent);
        assert!(updates.is_empty());
        assert_eq!(
            current.render(),
            "Active workflow runs (each sends a workflow notification when it ends; do not poll):\n- wf-0000abcd \"find-bugs\": running, phase Verify"
        );
        assert_eq!(
            WorkflowRunsState::default().render(),
            "No workflow runs are active."
        );
    }
}
