//! `output_style` of the config (UC2).

use std::sync::Arc;

use codex_config::ConfigLayerStack;
use codex_exec_server::ExecutorFileSystem;
use codex_utils_absolute_path::AbsolutePathBuf;

use super::DEFAULT_OUTPUT_STYLE;
use super::OutputStyleCatalog;
use super::OutputStyleConfig;
use super::OutputStyleSelection;
use super::catalog::builtin_style;

/// Use case 2 "Выбор стиля при загрузке конфига" (`specs/output-styles-2026-11-07.md`):
/// resolves `output_style` once, when the config is loaded, and freezes the selected text
/// (R2.5). With the `output_styles` feature off the key is ignored with one warning (R2.2);
/// unset, empty or `default` selects the default style and a built-in name reads no files
/// (R2.3); any other name is looked up in the catalog, and an unknown name warns and falls
/// back to the default style (R2.4).
pub(crate) async fn resolve_output_style(
    fs: &dyn ExecutorFileSystem,
    requested: Option<&str>,
    feature_enabled: bool,
    codex_home: &AbsolutePathBuf,
    config_layer_stack: &ConfigLayerStack,
    startup_warnings: &mut Vec<String>,
) -> Arc<OutputStyleConfig> {
    let requested_name = requested.map(str::trim).unwrap_or_default();
    let is_default =
        requested_name.is_empty() || requested_name.eq_ignore_ascii_case(DEFAULT_OUTPUT_STYLE);
    let selection = if !feature_enabled {
        if !is_default {
            startup_warnings.push(format!(
                "`output_style = \"{requested_name}\"` has no effect until the output styles feature is enabled (`[features] output_styles = true` or /experimental)."
            ));
        }
        OutputStyleSelection::Default
    } else if is_default {
        OutputStyleSelection::Default
    } else if let Some(style) = builtin_style(requested_name) {
        OutputStyleSelection::from_style(style)
    } else {
        let catalog = OutputStyleCatalog::load(fs, codex_home, config_layer_stack).await;
        match catalog.find(requested_name) {
            Some(style) => OutputStyleSelection::from_style(Arc::clone(style)),
            None => {
                startup_warnings.push(unknown_style_warning(requested_name, &catalog));
                OutputStyleSelection::Default
            }
        }
    };
    Arc::new(OutputStyleConfig {
        requested: requested.map(str::to_string),
        selection,
    })
}

/// R2.4: names the style, the available ones and why files were skipped.
fn unknown_style_warning(name: &str, catalog: &OutputStyleCatalog) -> String {
    let mut warning = format!(
        "Output style `{name}` was not found, so the default style is used. Available styles: {}. Project styles load only from trusted projects.",
        catalog.names().join(", ")
    );
    for skipped in &catalog.warnings {
        warning.push_str("\n- ");
        warning.push_str(skipped);
    }
    warning
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
