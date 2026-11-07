//! Fork: files under `CODEX_HOME` that only the user may read: the background terminal journal
//! (`unified_exec/terminal_journal.rs`) and the saved full output of truncated tool results
//! (`tools/output_spill`, `specs/tool-output-spill-2026-11-07.md`, R3.2).

use std::ffi::OsString;
use std::fs;
use std::fs::OpenOptions;
use std::path::Path;
use std::time::SystemTime;

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

/// What an entry of a [`PrivateDir`] is, seen without following a symlink.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    File,
    Dir,
    /// A symlink, a FIFO, a socket or a device.
    Other,
}

/// An entry of a [`PrivateDir`] with its `lstat` data.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Clone, Debug)]
pub(crate) struct DirEntryInfo {
    pub(crate) name: OsString,
    pub(crate) kind: EntryKind,
    pub(crate) bytes: u64,
    pub(crate) modified: SystemTime,
}

/// A directory held open by its descriptor. Every operation names an entry of this directory
/// and none follows a symlink, so a process that swaps a path component for a symlink after
/// the directory was opened cannot redirect what Codex creates, lists, removes or `chmod`s
/// (tool-output-spill, review 2026-11-07, S-1). Unix only; elsewhere it cannot be opened.
pub(crate) struct PrivateDir {
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    #[cfg(not(unix))]
    never: std::convert::Infallible,
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;
    use std::path::Path;
    use std::time::Duration;
    use std::time::SystemTime;

    use rustix::fs::AtFlags;
    use rustix::fs::Dir;
    use rustix::fs::Mode;
    use rustix::fs::OFlags;
    use rustix::fs::Stat;
    use rustix::io::Errno;

    use super::DirEntryInfo;
    use super::EntryKind;
    use super::PrivateDir;

    /// Nesting that [`PrivateDir::remove_dir_all`] follows; Codex writes no subdirectories.
    const MAX_REMOVE_DEPTH: usize = 8;

