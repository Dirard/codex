//! Fork: use cases 3 and 5 of `specs/tool-output-spill-2026-11-07.md`.

use super::*;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_utils_path_uri::PathUri;
use core_test_support::TempDirExt;
use pretty_assertions::assert_eq;
use std::fs;

fn limits() -> ToolOutputSpillConfig {
    ToolOutputSpillConfig::default()
}

/// The canonical `CODEX_HOME` of a test: the store opens it without following symlinks.
fn canonical_home(home: &tempfile::TempDir) -> PathBuf {
    home.path().canonicalize().expect("canonical home")
}

fn thread_dir(home: &tempfile::TempDir) -> PathBuf {
    canonical_home(home)
        .join(SPILL_DIR)
        .join(ThreadId::default().to_string())
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::symlink_metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

/// R3.1, R3.2: directories `0700`, the file `0600`, named by 16 hex digits of SHA-256 of the
/// call id and 8 random hex digits, holding the full text.
#[cfg(unix)]
#[test]
fn files_are_private() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let dir = thread_dir(&home);
    let record = save_blocking(&dir, &file_stem("call-1"), "one\ntwo\n", limits())?;
    let expected_stem = &codex_workflow::sha256_hex(b"call-1")[..16];
    assert_eq!(record.path.parent(), Some(dir.as_path()));
    let name = record
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let suffix = name
        .strip_prefix(&format!("{expected_stem}-"))
        .and_then(|rest| rest.strip_suffix(".txt"))
        .unwrap_or_else(|| panic!("unexpected name {name}"));
    assert!(
        suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{name}"
    );
    assert_eq!(fs::read_to_string(&record.path)?, "one\ntwo\n");
    assert_eq!(
        (record.lines, record.bytes, record.tokens, record.capped),
        (2, 8, 2, None)
    );
    assert_eq!(mode(&canonical_home(&home).join(SPILL_DIR)), 0o700);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&record.path), 0o600);
    // Nothing but the file is in the directory.
    assert_eq!(fs::read_dir(&dir)?.count(), 1);
    Ok(())
}

/// R3.2: a thread directory that is a symlink to someone else's directory is refused and the
/// target stays as it was.
#[cfg(unix)]
#[test]
fn a_symlinked_thread_directory_is_refused() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    fs::set_permissions(elsewhere.path(), fs::Permissions::from_mode(0o755))?;
    let dir = thread_dir(&home);
    fs::create_dir_all(dir.parent().expect("root"))?;
    std::os::unix::fs::symlink(elsewhere.path(), &dir)?;

    let error = save_blocking(&dir, &file_stem("call-1"), "secret", limits())
        .expect_err("a symlinked thread directory is refused");
    assert!(
        error
            .to_string()
            .contains("is a symlink or not a directory"),
        "{error}"
    );
    assert_eq!(fs::read_dir(elsewhere.path())?.count(), 0);
    assert_eq!(mode(elsewhere.path()), 0o755);
    Ok(())
}

/// R3.2: a new file is never opened through a symlink at its name, and its target stays as
/// it was.
#[cfg(unix)]
#[test]
fn a_new_file_never_opens_a_symlink() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::NamedTempFile::new()?;
    fs::write(outside.path(), "outside")?;
    let dir = thread_dir(&home);
    let opened = open_thread_dir(&dir, /*create*/ true)?;
    std::os::unix::fs::symlink(outside.path(), dir.join("linked.txt"))?;

    let error = opened
        .create_new_file(std::ffi::OsStr::new("linked.txt"))
        .expect_err("a symlink is never opened");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(outside.path())?, "outside");
    Ok(())
}

/// R3.3: a text larger than `max_file_bytes` keeps its first and last halves around one marker
/// line, and the record says so.
#[cfg(unix)]
#[test]
fn large_text_keeps_head_and_tail() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let dir = thread_dir(&home);
    let max_file_bytes = 64 * 1024;
    let full = (0..20_000)
        .map(|line| format!("line {line:05}"))
        .collect::<Vec<_>>()
        .join("\n");
    let record = save_blocking(
        &dir,
        &file_stem("call-1"),
        &full,
        ToolOutputSpillConfig {
            max_file_bytes,
            ..limits()
        },
    )?;
    let text = fs::read_to_string(&record.path)?;
    let half = (max_file_bytes / 2) as usize;
    let omitted = full.len() - 2 * half;
    assert!(text.starts_with(&full[..half]));
    assert!(text.ends_with(&full[full.len() - half..]));
    assert!(text.contains(&format!(
        "\n[... {omitted} bytes not saved: tool_output_spill.max_file_bytes ...]\n"
    )));
    assert_eq!(record.bytes, text.len() as u64);
    assert_eq!(record.lines, text.lines().count());
    assert_eq!(
        record.capped,
        Some(CappedSpill {
            kept_half: half as u64,
            original_bytes: full.len() as u64,
        })
    );
    Ok(())
}

