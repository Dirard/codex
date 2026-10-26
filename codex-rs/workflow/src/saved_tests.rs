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
        find_saved_script(&dirs, "verify"),
        Ok(SavedScript {
            path: project.join("verify.js"),
            source: "project verify".to_string(),
        })
    );
    assert_eq!(
        find_saved_script(&dirs, "report").map(|script| script.source),
        Ok("home report".to_string())
    );
    for bad in ["../verify", "a/b", "my flow", "", "-x"] {
        let error = find_saved_script(&dirs, bad).expect_err(bad);
        assert!(error.contains("is not valid"), "{error}");
    }
    let missing = find_saved_script(&dirs, "absent").expect_err("missing");
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
    let too_big = find_saved_script(&dirs, "big").expect_err("too big");
    assert!(too_big.contains("the limit is"), "{too_big}");
}
