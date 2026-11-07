use std::path::Path;

use codex_config::CONFIG_TOML_FILE;
use codex_config::LoaderOverrides;
use codex_config::config_toml::ConfigToml;
use codex_config::loader::project_trust_key;
use codex_features::Feature;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::*;
use crate::config::Config;
use crate::config::ConfigBuilder;
use crate::config::ConfigOverrides;
use crate::output_style::OutputStyleSource;
use crate::output_style::test_support::RecordingFs;
use crate::output_style::test_support::layer_stack;
use crate::output_style::test_support::style_file;
use crate::output_style::test_support::write_style;

fn absolute(path: &Path) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("absolute path")
}

/// Resolves `requested` against `codex_home` with no projects; returns the selected name, the
/// warnings and whether any file was touched.
async fn resolve(
    codex_home: &AbsolutePathBuf,
    requested: Option<&str>,
    feature_enabled: bool,
) -> (Arc<OutputStyleConfig>, Vec<String>, bool) {
    let fs = RecordingFs::default();
    let mut warnings = Vec::new();
    let config = resolve_output_style(
        &fs,
        requested,
        feature_enabled,
        codex_home,
        &layer_stack(codex_home, &[]),
        &mut warnings,
    )
    .await;
    (config, warnings, !fs.accessed().is_empty())
}

