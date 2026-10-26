//! Fork: rows and header of the `/workflows` views (R8.6.2 to R8.6.5).

use super::*;
use crate::workflows::test_fixtures::NOW_MS;
use crate::workflows::test_fixtures::completed_run;
use crate::workflows::test_fixtures::failed_run;
use crate::workflows::test_fixtures::running_run;
use pretty_assertions::assert_eq;

fn thread_id() -> ThreadId {
    ThreadId::from_string("00000000-0000-0000-0000-00000000aaaa").expect("thread id")
}

fn rows(params: &SelectionViewParams) -> Vec<(String, String, bool, bool)> {
    params
        .items
        .iter()
        .map(|item| {
            let prefix: String = item
                .name_prefix_spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            (
                format!("{prefix}{}", item.name),
                item.description.clone().unwrap_or_default(),
                item.is_disabled,
                item.secondary_action.is_some(),
            )
        })
        .collect()
}

#[test]
fn run_list_shows_newest_first_and_cancels_only_active_runs() {
    let (running, completed, failed) = (running_run(), completed_run(), failed_run());
    let runs = sorted_runs([&completed, &running, &failed]);
    let params = run_list_params(thread_id(), &runs, &BTreeSet::new(), NOW_MS, None);
    assert_eq!(params.view_id, Some(RUN_LIST_VIEW_ID));
    assert_eq!(
        rows(&params),
        vec![
            (
                "● find-bugs".to_string(),
                "wf-1a2b3c4d · running · Verify · 3 running, 4 done · 1m 20s".to_string(),
                false,
                true,
            ),
            (
                "✗ refactor-plan".to_string(),
                "wf-12345678 · failed · 2 agents · 14s".to_string(),
                false,
                false,
            ),
            (
                "✓ triage".to_string(),
                "wf-0f0f0f0f · completed · 12 agents (1 null) · 3m 02s".to_string(),
                false,
                false,
            ),
        ]
    );
    assert!(params.items.iter().all(|item| !item.dismiss_on_select));

    let cancelling = BTreeSet::from(["wf-1a2b3c4d".to_string()]);
    let params = run_list_params(thread_id(), &runs, &cancelling, NOW_MS, Some(0));
    assert_eq!(
        rows(&params)[0],
        (
            "● find-bugs".to_string(),
            "wf-1a2b3c4d · cancelling… · Verify · 3 running, 4 done · 1m 20s".to_string(),
            false,
            false,
        )
    );
    assert_eq!(params.initial_selected_idx, Some(0));
}

#[test]
fn run_view_lists_a_cancel_row_and_every_agent_call() {
    let run = running_run();
    let params = run_view_params(thread_id(), &run, /*cancelling*/ false, NOW_MS, None);
    assert_eq!(params.view_id, Some(RUN_VIEW_ID));
    assert_eq!(
        run_subtitle(&run, /*cancelling*/ false, NOW_MS),
        "wf-1a2b3c4d · running · 1m 20s"
    );
    assert_eq!(
        rows(&params),
        vec![
            (
                "Cancel run".to_string(),
                "interrupt its agents and stop the script".to_string(),
                false,
                false,
            ),
            (
                "✓ #1 find src/a.py".to_string(),
                "Find · done · from journal".to_string(),
                true,
                false,
            ),
            (
                "✓ #2 find src/b.py".to_string(),
                "Find · done · 15.2K tokens".to_string(),
                false,
                false,
            ),
            (
                "✓ #3 find src/c.py".to_string(),
                "Find · done · from journal".to_string(),
                true,
                false,
            ),
            (
                "● #4 verify a.py:12".to_string(),
                "Verify · running · 3.1K tokens".to_string(),
                false,
                false,
            ),
            (
                "? #5 verify b.py:40".to_string(),
                "Verify · awaiting approval".to_string(),
                false,
                false,
            ),
            (
                "✗ #6 verify c.py:7".to_string(),
                "Verify · null: timeout · 9K tokens".to_string(),
                false,
                false,
            ),
            (
                "● #7".to_string(),
                "Verify · running".to_string(),
                false,
                false,
            ),
            (
                "○ #8 synthesis".to_string(),
                "Synthesize · queued".to_string(),
                true,
                false,
            ),
        ]
    );
    assert_eq!(
        run_view_rows(&run)[..3],
        [
            RunViewRow::Cancel,
            RunViewRow::Agent(1),
            RunViewRow::Agent(2)
        ]
    );

    let params = run_view_params(thread_id(), &run, /*cancelling*/ true, NOW_MS, None);
    assert_eq!(
        rows(&params)[0],
        (
            "Cancelling…".to_string(),
            "waiting for its agents to stop".to_string(),
            true,
            false,
        )
    );
    assert_eq!(
        run_subtitle(&run, /*cancelling*/ true, NOW_MS),
        "wf-1a2b3c4d · cancelling… · 1m 20s"
    );
}

#[test]
fn finished_runs_offer_no_cancel() {
    let run = failed_run();
    assert_eq!(run_view_rows(&run), vec![RunViewRow::NoAgents]);
    let params = run_view_params(thread_id(), &run, /*cancelling*/ false, NOW_MS, None);
    assert_eq!(
        rows(&params),
        vec![("No agents".to_string(), String::new(), true, false)]
    );
}

#[test]
fn run_header_names_phases_agents_tokens_resume_and_the_log_tail() {
    let text = |run: &WorkflowRun| {
        run_header_lines(run)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        text(&running_run()),
        vec![
            "Fan-out bug search, adversarial verification and synthesis",
            "Phases   ✓ Find 3/3 · ▸ Verify 1/4 · Synthesize 0/1",
            "Agents   1 queued · 2 running · 1 awaiting approval · 3 done · 1 null (timeout 1)",
            "Tokens   48.2K charged · 157K cached · budget 200K",
            "Resumed  from wf-0a0b0c0d: 2 of 3 journaled results reused, 1 unused",
            "Log      log line 3",
            "         log line 4",
            "         log line 5",
            "         log line 6",
            "         log line 7",
        ]
    );
    assert_eq!(
        text(&failed_run()),
        vec![
            "Agents   2 done",
            "Tokens   8K charged · 1K cached",
            "Error    TypeError: plan is undefined",
            "Log      planning",
        ]
    );
    assert_eq!(
        text(&completed_run()),
        vec![
            "Sort open issues",
            "Agents   11 done · 1 null (schema_invalid 1)",
            "Tokens   120K charged · 0 cached",
            "Result   {\"bugs\": 9,",
        ]
    );
}

#[test]
fn script_text_stays_on_one_line() {
    assert_eq!(one_line("  find\n\tbugs  now \r"), "find bugs now");
}
