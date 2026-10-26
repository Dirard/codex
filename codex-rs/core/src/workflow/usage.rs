//! Live token accounting of workflow agents (UC5, R5.1, R5.4).

use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TokenUsage;
use codex_workflow::TokenCharge;

use crate::session::session::Session;
use crate::session::turn_context::TurnContext;

/// Fork: hook of `Session::record_token_usage_info`. Charges one model response of a
/// workflow agent to the run call waiting for it, so budgets see spending as it happens
/// (R5.1) and hard mode can stop running agents (R5.4). Other threads are not affected.
pub(crate) fn fork_charge_child_usage(
    session: &Session,
    turn_context: &TurnContext,
    usage: &TokenUsage,
) {
    let source = &turn_context.session_source;
    let SessionSource::SubAgent(_) = source else {
        return;
    };
    if let Some(path) = source.get_agent_path() {
        session
            .services
            .local_agent_runtime
            .workflow_children()
            .charge_usage(path.as_str(), usage);
    }
}

/// UC5, R5.1: usage as non-cached input plus output, with cached input apart.
pub(super) fn token_charge(usage: &TokenUsage) -> TokenCharge {
    let non_cached_input = usage.input_tokens.saturating_sub(usage.cached_input_tokens);
    TokenCharge {
        charged: u64::try_from(non_cached_input.saturating_add(usage.output_tokens)).unwrap_or(0),
        cached: u64::try_from(usage.cached_input_tokens).unwrap_or(0),
    }
}
