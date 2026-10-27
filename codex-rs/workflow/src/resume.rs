//! Answers of an earlier run that a resumed run reuses (UC6, R6.3, R6.5).

use std::collections::HashMap;
use std::collections::HashSet;

use serde_json::Value;

use crate::journal::JournalEvent;
use crate::journal::JournalRecord;
use crate::snapshot::ResumeStats;

/// A successful answer of the source journal.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedAnswer {
    pub value: Value,
    /// Where the source's isolated agent left its changes (R3.8); `None` when the agent was
    /// not isolated or the journal predates this field.
    pub worktree: Option<String>,
    /// The structured details of that worktree (R3.13); `None` like `worktree`, or for a
    /// journal that records only the report.
    pub worktree_details: Option<Value>,
}

/// UC6: successful agent results of a source journal by call key and ordinal.
///
/// Results the source itself took from its own source are included, so a chain of resumes
/// keeps reusing them. Null outcomes are not cached: those calls run again.
#[derive(Clone, Debug, Default)]
pub struct JournalCache {
    from: String,
    results: HashMap<(String, u64), CachedAnswer>,
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
                worktree,
                worktree_details,
                ..
            } = &record.event
            {
                results.insert(
                    (key.clone(), *ordinal),
                    CachedAnswer {
                        value: value.clone(),
                        worktree: worktree.clone(),
                        worktree_details: worktree_details.clone(),
                    },
                );
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

    /// R6.3: the source answer of the call with `key` and `ordinal`, if it succeeded there.
    /// Each pair is served once.
    pub fn take(&mut self, key: &str, ordinal: u64) -> Option<CachedAnswer> {
        let pair = (key.to_string(), ordinal);
        if self.served.contains(&pair) {
            return None;
        }
        let answer = self.results.get(&pair).cloned()?;
        self.served.insert(pair);
        Some(answer)
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
