use crate::ClientNotification;
use crate::ClientRequest;
use crate::ServerNotification;
use crate::ServerNotificationEnvelope;
use crate::ServerRequest;
use crate::export::GENERATED_TS_HEADER;
use crate::export::filter_experimental_ts_tree;
use crate::export::generate_index_ts_tree;
use crate::export::trim_trailing_line_whitespace;
use crate::protocol::common::visit_client_response_types;
use crate::protocol::common::visit_server_response_types;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde_json::Map;
use serde_json::Value;
use std::any::TypeId;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use ts_rs::TS;
use ts_rs::TypeVisitor;

#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaFixtureOptions {
    pub experimental_api: bool,
}

pub fn read_schema_fixture_tree(schema_root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let typescript_root = schema_root.join("typescript");
    let json_root = schema_root.join("json");

    let mut all = BTreeMap::new();
    for (rel, bytes) in collect_files_recursive(&typescript_root, TypeScriptHeaderMode::Preserve)? {
        all.insert(PathBuf::from("typescript").join(rel), bytes);
    }
    for (rel, bytes) in collect_files_recursive(&json_root, TypeScriptHeaderMode::Preserve)? {
        all.insert(PathBuf::from("json").join(rel), bytes);
    }

    Ok(all)
}

pub fn read_schema_fixture_subtree(
    schema_root: &Path,
    label: &str,
) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let subtree_root = schema_root.join(label);
    collect_files_recursive(&subtree_root, TypeScriptHeaderMode::Strip)
        .with_context(|| format!("read schema fixture subtree {}", subtree_root.display()))
}

#[doc(hidden)]
pub fn generate_typescript_schema_fixture_subtree_for_tests() -> Result<BTreeMap<PathBuf, Vec<u8>>>
{
    let mut files = BTreeMap::new();
    let mut seen = HashSet::new();

    collect_typescript_fixture_file::<ClientRequest>(&mut files, &mut seen)?;
    visit_typescript_fixture_dependencies(&mut files, &mut seen, |visitor| {
        visit_client_response_types(visitor);
    })?;
    collect_typescript_fixture_file::<ClientNotification>(&mut files, &mut seen)?;
    collect_typescript_fixture_file::<ServerRequest>(&mut files, &mut seen)?;
    visit_typescript_fixture_dependencies(&mut files, &mut seen, |visitor| {
        visit_server_response_types(visitor);
    })?;
    collect_typescript_fixture_file::<ServerNotification>(&mut files, &mut seen)?;
    collect_typescript_fixture_file::<ServerNotificationEnvelope>(&mut files, &mut seen)?;

    filter_experimental_ts_tree(&mut files)?;
    generate_index_ts_tree(&mut files);
    for content in files.values_mut() {
        *content = trim_trailing_line_whitespace(content);
    }

    Ok(files
        .into_iter()
        .map(|(path, content)| (path, content.into_bytes()))
        .collect())
}

/// Regenerates `schema/typescript/` and `schema/json/`.
///
/// This is intended to be used by tooling (e.g., `just write-app-server-schema`).
/// It deletes any previously generated files so stale artifacts are removed.
pub fn write_schema_fixtures(schema_root: &Path, prettier: Option<&Path>) -> Result<()> {
    write_schema_fixtures_with_options(schema_root, prettier, SchemaFixtureOptions::default())
}

pub fn check_schema_fixtures_with_options(
    schema_root: &Path,
    prettier: Option<&Path>,
    options: SchemaFixtureOptions,
) -> Result<()> {
    let generated_root = TempSchemaFixtureRoot::new()?;
    write_schema_fixtures_with_options(generated_root.path(), prettier, options).with_context(
        || {
            format!(
                "generate schema fixtures under {}",
                generated_root.display()
            )
        },
    )?;

    let checked_in_tree = read_schema_fixture_tree(schema_root)
        .with_context(|| format!("read schema fixture tree under {}", schema_root.display()))?;
    let generated_tree = read_schema_fixture_tree(generated_root.path()).with_context(|| {
        format!(
            "read generated schema fixture tree under {}",
            generated_root.display()
        )
    })?;

    ensure_schema_fixture_trees_match(schema_root, &checked_in_tree, &generated_tree)
}

/// Regenerates schema fixtures with configurable options.
pub fn write_schema_fixtures_with_options(
    schema_root: &Path,
    prettier: Option<&Path>,
    options: SchemaFixtureOptions,
) -> Result<()> {
    let typescript_out_dir = schema_root.join("typescript");
    let json_out_dir = schema_root.join("json");

    ensure_empty_dir(&typescript_out_dir)?;
    ensure_empty_dir(&json_out_dir)?;

    crate::generate_ts_with_options(
        &typescript_out_dir,
        prettier,
        crate::GenerateTsOptions {
            experimental_api: options.experimental_api,
            ..crate::GenerateTsOptions::default()
        },
    )?;
    crate::generate_json_with_experimental(&json_out_dir, options.experimental_api)?;

    Ok(())
}

