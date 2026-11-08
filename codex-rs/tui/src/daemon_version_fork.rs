//! Fork: the TUI runs turns only through a background server of its own build, and a
//! background server it cannot use never stops the TUI. A daemon whose version or fork
//! protocol digest differs, a daemon that does not answer, and a failed daemon start all
//! leave the session on the embedded app-server with one startup warning that names the
//! cause and the remedy. There is no blocking screen and no exit. A matching daemon keeps
//! the upstream flow; explicit endpoints are never checked.
//! Spec: `specs/daemon-version-guard-2026-11-08.md`.

use crate::AppServerTarget;
use crate::daemon_startup;
use crate::startup_draft::StartupDraft;
use crate::version::CODEX_CLI_VERSION;
use codex_app_server_client::RemoteAppServerEndpoint;
use codex_app_server_daemon::AppServerIdentity;
use codex_app_server_daemon::LifecycleCommand;
use codex_app_server_daemon::LifecycleOutput;
use std::io;
use std::path::PathBuf;

/// `daemon_exclusion` reason after falling back to the embedded app-server.
pub(crate) const EXCLUSION: &str = "an unusable background server";

/// Set by the shared PTY test harness so upstream tests keep the upstream daemon policy
/// (fake daemons of another build, exit on a failed start). Honored only in debug builds;
/// release builds always apply the fork policy.
const UPSTREAM_POLICY_ENV: &str = "CODEX_FORK_UPSTREAM_DAEMON_POLICY";

fn upstream_policy() -> bool {
    cfg!(debug_assertions) && std::env::var_os(UPSTREAM_POLICY_ENV).is_some()
}

/// The build this TUI was compiled as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuildIdentity {
    pub(crate) version: String,
    pub(crate) experimental_protocol_digest: String,
}

impl BuildIdentity {
    fn current() -> Self {
        Self {
            version: CODEX_CLI_VERSION.to_string(),
            experimental_protocol_digest:
                codex_app_server_protocol::go_manifest::initialize_digest_snapshot()
                    .experimental_protocol_digest
                    .to_string(),
        }
    }
}

/// How the daemon differs from this TUI build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mismatch {
    /// Another version, or none could be read.
    Version { daemon: Option<String>, tui: String },
    /// The same version without this build's protocol digest: upstream or another build.
    Build { version: String },
}

impl Mismatch {
    /// What the background server runs, as a predicate of "it".
    fn predicate(&self) -> String {
        match self {
            Self::Version {
                daemon: Some(daemon),
                tui,
            } => format!("runs Codex {daemon}, not this session's Codex {tui}"),
            Self::Version { daemon: None, tui } => {
                format!("reports no Codex version, and this session runs Codex {tui}")
            }
            Self::Build { version } => format!("runs another build of Codex {version}"),
        }
    }
}

/// What the daemon's lifecycle state allows for recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DaemonInstall {
    pub(crate) managed: bool,
    pub(crate) installed_version: Option<String>,
    pub(crate) can_copy_this_cli: bool,
}

/// The remedy named by the startup warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hint {
    RestartFixes,
    InstallThisCli,
    InstalledOther(String),
    Unmanaged,
    None,
}

/// UC2 "compare with this TUI build": an exact version string and the same fork protocol
/// digest; a missing version or digest is a mismatch.
pub(crate) fn mismatch(daemon: &AppServerIdentity, tui: &BuildIdentity) -> Option<Mismatch> {
    if daemon.version.as_deref() != Some(tui.version.as_str()) {
        return Some(Mismatch::Version {
            daemon: daemon.version.clone(),
            tui: tui.version.clone(),
        });
    }
    (daemon.experimental_protocol_digest.as_deref()
        != Some(tui.experimental_protocol_digest.as_str()))
    .then(|| Mismatch::Build {
        version: tui.version.clone(),
    })
}

/// UC2 "choose the remedy": suggest a restart only when it switches a managed daemon to
/// this version; otherwise name what would.
pub(crate) fn hint(install: &DaemonInstall, tui_version: &str) -> Hint {
    if !install.managed {
        return Hint::Unmanaged;
    }
    match install.installed_version.as_deref() {
        Some(installed) if installed == tui_version => Hint::RestartFixes,
        _ if install.can_copy_this_cli => Hint::InstallThisCli,
        Some(installed) => Hint::InstalledOther(installed.to_string()),
        None => Hint::None,
    }
}