/// R3.4, R3.5: only a local primary environment whose sandbox reads the thread directory and
/// a step that exposes `exec_command` get a directory.
#[cfg(not(windows))]
#[test]
fn the_reader_is_the_primary_environment() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let cwd = tempfile::tempdir()?;
    let thread_id = ThreadId::default();
    let reader = |remote: bool, policy: FileSystemSandboxPolicy| {
        Some(SpillReader {
            remote,
            policy,
            cwd: cwd.path().to_path_buf(),
        })
    };
    let expected = home
        .path()
        .canonicalize()?
        .join(SPILL_DIR)
        .join(thread_id.to_string());
    assert_eq!(
        reader_dir(
            reader(false, FileSystemSandboxPolicy::read_only()),
            /*exposes_exec_command*/ true,
            home.path(),
            thread_id,
        ),
        Some(expected)
    );
    assert_eq!(
        reader_dir(
            reader(true, FileSystemSandboxPolicy::read_only()),
            /*exposes_exec_command*/ true,
            home.path(),
            thread_id,
        ),
        None,
        "a remote primary environment cannot read the host's files"
    );
    assert_eq!(
        reader_dir(
            reader(false, FileSystemSandboxPolicy::unrestricted()),
            /*exposes_exec_command*/ false,
            home.path(),
            thread_id,
        ),
        None,
        "without exec_command nothing reads the file"
    );
    assert_eq!(
        reader_dir(
            None,
            /*exposes_exec_command*/ true,
            home.path(),
            thread_id
        ),
        None
    );
    let cwd_only = FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry::new(
        FileSystemPath::Path {
            path: PathUri::from_abs_path(&cwd.abs()),
        },
        FileSystemAccessMode::Read,
    )]);
    assert_eq!(
        reader_dir(
            reader(false, cwd_only),
            /*exposes_exec_command*/ true,
            home.path(),
            thread_id,
        ),
        None,
        "a sandbox that cannot read CODEX_HOME gets no notice"
    );
    Ok(())
}

/// R3.6: Windows has no reader in this version.
#[cfg(windows)]
#[test]
fn windows_has_no_reader() {
    let home = tempfile::tempdir().expect("home");
    assert_eq!(
        reader_dir(
            Some(SpillReader {
                remote: false,
                policy: FileSystemSandboxPolicy::unrestricted(),
                cwd: home.path().to_path_buf(),
            }),
            /*exposes_exec_command*/ true,
            home.path(),
            ThreadId::default(),
        ),
        None
    );
}

fn set_age(path: &Path, age: Duration) -> std::io::Result<()> {
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(SystemTime::now() - age)
}

/// R5.1: a thread over `max_thread_bytes` loses its oldest files first, never the new one.
#[cfg(unix)]
#[test]
fn thread_limit_evicts_oldest_but_not_the_new_file() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let dir = thread_dir(&home);
    let opened = open_thread_dir(&dir, /*create*/ true)?;
    for (name, age) in [("old", 300), ("middle", 200), ("young", 100)] {
        let path = dir.join(format!("{name}.txt"));
        fs::write(&path, vec![b'x'; 1000])?;
        set_age(&path, Duration::from_secs(age))?;
    }
    // The new file is the largest and, by its clock, even older than the others.
    let new = dir.join("new.txt");
    fs::write(&new, vec![b'y'; 1500])?;
    set_age(&new, Duration::from_secs(1000))?;

    enforce_thread_limit(&opened, 2600, std::ffi::OsStr::new("new.txt"));
    let mut left = fs::read_dir(&dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    left.sort();
    assert_eq!(left, vec!["new.txt".to_string(), "young.txt".to_string()]);
    Ok(())
}

