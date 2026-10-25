#![allow(clippy::expect_used)]
//! Host-side Git operations for agent worktrees: snapshot, metadata, checkpoint and removal.

use codex_worktree::AgentWorktreeBaseKind;
use codex_worktree::AgentWorktreeMetadata;
use codex_worktree::CheckpointOutcome;
use codex_worktree::CheckpointRequest;
use codex_worktree::CreateWorktree;
use codex_worktree::KeptReason;
use codex_worktree::ManagedWorktree;
use codex_worktree::RemoveOutcome;
use codex_worktree::WorktreeManager;
use codex_worktree::WorktreeSettings;
use codex_worktree::branch_changes;
use codex_worktree::checkpoint;
use codex_worktree::read_agent_metadata;
use codex_worktree::remove_if_unchanged;
use codex_worktree::resolve_head_commit;
use codex_worktree::snapshot_working_tree;
use codex_worktree::write_agent_metadata;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
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
        git(&repository, &["config", "user.email", "codex@example.invalid"]);
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

    assert_eq!(git(repository, &["status", "--porcelain=v1"]), status_before);
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
    assert_eq!(files.lines().collect::<Vec<_>>(), [".gitignore", "a.txt", "new.txt"]);
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
    git(repository, &["commit", "--quiet", "--no-gpg-sign", "-am", "other"]);
    git(repository, &["checkout", "--quiet", "main"]);
    fs::write(repository.join("a.txt"), "main\n").expect("write conflicting change");
    git(repository, &["commit", "--quiet", "--no-gpg-sign", "-am", "main"]);
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
    assert_eq!(branch_changes(&checkout.root, &metadata).expect("summary"), None);

    fs::write(checkout.root.join("a.txt"), "one\nagent\n").expect("modify tracked file");
    fs::write(checkout.root.join("new_file.txt"), "new\n").expect("write untracked file");
    let CheckpointOutcome::Committed {
        commit: first,
        index_refresh_error,
    } = checkpoint_worktree(&checkout.root, &metadata)
    else {
        panic!("expected a first checkpoint commit");
    };
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
fn checkpoint_refuses_a_branch_moved_outside_the_worktree() {
    let fixture = Fixture::new();
    let base = snapshot_working_tree(&fixture.repository).expect("snapshot");
    let checkout = fixture.create(&base);
    let metadata = metadata(&base, AgentWorktreeBaseKind::WorkingTree);
    fs::write(checkout.root.join("a.txt"), "agent\n").expect("modify tracked file");
    let CheckpointOutcome::Committed { commit: first, .. } =
        checkpoint_worktree(&checkout.root, &metadata)
    else {
        panic!("expected a checkpoint commit");
    };

    // Another writer moves the branch to an unrelated commit while the agent keeps working.
    git(&fixture.repository, &["checkout", "--quiet", "-b", "unrelated", "main"]);
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

    let error = checkpoint(CheckpointRequest {
        root: &checkout.root,
        metadata: &metadata,
        agent_path: AGENT_PATH,
    })
    .expect_err("a moved branch must not be overwritten");
    assert!(error.to_string().contains("refusing to overwrite"));
    assert_eq!(git(&fixture.repository, &["rev-parse", BRANCH]), elsewhere);
    assert_eq!(git(&checkout.root, &["rev-parse", "HEAD"]), first);
    assert_eq!(
        fs::read_to_string(checkout.root.join("a.txt")).expect("read agent change"),
        "agent again\n"
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
