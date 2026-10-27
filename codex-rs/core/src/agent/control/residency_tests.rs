use super::ThreadEvictionOutcome;
use crate::StartThreadOptions;
use crate::ThreadManager;
use crate::agent::LocalAgentControl;
use crate::agent::types::AgentMetadata;
use crate::codex_thread::CodexThread;
use crate::config::Config;
use crate::config::test_config;
use crate::thread_manager::ThreadManagerState;
use codex_features::Feature;
use codex_login::CodexAuth;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_protocol::protocol::ThreadSource;
use codex_protocol::protocol::TurnAbortReason;
use codex_protocol::protocol::TurnAbortedEvent;
use codex_protocol::protocol::TurnCompleteEvent;
use pretty_assertions::assert_eq;
use std::sync::Arc;

#[tokio::test]
async fn residency_slot_reservation_unloads_oldest_idle_v2_agent() {
    let mut config = test_config().await;
    let _ = config.features.enable(Feature::MultiAgentV2);
    config.multi_agent_v2.max_concurrent_threads_per_session = 2;
    let temp_home = tempfile::tempdir().expect("create temp home");
    config.codex_home = temp_home.path().to_path_buf().try_into().unwrap();
    config.cwd = temp_home.path().to_path_buf().try_into().unwrap();
    let manager = ThreadManager::with_models_provider_and_home_for_tests(
        CodexAuth::from_api_key("dummy"),
        config.model_provider.clone(),
        config.codex_home.to_path_buf(),
        Arc::new(codex_exec_server::EnvironmentManager::default_for_tests()),
    );
    let root = manager
        .start_thread(StartThreadOptions::new(config.clone()))
        .await
        .expect("start root thread");
    let control = manager.agent_control();
    let state = control
        .runtime
        .upgrade()
        .expect("thread manager should be live");
    let membership = control.runtime.admit_start().expect("admit residency work");

    let first_slot = control
        .reserve_v2_residency_slot(
            &state,
            &config,
            &membership,
            /*protected_thread_id*/ None,
        )
        .await
        .expect("first resident slot");
    let first =
        spawn_v2_subagent(&control, &state, config.clone(), root.thread_id, "worker-1").await;
    first_slot.commit(first.thread_id);
    mark_thread_completed(first.thread.as_ref()).await;

    let second_slot = control
        .reserve_v2_residency_slot(
            &state,
            &config,
            &membership,
            /*protected_thread_id*/ None,
        )
        .await
        .expect("second resident slot should evict the first idle agent");
    match manager.get_thread(first.thread_id).await {
        Err(err) => match err.details() {
            CodexErrorDetails::ThreadNotFound(thread_id) => assert_eq!(*thread_id, first.thread_id),
            _ => panic!("expected evicted thread to be missing, got {err:?}"),
        },
        Ok(_) => panic!("expected evicted thread to be missing"),
    }
    let second = spawn_v2_subagent(&control, &state, config, root.thread_id, "worker-2").await;
    second_slot.commit(second.thread_id);

    assert!(manager.get_thread(root.thread_id).await.is_ok());
    assert!(manager.get_thread(second.thread_id).await.is_ok());
}

