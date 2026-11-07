use std::sync::Arc;

use codex_features::Feature;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::InternalSessionSource;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;
use crate::context::world_state::WorldStateSnapshot;
use crate::context::world_state::WorldStateUpdateContent;
use crate::context::world_state::test_support::FragmentSectionTestExt as _;
use crate::output_style::OutputStyleCatalog;
use crate::output_style::OutputStyleConfig;
use crate::session::tests::make_session_and_context;
use crate::session::turn_context::TurnContext;

fn builtin(name: &str) -> Arc<OutputStyle> {
    OutputStyleCatalog::builtin()
        .find(name)
        .cloned()
        .expect("built-in style")
}

fn state(model: &str, style: Option<&str>) -> OutputStyleState {
    OutputStyleState::new(model, style.map(builtin))
}

fn text(fragment: &dyn ContextualUserFragment) -> String {
    match fragment.render() {
        ContentItem::InputText { text } => text,
        content => panic!("expected text, got {content:?}"),
    }
}

/// The first line inside the markers of the message the section adds, if any.
fn lead(
    current: &OutputStyleState,
    previous: PreviousSectionState<'_, OutputStyleSnapshot>,
) -> Option<String> {
    let (snapshot, fragment) = current.render_fragment_diff(previous);
    assert_eq!(
        snapshot,
        Some(current.snapshot()),
        "the snapshot is always written"
    );
    fragment.map(|fragment| {
        let text = text(fragment.as_ref());
        text.trim_start_matches(OPEN_MARKER)
            .trim_start()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    })
}

fn history_item(message: &OutputStyleInstructions) -> ResponseItem {
    ContextualUserFragment::into(message.clone())
}

/// R3.1–R3.4, R3.6: the message of each transition, by its lead line.
#[test]
fn render_diff_transitions() {
    use PreviousSectionState::Absent;
    use PreviousSectionState::Known;
    use PreviousSectionState::Unknown;

    let default_a = state("model-a", None).snapshot();
    let first_a = state("model-a", Some("outcome-first")).snapshot();
    let first_b = state("model-b", Some("outcome-first")).snapshot();
    let learning_a = state("model-a", Some("learning")).snapshot();
    let notice = Some(
        DEFAULT_NOTICE
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
    );
    let first = Some("Output style: `outcome-first`.".to_string());
    let replace = |name: &str| {
        Some(format!(
            "The user switched the output style to `{name}`. It replaces any earlier output style."
        ))
    };
    let restate = Some(
        "Output style: `outcome-first`. It stays in effect after the model change.".to_string(),
    );

    let cases = [
        // R3.1: a window starts.
        (
            "absent -> style",
            lead(&state("model-a", Some("outcome-first")), Absent),
        ),
        ("absent -> default", lead(&state("model-a", None), Absent)),
        // R3.4: nothing changed.
        (
            "same style, same model",
            lead(&state("model-a", Some("outcome-first")), Known(&first_a)),
        ),
        (
            "default, same model",
            lead(&state("model-a", None), Known(&default_a)),
        ),
        (
            "default, other model",
            lead(&state("model-b", None), Known(&default_a)),
        ),
        // R3.2: the style changed.
        (
            "default -> style",
            lead(&state("model-a", Some("outcome-first")), Known(&default_a)),
        ),
        (
            "style -> other style",
            lead(&state("model-a", Some("learning")), Known(&first_a)),
        ),
        (
            "style -> default",
            lead(&state("model-a", None), Known(&learning_a)),
        ),
        // R3.3: the model changed.
        (
            "same style, other model",
            lead(&state("model-a", Some("outcome-first")), Known(&first_b)),
        ),
        (
            "style change wins over model change",
            lead(&state("model-b", Some("learning")), Known(&first_a)),
        ),
        // R3.6: history shows a style without a snapshot.
        (
            "unknown -> style",
            lead(&state("model-a", Some("outcome-first")), Unknown),
        ),
        ("unknown -> default", lead(&state("model-a", None), Unknown)),
    ];
    let expected = [
        ("absent -> style", first),
        ("absent -> default", None),
        ("same style, same model", None),
        ("default, same model", None),
        ("default, other model", None),
        ("default -> style", replace("outcome-first")),
        ("style -> other style", replace("learning")),
        ("style -> default", notice.clone()),
        ("same style, other model", restate),
        ("style change wins over model change", replace("learning")),
        ("unknown -> style", replace("outcome-first")),
        ("unknown -> default", notice),
    ];
    assert_eq!(cases, expected);
}