/// R2.2: with the feature off, a style name selects `default` with one warning, and no key
/// gives no warning.
#[tokio::test]
async fn disabled_feature_ignores_the_key_with_one_warning() {
    let temp = TempDir::new().expect("temp dir");
    let codex_home = absolute(temp.path());
    let (config, warnings, touched) = resolve(&codex_home, Some("outcome-first"), false).await;
    assert_eq!(config.selection, OutputStyleSelection::Default);
    assert_eq!(config.requested.as_deref(), Some("outcome-first"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("has no effect until the output styles feature is enabled"));
    assert!(!touched);

    let (config, warnings, _) = resolve(&codex_home, None, false).await;
    assert_eq!(config.selection, OutputStyleSelection::Default);
    assert_eq!(warnings, Vec::<String>::new());
}

/// R2.3: no key, an empty name and `Default` select the default style; a built-in name
/// selects that style without reading any file.
#[tokio::test]
async fn default_and_builtin_names_read_no_files() {
    let temp = TempDir::new().expect("temp dir");
    let codex_home = absolute(temp.path());
    for requested in [None, Some(""), Some("Default")] {
        let (config, warnings, touched) = resolve(&codex_home, requested, true).await;
        assert_eq!(
            config.selection,
            OutputStyleSelection::Default,
            "{requested:?}"
        );
        assert_eq!(warnings, Vec::<String>::new());
        assert!(!touched);
    }
    let (config, warnings, touched) = resolve(&codex_home, Some("outcome-first"), true).await;
    assert_eq!(config.selection.name(), "outcome-first");
    assert_eq!(
        config.selection.style().map(|style| style.source),
        Some(OutputStyleSource::Builtin)
    );
    assert_eq!(warnings, Vec::<String>::new());
    assert!(!touched, "a built-in style must not read files");
}

/// R2.1, R2.5: a style from `$CODEX_HOME/output-styles/` is selected, and its text is frozen
/// when the config is loaded.
#[tokio::test]
async fn custom_style_text_is_frozen_at_load() {
    let temp = TempDir::new().expect("temp dir");
    let codex_home = absolute(temp.path());
    write_style(
        &codex_home,
        "terse.md",
        &style_file(None, "Terse.", "Old text."),
    );
    let (config, warnings, _) = resolve(&codex_home, Some("Terse"), true).await;
    assert_eq!(warnings, Vec::<String>::new());
    write_style(
        &codex_home,
        "terse.md",
        &style_file(None, "Terse.", "New text."),
    );
    let style = config.selection.style().expect("custom style selected");
    assert_eq!(style.name, "terse");
    assert_eq!(style.source, OutputStyleSource::User);
    assert_eq!(&*style.text, "Old text.");
}

/// R2.4, R1.2: an unknown name, or a style that only an untrusted project has, selects the
/// default style with a warning that names it, lists the available styles, mentions trusted
/// projects and carries the catalog's warnings; loading still succeeds.
#[tokio::test]
async fn unknown_and_untrusted_names_fall_back_to_default() {
    let temp = TempDir::new().expect("temp dir");
    let root = absolute(temp.path());
    let codex_home = root.join("home");
    let project = root.join("repo/.codex");
    write_style(&project, "evil.md", &style_file(None, "Evil.", "Evil."));
    write_style(&codex_home, "broken.md", "no frontmatter");
    for name in ["missing", "evil"] {
        let mut warnings = Vec::new();
        let config = resolve_output_style(
            &RecordingFs::default(),
            Some(name),
            true,
            &codex_home,
            &layer_stack(&codex_home, &[(&project, false)]),
            &mut warnings,
        )
        .await;
        assert_eq!(config.selection, OutputStyleSelection::Default);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let warning = &warnings[0];
        assert!(
            warning.contains(&format!("Output style `{name}` was not found")),
            "{warning}"
        );
        assert!(
            warning.contains("Available styles: default, outcome-first, explanatory, learning."),
            "{warning}"
        );
        assert!(warning.contains("Project styles load only from trusted projects."));
        assert!(warning.contains("broken.md"), "{warning}");
    }
}

/// R2.1: `output_style` works in the user file, the `--profile` file, a trusted project and
/// `-c`, and the layer with the highest precedence wins.
#[tokio::test]
async fn output_style_follows_layer_precedence() -> anyhow::Result<()> {
    for winner in ["user", "profile", "project", "cli"] {
        let codex_home = TempDir::new()?;
        let workspace = TempDir::new()?;
        let workspace_key = toml::Value::String(project_trust_key(workspace.path()));
        std::fs::write(
            codex_home.path().join(CONFIG_TOML_FILE),
            format!(
                "output_style = \"outcome-first\"\n[features]\noutput_styles = true\n[projects.{workspace_key}]\ntrust_level = \"trusted\"\n"
            ),
        )?;
        let profile_file = codex_home.path().join("work.config.toml");
        if winner != "user" {
            std::fs::write(&profile_file, "output_style = \"explanatory\"\n")?;
        }
        if winner == "project" || winner == "cli" {
            let project = workspace.path().join(".codex");
            std::fs::create_dir_all(&project)?;
            std::fs::write(
                project.join(CONFIG_TOML_FILE),
                "output_style = \"learning\"\n",
            )?;
        }
        let cli_overrides = if winner == "cli" {
            vec![(
                "output_style".to_string(),
                toml::Value::String("default".to_string()),
            )]
        } else {
            Vec::new()
        };
        let config = ConfigBuilder::without_managed_config_for_tests()
            .codex_home(codex_home.path().to_path_buf())
            .loader_overrides(LoaderOverrides {
                user_config_path: (winner != "user").then(|| absolute(&profile_file)),
                user_config_profile: (winner != "user")
                    .then(|| "work".parse().expect("profile-v2 name")),
                ..LoaderOverrides::without_managed_config_for_tests()
            })
            .cli_overrides(cli_overrides)
            .harness_overrides(ConfigOverrides {
                cwd: Some(workspace.path().to_path_buf()),
                ..Default::default()
            })
            .build()
            .await?;
        assert!(config.features.enabled(Feature::OutputStyles), "{winner}");
        let expected = match winner {
            "user" => "outcome-first",
            "profile" => "explanatory",
            "project" => "learning",
            _ => "default",
        };
        assert_eq!(config.output_style.selection.name(), expected, "{winner}");
    }
    Ok(())
}

/// R2.6: the fork's fields keep `Config` and `ConfigToml` small: one pointer each.
#[test]
fn config_fields_are_pointer_sized() {
    assert_eq!(
        std::mem::size_of_val(&ConfigToml::default().output_style),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<Arc<OutputStyleConfig>>(),
        std::mem::size_of::<usize>()
    );
    // `Config.output_style` has this type; the compiler checks the field.
    let _: fn(&Config) -> &Arc<OutputStyleConfig> = |config| &config.output_style;
}
