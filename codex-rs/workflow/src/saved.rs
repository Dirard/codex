//! Saved workflow scripts that `workflow(name)` and the `workflow` tool's `name` run (UC12,
//! R12.1, R12.3, R12.4), and the built-in scripts of the binary as the last source of the
//! same lookup (`specs/builtin-workflows-2026-11-07.md`, UC1).

use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use crate::builtin::BUILTIN_PREFIX;
use crate::builtin::builtin_names;
use crate::builtin::find_builtin;
use crate::meta::MAX_SCRIPT_BYTES;
use crate::meta::is_valid_name;

/// Where a saved script comes from (builtin-workflows UC1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptOrigin {
    /// A file of a project `.codex/workflows/` or `$CODEX_HOME/workflows/` folder.
    File(PathBuf),
    /// A built-in script compiled into the binary, by its own name.
    Builtin { name: String },
}

impl ScriptOrigin {
    /// The path a nested start records in its journal: the file, or `built-in:<name>` (R1.7).
    pub fn journal_path(&self) -> String {
        match self {
            Self::File(path) => path.display().to_string(),
            Self::Builtin { name } => format!("built-in:{name}"),
        }
    }
}

impl fmt::Display for ScriptOrigin {
    /// `built-in` or the file path, as the start result and the run log name it (R1.7).
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(formatter, "{}", path.display()),
            Self::Builtin { .. } => formatter.write_str("built-in"),
        }
    }
}

/// A saved script found by [`find_saved_script`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedScript {
    /// The requested name, `builtin-` prefix included.
    pub name: String,
    pub origin: ScriptOrigin,
    pub source: String,
    /// A file shadows a built-in script of the same name (R1.5).
    pub replaces_builtin: bool,
}

impl SavedScript {
    /// The file of a saved script; `None` for a built-in one.
    pub fn path(&self) -> Option<&Path> {
        match &self.origin {
            ScriptOrigin::File(path) => Some(path),
            ScriptOrigin::Builtin { .. } => None,
        }
    }
}

/// UC12: find `<name>.js` in `dirs`, searched in order. The name follows the rule for
/// `meta.name`, so it cannot leave the searched folders. The error names every searched
/// folder or the violated rule (R12.4).
///
/// With `builtins` (builtin-workflows R1.1–R1.4), a name with the `builtin-` prefix runs
/// only the built-in script of the rest of the name and never reads a file; any other name
/// falls back to the built-in script of that name when no folder has it, and the error of
/// a miss lists the built-in names. Without `builtins` the lookup is the folders' only.
pub fn find_saved_script(
    dirs: &[PathBuf],
    name: &str,
    builtins: bool,
) -> Result<SavedScript, String> {
    if !is_valid_name(name) {
        return Err(format!(
            "workflow name {name:?} is not valid: use letters, digits, `_` and `-`, start with a letter or digit and stay within 64 characters"
        ));
    }
    if builtins && let Some(rest) = name.strip_prefix(BUILTIN_PREFIX) {
        return match find_builtin(rest) {
            Some(builtin) => Ok(builtin_script(name, builtin)),
            None => Err(format!(
                "no built-in workflow {rest:?}; built-in workflows: {}",
                builtin_names().join(", ")
            )),
        };
    }
    let file_name = format!("{name}.js");
    for dir in dirs {
        let path = dir.join(&file_name);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        if metadata.len() > MAX_SCRIPT_BYTES as u64 {
            return Err(format!(
                "saved workflow {} has {} bytes; the limit is {MAX_SCRIPT_BYTES}",
                path.display(),
                metadata.len()
            ));
        }
        let source = std::fs::read_to_string(&path).map_err(|error| {
            format!("failed to read saved workflow {}: {error}", path.display())
        })?;
        return Ok(SavedScript {
            name: name.to_string(),
            origin: ScriptOrigin::File(path),
            source,
            replaces_builtin: builtins && find_builtin(name).is_some(),
        });
    }
    if builtins && let Some(builtin) = find_builtin(name) {
        return Ok(builtin_script(name, builtin));
    }
    let searched = dirs
        .iter()
        .map(|dir| display(dir))
        .collect::<Vec<_>>()
        .join(", ");
    let mut error = format!(
        "no saved workflow {file_name} in {}",
        if searched.is_empty() {
            "any folder".to_string()
        } else {
            searched
        }
    );
    if builtins {
        error.push_str(&format!(
            "; built-in workflows: {} (run one by its name, or as {BUILTIN_PREFIX}<name> when a file of the same name replaces it)",
            builtin_names().join(", ")
        ));
    }
    Err(error)
}

fn builtin_script(name: &str, builtin: &crate::builtin::BuiltinWorkflow) -> SavedScript {
    SavedScript {
        name: name.to_string(),
        origin: ScriptOrigin::Builtin {
            name: builtin.name.to_string(),
        },
        source: builtin.source.to_string(),
        replaces_builtin: false,
    }
}

fn display(dir: &Path) -> String {
    dir.display().to_string()
}

#[cfg(test)]
#[path = "saved_tests.rs"]
mod tests;
