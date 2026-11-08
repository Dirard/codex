//! Fork: the TUI runs turns only through a background server of its own build.
//! A daemon whose version or fork protocol digest differs is replaced by the embedded
//! app-server with a startup warning. When the managed installation already holds this
//! version and the terminal is interactive, the recovery screen also offers a confirmed
//! restart, checked once afterwards. Explicit endpoints are never checked.
//! Spec: `specs/daemon-version-guard-2026-11-08.md`.

use crate::AppServerTarget;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::BottomPaneView;
use crate::bottom_pane::ListSelectionView;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::daemon_startup;
use crate::keymap::RuntimeKeymap;
use crate::legacy_core::config::Config;
use crate::render::renderable::ColumnRenderable;
use crate::render::renderable::Renderable;
use crate::startup_draft::StartupDraft;
use crate::version::CODEX_CLI_VERSION;
use codex_app_server_client::RemoteAppServerEndpoint;
use codex_app_server_daemon::AppServerIdentity;
use codex_app_server_daemon::LifecycleCommand;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Wrap;
use std::collections::BTreeMap;
use std::io;
use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::mpsc::unbounded_channel;
use tokio_stream::StreamExt;

/// `daemon_exclusion` reason after falling back to the embedded app-server.
pub(crate) const EXCLUSION: &str = "a background server of another Codex build";

/// Set by the shared PTY test harness so upstream tests can keep faking daemons of another
/// build. Honored only in debug builds; release builds always check.
const ALLOW_OTHER_BUILD_ENV: &str = "CODEX_FORK_ALLOW_OTHER_DAEMON_BUILD";

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
    /// What the background server runs, as a predicate of "it" / "the background server".
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    OfferRestart,
    Fallback(Hint),
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

/// UC2 "decide the recovery route": offer a restart only when it switches a managed daemon
/// to this version and someone can answer; otherwise fall back with the applicable remedy.
pub(crate) fn route(install: &DaemonInstall, tui_version: &str, interactive: bool) -> Route {
    if !install.managed {
        return Route::Fallback(Hint::Unmanaged);
    }
    match install.installed_version.as_deref() {
        Some(installed) if installed == tui_version => {
            if interactive {
                Route::OfferRestart
            } else {
                Route::Fallback(Hint::RestartFixes)
            }
        }
        _ if install.can_copy_this_cli => Route::Fallback(Hint::InstallThisCli),
        Some(installed) => Route::Fallback(Hint::InstalledOther(installed.to_string())),
        None => Route::Fallback(Hint::None),
    }
}

/// The startup warning shown when the session uses the embedded app-server instead.
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

