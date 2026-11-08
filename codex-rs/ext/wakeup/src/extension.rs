//! The extension's contributors (`specs/scheduled-wakeups-2026-11-07.md`, UC1, UC3–UC5).

use std::sync::Arc;
use std::sync::Weak;
use std::time::Duration;

use chrono::Utc;
use codex_core::ThreadManager;
use codex_core::config::ScheduledWakeupsConfig;
use codex_core::context::InternalContextSource;
use codex_core::context::InternalModelContextFragment;
use codex_core::fork_wakeup::WakeupBeacon;
use codex_extension_api::ConfigContributor;
use codex_extension_api::ContextContributor;
use codex_extension_api::ContextualUserFragment;
use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::ExtensionMetrics;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::PromptFragment;
use codex_extension_api::PromptSlot;
use codex_extension_api::ThreadIdleCause;
use codex_extension_api::ThreadIdleInput;
use codex_extension_api::ThreadLifecycleContributor;
use codex_extension_api::ThreadReadyInput;
use codex_extension_api::ThreadStartInput;
use codex_extension_api::ThreadStopInput;
use codex_extension_api::ToolCall;
use codex_extension_api::ToolContributor;
use codex_extension_api::ToolExecutor;
use codex_extension_api::TurnAbortInput;
use codex_extension_api::TurnContextContributionInput;
use codex_extension_api::TurnInputContext;
use codex_extension_api::TurnInputContributor;
use codex_extension_api::TurnLifecycleContributor;
use codex_extension_api::TurnStartInput;
use codex_extension_api::TurnStopInput;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_protocol::protocol::TurnAbortReason;

use crate::scheduler::ThreadManagerHost;
use crate::scheduler::fire_if_due;
use crate::state::Eligibility;
use crate::state::WakeupThreadState;
use crate::tools::CancelWakeupTool;
use crate::tools::ScheduleWakeupTool;

/// How long context contributors wait for a start under way to settle, so they know whether
/// the turn is the extension's.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(5);

/// The thread's effective feature flag and limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakeupExtensionConfig {
    pub enabled: bool,
    pub limits: ScheduledWakeupsConfig,
    /// `unified_exec` and `exec_exit_notifications` are both on, so `exec_command` offers
    /// `notify_on_*` and the wakeup tool points to them for events
    /// (`specs/fork-tool-guidance-2026-11-08.md` UC1, R1.3).
    pub exit_notifications: bool,
}

pub struct WakeupExtension<C> {
    thread_manager: Weak<ThreadManager>,
    config: Arc<dyn Fn(&C) -> WakeupExtensionConfig + Send + Sync>,
}

impl<C> std::fmt::Debug for WakeupExtension<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WakeupExtension").finish_non_exhaustive()
    }
}

fn thread_state(thread_store: &ExtensionData) -> Option<Arc<WakeupThreadState>> {
    thread_store.get::<WakeupThreadState>()
}

/// R1.3: root threads may use wakeups; a MultiAgentV2 child once it is known not to be a
/// workflow agent; Guardian, review, compaction, memory and internal sessions and V1
/// children never.
fn eligibility_for(session_source: &SessionSource) -> Eligibility {
    match session_source {
        SessionSource::Cli
        | SessionSource::VSCode
        | SessionSource::Exec
        | SessionSource::Mcp
        | SessionSource::Custom(_) => Eligibility::Eligible,
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            agent_path: Some(_),
            ..
        }) => Eligibility::PendingCheck,
        SessionSource::SubAgent(_) | SessionSource::Internal(_) | SessionSource::Unknown => {
            Eligibility::NotEligible
        }
    }
}

