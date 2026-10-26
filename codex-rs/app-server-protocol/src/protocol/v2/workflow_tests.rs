use codex_workflow::RunStatus;
use codex_workflow::RunTracker;
use codex_workflow::TokenCharge;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

fn finished_snapshot() -> codex_workflow::RunSnapshot {
    let mut tracker = RunTracker::new(
        "wf-0a1b2c3d".to_string(),
        "find-bugs".to_string(),
        "Find bugs".to_string(),
        &["Find".to_string()],
        Some(5_000),
        1_000,
    );
    tracker.set_phase(1, "Find");
    tracker.agent_queued(1, Some("first".to_string()), Some("Find".to_string()));
    tracker.agent_queued(2, None, Some("Find".to_string()));
    tracker.agent_started(
        1,
        "/root/wf_0a1b2c3d_1_first".to_string(),
        "t-1".to_string(),
    );
    tracker.agent_finished(
        1,
        true,
        None,
        TokenCharge {
            charged: 120,
            cached: 30,
        },
    );
    tracker.agent_finished(2, false, Some("timeout"), TokenCharge::default());
    tracker.log("done");
    tracker.finish(
        RunStatus::Completed,
        3_500,
        Some("{\"ok\":1}".to_string()),
        None,
    );
    tracker.snapshot(/*include_agents*/ true)
}

/// R8.2: the public run mirrors the core snapshot in camelCase; `agents` only on request.
#[test]
fn workflow_run_mirrors_the_core_snapshot() {
    let snapshot = finished_snapshot();
    let revision = snapshot.revision;
    let run = WorkflowRun::from_snapshot(snapshot.clone(), /*include_agents*/ false);
    assert_eq!(
        serde_json::to_value(&run).expect("serialize run"),
        json!({
            "runId": "wf-0a1b2c3d",
            "name": "find-bugs",
            "description": "Find bugs",
            "status": "completed",
            "revision": revision,
            "currentPhase": "Find",
            "phases": [{"title": "Find", "started": 2, "finished": 2}],
            "agentCounts": {
                "queued": 0,
                "running": 0,
                "awaitingApproval": 0,
                "completed": 1,
                "null": 1,
            },
            "nullReasons": {"timeout": 1},
            "logsTail": ["done"],
            "chargedTokens": 120,
            "cachedTokens": 30,
            "budgetTokens": 5_000,
            "startedAtMs": 1_000,
            "finishedAtMs": 3_500,
            "resultPreview": "{\"ok\":1}",
            "error": null,
        })
    );

    let detailed = WorkflowRun::from_snapshot(snapshot, /*include_agents*/ true);
    assert_eq!(
        serde_json::to_value(detailed.agents.as_ref().expect("agents")).expect("agents"),
        json!([
            {
                "seq": 1,
                "label": "first",
                "phase": "Find",
                "state": "completed",
                "agentPath": "/root/wf_0a1b2c3d_1_first",
                "threadId": "t-1",
                "reason": null,
                "chargedTokens": 120,
                "cachedTokens": 30,
            },
            {
                "seq": 2,
                "label": null,
                "phase": "Find",
                "state": "null",
                "agentPath": null,
                "threadId": null,
                "reason": "timeout",
                "chargedTokens": 0,
                "cachedTokens": 0,
            },
        ])
    );
    assert_eq!(
        serde_json::from_value::<WorkflowRun>(
            serde_json::to_value(&detailed).expect("serialize run")
        )
        .expect("deserialize run"),
        detailed
    );
}

#[test]
fn workflow_read_params_default_to_no_result() {
    assert_eq!(
        serde_json::from_value::<ThreadWorkflowReadParams>(json!({
            "threadId": "thread-1",
            "runId": "wf-0a1b2c3d",
        }))
        .expect("deserialize params"),
        ThreadWorkflowReadParams {
            thread_id: "thread-1".to_string(),
            run_id: "wf-0a1b2c3d".to_string(),
            include_result: false,
        }
    );
}

/// R8.7: a resumed run reports its journal use and the calls answered from it; other
/// runs serialize without either field.
#[test]
fn resumed_runs_carry_resume_counts_and_cached_calls() {
    let mut tracker = RunTracker::new(
        "wf-0a1b2c3e".to_string(),
        "find-bugs".to_string(),
        "Find bugs".to_string(),
        &[],
        /*budget_tokens*/ None,
        1_000,
    );
    tracker.set_resume(codex_workflow::ResumeStats {
        from: "wf-0a1b2c3d".to_string(),
        source_results: 2,
        served: 1,
        unused: 1,
    });
    tracker.agent_cached(1, Some("first".to_string()), /*phase*/ None);
    let run = WorkflowRun::from_snapshot(
        tracker.snapshot(/*include_agents*/ true),
        /*include_agents*/ true,
    );
    let value = serde_json::to_value(&run).expect("serialize run");
    assert_eq!(
        value["resume"],
        json!({"from": "wf-0a1b2c3d", "sourceResults": 2, "served": 1, "unused": 1})
    );
    assert_eq!(value["agents"][0]["cached"], json!(true));
    assert_eq!(
        serde_json::from_value::<WorkflowRun>(value).expect("deserialize run"),
        run
    );

    let plain = serde_json::to_value(WorkflowRun::from_snapshot(
        finished_snapshot(),
        /*include_agents*/ true,
    ))
    .expect("serialize run");
    assert_eq!(plain.get("resume"), None);
    assert_eq!(plain["agents"][0].get("cached"), None);
}
