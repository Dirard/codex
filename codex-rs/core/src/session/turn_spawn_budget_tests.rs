use super::*;
use crate::agent::control::AgentMetadata;
use crate::agent::control::AgentRegistry;
use codex_protocol::error::CodexErrorDetails;
use pretty_assertions::assert_eq;
use tokio_util::sync::CancellationToken;

async fn capture_step(
    session: &Arc<Session>,
    turn: Arc<TurnContext>,
) -> anyhow::Result<Arc<StepContext>> {
    Ok(session
        .capture_step_context(turn, &CancellationToken::new())
        .await?)
}

async fn make_limit_one_session() -> (
    Arc<Session>,
    Arc<TurnContext>,
    async_channel::Receiver<Event>,
) {
    make_session_and_context_with_auth_and_config_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        |config| config.max_spawned_threads_per_turn = 1,
    )
    .await
}

fn spend_slot(registry: &Arc<AgentRegistry>, step: &StepContext) {
    registry
        .reserve_spawn_slot(/*max_threads*/ None, Some(&step.turn_spawn_budget))
        .expect("turn budget should have capacity")
        .commit(AgentMetadata::default());
}

fn assert_budget_exhausted(registry: &Arc<AgentRegistry>, step: &StepContext, epoch: &str) {
    let error =
        match registry.reserve_spawn_slot(/*max_threads*/ None, Some(&step.turn_spawn_budget)) {
            Ok(_) => panic!("{epoch} turn budget should be exhausted"),
            Err(error) => error,
        };
    let CodexErrorDetails::AgentLimitReached { max_threads } = error.details() else {
        panic!("expected AgentLimitReached");
    };
    assert_eq!(*max_threads, 1);
}

fn root_user_input() -> TurnInputRequest {
    TurnInputRequest::user_input(Vec::new())
}

#[tokio::test]
async fn automatic_followup_does_not_reset_turn_spawn_budget() -> anyhow::Result<()> {
    let (session, turn, _rx) = make_limit_one_session().await;
    let first = capture_step(&session, Arc::clone(&turn)).await?;
    let second = capture_step(&session, turn).await?;
    let registry = Arc::new(AgentRegistry::default());

    spend_slot(&registry, &first);
    assert_budget_exhausted(&registry, &second, "follow-up");
    Ok(())
}

#[tokio::test]
async fn new_root_input_gets_independent_turn_spawn_budget() -> anyhow::Result<()> {
    let (session, turn, _rx) = make_limit_one_session().await;
    let old_step = capture_step(&session, turn).await?;
    let registry = Arc::new(AgentRegistry::default());
    spend_slot(&registry, &old_step);

    session.start_or_steer_turn(root_user_input()).await?;
    let new_step = capture_step(&session, session.new_default_turn().await).await?;

    assert_budget_exhausted(&registry, &old_step, "old root");
    spend_slot(&registry, &new_step);
    Ok(())
}

#[tokio::test]
async fn late_descendant_uses_original_budget_during_root_rollover() -> anyhow::Result<()> {
    let (session, turn, _rx) = make_limit_one_session().await;
    let old_descendant_step = capture_step(&session, turn).await?;

    session.start_or_steer_turn(root_user_input()).await?;
    let new_root_step = capture_step(&session, session.new_default_turn().await).await?;
    let registry = Arc::new(AgentRegistry::default());

    spend_slot(&registry, &old_descendant_step);
    spend_slot(&registry, &new_root_step);
    assert_budget_exhausted(&registry, &old_descendant_step, "old descendant");
    assert_budget_exhausted(&registry, &new_root_step, "new root");
    Ok(())
}
