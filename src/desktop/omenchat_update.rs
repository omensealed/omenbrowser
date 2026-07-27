use iced::widget::operation::snap_to;
use iced::widget::scrollable::RelativeOffset;
use iced::Task;

use crate::chat::protocol::{RoomId, RICH_MESSAGE_MAX_MENTIONS};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::chat::{
    protocol::{
        MessageRevisionAction, PinAction, MESSAGE_REVISION_MAX_NUMBER,
        MESSAGE_REVISION_MAX_REPLACEMENT_BYTES,
    },
    ChatMessageRevisionPresentation, CHAT_ROLE_ADMIN, CHAT_ROLE_MODERATOR, CHAT_STATUS_BANNED,
    CHAT_STATUS_MUTED,
};
use crate::chat::{ChatEventKind, ChatSessionId};
use crate::micron::LinkAction;

use super::omenchat_desktop_state::PendingOmenChatInvitationRoom;
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
                self.omenchat.omenchat_invitation_room = None;
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
            Message::OmenChat(OmenChatMessage::TogglePin {
                session_id,
                room_id,
                event_id,
                action,
            }) => Ok(self.prepare_omenchat_pin_mutation(session_id, room_id, event_id, action)),
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::BeginMessageCorrection {
                session_id,
                room_id,
                event_id,
            }) => {
                self.begin_omenchat_message_correction(session_id, room_id, event_id);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::MessageCorrectionChanged { session_id, value }) => {
                self.update_omenchat_message_correction(session_id, value);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::SubmitMessageCorrection(session_id)) => {
                Ok(self.submit_omenchat_message_correction(session_id))
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::CancelMessageCorrection(session_id)) => {
                self.omenchat.omenchat_revision_drafts.remove(&session_id);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::BeginMessageDeletion {
                session_id,
                room_id,
                event_id,
            }) => {
                self.begin_omenchat_message_deletion(session_id, room_id, event_id);
                Ok(Task::none())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::ConfirmMessageDeletion) => {
                Ok(self.confirm_omenchat_message_deletion())
            }
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            Message::OmenChat(OmenChatMessage::CancelMessageDeletion) => {
                self.omenchat.omenchat_revision_delete_confirmation = None;
                Ok(Task::none())
            }
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
            Message::OmenChat(OmenChatMessage::CopyInvitation(session_id)) => {
                Ok(self.update_copy_omenchat_invitation(session_id))
            }
            #[cfg(feature = "desktop-qr")]
            Message::OmenChat(OmenChatMessage::ToggleInvitationQr(session_id)) => {
                self.update_toggle_omenchat_invitation_qr(session_id);
                Ok(Task::none())
            }
            #[cfg(feature = "desktop-qr")]
            Message::OmenChat(OmenChatMessage::CloseInvitationQr) => {
                self.omenchat.omenchat_invitation_qr = None;
                self.app.status.task = "closed OMENchat invitation QR".into();
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
            let entered = entered.to_owned();
            match self.preview_omenchat_invitation(&entered) {
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
        self.omenchat.omenchat_invitation_room =
            invitation
                .room_id
                .map(|room_id| PendingOmenChatInvitationRoom {
                    server_destination: invitation.server_destination.clone(),
                    room_id,
                    session_id: None,
                });
        self.omenchat.omenchat_server_entry.clear();
        let mut fields = Vec::new();
        if let Some(label) = invitation.display_label {
            fields.push(format!("name={label}"));
        }
        if invitation.room_id.is_some() {
            self.app.status.task =
                "opening OMENchat invitation; waiting for its authenticated room catalog".into();
        }
        let task = self.open_omenchat_link(LinkAction {
            target: format!("omenchat://{}", invitation.server_destination),
            fields,
        });
        if task.is_none() {
            self.omenchat.omenchat_invitation_room = None;
        }
        task.unwrap_or_else(Task::none)
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
            #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
            {
                self.omenchat.omenchat_revision_drafts.remove(&session_id);
                if self
                    .omenchat
                    .omenchat_revision_delete_confirmation
                    .is_some_and(|confirmation| confirmation.session_id == session_id)
                {
                    self.omenchat.omenchat_revision_delete_confirmation = None;
                }
            }
            #[cfg(feature = "desktop-qr")]
            self.clear_omenchat_invitation_qr_for_session(session_id);
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

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_message_revision_action_available(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
        action: MessageRevisionAction,
    ) -> bool {
        let (corrections, deletions) =
            self.omenchat_message_revision_action_targets(session_id, room_id);
        match action {
            MessageRevisionAction::Correct => corrections.contains(&event_id),
            MessageRevisionAction::Tombstone => deletions.contains(&event_id),
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_message_revision_action_targets(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> (
        std::collections::BTreeSet<u64>,
        std::collections::BTreeSet<u64>,
    ) {
        let mut corrections = std::collections::BTreeSet::new();
        let mut deletions = std::collections::BTreeSet::new();
        if !self
            .omenchat
            .omenchat_live_state
            .message_revisions_negotiated(session_id)
            || !self
                .omenchat
                .omenchat_live_state
                .durable_mutations_negotiated(session_id)
        {
            return (corrections, deletions);
        }
        let Some(local_user_id) = self.omenchat.chat_client.local_user_id(session_id) else {
            return (corrections, deletions);
        };
        let Some(session) = self.omenchat.chat_client.session(session_id) else {
            return (corrections, deletions);
        };
        let Some(local_user) = session
            .users
            .iter()
            .find(|user| user.user_id == local_user_id)
        else {
            return (corrections, deletions);
        };
        if local_user.status_bits & CHAT_STATUS_BANNED != 0 {
            return (corrections, deletions);
        }
        let can_moderate = local_user.role_bits & (CHAT_ROLE_MODERATOR | CHAT_ROLE_ADMIN) != 0;
        for target in session.events.iter().filter(|event| {
            event.room_id == room_id
                && crate::chat::model::chat_event_supports_message_revisions(event)
                && self
                    .omenchat
                    .chat_client
                    .message_revision_target_authoritative(session_id, room_id, event.event_id)
        }) {
            let revision = self.omenchat.chat_client.message_revision_for_target(
                session_id,
                room_id,
                target.event_id,
            );
            if revision.is_some_and(|revision| {
                revision.action == MessageRevisionAction::Tombstone
                    || revision.revision_number >= MESSAGE_REVISION_MAX_NUMBER
            }) {
                continue;
            }
            let is_author = target.actor_user_id == Some(local_user_id);
            if is_author
                && local_user.status_bits & CHAT_STATUS_MUTED == 0
                && revision.map_or(0, |revision| revision.revision_number)
                    < MESSAGE_REVISION_MAX_NUMBER.saturating_sub(1)
            {
                corrections.insert(target.event_id);
            }
            if is_author || can_moderate {
                deletions.insert(target.event_id);
            }
        }
        (corrections, deletions)
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn omenchat_pin_action_for_target(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    ) -> Option<PinAction> {
        if !self
            .omenchat
            .omenchat_live_state
            .pins_negotiated(session_id)
            || !self
                .omenchat
                .omenchat_live_state
                .durable_mutations_negotiated(session_id)
            || self
                .omenchat
                .omenchat_live_state
                .pin_mutation_is_pending(session_id, room_id, event_id)
            || !self
                .omenchat
                .chat_client
                .pin_target_authoritative(session_id, room_id, event_id)
        {
            return None;
        }
        let local_user_id = self.omenchat.chat_client.local_user_id(session_id)?;
        let session = self.omenchat.chat_client.session(session_id)?;
        if !session
            .rooms
            .iter()
            .any(|room| room.room_id == room_id && room.joined)
        {
            return None;
        }
        let local_user = session
            .users
            .iter()
            .find(|user| user.user_id == local_user_id)?;
        if local_user.status_bits & CHAT_STATUS_BANNED != 0
            || local_user.role_bits & (CHAT_ROLE_MODERATOR | CHAT_ROLE_ADMIN) == 0
        {
            return None;
        }
        let target = session.events.iter().find(|event| {
            event.room_id == room_id
                && event.event_id == event_id
                && crate::chat::model::chat_event_supports_pins(event)
        })?;
        if self
            .omenchat
            .chat_client
            .pin_for_target(session_id, room_id, target.event_id)
            .is_some()
        {
            return Some(PinAction::Unpin);
        }
        if self
            .omenchat
            .chat_client
            .message_revision_for_target(session_id, room_id, target.event_id)
            .is_some_and(|revision| revision.action == MessageRevisionAction::Tombstone)
        {
            return None;
        }
        Some(PinAction::Pin)
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    fn begin_omenchat_message_correction(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    ) {
        if !self.omenchat_message_revision_action_available(
            session_id,
            room_id,
            event_id,
            MessageRevisionAction::Correct,
        ) {
            self.set_omenchat_session_status(
                session_id,
                "message correction is unavailable for this target".into(),
            );
            return;
        }
        let Some(session) = self.omenchat.chat_client.session(session_id) else {
            return;
        };
        let Some(event) = session
            .events
            .iter()
            .find(|event| event.room_id == room_id && event.event_id == event_id)
        else {
            return;
        };
        let revision = self
            .omenchat
            .chat_client
            .message_revision_for_target(session_id, room_id, event_id);
        let replacement =
            match crate::chat::model::chat_message_revision_presentation(event, revision) {
                Some(ChatMessageRevisionPresentation::Edited { body, .. })
                | Some(ChatMessageRevisionPresentation::Original(body)) => body.to_owned(),
                Some(ChatMessageRevisionPresentation::Deleted { .. }) | None => return,
            };
        self.omenchat.omenchat_revision_drafts.insert(
            session_id,
            super::omenchat_desktop_state::OmenChatRevisionDraft {
                room_id,
                event_id,
                replacement,
            },
        );
        self.omenchat.omenchat_revision_delete_confirmation = None;
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    fn update_omenchat_message_correction(&mut self, session_id: ChatSessionId, value: String) {
        if value.len() > MESSAGE_REVISION_MAX_REPLACEMENT_BYTES {
            self.set_omenchat_session_status(
                session_id,
                format!(
                    "message correction is limited to {MESSAGE_REVISION_MAX_REPLACEMENT_BYTES} bytes"
                ),
            );
            return;
        }
        if let Some(draft) = self.omenchat.omenchat_revision_drafts.get_mut(&session_id) {
            draft.replacement = value;
        }
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    fn submit_omenchat_message_correction(&mut self, session_id: ChatSessionId) -> Task<Message> {
        let Some(draft) = self
            .omenchat
            .omenchat_revision_drafts
            .get(&session_id)
            .cloned()
        else {
            return Task::none();
        };
        self.prepare_omenchat_message_revision_mutation(
            session_id,
            draft.room_id,
            draft.event_id,
            MessageRevisionAction::Correct,
            Some(draft.replacement),
        )
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    fn begin_omenchat_message_deletion(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event_id: u64,
    ) {
        if !self.omenchat_message_revision_action_available(
            session_id,
            room_id,
            event_id,
            MessageRevisionAction::Tombstone,
        ) {
            self.set_omenchat_session_status(
                session_id,
                "message deletion is unavailable for this target".into(),
            );
            return;
        }
        self.omenchat.omenchat_revision_drafts.remove(&session_id);
        self.omenchat.omenchat_revision_delete_confirmation = Some(
            super::omenchat_desktop_state::OmenChatRevisionDeleteConfirmation {
                session_id,
                room_id,
                event_id,
            },
        );
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    fn confirm_omenchat_message_deletion(&mut self) -> Task<Message> {
        let Some(confirmation) = self.omenchat.omenchat_revision_delete_confirmation.take() else {
            return Task::none();
        };
        self.prepare_omenchat_message_revision_mutation(
            confirmation.session_id,
            confirmation.room_id,
            confirmation.event_id,
            MessageRevisionAction::Tombstone,
            None,
        )
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

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
mod tests {
    use super::*;
    use crate::app::{current_epoch_ms, App};
    use crate::chat::protocol::{
        MessageRevisionSnapshot, MessageRevisionSnapshotEntry, PinSnapshot, PinSnapshotEntry,
    };
    use crate::chat::{
        ChatEvent, ChatRoomSummary, ChatServerSummary, ChatSessionView, ChatUserSummary,
    };

    fn revision_test_desktop(suffix: &str) -> (DesktopApp, ChatSessionId, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-revision-controls-{}-{}-{suffix}",
            std::process::id(),
            current_epoch_ms()
        ));
        let mut desktop = DesktopApp::new(App::new(crate::config::AppConfig {
            paths: crate::config::AppPaths::from_root(root.clone()),
            settings: crate::storage::settings::AppSettings::default(),
        }));
        let session_id = desktop.omenchat.chat_client.reserve_session_id();
        let room = ChatRoomSummary {
            server_id: "server".into(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        assert!(desktop.omenchat.chat_client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server".into(),
                destination: "destination".into(),
                display_name: "Server".into(),
            },
            rooms: vec![room.clone()],
            active_room: room,
            users: vec![ChatUserSummary {
                server_id: "server".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            }],
            events: vec![ChatEvent {
                server_id: "server".into(),
                room_id: 1,
                event_id: 9,
                actor_user_id: Some(7),
                actor_display_name: Some("Alice".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "original".into(),
                },
            }],
            status: "ready".into(),
        }));
        assert!(desktop
            .omenchat
            .chat_client
            .bind_local_user_id(session_id, 7));
        desktop
            .omenchat
            .omenchat_live_state
            .set_durable_mutations_negotiated_for_test(session_id, true);
        desktop
            .omenchat
            .omenchat_live_state
            .set_message_revisions_negotiated_for_test(session_id, true);
        (desktop, session_id, root)
    }

    fn enable_revision_prepare(desktop: &mut DesktopApp, session_id: ChatSessionId) {
        desktop.app.paths.ensure().expect("isolated paths");
        desktop
            .omenchat
            .chat_client
            .replace_message_revision_snapshot(
                session_id,
                1,
                &MessageRevisionSnapshot {
                    target_event_ids: vec![9],
                    entries: Vec::new(),
                },
            )
            .expect("authoritative empty revision snapshot");
        let client_instance_id = crate::chat::protocol::ClientInstanceId::new([0x61; 16]);
        desktop
            .omenchat
            .omenchat_live_state
            .set_client_instance_id(Some(client_instance_id));
        desktop.omenchat.omenchat_authenticated_identity_hash = Some(vec![0x62; 16]);
        desktop.omenchat.omenchat_mutation_intent_worker = Some(
            crate::chat::mutation_intent_worker::MutationIntentWorker::start(
                desktop.app.paths.identity_storage_root(),
            )
            .expect("intent worker"),
        );
    }

    fn recover_revision_intents(
        desktop: &DesktopApp,
    ) -> Vec<crate::chat::mutation_intents::OutboundMutationIntent> {
        desktop
            .omenchat
            .omenchat_mutation_intent_worker
            .as_ref()
            .expect("worker")
            .try_recover()
            .expect("recovery admitted")
            .recv()
            .expect("recovery reply")
            .expect("recovery result")
    }

    fn shutdown_revision_test(mut desktop: DesktopApp, root: std::path::PathBuf) {
        desktop
            .omenchat
            .omenchat_mutation_intent_worker
            .take()
            .expect("worker")
            .shutdown()
            .expect("worker shutdown");
        drop(desktop);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn revision_controls_require_authority_preserve_drafts_and_confirm_deletion() {
        let (mut desktop, session_id, _) = revision_test_desktop("authority");
        assert!(!desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Correct,
        ));
        desktop
            .omenchat
            .chat_client
            .replace_message_revision_snapshot(
                session_id,
                1,
                &MessageRevisionSnapshot {
                    target_event_ids: vec![9],
                    entries: Vec::new(),
                },
            )
            .expect("authoritative empty revision snapshot");
        assert!(desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Correct,
        ));
        assert!(desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Tombstone,
        ));
        desktop
            .omenchat
            .omenchat_live_state
            .set_message_revisions_negotiated_for_test(session_id, false);
        let (correction_targets, deletion_targets) =
            desktop.omenchat_message_revision_action_targets(session_id, 1);
        assert!(correction_targets.is_empty());
        assert!(deletion_targets.is_empty());
        desktop
            .omenchat
            .omenchat_live_state
            .set_message_revisions_negotiated_for_test(session_id, true);

        desktop
            .omenchat
            .chat_drafts
            .insert(session_id, "ordinary draft".into());
        desktop.begin_omenchat_message_correction(session_id, 1, 9);
        assert_eq!(
            desktop
                .omenchat
                .omenchat_revision_drafts
                .get(&session_id)
                .map(|draft| draft.replacement.as_str()),
            Some("original")
        );
        desktop.update_omenchat_message_correction(session_id, "corrected".into());
        assert_eq!(
            desktop
                .omenchat
                .chat_drafts
                .get(&session_id)
                .map(String::as_str),
            Some("ordinary draft")
        );

        desktop.begin_omenchat_message_deletion(session_id, 1, 9);
        assert!(!desktop
            .omenchat
            .omenchat_revision_drafts
            .contains_key(&session_id));
        assert_eq!(
            desktop.omenchat.omenchat_revision_delete_confirmation,
            Some(
                super::super::omenchat_desktop_state::OmenChatRevisionDeleteConfirmation {
                    session_id,
                    room_id: 1,
                    event_id: 9,
                }
            )
        );
    }

    #[test]
    fn revision_controls_enforce_author_mute_and_moderator_boundaries() {
        let (mut desktop, session_id, _) = revision_test_desktop("permissions");
        desktop
            .omenchat
            .chat_client
            .replace_message_revision_snapshot(
                session_id,
                1,
                &MessageRevisionSnapshot {
                    target_event_ids: vec![9],
                    entries: vec![MessageRevisionSnapshotEntry {
                        target_event_id: 9,
                        latest_revision_event_id: 10,
                        action: MessageRevisionAction::Correct,
                        actor_user_id: 7,
                        at_unix: 2,
                        replacement: Some("corrected".into()),
                        revision_number: 1,
                    }],
                },
            )
            .expect("authoritative revision snapshot");
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .users[0]
            .status_bits = CHAT_STATUS_MUTED;
        assert!(!desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Correct,
        ));
        assert!(desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Tombstone,
        ));
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .users[0]
            .status_bits = CHAT_STATUS_BANNED;
        assert!(!desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Tombstone,
        ));

        let session = desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session");
        session.users[0].status_bits = 0;
        session.users[0].role_bits = CHAT_ROLE_MODERATOR;
        session.events[0].actor_user_id = Some(8);
        assert!(!desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Correct,
        ));
        assert!(desktop.omenchat_message_revision_action_available(
            session_id,
            1,
            9,
            MessageRevisionAction::Tombstone,
        ));
    }

    #[test]
    fn pin_controls_require_test_negotiation_role_authority_and_current_state() {
        let (mut desktop, session_id, _) = revision_test_desktop("pin-controls");
        assert_eq!(
            desktop.omenchat_pin_action_for_target(session_id, 1, 9),
            None
        );
        desktop
            .omenchat
            .omenchat_live_state
            .set_pins_negotiated_for_test(session_id, true);
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .users[0]
            .role_bits = CHAT_ROLE_MODERATOR;
        desktop
            .omenchat
            .chat_client
            .replace_pin_snapshot(
                session_id,
                1,
                &PinSnapshot {
                    target_event_ids: vec![9],
                    entries: Vec::new(),
                },
            )
            .expect("authoritative unpinned target");
        assert_eq!(
            desktop.omenchat_pin_action_for_target(session_id, 1, 9),
            Some(PinAction::Pin)
        );

        desktop
            .omenchat
            .chat_client
            .replace_pin_snapshot(
                session_id,
                1,
                &PinSnapshot {
                    target_event_ids: vec![9],
                    entries: vec![PinSnapshotEntry {
                        target_event_id: 9,
                        pin_event_id: 10,
                        actor_user_id: 7,
                        pinned_at_unix: 2,
                    }],
                },
            )
            .expect("authoritative pinned target");
        assert_eq!(
            desktop.omenchat_pin_action_for_target(session_id, 1, 9),
            Some(PinAction::Unpin)
        );
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .users[0]
            .role_bits = 0;
        assert_eq!(
            desktop.omenchat_pin_action_for_target(session_id, 1, 9),
            None
        );
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .users[0]
            .role_bits = CHAT_ROLE_MODERATOR;
        desktop.omenchat.chat_client.mark_pins_stale(session_id);
        assert_eq!(
            desktop.omenchat_pin_action_for_target(session_id, 1, 9),
            None
        );
        desktop
            .omenchat
            .chat_client
            .replace_pin_snapshot(
                session_id,
                1,
                &PinSnapshot {
                    target_event_ids: vec![9],
                    entries: Vec::new(),
                },
            )
            .expect("restored pin authority");
        desktop
            .omenchat
            .chat_client
            .replace_message_revision_snapshot(
                session_id,
                1,
                &MessageRevisionSnapshot {
                    target_event_ids: vec![9],
                    entries: vec![MessageRevisionSnapshotEntry {
                        target_event_id: 9,
                        latest_revision_event_id: 11,
                        action: MessageRevisionAction::Tombstone,
                        actor_user_id: 7,
                        at_unix: 3,
                        replacement: None,
                        revision_number: 1,
                    }],
                },
            )
            .expect("tombstone target");
        assert_eq!(
            desktop.omenchat_pin_action_for_target(session_id, 1, 9),
            None,
            "a new pin must not be offered for a tombstoned target"
        );
    }

    #[test]
    fn correction_prepare_persists_before_send_and_preserves_ordinary_draft() {
        let (mut desktop, session_id, root) = revision_test_desktop("persistence");
        enable_revision_prepare(&mut desktop, session_id);
        desktop
            .omenchat
            .chat_drafts
            .insert(session_id, "ordinary draft".into());

        let _task = desktop.prepare_omenchat_message_revision_mutation(
            session_id,
            1,
            9,
            MessageRevisionAction::Correct,
            Some("corrected".into()),
        );
        let recovered = recover_revision_intents(&desktop);
        assert_eq!(recovered.len(), 1);
        assert_eq!(
            recovered[0].op,
            crate::chat::protocol::ChatOp::RoomMessageRevision
        );
        assert_eq!(
            recovered[0].state,
            crate::chat::mutation_intents::OutboundMutationState::Prepared
        );
        assert_eq!(
            crate::chat::protocol::MessageRevisionRequest::from_frame_body(&recovered[0].body)
                .expect("stored revision request")
                .replacement
                .as_deref(),
            Some("corrected")
        );
        assert_eq!(
            desktop
                .omenchat
                .chat_drafts
                .get(&session_id)
                .map(String::as_str),
            Some("ordinary draft")
        );

        shutdown_revision_test(desktop, root);
    }

    #[test]
    fn pin_prepare_persists_before_send_and_preserves_ordinary_draft() {
        let (mut desktop, session_id, root) = revision_test_desktop("pin-persistence");
        enable_revision_prepare(&mut desktop, session_id);
        desktop
            .omenchat
            .omenchat_live_state
            .set_pins_negotiated_for_test(session_id, true);
        desktop
            .omenchat
            .chat_client
            .session_mut(session_id)
            .expect("session")
            .users[0]
            .role_bits = CHAT_ROLE_MODERATOR;
        desktop
            .omenchat
            .chat_client
            .replace_pin_snapshot(
                session_id,
                1,
                &PinSnapshot {
                    target_event_ids: vec![9],
                    entries: Vec::new(),
                },
            )
            .expect("authoritative pin target");
        desktop
            .omenchat
            .chat_drafts
            .insert(session_id, "ordinary draft".into());

        let _task = desktop.prepare_omenchat_pin_mutation(session_id, 1, 9, PinAction::Pin);
        let recovered = recover_revision_intents(&desktop);
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].op, crate::chat::protocol::ChatOp::RoomPin);
        assert_eq!(
            recovered[0].state,
            crate::chat::mutation_intents::OutboundMutationState::Prepared
        );
        assert_eq!(
            crate::chat::protocol::PinRequest::from_frame_body(&recovered[0].body)
                .expect("stored pin request")
                .action,
            PinAction::Pin
        );
        assert_eq!(
            desktop
                .omenchat
                .chat_drafts
                .get(&session_id)
                .map(String::as_str),
            Some("ordinary draft")
        );
        shutdown_revision_test(desktop, root);
    }

    #[test]
    fn deletion_requires_confirmation_before_durable_prepare() {
        let (mut desktop, session_id, root) = revision_test_desktop("delete-confirmation");
        enable_revision_prepare(&mut desktop, session_id);

        desktop.begin_omenchat_message_deletion(session_id, 1, 9);
        assert!(recover_revision_intents(&desktop).is_empty());
        let _task = desktop.confirm_omenchat_message_deletion();
        let recovered = recover_revision_intents(&desktop);
        assert_eq!(recovered.len(), 1);
        let request =
            crate::chat::protocol::MessageRevisionRequest::from_frame_body(&recovered[0].body)
                .expect("stored deletion request");
        assert_eq!(request.target_event_id, 9);
        assert_eq!(request.action, MessageRevisionAction::Tombstone);
        assert_eq!(request.replacement, None);
        assert!(desktop
            .omenchat
            .omenchat_revision_delete_confirmation
            .is_none());

        shutdown_revision_test(desktop, root);
    }
}
