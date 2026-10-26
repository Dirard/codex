//! Fork: protocol additions kept out of `common.rs`, which declares this module with
//! `#[macro_use]` and re-exports its items (`docs/fork/04-app-server-protocol.md`).

#[path = "fork_initialize.rs"]
mod fork_initialize;
pub use fork_initialize::ActiveProtocolMode;
pub use fork_initialize::InitializeResponse;

/// Expands a protocol list of `common.rs` with its upstream macro and generates the Go SDK
/// manifest entries from the same tokens (`go_manifest/macros.rs`).
macro_rules! fork_with_go_manifest {
    (client_request_definitions { $($list:tt)* }) => {
        client_request_definitions! { $($list)* }
        go_manifest_client_request_entries! { $($list)* }
    };
    (server_request_definitions { $($list:tt)* }) => {
        server_request_definitions! { $($list)* }
        go_manifest_server_request_entries! { $($list)* }
    };
    (server_notification_definitions { $($list:tt)* }) => {
        server_notification_definitions! { $($list)* }
        go_manifest_server_notification_entries! { $($list)* }
    };
    (client_notification_definitions { $($list:tt)* }) => {
        client_notification_definitions! { $($list)* }
        go_manifest_client_notification_entries! { $($list)* }
    };
}
