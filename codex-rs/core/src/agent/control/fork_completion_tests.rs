//! Fork: completion watchers of legacy children (`fork_completion.rs`) and the wake of a
//! legacy child that loses its turn reservation (H3, H5).

use super::*;
use futures::future::AbortHandle;
use pretty_assertions::assert_eq;
use std::sync::PoisonError;

impl LocalAgentControl {
    /// Prepares and starts a follow-up watcher, as a wake that keeps its reservation does.
    pub(super) async fn start_followup_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        session_source: SessionSource,
        child_reference: String,
        child_agent_path: Option<AgentPath>,
        status_updates: StatusSubscription,
    ) {
        if let Some(watcher) = self
            .prepare_followup_completion_watcher(
                child_thread_id,
                session_source,
                child_reference,
                child_agent_path,
                status_updates,
            )
            .await
        {
            watcher.start();
        }
    }
}

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

/// A legacy child whose wake lost its turn reservation: the wake reserved its turn, took
/// `LOST_MAIL`, waited for the previous completion watcher, and an interrupt took the
/// reservation in that window.
struct LostWake {
    _server: MockServer,
    harness: AgentControlHarness,
    response_mock: responses::ResponseMock,
    control: LocalAgentControl,
    parent_thread_id: ThreadId,
    parent_thread: Arc<CodexThread>,
    child_thread_id: ThreadId,
    child_thread: Arc<CodexThread>,
    child_path: AgentPath,
}

const LOST_MAIL: &str = "first follow-up, taken by a wake that lost its turn";
const NEXT_MAIL: &str = "second follow-up";

