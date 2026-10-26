//! Fork: model-visible notice that a background workflow run finished
//! (`specs/dynamic-workflow-2026-10-25.md`, UC7).

use codex_extension_items::ExtensionItem;
use codex_extension_items::workflow::WorkflowItem;
use codex_extension_items::workflow::WorkflowRunStatus;
use codex_protocol::items::TurnItem;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::input_text;
use codex_workflow::RunStatus;

use super::ContextualUserFragment;

/// Runtime-authored run summary; never user authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowNotification {
    body: String,
}

impl WorkflowNotification {
    pub(crate) const KIND: &'static str = "workflow.run_notification";

    /// `body` is already bounded by the producer.
    pub(crate) fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }

    /// UC8, R8.3: the terminal `workflow.run` item that shows a recorded notification in
    /// the turn that receives it. A run notifies once, so the id derives from the run id.
    pub(crate) fn turn_item(item: &ResponseItem) -> Option<TurnItem> {
        let ResponseItem::Message { role, content, .. } = item else {
            return None;
        };
        let [ContentItem::InputText { text }] = content.as_slice() else {
            return None;
        };
        if role != "user" {
            return None;
        }
        let notification = codex_workflow::parse_notification_fragment(text)?;
        Some(TurnItem::Extension(ExtensionItem::Workflow(WorkflowItem {
            id: format!("{}-notification", notification.run_id),
            run_id: notification.run_id,
            name: notification.name,
            status: match notification.status {
                RunStatus::Running => WorkflowRunStatus::Running,
                RunStatus::Completed => WorkflowRunStatus::Completed,
                RunStatus::Failed => WorkflowRunStatus::Failed,
                RunStatus::Cancelled => WorkflowRunStatus::Cancelled,
                RunStatus::Interrupted => WorkflowRunStatus::Interrupted,
            },
            notification: Some(notification.text),
        })))
    }

    /// Whether a recorded item is one of these notifications.
    pub(crate) fn is_notification_item(item: &ResponseItem) -> bool {
        matches!(item, ResponseItem::Message { role, content, .. }
        if role == "user"
            && !content.is_empty()
            && content.iter().all(|content| matches!(
                content,
                ContentItem::InputText { text } if Self::matches_text(text)
            )))
    }
}

impl ContextualUserFragment for WorkflowNotification {
    fn content_kind(&self) -> ContentItemKind {
        ContentItemKind(Self::KIND.to_string())
    }

    fn role(&self) -> &'static str {
        "user"
    }

    fn markers(&self) -> (&'static str, &'static str) {
        Self::type_markers()
    }

    fn type_markers() -> (&'static str, &'static str) {
        // App-server reads these tags back when it replays history (R8.3).
        (
            codex_workflow::NOTIFICATION_OPEN_TAG,
            codex_workflow::NOTIFICATION_CLOSE_TAG,
        )
    }

    fn body(&self) -> ContentItem {
        input_text(format!("\n{}\n", self.body.trim_end()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn renders_as_hidden_user_context() {
        let item = ContextualUserFragment::into(WorkflowNotification::new(
            "Workflow \"x\" (wf-1) completed in 1.0s.\n",
        ));
        let ResponseItem::Message { role, content, .. } = &item else {
            panic!("expected message, got {item:?}");
        };
        assert_eq!(role, "user");
        assert_eq!(
            content,
            &vec![ContentItem::InputText {
                text: "<workflow_notification>\nWorkflow \"x\" (wf-1) completed in 1.0s.\n</workflow_notification>".to_string(),
            }]
        );
        assert!(WorkflowNotification::is_notification_item(&item));
        assert!(crate::context::is_background_notification_item(&item));
        assert!(crate::context::is_contextual_user_fragment(&content[0]));
    }

    /// R8.3: a recorded notification shows as the terminal item of its run.
    #[test]
    fn notification_shows_as_the_terminal_run_item() {
        let text = "Workflow \"x\" (wf-1) interrupted in 1.0s: 0 agents (0 ok, 0 null), 0 tokens charged, 0 cached input tokens.";
        let item = ContextualUserFragment::into(WorkflowNotification::new(text));
        let Some(TurnItem::Extension(ExtensionItem::Workflow(workflow))) =
            WorkflowNotification::turn_item(&item)
        else {
            panic!("expected a workflow item");
        };
        assert_eq!(
            workflow,
            WorkflowItem {
                id: "wf-1-notification".to_string(),
                run_id: "wf-1".to_string(),
                name: "x".to_string(),
                status: WorkflowRunStatus::Interrupted,
                notification: Some(text.to_string()),
            }
        );
        let other = ContextualUserFragment::into(WorkflowNotification::new("not a run summary"));
        assert!(WorkflowNotification::turn_item(&other).is_none());
    }
}
