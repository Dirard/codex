use super::*;
use pretty_assertions::assert_eq;

fn change(path: &str, old: Option<&str>, new: Option<&str>) -> MergedChange {
    MergedChange {
        path: path.to_string(),
        old: old.map(str::to_string),
        new: new.map(str::to_string),
    }
}

/// R11.5: one operation per file; an update replaces every line of the current content.
#[test]
fn patch_text_replaces_every_line_of_each_file() {
    let patch = patch_text(&[
        change("a.txt", Some("one\ntwo\n"), Some("one\nTWO\n")),
        change("dir/new.txt", None, Some("x\n")),
        change("gone.txt", Some("y\n"), None),
    ]);
    assert_eq!(
        patch,
        "*** Begin Patch\n*** Update File: a.txt\n@@\n-one\n-two\n+one\n+TWO\n*** Add File: dir/new.txt\n+x\n*** Delete File: gone.txt\n*** End Patch"
    );
    codex_apply_patch::parse_patch(&patch).expect("a valid apply_patch body");
}

/// R11.12: a submodule change is refused with the command that updates the submodule, not
/// with the block's `git apply` command, which would skip it.
#[test]
fn submodule_refusal_names_the_submodule_command() {
    let moved = submodule_refusal("worker", "vendor/lib", Some("4347489d2bed"));
    assert_eq!(
        moved,
        "cannot apply agent worker: its branch moves submodule vendor/lib to 4347489d2bed, and apply_agent_worktree does not change submodules. Nothing was written. Update it yourself, then call apply_agent_worktree again for the other files.\nsubmodule: git -C vendor/lib checkout 4347489d2bed"
    );
    let removed = submodule_refusal("worker", "third party/lib's", None);
    assert!(
        removed.contains("removes submodule third party/lib's"),
        "{removed}"
    );
    assert!(
        removed.ends_with("\nsubmodule: git rm --cached 'third party/lib'\\''s'"),
        "{removed}"
    );
    assert!(!removed.contains("\napply:"));
}
