//! The world-state section `output_style` (UC3).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use codex_features::Feature;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::input_text;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use super::OutputStyle;
use super::OutputStyleSelection;
use crate::config::Config;
use crate::context::ContextualUserFragment;
use crate::context::world_state::PreviousSectionState;
use crate::context::world_state::SectionTransition;
use crate::context::world_state::WorldState;
use crate::context::world_state::WorldStateHash;
use crate::context::world_state::WorldStateSection;
use crate::context::world_state::WorldStateUpdate;
use crate::session::session::Session;
use crate::session::step_context::StepContext;

const OPEN_MARKER: &str = "<output_style>";
const CLOSE_MARKER: &str = "</output_style>";

/// Opening words of the default notice, which tell it apart from the messages that show a
/// style as active.
const DEFAULT_NOTICE_LEAD: &str = "The default output style is active again.";

/// R3.2, R3.5–R3.7: the message that takes a style back.
const DEFAULT_NOTICE: &str = "The default output style is active again. Earlier output style instructions no longer apply; communicate as your base instructions describe.";

/// R3.1: what a style governs and what it does not change.
const SCOPE: &str = "This output style governs how you communicate with the user in this thread: what you say before and between tool calls, progress updates, the final message, tone, and code comments. Where it conflicts with earlier guidance on these topics, follow this style. It does not change the task, how you use tools, safety and approval rules, the language you write in, the rules for formatting file references and code, formats required by the current collaboration mode, or output that must match a schema. Text that is not addressed to the user, such as tool arguments, prompts for other agents and compaction summaries, keeps its own format.";

/// The thread's selection made through `CodexThread::set_output_style` (UC4); it wins over
/// the config while the thread stays loaded and is never written to rollout (R4.5).
#[derive(Debug, Default)]
pub(crate) struct OutputStyleOverride {
    selection: Mutex<Option<OutputStyleSelection>>,
}

impl OutputStyleOverride {
    pub(crate) fn get(&self) -> Option<OutputStyleSelection> {
        self.selection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set(&self, selection: OutputStyleSelection) {
        *self
            .selection
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(selection);
    }
}

/// The style the model should follow in `session` now: the thread's selection, else the
/// selection of `config`, the thread's config (R4.2). Callers apply R3.5 themselves.
pub(crate) fn selection_for_session(session: &Session, config: &Config) -> OutputStyleSelection {
    session
        .services
        .thread_extension_data
        .get::<OutputStyleOverride>()
        .and_then(|selection| selection.get())
        .unwrap_or_else(|| config.output_style.selection.clone())
}

/// Use case 3 "Стиль в контексте модели: раздел world-state `output_style`"
/// (`specs/output-styles-2026-11-07.md`): adds the section to a step's world state while the
/// feature is enabled, or while the previous snapshot still shows an active style so that the
/// model is told it no longer applies (R3.7). Non-root agents always get `default` (R3.5).
pub(crate) fn add_world_state_section(
    session: &Session,
    world_state: &mut WorldState,
    step_context: &StepContext,
    previous_world_state: Option<&Map<String, Value>>,
) {
    let turn_context = step_context.turn.as_ref();
    let enabled = turn_context.config.features.enabled(Feature::OutputStyles);
    if !enabled && !previous_shows_active_style(previous_world_state) {
        return;
    }
    let style = if enabled && !turn_context.session_source.is_non_root_agent() {
        selection_for_session(session, &turn_context.config)
            .style()
            .cloned()
    } else {
        None
    };
    world_state.add_section(OutputStyleState {
        model: step_context.settings.model_info.slug.clone(),
        style,
        retained_check: previous_shows_active_style(previous_world_state),
    });
}

/// Whether the stored snapshot says the model follows a style other than `default`.
fn previous_shows_active_style(previous_world_state: Option<&Map<String, Value>>) -> bool {
    previous_world_state
        .and_then(|state| state.get(OutputStyleState::ID))
        .and_then(|section| section.get("style"))
        .is_some_and(|style| !style.is_null())
}

/// Use case 3 (`specs/output-styles-2026-11-07.md`): the model of the step and the style in
/// effect. The section announces a style when a window starts (R3.1), adds one message when
/// the style changes (R3.2) and restates it after a model change (R3.3).
#[derive(Clone, Debug)]
pub(crate) struct OutputStyleState {
    model: String,
    /// `None` is `default`.
    style: Option<Arc<OutputStyle>>,
    /// R3.6: the stored snapshot shows an active style, so it counts only while history still
    /// holds that style's message. A stored `default` needs no message in history, so a
    /// change from it is a replacement rather than a first announcement.
    retained_check: bool,
}

impl OutputStyleState {
    /// A state whose stored snapshot, if any, shows an active style.
    #[cfg(test)]
    pub(crate) fn new(model: &str, style: Option<Arc<OutputStyle>>) -> Self {
        Self {
            model: model.to_string(),
            style,
            retained_check: true,
        }
    }

