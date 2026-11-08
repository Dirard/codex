//! `clock.schedule_wakeup` and `clock.cancel_wakeup` (`specs/scheduled-wakeups-2026-11-07.md`,
//! UC1, UC2, UC4).
//!
//! Both join upstream's `clock` namespace (same description, merged by the tool plan) and are
//! `DirectModelOnly` like `clock.sleep`: Code Mode `exec` sees only a stub and workflow
//! scripts cannot call them (R1.4).

use std::collections::BTreeMap;
use std::sync::Arc;

use codex_core::config::ScheduledWakeupsConfig;
use codex_extension_api::FunctionCallError;
use codex_extension_api::ToolCall;
use codex_extension_api::ToolExecutor;
use codex_extension_api::ToolName;
use codex_extension_api::ToolOutput;
use codex_extension_api::ToolPayload;
use codex_extension_api::ToolSpec;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiNamespace;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolExposure;
use serde::Deserialize;

use crate::state::CancelReason;
use crate::state::ScheduleArgs;
use crate::state::WakeupThreadState;

pub const CLOCK_NAMESPACE: &str = "clock";
pub const SCHEDULE_WAKEUP_TOOL_NAME: &str = "schedule_wakeup";
pub const CANCEL_WAKEUP_TOOL_NAME: &str = "cancel_wakeup";
/// Upstream's description of the `clock` namespace; namespaces merge only when it matches.
const CLOCK_NAMESPACE_DESCRIPTION: &str = "Tools for reading and waiting on time.";

fn clock_namespace(tool: ResponsesApiTool) -> ToolSpec {
    ToolSpec::Namespace(ResponsesApiNamespace {
        name: CLOCK_NAMESPACE.to_string(),
        description: CLOCK_NAMESPACE_DESCRIPTION.to_string(),
        tools: vec![ResponsesApiNamespaceTool::Function(tool)],
    })
}

/// How to wait when background notifications are on: prefer an event over a timer, leave
/// sub-agents to `wait_agent` (whether their answers wake an idle thread depends on
/// `wake_on_agent_messages` and the mode, which this extension does not know), keep at most one
/// long fallback and cancel it (`specs/fork-tool-guidance-2026-11-08.md` UC1, R1.1, R1.2;
/// replaces UC1 of `specs/wakeup-trigger-guidance-2026-11-08.md`).
const WAIT_WITH_EVENTS: &str = "Resume this thread after a delay, with `prompt` as new input; end your turn after the call. To wake up when something happens, prefer exec_command with notify_on_exit or notify_on_output: Codex resumes you as soon as the command exits or prints a matching line, so one bounded background check loop beats a series of wakeups. Use a wakeup for what no shell command can check: a state you can see only through other tools or your own judgment, a moment within the next hour, or the pace of a /loop. Do not schedule wakeups to wait for background commands that already notify you or for sub-agents (wait_agent says how their answers reach you), except one long fallback (20-60 minutes) in case one hangs; cancel it with clock.cancel_wakeup once the work reports.";
/// The same without background notifications: no reference to `notify_on_*` (fork tool
/// guidance UC1, R1.2, R1.3).
const WAIT_WITHOUT_EVENTS: &str = "Resume this thread after a delay, with `prompt` as new input; end your turn after the call. Use it to check CI, a deploy or a long job instead of sleeping or polling. Do not schedule wakeups to wait for sub-agents (wait_agent says how their answers reach you), except one long fallback (20-60 minutes) in case one hangs; cancel it with clock.cancel_wakeup once the work reports.";
/// Delay choice and lifetime, shared by both variants (wakeup trigger guidance R1.3).
const WAKEUP_DELAY_AND_LIFETIME: &str = " Pick the delay from how fast the state changes: a build of about 8 minutes deserves one check after about 8 minutes, not eight checks a minute apart. One wakeup per thread; a new call replaces it. It lasts only while this Codex session runs.";

