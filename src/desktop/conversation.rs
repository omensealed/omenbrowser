use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use super::{conversation_scroll_id, sanitize_scroll_offset, DesktopApp, DesktopPane, Message};

impl DesktopApp {
    pub(super) fn dispatch_conversation_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::SwitchConversation(index) => Ok(self.update_switch_conversation(index)),
            Message::ConversationScrolled {
                conversation_id,
                offset,
            } => Ok(self.update_conversation_scrolled(conversation_id, offset)),
            Message::JumpConversationToPresent(conversation_id) => {
                Ok(self.update_jump_conversation_to_present(conversation_id))
            }
            Message::ConversationTitleChanged(value) => {
                self.update_conversation_title_changed(value);
                Ok(Task::none())
            }
            Message::ConversationBodyChanged(value) => {
                self.update_conversation_body_changed(value);
                Ok(Task::none())
            }
            Message::ConversationPanePeerChanged {
                conversation_id,
                value,
            } => {
                self.update_conversation_pane_peer_changed(conversation_id, value);
                Ok(Task::none())
            }
            Message::ConversationPaneTitleChanged {
                conversation_id,
                value,
            } => {
                self.update_conversation_pane_title_changed(conversation_id, value);
                Ok(Task::none())
            }
            Message::ConversationPaneBodyChanged {
                conversation_id,
                value,
            } => {
                self.update_conversation_pane_body_changed(conversation_id, value);
                Ok(Task::none())
            }
            Message::ConversationPaneBodyEdited {
                conversation_id,
                action,
            } => {
                self.update_conversation_pane_body_edited(conversation_id, action);
                Ok(Task::none())
            }
            Message::PickConversationAttachment(conversation_id) => {
                Ok(self.update_pick_conversation_attachment(conversation_id))
            }
            Message::ConversationAttachmentPicked {
                conversation_id,
                result,
            } => {
                self.update_conversation_attachment_picked(conversation_id, result);
                Ok(Task::none())
            }
            Message::RemoveConversationAttachment {
                conversation_id,
                index,
            } => {
                self.update_remove_conversation_attachment(conversation_id, index);
                Ok(Task::none())
            }
            Message::OpenConversationAttachment(path) => {
                self.update_open_conversation_attachment(path);
                Ok(Task::none())
            }
            Message::ToggleConversationPaneDeliveryMode(conversation_id) => {
                self.update_toggle_conversation_pane_delivery_mode(conversation_id);
                Ok(Task::none())
            }
            Message::ToggleConversationPaneTicket(conversation_id) => {
                self.update_toggle_conversation_pane_ticket(conversation_id);
                Ok(Task::none())
            }
            Message::SendConversationPaneDraft(conversation_id) => {
                self.update_send_conversation_pane_draft(conversation_id);
                Ok(Task::none())
            }
            Message::PrepareLatestLxmfRetryForConversation(conversation_id) => {
                self.update_prepare_latest_lxmf_retry_for_conversation(conversation_id);
                Ok(Task::none())
            }
            Message::SendLatestLxmfRetryForConversation(conversation_id) => {
                self.update_send_latest_lxmf_retry_for_conversation(conversation_id);
                Ok(Task::none())
            }
            Message::SelectConversationPaneRow {
                conversation_id,
                key,
            } => {
                self.update_select_conversation_pane_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::PrepareLxmfRetryForConversationRow {
                conversation_id,
                key,
            } => {
                self.update_prepare_lxmf_retry_for_conversation_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::SendLxmfRetryForConversationRow {
                conversation_id,
                key,
            } => {
                self.update_send_lxmf_retry_for_conversation_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::DismissConversationPaneRow {
                conversation_id,
                key,
            } => {
                self.update_dismiss_conversation_pane_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::CloseConversationPaneDetails { conversation_id } => {
                self.update_close_conversation_pane_details(conversation_id);
                Ok(Task::none())
            }
            Message::SyncPropagationForConversationRow {
                conversation_id,
                key,
            } => {
                self.update_sync_propagation_for_conversation_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::InspectConversationPanePeer(conversation_id) => {
                self.update_inspect_conversation_pane_peer(conversation_id);
                Ok(Task::none())
            }
            Message::RequestConversationPanePeerPath(conversation_id) => {
                self.update_request_conversation_pane_peer_path(conversation_id);
                Ok(Task::none())
            }
            Message::ConversationPaneDiagnostics(conversation_id) => {
                self.update_conversation_pane_diagnostics(conversation_id);
                Ok(Task::none())
            }
            Message::ToggleConversationPaneTrust(conversation_id) => {
                self.update_toggle_conversation_pane_trust(conversation_id);
                Ok(Task::none())
            }
            Message::ToggleConversationDeliveryMode => {
                self.update_toggle_conversation_delivery_mode();
                Ok(Task::none())
            }
            Message::ToggleConversationTicket => {
                self.update_toggle_conversation_ticket();
                Ok(Task::none())
            }
            Message::SendConversationDraft => {
                self.update_send_conversation_draft();
                Ok(Task::none())
            }
            Message::PrepareLatestLxmfRetry => {
                self.update_prepare_latest_lxmf_retry();
                Ok(Task::none())
            }
            Message::SendLatestLxmfRetry => {
                self.update_send_latest_lxmf_retry();
                Ok(Task::none())
            }
            Message::SelectConversationRow(key) => {
                self.update_select_conversation_row(key);
                Ok(Task::none())
            }
            Message::PrepareLxmfRetryForRow(key) => {
                self.update_prepare_lxmf_retry_for_row(key);
                Ok(Task::none())
            }
            Message::SendLxmfRetryForRow(key) => {
                self.update_send_lxmf_retry_for_row(key);
                Ok(Task::none())
            }
            Message::SyncPropagationForRow(key) => {
                self.update_sync_propagation_for_row(key);
                Ok(Task::none())
            }
            Message::SyncMessages => {
                self.update_sync_messages();
                Ok(Task::none())
            }
            Message::InspectLxmfPeer => {
                self.update_inspect_lxmf_peer();
                Ok(Task::none())
            }
            Message::RequestLxmfPeerPath => {
                self.update_request_lxmf_peer_path();
                Ok(Task::none())
            }
            _ => Err(message),
        }
    }

    pub(super) fn update_switch_conversation(&mut self, index: usize) -> Task<Message> {
        self.app.select_conversation_tab(index);
        self.ensure_pane_for_active_conversation();
        self.schedule_visible_workspace_scroll_restore(2);
        self.restore_visible_conversation_scrolls()
    }

    pub(super) fn update_conversation_scrolled(
        &mut self,
        conversation_id: u64,
        offset: RelativeOffset,
    ) -> Task<Message> {
        if !self.workspace_scroll_pane_is_visible(DesktopPane::Conversation(conversation_id))
            || self.is_workspace_scroll_restore_settling()
            || self
                .conversation
                .scroll_restore_locks
                .contains(&conversation_id)
        {
            return Task::none();
        }
        self.conversation
            .scroll_offsets
            .insert(conversation_id, sanitize_scroll_offset(offset));
        Task::none()
    }

    pub(super) fn update_jump_conversation_to_present(
        &mut self,
        conversation_id: u64,
    ) -> Task<Message> {
        self.conversation
            .scroll_offsets
            .insert(conversation_id, RelativeOffset { x: 0.0, y: 1.0 });
        snap_to(
            conversation_scroll_id(conversation_id),
            RelativeOffset { x: 0.0, y: 1.0 },
        )
    }
}
