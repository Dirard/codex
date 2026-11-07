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

/// Data `tool_specs` of the spec, with the clamp range of the effective config (R2.8).
pub(crate) fn schedule_wakeup_spec(limits: &ScheduledWakeupsConfig) -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "delay_seconds".to_string(),
            JsonSchema::integer(Some(format!(
                "Seconds from now; clamped to {}-{}.",
                limits.min_delay_seconds, limits.max_delay_seconds
            ))),
        ),
        (
            "prompt".to_string(),
            JsonSchema::string(Some(
                "What to do on wakeup, for your future self. Optional inside a /loop.".to_string(),
            )),
        ),
        (
            "reason".to_string(),
            JsonSchema::string(Some("Short reason shown to the user.".to_string())),
        ),
    ]);
    clock_namespace(ResponsesApiTool {
        name: SCHEDULE_WAKEUP_TOOL_NAME.to_string(),
        description: "End your turn and resume this thread later with `prompt` as new input, e.g. to poll CI, a deploy or a long job instead of sleeping. One wakeup per thread; a new call replaces it.".to_string(),
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

pub(crate) fn cancel_wakeup_spec() -> ToolSpec {
    clock_namespace(ResponsesApiTool {
        name: CANCEL_WAKEUP_TOOL_NAME.to_string(),
        description: "Cancel this thread's pending wakeup and stop its /loop.".to_string(),
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
        schedule_wakeup_spec(&self.state.limits())
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
