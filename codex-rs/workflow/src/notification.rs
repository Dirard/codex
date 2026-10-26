//! Text of the completion notification delivered to the parent thread (UC7, R7.1, R5.3).

use crate::snapshot::RunSnapshot;
use crate::snapshot::RunStatus;

/// Null reasons named in the summary line.
const TOP_NULL_REASONS: usize = 3;

/// Tags around the notification in the parent's history (core `WorkflowNotification`).
pub const NOTIFICATION_OPEN_TAG: &str = "<workflow_notification>";
pub const NOTIFICATION_CLOSE_TAG: &str = "</workflow_notification>";

/// The run named by a recorded completion notification (UC8, R8.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSummary {
    pub run_id: String,
    pub name: String,
    /// Always terminal.
    pub status: RunStatus,
    /// The notification text without its tags.
    pub text: String,
}

/// UC8, R8.3: read a recorded notification fragment, the text of
/// [`render_notification`] between [`NOTIFICATION_OPEN_TAG`] and
/// [`NOTIFICATION_CLOSE_TAG`], back into its run id, name and terminal status.
pub fn parse_notification_fragment(fragment: &str) -> Option<NotificationSummary> {
    let text = fragment
        .trim()
        .strip_prefix(NOTIFICATION_OPEN_TAG)?
        .strip_suffix(NOTIFICATION_CLOSE_TAG)?
        .trim();
    // Names are letters, digits, `_` and `-`, so the quoted name ends at the first quote.
    let header = text.lines().next()?.strip_prefix("Workflow \"")?;
    let (name, rest) = header.split_once("\" (")?;
    let (run_id, rest) = rest.split_once(") ")?;
    let (status, _) = rest.split_once(" in ")?;
    let status = RunStatus::parse(status).filter(|status| status.is_terminal())?;
    Some(NotificationSummary {
        run_id: run_id.to_string(),
        name: name.to_string(),
        status,
        text: text.to_string(),
    })
}

/// UC7: render the self-contained notification for a terminal run. `preview` is the
/// already truncated result preview.
pub fn render_notification(snapshot: &RunSnapshot, preview: Option<&str>, now_ms: i64) -> String {
    let agents = &snapshot.agents;
    let mut reasons: Vec<(&String, &u64)> = snapshot.null_reasons.iter().collect();
    reasons.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    let null_summary = if agents.null_results == 0 {
        String::new()
    } else {
        let named = reasons
            .iter()
            .take(TOP_NULL_REASONS)
            .map(|(reason, count)| format!("{reason} {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(": {named}")
    };
    let mut text = format!(
        "Workflow \"{}\" ({}) {} in {}: {} agents ({} ok, {} null{null_summary}), {} tokens charged, {} cached input tokens.",
        snapshot.name,
        snapshot.run_id,
        snapshot.status.as_str(),
        format_duration(snapshot.duration_ms(now_ms)),
        agents.total(),
        agents.completed,
        agents.null_results,
        snapshot.charged_tokens,
        snapshot.cached_tokens,
    );
    if let Some(error) = &snapshot.error {
        text.push_str("\nError:\n");
        text.push_str(error);
    }
    if let Some(preview) = preview {
        text.push_str("\nResult preview:\n");
        text.push_str(preview);
        text.push_str(&format!(
            "\nFull result: call workflow_status with {{\"run_id\": \"{}\", \"include_result\": true}}.",
            snapshot.run_id
        ));
    }
    text
}

/// `1.2s`, `84.4s`, `12m 5s`.
pub fn format_duration(duration_ms: u64) -> String {
    if duration_ms < 60_000 {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    } else {
        let seconds = duration_ms / 1000;
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::TokenCharge;
    use crate::snapshot::RunStatus;
    use crate::snapshot::RunTracker;
    use pretty_assertions::assert_eq;

    /// R7.1, R5.3: counts, null reasons, charged and cached tokens and the preview.
    #[test]
    fn notification_summarizes_the_run() {
        let mut tracker = RunTracker::new(
            "wf-0a1b2c3d".to_string(),
            "find-bugs".to_string(),
            "d".to_string(),
            &[],
            None,
            0,
        );
        for seq in 1..=4 {
            tracker.agent_queued(seq, None, None);
        }
        tracker.agent_finished(
            1,
            true,
            None,
            TokenCharge {
                charged: 1500,
                cached: 4000,
            },
        );
        tracker.agent_finished(2, false, Some("timeout"), TokenCharge::default());
        tracker.agent_finished(3, false, Some("schema_invalid"), TokenCharge::default());
        tracker.agent_finished(4, false, Some("timeout"), TokenCharge::default());
        tracker.finish(RunStatus::Completed, 84_400, None, None);
        assert_eq!(
            render_notification(&tracker.snapshot(false), Some("{\"bugs\":1}"), 99_000),
            "Workflow \"find-bugs\" (wf-0a1b2c3d) completed in 1m 24s: 4 agents (1 ok, 3 null: timeout 2, schema_invalid 1), 1500 tokens charged, 4000 cached input tokens.\nResult preview:\n{\"bugs\":1}\nFull result: call workflow_status with {\"run_id\": \"wf-0a1b2c3d\", \"include_result\": true}."
        );
    }

    /// R8.3: a rendered notification wrapped in its tags reads back into the run it names.
    #[test]
    fn recorded_notification_names_its_run() {
        let mut tracker = RunTracker::new(
            "wf-0a1b2c3d".to_string(),
            "find-bugs_2".to_string(),
            "d".to_string(),
            &[],
            None,
            0,
        );
        tracker.finish(
            RunStatus::Failed,
            1_000,
            None,
            Some("boom in 1s".to_string()),
        );
        let text = render_notification(&tracker.snapshot(false), Some("in (x) \"y\""), 2_000);
        let fragment = format!("{NOTIFICATION_OPEN_TAG}\n{text}\n{NOTIFICATION_CLOSE_TAG}");
        assert_eq!(
            parse_notification_fragment(&fragment),
            Some(NotificationSummary {
                run_id: "wf-0a1b2c3d".to_string(),
                name: "find-bugs_2".to_string(),
                status: RunStatus::Failed,
                text,
            })
        );
        assert_eq!(
            parse_notification_fragment("Workflow \"x\" (wf-1) completed in 1.0s."),
            None
        );
        assert_eq!(
            parse_notification_fragment(&format!(
                "{NOTIFICATION_OPEN_TAG}\nWorkflow \"x\" (wf-1) running in 1.0s.\n{NOTIFICATION_CLOSE_TAG}"
            )),
            None,
            "a running run is never notified"
        );
    }

    #[test]
    fn durations_are_readable() {
        assert_eq!(format_duration(1_234), "1.2s");
        assert_eq!(format_duration(725_000), "12m 5s");
    }
}
