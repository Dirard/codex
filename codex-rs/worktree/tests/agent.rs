#![allow(clippy::expect_used)]
//! Host-side Git operations for agent worktrees: snapshot, metadata, checkpoint and removal.

use codex_worktree::AgentWorktreeBaseKind;
use codex_worktree::AgentWorktreeMetadata;
use codex_worktree::AgentWorktreeState;
use codex_worktree::AgentWorktreeStatus;
use codex_worktree::CheckpointOutcome;
use codex_worktree::CheckpointRequest;
use codex_worktree::CreateWorktree;
use codex_worktree::KeptReason;
use codex_worktree::ManagedWorktree;
use codex_worktree::MergeOutcome;
use codex_worktree::MergedChange;
use codex_worktree::RecoveredBranch;
use codex_worktree::RemoveOutcome;
use codex_worktree::RestoreFrom;
use codex_worktree::SweepPolicy;
use codex_worktree::SweepReport;
use codex_worktree::WorktreeManager;
use codex_worktree::WorktreeSettings;
use codex_worktree::agent_branch_for_thread;
use codex_worktree::agent_worktree_status;
use codex_worktree::branch_changes;
use codex_worktree::checkpoint;
use codex_worktree::merge_agent_branch;
use codex_worktree::read_agent_metadata;
use codex_worktree::remove_if_unchanged;
use codex_worktree::resolve_head_commit;
use codex_worktree::restore_agent_worktree;
use codex_worktree::snapshot_working_tree;
use codex_worktree::sweep_agent_worktrees;
use codex_worktree::write_agent_metadata;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::time::SystemTime;
use tempfile::TempDir;

const BRANCH: &str = "codex/agents/probe-01a1748c";
const AGENT_PATH: &str = "/root/probe";
const THREAD_ID: &str = "01a1748c-0000-7000-8000-000000000000";

struct Fixture {
    _temp_dir: TempDir,
    codex_home: PathBuf,
    repository: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("create temporary test directory");
        let temp_root = dunce::canonicalize(temp_dir.path()).expect("canonicalize temporary root");
        let codex_home = temp_root.join("codex-home");
        let repository = temp_root.join("project");
        fs::create_dir_all(&codex_home).expect("create Codex home");
        fs::create_dir_all(&repository).expect("create repository");
        git(&repository, &["init", "--quiet", "-b", "main"]);
        git(&repository, &["config", "user.name", "Codex Test"]);
        git(
            &repository,
            &["config", "user.email", "codex@example.invalid"],
        );
        fs::write(repository.join(".gitignore"), "target/\n*.log\n").expect("write ignore file");
        fs::write(repository.join("a.txt"), "one\n").expect("write tracked file");
        git(&repository, &["add", "."]);
        git(
            &repository,
            &["commit", "--quiet", "--no-gpg-sign", "-m", "initial"],
        );
        Self {
            _temp_dir: temp_dir,
            codex_home,
            repository,
        }
    }

    fn manager(&self) -> WorktreeManager {
        WorktreeManager::new(
            WorktreeSettings::for_cli(&self.codex_home, /*desktop*/ None)
                .expect("default worktree settings"),
        )
    }

    fn create(&self, base: &str) -> ManagedWorktree {
        self.manager()
            .create(&CreateWorktree {
                source_cwd: self.repository.clone(),
                base: Some(base.to_owned()),
            })
            .expect("create agent worktree")
    }

    fn managed_entries(&self) -> Vec<PathBuf> {
        match fs::read_dir(self.codex_home.join("worktrees")) {
            Ok(entries) => entries
                .map(|entry| entry.expect("read managed root entry").path())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}

fn git_fails(cwd: &Path, args: &[&str]) -> bool {
    !Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git")
        .status
        .success()
}

fn metadata(base_sha: &str, base_kind: AgentWorktreeBaseKind) -> AgentWorktreeMetadata {
    AgentWorktreeMetadata {
        thread_id: THREAD_ID.to_owned(),
        base_sha: base_sha.to_owned(),
        base_kind,
        branch: BRANCH.to_owned(),
        source_root: Some("/work/parent-checkout".to_owned()),
    }
}

fn checkpoint_worktree(root: &Path, metadata: &AgentWorktreeMetadata) -> CheckpointOutcome {
    checkpoint(CheckpointRequest {
        root,
        metadata,
        agent_path: AGENT_PATH,
    })
    .expect("checkpoint agent worktree")
}

#[test]
fn snapshot_contains_tracked_and_untracked_changes_without_touching_the_source() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    fs::write(repository.join("a.txt"), "one\ntwo\n").expect("modify tracked file");
    fs::write(repository.join("new.txt"), "new\n").expect("write untracked file");
    fs::create_dir_all(repository.join("target")).expect("create ignored directory");
    fs::write(repository.join("target/out.bin"), "build\n").expect("write ignored file");
    fs::write(repository.join("debug.log"), "log\n").expect("write ignored log");
    let status_before = git(repository, &["status", "--porcelain=v1"]);
    let head_before = git(repository, &["rev-parse", "HEAD"]);
    let refs_before = git(repository, &["for-each-ref"]);
    let index_before = fs::read(repository.join(".git/index")).expect("read source index");

    let snapshot = snapshot_working_tree(repository).expect("snapshot working tree");

    assert_eq!(
        git(repository, &["status", "--porcelain=v1"]),
        status_before
    );
    assert_eq!(git(repository, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(git(repository, &["for-each-ref"]), refs_before);
    assert_eq!(
        fs::read(repository.join(".git/index")).expect("read source index"),
        index_before
    );
    assert_eq!(
        git(repository, &["rev-parse", &format!("{snapshot}^")]),
        head_before
    );
    assert_eq!(
        git(repository, &["log", "-1", "--format=%an <%ae>", &snapshot]),
        "Codex Agent <codex-agent@localhost.invalid>"
    );
    let files = git(repository, &["ls-tree", "-r", "--name-only", &snapshot]);
    assert_eq!(
        files.lines().collect::<Vec<_>>(),
        [".gitignore", "a.txt", "new.txt"]
    );
    assert_eq!(
        git(repository, &["show", &format!("{snapshot}:a.txt")]),
        "one\ntwo"
    );

    let checkout = fixture.create(&snapshot);
    assert_eq!(
        fs::read_to_string(checkout.root.join("a.txt")).expect("read worktree file"),
        "one\ntwo\n"
    );
    assert!(checkout.root.join("new.txt").exists());
    assert!(!checkout.root.join("target").exists());
    assert_eq!(git(&checkout.root, &["status", "--porcelain=v1"]), "");
}

#[test]
fn head_base_excludes_uncommitted_changes() {
    let fixture = Fixture::new();
    fs::write(fixture.repository.join("a.txt"), "dirty\n").expect("modify tracked file");
    let head = resolve_head_commit(&fixture.repository).expect("resolve HEAD");
    assert_eq!(head, git(&fixture.repository, &["rev-parse", "HEAD"]));
    let checkout = fixture.create(&head);
    assert_eq!(
        fs::read_to_string(checkout.root.join("a.txt")).expect("read worktree file"),
        "one\n"
    );
}

#[test]
fn snapshot_with_unmerged_index_fails_before_any_checkout() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    git(repository, &["checkout", "--quiet", "-b", "other"]);
    fs::write(repository.join("a.txt"), "other\n").expect("write conflicting change");
    git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-am", "other"],
    );
    git(repository, &["checkout", "--quiet", "main"]);
    fs::write(repository.join("a.txt"), "main\n").expect("write conflicting change");
    git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-am", "main"],
    );
    assert!(git_fails(repository, &["merge", "--no-edit", "other"]));

    let error = snapshot_working_tree(repository).expect_err("unmerged index must fail");
    assert!(
        error.to_string().contains("unmerged"),
        "unexpected error: {error:#}"
    );
    assert_eq!(fixture.managed_entries(), Vec::<PathBuf>::new());
}

#[test]
fn snapshot_ignores_signing_config_and_hooks() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let marker = repository.join("hook-ran");
    let hooks = repository.join(".git/hooks");
    for hook in ["pre-commit", "commit-msg", "post-commit", "post-checkout"] {
        let script = hooks.join(hook);
        fs::write(
            &script,
            format!("#!/bin/sh\necho {hook} >> '{}'\n", marker.display()),
        )
        .expect("write hook");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
                .expect("make hook executable");
        }
    }
    git(repository, &["config", "commit.gpgSign", "true"]);
    git(repository, &["config", "gpg.program", "false"]);
    fs::write(repository.join("new.txt"), "new\n").expect("write untracked file");

    let snapshot = snapshot_working_tree(repository).expect("snapshot with signing configured");
    let checkout = fixture.create(&snapshot);
    write_agent_metadata(
        &checkout.root,
        &metadata(&snapshot, AgentWorktreeBaseKind::WorkingTree),
    )
    .expect("write metadata");
    fs::write(checkout.root.join("agent.txt"), "agent\n").expect("write agent change");
    let outcome = checkpoint_worktree(
        &checkout.root,
        &metadata(&snapshot, AgentWorktreeBaseKind::WorkingTree),
    );

    assert!(matches!(outcome, CheckpointOutcome::Committed { .. }));
    assert!(!marker.exists(), "hooks must not run");
}

#[test]
fn agent_metadata_round_trips_through_worktree_config() {
    let fixture = Fixture::new();
    let head = resolve_head_commit(&fixture.repository).expect("resolve HEAD");
    let checkout = fixture.create(&head);
    assert_eq!(
        read_agent_metadata(&checkout.root).expect("read empty metadata"),
        None
    );
    let expected = metadata(&head, AgentWorktreeBaseKind::Head);
    write_agent_metadata(&checkout.root, &expected).expect("write metadata");

    assert_eq!(
        read_agent_metadata(&checkout.root).expect("read metadata"),
        Some(expected)
    );
    // Shared repository configuration stays untouched.
    assert!(git_fails(
        &fixture.repository,
        &["config", "--get", "codex.agentThread"]
    ));
}

#[test]
fn checkpoints_create_the_branch_lazily_and_stack_commits() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let checkout = fixture.create(&base);
    let metadata = metadata(&base, AgentWorktreeBaseKind::WorkingTree);

    assert_eq!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Unchanged
    );
    assert!(git_fails(
        &fixture.repository,
        &["rev-parse", "--verify", &format!("refs/heads/{BRANCH}")]
    ));
    assert_eq!(
        branch_changes(&checkout.root, &metadata).expect("summary"),
        None
    );

    fs::write(checkout.root.join("a.txt"), "one\nagent\n").expect("modify tracked file");
    fs::write(checkout.root.join("new_file.txt"), "new\n").expect("write untracked file");
    let CheckpointOutcome::Committed {
        commit: first,
        branch,
        index_refresh_error,
    } = checkpoint_worktree(&checkout.root, &metadata)
    else {
        panic!("expected a first checkpoint commit");
    };
    assert_eq!(branch, BRANCH);
    assert_eq!(index_refresh_error, None);
    assert_eq!(
        git(&checkout.root, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{BRANCH}")
    );
    assert_eq!(git(&checkout.root, &["status", "--porcelain=v1"]), "");
    assert_eq!(git(&fixture.repository, &["rev-parse", BRANCH]), first);
    let message = git(&fixture.repository, &["log", "-1", "--format=%B", &first]);
    assert!(message.starts_with("codex agent /root/probe checkpoint"));
    assert!(message.contains(&format!("Codex-Thread-Id: {THREAD_ID}")));
    assert!(message.contains("Codex-Agent-Path: /root/probe"));
    // R4.9: the base travels with the branch.
    assert!(message.contains(&format!("Codex-Agent-Base: {base}")));
    assert!(message.contains("Codex-Agent-Base-Kind: working_tree"));
    let changes = branch_changes(&checkout.root, &metadata)
        .expect("summary")
        .expect("branch exists");
    assert_eq!(changes.shortstat, "2 files changed, 2 insertions(+)");
    assert_eq!(changes.total_files, 2);
    assert_eq!(
        changes
            .files
            .iter()
            .map(|file| format!("{} {}", file.status, file.path))
            .collect::<Vec<_>>(),
        ["M a.txt", "A new_file.txt"]
    );

    assert_eq!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Unchanged
    );
    fs::remove_file(checkout.root.join("new_file.txt")).expect("delete file");
    let CheckpointOutcome::Committed { commit: second, .. } =
        checkpoint_worktree(&checkout.root, &metadata)
    else {
        panic!("expected a second checkpoint commit");
    };
    assert_eq!(
        git(&fixture.repository, &["rev-parse", &format!("{second}^")]),
        first
    );
    assert_eq!(git(&fixture.repository, &["rev-parse", BRANCH]), second);
}

