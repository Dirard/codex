use crate::context::CurrentTimeUnavailable;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_extension_items::ExtensionItem;
use codex_extension_items::sleep::SleepItem;
use codex_features::Feature;
use codex_protocol::items::TurnItem;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiNamespace;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolExposure;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;
use std::time::Instant;

const NAMESPACE: &str = "clock";
const TOOL_NAME: &str = "sleep";
const MAX_SLEEP_DURATION_MS: u64 = 12 * 60 * 60 * 1000;

pub struct SleepHandler {
    /// Fork: background terminal notifications are on, so the description says that one also
    /// ends the sleep (`specs/fork-tool-guidance-2026-11-08.md` UC1, R1.1).
    exit_notifications: bool,
}

impl SleepHandler {
    /// Fork: the sleep tool, described with background terminal notifications when they are on.
    pub(crate) fn new(exit_notifications: bool) -> Self {
        Self { exit_notifications }
    }
}

/// Fork: what ends a sleep besides new input when background terminal notifications are on; the
/// wake comes from the same input activity (`InputQueueActivity::BackgroundTerminal`)
/// (`specs/fork-tool-guidance-2026-11-08.md` UC1, R1.1).
const SLEEP_EXIT_NOTIFICATION_GUIDANCE: &str = " A notification from a background terminal with notify_on_exit or notify_on_output also ends it early, so do not poll that terminal between sleeps.";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SleepArgs {
    duration_ms: u64,
}

fn create_sleep_tool(exit_notifications: bool) -> ToolSpec {
    let properties = BTreeMap::from([(
        "duration_ms".to_string(),
        JsonSchema::number(Some(format!(
            "How long to sleep in milliseconds. Must be between 1 and {MAX_SLEEP_DURATION_MS}."
        ))),
    )]);

    ToolSpec::Namespace(ResponsesApiNamespace {
        name: NAMESPACE.to_string(),
        description: "Tools for reading and waiting on time.".to_string(),
        tools: vec![ResponsesApiNamespaceTool::Function(ResponsesApiTool {
            name: TOOL_NAME.to_string(),
            description: format!(
                "Pause execution for a specified duration. The sleep ends early when new input arrives for the active turn. Returns the elapsed wall-clock time.{}",
                if exit_notifications {
                    SLEEP_EXIT_NOTIFICATION_GUIDANCE
                } else {
                    ""
                }
            ),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["duration_ms".to_string()]),
                /*additional_properties*/ Some(false.into()),
            ),
            output_schema: None,
        })],
    })
}

impl ToolExecutor<ToolInvocation> for SleepHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::namespaced(NAMESPACE, TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_sleep_tool(self.exit_notifications)
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::DirectModelOnly
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(async move {
            let ToolInvocation {
                session,
                turn,
                call_id,
                payload,
                ..
            } = invocation;
            let ToolPayload::Function { arguments } = payload else {
                return Err(FunctionCallError::RespondToModel(format!(
                    "{TOOL_NAME} handler received unsupported payload"
                )));
            };
            let args: SleepArgs = parse_arguments(&arguments)?;
            if !(1..=MAX_SLEEP_DURATION_MS).contains(&args.duration_ms) {
                return Err(FunctionCallError::RespondToModel(format!(
                    "duration_ms must be between 1 and {MAX_SLEEP_DURATION_MS}"
                )));
            }

            let started = Instant::now();
            let item = TurnItem::Extension(ExtensionItem::Sleep(SleepItem {
                id: call_id,
                duration_ms: args.duration_ms,
            }));
            session.emit_turn_item_started(turn.as_ref(), &item).await;
            let turn_state = session
                .input_queue
                .turn_state_for_sub_id(&session.active_turn, &turn.sub_id)
                .await;
            let (mut activity_rx, pending_activity) = session
                .input_queue
                .subscribe_activity(turn_state.as_deref())
                .await;
            let sleep_result = if pending_activity.is_some() {
                Ok(true)
            } else {
                let sleep = session
                    .services
                    .time_provider
                    .sleep(session.thread_id, Duration::from_millis(args.duration_ms));
                tokio::pin!(sleep);
                tokio::select! {
                    result = &mut sleep => result.map(|()| false),
                    result = activity_rx.changed() => {
                        if result.is_ok() {
                            Ok(true)
                        } else {
                            sleep.await.map(|()| false)
                        }
                    }
                }
            };
            session.emit_turn_item_completed(turn.as_ref(), item).await;
            let interrupted = sleep_result.map_err(|err| {
                if turn.config.features.enabled(Feature::NonfatalClockReadErrors) {
                    tracing::error!(
                        thread_id = %session.thread_id,
                        turn_id = %turn.sub_id,
                        "failed to read current time for the sleep tool; the clock provider may be stalled"
                    );
                    FunctionCallError::RespondToModel(CurrentTimeUnavailable::MESSAGE.to_string())
                } else {
                    FunctionCallError::Fatal(format!("failed to sleep: {err:#}"))
                }
            })?;

            let message = if interrupted {
                "Sleep interrupted by new input."
            } else {
                "Sleep completed."
            };
            let wall_time_seconds = started.elapsed().as_secs_f64();
            Ok(boxed_tool_output(FunctionToolOutput::from_text(
                format!("Wall time: {wall_time_seconds:.4} seconds\n{message}"),
                /*success*/ Some(true),
            )))
        })
    }
}

impl CoreToolRuntime for SleepHandler {
    fn is_builtin_control_tool(&self) -> bool {
        true
    }
}

/// Fork (`specs/fork-tool-guidance-2026-11-08.md` UC1, R1.1): only with background terminal
/// notifications does the description say that one ends the sleep.
#[cfg(test)]
mod fork_tests {
    use super::*;

    fn description(exit_notifications: bool) -> String {
        let ToolSpec::Namespace(namespace) = create_sleep_tool(exit_notifications) else {
            panic!("sleep is namespaced");
        };
        let [ResponsesApiNamespaceTool::Function(tool)] = namespace.tools.as_slice() else {
            panic!("one sleep function");
        };
        tool.description.clone()
    }

    #[test]
    fn the_description_names_notifications_only_when_they_are_on() {
        assert!(
            description(/*exit_notifications*/ true).ends_with(SLEEP_EXIT_NOTIFICATION_GUIDANCE)
        );
        assert_eq!(
            description(/*exit_notifications*/ false),
            "Pause execution for a specified duration. The sleep ends early when new input arrives for the active turn. Returns the elapsed wall-clock time."
        );
    }
}
