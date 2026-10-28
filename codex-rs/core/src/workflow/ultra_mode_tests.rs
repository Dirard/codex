use std::sync::Arc;

use super::*;
use crate::context::world_state::WorldState;
use crate::context::world_state::WorldStateSnapshot;
use crate::context::world_state::WorldStateUpdateContent;
use crate::context::world_state::test_support::FragmentSectionTestExt as _;
use crate::session::tests::make_session_and_context;
use crate::session::tests::update_selected_settings_for_test;
use crate::session::tests::update_turn_settings_for_test;
use crate::session::turn_context::TurnContext;
use crate::tools::registry::ToolRegistry;
use crate::tools::router::ToolRouter;
use codex_features::Feature;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_utils_output_truncation::approx_token_count;
use pretty_assertions::assert_eq;
use serde_json::json;

/// R1.5: the enabling message stays within this many tokens.
const MAX_ENABLED_TOKENS: usize = 500;

fn state(enabled: bool) -> WorkflowUltraModeState {
    WorkflowUltraModeState { enabled }
}

fn text(fragment: &dyn ContextualUserFragment) -> String {
    match fragment.render() {
        ContentItem::InputText { text } => text,
        content => panic!("expected text, got {content:?}"),
    }
}

fn rendered(enabled: bool) -> String {
    text(&WorkflowUltraModeInstructions { enabled })
}

fn history_item(enabled: bool) -> ResponseItem {
    ContextualUserFragment::into(WorkflowUltraModeInstructions { enabled })
}

/// The message the section adds against `previous`, if any.
fn message(
    enabled: bool,
    previous: PreviousSectionState<'_, WorkflowUltraModeState>,
) -> Option<String> {
    let current = state(enabled);
    let (snapshot, fragment) = current.render_fragment_diff(previous);
    assert_eq!(snapshot, Some(current));
    fragment.map(|fragment| text(fragment.as_ref()))
}

/// The messages a world state with only this section adds against a stored snapshot and
/// retained history.
fn history_messages(
    enabled: bool,
    stored: Option<&WorldStateSnapshot>,
    history: &[ResponseItem],
) -> Vec<String> {
    let mut world_state = WorldState::default();
    world_state.add_section(state(enabled));
    world_state
        .render_history_fragment_diff(stored, history)
        .1
        .iter()
        .map(|fragment| text(fragment.as_ref()))
        .collect()
}

/// R1.1–R1.3: the section adds the enabling message when the mode turns on, nothing while
/// it stays the same, and the disabling message when it turns off after being on.
#[test]
fn section_transitions() {
    use PreviousSectionState::Absent;
    use PreviousSectionState::Known;
    use PreviousSectionState::Unknown;

    let (on, off) = (state(true), state(false));
    let cases = [
        ("off -> on", message(true, Known(&off))),
        ("on -> on", message(true, Known(&on))),
        ("on -> off", message(false, Known(&on))),
        ("off -> off", message(false, Known(&off))),
        ("absent -> on", message(true, Absent)),
        ("absent -> off", message(false, Absent)),
        ("unknown -> on", message(true, Unknown)),
        ("unknown -> off", message(false, Unknown)),
    ];
    let expected = [
        ("off -> on", Some(rendered(true))),
        ("on -> on", None),
        ("on -> off", Some(rendered(false))),
        ("off -> off", None),
        ("absent -> on", Some(rendered(true))),
        ("absent -> off", None),
        ("unknown -> on", Some(rendered(true))),
        ("unknown -> off", Some(rendered(false))),
    ];
    assert_eq!(cases, expected);
}

/// R1.3: without a stored snapshot (compaction, history without this section) the mode is
/// taken back only when retained history shows the enabling message; a disabling message or
/// a user message with the same text does not count.
#[test]
fn unknown_state_takes_back_only_a_retained_enabling_message() {
    let user_copy = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: rendered(true),
        }],
        phase: None,
        status: None,
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    };
    let none = Vec::<String>::new();
    assert_eq!(
        history_messages(false, None, &[history_item(true)]),
        vec![rendered(false)]
    );
    assert_eq!(history_messages(false, None, &[history_item(false)]), none);
    assert_eq!(history_messages(false, None, &[user_copy]), none);
    assert_eq!(history_messages(false, None, &[]), none);
    assert_eq!(
        history_messages(true, None, &[history_item(true)]),
        vec![rendered(true)]
    );
}

/// R1.1, R1.2: a stored enabled state counts only while history still shows the enabling
/// message: after compaction drops it the mode is announced again, and turning off then
/// leaves nothing to take back. A forked child that inherits the enabling message and the
/// snapshot but has no `workflow` tool gets the disabling message.
#[test]
fn stored_state_follows_retained_history() {
    let mut enabled = WorldState::default();
    enabled.add_section(state(true));
    let stored = enabled.render_full().0;
    let none = Vec::<String>::new();
    assert_eq!(
        history_messages(true, Some(&stored), &[history_item(true)]),
        none
    );
    assert_eq!(
        history_messages(true, Some(&stored), &[]),
        vec![rendered(true)]
    );
    assert_eq!(
        history_messages(false, Some(&stored), &[history_item(true)]),
        vec![rendered(false)]
    );
    assert_eq!(history_messages(false, Some(&stored), &[]), none);
}

