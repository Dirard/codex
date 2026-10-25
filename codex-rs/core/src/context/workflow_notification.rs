//! Fork: model-visible notice that a background workflow run finished
//! (`specs/dynamic-workflow-2026-10-25.md`, UC7).

use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::input_text;

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
        ("<workflow_notification>", "</workflow_notification>")
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
}