impl<C> ThreadLifecycleContributor<C> for WakeupExtension<C>
where
    C: Send + Sync + 'static,
{
    fn on_thread_start<'a>(&'a self, input: ThreadStartInput<'a, C>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            let config = (self.config)(input.config);
            let beacon = input
                .thread_store
                .get_or_init::<WakeupBeacon>(WakeupBeacon::default);
            let state = WakeupThreadState::new(
                input.thread_store.level_id().to_string(),
                config.enabled,
                config.limits,
                eligibility_for(input.session_source),
                beacon,
                Arc::new(ThreadManagerHost {
                    thread_manager: self.thread_manager.clone(),
                }),
                Arc::new(Utc::now),
            );
            state.set_exit_notifications(config.exit_notifications);
            input.thread_store.insert_arc(state);
        })
    }

    fn on_thread_ready<'a>(&'a self, input: ThreadReadyInput<'a, C>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            let Some(state) = thread_state(input.thread_store) else {
                return;
            };
            if state.eligibility() != Eligibility::PendingCheck {
                return;
            }
            let Some(thread_manager) = self.thread_manager.upgrade() else {
                return;
            };
            let Ok(thread_id) = ThreadId::from_string(input.thread_store.level_id()) else {
                return;
            };
            let Ok(thread) = thread_manager.get_thread(thread_id).await else {
                return;
            };
            state.set_eligibility(if codex_core::fork_wakeup::is_workflow_child(&thread) {
                Eligibility::NotEligible
            } else {
                Eligibility::Eligible
            });
        })
    }

    /// A due wakeup fires after a completed or failed turn, never after an interrupt (R3.2,
    /// R3.3); failures of the extension's turns are counted first (R3.8).
    fn on_thread_idle<'a>(&'a self, input: ThreadIdleInput<'a>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            let Some(state) = thread_state(input.thread_store) else {
                return;
            };
            match input.cause {
                ThreadIdleCause::Interrupted => {}
                ThreadIdleCause::Completed | ThreadIdleCause::Failed => {
                    state.on_idle(input.cause == ThreadIdleCause::Failed);
                    fire_if_due(state).await;
                }
            }
        })
    }

    /// R4.4: the timer stops before persistence closes and no turn starts any more.
    fn on_thread_stop<'a>(&'a self, input: ThreadStopInput<'a>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            if let Some(state) = thread_state(input.thread_store) {
                state.close();
            }
        })
    }
}

impl<C> ConfigContributor<C> for WakeupExtension<C>
where
    C: Send + Sync + 'static,
{
    fn on_config_changed(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
        _previous_config: &C,
        new_config: &C,
    ) {
        let config = (self.config)(new_config);
        if let Some(state) = thread_state(thread_store) {
            state.set_config(config.enabled, config.limits);
            state.set_exit_notifications(config.exit_notifications);
        }
    }
}

impl<C> TurnLifecycleContributor for WakeupExtension<C>
where
    C: Send + Sync + 'static,
{
    fn on_turn_start<'a>(&'a self, input: TurnStartInput<'a>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            if let Some(state) = thread_state(input.thread_store) {
                state.on_turn_started(input.turn_id);
            }
        })
    }

    fn on_turn_stop<'a>(&'a self, input: TurnStopInput<'a>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            if let Some(state) = thread_state(input.thread_store) {
                state.on_turn_finished(input.turn_store.level_id());
            }
        })
    }

    /// R4.3: an interrupt, or a budget limit, of the wakeup's own turn or of the turn that
    /// scheduled it stops it.
    fn on_turn_abort<'a>(&'a self, input: TurnAbortInput<'a>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            let Some(state) = thread_state(input.thread_store) else {
                return;
            };
            let turn_id = input.turn_store.level_id();
            match input.reason {
                TurnAbortReason::Interrupted | TurnAbortReason::BudgetLimited => {
                    state.on_turn_interrupted(turn_id);
                }
                TurnAbortReason::Replaced | TurnAbortReason::ReviewEnded => {
                    state.on_turn_finished(turn_id);
                }
            }
        })
    }
}

impl<C> ToolContributor for WakeupExtension<C>
where
    C: Send + Sync + 'static,
{
    fn tools(
        &self,
        _session_store: &ExtensionData,
        thread_store: &ExtensionData,
    ) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        let Some(state) = thread_state(thread_store) else {
            return Vec::new();
        };
        if !state.available() {
            return Vec::new();
        }
        vec![
            Arc::new(ScheduleWakeupTool {
                state: Arc::clone(&state),
            }),
            Arc::new(CancelWakeupTool { state }),
        ]
    }
}

