use super::*;
use crate::test_support::estimated_tokens;
use pretty_assertions::assert_eq;

/// R2.8: names, descriptions and parameter schemas of both tools stay within 200 tokens.
#[test]
fn tool_specs_stay_small() {
    let limits = ScheduledWakeupsConfig::default();
    let mut text = String::new();
    for spec in [schedule_wakeup_spec(&limits), cancel_wakeup_spec()] {
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
    let tokens = estimated_tokens(&text);
    assert!(tokens <= 200, "{tokens} tokens: {text}");
    assert!(text.contains("clamped to 60-3600"), "{text}");
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
