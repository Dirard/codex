use super::*;
use crossterm::event::KeyEvent;
use pretty_assertions::assert_eq;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn tui_build() -> BuildIdentity {
    BuildIdentity {
        version: "0.167.0".to_string(),
        experimental_protocol_digest: "fork-digest".to_string(),
    }
}

fn daemon(version: Option<&str>, digest: Option<&str>) -> AppServerIdentity {
    AppServerIdentity {
        version: version.map(str::to_string),
        experimental_protocol_digest: digest.map(str::to_string),
    }
}

#[test]
fn only_the_same_version_and_fork_protocol_digest_match() {
    let tui = tui_build();
    for (daemon, expected) in [
        (daemon(Some("0.167.0"), Some("fork-digest")), None),
        (
            daemon(Some("0.156.0"), Some("old-digest")),
            Some(Mismatch::Version {
                daemon: Some("0.156.0".to_string()),
                tui: "0.167.0".to_string(),
            }),
        ),
        // A patch release of the same line is another build too.
        (
            daemon(Some("0.167.1"), Some("fork-digest")),
            Some(Mismatch::Version {
                daemon: Some("0.167.1".to_string()),
                tui: "0.167.0".to_string(),
            }),
        ),
        (
            daemon(None, Some("fork-digest")),
            Some(Mismatch::Version {
                daemon: None,
                tui: "0.167.0".to_string(),
            }),
        ),
        // Upstream release with the fork's version number: no fork digest.
        (
            daemon(Some("0.167.0"), None),
            Some(Mismatch::Build {
                version: "0.167.0".to_string(),
            }),
        ),
        (
            daemon(Some("0.167.0"), Some("other-digest")),
            Some(Mismatch::Build {
                version: "0.167.0".to_string(),
            }),
        ),
    ] {
        assert_eq!(mismatch(&daemon, &tui), expected, "{daemon:?}");
    }
}

#[test]
fn restart_is_offered_only_when_it_switches_a_managed_daemon_to_this_build() {
    let install = |managed: bool, installed: Option<&str>, can_copy: bool| DaemonInstall {
        managed,
        installed_version: installed.map(str::to_string),
        can_copy_this_cli: can_copy,
    };
    for (install, interactive, expected) in [
        (
            install(true, Some("0.167.0"), false),
            true,
            Route::OfferRestart,
        ),
        // Nobody can answer: fall back and name the restart command.
        (
            install(true, Some("0.167.0"), true),
            false,
            Route::Fallback(Hint::RestartFixes),
        ),
        (
            install(true, Some("0.156.0"), true),
            true,
            Route::Fallback(Hint::InstallThisCli),
        ),
        (
            install(true, Some("0.156.0"), false),
            true,
            Route::Fallback(Hint::InstalledOther("0.156.0".to_string())),
        ),
        (
            install(true, None, false),
            true,
            Route::Fallback(Hint::None),
        ),
        (
            install(true, None, true),
            true,
            Route::Fallback(Hint::InstallThisCli),
        ),
        (
            install(false, Some("0.167.0"), true),
            true,
            Route::Fallback(Hint::Unmanaged),
        ),
    ] {
        assert_eq!(
            route(&install, "0.167.0", interactive),
            expected,
            "{install:?}, interactive: {interactive}"
        );
    }
}

#[test]
fn fallback_warning_names_both_builds_and_the_remedy() {
    let older = Mismatch::Version {
        daemon: Some("0.156.0".to_string()),
        tui: "0.167.0".to_string(),
    };
    let unknown = Mismatch::Version {
        daemon: None,
        tui: "0.167.0".to_string(),
    };
    let upstream = Mismatch::Build {
        version: "0.167.0".to_string(),
    };
    for (mismatch, hint, expected) in [
        (
            &older,
            Hint::RestartFixes,
            "Running without the shared background server: it runs Codex 0.156.0, not this session's Codex 0.167.0. Run `codex app-server daemon restart` to switch it to this build.",
        ),
        (
            &older,
            Hint::InstallThisCli,
            "Running without the shared background server: it runs Codex 0.156.0, not this session's Codex 0.167.0. Run `codex app-server daemon update --from-cli` to install this build for it.",
        ),
        (
            &older,
            Hint::InstalledOther("0.156.0".to_string()),
            "Running without the shared background server: it runs Codex 0.156.0, not this session's Codex 0.167.0. Its installation holds Codex 0.156.0, so restarting it would not help.",
        ),
        (
            &upstream,
            Hint::Unmanaged,
            "Running without the shared background server: it runs another build of Codex 0.167.0. It is not managed by Codex; restart it where it was started.",
        ),
        (
            &unknown,
            Hint::None,
            "Running without the shared background server: it reports no Codex version, and this session runs Codex 0.167.0.",
        ),
    ] {
        assert_eq!(fallback_warning(mismatch, &hint), expected);
    }
}

#[test]
fn recovery_screen_defaults_to_running_without_daemon_and_confirms_restart() {
    let mismatch = Mismatch::Version {
        daemon: Some("0.156.0".to_string()),
        tui: "0.167.0".to_string(),
    };
    let keymap = RuntimeKeymap::defaults();
    let mut view = recovery_view(&mismatch, "0.167.0", &keymap);
    let area = Rect::new(
        /*x*/ 0,
        /*y*/ 0,
        /*width*/ 90,
        view.desired_height(/*width*/ 90),
    );
    let mut buffer = Buffer::empty(area);
    view.render(area, &mut buffer);
    let text = buffer
        .content
        .chunks(90)
        .map(|row| {
            row.iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("daemon_version_recovery_menu", text);
    view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(view.take_last_selected_index(), Some(0));

    let mut view = recovery_view(&mismatch, "0.167.0", &keymap);
    view.handle_key_event(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
    assert!(
        !view.is_complete(),
        "restart needs an explicit confirmation"
    );
    view.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(view.take_last_selected_index(), Some(1));

    let mut view = recovery_view(&mismatch, "0.167.0", &keymap);
    view.handle_key_event(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
    assert_eq!(view.take_last_selected_index(), Some(2));
}

#[test]
fn explicit_and_embedded_targets_have_no_daemon_socket_to_check() {
    let socket_path = codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path_checked(
        std::env::temp_dir().join("app-server-control.sock"),
    )
    .expect("absolute socket path");
    let daemon = AppServerTarget::LocalDaemon {
        endpoint: RemoteAppServerEndpoint::UnixSocket {
            socket_path: socket_path.clone(),
        },
        allow_embedded_fallback: false,
    };
    assert_eq!(
        daemon_socket(&daemon),
        Some(socket_path.as_path().to_path_buf())
    );
    for target in [
        AppServerTarget::Embedded,
        AppServerTarget::Remote {
            endpoint: RemoteAppServerEndpoint::UnixSocket { socket_path },
        },
    ] {
        assert_eq!(daemon_socket(&target), None);
    }
}
