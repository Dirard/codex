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

/// R11.12, P3-4: a submodule change is refused with the command that puts it into the
/// checkout, not with the block's `git apply` command, which would skip it. A submodule the
/// parent did not check out is staged, since `git -C <path>` would run in the parent's own
/// repository; a removal goes through `git rm`, so the snapshot does not add it back.
#[test]
fn submodule_refusal_names_the_submodule_command() {
    let checkout = tempfile::TempDir::new().expect("checkout");
    let root = checkout.path();
    std::fs::create_dir_all(root.join("vendor/lib/.git")).expect("checked-out submodule");
    let moved = submodule_refusal("worker", root, "vendor/lib", Some("4347489d2bed"));
    assert_eq!(
        moved,
        format!(
            "cannot apply agent worker: its branch moves submodule vendor/lib to 4347489d2bed, and apply_agent_worktree does not change submodules. Nothing was written. Run this command, then call apply_agent_worktree again for the other files.\nsubmodule: git -C {}/vendor/lib checkout 4347489d2bed",
            root.display()
        )
    );
    let unpopulated = submodule_refusal("worker", root, "vendor/other", Some("4347489d2bed"));
    assert!(
        unpopulated.ends_with(&format!(
            "\nsubmodule: git -C {} update-index --cacheinfo 160000,4347489d2bed,vendor/other",
            root.display()
        )),
        "{unpopulated}"
    );
    let removed = submodule_refusal("worker", root, "third party/lib's", None);
    assert!(
        removed.contains("removes submodule third party/lib's"),
        "{removed}"
    );
    assert!(removed.contains("refuses a submodule with local changes"));
    assert!(
        removed.ends_with(&format!(
            "\nsubmodule: git -C {} rm -- 'third party/lib'\\''s'",
            root.display()
        )),
        "{removed}"
    );
    assert!(!removed.contains("\napply:"));
}
