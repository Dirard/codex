//! Fork: the `initialize` response reports the protocol, schema and manifest digests and
//! the active protocol mode, so a Go SDK client can check that it speaks the server's
//! protocol (`docs/fork/05-go-sdk.md`).

use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;

use crate::JsonSchema;
use crate::TS;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ActiveProtocolMode {
    Stable,
    Experimental,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub user_agent: String,
    /// Absolute path to the server's $CODEX_HOME directory.
    pub codex_home: AbsolutePathBuf,
    /// Platform family for the running app-server target, for example
    /// `"unix"` or `"windows"`.
    pub platform_family: String,
    /// Operating system for the running app-server target, for example
    /// `"macos"`, `"linux"`, or `"windows"`.
    pub platform_os: String,
    pub stable_protocol_digest: String,
    pub experimental_protocol_digest: String,
    pub stable_schema_digest: String,
    pub experimental_schema_digest: String,
    pub stable_manifest_digest: String,
    pub experimental_manifest_digest: String,
    pub active_protocol_mode: ActiveProtocolMode,
}

#[cfg(test)]
#[path = "fork_initialize_tests.rs"]
mod tests;
