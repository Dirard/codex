//! Saved workflow scripts that `workflow(name)` and the `workflow` tool's `name` run (UC12,
//! R12.1, R12.3, R12.4).

use std::path::Path;
use std::path::PathBuf;

use crate::meta::MAX_SCRIPT_BYTES;
use crate::meta::is_valid_name;

/// A saved script found by [`find_saved_script`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedScript {
    pub path: PathBuf,
    pub source: String,
}

/// UC12: find `<name>.js` in `dirs`, searched in order. The name follows the rule for
/// `meta.name`, so it cannot leave the searched folders. The error names every searched
/// folder or the violated rule (R12.4).
pub fn find_saved_script(dirs: &[PathBuf], name: &str) -> Result<SavedScript, String> {
    if !is_valid_name(name) {
        return Err(format!(
            "workflow name {name:?} is not valid: use letters, digits, `_` and `-`, start with a letter or digit and stay within 64 characters"
        ));
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
        return Ok(SavedScript { path, source });
    }
    let searched = dirs
        .iter()
        .map(|dir| display(dir))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "no saved workflow {file_name} in {}",
        if searched.is_empty() {
            "any folder".to_string()
        } else {
            searched
        }
    ))
}

fn display(dir: &Path) -> String {
    dir.display().to_string()
}

#[cfg(test)]
#[path = "saved_tests.rs"]
mod tests;
