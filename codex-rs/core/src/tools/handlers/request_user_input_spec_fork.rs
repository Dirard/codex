//! Fork: `request_user_input` in Default mode (`default_mode_request_user_input`,
//! `specs/explorer-and-questions-2026-10-25.md`).

#[cfg(test)]
use codex_features::Features;
use codex_protocol::config_types::ModeKind;
use codex_protocol::protocol::SessionSource;

/// When to ask, added to the description whenever Default mode allows the tool.
///
/// The description is shared by every mode, so the text names Default mode and leaves Plan
/// mode's rules alone. It follows the upstream Default-mode template: the tool is for optional
/// questions, and input that progress must wait for is asked in plain text. It steers questions
/// away from an asynchronous question tool without naming one: only some model catalogs list
/// `request_user_input_async` (tails review 2026-10-27, P3-11).
pub(crate) const DEFAULT_MODE_GUIDANCE: &str = "In Default mode, prefer reasonable assumptions and keep working: ask only optional questions whose answer would materially improve the work and that you cannot settle from the codebase, the conversation or your tools, such as a real choice between materially different approaches. Do not ask for permission, to confirm a plan you can carry out, or for facts you can discover. Ask at most once per decision. The user may not answer; if no answer arrives, continue with your recommended option and state that assumption. Do not use this tool for irreversible or destructive actions (deleting or overwriting data or work, migrations, publishing, pushing, spending money): ask the user in plain text and do not take such an action until the user answers. To ask in Default mode, use this tool rather than an asynchronous question tool: this tool returns the answer, or tells you that none came, before you continue. Plan mode keeps its own rules for this tool.";

/// Returned in Default mode when the request resolves without any answer.
pub(crate) const DEFAULT_MODE_NO_ANSWER_MESSAGE: &str = "No answer was received: the user did not answer, or this client cannot show questions. If your recommended option is reversible, continue with it and state that assumption in your reply. If it, or the step it leads to, is irreversible or destructive (deleting or overwriting data or work, migrations, publishing, pushing, spending money), do not take it without the user: finish your turn and ask in plain text instead. Do not ask again with request_user_input for this decision.";

/// Modes in which this session may ask. Default mode (behind
/// `default_mode_request_user_input`) is for root threads; sub-agents keep the Plan-only set
/// and its description because only the root thread may ask the user.
#[cfg(test)]
pub(crate) fn request_user_input_modes_for_session(
    features: &Features,
    session_source: &SessionSource,
) -> Vec<ModeKind> {
    modes_for_session(
        codex_tools::request_user_input_available_modes(features),
        session_source,
    )
}

/// `modes` without Default mode for sub-agents.
pub(crate) fn modes_for_session(
    modes: Vec<ModeKind>,
    session_source: &SessionSource,
) -> Vec<ModeKind> {
    modes
        .into_iter()
        .filter(|mode| mode.allows_request_user_input() || !session_source.is_non_root_agent())
        .collect()
}

/// In Default mode an empty response means nobody answered (client error, unsupported
/// client, or auto-resolution), not that the user chose nothing: the model gets
/// [`DEFAULT_MODE_NO_ANSWER_MESSAGE`] instead of the serialized `content`.
pub(crate) fn default_mode_content(
    mode: ModeKind,
    questions: &[codex_protocol::request_user_input::RequestUserInputQuestion],
    response: &codex_protocol::request_user_input::RequestUserInputResponse,
    content: String,
) -> String {
    let answered = questions.iter().any(|question| {
        response.answers.get(&question.id).is_some_and(|answer| {
            answer
                .answers
                .iter()
                .any(|answer| !answer.trim().is_empty())
        })
    });
    if mode == ModeKind::Default && !answered {
        DEFAULT_MODE_NO_ANSWER_MESSAGE.to_string()
    } else {
        content
    }
}

/// The description says when to ask whenever Default mode allows the tool.
pub(super) fn with_default_mode_guidance(
    description: String,
    available_modes: &[ModeKind],
) -> String {
    if available_modes.contains(&ModeKind::Default) {
        format!("{description}\n{DEFAULT_MODE_GUIDANCE}")
    } else {
        description
    }
}
