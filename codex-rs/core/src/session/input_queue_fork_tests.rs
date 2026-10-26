//! Fork: tests of the background notification queue and the model-step rule
//! (`input_queue_fork.rs`), moved out of the upstream `input_queue` tests unchanged.

use super::super::*;
use super::*;
use codex_protocol::AgentPath;
use pretty_assertions::assert_eq;

// Copy of the upstream test helper in `input_queue.rs`.
fn make_mail(
    author: AgentPath,
    recipient: AgentPath,
    content: &str,
    trigger_turn: bool,
) -> InterAgentCommunication {
    InterAgentCommunication::new(
        author,
        recipient,
        Vec::new(),
        content.to_string(),
        trigger_turn,
    )
}

/// Fork: the one rule for which input keeps a batch with a blocked prompt going.
#[test]
fn needs_model_step_covers_prompts_tool_output_and_turn_mail() {
    let prompt = |content: Vec<UserInput>| TurnInput::UserInput {
        content,
        client_id: None,
        metadata: UserInputMetadata::default(),
    };
    let item = |value: serde_json::Value| {
        ResponseItemEnvelope::new(serde_json::from_value(value).expect("response item"))
    };
    let mail = |trigger_turn| {
        TurnInput::InterAgentCommunication(
            make_mail(AgentPath::root(), AgentPath::root(), "task", trigger_turn).into(),
        )
    };
    let cases = [
        (
            "prompt",
            prompt(vec![UserInput::Text {
                text: "hello".to_string(),
                text_elements: Vec::new(),
            }]),
            true,
        ),
        ("empty prompt", prompt(Vec::new()), false),
        (
            "standalone tool output",
            TurnInput::FunctionCallOutput(item(serde_json::json!({
                "type": "function_call_output", "name": "send", "output": "done",
            }))),
            true,
        ),
        (
            "late function output",
            TurnInput::ResponseItem(item(serde_json::json!({
                "type": "function_call_output", "call_id": "call", "output": "done",
            }))),
            true,
        ),
        (
            "code-mode notify output",
            TurnInput::ResponseItem(item(serde_json::json!({
                "type": "custom_tool_call_output", "call_id": "call", "name": "exec",
                "output": "progress",
            }))),
            true,
        ),
        (
            "tool search output",
            TurnInput::ResponseItem(item(serde_json::json!({
                "type": "tool_search_output", "call_id": "call", "execution": "client",
                "status": "completed", "tools": [],
            }))),
            true,
        ),
        (
            "additional context",
            TurnInput::ResponseItem(item(serde_json::json!({
                "type": "message", "role": "developer",
                "content": [{"type": "input_text", "text": "context"}],
            }))),
            false,
        ),
        ("mail that asks for a turn", mail(true), true),
        ("queue-only mail", mail(false), false),
    ];
    for (name, input, expected) in cases {
        assert_eq!(input.needs_model_step(), expected, "{name}");
    }
}

fn background_notification(body: &str) -> ResponseItem {
    ContextualUserFragment::into(BackgroundTerminalNotification::new(body))
}

/// Fork: background terminal notifications wait at thread level and wake waits.
#[tokio::test]
async fn background_notifications_queue_after_mail_and_report_activity() {
    let input_queue = InputQueue::new();
    let (mut activity_rx, pending_activity) =
        input_queue.subscribe_activity(/*turn_state*/ None).await;
    assert_eq!(pending_activity, None);
    assert!(!input_queue.has_pending_queued_input().await);

    let mail = make_mail(
        AgentPath::try_from("/root/worker").expect("agent path"),
        AgentPath::root(),
        "mail",
        /*trigger_turn*/ false,
    );
    input_queue
        .enqueue_background_notification(
            /*key*/ None,
            /*session_id*/ None,
            /*wakes*/ false,
            |with_output| {
                assert!(with_output);
                background_notification("passive")
            },
        )
        .await;
    input_queue
        .enqueue_mailbox_communication(
            mail.clone().into(),
            Default::default(),
            /*turn_spawn_budget*/ None,
        )
        .await;
    activity_rx.changed().await.expect("activity");
    assert!(input_queue.has_pending_queued_input().await);
    assert!(!input_queue.has_waking_background_notifications().await);
    assert!(!input_queue.has_trigger_turn_mailbox_items().await);
    assert_eq!(
        input_queue.subscribe_activity(/*turn_state*/ None).await.1,
        Some(InputQueueActivity::BackgroundTerminal)
    );

    input_queue
        .enqueue_background_notification(
            /*key*/ None,
            /*session_id*/ None,
            /*wakes*/ true,
            |_| background_notification("waking"),
        )
        .await;
    assert!(input_queue.has_waking_background_notifications().await);

    let (items, _, _) = input_queue.drain_mailbox_input_items().await;
    assert_eq!(
        items,
        vec![
            TurnInput::InterAgentCommunication(mail.into()),
            TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                "passive"
            ))),
            TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification("waking"))),
        ]
    );
    assert!(items[1].needs_model_step());
    assert!(!input_queue.has_pending_queued_input().await);
    assert!(!input_queue.has_waking_background_notifications().await);
}

