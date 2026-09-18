use super::*;
use crate::agent::registry::AgentRegistry;
use pretty_assertions::assert_eq;

#[test_case::test_case(MultiAgentVersion::V1; "v1_send_input")]
#[test_case::test_case(MultiAgentVersion::V2; "v2_followup")]
#[tokio::test]
async fn idle_followup_adopts_requester_spawn_budget(version: MultiAgentVersion) {
    let server = MockServer::start().await;
    let responses = responses::mount_sse_sequence(
        &server,
        ["first", "second"]
            .into_iter()
            .map(|id| {
                responses::sse(vec![
                    responses::ev_response_created(id),
                    responses::ev_assistant_message(id, id),
                    responses::ev_completed(id),
                ])
            })
            .collect(),
    )
    .await;
    let (home, mut config) = test_config().await;
    config.model_provider.base_url = Some(format!("{}/v1", server.uri()));
    config.model_provider.supports_websockets = false;
    config.max_spawned_threads_per_turn = 1;
    match version {
        MultiAgentVersion::V2 => {
            config
                .features
                .enable(Feature::MultiAgentV2)
                .expect("enable v2");
        }
        MultiAgentVersion::V1 => {
            config
                .features
                .disable(Feature::MultiAgentV2)
                .expect("disable v2");
        }
        MultiAgentVersion::Disabled => unreachable!("test specifies an agent backend"),
    }
    let harness = AgentControlHarness::new_with_config(home, config).await;
    let (parent_id, parent) = harness.start_thread().await;
    let parent_turn = parent.session.new_default_turn().await;
    let old_budget = TurnSpawnBudget::new(/*limit*/ 1);
    let child_path = AgentPath::root().join("worker").expect("child path");
    let input = || {
        vec![UserInput::Text {
            text: "work".to_string(),
            text_elements: Vec::new(),
        }]
    };
    let child = harness
        .control
        .spawn_agent_with_metadata(
            harness.config.clone(),
            input(),
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: parent_id,
                depth: 1,
                agent_path: Some(child_path),
                agent_nickname: None,
                agent_role: None,
            }),
            SpawnAgentOptions {
                parent_thread_id: Some(parent_id),
                parent_turn_id: Some(parent_turn.sub_id.clone()),
                turn_spawn_budget: Some(old_budget.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("spawn worker");
    let child_thread = harness
        .manager
        .get_thread(child.thread_id)
        .await
        .expect("resident child");
    wait_until_completed(&child_thread, "first").await;
    let old_step = child_thread
        .session
        .capture_step_context(
            child_thread.session.new_default_turn().await,
            &CancellationToken::new(),
        )
        .await
        .expect("capture first task budget");
    let registry = Arc::new(AgentRegistry::default());
    assert!(
        registry
            .reserve_spawn_slot(/*max_threads*/ None, Some(&old_step.turn_spawn_budget))
            .is_err()
    );

    let new_budget = TurnSpawnBudget::new(/*limit*/ 1);
    if version == MultiAgentVersion::V2 {
        harness
            .control
            .send(crate::SendRequest {
                caller: parent_id,
                target: crate::AgentTarget::Id(child.thread_id),
                resume_config: crate::agent::child_config::build_agent_resume_config(&parent_turn)
                    .expect("capture resume config"),
                input: crate::AgentInput::Message {
                    message: AgentMessage::Plaintext("information only".to_string()),
                    mode: MessageDeliveryMode::QueueOnly,
                },
                start_options: TurnStartOptions::default(),
                turn_spawn_budget: Some(new_budget.clone()),
            })
            .await
            .expect("queue-only message accepted");
        timeout(Duration::from_secs(5), async {
            while !child_thread
                .session
                .input_queue
                .has_pending_mailbox_items()
                .await
            {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("queue-only message queued");
        let queued_step = child_thread
            .session
            .capture_step_context(
                child_thread.session.new_default_turn().await,
                &CancellationToken::new(),
            )
            .await
            .expect("capture unchanged budget");
        assert!(
            registry
                .reserve_spawn_slot(
                    /*max_threads*/ None,
                    Some(&queued_step.turn_spawn_budget)
                )
                .is_err()
        );
        assert_eq!(responses.requests().len(), 1);
    }
    match version {
        MultiAgentVersion::V2 => {
            harness
                .control
                .send(crate::SendRequest {
                    caller: parent_id,
                    target: crate::AgentTarget::Id(child.thread_id),
                    resume_config: crate::agent::child_config::build_agent_resume_config(
                        &parent_turn,
                    )
                    .expect("capture resume config"),
                    input: crate::AgentInput::Message {
                        message: AgentMessage::Plaintext("new work".to_string()),
                        mode: MessageDeliveryMode::TriggerTurn,
                    },
                    start_options: TurnStartOptions {
                        parent_turn_id: Some(parent_turn.sub_id.clone()),
                        ..Default::default()
                    },
                    turn_spawn_budget: Some(new_budget.clone()),
                })
                .await
                .expect("follow-up accepted");
        }
        MultiAgentVersion::V1 => {
            harness
                .control
                .send_input_with_spawn_budget(
                    child.thread_id,
                    input(),
                    TurnStartOptions {
                        parent_turn_id: Some(parent_turn.sub_id.clone()),
                        ..Default::default()
                    },
                    Some(new_budget.clone()),
                )
                .await
                .expect("new input accepted");
        }
        MultiAgentVersion::Disabled => unreachable!("test specifies an agent backend"),
    }
    wait_until_completed(&child_thread, "second").await;
    let new_step = child_thread
        .session
        .capture_step_context(
            child_thread.session.new_default_turn().await,
            &CancellationToken::new(),
        )
        .await
        .expect("capture new task budget");
    let _slot = registry
        .reserve_spawn_slot(/*max_threads*/ None, Some(&new_step.turn_spawn_budget))
        .expect("new task must not retain the first task's exhausted budget");
    assert!(
        registry
            .reserve_spawn_slot(/*max_threads*/ None, Some(&new_budget))
            .is_err()
    );
    assert!(
        registry
            .reserve_spawn_slot(/*max_threads*/ None, Some(&old_step.turn_spawn_budget))
            .is_err()
    );
    assert_eq!(responses.requests().len(), 2);
}

async fn wait_until_completed(thread: &Arc<CodexThread>, message: &str) {
    timeout(Duration::from_secs(5), async {
        loop {
            if thread.agent_status().await == AgentStatus::Completed(Some(message.to_string()))
                && thread.session.active_turn.lock().await.is_none()
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("task should finish and become idle");
}
