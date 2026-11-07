//! Fork: app-server API types for the output style of a loaded thread
//! (`specs/output-styles-2026-11-07.md`, UC4 R4.4). Both methods are experimental; the
//! initial style comes from `config.output_style` of `thread/start`, `thread/resume` and
//! `thread/fork`.

use crate::JsonSchema;
use crate::TS;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;

/// Where an output style comes from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum OutputStyleSource {
    /// Compiled into Codex.
    Builtin,
    /// A file in `$CODEX_HOME/output-styles/`.
    User,
    /// A file in `.codex/output-styles/` of a trusted project.
    Project,
}

/// An output style a thread can use.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct OutputStyleInfo {
    pub name: String,
    pub description: String,
    pub source: OutputStyleSource,
    /// The style file; absent for built-in styles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<AbsolutePathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadOutputStyleListParams {
    pub thread_id: String,
}

/// The styles of a loaded thread's config and the one in effect.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadOutputStyleListResponse {
    /// The style in effect; `default` when no style adds instructions.
    pub current: String,
    /// Built-in styles first, then custom styles by name.
    pub styles: Vec<OutputStyleInfo>,
    /// Style files and folders that were skipped, and why.
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadOutputStyleSetParams {
    pub thread_id: String,
    /// A style name from `thread/outputStyle/list`, compared case-insensitively.
    pub name: String,
}

/// The selected style; the thread uses it from its next model step while it stays loaded.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadOutputStyleSetResponse {
    pub style: OutputStyleInfo,
}

#[cfg(test)]
#[path = "output_style_tests.rs"]
mod tests;
