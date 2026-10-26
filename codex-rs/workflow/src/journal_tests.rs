use super::*;
use crate::key::call_key;
use crate::key::canonical_json;
use pretty_assertions::assert_eq;
use serde_json::json;

fn options(value: Value) -> serde_json::Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => panic!("options must be an object"),
    }
}

/// R6.2: the key ignores field order and `label`, but depends on `schema` and `model`.
#[test]
fn call_key_depends_only_on_result_affecting_options() {
    let base = call_key(
        "find bugs",
        &options(json!({"schema": {"b": 1, "a": [2, {"y": 1, "x": 2}]}, "label": "one"})),
    );
    let reordered = call_key(
        "find bugs",
        &options(
            json!({"label": "two", "phase": "Find", "schema": {"a": [2, {"x": 2, "y": 1}], "b": 1}}),
        ),
    );
    assert_eq!(base, reordered);
    assert_eq!(base.len(), 64);
    assert_ne!(
        base,
        call_key("find bugs", &options(json!({"schema": {"b": 1}})))
    );
    assert_ne!(
        base,
        call_key(
            "find bugs",
            &options(json!({"schema": {"b": 1, "a": [2, {"y": 1, "x": 2}]}, "model": "m"}))
        )
    );
    assert_eq!(
        canonical_json(&json!({"b": [true, null], "a": "\"q\""})),
        r#"{"a":"\"q\"","b":[true,null]}"#
    );
}

/// R6.1: script, args and records land in the run directory, one line per event.
#[test]
fn file_journal_persists_script_args_and_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run_dir = dir.path().join("thread").join("wf-1");
    let journal =
        Journal::create(&run_dir, "export const meta = {};", "{\"a\":1}").expect("create journal");
    assert_eq!(
        std::fs::read_to_string(run_dir.join("script.js")).expect("script"),
        "export const meta = {};"
    );
    assert_eq!(
        std::fs::read_to_string(run_dir.join("args.json")).expect("args"),
        "{\"a\":1}"
    );
    let finished = JournalEvent::AgentFinished {
        call: None,
        key: "k".to_string(),
        ordinal: 0,
        ok: true,
        value: Some(json!({"items": []})),
        reason: None,
        tokens: TokenCharge {
            charged: 10,
            cached: 5,
        },
        cached: false,
    };
    assert_eq!(
        journal
            .append(JournalEvent::Phase {
                title: "Find".to_string()
            })
            .expect("append"),
        1
    );
    assert_eq!(journal.append(finished.clone()).expect("append"), 2);
    // Flushed: a fresh reader sees both records before the writer is dropped.
    let text = std::fs::read_to_string(run_dir.join("journal.jsonl")).expect("journal");
    assert_eq!(text.lines().count(), 2);
    assert!(text.lines().nth(1).is_some_and(|line| {
        line.starts_with(r#"{"seq":2,"kind":"agent_finished","key":"k","ordinal":0,"ok":true"#)
    }));
    assert_eq!(
        journal.records().expect("records"),
        vec![
            JournalRecord {
                seq: 1,
                event: JournalEvent::Phase {
                    title: "Find".to_string()
                },
            },
            JournalRecord {
                seq: 2,
                event: finished,
            },
        ]
    );
}

/// R6.1, R6.4: journals written before stage 2 still read, a cut last line is skipped,
/// `journal_bounds` finds the first and last record of a long journal, and an existing
/// journal continues its numbering.
#[test]
fn existing_journals_read_and_continue() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run_dir = dir.path().join("wf-00000001");
    std::fs::create_dir_all(&run_dir).expect("dir");
    let mut text = String::new();
    text.push_str(r#"{"seq":1,"kind":"run_started","run_id":"wf-00000001","name":"old","script_sha256":"s","args_sha256":"a","parent_turn_id":"t","resume_from":null}"#);
    text.push('\n');
    let filler = "x".repeat(4096);
    for seq in 2..=60 {
        text.push_str(&format!(
            r#"{{"seq":{seq},"kind":"log","message":"{filler}"}}"#
        ));
        text.push('\n');
    }
    text.push_str(r#"{"seq":61,"kind":"agent_finished","key":"k","ordinal":0,"ok":true,"value":"v","reason":null,"tokens":{"charged":1,"cached":2}}"#);
    text.push('\n');
    text.push_str(r#"{"seq":62,"kind":"log","mess"#);
    std::fs::write(run_dir.join(JOURNAL_FILE), &text).expect("write");

    let records = read_records(&run_dir).expect("records");
    assert_eq!(records.len(), 61, "the cut last line is skipped");
    assert_eq!(
        records[60].event,
        JournalEvent::AgentFinished {
            call: None,
            key: "k".to_string(),
            ordinal: 0,
            ok: true,
            value: Some(json!("v")),
            reason: None,
            tokens: TokenCharge {
                charged: 1,
                cached: 2,
            },
            cached: false,
        }
    );
    let (first, last) = journal_bounds(&run_dir.join(JOURNAL_FILE)).expect("bounds");
    assert!(matches!(
        first.map(|record| record.event),
        Some(JournalEvent::RunStarted {
            started_at_ms: None,
            ..
        })
    ));
    assert_eq!(last.map(|record| record.seq), Some(61));

    let journal = Journal::open_existing(&run_dir).expect("open");
    let seq = journal
        .append(JournalEvent::RunFinished {
            status: "interrupted".to_string(),
            result_bytes: 0,
            tokens: TokenCharge::default(),
            error: None,
            finished_at_ms: Some(5),
        })
        .expect("append");
    assert_eq!(seq, 62);
    let (_, last) = journal_bounds(&run_dir.join(JOURNAL_FILE)).expect("bounds");
    assert_eq!(last.map(|record| record.seq), Some(62));
    // The new record starts on its own line after the cut one.
    assert_eq!(read_records(&run_dir).expect("records").len(), 62);
    journal.write_result("{\"ok\":1}").expect("result");
    assert_eq!(
        std::fs::read_to_string(run_dir.join(RESULT_FILE)).expect("result"),
        "{\"ok\":1}"
    );
}

/// R6.6: ephemeral journals keep records in memory only.
#[test]
fn memory_journal_keeps_records() {
    let journal = Journal::in_memory();
    journal
        .append(JournalEvent::Log {
            message: "hello".to_string(),
        })
        .expect("append");
    assert_eq!(journal.dir(), None);
    assert_eq!(journal.records().expect("records").len(), 1);
}