    /// Flags of a directory on the way to the one Codex works in: searched, never read.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn traverse_flags() -> OFlags {
        OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }

    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    fn traverse_flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }

    /// Flags of a directory whose entries Codex lists, creates and removes.
    fn working_flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }

    fn kind(stat: &Stat) -> EntryKind {
        match stat.st_mode & libc::S_IFMT {
            libc::S_IFREG => EntryKind::File,
            libc::S_IFDIR => EntryKind::Dir,
            _ => EntryKind::Other,
        }
    }

    fn modified(stat: &Stat) -> SystemTime {
        let nanos = u32::try_from(stat.st_mtime_nsec).unwrap_or(0);
        u64::try_from(stat.st_mtime)
            .map(|seconds| SystemTime::UNIX_EPOCH + Duration::new(seconds, nanos))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }

    fn not_private(name: &OsStr, err: Errno) -> io::Error {
        match err {
            Errno::LOOP | Errno::NOTDIR => io::Error::other(format!(
                "{} is a symlink or not a directory",
                Path::new(name).display()
            )),
            err => io::Error::from(err),
        }
    }

    impl PrivateDir {
        /// Opens the absolute, canonical `path` from `/`, one component at a time, and fails
        /// when any component is a symlink: a symlink there appeared after the path was made
        /// canonical.
        pub(crate) fn open_no_follow(path: &Path) -> io::Result<Self> {
            if !path.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} is not absolute", path.display()),
                ));
            }
            let mut fd = rustix::fs::open("/", traverse_flags(), Mode::empty())?;
            for component in path.components() {
                match component {
                    Component::RootDir => {}
                    Component::Normal(name) => {
                        fd = rustix::fs::openat(&fd, name, traverse_flags(), Mode::empty())
                            .map_err(|err| not_private(name, err))?;
                    }
                    Component::Prefix(_) | Component::CurDir | Component::ParentDir => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{} is not canonical", path.display()),
                        ));
                    }
                }
            }
            Ok(Self { fd })
        }

        /// The child directory `name`, refused when it is a symlink, not a directory or not
        /// owned by the current user. With `create`, a missing one is created `0700` and a
        /// wider mode is narrowed to `0700` through the descriptor, never through a path.
        pub(crate) fn private_child(&self, name: &OsStr, create: bool) -> io::Result<Self> {
            if create {
                match rustix::fs::mkdirat(&self.fd, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(err) => return Err(err.into()),
                }
            }
            let fd = rustix::fs::openat(&self.fd, name, working_flags(), Mode::empty())
                .map_err(|err| not_private(name, err))?;
            let stat = rustix::fs::fstat(&fd)?;
            // SAFETY: `geteuid` has no preconditions and cannot fail.
            if stat.st_uid != unsafe { libc::geteuid() } {
                return Err(io::Error::other(format!(
                    "{} is not owned by the current user",
                    Path::new(name).display()
                )));
            }
            if create && stat.st_mode & 0o077 != 0 {
                rustix::fs::fchmod(&fd, Mode::from_raw_mode(0o700))?;
            }
            Ok(Self { fd })
        }

        /// Creates the file `name` readable by the user only (`0600`); fails with
        /// `AlreadyExists` when any entry, a symlink included, has that name.
        pub(crate) fn create_new_file(&self, name: &OsStr) -> io::Result<File> {
            let fd = rustix::fs::openat(
                &self.fd,
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )?;
            Ok(File::from(fd))
        }

        /// The entries of this directory other than `.` and `..`; an entry removed while it
        /// is listed is left out.
        pub(crate) fn entries(&self) -> io::Result<Vec<DirEntryInfo>> {
            let mut dir = Dir::read_from(&self.fd)?;
            let mut entries = Vec::new();
            while let Some(entry) = dir.read() {
                let entry = entry?;
                let name = entry.file_name();
                if matches!(name.to_bytes(), b"." | b"..") {
                    continue;
                }
                let Ok(stat) = rustix::fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) else {
                    continue;
                };
                entries.push(DirEntryInfo {
                    name: OsStr::from_bytes(name.to_bytes()).to_os_string(),
                    kind: kind(&stat),
                    bytes: u64::try_from(stat.st_size).unwrap_or(0),
                    modified: modified(&stat),
                });
            }
            Ok(entries)
        }

        /// Removes the entry `name`, which must not be a directory; a symlink is removed, not
        /// followed.
        pub(crate) fn remove_file(&self, name: &OsStr) -> io::Result<()> {
            rustix::fs::unlinkat(&self.fd, name, AtFlags::empty()).map_err(io::Error::from)
        }

        /// Removes the directory `name` if it is empty.
        pub(crate) fn remove_empty_dir(&self, name: &OsStr) -> io::Result<()> {
            rustix::fs::unlinkat(&self.fd, name, AtFlags::REMOVEDIR).map_err(io::Error::from)
        }

        /// Removes the user's directory `name` with everything in it, through descriptors; a
        /// symlink at `name` is refused and a symlink inside is removed, not followed.
        pub(crate) fn remove_dir_all(&self, name: &OsStr) -> io::Result<()> {
            self.remove_dir_all_within(name, MAX_REMOVE_DEPTH)
        }

        fn remove_dir_all_within(&self, name: &OsStr, depth: usize) -> io::Result<()> {
            let child = self.private_child(name, /*create*/ false)?;
            for entry in child.entries()? {
                match entry.kind {
                    EntryKind::Dir if depth > 0 => {
                        child.remove_dir_all_within(&entry.name, depth - 1)?;
                    }
                    EntryKind::Dir => child.remove_empty_dir(&entry.name)?,
                    EntryKind::File | EntryKind::Other => child.remove_file(&entry.name)?,
                }
            }
            self.remove_empty_dir(name)
        }
    }
}

#[cfg(not(unix))]
impl PrivateDir {
    pub(crate) fn open_no_follow(_path: &Path) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "descriptor-relative directories need Unix",
        ))
    }

    pub(crate) fn private_child(
        &self,
        _name: &std::ffi::OsStr,
        _create: bool,
    ) -> std::io::Result<Self> {
        match self.never {}
    }

    pub(crate) fn create_new_file(&self, _name: &std::ffi::OsStr) -> std::io::Result<fs::File> {
        match self.never {}
    }

    pub(crate) fn entries(&self) -> std::io::Result<Vec<DirEntryInfo>> {
        match self.never {}
    }

    pub(crate) fn remove_file(&self, _name: &std::ffi::OsStr) -> std::io::Result<()> {
        match self.never {}
    }

    pub(crate) fn remove_empty_dir(&self, _name: &std::ffi::OsStr) -> std::io::Result<()> {
        match self.never {}
    }

    pub(crate) fn remove_dir_all(&self, _name: &std::ffi::OsStr) -> std::io::Result<()> {
        match self.never {}
    }
}
