use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use crate::chat::protocol::RoomId;
use crate::chat::ChatSessionId;
use crate::micron::LinkAction;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use super::OmenChatTransportCompletionMessage;
use super::{
    normalize_omenchat_manual_target, omenchat_scroll_id, sanitize_scroll_offset, DesktopApp,
    DesktopPane, Message, OmenChatMessage,
};

impl DesktopApp {
    pub(super) fn dispatch_omenchat_message(
        &mut self,
        message: Message,
    ) -> Result<Task<Message>, Message> {
        match message {
            Message::OmenChat(OmenChatMessage::NewPane) => Ok(self.update_new_omenchat_pane()),
            Message::OmenChat(OmenChatMessage::ServerEntryChanged(value)) => {
                self.update_omenchat_server_entry_changed(value);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::OpenServerEntry) => {
                Ok(self.update_open_omenchat_server_entry())
            }
            Message::OmenChat(OmenChatMessage::ToggleRooms) => {
                self.update_toggle_omenchat_rooms();
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::JoinRoom { session_id, room }) => {
                Ok(self.update_join_omenchat_room(session_id, room))
            }
            Message::OmenChat(OmenChatMessage::DraftChanged { session_id, value }) => {
                self.update_omenchat_draft_changed(session_id, value);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::Scrolled {
                session_id,
                room_id,
                offset,
            }) => Ok(self.update_omenchat_scrolled(session_id, room_id, offset)),
            Message::OmenChat(OmenChatMessage::JumpToPresent {
                session_id,
                room_id,
            }) => Ok(self.update_jump_omenchat_to_present(session_id, room_id)),
            Message::OmenChat(OmenChatMessage::SendDraft(session_id)) => {
                Ok(self.update_send_omenchat_draft(session_id))
            }
            Message::OmenChat(OmenChatMessage::ResendLocalEcho {
                session_id,
                room_id,
                event_id,
                body,
                action,
            }) => {
                self.update_resend_omenchat_local_echo(session_id, room_id, event_id, body, action);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::LoadOlderHistory(session_id)) => {
                self.update_load_older_omenchat_history(session_id);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::CopySessionDiagnostics(session_id)) => {
                Ok(self.update_copy_omenchat_session_diagnostics(session_id))
            }
            Message::OmenChat(OmenChatMessage::CloseSession(session_id)) => {
                self.update_close_omenchat_session(session_id);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::RequestPath(session_id)) => {
                Ok(self.update_request_omenchat_path(session_id))
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::ReconnectSession(session_id)) => {
                Ok(self.update_reconnect_omenchat_session(session_id))
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::ReconnectSessionIfDisconnected(session_id)) => {
                Ok(self.update_reconnect_omenchat_session_if_disconnected(session_id))
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChatTransportCompletion(
                OmenChatTransportCompletionMessage::PathRequest {
                    session_id,
                    destination,
                    result,
                },
            ) => Ok(self.update_omenchat_path_request_result(session_id, destination, result)),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChatTransportCompletion(OmenChatTransportCompletionMessage::LiveOpen(
                completion,
            )) => {
                Ok(self.update_omenchat_live_open_result(completion.descriptor, completion.result))
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChatTransportCompletion(
                OmenChatTransportCompletionMessage::LiveReconnect(completion),
            ) => Ok(self.update_omenchat_live_reconnect_result(
                completion.session_id,
                completion.generation,
                completion.descriptor,
                completion.result,
            )),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChatMutationCompletion(completion) => {
                Ok(self.update_omenchat_mutation_completion(*completion))
            }
            _ => self.dispatch_omenchat_media_message(message),
        }
    }

    pub(super) fn update_new_omenchat_pane(&mut self) -> Task<Message> {
        let Some(session_id) = self.create_blank_omenchat_session() else {
            return Task::none();
        };
        self.ensure_pane_for_omenchat(session_id);
        self.persist_workspace_panes("workspace panes");
        self.restore_visible_workspace_scrolls()
    }

    pub(super) fn update_omenchat_server_entry_changed(&mut self, value: String) {
        self.omenchat.omenchat_server_entry = value;
    }

    pub(super) fn update_open_omenchat_server_entry(&mut self) -> Task<Message> {
        let Some(target) = normalize_omenchat_manual_target(&self.omenchat.omenchat_server_entry)
        else {
            self.app.status.task = "enter an OMENchat destination hash or omenchat://<hash>".into();
            return Task::none();
        };
        self.omenchat.omenchat_server_entry.clear();
        self.open_omenchat_link(LinkAction {
            target,
            fields: Vec::new(),
        })
        .unwrap_or_else(Task::none)
    }

    pub(super) fn update_toggle_omenchat_rooms(&mut self) {
        self.omenchat.omenchat_rooms_visible = !self.omenchat.omenchat_rooms_visible;
    }

    pub(super) fn update_join_omenchat_room(
        &mut self,
        session_id: ChatSessionId,
        room: String,
    ) -> Task<Message> {
        self.join_omenchat_room(session_id, room);
        self.schedule_visible_workspace_scroll_restore(2);
        self.restore_omenchat_scroll(session_id)
    }

    pub(super) fn update_omenchat_draft_changed(
        &mut self,
        session_id: ChatSessionId,
        value: String,
    ) {
        self.omenchat.chat_drafts.insert(session_id, value);
    }

    pub(super) fn update_omenchat_scrolled(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        offset: RelativeOffset,
    ) -> Task<Message> {
        if !self.workspace_scroll_pane_is_visible(DesktopPane::OmenChat(session_id))
            || self.is_workspace_scroll_restore_settling()
            || self
                .omenchat
                .chat_scroll_bottom_locks
                .contains(&(session_id, room_id))
        {
            return Task::none();
        }
        self.omenchat
            .chat_scroll_offsets
            .insert((session_id, room_id), sanitize_scroll_offset(offset));
        Task::none()
    }

    pub(super) fn update_jump_omenchat_to_present(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> Task<Message> {
        self.omenchat
            .chat_scroll_offsets
            .insert((session_id, room_id), RelativeOffset { x: 0.0, y: 1.0 });
        snap_to(
            omenchat_scroll_id(session_id, room_id),
            RelativeOffset { x: 0.0, y: 1.0 },
        )
    }

    pub(super) fn update_send_omenchat_draft(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        {
            self.send_omenchat_draft_with_durable_intent(session_id)
        }
        #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
        {
            self.send_omenchat_draft(session_id);
            Task::none()
        }
    }

    pub(super) fn update_resend_omenchat_local_echo(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        body: String,
        action: bool,
    ) {
        self.resend_omenchat_local_echo(session_id, room_id, event_id, body, action);
    }

    pub(super) fn update_load_older_omenchat_history(&mut self, session_id: ChatSessionId) {
        self.load_older_omenchat_history(session_id);
    }

    pub(super) fn update_close_omenchat_session(&mut self, session_id: ChatSessionId) {
        self.close_omenchat_session(session_id);
        self.reconcile_workspace_panes_after_target_mutation(None, None);
        self.persist_workspace_panes("workspace panes");
    }
}
