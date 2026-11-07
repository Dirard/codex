use std::path::Path;

use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::*;
use crate::output_style::test_support::RecordingFs;
use crate::output_style::test_support::layer_stack;
use crate::output_style::test_support::style_file;
use crate::output_style::test_support::write_style;

fn absolute(path: &Path) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("absolute path")
}

struct Folders {
    _temp: TempDir,
    codex_home: AbsolutePathBuf,
    outer_project: AbsolutePathBuf,
    inner_project: AbsolutePathBuf,
}

fn folders() -> Folders {
    let temp = TempDir::new().expect("temp dir");
    let root = absolute(temp.path());
    Folders {
        codex_home: root.join("home"),
        outer_project: root.join("repo/.codex"),
        inner_project: root.join("repo/nested/.codex"),
        _temp: temp,
    }
}

fn summary(catalog: &OutputStyleCatalog) -> Vec<(String, OutputStyleSource)> {
    catalog
        .styles
        .iter()
        .map(|style| (style.name.clone(), style.source))
        .collect()
}

/// R1.1, R1.6, R7.3: the built-in styles go through the parser of custom styles, keep their
/// fixed order and names, and only `default` has no text.
#[test]
fn builtin_styles_are_valid_and_ordered() {
    for (stem, contents) in BUILTIN_FILES {
        let parsed = parse_style_file(contents, stem).expect("built-in style parses");
        assert_eq!(parsed.name, stem);
    }
    let catalog = OutputStyleCatalog::builtin();
    assert_eq!(
        catalog.names(),
        vec!["default", "outcome-first", "explanatory", "learning"]
    );
    assert!(catalog.warnings.is_empty());
    for style in &catalog.styles {
        assert_eq!(style.source, OutputStyleSource::Builtin);
        assert_eq!(style.path, None);
        assert_eq!(
            style.text.is_empty(),
            style.name == "default",
            "{}",
            style.name
        );
        assert!(!style.description.is_empty());
    }
    assert!(catalog.styles[0].is_default());
    assert_eq!(
        builtin_style("Outcome-First").map(|style| style.name.clone()),
        Some("outcome-first".to_string())
    );
}