/// R1.5: the wording of both messages, and the enabling one within 500 tokens.
#[test]
fn messages_snapshot() {
    let enabled = rendered(true);
    let disabled = rendered(false);
    assert!(
        approx_token_count(&enabled) <= MAX_ENABLED_TOKENS,
        "{} tokens",
        approx_token_count(&enabled)
    );
    assert!(is_enabling_fragment("developer", &enabled));
    assert!(!is_enabling_fragment("developer", &disabled));
    insta::assert_snapshot!(format!("{enabled}\n\n{disabled}"));
}

/// The world state of a root step with workflows and MultiAgentV2 on, whose tools come from
/// `add_workflow_tools` like a thread with a Code Mode host and the local agent backend;
/// `configure` changes the turn first. Returns the section's snapshot and the ultra mode
/// messages of a full render.
async fn assembled(
    effort: ReasoningEffort,
    configure: impl FnOnce(&mut TurnContext),
) -> (Option<serde_json::Value>, Vec<String>) {
    let (session, mut turn) = make_session_and_context().await;
    Arc::make_mut(&mut turn.config)
        .features
        .enable(Feature::Workflows)
        .expect("workflows can be enabled");
    turn.multi_agent_version = MultiAgentVersion::V2;
    update_turn_settings_for_test(&mut turn, |settings| {
        update_selected_settings_for_test(settings, |selected| {
            selected.collaboration_mode.settings.reasoning_effort = Some(effort);
        });
    });
    configure(&mut turn);
    let mut registry = ToolRegistry::empty_for_test();
    crate::workflow::add_workflow_tools(&turn, /*workflows_available*/ true, &mut registry);
    let router = ToolRouter::from_registry(
        &turn,
        turn.model_info(),
        registry,
        Vec::new(),
        &Default::default(),
    );
    let step = StepContext::for_test(Arc::new(turn)).with_tool_router_for_test(Arc::new(router));
    let world_state = session
        .build_world_state_for_step(&step, /*new_window*/ true)
        .await
        .expect("world state builds");
    let (snapshot, updates) = world_state.render_full();
    let messages = updates
        .into_iter()
        .filter_map(|update| match update.content {
            WorldStateUpdateContent::Fragment(fragment) => Some(text(fragment.as_ref())),
            WorldStateUpdateContent::Item(_) => None,
        })
        .filter(|text| text.contains(OPEN_MARKER))
        .collect();
    (
        snapshot
            .into_object()
            .get(WorkflowUltraModeState::ID)
            .cloned(),
        messages,
    )
}

/// R1.1: effort `ultra` with the `workflow` tool turns the mode on.
#[tokio::test]
async fn ultra_effort_with_the_workflow_tool_turns_the_mode_on() {
    let (section, messages) = assembled(ReasoningEffort::Ultra, |_| {}).await;
    assert_eq!(section, Some(json!({ "enabled": true })));
    assert_eq!(messages, vec![rendered(true)]);
}

/// R1.1: any other effort, or `[workflows] ultra_mode = false`, leaves the mode off.
#[tokio::test]
async fn max_effort_or_ultra_mode_false_leaves_the_mode_off() {
    let (section, messages) = assembled(ReasoningEffort::Max, |_| {}).await;
    assert_eq!(section, Some(json!({ "enabled": false })));
    assert_eq!(messages, Vec::<String>::new());

    let (section, messages) = assembled(ReasoningEffort::Ultra, |turn| {
        Arc::make_mut(&mut turn.config).workflows.ultra_mode = false;
    })
    .await;
    assert_eq!(section, Some(json!({ "enabled": false })));
    assert_eq!(messages, Vec::<String>::new());
}

/// R1.4: a sub-agent has no `workflow` tool, so its mode stays off and it gets no message;
/// with the `workflows` feature off the section is not built at all.
#[tokio::test]
async fn sub_agents_and_disabled_workflows_get_no_message() {
    let (section, messages) = assembled(ReasoningEffort::Ultra, |turn| {
        turn.session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ThreadId::new(),
            depth: 1,
            agent_path: Some(AgentPath::try_from("/root/worker").expect("agent path")),
            agent_nickname: None,
            agent_role: None,
        });
    })
    .await;
    assert_eq!(section, Some(json!({ "enabled": false })));
    assert_eq!(messages, Vec::<String>::new());

    let (section, messages) = assembled(ReasoningEffort::Ultra, |turn| {
        Arc::make_mut(&mut turn.config)
            .features
            .disable(Feature::Workflows)
            .expect("workflows can be disabled");
    })
    .await;
    assert_eq!(section, None);
    assert_eq!(messages, Vec::<String>::new());
}
