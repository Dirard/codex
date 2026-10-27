//! Fork R9 (docs/fork/02-compaction-context.md): the compaction progress guard checks and
//! counts only compactions the token limit forces. §4 item 11: the compactions of one turn
//! may replace only so much context (review 2026-10-27, P2-1, P3-1).

use codex_protocol::error::CodexErrorDetails;
use pretty_assertions::assert_eq;

use codex_protocol::config_types::AutoCompactTokenLimitScope;

use super::super::context_window::ContextWindowTokenStatus;
use super::DEFAULT_TURN_COMPACTION_TOKEN_LIMIT;
use super::SamplingOutcome;
use super::TurnCompactionLimit;
use super::TurnLoop;
use super::body_is_no_measure;
use super::replaced_context_tokens;
use super::replaced_history_estimate;

const FORCED: bool = true;
const OPTIONAL: bool = false;

/// The context of a typical compaction in these tests and the limit of a 272k window.
const CONTEXT_TOKENS: i64 = 200_000;
const WINDOW_272K: i64 = 258_400;

fn default_limit() -> TurnCompactionLimit {
    TurnCompactionLimit::resolve(/*configured*/ None, Some(WINDOW_272K))
}

/// Checks a compaction of `context_tokens` and installs it if the turn loop allows it.
fn compact(
    turn_loop: &mut TurnLoop,
    token_limit_reached: bool,
    context_tokens: i64,
    limit: TurnCompactionLimit,
) -> bool {
    let allowed = turn_loop
        .check_compaction_progress(token_limit_reached, context_tokens, limit)
        .is_ok_and(|compacts| compacts);
    if allowed {
        turn_loop.on_compacted(token_limit_reached);
    }
    allowed
}

/// Samples a response without progress, then compacts if the guard allows it.
fn compact_without_progress(turn_loop: &mut TurnLoop, token_limit_reached: bool) -> bool {
    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome::default(),
    );
    compact(
        turn_loop,
        token_limit_reached,
        CONTEXT_TOKENS,
        default_limit(),
    )
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
        .check_compaction_progress(FORCED, CONTEXT_TOKENS, default_limit())
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
    assert!(turn_loop.allows_optional_compaction(default_limit()));
    assert!(compact_without_progress(&mut turn_loop, OPTIONAL));
    assert!(turn_loop.allows_optional_compaction(default_limit()));
    assert!(compact_without_progress(&mut turn_loop, OPTIONAL));
    assert!(!turn_loop.allows_optional_compaction(default_limit()));

    // The token limit still forces its compactions, and they do not lift the stop.
    assert!(compact_without_progress(&mut turn_loop, FORCED));
    assert!(!turn_loop.allows_optional_compaction(default_limit()));

    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome {
            made_progress: true,
            answered: false,
        },
    );
    assert!(turn_loop.allows_optional_compaction(default_limit()));
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
    assert!(
        turn_loop
            .check_compaction_progress(FORCED, CONTEXT_TOKENS, default_limit())
            .is_ok_and(|compacts| compacts)
    );
}

/// Samples a response with progress, then compacts if the turn loop allows it.
fn compact_after_progress(
    turn_loop: &mut TurnLoop,
    token_limit_reached: bool,
    context_tokens: i64,
    limit: TurnCompactionLimit,
) -> bool {
    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome {
            made_progress: true,
            answered: false,
        },
    );
    compact(turn_loop, token_limit_reached, context_tokens, limit)
}

/// The limit counts context, not compactions: the larger of 8M tokens and 32 windows, unless
/// `model_auto_compact_turn_token_limit` sets it, where 0 turns it off.
#[test]
fn the_limit_is_eight_million_tokens_or_thirty_two_windows() {
    assert_eq!(
        TurnCompactionLimit::resolve(None, Some(32_000)),
        TurnCompactionLimit::tokens(DEFAULT_TURN_COMPACTION_TOKEN_LIMIT)
    );
    assert_eq!(
        TurnCompactionLimit::resolve(None, None),
        TurnCompactionLimit::tokens(DEFAULT_TURN_COMPACTION_TOKEN_LIMIT)
    );
    assert_eq!(
        TurnCompactionLimit::resolve(None, Some(WINDOW_272K)),
        TurnCompactionLimit::tokens(32 * 258_400)
    );
    assert_eq!(
        TurnCompactionLimit::resolve(None, Some(950_000)),
        TurnCompactionLimit::tokens(32 * 950_000)
    );
    assert_eq!(
        TurnCompactionLimit::resolve(Some(1_000_000), Some(WINDOW_272K)),
        TurnCompactionLimit::tokens(1_000_000)
    );
    assert_eq!(
        TurnCompactionLimit::resolve(Some(0), Some(WINDOW_272K)),
        TurnCompactionLimit::OFF
    );
}

