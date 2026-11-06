//! Constructs configured boards and installs tools over the shared board interface.
//! Storage, backend behavior and tool implementations live in `codex-message-board`.

mod backend;
mod extension;
mod mcp;

pub use backend::BoardBackend;
pub use backend::install;
// Fork: whether a board is bound to a thread (`wait_agent` waits for its posts).
pub use extension::is_bound;

/// Storage selected by the agent consumer for opening, setup and permanent deletion.
pub fn local_board_options() -> codex_message_board::LocalBoardOptions {
    codex_message_board::LocalBoardOptions {
        storage_namespace: "agent_message_board".into(),
    }
}