#[test]
fn checkpoint_never_moves_a_branch_moved_outside_the_worktree() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let checkout = fixture.create(&base);
    let metadata = metadata(&base, AgentWorktreeBaseKind::WorkingTree);
    write_agent_metadata(&checkout.root, &metadata).expect("write metadata");
    fs::write(checkout.root.join("a.txt"), "agent\n").expect("modify tracked file");
    let CheckpointOutcome::Committed { commit: first, .. } =
        checkpoint_worktree(&checkout.root, &metadata)
    else {
        panic!("expected a checkpoint commit");
    };

    // Another writer moves the branch to an unrelated commit while the agent keeps working.
    git(
        &fixture.repository,
        &["checkout", "--quiet", "-b", "unrelated", "main"],
    );
    fs::write(fixture.repository.join("b.txt"), "elsewhere\n").expect("write other change");
    git(&fixture.repository, &["add", "b.txt"]);
    git(
        &fixture.repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "elsewhere"],
    );
    let elsewhere = git(&fixture.repository, &["rev-parse", "HEAD"]);
    git(&checkout.root, &["checkout", "--quiet", "--detach", &first]);
    git(
        &fixture.repository,
        &["update-ref", &format!("refs/heads/{BRANCH}"), &elsewhere],
    );
    fs::write(checkout.root.join("a.txt"), "agent again\n").expect("modify tracked file");

    // The moved branch is not the worktree's any more: the work goes to a free name.
    let outcome = checkpoint(CheckpointRequest {
        root: &checkout.root,
        metadata: &metadata,
        agent_path: AGENT_PATH,
    })
    .expect("the checkpoint records the work on a free branch");
    let free = format!("{BRANCH}-2");
    assert!(matches!(
        outcome,
        CheckpointOutcome::Committed { ref branch, .. } if *branch == free
    ));
    assert_eq!(git(&fixture.repository, &["rev-parse", BRANCH]), elsewhere);
    assert_eq!(
        git(&checkout.root, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{free}")
    );
    assert_eq!(
        git(&fixture.repository, &["rev-parse", &format!("{free}^")]),
        first
    );
    assert_eq!(
        git(&fixture.repository, &["show", &format!("{free}:a.txt")]),
        "agent again"
    );
    assert_eq!(
        read_agent_metadata(&checkout.root)
            .expect("read metadata")
            .expect("metadata")
            .branch,
        free
    );
}

#[test]
fn a_branch_name_collision_moves_the_checkpoint_to_a_free_name() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let metadata = metadata(&base, AgentWorktreeBaseKind::WorkingTree);
    let first = fixture.create(&base);
    let second = fixture.create(&base);
    let clean = fixture.create(&base);
    for checkout in [&first, &second, &clean] {
        write_agent_metadata(&checkout.root, &metadata).expect("write metadata");
    }
    fs::write(first.root.join("first.txt"), "first\n").expect("write first change");
    fs::write(second.root.join("second.txt"), "second\n").expect("write second change");

    let CheckpointOutcome::Committed {
        commit: first_commit,
        ..
    } = checkpoint_worktree(&first.root, &metadata)
    else {
        panic!("expected the first checkpoint commit");
    };
    let outcome = checkpoint(CheckpointRequest {
        root: &second.root,
        metadata: &metadata,
        agent_path: AGENT_PATH,
    })
    .expect("a colliding checkpoint records its work on a free name");

    // The first worktree's branch is untouched; the second one lives on `<branch>-2`.
    let free = format!("{BRANCH}-2");
    assert!(matches!(
        outcome,
        CheckpointOutcome::Committed { ref branch, .. } if *branch == free
    ));
    assert_eq!(
        git(&fixture.repository, &["rev-parse", BRANCH]),
        first_commit
    );
    assert_eq!(
        git(&second.root, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{free}")
    );
    assert_eq!(
        git(
            &fixture.repository,
            &["show", &format!("{free}:second.txt")]
        ),
        "second"
    );
    assert!(git_fails(
        &fixture.repository,
        &["cat-file", "-e", &format!("{free}:first.txt")]
    ));
    let persisted = read_agent_metadata(&second.root)
        .expect("read metadata")
        .expect("metadata");
    assert_eq!(persisted.branch, free);
    let changes = branch_changes(&second.root, &persisted)
        .expect("summary")
        .expect("own branch");
    assert_eq!(
        changes
            .files
            .iter()
            .map(|file| format!("{} {}", file.status, file.path))
            .collect::<Vec<_>>(),
        ["A second.txt"]
    );
    // Under the stale name the second worktree reports no branch rather than the first's.
    assert_eq!(
        branch_changes(&second.root, &metadata).expect("summary"),
        None
    );
    // A later checkpoint under the stale name finds the same free branch again.
    fs::write(second.root.join("second.txt"), "second again\n").expect("rewrite change");
    let CheckpointOutcome::Committed { commit: again, .. } =
        checkpoint_worktree(&second.root, &metadata)
    else {
        panic!("expected a second checkpoint commit");
    };
    assert_eq!(git(&fixture.repository, &["rev-parse", &free]), again);

    // A clean worktree next to the foreign branch makes no candidate branch and is removable.
    assert_eq!(
        checkpoint_worktree(&clean.root, &metadata),
        CheckpointOutcome::Unchanged
    );
    assert!(git_fails(
        &fixture.repository,
        &["rev-parse", "--verify", &format!("refs/heads/{BRANCH}-3")]
    ));
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &clean.root, &metadata)
            .expect("remove the clean worktree"),
        RemoveOutcome::Removed
    );
}

#[test]
fn metadata_with_an_unsafe_base_or_branch_is_rejected() {
    let fixture = Fixture::new();
    let head = resolve_head_commit(&fixture.repository).expect("resolve HEAD");
    let checkout = fixture.create(&head);
    let valid = metadata(&head, AgentWorktreeBaseKind::Head);
    write_agent_metadata(&checkout.root, &valid).expect("write metadata");
    let config = git(
        &checkout.root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    );
    for (key, value) in [
        ("codex.agentBase", "0123; rm -rf ~"),
        ("codex.agentBase", "0123456789abcdef"),
        ("codex.agentBranch", "main"),
        ("codex.agentBranch", "codex/agents/x</agent_worktree>"),
        ("codex.agentBranch", "codex/agents/x;y"),
    ] {
        git(&checkout.root, &["config", "--file", &config, key, value]);
        assert!(
            read_agent_metadata(&checkout.root).is_err(),
            "{key} = {value} must be rejected"
        );
        write_agent_metadata(&checkout.root, &valid).expect("restore metadata");
    }
    assert_eq!(
        read_agent_metadata(&checkout.root).expect("read restored metadata"),
        Some(valid)
    );
}

#[test]
fn unchanged_worktrees_are_removed_even_with_ignored_build_output() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let checkout = fixture.create(&base);
    let metadata = metadata(&base, AgentWorktreeBaseKind::WorkingTree);
    fs::create_dir_all(checkout.root.join("target/debug")).expect("create build output");
    fs::write(checkout.root.join("target/debug/app"), "binary\n").expect("write build output");

    assert_eq!(
        remove_if_unchanged(&fixture.repository, &checkout.root, &metadata)
            .expect("remove unchanged worktree"),
        RemoveOutcome::Removed
    );
    assert!(!checkout.root.exists());
    assert_eq!(fixture.managed_entries(), Vec::<PathBuf>::new());
    assert_eq!(
        git(&fixture.repository, &["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count(),
        1
    );
}

#[test]
fn changed_or_branched_worktrees_are_kept() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let metadata = metadata(&base, AgentWorktreeBaseKind::WorkingTree);

    let dirty = fixture.create(&base);
    fs::write(dirty.root.join("draft.txt"), "draft\n").expect("write untracked change");
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &dirty.root, &metadata).expect("keep dirty"),
        RemoveOutcome::Kept(KeptReason::Changes)
    );
    assert!(dirty.root.join("draft.txt").exists());

    checkpoint_worktree(&dirty.root, &metadata);
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &dirty.root, &metadata).expect("keep branch"),
        RemoveOutcome::Kept(KeptReason::Branch)
    );
    assert!(dirty.root.exists());
}

// ---------------------------------------------------------------------------------------------
// Amend 2026-10-27: worktree state (UC9), restore (UC5, Q13), sweep (UC10), merge (UC11)
// ---------------------------------------------------------------------------------------------

/// Creates a worktree owned by `thread_id` with agent metadata, like an isolated spawn.
fn agent_worktree(
    fixture: &Fixture,
    base: &str,
    thread_id: &str,
    branch: &str,
) -> (ManagedWorktree, AgentWorktreeMetadata) {
    let checkout = fixture.create(base);
    let metadata = AgentWorktreeMetadata {
        thread_id: thread_id.to_owned(),
        base_sha: base.to_owned(),
        base_kind: AgentWorktreeBaseKind::WorkingTree,
        branch: branch.to_owned(),
        source_root: fixture.repository.to_str().map(str::to_owned),
    };
    fixture
        .manager()
        .bind_thread(&checkout.root, thread_id)
        .expect("bind owner");
    write_agent_metadata(&checkout.root, &metadata).expect("write agent metadata");
    (checkout, metadata)
}

/// Moves the last activity of a worktree `hours` into the past (R10.4).
fn age(root: &Path, hours: u64) {
    let time = SystemTime::now() - Duration::from_secs(hours * 3600);
    let git_dir = PathBuf::from(git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    ));
    let mut paths = vec![root.to_path_buf()];
    for file in [
        "HEAD",
        "index",
        "config.worktree",
        "codex-thread.json",
        "logs/HEAD",
    ] {
        paths.push(git_dir.join(file));
    }
    for path in paths.into_iter().filter(|path| path.exists()) {
        fs::File::open(&path)
            .and_then(|file| file.set_modified(time))
            .expect("set modification time");
    }
}

fn sweep(fixture: &Fixture, policy: SweepPolicy, in_use: &[&Path]) -> SweepReport {
    let held = in_use
        .iter()
        .map(|path| path.to_path_buf())
        .collect::<Vec<_>>();
    sweep_agent_worktrees(
        &fixture.manager(),
        &fixture.repository,
        &policy,
        &|root| held.iter().any(|held| held == root),
        &|_| false,
        SystemTime::now(),
    )
    .expect("sweep agent worktrees")
}

const DAY_POLICY: SweepPolicy = SweepPolicy {
    keep_count: 15,
    max_idle: Duration::from_secs(24 * 3600),
    min_idle: Duration::from_secs(3600),
};