/// Fork: many pending notifications drop their output, then collapse into a summary.
#[tokio::test]
async fn background_notifications_are_bounded() {
    let input_queue = InputQueue::new();
    let mut with_output_flags = Vec::new();
    let mut outcomes = Vec::new();
    for index in 0..MAX_PENDING_BACKGROUND_NOTIFICATIONS + 2 {
        let session_id = 1000 + i32::try_from(index).expect("small index");
        outcomes.push(
            input_queue
                .enqueue_background_notification(
                    /*key*/ None,
                    Some(session_id),
                    /*wakes*/ index == MAX_PENDING_BACKGROUND_NOTIFICATIONS + 1,
                    |with_output| {
                        with_output_flags.push(with_output);
                        background_notification(&format!("notification {index}"))
                    },
                )
                .await,
        );
    }
    assert_eq!(
        outcomes[MAX_PENDING_BACKGROUND_NOTIFICATIONS - 1..],
        [
            BackgroundNotificationEnqueue::Queued,
            BackgroundNotificationEnqueue::Dropped,
            BackgroundNotificationEnqueue::Dropped,
        ]
    );
    assert_eq!(
        with_output_flags.len(),
        MAX_PENDING_BACKGROUND_NOTIFICATIONS
    );
    assert_eq!(
        with_output_flags.iter().filter(|flag| **flag).count(),
        DETAILED_PENDING_BACKGROUND_NOTIFICATIONS
    );
    // The dropped notification still asks for a turn.
    assert!(input_queue.has_waking_background_notifications().await);

    let (items, _, _) = input_queue.drain_mailbox_input_items().await;
    assert_eq!(items.len(), MAX_PENDING_BACKGROUND_NOTIFICATIONS + 1);
    assert_eq!(
        items.last(),
        Some(&TurnInput::ResponseItem(ResponseItemEnvelope::new(
            background_notification(
                "2 more background terminals stopped while too many notifications were pending (sessions 1032, 1033); call write_stdin with one of these session IDs to read the exit code and output of a terminal that exited on its own."
            )
        )))
    );
    assert!(!input_queue.has_pending_queued_input().await);
}

/// Fork (R3.4): the summary lists a bounded number of dropped sessions.
#[test]
fn dropped_summary_lists_a_bounded_number_of_sessions() {
    let ids = (1000..1000 + MAX_LISTED_DROPPED_SESSIONS as i32).collect::<Vec<_>>();
    let summary = dropped_summary(MAX_LISTED_DROPPED_SESSIONS + 3, &ids);
    assert!(
        summary.starts_with("19 more background terminals stopped while too many notifications were pending (sessions 1000, 1001,"),
        "{summary}"
    );
    assert!(
        summary.contains("1015 and 3 more); call write_stdin"),
        "{summary}"
    );
    assert_eq!(
        dropped_summary(2, &[]),
        "2 more background terminals stopped while too many notifications were pending; call write_stdin with one of these session IDs to read the exit code and output of a terminal that exited on its own."
    );
}

/// Fork (R2.10): a poll that showed the exit withdraws exactly its own notification.
#[tokio::test]
async fn withdraw_removes_only_the_keyed_notification() {
    let input_queue = InputQueue::new();
    for (key, body) in [(Some(7), "seven"), (Some(8), "eight"), (None, "summary")] {
        input_queue
            .enqueue_background_notification(
                key,
                /*session_id*/ None,
                /*wakes*/ true,
                |_| background_notification(body),
            )
            .await;
    }
    assert!(input_queue.withdraw_background_notification(7).await);
    assert!(!input_queue.withdraw_background_notification(7).await);
    assert_eq!(
        input_queue.take_background_notifications().await,
        vec![
            background_notification("eight"),
            background_notification("summary")
        ]
    );
}

