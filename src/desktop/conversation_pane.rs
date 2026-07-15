use iced::{widget::text_editor, Task};
use std::path::PathBuf;

use super::{conversation_editor_text, pick_conversation_attachment_file, DesktopApp, Message};
use crate::workspace::WorkspaceSection;

impl DesktopApp {
    pub(super) fn update_conversation_pane_peer_changed(
        &mut self,
        conversation_id: u64,
        value: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.set_active_conversation_peer_hash(value);
        }
    }

    pub(super) fn update_conversation_pane_title_changed(
        &mut self,
        conversation_id: u64,
        value: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.set_active_conversation_draft_title(value);
        }
    }

    pub(super) fn update_conversation_pane_body_changed(
        &mut self,
        conversation_id: u64,
        value: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.set_active_conversation_draft_body(value);
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(super) fn update_conversation_pane_body_edited(
        &mut self,
        conversation_id: u64,
        action: text_editor::Action,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            let editor = self.conversation_body_editor_mut(conversation_id);
            editor.perform(action);
            let value = conversation_editor_text(editor);
            self.app.set_active_conversation_draft_body(value);
        }
    }

    pub(super) fn update_pick_conversation_attachment(
        &mut self,
        conversation_id: u64,
    ) -> Task<Message> {
        Task::perform(
            async move { pick_conversation_attachment_file() },
            move |result| {
                Message::ConversationCompletion(
                    super::ConversationCompletionMessage::AttachmentPicked {
                        conversation_id,
                        result,
                    },
                )
            },
        )
    }

    pub(super) fn update_conversation_attachment_picked(
        &mut self,
        conversation_id: u64,
        result: Result<Option<PathBuf>, String>,
    ) {
        match result {
            Ok(Some(path)) => {
                if self.select_conversation_by_id(conversation_id) {
                    self.app.add_active_conversation_attachment(path.clone());
                    self.app.status.task = format!(
                        "attached {}",
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("file")
                    );
                }
            }
            Ok(None) => {
                self.app.status.task = "attachment picker cancelled".into();
            }
            Err(error) => {
                self.app.status.task = format!("attachment picker failed: {error}");
            }
        }
    }

    pub(super) fn update_remove_conversation_attachment(
        &mut self,
        conversation_id: u64,
        index: usize,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.remove_active_conversation_attachment(index);
        }
    }

    pub(super) fn update_open_conversation_attachment(&mut self, path: PathBuf) {
        self.open_local_file(path);
    }

    pub(super) fn update_toggle_conversation_pane_delivery_mode(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.toggle_active_conversation_delivery_mode();
        }
    }

    pub(super) fn update_toggle_conversation_pane_ticket(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.toggle_active_conversation_ticket();
        }
    }

    pub(super) fn update_send_conversation_pane_draft(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.send_active_conversation_draft();
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(super) fn update_prepare_latest_lxmf_retry_for_conversation(
        &mut self,
        conversation_id: u64,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.prepare_latest_lxmf_retry();
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(super) fn update_send_latest_lxmf_retry_for_conversation(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.send_latest_lxmf_retry();
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(super) fn update_select_conversation_pane_row(
        &mut self,
        conversation_id: u64,
        key: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.select_active_conversation_message(key);
        }
    }

    pub(super) fn update_prepare_lxmf_retry_for_conversation_row(
        &mut self,
        conversation_id: u64,
        key: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.prepare_lxmf_retry_by_message_key(&key);
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(super) fn update_send_lxmf_retry_for_conversation_row(
        &mut self,
        conversation_id: u64,
        key: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.send_lxmf_retry_by_message_key(&key);
            self.sync_conversation_body_editor(conversation_id);
        }
    }

    pub(super) fn update_dismiss_conversation_pane_row(
        &mut self,
        conversation_id: u64,
        key: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.dismiss_active_conversation_message(&key);
        }
    }

    pub(super) fn update_close_conversation_pane_details(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.clear_active_conversation_message_selection();
        }
    }

    pub(super) fn update_sync_propagation_for_conversation_row(
        &mut self,
        conversation_id: u64,
        key: String,
    ) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.sync_propagation_for_message_key(&key);
        }
    }

    pub(super) fn update_inspect_conversation_pane_peer(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.inspect_active_lxmf_peer();
        }
    }

    pub(super) fn update_request_conversation_pane_peer_path(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.request_active_lxmf_peer_path();
        }
    }

    pub(super) fn update_conversation_pane_diagnostics(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app
                .set_diagnostics_target_for_conversation(conversation_id);
            self.app.inspect_active_lxmf_peer();
            self.app.switch_section(WorkspaceSection::Diagnostics);
        }
    }

    pub(super) fn update_toggle_conversation_pane_trust(&mut self, conversation_id: u64) {
        if self.select_conversation_by_id(conversation_id) {
            self.app.toggle_active_lxmf_peer_trust();
        }
    }
}
