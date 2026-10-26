//! Fork: tool plan hooks of `spec_plan_fork.rs`.

use super::*;
use pretty_assertions::assert_eq;

/// R3: the stub of a direct-only tool does not take the Code Mode name of a nested tool, so a
/// dynamic `request-user-input` stays callable as `tools.request_user_input`.
#[tokio::test]
async fn direct_only_stub_does_not_shadow_a_nested_tool() {
    let plan = probe_with(
        |turn| set_features(turn, &[Feature::CodeMode, Feature::CodeModeOnly]),
        ToolPlanInputs {
            dynamic_tools: vec![dynamic_tool(
                /*namespace*/ None,
                "request-user-input",
                /*defer_loading*/ false,
            )],
            ..ToolPlanInputs::default()
        },
    )
    .await;

    assert_eq!(
        plan.exposure("request_user_input"),
        ToolExposure::DirectModelOnly
    );
    assert_eq!(
        plan.code_mode_tool_names.get("request_user_input"),
        Some(&ToolName::plain("request-user-input"))
    );
    // The direct tool stays visible to the model; `exec` declares the dynamic tool.
    plan.assert_visible_contains(&["request_user_input"]);
    let ToolSpec::Freeform(exec) = plan.visible_spec(codex_code_mode::PUBLIC_TOOL_NAME) else {
        panic!("expected code mode exec tool");
    };
    assert!(exec.description.contains(
        "### `request_user_input` (`request-user-input`)\nrequest-user-input dynamic tool"
    ));
    assert!(exec.description.contains("request_user_input(args:"));
}