/// Fork (R2.12): a turn that drains a notification runs its release callback; a
/// withdrawn notification drops it unrun.
#[tokio::test]
async fn draining_a_notification_runs_its_release_callback() {
    let input_queue = InputQueue::new();
    let released = Arc::new(std::sync::Mutex::new(Vec::new()));
    for (key, body) in [(1, "sampled"), (2, "withdrawn")] {
        let released = Arc::clone(&released);
        input_queue
            .enqueue_background_notification(
                Some(key),
                /*session_id*/ None,
                /*wakes*/ true,
                move |_| QueuedBackgroundNotification {
                    item: background_notification(body),
                    on_sampled: Some(Box::new(move || {
                        released.lock().expect("released").push(key);
                    })),
                },
            )
            .await;
    }
    assert!(input_queue.withdraw_background_notification(2).await);
    assert!(released.lock().expect("released").is_empty());
    let (items, _, _) = input_queue.drain_mailbox_input_items().await;
    assert_eq!(items.len(), 1);
    assert_eq!(*released.lock().expect("released"), vec![1]);
}

/// Fork (R3.7): an abort returns the turn's background terminal notifications to the
/// thread queue without a wake and drops the rest of its input as before.
#[tokio::test]
async fn clear_pending_returns_background_notifications_to_the_thread_queue() {
    let input_queue = InputQueue::new();
    input_queue
        .enqueue_background_notification(
            /*key*/ None,
            /*session_id*/ None,
            /*wakes*/ false,
            |_| background_notification("still queued"),
        )
        .await;
    let active_turn = ActiveTurn::default();
    input_queue
        .extend_pending_input_for_turn_state(
            active_turn.turn_state.as_ref(),
            vec![
                TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                    "taken by the aborted turn",
                ))),
                TurnInput::UserInput {
                    content: vec![UserInput::Text {
                        text: "dropped".to_string(),
                        text_elements: Vec::new(),
                    }],
                    client_id: None,
                    metadata: UserInputMetadata::default(),
                },
            ],
        )
        .await;

    input_queue.clear_pending(&active_turn).await;

    assert!(active_turn.turn_state.lock().await.pending_input.is_empty());
    assert!(!input_queue.has_waking_background_notifications().await);
    assert_eq!(
        input_queue.take_background_notifications().await,
        vec![
            background_notification("taken by the aborted turn"),
            background_notification("still queued"),
        ]
    );
}

fn workflow_notification(body: &str) -> ResponseItem {
    ContextualUserFragment::into(crate::context::WorkflowNotification::new(body))
}

/// Fork (dynamic workflow R7.7, B14): an abort returns a workflow notification from the
/// turn's input too, as waking, and names the workflow activity; a background terminal
/// notification still returns without a wake.
#[tokio::test]
async fn clear_pending_returns_workflow_notifications_as_waking() {
    let input_queue = InputQueue::new();
    let (mut activity_rx, _) = input_queue.subscribe_activity(/*turn_state*/ None).await;
    let active_turn = ActiveTurn::default();
    input_queue
        .extend_pending_input_for_turn_state(
            active_turn.turn_state.as_ref(),
            vec![
                TurnInput::ResponseItem(ResponseItemEnvelope::new(workflow_notification(
                    "Workflow \"x\" (wf-1) completed in 1.0s.",
                ))),
                TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                    "exited",
                ))),
            ],
        )
        .await;

    input_queue.clear_pending(&active_turn).await;

    activity_rx.changed().await.expect("activity");
    assert_eq!(*activity_rx.borrow(), InputQueueActivity::Workflow);
    assert!(input_queue.has_waking_background_notifications().await);
    assert_eq!(
        input_queue.take_background_notifications().await,
        vec![
            workflow_notification("Workflow \"x\" (wf-1) completed in 1.0s."),
            background_notification("exited"),
        ]
    );
}

