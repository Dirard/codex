//! Fork: what the shared test harnesses use so that upstream tests keep upstream feature
//! defaults while the fork turns some of its features on by default
//! (`specs/fork-features-default-on-2026-11-07.md`, UC6).
//!
//! Nothing in a Codex binary calls these functions: the production defaults do not depend on a
//! test switch (R6.3). The features they turn off come from
//! [`codex_features::fork_default_on_features`], so a fork feature that becomes default-on later
//! is covered without editing the harnesses (R6.2).

use codex_features::fork_default_on_features;
use toml_edit::DocumentMut;
use toml_edit::TomlError;

/// The packaged defaults that `load_config_layers_state` embeds when `LoaderOverrides` names no
/// packaged defaults file.
const EMBEDDED_PACKAGED_DEFAULTS: &str = include_str!("../defaults.toml");

/// Packaged defaults for a test harness: the embedded packaged defaults with every default-on
/// fork feature turned off. Loaded as the packaged defaults layer, the lowest config layer, it
/// yields to any value a test sets in its `config.toml`, a profile, a project config or `-c`
/// (R6.1).
pub fn upstream_test_packaged_defaults_toml() -> String {
    match pin_in_features_table(EMBEDDED_PACKAGED_DEFAULTS) {
        Ok(text) => text,
        Err(err) => unreachable!("the embedded packaged defaults are valid TOML: {err}"),
    }
}

/// Keys of the default-on fork features that `config_toml` does not set, as a boolean in
/// `[features]` or as the `enabled` of a settings table. A harness that cannot add a config layer
/// below the test's `config.toml` (the `codex exec` binary) turns these off with `-c`, which
/// keeps the settings of a table without `enabled` and leaves the feature off, as the other
/// harnesses do.
pub fn default_on_fork_features_unset_in(
    config_toml: &str,
) -> Result<Vec<&'static str>, TomlError> {
    let document = config_toml.parse::<DocumentMut>()?;
    let features = document
        .get("features")
        .and_then(toml_edit::Item::as_table_like);
    Ok(fork_default_on_features()
        .into_iter()
        .map(codex_features::Feature::key)
        .filter(|key| {
            let Some(item) = features.and_then(|features| features.get(key)) else {
                return true;
            };
            item.as_table_like()
                .is_some_and(|table| !table.contains_key("enabled"))
        })
        .collect())
}

/// CLI overrides (`features.<key> = false`) that turn every default-on fork feature off, for a
/// harness that builds its configuration from defaults and CLI overrides only (the TUI's
/// chatwidget tests). Later overrides of the test win.
pub fn upstream_test_feature_overrides() -> Vec<(String, toml::Value)> {
    fork_default_on_features()
        .into_iter()
        .map(|feature| {
            (
                format!("features.{}", feature.key()),
                toml::Value::Boolean(false),
            )
        })
        .collect()
}

/// `config_toml` with every default-on fork feature turned off unless the document sets it, as
/// the `config.toml` writer of the app-server tests needs it. Everything else in the document,
/// its formatting included, is kept, and a test may still append its own `[features]` table to
/// the result:
/// - a feature with a settings table (`features.<key>` is a structured feature path) gets a
///   `[features.<key>]` table with `enabled = false`, or `enabled = false` in its existing table
///   without `enabled`, which keeps meaning what it meant while the feature was off by default;
/// - a boolean-only feature (`workflows`, `wake_on_agent_messages`) gets `<key> = false` only in a
///   `[features]` table the document already defines. Without one it keeps its default: a
///   `[features]` table added here would clash with one the test appends. Their behavior needs
///   sub-agents, which tests usually turn on in `[features]` (`multi_agent_v2`).
pub fn pin_default_on_fork_features_off(config_toml: &str) -> Result<String, TomlError> {
    let features_to_pin = fork_default_on_features();
    if features_to_pin.is_empty() {
        return Ok(config_toml.to_string());
    }
    let mut document = config_toml.parse::<DocumentMut>()?;
    let features = document.entry("features").or_insert_with(|| {
        let mut table = toml_edit::Table::new();
        table.set_implicit(true);
        toml_edit::Item::Table(table)
    });
    let defines_features_table = match features {
        toml_edit::Item::Table(table) => !table.is_implicit(),
        toml_edit::Item::Value(value) => value.is_inline_table(),
        _ => false,
    };
    let Some(features) = features.as_table_like_mut() else {
        // Not a table: the config loader reports it; there is nothing to pin.
        return Ok(config_toml.to_string());
    };
    for feature in features_to_pin {
        let key = feature.key();
        match features.get_mut(key) {
            Some(item) => {
                if let Some(table) = item.as_table_like_mut()
                    && !table.contains_key("enabled")
                {
                    table.insert("enabled", toml_edit::value(false));
                }
            }
            None if crate::is_structured_feature_path(&["features", key]) => {
                let mut table = toml_edit::Table::new();
                table.insert("enabled", toml_edit::value(false));
                features.insert(key, toml_edit::Item::Table(table));
            }
            None if defines_features_table => {
                features.insert(key, toml_edit::value(false));
            }
            None => {}
        }
    }
    Ok(document.to_string())
}

/// `config_toml` with `<key> = false` in `[features]` for every default-on fork feature it does
/// not set; for the packaged defaults, which no test appends to.
fn pin_in_features_table(config_toml: &str) -> Result<String, TomlError> {
    let mut document = config_toml.parse::<DocumentMut>()?;
    let features = document.entry("features").or_insert_with(toml_edit::table);
    if let Some(features) = features.as_table_like_mut() {
        for feature in fork_default_on_features() {
            if !features.contains_key(feature.key()) {
                features.insert(feature.key(), toml_edit::value(false));
            }
        }
    }
    Ok(document.to_string())
}

#[cfg(test)]
#[path = "test_defaults_fork_tests.rs"]
mod tests;
