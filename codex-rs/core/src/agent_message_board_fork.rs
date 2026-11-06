//! Fork: who can post to the message board bound to a thread, so that `wait_agent` without
//! active agents keeps waiting while posts can still arrive
//! (`docs/fork/01-multi-agent-runtime.md`).
//!
//! The board lives in `codex-agent-message-board-host`, which depends on this crate, so the
//! host registers its classifier when it installs the board. Without a registered host no
//! board can be bound to a thread.

use std::sync::OnceLock;

use codex_extension_api::ExtensionData;

/// Who can post to the message board bound to a thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardAudience {
    /// No board is bound to the thread.
    None,
    /// A local or in-memory board, which only agents of this session tree can post to.
    Tree,
    /// A remote or host-connected board, whose authors can be outside this runtime.
    External,
}

static CLASSIFIER: OnceLock<fn(&ExtensionData) -> BoardAudience> = OnceLock::new();

/// Registers how the board host classifies the board of a thread. The classifier is a pure
/// function of the thread's extension data, so the first registration serves every runtime.
pub fn register_board_audience(classify: fn(&ExtensionData) -> BoardAudience) {
    let _ = CLASSIFIER.set(classify);
}

pub(crate) fn board_audience(thread_store: &ExtensionData) -> BoardAudience {
    CLASSIFIER
        .get()
        .map_or(BoardAudience::None, |classify| classify(thread_store))
}