/// R7.1–R7.3: the texts of the built-in styles, each at most 3 KiB and with the phrases the
/// requirements ask for.
#[test]
fn builtin_style_texts() {
    let catalog = OutputStyleCatalog::builtin();
    let text = |name: &str| catalog.find(name).expect("built-in style").text.to_string();
    for style in &catalog.styles {
        assert!(
            style.text.len() <= 3 * 1024,
            "{}: {}",
            style.name,
            style.text.len()
        );
    }
    let outcome_first = text("outcome-first");
    for phrase in [
        "one sentence",
        "Start with the outcome",
        "did not watch",
        "\"real\"",
        "Ask only when you are blocked",
        "Code comments",
    ] {
        assert!(
            outcome_first.contains(phrase),
            "outcome-first lacks {phrase}"
        );
    }
    assert!(text("explanatory").contains("Insight:"));
    let learning = text("learning");
    assert!(learning.contains("TODO(human)"));
    assert!(learning.contains("Insight:"));
    assert!(learning.contains("write the code yourself"));
    insta::assert_snapshot!(
        "builtin_output_styles",
        ["outcome-first", "explanatory", "learning"]
            .iter()
            .map(|name| format!("## {name}\n{}", text(name)))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

/// R1.3: `name` comes from the frontmatter or the file stem, unknown keys and CRLF line ends
/// are fine, and the text is the trimmed body without the frontmatter lines.
#[test]
fn parse_style_file_reads_name_description_and_text() {
    let parsed = parse_style_file(
        "---\r\nname: Terse\r\ndescription: Short answers.\r\nkeep-coding-instructions: true\r\n---\r\n\r\nBe brief.\r\nNo filler.\r\n",
        "ignored",
    )
    .expect("parses");
    assert_eq!(
        parsed,
        ParsedStyleFile {
            name: "Terse".to_string(),
            description: "Short answers.".to_string(),
            text: "Be brief.\nNo filler.".to_string(),
        }
    );
    let parsed = parse_style_file(&style_file(None, "From the stem.", "Text."), "stem-name")
        .expect("parses");
    assert_eq!(parsed.name, "stem-name");
    assert_eq!(parsed.text, "Text.");
}

/// R1.3: files without frontmatter, without a description, with an empty or too long text,
/// or with a name over 64 characters are rejected with a reason.
#[test]
fn parse_style_file_rejects_invalid_files() {
    let long_text = "x".repeat(MAX_STYLE_TEXT_BYTES + 1);
    let long_name = "n".repeat(65);
    let cases = [
        ("Just text.".to_string(), "missing YAML frontmatter"),
        (
            "---\nname: a\n---\nText.".to_string(),
            "missing field `description`",
        ),
        (style_file(None, "Empty.", "  \n"), "empty"),
        (style_file(None, "Long.", &long_text), "more than the limit"),
        (
            style_file(Some(&long_name), "Long name.", "Text."),
            "invalid name",
        ),
    ];
    for (contents, reason) in cases {
        let error = parse_style_file(&contents, "stem").expect_err("rejected");
        assert!(error.contains(reason), "{error} lacks {reason}");
    }
}

/// R1.3: a skipped file gives one warning naming its path and the reason, and the other
/// styles still load.
#[tokio::test]
async fn invalid_files_are_skipped_with_a_warning() {
    let folders = folders();
    write_style(
        &folders.codex_home,
        "good.md",
        &style_file(None, "Good.", "Text."),
    );
    write_style(&folders.codex_home, "bad.md", "no frontmatter");
    write_style(
        &folders.codex_home,
        "empty.md",
        &style_file(None, "Empty.", ""),
    );
    let catalog = OutputStyleCatalog::load(
        &RecordingFs::default(),
        &folders.codex_home,
        &layer_stack(&folders.codex_home, &[]),
    )
    .await;
    assert_eq!(
        catalog.find("good").map(|style| style.source),
        Some(OutputStyleSource::User)
    );
    assert_eq!(catalog.warnings.len(), 2, "{:?}", catalog.warnings);
    let dir = folders.codex_home.join(OUTPUT_STYLES_DIR);
    assert!(catalog.warnings[0].contains(&dir.join("bad.md").display().to_string()));
    assert!(catalog.warnings[0].contains("missing YAML frontmatter"));
    assert!(catalog.warnings[1].contains(&dir.join("empty.md").display().to_string()));
    assert!(catalog.warnings[1].contains("empty"));
}

/// R1.4, R1.6: a project style wins over a user style of the same name in any case, the
/// nearer project wins over the farther one, the first file by name wins inside one folder,
/// and built-in names are reserved; custom styles follow the built-in ones by name.
#[tokio::test]
async fn name_precedence_and_order() {
    let folders = folders();
    write_style(
        &folders.codex_home,
        "terse.md",
        &style_file(None, "User terse.", "User."),
    );
    write_style(
        &folders.codex_home,
        "zeta.md",
        &style_file(Some("Zeta"), "User zeta.", "Z."),
    );
    write_style(
        &folders.codex_home,
        "learning.md",
        &style_file(Some("Learning"), "Mine.", "L."),
    );
    write_style(
        &folders.outer_project,
        "a.md",
        &style_file(Some("Terse"), "Outer.", "Outer."),
    );
    write_style(
        &folders.outer_project,
        "b.md",
        &style_file(Some("alpha"), "Outer alpha.", "A."),
    );
    write_style(
        &folders.inner_project,
        "a.md",
        &style_file(Some("TERSE"), "Inner.", "Inner."),
    );
    write_style(
        &folders.inner_project,
        "b.md",
        &style_file(Some("beta"), "First beta.", "B1."),
    );
    write_style(
        &folders.inner_project,
        "c.md",
        &style_file(Some("Beta"), "Second beta.", "B2."),
    );
    let stack = layer_stack(
        &folders.codex_home,
        &[
            (&folders.outer_project, true),
            (&folders.inner_project, true),
        ],
    );
    let catalog =
        OutputStyleCatalog::load(&RecordingFs::default(), &folders.codex_home, &stack).await;

    assert_eq!(
        summary(&catalog),
        vec![
            ("default".to_string(), OutputStyleSource::Builtin),
            ("outcome-first".to_string(), OutputStyleSource::Builtin),
            ("explanatory".to_string(), OutputStyleSource::Builtin),
            ("learning".to_string(), OutputStyleSource::Builtin),
            ("alpha".to_string(), OutputStyleSource::Project),
            ("beta".to_string(), OutputStyleSource::Project),
            ("TERSE".to_string(), OutputStyleSource::Project),
            ("Zeta".to_string(), OutputStyleSource::User),
        ]
    );
    assert_eq!(
        catalog.find("terse").map(|style| &*style.text),
        Some("Inner.")
    );
    assert_eq!(catalog.find("BETA").map(|style| &*style.text), Some("B1."));
    let warnings = catalog.warnings.join("\n");
    assert_eq!(catalog.warnings.len(), 4, "{warnings}");
    assert!(
        warnings.contains("`Learning` is the name of a built-in style"),
        "{warnings}"
    );
    assert!(
        warnings.contains("the name `Beta` is already used by"),
        "{warnings}"
    );
    assert!(
        warnings.contains("the name `Terse` is already used by"),
        "{warnings}"
    );
    assert!(
        warnings.contains("the name `terse` is already used by"),
        "{warnings}"
    );
}

/// R1.2: the `output-styles` folder of a disabled (untrusted) project layer is not even
/// read, so its styles cannot be listed or selected.
#[tokio::test]
async fn untrusted_project_styles_are_not_read() {
    let folders = folders();
    write_style(
        &folders.outer_project,
        "evil.md",
        &style_file(None, "Evil.", "Evil."),
    );
    write_style(
        &folders.inner_project,
        "ok.md",
        &style_file(None, "Ok.", "Ok."),
    );
    let stack = layer_stack(
        &folders.codex_home,
        &[
            (&folders.outer_project, false),
            (&folders.inner_project, true),
        ],
    );
    let fs = RecordingFs::default();
    let catalog = OutputStyleCatalog::load(&fs, &folders.codex_home, &stack).await;
    assert!(catalog.find("ok").is_some());
    assert!(catalog.find("evil").is_none());
    let outer = folders.outer_project.display().to_string();
    assert!(
        fs.accessed().iter().all(|path| !path.starts_with(&outer)),
        "{:?}",
        fs.accessed()
    );
}

/// R1.5: only `.md` regular files directly inside the folder are read, without hidden files
/// or sub-folders, at most 256 by file name with one warning; a missing folder is silent.
#[tokio::test]
async fn folder_listing_rules_and_limit() {
    let folders = folders();
    for index in 0..300 {
        write_style(
            &folders.codex_home,
            &format!("style-{index:03}.md"),
            &style_file(None, "Many.", "Text."),
        );
    }
    write_style(
        &folders.codex_home,
        ".hidden.md",
        &style_file(None, "Hidden.", "Text."),
    );
    write_style(
        &folders.codex_home,
        "notes.txt",
        &style_file(None, "Text file.", "Text."),
    );
    let nested = folders.codex_home.join(OUTPUT_STYLES_DIR).join("nested");
    write_style(&nested, "deep.md", &style_file(None, "Deep.", "Text."));
    std::fs::create_dir_all(folders.codex_home.join(OUTPUT_STYLES_DIR).join("folder.md"))
        .expect("create folder named like a style");

    let stack = layer_stack(&folders.codex_home, &[(&folders.inner_project, true)]);
    let catalog =
        OutputStyleCatalog::load(&RecordingFs::default(), &folders.codex_home, &stack).await;
    let custom = catalog.names().split_off(4);
    assert_eq!(custom.len(), MAX_FILES_PER_DIR);
    assert_eq!(custom.first(), Some(&"style-000"));
    assert_eq!(custom.last(), Some(&"style-255"));
    assert_eq!(catalog.warnings.len(), 1, "{:?}", catalog.warnings);
    assert!(catalog.warnings[0].contains("has 300 style files"));
}
