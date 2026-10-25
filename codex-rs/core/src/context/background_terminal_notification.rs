//! Fork: model-visible notice that a background unified exec process exited.

use codex_protocol::models::ContentItem;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::input_text;

use super::ContextualUserFragment;

/// Runtime-authored report about one background terminal; never user authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackgroundTerminalNotification {
    body: String,
}

impl BackgroundTerminalNotification {
    pub(crate) const KIND: &'static str = "unified_exec.background_terminal_notification";

    /// `body` is already bounded by the producer (see `exit_notification`).
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

impl ContextualUserFragment for BackgroundTerminalNotification {
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
        (
            "<background_terminal_notification>",
            "</background_terminal_notification>",
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
        let item = ContextualUserFragment::into(BackgroundTerminalNotification::new(
            "Background terminal session 1000 exited with code 0 after 1.0s.\n",
        ));
        let ResponseItem::Message { role, content, .. } = &item else {
            panic!("expected message, got {item:?}");
        };
        assert_eq!(role, "user");
        assert_eq!(
            content,
            &vec![ContentItem::InputText {
                text: "<background_terminal_notification>\nBackground terminal session 1000 exited with code 0 after 1.0s.\n</background_terminal_notification>".to_string(),
            }]
        );
        assert!(BackgroundTerminalNotification::is_notification_item(&item));
        assert!(crate::context::is_contextual_user_fragment(&content[0]));
    }

    #[test]
    fn ordinary_user_text_is_not_a_notification() {
        let item = ResponseItem::Message {
            encrypted_content: None,
            status: None,
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "background terminal finished".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        assert!(!BackgroundTerminalNotification::is_notification_item(&item));
    }
}