/// Fork (R3.8, dynamic workflow R7.7): a task other than a regular turn takes mail but
/// leaves background notifications queued with their wake, key and release callback.
#[tokio::test]
async fn non_regular_tasks_leave_background_notifications_queued() {
    let input_queue = InputQueue::new();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let release_flag = Arc::clone(&released);
    input_queue
        .enqueue_background_notification(Some(7), Some(1000), /*wakes*/ true, move |_| {
            QueuedBackgroundNotification {
                item: background_notification("exited"),
                on_sampled: Some(Box::new(move || {
                    release_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                })),
            }
        })
        .await;
    input_queue
        .enqueue_workflow_notification(workflow_notification(
            "Workflow \"x\" (wf-1) completed in 1.0s.",
        ))
        .await;
    let mail = make_mail(
        AgentPath::try_from("/root/worker").expect("agent path"),
        AgentPath::root(),
        "mail",
        /*trigger_turn*/ false,
    );
    input_queue
        .enqueue_mailbox_communication(
            mail.clone().into(),
            Default::default(),
            /*turn_spawn_budget*/ None,
        )
        .await;

    let (items, _, _) = input_queue
        .drain_mailbox_input_items_for_task(/*take_background_notifications*/ false)
        .await;
    assert_eq!(items, vec![TurnInput::InterAgentCommunication(mail.into())]);
    assert!(input_queue.has_waking_background_notifications().await);
    assert!(!released.load(std::sync::atomic::Ordering::SeqCst));
    // The withdrawal key survived.
    assert!(input_queue.withdraw_background_notification(7).await);

    let (items, _, _) = input_queue.drain_mailbox_input_items().await;
    assert_eq!(
        items,
        vec![TurnInput::ResponseItem(ResponseItemEnvelope::new(
            workflow_notification("Workflow \"x\" (wf-1) completed in 1.0s.")
        ))]
    );
}

/// Fork (dynamic workflow R7.8): workflow notifications do not count toward the limits
/// of background terminal notifications.
#[tokio::test]
async fn workflow_notifications_do_not_count_toward_terminal_limits() {
    let input_queue = InputQueue::new();
    for index in 0..MAX_PENDING_BACKGROUND_NOTIFICATIONS {
        input_queue
            .enqueue_workflow_notification(workflow_notification(&format!(
                "Workflow \"w{index}\" (wf-{index}) completed in 1.0s."
            )))
            .await;
    }
    let mut with_output = None;
    let outcome = input_queue
        .enqueue_background_notification(
            /*key*/ None,
            Some(1000),
            /*wakes*/ true,
            |output| {
                with_output = Some(output);
                background_notification("exited")
            },
        )
        .await;
    assert_eq!(outcome, BackgroundNotificationEnqueue::Queued);
    assert_eq!(with_output, Some(true));
    let items = input_queue.take_background_notifications().await;
    assert_eq!(items.len(), MAX_PENDING_BACKGROUND_NOTIFICATIONS + 1);
    assert_eq!(items.last(), Some(&background_notification("exited")));
}

/// Fork (R6.7, R6.10): an output watch keeps one notification in the queue and later events
/// replace it; it wakes the thread, is never dropped and does not count toward the limits
/// of exit notifications.
#[tokio::test]
async fn output_watch_notifications_replace_their_queued_entry_and_are_not_counted() {
    let input_queue = InputQueue::new();
    for index in 0..MAX_PENDING_BACKGROUND_NOTIFICATIONS - 1 {
        input_queue
            .enqueue_background_notification(
                /*key*/ None,
                Some(1000 + i32::try_from(index).expect("small index")),
                /*wakes*/ false,
                |_| background_notification(&format!("exited {index}")),
            )
            .await;
    }
    let mut seen = Vec::new();
    let changed = input_queue
        .offer_output_notification(/*key*/ 7, |queued| {
            seen.push(queued);
            Some(background_notification("ready"))
        })
        .await;
    assert!(changed);
    assert!(input_queue.has_waking_background_notifications().await);
    let changed = input_queue
        .offer_output_notification(/*key*/ 7, |queued| {
            seen.push(queued);
            Some(background_notification("ready, then ERROR"))
        })
        .await;
    assert!(changed);
    let unchanged = input_queue
        .offer_output_notification(/*key*/ 7, |queued| {
            seen.push(queued);
            None
        })
        .await;
    assert!(!unchanged);
    assert_eq!(seen, vec![false, true, true]);

    // Only exit notifications count: the last free place still takes one, the next drops.
    for (session_id, expected) in [
        (2000, BackgroundNotificationEnqueue::Queued),
        (2001, BackgroundNotificationEnqueue::Dropped),
    ] {
        let outcome = input_queue
            .enqueue_background_notification(
                /*key*/ None,
                Some(session_id),
                /*wakes*/ false,
                |_| background_notification("exited"),
            )
            .await;
        assert_eq!(outcome, expected);
    }

    let (items, _, _) = input_queue.drain_mailbox_input_items().await;
    let output_items = items
        .iter()
        .filter(|item| {
            matches!(item, TurnInput::ResponseItem(envelope)
                if envelope.item == background_notification("ready, then ERROR"))
        })
        .count();
    assert_eq!(output_items, 1);
    assert_eq!(items.len(), MAX_PENDING_BACKGROUND_NOTIFICATIONS + 2);

    // Once a turn took it, the next events make a new notification.
    let mut queued_after_drain = None;
    input_queue
        .offer_output_notification(/*key*/ 7, |queued| {
            queued_after_drain = Some(queued);
            Some(background_notification("ERROR again"))
        })
        .await;
    assert_eq!(queued_after_drain, Some(false));
}

