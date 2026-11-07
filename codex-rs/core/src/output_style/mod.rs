//! Fork: output styles (`specs/output-styles-2026-11-07.md`).
//!
//! An output style tells the model how to communicate with the user: what to say before and
//! between tool calls, how to shape the final message, and how to write code comments. Styles
//! are the built-in `default`, `outcome-first`, `explanatory` and `learning`, plus Markdown
//! files in `$CODEX_HOME/output-styles/` and in `.codex/output-styles/` of trusted projects.
//! The selected style reaches the model as the world-state section `output_style`, a developer
//! message that never replaces the base instructions, and only root threads get it.

mod catalog;
mod config;
mod section;
mod thread_api;

#[cfg(test)]
mod test_support;

use std::sync::Arc;

use codex_utils_absolute_path::AbsolutePathBuf;

pub use catalog::OutputStyleCatalog;
pub(crate) use config::resolve_output_style;
pub(crate) use section::add_world_state_section;
pub use thread_api::OutputStyleError;
pub use thread_api::OutputStyleListing;

/// Name of the style that adds no instructions.
pub const DEFAULT_OUTPUT_STYLE: &str = "default";

/// Where a style comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStyleSource {
    /// Compiled into Codex.
    Builtin,
    /// A file in `$CODEX_HOME/output-styles/`.
    User,
    /// A file in `.codex/output-styles/` of a trusted project.
    Project,
}

/// One output style of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputStyle {
    /// As written: the frontmatter `name` or the file stem.
    pub name: String,
    /// One line for pickers.
    pub description: String,
    pub source: OutputStyleSource,
    /// The style file; `None` for built-in styles.
    pub path: Option<AbsolutePathBuf>,
    /// The instructions without frontmatter, trimmed; empty only for `default`.
    pub text: Arc<str>,
}

impl OutputStyle {
    /// Whether this is the built-in `default` style, which adds no instructions.
    pub fn is_default(&self) -> bool {
        self.source == OutputStyleSource::Builtin && self.name == DEFAULT_OUTPUT_STYLE
    }
}

/// The style a thread uses; its text is frozen when it is selected (R2.5).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OutputStyleSelection {
    #[default]
    Default,
    Style(Arc<OutputStyle>),
}

impl OutputStyleSelection {
    /// The style that adds instructions, `None` for `default`.
    pub fn style(&self) -> Option<&Arc<OutputStyle>> {
        match self {
            Self::Default => None,
            Self::Style(style) => Some(style),
        }
    }

    /// The selected style's name, `default` when none adds instructions.
    pub fn name(&self) -> &str {
        self.style()
            .map_or(DEFAULT_OUTPUT_STYLE, |style| style.name.as_str())
    }

    /// `default` for the built-in `default` style, the style otherwise.
    pub(crate) fn from_style(style: Arc<OutputStyle>) -> Self {
        if style.is_default() {
            Self::Default
        } else {
            Self::Style(style)
        }
    }
}

/// `output_style` of the config as loaded (UC2): what was asked for and what was selected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputStyleConfig {
    /// The value of `output_style` as written.
    pub requested: Option<String>,
    pub selection: OutputStyleSelection,
}
