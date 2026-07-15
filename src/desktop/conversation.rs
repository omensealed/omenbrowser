use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use super::{
    conversation_scroll_id, sanitize_scroll_offset, ConversationCompletionMessage,
    ConversationMessage, DesktopApp, DesktopPane, Message,
};

impl DesktopApp {
    pub(super) fn dispatch_conversation_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::Conversation(ConversationMessage::Switch(index)) => {
                Ok(self.update_switch_conversation(index))
            }
            Message::Conversation(ConversationMessage::Scrolled {
                conversation_id,
                offset,
            }) => Ok(self.update_conversation_scrolled(conversation_id, offset)),
            Message::Conversation(ConversationMessage::JumpToPresent(conversation_id)) => {
                Ok(self.update_jump_conversation_to_present(conversation_id))
            }
            Message::Conversation(ConversationMessage::TitleChanged(value)) => {
                self.update_conversation_title_changed(value);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::BodyChanged(value)) => {
                self.update_conversation_body_changed(value);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PanePeerChanged {
                conversation_id,
                value,
            }) => {
                self.update_conversation_pane_peer_changed(conversation_id, value);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PaneTitleChanged {
                conversation_id,
                value,
            }) => {
                self.update_conversation_pane_title_changed(conversation_id, value);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PaneBodyChanged {
                conversation_id,
                value,
            }) => {
                self.update_conversation_pane_body_changed(conversation_id, value);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PaneBodyEdited {
                conversation_id,
                action,
            }) => {
                self.update_conversation_pane_body_edited(conversation_id, action);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PickAttachment(conversation_id)) => {
                Ok(self.update_pick_conversation_attachment(conversation_id))
            }
            Message::ConversationCompletion(ConversationCompletionMessage::AttachmentPicked {
                conversation_id,
                result,
            }) => {
                self.update_conversation_attachment_picked(conversation_id, result);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::RemoveAttachment {
                conversation_id,
                index,
            }) => {
                self.update_remove_conversation_attachment(conversation_id, index);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::OpenAttachment(path)) => {
                self.update_open_conversation_attachment(path);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::TogglePaneDeliveryMode(conversation_id)) => {
                self.update_toggle_conversation_pane_delivery_mode(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::TogglePaneTicket(conversation_id)) => {
                self.update_toggle_conversation_pane_ticket(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SendPaneDraft(conversation_id)) => {
                self.update_send_conversation_pane_draft(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PrepareLatestRetryForConversation(
                conversation_id,
            )) => {
                self.update_prepare_latest_lxmf_retry_for_conversation(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SendLatestRetryForConversation(
                conversation_id,
            )) => {
                self.update_send_latest_lxmf_retry_for_conversation(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SelectPaneRow {
                conversation_id,
                key,
            }) => {
                self.update_select_conversation_pane_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PrepareRetryForConversationRow {
                conversation_id,
                key,
            }) => {
                self.update_prepare_lxmf_retry_for_conversation_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SendRetryForConversationRow {
                conversation_id,
                key,
            }) => {
                self.update_send_lxmf_retry_for_conversation_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::DismissPaneRow {
                conversation_id,
                key,
            }) => {
                self.update_dismiss_conversation_pane_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::ClosePaneDetails { conversation_id }) => {
                self.update_close_conversation_pane_details(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SyncPropagationForConversationRow {
                conversation_id,
                key,
            }) => {
                self.update_sync_propagation_for_conversation_row(conversation_id, key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::InspectPanePeer(conversation_id)) => {
                self.update_inspect_conversation_pane_peer(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::RequestPanePeerPath(conversation_id)) => {
                self.update_request_conversation_pane_peer_path(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PaneDiagnostics(conversation_id)) => {
                self.update_conversation_pane_diagnostics(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::TogglePaneTrust(conversation_id)) => {
                self.update_toggle_conversation_pane_trust(conversation_id);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::ToggleDeliveryMode) => {
                self.update_toggle_conversation_delivery_mode();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::ToggleTicket) => {
                self.update_toggle_conversation_ticket();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SendDraft) => {
                self.update_send_conversation_draft();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PrepareLatestRetry) => {
                self.update_prepare_latest_lxmf_retry();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SendLatestRetry) => {
                self.update_send_latest_lxmf_retry();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SelectRow(key)) => {
                self.update_select_conversation_row(key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::PrepareRetryForRow(key)) => {
                self.update_prepare_lxmf_retry_for_row(key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SendRetryForRow(key)) => {
                self.update_send_lxmf_retry_for_row(key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SyncPropagationForRow(key)) => {
                self.update_sync_propagation_for_row(key);
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::SyncMessages) => {
                self.update_sync_messages();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::InspectPeer) => {
                self.update_inspect_lxmf_peer();
                Ok(Task::none())
            }
            Message::Conversation(ConversationMessage::RequestPeerPath) => {
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
