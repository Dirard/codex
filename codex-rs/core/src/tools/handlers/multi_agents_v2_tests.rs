use super::agent_message_from_tool;
use crate::MessageDeliveryMode;
use crate::tools::context::ToolCallSource;
use codex_features::MultiAgentMessageDelivery;
use codex_protocol::AgentPath;

#[test]
fn plaintext_delivery_applies_to_code_mode_messages() {
    let author = AgentPath::root();
    let recipient = author.join("worker").expect("valid child path");
    let communication = agent_message_from_tool(
        "cross-provider task".to_string(),
        &ToolCallSource::CodeMode {
            cell_id: "cell-1".to_string(),
            runtime_tool_call_id: "tool-1".to_string(),
        },
        MultiAgentMessageDelivery::Plaintext,
    )
    .into_communication(author, recipient, MessageDeliveryMode::TriggerTurn);

    assert_eq!(communication.encrypted_content, None);
    assert!(communication.content.contains("cross-provider task"));
}