fn ensure_empty_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)
            .with_context(|| format!("failed to remove {}", dir.display()))?;
    }
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(())
}

fn ensure_schema_fixture_trees_match(
    schema_root: &Path,
    checked_in_tree: &BTreeMap<PathBuf, Vec<u8>>,
    generated_tree: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    let checked_in_paths = checked_in_tree
        .keys()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let generated_paths = generated_tree
        .keys()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();

    if checked_in_paths != generated_paths {
        bail!(
            "schema fixture drift under {}: file set differs",
            schema_root.display()
        );
    }

    for (path, checked_in) in checked_in_tree {
        let generated = generated_tree
            .get(path)
            .with_context(|| format!("missing generated schema fixture {}", path.display()))?;
        if checked_in != generated {
            bail!(
                "schema fixture drift under {}: {} differs from freshly generated output",
                schema_root.display(),
                path.display()
            );
        }
    }

    Ok(())
}

struct TempSchemaFixtureRoot {
    path: PathBuf,
}

impl TempSchemaFixtureRoot {
    fn new() -> Result<Self> {
        let parent = std::env::temp_dir();
        let process_id = std::process::id();
        for attempt in 0..100 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock is before Unix epoch")?
                .as_nanos();
            let path = parent.join(format!(
                "codex-app-server-schema-fixtures-{process_id}-{nanos}-{attempt}"
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to create {}", path.display()));
                }
            }
        }
        bail!(
            "failed to create unique schema fixture temp directory under {}",
            parent.display()
        );
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn display(&self) -> std::path::Display<'_> {
        self.path.display()
    }
}

impl Drop for TempSchemaFixtureRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[derive(Clone, Copy, Debug)]
enum TypeScriptHeaderMode {
    Preserve,
    Strip,
}

fn read_file_bytes(path: &Path, typescript_header_mode: TypeScriptHeaderMode) -> Result<Vec<u8>> {
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if path.extension().is_some_and(|ext| ext == "json") {
        let value: Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse JSON in {}", path.display()))?;
        let value = canonicalize_schema_json(&value);
        let normalized = serde_json::to_vec_pretty(&value)
            .with_context(|| format!("failed to reserialize JSON in {}", path.display()))?;
        return Ok(normalized);
    }
    if path.extension().is_some_and(|ext| ext == "ts") {
        // Windows checkouts (and some generators) may produce CRLF; normalize so the
        // fixture test is platform-independent.
        let text = String::from_utf8(bytes)
            .with_context(|| format!("expected UTF-8 TypeScript in {}", path.display()))?;
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = match typescript_header_mode {
            TypeScriptHeaderMode::Preserve => text,
            // Legacy in-memory TypeScript fixture comparisons care about schema content,
            // not whether ts-rs output has been written with the standard tree banner.
            TypeScriptHeaderMode::Strip => text
                .strip_prefix(GENERATED_TS_HEADER)
                .unwrap_or(&text)
                .to_string(),
        };
        return Ok(text.into_bytes());
    }
    Ok(bytes)
}

pub(crate) fn canonicalize_schema_json(value: &Value) -> Value {
    canonicalize_json_for_key(None, value)
}

fn canonicalize_json_for_key(parent_key: Option<&str>, value: &Value) -> Value {
    match value {
        Value::Array(items) => {
            let mut items = items
                .iter()
                .map(|item| canonicalize_json_for_key(None, item))
                .collect::<Vec<_>>();
            if should_sort_schema_array(parent_key, &items) {
                items.sort_by_key(schema_array_sort_key);
            }
            Value::Array(items)
        }
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by_key(|(key, _)| *key);
            let mut sorted = Map::with_capacity(map.len());
            for (key, child) in entries {
                sorted.insert(key.clone(), canonicalize_json_for_key(Some(key), child));
            }
            Value::Object(sorted)
        }
        _ => value.clone(),
    }
}

fn should_sort_schema_array(parent_key: Option<&str>, items: &[Value]) -> bool {
    match parent_key {
        Some("required") => items.iter().all(|item| matches!(item, Value::String(_))),
        Some("enum") => items.iter().all(is_scalar_json_value),
        Some("type") => items.iter().all(is_json_schema_primitive_type_name),
        _ => false,
    }
}

