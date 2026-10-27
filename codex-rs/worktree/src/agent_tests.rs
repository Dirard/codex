//! Tests locale-independent summaries and agent branch validation.

use pretty_assertions::assert_eq;

use super::branch_ref;
use super::format_shortstat;

/// K5: the `/proc` scan excludes a process by pid only while it is a registered own Git child,
/// never by its command name. A foreign process in the worktree (here a `sleep`, standing in
/// for a foreign `git`) is always seen; the same pid, registered as the crate's own child, is
/// skipped; once the guard is dropped it is seen again.
#[cfg(target_os = "linux")]
#[test]
fn the_proc_scan_excludes_own_git_children_by_pid_not_name() {
    use std::process::Command;

    let dir = tempfile::tempdir().expect("temp worktree");
    let root = dir.path();
    let mut child = Command::new("sleep")
        .arg("30")
        .current_dir(root)
        .spawn()
        .expect("spawn a process in the worktree");
    let pid = child.id();
    let in_worktree = || {
        super::processes_in(root)
            .iter()
            .any(|line| line.starts_with(&format!("pid {pid}:")))
    };

    assert!(in_worktree(), "a foreign process in the worktree is seen");
    {
        let _tracked = crate::git::fork::GitChild::new(pid);
        assert!(!in_worktree(), "an own Git child is excluded by pid");
    }
    assert!(in_worktree(), "the pid is seen again once the guard is dropped");

    child.kill().expect("stop the process");
    child.wait().expect("reap the process");
}

#[test]
fn shortstat_matches_git_wording() {
    assert_eq!(format_shortstat(1, 1, 0), "1 file changed, 1 insertion(+)");
    assert_eq!(
        format_shortstat(2, 3, 1),
        "2 files changed, 3 insertions(+), 1 deletion(-)"
    );
    assert_eq!(format_shortstat(1, 0, 0), "1 file changed");
}

#[test]
fn agent_branches_stay_in_their_namespace() {
    assert_eq!(
        branch_ref("codex/agents/probe-01a1748c").expect("valid branch"),
        "refs/heads/codex/agents/probe-01a1748c"
    );
    assert_eq!(
        branch_ref("codex/agents/probe-83334444deadbeef-2").expect("valid free branch"),
        "refs/heads/codex/agents/probe-83334444deadbeef-2"
    );
    for invalid in [
        "main",
        "codex/agents/../main",
        "codex/agents/with space",
        "codex/agents/a:b",
        "codex/agents/x</agent_worktree>",
        "codex/agents/x;y",
        "codex/agents/x\ny",
    ] {
        assert!(branch_ref(invalid).is_err(), "{invalid} must be rejected");
    }
}
