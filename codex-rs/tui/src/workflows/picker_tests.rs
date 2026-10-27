//! Fork: grouping of workflow agents in `/subagents` (R8.6.7).

use super::*;
use pretty_assertions::assert_eq;

fn item(name: &str) -> SelectionItem {
    SelectionItem {
        name: name.to_string(),
        name_prefix_spans: vec!["•".into(), " ".into()],
        ..Default::default()
    }
}

#[test]
fn workflow_child_names_are_parsed() {
    assert_eq!(
        parse_workflow_child("/root/wf_1a2b3c4d_7_find_src_a_py"),
        Some(WorkflowChild {
            run_id: "wf-1a2b3c4d".to_string(),
            seq: 7,
            label: Some("find_src_a_py".to_string()),
        })
    );
    assert_eq!(
        parse_workflow_child("wf_1a2b3c4d_3"),
        Some(WorkflowChild {
            run_id: "wf-1a2b3c4d".to_string(),
            seq: 3,
            label: None,
        })
    );
    assert_eq!(
        parse_workflow_child("/root/wf_1a2b3c4d_7_find_src_a_py")
            .map(|child| child.row_name())
            .as_deref(),
        Some("#7 find_src_a_py")
    );
    for path in [
        "/root/worker",
        "/root/wf_1a2b3c4_7",
        "/root/wf_1A2B3C4D_7",
        "/root/wf_1a2b3c4d_",
        "/root/wf_1a2b3c4d_x7",
        "/root/wf_1a2b3c4d_+7",
        "/root/wf_1a2b3c4d_7_",
        "/root/wf_1a2b3c4d",
        "",
    ] {
        assert_eq!(parse_workflow_child(path), None, "{path}");
    }
}

#[test]
fn no_layout_without_workflow_agents() {
    assert_eq!(
        PickerLayout::build([None, Some("/root/worker"), Some("/root/explorer")]),
        None
    );
}

#[test]
fn workflow_agents_follow_their_run_row_at_the_first_agent() {
    let layout = PickerLayout::build([
        None,
        Some("/root/worker"),
        Some("/root/wf_0000aaaa_1_find"),
        Some("/root/reviewer"),
        Some("/root/wf_0000bbbb_1"),
        Some("/root/wf_0000aaaa_2_verify"),
    ])
    .expect("workflow agents are grouped");
    assert_eq!(
        layout.rows(),
        &[
            PickerRow::Thread(0),
            PickerRow::Thread(1),
            PickerRow::Run {
                run_id: "wf-0000aaaa".to_string(),
                children: 2,
            },
            PickerRow::Thread(2),
            PickerRow::Thread(5),
            PickerRow::Thread(3),
            PickerRow::Run {
                run_id: "wf-0000bbbb".to_string(),
                children: 1,
            },
            PickerRow::Thread(4),
        ]
    );
    assert_eq!(layout.visible_index(2), None);
    assert_eq!(layout.visible_index(4), Some(5));
    assert_eq!(layout.visible_index(8), None);
    assert_eq!(layout.display_index(5), Some(4));
    assert_eq!(layout.display_index(3), Some(5));
    for visible in 0..6 {
        let display = layout
            .display_index(visible)
            .expect("every thread has a row");
        assert_eq!(layout.visible_index(display), Some(visible));
    }
    // A run row is a key of its own, so a refresh can keep the cursor on it.
    assert_eq!(layout.run_at(2), Some("wf-0000aaaa"));
    assert_eq!(layout.run_at(6), Some("wf-0000bbbb"));
    assert_eq!(layout.run_at(3), None);
    assert_eq!(layout.run_at(8), None);
    assert_eq!(layout.run_row("wf-0000bbbb"), Some(6));
    assert_eq!(layout.run_row("wf-0000cccc"), None);

    let items = vec![
        item("Main [default]"),
        item("/root/worker"),
        item("/root/wf_0000aaaa_1_find"),
        item("/root/reviewer"),
        item("/root/wf_0000bbbb_1"),
        item("/root/wf_0000aaaa_2_verify"),
    ];
    let grouped = layout.group_items(items, |run_id, children| {
        item(&format!("Workflow {run_id} ({children})"))
    });
    assert_eq!(
        grouped
            .iter()
            .map(|item| {
                let prefix: String = item
                    .name_prefix_spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                format!("{prefix}{}", item.name)
            })
            .collect::<Vec<_>>(),
        vec![
            "• Main [default]",
            "• /root/worker",
            "• Workflow wf-0000aaaa (2)",
            "  • #1 find",
            "  • #2 verify",
            "• /root/reviewer",
            "• Workflow wf-0000bbbb (1)",
            "  • #1",
        ]
    );
}
