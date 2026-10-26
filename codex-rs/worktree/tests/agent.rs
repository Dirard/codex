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
            base_sha: base.clone(),
            base_kind: AgentWorktreeBaseKind::WorkingTree,
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
    assert_eq!(recovered.base_sha, base);

    // A checkpoint written before R4.9 has no base trailers: its parent is the base.
    let tree = git(
        &fixture.repository,
        &["rev-parse", &format!("{base}^{{tree}}")],
    );
    let old = git(
        &fixture.repository,
        &[
            "commit-tree",
            "-p",
            &base,
            "-m",
            &format!(
                "codex agent /root/old checkpoint\n\nCodex-Agent-Path: /root/old\nCodex-Thread-Id: {other_thread}0"
            ),
            &tree,
        ],
    );
    let old_branch = "codex/agents/old-0000000000000000";
    git(&fixture.repository, &["branch", old_branch, &old]);
    assert_eq!(
        agent_branch_for_thread(&fixture.repository, old_branch, &format!("{other_thread}0"))
            .expect("find old branch"),
        Some(RecoveredBranch {
            branch: old_branch.to_owned(),
            tip: old,
            base_sha: base,
            base_kind: AgentWorktreeBaseKind::WorkingTree,
        })
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