/// UC2 "probe daemon identity → compare with this TUI build → decide the recovery route".
/// Returns the startup warning after switching `target` to the embedded app-server, or
/// `None` when the session may use the daemon (or the guard does not apply).
pub(crate) async fn check(
    startup: &mut StartupDraft,
    target: &mut AppServerTarget,
    config: &Config,
    implicit_connection: bool,
) -> io::Result<Option<String>> {
    let Some(socket_path) = daemon_socket(target).filter(|_| implicit_connection) else {
        return Ok(None);
    };
    if cfg!(debug_assertions) && std::env::var_os(ALLOW_OTHER_BUILD_ENV).is_some() {
        return Ok(None);
    }
    // An unreachable daemon is left to the upstream compatibility check.
    let Ok(identity) = startup
        .run_until(codex_app_server_daemon::probe_app_server_identity(
            &socket_path,
        ))
        .await?
    else {
        return Ok(None);
    };
    let tui = BuildIdentity::current();
    let Some(mismatch) = mismatch(&identity, &tui) else {
        return Ok(None);
    };
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
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let warning = match route(&install, &tui.version, interactive) {
        Route::Fallback(hint) => Some(fallback_warning(&mismatch, &hint)),
        Route::OfferRestart => {
            offer_restart(startup, &socket_path, config, &mismatch, &tui).await?
        }
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

/// UC2.1 "show the recovery screen → apply the choice". Restarts at most once and checks
/// the restarted daemon once; returns the warning when the session falls back.
async fn offer_restart(
    startup: &mut StartupDraft,
    socket_path: &Path,
    config: &Config,
    mismatch: &Mismatch,
    tui: &BuildIdentity,
) -> io::Result<Option<String>> {
    startup.flush_pending_events().await?;
    let keymap = RuntimeKeymap::from_config(&config.tui_keymap).map_err(io::Error::other)?;
    let mut view = recovery_view(mismatch, &tui.version, &keymap);
    let selection = run_view(startup.tui_mut(), &mut view, &keymap).await?;
    match selection {
        Some(0) => Ok(Some(fallback_warning(mismatch, &Hint::RestartFixes))),
        Some(1) => {
            let restarted = startup
                .tui_mut()
                .with_restored(crate::tui::TerminalHandoff::Restore, || async {
                    crossterm::terminal::disable_raw_mode()?;
                    codex_app_server_daemon::run(LifecycleCommand::Restart {
                        feature_overrides: BTreeMap::new(),
                    })
                    .await
                    .map_err(|err| io::Error::other(format!("{err:#}")))
                })
                .await;
            if let Err(err) = restarted {
                return Ok(Some(format!(
                    "Running without the shared background server: restarting it failed: {err}."
                )));
            }
            let warning = match startup
                .run_until(codex_app_server_daemon::probe_app_server_identity(
                    socket_path,
                ))
                .await?
            {
                Ok(identity) => self::mismatch(&identity, tui).map(|mismatch| {
                    format!(
                        "Running without the shared background server: after a restart, it {}.",
                        mismatch.predicate()
                    )
                }),
                Err(err) => Some(format!(
                    "Running without the shared background server: the restarted server did not answer: {err:#}."
                )),
            };
            Ok(warning)
        }
        Some(_) | None => Err(io::Error::other(format!(
            "Cannot use the shared background server: it {}.\n{}",
            mismatch.predicate(),
            daemon_startup::FAILURE_HINT
        ))),
    }
}

/// Mirrors the event loop of `daemon_recovery::check` for the fork's own screen.
async fn run_view(
    tui: &mut crate::tui::Tui,
    view: &mut ListSelectionView,
    keymap: &RuntimeKeymap,
) -> io::Result<Option<usize>> {
    let mut chord_matcher = crate::keymap::KeyChordMatcher::default();
    tui.discard_pending_input_before_interactive_screen()?;
    let selection = {
        let events = tui.event_stream();
        tokio::pin!(events);
        loop {
            let height = view.desired_height(tui.terminal.size()?.width);
            tui.draw(height, |frame| {
                view.render(frame.area(), frame.buffer_mut())
            })?;
            let Some(event) = events.next().await else {
                break None;
            };
            tui.screen_size_for_event(&event)?;
            if matches!(
                &event,
                crate::tui::TuiEvent::Paste(_) | crate::tui::TuiEvent::FocusLost
            ) || matches!(&event, crate::tui::TuiEvent::Mouse(mouse) if mouse.kind != crossterm::event::MouseEventKind::Moved)
            {
                chord_matcher.cancel();
            }
            if let crate::tui::TuiEvent::Key(key) = event
                && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c' | 'd'))
                {
                    break None;
                }
                let key = match chord_matcher.advance(
                    key,
                    &keymap.chords,
                    crate::keymap::KeymapContextSet::new(crate::keymap::KeymapContext::List),
                ) {
                    crate::keymap::KeyChordMatch::PassThrough => key,
                    crate::keymap::KeyChordMatch::Completed(key) => key,
                    crate::keymap::KeyChordMatch::Pending(_)
                    | crate::keymap::KeyChordMatch::Cancelled
                    | crate::keymap::KeyChordMatch::Ignored => continue,
                };
                view.handle_key_event(key);
            }
            if view.is_complete() {
                break view.take_last_selected_index();
            }
        }
    };
    tui.terminal.clear()?;
    Ok(selection)
}

fn recovery_view(
    mismatch: &Mismatch,
    tui_version: &str,
    keymap: &RuntimeKeymap,
) -> ListSelectionView {
    let mut header = ColumnRenderable::new();
    header.push(Line::from(
        "Background server runs another Codex build".bold(),
    ));
    header.push(
        Paragraph::new(format!("The background server {}.", mismatch.predicate()))
            .wrap(Wrap { trim: false }),
    );
    header.push(
        Paragraph::new(format!(
            "Its installation already holds Codex {tui_version}, so a restart switches it to this build. Restart may interrupt active or queued work in other chats."
        ))
        .wrap(Wrap { trim: false }),
    );
    let (tx, _rx) = unbounded_channel::<AppEvent>();
    ListSelectionView::new(
        SelectionViewParams {
            header: Box::new(header),
            initial_selected_idx: Some(0),
            items: vec![
                SelectionItem {
                    name: "Run without daemon this time".to_string(),
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Restart background server (recommended)".to_string(),
                    dismiss_on_select: true,
                    require_explicit_confirmation: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Cancel".to_string(),
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..SelectionViewParams::picker()
        },
        AppEventSender::new(tx),
        keymap.list.clone(),
    )
}

#[cfg(test)]
#[path = "daemon_version_fork_tests.rs"]
mod tests;
