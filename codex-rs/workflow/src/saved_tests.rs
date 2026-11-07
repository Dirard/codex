use super::*;
use pretty_assertions::assert_eq;

/// R12.1, R12.4: the first folder that has the script wins; bad names and missing scripts
/// fail with the searched folders.
#[test]
fn saved_scripts_are_found_in_order() {
    let root = tempfile::tempdir().expect("tempdir");
    let project = root.path().join("project");
    let home = root.path().join("home");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::write(home.join("verify.js"), "home verify").expect("write");
    std::fs::write(home.join("report.js"), "home report").expect("write");
    std::fs::write(project.join("verify.js"), "project verify").expect("write");
    let dirs = vec![project.clone(), home.clone()];

    assert_eq!(
        find_saved_script(&dirs, "verify", /*builtins*/ false),
        Ok(SavedScript {
            name: "verify".to_string(),
            origin: ScriptOrigin::File(project.join("verify.js")),
            source: "project verify".to_string(),
            replaces_builtin: false,
        })
    );
    assert_eq!(
        find_saved_script(&dirs, "report", /*builtins*/ false).map(|script| script.source),
        Ok("home report".to_string())
    );
    for bad in ["../verify", "a/b", "my flow", "", "-x"] {
        let error = find_saved_script(&dirs, bad, /*builtins*/ true).expect_err(bad);
        assert!(error.contains("is not valid"), "{error}");
    }
    let missing = find_saved_script(&dirs, "absent", /*builtins*/ false).expect_err("missing");
    assert!(
        missing.starts_with("no saved workflow absent.js in "),
        "{missing}"
    );
    assert!(
        missing.contains(&project.display().to_string()),
        "{missing}"
    );
    assert!(missing.contains(&home.display().to_string()), "{missing}");

    std::fs::write(project.join("big.js"), "x".repeat(MAX_SCRIPT_BYTES + 1)).expect("write");
    let too_big = find_saved_script(&dirs, "big", /*builtins*/ false).expect_err("too big");
    assert!(too_big.contains("the limit is"), "{too_big}");
}

/// Builtin-workflows R1.1, R1.3, R1.4: without files `review` is the built-in script when
/// built-ins are enabled; without them the error does not mention built-ins.
#[test]
fn builtins_are_the_last_source() {
    let root = tempfile::tempdir().expect("tempdir");
    let dirs = vec![root.path().join("project"), root.path().join("home")];

    let review = find_saved_script(&dirs, "review", /*builtins*/ true).expect("built-in");
    assert_eq!(
        review.origin,
        ScriptOrigin::Builtin {
            name: "review".to_string()
        }
    );
    assert_eq!(
        review.source,
        crate::find_builtin("review").expect("table").source
    );
    assert!(!review.replaces_builtin);
    assert_eq!(review.origin.to_string(), "built-in");
    assert_eq!(review.origin.journal_path(), "built-in:review");
    assert_eq!(review.path(), None);

    for name in ["review", "builtin-review"] {
        let error = find_saved_script(&dirs, name, /*builtins*/ false).expect_err(name);
        assert!(
            error.starts_with(&format!("no saved workflow {name}.js in ")),
            "{error}"
        );
        assert!(!error.contains("built-in"), "{error}");
    }
}

/// Builtin-workflows R1.1, R1.2, R1.5: a project file replaces a `$CODEX_HOME` file and the
/// built-in of the same name; `builtin-review` runs the built-in even then; a file named
/// `builtin-review.js` is never read while built-ins are enabled.
#[test]
fn files_replace_builtins_and_the_prefix_reaches_them() {
    let root = tempfile::tempdir().expect("tempdir");
    let project = root.path().join("project");
    let home = root.path().join("home");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::create_dir_all(&home).expect("home");
    let dirs = vec![project.clone(), home.clone()];

    std::fs::write(home.join("review.js"), "home review").expect("write");
    let from_home = find_saved_script(&dirs, "review", /*builtins*/ true).expect("home");
    assert_eq!(from_home.origin, ScriptOrigin::File(home.join("review.js")));
    assert!(from_home.replaces_builtin);
    std::fs::write(project.join("review.js"), "project review").expect("write");
    let from_project = find_saved_script(&dirs, "review", /*builtins*/ true).expect("project");
    assert_eq!(
        (
            from_project.origin,
            from_project.source,
            from_project.replaces_builtin
        ),
        (
            ScriptOrigin::File(project.join("review.js")),
            "project review".to_string(),
            true
        )
    );
    // Without built-ins the same file is an ordinary saved script.
    assert!(
        !find_saved_script(&dirs, "review", /*builtins*/ false)
            .expect("file")
            .replaces_builtin
    );

    std::fs::write(project.join("builtin-review.js"), "a file").expect("write");
    let builtin = find_saved_script(&dirs, "builtin-review", /*builtins*/ true).expect("prefix");
    assert_eq!(
        (builtin.name.as_str(), &builtin.origin),
        (
            "builtin-review",
            &ScriptOrigin::Builtin {
                name: "review".to_string()
            }
        )
    );
    assert_eq!(
        builtin.source,
        crate::find_builtin("review").expect("table").source
    );
    // Without built-ins the prefix means nothing and the file is found.
    assert_eq!(
        find_saved_script(&dirs, "builtin-review", /*builtins*/ false)
            .expect("file")
            .source,
        "a file"
    );
}

/// Builtin-workflows R1.2, R1.4: `builtin-absent` lists the built-in names; an unknown name
/// lists the searched folders and the built-in names.
#[test]
fn misses_list_the_builtins() {
    let root = tempfile::tempdir().expect("tempdir");
    let dirs = vec![root.path().join("project"), root.path().join("home")];
    let names = crate::builtin_names().join(", ");

    let error = find_saved_script(&dirs, "builtin-absent", /*builtins*/ true).expect_err("absent");
    assert_eq!(
        error,
        format!("no built-in workflow \"absent\"; built-in workflows: {names}")
    );
    let error = find_saved_script(&dirs, "absent", /*builtins*/ true).expect_err("absent");
    assert_eq!(
        error,
        format!(
            "no saved workflow absent.js in {}, {}; built-in workflows: {names} (run one by its name, or as builtin-<name> when a file of the same name replaces it)",
            dirs[0].display(),
            dirs[1].display()
        )
    );
}
