//! Fork: upstream tests keep upstream feature defaults while the fork turns some of its features
//! on by default (`specs/fork-features-default-on-2026-11-07.md`, UC6).
//!
//! [`crate::load_default_config_for_test`] loads every test configuration with packaged defaults
//! that turn the default-on fork features off. Packaged defaults are the lowest config layer, so
//! a feature the test sets in its `config.toml`, a profile, a project config or `-c` keeps the
//! test's value, and a mutator that enables a feature after loading still does (R6.1). Fork tests
//! that need the production defaults load with `ConfigBuilder` directly, or mark their
//! `CODEX_HOME` with [`use_production_feature_defaults`] from a pre-build hook
//! (`core/tests/suite/fork_default_features.rs`).

use codex_config::LoaderOverrides;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::path::Path;

/// File in the test's `CODEX_HOME` that holds the packaged defaults of the harness. Its name does
/// not end in `.config.toml`, so it is never read as a profile.
pub const UPSTREAM_TEST_PACKAGED_DEFAULTS_FILE: &str = "fork-upstream-test-packaged-defaults.toml";

/// Marker in the test's `CODEX_HOME`: the harness loads with the production feature defaults.
const PRODUCTION_FEATURE_DEFAULTS_MARKER: &str = "fork-production-feature-defaults";

/// Pre-build hook of `TestCodexBuilder` for fork tests of the production feature defaults
/// (R6.5): the harness loads `codex_home` without turning the default-on fork features off.
pub fn use_production_feature_defaults(codex_home: &Path) {
    let marker = codex_home.join(PRODUCTION_FEATURE_DEFAULTS_MARKER);
    if let Err(err) = std::fs::write(&marker, "") {
        panic!("write {}: {err}", marker.display());
    }
}

/// `-c` arguments of the `codex exec` harness (`crate::test_codex_exec`): the binary has no
/// test-only config layer, so every default-on fork feature that the test's `config.toml` does not
/// set yet is turned off on the command line. The test's own `-c` arguments come later and win;
/// a feature the test sets in a profile or project config instead would stay off. Nothing when
/// [`use_production_feature_defaults`] marked `codex_home`.
pub fn upstream_test_exec_args(codex_home: &Path) -> Vec<String> {
    if codex_home.join(PRODUCTION_FEATURE_DEFAULTS_MARKER).exists() {
        return Vec::new();
    }
    let config_toml = std::fs::read_to_string(codex_home.join("config.toml")).unwrap_or_default();
    let keys = codex_config::default_on_fork_features_unset_in(&config_toml).unwrap_or_else(|_| {
        codex_features::fork_default_on_features()
            .into_iter()
            .map(codex_features::Feature::key)
            .collect()
    });
    keys.into_iter()
        .flat_map(|key| ["-c".to_string(), format!("features.{key}=false")])
        .collect()
}

/// [`LoaderOverrides::without_managed_config_for_tests`] whose packaged defaults are the embedded
/// ones with every default-on fork feature turned off, written to `codex_home`; without the
/// pins when [`use_production_feature_defaults`] marked `codex_home`.
pub fn upstream_test_loader_overrides(codex_home: &Path) -> LoaderOverrides {
    if codex_home.join(PRODUCTION_FEATURE_DEFAULTS_MARKER).exists() {
        return LoaderOverrides::without_managed_config_for_tests();
    }
    let path = codex_home.join(UPSTREAM_TEST_PACKAGED_DEFAULTS_FILE);
    if let Err(err) = std::fs::write(&path, codex_config::upstream_test_packaged_defaults_toml()) {
        panic!("write {}: {err}", path.display());
    }
    let path = match AbsolutePathBuf::from_absolute_path(&path) {
        Ok(path) => path,
        Err(err) => panic!("{} is not absolute: {err}", path.display()),
    };
    LoaderOverrides {
        packaged_defaults_path: Some(path),
        ..LoaderOverrides::without_managed_config_for_tests()
    }
}
