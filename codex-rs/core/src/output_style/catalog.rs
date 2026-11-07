//! The output style catalog: built-in styles and Markdown style files (UC1).

use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::LazyLock;

use codex_config::ConfigLayerSource;
use codex_config::ConfigLayerStack;
use codex_exec_server::ExecutorFileSystem;
use codex_exec_server::ReadFileOptions;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;

use super::DEFAULT_OUTPUT_STYLE;
use super::OutputStyle;
use super::OutputStyleSource;

/// Folder of style files under `$CODEX_HOME` and under a project's `.codex/` folder.
pub(crate) const OUTPUT_STYLES_DIR: &str = "output-styles";
/// R1.3: the longest style text, in bytes.
pub(crate) const MAX_STYLE_TEXT_BYTES: usize = 16 * 1024;
/// R1.5: the most style files read from one folder.
pub(crate) const MAX_FILES_PER_DIR: usize = 256;

const DEFAULT_DESCRIPTION: &str = "Codex's standard communication; no extra instructions.";

/// Built-in style files, in the order of R1.6 after `default`.
const BUILTIN_FILES: [(&str, &str); 3] = [
    (
        "outcome-first",
        include_str!("../../assets/output_styles/outcome-first.md"),
    ),
    (
        "explanatory",
        include_str!("../../assets/output_styles/explanatory.md"),
    ),
    (
        "learning",
        include_str!("../../assets/output_styles/learning.md"),
    ),
];

/// R1.1, R1.6: the built-in styles, `default` first. The files go through the parser of
/// custom styles; a test keeps them valid.
static BUILTIN_STYLES: LazyLock<Vec<Arc<OutputStyle>>> = LazyLock::new(|| {
    let mut styles = vec![Arc::new(OutputStyle {
        name: DEFAULT_OUTPUT_STYLE.to_string(),
        description: DEFAULT_DESCRIPTION.to_string(),
        source: OutputStyleSource::Builtin,
        path: None,
        text: Arc::from(""),
    })];
    styles.extend(BUILTIN_FILES.iter().filter_map(|(stem, contents)| {
        let parsed = parse_style_file(contents, stem)
            .inspect_err(|error| tracing::error!("built-in output style {stem}: {error}"))
            .ok()?;
        Some(Arc::new(OutputStyle {
            name: parsed.name,
            description: parsed.description,
            source: OutputStyleSource::Builtin,
            path: None,
            text: Arc::from(parsed.text),
        }))
    }));
    styles
});

/// The built-in style named `name` (ASCII case-insensitive), without reading any files.
pub(crate) fn builtin_style(name: &str) -> Option<Arc<OutputStyle>> {
    BUILTIN_STYLES
        .iter()
        .find(|style| style.name.eq_ignore_ascii_case(name))
        .cloned()
}

/// A style file's metadata and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedStyleFile {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) text: String,
}

/// R1.3: reads `name` (default: `file_stem`) and the required `description` from the YAML
/// frontmatter between `---` lines, other keys ignored, and takes the trimmed rest as the
/// style text. Errors name the reason a file is skipped.
pub(crate) fn parse_style_file(contents: &str, file_stem: &str) -> Result<ParsedStyleFile, String> {
    let frontmatter =
        codex_skills::parse_skill_frontmatter_metadata(contents, || file_stem.to_string())
            .map_err(|error| error.to_string())?;
    let text = text_after_frontmatter(contents).trim().to_string();
    if text.is_empty() {
        return Err("the style text after the frontmatter is empty".to_string());
    }
    if text.len() > MAX_STYLE_TEXT_BYTES {
        return Err(format!(
            "the style text has {} bytes, more than the limit of {MAX_STYLE_TEXT_BYTES}",
            text.len()
        ));
    }
    Ok(ParsedStyleFile {
        name: frontmatter.name,
        description: frontmatter.description,
        text,
    })
}

/// Everything after the line that closes the frontmatter, the same split as the skills
/// parser (`skills/src/parser.rs#extract_frontmatter`); line ends become `\n`.
fn text_after_frontmatter(contents: &str) -> String {
    let mut lines = contents.lines();
    if !matches!(lines.next(), Some(line) if line.trim() == "---") {
        return String::new();
    }
    for line in lines.by_ref() {
        if line.trim() == "---" {
            break;
        }
    }
    lines.collect::<Vec<_>>().join("\n")
}

/// The styles a thread can select, with the reasons files were skipped (UC1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputStyleCatalog {
    /// Built-in styles in a fixed order, then custom styles by name (R1.6); names are unique
    /// ASCII case-insensitively.
    pub styles: Vec<Arc<OutputStyle>>,
    /// Files and folders that were skipped, and why (R1.3–R1.5).
    pub warnings: Vec<String>,
}

impl OutputStyleCatalog {
    /// Only the built-in styles.
    pub fn builtin() -> Self {
        Self {
            styles: BUILTIN_STYLES.clone(),
            warnings: Vec::new(),
        }
    }

