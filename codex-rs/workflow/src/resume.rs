//! Answers of an earlier run that a resumed run reuses (UC6, R6.3, R6.5).

use std::collections::HashMap;
use std::collections::HashSet;

use serde_json::Value;

use crate::journal::JournalEvent;
use crate::journal::JournalRecord;
use crate::snapshot::ResumeStats;

/// UC6: successful agent results of a source journal by call key and ordinal.
///
/// Results the source itself took from its own source are included, so a chain of resumes
/// keeps reusing them. Null outcomes are not cached: those calls run again.
#[derive(Clone, Debug, Default)]
pub struct JournalCache {
    from: String,
    results: HashMap<(String, u64), Value>,
    served: HashSet<(String, u64)>,
}

impl JournalCache {
    /// Build the cache of run `from` from its journal records.
    pub fn from_records(from: &str, records: &[JournalRecord]) -> Self {
        let mut results = HashMap::new();
        for record in records {
            if let JournalEvent::AgentFinished {
                key,
                ordinal,
                ok: true,
                value: Some(value),
                ..
            } = &record.event
            {
                results.insert((key.clone(), *ordinal), value.clone());
            }
        }
        Self {
            from: from.to_string(),
            results,
            served: HashSet::new(),
        }
    }

    /// The source run's id.
    pub fn from_run(&self) -> &str {
        &self.from
    }

    /// R6.3: the source result of the call with `key` and `ordinal`, if it succeeded there.
    /// Each pair is served once.
    pub fn take(&mut self, key: &str, ordinal: u64) -> Option<Value> {
        let pair = (key.to_string(), ordinal);
        if self.served.contains(&pair) {
            return None;
        }
        let value = self.results.get(&pair).cloned()?;
        self.served.insert(pair);
        Some(value)
    }

    /// R6.5: counts reported for the resumed run.
    pub fn stats(&self) -> ResumeStats {
        let source_results = self.results.len() as u64;
        let served = self.served.len() as u64;
        ResumeStats {
            from: self.from.clone(),
            source_results,
            served,
            unused: source_results.saturating_sub(served),
        }
    }
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
