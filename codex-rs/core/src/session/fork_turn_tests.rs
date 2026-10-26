//! Fork R9 (docs/fork/02-compaction-context.md): the compaction progress guard checks and
//! counts only compactions the token limit forces.

use codex_protocol::error::CodexErrorDetails;
use pretty_assertions::assert_eq;

use super::SamplingOutcome;
use super::TurnLoop;

const FORCED: bool = true;
const OPTIONAL: bool = false;

/// Samples a response without progress, then compacts if the guard allows it.
fn compact_without_progress(turn_loop: &mut TurnLoop, token_limit_reached: bool) -> bool {
    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome::default(),
    );
    let allowed = turn_loop
        .check_compaction_progress(token_limit_reached)
        .is_ok();
    if allowed {
        turn_loop.on_compacted(token_limit_reached);
    }
    allowed
}

#[test]
fn third_forced_compaction_without_progress_fails() {
    let mut turn_loop = TurnLoop::default();
    assert!(compact_without_progress(&mut turn_loop, FORCED));
    assert!(compact_without_progress(&mut turn_loop, FORCED));

    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome::default(),
    );
    let error = turn_loop
        .check_compaction_progress(FORCED)
        .expect_err("third forced compaction without progress");
    assert!(matches!(
        error.details(),
        CodexErrorDetails::ContextWindowExceeded
    ));
}

#[test]
fn optional_compaction_neither_fails_nor_counts() {
    let mut turn_loop = TurnLoop::default();
    assert!(compact_without_progress(&mut turn_loop, FORCED));
    assert!(compact_without_progress(&mut turn_loop, FORCED));
    // The context still fits, so a ready candidate installs after the guard tripped.
    assert!(compact_without_progress(&mut turn_loop, OPTIONAL));

    let mut turn_loop = TurnLoop::default();
    for _ in 0..3 {
        assert!(compact_without_progress(&mut turn_loop, OPTIONAL));
    }
    // Optional compactions leave both forced attempts to the token limit.
    assert_eq!(
        [
            compact_without_progress(&mut turn_loop, FORCED),
            compact_without_progress(&mut turn_loop, FORCED),
            compact_without_progress(&mut turn_loop, FORCED),
        ],
        [true, true, false]
    );
}

/// Review R9-1 (docs/fork/reviews/2026-10-26-sandbox-compaction-review.md): optional
/// compactions in a row without progress stop starting until progress, and forced ones go on.
#[test]
fn optional_compactions_without_progress_stop_until_progress() {
    let mut turn_loop = TurnLoop::default();
    assert!(turn_loop.allows_optional_compaction());
    assert!(compact_without_progress(&mut turn_loop, OPTIONAL));
    assert!(turn_loop.allows_optional_compaction());
    assert!(compact_without_progress(&mut turn_loop, OPTIONAL));
    assert!(!turn_loop.allows_optional_compaction());

    // The token limit still forces its compactions, and they do not lift the stop.
    assert!(compact_without_progress(&mut turn_loop, FORCED));
    assert!(!turn_loop.allows_optional_compaction());

    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome {
            made_progress: true,
            answered: false,
        },
    );
    assert!(turn_loop.allows_optional_compaction());
}

#[test]
fn progress_resets_the_guard() {
    let mut turn_loop = TurnLoop::default();
    assert!(compact_without_progress(&mut turn_loop, FORCED));
    assert!(compact_without_progress(&mut turn_loop, FORCED));

    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome {
            made_progress: true,
            answered: false,
        },
    );
    assert!(turn_loop.check_compaction_progress(FORCED).is_ok());
}