#[test]
fn worktree_state_tracks_changes_branch_and_removal() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let status = |metadata: &AgentWorktreeMetadata| {
        agent_worktree_status(&checkout.root, metadata).expect("worktree status")
    };

    assert_eq!(
        status(&metadata),
        AgentWorktreeStatus {
            state: AgentWorktreeState::Clean,
            branch: None
        }
    );
    fs::create_dir_all(checkout.root.join("target")).expect("create build output");
    fs::write(checkout.root.join("target/out.bin"), "build\n").expect("write build output");
    assert_eq!(status(&metadata).state, AgentWorktreeState::Clean);

    fs::write(checkout.root.join("a.txt"), "one\nagent\n").expect("modify tracked file");
    assert_eq!(
        status(&metadata),
        AgentWorktreeStatus {
            state: AgentWorktreeState::Changed,
            branch: None
        }
    );
    checkpoint_worktree(&checkout.root, &metadata);
    assert_eq!(
        status(&metadata),
        AgentWorktreeStatus {
            state: AgentWorktreeState::Changed,
            branch: Some(BRANCH.to_owned())
        }
    );

    fs::remove_dir_all(&checkout.root).expect("remove the checkout");
    assert_eq!(
        status(&metadata),
        AgentWorktreeStatus {
            state: AgentWorktreeState::Removed,
            branch: None
        }
    );
}

#[test]
fn a_removed_worktree_is_restored_from_its_branch_or_base() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let manager = fixture.manager();

    // With a branch: the checkout comes back at its old path, attached to the branch.
    let (branched, branched_metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(branched.root.join("agent.txt"), "agent\n").expect("write agent file");
    checkpoint_worktree(&branched.root, &branched_metadata);
    git(
        &fixture.repository,
        &[
            "worktree",
            "remove",
            "--force",
            branched.root.to_str().expect("UTF-8 path"),
        ],
    );
    assert!(!branched.root.exists());
    let recovered = agent_branch_for_thread(&fixture.repository, BRANCH, THREAD_ID)
        .expect("find branch")
        .expect("own branch");
    assert_eq!(
        recovered,
        RecoveredBranch {
            branch: BRANCH.to_owned(),
            tip: git(&fixture.repository, &["rev-parse", BRANCH]),
        }
    );
    restore_agent_worktree(
        &manager,
        &fixture.repository,
        &branched.root,
        &branched_metadata,
        &RestoreFrom::Branch {
            name: recovered.branch,
            tip: recovered.tip,
        },
    )
    .expect("restore from the branch");
    assert_eq!(
        fs::read_to_string(branched.root.join("agent.txt")).expect("read restored file"),
        "agent\n"
    );
    assert_eq!(
        git(&branched.root, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{BRANCH}")
    );
    assert_eq!(git(&branched.root, &["status", "--porcelain=v1"]), "");
    assert_eq!(
        manager.owner(&branched.root).expect("owner"),
        Some(THREAD_ID.to_owned())
    );
    assert_eq!(
        read_agent_metadata(&branched.root).expect("metadata"),
        Some(branched_metadata)
    );

    // Without a branch: the checkout comes back detached at its base.
    let clean_thread = "01a1748c-0000-7000-8000-0000000000c1";
    let clean_branch = "codex/agents/clean-00000000000000c1";
    let (clean, clean_metadata) = agent_worktree(&fixture, &base, clean_thread, clean_branch);
    fs::remove_dir_all(&clean.root).expect("remove the checkout");
    assert_eq!(
        agent_branch_for_thread(&fixture.repository, clean_branch, clean_thread)
            .expect("search branches"),
        None
    );
    restore_agent_worktree(
        &manager,
        &fixture.repository,
        &clean.root,
        &clean_metadata,
        &RestoreFrom::Base,
    )
    .expect("restore from the base");
    assert_eq!(git(&clean.root, &["rev-parse", "HEAD"]), base);
    assert!(git_fails(&clean.root, &["symbolic-ref", "-q", "HEAD"]));

    // An occupied path or a missing base fails and leaves no worktree behind.
    let occupied = restore_agent_worktree(
        &manager,
        &fixture.repository,
        &clean.root,
        &clean_metadata,
        &RestoreFrom::Base,
    )
    .expect_err("occupied path");
    assert!(format!("{occupied:#}").contains("occupied"), "{occupied:#}");
    fs::remove_dir_all(&clean.root).expect("remove the checkout");
    let missing_base = AgentWorktreeMetadata {
        base_sha: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        ..clean_metadata
    };
    let missing = restore_agent_worktree(
        &manager,
        &fixture.repository,
        &clean.root,
        &missing_base,
        &RestoreFrom::Base,
    )
    .expect_err("missing base");
    assert!(
        format!("{missing:#}").contains("no longer in the repository"),
        "{missing:#}"
    );
    assert!(!clean.root.exists());
}

#[test]
fn agent_branch_for_thread_finds_free_names_and_old_checkpoints() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    // Another worktree holds the recorded name, so the checkpoint moves to `-2`.
    let other_thread = "01a1748c-0000-7000-8000-0000000000ff";
    let (other, other_metadata) = agent_worktree(&fixture, &base, other_thread, BRANCH);
    fs::write(other.root.join("other.txt"), "other\n").expect("write other file");
    checkpoint_worktree(&other.root, &other_metadata);
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("mine.txt"), "mine\n").expect("write own file");
    let CheckpointOutcome::Committed { branch, .. } =
        checkpoint_worktree(&checkout.root, &metadata)
    else {
        panic!("expected a checkpoint");
    };
    assert_eq!(branch, format!("{BRANCH}-2"));
    let recovered = agent_branch_for_thread(&fixture.repository, BRANCH, THREAD_ID)
        .expect("find branch")
        .expect("own branch");
    assert_eq!(recovered.branch, format!("{BRANCH}-2"));

    // A checkpoint written before R4.9 has no base trailers; the host wrote it, so it counts.
    let tree = git(
        &fixture.repository,
        &["rev-parse", &format!("{base}^{{tree}}")],
    );
    let checkpoint_commit = |author: &str, email: &str, thread: &str| {
        git(
            &fixture.repository,
            &[
                "-c",
                &format!("user.name={author}"),
                "-c",
                &format!("user.email={email}"),
                "commit-tree",
                "-p",
                &base,
                "-m",
                &format!(
                    "codex agent /root/old checkpoint\n\nCodex-Agent-Path: /root/old\nCodex-Thread-Id: {thread}"
                ),
                &tree,
            ],
        )
    };
    let old_thread = format!("{other_thread}0");
    let old = checkpoint_commit(
        codex_worktree::AGENT_COMMIT_AUTHOR_NAME,
        codex_worktree::AGENT_COMMIT_AUTHOR_EMAIL,
        &old_thread,
    );
    let old_branch = "codex/agents/old-0000000000000000";
    git(&fixture.repository, &["branch", old_branch, &old]);
    assert_eq!(
        agent_branch_for_thread(&fixture.repository, old_branch, &old_thread)
            .expect("find old branch"),
        Some(RecoveredBranch {
            branch: old_branch.to_owned(),
            tip: old,
        })
    );

    // P3-7: a commit that only looks like a checkpoint (another author) is not the host's.
    let forged_thread = format!("{other_thread}1");
    let forged = checkpoint_commit("Someone", "someone@example.invalid", &forged_thread);
    let forged_branch = "codex/agents/forged-0000000000000000";
    git(&fixture.repository, &["branch", forged_branch, &forged]);
    assert_eq!(
        agent_branch_for_thread(&fixture.repository, forged_branch, &forged_thread)
            .expect("search the forged branch"),
        None
    );
}

#[test]
fn sweep_removes_only_unchanged_idle_agent_worktrees() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let thread = |suffix: &str| format!("01a1748c-0000-7000-8000-0000000000{suffix}");
    let branch = |suffix: &str| format!("codex/agents/sweep-00000000000000{suffix}");

    let (changed, changed_metadata) = agent_worktree(&fixture, &base, &thread("a1"), &branch("a1"));
    fs::write(changed.root.join("work.txt"), "work\n").expect("write change");
    checkpoint_worktree(&changed.root, &changed_metadata);
    let (dirty, _) = agent_worktree(&fixture, &base, &thread("a2"), &branch("a2"));
    fs::write(dirty.root.join("draft.txt"), "draft\n").expect("write uncommitted change");
    let (stale, _) = agent_worktree(&fixture, &base, &thread("a3"), &branch("a3"));
    fs::create_dir_all(stale.root.join("target")).expect("create build output");
    fs::write(stale.root.join("target/out.bin"), "build\n").expect("write build output");
    let (fresh, _) = agent_worktree(&fixture, &base, &thread("a4"), &branch("a4"));
    let (held, _) = agent_worktree(&fixture, &base, &thread("a5"), &branch("a5"));
    let desktop = fixture.create(&base);
    for root in [
        &changed.root,
        &dirty.root,
        &stale.root,
        &held.root,
        &desktop.root,
    ] {
        age(root, 48);
    }
    let branches_before = git(&fixture.repository, &["for-each-ref", "refs/heads"]);

    let report = sweep(&fixture, DAY_POLICY, &[&held.root]);

    assert_eq!(report.removed, vec![stale.root.clone()]);
    assert_eq!(report.failed, Vec::new());
    assert!(!stale.root.exists());
    for kept in [
        &changed.root,
        &dirty.root,
        &fresh.root,
        &held.root,
        &desktop.root,
    ] {
        assert!(kept.exists(), "{} must stay", kept.display());
    }
    assert_eq!(
        git(&fixture.repository, &["for-each-ref", "refs/heads"]),
        branches_before
    );
}

#[test]
fn sweep_keeps_the_most_recent_unchanged_worktrees() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let mut roots = Vec::new();
    for (index, hours) in [3_u64, 5, 7].into_iter().enumerate() {
        let (checkout, _) = agent_worktree(
            &fixture,
            &base,
            &format!("01a1748c-0000-7000-8000-00000000000{index}"),
            &format!("codex/agents/keep-000000000000000{index}"),
        );
        age(&checkout.root, hours);
        roots.push(checkout.root);
    }
    let (young, _) = agent_worktree(
        &fixture,
        &base,
        "01a1748c-0000-7000-8000-0000000000b0",
        "codex/agents/keep-00000000000000b0",
    );
    let policy = SweepPolicy {
        keep_count: 1,
        ..DAY_POLICY
    };

    let report = sweep(&fixture, policy, &[]);

    // `keep_count = 1` keeps the most recent unchanged worktree (the young one); the older
    // ones beyond the count are removed although none of them is a day old.
    let mut removed = report.removed;
    removed.sort();
    let mut expected = roots.clone();
    expected.sort();
    assert_eq!(removed, expected);
    assert!(young.root.exists());

    // A young worktree beyond the count stays too.
    let (second_young, _) = agent_worktree(
        &fixture,
        &base,
        "01a1748c-0000-7000-8000-0000000000b1",
        "codex/agents/keep-00000000000000b1",
    );
    let report = sweep(
        &fixture,
        SweepPolicy {
            keep_count: 0,
            ..DAY_POLICY
        },
        &[],
    );
    assert_eq!(report.removed, Vec::<PathBuf>::new());
    assert!(young.root.exists() && second_young.root.exists());
}

