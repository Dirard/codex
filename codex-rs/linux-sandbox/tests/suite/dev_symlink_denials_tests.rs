//! Deny-read rules for the `/dev` symlinks that bubblewrap creates must start the sandbox and
//! keep the denied path unusable.

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

use super::LONG_TIMEOUT_MS;
use super::codex_linux_sandbox_exe;
use super::create_env_from_core_vars;
use super::run_cmd_result_with_permission_profile_for_cwd;
use super::should_skip_bwrap_tests;
use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::FileSystemSandboxPolicyContext;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;

// Each script fails when the denied path still works, then shows that the
// neighbouring `/dev` entries and the sandboxed command itself still do. A
// writable root `/` mounts bubblewrap's `/dev` again after binding `/`.
#[test_case::test_case(
    "/dev/fd",
    "exec 3<data; if /bin/cat /dev/fd/3; then exit 21; fi; /bin/cat <&3 >/dev/stdout",
    false;
    "fd"
)]
#[test_case::test_case(
    "/dev/stdin",
    "if /bin/cat /dev/stdin <data; then exit 22; fi; /bin/cat /dev/fd/0 <data",
    false;
    "stdin"
)]
#[test_case::test_case(
    "/dev/fd",
    "exec 3<data; if /bin/cat /dev/fd/3; then exit 21; fi; /bin/cat <&3 >/dev/stdout",
    true;
    "fd under writable root"
)]
#[tokio::test]
async fn sandbox_denies_bwrap_dev_symlinks(denied: &str, script: &str, writable_root: bool) {
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
    if writable_root {
        // An approved command writes everywhere but keeps the denials.
        let cwd = cwd.clone().into();
        policy = policy.for_approved_command(&FileSystemSandboxPolicyContext {
            cwd: &cwd,
            workspace_roots: std::slice::from_ref(&cwd),
            user_home_dir: None,
            temporary_directories: Some(&[]),
        });
    }
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

fn read_only_profile() -> String {
    serde_json::to_string(&PermissionProfile::read_only())
        .expect("read-only permission profile should serialize")
}

/// Review S3 (docs/fork/reviews/2026-10-26-sandbox-compaction-review.md): only the inner stage
/// applies the flag, so any other stage fails instead of running without the denial.
#[test]
fn deny_dev_symlink_requires_the_inner_stage() {
    let output = Command::new(codex_linux_sandbox_exe())
        .args(["--sandbox-policy-cwd", "/", "--permission-profile"])
        .arg(read_only_profile())
        .args(["--deny-dev-symlink", "/dev/fd", "--", "/bin/true"])
        .output()
        .expect("sandbox helper should start");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--deny-dev-symlink is only supported in the inner sandbox stage"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Review S4: an inner stage started outside bubblewrap must not touch the `/dev` it finds,
/// here the host's.
#[test]
fn inner_stage_refuses_a_dev_that_is_not_the_sandbox_tmpfs() {
    let user = std::fs::metadata("/proc/self").map(|metadata| metadata.uid());
    if user.as_ref().is_ok_and(|user| *user == 0) || !Path::new("/dev/fd").is_symlink() {
        eprintln!("skipping: needs an unprivileged user and a host /dev/fd symlink");
        return;
    }

    let output = Command::new(codex_linux_sandbox_exe())
        .args(["--sandbox-policy-cwd", "/", "--permission-profile"])
        .arg(read_only_profile())
        .args([
            "--apply-seccomp-then-exec",
            "--deny-dev-symlink",
            "/dev/fd",
            "--",
            "/bin/true",
        ])
        .output()
        .expect("sandbox helper should start");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("/dev is not the sandbox /dev tmpfs of bubblewrap"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Path::new("/dev/fd").is_symlink());
}

/// Review S4: a capability-bearing caller, such as root outside bubblewrap, fails the
/// capability check before the inner stage changes its `/dev`. A user namespace with a fresh
/// tmpfs on `/dev` stands in for that caller, so the host stays untouched.
#[test]
fn inner_stage_checks_capabilities_before_changing_dev() {
    match Command::new("unshare")
        .args(["--user", "--map-root-user", "--mount", "--", "true"])
        .output()
    {
        Ok(probe) if probe.status.success() => {}
        _ => {
            eprintln!("skipping: unprivileged user and mount namespaces are unavailable");
            return;
        }
    }

    let output = Command::new("unshare")
        .args([
            "--user",
            "--map-root-user",
            "--mount",
            "--",
            "/bin/sh",
            "-c",
        ])
        .arg(
            r#"mount -t tmpfs tmpfs /dev || exit 90
ln -s /proc/self/fd /dev/fd || exit 91
"$0" --sandbox-policy-cwd / --permission-profile "$1" \
    --apply-seccomp-then-exec --deny-dev-symlink /dev/fd -- /bin/true
status=$?
if [ -L /dev/fd ]; then printf intact; fi
exit "$status""#,
        )
        .arg(codex_linux_sandbox_exe())
        .arg(read_only_profile())
        .output()
        .expect("unshare should start");

    let stderr = String::from_utf8_lossy(&output.stderr);
    if matches!(output.status.code(), Some(90 | 91)) {
        eprintln!("skipping: cannot mount a tmpfs on /dev in a user namespace: {stderr}");
        return;
    }
    assert!(!output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "intact");
    assert!(
        stderr.contains("Linux sandbox retained effective or permitted capabilities"),
        "unexpected stderr: {stderr}"
    );
}
