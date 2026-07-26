use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use crate::chat::protocol::{RoomId, RICH_MESSAGE_MAX_MENTIONS};
use crate::chat::{ChatEventKind, ChatSessionId};
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
            Message::OmenChat(OmenChatMessage::ConfirmInvitation) => {
                Ok(self.update_confirm_omenchat_invitation())
            }
            Message::OmenChat(OmenChatMessage::CancelInvitation) => {
                self.omenchat.omenchat_invitation_preview.cancel();
                self.app.status.task =
                    "OMENchat invitation cancelled; no connection was opened".into();
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::ToggleRooms) => {
                self.update_toggle_omenchat_rooms();
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::JoinRoom { session_id, room }) => {
                Ok(self.update_join_omenchat_room(session_id, room))
            }
            Message::OmenChat(OmenChatMessage::ToggleMuteExceptMentions {
                session_id,
                room_id,
            }) => {
                self.update_toggle_omenchat_mute_except_mentions(session_id, room_id);
                Ok(Task::none())
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
            Message::OmenChat(OmenChatMessage::JumpToEvent {
                session_id,
                room_id,
                event_id,
            }) => Ok(self.update_jump_omenchat_to_event(session_id, room_id, event_id)),
            Message::OmenChat(OmenChatMessage::BeginReply {
                session_id,
                room_id,
                event_id,
            }) => {
                self.update_begin_omenchat_reply(session_id, room_id, event_id);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::CancelReply(session_id)) => {
                self.omenchat.omenchat_reply_drafts.remove(&session_id);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::ToggleMention {
                session_id,
                user_id,
            }) => {
                self.update_toggle_omenchat_mention(session_id, user_id);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::ClearMentions(session_id)) => {
                self.omenchat.omenchat_selected_mentions.remove(&session_id);
                Ok(Task::none())
            }
            Message::OmenChat(OmenChatMessage::SendDraft(session_id)) => {
                Ok(self.update_send_omenchat_draft(session_id))
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::ToggleReaction {
                session_id,
                room_id,
                event_id,
                token,
            }) => Ok(self.prepare_omenchat_reaction_mutation(session_id, room_id, event_id, token)),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::BeginMutationResolution {
                mutation_id,
                action,
            }) => {
                self.begin_omenchat_mutation_resolution(mutation_id, action);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::ConfirmMutationResolution) => {
                Ok(self.confirm_omenchat_mutation_resolution())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::CancelMutationResolution) => {
                self.cancel_omenchat_mutation_resolution();
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::ToggleRecoveredMutationReview(destination)) => {
                self.toggle_omenchat_recovered_mutation_review(destination);
                Ok(Task::none())
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
        let entered = self.omenchat.omenchat_server_entry.trim();
        if entered.starts_with("omenchat://") && entered.contains('?') {
            match self
                .omenchat
                .omenchat_invitation_preview
                .replace_from_uri(entered, &self.app.directory_state.entries)
            {
                Ok(()) => {
                    self.app.status.task =
                        "review the OMENchat invitation; no connection has been opened".into();
                }
                Err(error) => {
                    self.app.status.task = format!("invalid OMENchat invitation: {error}");
                }
            }
            return Task::none();
        }
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

    pub(super) fn update_confirm_omenchat_invitation(&mut self) -> Task<Message> {
        let Some(invitation) = self.omenchat.omenchat_invitation_preview.take_confirmable() else {
            self.app.status.task =
                "OMENchat invitation cannot be confirmed; review its identity evidence".into();
            return Task::none();
        };
        self.omenchat.omenchat_server_entry.clear();
        let mut fields = Vec::new();
        if let Some(label) = invitation.display_label {
            fields.push(format!("name={label}"));
        }
        if invitation.room_id.is_some() {
            self.app.status.task =
                "opening OMENchat invitation; select the suggested room after its catalog loads"
                    .into();
        }
        self.open_omenchat_link(LinkAction {
            target: format!("omenchat://{}", invitation.server_destination),
            fields,
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
        let previous_room_id = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id);
        self.join_omenchat_room(session_id, room);
        let current_room_id = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.active_room.room_id);
        if previous_room_id != current_room_id {
            self.omenchat.omenchat_reply_drafts.remove(&session_id);
            self.omenchat.omenchat_selected_mentions.remove(&session_id);
        }
        self.schedule_visible_workspace_scroll_restore(2);
        self.restore_omenchat_scroll(session_id)
    }

    pub(in crate::desktop) fn omenchat_reply_mentions_available(
        &self,
        session_id: ChatSessionId,
    ) -> bool {
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        {
            self.omenchat
                .omenchat_live_state
                .reply_mentions_negotiated(session_id)
        }
        #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
        {
            let _ = session_id;
            false
        }
    }

    pub(in crate::desktop) fn omenchat_reactions_available(
        &self,
        session_id: ChatSessionId,
    ) -> bool {
        #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
        {
            self.omenchat
                .omenchat_live_state
                .reactions_negotiated(session_id)
                && self
                    .omenchat
                    .omenchat_live_state
                    .durable_mutations_negotiated(session_id)
                && self
                    .omenchat
                    .chat_client
                    .local_user_id(session_id)
                    .is_some()
        }
        #[cfg(not(any(feature = "chat-client-rns", feature = "chat-client-rns-clean")))]
        {
            let _ = session_id;
            false
        }
    }

    fn update_begin_omenchat_reply(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    ) {
        if !self.omenchat_reply_mentions_available(session_id) {
            self.set_omenchat_session_status(
                session_id,
                "replies are unavailable because reply-mentions-v1 was not negotiated".into(),
            );
            return;
        }
        let valid = self
            .omenchat
            .chat_client
            .session(session_id)
            .filter(|session| session.active_room.room_id == room_id)
            .and_then(|session| {
                session
                    .events
                    .iter()
                    .find(|event| event.room_id == room_id && event.event_id == event_id)
            })
            .is_some_and(|event| {
                matches!(
                    event.kind,
                    ChatEventKind::Message { .. } | ChatEventKind::RichMessage { .. }
                )
            });
        if !valid {
            self.set_omenchat_session_status(
                session_id,
                "that reply target is no longer available in the current room".into(),
            );
            return;
        }
        self.omenchat.omenchat_reply_drafts.insert(
            session_id,
            super::omenchat_desktop_state::OmenChatReplyDraft { room_id, event_id },
        );
    }

    fn update_toggle_omenchat_mention(&mut self, session_id: ChatSessionId, user_id: u32) {
        if !self.omenchat_reply_mentions_available(session_id) {
            self.set_omenchat_session_status(
                session_id,
                "mentions are unavailable because reply-mentions-v1 was not negotiated".into(),
            );
            return;
        }
        let member_present = self
            .omenchat
            .chat_client
            .session(session_id)
            .is_some_and(|session| {
                user_id != 0
                    && self.omenchat.chat_client.local_user_id(session_id) != Some(user_id)
                    && session.users.iter().any(|user| user.user_id == user_id)
            });
        if !member_present {
            self.set_omenchat_session_status(
                session_id,
                "that mention target is no longer a member of this room".into(),
            );
            return;
        }
        let selected = self
            .omenchat
            .omenchat_selected_mentions
            .entry(session_id)
            .or_default();
        if !selected.remove(&user_id) {
            if selected.len() >= RICH_MESSAGE_MAX_MENTIONS {
                self.set_omenchat_session_status(
                    session_id,
                    format!("a message can mention at most {RICH_MESSAGE_MAX_MENTIONS} members"),
                );
                return;
            }
            selected.insert(user_id);
        }
        if selected.is_empty() {
            self.omenchat.omenchat_selected_mentions.remove(&session_id);
        }
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
            RelativeOffset { x: 0.0, y: 0.0 },
        )
    }

    pub(super) fn update_jump_omenchat_to_event(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    ) -> Task<Message> {
        let Some(session) = self.omenchat.chat_client.session(session_id) else {
            return Task::none();
        };
        if session.active_room.room_id != room_id {
            return Task::none();
        }
        let retained = session
            .events
            .iter()
            .filter(|event| event.room_id == room_id)
            .collect::<Vec<_>>();
        let Some(index) = retained.iter().position(|event| event.event_id == event_id) else {
            return Task::none();
        };
        let y = if retained.len() <= 1 {
            0.0
        } else {
            index as f32 / (retained.len() - 1) as f32
        };
        let offset = RelativeOffset { x: 0.0, y };
        self.omenchat
            .chat_scroll_offsets
            .insert((session_id, room_id), offset);
        snap_to(
            omenchat_scroll_id(session_id, room_id),
            super::workspace_scroll_omenchat::omenchat_offset_to_bottom_anchored_widget(offset),
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