#[tokio::test]
async fn interrupted_v2_agent_is_lost_after_residency_eviction() {
    let mut config = test_config().await;
    let _ = config.features.enable(Feature::MultiAgentV2);
    config.multi_agent_v2.max_concurrent_threads_per_session = 2;
    let temp_home = tempfile::tempdir().expect("create temp home");
    config.codex_home = temp_home.path().to_path_buf().try_into().unwrap();
    config.cwd = temp_home.path().to_path_buf().try_into().unwrap();
    let manager = ThreadManager::with_models_provider_and_home_for_tests(
        CodexAuth::from_api_key("dummy"),
        config.model_provider.clone(),
        config.codex_home.to_path_buf(),
        Arc::new(codex_exec_server::EnvironmentManager::default_for_tests()),
    );
    let root = manager
        .start_thread(StartThreadOptions::new(config.clone()))
        .await
        .expect("start root thread");
    let control = manager.agent_control();
    let state = control
        .runtime
        .upgrade()
        .expect("thread manager should be live");
    let membership = control.runtime.admit_start().expect("admit residency work");

    let first_slot = control
        .reserve_v2_residency_slot(
            &state,
            &config,
            &membership,
            /*protected_thread_id*/ None,
        )
        .await
        .expect("first resident slot");
    let first =
        spawn_v2_subagent(&control, &state, config.clone(), root.thread_id, "worker-1").await;
    first_slot.commit(first.thread_id);
    mark_thread_interrupted(first.thread.as_ref()).await;

    let second_slot = control
        .reserve_v2_residency_slot(
            &state,
            &config,
            &membership,
            /*protected_thread_id*/ None,
        )
        .await
        .expect("second resident slot should evict the first interrupted idle agent");
    match manager.get_thread(first.thread_id).await {
        Err(err) => match err.details() {
            CodexErrorDetails::ThreadNotFound(thread_id) => assert_eq!(*thread_id, first.thread_id),
            _ => panic!("expected evicted thread to be missing, got {err:?}"),
        },
        Ok(_) => panic!("expected evicted thread to be missing"),
    }
    let second =
        spawn_v2_subagent(&control, &state, config.clone(), root.thread_id, "worker-2").await;
    second_slot.commit(second.thread_id);
    mark_thread_completed(second.thread.as_ref()).await;

    let err = control
        .ensure_v2_agent_loaded(config, first.thread_id, /*parent*/ None)
        .await
        .expect_err("evicted interrupted agent should stay lost");
    match err.details() {
        CodexErrorDetails::ThreadNotFound(thread_id) => assert_eq!(*thread_id, first.thread_id),
        _ => panic!("expected ThreadNotFound, got {err:?}"),
    }

    assert!(manager.get_thread(root.thread_id).await.is_ok());
    assert!(manager.get_thread(second.thread_id).await.is_ok());
    match manager.get_thread(first.thread_id).await {
        Err(err) => match err.details() {
            CodexErrorDetails::ThreadNotFound(thread_id) => assert_eq!(*thread_id, first.thread_id),
            _ => panic!("expected evicted thread to be missing, got {err:?}"),
        },
        Ok(_) => panic!("expected evicted thread to be missing"),
    }
}

#[tokio::test]
async fn grandchild_result_is_retained_for_evicted_parent() {
    let mut config = test_config().await;
    let _ = config.features.enable(Feature::MultiAgentV2);
    let temp_home = tempfile::tempdir().expect("create temp home");
    config.codex_home = temp_home.path().to_path_buf().try_into().unwrap();
    config.cwd = temp_home.path().to_path_buf().try_into().unwrap();
    let manager = ThreadManager::with_models_provider_and_home_for_tests(
        CodexAuth::from_api_key("dummy"),
        config.model_provider.clone(),
        config.codex_home.to_path_buf(),
        Arc::new(codex_exec_server::EnvironmentManager::default_for_tests()),
    );
    let root = manager
        .start_thread(StartThreadOptions::new(config.clone()))
        .await
        .expect("start root thread");
    let control = manager.agent_control();
    let state = control
        .runtime
        .upgrade()
        .expect("thread manager should be live");
    let membership = control.runtime.admit_start().expect("admit residency work");

    let worker_path = AgentPath::root().join("worker").expect("worker path");
    let tester_path = worker_path.join("tester").expect("tester path");
    let worker = spawn_thread_spawn_agent(
        &control,
        &state,
        config.clone(),
        root.thread_id,
        worker_path.clone(),
        /*depth*/ 1,
    )
    .await;
    let tester = spawn_thread_spawn_agent(
        &control,
        &state,
        config,
        worker.thread_id,
        tester_path.clone(),
        /*depth*/ 2,
    )
    .await;

    mark_thread_completed(worker.thread.as_ref()).await;
    let outcome = state
        .try_evict_v2_thread(Arc::clone(&worker.thread), &membership)
        .await
        .expect("evict idle worker");
    assert!(matches!(outcome, ThreadEvictionOutcome::Evicted));
    assert!(manager.get_thread(worker.thread_id).await.is_err());

    mark_thread_completed(tester.thread.as_ref()).await;

    let retained = control.runtime.mailboxes.take(worker.thread_id);
    let [mail] = retained.as_slice() else {
        panic!("expected one retained result for the evicted worker, got {retained:?}");
    };
    assert_eq!(mail.author, tester_path);
    assert_eq!(mail.recipient, worker_path);
    assert!(!mail.trigger_turn);
}