#[test]
fn merge_agent_branch_merges_over_parent_edits() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    fs::write(repository.join("geometry.py"), "a\nb\nc\nd\ne\nf\n").expect("write file");
    git(repository, &["add", "geometry.py"]);
    git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "geometry"],
    );
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("geometry.py"), "a\nb\nc\nd\ne\nF\n").expect("agent edit");
    fs::write(checkout.root.join("added.txt"), "added\n").expect("agent file");
    checkpoint_worktree(&checkout.root, &metadata);
    // The parent edits the start of the same file after the spawn.
    fs::write(repository.join("geometry.py"), "A\nb\nc\nd\ne\nf\n").expect("parent edit");
    let status_before = git(repository, &["status", "--porcelain=v1"]);
    let index_before = fs::read(repository.join(".git/index")).expect("read index");
    let head_before = git(repository, &["rev-parse", "HEAD"]);

    let outcome = merge_agent_branch(repository, &checkout.root, &metadata).expect("merge");

    let MergeOutcome::Clean(merge) = outcome else {
        panic!("expected a clean merge, got {outcome:?}");
    };
    assert_eq!(merge.tip, git(repository, &["rev-parse", BRANCH]));
    assert!(!merge.pending_worktree_changes);
    assert_eq!(
        merge.changes,
        vec![
            MergedChange {
                path: "added.txt".to_owned(),
                old: None,
                new: Some("added\n".to_owned()),
            },
            MergedChange {
                path: "geometry.py".to_owned(),
                old: Some("A\nb\nc\nd\ne\nf\n".to_owned()),
                new: Some("A\nb\nc\nd\ne\nF\n".to_owned()),
            },
        ]
    );
    // R11.5: the size of the change is reported like `git diff --shortstat`.
    assert_eq!(
        merge.shortstat,
        "2 files changed, 2 insertions(+), 1 deletion(-)"
    );
    assert_eq!(
        git(repository, &["status", "--porcelain=v1"]),
        status_before
    );
    assert_eq!(
        fs::read(repository.join(".git/index")).expect("read index"),
        index_before
    );
    assert_eq!(git(repository, &["rev-parse", "HEAD"]), head_before);
    assert!(!repository.join("added.txt").exists());
}

#[test]
fn merge_agent_branch_reports_conflicts_and_unrepresentable_changes() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let base = snapshot_working_tree(repository).expect("snapshot");

    // No branch yet.
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("a.txt"), "agent\n").expect("agent edit");
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::NoBranch {
            pending_worktree_changes: true
        }
    );

    // The same line on both sides conflicts.
    checkpoint_worktree(&checkout.root, &metadata);
    fs::write(repository.join("a.txt"), "parent\n").expect("parent edit");
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::Conflicts(vec!["a.txt".to_owned()])
    );

    // After the parent takes the agent's line the branch is already applied.
    fs::write(repository.join("a.txt"), "agent\n").expect("parent takes the change");
    assert!(matches!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::AlreadyApplied { .. }
    ));

    // An added file that exists in the parent as an ignored file conflicts.
    fs::write(checkout.root.join("debug.log"), "agent log\n").expect("agent ignored-name file");
    git(&checkout.root, &["add", "--force", "debug.log"]);
    checkpoint_worktree(&checkout.root, &metadata);
    fs::write(repository.join("debug.log"), "parent log\n").expect("parent ignored file");
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::Conflicts(vec!["debug.log".to_owned()])
    );
    fs::remove_file(repository.join("debug.log")).expect("remove parent log");

    // Binary content and executable files cannot be written through a text patch.
    let other_thread = "01a1748c-0000-7000-8000-0000000000e1";
    let other_branch = "codex/agents/binary-00000000000000e1";
    let (binary, binary_metadata) = agent_worktree(&fixture, &base, other_thread, other_branch);
    fs::write(binary.root.join("blob.bin"), [0_u8, 1, 2, 3]).expect("write binary");
    checkpoint_worktree(&binary.root, &binary_metadata);
    assert_eq!(
        merge_agent_branch(repository, &binary.root, &binary_metadata).expect("merge"),
        MergeOutcome::Unrepresentable {
            path: "blob.bin".to_owned(),
            reason: "binary or non-UTF-8 content",
        }
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_thread = "01a1748c-0000-7000-8000-0000000000e2";
        let script_branch = "codex/agents/script-00000000000000e2";
        let (script, script_metadata) =
            agent_worktree(&fixture, &base, script_thread, script_branch);
        let path = script.root.join("run.sh");
        fs::write(&path, "#!/bin/sh\n").expect("write script");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("make executable");
        checkpoint_worktree(&script.root, &script_metadata);
        assert_eq!(
            merge_agent_branch(repository, &script.root, &script_metadata).expect("merge"),
            MergeOutcome::Unrepresentable {
                path: "run.sh".to_owned(),
                reason: "an executable file",
            }
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Review 2026-10-27 of the finished feature (docs/fork/reviews/2026-10-27-worktree-finish-review.md)
// ---------------------------------------------------------------------------------------------

/// The administration directory of a linked worktree.
fn git_dir(root: &Path) -> PathBuf {
    PathBuf::from(git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    ))
}

fn registered_worktrees(repository: &Path) -> Vec<String> {
    git(repository, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(str::to_owned)
        .collect()
}

/// Points `branch` at a new checkpoint commit of `tree` on top of `parent`, written with the
/// host's identity as an agent with full access could forge it.
fn commit_tree_on(repository: &Path, branch: &str, tree: &str, parent: &str) -> String {
    let commit = git(
        repository,
        &[
            "-c",
            &format!("user.name={}", codex_worktree::AGENT_COMMIT_AUTHOR_NAME),
            "-c",
            &format!("user.email={}", codex_worktree::AGENT_COMMIT_AUTHOR_EMAIL),
            "commit-tree",
            "-p",
            parent,
            "-m",
            &format!("codex agent {AGENT_PATH} checkpoint\n\nCodex-Thread-Id: {THREAD_ID}"),
            tree,
        ],
    );
    git(repository, &["branch", "--force", branch, &commit]);
    commit
}

/// One `git mktree` input line.
fn tree_entry(mode: &str, kind: &str, oid: &str, name: &str) -> String {
    format!("{mode} {kind} {oid}\t{name}\n")
}

fn mktree(repository: &Path, entries: &str) -> String {
    use std::io::Write as _;
    let mut child = Command::new("git")
        .current_dir(repository)
        .arg("mktree")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("start git mktree");
    child
        .stdin
        .take()
        .expect("mktree stdin")
        .write_all(entries.as_bytes())
        .expect("write mktree input");
    let output = child.wait_with_output().expect("run git mktree");
    assert!(output.status.success(), "git mktree failed");
    String::from_utf8(output.stdout)
        .expect("UTF-8 tree id")
        .trim()
        .to_owned()
}

/// P2-1: a sweep never removes a worktree that another process holds, however idle it looks.
#[cfg(target_os = "linux")]
#[test]
fn a_sweep_keeps_a_worktree_another_process_holds() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (held, _) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    age(&held.root, 48);
    let lock = git_dir(&held.root).join("codex-agent.lock");
    let ready = fixture.repository.join("holder-ready");
    // Another Codex process holds the worktree for its live agent: a shared lock on the
    // worktree's lock file, kept while that process lives. `--no-fork` and `exec` keep the
    // lock in the one process that is killed below.
    let mut holder = Command::new("flock")
        .arg("--shared")
        .arg("--no-fork")
        .arg(&lock)
        .arg("sh")
        .arg("-c")
        .arg(format!("touch '{}' && exec sleep 60", ready.display()))
        .spawn()
        .expect("start the holding process");
    while !ready.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }

    let report = sweep(&fixture, DAY_POLICY, &[]);
    assert_eq!(report.removed, Vec::<PathBuf>::new());
    assert!(held.root.exists(), "a held worktree must stay");

    holder.kill().expect("stop the holding process");
    holder.wait().expect("reap the holding process");
    age(&held.root, 48);
    let report = sweep(&fixture, DAY_POLICY, &[]);
    assert_eq!(report.removed, vec![held.root]);
}

/// P3-4: the sweep of a session started inside an agent worktree keeps that worktree.
#[test]
fn a_sweep_keeps_the_worktree_the_session_runs_in() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (current, _) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    age(&current.root, 48);

    let report = sweep_agent_worktrees(
        &fixture.manager(),
        &current.root,
        &DAY_POLICY,
        &|_| false,
        &|_| false,
        SystemTime::now(),
    )
    .expect("sweep from inside the worktree");

    assert_eq!(report.removed, Vec::<PathBuf>::new());
    assert!(current.root.exists());
}

/// P3-3: restoring or dropping an agent worktree removes only its own stale registration.
#[test]
fn restore_and_removal_keep_other_missing_registrations() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    // A user's worktree on a drive that is not mounted right now.
    let unmounted = fixture
        .repository
        .parent()
        .expect("temp root")
        .join("usb-wt");
    git(
        &fixture.repository,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            unmounted.to_str().expect("UTF-8 path"),
        ],
    );
    fs::remove_dir_all(&unmounted).expect("unmount the drive");

    let (branched, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(branched.root.join("agent.txt"), "agent\n").expect("write agent file");
    let CheckpointOutcome::Committed { commit, .. } =
        checkpoint_worktree(&branched.root, &metadata)
    else {
        panic!("expected a checkpoint");
    };
    fs::remove_dir_all(&branched.root).expect("remove the agent checkout");
    restore_agent_worktree(
        &fixture.manager(),
        &fixture.repository,
        &branched.root,
        &metadata,
        &RestoreFrom::Branch {
            name: BRANCH.to_owned(),
            tip: commit,
        },
    )
    .expect("restore from the branch");
    assert!(
        registered_worktrees(&fixture.repository).contains(&unmounted.display().to_string()),
        "the restore pruned the user's worktree"
    );

    let other_thread = "01a1748c-0000-7000-8000-0000000000d1";
    let (clean, clean_metadata) = agent_worktree(
        &fixture,
        &base,
        other_thread,
        "codex/agents/clean-00000000000000d1",
    );
    fs::remove_dir_all(&clean.root).expect("remove the clean checkout");
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &clean.root, &clean_metadata)
            .expect("drop the missing worktree"),
        RemoveOutcome::AlreadyMissing
    );
    let registered = registered_worktrees(&fixture.repository);
    assert!(
        registered.contains(&unmounted.display().to_string()),
        "the removal pruned the user's worktree"
    );
    assert!(!registered.contains(&clean.root.display().to_string()));
}

