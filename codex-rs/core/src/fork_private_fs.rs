//! Fork: files under `CODEX_HOME` that only the user may read: the background terminal journal
//! (`unified_exec/terminal_journal.rs`) and the saved full output of truncated tool results
//! (`tools/output_spill`, `specs/tool-output-spill-2026-11-07.md`, R3.2).

use std::fs;
use std::fs::OpenOptions;
use std::path::Path;

/// Creates `dir` and its missing parents readable by the user only (`0700` on Unix), and
/// narrows an existing `dir` that an older version created by the umask.
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::PermissionsExt;
        builder.mode(0o700);
        builder.create(dir)?;
        let metadata = fs::metadata(dir)?;
        if metadata.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        builder.create(dir)
    }
}

/// [`create_private_dir`], and then refuses a `dir` that is a symlink or, on Unix, is not owned
/// by the current user: another user, or a sandboxed process that could write there, must not
/// redirect what Codex writes.
pub(crate) fn create_owned_private_dir(dir: &Path) -> std::io::Result<()> {
    // An existing `dir` is checked before `create_private_dir` narrows its mode: through a
    // symlink that would change the permissions of the target.
    match fs::symlink_metadata(dir) {
        Ok(_) => check_owned_dir(dir)?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    create_private_dir(dir)?;
    check_owned_dir(dir)
}

/// Whether `dir` itself, not a symlink to it, is a directory of the current user.
pub(crate) fn check_owned_dir(dir: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::other(format!(
            "{} is not a directory",
            dir.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::geteuid() };
        if metadata.uid() != uid {
            return Err(std::io::Error::other(format!(
                "{} is not owned by the current user",
                dir.display()
            )));
        }
    }
    Ok(())
}

/// Options that create files readable by the user only (`0600` on Unix) and never open a
/// symlink (`O_NOFOLLOW`).
pub(crate) fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
}