/// The startup warning for a daemon of another build.
pub(crate) fn fallback_warning(mismatch: &Mismatch, hint: &Hint) -> String {
    let hint = match hint {
        Hint::RestartFixes => {
            " Run `codex app-server daemon restart` to switch it to this build.".to_string()
        }
        Hint::InstallThisCli => {
            " Run `codex app-server daemon update --from-cli` to install this build for it."
                .to_string()
        }
        Hint::InstalledOther(version) => {
            format!(" Its installation holds Codex {version}, so restarting it would not help.")
        }
        Hint::Unmanaged => {
            " It is not managed by Codex; restart it where it was started.".to_string()
        }
        Hint::None => String::new(),
    };
    format!(
        "Running without the shared background server: it {}.{hint}",
        mismatch.predicate()
    )
}

/// The startup warning for a daemon that did not answer the identity probe.
pub(crate) fn no_response_warning(cause: &str) -> String {
    format!(
        "Running without the shared background server: it does not respond ({cause}). Run `codex app-server daemon restart`, or use --no-daemon."
    )
}

/// The startup warning for a daemon that could not start or be installed. The cause comes
/// from the upstream start error, without its fatal `--no-daemon` hint.
pub(crate) fn start_failure_warning(error: &io::Error) -> String {
    let message = error.to_string();
    let cause = message
        .strip_suffix(daemon_startup::FAILURE_HINT)
        .unwrap_or(&message)
        .trim()
        .trim_end_matches('.');
    format!(
        "Running without the shared background server: it could not start ({cause}). Use --no-daemon or set features.daemon_auto_start = false to skip starting it."
    )
}

/// UC3 "fall back when the daemon cannot start": a start or installation error becomes
/// the startup warning and leaves the session without a daemon, instead of ending it.
pub(crate) fn started(
    result: io::Result<Option<LifecycleOutput>>,
    warning: &mut Option<String>,
) -> io::Result<Option<LifecycleOutput>> {
    if upstream_policy() {
        return result;
    }
    Ok(result.unwrap_or_else(|error| {
        let message = start_failure_warning(&error);
        tracing::warn!("{message}");
        *warning = Some(message);
        None
    }))
}

/// UC2 "probe daemon identity → compare with this TUI build → choose the remedy".
/// Returns the startup warning after switching `target` to the embedded app-server, or
/// `None` when the session may use the daemon (or the guard does not apply).
pub(crate) async fn check(
    startup: &mut StartupDraft,
    target: &mut AppServerTarget,
    implicit_connection: bool,
) -> io::Result<Option<String>> {
    let Some(socket_path) = daemon_socket(target).filter(|_| implicit_connection) else {
        return Ok(None);
    };
    if upstream_policy() {
        return Ok(None);
    }
    let tui = BuildIdentity::current();
    let warning = match startup
        .run_until(codex_app_server_daemon::probe_app_server_identity(
            &socket_path,
        ))
        .await?
    {
        Err(error) => Some(no_response_warning(&format!("{error:#}"))),
        Ok(identity) => match mismatch(&identity, &tui) {
            None => None,
            Some(mismatch) => {
                let status = startup
                    .run_until(codex_app_server_daemon::run(LifecycleCommand::Version))
                    .await?
                    .ok();
                let install = DaemonInstall {
                    managed: status
                        .as_ref()
                        .is_some_and(|status| status.backend.is_some()),
                    installed_version: status.and_then(|status| status.managed_codex_version),
                    can_copy_this_cli: codex_install_context::InstallContext::current()
                        .package_layout
                        .is_some(),
                };
                Some(fallback_warning(&mismatch, &hint(&install, &tui.version)))
            }
        },
    };
    if let Some(warning) = &warning {
        tracing::warn!("{warning}");
        *target = AppServerTarget::Embedded;
    }
    Ok(warning)
}

fn daemon_socket(target: &AppServerTarget) -> Option<PathBuf> {
    match target {
        AppServerTarget::LocalDaemon {
            endpoint: RemoteAppServerEndpoint::UnixSocket { socket_path },
            ..
        } => Some(socket_path.as_path().to_path_buf()),
        _ => None,
    }
}

#[cfg(test)]
#[path = "daemon_version_fork_tests.rs"]
mod tests;
