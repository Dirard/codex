use super::*;
use codex_protocol::protocol::TurnEnvironmentSelection;
use codex_protocol::protocol::TurnEnvironmentSelections;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;

fn absolute_path(path: impl AsRef<std::path::Path>) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("path should be absolute")
}

fn selection(environment_id: &str, cwd: &AbsolutePathBuf) -> TurnEnvironmentSelection {
    TurnEnvironmentSelection {
        environment_id: environment_id.to_string(),
        cwd: PathUri::from_abs_path(cwd),
    }
}

#[test]
fn apply_cwd_to_local_environment_preserves_non_local_environment_cwds() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let original_local = absolute_path(temp_dir.path().join("local-original"));
    let original_remote = absolute_path(temp_dir.path().join("remote-original"));
    let requested_cwd = absolute_path(temp_dir.path().join("requested"));
    let mut environments = TurnEnvironmentSelections::new(
        original_local.clone(),
        vec![
            selection(LOCAL_ENVIRONMENT_ID, &original_local),
            selection("remote-container", &original_remote),
        ],
    );

    apply_cwd_to_local_environment(&mut environments, requested_cwd.clone());

    assert_eq!(
        environments,
        TurnEnvironmentSelections::new(
            requested_cwd.clone(),
            vec![
                selection(LOCAL_ENVIRONMENT_ID, &requested_cwd),
                selection("remote-container", &original_remote),
            ],
        )
    );
}
