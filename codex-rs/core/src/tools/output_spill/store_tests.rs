//! Fork: use cases 3 and 5 of `specs/tool-output-spill-2026-11-07.md`.

use super::*;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_utils_path_uri::PathUri;
use core_test_support::TempDirExt;
use pretty_assertions::assert_eq;

fn limits() -> ToolOutputSpillConfig {
    ToolOutputSpillConfig::default()
}

fn thread_dir(home: &tempfile::TempDir) -> PathBuf {
    home.path()
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
/// call id, holding the full text.
#[cfg(unix)]
#[test]
fn files_are_private() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let dir = thread_dir(&home);
    let record = save_blocking(&dir, &file_stem("call-1"), "one\ntwo\n", limits())?;
    let expected_stem = &codex_workflow::sha256_hex(b"call-1")[..16];
    assert_eq!(record.path, dir.join(format!("{expected_stem}.txt")));
    assert_eq!(fs::read_to_string(&record.path)?, "one\ntwo\n");
    assert_eq!(
        (record.lines, record.bytes, record.tokens, record.capped),
        (2, 8, 2, None)
    );
    assert_eq!(mode(&home.path().join(SPILL_DIR)), 0o700);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&record.path), 0o600);
    // Nothing but the file is left: the temporary file was renamed over it.
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
    assert!(error.to_string().contains("is not a directory"), "{error}");
    assert_eq!(fs::read_dir(elsewhere.path())?.count(), 0);
    assert_eq!(mode(elsewhere.path()), 0o755);
    Ok(())
}

/// R3.2: a symlink at the file's place is replaced by the new file, not followed.
#[cfg(unix)]
#[test]
fn a_symlink_at_the_target_is_replaced_not_followed() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let outside = tempfile::NamedTempFile::new()?;
    fs::write(outside.path(), "outside")?;
    let dir = thread_dir(&home);
    crate::fork_private_fs::create_private_dir(&dir)?;
    let stem = file_stem("call-1");
    std::os::unix::fs::symlink(outside.path(), dir.join(format!("{stem}.txt")))?;

    let record = save_blocking(&dir, &stem, "full output", limits())?;
    assert_eq!(fs::read_to_string(outside.path())?, "outside");
    assert!(fs::symlink_metadata(&record.path)?.file_type().is_file());
    assert_eq!(fs::read_to_string(&record.path)?, "full output");
    Ok(())
}

/// R3.3: a text larger than `max_file_bytes` keeps its first and last halves around one marker
/// line, and the record says so.
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
#[test]
fn thread_limit_evicts_oldest_but_not_the_new_file() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let dir = thread_dir(&home);
    crate::fork_private_fs::create_private_dir(&dir)?;
    for (name, age) in [("old", 300), ("middle", 200), ("young", 100)] {
        let path = dir.join(format!("{name}.txt"));
        fs::write(&path, vec![b'x'; 1000])?;
        set_age(&path, Duration::from_secs(age))?;
    }
    // The new file is the largest and, by its clock, even older than the others.
    let new = dir.join("new.txt");
    fs::write(&new, vec![b'y'; 1500])?;
    set_age(&new, Duration::from_secs(1000))?;

    enforce_thread_limit(&dir, 2600, &new);
    let mut left = fs::read_dir(&dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    left.sort();
    assert_eq!(left, vec!["new.txt".to_string(), "young.txt".to_string()]);
    Ok(())
}

/// R5.2: the sweep removes old spill files, temporary files older than an hour and empty thread
/// directories, then the oldest files until the root fits; it runs once per process.
#[test]
fn sweep_removes_old_files_tmp_files_and_fits_the_total() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let root = home.path().join(SPILL_DIR);
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
        (first.join("stale.1234.tmp"), Duration::from_secs(7200), 10),
        (first.join("fresh.5678.tmp"), Duration::from_secs(60), 10),
        (second.join("kept-young.txt"), day, 400),
        (second.join("kept-newest.txt"), Duration::from_secs(60), 400),
    ];
    for (path, age, bytes) in &files {
        fs::write(path, vec![b'z'; *bytes])?;
        set_age(path, *age)?;
    }

    reset_sweep_for_test();
    assert!(sweep_once_for_test(
        &root,
        ToolOutputSpillConfig {
            max_total_bytes: 900,
            max_age_days: 7,
            ..limits()
        },
    ));
    let exists = |path: &Path| path.exists();
    assert!(
        !exists(&first.join("expired.txt")),
        "older than max_age_days"
    );
    assert!(
        !exists(&first.join("stale.1234.tmp")),
        "temporary for an hour"
    );
    assert!(exists(&first.join("fresh.5678.tmp")), "a write in progress");
    assert!(
        !exists(&first.join("kept-old.txt")),
        "the oldest file goes until the root fits"
    );
    assert!(exists(&second.join("kept-young.txt")));
    assert!(exists(&second.join("kept-newest.txt")));
    assert!(!exists(&empty), "empty thread directories go");

    // A second sweep in the same process does nothing.
    fs::write(second.join("late.txt"), vec![b'z'; 400])?;
    assert!(!sweep_once_for_test(
        &root,
        ToolOutputSpillConfig {
            max_total_bytes: 1,
            ..limits()
        },
    ));
    assert!(exists(&second.join("late.txt")));
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
