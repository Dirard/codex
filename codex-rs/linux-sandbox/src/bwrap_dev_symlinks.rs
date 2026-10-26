//! Deny-read for the absolute symlinks that bubblewrap's `--dev` creates.
//!
//! `--dev /dev` adds `/dev/fd`, `/dev/stdin`, `/dev/stdout`, `/dev/stderr`, and `/dev/core` as
//! symlinks into the sandbox's own procfs. Bubblewrap resolves mount destinations from its
//! staging root, where `/proc` does not exist, so a mask on one of them aborts startup
//! (`Can't mount tmpfs on /newroot/dev/fd`), and `--symlink` refuses to replace them. Their
//! targets are per-process procfs entries, which no mount can cover either. The inner stage
//! therefore replaces each denied symlink on the sandbox-private `/dev` tmpfs before the command
//! starts.
//!
//! Limit: unlike the mount masks of other deny paths, this denial is not tamper-proof. The
//! `/dev` tmpfs belongs to the sandbox user and stays writable to the command, so the command
//! can remove the placeholder and recreate the symlink (`rmdir /dev/fd; ln -s /proc/self/fd
//! /dev/fd`). A mount would make the placeholder busy, but nothing can mount it: bubblewrap
//! follows the symlink, the inner stage has dropped every capability before it runs, and
//! building `/dev` without `--dev` loses its private devpts instance. The denial therefore
//! covers the path the command starts with. Nothing behind these paths is data the command
//! cannot reach otherwise: `/proc/self/fd` lists its own descriptors, and `/proc/kcore` needs
//! `CAP_SYS_RAWIO`.

use std::fs;
use std::fs::DirBuilder;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;

use super::BwrapArgs;
use super::path_to_string;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result;
use rustix::fs::StatVfsMountFlags;

/// Absolute symlinks created by bubblewrap's `--dev` (`SETUP_MOUNT_DEV` in `bubblewrap.c`).
const BWRAP_DEV_SYMLINKS: [&str; 5] = [
    "/dev/fd",
    "/dev/stdin",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/core",
];

const DENY_DEV_SYMLINK_ARG: &str = "--deny-dev-symlink";

/// Leaves a deny path through one of bubblewrap's `/dev` symlinks to the inner stage, when the
/// sandbox `/dev` there is bubblewrap's own tmpfs.
///
/// A path below such a symlink denies the whole symlink: its procfs target cannot be masked
/// more narrowly. A writable root at the symlink or above it, other than `/`, puts the host
/// `/dev` in the sandbox: such a denial is not deferred, so the writable-symlink check fails
/// closed. A writable `/` keeps the symlink on the tmpfs, because binding `/` mounts `--dev
/// /dev` again.
pub(super) fn defer_denial(
    bwrap_args: &mut BwrapArgs,
    unreadable_root: &Path,
    allowed_write_paths: &[PathBuf],
) -> bool {
    let Some(symlink) = BWRAP_DEV_SYMLINKS
        .into_iter()
        .map(Path::new)
        .find(|symlink| unreadable_root.starts_with(symlink))
    else {
        return false;
    };
    if allowed_write_paths
        .iter()
        .any(|root| root != Path::new("/") && symlink.starts_with(root))
    {
        return false;
    }
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

/// Fails unless `--deny-dev-symlink` reaches the inner stage, the only stage that applies it,
/// as `--verify-fd-mount` does.
pub(crate) fn ensure_inner_stage(apply_seccomp_then_exec: bool, symlinks: &[PathBuf]) {
    if !apply_seccomp_then_exec && !symlinks.is_empty() {
        panic!("--deny-dev-symlink is only supported in the inner sandbox stage");
    }
}

/// Replaces denied bubblewrap `/dev` symlinks with inaccessible placeholders, mirroring the
/// outer masks: an empty `000` directory for a directory target, otherwise an empty `000` file.
///
/// Before it changes anything, it requires `/dev` to be bubblewrap's sandbox tmpfs, so that a
/// misplaced call never touches a host `/dev` (`ensure_sandbox_dev`).
pub(crate) fn replace_denied(symlinks: &[PathBuf]) -> io::Result<()> {
    let mut sandbox_dev_checked = false;
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
        if !sandbox_dev_checked {
            ensure_sandbox_dev(Path::new("/dev"))?;
            sandbox_dev_checked = true;
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

/// Requires `dev` to look like the tmpfs that bubblewrap's `--dev` mounts: a writable `nodev`
/// tmpfs owned by the sandbox user. A host `/dev` is never all of that: devtmpfs and container
/// tmpfs hold device nodes, so they are not `nodev`; bubblewrap binds a host `/dev` read-only
/// unless a writable root asks otherwise; and inside the sandbox's user namespace the host root
/// owner is unmapped.
fn ensure_sandbox_dev(dev: &Path) -> io::Result<()> {
    let filesystem = rustix::fs::statfs(dev)?;
    let mount_flags = rustix::fs::statvfs(dev)?.f_flag;
    let owner = fs::metadata(dev)?.uid();
    // SAFETY: geteuid has no preconditions and cannot fail.
    let user = unsafe { libc::geteuid() };
    if filesystem.f_type != libc::TMPFS_MAGIC
        || !mount_flags.contains(StatVfsMountFlags::NODEV)
        || mount_flags.contains(StatVfsMountFlags::RDONLY)
        || owner != user
    {
        return Err(io::Error::other(format!(
            "{} is not the sandbox /dev tmpfs of bubblewrap",
            dev.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "bwrap_dev_symlinks_tests.rs"]
mod tests;
