#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use iced::Task;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::app::current_epoch_ms;
#[cfg(feature = "chat-client")]
use crate::chat::client::CHAT_CLIENT_MAX_SESSIONS;
use crate::chat::protocol::RoomId;
use crate::chat::store::ChatStore;
use crate::chat::{ChatClientEvent, ChatSessionId, OmenChatDescriptor};

use super::{
    human_bytes, is_omenchat_local_echo_event, omenchat_upload_cache_key, DesktopApp, DesktopPane,
    OmenChatMediaLoadState, OMENCHAT_PENDING_DESTINATION_PREFIX,
};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use super::{Message, OMENCHAT_MAX_HEARTBEAT_IDLE_MS, OMENCHAT_MIN_HEARTBEAT_IDLE_MS};

impl DesktopApp {
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn request_omenchat_path_task(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        let Some(destination) = self
            .omenchat
            .chat_client
            .session(session_id)
            .map(|session| session.server.destination.clone())
        else {
            self.app.status.task = "cannot request OMENchat path: session is closed".into();
            return Task::none();
        };
        if destination == "mockchatdestination" || destination.len() < 32 {
            self.set_omenchat_session_status(
                session_id,
                "cannot request path for a mock/invalid OMENchat destination".into(),
            );
            return Task::none();
        }
        if !self.app.runtime_status.connected {
            self.set_omenchat_session_status(
                session_id,
                "Reticulum runtime is not connected; request path after startup".into(),
            );
            return Task::none();
        }
        self.set_omenchat_session_status(
            session_id,
            format!("requesting path for OMENchat server {destination}"),
        );
        self.set_omenchat_connection_state(session_id, crate::chat::ChatConnectionState::Resolving);
        let runtime = self.app.runtime.clone();
        let request_destination = destination.clone();
        Task::perform(
            async move {
                let result = runtime
                    .request_path(&request_destination, "OMENchat server path request", true)
                    .await
                    .map_err(|error| error.to_string());
                (session_id, destination, result)
            },
            |(session_id, destination, result)| {
                Message::OmenChatTransportCompletion(
                    super::OmenChatTransportCompletionMessage::PathRequest {
                        session_id,
                        destination,
                        result,
                    },
                )
            },
        )
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn reconnect_omenchat_session_task(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        if !self.app.runtime_status.connected {
            self.set_omenchat_session_status(
                session_id,
                "Reticulum runtime is not connected; reconnect after startup".into(),
            );
            return Task::none();
        }
        let Some(descriptor) = self.omenchat_descriptor_for_session(session_id) else {
            self.app.status.task = "cannot reconnect OMENchat session: session is closed".into();
            return Task::none();
        };
        if descriptor.server_destination == "mockchatdestination"
            || descriptor.server_destination.len() < 32
        {
            self.set_omenchat_session_status(
                session_id,
                "cannot reconnect a mock/invalid OMENchat destination".into(),
            );
            return Task::none();
        }
        self.disconnect_omenchat_session(
            session_id,
            "manual reconnect requested; closing existing link before reconnect",
        );
        self.omenchat.omenchat_live_opening.insert(session_id);
        self.omenchat.omenchat_live_retry_after.remove(&session_id);
        self.omenchat.omenchat_live_retry_count.remove(&session_id);
        let generation = self.next_omenchat_reconnect_generation(session_id);
        self.set_omenchat_session_status(session_id, "reconnecting live OMENchat link".to_string());
        self.set_omenchat_connection_state(
            session_id,
            crate::chat::ChatConnectionState::Reconnecting,
        );
        self.open_live_omenchat_reconnect_task(session_id, generation, descriptor)
    }

    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    pub(in crate::desktop) fn reconnect_omenchat_session_if_disconnected_task(
        &mut self,
        session_id: ChatSessionId,
    ) -> Task<Message> {
        if self
            .omenchat
            .omenchat_live_transports
            .contains_key(&session_id)
        {
            self.clear_omenchat_pending_reconnect(session_id);
            let state = if self
                .omenchat
                .chat_client
                .session(session_id)
                .is_some_and(|session| session.active_room.joined)
            {
                crate::chat::ChatConnectionState::Joined
            } else {
                crate::chat::ChatConnectionState::Authenticating
            };
            self.set_omenchat_connection_state(session_id, state);
            self.set_omenchat_session_status(
                session_id,
                "reconnect skipped: live OMENchat link is already active".into(),
            );
            return Task::none();
        }
        if self.omenchat.omenchat_live_opening.contains(&session_id) {
            self.set_omenchat_connection_state(
                session_id,
                crate::chat::ChatConnectionState::Reconnecting,
            );
            self.set_omenchat_session_status(
                session_id,
                "reconnect skipped: live OMENchat reconnect is already pending".into(),
            );
            return Task::none();
        }
        self.reconnect_omenchat_session_task(session_id)
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn try_open_omenchat_status_session(
        &mut self,
        descriptor: OmenChatDescriptor,
        status: String,
    ) -> Option<ChatSessionId> {
        let server_destination = descriptor.server_destination;
        if let Some(session) = self
            .omenchat
            .chat_client
            .sessions()
            .iter()
            .find(|session| session.server.destination == server_destination)
        {
            let session_id = session.session_id;
            self.set_omenchat_session_status(session_id, status);
            self.omenchat.chat_drafts.entry(session_id).or_default();
            self.omenchat
                .omenchat_connection_states
                .entry(session_id)
                .or_default();
            return Some(session_id);
        }

        let session_id = self.omenchat.chat_client.reserve_session_id();
        let server = crate::chat::model::ChatServerSummary {
            server_id: server_destination.clone(),
            destination: server_destination,
            display_name: descriptor
                .display_name
                .unwrap_or_else(|| "OMENchat Server".to_string()),
        };
        let room_name = descriptor
            .rooms_hint
            .first()
            .cloned()
            .unwrap_or_else(|| "lobby".to_string());
        let room = crate::chat::model::ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: room_name,
            topic: None,
            unread: 0,
            joined: false,
        };
        let session_capacity_reached =
            self.omenchat.chat_client.sessions().len() >= CHAT_CLIENT_MAX_SESSIONS;
        if !self
            .omenchat
            .chat_client
            .push_session(crate::chat::ChatSessionView {
                session_id,
                server,
                rooms: vec![room.clone()],
                active_room: room,
                users: Vec::new(),
                events: Vec::new(),
                status,
            })
        {
            self.app.status.task = if session_capacity_reached {
                "OMENchat session limit reached; close a chat before opening another".into()
            } else {
                "OMENchat descriptor metadata exceeds client limits".into()
            };
            return None;
        }
        self.omenchat.chat_drafts.entry(session_id).or_default();
        self.set_omenchat_connection_state(
            session_id,
            crate::chat::ChatConnectionState::Disconnected,
        );
        self.persist_omenchat_session(session_id);
        Some(session_id)
    }

    #[cfg(all(test, feature = "chat-client"))]
    pub(in crate::desktop) fn open_omenchat_status_session(
        &mut self,
        descriptor: OmenChatDescriptor,
        status: String,
    ) -> ChatSessionId {
        self.try_open_omenchat_status_session(descriptor, status)
            .expect("isolated test session catalog must have capacity")
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn create_blank_omenchat_session(&mut self) -> Option<ChatSessionId> {
        let session_id = self.omenchat.chat_client.reserve_session_id();
        let server_destination = format!("{OMENCHAT_PENDING_DESTINATION_PREFIX}{session_id}");
        let server = crate::chat::model::ChatServerSummary {
            server_id: server_destination.clone(),
            destination: server_destination,
            display_name: "New Chat".into(),
        };
        let room = crate::chat::model::ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: Some("Enter an OMENchat destination hash above, then press Open.".into()),
            unread: 0,
            joined: false,
        };
        if !self
            .omenchat
            .chat_client
            .push_session(crate::chat::ChatSessionView {
                session_id,
                server,
                rooms: vec![room.clone()],
                active_room: room,
                users: Vec::new(),
                events: Vec::new(),
                status: "enter an OMENchat destination hash, then press Open".into(),
            })
        {
            self.app.status.task =
                "OMENchat session limit reached; close a chat before opening another".into();
            return None;
        }
        self.omenchat.chat_drafts.entry(session_id).or_default();
        self.set_omenchat_connection_state(
            session_id,
            crate::chat::ChatConnectionState::Disconnected,
        );
        self.remember_omenchat_bottom(session_id);
        self.app.status.task = "created blank OMENchat client pane".into();
        Some(session_id)
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn omenchat_session_upload_max_file_bytes(
        &self,
        session_id: ChatSessionId,
    ) -> Option<u64> {
        self.omenchat
            .omenchat_upload_max_file_bytes
            .get(&session_id)
            .copied()
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn omenchat_session_upload_quota(
        &self,
        session_id: ChatSessionId,
    ) -> Option<u64> {
        self.omenchat
            .omenchat_upload_quotas
            .get(&session_id)
            .copied()
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn mark_hidden_omenchat_room_unread(
        &mut self,
        session_id: ChatSessionId,
        event: &crate::chat::ChatEvent,
    ) {
        if self
            .find_workspace_pane(&DesktopPane::OmenChat(session_id))
            .is_some()
        {
            return;
        }
        if !self
            .omenchat
            .chat_client
            .event_allows_unread(session_id, event)
        {
            return;
        }
        let Some(session) = self.omenchat.chat_client.session_mut(session_id) else {
            return;
        };
        if session.active_room.room_id != event.room_id {
            self.persist_omenchat_session(session_id);
            return;
        }
        session.active_room.unread = session.active_room.unread.saturating_add(1);
        if let Some(room) = session
            .rooms
            .iter_mut()
            .find(|room| room.room_id == event.room_id)
        {
            room.unread = room.unread.saturating_add(1);
        }
        self.persist_omenchat_session(session_id);
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn update_toggle_omenchat_mute_except_mentions(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) {
        let Some((server_id, room_name)) =
            self.omenchat
                .chat_client
                .session(session_id)
                .and_then(|session| {
                    session
                        .rooms
                        .iter()
                        .find(|room| room.room_id == room_id)
                        .or_else(|| {
                            (session.active_room.room_id == room_id).then_some(&session.active_room)
                        })
                        .map(|room| (session.server.server_id.clone(), room.name.clone()))
                })
        else {
            self.app.status.task = "cannot change notification policy: room is unavailable".into();
            return;
        };
        if self
            .omenchat
            .chat_client
            .local_user_id(session_id)
            .is_none()
        {
            self.set_omenchat_session_status(
                session_id,
                "mute except mentions requires a negotiated local OMENchat user identity".into(),
            );
            return;
        }
        let enabled = !self
            .omenchat
            .chat_client
            .room_mute_except_mentions(session_id, room_id);
        if !self
            .omenchat
            .chat_client
            .set_room_mute_except_mentions(session_id, room_id, enabled)
        {
            self.app.status.task = "cannot change notification policy: room is unavailable".into();
            return;
        }
        let persist_result = self.omenchat.chat_store.as_mut().map_or(Ok(()), |store| {
            store.set_room_mute_except_mentions(&server_id, room_id, enabled)
        });
        if let Err(error) = persist_result {
            let _ = self
                .omenchat
                .chat_client
                .set_room_mute_except_mentions(session_id, room_id, !enabled);
            self.set_omenchat_session_status(
                session_id,
                format!("could not save notification policy: {error}"),
            );
            return;
        }
        self.set_omenchat_session_status(
            session_id,
            if enabled {
                format!("#{room_name} will count only authoritative mentions as unread")
            } else {
                format!("#{room_name} will count all new events as unread")
            },
        );
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn restore_cached_omenchat_room_history(
        &mut self,
        session_id: ChatSessionId,
    ) -> usize {
        let cached_loaded = if let Some(store) = self.omenchat.chat_store.as_ref() {
            match self
                .omenchat
                .chat_client
                .load_cached_room_history(store, session_id, 100)
            {
                Ok(count) => count,
                Err(error) => {
                    self.set_omenchat_session_status(
                        session_id,
                        format!("cached room history load failed: {error}"),
                    );
                    0
                }
            }
        } else {
            0
        };
        if cached_loaded > 0 {
            if self
                .workspace
                .workspace_panes
                .iter()
                .any(|(_, pane)| matches!(pane, DesktopPane::OmenChat(id) if *id == session_id))
            {
                self.lock_omenchat_bottom_until_restore_settles(session_id);
                self.schedule_visible_workspace_scroll_restore(5);
            } else {
                self.remember_omenchat_bottom_if_missing(session_id);
            }
        }
        self.persist_omenchat_session(session_id);
        cached_loaded
    }

    #[cfg(feature = "chat-client")]
    pub(in crate::desktop) fn apply_omenchat_client_events_status(
        &mut self,
        events: &[ChatClientEvent],
    ) {
        for event in events {
            match event {
                ChatClientEvent::ServerOpened { session_id, server } => {
                    self.bind_omenchat_invitation_room(*session_id, &server.destination);
                }
                ChatClientEvent::RoomsUpdated { session_id, rooms } => {
                    self.consume_omenchat_invitation_room_catalog(*session_id, rooms);
                }
                ChatClientEvent::RoomJoined { session_id, .. } => {
                    #[cfg(feature = "desktop-qr")]
                    self.clear_omenchat_invitation_qr_for_session(*session_id);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    self.set_omenchat_connection_state(
                        *session_id,
                        crate::chat::ChatConnectionState::Joined,
                    );
                    self.restore_cached_omenchat_room_history(*session_id);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    if self
                        .omenchat
                        .omenchat_live_transports
                        .contains_key(session_id)
                    {
                        self.sync_recent_omenchat_room_history_if_needed(*session_id);
                    } else {
                        self.omenchat
                            .omenchat_recent_sync_pending
                            .insert(*session_id);
                    }
                }
                ChatClientEvent::ServerMotd { session_id, motd } => {
                    let motd = motd.trim();
                    if motd.is_empty() {
                        self.omenchat.omenchat_motds.remove(session_id);
                    } else {
                        self.omenchat
                            .omenchat_motds
                            .insert(*session_id, motd.to_owned());
                    }
                }
                ChatClientEvent::ServerPolicy {
                    session_id,
                    upload_quota_bytes,
                    upload_max_file_bytes,
                    ping_interval_seconds,
                } => {
                    self.omenchat
                        .omenchat_upload_quotas
                        .insert(*session_id, *upload_quota_bytes);
                    self.omenchat
                        .omenchat_upload_max_file_bytes
                        .insert(*session_id, *upload_max_file_bytes);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    if let Some(transport) =
                        self.omenchat.omenchat_live_transports.get_mut(session_id)
                    {
                        transport.heartbeat_idle_ms =
                            ping_interval_seconds.saturating_mul(1_000).clamp(
                                OMENCHAT_MIN_HEARTBEAT_IDLE_MS,
                                OMENCHAT_MAX_HEARTBEAT_IDLE_MS,
                            );
                    }
                    let quota = if *upload_quota_bytes == 0 {
                        "uploads disabled".into()
                    } else {
                        format!("upload quota {}", human_bytes(*upload_quota_bytes))
                    };
                    self.set_omenchat_session_status(
                        *session_id,
                        format!(
                            "server policy: {quota}; max file {}; ping every {ping_interval_seconds}s",
                            human_bytes(*upload_max_file_bytes)
                        ),
                    );
                }
                ChatClientEvent::UserUpdated { session_id, .. } => {
                    self.persist_omenchat_session(*session_id);
                }
                ChatClientEvent::LocalUserBound {
                    session_id,
                    user_id,
                } => {
                    if self
                        .omenchat
                        .chat_client
                        .bind_local_user_id(*session_id, *user_id)
                    {
                        self.persist_omenchat_session(*session_id);
                    }
                }
                ChatClientEvent::EventAppended { session_id, event } => {
                    self.mark_hidden_omenchat_room_unread(*session_id, event);
                    if !is_omenchat_local_echo_event(event) {
                        self.persist_omenchat_session(*session_id);
                    }
                }
                ChatClientEvent::HistoryPrepended { session_id, events } => {
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    self.mark_omenchat_recent_sync_complete(*session_id);
                    if let Some(store) = self.omenchat.chat_store.as_mut() {
                        if let Err(error) = store.append_events(events.clone()) {
                            tracing::warn!(
                                "failed to persist received OMENchat history batch for session {session_id}: {error}"
                            );
                        }
                    }
                    self.persist_omenchat_session(*session_id);
                }
                ChatClientEvent::ReactionDeltaApplied {
                    session_id,
                    room_id,
                    event,
                } => {
                    let server_id = self
                        .omenchat
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.server.server_id.clone());
                    if let (Some(store), Some(server_id)) =
                        (self.omenchat.chat_store.as_mut(), server_id)
                    {
                        if let Err(error) = store.apply_reaction_event(&server_id, *room_id, *event)
                        {
                            tracing::warn!(
                                "failed to persist received OMENchat reaction delta for session {session_id}: {error}"
                            );
                        }
                    }
                }
                ChatClientEvent::ReactionSnapshotApplied {
                    session_id,
                    room_id,
                    snapshot,
                } => {
                    let server_id = self
                        .omenchat
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.server.server_id.clone());
                    if let (Some(store), Some(server_id)) =
                        (self.omenchat.chat_store.as_mut(), server_id)
                    {
                        if let Err(error) =
                            store.replace_reaction_snapshot(&server_id, *room_id, snapshot.clone())
                        {
                            tracing::warn!(
                                "failed to persist received OMENchat reaction snapshot for session {session_id}: {error}"
                            );
                        }
                    }
                }
                ChatClientEvent::MessageRevisionDeltaApplied {
                    session_id,
                    room_id,
                    event,
                } => {
                    let server_id = self
                        .omenchat
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.server.server_id.clone());
                    if let (Some(store), Some(server_id)) =
                        (self.omenchat.chat_store.as_mut(), server_id)
                    {
                        if let Err(error) =
                            store.apply_message_revision_event(&server_id, *room_id, event.clone())
                        {
                            tracing::warn!(
                                "failed to persist received OMENchat message revision delta for session {session_id}: {error}"
                            );
                        }
                    }
                }
                ChatClientEvent::MessageRevisionSnapshotApplied {
                    session_id,
                    room_id,
                    snapshot,
                } => {
                    let server_id = self
                        .omenchat
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.server.server_id.clone());
                    if let (Some(store), Some(server_id)) =
                        (self.omenchat.chat_store.as_mut(), server_id)
                    {
                        if let Err(error) = store.replace_message_revision_snapshot(
                            &server_id,
                            *room_id,
                            snapshot.clone(),
                        ) {
                            tracing::warn!(
                                "failed to persist received OMENchat message revision snapshot for session {session_id}: {error}"
                            );
                        }
                    }
                }
                ChatClientEvent::PinDeltaApplied {
                    session_id,
                    room_id,
                    event,
                } => {
                    let server_id = self
                        .omenchat
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.server.server_id.clone());
                    if let (Some(store), Some(server_id)) =
                        (self.omenchat.chat_store.as_mut(), server_id)
                    {
                        if let Err(error) = store.apply_pin_event(&server_id, *room_id, *event) {
                            tracing::warn!(
                                "failed to persist received OMENchat pin delta for session {session_id}: {error}"
                            );
                        }
                    }
                }
                ChatClientEvent::PinSnapshotApplied {
                    session_id,
                    room_id,
                    snapshot,
                } => {
                    let server_id = self
                        .omenchat
                        .chat_client
                        .session(*session_id)
                        .map(|session| session.server.server_id.clone());
                    if let (Some(store), Some(server_id)) =
                        (self.omenchat.chat_store.as_mut(), server_id)
                    {
                        if let Err(error) =
                            store.replace_pin_snapshot(&server_id, *room_id, snapshot.clone())
                        {
                            tracing::warn!(
                                "failed to persist received OMENchat pin snapshot for session {session_id}: {error}"
                            );
                        }
                    }
                }
                ChatClientEvent::HistorySynced { session_id, .. } => {
                    #[cfg(not(any(
                        feature = "chat-client-rns",
                        feature = "chat-client-rns-clean"
                    )))]
                    let _ = session_id;
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    self.mark_omenchat_recent_sync_complete(*session_id);
                }
                ChatClientEvent::HistorySyncNeeded {
                    session_id,
                    room_id,
                } => {
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    {
                        self.omenchat.omenchat_recent_sync_links.remove(session_id);
                        self.omenchat
                            .omenchat_recent_sync_attempts
                            .remove(session_id);
                        self.omenchat
                            .omenchat_recent_sync_due_after
                            .insert(*session_id, current_epoch_ms().saturating_add(250));
                        tracing::debug!(
                            session_id = *session_id,
                            room_id = *room_id,
                            "OMENchat live event gap detected; scheduled bounded recent sync"
                        );
                    }
                    #[cfg(not(any(
                        feature = "chat-client-rns",
                        feature = "chat-client-rns-clean"
                    )))]
                    let _ = (session_id, room_id);
                }
                ChatClientEvent::UploadAccepted {
                    session_id,
                    resource_id,
                    filename,
                    bytes,
                } => {
                    let pending_key = (*session_id, filename.clone(), *bytes);
                    if let Some(source_path) = self
                        .omenchat
                        .omenchat_pending_upload_sources
                        .remove(&pending_key)
                    {
                        match self.cache_omenchat_upload_source_file(
                            *session_id,
                            resource_id,
                            filename,
                            &source_path,
                        ) {
                            Ok(path) => {
                                self.set_omenchat_session_status(
                                    *session_id,
                                    format!("upload accepted and cached locally: {path}"),
                                );
                            }
                            Err(error) => {
                                self.set_omenchat_session_status(
                                    *session_id,
                                    format!("upload accepted; local cache failed: {error}"),
                                );
                            }
                        }
                    } else {
                        self.set_omenchat_session_status(
                            *session_id,
                            format!("upload accepted: {filename} ({})", human_bytes(*bytes)),
                        );
                    }
                }
                ChatClientEvent::UploadRejected { session_id, reason } => {
                    self.omenchat
                        .omenchat_pending_upload_sources
                        .retain(|(pending_session_id, _, _), _| *pending_session_id != *session_id);
                    self.set_omenchat_session_status(
                        *session_id,
                        format!("upload rejected: {reason}"),
                    );
                }
                ChatClientEvent::UploadResourceAvailable {
                    session_id,
                    resource_id,
                    filename,
                    content_type,
                    bytes,
                } => match self.cache_omenchat_upload_resource(
                    *session_id,
                    resource_id,
                    filename,
                    content_type.as_deref(),
                    bytes,
                ) {
                    Ok(path) => {
                        self.set_omenchat_session_status(
                            *session_id,
                            format!("upload resource cached: {path}"),
                        );
                    }
                    Err(error) => {
                        self.set_omenchat_session_status(
                            *session_id,
                            format!("upload resource cache failed: {error}"),
                        );
                    }
                },
                ChatClientEvent::UploadResourceProgress {
                    session_id,
                    resource_id,
                    filename,
                    received,
                    total,
                } => {
                    self.omenchat.omenchat_media_cache.insert(
                        omenchat_upload_cache_key(*session_id, resource_id),
                        OmenChatMediaLoadState::Loading {
                            message: format!(
                                "receiving {filename}: {} / {}",
                                human_bytes(*received),
                                human_bytes(*total)
                            ),
                            received: Some(*received),
                            total: Some(*total),
                        },
                    );
                }
                ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message,
                } => {
                    self.clear_omenchat_invitation_room_for_session(*session_id);
                    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
                    if matches!(
                        self.omenchat_connection_state(*session_id),
                        crate::chat::ChatConnectionState::Resolving
                            | crate::chat::ChatConnectionState::Connecting
                            | crate::chat::ChatConnectionState::Authenticating
                            | crate::chat::ChatConnectionState::Reconnecting
                    ) {
                        self.set_omenchat_connection_state(
                            *session_id,
                            crate::chat::ChatConnectionState::Failed { retryable: true },
                        );
                    }
                    self.set_omenchat_session_status(*session_id, format!("error: {message}"));
                }
                _ => {}
            }
        }
    }

    pub(in crate::desktop) fn bind_omenchat_invitation_room(
        &mut self,
        session_id: ChatSessionId,
        server_destination: &str,
    ) {
        let Some(pending) = self.omenchat.omenchat_invitation_room.as_mut() else {
            return;
        };
        if pending
            .server_destination
            .eq_ignore_ascii_case(server_destination)
            && pending
                .session_id
                .is_none_or(|bound_session| bound_session == session_id)
        {
            pending.session_id = Some(session_id);
        }
    }

    pub(in crate::desktop) fn clear_omenchat_invitation_room_for_destination(
        &mut self,
        server_destination: &str,
    ) {
        if self
            .omenchat
            .omenchat_invitation_room
            .as_ref()
            .is_some_and(|pending| {
                pending
                    .server_destination
                    .eq_ignore_ascii_case(server_destination)
            })
        {
            self.omenchat.omenchat_invitation_room = None;
        }
    }

    pub(in crate::desktop) fn clear_omenchat_invitation_room_for_session(
        &mut self,
        session_id: ChatSessionId,
    ) {
        if self
            .omenchat
            .omenchat_invitation_room
            .as_ref()
            .is_some_and(|pending| pending.session_id == Some(session_id))
        {
            self.omenchat.omenchat_invitation_room = None;
        }
    }

    pub(in crate::desktop) fn consume_omenchat_invitation_room_catalog(
        &mut self,
        session_id: ChatSessionId,
        rooms: &[crate::chat::ChatRoomSummary],
    ) {
        let Some(pending) = self.omenchat.omenchat_invitation_room.as_ref() else {
            return;
        };
        if pending.session_id != Some(session_id) {
            return;
        }
        let room_id = pending.room_id;
        let room_name = rooms
            .iter()
            .find(|room| room.room_id == room_id)
            .map(|room| room.name.clone());
        self.omenchat.omenchat_invitation_room = None;
        if let Some(room_name) = room_name {
            self.join_omenchat_room(session_id, room_name.clone());
            self.app.status.task =
                format!("opened OMENchat invitation room #{room_name} ({room_id})");
        } else {
            self.set_omenchat_session_status(
                session_id,
                format!("invitation room {room_id} was not present in the authenticated catalog"),
            );
            self.app.status.task =
                format!("OMENchat invitation room {room_id} is unavailable on this server");
        }
    }
}

#[cfg(all(test, feature = "chat-client"))]
#[path = "omenchat_state_tests.rs"]
mod tests;
