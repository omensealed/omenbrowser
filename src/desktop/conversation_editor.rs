use iced::widget::text_editor;

use super::{DesktopApp, DesktopPane};

impl DesktopApp {
    pub(in crate::desktop) fn select_conversation_by_id(&mut self, conversation_id: u64) -> bool {
        let Some(index) = self
            .app
            .workspace
            .conversations
            .iter()
            .position(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        self.app.select_conversation_tab(index);
        if let Some(pane) = self.find_workspace_pane(&DesktopPane::Conversation(conversation_id)) {
            self.workspace.active_workspace_pane = pane;
        }
        self.ensure_conversation_body_editor(conversation_id);
        true
    }

    pub(in crate::desktop) fn ensure_conversation_body_editor(&mut self, conversation_id: u64) {
        if self
            .conversation
            .body_editors
            .contains_key(&conversation_id)
        {
            return;
        }
        let body = self
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.draft_body.as_str())
            .unwrap_or_default();
        self.conversation
            .body_editors
            .insert(conversation_id, text_editor::Content::with_text(body));
    }

    pub(in crate::desktop) fn conversation_body_editor_mut(
        &mut self,
        conversation_id: u64,
    ) -> &mut text_editor::Content {
        self.ensure_conversation_body_editor(conversation_id);
        self.conversation
            .body_editors
            .get_mut(&conversation_id)
            .expect("conversation body editor was just ensured")
    }

    pub(in crate::desktop) fn sync_conversation_body_editor(&mut self, conversation_id: u64) {
        let Some(body) = self
            .app
            .workspace
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .map(|conversation| conversation.draft_body.clone())
        else {
            self.conversation.body_editors.remove(&conversation_id);
            return;
        };
        let needs_replace = self
            .conversation
            .body_editors
            .get(&conversation_id)
            .is_none_or(|editor| conversation_editor_text(editor) != body);
        if needs_replace {
            self.conversation
                .body_editors
                .insert(conversation_id, text_editor::Content::with_text(&body));
        }
    }

    pub(in crate::desktop) fn sync_conversation_body_editors(&mut self) {
        let conversation_ids = self
            .app
            .workspace
            .conversations
            .iter()
            .map(|conversation| conversation.id)
            .collect::<Vec<_>>();
        for conversation_id in conversation_ids {
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(in crate::desktop) fn clear_conversation_body_editor(&mut self, conversation_id: u64) {
        self.conversation
            .body_editors
            .insert(conversation_id, text_editor::Content::new());
    }
}

pub(in crate::desktop) fn conversation_editor_text(editor: &text_editor::Content) -> String {
    editor
        .lines()
        .map(|line| line.text.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "native-reticulum")]
    use super::{conversation_editor_text, DesktopApp};

    #[cfg(feature = "native-reticulum")]
    #[test]
    fn blocked_conversation_pane_send_keeps_editor_when_no_message_row_exists() {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-desktop-send-blocked-keeps-draft-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::config::AppPaths::from_root(root);
        paths.ensure().expect("paths");
        let mut settings = crate::storage::settings::AppSettings::default();
        settings.runtime_backend = crate::storage::settings::RuntimeBackendSetting::Reticulum;
        let mut app = crate::app::App::new(crate::config::AppConfig { paths, settings });
        app.runtime_status.backend = crate::runtime::network::RuntimeBackendName::Reticulum;
        app.runtime_status.connected = true;
        app.runtime_status.active_identity = Some(crate::identity::IdentityProfile {
            label: "test".into(),
            path: app.paths.identities_dir.join("default_identity"),
            hash_hex: "00".repeat(16),
            managed: true,
        });
        let conversation_id = app.active_conversation().id;
        app.set_active_conversation_peer_hash("not-a-hash".into());
        app.set_active_conversation_draft_body("do not lose this".into());
        let mut desktop = DesktopApp::new(app);

        let _ = desktop.update(crate::desktop::Message::Conversation(
            crate::desktop::ConversationMessage::SendPaneDraft(conversation_id),
        ));

        let conversation = desktop.app.active_conversation();
        assert_eq!(conversation.draft_body, "do not lose this");
        assert!(conversation.pending_send.is_none());
        assert!(conversation.thread.messages.is_empty());
        let editor = desktop
            .conversation
            .body_editors
            .get(&conversation_id)
            .expect("conversation editor");
        assert_eq!(conversation_editor_text(editor), "do not lose this");
    }
}