/// P3-2: a restore never attaches a branch that another checkout has checked out.
#[test]
fn restore_refuses_a_branch_checked_out_elsewhere() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (branched, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(branched.root.join("agent.txt"), "agent\n").expect("write agent file");
    let CheckpointOutcome::Committed { commit, .. } =
        checkpoint_worktree(&branched.root, &metadata)
    else {
        panic!("expected a checkpoint");
    };
    git(
        &fixture.repository,
        &[
            "worktree",
            "remove",
            "--force",
            branched.root.to_str().expect("UTF-8 path"),
        ],
    );
    // The user looks at the result in the main checkout.
    git(&fixture.repository, &["switch", "--quiet", BRANCH]);

    let error = restore_agent_worktree(
        &fixture.manager(),
        &fixture.repository,
        &branched.root,
        &metadata,
        &RestoreFrom::Branch {
            name: BRANCH.to_owned(),
            tip: commit,
        },
    )
    .expect_err("the branch is checked out in the main checkout");
    assert!(format!("{error:#}").contains("checked out"), "{error:#}");
    assert!(!branched.root.exists());
    assert_eq!(
        git(&fixture.repository, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{BRANCH}")
    );
}

/// P3-7: a restore from a branch requires the recorded base to be an ancestor of its tip.
#[test]
fn restore_requires_the_base_to_be_an_ancestor_of_the_branch() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (branched, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(branched.root.join("agent.txt"), "agent\n").expect("write agent file");
    let CheckpointOutcome::Committed { commit, .. } =
        checkpoint_worktree(&branched.root, &metadata)
    else {
        panic!("expected a checkpoint");
    };
    fs::remove_dir_all(&branched.root).expect("remove the agent checkout");
    // A commit beside the branch, not below it.
    fs::write(fixture.repository.join("a.txt"), "elsewhere\n").expect("edit");
    let elsewhere = snapshot_working_tree(&fixture.repository).expect("unrelated snapshot");

    let error = restore_agent_worktree(
        &fixture.manager(),
        &fixture.repository,
        &branched.root,
        &AgentWorktreeMetadata {
            base_sha: elsewhere,
            ..metadata
        },
        &RestoreFrom::Branch {
            name: BRANCH.to_owned(),
            tip: commit,
        },
    )
    .expect_err("a base beside the branch");
    assert!(format!("{error:#}").contains("ancestor"), "{error:#}");
    assert!(!branched.root.exists());
}

/// P2-3, R11.4: an added file below an ignored file or an ignored symlink of the parent
/// conflicts, so nothing is written halfway.
#[cfg(unix)]
#[test]
fn merge_conflicts_with_an_ignored_file_or_symlink_on_an_added_path() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    fs::write(repository.join(".gitignore"), "target/\n*.log\ngen\n").expect("ignore gen");
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::create_dir_all(checkout.root.join("gen")).expect("agent directory");
    fs::write(checkout.root.join("gen/x.txt"), "agent\n").expect("agent file");
    fs::write(checkout.root.join("0-first.txt"), "first\n").expect("agent file");
    git(&checkout.root, &["add", "--force", "gen/x.txt"]);
    checkpoint_worktree(&checkout.root, &metadata);

    // An ignored file where the agent added a directory.
    fs::write(repository.join("gen"), "generated\n").expect("parent ignored file");
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::Conflicts(vec!["gen/x.txt".to_owned()])
    );

    // An ignored symlink to a directory outside the checkout.
    fs::remove_file(repository.join("gen")).expect("remove the ignored file");
    let outside = repository.parent().expect("temp root").join("outside");
    fs::create_dir_all(&outside).expect("outside directory");
    std::os::unix::fs::symlink(&outside, repository.join("gen")).expect("ignored symlink");
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::Conflicts(vec!["gen/x.txt".to_owned()])
    );
}

/// P3-1: paths of the merge result are validated: no `..`, `.git` or nested Codex
/// configuration reaches the patch, whatever tree the branch holds.
#[test]
fn merge_rejects_crafted_and_protected_paths() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let base = snapshot_working_tree(repository).expect("snapshot");
    let base_tree = git(repository, &["rev-parse", &format!("{base}^{{tree}}")]);
    let blob = git(repository, &["hash-object", "-w", "--stdin"]);
    let base_entries = git(repository, &["ls-tree", &base_tree]);
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    // Without its worktree the branch is found by the thread's checkpoint trailer, so any
    // tree written to it reaches the merge.
    fs::remove_dir_all(&checkout.root).expect("remove the agent checkout");
    let with_entry = |entry: String| {
        let mut entries = base_entries
            .lines()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        entries.push_str(&entry);
        mktree(repository, &entries)
    };
    let nested = |name: &str| mktree(repository, &tree_entry("100644", "blob", &blob, name));
    for (name, inner) in [
        ("..", "escape.txt"),
        (".git", "config.worktree"),
        (".GIT", "hooks.txt"),
    ] {
        let tree = with_entry(tree_entry("040000", "tree", &nested(inner), name));
        commit_tree_on(repository, BRANCH, &tree, &base);
        let outcome = merge_agent_branch(repository, &checkout.root, &metadata).expect("merge");
        assert!(
            matches!(outcome, MergeOutcome::Unrepresentable { .. }),
            "{name}/{inner}: {outcome:?}"
        );
    }
    let sub_codex = mktree(
        repository,
        &tree_entry("040000", "tree", &nested("config.toml"), ".codex"),
    );
    let tree = with_entry(tree_entry("040000", "tree", &sub_codex, "sub"));
    commit_tree_on(repository, BRANCH, &tree, &base);
    let outcome = merge_agent_branch(repository, &checkout.root, &metadata).expect("merge");
    assert!(
        matches!(outcome, MergeOutcome::Unrepresentable { .. }),
        "sub/.codex/config.toml: {outcome:?}"
    );
}

/// P2-1: a hold of this process keeps the worktree from every removal until it is dropped.
#[test]
fn a_held_worktree_is_neither_swept_nor_removed() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (held, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    age(&held.root, 48);
    let hold = codex_worktree::hold_agent_worktree(&held.root).expect("hold the worktree");

    assert_eq!(
        sweep(&fixture, DAY_POLICY, &[]).removed,
        Vec::<PathBuf>::new()
    );
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &held.root, &metadata).expect("keep"),
        RemoveOutcome::Kept(KeptReason::InUse)
    );
    assert!(held.root.exists());

    drop(hold);
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &held.root, &metadata).expect("remove"),
        RemoveOutcome::Removed
    );
}

/// P3-7: the merge refuses a recorded base that is not an ancestor of the branch.
#[test]
fn merge_refuses_a_base_beside_the_branch() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("agent.txt"), "agent\n").expect("agent file");
    checkpoint_worktree(&checkout.root, &metadata);
    fs::write(repository.join("a.txt"), "elsewhere\n").expect("edit");
    let elsewhere = snapshot_working_tree(repository).expect("unrelated snapshot");

    let outcome = merge_agent_branch(
        repository,
        &checkout.root,
        &AgentWorktreeMetadata {
            base_sha: elsewhere,
            ..metadata
        },
    )
    .expect("merge");
    assert_eq!(
        outcome,
        MergeOutcome::BaseNotAncestor {
            tip: git(repository, &["rev-parse", BRANCH]),
        }
    );
}

/// P3-5: a worktree whose work is all on its own branch is removed after `max_idle` when it
/// can be restored from the branch; the branch stays.
#[test]
fn sweep_removes_restorable_worktrees_parked_on_their_branch() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let thread = |suffix: &str| format!("01a1748c-0000-7000-8000-0000000000{suffix}");
    let branch = |suffix: &str| format!("codex/agents/parked-00000000000000{suffix}");
    let mut parked = Vec::new();
    for suffix in ["f1", "f2", "f3"] {
        let (checkout, metadata) =
            agent_worktree(&fixture, &base, &thread(suffix), &branch(suffix));
        fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
        checkpoint_worktree(&checkout.root, &metadata);
        parked.push(checkout.root);
    }
    // One gained uncommitted work after its last checkpoint; one is recent.
    fs::write(parked[1].join("draft.txt"), "draft\n").expect("uncommitted change");
    age(&parked[0], 48);
    age(&parked[1], 48);
    age(&parked[2], 2);
    let sweep_with = |restorable: bool| {
        sweep_agent_worktrees(
            &fixture.manager(),
            &fixture.repository,
            &DAY_POLICY,
            &|_| false,
            &|_| restorable,
            SystemTime::now(),
        )
        .expect("sweep")
    };

    assert_eq!(sweep_with(false).removed, Vec::<PathBuf>::new());
    assert_eq!(sweep_with(true).removed, vec![parked[0].clone()]);
    assert!(parked[1].exists() && parked[2].exists());
    assert_eq!(
        git(
            &fixture.repository,
            &["rev-parse", "--verify", &branch("f1")]
        )
        .len(),
        40
    );
}

/// R5.6, R9.3: records round-trip, reject what the host could not have written, and are
/// pruned only when they can restore nothing any more.
#[test]
fn agent_records_round_trip_and_are_pruned_when_useless() {
    let fixture = Fixture::new();
    let records = fixture.codex_home.join("agent-worktrees");
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let record =
        codex_worktree::AgentWorktreeRecord::new(&checkout.root, &fixture.repository, &metadata);
    codex_worktree::write_agent_record(&records, &record).expect("write record");
    assert_eq!(
        codex_worktree::read_agent_record(&records, THREAD_ID).expect("read record"),
        Some(record.clone())
    );
    assert_eq!(
        codex_worktree::read_agent_record(&records, "01a1748c-0000-7000-8000-00000000ffff")
            .expect("read a missing record"),
        None
    );
    assert!(codex_worktree::read_agent_record(&records, "../escape").is_err());
    let forged = codex_worktree::AgentWorktreeRecord {
        thread_id: "01a1748c-0000-7000-8000-0000000000ee".to_owned(),
        branch: "main".to_owned(),
        ..record
    };
    codex_worktree::write_agent_record(&records, &forged).expect("write a forged record");
    assert!(codex_worktree::read_agent_record(&records, &forged.thread_id).is_err());
    fs::remove_file(records.join(format!("{}.json", forged.thread_id))).expect("drop forged");

    let later = SystemTime::now() + Duration::from_secs(48 * 3600);
    let prune = |now| {
        codex_worktree::prune_agent_records(&records, now, Duration::from_secs(24 * 3600))
            .expect("prune records")
    };
    // The worktree exists, then only its branch: the record still restores something.
    assert_eq!(prune(later), 0);
    fs::write(checkout.root.join("agent.txt"), "agent\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    git(
        &fixture.repository,
        &[
            "worktree",
            "remove",
            "--force",
            checkout.root.to_str().expect("UTF-8 path"),
        ],
    );
    assert_eq!(prune(later), 0);
    // Without the branch it is useless, but only once it is old enough.
    git(&fixture.repository, &["branch", "-D", BRANCH]);
    assert_eq!(prune(SystemTime::now()), 0);
    assert_eq!(prune(later), 1);
    assert_eq!(
        codex_worktree::read_agent_record(&records, THREAD_ID).expect("read record"),
        None
    );
}

// ---------------------------------------------------------------------------------------------
// Amend 2026-10-27, deferred items: submodules and LFS (R2.11, R2.12, R11.12), close (UC12)
// ---------------------------------------------------------------------------------------------

/// Commits `message` in `cwd` as a test user, without signing.
fn commit_all(cwd: &Path, message: &str) {
    git(
        cwd,
        &[
            "-c",
            "user.name=Codex Test",
            "-c",
            "user.email=codex@example.invalid",
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-am",
            message,
        ],
    );
}

/// Adds a submodule `sub` at a commit of a separate repository and commits it in the fixture;
/// returns the submodule's commit.
fn add_submodule(fixture: &Fixture) -> String {
    let source = fixture
        .repository
        .parent()
        .expect("repository parent")
        .join("sub-source");
    fs::create_dir_all(&source).expect("create submodule source");
    git(&source, &["init", "--quiet", "-b", "main"]);
    fs::write(source.join("s.txt"), "s1\n").expect("write submodule file");
    git(&source, &["add", "."]);
    commit_all(&source, "s1");
    git(
        &fixture.repository,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--quiet",
            source.to_str().expect("UTF-8 submodule source"),
            "sub",
        ],
    );
    git(
        &fixture.repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "add sub"],
    );
    git(&fixture.repository.join("sub"), &["rev-parse", "HEAD"])
}

/// Commits a change inside the parent's submodule checkout and returns the new commit.
fn commit_in_submodule(fixture: &Fixture, content: &str) -> String {
    let submodule = fixture.repository.join("sub");
    fs::write(submodule.join("s.txt"), content).expect("edit submodule file");
    commit_all(&submodule, "move");
    git(&submodule, &["rev-parse", "HEAD"])
}

