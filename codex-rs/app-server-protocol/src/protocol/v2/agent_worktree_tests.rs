use super::*;
use crate::protocol::v2::ThreadExtra;
use pretty_assertions::assert_eq;
use serde_json::json;

/// R9.4: `ThreadExtra` stays `{}` without a worktree and carries `agentWorktree` with one.
#[test]
fn thread_extra_serializes_the_agent_worktree() {
    assert_eq!(
        serde_json::to_value(ThreadExtra::default()).expect("serialize extra"),
        json!({})
    );
    let extra = ThreadExtra {
        agent_worktree: Some(ThreadAgentWorktree {
            path: AbsolutePathBuf::from_absolute_path("/home/u/.codex/worktrees/6d88/demo")
                .expect("absolute path"),
            branch: None,
            status: ThreadAgentWorktreeStatus::Clean,
        }),
    };
    let value = serde_json::to_value(&extra).expect("serialize extra");
    assert_eq!(
        value,
        json!({
            "agentWorktree": {
                "path": "/home/u/.codex/worktrees/6d88/demo",
                "branch": null,
                "status": "clean",
            }
        })
    );
    assert_eq!(
        serde_json::from_value::<ThreadExtra>(value).expect("deserialize extra"),
        extra
    );
    assert_eq!(
        serde_json::from_value::<ThreadExtra>(json!({})).expect("deserialize empty extra"),
        ThreadExtra::default()
    );
}
