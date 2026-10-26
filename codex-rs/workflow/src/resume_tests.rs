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
            ok: value.is_some(),
            reason: value.is_none().then(|| "timeout".to_string()),
            value,
            tokens: TokenCharge::default(),
            cached,
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
    assert_eq!(cache.take("a", 1), Some(json!("second a")));
    assert_eq!(cache.take("a", 1), None, "a pair is served once");
    assert_eq!(cache.take("b", 0), None, "null outcomes run again");
    assert_eq!(cache.take("c", 0), Some(json!({"x": 1})));
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