impl LostWake {
    async fn new() -> Self {
        let server = MockServer::start().await;
        let response_mock = responses::mount_sse_once(
            &server,
            responses::sse(vec![
                responses::ev_response_created("second-response"),
                responses::ev_assistant_message("second-message", "second completion"),
                responses::ev_completed("second-response"),
            ]),
        )
        .await;
        let (home, mut config) = test_config().await;
        config.model_provider.base_url = Some(format!("{}/v1", server.uri()));
        config.model_provider.supports_websockets = false;
        let harness = AgentControlHarness::new_with_config(home, config).await;
        let mut parent_config = harness.config.clone();
        let _ = parent_config.features.enable(Feature::MultiAgentV2);
        let parent = harness
            .manager
            .start_thread(StartThreadOptions::new(parent_config))
            .await
            .expect("parent thread should start");
        let child_path = AgentPath::root().join("reviewer").expect("child path");
        let child_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: parent.thread_id,
            depth: 1,
            agent_path: Some(child_path.clone()),
            agent_nickname: None,
            agent_role: Some("reviewer".to_string()),
        });
        let mut child_config = harness.config.clone();
        child_config.agents_enabled = false;
        let child = harness
            .manager
            .start_thread(StartThreadOptions {
                session_source: Some(child_source),
                ..StartThreadOptions::new(child_config)
            })
            .await
            .expect("child thread should start");
        let child_thread = child.thread;

        let first_turn = child_thread.session.new_default_turn().await;
        child_thread
            .session
            .send_event(
                first_turn.as_ref(),
                EventMsg::TurnComplete(TurnCompleteEvent {
                    root_turn_id: None,
                    turn_id: first_turn.sub_id.clone(),
                    started_at: None,
                    last_agent_message: Some("first completion".to_string()),
                    error: None,
                    completed_at: None,
                    duration_ms: None,
                    time_to_first_token_ms: None,
                }),
            )
            .await;
        assert_eq!(
            child_thread.agent_status().await,
            AgentStatus::Completed(Some("first completion".to_string()))
        );

        // The watcher of the first turn ends only when the test releases it, so the wake waits
        // for it after it reserved its turn and took its mail.
        let control = child_thread
            .session
            .services
            .local_agent_runtime
            .control(child_thread.session.session_id());
        let (release_previous_watcher, previous_watcher_released) = oneshot::channel::<()>();
        let previous_watcher = tokio::spawn(async move {
            let _ = previous_watcher_released.await;
        });
        control
            .runtime
            .completion_watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                child.thread_id,
                (AbortHandle::new_pair().0, previous_watcher),
            );
        child_thread
            .session
            .input_queue
            .enqueue_mailbox_communication(
                InterAgentCommunication::new(
                    AgentPath::root(),
                    child_path.clone(),
                    Vec::new(),
                    LOST_MAIL.to_string(),
                    /*trigger_turn*/ true,
                )
                .into(),
                TurnStartOptions::default(),
            )
            .await;
        let session = Arc::clone(&child_thread.session);
        let wake = tokio::spawn(async move {
            session
                .maybe_start_turn_for_pending_work_with_sub_id("lost-wake".to_string())
                .await;
        });
        wait_until_no_recorded_watcher(&control, child.thread_id).await;

        // An interrupt takes the reservation before the wake starts its turn.
        child_thread
            .session
            .abort_all_tasks(TurnAbortReason::Interrupted)
            .await;
        release_previous_watcher
            .send(())
            .expect("the previous watcher should wait for its release");
        timeout(Duration::from_secs(5), wake)
            .await
            .expect("the wake should end")
            .expect("the wake should not panic");

        Self {
            _server: server,
            harness,
            response_mock,
            control,
            parent_thread_id: parent.thread_id,
            parent_thread: parent.thread,
            child_thread_id: child.thread_id,
            child_thread,
            child_path,
        }
    }

    /// Sends the next follow-up and waits until its completion reaches the parent; returns
    /// the model request of its turn.
    async fn next_followup_reaches_parent(&self) -> responses::ResponsesRequest {
        self.harness
            .control
            .send_inter_agent_communication(
                self.child_thread_id,
                InterAgentCommunication::new(
                    AgentPath::root(),
                    self.child_path.clone(),
                    Vec::new(),
                    NEXT_MAIL.to_string(),
                    /*trigger_turn*/ true,
                ),
                AgentCommunicationContext::new(
                    AgentCommunicationKind::Followup,
                    self.parent_thread_id,
                ),
                TurnStartOptions {
                    parent_turn_id: Some("parent-turn".to_string()),
                    root_turn_id: Some("parent-turn".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("follow-up should be accepted");
        timeout(Duration::from_secs(10), async {
            while !history_contains_text(
                self.parent_thread.session.clone_history().await.raw_items(),
                "second completion",
            ) {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the next follow-up should reach the parent");
        self.response_mock.single_request()
    }
}

/// H3: a wake that loses its turn reservation while it prepares the follow-up watcher starts
/// no watcher. A watcher left behind would report a turn the wake did not start and would
/// hold up the watcher of the next follow-up.
#[tokio::test]
async fn wake_that_loses_its_reservation_starts_no_followup_watcher() {
    let lost_wake = LostWake::new().await;

    assert_eq!(
        recorded_watcher(&lost_wake.control, lost_wake.child_thread_id),
        None
    );
    lost_wake.next_followup_reaches_parent().await;
}

/// H5: a wake that loses its turn reservation puts the mail it took back in the mailbox,
/// where the next turn finds it ahead of later mail.
#[tokio::test]
async fn wake_that_loses_its_reservation_returns_its_mail() {
    let lost_wake = LostWake::new().await;

    assert!(
        lost_wake
            .child_thread
            .session
            .input_queue
            .has_trigger_turn_mailbox_items()
            .await,
        "the lost wake's mail should be back in the mailbox"
    );
    let request = serde_json::to_string(&lost_wake.next_followup_reaches_parent().await.input())
        .expect("serialize request input");
    let lost_mail = request
        .find(LOST_MAIL)
        .expect("the next turn should receive the lost wake's mail");
    let next_mail = request
        .find(NEXT_MAIL)
        .expect("the next turn should receive its own mail");
    assert!(lost_mail < next_mail, "the lost mail keeps its place");
}
