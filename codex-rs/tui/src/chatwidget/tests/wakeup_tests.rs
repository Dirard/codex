//! Fork: `/loop` and the display of scheduled wakeups
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC9).

use super::*;
use crate::wakeups::WakeupsEvent;
use codex_app_server_protocol::ThreadLoop;
use codex_app_server_protocol::ThreadWakeup;
use codex_app_server_protocol::ThreadWakeupCancelReason;
use codex_app_server_protocol::ThreadWakeupChange;
use codex_app_server_protocol::ThreadWakeupOrigin;
use codex_app_server_protocol::ThreadWakeupState;
use codex_app_server_protocol::ThreadWakeupUpdatedNotification;
use pretty_assertions::assert_eq;

fn app_events(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> Vec<AppEvent> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn history_text(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> String {
    drain_insert_history(rx)
        .iter()
        .flat_map(|lines| lines.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

async fn wakeup_chat() -> (ChatWidget, tokio::sync::mpsc::UnboundedReceiver<AppEvent>) {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    drain_insert_history(&mut rx);
    chat.set_feature_enabled(Feature::ScheduledWakeups, /*enabled*/ true);
    app_events(&mut rx);
    (chat, rx)
}

fn submit(chat: &mut ChatWidget, text: &str) {
    chat.bottom_pane
        .set_composer_text(text.to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn wakeup(due_at: i64, due: bool) -> ThreadWakeup {
    ThreadWakeup {
        id: "wk-1a2b3c4d".to_string(),
        origin: ThreadWakeupOrigin::Model,
        due_at,
        delay_seconds: 300,
        requested_delay_seconds: 300,
        due,
        reason: Some("CI still\u{7}running".to_string()),
        prompt: Some("check CI".to_string()),
        scheduled_in_turn_id: Some("turn-1".to_string()),
    }
}

fn notification(
    change: ThreadWakeupChange,
    cancel_reason: Option<ThreadWakeupCancelReason>,
    state: ThreadWakeupState,
) -> ThreadWakeupUpdatedNotification {
    ThreadWakeupUpdatedNotification {
        thread_id: String::new(),
        change,
        cancel_reason,
        turn_id: None,
        state,
    }
}

/// 12:05:00Z on 2026-11-07.
const DUE_AT: i64 = 1_794_053_100;

/// R9.1, R9.4: `/loop` exists only with the feature; it parses its arguments, sends no
/// request for a bad interval, and `/loop stop` works during a turn.
#[tokio::test]
async fn loop_command_follows_the_feature_and_parses() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    drain_insert_history(&mut rx);
    chat.bottom_pane
        .set_composer_text("/loo".to_string(), Vec::new(), Vec::new());
    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(!popup.contains("/loop"), "{popup}");
    chat.bottom_pane
        .set_composer_text(String::new(), Vec::new(), Vec::new());

    chat.set_feature_enabled(Feature::ScheduledWakeups, /*enabled*/ true);
    chat.bottom_pane
        .set_composer_text("/loo".to_string(), Vec::new(), Vec::new());
    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains("/loop") && popup.contains("repeat a prompt"),
        "{popup}"
    );
    app_events(&mut rx);

    submit(&mut chat, "/loop 5m check the deploy");
    let events = app_events(&mut rx);
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::Wakeups(WakeupsEvent::StartLoop { interval_s: Some(300), prompt })
                if prompt == "check the deploy"
        )),
        "{events:?}"
    );

    submit(&mut chat, "/loop 30s check");
    let events = app_events(&mut rx);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::Wakeups(_))),
        "{events:?}"
    );

    chat.bottom_pane.set_task_running(/*running*/ true);
    submit(&mut chat, "/loop stop");
    let events = app_events(&mut rx);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AppEvent::Wakeups(WakeupsEvent::Stop))),
        "{events:?}"
    );
}

/// R9.2, R9.3, R10.4: the status line shows the wakeup or the loop; a fired wakeup, a loop's
/// start and end and a user cancel each add one info cell; a model cancel adds none.
#[tokio::test]
async fn status_line_and_cells_follow_updates() {
    let (mut chat, mut rx) = wakeup_chat().await;
    let scheduled = ThreadWakeupState {
        wakeup: Some(wakeup(DUE_AT, /*due*/ false)),
        ..Default::default()
    };
    chat.on_wakeup_updated(notification(
        ThreadWakeupChange::Scheduled,
        None,
        scheduled.clone(),
    ));
    assert_eq!(
        chat.bottom_pane.workflow_status_indicator(),
        Some("⏰ wakeup at 12:05:00Z · CI still?running")
    );
    assert_eq!(history_text(&mut rx), "");

    chat.on_wakeup_updated(notification(
        ThreadWakeupChange::Fired,
        None,
        ThreadWakeupState::default(),
    ));
    assert_eq!(chat.bottom_pane.workflow_status_indicator(), None);
    assert!(history_text(&mut rx).contains("⏰ Scheduled wakeup fired: CI still?running"),);

    let looping = ThreadWakeupState {
        wakeup: Some(ThreadWakeup {
            origin: ThreadWakeupOrigin::Loop,
            reason: None,
            ..wakeup(DUE_AT, /*due*/ false)
        }),
        loop_state: Some(ThreadLoop {
            id: "loop-9f8e7d6c".to_string(),
            prompt: "check the deploy".to_string(),
            interval_seconds: Some(300),
            iteration: 3,
            max_iterations: 50,
            started_at: DUE_AT - 3_600,
            ends_at: DUE_AT + 3_600,
        }),
        ..Default::default()
    };
    chat.on_wakeup_updated(notification(ThreadWakeupChange::LoopStarted, None, looping));
    assert_eq!(
        chat.bottom_pane.workflow_status_indicator(),
        Some("⏰ /loop 3/50 · next at 12:05:00Z")
    );
    assert!(history_text(&mut rx).contains("⏰ /loop started: every 5m, at most 50 iterations"));
    chat.on_wakeup_updated(notification(
        ThreadWakeupChange::LoopEnded,
        Some(ThreadWakeupCancelReason::Completed),
        ThreadWakeupState::default(),
    ));
    assert!(
        history_text(&mut rx)
            .contains("⏰ /loop ended after 3 iterations: the model did not schedule the next one")
    );

    chat.on_wakeup_updated(notification(
        ThreadWakeupChange::Scheduled,
        None,
        scheduled.clone(),
    ));
    chat.on_wakeup_updated(notification(
        ThreadWakeupChange::Cancelled,
        Some(ThreadWakeupCancelReason::Model),
        ThreadWakeupState::default(),
    ));
    assert_eq!(history_text(&mut rx), "");
    chat.on_wakeup_updated(notification(ThreadWakeupChange::Scheduled, None, scheduled));
    chat.on_wakeup_updated(notification(
        ThreadWakeupChange::Cancelled,
        Some(ThreadWakeupCancelReason::Interrupt),
        ThreadWakeupState::default(),
    ));
    assert!(
        history_text(&mut rx).contains("⏰ Scheduled wakeup cancelled: you interrupted its turn")
    );
}
