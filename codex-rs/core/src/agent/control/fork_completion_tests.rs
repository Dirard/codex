//! Fork: completion watchers of legacy children (`fork_completion.rs`).

use super::*;
use pretty_assertions::assert_eq;
use std::sync::PoisonError;

/// The task id of the recorded completion watcher of `child_thread_id`.
fn recorded_watcher(
    control: &LocalAgentControl,
    child_thread_id: ThreadId,
) -> Option<tokio::task::Id> {
    control
        .runtime
        .completion_watchers
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&child_thread_id)
        .map(|(_, completion)| completion.id())
}

async fn wait_until_no_recorded_watcher(control: &LocalAgentControl, child_thread_id: ThreadId) {
    timeout(Duration::from_secs(5), async {
        while recorded_watcher(control, child_thread_id).is_some() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a finished completion watcher should drop its record");
}

fn thread_spawn_source(parent_thread_id: ThreadId) -> SessionSource {
    SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id,
        depth: 1,
        agent_path: None,
        agent_nickname: None,
        agent_role: Some("explorer".to_string()),
    })
}

/// B9: a watcher that reported its child drops its record instead of keeping it for the
/// lifetime of the tree.
#[tokio::test]
async fn finished_completion_watcher_drops_its_record() {
    let harness = AgentControlHarness::new().await;
    let (parent_thread_id, parent_thread) = harness.start_thread().await;
    let child_thread_id = ThreadId::new();

    harness.control.maybe_start_completion_watcher(
        child_thread_id,
        Some(thread_spawn_source(parent_thread_id)),
        child_thread_id.to_string(),
        /*child_agent_path*/ None,
    );

    assert_eq!(wait_for_subagent_notification(&parent_thread).await, true);
    wait_until_no_recorded_watcher(&harness.control, child_thread_id).await;
}

/// B9: a replaced watcher ends without dropping the record of the watcher that replaced it,
/// and the newer watcher drops its own record once it reported the child.
#[tokio::test]
async fn replaced_completion_watcher_keeps_the_newer_record() {
    let harness = AgentControlHarness::new().await;
    let (parent_thread_id, parent_thread) = harness.start_thread().await;
    let (child_thread_id, child_thread) = harness.start_thread().await;
    let source = thread_spawn_source(parent_thread_id);

    harness.control.maybe_start_completion_watcher(
        child_thread_id,
        Some(source.clone()),
        "first watcher".to_string(),
        /*child_agent_path*/ None,
    );
    let first_watcher =
        recorded_watcher(&harness.control, child_thread_id).expect("first watcher is recorded");
    harness.control.maybe_start_completion_watcher(
        child_thread_id,
        Some(source),
        "second watcher".to_string(),
        /*child_agent_path*/ None,
    );
    let second_watcher =
        recorded_watcher(&harness.control, child_thread_id).expect("second watcher is recorded");
    assert_ne!(first_watcher, second_watcher);

    // The aborted first watcher ends here; the second keeps waiting for the child.
    tokio::task::yield_now().await;
    sleep(Duration::from_millis(50)).await;
    assert_eq!(
        recorded_watcher(&harness.control, child_thread_id),
        Some(second_watcher)
    );

    let turn = child_thread.session.new_default_turn().await;
    child_thread
        .session
        .send_event(
            turn.as_ref(),
            EventMsg::TurnComplete(TurnCompleteEvent {
                root_turn_id: None,
                turn_id: turn.sub_id.clone(),
                started_at: None,
                last_agent_message: Some("child completion".to_string()),
                error: None,
                completed_at: None,
                duration_ms: None,
                time_to_first_token_ms: None,
            }),
        )
        .await;
    assert_eq!(wait_for_subagent_notification(&parent_thread).await, true);
    let history = parent_thread.session.clone_history().await;
    assert!(history_contains_text(history.raw_items(), "second watcher"));
    assert!(!history_contains_text(history.raw_items(), "first watcher"));
    wait_until_no_recorded_watcher(&harness.control, child_thread_id).await;
}
