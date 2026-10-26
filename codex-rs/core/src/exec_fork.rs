//! Fork: an I/O drain timeout stops the output readers and keeps what they captured.
//!
//! Upstream aborts a reader task whose pipe stays open past the drain timeout and reports
//! empty output. The fork cancels the readers instead, so the command keeps the output
//! read so far (`7fea25a65a`, "preserve legacy output omission metadata").

use std::future::Future;

use tokio_util::sync::CancellationToken;

/// Awaits the next `read`, or returns `Ok(0)` (end of output) once the drain timeout
/// cancelled the reader.
pub(super) async fn read_unless_cancelled(
    read: impl Future<Output = std::io::Result<usize>>,
    cancellation: &CancellationToken,
) -> std::io::Result<usize> {
    tokio::select! {
        _ = cancellation.cancelled() => Ok(0),
        read = read => read,
    }
}

/// Awaits an output delta `send`. Returns `false` when the drain timeout cancelled the
/// reader while the event channel was full.
pub(super) async fn send_unless_cancelled<T>(
    send: impl Future<Output = T>,
    cancellation: &CancellationToken,
) -> bool {
    tokio::select! {
        _ = cancellation.cancelled() => false,
        _ = send => true,
    }
}
