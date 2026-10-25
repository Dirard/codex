use super::*;
use pretty_assertions::assert_eq;

fn report(outcome: ExitOutcome, unread: &[u8]) -> ExitReport {
    ExitReport {
        outcome,
        duration: Duration::from_millis(2_500),
        unread_tail: unread.to_vec(),
        unread_earlier_bytes: 0,
        sandbox_denied: false,
    }
}

fn exited(code: i32) -> ExitOutcome {
    ExitOutcome::Exited {
        exit_code: Some(code),
    }
}

#[test]
fn delivers_once_whether_exit_or_arming_comes_first() {
    let notifier = ExitNotifier::new(1000, "make".to_string());
    assert!(notifier.on_exit(report(exited(0), b"ok\n")).is_none());
    assert!(!notifier.is_pending());
    let pending = notifier.arm().expect("arming after exit delivers");
    assert_eq!(pending.process_id, 1000);
    assert!(notifier.arm().is_none());
    assert!(notifier.on_exit(report(exited(0), b"")).is_none());

    let notifier = ExitNotifier::new(1001, "make".to_string());
    assert!(notifier.arm().is_none());
    assert!(notifier.is_pending());
    assert!(notifier.on_exit(report(exited(2), b"")).is_some());
    assert!(!notifier.is_pending());
    assert!(notifier.on_exit(report(exited(2), b"")).is_none());
    assert!(notifier.arm().is_none());
}

#[test]
fn observed_exit_and_unarmed_exit_never_deliver() {
    let notifier = ExitNotifier::new(1000, "make".to_string());
    assert!(notifier.arm().is_none());
    notifier.observe();
    assert!(!notifier.is_pending());
    assert!(notifier.on_exit(report(exited(0), b"")).is_none());

    // `exec_command` saw the exit itself, so the notifier was never armed.
    let notifier = ExitNotifier::new(1001, "make".to_string());
    assert!(notifier.on_exit(report(exited(0), b"")).is_none());
    assert!(!notifier.is_pending());
}

#[test]
fn shutdown_suppresses_and_codex_causes_replace_exit_codes() {
    let notifier = ExitNotifier::new(1000, "sleep 100".to_string());
    notifier.arm();
    notifier.set_termination_cause(TerminationCause::Shutdown);
    assert!(notifier.on_exit(report(exited(137), b"")).is_none());
    assert!(!notifier.is_pending());

    let notifier = ExitNotifier::new(1001, "sleep 100".to_string());
    notifier.arm();
    notifier.set_termination_cause(TerminationCause::ProcessLimit);
    notifier.set_termination_cause(TerminationCause::ClientRequest);
    let pending = notifier
        .on_exit(report(exited(137), b""))
        .expect("limit termination is reported");
    assert_eq!(
        pending.report.outcome,
        ExitOutcome::Terminated(TerminationCause::ProcessLimit)
    );
    assert!(pending.wakes());

    let notifier = ExitNotifier::new(1002, "sleep 100".to_string());
    notifier.arm();
    notifier.set_termination_cause(TerminationCause::ClientRequest);
    let pending = notifier
        .on_exit(report(exited(137), b""))
        .expect("reported");
    assert!(!pending.wakes());

    // A failure explains a Codex termination better than its cause.
    let notifier = ExitNotifier::new(1003, "curl example.com".to_string());
    notifier.arm();
    notifier.set_termination_cause(TerminationCause::ClientRequest);
    let failure = ExitOutcome::Failed {
        message: "Network access was denied by the Codex sandbox network proxy.".to_string(),
    };
    let pending = notifier
        .on_exit(report(failure.clone(), b""))
        .expect("failure is reported");
    assert_eq!(pending.report.outcome, failure);
    assert!(pending.wakes());
}

#[test]
fn renders_exit_command_and_unread_output() {
    let pending = PendingExitNotification {
        process_id: 1000,
        command: "cargo build".to_string(),
        report: report(exited(0), b"Compiling\nFinished\n\n"),
    };
    assert_eq!(
        pending.render(/*with_output*/ true),
        "Background terminal session 1000 exited with code 0 after 2.5s.\n\
         Command: cargo build\n\
         Unread output (last 2 lines):\n\
         Compiling\n\
         Finished"
    );
    assert_eq!(
        pending.render(/*with_output*/ false),
        "Background terminal session 1000 exited with code 0 after 2.5s.\n\
         Command: cargo build\n\
         Output omitted because many background terminal notifications are pending; call write_stdin with session_id 1000 to read it."
    );
}

#[test]
fn renders_signals_failures_terminations_and_sandbox_denials() {
    let render = |outcome, sandbox_denied| {
        PendingExitNotification {
            process_id: 7,
            command: "run".to_string(),
            report: ExitReport {
                sandbox_denied,
                ..report(outcome, b"")
            },
        }
        .render(/*with_output*/ true)
    };
    let signal = if cfg!(unix) { " (signal 9)" } else { "" };
    assert_eq!(
        render(exited(137), false),
        format!(
            "Background terminal session 7 exited with code 137{signal} after 2.5s.\nCommand: run\nNo unread output."
        )
    );
    assert_eq!(
        render(ExitOutcome::Exited { exit_code: None }, false),
        "Background terminal session 7 exited after 2.5s (exit code unavailable).\nCommand: run\nNo unread output."
    );
    assert_eq!(
        render(
            ExitOutcome::Failed {
                message: "denied\n".to_string()
            },
            false
        ),
        "Background terminal session 7 failed after 2.5s: denied\nCommand: run\nNo unread output."
    );
    assert_eq!(
        render(
            ExitOutcome::Terminated(TerminationCause::ProcessLimit),
            false
        ),
        "Background terminal session 7 was terminated by Codex after 2.5s because the limit of 64 open terminals was reached.\nCommand: run\nNo unread output."
    );
    assert_eq!(
        render(exited(1), true),
        "Background terminal session 7 exited with code 1 after 2.5s.\nCommand: run\nThe Codex sandbox likely blocked this command.\nNo unread output."
    );
}

#[test]
fn bounds_command_lines_and_utf8() {
    let mut output = Vec::new();
    // Start inside a two-byte scalar, as `last_bytes` can.
    output.extend_from_slice(&"é".as_bytes()[1..]);
    for line in 0..50 {
        output.extend_from_slice(format!("line {line}\n").as_bytes());
    }
    let pending = PendingExitNotification {
        process_id: 3,
        command: "x".repeat(2_000),
        report: ExitReport {
            unread_earlier_bytes: 100,
            ..report(exited(0), &output)
        },
    };
    let rendered = pending.render(/*with_output*/ true);
    let command_line = rendered.lines().nth(1).expect("command line");
    assert!(command_line.len() <= "Command: ".len() + NOTIFICATION_COMMAND_MAX_BYTES);
    assert!(command_line.contains("…[truncated]…"));
    let skipped: usize = 1
        + (0..10)
            .map(|line| format!("line {line}\n").len())
            .sum::<usize>();
    assert!(rendered.contains(&format!(
        "Earlier unread output omitted ({} bytes); call write_stdin with session_id 3 to read it.",
        100 + skipped
    )));
    assert!(rendered.contains("Unread output (last 40 lines):\nline 10\n"));
    assert!(rendered.ends_with("line 49"));
    assert!(!rendered.contains('\u{fffd}'));
}