fn is_scalar_json_value(item: &Value) -> bool {
    matches!(
        item,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn is_json_schema_primitive_type_name(item: &Value) -> bool {
    matches!(
        item,
        Value::String(value)
            if matches!(
                value.as_str(),
                "array" | "boolean" | "integer" | "null" | "number" | "object" | "string"
            )
    )
}

fn schema_array_sort_key(item: &Value) -> String {
    match item {
        Value::Null => "null".to_string(),
        Value::Bool(value) => format!("bool:{value}"),
        Value::Number(value) => format!("number:{value}"),
        Value::String(value) => format!("string:{value}"),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(item).unwrap_or_default(),
    }
}

fn collect_files_recursive(
    root: &Path,
    typescript_header_mode: TypeScriptHeaderMode,
) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = BTreeMap::new();

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .with_context(|| format!("failed to read dir {}", dir.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to read dir entry in {}", dir.display()))?;
            let path = entry.path();
            // On some platforms, Bazel runfiles are symlinks. `DirEntry::file_type()` does not
            // follow symlinks, so use `metadata()` here to treat symlinks as the files/dirs they
            // point to.
            let metadata = std::fs::metadata(&path)
                .with_context(|| format!("failed to stat {}", path.display()))?;
            if metadata.is_dir() {
                stack.push(path);
                continue;
            } else if !metadata.is_file() {
                continue;
            }

            let rel = path
                .strip_prefix(root)
                .with_context(|| {
                    format!(
                        "failed to strip prefix {} from {}",
                        root.display(),
                        path.display()
                    )
                })?
                .to_path_buf();

            files.insert(rel, read_file_bytes(&path, typescript_header_mode)?);
        }
    }

    Ok(files)
}

fn collect_typescript_fixture_file<T: TS + 'static + ?Sized>(
    files: &mut BTreeMap<PathBuf, String>,
    seen: &mut HashSet<TypeId>,
) -> Result<()> {
    let Some(output_path) = T::output_path() else {
        return Ok(());
    };
    if !seen.insert(TypeId::of::<T>()) {
        return Ok(());
    }

    let contents = T::export_to_string().context("export TypeScript fixture content")?;
    let output_path = normalize_relative_fixture_path(&output_path);
    files.insert(
        output_path,
        contents.replace("\r\n", "\n").replace('\r', "\n"),
    );

    let mut visitor = TypeScriptFixtureCollector {
        files,
        seen,
        error: None,
    };
    T::visit_dependencies(&mut visitor);
    if let Some(error) = visitor.error {
        return Err(error);
    }

    Ok(())
}

fn normalize_relative_fixture_path(path: &Path) -> PathBuf {
    path.components().collect()
}

fn visit_typescript_fixture_dependencies(
    files: &mut BTreeMap<PathBuf, String>,
    seen: &mut HashSet<TypeId>,
    visit: impl FnOnce(&mut TypeScriptFixtureCollector<'_>),
) -> Result<()> {
    let mut visitor = TypeScriptFixtureCollector {
        files,
        seen,
        error: None,
    };
    visit(&mut visitor);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(())
}

struct TypeScriptFixtureCollector<'a> {
    files: &'a mut BTreeMap<PathBuf, String>,
    seen: &'a mut HashSet<TypeId>,
    error: Option<anyhow::Error>,
}

impl TypeVisitor for TypeScriptFixtureCollector<'_> {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if self.error.is_some() {
            return;
        }
        self.error = collect_typescript_fixture_file::<T>(self.files, self.seen).err();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn canonicalize_json_sorts_required_string_arrays() {
        let value = serde_json::json!({
            "required": ["b", "a"],
        });
        let expected = serde_json::json!({
            "required": ["a", "b"],
        });
        assert_eq!(canonicalize_schema_json(&value), expected);
    }

    #[test]
    fn canonicalize_json_sorts_scalar_enum_and_type_arrays() {
        let value = serde_json::json!({
            "enum": ["z", "a", "m"],
            "type": ["null", "string"],
        });
        let expected = serde_json::json!({
            "enum": ["a", "m", "z"],
            "type": ["null", "string"],
        });
        assert_eq!(canonicalize_schema_json(&value), expected);
    }

    #[test]
    fn canonicalize_json_preserves_one_of_any_of_and_all_of_order() {
        let value = serde_json::json!({
            "oneOf": [
                {"$ref": "#/definitions/B"},
                {"$ref": "#/definitions/A"}
            ],
            "anyOf": [
                {"title": "B"},
                {"title": "A"}
            ],
            "allOf": [
                {"type": "string"},
                {"type": "null"}
            ],
        });
        let expected = value.clone();
        assert_eq!(canonicalize_schema_json(&value), expected);
    }
}