/// R3.2: every message is a separate developer message.
#[test]
fn updates_are_standalone() {
    let (_, updates) =
        state("model-a", Some("outcome-first")).render_diff(PreviousSectionState::Absent);
    assert_eq!(updates.len(), 1);
    assert!(matches!(
        updates[0].placement,
        crate::context::world_state::Placement::Standalone
    ));
}

/// R3.6: retained history counts the first announcement, a replacement and a restatement as
/// an active style, but not the default notice or a user message with the same text; the
/// snapshot hashes the style text, not the wrapper.
#[test]
fn history_matchers_and_hash() {
    let style = builtin("explanatory");
    for lead in [Lead::First, Lead::Replace, Lead::AfterModelSwitch] {
        let message = text(&OutputStyleInstructions::active(lead, &style));
        assert!(is_active_style_fragment("developer", &message), "{lead:?}");
        assert!(!is_active_style_fragment("user", &message), "{lead:?}");
    }
    let notice = text(&OutputStyleInstructions::default_notice());
    assert!(!is_active_style_fragment("developer", &notice));
    assert!(!is_active_style_fragment("developer", "Output style: `x`."));

    let snapshot = state("model-a", Some("explanatory")).snapshot();
    assert_eq!(
        snapshot.style.map(|style| style.hash),
        Some(text_hash(&style.text))
    );
}

/// R3.6: a stored style counts only while history still shows it; without a snapshot a
/// retained style is replaced or taken back, and a default notice alone leaves nothing to do.
#[test]
fn retained_history_decides_the_previous_state() {
    let style = builtin("outcome-first");
    let announced = history_item(&OutputStyleInstructions::active(Lead::First, &style));
    let notice = history_item(&OutputStyleInstructions::default_notice());
    let messages = |current: OutputStyleState,
                    stored: Option<&WorldStateSnapshot>,
                    history: &[ResponseItem]| {
        let mut world_state = WorldState::default();
        world_state.add_section(current);
        world_state
            .render_history_fragment_diff(stored, history)
            .1
            .iter()
            .map(|fragment| text(fragment.as_ref()))
            .collect::<Vec<_>>()
    };
    let mut stored_state = WorldState::default();
    stored_state.add_section(state("model-a", Some("outcome-first")));
    let stored = stored_state.render_full().0;
    let active = |lead: Lead| text(&OutputStyleInstructions::active(lead, &style));
    let none = Vec::<String>::new();

    assert_eq!(
        messages(
            state("model-a", Some("outcome-first")),
            Some(&stored),
            std::slice::from_ref(&announced)
        ),
        none
    );
    // Compaction or rollback dropped the announcement: announce again.
    assert_eq!(
        messages(state("model-a", Some("outcome-first")), Some(&stored), &[]),
        vec![active(Lead::First)]
    );
    // No snapshot, a style in history (a forked child): replace it or take it back.
    assert_eq!(
        messages(
            state("model-a", Some("outcome-first")),
            None,
            std::slice::from_ref(&announced)
        ),
        vec![active(Lead::Replace)]
    );
    assert_eq!(
        messages(state("model-a", None), None, &[announced]),
        vec![text(&OutputStyleInstructions::default_notice())]
    );
    // A stored default needs no message in history: a new style replaces it.
    let mut default_state = WorldState::default();
    default_state.add_section(state("model-a", None).after_default());
    let stored_default = default_state.render_full().0;
    assert_eq!(
        messages(
            state("model-a", Some("outcome-first")).after_default(),
            Some(&stored_default),
            &[],
        ),
        vec![active(Lead::Replace)]
    );
    assert_eq!(messages(state("model-a", None), None, &[notice]), none);
    assert_eq!(messages(state("model-a", None), None, &[]), none);
}

/// R3.11: the wrapper adds at most 1 KiB to the longest built-in text.
#[test]
fn wrapper_stays_within_one_kib() {
    for style in OutputStyleCatalog::builtin().styles {
        if style.is_default() {
            continue;
        }
        for lead in [Lead::First, Lead::Replace, Lead::AfterModelSwitch] {
            let message = text(&OutputStyleInstructions::active(lead, &style));
            assert!(
                message.len() <= style.text.len() + 1024,
                "{} {lead:?}: {} bytes of wrapper",
                style.name,
                message.len() - style.text.len()
            );
        }
    }
}

/// Review O-4: the scope leaves the rules of the current collaboration mode, such as when Plan
/// mode asks questions, to the mode, so a style's own question guidance does not override them.
#[test]
fn collaboration_mode_rules_win_over_the_style() {
    let message = text(&OutputStyleInstructions::active(
        Lead::First,
        &builtin("outcome-first"),
    ));
    assert!(
        message.contains(
            "the rules of the current collaboration mode, such as when to ask questions and how to present a plan"
        ),
        "{message}"
    );
}