/// Fork: a V2 agent with a queued notification that would start its turn stays resident.
#[tokio::test]
async fn queued_background_terminal_notification_blocks_eviction() {
    let mut config = test_config().await;
    let _ = config.features.enable(Feature::MultiAgentV2);
    let temp_home = tempfile::tempdir().expect("create temp home");
    config.codex_home = temp_home.path().to_path_buf().try_into().unwrap();
    config.cwd = temp_home.path().to_path_buf().try_into().unwrap();
    let manager = ThreadManager::with_models_provider_and_home_for_tests(
        CodexAuth::from_api_key("dummy"),
        config.model_provider.clone(),
        config.codex_home.to_path_buf(),
        Arc::new(codex_exec_server::EnvironmentManager::default_for_tests()),
    );
    let root = manager
        .start_thread(StartThreadOptions::new(config.clone()))
        .await
        .expect("start root thread");
    let control = manager.agent_control();
    let state = control
        .runtime
        .upgrade()
        .expect("thread manager should be live");
    let membership = control.runtime.admit_start().expect("admit residency work");
    let worker = spawn_thread_spawn_agent(
        &control,
        &state,
        config,
        root.thread_id,
        AgentPath::root().join("worker").expect("worker path"),
        /*depth*/ 1,
    )
    .await;
    mark_thread_completed(worker.thread.as_ref()).await;
    worker
        .thread
        .session
        .input_queue
        .enqueue_background_notification(/*key*/ None, Some(1000), /*wakes*/ true, |_| {
            crate::context::ContextualUserFragment::into(
                crate::context::BackgroundTerminalNotification::new("exited"),
            )
        })
        .await;

    let outcome = state
        .try_evict_v2_thread(Arc::clone(&worker.thread), &membership)
        .await
        .expect("try to evict worker");
    assert_eq!(outcome, ThreadEvictionOutcome::Busy);
    assert!(manager.get_thread(worker.thread_id).await.is_ok());

    // Once the notification is consumed the idle worker can be unloaded again.
    worker
        .thread
        .session
        .input_queue
        .drain_mailbox_input_items()
        .await;
    let outcome = state
        .try_evict_v2_thread(Arc::clone(&worker.thread), &membership)
        .await
        .expect("evict idle worker");
    assert_eq!(outcome, ThreadEvictionOutcome::Evicted);
}

/// Fork (R3.6): a notification that never starts a turn does not keep its agent resident;
/// the shutdown records it in the agent's history (R2.5).
#[tokio::test]
async fn passive_background_terminal_notification_does_not_block_eviction() {
    let mut config = test_config().await;
    let _ = config.features.enable(Feature::MultiAgentV2);
    let temp_home = tempfile::tempdir().expect("create temp home");
    config.codex_home = temp_home.path().to_path_buf().try_into().unwrap();
    config.cwd = temp_home.path().to_path_buf().try_into().unwrap();
    let manager = ThreadManager::with_models_provider_and_home_for_tests(
        CodexAuth::from_api_key("dummy"),
        config.model_provider.clone(),
        config.codex_home.to_path_buf(),
        Arc::new(codex_exec_server::EnvironmentManager::default_for_tests()),
    );
    let root = manager
        .start_thread(StartThreadOptions::new(config.clone()))
        .await
        .expect("start root thread");
    let control = manager.agent_control();
    let state = control
        .runtime
        .upgrade()
        .expect("thread manager should be live");
    let membership = control.runtime.admit_start().expect("admit residency work");
    let worker = spawn_thread_spawn_agent(
        &control,
        &state,
        config,
        root.thread_id,
        AgentPath::root().join("worker").expect("worker path"),
        /*depth*/ 1,
    )
    .await;
    mark_thread_completed(worker.thread.as_ref()).await;
    worker
        .thread
        .session
        .input_queue
        .enqueue_background_notification(/*key*/ None, Some(1000), /*wakes*/ false, |_| {
            crate::context::ContextualUserFragment::into(
                crate::context::BackgroundTerminalNotification::new(
                    "terminated at the user's request",
                ),
            )
        })
        .await;

    let outcome = state
        .try_evict_v2_thread(Arc::clone(&worker.thread), &membership)
        .await
        .expect("evict worker");
    assert_eq!(outcome, ThreadEvictionOutcome::Evicted);
    let history = worker.thread.session.clone_history().await;
    assert_eq!(
        history
            .raw_items()
            .filter(
                |item| crate::context::BackgroundTerminalNotification::is_notification_item(item)
            )
            .count(),
        1
    );
}

