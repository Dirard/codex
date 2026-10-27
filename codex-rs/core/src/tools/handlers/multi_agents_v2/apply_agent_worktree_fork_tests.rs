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
