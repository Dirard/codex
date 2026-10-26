use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn output_item(item: serde_json::Value) -> ResponseItem {
    serde_json::from_value(item).expect("valid response item")
}

fn assistant_output(text: &str, phase: Option<&str>) -> ResponseItem {
    output_item(json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}],
        "phase": phase,
    }))
}

/// Fork: once a local compaction response calls a tool, only a non-commentary assistant
/// message after the last call is a summary; a preamble before the call never is.
#[test]
fn tool_call_without_summary_requires_final_message_after_last_call() {
    let call = output_item(json!({
        "type": "function_call",
        "name": "lookup",
        "arguments": "{}",
        "call_id": "call-1",
    }));
    let summary = assistant_output("SUMMARY", /*phase*/ None);
    let final_summary = assistant_output("SUMMARY", Some("final_answer"));
    let preamble = assistant_output("Let me check the config first.", Some("commentary"));
    let untagged_preamble = assistant_output("Let me check the config first.", /*phase*/ None);
    let empty = assistant_output("  ", /*phase*/ None);
    for (name, output, expected) in [
        ("summary only", vec![summary.clone()], false),
        (
            "commentary only keeps upstream",
            vec![preamble.clone()],
            false,
        ),
        ("empty response keeps upstream", Vec::new(), false),
        ("tool call only", vec![call.clone()], true),
        (
            "commentary preamble",
            vec![preamble.clone(), call.clone()],
            true,
        ),
        (
            "untagged preamble",
            vec![untagged_preamble, call.clone()],
            true,
        ),
        ("empty message after call", vec![call.clone(), empty], true),
        (
            "commentary after call",
            vec![call.clone(), preamble.clone()],
            true,
        ),
        (
            "commentary after summary",
            vec![call.clone(), final_summary.clone(), preamble.clone()],
            true,
        ),
        ("summary after call", vec![call.clone(), summary], false),
        (
            "preamble, call, then final summary",
            vec![preamble, call, final_summary],
            false,
        ),
    ] {
        assert_eq!(tool_call_without_summary(&output), expected, "{name}");
    }
}
