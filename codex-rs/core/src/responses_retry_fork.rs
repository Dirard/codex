//! Fork: `ServerOverloaded` retries with backoff on an independent counter with a capacity
//! floor, where upstream treats an unadvised overload as terminal
//! (docs/fork/02-compaction-context.md F6).

use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;

use crate::session::turn_context::TurnContext;

/// Fork capacity floor for `ServerOverloaded` unless an explicit stream budget is smaller.
const SERVER_OVERLOADED_RETRY_FLOOR: u64 = 3;

/// Retries allowed for `ServerOverloaded` on its independent counter, raised after per-request
/// caps to the fork floor. An explicit `stream_max_retries` below the floor is honored exactly,
/// so an explicit zero fails fast; an explicit value at or above it, such as the default five,
/// gets the floor like the unset default, so asking for more never yields fewer retries.
pub(super) fn server_overloaded_max_retries(
    max_retries: u64,
    configured_stream_retries: Option<u64>,
) -> u64 {
    let floor = configured_stream_retries.map_or(SERVER_OVERLOADED_RETRY_FLOOR, |configured| {
        configured.min(SERVER_OVERLOADED_RETRY_FLOOR)
    });
    max_retries.max(floor)
}

/// The counter and the limit that `err` retries against.
pub(super) fn retry_budget<'a>(
    err: &CodexErr,
    retries: &'a mut u64,
    server_overloaded_retries: &'a mut u64,
    max_retries: u64,
    turn_context: &TurnContext,
) -> (&'a mut u64, u64) {
    if matches!(err.details(), CodexErrorDetails::ServerOverloaded) {
        (
            server_overloaded_retries,
            server_overloaded_max_retries(
                max_retries,
                turn_context.provider.info().stream_max_retries,
            ),
        )
    } else {
        (retries, max_retries)
    }
}