/// Model chooses how to wait: the wakeup tool, with the clamp range of the effective config
/// and, when background notifications are on, a pointer to waking on events instead
/// (`specs/wakeup-trigger-guidance-2026-11-08.md` UC1, `specs/fork-tool-guidance-2026-11-08.md`
/// UC1).
pub(crate) fn schedule_wakeup_spec(
    limits: &ScheduledWakeupsConfig,
    exit_notifications: bool,
) -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "delay_seconds".to_string(),
            JsonSchema::integer(Some(format!(
                "Seconds from now; clamped to {}-{}. Match it to what you wait for.",
                limits.min_delay_seconds, limits.max_delay_seconds
            ))),
        ),
        (
            "prompt".to_string(),
            JsonSchema::string(Some(
                "Instructions for your future self that stand on their own: what to check and what to do next. Required except in a self-paced /loop.".to_string(),
            )),
        ),
        (
            "reason".to_string(),
            JsonSchema::string(Some(
                "Short reason shown to the user: what you are waiting for.".to_string(),
            )),
        ),
    ]);
    let guidance = if exit_notifications {
        WAIT_WITH_EVENTS
    } else {
        WAIT_WITHOUT_EVENTS
    };
    clock_namespace(ResponsesApiTool {
        name: SCHEDULE_WAKEUP_TOOL_NAME.to_string(),
        description: format!("{guidance}{WAKEUP_DELAY_AND_LIFETIME}"),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["delay_seconds".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

/// `clock.cancel_wakeup`, with when to call it (`specs/fork-tool-guidance-2026-11-08.md` UC1,
/// R1.1).
pub(crate) fn cancel_wakeup_spec() -> ToolSpec {
    clock_namespace(ResponsesApiTool {
        name: CANCEL_WAKEUP_TOOL_NAME.to_string(),
        description: "Cancel this thread's pending wakeup and stop its /loop: a fallback once the work it guarded has reported, or a loop whose goal is met.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(BTreeMap::new(), Some(Vec::new()), Some(false.into())),
        output_schema: None,
    })
}

/// Plain-text tool output.
pub(crate) struct TextOutput {
    text: String,
}

impl ToolOutput for TextOutput {
    fn log_output(&self) -> String {
        self.text.clone()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        let output = FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(self.text.clone()),
            success: Some(true),
        };
        if matches!(payload, ToolPayload::Custom { .. }) {
            return ResponseInputItem::CustomToolCallOutput {
                call_id: call_id.to_string(),
                name: None,
                output,
            };
        }
        ResponseInputItem::FunctionCallOutput {
            call_id: call_id.to_string(),
            output,
        }
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> serde_json::Value {
        serde_json::Value::String(self.text.clone())
    }
}

fn text_output(text: String) -> Box<dyn ToolOutput> {
    Box::new(TextOutput { text })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScheduleArgs {
    delay_seconds: serde_json::Value,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

fn function_arguments<'a>(call: &'a ToolCall<'_>) -> Result<&'a str, FunctionCallError> {
    call.function_arguments()
}

pub(crate) fn parse_schedule_args(arguments: &str) -> Result<ScheduleArgs, String> {
    let raw: RawScheduleArgs = serde_json::from_str(arguments)
        .map_err(|error| format!("Wakeup not scheduled: invalid arguments: {error}"))?;
    let delay_seconds = raw
        .delay_seconds
        .as_u64()
        .filter(|delay| *delay > 0)
        .ok_or_else(|| {
            "Wakeup not scheduled: delay_seconds must be a positive integer.".to_string()
        })?;
    Ok(ScheduleArgs {
        delay_seconds,
        prompt: raw.prompt,
        reason: raw.reason,
    })
}

pub(crate) struct ScheduleWakeupTool {
    pub(crate) state: Arc<WakeupThreadState>,
}

impl<'call> ToolExecutor<ToolCall<'call>> for ScheduleWakeupTool {
    fn tool_name(&self) -> ToolName {
        ToolName::namespaced(CLOCK_NAMESPACE, SCHEDULE_WAKEUP_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        schedule_wakeup_spec(&self.state.limits(), self.state.exit_notifications())
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::DirectModelOnly
    }

    /// Check the arguments, the loop mode and the chain's limits, then schedule (UC2).
    fn handle<'a>(&'a self, invocation: ToolCall<'call>) -> codex_tools::ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            let args = parse_schedule_args(function_arguments(&invocation)?)
                .map_err(FunctionCallError::RespondToModel)?;
            // No `fire_lock`: the calling turn runs, so no start of this thread is under way.
            self.state
                .schedule_from_model(args, &invocation.turn_id)
                .map(text_output)
                .map_err(FunctionCallError::RespondToModel)
        })
    }
}

pub(crate) struct CancelWakeupTool {
    pub(crate) state: Arc<WakeupThreadState>,
}

impl<'call> ToolExecutor<ToolCall<'call>> for CancelWakeupTool {
    fn tool_name(&self) -> ToolName {
        ToolName::namespaced(CLOCK_NAMESPACE, CANCEL_WAKEUP_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        cancel_wakeup_spec()
    }

    fn exposure(&self) -> ToolExposure {
        ToolExposure::DirectModelOnly
    }

    /// Stop the timer and the loop; the model cancelled it, so no notice follows (UC4).
    fn handle<'a>(&'a self, invocation: ToolCall<'call>) -> codex_tools::ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async move {
            let _ = function_arguments(&invocation)?;
            let cancelled = self.state.cancel(CancelReason::Model);
            Ok(text_output(crate::fragments::cancel_output(&cancelled)))
        })
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
