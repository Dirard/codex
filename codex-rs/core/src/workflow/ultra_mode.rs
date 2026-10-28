//! Workflow ultra mode (`specs/fork-workflows-ultra-mode-2026-10-28.md`).
//!
//! With reasoning effort `ultra` a root thread that has the `workflow` tool is told to run a
//! workflow for every substantive task, with the quality patterns of the tool description
//! used by default. The mode is a world-state section, so it survives compaction and resume,
//! and leaving it adds one message that takes it back. Approvals, budgets, limits and the
//! tool description stay as configured (R1.6).

use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;
use codex_protocol::openai_models::ReasoningEffort;
use codex_tools::ToolName;
use serde::Deserialize;
use serde::Serialize;

use super::tools::WORKFLOW_TOOL;
use crate::context::ContextualUserFragment;
use crate::context::world_state::PreviousSectionState;
use crate::context::world_state::SectionTransition;
use crate::context::world_state::WorldStateSection;
use crate::context::world_state::WorldStateUpdate;
use crate::session::step_context::StepContext;

const OPEN_MARKER: &str = "<workflow_ultra_mode>";
const CLOSE_MARKER: &str = "</workflow_ultra_mode>";
/// Opening words of the enabling message, which tell it apart from the disabling one in
/// retained history.
const ENABLED_LEAD: &str = "Workflow ultra mode is active";

/// R1.5: the enabling message.
const ENABLED_TEXT: &str = "Workflow ultra mode is active because reasoning effort is ultra. It stays active until a later workflow mode message changes it; explicit user requests override it.

Use the `workflow` tool by default for every substantive task, not only for large fan-outs: aim for the most thorough, correct result, and treat token cost as no constraint unless the user set a budget. Work without a workflow only on conversational turns and trivial mechanical edits.
- Scout inline first (list files, scope the change, find the items), then start a workflow over what you found. For multi-phase work run several workflows in sequence (understand, design, implement, review) and read each result before starting the next.
- Make results trustworthy: have independent skeptics try to refute each finding and keep it only if most fail; give verifiers different lenses (correctness, security, does it reproduce) when a finding can fail in several ways; repeat finding rounds until two rounds bring nothing new; finish with a completeness critic that asks what is missing.
- No silent caps: when a script samples or limits coverage, `log()` what was left out.
- Prefer `pipeline()`; use a barrier only when a stage needs all earlier results together, such as deduplication.
- Workflow agents start without this conversation: put the context each one needs into its prompt.";

/// R1.2, R1.3: the disabling message.
const DISABLED_TEXT: &str = "Workflow ultra mode is no longer active. Start workflows only when the `workflow` tool description says they fit or the user asks for one.";

/// Use case "Включать и снимать ультра-режим workflows по effort"
/// (`specs/fork-workflows-ultra-mode-2026-10-28.md`): whether workflow ultra mode is on for
/// the step. The section adds the enabling message when the mode turns on, or when history no
/// longer shows it while it is on, and one disabling message when it turns off (R1.1–R1.3).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct WorkflowUltraModeState {
    enabled: bool,
}

impl WorkflowUltraModeState {
    /// R1.1, R1.4: on when the step's effort is `ultra`, its tools include `workflow` (root
    /// threads with MultiAgentV2, the local agent backend and a Code Mode host) and
    /// `[workflows] ultra_mode` is true.
    pub(crate) fn new(step_context: &StepContext) -> Self {
        Self {
            enabled: step_context.turn.config.workflows.ultra_mode
                && step_context.settings.effective_reasoning_effort()
                    == Some(ReasoningEffort::Ultra)
                && step_context
                    .tool_router
                    .exposes_tool(&ToolName::plain(WORKFLOW_TOOL)),
        }
    }
}

/// R1.3: whether `text` is the enabling message, the only fragment of this section that
/// shows the mode as on.
fn is_enabling_fragment(role: &str, text: &str) -> bool {
    role == "developer"
        && WorkflowUltraModeInstructions::matches_text(text)
        && text
            .trim_start()
            .get(OPEN_MARKER.len()..)
            .is_some_and(|body| body.trim_start().starts_with(ENABLED_LEAD))
}

/// The developer message of the mode; its markers let retained history recognize it after
/// compaction or resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkflowUltraModeInstructions {
    enabled: bool,
}

impl ContextualUserFragment for WorkflowUltraModeInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("workflow.ultra_mode".to_string())
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
        let text = if self.enabled {
            ENABLED_TEXT
        } else {
            DISABLED_TEXT
        };
        input_text(format!("\n{text}\n"))
    }
}

impl WorldStateSection for WorkflowUltraModeState {
    const ID: &'static str = "workflow_ultra_mode";
    type Snapshot = Self;

    /// R1.3: without a stored snapshot only the enabling message makes the previous state
    /// `Unknown`; a disabling message alone leaves nothing to take back.
    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        is_enabling_fragment(role, text)
    }

    /// An enabled snapshot counts only while history still shows the enabling message, so
    /// after compaction drops it the mode is announced again.
    fn has_retained_fragment_matcher() -> bool {
        true
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        is_enabling_fragment(role, text)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> SectionTransition<Self::Snapshot> {
        let message = match previous {
            PreviousSectionState::Known(previous) if previous.enabled == self.enabled => None,
            // R1.1: on, and the model does not see it as on.
            _ if self.enabled => Some(true),
            // R1.2, R1.3: off after the model saw it on.
            PreviousSectionState::Known(_) | PreviousSectionState::Unknown => Some(false),
            PreviousSectionState::Absent => None,
        };
        let updates = message
            .map(|enabled| {
                vec![WorldStateUpdate::fragment(WorkflowUltraModeInstructions {
                    enabled,
                })]
            })
            .unwrap_or_default();
        (Some(self.clone()), updates)
    }
}

#[cfg(test)]
#[path = "ultra_mode_tests.rs"]
mod tests;
