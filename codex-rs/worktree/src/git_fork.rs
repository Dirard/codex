//! Fork (K5, deferred-fixes-3 2026-10-27): a registry of the Git children this process spawns.
//!
//! The `/proc` safety net ([`super::super::agent::processes_in`]) keeps a worktree while a live
//! process has its working directory inside it. The crate runs its own Git there, so a scan
//! that ran while one of those commands was alive would see it and keep the worktree forever.
//! Excluding by command name and parent pid was both too broad (a foreign `git` that is a
//! direct child of Codex was skipped) and too narrow (a grandchild was not). The scan now
//! excludes exactly the pids this process registered here while they run: a long-lived Git
//! child the crate started is skipped, and a foreign Git — whatever its name or parent — is not.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::PoisonError;

fn registry() -> &'static Mutex<HashMap<u32, usize>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u32, usize>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A Git child of this process, recorded in the registry while it is alive. Created when the
/// crate spawns Git and dropped when the command returns.
#[derive(Debug)]
pub(crate) struct GitChild {
    pid: u32,
}

impl GitChild {
    pub(crate) fn new(pid: u32) -> Self {
        *registry()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(pid)
            .or_insert(0) += 1;
        Self { pid }
    }
}

impl Drop for GitChild {
    fn drop(&mut self) {
        let mut guard = registry().lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = guard.get_mut(&self.pid) {
            *count -= 1;
            if *count == 0 {
                guard.remove(&self.pid);
            }
        }
    }
}

/// Whether `pid` is a Git child this process spawned and still runs.
pub(crate) fn is_own_git_child(pid: u32) -> bool {
    registry()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&pid)
}