/// R2.11: the worktree never checks out submodules, so an emptied or removed submodule
/// directory is neither recorded on the agent branch nor counted as a change.
#[test]
fn unpopulated_submodules_are_neither_recorded_nor_counted() {
    let fixture = Fixture::new();
    add_submodule(&fixture);
    // The parent moved its submodule without committing: the base carries the new gitlink.
    let moved = commit_in_submodule(&fixture, "s2\n");
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    assert_eq!(
        git(&fixture.repository, &["ls-tree", &base, "sub"]),
        format!("160000 commit {moved}\tsub")
    );
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let sub = checkout.root.join("sub");
    assert_eq!(
        fs::read_dir(&sub)
            .expect("empty submodule directory")
            .count(),
        0
    );
    assert_eq!(
        codex_worktree::unpopulated_gitlinks(&checkout.root).expect("gitlinks"),
        vec![PathBuf::from("sub")]
    );
    assert_eq!(
        agent_worktree_status(&checkout.root, &metadata)
            .expect("state")
            .state,
        AgentWorktreeState::Clean
    );

    // The agent removes the empty directory: still nothing to record.
    fs::remove_dir(&sub).expect("remove the empty submodule directory");
    assert_eq!(
        agent_worktree_status(&checkout.root, &metadata)
            .expect("state")
            .state,
        AgentWorktreeState::Clean
    );
    assert_eq!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Unchanged
    );

    // With a real change, only the change reaches the branch; the gitlink stays.
    fs::write(checkout.root.join("b.txt"), "agent\n").expect("agent change");
    assert!(matches!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Committed { .. }
    ));
    assert_eq!(
        git(&fixture.repository, &["ls-tree", BRANCH, "sub"]),
        format!("160000 commit {moved}\tsub")
    );
    assert_eq!(
        git(
            &fixture.repository,
            &["diff", "--name-status", &base, BRANCH]
        ),
        "A\tb.txt"
    );
    // At the tip of its branch without other changes, the worktree can be closed.
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            &fixture.repository,
            &checkout.root,
            &metadata,
            /*discard_submodule_files*/ false
        )
        .expect("close"),
        RemoveOutcome::Removed
    );
    assert!(!checkout.root.exists());
}

/// R11.12: a branch that moves or removes a submodule is reported as a submodule change with
/// its commit, not as a conflict; once the parent's submodule is at that commit, the rest
/// merges cleanly.
#[test]
fn merge_reports_submodule_changes_with_their_commit() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let original = add_submodule(&fixture);
    let moved = commit_in_submodule(&fixture, "s2\n");
    git(&repository.join("sub"), &["checkout", "--quiet", &original]);
    let base = snapshot_working_tree(repository).expect("snapshot");
    let base_entries = git(repository, &["ls-tree", &base])
        .lines()
        .filter(|line| !line.ends_with("\tsub"))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let blob = git(repository, &["hash-object", "-w", "--stdin"]);
    let note = tree_entry("100644", "blob", &blob, "note.txt");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    // Without its worktree the branch is found by the checkpoint trailer, as an agent with
    // full access could have written it.
    fs::remove_dir_all(&checkout.root).expect("remove the agent checkout");

    let bumped = mktree(
        repository,
        &format!(
            "{base_entries}{note}{}",
            tree_entry("160000", "commit", &moved, "sub")
        ),
    );
    commit_tree_on(repository, BRANCH, &bumped, &base);
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::Submodule {
            path: "sub".to_owned(),
            commit: Some(moved.clone()),
        }
    );

    let removed = mktree(repository, &format!("{base_entries}{note}"));
    commit_tree_on(repository, BRANCH, &removed, &base);
    assert_eq!(
        merge_agent_branch(repository, &checkout.root, &metadata).expect("merge"),
        MergeOutcome::Submodule {
            path: "sub".to_owned(),
            commit: None,
        }
    );

    // The parent updates its submodule itself, as the refusal tells it to.
    commit_tree_on(repository, BRANCH, &bumped, &base);
    git(&repository.join("sub"), &["checkout", "--quiet", &moved]);
    let outcome = merge_agent_branch(repository, &checkout.root, &metadata).expect("merge");
    let MergeOutcome::Clean(merge) = outcome else {
        panic!("expected a clean merge, got {outcome:?}");
    };
    assert_eq!(
        merge
            .changes
            .iter()
            .map(|change| change.path.as_str())
            .collect::<Vec<_>>(),
        vec!["note.txt"]
    );
}

/// R2.12: configured LFS filters never run in host operations, so a pointer file keeps its
/// exact bytes in the base, the worktree, the branch and the merge.
#[test]
fn lfs_pointers_pass_every_host_operation_unchanged() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
    fs::write(
        repository.join(".gitattributes"),
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    )
    .expect("write attributes");
    fs::write(repository.join("model.bin"), pointer).expect("write pointer");
    git(repository, &["add", "."]);
    git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "lfs pointer"],
    );
    // Filters that would visibly corrupt the content, and that Git must not skip.
    for (key, value) in [
        ("filter.lfs.clean", "sed s/^/CLEAN:/"),
        ("filter.lfs.smudge", "sed s/^/SMUDGE:/"),
        ("filter.lfs.required", "true"),
    ] {
        git(repository, &["config", key, value]);
    }
    // Rewriting the file makes its stat dirty, so staging hashes it again.
    let rewrite = |path: &Path| {
        std::thread::sleep(Duration::from_millis(20));
        fs::write(path, pointer).expect("rewrite pointer");
    };
    rewrite(&repository.join("model.bin"));
    let base = snapshot_working_tree(repository).expect("snapshot");
    assert_eq!(
        git(
            repository,
            &["cat-file", "blob", &format!("{base}:model.bin")]
        ),
        pointer.trim_end()
    );

    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    assert_eq!(
        fs::read_to_string(checkout.root.join("model.bin")).expect("read worktree pointer"),
        pointer
    );
    rewrite(&checkout.root.join("model.bin"));
    fs::write(checkout.root.join("a.txt"), "one\nagent\n").expect("agent change");
    assert!(matches!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Committed { .. }
    ));
    assert_eq!(
        git(repository, &["diff", "--name-only", &base, BRANCH]),
        "a.txt"
    );
    assert_eq!(
        git(
            repository,
            &["cat-file", "blob", &format!("{BRANCH}:model.bin")]
        ),
        pointer.trim_end()
    );

    rewrite(&repository.join("model.bin"));
    let outcome = merge_agent_branch(repository, &checkout.root, &metadata).expect("merge");
    let MergeOutcome::Clean(merge) = outcome else {
        panic!("expected a clean merge, got {outcome:?}");
    };
    assert_eq!(
        merge
            .changes
            .iter()
            .map(|change| change.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a.txt"]
    );
    assert_eq!(
        fs::read_to_string(repository.join("model.bin")).expect("read parent pointer"),
        pointer
    );
}

/// UC12, R12.5: closing removes a worktree at the tip of its own branch or at its base, keeps
/// the branch, and keeps worktrees with uncommitted work, extra commits or a live holder.
#[test]
fn remove_agent_checkout_removes_parked_or_unchanged_worktrees_only() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let base = snapshot_working_tree(repository).expect("snapshot");
    let thread = |suffix: &str| format!("01a1748c-0000-7000-8000-0000000000{suffix}");
    let branch = |suffix: &str| format!("codex/agents/closed-00000000000000{suffix}");
    let close = |root: &Path, metadata: &AgentWorktreeMetadata| {
        codex_worktree::remove_agent_checkout(
            repository, root, metadata, /*discard_submodule_files*/ false,
        )
        .expect("close")
    };

    let (unchanged, metadata) = agent_worktree(&fixture, &base, &thread("c1"), &branch("c1"));
    assert_eq!(close(&unchanged.root, &metadata), RemoveOutcome::Removed);
    assert!(!unchanged.root.exists());
    assert_eq!(
        close(&unchanged.root, &metadata),
        RemoveOutcome::AlreadyMissing
    );

    let (parked, metadata) = agent_worktree(&fixture, &base, &thread("c2"), &branch("c2"));
    fs::write(parked.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&parked.root, &metadata);
    assert_eq!(close(&parked.root, &metadata), RemoveOutcome::Removed);
    assert_eq!(
        git(repository, &["rev-parse", "--verify", &branch("c2")]).len(),
        40
    );

    let (dirty, metadata) = agent_worktree(&fixture, &base, &thread("c3"), &branch("c3"));
    fs::write(dirty.root.join("draft.txt"), "draft\n").expect("uncommitted change");
    assert_eq!(
        close(&dirty.root, &metadata),
        RemoveOutcome::Kept(KeptReason::Changes)
    );

    let (ahead, metadata) = agent_worktree(&fixture, &base, &thread("c4"), &branch("c4"));
    fs::write(ahead.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&ahead.root, &metadata);
    git(&ahead.root, &["checkout", "--quiet", "--detach"]);
    fs::write(ahead.root.join("work.txt"), "more\n").expect("agent change");
    commit_all(&ahead.root, "own commit");
    assert_eq!(
        close(&ahead.root, &metadata),
        RemoveOutcome::Kept(KeptReason::Commits)
    );

    let (held, metadata) = agent_worktree(&fixture, &base, &thread("c5"), &branch("c5"));
    let hold = codex_worktree::hold_agent_worktree(&held.root).expect("hold the worktree");
    assert_eq!(
        close(&held.root, &metadata),
        RemoveOutcome::Kept(KeptReason::InUse)
    );
    drop(hold);
    assert_eq!(close(&held.root, &metadata), RemoveOutcome::Removed);
    assert!(dirty.root.exists() && ahead.root.exists());
}

/// D10, R12.7: an applied branch is retired only once its worktree is gone, while it is the
/// thread's own branch at the applied tip, checked out nowhere and contained in the parent;
/// its commit stays reachable (P3-2), and a restore starts from the applied tip, or from the
/// base once that commit is unusable.
#[test]
fn an_applied_branch_is_deleted_only_while_it_is_unchanged_and_free() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let base = snapshot_working_tree(repository).expect("snapshot");
    // The branch is found by the thread's checkpoints, so the thread ID must match theirs.
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    let tip = git(repository, &["rev-parse", BRANCH]);
    // The parent applied the change as an uncommitted edit.
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies");
    let delete = |expected: &str| {
        codex_worktree::retire_applied_branch(repository, &checkout.root, &metadata, expected)
            .expect("retire applied branch")
    };

    // While the worktree lives, its HEAD is attached to the branch.
    assert!(!delete(&tip));
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            repository,
            &checkout.root,
            &metadata,
            /*discard_submodule_files*/ false
        )
        .expect("close"),
        RemoveOutcome::Removed
    );
    // Another tip, a checkout of the branch elsewhere, a parent without the change: kept.
    assert!(!delete(&base));
    let elsewhere = repository
        .parent()
        .expect("repository parent")
        .join("elsewhere");
    git(
        repository,
        &[
            "worktree",
            "add",
            "--quiet",
            elsewhere.to_str().expect("UTF-8 path"),
            BRANCH,
        ],
    );
    assert!(!delete(&tip));
    git(
        repository,
        &[
            "worktree",
            "remove",
            "--force",
            elsewhere.to_str().expect("UTF-8 path"),
        ],
    );
    fs::remove_file(repository.join("work.txt")).expect("parent drops the change");
    assert!(!delete(&tip));
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies again");
    assert!(delete(&tip));
    assert!(git_fails(repository, &["rev-parse", "--verify", BRANCH]));
    // P3-2: the retired commit stays reachable outside the branch list.
    assert_eq!(
        git(
            repository,
            &[
                "rev-parse",
                &codex_worktree::applied_ref(THREAD_ID).expect("applied ref")
            ]
        ),
        tip
    );

    // The applied tip brings the agent's work back, detached.
    restore_agent_worktree(
        &fixture.manager(),
        repository,
        &checkout.root,
        &metadata,
        &RestoreFrom::Commit { tip: tip.clone() },
    )
    .expect("restore from the applied tip");
    assert_eq!(git(&checkout.root, &["rev-parse", "HEAD"]), tip);
    assert!(git_fails(&checkout.root, &["symbolic-ref", "-q", "HEAD"]));
    assert_eq!(
        fs::read_to_string(checkout.root.join("work.txt")).expect("agent file"),
        "work\n"
    );
    // P3-5: a commit that is gone fails the restore instead of starting from the base.
    fs::remove_dir_all(&checkout.root).expect("remove the checkout");
    let missing = "0".repeat(40);
    assert!(!codex_worktree::usable_commit(repository, &base, &missing).expect("check"));
    assert!(codex_worktree::usable_commit(repository, &base, &tip).expect("check"));
    let error = restore_agent_worktree(
        &fixture.manager(),
        repository,
        &checkout.root,
        &metadata,
        &RestoreFrom::Commit { tip: missing },
    )
    .expect_err("an unusable commit is not replaced by the base");
    assert!(
        format!("{error:#}").contains("no longer in the repository"),
        "{error:#}"
    );
    assert!(!checkout.root.exists());
}