/// R5.2: the sweep removes old spill files and empty thread directories, then the oldest files
/// until the root fits.
#[cfg(unix)]
#[test]
fn sweep_removes_old_files_and_fits_the_total() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let root = canonical_home(&home).join(SPILL_DIR);
    let first = root.join("thread-a");
    let second = root.join("thread-b");
    let empty = root.join("thread-c");
    for dir in [&first, &second, &empty] {
        crate::fork_private_fs::create_private_dir(dir)?;
    }
    let day = Duration::from_secs(SECONDS_PER_DAY);
    let files = [
        (first.join("expired.txt"), 8 * day, 10),
        (first.join("kept-old.txt"), 3 * day, 400),
        (second.join("kept-young.txt"), day, 400),
        (second.join("kept-newest.txt"), Duration::from_secs(60), 400),
    ];
    for (path, age, bytes) in &files {
        fs::write(path, vec![b'z'; *bytes])?;
        set_age(path, *age)?;
    }

    sweep(
        &root,
        ToolOutputSpillConfig {
            max_total_bytes: 900,
            max_age_days: 7,
            ..limits()
        },
        SystemTime::now(),
    );
    let exists = |path: &Path| path.exists();
    assert!(
        !exists(&first.join("expired.txt")),
        "older than max_age_days"
    );
    assert!(
        !exists(&first.join("kept-old.txt")),
        "the oldest file goes until the root fits"
    );
    assert!(exists(&second.join("kept-young.txt")));
    assert!(exists(&second.join("kept-newest.txt")));
    assert!(!exists(&empty), "empty thread directories go");
    Ok(())
}

/// R5.3: discarding threads removes only their directories and leaves a symlink alone.
#[cfg(unix)]
#[test]
fn discard_removes_only_named_threads_and_refuses_symlinks() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    fs::write(elsewhere.path().join("keep.txt"), "keep")?;
    let root = home.path().join(SPILL_DIR);
    let deleted = ThreadId::default();
    let kept = ThreadId::default();
    let linked = ThreadId::default();
    for thread in [deleted, kept] {
        let dir = root.join(thread.to_string());
        crate::fork_private_fs::create_private_dir(&dir)?;
        fs::write(dir.join("a.txt"), "a")?;
    }
    std::os::unix::fs::symlink(elsewhere.path(), root.join(linked.to_string()))?;

    discard_threads_blocking(home.path(), &[deleted, linked]);
    assert!(!root.join(deleted.to_string()).exists());
    assert!(root.join(kept.to_string()).join("a.txt").exists());
    assert!(elsewhere.path().join("keep.txt").exists());
    assert!(
        fs::symlink_metadata(root.join(linked.to_string()))?
            .file_type()
            .is_symlink()
    );
    // A home without saved outputs is fine.
    let empty_home = tempfile::tempdir()?;
    discard_threads_blocking(empty_home.path(), &[deleted]);
    Ok(())
}

/// The names of the entries of `dir`, sorted.
fn names_in(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    Ok(names)
}

/// S-1 (review 2026-11-07): a sandbox that can write the spill root, `CODEX_HOME` or the
/// directory that holds `CODEX_HOME` could swap them for symlinks, so it gets no reader. A
/// workspace that holds a protected `.codex` keeps one; so do profiles without a sandbox.
#[cfg(not(windows))]
#[test]
fn a_sandbox_that_can_write_the_spill_root_gets_no_reader() -> anyhow::Result<()> {
    let workspace = tempfile::tempdir()?;
    let workspace_path = workspace.path().canonicalize()?;
    let thread_id = ThreadId::default();
    let writable = |root: &Path| {
        FileSystemSandboxPolicy::restricted(vec![FileSystemSandboxEntry::new(
            FileSystemPath::Path {
                path: PathUri::from_abs_path(
                    &codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(root)
                        .expect("absolute"),
                ),
            },
            FileSystemAccessMode::Write,
        )])
    };
    let reader = |policy: FileSystemSandboxPolicy| {
        Some(SpillReader {
            remote: false,
            policy,
            cwd: workspace_path.clone(),
        })
    };
    let has_reader = |policy: FileSystemSandboxPolicy, codex_home: &Path| {
        reader_dir(
            reader(policy),
            /*exposes_exec_command*/ true,
            codex_home,
            thread_id,
        )
        .is_some()
    };

    let plain_home = workspace_path.join("codex-home");
    fs::create_dir_all(&plain_home)?;
    assert!(
        !has_reader(writable(&workspace_path), &plain_home),
        "CODEX_HOME inside a writable root"
    );
    assert!(
        !has_reader(writable(&plain_home), &plain_home),
        "CODEX_HOME itself writable"
    );
    let nested_home = workspace_path.join("outer").join("inner").join(".codex");
    fs::create_dir_all(&nested_home)?;
    assert!(
        !has_reader(writable(&workspace_path), &nested_home),
        "an unprotected ancestor of CODEX_HOME can be swapped"
    );

    let protected_home = workspace_path.join(".codex");
    fs::create_dir_all(&protected_home)?;
    assert!(
        has_reader(writable(&workspace_path), &protected_home),
        "a protected .codex at the top of the workspace"
    );
    assert!(has_reader(
        FileSystemSandboxPolicy::unrestricted(),
        &plain_home
    ));
    assert!(has_reader(
        FileSystemSandboxPolicy::external_sandbox(),
        &plain_home
    ));
    Ok(())
}

