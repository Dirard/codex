//! Fork: workflow runs shared by the `/workflows` screen tests.

use std::collections::BTreeMap;

use codex_app_server_protocol::WorkflowAgent;
use codex_app_server_protocol::WorkflowAgentCounts;
use codex_app_server_protocol::WorkflowAgentState;
use codex_app_server_protocol::WorkflowPhase;
use codex_app_server_protocol::WorkflowResumeStats;
use codex_app_server_protocol::WorkflowRun;
use codex_app_server_protocol::WorkflowRunStatus;

/// The clock the fixtures are rendered against: the running run is 1m 20s old.
pub(crate) const NOW_MS: i64 = 1_080_000;

pub(crate) fn agent_thread(seq: u64) -> String {
    format!("00000000-0000-0000-0000-{seq:012}")
}

fn agent(
    seq: u64,
    label: Option<&str>,
    phase: Option<&str>,
    state: WorkflowAgentState,
) -> WorkflowAgent {
    let spawned = !matches!(state, WorkflowAgentState::Queued);
    WorkflowAgent {
        seq,
        label: label.map(str::to_string),
        phase: phase.map(str::to_string),
        state,
        agent_path: spawned.then(|| format!("/root/wf_1a2b3c4d_{seq}")),
        thread_id: spawned.then(|| agent_thread(seq)),
        reason: None,
        charged_tokens: 0,
        cached_tokens: 0,
        cached: false,
    }
}

fn cached(mut agent: WorkflowAgent) -> WorkflowAgent {
    agent.agent_path = None;
    agent.thread_id = None;
    agent.cached = true;
    agent
}

fn charged(mut agent: WorkflowAgent, tokens: u64) -> WorkflowAgent {
    agent.charged_tokens = tokens;
    agent
}

/// A resumed run in its `Verify` phase with agents in every state.
pub(crate) fn running_run() -> WorkflowRun {
    let mut null = agent(
        6,
        Some("verify c.py:7"),
        Some("Verify"),
        WorkflowAgentState::Null,
    );
    null.reason = Some("timeout".to_string());
    WorkflowRun {
        run_id: "wf-1a2b3c4d".to_string(),
        name: "find-bugs".to_string(),
        description: "Fan-out bug search, adversarial verification and synthesis".to_string(),
        status: WorkflowRunStatus::Running,
        revision: 42,
        current_phase: Some("Verify".to_string()),
        phases: vec![
            WorkflowPhase {
                title: "Find".to_string(),
                started: 3,
                finished: 3,
            },
            WorkflowPhase {
                title: "Verify".to_string(),
                started: 4,
                finished: 1,
            },
            WorkflowPhase {
                title: "Synthesize".to_string(),
                started: 1,
                finished: 0,
            },
        ],
        agent_counts: WorkflowAgentCounts {
            queued: 1,
            running: 2,
            awaiting_approval: 1,
            completed: 3,
            null_results: 1,
        },
        null_reasons: BTreeMap::from([("timeout".to_string(), 1)]),
        logs_tail: (1..=7).map(|line| format!("log line {line}")).collect(),
        charged_tokens: 48_200,
        cached_tokens: 157_000,
        budget_tokens: Some(200_000),
        started_at_ms: 1_000_000,
        finished_at_ms: None,
        result_preview: None,
        error: None,
        agents: Some(vec![
            cached(agent(
                1,
                Some("find src/a.py"),
                Some("Find"),
                WorkflowAgentState::Completed,
            )),
            charged(
                agent(
                    2,
                    Some("find src/b.py"),
                    Some("Find"),
                    WorkflowAgentState::Completed,
                ),
                15_210,
            ),
            cached(agent(
                3,
                Some("find src/c.py"),
                Some("Find"),
                WorkflowAgentState::Completed,
            )),
            charged(
                agent(
                    4,
                    Some("verify a.py:12"),
                    Some("Verify"),
                    WorkflowAgentState::Running,
                ),
                3_100,
            ),
            agent(
                5,
                Some("verify b.py:40"),
                Some("Verify"),
                WorkflowAgentState::AwaitingApproval,
            ),
            charged(null, 9_000),
            agent(7, None, Some("Verify"), WorkflowAgentState::Running),
            agent(
                8,
                Some("synthesis"),
                Some("Synthesize"),
                WorkflowAgentState::Queued,
            ),
        ]),
        resume: Some(WorkflowResumeStats {
            from: "wf-0a0b0c0d".to_string(),
            source_results: 3,
            served: 2,
            unused: 1,
        }),
    }
}

/// A finished run with a result, 3m 02s long.
pub(crate) fn completed_run() -> WorkflowRun {
    WorkflowRun {
        run_id: "wf-0f0f0f0f".to_string(),
        name: "triage".to_string(),
        description: "Sort open issues".to_string(),
        status: WorkflowRunStatus::Completed,
        revision: 30,
        current_phase: None,
        phases: Vec::new(),
        agent_counts: WorkflowAgentCounts {
            completed: 11,
            null_results: 1,
            ..Default::default()
        },
        null_reasons: BTreeMap::from([("schema_invalid".to_string(), 1)]),
        logs_tail: Vec::new(),
        charged_tokens: 120_000,
        cached_tokens: 0,
        budget_tokens: None,
        started_at_ms: 900_000,
        finished_at_ms: Some(1_082_000),
        result_preview: Some("{\"bugs\": 9,\n \"fixed\": 0}".to_string()),
        error: None,
        agents: None,
        resume: None,
    }
}

/// A run whose script threw after two agents.
pub(crate) fn failed_run() -> WorkflowRun {
    WorkflowRun {
        run_id: "wf-12345678".to_string(),
        name: "refactor-plan".to_string(),
        description: String::new(),
        status: WorkflowRunStatus::Failed,
        revision: 9,
        current_phase: Some("Plan".to_string()),
        phases: Vec::new(),
        agent_counts: WorkflowAgentCounts {
            completed: 2,
            ..Default::default()
        },
        null_reasons: BTreeMap::new(),
        logs_tail: vec!["planning".to_string()],
        charged_tokens: 8_000,
        cached_tokens: 1_000,
        budget_tokens: None,
        started_at_ms: 950_000,
        finished_at_ms: Some(964_000),
        result_preview: None,
        error: Some("TypeError: plan is undefined\n    at workflow.js:12:3".to_string()),
        agents: Some(Vec::new()),
        resume: None,
    }
}