// ---------------------------------------------------------------------------------------------
// Review 2026-10-27 of the deferred items (docs/fork/reviews/2026-10-27-deferred-review.md)
// ---------------------------------------------------------------------------------------------

/// Runs `command` with `sh -c` in `cwd`, as the model runs a command a refusal names.
fn run_shell(cwd: &Path, command: &str) {
    let output = Command::new("sh")
        .current_dir(cwd)
        .args(["-c", command])
        .output()
        .expect("run sh");
    assert!(
        output.status.success(),
        "`{command}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// P3-3: files the agent put into the directory of a submodule its worktree does not check
/// out are neither recorded nor counted, so closing and finalizing keep them instead of
/// dropping them with the checkout.
#[test]
fn closing_keeps_files_inside_an_unpopulated_submodule() {
    let fixture = Fixture::new();
    add_submodule(&fixture);
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let notes = checkout.root.join("sub").join("notes.txt");
    fs::write(&notes, "notes\n").expect("write into the submodule directory");
    assert_eq!(
        agent_worktree_status(&checkout.root, &metadata)
            .expect("state")
            .state,
        AgentWorktreeState::Clean
    );
    assert_eq!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Unchanged
    );

    assert_eq!(
        codex_worktree::unpopulated_submodule_files(&checkout.root).expect("submodule files"),
        vec![PathBuf::from("sub")]
    );

    let close = |discard| {
        codex_worktree::remove_agent_checkout(
            &fixture.repository,
            &checkout.root,
            &metadata,
            discard,
        )
        .expect("close")
    };
    assert_eq!(
        close(/*discard*/ false),
        RemoveOutcome::Kept(KeptReason::SubmoduleFiles)
    );
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &checkout.root, &metadata).expect("finalize"),
        RemoveOutcome::Kept(KeptReason::SubmoduleFiles)
    );
    assert!(notes.exists());
    // Confirmed, the close drops them with the checkout.
    assert_eq!(close(/*discard*/ true), RemoveOutcome::Removed);
    assert!(!checkout.root.exists());
}

/// P3-4: the command of a submodule refusal leads to a merge without the submodule change:
/// moving a submodule the parent checked out, removing it, and moving one the parent did
/// not check out (where `git -C <path>` would run in the parent's repository).
#[test]
fn submodule_refusal_commands_lead_to_a_clean_merge() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let original = add_submodule(&fixture);
    let moved = commit_in_submodule(&fixture, "s2\n");
    git(&repository.join("sub"), &["checkout", "--quiet", &original]);
    let base = snapshot_working_tree(repository).expect("snapshot");
    let base_entries = git(repository, &["ls-tree", &base])
        .lines()
        .filter(|line| !line.ends_with("\tsub"))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let blob = git(repository, &["hash-object", "-w", "--stdin"]);
    let note = tree_entry("100644", "blob", &blob, "note.txt");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::remove_dir_all(&checkout.root).expect("remove the agent checkout");
    let merge = || merge_agent_branch(repository, &checkout.root, &metadata).expect("merge");
    let only_the_note = |outcome: MergeOutcome| match outcome {
        MergeOutcome::Clean(merge) => assert_eq!(
            merge
                .changes
                .iter()
                .map(|change| change.path.as_str())
                .collect::<Vec<_>>(),
            vec!["note.txt"]
        ),
        outcome => panic!("expected a clean merge of the note, got {outcome:?}"),
    };
    let refusal_command = |outcome: MergeOutcome| match outcome {
        MergeOutcome::Submodule { path, commit } => {
            codex_worktree::submodule_command(repository, &path, commit.as_deref())
        }
        outcome => panic!("expected a submodule change, got {outcome:?}"),
    };

    // The agent's branch moves the submodule the parent checked out.
    let bumped = mktree(
        repository,
        &format!(
            "{base_entries}{note}{}",
            tree_entry("160000", "commit", &moved, "sub")
        ),
    );
    commit_tree_on(repository, BRANCH, &bumped, &base);
    run_shell(repository, &refusal_command(merge()));
    only_the_note(merge());
    git(&repository.join("sub"), &["checkout", "--quiet", &original]);

    // The agent's branch removes the submodule.
    let removed = mktree(repository, &format!("{base_entries}{note}"));
    commit_tree_on(repository, BRANCH, &removed, &base);
    run_shell(repository, &refusal_command(merge()));
    only_the_note(merge());
    git(repository, &["reset", "--quiet", "--hard"]);

    // The parent never checked the submodule out; the branch moves it. The command runs
    // from a subdirectory of the checkout, as a model's shell may.
    git(
        repository,
        &["submodule", "deinit", "--quiet", "--force", "sub"],
    );
    assert!(!repository.join("sub").join(".git").exists());
    commit_tree_on(repository, BRANCH, &bumped, &base);
    let command = refusal_command(merge());
    assert!(command.contains("update-index"), "{command}");
    fs::create_dir_all(repository.join("docs")).expect("create a subdirectory");
    run_shell(&repository.join("docs"), &command);
    only_the_note(merge());
}

/// P3-2: after Codex retires an applied branch, the agent's work stays reachable from a ref,
/// also once the parent discards its uncommitted copy, and the host record that restores it
/// is kept while that commit is the only copy; once the parent commits the change, both go.
#[test]
fn a_retired_applied_branch_keeps_its_work_and_record() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let records = fixture.codex_home.join("agent-worktrees");
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    let tip = git(repository, &["rev-parse", BRANCH]);
    let mut record =
        codex_worktree::AgentWorktreeRecord::new(&checkout.root, repository, &metadata);
    record.applied_tip = Some(tip.clone());
    codex_worktree::write_agent_record(&records, &record).expect("write record");
    // The parent applied the change as an uncommitted edit and closed the worktree.
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies");
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            repository,
            &checkout.root,
            &metadata,
            /*discard_submodule_files*/ false
        )
        .expect("close"),
        RemoveOutcome::Removed
    );
    assert!(
        codex_worktree::retire_applied_branch(repository, &checkout.root, &metadata, &tip)
            .expect("retire the applied branch")
    );
    assert!(git_fails(repository, &["rev-parse", "--verify", BRANCH]));
    let retired = codex_worktree::applied_ref(THREAD_ID).expect("applied ref");
    assert_eq!(git(repository, &["rev-parse", &retired]), tip);
    assert!(!git(repository, &["branch", "--list", "codex/*"]).contains("agents"));

    // The user discards the parent's uncommitted copy.
    fs::remove_file(repository.join("work.txt")).expect("discard the parent's copy");
    let holders = git(
        repository,
        &["for-each-ref", "--contains", &tip, "--format=%(refname)"],
    );
    assert!(!holders.is_empty(), "no ref holds the applied tip");
    let later = SystemTime::now() + Duration::from_secs(48 * 3600);
    let prune = || {
        codex_worktree::prune_agent_records(&records, later, Duration::from_secs(24 * 3600))
            .expect("prune records")
    };
    assert_eq!(prune(), 0);
    assert_eq!(
        codex_worktree::read_agent_record(&records, THREAD_ID)
            .expect("read record")
            .and_then(|record| record.applied_tip),
        Some(tip.clone())
    );

    // Committed by the parent, the work no longer needs the ref nor the record.
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies again");
    git(repository, &["add", "work.txt"]);
    git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "take the work"],
    );
    assert_eq!(prune(), 1);
    assert_eq!(
        git(
            repository,
            &["for-each-ref", "--contains", &tip, "--format=%(refname)"],
        ),
        ""
    );
}

/// P3-8: thousands of submodules with long paths, more than one command line holds, neither
/// break the state, the checkpoint nor the removal of an agent worktree.
#[test]
fn thousands_of_submodules_fit_the_host_commands() {
    use std::fmt::Write as _;
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let head = git(repository, &["rev-parse", "HEAD"]);
    let directory = format!("vendor/{}", "d".repeat(200));
    let count = 12_000;
    let mut modules = String::new();
    let mut index = String::new();
    for number in 0..count {
        let path = format!("{directory}/m{number}");
        let _ = write!(
            modules,
            "[submodule \"m{number}\"]\n\tpath = {path}\n\turl = ./m{number}\n"
        );
        let _ = writeln!(index, "160000 {head} 0\t{path}");
    }
    fs::write(repository.join(".gitmodules"), modules).expect("write .gitmodules");
    let mut child = Command::new("git")
        .current_dir(repository)
        .args(["update-index", "--index-info"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("start git update-index");
    {
        use std::io::Write as _;
        child
            .stdin
            .take()
            .expect("update-index stdin")
            .write_all(index.as_bytes())
            .expect("write index entries");
    }
    assert!(child.wait().expect("run git update-index").success());
    git(repository, &["add", ".gitmodules"]);
    git(
        repository,
        &[
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-m",
            "many submodules",
        ],
    );
    // The parent has no submodule directories either, so its HEAD is the base.
    let base = resolve_head_commit(repository).expect("HEAD");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    assert_eq!(
        codex_worktree::unpopulated_gitlinks(&checkout.root)
            .expect("gitlinks")
            .len(),
        count
    );

    // An emptied submodule directory is not a change; an agent file is.
    let _ = fs::remove_dir(checkout.root.join(&directory).join("m7"));
    assert_eq!(
        agent_worktree_status(&checkout.root, &metadata)
            .expect("state")
            .state,
        AgentWorktreeState::Clean
    );
    fs::write(checkout.root.join("b.txt"), "agent\n").expect("agent change");
    assert!(matches!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Committed { .. }
    ));
    assert_eq!(
        git(repository, &["diff", "--name-status", &base, BRANCH]),
        "A\tb.txt"
    );
}

/// D10, P3-2, P3-3: a worktree on a branch the parent applied stays after its tree stopped
/// until the sweep removes it like any parked worktree; the record tidying of the same
/// sweep then retires the branch, keeping its commit, also when an earlier process applied
/// it.
#[test]
fn a_sweep_retires_the_branches_the_parent_applied() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let records = fixture.codex_home.join("agent-worktrees");
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    let tip = git(repository, &["rev-parse", BRANCH]);
    let mut record =
        codex_worktree::AgentWorktreeRecord::new(&checkout.root, repository, &metadata);
    record.applied_tip = Some(tip.clone());
    codex_worktree::write_agent_record(&records, &record).expect("write record");
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies");
    age(&checkout.root, 48);
    let record_file = records.join(format!("{THREAD_ID}.json"));
    fs::File::open(&record_file)
        .and_then(|file| file.set_modified(SystemTime::now() - Duration::from_secs(48 * 3600)))
        .expect("age the record");

    let report = sweep_agent_worktrees(
        &fixture.manager(),
        repository,
        &DAY_POLICY,
        &|_| false,
        &|_| true,
        SystemTime::now(),
    )
    .expect("sweep");
    assert_eq!(report.removed, vec![checkout.root]);
    assert_eq!(
        codex_worktree::prune_agent_records(
            &records,
            SystemTime::now(),
            Duration::from_secs(24 * 3600)
        )
        .expect("prune records"),
        0
    );

    // The branch is retired; its commit and the record stay while the parent holds the work
    // only as an uncommitted edit.
    assert!(git_fails(repository, &["rev-parse", "--verify", BRANCH]));
    assert_eq!(
        git(
            repository,
            &[
                "rev-parse",
                &codex_worktree::applied_ref(THREAD_ID).expect("applied ref")
            ]
        ),
        tip
    );
    assert!(record_file.exists());
}