/// Review 2026-10-27, P2-1: a productive turn on a 32k window (a local model, auto-compaction
/// at about 90%) compacts every ~28k tokens. A limit of 32 compactions stopped it after less
/// than a million tokens of context; the limit in tokens lets it compact about 280 times.
#[test]
fn a_long_turn_on_a_small_window_keeps_compacting() {
    let limit = TurnCompactionLimit::resolve(None, Some(30_400));
    let mut turn_loop = TurnLoop::default();
    for compaction in 0..250 {
        let token_limit_reached = compaction % 3 != 0;
        assert!(
            compact_after_progress(&mut turn_loop, token_limit_reached, 28_000, limit),
            "compaction {compaction}"
        );
    }
    assert!(turn_loop.allows_optional_compaction(limit));
}

/// docs/fork/02-compaction-context.md §4 item 11: progress resets the progress guards, so a
/// turn whose every step refills the context would compact without end. Once its compactions
/// replaced the limit, the next forced one stops the turn with an error that says so: on a
/// 272k window after 32 compactions of a full window.
#[test]
fn a_turn_that_keeps_refilling_its_context_stops() {
    let mut turn_loop = TurnLoop::default();
    for compaction in 0..32 {
        let token_limit_reached = compaction % 2 == 0;
        assert!(
            compact_after_progress(
                &mut turn_loop,
                token_limit_reached,
                WINDOW_272K,
                default_limit()
            ),
            "compaction {compaction}"
        );
    }
    assert!(!turn_loop.allows_optional_compaction(default_limit()));

    turn_loop.on_sampled(
        /*model_needs_follow_up*/ true,
        SamplingOutcome {
            made_progress: true,
            answered: false,
        },
    );
    let error = turn_loop
        .check_compaction_progress(FORCED, WINDOW_272K, default_limit())
        .expect_err("a forced compaction past the limit");
    assert!(matches!(
        error.details(),
        CodexErrorDetails::InvalidRequest(_)
    ));
    assert_eq!(
        error.to_string(),
        "Codex stopped this turn: its 32 compactions replaced 8268800 tokens of context, the most one turn may compact, and the context is full again. Send a new message to continue in a new turn, or raise model_auto_compact_turn_token_limit in config.toml (0 turns the limit off)."
    );
}

/// Review 2026-10-27, P3-1 (R9): past the limit, an optional compaction, such as a
/// `new_context` reset in `TokenBudget`, is skipped instead of failing the turn: the context
/// still fits. Only the compaction the token limit forces stops the turn.
#[test]
fn an_optional_compaction_past_the_limit_is_skipped() {
    let limit = TurnCompactionLimit::tokens(500_000);
    let mut turn_loop = TurnLoop::default();
    assert!(compact_after_progress(
        &mut turn_loop,
        FORCED,
        300_000,
        limit
    ));
    assert!(compact_after_progress(
        &mut turn_loop,
        OPTIONAL,
        300_000,
        limit
    ));

    for _ in 0..3 {
        assert!(
            !turn_loop
                .check_compaction_progress(OPTIONAL, 100_000, limit)
                .expect("an optional compaction past the limit does not fail the turn"),
        );
    }
    assert!(
        turn_loop
            .check_compaction_progress(FORCED, 300_000, limit)
            .is_err()
    );
}

/// `model_auto_compact_turn_token_limit = 0` turns the limit off; a configured limit counts
/// the context of each compaction, forced and optional, and each turn on its own.
#[test]
fn the_limit_follows_the_configuration_and_each_turn() {
    let mut turn_loop = TurnLoop::default();
    for _ in 0..200 {
        assert!(compact_after_progress(
            &mut turn_loop,
            FORCED,
            WINDOW_272K,
            TurnCompactionLimit::OFF
        ));
    }

    let limit = TurnCompactionLimit::resolve(Some(1_000_000), Some(WINDOW_272K));
    let mut turn_loop = TurnLoop::default();
    for compaction in 0..5 {
        assert!(
            compact_after_progress(&mut turn_loop, compaction % 2 == 0, CONTEXT_TOKENS, limit),
            "compaction {compaction}"
        );
    }
    assert!(!compact_after_progress(
        &mut turn_loop,
        FORCED,
        CONTEXT_TOKENS,
        limit
    ));
    // A compaction that does not install does not count.
    let mut turn_loop = TurnLoop::default();
    for _ in 0..10 {
        assert!(
            turn_loop
                .check_compaction_progress(FORCED, CONTEXT_TOKENS, limit)
                .is_ok_and(|compacts| compacts)
        );
    }
    assert!(compact_after_progress(
        &mut turn_loop,
        FORCED,
        CONTEXT_TOKENS,
        limit
    ));
    assert!(
        TurnLoop::default()
            .check_compaction_progress(FORCED, CONTEXT_TOKENS, limit)
            .is_ok_and(|compacts| compacts)
    );
}

