use super::*;
use codex_worktree::ChangedPath;
use pretty_assertions::assert_eq;

fn abs(path: &str) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("absolute test path")
}

fn binding(protected: bool) -> AgentWorktreeBinding {
    AgentWorktreeBinding {
        thread_id: ThreadId::new(),
        agent_path: AgentPath::root().join("probe").expect("agent path"),
        environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
        root: abs("/home/u/.codex/worktrees/6d88/demo"),
        cwd: abs("/home/u/.codex/worktrees/6d88/demo/sub"),
        source_root: abs("/work/demo"),
        metadata: AgentWorktreeMetadata {
            thread_id: "01a1748c-0000-7000-8000-000000000000".to_string(),
            base_sha: "378e730aa1b2c3d4e5f60718293a4b5c6d7e8f90".to_string(),
            base_kind: AgentWorktreeBaseKind::WorkingTree,
            branch: "codex/agents/probe-01a1748c".to_string(),
        },
        parent_checkout_protected: AtomicBool::new(protected),
        state: Mutex::default(),
    }
}

#[test]
fn workspace_roots_move_into_the_worktree() {
    let source = abs("/work/demo");
    let worktree = abs("/home/u/.codex/worktrees/6d88/demo");
    let rebased = rebase_workspace_roots(
        &[
            abs("/work/demo"),
            abs("/work/demo/crates/a"),
            abs("/opt/extra"),
            abs("/work"),
        ],
        &source,
        &worktree,
    );
    assert_eq!(
        rebased,
        [
            abs("/home/u/.codex/worktrees/6d88/demo"),
            abs("/home/u/.codex/worktrees/6d88/demo/crates/a"),
            abs("/opt/extra"),
        ]
    );
}

#[test]
fn a_dropped_ancestor_root_is_replaced_by_the_worktree() {
    let source = abs("/work/demo");
    let worktree = abs("/home/u/.codex/worktrees/6d88/demo");
    assert_eq!(
        rebase_workspace_roots(&[abs("/work")], &source, &worktree),
        [worktree.clone()]
    );
}

#[test]
fn absolute_permission_entries_follow_the_worktree() {
    use codex_protocol::permissions::FileSystemAccessMode;
    let source = abs("/work/demo");
    let worktree = abs("/home/u/.codex/worktrees/6d88/demo");
    let entry = |path: FileSystemPath, access| FileSystemSandboxEntry {
        path,
        access,
        missing_path_behavior: None,
    };
    let profile = PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted {
            entries: vec![
                entry(
                    FileSystemPath::Path {
                        path: PathUri::from_abs_path(&abs("/work/demo/generated")),
                    },
                    FileSystemAccessMode::Write,
                ),
                entry(
                    FileSystemPath::Path {
                        path: PathUri::from_abs_path(&abs("/opt/cache")),
                    },
                    FileSystemAccessMode::Write,
                ),
                entry(
                    FileSystemPath::GlobPattern {
                        pattern: "/work/demo/**/.env".to_string(),
                    },
                    FileSystemAccessMode::Deny,
                ),
            ],
            glob_scan_max_depth: None,
        },
        network: codex_protocol::permissions::NetworkSandboxPolicy::Restricted,
    };
    let PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted { entries, .. },
        ..
    } = rebase_permission_profile(&profile, &source, &worktree).expect("rebase profile")
    else {
        panic!("managed profile stays managed");
    };
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        [
            FileSystemPath::Path {
                path: PathUri::from_abs_path(&abs("/home/u/.codex/worktrees/6d88/demo/generated")),
            },
            FileSystemPath::Path {
                path: PathUri::from_abs_path(&abs("/opt/cache")),
            },
            FileSystemPath::GlobPattern {
                pattern: "/home/u/.codex/worktrees/6d88/demo/**/.env".to_string(),
            },
        ]
    );
}

#[test]
fn branch_names_use_the_task_and_thread_prefix() {
    let thread_id =
        ThreadId::from_string("01a1748c-1111-7222-8333-444455556666").expect("valid thread id");
    let agent_path = AgentPath::root()
        .join("reviewer")
        .and_then(|path| path.join("probe"))
        .expect("agent path");
    assert_eq!(
        agent_branch_name(&agent_path, thread_id),
        "codex/agents/probe-01a1748c"
    );
}

#[test]
fn completion_block_names_branch_changes_and_apply_command() {
    let block = render_worktree_block(
        &binding(true),
        None,
        Ok(Some(BranchChanges {
            shortstat: "2 files changed, 2 insertions(+)".to_string(),
            files: vec![
                ChangedPath {
                    status: "M".to_string(),
                    path: "a.txt".to_string(),
                },
                ChangedPath {
                    status: "A".to_string(),
                    path: "new_file.txt".to_string(),
                },
            ],
            total_files: 2,
        })),
    );
    assert_eq!(
        block,
        "<agent_worktree>
branch: codex/agents/probe-01a1748c
base: 378e730aa1b2 (snapshot of the parent working tree)
changes: 2 files changed, 2 insertions(+)
files: M a.txt; A new_file.txt
checkout: /home/u/.codex/worktrees/6d88/demo
apply: git diff --binary 378e730aa1b2c3d4e5f60718293a4b5c6d7e8f90 codex/agents/probe-01a1748c | git apply
</agent_worktree>"
    );
    for forbidden in ["git merge", "cherry-pick", "--3way"] {
        assert!(
            !block.contains(forbidden),
            "{forbidden} must not be offered"
        );
    }
}

#[test]
fn completion_block_without_changes_has_no_branch_or_command() {
    let block = render_worktree_block(&binding(true), None, Ok(None));
    assert!(block.contains("branch: none\n"));
    assert!(block.contains("changes: none\n"));
    assert!(!block.contains("apply:"));
}

#[test]
fn completion_block_reports_a_failed_checkpoint() {
    let error = "cannot update the agent branch:\n  lock exists".to_string();
    let block = render_worktree_block(&binding(true), Some(&error), Ok(None));
    assert!(block.contains("checkpoint_failed: cannot update the agent branch: lock exists\n"));
    assert!(block.contains("checkout: /home/u/.codex/worktrees/6d88/demo\n"));
}

#[test]
fn isolation_note_describes_the_worktree_and_protection() {
    let protected = isolation_developer_note(&binding(true));
    assert!(protected.starts_with(
        "You are working in an isolated git worktree at /home/u/.codex/worktrees/6d88/demo/sub."
    ));
    assert!(protected.contains("snapshot of the parent working tree (378e730aa1b2)"));
    assert!(protected.contains("parent checkout /work/demo, which is read-only for you"));
    assert!(protected.contains("branch codex/agents/probe-01a1748c"));
    let unprotected = isolation_developer_note(&binding(false));
    assert!(unprotected.contains("which is still writable (the sandbox does not protect it)"));
}
