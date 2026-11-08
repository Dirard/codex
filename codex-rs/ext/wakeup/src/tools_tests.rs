use super::*;
use crate::test_support::estimated_tokens;
use pretty_assertions::assert_eq;

/// Wakeup trigger guidance R1.4, R1.7 (amends R2.8): names, descriptions and parameter schemas
/// of both tools stay within 450 tokens, with and without background notifications.
#[test]
fn tool_specs_stay_small() {
    let limits = ScheduledWakeupsConfig::default();
    for exit_notifications in [false, true] {
        let text = tool_text(&limits, exit_notifications);
        let tokens = estimated_tokens(&text);
        assert!(tokens <= 450, "{tokens} tokens: {text}");
        assert!(text.contains("clamped to 60-3600"), "{text}");
    }
}

/// Wakeup trigger guidance R1.1-R1.3, R1.5 and fork tool guidance UC1
/// (`specs/fork-tool-guidance-2026-11-08.md`, R1.1, R1.2): with background notifications the
/// description prefers waking on an event through a bounded check loop; without them it never
/// mentions `notify_on_*`. Whether a sub-agent's answer wakes an idle thread depends on
/// `wake_on_agent_messages` and the mode, so neither variant claims it does or does not: both
/// leave sub-agents to `wait_agent`, and a wakeup is at most a fallback that is cancelled once the
/// work reports.
#[test]
fn the_description_prefers_events_when_notifications_are_on() {
    let limits = ScheduledWakeupsConfig::default();
    let with_events = tool_text(&limits, /*exit_notifications*/ true);
    for expected in [
        "end your turn after the call",
        "prefer exec_command with notify_on_exit or notify_on_output",
        "one bounded background check loop",
        "what no shell command can check",
        "Do not schedule wakeups to wait for background commands that already notify you or for sub-agents",
        "wait_agent says how their answers reach you",
        "except one long fallback (20-60 minutes)",
        "cancel it with clock.cancel_wakeup once the work reports",
        "Pick the delay from how fast the state changes",
        "what to check and what to do next",
        "Required except in a self-paced /loop",
        "what you are waiting for",
        "a fallback once the work it guarded has reported",
    ] {
        assert!(with_events.contains(expected), "{expected}: {with_events}");
    }
    let without_events = tool_text(&limits, /*exit_notifications*/ false);
    assert!(!without_events.contains("notify_on"), "{without_events}");
    for expected in [
        "Do not schedule wakeups to wait for sub-agents",
        "wait_agent says how their answers reach you",
        "except one long fallback (20-60 minutes)",
        "cancel it with clock.cancel_wakeup once the work reports",
        "Pick the delay from how fast the state changes",
        "It lasts only while this Codex session runs.",
    ] {
        assert!(
            without_events.contains(expected),
            "{expected}: {without_events}"
        );
    }
    for text in [&with_events, &without_events] {
        for wrong in ["report back", "never wake", "until <check>"] {
            assert!(!text.contains(wrong), "{wrong}: {text}");
        }
    }
}

fn tool_text(limits: &ScheduledWakeupsConfig, exit_notifications: bool) -> String {
    let mut text = String::new();
    for spec in [
        schedule_wakeup_spec(limits, exit_notifications),
        cancel_wakeup_spec(),
    ] {
        let ToolSpec::Namespace(namespace) = spec else {
            panic!("clock tools are namespaced");
        };
        assert_eq!(namespace.name, CLOCK_NAMESPACE);
        assert_eq!(namespace.description, CLOCK_NAMESPACE_DESCRIPTION);
        for tool in namespace.tools {
            let ResponsesApiNamespaceTool::Function(tool) = tool else {
                panic!("clock tools are functions");
            };
            text.push_str(&tool.name);
            text.push_str(&tool.description);
            text.push_str(&serde_json::to_string(&tool.parameters).expect("schema"));
        }
    }
    text
}

/// R2.3: `delay_seconds` must be a positive integer, and unknown arguments are rejected.
#[test]
fn schedule_arguments_are_checked() {
    assert_eq!(
        parse_schedule_args(r#"{"delay_seconds": 90, "prompt": "p"}"#),
        Ok(ScheduleArgs {
            delay_seconds: 90,
            prompt: Some("p".to_string()),
            reason: None,
        })
    );
    for arguments in [
        r#"{"delay_seconds": 0}"#,
        r#"{"delay_seconds": -5}"#,
        r#"{"delay_seconds": 1.5}"#,
        r#"{"delay_seconds": "60"}"#,
    ] {
        assert_eq!(
            parse_schedule_args(arguments),
            Err("Wakeup not scheduled: delay_seconds must be a positive integer.".to_string()),
            "{arguments}"
        );
    }
    assert!(parse_schedule_args(r#"{"delay_seconds": 60, "when": 1}"#).is_err());
}
