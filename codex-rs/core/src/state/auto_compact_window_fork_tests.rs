//! Fork: a new compaction window starts with a fresh prefill baseline, and a prepared
//! window commits only over the window it was prepared from
//! (docs/fork/02-compaction-context.md F4).

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn advance_starts_with_a_fresh_prefill_baseline() {
    let mut window = AutoCompactWindow::new_with_ids(AutoCompactWindowIds::new_initial());
    window.ensure_server_observed_prefill_from_usage(&TokenUsage {
        input_tokens: 100_000,
        ..Default::default()
    });

    window.advance();
    window.set_estimated_prefill(/*tokens*/ 20_000);

    assert_eq!(
        window.snapshot(),
        AutoCompactWindowSnapshot {
            prefill_input_tokens: Some(20_000),
        }
    );
}

#[test]
fn committing_prepared_window_starts_with_a_fresh_prefill_baseline() {
    let mut window = AutoCompactWindow::new_with_ids(AutoCompactWindowIds::new_initial());
    window.ensure_server_observed_prefill_from_usage(&TokenUsage {
        input_tokens: 100_000,
        ..Default::default()
    });
    let prepared = window.prepare_advance();

    assert!(window.commit_prepared(prepared));
    window.set_estimated_prefill(/*tokens*/ 20_000);

    assert_eq!(
        window.snapshot(),
        AutoCompactWindowSnapshot {
            prefill_input_tokens: Some(20_000),
        }
    );
}

#[test]
fn stale_prepared_window_does_not_clear_current_prefill() {
    let mut window = AutoCompactWindow::new_with_ids(AutoCompactWindowIds::new_initial());
    let stale = window.prepare_advance();
    window.advance();
    window.set_estimated_prefill(/*tokens*/ 20_000);

    assert!(!window.commit_prepared(stale));
    assert_eq!(
        window.snapshot(),
        AutoCompactWindowSnapshot {
            prefill_input_tokens: Some(20_000),
        }
    );
}
