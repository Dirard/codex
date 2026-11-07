use super::*;
use pretty_assertions::assert_eq;

/// R9.1: an interval word starts a fixed loop within the configured range, plain text a
/// self-paced loop; `stop` stops, nothing shows; a bad interval is an error.
#[test]
fn parses_loop_arguments() {
    let limits = ScheduledWakeupsConfig::default();
    assert_eq!(parse_loop_args("", &limits), LoopCommand::Show);
    assert_eq!(parse_loop_args("  stop ", &limits), LoopCommand::Stop);
    assert_eq!(
        parse_loop_args("5m check the deploy", &limits),
        LoopCommand::Start {
            interval_s: Some(300),
            prompt: "check the deploy".to_string(),
        }
    );
    assert_eq!(
        parse_loop_args("1h  watch CI", &limits),
        LoopCommand::Start {
            interval_s: Some(3_600),
            prompt: "watch CI".to_string(),
        }
    );
    assert_eq!(
        parse_loop_args("watch CI until it is green", &limits),
        LoopCommand::Start {
            interval_s: None,
            prompt: "watch CI until it is green".to_string(),
        }
    );
    assert_eq!(
        parse_loop_args("30s check", &limits),
        LoopCommand::Error("The /loop interval must be from 1m to 1h.".to_string())
    );
    assert_eq!(
        parse_loop_args("2h check", &limits),
        LoopCommand::Error("The /loop interval must be from 1m to 1h.".to_string())
    );
    assert_eq!(
        parse_loop_args("5m", &limits),
        LoopCommand::Error("Usage: /loop [<interval>] <prompt>".to_string())
    );
    // Words that only look like units stay part of the prompt.
    assert_eq!(
        parse_loop_args("m check", &limits),
        LoopCommand::Start {
            interval_s: None,
            prompt: "m check".to_string(),
        }
    );
}
