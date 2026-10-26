//! Fork: session hooks for tests outside the `session` module.

use codex_protocol::models::BaseInstructionsProvenance;

use super::session::Session;

impl Session {
    /// Fork: lets tests outside this module give a test session model-derived instructions.
    pub(crate) async fn set_base_instructions_provenance_for_test(
        &self,
        provenance: Option<BaseInstructionsProvenance>,
    ) {
        self.state.lock().await.base_instructions_provenance = provenance;
    }
}