/// S-1: a thread directory swapped for a symlink after Codex opened it does not redirect the
/// write or the thread limit: the target of the symlink stays as it was.
#[cfg(unix)]
#[test]
fn a_thread_directory_swapped_for_a_symlink_is_not_followed() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let victim = elsewhere.path().join("victim.txt");
    fs::write(&victim, "user data")?;
    set_age(&victim, Duration::from_secs(1000))?;
    let dir = thread_dir(&home);
    let moved = dir.with_extension("moved");

    let record = save_blocking_with(
        &dir,
        &file_stem("call-1"),
        "attacker controlled",
        ToolOutputSpillConfig {
            max_thread_bytes: 1,
            ..limits()
        },
        || {
            fs::rename(&dir, &moved).expect("move the thread directory");
            std::os::unix::fs::symlink(elsewhere.path(), &dir).expect("swap in a symlink");
        },
    );

    assert_eq!(names_in(elsewhere.path())?, vec!["victim.txt".to_string()]);
    assert_eq!(fs::read_to_string(&victim)?, "user data");
    if let Ok(record) = record {
        assert!(
            !record.path.starts_with(elsewhere.path()),
            "{}",
            record.path.display()
        );
    }
    Ok(())
}

/// S-1: a symlink anywhere above the spill root, where the canonical path had a directory, is
/// refused: nothing is written through it.
#[cfg(unix)]
#[test]
fn a_symlink_above_the_spill_root_is_refused() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let real = canonical_home(&home).join("real");
    fs::create_dir(&real)?;
    let linked = canonical_home(&home).join("linked");
    std::os::unix::fs::symlink(&real, &linked)?;
    let dir = linked.join(SPILL_DIR).join(ThreadId::default().to_string());

    let error = save_blocking(&dir, &file_stem("call-1"), "output", limits())
        .expect_err("a symlinked CODEX_HOME component is refused");
    assert!(
        error
            .to_string()
            .contains("is a symlink or not a directory"),
        "{error}"
    );
    assert_eq!(names_in(&real)?, Vec::<String>::new());
    Ok(())
}

/// S-1: a thread directory swapped for a symlink while the sweep walks the root does not lead
/// the sweep out of it.
#[cfg(unix)]
#[test]
fn a_thread_directory_swapped_during_the_sweep_is_not_followed() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let elsewhere = tempfile::tempdir()?;
    let day = Duration::from_secs(SECONDS_PER_DAY);
    let expired = elsewhere.path().join("expired.txt");
    let large = elsewhere.path().join("large.txt");
    fs::write(&expired, "old user data")?;
    set_age(&expired, 30 * day)?;
    fs::write(&large, vec![b'u'; 4096])?;
    let root = canonical_home(&home).join(SPILL_DIR);
    let decoy = root.join("decoy");
    crate::fork_private_fs::create_private_dir(&decoy)?;

    sweep_with(
        &root,
        ToolOutputSpillConfig {
            max_total_bytes: 1,
            ..limits()
        },
        SystemTime::now(),
        |dir| {
            if dir.ends_with("decoy") {
                fs::rename(dir, dir.with_extension("moved")).expect("move the thread directory");
                std::os::unix::fs::symlink(elsewhere.path(), dir).expect("swap in a symlink");
            }
        },
    );

    assert_eq!(
        names_in(elsewhere.path())?,
        vec!["expired.txt".to_string(), "large.txt".to_string()]
    );
    Ok(())
}

/// S-2: two outputs of one call id in a thread keep two files, and each record names the file
/// with its own text.
#[cfg(unix)]
#[test]
fn a_repeated_call_id_keeps_both_files() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let dir = thread_dir(&home);
    let stem = file_stem("call_0");
    let first = save_blocking(&dir, &stem, "first output", limits())?;
    let second = save_blocking(&dir, &stem, "second output", limits())?;
    assert_ne!(first.path, second.path);
    assert_eq!(fs::read_to_string(&first.path)?, "first output");
    assert_eq!(fs::read_to_string(&second.path)?, "second output");
    Ok(())
}

/// S-3: a root is swept again once the interval has passed, and each root on its own clock.
#[test]
fn sweeps_repeat_after_the_interval_per_home() {
    let home = tempfile::tempdir().expect("home");
    let other = tempfile::tempdir().expect("other home");
    let start = Instant::now();
    assert!(claim_sweep(home.path(), start));
    assert!(!claim_sweep(home.path(), start + Duration::from_secs(60)));
    assert!(claim_sweep(other.path(), start + Duration::from_secs(60)));
    assert!(claim_sweep(
        home.path(),
        start + SWEEP_INTERVAL + Duration::from_secs(1)
    ));
}
