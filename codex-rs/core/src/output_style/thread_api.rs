//! Output styles of a thread for clients: app-server and through it the TUI (UC4).

use std::sync::Arc;

use codex_exec_server::LOCAL_FS;
use codex_features::Feature;

use super::DEFAULT_OUTPUT_STYLE;
use super::OutputStyle;
use super::OutputStyleCatalog;
use super::OutputStyleSelection;
use super::section::OutputStyleOverride;
use super::section::selection_for_session;
use crate::codex_thread::CodexThread;

/// The styles a thread can use and the one in effect (R4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputStyleListing {
    /// The name of the style in effect, `default` when none adds instructions.
    pub current: String,
    /// Built from the thread's config at call time; its warnings name skipped files.
    pub catalog: OutputStyleCatalog,
}

/// Why [`CodexThread::set_output_style`] refused a style (R4.3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OutputStyleError {
    #[error(
        "output styles are disabled for this thread; enable the `output_styles` feature (`[features] output_styles = true`) first"
    )]
    FeatureDisabled,
    #[error("output styles apply only to root threads, and this thread is a subagent")]
    NotRootThread,
    #[error("unknown output style `{name}`; available styles: {available}")]
    UnknownStyle { name: String, available: String },
}

impl CodexThread {
    /// Fork: the style in effect and the catalog of this thread's config (R4.1).
    pub async fn output_styles(&self) -> OutputStyleListing {
        let config = self.session.get_config().await;
        let catalog = OutputStyleCatalog::load(
            LOCAL_FS.as_ref(),
            &config.codex_home,
            &config.config_layer_stack,
        )
        .await;
        let current = if config.features.enabled(Feature::OutputStyles)
            && !self.session_source.is_non_root_agent()
        {
            selection_for_session(&self.session, &config)
                .name()
                .to_string()
        } else {
            DEFAULT_OUTPUT_STYLE.to_string()
        };
        OutputStyleListing { current, catalog }
    }

    /// Fork: selects `name` from a catalog built now, freezing its text, for this thread while
    /// it stays loaded (R4.2, R4.5); the next model step tells the model (UC3). Nothing changes
    /// when the feature is off, the thread is not a root thread or the name is unknown (R4.3).
    pub async fn set_output_style(&self, name: &str) -> Result<Arc<OutputStyle>, OutputStyleError> {
        let config = self.session.get_config().await;
        if !config.features.enabled(Feature::OutputStyles) {
            return Err(OutputStyleError::FeatureDisabled);
        }
        if self.session_source.is_non_root_agent() {
            return Err(OutputStyleError::NotRootThread);
        }
        let catalog = OutputStyleCatalog::load(
            LOCAL_FS.as_ref(),
            &config.codex_home,
            &config.config_layer_stack,
        )
        .await;
        let style =
            catalog
                .find(name.trim())
                .cloned()
                .ok_or_else(|| OutputStyleError::UnknownStyle {
                    name: name.to_string(),
                    available: catalog.names().join(", "),
                })?;
        self.session
            .services
            .thread_extension_data
            .get_or_init(OutputStyleOverride::default)
            .set(OutputStyleSelection::from_style(Arc::clone(&style)));
        Ok(style)
    }
}