/// The wording of the messages for review.
#[test]
fn messages_snapshot() {
    let style = builtin("explanatory");
    insta::assert_snapshot!(
        [
            text(&OutputStyleInstructions::active(Lead::First, &style)),
            text(&OutputStyleInstructions::active(Lead::Replace, &style)),
            text(&OutputStyleInstructions::active(
                Lead::AfterModelSwitch,
                &style
            )),
            text(&OutputStyleInstructions::default_notice()),
        ]
        .join("\n\n")
    );
}

/// The section's snapshot and its messages in a full render of a step whose turn
/// `configure` changed.
async fn assembled(
    configure: impl FnOnce(&mut TurnContext),
) -> (Option<serde_json::Value>, Vec<String>) {
    let (session, mut turn) = make_session_and_context().await;
    {
        let config = Arc::make_mut(&mut turn.config);
        config
            .features
            .enable(Feature::OutputStyles)
            .expect("output styles can be enabled");
        config.output_style = Arc::new(OutputStyleConfig {
            requested: Some("outcome-first".to_string()),
            selection: OutputStyleSelection::Style(builtin("outcome-first")),
        });
    }
    configure(&mut turn);
    let step = StepContext::for_test(Arc::new(turn));
    let world_state = session
        .build_world_state_for_step(&step, /*new_window*/ true)
        .await
        .expect("world state builds");
    let (snapshot, updates) = world_state.render_full();
    let messages = updates
        .into_iter()
        .filter_map(|update| match update.content {
            WorldStateUpdateContent::Fragment(fragment) => Some(text(fragment.as_ref())),
            WorldStateUpdateContent::Item(_) | WorldStateUpdateContent::Environment(_) => None,
        })
        .filter(|text| text.contains(OPEN_MARKER))
        .collect();
    (
        snapshot.into_object().get(OutputStyleState::ID).cloned(),
        messages,
    )
}

/// R3.1: a root thread with a selected style announces it once in a new window.
#[tokio::test]
async fn root_thread_announces_the_selected_style() {
    let (section, messages) = assembled(|_| {}).await;
    let section = section.expect("section present");
    assert_eq!(section["style"]["name"], json!("outcome-first"));
    assert_eq!(messages.len(), 1);
    assert!(messages[0].starts_with("<output_style>\nOutput style: `outcome-first`."));
}

/// R3.5: spawned agents, Guardian and internal sessions use `default` even when the config
/// selects a style.
#[tokio::test]
async fn non_root_agents_use_the_default_style() {
    let sources = [
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ThreadId::new(),
            depth: 1,
            agent_path: Some(AgentPath::try_from("/root/worker").expect("agent path")),
            agent_nickname: None,
            agent_role: None,
        }),
        SessionSource::SubAgent(SubAgentSource::Other("guardian".to_string())),
        SessionSource::Internal(InternalSessionSource::MemoryConsolidation),
    ];
    for source in sources {
        let (section, messages) = assembled(|turn| turn.session_source = source.clone()).await;
        let section = section.expect("section present while the feature is on");
        assert_eq!(section.get("style"), None, "{source:?}");
        assert_eq!(messages, Vec::<String>::new(), "{source:?}");
    }
}

/// R3.7: with the feature off and no earlier style, the section is not built at all.
#[tokio::test]
async fn disabled_feature_adds_no_section() {
    let (section, messages) = assembled(|turn| {
        Arc::make_mut(&mut turn.config)
            .features
            .disable(Feature::OutputStyles)
            .expect("output styles can be disabled");
    })
    .await;
    assert_eq!(section, None);
    assert_eq!(messages, Vec::<String>::new());
}

/// R3.7: with the feature off, the section stays only while the previous snapshot shows an
/// active style.
#[test]
fn previous_snapshot_with_a_style_keeps_the_section() {
    let with_style = state("model-a", Some("learning")).snapshot();
    let without_style = state("model-a", None).snapshot();
    let checkpoint = |snapshot: &OutputStyleSnapshot| {
        let mut map = Map::new();
        map.insert(
            OutputStyleState::ID.to_string(),
            serde_json::to_value(snapshot).expect("snapshot serializes"),
        );
        map
    };
    assert!(previous_shows_active_style(Some(&checkpoint(&with_style))));
    assert!(!previous_shows_active_style(Some(&checkpoint(
        &without_style
    ))));
    assert!(!previous_shows_active_style(Some(&Map::new())));
    assert!(!previous_shows_active_style(None));
}
