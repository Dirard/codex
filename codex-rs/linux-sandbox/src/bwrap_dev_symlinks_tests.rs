use super::*;
use crate::bwrap::BwrapOptions;
use crate::bwrap::create_bwrap_command_args;
use codex_protocol::protocol::FileSystemAccessMode;
use codex_protocol::protocol::FileSystemSandboxEntry;
use codex_protocol::protocol::FileSystemSandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;

fn policy_denying(paths: &[&str]) -> FileSystemSandboxPolicy {
    let mut policy = FileSystemSandboxPolicy::read_only();
    policy.entries.extend(paths.iter().map(|path| {
        FileSystemSandboxEntry::new(
            AbsolutePathBuf::from_absolute_path(path)
                .expect("absolute path")
                .into(),
            FileSystemAccessMode::Deny,
        )
    }));
    policy
}

#[test]
fn denied_dev_symlinks_reach_inner_stage_instead_of_mounts() {
    let inner = ["/inner", "--apply-seccomp-then-exec", "--", "/bin/true"].map(String::from);

    let args = create_bwrap_command_args(
        inner.to_vec(),
        &policy_denying(&["/dev/fd", "/dev/stderr/child"]),
        Path::new("/"),
        Path::new("/"),
        BwrapOptions::default(),
    )
    .expect("create bwrap args")
    .args;

    let separator = args.iter().position(|arg| arg == "--").expect("separator");
    assert!(
        !args[..separator]
            .iter()
            .any(|arg| arg.starts_with("/dev/fd") || arg.starts_with("/dev/stderr")),
        "bubblewrap cannot mount over its own /dev symlinks: {args:#?}"
    );
    assert_eq!(
        args[separator + 1..],
        [
            "/inner",
            "--deny-dev-symlink",
            "/dev/fd",
            "--deny-dev-symlink",
            "/dev/stderr",
            "--apply-seccomp-then-exec",
            "--",
            "/bin/true",
        ]
    );
}

#[test]
fn denied_dev_symlinks_require_inner_stage() {
    let error = create_bwrap_command_args(
        vec!["/bin/true".to_string()],
        &policy_denying(&["/dev/stdin"]),
        Path::new("/"),
        Path::new("/"),
        BwrapOptions::default(),
    )
    .expect_err("only the inner stage can enforce the denial");

    assert_eq!(
        error.to_string(),
        "Fatal error: cannot enforce sandbox deny-read path /dev/stdin without the inner sandbox stage"
    );
}

#[test]
fn inner_stage_replaces_only_bwrap_dev_symlinks() {
    let error = replace_denied(&[PathBuf::from("/tmp/file")]).expect_err("unknown path");

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}
