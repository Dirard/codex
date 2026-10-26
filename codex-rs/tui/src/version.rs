/// The current Codex CLI version as embedded at compile time.
#[cfg(not(test))]
pub const CODEX_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

// Fork: codex/fork stamps its release version into Cargo.toml, while upstream
// keeps the source-build version 0.0.0 there and records snapshots with it.
// Unit tests render that version, so they do not change with the stamp
// (specs/fork-completion-2026-10-26.md, R1.2).
#[cfg(test)]
pub const CODEX_CLI_VERSION: &str = "0.0.0";
