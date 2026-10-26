//! Fork: `turn/start` and `turn/steer` reject `additionalContext` over the protocol limits
//! before the turn starts (`docs/fork/04-app-server-protocol.md`). A child module of
//! `turn_processor`, so it uses that module's imports and private helpers.

use super::*;

const ADDITIONAL_CONTEXT_TOO_LARGE_ERROR_CODE: &str = "additional_context_too_large";

impl TurnRequestProcessor {
    /// Tracks and returns the error when `additional_context` exceeds a limit.
    pub(super) fn fork_reject_oversized_additional_context(
        &self,
        request_id: &ConnectionRequestId,
        additional_context: &Option<HashMap<String, AdditionalContextEntry>>,
    ) -> Result<(), JSONRPCErrorError> {
        if let Err(error) = validate_additional_context_limits(additional_context) {
            self.track_error_response(
                request_id,
                &error,
                Some(AnalyticsJsonRpcError::Input(InputError::TooLarge)),
            );
            return Err(error);
        }
        Ok(())
    }
}

fn validate_additional_context_limits(
    additional_context: &Option<HashMap<String, AdditionalContextEntry>>,
) -> Result<(), JSONRPCErrorError> {
    let Some(additional_context) = additional_context else {
        return Ok(());
    };
    if additional_context.len() > MAX_ADDITIONAL_CONTEXT_ENTRIES {
        return Err(additional_context_too_large_error(
            "additionalContext entries",
            MAX_ADDITIONAL_CONTEXT_ENTRIES,
            additional_context.len(),
        ));
    }

    let mut total_bytes = 0usize;
    for (key, entry) in additional_context {
        let key_bytes = key.len();
        if key_bytes > MAX_ADDITIONAL_CONTEXT_KEY_BYTES {
            return Err(additional_context_too_large_error(
                "additionalContext key bytes",
                MAX_ADDITIONAL_CONTEXT_KEY_BYTES,
                key_bytes,
            ));
        }

        let value_bytes = entry.value.len();
        if value_bytes > MAX_ADDITIONAL_CONTEXT_VALUE_BYTES {
            return Err(additional_context_too_large_error(
                "additionalContext value bytes",
                MAX_ADDITIONAL_CONTEXT_VALUE_BYTES,
                value_bytes,
            ));
        }

        total_bytes = total_bytes
            .saturating_add(key_bytes)
            .saturating_add(value_bytes);
        if total_bytes > MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES {
            return Err(additional_context_too_large_error(
                "additionalContext total bytes",
                MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES,
                total_bytes,
            ));
        }
    }

    Ok(())
}

fn additional_context_too_large_error(
    field: &'static str,
    limit: usize,
    actual: usize,
) -> JSONRPCErrorError {
    let mut error = invalid_params(format!("{field} exceeds the maximum of {limit}."));
    error.data = Some(serde_json::json!({
        "input_error_code": ADDITIONAL_CONTEXT_TOO_LARGE_ERROR_CODE,
        "field": field,
        "limit": limit,
        "actual": actual,
    }));
    error
}

#[cfg(test)]
#[path = "turn_processor_additional_context_tests.rs"]
mod additional_context_tests;
