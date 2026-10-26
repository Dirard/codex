//! Deny-read rules for the `/dev` symlinks that bubblewrap creates must start the sandbox and
//! keep the denied path unusable.

use super::LONG_TIMEOUT_MS;
use super::create_env_from_core_vars;
use super::run_cmd_result_with_permission_profile_for_cwd;
use super::should_skip_bwrap_tests;
use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;

// Each script fails when the denied path still works, then shows that the
// neighbouring `/dev` entries and the sandboxed command itself still do.
#[test_case::test_case(
    "/dev/fd",
    "exec 3<data; if /bin/cat /dev/fd/3; then exit 21; fi; /bin/cat <&3 >/dev/stdout";
    "fd"
)]
#[test_case::test_case(
    "/dev/stdin",
    "if /bin/cat /dev/stdin <data; then exit 22; fi; /bin/cat /dev/fd/0 <data";
    "stdin"
)]
#[tokio::test]
async fn sandbox_denies_bwrap_dev_symlinks(denied: &str, script: &str) {
    if should_skip_bwrap_tests().await {
        eprintln!("skipping bwrap test: bwrap sandbox prerequisites are unavailable");
        return;
    }

    let workspace = tempfile::tempdir().expect("tempdir");
    std::fs::write(workspace.path().join("data"), "visible").expect("write data");
    let cwd = AbsolutePathBuf::try_from(workspace.path()).expect("absolute workspace");
    let mut policy = FileSystemSandboxPolicy::read_only();
    policy.entries.push(FileSystemSandboxEntry::new(
        cwd.clone().into(),
        FileSystemAccessMode::Write,
    ));
    policy.entries.push(FileSystemSandboxEntry::new(
        AbsolutePathBuf::from_absolute_path(denied)
            .expect("absolute denied path")
            .into(),
        FileSystemAccessMode::Deny,
    ));
    let output = run_cmd_result_with_permission_profile_for_cwd(
        &["/bin/sh", "-c", script],
        cwd,
        PermissionProfile::from_runtime_permissions(&policy, NetworkSandboxPolicy::Restricted),
        create_env_from_core_vars(),
        LONG_TIMEOUT_MS,
        /*use_legacy_landlock*/ false,
    )
    .await
    .expect("sandboxed command should execute");

    assert_eq!(
        (output.exit_code, output.stdout.text.as_str()),
        (0, "visible"),
        "stderr: {}",
        output.stderr.text
    );
}