    /// A state whose stored snapshot shows the default style.
    #[cfg(test)]
    pub(crate) fn after_default(mut self) -> Self {
        self.retained_check = false;
        self
    }

    fn snapshot(&self) -> OutputStyleSnapshot {
        OutputStyleSnapshot {
            model: self.model.clone(),
            style: self.style.as_ref().map(|style| OutputStyleSnapshotStyle {
                name: style.name.clone(),
                hash: text_hash(&style.text),
            }),
        }
    }
}

/// The hash of a style's text, not of its wrapper, so that rewording a lead line is not a
/// style change.
fn text_hash(text: &str) -> WorldStateHash {
    WorldStateHash::from_json(&Value::String(text.to_string()))
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct OutputStyleSnapshot {
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<OutputStyleSnapshotStyle>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct OutputStyleSnapshotStyle {
    name: String,
    hash: WorldStateHash,
}

/// How a message that shows a style as active starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lead {
    /// R3.1, R3.6: a window starts or history lost the style.
    First,
    /// R3.2, R3.6: the style changed, or history shows a style of unknown state.
    Replace,
    /// R3.3: the model changed.
    AfterModelSwitch,
}

/// The developer message of the section; its markers let retained history recognize it after
/// compaction or resume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutputStyleInstructions {
    body: String,
}

impl OutputStyleInstructions {
    fn active(lead: Lead, style: &OutputStyle) -> Self {
        let name = &style.name;
        let lead = match lead {
            Lead::First => format!("Output style: `{name}`."),
            Lead::Replace => format!(
                "The user switched the output style to `{name}`. It replaces any earlier output style."
            ),
            Lead::AfterModelSwitch => {
                format!("Output style: `{name}`. It stays in effect after the model change.")
            }
        };
        Self {
            body: format!("{lead}\n{SCOPE}\n\n{}", style.text),
        }
    }

    fn default_notice() -> Self {
        Self {
            body: DEFAULT_NOTICE.to_string(),
        }
    }
}

impl ContextualUserFragment for OutputStyleInstructions {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind("output_style.instructions".to_string())
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

/// R3.6: whether `text` shows a style as active: the section's markers around anything but
/// the default notice.
pub(crate) fn is_active_style_fragment(role: &str, text: &str) -> bool {
    role == "developer"
        && OutputStyleInstructions::matches_text(text)
        && text
            .trim_start()
            .get(OPEN_MARKER.len()..)
            .is_some_and(|body| !body.trim_start().starts_with(DEFAULT_NOTICE_LEAD))
}

impl WorldStateSection for OutputStyleState {
    const ID: &'static str = "output_style";
    type Snapshot = OutputStyleSnapshot;

    /// R3.6: without a stored snapshot only a message that shows a style makes the previous
    /// state `Unknown`; a default notice alone leaves nothing to take back.
    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        is_active_style_fragment(role, text)
    }

    /// R3.6: a snapshot with a style counts only while history still shows it, so after
    /// compaction or rollback drops it the style is announced again.
    fn has_retained_fragment_matcher() -> bool {
        true
    }

    fn has_current_retained_fragment_matcher(&self) -> bool {
        self.retained_check
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        is_active_style_fragment(role, text)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> SectionTransition<Self::Snapshot> {
        let current = self.snapshot();
        let message = match (previous, &self.style) {
            (PreviousSectionState::Known(previous), Some(style)) => {
                if previous.style != current.style {
                    Some(OutputStyleInstructions::active(Lead::Replace, style))
                } else if previous.model != current.model {
                    Some(OutputStyleInstructions::active(
                        Lead::AfterModelSwitch,
                        style,
                    ))
                } else {
                    None
                }
            }
            (PreviousSectionState::Known(previous), None) => previous
                .style
                .is_some()
                .then(OutputStyleInstructions::default_notice),
            (PreviousSectionState::Absent, Some(style)) => {
                Some(OutputStyleInstructions::active(Lead::First, style))
            }
            (PreviousSectionState::Absent, None) => None,
            (PreviousSectionState::Unknown, Some(style)) => {
                Some(OutputStyleInstructions::active(Lead::Replace, style))
            }
            (PreviousSectionState::Unknown, None) => {
                Some(OutputStyleInstructions::default_notice())
            }
        };
        let updates = message
            .map(|message| vec![WorldStateUpdate::fragment(message).standalone()])
            .unwrap_or_default();
        (Some(current), updates)
    }
}

#[cfg(test)]
#[path = "section_tests.rs"]
mod tests;
