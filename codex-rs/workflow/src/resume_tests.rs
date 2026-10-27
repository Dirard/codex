use super::*;
use crate::journal::TokenCharge;
use pretty_assertions::assert_eq;
use serde_json::json;

fn finished(
    seq: u64,
    key: &str,
    ordinal: u64,
    value: Option<Value>,
    cached: bool,
) -> JournalRecord {
    JournalRecord {
        seq,
        event: JournalEvent::AgentFinished {
            call: Some(seq),
            key: key.to_string(),
            ordinal,
            label: None,
            phase: None,
            ok: value.is_some(),
            reason: value.is_none().then(|| "timeout".to_string()),
            value,
            tokens: TokenCharge::default(),
            cached,
            worktree: None,
            worktree_details: None,
        },
    }
}

/// R6.3, R6.5: only successful results are cached, by key and ordinal, cached ones included;
/// each pair is served once and the counts follow.
#[test]
fn cache_serves_successful_results_by_key_and_ordinal() {
    let records = vec![
        finished(1, "a", 0, Some(json!("first a")), false),
        finished(2, "a", 1, Some(json!("second a")), false),
        finished(3, "b", 0, None, false),
        finished(4, "c", 0, Some(json!({"x": 1})), true),
        JournalRecord {
            seq: 5,
            event: JournalEvent::Log {
                message: "noise".to_string(),
            },
        },
    ];
    let mut cache = JournalCache::from_records("wf-00000001", &records);
    assert_eq!(cache.stats().source_results, 3);
    assert_eq!(
        cache.take("a", 1).map(|answer| answer.value),
        Some(json!("second a"))
    );
    assert_eq!(cache.take("a", 1), None, "a pair is served once");
    assert_eq!(cache.take("b", 0), None, "null outcomes run again");
    assert_eq!(
        cache.take("c", 0).map(|answer| answer.value),
        Some(json!({"x": 1}))
    );
    assert_eq!(cache.take("a", 2), None);
    assert_eq!(
        cache.stats(),
        ResumeStats {
            from: "wf-00000001".to_string(),
            source_results: 3,
            served: 2,
            unused: 1,
        }
    );
}

/// R6.3, R3.8: an isolated agent's worktree report travels with its cached answer; a
/// journal that predates the report has none.
#[test]
fn cached_answers_keep_the_worktree_report() {
    let report = "<agent_worktree>\nbranch: b\n</agent_worktree>".to_string();
    let mut reported = finished(1, "w", 0, Some(json!("isolated")), false);
    if let JournalEvent::AgentFinished { worktree, .. } = &mut reported.event {
        *worktree = Some(report.clone());
    }
    let records = vec![reported, finished(2, "w", 1, Some(json!("older")), false)];
    let mut cache = JournalCache::from_records("wf-00000001", &records);
    assert_eq!(
        cache.take("w", 0),
        Some(CachedAnswer {
            value: json!("isolated"),
            worktree: Some(report),
            worktree_details: None,
        })
    );
    assert_eq!(
        cache.take("w", 1),
        Some(CachedAnswer {
            value: json!("older"),
            worktree: None,
            worktree_details: None,
        })
    );
}

/// R6.3, R3.13: the structured worktree details travel with the cached answer, so a resumed
/// call with `details: true` returns them; a record without them gives `None`.
#[test]
fn cached_answers_keep_the_worktree_details() {
    let details = json!({ "agent": "/root/wf_1_1", "branch": "codex/agents/x", "changed": true });
    let mut reported = finished(1, "w", 0, Some(json!("isolated")), false);
    if let JournalEvent::AgentFinished {
        worktree,
        worktree_details,
        ..
    } = &mut reported.event
    {
        *worktree = Some("<agent_worktree>\n</agent_worktree>".to_string());
        *worktree_details = Some(details.clone());
    }
    let records = vec![reported, finished(2, "w", 1, Some(json!("older")), false)];
    let mut cache = JournalCache::from_records("wf-00000001", &records);
    assert_eq!(
        cache
            .take("w", 0)
            .and_then(|answer| answer.worktree_details),
        Some(details)
    );
    assert_eq!(
        cache.take("w", 1).map(|answer| answer.worktree_details),
        Some(None)
    );
}