async fn spawn_v2_subagent(
    control: &LocalAgentControl,
    state: &Arc<ThreadManagerState>,
    config: Config,
    parent_thread_id: ThreadId,
    label: &str,
) -> crate::thread_manager::NewThread {
    state
        .spawn_child_thread(
            crate::thread_manager::StartThreadOptions {
                session_source: Some(SessionSource::SubAgent(SubAgentSource::Other(
                    label.to_string(),
                ))),
                thread_source: Some(ThreadSource::Subagent),
                ..crate::thread_manager::StartThreadOptions::new(config)
            },
            control.clone(),
            Some(parent_thread_id),
            /*inherited_exec_policy*/ None,
        )
        .await
        .expect("spawn v2 subagent")
}

async fn spawn_thread_spawn_agent(
    control: &LocalAgentControl,
    state: &Arc<ThreadManagerState>,
    config: Config,
    parent_thread_id: ThreadId,
    agent_path: AgentPath,
    depth: i32,
) -> crate::thread_manager::NewThread {
    let reservation = control
        .runtime
        .registry
        .reserve_spawn_slot(/*max_threads*/ None)
        .expect("reserve spawn slot");
    let agent = state
        .spawn_child_thread(
            crate::thread_manager::StartThreadOptions {
                session_source: Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id,
                    depth,
                    agent_path: Some(agent_path.clone()),
                    agent_nickname: None,
                    agent_role: None,
                })),
                thread_source: Some(ThreadSource::Subagent),
                ..crate::thread_manager::StartThreadOptions::new(config)
            },
            control.clone(),
            Some(parent_thread_id),
            /*inherited_exec_policy*/ None,
        )
        .await
        .expect("spawn thread-spawn agent");
    // Register the agent as the spawn tool does, so routing knows it after eviction.
    reservation.commit(AgentMetadata {
        agent_id: Some(agent.thread_id),
        agent_path: Some(agent_path),
        ..Default::default()
    });
    agent
}

async fn mark_thread_completed(thread: &CodexThread) {
    let turn = thread.session.new_default_turn().await;
    thread
        .session
        .send_event(
            turn.as_ref(),
            EventMsg::TurnComplete(TurnCompleteEvent {
                root_turn_id: None,
                turn_id: turn.sub_id.clone(),
                started_at: None,
                last_agent_message: Some("done".to_string()),
                error: None,
                completed_at: None,
                duration_ms: None,
                time_to_first_token_ms: None,
            }),
        )
        .await;
    clear_active_turn(thread).await;
}

async fn mark_thread_interrupted(thread: &CodexThread) {
    let turn = thread.session.new_default_turn().await;
    thread
        .session
        .send_event(
            turn.as_ref(),
            EventMsg::TurnAborted(TurnAbortedEvent {
                root_turn_id: None,
                turn_id: Some(turn.sub_id.clone()),
                started_at: None,
                reason: TurnAbortReason::Interrupted,
                error: None,
                completed_at: None,
                duration_ms: None,
            }),
        )
        .await;
    clear_active_turn(thread).await;
}

async fn clear_active_turn(thread: &CodexThread) {
    // The fixture has no task runner to clear the turn after the terminal event.
    *thread.session.active_turn.lock().await = None;
}