// ---------------------------------------------------------------------------------------------
// Review 2026-10-27 of the deferred fixes (docs/fork/reviews/2026-10-27-deferred-fixes-review.md)
// ---------------------------------------------------------------------------------------------

/// F2, F10: a gitlink that `.gitmodules` does not register (here its path moved) is still a
/// submodule: removing its empty directory is not a change, and files the agent put into it,
/// which Git never shows, keep the worktree from a close, a finalize and a sweep.
#[test]
fn files_in_an_unregistered_gitlink_directory_keep_the_worktree() {
    let fixture = Fixture::new();
    add_submodule(&fixture);
    git(
        &fixture.repository,
        &[
            "config",
            "--file",
            ".gitmodules",
            "submodule.sub.path",
            "old-sub",
        ],
    );
    git(&fixture.repository, &["add", ".gitmodules"]);
    commit_all(&fixture.repository, "stale .gitmodules");
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let sub = checkout.root.join("sub");

    // R2.11 holds for it: its emptied directory is not the agent's change.
    fs::remove_dir(&sub).expect("remove the empty directory");
    assert_eq!(
        agent_worktree_status(&checkout.root, &metadata)
            .expect("state")
            .state,
        AgentWorktreeState::Clean
    );
    fs::create_dir(&sub).expect("recreate it");
    let notes = sub.join("notes.txt");
    fs::write(&notes, "notes\n").expect("write into the gitlink directory");
    assert_eq!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Unchanged
    );
    assert_eq!(
        codex_worktree::unpopulated_submodule_files(&checkout.root).expect("submodule files"),
        vec![PathBuf::from("sub")]
    );
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            &fixture.repository,
            &checkout.root,
            &metadata,
            /*discard_unrecorded*/ false,
        )
        .expect("close"),
        RemoveOutcome::Kept(KeptReason::SubmoduleFiles)
    );
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &checkout.root, &metadata).expect("finalize"),
        RemoveOutcome::Kept(KeptReason::SubmoduleFiles)
    );
    assert!(notes.exists());
}

/// F3: a repository the agent created in its worktree with commits of its own is recorded by
/// a checkpoint only as a gitlink, so its commits exist nowhere else; a close, a finalize and
/// a parked sweep keep the worktree, and only `discard_unrecorded` removes it.
#[test]
fn a_nested_repository_with_its_own_commits_keeps_the_worktree() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let tool = checkout.root.join("tool");
    fs::create_dir_all(&tool).expect("create the nested repository");
    git(&tool, &["init", "--quiet", "-b", "main"]);
    fs::write(tool.join("x.txt"), "x\n").expect("nested file");
    git(&tool, &["add", "."]);
    commit_all(&tool, "nested");
    let nested_commit = git(&tool, &["rev-parse", "HEAD"]);
    assert!(matches!(
        checkpoint_worktree(&checkout.root, &metadata),
        CheckpointOutcome::Committed { .. }
    ));
    assert_eq!(
        git(&fixture.repository, &["ls-tree", BRANCH, "tool"]),
        format!("160000 commit {nested_commit}\ttool")
    );
    assert!(git_fails(
        &fixture.repository,
        &["cat-file", "-e", &format!("{nested_commit}^{{commit}}")]
    ));
    assert_eq!(
        codex_worktree::nested_repositories(&checkout.root).expect("nested repositories"),
        vec!["tool (commits that the repository does not have)".to_string()]
    );

    let close = |discard| {
        codex_worktree::remove_agent_checkout(
            &fixture.repository,
            &checkout.root,
            &metadata,
            discard,
        )
        .expect("close")
    };
    assert_eq!(
        close(/*discard*/ false),
        RemoveOutcome::Kept(KeptReason::NestedRepository)
    );
    age(&checkout.root, 48);
    let report = sweep_agent_worktrees(
        &fixture.manager(),
        &fixture.repository,
        &DAY_POLICY,
        &|_| false,
        &|_| true,
        SystemTime::now(),
    )
    .expect("sweep");
    assert_eq!(report.removed, Vec::<PathBuf>::new());
    // Uncommitted files in it count too.
    fs::write(tool.join("draft.txt"), "draft\n").expect("nested draft");
    assert_eq!(
        codex_worktree::nested_repositories(&checkout.root).expect("nested repositories"),
        vec!["tool (uncommitted files and commits that the repository does not have)".to_string()]
    );
    fs::remove_file(tool.join("draft.txt")).expect("drop the draft");
    assert_eq!(close(/*discard*/ true), RemoveOutcome::Removed);
    assert!(!checkout.root.exists());
}

/// F1: a live process in the worktree keeps it from every removal, a close with
/// `discard_unrecorded` included; it would otherwise recreate the path as a plain
/// directory and lose what it writes.
#[cfg(target_os = "linux")]
#[test]
fn a_live_process_keeps_the_worktree() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    let mut process = Command::new("sleep")
        .arg("30")
        .current_dir(checkout.root.join("."))
        .spawn()
        .expect("start a process in the worktree");
    let processes = codex_worktree::processes_in(&checkout.root);
    assert!(
        processes
            .iter()
            .any(|process_line| process_line.ends_with(": sleep 30")),
        "{processes:?}"
    );
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            &fixture.repository,
            &checkout.root,
            &metadata,
            /*discard_unrecorded*/ true,
        )
        .expect("close"),
        RemoveOutcome::Kept(KeptReason::Process)
    );
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &checkout.root, &metadata).expect("finalize"),
        RemoveOutcome::Kept(KeptReason::Process)
    );
    process.kill().expect("stop the process");
    process.wait().expect("reap the process");
    assert_eq!(
        remove_if_unchanged(&fixture.repository, &checkout.root, &metadata).expect("finalize"),
        RemoveOutcome::Removed
    );
}

/// F10: a worktree parked on its branch with files in an unpopulated submodule directory
/// stays through a sweep that would otherwise remove it.
#[test]
fn a_parked_worktree_with_submodule_files_survives_the_sweep() {
    let fixture = Fixture::new();
    add_submodule(&fixture);
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    let notes = checkout.root.join("sub").join("notes.txt");
    fs::write(&notes, "notes\n").expect("write into the submodule directory");
    age(&checkout.root, 48);
    let sweep = || {
        sweep_agent_worktrees(
            &fixture.manager(),
            &fixture.repository,
            &DAY_POLICY,
            &|_| false,
            &|_| true,
            SystemTime::now(),
        )
        .expect("sweep")
    };
    assert_eq!(sweep().removed, Vec::<PathBuf>::new());
    assert!(notes.exists());
    fs::remove_file(&notes).expect("move the notes out");
    assert_eq!(sweep().removed, vec![checkout.root]);
}

/// F9: a retired commit of the thread is replaced only by its descendant: a branch applied
/// at an unrelated tip stays rather than overwrite the ref that keeps the earlier work.
#[test]
fn retiring_never_overwrites_an_unrelated_retired_commit() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    let tip = git(repository, &["rev-parse", BRANCH]);
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies");
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            repository,
            &checkout.root,
            &metadata,
            /*discard_unrecorded*/ false
        )
        .expect("close"),
        RemoveOutcome::Removed
    );
    // An earlier retired commit that the tip does not descend from.
    let blob = git(repository, &["hash-object", "-w", "--stdin"]);
    let tree = mktree(
        repository,
        &tree_entry("100644", "blob", &blob, "other.txt"),
    );
    let unrelated = commit_tree_on(repository, "codex/agents/scratch", &tree, &base);
    git(repository, &["branch", "-D", "codex/agents/scratch"]);
    let applied = codex_worktree::applied_ref(THREAD_ID).expect("applied ref");
    git(repository, &["update-ref", &applied, &unrelated]);

    assert!(
        !codex_worktree::retire_applied_branch(repository, &checkout.root, &metadata, &tip)
            .expect("retire")
    );
    assert_eq!(git(repository, &["rev-parse", &applied]), unrelated);
    assert_eq!(git(repository, &["rev-parse", BRANCH]), tip);
    // A descendant replaces it, compared and swapped.
    git(repository, &["update-ref", &applied, &base]);
    assert!(
        codex_worktree::retire_applied_branch(repository, &checkout.root, &metadata, &tip)
            .expect("retire")
    );
    assert_eq!(git(repository, &["rev-parse", &applied]), tip);
}

/// F9: a record whose parent checkout is gone keeps living while it may own a retired
/// commit; a retired commit without a record goes once the checkout's HEAD holds its changes,
/// and stays while it is the only copy.
#[test]
fn retired_commits_never_lose_their_record_and_orphans_go_once_committed() {
    let fixture = Fixture::new();
    let repository = &fixture.repository;
    let records = fixture.codex_home.join("agent-worktrees");
    let base = snapshot_working_tree(repository).expect("snapshot");
    let (checkout, metadata) = agent_worktree(&fixture, &base, THREAD_ID, BRANCH);
    fs::write(checkout.root.join("work.txt"), "work\n").expect("agent change");
    checkpoint_worktree(&checkout.root, &metadata);
    let tip = git(repository, &["rev-parse", BRANCH]);
    fs::write(repository.join("work.txt"), "work\n").expect("parent applies");
    assert_eq!(
        codex_worktree::remove_agent_checkout(
            repository,
            &checkout.root,
            &metadata,
            /*discard_unrecorded*/ false
        )
        .expect("close"),
        RemoveOutcome::Removed
    );
    assert!(
        codex_worktree::retire_applied_branch(repository, &checkout.root, &metadata, &tip)
            .expect("retire")
    );
    let later = SystemTime::now() + Duration::from_secs(48 * 3600);
    let prune = || {
        codex_worktree::prune_agent_records(&records, later, Duration::from_secs(24 * 3600))
            .expect("prune records")
    };

    // A record whose parent checkout is gone, with an applied tip: kept.
    let mut orphaned = codex_worktree::AgentWorktreeRecord::new(
        &checkout.root,
        &repository.join("gone"),
        &metadata,
    );
    orphaned.applied_tip = Some(tip.clone());
    codex_worktree::write_agent_record(&records, &orphaned).expect("write record");
    assert_eq!(prune(), 0);
    fs::remove_file(records.join(format!("{THREAD_ID}.json"))).expect("drop the record");

    // Without a record: the ref stays while the parent holds the work only uncommitted.
    let orphans =
        || codex_worktree::prune_orphan_applied_refs(repository, &records).expect("prune orphans");
    let applied = codex_worktree::applied_ref(THREAD_ID).expect("applied ref");
    assert_eq!(orphans(), 0);
    assert_eq!(git(repository, &["rev-parse", &applied]), tip);
    git(repository, &["add", "work.txt"]);
    git(
        repository,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "take the work"],
    );
    assert_eq!(orphans(), 1);
    assert!(git_fails(repository, &["rev-parse", "--verify", &applied]));
}