    /// Use case 1 "Каталог стилей: встроенные, пользовательские и проектные"
    /// (`specs/output-styles-2026-11-07.md`): the built-in styles, then the `*.md` files of
    /// `output-styles/` in each enabled project layer (nearest project first) and in
    /// `codex_home`. Disabled (untrusted) project layers are not read (R1.2); invalid files
    /// and name clashes are skipped with a warning (R1.3, R1.4); a missing folder adds
    /// nothing (R1.5).
    pub async fn load(
        fs: &dyn ExecutorFileSystem,
        codex_home: &AbsolutePathBuf,
        layer_stack: &ConfigLayerStack,
    ) -> Self {
        let mut dirs: Vec<(AbsolutePathBuf, OutputStyleSource)> = layer_stack
            .layers_high_to_low()
            .filter_map(|layer| match &layer.name {
                ConfigLayerSource::Project { dot_codex_folder } => Some((
                    dot_codex_folder.join(OUTPUT_STYLES_DIR),
                    OutputStyleSource::Project,
                )),
                _ => None,
            })
            .collect();
        dirs.push((codex_home.join(OUTPUT_STYLES_DIR), OutputStyleSource::User));
        let mut seen_dirs = Vec::new();
        dirs.retain(|(dir, _)| {
            let new = !seen_dirs.contains(dir);
            seen_dirs.push(dir.clone());
            new
        });

        let mut catalog = Self::builtin();
        // Lowercase name -> the path that holds it (`None` for built-in styles).
        let mut names: HashMap<String, Option<AbsolutePathBuf>> = catalog
            .styles
            .iter()
            .map(|style| (style.name.to_ascii_lowercase(), None))
            .collect();
        let mut custom = Vec::new();
        for (dir, source) in dirs {
            for (path, file_stem) in list_style_files(fs, &dir, &mut catalog.warnings).await {
                let contents = match fs
                    .read_file_text(
                        &PathUri::from_abs_path(&path),
                        ReadFileOptions::default(),
                        /*sandbox*/ None,
                    )
                    .await
                {
                    Ok(contents) => contents,
                    Err(error) => {
                        catalog.warnings.push(skipped(&path, &error.to_string()));
                        continue;
                    }
                };
                let parsed = match parse_style_file(&contents, &file_stem) {
                    Ok(parsed) => parsed,
                    Err(reason) => {
                        catalog.warnings.push(skipped(&path, &reason));
                        continue;
                    }
                };
                match names.get(&parsed.name.to_ascii_lowercase()) {
                    Some(None) => {
                        catalog.warnings.push(skipped(
                            &path,
                            &format!("`{}` is the name of a built-in style", parsed.name),
                        ));
                        continue;
                    }
                    Some(Some(winner)) => {
                        catalog.warnings.push(skipped(
                            &path,
                            &format!(
                                "the name `{}` is already used by {}",
                                parsed.name,
                                winner.display()
                            ),
                        ));
                        continue;
                    }
                    None => {}
                }
                names.insert(parsed.name.to_ascii_lowercase(), Some(path.clone()));
                custom.push(Arc::new(OutputStyle {
                    name: parsed.name,
                    description: parsed.description,
                    source,
                    path: Some(path),
                    text: Arc::from(parsed.text),
                }));
            }
        }
        custom.sort_by_cached_key(|style| style.name.to_ascii_lowercase());
        catalog.styles.extend(custom);
        catalog
    }

    /// The style named `name`, compared ASCII case-insensitively (R1.4).
    pub fn find(&self, name: &str) -> Option<&Arc<OutputStyle>> {
        self.styles
            .iter()
            .find(|style| style.name.eq_ignore_ascii_case(name))
    }

    /// The names of all styles in catalog order.
    pub fn names(&self) -> Vec<&str> {
        self.styles
            .iter()
            .map(|style| style.name.as_str())
            .collect()
    }
}

/// R1.5: the `.md` regular files directly inside `dir` (no hidden files), by file name, at
/// most [`MAX_FILES_PER_DIR`]; a missing folder lists nothing.
async fn list_style_files(
    fs: &dyn ExecutorFileSystem,
    dir: &AbsolutePathBuf,
    warnings: &mut Vec<String>,
) -> Vec<(AbsolutePathBuf, String)> {
    let mut entries = match fs
        .read_directory(&PathUri::from_abs_path(dir), /*sandbox*/ None)
        .await
    {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            warnings.push(format!(
                "could not read the output style folder {}: {error}",
                dir.display()
            ));
            return Vec::new();
        }
    };
    entries.retain(|entry| {
        entry.is_file && !entry.file_name.starts_with('.') && entry.file_name.ends_with(".md")
    });
    entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    if entries.len() > MAX_FILES_PER_DIR {
        warnings.push(format!(
            "the output style folder {} has {} style files; only the first {MAX_FILES_PER_DIR} by file name were read",
            dir.display(),
            entries.len()
        ));
        entries.truncate(MAX_FILES_PER_DIR);
    }
    entries
        .into_iter()
        .map(|entry| {
            let stem = entry
                .file_name
                .strip_suffix(".md")
                .unwrap_or(&entry.file_name)
                .to_string();
            (dir.join(&entry.file_name), stem)
        })
        .collect()
}

fn skipped(path: &AbsolutePathBuf, reason: &str) -> String {
    format!("skipped output style file {}: {reason}", path.display())
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
