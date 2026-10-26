//! Deny-read for the absolute symlinks that bubblewrap's `--dev` creates.
//!
//! `--dev /dev` adds `/dev/fd`, `/dev/stdin`, `/dev/stdout`, `/dev/stderr`, and `/dev/core` as
//! symlinks into the sandbox's own procfs. Bubblewrap resolves mount destinations from its
//! staging root, where `/proc` does not exist, so a mask on one of them aborts startup
//! (`Can't mount tmpfs on /newroot/dev/fd`), and `--symlink` refuses to replace them. Their
//! targets are per-process procfs entries, which no mount can cover either. The inner stage
//! therefore replaces each denied symlink on the sandbox-private `/dev` tmpfs before the command
//! starts. That tmpfs stays writable to the command, as before, and `/proc/self/fd` still lists
//! the command's own descriptors: the denial covers the path, not the descriptors behind it.

use std::fs;
use std::fs::DirBuilder;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;

use super::BwrapArgs;
use super::path_to_string;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result;

/// Absolute symlinks created by bubblewrap's `--dev` (`SETUP_MOUNT_DEV` in `bubblewrap.c`).
const BWRAP_DEV_SYMLINKS: [&str; 5] = [
    "/dev/fd",
    "/dev/stdin",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/core",
];

const DENY_DEV_SYMLINK_ARG: &str = "--deny-dev-symlink";

/// Leaves a deny path through one of bubblewrap's `/dev` symlinks to the inner stage.
///
/// A path below such a symlink denies the whole symlink: its procfs target cannot be masked
/// more narrowly.
pub(super) fn defer_denial(bwrap_args: &mut BwrapArgs, unreadable_root: &Path) -> bool {
    let Some(symlink) = BWRAP_DEV_SYMLINKS
        .into_iter()
        .map(Path::new)
        .find(|symlink| unreadable_root.starts_with(symlink))
    else {
        return false;
    };
    if !bwrap_args
        .dev_symlink_denials
        .iter()
        .any(|denied| denied == symlink)
    {
        bwrap_args.dev_symlink_denials.push(symlink.to_path_buf());
    }
    true
}

/// Hands deferred denials to the inner stage, the only code that can enforce them.
pub(super) fn add_inner_stage_args(
    mut command: Vec<String>,
    denials: &[PathBuf],
) -> Result<Vec<String>> {
    let Some(denied) = denials.first() else {
        return Ok(command);
    };
    if !command
        .iter()
        .skip(1)
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| arg == "--apply-seccomp-then-exec")
    {
        return Err(CodexErr::Fatal(format!(
            "cannot enforce sandbox deny-read path {} without the inner sandbox stage",
            denied.display()
        )));
    }
    let args = denials
        .iter()
        .flat_map(|denied| [DENY_DEV_SYMLINK_ARG.to_string(), path_to_string(denied)]);
    command.splice(1..1, args);
    Ok(command)
}

/// Replaces denied bubblewrap `/dev` symlinks with inaccessible placeholders, mirroring the
/// outer masks: an empty `000` directory for a directory target, otherwise an empty `000` file.
pub(crate) fn replace_denied(symlinks: &[PathBuf]) -> io::Result<()> {
    for symlink in symlinks {
        if !BWRAP_DEV_SYMLINKS
            .into_iter()
            .any(|known| Path::new(known) == symlink)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a bubblewrap /dev symlink", symlink.display()),
            ));
        }
        match fs::symlink_metadata(symlink) {
            Ok(metadata) if metadata.file_type().is_symlink() => {}
            // A deny mask on `/dev` itself already hides the symlink.
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) =>
            {
                continue;
            }
            Ok(_) => {
                return Err(io::Error::other(format!(
                    "{} is not a symlink",
                    symlink.display()
                )));
            }
            Err(err) => return Err(err),
        }
        let directory = symlink.is_dir();
        fs::remove_file(symlink)?;
        if directory {
            DirBuilder::new().mode(0o000).create(symlink)?;
        } else {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o000)
                .open(symlink)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "bwrap_dev_symlinks_tests.rs"]
mod tests;