/// Review of the fixes P3-6: with `body_after_prefix`, a compaction keeps the prefix of its
/// window (instructions, tools, the carried summary) and replaces only the body after it, so
/// only the body counts toward the turn's limit. A 100k prefix with a 20k body threshold would
/// otherwise reach the limit after a sixth of the context it allows.
#[test]
fn a_body_after_prefix_compaction_counts_only_the_body() {
    let status = ContextWindowTokenStatus {
        active_context_tokens: 120_000,
        auto_compact_scope_tokens: 20_000,
        auto_compact_scope_limit: Some(20_000),
        full_context_window_limit: Some(WINDOW_272K),
        base_window_tokens_remaining: Some(0),
        auto_compact_window_prefill_tokens: Some(100_000),
        full_context_window_limit_reached: false,
        token_limit_reached: true,
        turn_end_compaction_threshold_reached: false,
    };
    assert!(!body_is_no_measure(
        AutoCompactTokenLimitScope::BodyAfterPrefix,
        &status
    ));
    assert_eq!(
        replaced_context_tokens(
            AutoCompactTokenLimitScope::BodyAfterPrefix,
            &status,
            /*history_estimate*/ None
        ),
        20_000
    );
    assert_eq!(
        replaced_context_tokens(
            AutoCompactTokenLimitScope::Total,
            &status,
            /*history_estimate*/ None
        ),
        120_000
    );
}

/// Review of the fixes 2, N-3: the body understates a `body_after_prefix` compaction that the
/// full window forced (a large prefix filled it) or that no usage measured (no prefix, the
/// body reads 0). Such a compaction counts at least the estimated history it replaces, so the
/// turn's limit does not weaken by "window / body".
#[test]
fn a_compaction_the_body_does_not_measure_counts_the_replaced_history() {
    let full_window = ContextWindowTokenStatus {
        active_context_tokens: WINDOW_272K,
        auto_compact_scope_tokens: 5_000,
        auto_compact_scope_limit: Some(40_000),
        full_context_window_limit: Some(WINDOW_272K),
        base_window_tokens_remaining: Some(0),
        auto_compact_window_prefill_tokens: Some(WINDOW_272K - 5_000),
        full_context_window_limit_reached: true,
        token_limit_reached: true,
        turn_end_compaction_threshold_reached: false,
    };
    let scope = AutoCompactTokenLimitScope::BodyAfterPrefix;
    assert!(body_is_no_measure(scope, &full_window));
    assert_eq!(
        replaced_context_tokens(scope, &full_window, Some(200_000)),
        200_000
    );
    // The body counts when it is larger than the estimate.
    assert_eq!(
        replaced_context_tokens(scope, &full_window, Some(1_000)),
        5_000
    );

    let no_usage = ContextWindowTokenStatus {
        active_context_tokens: 90_000,
        auto_compact_scope_tokens: 0,
        auto_compact_window_prefill_tokens: None,
        full_context_window_limit_reached: false,
        ..full_window
    };
    assert!(body_is_no_measure(scope, &no_usage));
    assert_eq!(
        replaced_context_tokens(scope, &no_usage, Some(70_000)),
        70_000
    );
    // Without an estimate, the whole context counts.
    assert_eq!(replaced_context_tokens(scope, &no_usage, None), 90_000);
}

/// Review of the fixes 2, N-3: the estimate leaves out what the next window keeps, the base
/// instructions and tool declarations, and counts the rest as the token accounting does.
#[test]
fn the_replaced_history_leaves_out_instructions_and_tools() {
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    let message = |text: String| ResponseItem::Message {
        status: None,
        encrypted_content: None,
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    };
    let user = message("word ".repeat(400));
    let summary = message("summary ".repeat(200));
    let instructions = crate::context::ContextualUserFragment::into(
        crate::context::BaseInstructionsFragment("You are Codex. ".repeat(100)),
    );
    assert!(crate::context::BaseInstructionsFragment::text_from_item(&instructions).is_some());
    let expected = crate::context_manager::estimate_item_token_count(&user)
        + crate::context_manager::estimate_item_token_count(&summary);
    assert!(expected > 0);
    assert_eq!(
        replaced_history_estimate([&instructions, &summary, &user].into_iter()),
        expected
    );
}