#[tokio::test]
async fn turn_local_background_notification_reports_its_activity() {
    let input_queue = InputQueue::new();
    let turn_state = Mutex::new(TurnState::default());
    input_queue
        .extend_pending_input_for_turn_state(
            &turn_state,
            vec![TurnInput::ResponseItem(ResponseItemEnvelope::new(
                background_notification("exited"),
            ))],
        )
        .await;
    assert_eq!(
        input_queue.subscribe_activity(Some(&turn_state)).await.1,
        Some(InputQueueActivity::BackgroundTerminal)
    );
}

/// Fork (H5): a wake that lost its turn reservation puts its mail and background
/// notifications back in front of the thread queues. Mail that asks for a turn carries the
/// wake's start options; other input of the reservation is dropped.
#[tokio::test]
async fn lost_wake_input_returns_to_the_front_of_the_thread_queues() {
    let input_queue = InputQueue::new();
    let later_mail = make_mail(
        AgentPath::root(),
        AgentPath::root(),
        "later mail",
        /*trigger_turn*/ false,
    );
    input_queue
        .enqueue_mailbox_communication(
            later_mail.clone().into(),
            Default::default(),
            /*turn_spawn_budget*/ None,
        )
        .await;
    input_queue
        .enqueue_background_notification(
            /*key*/ None,
            /*session_id*/ None,
            /*wakes*/ false,
            |_| background_notification("later notification"),
        )
        .await;
    let lost_trigger_mail = make_mail(
        AgentPath::root(),
        AgentPath::root(),
        "lost trigger mail",
        /*trigger_turn*/ true,
    );
    let lost_queued_mail = make_mail(
        AgentPath::root(),
        AgentPath::root(),
        "lost queue-only mail",
        /*trigger_turn*/ false,
    );

    input_queue
        .requeue_lost_wake_input(
            vec![
                TurnInput::InterAgentCommunication(lost_trigger_mail.clone().into()),
                TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                    "lost notification",
                ))),
                TurnInput::InterAgentCommunication(lost_queued_mail.clone().into()),
                TurnInput::UserInput {
                    content: vec![UserInput::Text {
                        text: "dropped".to_string(),
                        text_elements: Vec::new(),
                    }],
                    client_id: None,
                    metadata: UserInputMetadata::default(),
                },
            ],
            TurnStartOptions {
                turn_trigger: Some("agent_message".to_string()),
                parent_turn_id: Some("parent-turn".to_string()),
                root_turn_id: Some("root-turn".to_string()),
                ..Default::default()
            },
        )
        .await;

    assert!(input_queue.has_trigger_turn_mailbox_items().await);
    let (items, start_options, _) = input_queue.drain_mailbox_input_items().await;
    assert_eq!(
        items,
        vec![
            TurnInput::InterAgentCommunication(lost_trigger_mail.into()),
            TurnInput::InterAgentCommunication(lost_queued_mail.into()),
            TurnInput::InterAgentCommunication(later_mail.into()),
            TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                "lost notification"
            ))),
            TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                "later notification"
            ))),
        ]
    );
    assert_eq!(
        (
            start_options.turn_trigger.as_deref(),
            start_options.parent_turn_id.as_deref(),
            start_options.root_turn_id.as_deref(),
        ),
        (
            Some("agent_message"),
            Some("parent-turn"),
            Some("root-turn")
        )
    );
}