impl<C> ContextContributor for WakeupExtension<C>
where
    C: Send + Sync + 'static,
{
    /// The notice of a cancel the model did not make (R4.5) and the loop context of an
    /// iteration (R5.2), as hidden context of the turn.
    fn contribute_turn_context<'a>(
        &'a self,
        input: TurnContextContributionInput<'a>,
    ) -> ExtensionFuture<'a, Vec<PromptFragment>> {
        Box::pin(async move {
            let Some(state) = thread_state(input.thread_store) else {
                return Vec::new();
            };
            settle(&state).await;
            let mut fragments = Vec::new();
            if let Some(notice) = state.take_notice() {
                fragments.push(PromptFragment::from_fragment(
                    PromptSlot::DeveloperCapabilities,
                    &InternalModelContextFragment::new(
                        InternalContextSource::from_static("scheduled_wakeup"),
                        notice,
                    ),
                ));
            }
            if let Some(context) = state.loop_context(input.turn_id) {
                fragments.push(PromptFragment::from_fragment(
                    PromptSlot::DeveloperCapabilities,
                    &InternalModelContextFragment::new(
                        InternalContextSource::from_static("loop"),
                        context,
                    ),
                ));
            }
            fragments
        })
    }
}

impl<C> TurnInputContributor for WakeupExtension<C>
where
    C: Send + Sync + 'static,
{
    /// A turn the extension did not start that receives user input starts the model's chain
    /// over (R2.4); a loop iteration's prompt does not (R5.6).
    fn contribute<'a>(
        &'a self,
        input: TurnInputContext<'a>,
        _extension_metrics: Option<Arc<dyn ExtensionMetrics>>,
        _session_store: &'a ExtensionData,
        thread_store: &'a ExtensionData,
        _turn_store: &'a ExtensionData,
    ) -> ExtensionFuture<'a, Vec<Box<dyn ContextualUserFragment + Send>>> {
        Box::pin(async move {
            if input.user_input.is_empty() {
                return Vec::new();
            }
            if let Some(state) = thread_state(thread_store) {
                settle(&state).await;
                state.on_user_input(&input.turn_id);
            }
            Vec::new()
        })
    }
}

/// Waits until a start under way has recorded whether its turn is the extension's.
async fn settle(state: &WakeupThreadState) {
    let _ = tokio::time::timeout(SETTLE_TIMEOUT, state.fire_lock.lock()).await;
}

/// Installs the extension. Register it after upstream's message queue and goal, so their idle
/// work starts first.
pub fn install<C>(
    registry: &mut ExtensionRegistryBuilder<C>,
    thread_manager: Weak<ThreadManager>,
    config: impl Fn(&C) -> WakeupExtensionConfig + Send + Sync + 'static,
) where
    C: Send + Sync + 'static,
{
    let extension = Arc::new(WakeupExtension {
        thread_manager,
        config: Arc::new(config),
    });
    registry.thread_lifecycle_contributor(extension.clone());
    registry.config_contributor(extension.clone());
    registry.turn_lifecycle_contributor(extension.clone());
    registry.tool_contributor(extension.clone());
    registry.prompt_contributor(extension.clone());
    registry.turn_input_contributor(extension);
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::AgentPath;
    use codex_protocol::protocol::InternalSessionSource;
    use pretty_assertions::assert_eq;

    /// R1.3: root sources may use wakeups, a MultiAgentV2 child after its check, everything
    /// else never.
    #[test]
    fn eligibility_follows_the_session_source() {
        let spawn = |agent_path: Option<AgentPath>| {
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: ThreadId::new(),
                depth: 1,
                agent_path,
                agent_nickname: None,
                agent_role: None,
            })
        };
        for source in [
            SessionSource::Cli,
            SessionSource::VSCode,
            SessionSource::Exec,
            SessionSource::Mcp,
            SessionSource::Custom("sdk".to_string()),
        ] {
            assert_eq!(
                eligibility_for(&source),
                Eligibility::Eligible,
                "{source:?}"
            );
        }
        assert_eq!(
            eligibility_for(&spawn(Some(
                AgentPath::root().join("ci_watch").expect("path")
            ))),
            Eligibility::PendingCheck
        );
        for source in [
            spawn(None),
            SessionSource::SubAgent(SubAgentSource::Review),
            SessionSource::SubAgent(SubAgentSource::Compact),
            SessionSource::SubAgent(SubAgentSource::MemoryConsolidation),
            SessionSource::SubAgent(SubAgentSource::Other("guardian".to_string())),
            SessionSource::Internal(InternalSessionSource::MemoryConsolidation),
            SessionSource::Unknown,
        ] {
            assert_eq!(
                eligibility_for(&source),
                Eligibility::NotEligible,
                "{source:?}"
            );
        }
    }
}
