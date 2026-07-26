use std::collections::{BTreeMap, BTreeSet};

use super::client::{
    enforce_client_event_presentation_bounds, enforce_room_catalog_bounds,
    enforce_user_catalog_bounds, ChatClient, ChatClientEvent, ChatClientRequest, ChatSessionId,
    ChatSessionView, DurableMutationTerminalState, CHAT_CLIENT_MAX_SESSIONS,
};
use super::descriptor::OmenChatDescriptor;
use super::model::{
    bounded_chat_text, chat_text_fits, ChatEvent, ChatEventKind, ChatMessageMetadata,
    ChatRoomSummary, ChatServerSummary, ChatUserSummary, CHAT_ACTOR_DISPLAY_MAX_BYTES,
    CHAT_CONTENT_TYPE_MAX_BYTES, CHAT_MOTD_MAX_BYTES, CHAT_RESOURCE_ID_MAX_BYTES, CHAT_ROLE_ADMIN,
    CHAT_ROLE_MODERATOR, CHAT_ROLE_TRUSTED, CHAT_ROOM_NAME_MAX_BYTES, CHAT_ROOM_TOPIC_MAX_BYTES,
    CHAT_STATUS_BANNED, CHAT_STATUS_MAX_BYTES, CHAT_STATUS_MUTED, CHAT_UPLOAD_FILENAME_MAX_BYTES,
    CHAT_USER_DISPLAY_MAX_BYTES,
};
use super::mutation_intents::{OutboundMutationIntent, OutboundMutationState};
use super::protocol::{
    canonical_mutation_request_hash, parse_rich_message_event_metadata,
    parse_session_accept_negotiation, with_session_open_negotiation, ChatErrorCode, ChatOp,
    ClientInstanceId, DurableMutationEnvelope, Frame, FrameBody, FrameValue, MessageRevisionEvent,
    MessageRevisionSnapshot, MutationId, ReactionAck, ReactionEvent, ReactionRequest,
    ReactionSnapshot, RichMessageBody, RoomId, SessionOpenNegotiation, DEFAULT_JOIN_BACKLOG_EVENTS,
    DURABLE_MUTATION_CAPABILITY, DURABLE_NOTICE_ACK_CAPABILITY, PROTOCOL_NAME,
    REACTIONS_CAPABILITY, REPLY_MENTIONS_CAPABILITY,
};
use super::rns::{recv_chat_event, send_chat_frame, ChatLinkEvent, ChatLinkTransport};

pub const LIVE_INLINE_DOWNLOAD_MAX_ITEMS: usize = 16;
pub const LIVE_INLINE_DOWNLOAD_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const LIVE_INLINE_DOWNLOAD_MAX_RESOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const LIVE_INLINE_DOWNLOAD_MAX_PENDING_CHUNKS: usize = 1_024;
pub const LIVE_PENDING_UPLOAD_MAX_ITEMS: usize = 4;
pub const LIVE_PENDING_UPLOAD_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const LIVE_PENDING_UPLOAD_MAX_RESOURCE_BYTES: usize = 8 * 1024 * 1024;
pub const LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS: usize = 256;
pub const LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveInlineDownloadMetrics {
    pub items: usize,
    pub reserved_bytes: usize,
    pub retained_bytes: usize,
    pub pending_chunks: usize,
    pub rejected: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LivePendingUploadMetrics {
    pub items: usize,
    pub bytes: usize,
    pub rejected: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LivePendingLocalEchoMetrics {
    pub items: usize,
    pub rejected: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveChatClientState {
    client_instance_id: Option<ClientInstanceId>,
    durable_mutation_owner_ready: bool,
    durable_requests: BTreeSet<ChatSessionId>,
    durable_sessions: BTreeSet<ChatSessionId>,
    durable_notice_ack_sessions: BTreeSet<ChatSessionId>,
    reply_mentions_requests: BTreeSet<ChatSessionId>,
    reply_mentions_sessions: BTreeSet<ChatSessionId>,
    reaction_requests: BTreeSet<ChatSessionId>,
    reaction_sessions: BTreeSet<ChatSessionId>,
    message_revision_sessions: BTreeSet<ChatSessionId>,
    local_user_ids: BTreeMap<ChatSessionId, u32>,
    next_seq_by_session: BTreeMap<ChatSessionId, u64>,
    pending_local_echoes: BTreeMap<(ChatSessionId, u32), PendingLocalEcho>,
    pending_uploads: BTreeMap<(ChatSessionId, u32), PendingLiveUpload>,
    pending_upload_downloads: BTreeMap<String, PendingLiveUploadDownload>,
    rejected_pending_local_echoes: u64,
    rejected_upload_downloads: u64,
    rejected_pending_uploads: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SequenceSpaceExhausted;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingLocalEcho {
    session_id: ChatSessionId,
    room_id: RoomId,
    temp_event_id: Option<u64>,
    mutation_id: Option<MutationId>,
    command_result: Option<PendingCommandResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingCommandResult {
    Part,
    Topic,
    Create {
        room_name: String,
    },
    User {
        command: PendingUserCommand,
        target: String,
    },
    Reaction(ReactionRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingUserCommand {
    Role { role_bits: u64 },
    Unban,
    Kick,
    Ban,
    Mute,
    Unmute,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingLiveUpload {
    session_id: ChatSessionId,
    filename: String,
    content_type: Option<String>,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingLiveUploadDownload {
    session_id: ChatSessionId,
    filename: String,
    content_type: Option<String>,
    total_len: usize,
    bytes: Vec<u8>,
    pending_chunks: BTreeMap<usize, Vec<u8>>,
    done_seen: bool,
}

impl LiveChatClientState {
    pub fn set_client_instance_id(&mut self, client_instance_id: Option<ClientInstanceId>) {
        self.client_instance_id = client_instance_id;
        self.durable_mutation_owner_ready = client_instance_id.is_some();
    }

    pub fn client_instance_id(&self) -> Option<ClientInstanceId> {
        self.client_instance_id
    }

    pub fn set_durable_mutation_owner_ready(&mut self, ready: bool) {
        self.durable_mutation_owner_ready = ready && self.client_instance_id.is_some();
    }

    pub fn durable_mutation_owner_ready(&self) -> bool {
        self.durable_mutation_owner_ready
    }

    pub fn durable_mutations_negotiated(&self, session_id: ChatSessionId) -> bool {
        self.durable_sessions.contains(&session_id)
    }

    #[cfg(test)]
    pub(crate) fn set_durable_mutations_negotiated_for_test(
        &mut self,
        session_id: ChatSessionId,
        negotiated: bool,
    ) {
        if negotiated {
            self.durable_sessions.insert(session_id);
        } else {
            self.durable_sessions.remove(&session_id);
        }
    }

    pub fn durable_notice_ack_negotiated(&self, session_id: ChatSessionId) -> bool {
        self.durable_notice_ack_sessions.contains(&session_id)
    }

    pub fn reply_mentions_negotiated(&self, session_id: ChatSessionId) -> bool {
        self.reply_mentions_sessions.contains(&session_id)
    }

    pub fn reactions_negotiated(&self, session_id: ChatSessionId) -> bool {
        self.reaction_sessions.contains(&session_id)
    }

    pub fn message_revisions_negotiated(&self, session_id: ChatSessionId) -> bool {
        self.message_revision_sessions.contains(&session_id)
    }

    #[cfg(test)]
    pub(crate) fn set_reply_mentions_negotiated_for_test(
        &mut self,
        session_id: ChatSessionId,
        negotiated: bool,
    ) {
        if negotiated {
            self.reply_mentions_sessions.insert(session_id);
        } else {
            self.reply_mentions_sessions.remove(&session_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_reactions_negotiated_for_test(
        &mut self,
        session_id: ChatSessionId,
        negotiated: bool,
    ) {
        if negotiated {
            self.reaction_sessions.insert(session_id);
        } else {
            self.reaction_sessions.remove(&session_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_message_revisions_negotiated_for_test(
        &mut self,
        session_id: ChatSessionId,
        negotiated: bool,
    ) {
        if negotiated {
            self.message_revision_sessions.insert(session_id);
        } else {
            self.message_revision_sessions.remove(&session_id);
        }
    }

    pub fn local_user_id(&self, session_id: ChatSessionId) -> Option<u32> {
        self.local_user_ids.get(&session_id).copied()
    }

    pub fn durable_mutation_is_pending(
        &self,
        session_id: ChatSessionId,
        mutation_id: MutationId,
    ) -> bool {
        self.pending_local_echoes.values().any(|pending| {
            pending.session_id == session_id && pending.mutation_id == Some(mutation_id)
        })
    }

    fn reserve_sequence_range(
        &mut self,
        session_id: ChatSessionId,
        count: u64,
    ) -> Result<u64, SequenceSpaceExhausted> {
        let next = self
            .next_seq_by_session
            .get(&session_id)
            .copied()
            .unwrap_or(1);
        let Some(last) = next.checked_add(count.saturating_sub(1)) else {
            return Err(SequenceSpaceExhausted);
        };
        if count == 0 || next == 0 || last > u64::from(u32::MAX) {
            return Err(SequenceSpaceExhausted);
        }
        self.next_seq_by_session
            .insert(session_id, last.saturating_add(1));
        Ok(next)
    }

    fn reserve_seq(&mut self, session_id: ChatSessionId) -> Result<u32, SequenceSpaceExhausted> {
        let next = self.reserve_sequence_range(session_id, 1)?;
        u32::try_from(next).map_err(|_| SequenceSpaceExhausted)
    }

    fn reserve_seq_pair(
        &mut self,
        session_id: ChatSessionId,
    ) -> Result<[u32; 2], SequenceSpaceExhausted> {
        let first = self.reserve_sequence_range(session_id, 2)?;
        let second = first.saturating_add(1);
        let first = u32::try_from(first).map_err(|_| SequenceSpaceExhausted)?;
        let second = u32::try_from(second).map_err(|_| SequenceSpaceExhausted)?;
        Ok([first, second])
    }

    pub fn inline_download_metrics(&self) -> LiveInlineDownloadMetrics {
        LiveInlineDownloadMetrics {
            items: self.pending_upload_downloads.len(),
            reserved_bytes: self
                .pending_upload_downloads
                .values()
                .map(|download| download.total_len)
                .fold(0, usize::saturating_add),
            retained_bytes: self
                .pending_upload_downloads
                .values()
                .map(PendingLiveUploadDownload::retained_payload_bytes)
                .fold(0, usize::saturating_add),
            pending_chunks: self
                .pending_upload_downloads
                .values()
                .map(|download| download.pending_chunks.len())
                .fold(0, usize::saturating_add),
            rejected: self.rejected_upload_downloads,
        }
    }

    pub fn pending_upload_metrics(&self) -> LivePendingUploadMetrics {
        LivePendingUploadMetrics {
            items: self.pending_uploads.len(),
            bytes: self
                .pending_uploads
                .values()
                .map(|upload| upload.bytes.capacity())
                .fold(0, usize::saturating_add),
            rejected: self.rejected_pending_uploads,
        }
    }

    pub fn pending_local_echo_metrics(&self) -> LivePendingLocalEchoMetrics {
        LivePendingLocalEchoMetrics {
            items: self.pending_local_echoes.len(),
            rejected: self.rejected_pending_local_echoes,
        }
    }

    pub fn pending_local_echo_session_items(&self, session_id: ChatSessionId) -> usize {
        self.pending_local_echoes
            .values()
            .filter(|echo| echo.session_id == session_id)
            .count()
    }

    pub fn cancel_session_transfers(&mut self, session_id: ChatSessionId) {
        self.pending_local_echoes
            .retain(|_, echo| echo.session_id != session_id);
        self.pending_uploads
            .retain(|_, upload| upload.session_id != session_id);
        self.pending_upload_downloads
            .retain(|_, download| download.session_id != session_id);
    }

    pub fn retire_session_link_state(
        &mut self,
        session_id: ChatSessionId,
    ) -> BTreeSet<(RoomId, u64)> {
        let retired_echoes = self
            .pending_local_echoes
            .values()
            .filter(|echo| echo.session_id == session_id && echo.mutation_id.is_some())
            .filter_map(|echo| echo.temp_event_id.map(|event_id| (echo.room_id, event_id)))
            .collect();
        self.cancel_session_transfers(session_id);
        self.next_seq_by_session.remove(&session_id);
        self.durable_requests.remove(&session_id);
        self.durable_sessions.remove(&session_id);
        self.durable_notice_ack_sessions.remove(&session_id);
        self.reply_mentions_requests.remove(&session_id);
        self.reply_mentions_sessions.remove(&session_id);
        self.reaction_requests.remove(&session_id);
        self.reaction_sessions.remove(&session_id);
        self.local_user_ids.remove(&session_id);
        retired_echoes
    }
}

fn sequence_space_exhausted_event(session_id: ChatSessionId) -> ChatClientEvent {
    ChatClientEvent::Error {
        session_id: Some(session_id),
        message: "OMENchat link sequence space exhausted; reconnect before sending more operations"
            .into(),
    }
}

impl PendingLiveUploadDownload {
    fn retained_payload_bytes(&self) -> usize {
        self.bytes.len().saturating_add(
            self.pending_chunks
                .values()
                .map(Vec::len)
                .fold(0, usize::saturating_add),
        )
    }
}

pub fn handle_live_request<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    request: ChatClientRequest,
) -> Vec<ChatClientEvent> {
    let mut events = match request {
        ChatClientRequest::OpenServer(descriptor) => {
            open_live_server(client, state, transport, descriptor)
        }
        ChatClientRequest::JoinRoom { session_id, room } => {
            let room = room.trim().trim_start_matches('#');
            if room.is_empty() || !chat_text_fits(room, CHAT_ROOM_NAME_MAX_BYTES) {
                vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "room name is empty or exceeds client limits".into(),
                }]
            } else {
                let seq = match state.reserve_seq(session_id) {
                    Ok(seq) => seq,
                    Err(_) => return vec![sequence_space_exhausted_event(session_id)],
                };
                send_frame_or_error(
                    transport,
                    Frame::new(
                        ChatOp::JoinRoom,
                        seq,
                        None,
                        FrameBody::Text(room.to_owned()),
                    ),
                    Some(session_id),
                )
                .map_or_else(
                    || drain_live_events(client, transport, Some(session_id)),
                    |event| vec![event],
                )
            }
        }
        ChatClientRequest::PartRoom { session_id, room } => {
            part_live_room(client, state, transport, session_id, room)
        }
        ChatClientRequest::SendMessage {
            session_id,
            room: _,
            body,
        } => send_live_room_text(
            client,
            state,
            transport,
            session_id,
            body,
            ChatOp::RoomMessage,
        ),
        ChatClientRequest::SendAction {
            session_id,
            room: _,
            body,
        } => send_live_room_text(
            client,
            state,
            transport,
            session_id,
            body,
            ChatOp::RoomAction,
        ),
        ChatClientRequest::SendNotice {
            session_id,
            room: _,
            body,
        } => send_live_room_text(
            client,
            state,
            transport,
            session_id,
            body,
            ChatOp::RoomNotice,
        ),
        ChatClientRequest::SendUpload {
            session_id,
            room: _,
            filename,
            content_type,
            bytes,
        } => send_live_upload_offer(
            client,
            state,
            transport,
            session_id,
            filename,
            content_type,
            bytes,
        ),
        ChatClientRequest::RequestUpload {
            session_id,
            room: _,
            resource_id,
        } => request_live_upload_resource(client, state, transport, session_id, resource_id),
        ChatClientRequest::RefreshRooms { session_id } => {
            refresh_live_rooms(client, state, transport, session_id)
        }
        ChatClientRequest::SetTopic { session_id, topic } => {
            set_live_room_topic(client, state, transport, session_id, topic)
        }
        ChatClientRequest::CreateRoom {
            session_id,
            room,
            topic,
        } => create_live_room(client, state, transport, session_id, room, topic),
        ChatClientRequest::ModerateUser {
            session_id,
            action,
            target,
        } => moderate_live_user(client, state, transport, session_id, action, target),
        ChatClientRequest::SyncRecent { session_id } => {
            sync_live_recent_history(client, state, transport, session_id)
        }
        ChatClientRequest::LoadOlder { session_id } => {
            load_live_history_before(client, state, transport, session_id)
        }
    };
    client.enforce_status_bounds();
    enforce_client_event_presentation_bounds(&mut events);
    events
}

pub fn reconnect_live_server<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    descriptor: OmenChatDescriptor,
) -> Vec<ChatClientEvent> {
    let Some(session) = client.session(session_id) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat reconnect session no longer exists".into(),
        }];
    };
    if session.server.destination != descriptor.server_destination {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat reconnect destination changed".into(),
        }];
    }
    let retired_echoes = state.retire_session_link_state(session_id);
    let session = client
        .session_mut(session_id)
        .expect("reconnect session was validated above");
    session
        .events
        .retain(|event| !retired_echoes.contains(&(event.room_id, event.event_id)));
    session.status = "live link connected; reopening OMENchat session".into();

    let mut events = vec![ChatClientEvent::ServerOpened {
        session_id,
        server: session.server.clone(),
    }];
    events.extend(send_session_open_and_join(
        client,
        state,
        transport,
        session_id,
        descriptor.local_display_name.as_deref(),
    ));
    enforce_client_event_presentation_bounds(&mut events);
    events
}

pub fn drain_live_events<T: ChatLinkTransport>(
    client: &mut ChatClient,
    transport: &mut T,
    preferred_session_id: Option<ChatSessionId>,
) -> Vec<ChatClientEvent> {
    drain_live_events_inner(client, None, transport, preferred_session_id)
}

pub fn drain_live_events_with_state<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    preferred_session_id: Option<ChatSessionId>,
) -> Vec<ChatClientEvent> {
    drain_live_events_inner(client, Some(state), transport, preferred_session_id)
}

fn drain_live_events_inner<T: ChatLinkTransport>(
    client: &mut ChatClient,
    mut state: Option<&mut LiveChatClientState>,
    transport: &mut T,
    preferred_session_id: Option<ChatSessionId>,
) -> Vec<ChatClientEvent> {
    let mut events = Vec::new();
    loop {
        match recv_chat_event(transport) {
            Ok(Some(link_event)) => apply_live_link_event(
                client,
                state.as_deref_mut(),
                transport,
                preferred_session_id,
                link_event,
                &mut events,
            ),
            Ok(None) => break,
            Err(error) => {
                events.push(ChatClientEvent::Error {
                    session_id: preferred_session_id,
                    message: format!("OMENchat live frame decode failed: {error}"),
                });
                break;
            }
        }
    }
    client.enforce_status_bounds();
    enforce_client_event_presentation_bounds(&mut events);
    events
}

pub fn ping_live_session<T: ChatLinkTransport>(
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
) -> Option<ChatClientEvent> {
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return Some(sequence_space_exhausted_event(session_id)),
    };
    send_frame_or_error(
        transport,
        Frame::new(ChatOp::Ping, seq, None, FrameBody::Empty),
        Some(session_id),
    )
}

fn open_live_server<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    descriptor: OmenChatDescriptor,
) -> Vec<ChatClientEvent> {
    let local_display_name = descriptor.local_display_name.clone();
    let session_id = client.reserve_session_id();
    let server = ChatServerSummary {
        server_id: descriptor.server_destination.clone(),
        destination: descriptor.server_destination,
        display_name: descriptor
            .display_name
            .unwrap_or_else(|| "OMENchat Server".to_string()),
    };
    let active_room = ChatRoomSummary {
        server_id: server.server_id.clone(),
        room_id: 1,
        name: descriptor
            .rooms_hint
            .first()
            .cloned()
            .unwrap_or_else(|| "lobby".to_string()),
        topic: None,
        unread: 0,
        joined: false,
    };
    let session_capacity_reached = client.sessions().len() >= CHAT_CLIENT_MAX_SESSIONS;
    if !client.push_session(ChatSessionView {
        session_id,
        server,
        active_room: active_room.clone(),
        users: Vec::new(),
        events: Vec::new(),
        rooms: vec![active_room.clone()],
        status: "live link connected; opening OMENchat session".into(),
    }) {
        let message = if session_capacity_reached {
            "OMENchat client session limit reached; close a session before opening another"
        } else {
            "OMENchat descriptor metadata exceeds client limits"
        };
        return vec![ChatClientEvent::Error {
            session_id: None,
            message: message.into(),
        }];
    }

    let mut events = vec![ChatClientEvent::ServerOpened {
        session_id,
        server: client
            .session(session_id)
            .expect("new live session")
            .server
            .clone(),
    }];

    events.extend(send_session_open_and_join(
        client,
        state,
        transport,
        session_id,
        local_display_name.as_deref(),
    ));
    events
}

fn send_session_open_and_join<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    local_display_name: Option<&str>,
) -> Vec<ChatClientEvent> {
    let mut events = Vec::new();
    client.mark_reactions_stale(session_id);
    client.mark_message_revisions_stale(session_id);
    let [session_open_seq, join_seq] = match state.reserve_seq_pair(session_id) {
        Ok(sequences) => sequences,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    state.durable_sessions.remove(&session_id);
    state.durable_notice_ack_sessions.remove(&session_id);
    state.durable_requests.remove(&session_id);
    state.reply_mentions_requests.remove(&session_id);
    state.reply_mentions_sessions.remove(&session_id);
    state.reaction_requests.remove(&session_id);
    state.reaction_sessions.remove(&session_id);
    state.message_revision_sessions.remove(&session_id);
    state.local_user_ids.remove(&session_id);
    let mut durable_requested = false;
    let mut session_open_body = local_display_name
        .map(|name| {
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::String(bounded_chat_text(name.trim(), CHAT_USER_DISPLAY_MAX_BYTES)),
            ])
        })
        .unwrap_or(FrameBody::Empty);
    if let Some(client_instance_id) = state
        .client_instance_id
        .filter(|_| state.durable_mutation_owner_ready)
    {
        session_open_body = match with_session_open_negotiation(
            session_open_body,
            &SessionOpenNegotiation {
                requested_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    DURABLE_NOTICE_ACK_CAPABILITY.into(),
                    REPLY_MENTIONS_CAPABILITY.into(),
                    REACTIONS_CAPABILITY.into(),
                ],
                client_instance_id: Some(client_instance_id),
            },
        ) {
            Ok(body) => body,
            Err(error) => {
                return vec![ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: format!("OMENchat capability negotiation failed: {error}"),
                }];
            }
        };
        durable_requested = true;
    }
    if let Err(error) = send_chat_frame(
        transport,
        &Frame::new(
            ChatOp::SessionOpen,
            session_open_seq,
            None,
            session_open_body,
        ),
    ) {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: format!("OMENchat live session open failed: {error}"),
        });
        return events;
    }
    if durable_requested {
        state.durable_requests.insert(session_id);
        state.reply_mentions_requests.insert(session_id);
        state.reaction_requests.insert(session_id);
    }

    let room_name = client
        .session(session_id)
        .map(|session| session.active_room.name.clone())
        .unwrap_or_else(|| "lobby".to_string());
    if let Err(error) = send_chat_frame(
        transport,
        &Frame::new(ChatOp::JoinRoom, join_seq, None, FrameBody::Text(room_name)),
    ) {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: format!("OMENchat live join failed: {error}"),
        });
        return events;
    }

    events.extend(drain_live_events(client, transport, Some(session_id)));
    events
}

fn part_live_room<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    room: Option<String>,
) -> Vec<ChatClientEvent> {
    if room.as_deref().is_some_and(|room| {
        !chat_text_fits(
            room.trim().trim_start_matches('#'),
            CHAT_ROOM_NAME_MAX_BYTES,
        )
    }) {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "room name exceeds client limits".into(),
        }];
    }
    let Some(room_id) = client.session(session_id).and_then(|session| {
        room.as_deref()
            .and_then(|name| {
                let name = name.trim().trim_start_matches('#');
                session
                    .rooms
                    .iter()
                    .find(|room| room.name.eq_ignore_ascii_case(name))
                    .map(|room| room.room_id)
            })
            .or(Some(session.active_room.room_id))
    }) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    send_frame_or_error(
        transport,
        Frame::new(ChatOp::PartRoom, seq, Some(room_id), FrameBody::Empty),
        Some(session_id),
    )
    .map_or_else(
        || drain_live_events(client, transport, Some(session_id)),
        |event| vec![event],
    )
}

fn send_live_room_text<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    body: String,
    op: ChatOp,
) -> Vec<ChatClientEvent> {
    let Some(room_id) = client
        .session(session_id)
        .map(|session| session.active_room.room_id)
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let tracks_server_acceptance = matches!(op, ChatOp::RoomMessage | ChatOp::RoomAction);
    if tracks_server_acceptance
        && (state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
            || state.pending_local_echo_session_items(session_id)
                >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION)
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        let message =
            "OMENchat pending message queue is full; wait for server acceptance or reconnect";
        if let Some(session) = client.session_mut(session_id) {
            session.status = message.into();
        }
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }];
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let frame = Frame::new(op, seq, Some(room_id), FrameBody::Text(body.clone()));
    match send_frame_or_error(transport, frame, Some(session_id)) {
        Some(event) => vec![event],
        None => {
            if tracks_server_acceptance {
                let Some(local_echo) =
                    append_pending_local_echo(client, session_id, room_id, seq, body, op, None)
                else {
                    return drain_live_events(client, transport, Some(session_id));
                };
                state.pending_local_echoes.insert(
                    (session_id, seq),
                    PendingLocalEcho {
                        session_id,
                        room_id,
                        temp_event_id: Some(local_echo.event_id),
                        mutation_id: None,
                        command_result: None,
                    },
                );
                let mut events = vec![ChatClientEvent::EventAppended {
                    session_id,
                    event: local_echo,
                }];
                events.extend(drain_live_events_with_state(
                    client,
                    state,
                    transport,
                    Some(session_id),
                ));
                events
            } else {
                drain_live_events_with_state(client, state, transport, Some(session_id))
            }
        }
    }
}

pub fn send_uncertain_durable_room_text<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    intent: &OutboundMutationIntent,
) -> Vec<ChatClientEvent> {
    let error = |message: &str| {
        vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }]
    };
    if !state.durable_mutations_negotiated(session_id) {
        return error("durable OMENchat mutation was not negotiated for this live session");
    }
    if intent.op == ChatOp::RoomNotice && !state.durable_notice_ack_negotiated(session_id) {
        return error("durable OMENchat room notices were not negotiated for this live session");
    }
    if intent.state != OutboundMutationState::SentUncertain {
        return error("durable OMENchat mutation must be persisted as uncertain before sending");
    }
    if intent.expires_at <= current_unix_secs() {
        return error("durable OMENchat mutation has expired");
    }
    if state.client_instance_id != Some(intent.client_instance_id) {
        return error("durable OMENchat mutation belongs to a different client instance");
    }
    let Some(session) = client.session(session_id) else {
        return error("OMENchat live session is not available");
    };
    if session.server.destination != intent.server_destination {
        return error("durable OMENchat mutation belongs to a different server");
    }
    let room_id = session.active_room.room_id;
    if intent.room_id != Some(room_id) {
        return error("durable OMENchat mutation belongs to a different room");
    }
    if !matches!(
        intent.op,
        ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice
    ) {
        return error("durable OMENchat mutation operation is not enabled for live client sending");
    }
    if !matches!(
        canonical_mutation_request_hash(intent.op, intent.room_id, &intent.body),
        Ok(request_hash) if request_hash == intent.request_hash
    ) {
        return error("durable OMENchat mutation request hash does not match its stored request");
    }
    let (body, metadata) = match &intent.body {
        FrameBody::Text(body) => (body.clone(), None),
        body if intent.op == ChatOp::RoomMessage => {
            if !state.reply_mentions_negotiated(session_id) {
                return error(
                    "durable OMENchat reply/mention retry requires reply-mentions-v1 negotiation",
                );
            }
            let rich = match RichMessageBody::from_frame_body(body) {
                Ok(rich) => rich,
                Err(_) => return error("durable OMENchat rich message body is invalid"),
            };
            if rich
                .reply_to
                .is_some_and(|reference| reference.room_id != room_id)
            {
                return error("durable OMENchat reply belongs to a different room");
            }
            let metadata = ChatMessageMetadata {
                reply_to_event_id: rich.reply_to.map(|reference| reference.event_id),
                mentioned_user_ids: rich.mentioned_user_ids,
            };
            (rich.body, Some(metadata))
        }
        _ => return error("durable OMENchat room text body is invalid"),
    };
    if state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
        || state.pending_local_echo_session_items(session_id)
            >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        return error(
            "OMENchat pending message queue is full; wait for server acceptance or reconnect",
        );
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let envelope = match (DurableMutationEnvelope {
        mutation_id: intent.mutation_id,
        request_hash: intent.request_hash,
        body: intent.body.clone(),
    })
    .into_frame_body()
    {
        Ok(body) => body,
        Err(error) => {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: format!("durable OMENchat mutation envelope is invalid: {error}"),
            }];
        }
    };
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(intent.op, seq, Some(room_id), envelope),
        Some(session_id),
    ) {
        return vec![event];
    }
    let Some(local_echo) =
        append_pending_local_echo(client, session_id, room_id, seq, body, intent.op, metadata)
    else {
        return drain_live_events(client, transport, Some(session_id));
    };
    state.pending_local_echoes.insert(
        (session_id, seq),
        PendingLocalEcho {
            session_id,
            room_id,
            temp_event_id: Some(local_echo.event_id),
            mutation_id: Some(intent.mutation_id),
            command_result: None,
        },
    );
    let mut events = vec![ChatClientEvent::EventAppended {
        session_id,
        event: local_echo,
    }];
    events.extend(drain_live_events_with_state(
        client,
        state,
        transport,
        Some(session_id),
    ));
    events
}

pub fn send_uncertain_durable_reaction<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    intent: &OutboundMutationIntent,
) -> Vec<ChatClientEvent> {
    let error = |message: &str| {
        vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }]
    };
    if !state.durable_mutations_negotiated(session_id) || !state.reactions_negotiated(session_id) {
        return error("durable OMENchat reactions were not negotiated for this live session");
    }
    if intent.op != ChatOp::RoomReaction
        || intent.state != OutboundMutationState::SentUncertain
        || intent.expires_at <= current_unix_secs()
    {
        return error("durable OMENchat reaction is not eligible for transmission");
    }
    if state.client_instance_id != Some(intent.client_instance_id) {
        return error("durable OMENchat reaction belongs to a different client instance");
    }
    let Some(session) = client.session(session_id) else {
        return error("OMENchat live session is not available");
    };
    let Some(room_id) = intent.room_id else {
        return error("durable OMENchat reaction has no room identity");
    };
    if session.server.destination != intent.server_destination
        || !session.rooms.iter().any(|room| room.room_id == room_id)
    {
        return error("durable OMENchat reaction belongs to a different server or room");
    }
    let request = match ReactionRequest::from_frame_body(&intent.body) {
        Ok(request) => request,
        Err(_) => return error("durable OMENchat reaction request is invalid"),
    };
    if !session.events.iter().any(|event| {
        event.room_id == room_id
            && event.event_id == request.target_event_id
            && event.event_id <= u64::MAX.saturating_sub(1_000_000)
            && super::model::chat_event_supports_reactions(event)
    }) {
        return error("durable OMENchat reaction target is no longer retained");
    }
    if !matches!(
        canonical_mutation_request_hash(intent.op, intent.room_id, &intent.body),
        Ok(request_hash) if request_hash == intent.request_hash
    ) {
        return error("durable OMENchat reaction hash does not match its stored request");
    }
    if state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
        || state.pending_local_echo_session_items(session_id)
            >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        return error("OMENchat pending mutation queue is full; wait for server acceptance");
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let envelope = match (DurableMutationEnvelope {
        mutation_id: intent.mutation_id,
        request_hash: intent.request_hash,
        body: intent.body.clone(),
    })
    .into_frame_body()
    {
        Ok(body) => body,
        Err(envelope_error) => {
            return error(&format!(
                "durable OMENchat reaction envelope is invalid: {envelope_error}"
            ))
        }
    };
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(ChatOp::RoomReaction, seq, Some(room_id), envelope),
        Some(session_id),
    ) {
        return vec![event];
    }
    state.pending_local_echoes.insert(
        (session_id, seq),
        PendingLocalEcho {
            session_id,
            room_id,
            temp_event_id: None,
            mutation_id: Some(intent.mutation_id),
            command_result: Some(PendingCommandResult::Reaction(request)),
        },
    );
    if let Some(session) = client.session_mut(session_id) {
        session.status = "reaction request sent; awaiting server result".into();
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

pub fn send_uncertain_durable_part_room<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    intent: &OutboundMutationIntent,
) -> Vec<ChatClientEvent> {
    let error = |message: &str| {
        vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }]
    };
    if !state.durable_mutations_negotiated(session_id) {
        return error("durable OMENchat mutation was not negotiated for this live session");
    }
    if intent.state != OutboundMutationState::SentUncertain {
        return error("durable OMENchat mutation must be persisted as uncertain before sending");
    }
    if intent.expires_at <= current_unix_secs() {
        return error("durable OMENchat mutation has expired");
    }
    if state.client_instance_id != Some(intent.client_instance_id) {
        return error("durable OMENchat mutation belongs to a different client instance");
    }
    let Some(session) = client.session(session_id) else {
        return error("OMENchat live session is not available");
    };
    if session.server.destination != intent.server_destination {
        return error("durable OMENchat mutation belongs to a different server");
    }
    let Some(room_id) = intent.room_id else {
        return error("durable OMENchat room leave has no room identity");
    };
    if !session.rooms.iter().any(|room| room.room_id == room_id) {
        return error("durable OMENchat room leave belongs to an unavailable room");
    }
    if intent.op != ChatOp::PartRoom || intent.body != FrameBody::Empty {
        return error("durable OMENchat room leave request is invalid");
    }
    if !matches!(
        canonical_mutation_request_hash(intent.op, intent.room_id, &intent.body),
        Ok(request_hash) if request_hash == intent.request_hash
    ) {
        return error("durable OMENchat mutation request hash does not match its stored request");
    }
    if state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
        || state.pending_local_echo_session_items(session_id)
            >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        return error(
            "OMENchat pending mutation queue is full; wait for the server result or reconnect",
        );
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let envelope = match (DurableMutationEnvelope {
        mutation_id: intent.mutation_id,
        request_hash: intent.request_hash,
        body: intent.body.clone(),
    })
    .into_frame_body()
    {
        Ok(body) => body,
        Err(error) => {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: format!("durable OMENchat mutation envelope is invalid: {error}"),
            }];
        }
    };
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(ChatOp::PartRoom, seq, Some(room_id), envelope),
        Some(session_id),
    ) {
        return vec![event];
    }
    state.pending_local_echoes.insert(
        (session_id, seq),
        PendingLocalEcho {
            session_id,
            room_id,
            temp_event_id: None,
            mutation_id: Some(intent.mutation_id),
            command_result: Some(PendingCommandResult::Part),
        },
    );
    if let Some(session) = client.session_mut(session_id) {
        session.status =
            "room leave sent; awaiting the server result before changing local membership".into();
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

pub fn send_uncertain_durable_topic<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    intent: &OutboundMutationIntent,
) -> Vec<ChatClientEvent> {
    let error = |message: &str| {
        vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }]
    };
    if !state.durable_mutations_negotiated(session_id) {
        return error("durable OMENchat mutation was not negotiated for this live session");
    }
    if intent.state != OutboundMutationState::SentUncertain {
        return error("durable OMENchat mutation must be persisted as uncertain before sending");
    }
    if intent.expires_at <= current_unix_secs() {
        return error("durable OMENchat mutation has expired");
    }
    if state.client_instance_id != Some(intent.client_instance_id) {
        return error("durable OMENchat mutation belongs to a different client instance");
    }
    let Some(session) = client.session(session_id) else {
        return error("OMENchat live session is not available");
    };
    if session.server.destination != intent.server_destination {
        return error("durable OMENchat mutation belongs to a different server");
    }
    let room_id = session.active_room.room_id;
    if intent.room_id != Some(room_id) {
        return error("durable OMENchat topic update belongs to a different room");
    }
    if intent.op != ChatOp::Command {
        return error("durable OMENchat topic update operation is invalid");
    }
    let FrameBody::Text(command) = &intent.body else {
        return error("durable OMENchat topic update body is invalid");
    };
    let command = command.trim();
    let (name, topic) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    if name != "topic" || !chat_text_fits(topic.trim(), CHAT_ROOM_TOPIC_MAX_BYTES) {
        return error("durable OMENchat topic update body is invalid");
    }
    if !matches!(
        canonical_mutation_request_hash(intent.op, intent.room_id, &intent.body),
        Ok(request_hash) if request_hash == intent.request_hash
    ) {
        return error("durable OMENchat mutation request hash does not match its stored request");
    }
    if state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
        || state.pending_local_echo_session_items(session_id)
            >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        return error(
            "OMENchat pending mutation queue is full; wait for the server result or reconnect",
        );
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let envelope = match (DurableMutationEnvelope {
        mutation_id: intent.mutation_id,
        request_hash: intent.request_hash,
        body: intent.body.clone(),
    })
    .into_frame_body()
    {
        Ok(body) => body,
        Err(error) => {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: format!("durable OMENchat mutation envelope is invalid: {error}"),
            }];
        }
    };
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(ChatOp::Command, seq, Some(room_id), envelope),
        Some(session_id),
    ) {
        return vec![event];
    }
    state.pending_local_echoes.insert(
        (session_id, seq),
        PendingLocalEcho {
            session_id,
            room_id,
            temp_event_id: None,
            mutation_id: Some(intent.mutation_id),
            command_result: Some(PendingCommandResult::Topic),
        },
    );
    if let Some(session) = client.session_mut(session_id) {
        session.status =
            "topic update sent; awaiting the server result before changing local room metadata"
                .into();
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

pub fn send_uncertain_durable_create<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    intent: &OutboundMutationIntent,
) -> Vec<ChatClientEvent> {
    let error = |message: &str| {
        vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }]
    };
    if !state.durable_mutations_negotiated(session_id) {
        return error("durable OMENchat mutation was not negotiated for this live session");
    }
    if intent.state != OutboundMutationState::SentUncertain {
        return error("durable OMENchat mutation must be persisted as uncertain before sending");
    }
    if intent.expires_at <= current_unix_secs() {
        return error("durable OMENchat mutation has expired");
    }
    if state.client_instance_id != Some(intent.client_instance_id) {
        return error("durable OMENchat mutation belongs to a different client instance");
    }
    let Some(session) = client.session(session_id) else {
        return error("OMENchat live session is not available");
    };
    if session.server.destination != intent.server_destination {
        return error("durable OMENchat mutation belongs to a different server");
    }
    if intent.room_id.is_some() || intent.op != ChatOp::Command {
        return error("durable OMENchat room creation scope is invalid");
    }
    let FrameBody::Text(command) = &intent.body else {
        return error("durable OMENchat room creation body is invalid");
    };
    let command = command.trim();
    let (name, rest) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    if name != "create" {
        return error("durable OMENchat room creation body is invalid");
    }
    let (room, topic) = rest
        .trim()
        .split_once(char::is_whitespace)
        .unwrap_or((rest.trim(), ""));
    let normalized_room_name = normalize_created_room_name(room);
    if normalized_room_name.is_empty()
        || !chat_text_fits(room, CHAT_ROOM_NAME_MAX_BYTES)
        || !chat_text_fits(topic.trim(), CHAT_ROOM_TOPIC_MAX_BYTES)
    {
        return error("durable OMENchat room creation body is invalid");
    }
    if !matches!(
        canonical_mutation_request_hash(intent.op, intent.room_id, &intent.body),
        Ok(request_hash) if request_hash == intent.request_hash
    ) {
        return error("durable OMENchat mutation request hash does not match its stored request");
    }
    if state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
        || state.pending_local_echo_session_items(session_id)
            >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        return error(
            "OMENchat pending mutation queue is full; wait for the server result or reconnect",
        );
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let envelope = match (DurableMutationEnvelope {
        mutation_id: intent.mutation_id,
        request_hash: intent.request_hash,
        body: intent.body.clone(),
    })
    .into_frame_body()
    {
        Ok(body) => body,
        Err(error) => {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: format!("durable OMENchat mutation envelope is invalid: {error}"),
            }];
        }
    };
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(ChatOp::Command, seq, None, envelope),
        Some(session_id),
    ) {
        return vec![event];
    }
    state.pending_local_echoes.insert(
        (session_id, seq),
        PendingLocalEcho {
            session_id,
            room_id: session.active_room.room_id,
            temp_event_id: None,
            mutation_id: Some(intent.mutation_id),
            command_result: Some(PendingCommandResult::Create {
                room_name: normalized_room_name,
            }),
        },
    );
    if let Some(session) = client.session_mut(session_id) {
        session.status =
            "room creation sent; awaiting the server result before adding it locally".into();
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

pub(crate) fn normalize_created_room_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('#')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .take(48)
        .collect()
}

pub(crate) fn normalized_role_label(label: &str) -> Option<(&'static str, u64)> {
    match label.trim().to_ascii_lowercase().as_str() {
        "standard" | "user" | "none" => Some(("standard", 0)),
        "trusted" | "trust" => Some(("trusted", CHAT_ROLE_TRUSTED)),
        "mod" | "moderator" => Some(("mod", CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR)),
        "admin" | "administrator" => Some((
            "admin",
            CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR | CHAT_ROLE_ADMIN,
        )),
        _ => None,
    }
}

pub(crate) fn durable_user_target_is_correlatable(session: &ChatSessionView, target: &str) -> bool {
    let target = target.trim().trim_start_matches('@');
    if target.is_empty() || !chat_text_fits(target, CHAT_USER_DISPLAY_MAX_BYTES + 32) {
        return false;
    }
    if target
        .parse::<u32>()
        .ok()
        .is_some_and(|user_id| session.users.iter().any(|user| user.user_id == user_id))
        || session
            .users
            .iter()
            .any(|user| user.display_name.eq_ignore_ascii_case(target))
    {
        return true;
    }
    !target.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn send_uncertain_durable_user_command<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    intent: &OutboundMutationIntent,
) -> Vec<ChatClientEvent> {
    let error = |message: &str| {
        vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }]
    };
    if !state.durable_mutations_negotiated(session_id) {
        return error("durable OMENchat mutation was not negotiated for this live session");
    }
    if intent.state != OutboundMutationState::SentUncertain {
        return error("durable OMENchat mutation must be persisted as uncertain before sending");
    }
    if intent.expires_at <= current_unix_secs() {
        return error("durable OMENchat mutation has expired");
    }
    if state.client_instance_id != Some(intent.client_instance_id) {
        return error("durable OMENchat mutation belongs to a different client instance");
    }
    let Some(session) = client.session(session_id) else {
        return error("OMENchat live session is not available");
    };
    if session.server.destination != intent.server_destination {
        return error("durable OMENchat mutation belongs to a different server");
    }
    let Some(room_id) = intent.room_id else {
        return error("durable OMENchat user command has no room identity");
    };
    if !session.rooms.iter().any(|room| room.room_id == room_id) || intent.op != ChatOp::Command {
        return error("durable OMENchat user command scope is invalid");
    }
    let FrameBody::Text(command) = &intent.body else {
        return error("durable OMENchat user command body is invalid");
    };
    let command = command.trim();
    let (name, rest) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    let (pending_command, target) = match name {
        "role" => {
            let (target, role) = rest
                .trim()
                .split_once(char::is_whitespace)
                .unwrap_or((rest.trim(), ""));
            let Some((canonical_role, role_bits)) = normalized_role_label(role) else {
                return error("durable OMENchat role command body is invalid");
            };
            if canonical_role != role {
                return error("durable OMENchat role command is not canonical");
            }
            (PendingUserCommand::Role { role_bits }, target)
        }
        "unban" => (PendingUserCommand::Unban, rest.trim()),
        "kick" => (PendingUserCommand::Kick, rest.trim()),
        "ban" => (PendingUserCommand::Ban, rest.trim()),
        "mute" => (PendingUserCommand::Mute, rest.trim()),
        "unmute" => (PendingUserCommand::Unmute, rest.trim()),
        _ => return error("durable OMENchat user command body is invalid"),
    };
    let target = target.trim().trim_start_matches('@');
    if !durable_user_target_is_correlatable(session, target) {
        return error("durable OMENchat user target cannot be correlated from the result");
    }
    if !matches!(
        canonical_mutation_request_hash(intent.op, intent.room_id, &intent.body),
        Ok(request_hash) if request_hash == intent.request_hash
    ) {
        return error("durable OMENchat mutation request hash does not match its stored request");
    }
    if state.pending_local_echoes.len() >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS
        || state.pending_local_echo_session_items(session_id)
            >= LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
    {
        state.rejected_pending_local_echoes = state.rejected_pending_local_echoes.saturating_add(1);
        return error(
            "OMENchat pending mutation queue is full; wait for the server result or reconnect",
        );
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let envelope = match (DurableMutationEnvelope {
        mutation_id: intent.mutation_id,
        request_hash: intent.request_hash,
        body: intent.body.clone(),
    })
    .into_frame_body()
    {
        Ok(body) => body,
        Err(error) => {
            return vec![ChatClientEvent::Error {
                session_id: Some(session_id),
                message: format!("durable OMENchat mutation envelope is invalid: {error}"),
            }];
        }
    };
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(ChatOp::Command, seq, Some(room_id), envelope),
        Some(session_id),
    ) {
        return vec![event];
    }
    state.pending_local_echoes.insert(
        (session_id, seq),
        PendingLocalEcho {
            session_id,
            room_id,
            temp_event_id: None,
            mutation_id: Some(intent.mutation_id),
            command_result: Some(PendingCommandResult::User {
                command: pending_command,
                target: target.to_owned(),
            }),
        },
    );
    if let Some(session) = client.session_mut(session_id) {
        session.status =
            format!("{name} sent; awaiting the exact server result before applying it locally");
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

fn append_pending_local_echo(
    client: &mut ChatClient,
    session_id: ChatSessionId,
    room_id: RoomId,
    seq: u32,
    body: String,
    op: ChatOp,
    metadata: Option<ChatMessageMetadata>,
) -> Option<ChatEvent> {
    let server_id = client
        .session(session_id)
        .map(|session| session.server.server_id.clone())?;
    let event = ChatEvent {
        server_id,
        room_id,
        event_id: local_echo_event_id(seq),
        actor_user_id: None,
        actor_display_name: Some("You".into()),
        at_unix: current_unix_secs(),
        kind: match op {
            ChatOp::RoomAction => ChatEventKind::Action { body },
            ChatOp::RoomNotice => ChatEventKind::Notice { body },
            _ => match metadata {
                Some(metadata) => ChatEventKind::RichMessage { body, metadata },
                None => ChatEventKind::Message { body },
            },
        },
    };
    append_event(client, session_id, event.clone(), false);
    Some(event)
}

fn local_echo_event_id(seq: u32) -> u64 {
    u64::MAX.saturating_sub(seq as u64)
}

fn is_local_echo_event_id(event_id: u64) -> bool {
    event_id > u64::MAX.saturating_sub(1_000_000)
}

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn send_live_upload_offer<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    filename: String,
    content_type: Option<String>,
    mut bytes: Vec<u8>,
) -> Vec<ChatClientEvent> {
    let Some(room_id) = client
        .session(session_id)
        .map(|session| session.active_room.room_id)
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let filename = filename.trim().to_owned();
    if filename.is_empty() || bytes.is_empty() {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "usage: /upload <path> with a non-empty file".into(),
        }];
    }
    bytes.shrink_to_fit();
    let owned_bytes = bytes.capacity();
    let pending_metrics = state.pending_upload_metrics();
    if filename.len() > CHAT_UPLOAD_FILENAME_MAX_BYTES
        || content_type
            .as_ref()
            .is_some_and(|value| value.len() > CHAT_CONTENT_TYPE_MAX_BYTES)
        || owned_bytes > LIVE_PENDING_UPLOAD_MAX_RESOURCE_BYTES
        || pending_metrics.items >= LIVE_PENDING_UPLOAD_MAX_ITEMS
        || pending_metrics.bytes.saturating_add(owned_bytes) > LIVE_PENDING_UPLOAD_MAX_BYTES
    {
        state.rejected_pending_uploads = state.rejected_pending_uploads.saturating_add(1);
        let message = "OMENchat pending upload queue is full or the upload exceeds client limits";
        if let Some(session) = client.session_mut(session_id) {
            session.status = message.into();
        }
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: message.into(),
        }];
    }
    let byte_len = bytes.len() as u64;
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    state.pending_uploads.insert(
        (session_id, seq),
        PendingLiveUpload {
            session_id,
            filename: filename.clone(),
            content_type: content_type.clone(),
            bytes,
        },
    );
    let mut fields = vec![
        FrameValue::String(filename.clone()),
        FrameValue::U64(byte_len),
    ];
    fields.push(
        content_type
            .map(FrameValue::String)
            .unwrap_or(FrameValue::Nil),
    );
    if let Some(event) = send_frame_or_error(
        transport,
        Frame::new(
            ChatOp::UploadOffer,
            seq,
            Some(room_id),
            FrameBody::Fields(fields),
        ),
        Some(session_id),
    ) {
        state.pending_uploads.remove(&(session_id, seq));
        return vec![event];
    }
    if let Some(session) = client.session_mut(session_id) {
        session.status = format!("offered upload {filename} ({})", human_bytes(byte_len));
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

fn request_live_upload_resource<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    resource_id: String,
) -> Vec<ChatClientEvent> {
    let Some(room_id) = client
        .session(session_id)
        .map(|session| session.active_room.room_id)
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let resource_id = resource_id.trim().to_owned();
    if resource_id.is_empty() || resource_id.len() > CHAT_RESOURCE_ID_MAX_BYTES {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "upload resource id is empty or exceeds client limits".into(),
        }];
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let frame = Frame::new(
        ChatOp::UploadFetch,
        seq,
        Some(room_id),
        FrameBody::Fields(vec![FrameValue::String(resource_id.clone())]),
    );
    if let Some(event) = send_frame_or_error(transport, frame, Some(session_id)) {
        return vec![event];
    }
    if let Some(session) = client.session_mut(session_id) {
        session.status = format!("requested upload resource {resource_id}");
    }
    drain_live_events_with_state(client, state, transport, Some(session_id))
}

fn load_live_history_before<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
) -> Vec<ChatClientEvent> {
    let Some(session) = client.session(session_id) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let before = session
        .events
        .iter()
        .filter(|event| event.room_id == session.active_room.room_id)
        .map(|event| event.event_id)
        .min()
        .unwrap_or(u64::MAX);
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let frame = Frame::new(
        ChatOp::HistoryBefore,
        seq,
        Some(session.active_room.room_id),
        FrameBody::Fields(vec![FrameValue::U64(before)]),
    );
    if let Some(event) = send_frame_or_error(transport, frame, Some(session_id)) {
        return vec![event];
    }
    if let Some(session) = client.session_mut(session_id) {
        session.status = "requested older room history".into();
    }
    drain_live_events(client, transport, Some(session_id))
}

fn sync_live_recent_history<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
) -> Vec<ChatClientEvent> {
    let Some(session) = client.session(session_id) else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let fingerprint = recent_history_fingerprint(session);
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    let frame = Frame::new(
        ChatOp::HistoryRecent,
        seq,
        Some(session.active_room.room_id),
        FrameBody::Fields(vec![
            FrameValue::U64(fingerprint.first_event_id),
            FrameValue::U64(fingerprint.last_event_id),
            FrameValue::U64(fingerprint.event_count),
            FrameValue::U64(fingerprint.checksum),
        ]),
    );
    tracing::debug!(
        session_id,
        room_id = session.active_room.room_id,
        first_event_id = fingerprint.first_event_id,
        last_event_id = fingerprint.last_event_id,
        event_count = fingerprint.event_count,
        checksum = fingerprint.checksum,
        "OMENchat requesting bounded recent room history sync"
    );
    if let Some(event) = send_frame_or_error(transport, frame, Some(session_id)) {
        return vec![event];
    }
    if let Some(session) = client.session_mut(session_id) {
        session.status = "requested recent room history".into();
    }
    drain_live_events(client, transport, Some(session_id))
}

fn refresh_live_rooms<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
) -> Vec<ChatClientEvent> {
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    send_frame_or_error(
        transport,
        Frame::new(ChatOp::Command, seq, None, FrameBody::Text("rooms".into())),
        Some(session_id),
    )
    .map_or_else(
        || drain_live_events(client, transport, Some(session_id)),
        |event| vec![event],
    )
}

fn set_live_room_topic<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    topic: String,
) -> Vec<ChatClientEvent> {
    let Some(room_id) = client
        .session(session_id)
        .map(|session| session.active_room.room_id)
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    if !chat_text_fits(topic.trim(), CHAT_ROOM_TOPIC_MAX_BYTES) {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "room topic exceeds client limits".into(),
        }];
    }
    let command = format!("topic {}", topic.trim()).trim().to_owned();
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    send_frame_or_error(
        transport,
        Frame::new(
            ChatOp::Command,
            seq,
            Some(room_id),
            FrameBody::Text(command),
        ),
        Some(session_id),
    )
    .map_or_else(
        || drain_live_events(client, transport, Some(session_id)),
        |event| vec![event],
    )
}

fn create_live_room<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    room: String,
    topic: Option<String>,
) -> Vec<ChatClientEvent> {
    if client.session(session_id).is_none() {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    }
    let room = room.trim().trim_start_matches('#');
    if room.is_empty()
        || !chat_text_fits(room, CHAT_ROOM_NAME_MAX_BYTES)
        || topic
            .as_deref()
            .is_some_and(|topic| !chat_text_fits(topic.trim(), CHAT_ROOM_TOPIC_MAX_BYTES))
    {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "room name or topic is empty or exceeds client limits".into(),
        }];
    }
    let command = topic
        .as_deref()
        .map(str::trim)
        .filter(|topic| !topic.is_empty())
        .map(|topic| format!("create {room} {topic}"))
        .unwrap_or_else(|| format!("create {room}"));
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    send_frame_or_error(
        transport,
        Frame::new(ChatOp::Command, seq, None, FrameBody::Text(command)),
        Some(session_id),
    )
    .map_or_else(
        || drain_live_events(client, transport, Some(session_id)),
        |event| vec![event],
    )
}

fn moderate_live_user<T: ChatLinkTransport>(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    transport: &mut T,
    session_id: ChatSessionId,
    action: String,
    target: String,
) -> Vec<ChatClientEvent> {
    let Some(room_id) = client
        .session(session_id)
        .map(|session| session.active_room.room_id)
    else {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat live session is not available".into(),
        }];
    };
    let action = action.trim().to_ascii_lowercase();
    let target = target.trim();
    if target.is_empty()
        || !chat_text_fits(target, CHAT_USER_DISPLAY_MAX_BYTES + 32)
        || !matches!(
            action.as_str(),
            "kick" | "ban" | "unban" | "mute" | "unmute" | "role"
        )
    {
        return vec![ChatClientEvent::Error {
            session_id: Some(session_id),
            message:
                "usage: /kick <user>, /ban <user>, /unban <user>, /mute <user>, /unmute <user>, or /role <user> <role>".into(),
        }];
    }
    let seq = match state.reserve_seq(session_id) {
        Ok(seq) => seq,
        Err(_) => return vec![sequence_space_exhausted_event(session_id)],
    };
    send_frame_or_error(
        transport,
        Frame::new(
            ChatOp::Command,
            seq,
            Some(room_id),
            FrameBody::Text(format!("{action} {target}")),
        ),
        Some(session_id),
    )
    .map_or_else(
        || drain_live_events(client, transport, Some(session_id)),
        |event| vec![event],
    )
}

fn send_frame_or_error<T: ChatLinkTransport>(
    transport: &mut T,
    frame: Frame,
    session_id: Option<ChatSessionId>,
) -> Option<ChatClientEvent> {
    send_chat_frame(transport, &frame)
        .err()
        .map(|error| ChatClientEvent::Error {
            session_id,
            message: format!("OMENchat live send failed: {error}"),
        })
}

fn apply_live_link_event(
    client: &mut ChatClient,
    state: Option<&mut LiveChatClientState>,
    transport: &mut dyn ChatLinkTransport,
    preferred_session_id: Option<ChatSessionId>,
    link_event: ChatLinkEvent,
    events: &mut Vec<ChatClientEvent>,
) {
    match link_event {
        ChatLinkEvent::Frame(frame) => apply_frame_with_state(
            client,
            state,
            transport,
            preferred_session_id,
            frame,
            events,
        ),
        ChatLinkEvent::InlineBatch {
            op,
            room_id,
            values,
        }
        | ChatLinkEvent::ResourceBatch {
            op,
            room_id,
            values,
            ..
        } => {
            let reactions_negotiated = preferred_session_id.is_some_and(|session_id| {
                state
                    .as_deref()
                    .is_some_and(|state| state.reactions_negotiated(session_id))
            });
            let message_revisions_negotiated = preferred_session_id.is_some_and(|session_id| {
                state
                    .as_deref()
                    .is_some_and(|state| state.message_revisions_negotiated(session_id))
            });
            apply_batch(
                client,
                preferred_session_id,
                op,
                room_id,
                values,
                BatchCapabilities {
                    reactions: reactions_negotiated,
                    message_revisions: message_revisions_negotiated,
                },
                events,
            );
        }
        ChatLinkEvent::UploadResource {
            resource_id,
            filename,
            content_type,
            data,
            ..
        } => {
            let Some(session_id) = preferred_session_id else {
                return;
            };
            let bytes = data.len();
            if let Some(session) = client.session_mut(session_id) {
                session.status = format!(
                    "upload resource received: {filename} ({})",
                    human_bytes(bytes as u64)
                );
            }
            events.push(ChatClientEvent::UploadResourceAvailable {
                session_id,
                resource_id,
                filename,
                content_type,
                bytes: data,
            });
        }
    }
}

fn apply_frame(
    client: &mut ChatClient,
    preferred_session_id: Option<ChatSessionId>,
    frame: Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let mut transport = NoopChatTransport;
    apply_frame_with_state(
        client,
        None,
        &mut transport,
        preferred_session_id,
        frame,
        events,
    );
}

struct NoopChatTransport;

impl ChatLinkTransport for NoopChatTransport {
    fn send_frame(&mut self, _frame_bytes: Vec<u8>) -> anyhow::Result<()> {
        Ok(())
    }

    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn fetch_resource(&mut self, _resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

fn apply_frame_with_state(
    client: &mut ChatClient,
    mut state: Option<&mut LiveChatClientState>,
    transport: &mut dyn ChatLinkTransport,
    preferred_session_id: Option<ChatSessionId>,
    frame: Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    match frame.op {
        ChatOp::SessionAccept => {
            let accepted_capabilities = parse_session_accept_negotiation(&frame.body)
                .ok()
                .flatten()
                .map(|negotiation| negotiation.accepted_capabilities)
                .unwrap_or_default();
            let durable_accepted = accepted_capabilities
                .iter()
                .any(|capability| capability == DURABLE_MUTATION_CAPABILITY);
            let durable_notice_ack_accepted = accepted_capabilities
                .iter()
                .any(|capability| capability == DURABLE_NOTICE_ACK_CAPABILITY);
            let reply_mentions_accepted = accepted_capabilities
                .iter()
                .any(|capability| capability == REPLY_MENTIONS_CAPABILITY);
            let reactions_accepted = accepted_capabilities
                .iter()
                .any(|capability| capability == REACTIONS_CAPABILITY);
            if let (Some(session_id), Some(state)) = (preferred_session_id, state) {
                // The desktop does not request message-revisions-v1 yet. An
                // unsolicited acceptance must never activate dormant state.
                state.message_revision_sessions.remove(&session_id);
                let request_pending = state.durable_requests.remove(&session_id);
                let reply_mentions_requested = state.reply_mentions_requests.remove(&session_id);
                let reactions_requested = state.reaction_requests.remove(&session_id);
                let already_accepted = state.durable_sessions.contains(&session_id);
                if durable_accepted
                    && state.client_instance_id.is_some()
                    && (request_pending || already_accepted)
                {
                    state.durable_sessions.insert(session_id);
                    if durable_notice_ack_accepted {
                        state.durable_notice_ack_sessions.insert(session_id);
                    } else {
                        state.durable_notice_ack_sessions.remove(&session_id);
                    }
                    if reply_mentions_requested && reply_mentions_accepted {
                        state.reply_mentions_sessions.insert(session_id);
                    } else {
                        state.reply_mentions_sessions.remove(&session_id);
                        state.local_user_ids.remove(&session_id);
                    }
                    if reactions_requested && reactions_accepted {
                        state.reaction_sessions.insert(session_id);
                    } else {
                        state.reaction_sessions.remove(&session_id);
                    }
                } else {
                    state.durable_sessions.remove(&session_id);
                    state.durable_notice_ack_sessions.remove(&session_id);
                    state.reply_mentions_sessions.remove(&session_id);
                    state.reaction_sessions.remove(&session_id);
                    state.local_user_ids.remove(&session_id);
                }
            }
            let policy = body_values(&frame.body).map(|values| {
                let upload_quota_bytes = values.get(3)?.as_u64()?;
                let ping_interval_seconds = values.get(4)?.as_u64()?.clamp(5, 600);
                let upload_max_file_bytes = values
                    .get(5)
                    .and_then(FrameValueExt::as_u64)
                    .unwrap_or(512 * 1024);
                Some((
                    upload_quota_bytes,
                    upload_max_file_bytes,
                    ping_interval_seconds,
                ))
            });
            let motd = body_values(&frame.body)
                .and_then(|values| values.get(2))
                .and_then(FrameValueExt::as_str)
                .map(str::trim)
                .filter(|motd| !motd.is_empty())
                .map(|motd| bounded_chat_text(motd, CHAT_MOTD_MAX_BYTES));
            let mut rooms = body_values(&frame.body)
                .and_then(|values| values.get(1))
                .and_then(FrameValueExt::as_array)
                .map(|values| {
                    let server_id = preferred_session_id
                        .and_then(|id| client.session(id))
                        .map(|session| session.server.server_id.clone())
                        .unwrap_or_default();
                    values
                        .iter()
                        .filter_map(|value| parse_room(value, server_id.clone(), false))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let active_room_id = preferred_session_id
                .and_then(|id| client.session(id))
                .map(|session| session.active_room.room_id)
                .unwrap_or(1);
            let room_catalog_dropped = enforce_room_catalog_bounds(&mut rooms, active_room_id);
            if let Some(session_id) = preferred_session_id {
                if let Some(motd) = motd {
                    events.push(ChatClientEvent::ServerMotd { session_id, motd });
                }
                if !rooms.is_empty() {
                    if let Some(session) = client.session_mut(session_id) {
                        session.rooms = merge_rooms(session.rooms.clone(), rooms.clone());
                        session.enforce_catalog_bounds();
                    }
                    events.push(ChatClientEvent::RoomsUpdated { session_id, rooms });
                }
                if let Some(Some((
                    upload_quota_bytes,
                    upload_max_file_bytes,
                    ping_interval_seconds,
                ))) = policy
                {
                    events.push(ChatClientEvent::ServerPolicy {
                        session_id,
                        upload_quota_bytes,
                        upload_max_file_bytes,
                        ping_interval_seconds,
                    });
                }
            }
            if let Some(session) = preferred_session_id.and_then(|id| client.session_mut(id)) {
                session.status = if room_catalog_dropped == 0 {
                    "session accepted; joining room".into()
                } else {
                    format!(
                        "session accepted; limited oversized room catalog by {room_catalog_dropped} entries"
                    )
                };
            }
        }
        ChatOp::JoinAccept => {
            let Some(session_id) = preferred_session_id else {
                return;
            };
            let Some(room_value) = body_values(&frame.body).and_then(|values| values.first())
            else {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat join response did not include a room".into(),
                });
                return;
            };
            let Some(room) = parse_room(
                room_value,
                client
                    .session(session_id)
                    .map(|s| s.server.server_id.clone())
                    .unwrap_or_default(),
                true,
            ) else {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat join response had an invalid room shape".into(),
                });
                return;
            };
            let mut local_user_id = None;
            if let Some(state) = state.as_deref_mut() {
                if state.reply_mentions_sessions.contains(&session_id) {
                    match body_values(&frame.body)
                        .and_then(|values| values.get(1))
                        .and_then(FrameValueExt::as_u64)
                        .and_then(|user_id| u32::try_from(user_id).ok())
                        .filter(|user_id| *user_id != 0)
                    {
                        Some(user_id) => {
                            state.local_user_ids.insert(session_id, user_id);
                            local_user_id = Some(user_id);
                        }
                        None => {
                            state.reply_mentions_sessions.remove(&session_id);
                            state.local_user_ids.remove(&session_id);
                        }
                    }
                } else {
                    state.local_user_ids.remove(&session_id);
                }
            }
            if let Some(session) = client.session_mut(session_id) {
                session.rooms = merge_rooms(session.rooms.clone(), vec![room.clone()]);
                for current in &mut session.rooms {
                    if current.room_id == room.room_id {
                        current.joined = true;
                    }
                }
                session.active_room = room.clone();
                clear_room_unread(session, room.room_id);
                session.users.clear();
                session.enforce_catalog_bounds();
                session.status = "joined live room".into();
            }
            if let Some(user_id) = local_user_id {
                if client.bind_local_user_id(session_id, user_id) {
                    events.push(ChatClientEvent::LocalUserBound {
                        session_id,
                        user_id,
                    });
                }
            }
            events.push(ChatClientEvent::RoomJoined {
                session_id,
                room,
                users: client
                    .session(session_id)
                    .map(|session| session.users.clone())
                    .unwrap_or_default(),
                latest_events: client
                    .session(session_id)
                    .map(|session| {
                        session
                            .events
                            .iter()
                            .filter(|event| event.room_id == session.active_room.room_id)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
        ChatOp::RoomEvent => {
            let Some(session_id) = preferred_session_id else {
                return;
            };
            let server_id = client
                .session(session_id)
                .map(|session| session.server.server_id.clone())
                .unwrap_or_default();
            let Some(value) = body_values(&frame.body).and_then(|values| values.first()) else {
                return;
            };
            let Some(event) = parse_event(value, server_id, frame.room_id.unwrap_or(1)) else {
                return;
            };
            let gap_detected =
                live_event_gap_detected(client, session_id, event.room_id, event.event_id);
            append_event(client, session_id, event.clone(), false);
            events.push(ChatClientEvent::EventAppended { session_id, event });
            if gap_detected {
                events.push(ChatClientEvent::HistorySyncNeeded {
                    session_id,
                    room_id: frame.room_id.unwrap_or(1),
                });
            }
        }
        ChatOp::ReactionEvent => {
            let Some(session_id) = preferred_session_id else {
                return;
            };
            let negotiated = state
                .as_deref()
                .is_some_and(|state| state.reactions_negotiated(session_id));
            if !negotiated {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "ignored OMENchat reaction event without reactions-v1 negotiation"
                        .into(),
                });
                return;
            }
            let Some(room_id) = frame.room_id else {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat reaction event did not identify a room".into(),
                });
                return;
            };
            let event = match ReactionEvent::from_frame_body(&frame.body) {
                Ok(event) => event,
                Err(error) => {
                    events.push(ChatClientEvent::Error {
                        session_id: Some(session_id),
                        message: format!("invalid OMENchat reaction event: {error}"),
                    });
                    return;
                }
            };
            match client.apply_reaction_event(session_id, room_id, event) {
                Ok(true) => events.push(ChatClientEvent::ReactionDeltaApplied {
                    session_id,
                    room_id,
                    event,
                }),
                Ok(false) => {}
                Err(error) => events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: error.into(),
                }),
            }
        }
        ChatOp::MessageRevisionEvent => {
            let Some(session_id) = preferred_session_id else {
                return;
            };
            let negotiated = state
                .as_deref()
                .is_some_and(|state| state.message_revisions_negotiated(session_id));
            if !negotiated {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message:
                        "ignored OMENchat message revision event without message-revisions-v1 negotiation"
                            .into(),
                });
                return;
            }
            let Some(room_id) = frame.room_id else {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat message revision event did not identify a room".into(),
                });
                return;
            };
            let event = match MessageRevisionEvent::from_frame_body(&frame.body) {
                Ok(event) => event,
                Err(error) => {
                    events.push(ChatClientEvent::Error {
                        session_id: Some(session_id),
                        message: format!("invalid OMENchat message revision event: {error}"),
                    });
                    return;
                }
            };
            match client.apply_message_revision_event(session_id, room_id, event.clone()) {
                Ok(true) => events.push(ChatClientEvent::MessageRevisionDeltaApplied {
                    session_id,
                    room_id,
                    event,
                }),
                Ok(false) => {}
                Err(error) => events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: error.into(),
                }),
            }
        }
        ChatOp::ReactionAck => {
            apply_reaction_ack(client, state, preferred_session_id, &frame, events);
        }
        ChatOp::MessageAck => {
            if let Some(state) = state {
                apply_message_ack(client, state, preferred_session_id, &frame, events);
            }
        }
        ChatOp::Error | ChatOp::SessionReject => {
            if frame.op == ChatOp::Error {
                if let Some(state) = state {
                    apply_durable_terminal_error(
                        client,
                        state,
                        preferred_session_id,
                        &frame,
                        events,
                    );
                }
            }
            events.push(ChatClientEvent::Error {
                session_id: preferred_session_id,
                message: parse_error_text(&frame.body),
            });
        }
        ChatOp::Pong => {
            if let Some(session) = preferred_session_id.and_then(|id| client.session_mut(id)) {
                session.status = "live ping acknowledged".into();
            }
        }
        ChatOp::HistoryEnd => {
            if let Some(session) = preferred_session_id.and_then(|id| client.session_mut(id)) {
                session.status = "start of room history reached".into();
            }
        }
        ChatOp::HistoryCurrent => {
            if let Some(session_id) = preferred_session_id {
                if let Some(session) = client.session_mut(session_id) {
                    session.status = "room history sync current".into();
                }
                tracing::debug!(
                    session_id,
                    room_id = frame.room_id.unwrap_or(1),
                    "OMENchat recent room history is current"
                );
                events.push(ChatClientEvent::HistorySynced {
                    session_id,
                    room_id: frame.room_id.unwrap_or(1),
                });
            }
        }
        ChatOp::CommandResult => {
            let durable_command_match = state.as_deref().and_then(|state| {
                durable_command_result_match(client, state, preferred_session_id, &frame)
            });
            match durable_command_match {
                Some(Ok(mutation_id)) => {
                    apply_command_result(client, preferred_session_id, &frame.body, events);
                    if let (Some(session_id), Some(state)) =
                        (preferred_session_id, state.as_deref_mut())
                    {
                        state.pending_local_echoes.remove(&(session_id, frame.seq));
                        events.push(ChatClientEvent::DurableMutationAcknowledged {
                            session_id,
                            mutation_id,
                        });
                    }
                }
                Some(Err(())) => {
                    events.push(ChatClientEvent::Error {
                        session_id: preferred_session_id,
                        message: "OMENchat ignored a mismatched durable command result".into(),
                    });
                }
                None => {
                    apply_command_result(client, preferred_session_id, &frame.body, events);
                }
            }
        }
        ChatOp::RoomDelta => {
            apply_room_delta(client, preferred_session_id, &frame.body, events);
        }
        ChatOp::UserDelta => {
            apply_user_delta(client, preferred_session_id, &frame.body, events);
        }
        ChatOp::UploadAccept => {
            apply_upload_accept(
                client,
                state,
                transport,
                preferred_session_id,
                &frame,
                events,
            );
        }
        ChatOp::UploadReject => {
            apply_upload_reject(client, state, preferred_session_id, &frame, events);
        }
        ChatOp::UploadComplete => {
            apply_upload_complete(client, preferred_session_id, &frame, events);
        }
        ChatOp::UploadInlineChunk => {
            apply_upload_inline_chunk(client, state, preferred_session_id, &frame, events);
        }
        _ => {}
    }
}

fn apply_reaction_ack(
    client: &mut ChatClient,
    state: Option<&mut LiveChatClientState>,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let (Some(state), Some(session_id)) = (state, preferred_session_id) else {
        return;
    };
    let Some(pending) = state
        .pending_local_echoes
        .get(&(session_id, frame.seq))
        .cloned()
    else {
        return;
    };
    let Some(PendingCommandResult::Reaction(expected)) = pending.command_result else {
        return;
    };
    if !state.reactions_negotiated(session_id) || frame.room_id != Some(pending.room_id) {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat ignored a reaction acknowledgement outside its negotiated room"
                .into(),
        });
        return;
    }
    let Ok(ack) = ReactionAck::from_frame_body(&frame.body) else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat ignored an invalid reaction acknowledgement".into(),
        });
        return;
    };
    if ack.target_event_id != expected.target_event_id
        || ack.token != expected.token
        || ack.action != expected.action
        || state.local_user_id(session_id) != Some(ack.actor_user_id)
    {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat ignored a mismatched reaction acknowledgement".into(),
        });
        return;
    }
    state.pending_local_echoes.remove(&(session_id, frame.seq));
    if let Some(session) = client.session_mut(session_id) {
        session.status = if ack.changed {
            "reaction accepted by server".into()
        } else {
            "reaction already matched the requested state".into()
        };
    }
    if let Some(mutation_id) = pending.mutation_id {
        events.push(ChatClientEvent::DurableMutationAcknowledged {
            session_id,
            mutation_id,
        });
    }
}

fn apply_upload_inline_chunk(
    client: &mut ChatClient,
    state: Option<&mut LiveChatClientState>,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let Some(state) = state else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload chunk received without live state".into(),
        });
        return;
    };
    let Some(values) = body_values(&frame.body) else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload chunk had an invalid body".into(),
        });
        return;
    };
    let Some(resource_id) = values.first().and_then(FrameValueExt::as_str) else {
        return;
    };
    let Some(filename) = values.get(1).and_then(FrameValueExt::as_str) else {
        return;
    };
    let total_len = values.get(2).and_then(FrameValueExt::as_u64).unwrap_or(0) as usize;
    let content_type = values.get(3).and_then(FrameValueExt::as_str);
    let offset = values
        .get(4)
        .and_then(FrameValueExt::as_u64)
        .unwrap_or(usize::MAX as u64) as usize;
    let Some(chunk) = values.get(5).and_then(FrameValueExt::as_bytes) else {
        return;
    };
    let done = values
        .get(6)
        .and_then(FrameValueExt::as_bool)
        .unwrap_or(false);
    if resource_id.len() > CHAT_RESOURCE_ID_MAX_BYTES
        || filename.len() > CHAT_UPLOAD_FILENAME_MAX_BYTES
        || content_type.is_some_and(|value| value.len() > CHAT_CONTENT_TYPE_MAX_BYTES)
        || total_len > LIVE_INLINE_DOWNLOAD_MAX_RESOURCE_BYTES
    {
        let remove_existing = state
            .pending_upload_downloads
            .get(resource_id)
            .is_some_and(|download| download.session_id == session_id);
        reject_inline_download(
            client,
            state,
            session_id,
            resource_id,
            "inline upload metadata exceeds client limits",
            remove_existing,
            events,
        );
        return;
    }

    if let Some(entry) = state.pending_upload_downloads.get(resource_id) {
        if entry.session_id != session_id {
            reject_inline_download(
                client,
                state,
                session_id,
                resource_id,
                "inline upload resource id belongs to another session",
                false,
                events,
            );
            return;
        }
        if entry.filename != filename
            || entry.content_type.as_deref() != content_type
            || entry.total_len != total_len
        {
            reject_inline_download(
                client,
                state,
                session_id,
                resource_id,
                "inline upload metadata changed during transfer",
                true,
                events,
            );
            return;
        }
    } else {
        let metrics = state.inline_download_metrics();
        if metrics.items >= LIVE_INLINE_DOWNLOAD_MAX_ITEMS
            || metrics.reserved_bytes.saturating_add(total_len) > LIVE_INLINE_DOWNLOAD_MAX_BYTES
        {
            reject_inline_download(
                client,
                state,
                session_id,
                resource_id,
                "inline upload queue is full",
                false,
                events,
            );
            return;
        }
        state.pending_upload_downloads.insert(
            resource_id.to_owned(),
            PendingLiveUploadDownload {
                session_id,
                filename: filename.to_owned(),
                content_type: content_type.map(ToOwned::to_owned),
                total_len,
                bytes: Vec::with_capacity(total_len.min(512 * 1024)),
                pending_chunks: BTreeMap::new(),
                done_seen: false,
            },
        );
    }

    let mut rejection = None;
    let entry = state
        .pending_upload_downloads
        .get_mut(resource_id)
        .expect("inline upload inserted or previously present");
    if done {
        entry.done_seen = true;
    }
    if offset > entry.total_len || offset.saturating_add(chunk.len()) > entry.total_len {
        rejection = Some("inline upload chunk is outside the declared resource");
    } else if offset < entry.bytes.len() {
        let end = offset.saturating_add(chunk.len());
        let duplicate = end <= entry.bytes.len() && entry.bytes[offset..end] == *chunk;
        if !duplicate {
            rejection = Some("inline upload chunk conflicts with received data");
        }
    } else if let Some(existing) = entry.pending_chunks.get(&offset) {
        if existing.as_slice() != chunk {
            rejection = Some("inline upload chunk conflicts at a pending offset");
        }
    } else if entry.pending_chunks.len() >= LIVE_INLINE_DOWNLOAD_MAX_PENDING_CHUNKS {
        rejection = Some("inline upload fragment limit exceeded");
    } else if entry.retained_payload_bytes().saturating_add(chunk.len()) > entry.total_len {
        rejection = Some("inline upload retained bytes exceed the declared resource");
    } else {
        entry
            .pending_chunks
            .entry(offset)
            .or_insert_with(|| chunk.to_vec());
    }
    if rejection.is_none() {
        while let Some(chunk) = entry.pending_chunks.remove(&entry.bytes.len()) {
            entry.bytes.extend_from_slice(&chunk);
        }
    }
    if let Some(message) = rejection {
        reject_inline_download(
            client,
            state,
            session_id,
            resource_id,
            message,
            true,
            events,
        );
        return;
    }
    let entry = state
        .pending_upload_downloads
        .get(resource_id)
        .expect("accepted inline upload remains present");
    if let Some(session) = client.session_mut(session_id) {
        session.status = format!(
            "upload resource receiving: {} / {}",
            human_bytes(entry.bytes.len() as u64),
            human_bytes(entry.total_len as u64)
        );
    }
    events.push(ChatClientEvent::UploadResourceProgress {
        session_id,
        resource_id: resource_id.to_owned(),
        filename: filename.to_owned(),
        received: entry.bytes.len() as u64,
        total: entry.total_len as u64,
    });
    if (entry.done_seen || entry.bytes.len() >= entry.total_len)
        && entry.bytes.len() >= entry.total_len
    {
        let Some(entry) = state.pending_upload_downloads.remove(resource_id) else {
            return;
        };
        if entry.bytes.len() != entry.total_len {
            events.push(ChatClientEvent::Error {
                session_id: Some(session_id),
                message: format!(
                    "OMENchat upload resource incomplete: got {}, expected {}",
                    entry.bytes.len(),
                    entry.total_len
                ),
            });
            return;
        }
        if let Some(session) = client.session_mut(session_id) {
            session.status = format!(
                "upload resource received: {} ({})",
                entry.filename,
                human_bytes(entry.bytes.len() as u64)
            );
        }
        events.push(ChatClientEvent::UploadResourceAvailable {
            session_id,
            resource_id: resource_id.to_owned(),
            filename: entry.filename,
            content_type: entry.content_type,
            bytes: entry.bytes,
        });
    }
}

fn reject_inline_download(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    session_id: ChatSessionId,
    resource_id: &str,
    message: &str,
    remove_existing: bool,
    events: &mut Vec<ChatClientEvent>,
) {
    if remove_existing {
        state.pending_upload_downloads.remove(resource_id);
    }
    state.rejected_upload_downloads = state.rejected_upload_downloads.saturating_add(1);
    let message = format!("OMENchat {message}");
    if let Some(session) = client.session_mut(session_id) {
        session.status = message.clone();
    }
    events.push(ChatClientEvent::Error {
        session_id: Some(session_id),
        message,
    });
}

fn apply_message_ack(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let Some(pending) = state
        .pending_local_echoes
        .get(&(session_id, frame.seq))
        .cloned()
    else {
        return;
    };
    let Some(temp_event_id) = pending.temp_event_id else {
        return;
    };
    let Some(values) = body_values(&frame.body) else {
        return;
    };
    let Some(event_id) = values.first().and_then(FrameValueExt::as_u64) else {
        return;
    };
    let kind_id = values.get(1).and_then(FrameValueExt::as_u64).unwrap_or(1);
    let actor_user_id = values
        .get(2)
        .and_then(FrameValueExt::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let at_unix = values
        .get(3)
        .and_then(FrameValueExt::as_i64)
        .unwrap_or_else(current_unix_secs);
    let actor_display_name = values
        .get(4)
        .and_then(FrameValueExt::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let Some(session) = client.session_mut(pending.session_id) else {
        return;
    };
    let Some(event) = session
        .events
        .iter_mut()
        .find(|event| event.room_id == pending.room_id && event.event_id == temp_event_id)
    else {
        return;
    };
    event.event_id = event_id;
    event.actor_user_id = actor_user_id;
    event.actor_display_name = actor_display_name;
    event.at_unix = at_unix;
    if let ChatEventKind::Message { body } = &event.kind {
        event.kind = match kind_id {
            2 => ChatEventKind::Action { body: body.clone() },
            3 => ChatEventKind::Notice { body: body.clone() },
            _ => event.kind.clone(),
        };
    }
    let confirmed = event.clone();
    state.pending_local_echoes.remove(&(session_id, frame.seq));
    session.status = if matches!(&confirmed.kind, ChatEventKind::Notice { .. }) {
        "notice accepted by server".into()
    } else {
        "message accepted by server".into()
    };
    events.push(ChatClientEvent::EventAppended {
        session_id: pending.session_id,
        event: confirmed,
    });
    if let Some(mutation_id) = pending.mutation_id {
        events.push(ChatClientEvent::DurableMutationAcknowledged {
            session_id: pending.session_id,
            mutation_id,
        });
    }
}

fn apply_durable_terminal_error(
    client: &mut ChatClient,
    state: &mut LiveChatClientState,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let terminal = match frame_error_code(&frame.body) {
        Some(code) if code == ChatErrorCode::DurableMutationConflict as u16 => {
            DurableMutationTerminalState::Conflict
        }
        Some(code) if code == ChatErrorCode::DurableMutationResultExpired as u16 => {
            DurableMutationTerminalState::Expired
        }
        _ => return,
    };
    let Some(pending) = state
        .pending_local_echoes
        .get(&(session_id, frame.seq))
        .cloned()
    else {
        return;
    };
    let Some(mutation_id) = pending.mutation_id else {
        return;
    };
    state.pending_local_echoes.remove(&(session_id, frame.seq));
    if let (Some(temp_event_id), Some(session)) =
        (pending.temp_event_id, client.session_mut(session_id))
    {
        session
            .events
            .retain(|event| event.room_id != pending.room_id || event.event_id != temp_event_id);
    }
    events.push(ChatClientEvent::DurableMutationTerminal {
        session_id,
        mutation_id,
        state: terminal,
    });
}

fn durable_command_result_match(
    client: &ChatClient,
    state: &LiveChatClientState,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
) -> Option<Result<MutationId, ()>> {
    let session_id = preferred_session_id?;
    let pending = state.pending_local_echoes.get(&(session_id, frame.seq))?;
    if pending.temp_event_id.is_some() {
        return None;
    }
    let command_result = pending.command_result.as_ref()?;
    let (expected_command, expected_frame_room_id) = match command_result {
        PendingCommandResult::Part => ("part", Some(pending.room_id)),
        PendingCommandResult::Topic => ("topic", Some(pending.room_id)),
        PendingCommandResult::Create { .. } => ("create", None),
        PendingCommandResult::User { command, .. } => (
            match command {
                PendingUserCommand::Role { .. } => "role",
                PendingUserCommand::Unban => "unban",
                PendingUserCommand::Kick => "kick",
                PendingUserCommand::Ban => "ban",
                PendingUserCommand::Mute => "mute",
                PendingUserCommand::Unmute => "unmute",
            },
            Some(pending.room_id),
        ),
        PendingCommandResult::Reaction(_) => return None,
    };
    let Some(mutation_id) = pending.mutation_id else {
        return Some(Err(()));
    };
    if frame.room_id != expected_frame_room_id {
        return Some(Err(()));
    }
    let Some(values) = body_values(&frame.body) else {
        return Some(Err(()));
    };
    if values.first().and_then(FrameValueExt::as_str) != Some(expected_command) {
        return Some(Err(()));
    }
    match command_result {
        PendingCommandResult::Part
        | PendingCommandResult::Topic
        | PendingCommandResult::Create { .. } => {
            let server_id = client
                .session(session_id)
                .map(|session| session.server.server_id.clone())
                .unwrap_or_default();
            let Some(room) = values
                .get(1)
                .and_then(|value| parse_room(value, server_id, false))
            else {
                return Some(Err(()));
            };
            match command_result {
                PendingCommandResult::Part | PendingCommandResult::Topic
                    if room.room_id != pending.room_id =>
                {
                    return Some(Err(()));
                }
                PendingCommandResult::Create { room_name } if room.name != *room_name => {
                    return Some(Err(()));
                }
                _ => {}
            }
        }
        PendingCommandResult::User { command, target } => {
            let Some(user) = values
                .get(1)
                .and_then(|value| parse_user(value, String::new()))
            else {
                return Some(Err(()));
            };
            let target_matches = match target.parse::<u32>() {
                Ok(target_user_id) => user.user_id == target_user_id,
                Err(_) => user.display_name.eq_ignore_ascii_case(target),
            };
            if !target_matches {
                return Some(Err(()));
            }
            match command {
                PendingUserCommand::Role { role_bits } if user.role_bits != *role_bits => {
                    return Some(Err(()));
                }
                PendingUserCommand::Unban if user.status_bits & CHAT_STATUS_BANNED != 0 => {
                    return Some(Err(()));
                }
                PendingUserCommand::Ban if user.status_bits & CHAT_STATUS_BANNED == 0 => {
                    return Some(Err(()));
                }
                PendingUserCommand::Mute if user.status_bits & CHAT_STATUS_MUTED == 0 => {
                    return Some(Err(()));
                }
                PendingUserCommand::Unmute if user.status_bits & CHAT_STATUS_MUTED != 0 => {
                    return Some(Err(()));
                }
                _ => {}
            }
        }
        PendingCommandResult::Reaction(_) => return None,
    }
    Some(Ok(mutation_id))
}

fn apply_upload_accept(
    client: &mut ChatClient,
    state: Option<&mut LiveChatClientState>,
    transport: &mut dyn ChatLinkTransport,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let Some(values) = body_values(&frame.body) else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload accept had an invalid body".into(),
        });
        return;
    };
    let Some(resource_id) = values
        .first()
        .and_then(FrameValueExt::as_str)
        .map(str::to_owned)
    else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload accept did not include a resource id".into(),
        });
        return;
    };
    let Some(state) = state else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload accepted after pending upload state was unavailable".into(),
        });
        return;
    };
    let Some(upload) = state.pending_uploads.remove(&(session_id, frame.seq)) else {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload accepted but no matching pending file exists".into(),
        });
        return;
    };
    if upload.session_id != session_id {
        state
            .pending_uploads
            .insert((upload.session_id, frame.seq), upload);
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload accept belongs to another session".into(),
        });
        return;
    }
    let byte_len = upload.bytes.len() as u64;
    if let Err(error) = transport.send_resource(&resource_id, upload.bytes) {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: format!("OMENchat upload resource send failed: {error}"),
        });
        return;
    }
    if let Some(session) = client.session_mut(session_id) {
        let content_suffix = upload
            .content_type
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|value| format!(" as {value}"))
            .unwrap_or_default();
        session.status = format!(
            "upload accepted; sending {} ({}){}",
            upload.filename,
            human_bytes(byte_len),
            content_suffix
        );
    }
    events.push(ChatClientEvent::UploadAccepted {
        session_id,
        resource_id,
        filename: upload.filename,
        bytes: byte_len,
    });
}

fn apply_upload_reject(
    client: &mut ChatClient,
    state: Option<&mut LiveChatClientState>,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    if let Some(state) = state {
        if state
            .pending_uploads
            .get(&(session_id, frame.seq))
            .is_some_and(|upload| upload.session_id == session_id)
        {
            state.pending_uploads.remove(&(session_id, frame.seq));
        }
    }
    let reason = body_values(&frame.body)
        .and_then(|values| values.first())
        .and_then(FrameValueExt::as_str)
        .unwrap_or("upload rejected by server");
    let reason = bounded_chat_text(reason, CHAT_STATUS_MAX_BYTES);
    if let Some(session) = client.session_mut(session_id) {
        session.status = reason.clone();
    }
    events.push(ChatClientEvent::UploadRejected { session_id, reason });
}

fn apply_upload_complete(
    client: &mut ChatClient,
    preferred_session_id: Option<ChatSessionId>,
    frame: &Frame,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let Some(values) = body_values(&frame.body) else {
        return;
    };
    let resource_id = values.first().and_then(FrameValueExt::as_str).unwrap_or("");
    let filename = values
        .get(1)
        .and_then(FrameValueExt::as_str)
        .unwrap_or("upload");
    if resource_id.is_empty() || !chat_text_fits(resource_id, CHAT_RESOURCE_ID_MAX_BYTES) {
        events.push(ChatClientEvent::Error {
            session_id: Some(session_id),
            message: "OMENchat upload completion resource id exceeds client limits".into(),
        });
        return;
    }
    let resource_id = resource_id.to_owned();
    let filename = bounded_chat_text(filename, CHAT_UPLOAD_FILENAME_MAX_BYTES);
    let bytes = values.get(2).and_then(FrameValueExt::as_u64).unwrap_or(0);
    if let Some(session) = client.session_mut(session_id) {
        session.status = format!("upload complete: {filename} ({})", human_bytes(bytes));
    }
    events.push(ChatClientEvent::UploadCompleted {
        session_id,
        resource_id,
        filename,
        bytes,
    });
}

fn apply_user_delta(
    client: &mut ChatClient,
    preferred_session_id: Option<ChatSessionId>,
    body: &FrameBody,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let server_id = client
        .session(session_id)
        .map(|session| session.server.server_id.clone())
        .unwrap_or_default();
    let Some(user) = body_values(body)
        .and_then(|values| values.first())
        .and_then(|value| parse_user(value, server_id))
    else {
        if let Some(session) = client.session_mut(session_id) {
            session.status = "server returned an invalid user update".into();
        }
        return;
    };
    if let Some(session) = client.session_mut(session_id) {
        if let Some(current) = session.users.iter_mut().find(|current| {
            current.user_id == user.user_id || current.display_name == user.display_name
        }) {
            *current = user.clone();
        } else {
            session.users.push(user.clone());
            session
                .users
                .sort_by(|left, right| left.display_name.cmp(&right.display_name));
        }
        let (_, dropped_users) = session.enforce_catalog_bounds();
        session.status = if dropped_users == 0 {
            format!("user updated: {}", user.display_name)
        } else {
            format!(
                "user updated: {}; limited oversized user catalog by {dropped_users} entries",
                user.display_name
            )
        };
    }
    events.push(ChatClientEvent::UserUpdated { session_id, user });
}

fn apply_room_delta(
    client: &mut ChatClient,
    preferred_session_id: Option<ChatSessionId>,
    body: &FrameBody,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let server_id = client
        .session(session_id)
        .map(|session| session.server.server_id.clone())
        .unwrap_or_default();
    let Some(mut room) = body_values(body)
        .and_then(|values| values.first())
        .and_then(|value| parse_room(value, server_id, false))
    else {
        if let Some(session) = client.session_mut(session_id) {
            session.status = "server returned an invalid room update".into();
        }
        return;
    };
    if let Some(session) = client.session_mut(session_id) {
        if let Some(current) = session
            .rooms
            .iter()
            .find(|current| current.room_id == room.room_id)
        {
            room.joined = current.joined;
            room.unread = current.unread;
        }
        let active = session.active_room.room_id == room.room_id;
        if active {
            room.joined = session.active_room.joined;
            session.active_room = room.clone();
        }
        session.rooms = merge_rooms(session.rooms.clone(), vec![room.clone()]);
        session.status = format!("room updated: #{}", room.name);
        let (dropped, _) = session.enforce_catalog_bounds();
        if dropped > 0 {
            session.status.push_str(&format!(
                "; limited oversized room catalog by {dropped} entries"
            ));
        }
    }
    events.push(ChatClientEvent::RoomsUpdated {
        session_id,
        rooms: vec![room],
    });
}

fn apply_command_result(
    client: &mut ChatClient,
    preferred_session_id: Option<ChatSessionId>,
    body: &FrameBody,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let Some(values) = body_values(body) else {
        return;
    };
    let Some(command) = values.first().and_then(FrameValueExt::as_str) else {
        return;
    };
    let server_id = client
        .session(session_id)
        .map(|session| session.server.server_id.clone())
        .unwrap_or_default();
    match command {
        "rooms" => {
            let mut rooms = values
                .get(1)
                .and_then(FrameValueExt::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| parse_room(value, server_id.clone(), false))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let active_room_id = client
                .session(session_id)
                .map(|session| session.active_room.room_id)
                .unwrap_or(1);
            let room_catalog_dropped = enforce_room_catalog_bounds(&mut rooms, active_room_id);
            if rooms.is_empty() {
                if let Some(session) = client.session_mut(session_id) {
                    session.status = "server returned no rooms".into();
                }
                return;
            }
            if let Some(session) = client.session_mut(session_id) {
                session.rooms = merge_rooms(session.rooms.clone(), rooms.clone());
                let (merged_dropped, _) = session.enforce_catalog_bounds();
                let dropped = room_catalog_dropped.saturating_add(merged_dropped);
                session.status = if dropped == 0 {
                    format!("rooms refreshed: {}", rooms.len())
                } else {
                    format!(
                        "rooms refreshed: {}; limited oversized catalog by {dropped} entries",
                        rooms.len()
                    )
                };
            }
            events.push(ChatClientEvent::RoomsUpdated { session_id, rooms });
        }
        "topic" => {
            let Some(room) = values
                .get(1)
                .and_then(|value| parse_room(value, server_id, true))
            else {
                if let Some(session) = client.session_mut(session_id) {
                    session.status = "server returned an invalid topic update".into();
                }
                return;
            };
            if let Some(session) = client.session_mut(session_id) {
                let active = session.active_room.room_id == room.room_id;
                session.rooms = merge_rooms(session.rooms.clone(), vec![room.clone()]);
                if active {
                    session.active_room = room.clone();
                }
                session.status = if room.topic.as_deref().unwrap_or("").is_empty() {
                    "topic cleared".into()
                } else {
                    "topic updated".into()
                };
                let (dropped, _) = session.enforce_catalog_bounds();
                if dropped > 0 {
                    session.status.push_str(&format!(
                        "; limited oversized room catalog by {dropped} entries"
                    ));
                }
            }
            events.push(ChatClientEvent::RoomsUpdated {
                session_id,
                rooms: vec![room],
            });
        }
        "create" => {
            let Some(room) = values
                .get(1)
                .and_then(|value| parse_room(value, server_id, false))
            else {
                if let Some(session) = client.session_mut(session_id) {
                    session.status = "server returned an invalid room create result".into();
                }
                return;
            };
            if let Some(session) = client.session_mut(session_id) {
                session.rooms = merge_rooms(session.rooms.clone(), vec![room.clone()]);
                session.status = format!("room created: #{}", room.name);
                let (dropped, _) = session.enforce_catalog_bounds();
                if dropped > 0 {
                    session.status.push_str(&format!(
                        "; limited oversized room catalog by {dropped} entries"
                    ));
                }
            }
            events.push(ChatClientEvent::RoomsUpdated {
                session_id,
                rooms: vec![room],
            });
        }
        "part" => {
            let Some(room) = values
                .get(1)
                .and_then(|value| parse_room(value, server_id, false))
            else {
                if let Some(session) = client.session_mut(session_id) {
                    session.status = "server returned an invalid room part result".into();
                }
                return;
            };
            if let Some(session) = client.session_mut(session_id) {
                let active = session.active_room.room_id == room.room_id;
                session.rooms = merge_rooms(session.rooms.clone(), vec![room.clone()]);
                if let Some(current) = session
                    .rooms
                    .iter_mut()
                    .find(|current| current.room_id == room.room_id)
                {
                    current.joined = false;
                }
                if active {
                    session.users.clear();
                    if let Some(next_room) = session.rooms.iter().find(|room| room.joined).cloned()
                    {
                        session.active_room = next_room.clone();
                        session.status =
                            format!("left #{}; selected #{}", room.name, next_room.name);
                    } else {
                        session.active_room.joined = false;
                        session.status =
                            format!("left #{}; join another room to resume chat", room.name);
                    }
                } else {
                    session.status = format!("left #{}", room.name);
                }
                session.enforce_catalog_bounds();
            }
            events.push(ChatClientEvent::RoomsUpdated {
                session_id,
                rooms: vec![room],
            });
        }
        "kick" | "ban" | "unban" | "mute" | "unmute" | "role" => {
            let target_user = values
                .get(1)
                .and_then(|value| parse_user(value, String::new()));
            let target = target_user
                .as_ref()
                .map(|user| user.display_name.clone())
                .or_else(|| {
                    values
                        .get(1)
                        .and_then(FrameValueExt::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "user".into());
            if let Some(session) = client.session_mut(session_id) {
                if matches!(command, "kick" | "ban") {
                    if let Some(target_user) = target_user {
                        session
                            .users
                            .retain(|user| user.user_id != target_user.user_id);
                    } else {
                        session
                            .users
                            .retain(|user| !user.display_name.eq_ignore_ascii_case(&target));
                    }
                } else if matches!(command, "role" | "unban" | "mute" | "unmute") {
                    if let Some(target_user) = target_user {
                        if let Some(current) = session
                            .users
                            .iter_mut()
                            .find(|user| user.user_id == target_user.user_id)
                        {
                            *current = target_user;
                        }
                    }
                }
                session.status = format!("{command} applied to {target}");
            }
        }
        _ => {}
    }
}

fn merge_rooms(
    mut existing: Vec<ChatRoomSummary>,
    incoming: Vec<ChatRoomSummary>,
) -> Vec<ChatRoomSummary> {
    for room in incoming {
        if let Some(current) = existing
            .iter_mut()
            .find(|current| current.room_id == room.room_id)
        {
            let unread = if room.unread == 0 {
                current.unread
            } else {
                room.unread
            };
            let joined = current.joined || room.joined;
            *current = room;
            current.unread = unread;
            current.joined = joined;
        } else {
            existing.push(room);
        }
    }
    existing.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.room_id.cmp(&right.room_id))
    });
    existing
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BatchCapabilities {
    reactions: bool,
    message_revisions: bool,
}

fn apply_batch(
    client: &mut ChatClient,
    preferred_session_id: Option<ChatSessionId>,
    op: ChatOp,
    room_id: Option<u32>,
    values: Vec<FrameValue>,
    capabilities: BatchCapabilities,
    events: &mut Vec<ChatClientEvent>,
) {
    let Some(session_id) = preferred_session_id else {
        return;
    };
    let Some(server_id) = client
        .session(session_id)
        .map(|session| session.server.server_id.clone())
    else {
        return;
    };

    match op {
        ChatOp::UserListSnapshotInline | ChatOp::UserListSnapshotResource => {
            if let Some(snapshot_room_id) = room_id {
                let active_room_id = client
                    .session(session_id)
                    .map(|session| session.active_room.room_id);
                if active_room_id != Some(snapshot_room_id) {
                    return;
                }
            }
            let mut users = values
                .iter()
                .filter_map(|value| parse_user(value, server_id.clone()))
                .collect::<Vec<_>>();
            let dropped = enforce_user_catalog_bounds(&mut users);
            if let Some(session) = client.session_mut(session_id) {
                session.users = users;
                session.status = if dropped == 0 {
                    "live userlist updated".into()
                } else {
                    format!("live userlist updated; limited oversized catalog by {dropped} entries")
                };
            }
        }
        ChatOp::HistoryInline | ChatOp::HistoryResourceOffer => {
            let parsed = values
                .iter()
                .filter_map(|value| parse_event(value, server_id.clone(), room_id.unwrap_or(1)))
                .collect::<Vec<_>>();
            let existing = client
                .session(session_id)
                .map(|session| {
                    session
                        .events
                        .iter()
                        .map(|event| (event.room_id, event.event_id))
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default();
            let mut seen = BTreeSet::new();
            let added = parsed
                .into_iter()
                .filter(|event| {
                    let key = (event.room_id, event.event_id);
                    !existing.contains(&key) && seen.insert(key)
                })
                .collect::<Vec<_>>();
            if !added.is_empty() {
                client.prepend_history_events(session_id, added.clone());
                if let Some(session) = client.session_mut(session_id) {
                    session.status = format!("synced {} recent room history event(s)", added.len());
                }
                tracing::debug!(
                    session_id,
                    room_id = room_id.unwrap_or(1),
                    count = added.len(),
                    first_event_id = added.first().map(|event| event.event_id).unwrap_or(0),
                    last_event_id = added.last().map(|event| event.event_id).unwrap_or(0),
                    "OMENchat merged recent room history"
                );
                events.push(ChatClientEvent::HistoryPrepended {
                    session_id,
                    events: added,
                });
            } else if !values.is_empty() {
                if let Some(session) = client.session_mut(session_id) {
                    session.status = "room history sync current".into();
                }
                tracing::debug!(
                    session_id,
                    room_id = room_id.unwrap_or(1),
                    count = values.len(),
                    "OMENchat recent room history batch matched local cache"
                );
                events.push(ChatClientEvent::HistorySynced {
                    session_id,
                    room_id: room_id.unwrap_or(1),
                });
            }
        }
        ChatOp::ReactionSnapshotInline | ChatOp::ReactionSnapshotResource => {
            if !capabilities.reactions {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "ignored OMENchat reaction snapshot without reactions-v1 negotiation"
                        .into(),
                });
                return;
            }
            let Some(room_id) = room_id else {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat reaction snapshot did not identify a room".into(),
                });
                return;
            };
            let snapshot = match ReactionSnapshot::from_frame_body(&FrameBody::Fields(values)) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    events.push(ChatClientEvent::Error {
                        session_id: Some(session_id),
                        message: format!("invalid OMENchat reaction snapshot: {error}"),
                    });
                    return;
                }
            };
            match client.replace_reaction_snapshot(session_id, room_id, &snapshot) {
                Ok(()) => events.push(ChatClientEvent::ReactionSnapshotApplied {
                    session_id,
                    room_id,
                    snapshot,
                }),
                Err(error) => events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: error.into(),
                }),
            }
        }
        ChatOp::MessageRevisionSnapshotInline | ChatOp::MessageRevisionSnapshotResource => {
            if !capabilities.message_revisions {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message:
                        "ignored OMENchat message revision snapshot without message-revisions-v1 negotiation"
                            .into(),
                });
                return;
            }
            let Some(room_id) = room_id else {
                events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: "OMENchat message revision snapshot did not identify a room".into(),
                });
                return;
            };
            let snapshot =
                match MessageRevisionSnapshot::from_frame_body(&FrameBody::Fields(values)) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        events.push(ChatClientEvent::Error {
                            session_id: Some(session_id),
                            message: format!("invalid OMENchat message revision snapshot: {error}"),
                        });
                        return;
                    }
                };
            match client.replace_message_revision_snapshot(session_id, room_id, &snapshot) {
                Ok(()) => events.push(ChatClientEvent::MessageRevisionSnapshotApplied {
                    session_id,
                    room_id,
                    snapshot,
                }),
                Err(error) => events.push(ChatClientEvent::Error {
                    session_id: Some(session_id),
                    message: error.into(),
                }),
            }
        }
        _ => {}
    }
}

fn append_event(
    client: &mut ChatClient,
    session_id: ChatSessionId,
    event: ChatEvent,
    sort_after: bool,
) -> bool {
    let event_allows_unread = client.event_allows_unread(session_id, &event);
    {
        let Some(session) = client.session_mut(session_id) else {
            return false;
        };
        if session.events.iter().any(|existing| {
            existing.room_id == event.room_id && existing.event_id == event.event_id
        }) {
            return false;
        }
        if event.room_id == session.active_room.room_id {
            clear_room_unread(session, event.room_id);
        } else if event_allows_unread {
            increment_room_unread(session, event.room_id);
        }
    }
    let edge = if sort_after {
        super::client::HistoryWindowEdge::Oldest
    } else {
        super::client::HistoryWindowEdge::Newest
    };
    let retained = client.append_event_bounded(session_id, event, sort_after, edge);
    if retained {
        if let Some(session) = client.session_mut(session_id) {
            session.status = "live events updated".into();
        }
    }
    retained
}

fn human_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn live_event_gap_detected(
    client: &ChatClient,
    session_id: ChatSessionId,
    room_id: RoomId,
    event_id: u64,
) -> bool {
    let Some(session) = client.session(session_id) else {
        return false;
    };
    let Some(last_event_id) = session
        .events
        .iter()
        .filter(|event| event.room_id == room_id)
        .map(|event| event.event_id)
        .max()
    else {
        return false;
    };
    event_id > last_event_id.saturating_add(1)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RecentHistoryFingerprint {
    first_event_id: u64,
    last_event_id: u64,
    event_count: u64,
    checksum: u64,
}

fn recent_history_fingerprint(session: &ChatSessionView) -> RecentHistoryFingerprint {
    let mut events = session
        .events
        .iter()
        .filter(|event| event.room_id == session.active_room.room_id)
        .cloned()
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.event_id);
    let keep = usize::from(DEFAULT_JOIN_BACKLOG_EVENTS);
    if events.len() > keep {
        events = events.split_off(events.len() - keep);
    }
    chat_event_fingerprint(&events)
}

fn chat_event_fingerprint(events: &[ChatEvent]) -> RecentHistoryFingerprint {
    let mut checksum = 0xcbf29ce484222325_u64;
    for event in events {
        checksum = fnv_mix_u64(checksum, event.event_id);
        checksum = fnv_mix_u64(checksum, event.room_id as u64);
        checksum = fnv_mix_u64(checksum, event.actor_user_id.unwrap_or_default() as u64);
        checksum = fnv_mix_u64(checksum, event.at_unix as u64);
        checksum = fnv_mix_bytes(checksum, event.actor_display_name.as_deref().unwrap_or(""));
        match &event.kind {
            ChatEventKind::Message { body } => {
                checksum = fnv_mix_u64(checksum, 1);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ChatEventKind::RichMessage { body, metadata } => {
                checksum = fnv_mix_u64(checksum, 1);
                checksum = fnv_mix_bytes(checksum, body);
                checksum = fnv_mix_u64(checksum, metadata.reply_to_event_id.unwrap_or_default());
                for user_id in &metadata.mentioned_user_ids {
                    checksum = fnv_mix_u64(checksum, u64::from(*user_id));
                }
            }
            ChatEventKind::Action { body } => {
                checksum = fnv_mix_u64(checksum, 2);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ChatEventKind::Notice { body } => {
                checksum = fnv_mix_u64(checksum, 3);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ChatEventKind::System { body } => {
                checksum = fnv_mix_u64(checksum, 4);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ChatEventKind::Upload {
                resource_id,
                filename,
                bytes,
            } => {
                checksum = fnv_mix_u64(checksum, 5);
                checksum = fnv_mix_bytes(checksum, resource_id);
                checksum = fnv_mix_bytes(checksum, filename);
                checksum = fnv_mix_u64(checksum, *bytes);
            }
        }
    }
    RecentHistoryFingerprint {
        first_event_id: events.first().map(|event| event.event_id).unwrap_or(0),
        last_event_id: events.last().map(|event| event.event_id).unwrap_or(0),
        event_count: events.len() as u64,
        checksum,
    }
}

fn fnv_mix_u64(mut checksum: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        checksum ^= u64::from(byte);
        checksum = checksum.wrapping_mul(0x100000001b3);
    }
    checksum
}

fn fnv_mix_bytes(mut checksum: u64, value: &str) -> u64 {
    for byte in value.as_bytes() {
        checksum ^= u64::from(*byte);
        checksum = checksum.wrapping_mul(0x100000001b3);
    }
    checksum
}

fn clear_room_unread(session: &mut ChatSessionView, room_id: u32) {
    if session.active_room.room_id == room_id {
        session.active_room.unread = 0;
    }
    if let Some(room) = session
        .rooms
        .iter_mut()
        .find(|room| room.room_id == room_id)
    {
        room.unread = 0;
    }
}

fn increment_room_unread(session: &mut ChatSessionView, room_id: u32) {
    if session.active_room.room_id == room_id {
        return;
    }
    if let Some(room) = session
        .rooms
        .iter_mut()
        .find(|room| room.room_id == room_id)
    {
        room.unread = room.unread.saturating_add(1);
    } else {
        session.rooms.push(ChatRoomSummary {
            server_id: session.server.server_id.clone(),
            room_id,
            name: format!("room-{room_id}"),
            topic: None,
            unread: 1,
            joined: false,
        });
    }
}

fn parse_room(value: &FrameValue, server_id: String, joined: bool) -> Option<ChatRoomSummary> {
    let fields = value.as_array()?;
    let name = fields.get(1)?.as_str()?.trim();
    if name.is_empty() || !chat_text_fits(name, CHAT_ROOM_NAME_MAX_BYTES) {
        return None;
    }
    let topic = match fields.get(2) {
        Some(FrameValue::String(topic)) if !topic.trim().is_empty() => {
            let topic = topic.trim();
            if !chat_text_fits(topic, CHAT_ROOM_TOPIC_MAX_BYTES) {
                return None;
            }
            Some(topic.to_owned())
        }
        _ => None,
    };
    Some(ChatRoomSummary {
        server_id,
        room_id: fields.first()?.as_u64()? as u32,
        name: name.to_owned(),
        topic,
        unread: 0,
        joined,
    })
}

fn parse_user(value: &FrameValue, server_id: String) -> Option<ChatUserSummary> {
    let fields = value.as_array()?;
    let display_name = fields.get(1)?.as_str()?.trim();
    if display_name.is_empty() || !chat_text_fits(display_name, CHAT_USER_DISPLAY_MAX_BYTES) {
        return None;
    }
    Some(ChatUserSummary {
        server_id,
        user_id: fields.first()?.as_u64()? as u32,
        display_name: display_name.to_owned(),
        role_bits: fields.get(2).and_then(FrameValueExt::as_u64).unwrap_or(0),
        status_bits: fields.get(3).and_then(FrameValueExt::as_u64).unwrap_or(0) as u32,
        lxmf_available: fields
            .get(4)
            .and_then(FrameValueExt::as_bool)
            .unwrap_or(false),
    })
}

fn parse_event(value: &FrameValue, server_id: String, room_id: u32) -> Option<ChatEvent> {
    let fields = value.as_array()?;
    let kind_id = fields.get(1)?.as_u64()?;
    let body = fields.get(4)?.as_str()?.to_string();
    let kind = match kind_id {
        1 if fields.len() <= 6 => ChatEventKind::Message { body },
        1 => {
            let metadata = parse_rich_message_event_metadata(fields).ok()??;
            ChatEventKind::RichMessage {
                body,
                metadata: super::model::ChatMessageMetadata {
                    reply_to_event_id: metadata.reply_to_event_id,
                    mentioned_user_ids: metadata.mentioned_user_ids,
                },
            }
        }
        2 => ChatEventKind::Action { body },
        3 => ChatEventKind::Notice { body },
        4 => ChatEventKind::System { body },
        5 => {
            let resource_id = fields.get(6)?.as_str()?;
            let filename = fields.get(7)?.as_str()?;
            if resource_id.is_empty() || !chat_text_fits(resource_id, CHAT_RESOURCE_ID_MAX_BYTES) {
                return None;
            }
            ChatEventKind::Upload {
                resource_id: resource_id.to_owned(),
                filename: bounded_chat_text(filename, CHAT_UPLOAD_FILENAME_MAX_BYTES),
                bytes: fields.get(8)?.as_u64()?,
            }
        }
        _ => return None,
    };
    Some(ChatEvent {
        server_id,
        room_id,
        event_id: fields.first()?.as_u64()?,
        actor_user_id: match fields.get(2) {
            Some(FrameValue::Nil) | None => None,
            Some(value) => Some(value.as_u64()? as u32),
        },
        actor_display_name: match fields.get(5) {
            Some(FrameValue::String(name)) if !name.trim().is_empty() => {
                Some(bounded_chat_text(name.trim(), CHAT_ACTOR_DISPLAY_MAX_BYTES))
            }
            _ => None,
        },
        at_unix: fields.get(3)?.as_i64()?,
        kind,
    })
}

fn body_values(body: &FrameBody) -> Option<&[FrameValue]> {
    match body {
        FrameBody::Fields(values) => Some(values),
        _ => None,
    }
}

fn parse_error_text(body: &FrameBody) -> String {
    match body {
        FrameBody::Text(value) => bounded_chat_text(value, CHAT_STATUS_MAX_BYTES),
        FrameBody::Fields(values) => {
            let code = values.iter().find_map(FrameValueExt::as_u64);
            let message = values
                .iter()
                .find_map(FrameValueExt::as_str)
                .unwrap_or("OMENchat server returned an error");
            if let Some(label) = code.and_then(error_code_label) {
                let available = CHAT_STATUS_MAX_BYTES.saturating_sub(label.len() + 2);
                format!("{label}: {}", bounded_chat_text(message, available))
            } else {
                bounded_chat_text(message, CHAT_STATUS_MAX_BYTES)
            }
        }
        FrameBody::Empty => "OMENchat server returned an error".into(),
    }
}

fn frame_error_code(body: &FrameBody) -> Option<u16> {
    let FrameBody::Fields(values) = body else {
        return None;
    };
    values
        .first()
        .and_then(FrameValueExt::as_u64)
        .and_then(|code| u16::try_from(code).ok())
}

fn error_code_label(code: u64) -> Option<&'static str> {
    match code as u16 {
        value if value == ChatErrorCode::PermissionDenied as u16 => Some("permission denied"),
        value if value == ChatErrorCode::NotJoined as u16 => Some("not joined"),
        value if value == ChatErrorCode::RoomNotFound as u16 => Some("room not found"),
        value if value == ChatErrorCode::UserNotFound as u16 => Some("user not found"),
        value if value == ChatErrorCode::RateLimited as u16 => Some("rate limited"),
        value if value == ChatErrorCode::HistoryUnavailable as u16 => Some("history unavailable"),
        value if value == ChatErrorCode::MalformedFrame as u16 => Some("malformed frame"),
        value if value == ChatErrorCode::UnsupportedProtocolVersion as u16 => {
            Some("unsupported protocol")
        }
        value if value == ChatErrorCode::CompressionUnsupported as u16 => {
            Some("compression unsupported")
        }
        value if value == ChatErrorCode::ResourceUnavailable as u16 => Some("resource unavailable"),
        value if value == ChatErrorCode::DurableMutationNotNegotiated as u16 => {
            Some("durable mutation not negotiated")
        }
        value if value == ChatErrorCode::DurableMutationMalformed as u16 => {
            Some("malformed durable mutation")
        }
        value if value == ChatErrorCode::DurableMutationConflict as u16 => {
            Some("durable mutation conflict")
        }
        value if value == ChatErrorCode::DurableMutationResultExpired as u16 => {
            Some("durable mutation result expired")
        }
        value if value == ChatErrorCode::DurableMutationStoreBusy as u16 => {
            Some("durable mutation store busy")
        }
        _ => None,
    }
}

trait FrameValueExt {
    fn as_array(&self) -> Option<&[FrameValue]>;
    fn as_bool(&self) -> Option<bool>;
    fn as_bytes(&self) -> Option<&[u8]>;
    fn as_i64(&self) -> Option<i64>;
    fn as_str(&self) -> Option<&str>;
    fn as_u64(&self) -> Option<u64>;
}

impl FrameValueExt for FrameValue {
    fn as_array(&self) -> Option<&[FrameValue]> {
        match self {
            FrameValue::Array(values) => Some(values),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            FrameValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            FrameValue::Bytes(value) => Some(value),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            FrameValue::I64(value) => Some(*value),
            FrameValue::U64(value) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            FrameValue::String(value) => Some(value),
            _ => None,
        }
    }

    fn as_u64(&self) -> Option<u64> {
        match self {
            FrameValue::U64(value) => Some(*value),
            FrameValue::I64(value) if *value >= 0 => Some(*value as u64),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::codec::decode_frame;
    use crate::chat::protocol::batch::compressed_values_body;
    use crate::chat::rns::CapturedChatTransport;

    fn reaction_test_client() -> (ChatClient, ChatSessionId) {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let server_id = "reaction-server".to_string();
        assert!(client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: server_id.clone(),
                destination: "reaction-destination".into(),
                display_name: "Reaction Test".into(),
            },
            rooms: vec![room_summary(&server_id, 1, "lobby")],
            active_room: room_summary(&server_id, 1, "lobby"),
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id,
                room_id: 1,
                event_id: 10,
                actor_user_id: Some(1),
                actor_display_name: Some("Alice".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "target".into(),
                },
            }],
            status: "joined".into(),
        }));
        (client, session_id)
    }

    #[test]
    fn reaction_delta_and_snapshot_parsers_are_negotiated_bounded_and_authoritative() {
        let (mut client, session_id) = reaction_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let reaction = ReactionEvent {
            reaction_event_id: 1,
            target_event_id: 10,
            actor_user_id: 7,
            token: crate::chat::protocol::ReactionToken::Heart,
            action: crate::chat::protocol::ReactionAction::Add,
            at_unix: 2,
        };
        let frame = Frame::new(
            ChatOp::ReactionEvent,
            1,
            Some(1),
            reaction.into_frame_body().expect("reaction body"),
        );
        let mut events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame.clone(),
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("without reactions-v1 negotiation")
        ));
        assert!(client
            .reactions_for_targets(session_id, 1, &[10])
            .is_empty());

        state.set_reactions_negotiated_for_test(session_id, true);
        events.clear();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame.clone(),
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::ReactionDeltaApplied { event, .. }] if *event == reaction
        ));
        assert_eq!(
            client.reactions_for_targets(session_id, 1, &[10])[0].token,
            crate::chat::protocol::ReactionToken::Heart
        );
        assert!(!client.reaction_snapshot_complete(session_id, 1, 10));

        events.clear();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame,
            &mut events,
        );
        assert!(events.is_empty(), "duplicate delta must be idempotent");

        let snapshot = ReactionSnapshot {
            target_event_ids: vec![10],
            entries: vec![crate::chat::protocol::ReactionSnapshotEntry {
                target_event_id: 10,
                actor_user_id: 8,
                token: crate::chat::protocol::ReactionToken::Celebrate,
                created_at_unix: 3,
            }],
        };
        let FrameBody::Fields(values) = snapshot.clone().into_frame_body().expect("snapshot body")
        else {
            panic!("snapshot fields");
        };
        events.clear();
        apply_batch(
            &mut client,
            Some(session_id),
            ChatOp::ReactionSnapshotInline,
            Some(1),
            values,
            BatchCapabilities {
                reactions: true,
                message_revisions: false,
            },
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::ReactionSnapshotApplied {
                snapshot: applied,
                ..
            }] if applied == &snapshot
        ));
        let retained = client.reactions_for_targets(session_id, 1, &[10]);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].actor_user_id, 8);
        assert_eq!(
            retained[0].token,
            crate::chat::protocol::ReactionToken::Celebrate
        );
        assert!(client.reaction_snapshot_complete(session_id, 1, 10));
        client.mark_reactions_stale(session_id);
        assert!(!client.reaction_snapshot_complete(session_id, 1, 10));
        assert_eq!(client.reactions_for_targets(session_id, 1, &[10]), retained);
    }

    #[test]
    fn message_revision_delta_and_snapshot_reducers_remain_dormant_and_idempotent() {
        let (mut client, session_id) = reaction_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let revision = MessageRevisionEvent {
            revision_event_id: 20,
            target_event_id: 10,
            action: crate::chat::protocol::MessageRevisionAction::Correct,
            actor_user_id: 7,
            at_unix: 2,
            replacement: Some("corrected".into()),
            revision_number: 1,
            actor_display_name: Some("Alice".into()),
        };
        let frame = Frame::new(
            ChatOp::MessageRevisionEvent,
            1,
            Some(1),
            revision.clone().into_frame_body().expect("revision body"),
        );
        let mut events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame.clone(),
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("without message-revisions-v1 negotiation")
        ));
        assert!(client
            .message_revision_for_target(session_id, 1, 10)
            .is_none());

        state.set_message_revisions_negotiated_for_test(session_id, true);
        events.clear();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame.clone(),
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::MessageRevisionDeltaApplied { event, .. }]
                if event == &revision
        ));
        assert!(client.message_revision_target_authoritative(session_id, 1, 10));
        client.mark_message_revisions_stale(session_id);
        assert!(!client.message_revision_target_authoritative(session_id, 1, 10));
        events.clear();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame.clone(),
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::MessageRevisionDeltaApplied { event, .. }]
                if event == &revision
        ));
        assert!(client.message_revision_target_authoritative(session_id, 1, 10));
        events.clear();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            frame,
            &mut events,
        );
        assert!(
            events.is_empty(),
            "authoritative exact revision replay must be idempotent"
        );

        let snapshot = MessageRevisionSnapshot {
            target_event_ids: vec![10],
            entries: vec![crate::chat::protocol::MessageRevisionSnapshotEntry {
                target_event_id: 10,
                latest_revision_event_id: 21,
                action: crate::chat::protocol::MessageRevisionAction::Tombstone,
                actor_user_id: 8,
                at_unix: 3,
                replacement: None,
                revision_number: 2,
            }],
        };
        let FrameBody::Fields(values) = snapshot.clone().into_frame_body().expect("snapshot body")
        else {
            panic!("snapshot fields");
        };
        events.clear();
        apply_batch(
            &mut client,
            Some(session_id),
            ChatOp::MessageRevisionSnapshotInline,
            Some(1),
            values,
            BatchCapabilities {
                reactions: false,
                message_revisions: true,
            },
            &mut events,
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::MessageRevisionSnapshotApplied {
                snapshot: applied,
                ..
            }] if applied == &snapshot
        ));
        assert_eq!(
            client
                .message_revision_for_target(session_id, 1, 10)
                .expect("tombstone")
                .action,
            crate::chat::protocol::MessageRevisionAction::Tombstone
        );
        assert!(client.message_revision_snapshot_complete(session_id, 1, 10));

        let client_instance_id = ClientInstanceId::new([0x93; 16]);
        state.set_client_instance_id(Some(client_instance_id));
        state.set_durable_mutations_negotiated_for_test(session_id, true);
        let body = crate::chat::protocol::MessageRevisionRequest {
            target_event_id: 10,
            action: crate::chat::protocol::MessageRevisionAction::Tombstone,
            replacement: None,
        }
        .into_frame_body()
        .expect("dormant request");
        let intent = OutboundMutationIntent {
            server_destination: "reaction-destination".into(),
            authenticated_identity_hash: vec![1; 16],
            client_instance_id,
            mutation_id: MutationId::new([0x94; 16]),
            request_hash: canonical_mutation_request_hash(
                ChatOp::RoomMessageRevision,
                Some(1),
                &body,
            )
            .expect("dormant request hash"),
            op: ChatOp::RoomMessageRevision,
            room_id: Some(1),
            body,
            state: OutboundMutationState::SentUncertain,
            created_at: current_unix_secs(),
            expires_at: current_unix_secs().saturating_add(60),
            correlation_id: None,
        };
        let blocked = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(
            blocked.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("operation is not enabled")
        ));
        assert!(
            transport.sent_frames.is_empty(),
            "dormant revision intent must have no production sender"
        );
    }

    #[test]
    fn session_accept_surfaces_optional_server_motd() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: Vec::new(),
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "opening".into(),
        });
        let mut events = Vec::new();

        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::SessionAccept,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("omenchat-v0.1".into()),
                    FrameValue::Array(vec![]),
                    FrameValue::String("Welcome to the field node".into()),
                ]),
            ),
            &mut events,
        );

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::ServerMotd { session_id: 1, motd }]
            if motd == "Welcome to the field node"
        ));
    }

    #[test]
    fn session_accept_surfaces_server_policy_when_advertised() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: Vec::new(),
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "opening".into(),
        });
        let mut events = Vec::new();

        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::SessionAccept,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("omenchat-v0.1".into()),
                    FrameValue::Array(vec![]),
                    FrameValue::Nil,
                    FrameValue::U64(12_345),
                    FrameValue::U64(45),
                    FrameValue::U64(512),
                ]),
            ),
            &mut events,
        );

        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::ServerPolicy {
                session_id: 1,
                upload_quota_bytes: 12_345,
                upload_max_file_bytes: 512,
                ping_interval_seconds: 45
            }
        )));
    }

    #[test]
    fn live_open_sends_session_open_and_join_then_applies_server_frames() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::SessionAccept,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("omenchat-v0.1".into()),
                    FrameValue::Array(vec![]),
                ]),
            ))
            .expect("session accept");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::JoinAccept,
                2,
                Some(1),
                FrameBody::Fields(vec![room_value(1, "lobby")]),
            ))
            .expect("join accept");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UserListSnapshotInline,
                2,
                Some(1),
                compressed_values_body(&[FrameValue::Array(vec![
                    FrameValue::U64(7),
                    FrameValue::String("Operator".into()),
                    FrameValue::U64(1),
                    FrameValue::U64(0),
                    FrameValue::Bool(true),
                ])])
                .expect("userlist"),
            ))
            .expect("userlist");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                2,
                Some(1),
                compressed_values_body(&[event_value(10, 7, "hello")]).expect("history"),
            ))
            .expect("history");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "abcd".into(),
                display_name: Some("Test Chat".into()),
                ..OmenChatDescriptor::default()
            }),
        );

        assert_eq!(transport.sent_frames.len(), 2);
        let session_open = decode_frame(&transport.sent_frames[0]).expect("session open");
        assert_eq!(
            crate::chat::protocol::parse_session_open_negotiation(&session_open.body),
            Ok(None)
        );
        assert!(matches!(
            events.first(),
            Some(ChatClientEvent::ServerOpened { session_id: 1, .. })
        ));
        let session = client.session(1).expect("session");
        assert_eq!(session.server.display_name, "Test Chat");
        assert_eq!(session.active_room.name, "lobby");
        assert_eq!(session.rooms[0].name, "lobby");
        assert_eq!(session.users[0].display_name, "Operator");
        assert_eq!(session.events.len(), 1);
    }

    #[test]
    fn live_open_requests_supported_durable_extensions_with_persistent_client_identity() {
        let client_instance_id = ClientInstanceId::new([7; 16]);
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "abcd".into(),
                local_display_name: Some("Alice".into()),
                ..OmenChatDescriptor::default()
            }),
        );

        assert!(matches!(
            events.first(),
            Some(ChatClientEvent::ServerOpened { session_id: 1, .. })
        ));
        let session_open = decode_frame(&transport.sent_frames[0]).expect("session open");
        assert_eq!(session_open.op, ChatOp::SessionOpen);
        let negotiation = crate::chat::protocol::parse_session_open_negotiation(&session_open.body)
            .expect("valid negotiation")
            .expect("explicit negotiation");
        assert!(
            !negotiation
                .requested_capabilities
                .iter()
                .any(|capability| capability
                    == crate::chat::protocol::MESSAGE_REVISIONS_CAPABILITY),
            "dormant message revisions must not be requested"
        );
        assert_eq!(
            crate::chat::protocol::parse_session_open_negotiation(&session_open.body),
            Ok(Some(SessionOpenNegotiation {
                requested_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    DURABLE_NOTICE_ACK_CAPABILITY.into(),
                    REPLY_MENTIONS_CAPABILITY.into(),
                    REACTIONS_CAPABILITY.into(),
                ],
                client_instance_id: Some(client_instance_id),
            }))
        );
        assert!(state.durable_requests.contains(&1));
        assert!(state.reply_mentions_requests.contains(&1));
        assert!(state.reaction_requests.contains(&1));
        assert!(!state.durable_mutations_negotiated(1));
        assert!(!state.reply_mentions_negotiated(1));
        assert!(!state.reactions_negotiated(1));
        assert!(!state.message_revisions_negotiated(1));
    }

    #[test]
    fn durable_session_activation_requires_acceptance_and_is_cleared_on_downgrade() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: Vec::new(),
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "opening".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(ClientInstanceId::new([8; 16])));
        state.durable_requests.insert(session_id);
        state.reply_mentions_requests.insert(session_id);
        let mut transport = NoopChatTransport;
        let mut events = Vec::new();
        let accepted_body = crate::chat::protocol::with_session_accept_negotiation(
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::Array(Vec::new()),
            ]),
            &crate::chat::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REACTIONS_CAPABILITY.into(),
                    crate::chat::protocol::MESSAGE_REVISIONS_CAPABILITY.into(),
                ],
            },
        )
        .expect("negotiated accept");

        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(ChatOp::SessionAccept, 1, None, accepted_body.clone()),
            &mut events,
        );
        assert!(state.durable_mutations_negotiated(session_id));
        assert!(!state.durable_notice_ack_negotiated(session_id));
        assert!(!state.reply_mentions_negotiated(session_id));
        assert!(!state.reactions_negotiated(session_id));
        assert!(!state.message_revisions_negotiated(session_id));

        state.reaction_requests.insert(session_id);
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(ChatOp::SessionAccept, 2, None, accepted_body.clone()),
            &mut events,
        );
        assert!(state.reactions_negotiated(session_id));
        assert!(!state.message_revisions_negotiated(session_id));

        let notice_accepted_body = crate::chat::protocol::with_session_accept_negotiation(
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::Array(Vec::new()),
            ]),
            &crate::chat::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    DURABLE_NOTICE_ACK_CAPABILITY.into(),
                ],
            },
        )
        .expect("notice acknowledgement accept");
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(ChatOp::SessionAccept, 3, None, notice_accepted_body),
            &mut events,
        );
        assert!(state.durable_mutations_negotiated(session_id));
        assert!(state.durable_notice_ack_negotiated(session_id));
        assert!(!state.reactions_negotiated(session_id));

        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(
                ChatOp::SessionAccept,
                4,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String(PROTOCOL_NAME.into()),
                    FrameValue::Array(Vec::new()),
                ]),
            ),
            &mut events,
        );
        assert!(!state.durable_mutations_negotiated(session_id));
        assert!(!state.durable_notice_ack_negotiated(session_id));

        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(ChatOp::SessionAccept, 5, None, accepted_body),
            &mut events,
        );
        assert!(!state.durable_mutations_negotiated(session_id));
        assert!(!state.durable_notice_ack_negotiated(session_id));
    }

    fn durable_room_text_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let body = FrameBody::Text("durable hello".into());
        OutboundMutationIntent {
            server_destination: "abcd".into(),
            authenticated_identity_hash: vec![3; 16],
            client_instance_id,
            mutation_id: MutationId::new([4; 16]),
            request_hash: crate::chat::protocol::canonical_mutation_request_hash(
                ChatOp::RoomMessage,
                Some(1),
                &body,
            )
            .expect("request hash"),
            op: ChatOp::RoomMessage,
            room_id: Some(1),
            body,
            state,
            created_at: 10,
            expires_at: i64::MAX,
            correlation_id: None,
        }
    }

    fn durable_reaction_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let body = ReactionRequest {
            target_event_id: 9,
            token: super::super::protocol::ReactionToken::Heart,
            action: super::super::protocol::ReactionAction::Add,
        }
        .into_frame_body()
        .expect("reaction body");
        OutboundMutationIntent {
            server_destination: "abcd".into(),
            authenticated_identity_hash: vec![3; 16],
            client_instance_id,
            mutation_id: MutationId::new([8; 16]),
            request_hash: crate::chat::protocol::canonical_mutation_request_hash(
                ChatOp::RoomReaction,
                Some(1),
                &body,
            )
            .expect("request hash"),
            op: ChatOp::RoomReaction,
            room_id: Some(1),
            body,
            state,
            created_at: 10,
            expires_at: i64::MAX,
            correlation_id: None,
        }
    }

    #[test]
    fn durable_reaction_requires_both_capabilities_and_never_applies_optimistically() {
        let client_instance_id = ClientInstanceId::new([2; 16]);
        let (mut client, session_id) = live_test_client();
        client
            .session_mut(session_id)
            .expect("session")
            .events
            .push(parse_event(&event_value(9, 2, "target"), "abcd".into(), 1).expect("event"));
        assert!(client.bind_local_user_id(session_id, 7));
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        state.local_user_ids.insert(session_id, 7);
        let intent =
            durable_reaction_intent(client_instance_id, OutboundMutationState::SentUncertain);
        let mut transport = CapturedChatTransport::default();

        let blocked = send_uncertain_durable_reaction(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(
            blocked.as_slice(),
            [ChatClientEvent::Error { .. }]
        ));
        assert!(transport.sent_frames.is_empty());

        state.set_reactions_negotiated_for_test(session_id, true);
        let sent = send_uncertain_durable_reaction(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(sent.is_empty());
        assert_eq!(transport.sent_frames.len(), 1);
        assert!(client.reactions_for_targets(session_id, 1, &[9]).is_empty());
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));
    }

    #[test]
    fn durable_reaction_ack_must_match_exact_request_and_local_identity() {
        let client_instance_id = ClientInstanceId::new([2; 16]);
        let (mut client, session_id) = live_test_client();
        client
            .session_mut(session_id)
            .expect("session")
            .events
            .push(parse_event(&event_value(9, 2, "target"), "abcd".into(), 1).expect("event"));
        assert!(client.bind_local_user_id(session_id, 7));
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        state.set_reactions_negotiated_for_test(session_id, true);
        state.local_user_ids.insert(session_id, 7);
        let intent =
            durable_reaction_intent(client_instance_id, OutboundMutationState::SentUncertain);
        let mut transport = CapturedChatTransport::default();
        assert!(send_uncertain_durable_reaction(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        )
        .is_empty());
        let sent = decode_frame(&transport.sent_frames[0]).expect("sent frame");

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ReactionAck,
                sent.seq,
                Some(1),
                ReactionAck {
                    target_event_id: 9,
                    actor_user_id: 8,
                    token: super::super::protocol::ReactionToken::Heart,
                    action: super::super::protocol::ReactionAction::Add,
                    changed: true,
                    reaction_event_id: Some(10),
                }
                .into_frame_body()
                .expect("ack"),
            ))
            .expect("mismatched ack");
        let mismatched =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            mismatched.as_slice(),
            [ChatClientEvent::Error { message, .. }] if message.contains("mismatched")
        ));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));

        state.set_reactions_negotiated_for_test(session_id, false);
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ReactionAck,
                sent.seq,
                Some(1),
                ReactionAck {
                    target_event_id: 9,
                    actor_user_id: 7,
                    token: super::super::protocol::ReactionToken::Heart,
                    action: super::super::protocol::ReactionAction::Add,
                    changed: true,
                    reaction_event_id: Some(10),
                }
                .into_frame_body()
                .expect("ack"),
            ))
            .expect("matching ack");
        let capability_lost =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            capability_lost.as_slice(),
            [ChatClientEvent::Error { message, .. }] if message.contains("outside its negotiated room")
        ));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));

        state.set_reactions_negotiated_for_test(session_id, true);
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ReactionAck,
                sent.seq,
                Some(1),
                ReactionAck {
                    target_event_id: 9,
                    actor_user_id: 7,
                    token: super::super::protocol::ReactionToken::Heart,
                    action: super::super::protocol::ReactionAction::Add,
                    changed: true,
                    reaction_event_id: Some(10),
                }
                .into_frame_body()
                .expect("ack"),
            ))
            .expect("matching ack after capability restore");
        let acknowledged =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(acknowledged.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged { mutation_id, .. }
                if *mutation_id == intent.mutation_id
        )));
        assert!(!state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert!(client.reactions_for_targets(session_id, 1, &[9]).is_empty());
    }

    fn durable_rich_room_text_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let body = RichMessageBody {
            body: "durable reply".into(),
            reply_to: Some(super::super::protocol::ReplyReference {
                room_id: 1,
                event_id: 9,
            }),
            mentioned_user_ids: vec![2, 7],
        }
        .into_frame_body()
        .expect("rich message body");
        let mut intent = durable_room_text_intent(client_instance_id, state);
        intent.body = body;
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("rich request hash");
        intent
    }

    fn durable_room_action_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let mut intent = durable_room_text_intent(client_instance_id, state);
        intent.op = ChatOp::RoomAction;
        intent.body = FrameBody::Text("waves".into());
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("action request hash");
        intent
    }

    fn durable_room_notice_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let mut intent = durable_room_text_intent(client_instance_id, state);
        intent.op = ChatOp::RoomNotice;
        intent.body = FrameBody::Text("maintenance soon".into());
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("notice request hash");
        intent
    }

    fn durable_part_room_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let mut intent = durable_room_text_intent(client_instance_id, state);
        intent.op = ChatOp::PartRoom;
        intent.body = FrameBody::Empty;
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("part request hash");
        intent
    }

    fn durable_topic_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let mut intent = durable_room_text_intent(client_instance_id, state);
        intent.op = ChatOp::Command;
        intent.body = FrameBody::Text("topic Durable topic".into());
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("topic request hash");
        intent
    }

    fn durable_create_intent(
        client_instance_id: ClientInstanceId,
        state: OutboundMutationState,
    ) -> OutboundMutationIntent {
        let mut intent = durable_room_text_intent(client_instance_id, state);
        intent.op = ChatOp::Command;
        intent.room_id = None;
        intent.body = FrameBody::Text("create #op!s Durable operations".into());
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("create request hash");
        intent
    }

    fn durable_user_command_intent(
        client_instance_id: ClientInstanceId,
        mutation_marker: u8,
        command: &str,
    ) -> OutboundMutationIntent {
        let mut intent =
            durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);
        intent.mutation_id = MutationId::new([mutation_marker; 16]);
        intent.op = ChatOp::Command;
        intent.body = FrameBody::Text(command.into());
        intent.request_hash = crate::chat::protocol::canonical_mutation_request_hash(
            intent.op,
            intent.room_id,
            &intent.body,
        )
        .expect("user command request hash");
        intent
    }

    fn user_value(
        user_id: u64,
        display_name: &str,
        role_bits: u64,
        status_bits: u64,
    ) -> FrameValue {
        FrameValue::Array(vec![
            FrameValue::U64(user_id),
            FrameValue::String(display_name.into()),
            FrameValue::U64(role_bits),
            FrameValue::U64(status_bits),
            FrameValue::Bool(false),
        ])
    }

    #[test]
    fn durable_topic_waits_for_matching_result_before_updating_and_acknowledging() {
        let client_instance_id = ClientInstanceId::new([2; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let mut room = room_summary("abcd", 1, "lobby");
        room.topic = Some("Old topic".into());
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        let intent = durable_topic_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_topic(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        assert!(events.is_empty());
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .topic
                .as_deref(),
            Some("Old topic")
        );
        let sent = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(sent.op, ChatOp::Command);
        assert_eq!(sent.room_id, Some(1));
        let envelope = DurableMutationEnvelope::from_frame_body(&sent.body).expect("envelope");
        assert_eq!(envelope.mutation_id, intent.mutation_id);
        assert_eq!(envelope.request_hash, intent.request_hash);
        assert_eq!(envelope.body, FrameBody::Text("topic Durable topic".into()));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("part".into()),
                    room_value(1, "lobby"),
                ]),
            ))
            .expect("mismatched command result");
        let mismatched =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            mismatched.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("mismatched durable command result")
        ));
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .topic
                .as_deref(),
            Some("Old topic")
        );
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("topic".into()),
                    room_value_with_topic(1, "lobby", "Durable topic"),
                ]),
            ))
            .expect("topic result");
        let acknowledged =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));

        assert!(acknowledged.iter().any(|event| matches!(
            event,
            ChatClientEvent::RoomsUpdated { session_id: updated, .. } if *updated == session_id
        )));
        assert!(acknowledged.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged {
                session_id: acknowledged_session,
                mutation_id,
            } if *acknowledged_session == session_id && *mutation_id == intent.mutation_id
        )));
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .topic
                .as_deref(),
            Some("Durable topic")
        );
        assert!(!state.durable_mutation_is_pending(session_id, intent.mutation_id));
    }

    #[test]
    fn durable_create_waits_for_matching_normalized_room_before_acknowledging() {
        let client_instance_id = ClientInstanceId::new([5; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        let intent =
            durable_create_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_create(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        assert!(events.is_empty());
        assert_eq!(client.session(session_id).expect("session").rooms.len(), 1);
        let sent = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(sent.op, ChatOp::Command);
        assert_eq!(sent.room_id, None);
        let envelope = DurableMutationEnvelope::from_frame_body(&sent.body).expect("envelope");
        assert_eq!(envelope.mutation_id, intent.mutation_id);
        assert_eq!(envelope.request_hash, intent.request_hash);
        assert_eq!(
            envelope.body,
            FrameBody::Text("create #op!s Durable operations".into())
        );

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("create".into()),
                    room_value_with_topic(2, "wrong-room", "Durable operations"),
                ]),
            ))
            .expect("mismatched result");
        let mismatched =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            mismatched.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("mismatched durable command result")
        ));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert_eq!(client.session(session_id).expect("session").rooms.len(), 1);

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("create".into()),
                    room_value_with_topic(2, "ops", "Durable operations"),
                ]),
            ))
            .expect("matching result");
        let matched =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matched.iter().any(|event| {
            matches!(
                event,
                ChatClientEvent::RoomsUpdated { rooms, .. }
                    if rooms.first().map(|room| room.name.as_str()) == Some("ops")
            )
        }));
        assert!(matched.iter().any(|event| {
            matches!(
                event,
                ChatClientEvent::DurableMutationAcknowledged { mutation_id, .. }
                    if *mutation_id == intent.mutation_id
            )
        }));
        assert!(!state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert!(client
            .session(session_id)
            .expect("session")
            .rooms
            .iter()
            .any(|room| room.name == "ops"));
    }

    #[test]
    fn durable_role_and_unban_require_matching_user_and_result_state() {
        let client_instance_id = ClientInstanceId::new([6; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 2,
                display_name: "Bob".into(),
                role_bits: 0,
                status_bits: CHAT_STATUS_BANNED,
                lxmf_available: false,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        let role = durable_user_command_intent(client_instance_id, 6, "role Bob mod");

        assert!(send_uncertain_durable_user_command(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &role,
        )
        .is_empty());
        let role_frame =
            crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("role frame");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                role_frame.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("role".into()),
                    user_value(2, "Bob", CHAT_ROLE_TRUSTED, CHAT_STATUS_BANNED.into()),
                ]),
            ))
            .expect("wrong role result");
        let wrong_role =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            wrong_role.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("mismatched durable command result")
        ));
        assert!(state.durable_mutation_is_pending(session_id, role.mutation_id));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                role_frame.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("role".into()),
                    user_value(
                        2,
                        "Bob",
                        CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR,
                        CHAT_STATUS_BANNED.into(),
                    ),
                ]),
            ))
            .expect("matching role result");
        let role_events =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(role_events.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged { mutation_id, .. }
                if *mutation_id == role.mutation_id
        )));
        assert_eq!(
            client.session(session_id).expect("session").users[0].role_bits,
            CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR
        );

        let unban = durable_user_command_intent(client_instance_id, 7, "unban Bob");
        assert!(send_uncertain_durable_user_command(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &unban,
        )
        .is_empty());
        let unban_frame =
            crate::chat::codec::decode_frame(&transport.sent_frames[1]).expect("unban frame");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                unban_frame.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("unban".into()),
                    user_value(3, "Alice", 0, 0),
                ]),
            ))
            .expect("wrong user result");
        let wrong_user =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            wrong_user.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("mismatched durable command result")
        ));
        assert!(state.durable_mutation_is_pending(session_id, unban.mutation_id));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                unban_frame.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("unban".into()),
                    user_value(2, "Bob", CHAT_ROLE_TRUSTED | CHAT_ROLE_MODERATOR, 0),
                ]),
            ))
            .expect("matching unban result");
        let unban_events =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(unban_events.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged { mutation_id, .. }
                if *mutation_id == unban.mutation_id
        )));
        assert_eq!(
            client.session(session_id).expect("session").users[0].status_bits & CHAT_STATUS_BANNED,
            0
        );
    }

    #[test]
    fn durable_active_peer_moderation_requires_exact_user_and_status_result() {
        for (
            index,
            command,
            initial_status,
            wrong_user_id,
            wrong_display,
            wrong_status,
            exact_status,
            removes_user,
        ) in [
            (0, "kick Bob", 0, 2, "Alice", 0, 0, true),
            (1, "ban Bob", 0, 2, "Bob", 0, CHAT_STATUS_BANNED, true),
            (
                2,
                "mute 2",
                0,
                3,
                "2",
                CHAT_STATUS_MUTED,
                CHAT_STATUS_MUTED,
                false,
            ),
            (
                3,
                "unmute Bob",
                CHAT_STATUS_MUTED,
                2,
                "Bob",
                CHAT_STATUS_MUTED,
                0,
                false,
            ),
        ] {
            let client_instance_id = ClientInstanceId::new([20 + index; 16]);
            let mut client = ChatClient::new();
            let session_id = client.reserve_session_id();
            client.push_session(ChatSessionView {
                session_id,
                server: ChatServerSummary {
                    server_id: "abcd".into(),
                    destination: "abcd".into(),
                    display_name: "Test Chat".into(),
                },
                rooms: vec![room_summary("abcd", 1, "lobby")],
                active_room: room_summary("abcd", 1, "lobby"),
                users: vec![ChatUserSummary {
                    server_id: "abcd".into(),
                    user_id: 2,
                    display_name: "Bob".into(),
                    role_bits: 0,
                    status_bits: initial_status,
                    lxmf_available: false,
                }],
                events: Vec::new(),
                status: "ready".into(),
            });
            let mut state = LiveChatClientState::default();
            state.set_client_instance_id(Some(client_instance_id));
            state.durable_sessions.insert(session_id);
            let mut transport = CapturedChatTransport::default();
            let intent = durable_user_command_intent(client_instance_id, 30 + index, command);

            assert!(send_uncertain_durable_user_command(
                &mut client,
                &mut state,
                &mut transport,
                session_id,
                &intent,
            )
            .is_empty());
            let sent =
                crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("command frame");
            let command_name = command.split_whitespace().next().expect("command name");
            transport
                .push_incoming_frame(&Frame::new(
                    ChatOp::CommandResult,
                    sent.seq,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String(command_name.into()),
                        user_value(wrong_user_id, wrong_display, 0, wrong_status.into()),
                    ]),
                ))
                .expect("mismatched moderation result");
            let mismatched = drain_live_events_with_state(
                &mut client,
                &mut state,
                &mut transport,
                Some(session_id),
            );
            assert!(
                matches!(
                    mismatched.as_slice(),
                    [ChatClientEvent::Error { message, .. }]
                        if message.contains("mismatched durable command result")
                ),
                "{command}"
            );
            assert!(
                state.durable_mutation_is_pending(session_id, intent.mutation_id),
                "{command}"
            );

            transport
                .push_incoming_frame(&Frame::new(
                    ChatOp::CommandResult,
                    sent.seq,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String(command_name.into()),
                        user_value(2, "Bob", 0, exact_status.into()),
                    ]),
                ))
                .expect("exact moderation result");
            let acknowledged = drain_live_events_with_state(
                &mut client,
                &mut state,
                &mut transport,
                Some(session_id),
            );
            assert!(
                acknowledged.iter().any(|event| matches!(
                    event,
                    ChatClientEvent::DurableMutationAcknowledged { mutation_id, .. }
                        if *mutation_id == intent.mutation_id
                )),
                "{command}"
            );
            let target = client
                .session(session_id)
                .expect("session")
                .users
                .iter()
                .find(|user| user.user_id == 2);
            if removes_user {
                assert!(target.is_none(), "{command}");
            } else {
                assert_eq!(
                    target.expect("updated target").status_bits,
                    exact_status,
                    "{command}"
                );
            }
        }
    }

    #[test]
    fn durable_part_waits_for_matching_result_before_leaving_and_acknowledging() {
        let client_instance_id = ClientInstanceId::new([2; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "help"),
            ],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        let intent =
            durable_part_room_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_part_room(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        assert!(events.is_empty());
        assert!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .joined
        );
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));
        let sent = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(sent.op, ChatOp::PartRoom);
        assert_eq!(sent.room_id, Some(1));
        let envelope = DurableMutationEnvelope::from_frame_body(&sent.body).expect("envelope");
        assert_eq!(envelope.mutation_id, intent.mutation_id);
        assert_eq!(envelope.request_hash, intent.request_hash);
        assert_eq!(envelope.body, FrameBody::Empty);

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageAck,
                sent.seq,
                Some(1),
                FrameBody::Fields(vec![FrameValue::U64(99)]),
            ))
            .expect("wrong response type");
        let wrong_type =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(!wrong_type
            .iter()
            .any(|event| matches!(event, ChatClientEvent::DurableMutationAcknowledged { .. })));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("part".into()),
                    room_value(2, "help"),
                ]),
            ))
            .expect("mismatched part result");
        let mismatched =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(matches!(
            mismatched.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("mismatched durable command result")
        ));
        assert!(
            client
                .session(session_id)
                .expect("session")
                .rooms
                .iter()
                .find(|room| room.room_id == 2)
                .expect("help room")
                .joined
        );
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq.saturating_add(1),
                Some(1),
                FrameBody::Fields(vec![FrameValue::String("rooms".into())]),
            ))
            .expect("unrelated result");
        let unrelated =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert!(!unrelated
            .iter()
            .any(|event| matches!(event, ChatClientEvent::DurableMutationAcknowledged { .. })));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .joined
        );

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                sent.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("part".into()),
                    room_value(1, "lobby"),
                ]),
            ))
            .expect("part result");
        let acknowledged =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));

        assert!(acknowledged.iter().any(|event| matches!(
            event,
            ChatClientEvent::RoomsUpdated { session_id: updated, .. } if *updated == session_id
        )));
        assert!(acknowledged.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged {
                session_id: acknowledged_session,
                mutation_id,
            } if *acknowledged_session == session_id && *mutation_id == intent.mutation_id
        )));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.room_id, 2);
        assert!(session.active_room.joined);
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.room_id == 1)
                .map(|room| room.joined),
            Some(false)
        );
        assert!(!state.durable_mutation_is_pending(session_id, intent.mutation_id));
    }

    #[test]
    fn durable_room_text_requires_negotiation_and_uncertain_persistence_before_send() {
        let client_instance_id = ClientInstanceId::new([2; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        let mut transport = CapturedChatTransport::default();

        let intent =
            durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);
        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
        assert!(transport.sent_frames.is_empty());

        state.durable_sessions.insert(session_id);
        let prepared =
            durable_room_text_intent(client_instance_id, OutboundMutationState::Prepared);
        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &prepared,
        );
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
        assert!(transport.sent_frames.is_empty());

        let mut tampered =
            durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);
        tampered.request_hash = crate::chat::protocol::RequestHash::new([9; 32]);
        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &tampered,
        );
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
        assert!(transport.sent_frames.is_empty());
    }

    #[test]
    fn durable_rich_room_text_requires_capability_and_preserves_local_echo_metadata() {
        let client_instance_id = ClientInstanceId::new([0x22; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let intent =
            durable_rich_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);
        let mut transport = CapturedChatTransport::default();

        let blocked = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(
            blocked.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("reply/mention retry requires")
        ));
        assert!(transport.sent_frames.is_empty());
        assert!(client
            .session(session_id)
            .expect("session")
            .events
            .is_empty());

        state.reply_mentions_sessions.insert(session_id);
        let sent = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert_eq!(transport.sent_frames.len(), 1);
        assert!(sent.iter().any(|event| matches!(
            event,
            ChatClientEvent::EventAppended {
                event: ChatEvent {
                    kind: ChatEventKind::RichMessage { body, metadata },
                    ..
                },
                ..
            } if body == "durable reply"
                && metadata.reply_to_event_id == Some(9)
                && metadata.mentioned_user_ids == vec![2, 7]
        )));
    }

    #[test]
    fn durable_room_text_sends_canonical_envelope_and_correlates_acknowledgement() {
        let client_instance_id = ClientInstanceId::new([2; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageAck,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::U64(11),
                    FrameValue::U64(1),
                    FrameValue::U64(7),
                    FrameValue::I64(12),
                    FrameValue::String("Alice".into()),
                ]),
            ))
            .expect("message ack");
        let intent =
            durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        let sent = decode_frame(&transport.sent_frames[0]).expect("durable frame");
        let envelope = DurableMutationEnvelope::from_frame_body(&sent.body).expect("envelope");
        assert_eq!(sent.op, ChatOp::RoomMessage);
        assert_eq!(sent.room_id, Some(1));
        assert_eq!(envelope.mutation_id, intent.mutation_id);
        assert_eq!(envelope.request_hash, intent.request_hash);
        assert_eq!(envelope.body, intent.body);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged {
                session_id: acknowledged_session,
                mutation_id,
            } if *acknowledged_session == session_id && *mutation_id == intent.mutation_id
        )));
        assert!(state.pending_local_echoes.is_empty());
        assert_eq!(
            client.session(session_id).expect("session").events[0].event_id,
            11
        );
    }

    #[test]
    fn durable_room_action_sends_canonical_envelope_and_correlates_acknowledgement() {
        let client_instance_id = ClientInstanceId::new([12; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageAck,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::U64(21),
                    FrameValue::U64(2),
                    FrameValue::U64(7),
                    FrameValue::I64(22),
                    FrameValue::String("Alice".into()),
                ]),
            ))
            .expect("action acknowledgement");
        let intent =
            durable_room_action_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        let sent = decode_frame(&transport.sent_frames[0]).expect("durable action frame");
        let envelope = DurableMutationEnvelope::from_frame_body(&sent.body).expect("envelope");
        assert_eq!(sent.op, ChatOp::RoomAction);
        assert_eq!(sent.room_id, Some(1));
        assert_eq!(envelope.mutation_id, intent.mutation_id);
        assert_eq!(envelope.request_hash, intent.request_hash);
        assert_eq!(envelope.body, intent.body);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged {
                session_id: acknowledged_session,
                mutation_id,
            } if *acknowledged_session == session_id && *mutation_id == intent.mutation_id
        )));
        assert!(state.pending_local_echoes.is_empty());
        assert_eq!(
            client.session(session_id).expect("session").events[0].kind,
            ChatEventKind::Action {
                body: "waves".into()
            }
        );
    }

    #[test]
    fn durable_room_notice_sends_canonical_envelope_and_correlates_acknowledgement() {
        let client_instance_id = ClientInstanceId::new([13; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        state.durable_notice_ack_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageAck,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::U64(22),
                    FrameValue::U64(3),
                    FrameValue::U64(7),
                    FrameValue::I64(23),
                    FrameValue::String("Alice".into()),
                ]),
            ))
            .expect("notice acknowledgement");
        let intent =
            durable_room_notice_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        let sent = decode_frame(&transport.sent_frames[0]).expect("durable notice frame");
        let envelope = DurableMutationEnvelope::from_frame_body(&sent.body).expect("envelope");
        assert_eq!(sent.op, ChatOp::RoomNotice);
        assert_eq!(sent.room_id, Some(1));
        assert_eq!(envelope.mutation_id, intent.mutation_id);
        assert_eq!(envelope.request_hash, intent.request_hash);
        assert_eq!(envelope.body, intent.body);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::DurableMutationAcknowledged {
                session_id: acknowledged_session,
                mutation_id,
            } if *acknowledged_session == session_id && *mutation_id == intent.mutation_id
        )));
        assert!(state.pending_local_echoes.is_empty());
        assert_eq!(
            client.session(session_id).expect("session").events[0].kind,
            ChatEventKind::Notice {
                body: "maintenance soon".into()
            }
        );
        assert_eq!(
            client.session(session_id).expect("session").status,
            "notice accepted by server"
        );
    }

    #[test]
    fn durable_terminal_errors_release_only_correlated_pending_echoes() {
        for (code, expected) in [
            (
                ChatErrorCode::DurableMutationConflict,
                DurableMutationTerminalState::Conflict,
            ),
            (
                ChatErrorCode::DurableMutationResultExpired,
                DurableMutationTerminalState::Expired,
            ),
        ] {
            let client_instance_id = ClientInstanceId::new([42; 16]);
            let mut client = ChatClient::new();
            let session_id = client.reserve_session_id();
            client.push_session(ChatSessionView {
                session_id,
                server: ChatServerSummary {
                    server_id: "abcd".into(),
                    destination: "abcd".into(),
                    display_name: "Test Chat".into(),
                },
                rooms: vec![room_summary("abcd", 1, "lobby")],
                active_room: room_summary("abcd", 1, "lobby"),
                users: Vec::new(),
                events: Vec::new(),
                status: "ready".into(),
            });
            let mut state = LiveChatClientState::default();
            state.set_client_instance_id(Some(client_instance_id));
            state.durable_sessions.insert(session_id);
            let mut transport = CapturedChatTransport::default();
            transport
                .push_incoming_frame(&Frame::new(
                    ChatOp::Error,
                    1,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::U64(code as u16 as u64),
                        FrameValue::String("terminal durable result".into()),
                    ]),
                ))
                .expect("terminal response");
            let intent =
                durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);

            let events = send_uncertain_durable_room_text(
                &mut client,
                &mut state,
                &mut transport,
                session_id,
                &intent,
            );

            assert!(events.iter().any(|event| matches!(
                event,
                ChatClientEvent::DurableMutationTerminal {
                    session_id: terminal_session,
                    mutation_id,
                    state: terminal_state,
                } if *terminal_session == session_id
                    && *mutation_id == intent.mutation_id
                    && *terminal_state == expected
            )));
            assert!(events
                .iter()
                .any(|event| matches!(event, ChatClientEvent::Error { .. })));
            assert!(state.pending_local_echoes.is_empty());
            assert!(client
                .session(session_id)
                .expect("session")
                .events
                .is_empty());
        }
    }

    #[test]
    fn nonterminal_or_uncorrelated_durable_errors_preserve_uncertain_work() {
        let client_instance_id = ClientInstanceId::new([43; 16]);
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let mut transport = CapturedChatTransport::default();
        for (seq, code) in [
            (99, ChatErrorCode::DurableMutationConflict),
            (1, ChatErrorCode::DurableMutationStoreBusy),
        ] {
            transport
                .push_incoming_frame(&Frame::new(
                    ChatOp::Error,
                    seq,
                    Some(1),
                    FrameBody::Fields(vec![FrameValue::U64(code as u16 as u64)]),
                ))
                .expect("error response");
        }
        let intent =
            durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);

        let events = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );

        assert!(!events
            .iter()
            .any(|event| matches!(event, ChatClientEvent::DurableMutationTerminal { .. })));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert_eq!(client.session(session_id).expect("session").events.len(), 1);
    }

    #[test]
    fn live_open_refuses_session_overload_before_sending_frames() {
        use crate::chat::client::CHAT_CLIENT_MAX_SESSIONS;

        let mut client = ChatClient::new();
        for index in 0..CHAT_CLIENT_MAX_SESSIONS {
            let session_id = client.reserve_session_id();
            assert!(client.push_session(ChatSessionView {
                session_id,
                server: ChatServerSummary {
                    server_id: format!("server-{index}"),
                    destination: format!("destination-{index}"),
                    display_name: format!("Server {index}"),
                },
                active_room: room_summary(&format!("server-{index}"), 1, "lobby"),
                rooms: vec![room_summary(&format!("server-{index}"), 1, "lobby")],
                users: Vec::new(),
                events: Vec::new(),
                status: "ready".into(),
            }));
        }
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::OpenServer(OmenChatDescriptor {
                server_destination: "overload".into(),
                ..OmenChatDescriptor::default()
            }),
        );

        assert_eq!(client.sessions().len(), CHAT_CLIENT_MAX_SESSIONS);
        assert!(transport.sent_frames.is_empty());
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::Error {
                session_id: None,
                message
            }] if message.contains("session limit reached")
        ));
    }

    #[test]
    fn live_upload_offer_sends_accepted_resource_payload() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "joined".into(),
        });

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendUpload {
                session_id,
                room: "lobby".into(),
                filename: "proof.txt".into(),
                content_type: Some("text/plain".into()),
                bytes: b"proof".to_vec(),
            },
        );

        assert!(events.is_empty());
        let offer = decode_frame(&transport.sent_frames[0]).expect("upload offer");
        assert_eq!(offer.op, ChatOp::UploadOffer);
        assert_eq!(offer.room_id, Some(1));
        assert_eq!(offer.seq, 1);
        assert_eq!(
            state.pending_upload_metrics(),
            LivePendingUploadMetrics {
                items: 1,
                bytes: 5,
                rejected: 0,
            }
        );

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UploadAccept,
                offer.seq,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("upload:1:7:1".into()),
                    FrameValue::U64(50 * 1024 * 1024),
                    FrameValue::U64(5),
                    FrameValue::U64(0),
                ]),
            ))
            .expect("upload accept");
        let events =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));

        assert_eq!(
            transport.sent_resources.get("upload:1:7:1"),
            Some(&b"proof".to_vec())
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::UploadAccepted {
                filename,
                bytes: 5,
                ..
            }] if filename == "proof.txt"
        ));
        assert_eq!(
            state.pending_upload_metrics(),
            LivePendingUploadMetrics::default()
        );
    }

    #[test]
    fn live_upload_inline_chunks_emit_available_resource() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "joined".into(),
        });
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UploadInlineChunk,
                4,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("upload:1:7:4".into()),
                    FrameValue::String("image.png".into()),
                    FrameValue::U64(7),
                    FrameValue::String("image/png".into()),
                    FrameValue::U64(0),
                    FrameValue::Bytes(b"abc".to_vec()),
                    FrameValue::Bool(false),
                ]),
            ))
            .expect("chunk 1");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UploadInlineChunk,
                4,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("upload:1:7:4".into()),
                    FrameValue::String("image.png".into()),
                    FrameValue::U64(7),
                    FrameValue::String("image/png".into()),
                    FrameValue::U64(3),
                    FrameValue::Bytes(b"defg".to_vec()),
                    FrameValue::Bool(true),
                ]),
            ))
            .expect("chunk 2");

        let events =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));

        assert!(matches!(
            events.as_slice(),
            [
                ChatClientEvent::UploadResourceProgress {
                    resource_id: first_resource_id,
                    received: 3,
                    total: 7,
                    ..
                },
                ChatClientEvent::UploadResourceProgress {
                    resource_id: second_resource_id,
                    received: 7,
                    total: 7,
                    ..
                },
                ChatClientEvent::UploadResourceAvailable {
                resource_id,
                filename,
                content_type: Some(content_type),
                bytes,
                ..
            }
            ] if first_resource_id == "upload:1:7:4"
                && second_resource_id == "upload:1:7:4"
                && resource_id == "upload:1:7:4"
                && filename == "image.png"
                && content_type == "image/png"
                && bytes == b"abcdefg"
        ));
        assert_eq!(
            state.inline_download_metrics(),
            LiveInlineDownloadMetrics::default()
        );
    }

    #[test]
    fn live_upload_inline_chunks_buffer_out_of_order_offsets() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "joined".into(),
        });
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UploadInlineChunk,
                4,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("upload:1:7:4".into()),
                    FrameValue::String("image.png".into()),
                    FrameValue::U64(7),
                    FrameValue::String("image/png".into()),
                    FrameValue::U64(3),
                    FrameValue::Bytes(b"defg".to_vec()),
                    FrameValue::Bool(true),
                ]),
            ))
            .expect("chunk 2");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UploadInlineChunk,
                4,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("upload:1:7:4".into()),
                    FrameValue::String("image.png".into()),
                    FrameValue::U64(7),
                    FrameValue::String("image/png".into()),
                    FrameValue::U64(0),
                    FrameValue::Bytes(b"abc".to_vec()),
                    FrameValue::Bool(false),
                ]),
            ))
            .expect("chunk 1");

        let events =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));

        assert!(
            !events
                .iter()
                .any(|event| matches!(event, ChatClientEvent::Error { .. })),
            "out-of-order chunks should buffer without an error: {events:#?}"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::UploadResourceAvailable {
                resource_id,
                filename,
                content_type: Some(content_type),
                bytes,
                ..
            } if resource_id == "upload:1:7:4"
                && filename == "image.png"
                && content_type == "image/png"
                && bytes == b"abcdefg"
        )));
        assert_eq!(
            client
                .session(session_id)
                .map(|session| session.status.as_str()),
            Some("upload resource received: image.png (7 B)")
        );
        assert_eq!(
            state.inline_download_metrics(),
            LiveInlineDownloadMetrics::default()
        );
    }

    #[test]
    fn live_inline_downloads_enforce_item_and_reserved_byte_budgets() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let mut events = Vec::new();
        for index in 0..LIVE_INLINE_DOWNLOAD_MAX_ITEMS {
            apply_frame_with_state(
                &mut client,
                Some(&mut state),
                &mut transport,
                Some(session_id),
                inline_chunk_frame(
                    &format!("resource-{index}"),
                    1024 * 1024,
                    1,
                    vec![index as u8],
                ),
                &mut events,
            );
        }
        let metrics = state.inline_download_metrics();
        assert_eq!(metrics.items, LIVE_INLINE_DOWNLOAD_MAX_ITEMS);
        assert_eq!(metrics.reserved_bytes, LIVE_INLINE_DOWNLOAD_MAX_BYTES);
        assert_eq!(metrics.retained_bytes, LIVE_INLINE_DOWNLOAD_MAX_ITEMS);
        assert_eq!(metrics.pending_chunks, LIVE_INLINE_DOWNLOAD_MAX_ITEMS);
        assert_eq!(metrics.rejected, 0);

        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            inline_chunk_frame("resource-overload", 1, 0, vec![1]),
            &mut events,
        );
        let metrics = state.inline_download_metrics();
        assert_eq!(metrics.items, LIVE_INLINE_DOWNLOAD_MAX_ITEMS);
        assert_eq!(metrics.reserved_bytes, LIVE_INLINE_DOWNLOAD_MAX_BYTES);
        assert_eq!(metrics.rejected, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::Error { message, .. } if message.contains("queue is full")
        )));
    }

    #[test]
    fn live_inline_download_rejects_oversized_resource_before_retention() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let mut events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            inline_chunk_frame(
                "oversized",
                LIVE_INLINE_DOWNLOAD_MAX_RESOURCE_BYTES + 1,
                0,
                vec![1],
            ),
            &mut events,
        );

        assert_eq!(
            state.inline_download_metrics(),
            LiveInlineDownloadMetrics {
                rejected: 1,
                ..LiveInlineDownloadMetrics::default()
            }
        );
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
    }

    #[test]
    fn live_inline_download_rejects_overlapping_retained_bytes() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let mut events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            inline_chunk_frame("overlap", 1_024, 1, vec![1; 700]),
            &mut events,
        );
        assert_eq!(state.inline_download_metrics().retained_bytes, 700);

        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            inline_chunk_frame("overlap", 1_024, 2, vec![2; 700]),
            &mut events,
        );
        let metrics = state.inline_download_metrics();
        assert_eq!(metrics.items, 0);
        assert_eq!(metrics.retained_bytes, 0);
        assert_eq!(metrics.rejected, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::Error { message, .. }
                if message.contains("retained bytes exceed")
        )));
    }

    #[test]
    fn live_inline_download_rejects_fragment_saturation_and_releases_state() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let mut events = Vec::new();
        for offset in 1..=LIVE_INLINE_DOWNLOAD_MAX_PENDING_CHUNKS + 1 {
            apply_frame_with_state(
                &mut client,
                Some(&mut state),
                &mut transport,
                Some(session_id),
                inline_chunk_frame(
                    "fragmented",
                    LIVE_INLINE_DOWNLOAD_MAX_RESOURCE_BYTES,
                    offset,
                    vec![1],
                ),
                &mut events,
            );
        }
        let metrics = state.inline_download_metrics();
        assert_eq!(metrics.items, 0);
        assert_eq!(metrics.pending_chunks, 0);
        assert_eq!(metrics.rejected, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::Error { message, .. } if message.contains("fragment limit")
        )));
    }

    #[test]
    fn live_pending_uploads_enforce_item_and_byte_budgets_before_sending() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        for index in 0..LIVE_PENDING_UPLOAD_MAX_ITEMS {
            assert!(handle_live_request(
                &mut client,
                &mut state,
                &mut transport,
                ChatClientRequest::SendUpload {
                    session_id,
                    room: "lobby".into(),
                    filename: format!("item-{index}"),
                    content_type: None,
                    bytes: vec![index as u8],
                },
            )
            .is_empty());
        }
        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendUpload {
                session_id,
                room: "lobby".into(),
                filename: "item-overload".into(),
                content_type: None,
                bytes: vec![9],
            },
        );
        assert_eq!(transport.sent_frames.len(), LIVE_PENDING_UPLOAD_MAX_ITEMS);
        assert_eq!(
            state.pending_upload_metrics(),
            LivePendingUploadMetrics {
                items: LIVE_PENDING_UPLOAD_MAX_ITEMS,
                bytes: LIVE_PENDING_UPLOAD_MAX_ITEMS,
                rejected: 1,
            }
        );
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));

        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        for index in 0..2 {
            assert!(handle_live_request(
                &mut client,
                &mut state,
                &mut transport,
                ChatClientRequest::SendUpload {
                    session_id,
                    room: "lobby".into(),
                    filename: format!("large-{index}"),
                    content_type: None,
                    bytes: vec![0; LIVE_PENDING_UPLOAD_MAX_RESOURCE_BYTES],
                },
            )
            .is_empty());
        }
        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendUpload {
                session_id,
                room: "lobby".into(),
                filename: "byte-overload".into(),
                content_type: None,
                bytes: vec![1],
            },
        );
        assert_eq!(
            state.pending_upload_metrics().bytes,
            LIVE_PENDING_UPLOAD_MAX_BYTES
        );
        assert_eq!(state.pending_upload_metrics().rejected, 1);
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
    }

    #[test]
    fn live_session_cancellation_releases_pending_transfer_state() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        assert!(handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendUpload {
                session_id,
                room: "lobby".into(),
                filename: "pending.bin".into(),
                content_type: None,
                bytes: vec![1; 32],
            },
        )
        .is_empty());
        let mut events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            inline_chunk_frame("download", 64, 1, vec![2; 16]),
            &mut events,
        );
        assert_eq!(state.pending_upload_metrics().items, 1);
        assert_eq!(state.inline_download_metrics().items, 1);

        state.cancel_session_transfers(session_id);

        assert_eq!(state.pending_upload_metrics().items, 0);
        assert_eq!(state.pending_upload_metrics().bytes, 0);
        assert_eq!(state.inline_download_metrics().items, 0);
        assert_eq!(state.inline_download_metrics().retained_bytes, 0);
    }

    #[test]
    fn live_reconnect_releases_prior_link_transfer_state() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        assert!(handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendUpload {
                session_id,
                room: "lobby".into(),
                filename: "pending.bin".into(),
                content_type: None,
                bytes: vec![1; 32],
            },
        )
        .is_empty());
        let mut events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            inline_chunk_frame("download", 64, 1, vec![2; 16]),
            &mut events,
        );
        assert!(matches!(
            handle_live_request(
                &mut client,
                &mut state,
                &mut transport,
                ChatClientRequest::SendMessage {
                    session_id,
                    room: "lobby".into(),
                    body: "legacy uncertain".into(),
                },
            )
            .as_slice(),
            [ChatClientEvent::EventAppended { .. }]
        ));

        let reconnect_events = reconnect_live_server(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            OmenChatDescriptor {
                server_destination: "abcd".into(),
                ..OmenChatDescriptor::default()
            },
        );

        assert!(matches!(
            reconnect_events.first(),
            Some(ChatClientEvent::ServerOpened { .. })
        ));
        assert_eq!(state.pending_upload_metrics().items, 0);
        assert_eq!(state.inline_download_metrics().items, 0);
        assert_eq!(client.session(session_id).expect("session").events.len(), 1);
        let reconnect_sequences = transport
            .sent_frames
            .iter()
            .rev()
            .take(2)
            .map(|bytes| decode_frame(bytes).expect("reconnect frame").seq)
            .collect::<Vec<_>>();
        assert_eq!(reconnect_sequences, vec![2, 1]);
    }

    #[test]
    fn live_reconnect_removes_retired_durable_echo_and_requires_renegotiation() {
        let (mut client, session_id) = live_test_client();
        let client_instance_id = ClientInstanceId::new([55; 16]);
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        let intent =
            durable_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);
        let mut transport = CapturedChatTransport::default();

        let sent = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(
            sent.as_slice(),
            [ChatClientEvent::EventAppended { .. }]
        ));
        assert!(state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert_eq!(client.session(session_id).expect("session").events.len(), 1);

        let reconnect_events = reconnect_live_server(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            OmenChatDescriptor {
                server_destination: "abcd".into(),
                ..OmenChatDescriptor::default()
            },
        );

        assert!(matches!(
            reconnect_events.first(),
            Some(ChatClientEvent::ServerOpened { .. })
        ));
        assert!(client
            .session(session_id)
            .expect("session")
            .events
            .is_empty());
        assert!(!state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert!(!state.durable_mutations_negotiated(session_id));
        assert!(state.durable_requests.contains(&session_id));
        let frames_before_retry = transport.sent_frames.len();
        assert!(matches!(
            send_uncertain_durable_room_text(
                &mut client,
                &mut state,
                &mut transport,
                session_id,
                &intent,
            )
            .as_slice(),
            [ChatClientEvent::Error { .. }]
        ));
        assert_eq!(transport.sent_frames.len(), frames_before_retry);
    }

    #[test]
    fn live_reconnect_does_not_resend_rich_intent_when_capability_is_lost() {
        let (mut client, session_id) = live_test_client();
        let client_instance_id = ClientInstanceId::new([0x56; 16]);
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(client_instance_id));
        state.durable_sessions.insert(session_id);
        state.reply_mentions_sessions.insert(session_id);
        let intent =
            durable_rich_room_text_intent(client_instance_id, OutboundMutationState::SentUncertain);
        let mut transport = CapturedChatTransport::default();

        let sent = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(
            sent.as_slice(),
            [ChatClientEvent::EventAppended { .. }]
        ));
        assert_eq!(transport.sent_frames.len(), 1);

        let reconnect = reconnect_live_server(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            OmenChatDescriptor {
                server_destination: "abcd".into(),
                ..OmenChatDescriptor::default()
            },
        );
        assert!(matches!(
            reconnect.first(),
            Some(ChatClientEvent::ServerOpened { .. })
        ));
        assert!(!state.reply_mentions_negotiated(session_id));
        assert!(!state.durable_mutation_is_pending(session_id, intent.mutation_id));
        assert!(state.reply_mentions_requests.contains(&session_id));

        // An older replacement peer may reaccept durable mutations without
        // accepting the richer extension. Applying that response consumes the
        // pending request while leaving rich messages disabled.
        let legacy_server_accept = crate::chat::protocol::with_session_accept_negotiation(
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::Array(Vec::new()),
            ]),
            &crate::chat::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
            },
        )
        .expect("legacy server durable accept");
        let mut accept_events = Vec::new();
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(ChatOp::SessionAccept, 3, None, legacy_server_accept),
            &mut accept_events,
        );
        assert!(state.durable_mutations_negotiated(session_id));
        assert!(!state.reply_mentions_negotiated(session_id));
        assert!(state.reply_mentions_requests.is_empty());

        // The uncertain rich intent remains blocked and is not converted to a
        // legacy message.
        let reconnect_frame_count = transport.sent_frames.len();
        let blocked = send_uncertain_durable_room_text(
            &mut client,
            &mut state,
            &mut transport,
            session_id,
            &intent,
        );
        assert!(matches!(
            blocked.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("reply/mention retry requires")
        ));
        assert_eq!(transport.sent_frames.len(), reconnect_frame_count);
    }

    #[test]
    fn live_join_room_switches_active_room_and_retains_other_room_history() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::JoinAccept,
                1,
                Some(2),
                FrameBody::Fields(vec![room_value(2, "support")]),
            ))
            .expect("join accept");
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                1,
                Some(2),
                compressed_values_body(&[event_value(20, 7, "support hello")]).expect("history"),
            ))
            .expect("history");
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "support"),
            ],
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "abcd".into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::System {
                    body: "old lobby event".into(),
                },
            }],
            status: "ready".into(),
        });

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::JoinRoom {
                session_id,
                room: "support".into(),
            },
        );

        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.name, "support");
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.name == "lobby")
                .map(|room| room.joined),
            Some(true)
        );
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.name == "support")
                .map(|room| room.joined),
            Some(true)
        );
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| (event.room_id, event.event_id))
                .collect::<Vec<_>>(),
            vec![(1, 1), (2, 20)]
        );
        assert!(matches!(
            events.first(),
            Some(ChatClientEvent::RoomJoined {
                room,
                latest_events,
                ..
            }) if room.name == "support" && latest_events.iter().all(|event| event.room_id == 2)
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::HistoryPrepended { events, .. }
                if events.iter().map(|event| event.room_id).collect::<Vec<_>>() == vec![2]
        )));
    }

    #[test]
    fn live_rejoin_same_room_preserves_restored_cached_history() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::JoinAccept,
                1,
                Some(1),
                FrameBody::Fields(vec![room_value(1, "lobby")]),
            ))
            .expect("join accept");
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "abcd".into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "restored cached message".into(),
                },
            }],
            status: "restored from local cache".into(),
        });

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::JoinRoom {
                session_id,
                room: "lobby".into(),
            },
        );

        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.name, "lobby");
        assert_eq!(session.events.len(), 1);
        assert_eq!(
            session.events[0].kind,
            ChatEventKind::Message {
                body: "restored cached message".into()
            }
        );
        assert!(matches!(
            events.first(),
            Some(ChatClientEvent::RoomJoined { room, .. }) if room.name == "lobby"
        ));
    }

    #[test]
    fn live_load_older_history_uses_active_room_event_floor() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 2,
                name: "help".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "help"),
            ],
            users: Vec::new(),
            events: vec![
                ChatEvent {
                    server_id: "abcd".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "older lobby row".into(),
                    },
                },
                ChatEvent {
                    server_id: "abcd".into(),
                    room_id: 2,
                    event_id: 10,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 10,
                    kind: ChatEventKind::Message {
                        body: "active help row".into(),
                    },
                },
            ],
            status: "ready".into(),
        });

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::LoadOlder { session_id },
        );

        assert!(events.is_empty());
        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("frame");
        assert_eq!(frame.op, ChatOp::HistoryBefore);
        assert_eq!(frame.room_id, Some(2));
        assert_eq!(
            match &frame.body {
                FrameBody::Fields(values) => values.iter().find_map(FrameValueExt::as_u64),
                _ => None,
            },
            Some(10)
        );
        assert_eq!(
            client.session(session_id).expect("session").status,
            "requested older room history"
        );
    }

    #[test]
    fn live_sync_recent_history_requests_latest_active_room_batch() {
        let mut client = ChatClient::new();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                7,
                Some(2),
                compressed_values_body(&[event_value(11, 7, "missed while offline")])
                    .expect("history"),
            ))
            .expect("history frame");
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 2,
                name: "help".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "help"),
            ],
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "abcd".into(),
                room_id: 2,
                event_id: 10,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 10,
                kind: ChatEventKind::Message {
                    body: "cached active help row".into(),
                },
            }],
            status: "ready".into(),
        });

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SyncRecent { session_id },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("frame");
        assert_eq!(frame.op, ChatOp::HistoryRecent);
        assert_eq!(frame.room_id, Some(2));
        let FrameBody::Fields(values) = &frame.body else {
            panic!("recent sync fingerprint fields");
        };
        assert_eq!(values.first().and_then(FrameValueExt::as_u64), Some(10));
        assert_eq!(values.get(1).and_then(FrameValueExt::as_u64), Some(10));
        assert_eq!(values.get(2).and_then(FrameValueExt::as_u64), Some(1));
        assert!(values.get(3).and_then(FrameValueExt::as_u64).is_some());
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistoryPrepended { events, .. }]
                if events.iter().map(|event| event.event_id).collect::<Vec<_>>() == vec![11]
        ));
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .events
                .iter()
                .filter(|event| event.room_id == 2)
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![10, 11]
        );
    }

    #[test]
    fn live_history_batch_reports_room_history_status() {
        let mut client = ChatClient::new();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                7,
                Some(1),
                compressed_values_body(&[event_value(1, 7, "one"), event_value(2, 7, "two")])
                    .expect("history"),
            ))
            .expect("history frame");

        let events = drain_live_events(&mut client, &mut transport, Some(session_id));

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistoryPrepended { events, .. }] if events.len() == 2
        ));
        assert_eq!(
            client.session(session_id).expect("session").status,
            "synced 2 recent room history event(s)"
        );
    }

    #[test]
    fn live_history_recovery_preserves_rich_reply_and_mentions() {
        let mut client = ChatClient::new();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let rich_event = FrameValue::Array(vec![
            FrameValue::U64(12),
            FrameValue::U64(1),
            FrameValue::U64(7),
            FrameValue::I64(99),
            FrameValue::String("recovered reply".into()),
            FrameValue::String("Alice".into()),
            FrameValue::U64(11),
            FrameValue::Array(vec![FrameValue::U64(2), FrameValue::U64(9)]),
        ]);
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                7,
                Some(1),
                compressed_values_body(&[rich_event]).expect("rich history"),
            ))
            .expect("history frame");

        let events = drain_live_events(&mut client, &mut transport, Some(session_id));

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistoryPrepended { events, .. }]
                if matches!(
                    events.as_slice(),
                    [ChatEvent {
                        kind: ChatEventKind::RichMessage { body, metadata },
                        ..
                    }] if body == "recovered reply"
                        && metadata.reply_to_event_id == Some(11)
                        && metadata.mentioned_user_ids == vec![2, 9]
                )
        ));
    }

    #[test]
    fn live_duplicate_recent_history_reports_room_current() {
        let mut client = ChatClient::new();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "abcd".into(),
                room_id: 1,
                event_id: 2,
                actor_user_id: Some(7),
                actor_display_name: None,
                at_unix: 2,
                kind: ChatEventKind::Message {
                    body: "already cached".into(),
                },
            }],
            status: "requested recent room history".into(),
        });
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                7,
                Some(1),
                compressed_values_body(&[event_value(2, 7, "already cached")]).expect("history"),
            ))
            .expect("history frame");

        let events = drain_live_events(&mut client, &mut transport, Some(session_id));

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::HistorySynced {
                session_id: 1,
                room_id: 1
            }]
        ));
        assert_eq!(
            client.session(session_id).expect("session").status,
            "room history sync current"
        );
    }

    #[test]
    fn live_userlist_snapshot_for_inactive_room_does_not_replace_active_users() {
        let mut client = ChatClient::new();
        let mut transport = CapturedChatTransport::default();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 2, "help"),
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "help"),
            ],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 9,
                display_name: "ActiveUser".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::UserListSnapshotInline,
                7,
                Some(1),
                compressed_values_body(&[FrameValue::Array(vec![
                    FrameValue::U64(7),
                    FrameValue::String("StaleLobbyUser".into()),
                    FrameValue::U64(0),
                    FrameValue::U64(0),
                    FrameValue::Bool(true),
                ])])
                .expect("userlist"),
            ))
            .expect("userlist frame");

        let events = drain_live_events(&mut client, &mut transport, Some(session_id));

        assert!(events.is_empty());
        let session = client.session(session_id).expect("session");
        assert_eq!(session.users.len(), 1);
        assert_eq!(session.users[0].display_name, "ActiveUser");
        assert_eq!(session.status, "ready");
    }

    #[test]
    fn live_room_and_user_catalog_snapshots_are_bounded_and_visible() {
        use crate::chat::client::{CHAT_SESSION_MAX_ROOMS, CHAT_SESSION_MAX_USERS};

        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let room_values = (1..=CHAT_SESSION_MAX_ROOMS as u64 + 1)
            .map(|room_id| room_value(room_id, &format!("room-{room_id:04}")))
            .collect();
        let mut events = Vec::new();
        apply_command_result(
            &mut client,
            Some(session_id),
            &FrameBody::Fields(vec![
                FrameValue::String("rooms".into()),
                FrameValue::Array(room_values),
            ]),
            &mut events,
        );

        let room_update_len = events.iter().find_map(|event| match event {
            ChatClientEvent::RoomsUpdated { rooms, .. } => Some(rooms.len()),
            _ => None,
        });
        assert_eq!(room_update_len, Some(CHAT_SESSION_MAX_ROOMS));
        let session = client.session(session_id).expect("bounded room catalog");
        assert_eq!(session.rooms.len(), CHAT_SESSION_MAX_ROOMS);
        assert!(session.status.contains("limited oversized catalog"));

        let user_values = (1..=CHAT_SESSION_MAX_USERS as u64 + 1)
            .map(|user_id| {
                FrameValue::Array(vec![
                    FrameValue::U64(user_id),
                    FrameValue::String(format!("user-{user_id:04}")),
                    FrameValue::U64(0),
                    FrameValue::U64(0),
                    FrameValue::Bool(false),
                ])
            })
            .collect();
        apply_batch(
            &mut client,
            Some(session_id),
            ChatOp::UserListSnapshotInline,
            Some(1),
            user_values,
            BatchCapabilities::default(),
            &mut events,
        );

        let session = client.session(session_id).expect("bounded user catalog");
        assert_eq!(session.users.len(), CHAT_SESSION_MAX_USERS);
        assert!(session.status.contains("limited oversized catalog"));
    }

    #[test]
    fn live_send_message_local_echo_is_confirmed_by_message_ack() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageAck,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::U64(11),
                    FrameValue::U64(1),
                    FrameValue::U64(7),
                    FrameValue::I64(12),
                    FrameValue::String("Alice".into()),
                ]),
            ))
            .expect("message ack");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: "lobby".into(),
                body: "sent".into(),
            },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        assert_eq!(events.len(), 2);
        assert!(events.iter().any(|event| {
            matches!(event, ChatClientEvent::EventAppended { event, .. }
                if is_local_echo_event_id(event.event_id))
        }));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].event_id, 11);
        assert_eq!(session.events[0].actor_user_id, Some(7));
        assert_eq!(
            session.events[0].actor_display_name.as_deref(),
            Some("Alice")
        );
        assert_eq!(session.status, "message accepted by server");
    }

    #[test]
    fn live_send_message_without_ack_keeps_pending_local_echo() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: "lobby".into(),
                body: "unsent until ack".into(),
            },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        assert_eq!(state.pending_local_echoes.len(), 1);
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::EventAppended { session_id: 1, event }]
                if is_local_echo_event_id(event.event_id)
                    && event.actor_display_name.as_deref() == Some("You")
        ));
        assert_eq!(client.session(session_id).expect("session").events.len(), 1);
    }

    #[test]
    fn live_sequences_are_session_scoped_atomic_and_reset_only_on_link_retirement() {
        let mut state = LiveChatClientState::default();

        assert_eq!(state.reserve_seq(11), Ok(1));
        assert_eq!(state.reserve_seq(22), Ok(1));
        assert_eq!(state.reserve_seq(11), Ok(2));

        state
            .next_seq_by_session
            .insert(11, u64::from(u32::MAX) - 1);
        assert_eq!(state.reserve_seq_pair(11), Ok([u32::MAX - 1, u32::MAX]));
        assert_eq!(state.reserve_seq(11), Err(SequenceSpaceExhausted));
        assert_eq!(state.reserve_seq(22), Ok(2));

        state.next_seq_by_session.insert(33, u64::from(u32::MAX));
        assert!(state.reserve_seq_pair(33).is_err());
        assert_eq!(state.reserve_seq(33), Ok(u32::MAX));
        assert!(state.reserve_seq(33).is_err());

        let _ = state.retire_session_link_state(11);
        assert_eq!(state.reserve_seq_pair(11), Ok([1, 2]));
    }

    #[test]
    fn live_pending_correlations_allow_equal_sequences_on_independent_links() {
        let mut state = LiveChatClientState::default();
        for session_id in [11, 22] {
            state.pending_local_echoes.insert(
                (session_id, 1),
                PendingLocalEcho {
                    session_id,
                    room_id: 1,
                    temp_event_id: Some(local_echo_event_id(1)),
                    mutation_id: None,
                    command_result: None,
                },
            );
            state.pending_uploads.insert(
                (session_id, 2),
                PendingLiveUpload {
                    session_id,
                    filename: format!("session-{session_id}.bin"),
                    content_type: None,
                    bytes: vec![session_id as u8],
                },
            );
        }

        assert_eq!(state.pending_local_echo_metrics().items, 2);
        assert_eq!(state.pending_upload_metrics().items, 2);
        assert_eq!(state.reserve_seq(11), Ok(1));
        state.cancel_session_transfers(11);
        assert!(state.pending_local_echoes.contains_key(&(22, 1)));
        assert!(state.pending_uploads.contains_key(&(22, 2)));
        assert_eq!(state.pending_local_echo_metrics().items, 1);
        assert_eq!(state.pending_upload_metrics().items, 1);
        assert_eq!(state.reserve_seq(11), Ok(2));
    }

    #[test]
    fn live_sequence_exhaustion_rejects_before_frame_or_local_echo() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        state
            .next_seq_by_session
            .insert(session_id, u64::from(u32::MAX) + 1);
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: "lobby".into(),
                body: "must not be sent with a reused sequence".into(),
            },
        );

        assert!(transport.sent_frames.is_empty());
        assert_eq!(state.pending_local_echo_metrics().items, 0);
        assert!(client
            .session(session_id)
            .expect("session")
            .events
            .is_empty());
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("sequence space exhausted")
        ));
    }

    #[test]
    fn live_pending_local_echoes_enforce_per_session_budget_and_release_capacity() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        for index in 0..LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION {
            let events = handle_live_request(
                &mut client,
                &mut state,
                &mut transport,
                ChatClientRequest::SendMessage {
                    session_id,
                    room: "lobby".into(),
                    body: format!("pending-{index}"),
                },
            );
            assert!(matches!(
                events.as_slice(),
                [ChatClientEvent::EventAppended { .. }]
            ));
        }

        let rejected = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: "lobby".into(),
                body: "must remain in the composer".into(),
            },
        );
        assert_eq!(
            transport.sent_frames.len(),
            LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
        );
        assert_eq!(
            state.pending_local_echo_metrics(),
            LivePendingLocalEchoMetrics {
                items: LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION,
                rejected: 1,
            }
        );
        assert!(matches!(
            rejected.as_slice(),
            [ChatClientEvent::Error { message, .. }]
                if message.contains("pending message queue is full")
        ));

        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageAck,
                1,
                Some(1),
                FrameBody::Fields(vec![FrameValue::U64(101)]),
            ))
            .expect("message ack");
        let _ =
            drain_live_events_with_state(&mut client, &mut state, &mut transport, Some(session_id));
        assert_eq!(
            state.pending_local_echo_metrics().items,
            LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION - 1
        );

        let admitted = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendMessage {
                session_id,
                room: "lobby".into(),
                body: "capacity restored".into(),
            },
        );
        assert!(matches!(
            admitted.as_slice(),
            [ChatClientEvent::EventAppended { .. }]
        ));
        assert_eq!(
            state.pending_local_echo_metrics().items,
            LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION
        );

        state.cancel_session_transfers(session_id);
        assert_eq!(state.pending_local_echo_metrics().items, 0);
        assert_eq!(state.pending_local_echo_metrics().rejected, 1);
    }

    #[test]
    fn live_pending_local_echoes_enforce_global_budget_before_sending() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        for index in 0..LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS {
            let owner_session_id =
                100 + (index / LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS_PER_SESSION) as u64;
            let seq = index as u32 + 1;
            state.pending_local_echoes.insert(
                (owner_session_id, seq),
                PendingLocalEcho {
                    session_id: owner_session_id,
                    room_id: 1,
                    temp_event_id: Some(local_echo_event_id(seq)),
                    mutation_id: None,
                    command_result: None,
                },
            );
        }
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendAction {
                session_id,
                room: "lobby".into(),
                body: "not sent after global saturation".into(),
            },
        );

        assert!(transport.sent_frames.is_empty());
        assert_eq!(
            state.pending_local_echo_metrics(),
            LivePendingLocalEchoMetrics {
                items: LIVE_PENDING_LOCAL_ECHO_MAX_ITEMS,
                rejected: 1,
            }
        );
        assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
    }

    #[test]
    fn live_room_event_gap_requests_recent_history_sync() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "abcd".into(),
                room_id: 1,
                event_id: 11,
                actor_user_id: Some(7),
                actor_display_name: None,
                at_unix: 11,
                kind: ChatEventKind::Message {
                    body: "cached".into(),
                },
            }],
            status: "ready".into(),
        });
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::RoomEvent,
                1,
                Some(1),
                FrameBody::Fields(vec![event_value(13, 7, "new")]),
            ))
            .expect("room event");

        let events = drain_live_events(&mut client, &mut transport, Some(session_id));

        assert!(matches!(
            events.as_slice(),
            [
                ChatClientEvent::EventAppended { session_id: 1, .. },
                ChatClientEvent::HistorySyncNeeded {
                    session_id: 1,
                    room_id: 1
                }
            ]
        ));
    }

    #[test]
    fn live_room_event_for_inactive_room_increments_unread() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "support"),
            ],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut events = Vec::new();
        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::RoomEvent,
                1,
                Some(2),
                FrameBody::Fields(vec![event_value(11, 7, "support ping")]),
            ),
            &mut events,
        );

        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.unread, 0);
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.room_id == 2)
                .map(|room| room.unread),
            Some(1)
        );
        assert_eq!(session.events.len(), 1);
    }

    #[test]
    fn live_join_room_clears_room_unread_count() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let mut support = room_summary("abcd", 2, "support");
        support.unread = 4;
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby"), support],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut events = Vec::new();
        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::JoinAccept,
                1,
                Some(2),
                FrameBody::Fields(vec![room_value(2, "support")]),
            ),
            &mut events,
        );

        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.name, "support");
        assert_eq!(session.active_room.unread, 0);
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.room_id == 2)
                .map(|room| room.unread),
            Some(0)
        );
    }

    #[test]
    fn live_refresh_rooms_uses_command_result_room_catalog() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("rooms".into()),
                    FrameValue::Array(vec![room_value(1, "lobby"), room_value(2, "ops")]),
                ]),
            ))
            .expect("rooms command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::RefreshRooms { session_id },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::Command);
        assert_eq!(frame.body, FrameBody::Text("rooms".into()));
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { session_id: 1, rooms }] if rooms.len() == 2
        ));
        assert!(client
            .session(session_id)
            .expect("session")
            .rooms
            .iter()
            .any(|room| room.name == "ops"));
    }

    #[test]
    fn live_set_topic_updates_active_room_from_command_result() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("topic".into()),
                    room_value_with_topic(1, "lobby", "Operational updates"),
                ]),
            ))
            .expect("topic command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SetTopic {
                session_id,
                topic: "Operational updates".into(),
            },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::Command);
        assert_eq!(frame.room_id, Some(1));
        assert_eq!(
            frame.body,
            FrameBody::Text("topic Operational updates".into())
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { session_id: 1, rooms }]
                if rooms.first().and_then(|room| room.topic.as_deref())
                    == Some("Operational updates")
        ));
        assert_eq!(
            client
                .session(session_id)
                .expect("session")
                .active_room
                .topic
                .as_deref(),
            Some("Operational updates")
        );
    }

    #[test]
    fn live_outbound_operational_metadata_is_rejected_before_send() {
        let (mut client, session_id) = live_test_client();
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        let requests = [
            ChatClientRequest::JoinRoom {
                session_id,
                room: "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1),
            },
            ChatClientRequest::SetTopic {
                session_id,
                topic: "t".repeat(CHAT_ROOM_TOPIC_MAX_BYTES + 1),
            },
            ChatClientRequest::CreateRoom {
                session_id,
                room: "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1),
                topic: None,
            },
            ChatClientRequest::ModerateUser {
                session_id,
                action: "ban".into(),
                target: "u".repeat(CHAT_USER_DISPLAY_MAX_BYTES + 33),
            },
        ];

        for request in requests {
            let events = handle_live_request(&mut client, &mut state, &mut transport, request);
            assert!(matches!(events.as_slice(), [ChatClientEvent::Error { .. }]));
        }
        assert!(transport.sent_frames.is_empty());
    }

    #[test]
    fn live_room_delta_updates_active_room_and_preserves_joined_state() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut events = Vec::new();

        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::RoomDelta,
                1,
                Some(1),
                FrameBody::Fields(vec![room_value_with_topic(1, "lobby", "New live topic")]),
            ),
            &mut events,
        );

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { session_id: 1, rooms }]
                if rooms.first().and_then(|room| room.topic.as_deref()) == Some("New live topic")
                    && rooms.first().map(|room| room.joined) == Some(true)
        ));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.topic.as_deref(), Some("New live topic"));
        assert!(session.active_room.joined);
        assert!(session.rooms.iter().any(|room| {
            room.room_id == 1 && room.topic.as_deref() == Some("New live topic") && room.joined
        }));
    }

    #[test]
    fn live_create_room_sends_command_and_merges_room_result() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("create".into()),
                    room_value_with_topic(2, "ops", "Operations desk"),
                ]),
            ))
            .expect("create command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::CreateRoom {
                session_id,
                room: "#ops".into(),
                topic: Some("Operations desk".into()),
            },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::Command);
        assert_eq!(frame.room_id, None);
        assert_eq!(
            frame.body,
            FrameBody::Text("create ops Operations desk".into())
        );
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { session_id: 1, rooms }]
                if rooms.first().map(|room| room.name.as_str()) == Some("ops")
        ));
        assert!(client
            .session(session_id)
            .expect("session")
            .rooms
            .iter()
            .any(|room| room.name == "ops" && room.topic.as_deref() == Some("Operations desk")));
    }

    #[test]
    fn live_moderation_command_targets_active_room() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 7,
                name: "ops".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 7, "ops")],
            users: vec![
                ChatUserSummary {
                    server_id: "abcd".into(),
                    user_id: 1,
                    display_name: "Alice".into(),
                    role_bits: 0,
                    status_bits: 0,
                    lxmf_available: false,
                },
                ChatUserSummary {
                    server_id: "abcd".into(),
                    user_id: 2,
                    display_name: "Bob".into(),
                    role_bits: 0,
                    status_bits: 0,
                    lxmf_available: false,
                },
            ],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(7),
                FrameBody::Fields(vec![
                    FrameValue::String("kick".into()),
                    FrameValue::Array(vec![
                        FrameValue::U64(2),
                        FrameValue::String("Bob".into()),
                        FrameValue::U64(0),
                        FrameValue::U64(0),
                        FrameValue::Bool(false),
                    ]),
                ]),
            ))
            .expect("kick command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::ModerateUser {
                session_id,
                action: "kick".into(),
                target: "Bob".into(),
            },
        );

        assert!(events.is_empty());
        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::Command);
        assert_eq!(frame.room_id, Some(7));
        assert_eq!(frame.body, FrameBody::Text("kick Bob".into()));
        assert_eq!(
            client.session(session_id).expect("session").status,
            "kick applied to Bob"
        );
        assert!(!client
            .session(session_id)
            .expect("session")
            .users
            .iter()
            .any(|user| user.display_name == "Bob"));
    }

    #[test]
    fn live_unban_command_targets_active_room_without_pruning_userlist() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 7,
                name: "ops".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 7, "ops")],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 2,
                display_name: "Bob".into(),
                role_bits: 0,
                status_bits: 1,
                lxmf_available: false,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(7),
                FrameBody::Fields(vec![
                    FrameValue::String("unban".into()),
                    FrameValue::Array(vec![
                        FrameValue::U64(2),
                        FrameValue::String("Bob".into()),
                        FrameValue::U64(0),
                        FrameValue::U64(0),
                        FrameValue::Bool(false),
                    ]),
                ]),
            ))
            .expect("unban command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::ModerateUser {
                session_id,
                action: "unban".into(),
                target: "Bob".into(),
            },
        );

        assert!(events.is_empty());
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.room_id, Some(7));
        assert_eq!(frame.body, FrameBody::Text("unban Bob".into()));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.status, "unban applied to Bob");
        assert!(session.users.iter().any(|user| user.display_name == "Bob"));
    }

    #[test]
    fn live_mute_command_targets_active_room_without_pruning_userlist() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 7,
                name: "ops".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 7, "ops")],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 2,
                display_name: "Bob".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(7),
                FrameBody::Fields(vec![
                    FrameValue::String("mute".into()),
                    FrameValue::Array(vec![
                        FrameValue::U64(2),
                        FrameValue::String("Bob".into()),
                        FrameValue::U64(0),
                        FrameValue::U64(2),
                        FrameValue::Bool(false),
                    ]),
                ]),
            ))
            .expect("mute command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::ModerateUser {
                session_id,
                action: "mute".into(),
                target: "Bob".into(),
            },
        );

        assert!(events.is_empty());
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.room_id, Some(7));
        assert_eq!(frame.body, FrameBody::Text("mute Bob".into()));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.status, "mute applied to Bob");
        assert!(session.users.iter().any(|user| user.display_name == "Bob"));
    }

    #[test]
    fn live_role_command_updates_visible_user_role() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 7,
                name: "ops".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 7, "ops")],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 2,
                display_name: "Bob".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(7),
                FrameBody::Fields(vec![
                    FrameValue::String("role".into()),
                    FrameValue::Array(vec![
                        FrameValue::U64(2),
                        FrameValue::String("Bob".into()),
                        FrameValue::U64(3),
                        FrameValue::U64(0),
                        FrameValue::Bool(false),
                    ]),
                ]),
            ))
            .expect("role command result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::ModerateUser {
                session_id,
                action: "role".into(),
                target: "Bob mod".into(),
            },
        );

        assert!(events.is_empty());
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.room_id, Some(7));
        assert_eq!(frame.body, FrameBody::Text("role Bob mod".into()));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.status, "role applied to Bob");
        assert_eq!(
            session
                .users
                .iter()
                .find(|user| user.display_name == "Bob")
                .map(|user| user.role_bits),
            Some(3)
        );
    }

    #[test]
    fn live_user_delta_updates_visible_user_role_without_command_result() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 7,
                name: "ops".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 7, "ops")],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 2,
                display_name: "Bob".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut events = Vec::new();

        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::UserDelta,
                1,
                Some(7),
                FrameBody::Fields(vec![FrameValue::Array(vec![
                    FrameValue::U64(2),
                    FrameValue::String("Bob".into()),
                    FrameValue::U64(3),
                    FrameValue::U64(0),
                    FrameValue::Bool(false),
                ])]),
            ),
            &mut events,
        );

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::UserUpdated { session_id: 1, user }]
                if user.display_name == "Bob" && user.role_bits == 3
        ));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.status, "user updated: Bob");
        assert_eq!(
            session
                .users
                .iter()
                .find(|user| user.display_name == "Bob")
                .map(|user| user.role_bits),
            Some(3)
        );
    }

    #[test]
    fn live_send_action_uses_room_action_op() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendAction {
                session_id,
                room: "lobby".into(),
                body: "waves".into(),
            },
        );

        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatClientEvent::EventAppended {
                session_id: appended_session,
                event,
            } => {
                assert_eq!(*appended_session, session_id);
                assert_eq!(event.room_id, 1);
                assert_eq!(event.actor_display_name.as_deref(), Some("You"));
                assert!(is_local_echo_event_id(event.event_id));
                assert_eq!(
                    event.kind,
                    ChatEventKind::Action {
                        body: "waves".into()
                    }
                );
            }
            other => panic!("expected local action echo, got {other:?}"),
        }
        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::RoomAction);
        assert_eq!(frame.body, FrameBody::Text("waves".into()));
    }

    #[test]
    fn live_send_notice_uses_room_notice_op() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::SendNotice {
                session_id,
                room: "lobby".into(),
                body: "server restart in 5".into(),
            },
        );

        assert!(events.is_empty());
        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::RoomNotice);
        assert_eq!(frame.body, FrameBody::Text("server restart in 5".into()));
    }

    #[test]
    fn live_part_room_sends_part_and_marks_active_room_left() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("part".into()),
                    room_value(1, "lobby"),
                ]),
            ))
            .expect("part result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::PartRoom {
                session_id,
                room: None,
            },
        );

        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::PartRoom);
        assert_eq!(frame.room_id, Some(1));
        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { .. }]
        ));
        let session = client.session(session_id).expect("session");
        assert!(!session.active_room.joined);
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.room_id == 1)
                .map(|room| room.joined),
            Some(false)
        );
        assert!(session.users.is_empty());
    }

    #[test]
    fn live_part_active_room_selects_next_joined_room() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![
                room_summary("abcd", 1, "lobby"),
                room_summary("abcd", 2, "help"),
            ],
            users: vec![ChatUserSummary {
                server_id: "abcd".into(),
                user_id: 7,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: Vec::new(),
            status: "ready".into(),
        });
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::CommandResult,
                1,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("part".into()),
                    room_value(1, "lobby"),
                ]),
            ))
            .expect("part result");

        let events = handle_live_request(
            &mut client,
            &mut state,
            &mut transport,
            ChatClientRequest::PartRoom {
                session_id,
                room: None,
            },
        );

        assert!(matches!(
            events.as_slice(),
            [ChatClientEvent::RoomsUpdated { .. }]
        ));
        let session = client.session(session_id).expect("session");
        assert_eq!(session.active_room.name, "help");
        assert!(session.active_room.joined);
        assert!(session.users.is_empty());
        assert_eq!(session.status, "left #lobby; selected #help");
        assert_eq!(
            session
                .rooms
                .iter()
                .find(|room| room.name == "lobby")
                .map(|room| room.joined),
            Some(false)
        );
    }

    #[test]
    fn live_ping_sends_ping_frame_for_health_checks() {
        let mut state = LiveChatClientState::default();
        let mut transport = CapturedChatTransport::default();

        assert_eq!(ping_live_session(&mut state, &mut transport, 7), None);

        assert_eq!(transport.sent_frames.len(), 1);
        let frame = crate::chat::codec::decode_frame(&transport.sent_frames[0]).expect("decode");
        assert_eq!(frame.op, ChatOp::Ping);
        assert_eq!(frame.seq, 1);
    }

    #[test]
    fn live_history_end_marks_beginning_of_room_history() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "abcd".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        });

        let mut events = Vec::new();
        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(ChatOp::HistoryEnd, 7, Some(1), FrameBody::Empty),
            &mut events,
        );

        assert!(events.is_empty());
        assert_eq!(
            client.session(session_id).expect("session").status,
            "start of room history reached"
        );
    }

    #[test]
    fn parse_event_preserves_actor_display_name() {
        let event = parse_event(
            &FrameValue::Array(vec![
                FrameValue::U64(22),
                FrameValue::U64(1),
                FrameValue::U64(7),
                FrameValue::I64(123),
                FrameValue::String("hello".into()),
                FrameValue::String("Alice".into()),
            ]),
            "server-a".into(),
            1,
        )
        .expect("event");

        assert_eq!(event.actor_user_id, Some(7));
        assert_eq!(event.actor_display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn live_inactive_room_unread_respects_mute_except_authoritative_mentions() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server".into(),
                destination: "server".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("server", 1, "lobby"),
            rooms: vec![
                room_summary("server", 1, "lobby"),
                room_summary("server", 2, "help"),
            ],
            users: Vec::new(),
            events: Vec::new(),
            status: "ready".into(),
        }));
        assert!(client.bind_local_user_id(session_id, 7));
        assert!(client.set_room_mute_except_mentions(session_id, 2, true));

        let event = |event_id, mentioned_user_ids| ChatEvent {
            server_id: "server".into(),
            room_id: 2,
            event_id,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: event_id as i64,
            kind: ChatEventKind::RichMessage {
                body: "message".into(),
                metadata: super::super::model::ChatMessageMetadata {
                    reply_to_event_id: None,
                    mentioned_user_ids,
                },
            },
        };
        assert!(append_event(
            &mut client,
            session_id,
            event(1, Vec::new()),
            false
        ));
        assert_eq!(
            client
                .session(session_id)
                .and_then(|session| session.rooms.iter().find(|room| room.room_id == 2))
                .map(|room| room.unread),
            Some(0)
        );
        assert!(append_event(
            &mut client,
            session_id,
            event(2, vec![7]),
            false
        ));
        assert_eq!(
            client
                .session(session_id)
                .and_then(|session| session.rooms.iter().find(|room| room.room_id == 2))
                .map(|room| room.unread),
            Some(1)
        );
    }

    #[test]
    fn parse_event_accepts_exact_rich_metadata_and_rejects_partial_extensions() {
        let rich = parse_event(
            &FrameValue::Array(vec![
                FrameValue::U64(22),
                FrameValue::U64(1),
                FrameValue::U64(7),
                FrameValue::I64(99),
                FrameValue::String("reply".into()),
                FrameValue::String("Alice".into()),
                FrameValue::U64(21),
                FrameValue::Array(vec![FrameValue::U64(2), FrameValue::U64(9)]),
            ]),
            "server".into(),
            3,
        )
        .expect("rich event");
        assert_eq!(
            rich.kind,
            ChatEventKind::RichMessage {
                body: "reply".into(),
                metadata: super::super::model::ChatMessageMetadata {
                    reply_to_event_id: Some(21),
                    mentioned_user_ids: vec![2, 9],
                },
            }
        );

        assert!(parse_event(
            &FrameValue::Array(vec![
                FrameValue::U64(23),
                FrameValue::U64(1),
                FrameValue::U64(7),
                FrameValue::I64(100),
                FrameValue::String("partial".into()),
                FrameValue::String("Alice".into()),
                FrameValue::U64(22),
            ]),
            "server".into(),
            3,
        )
        .is_none());
    }

    #[test]
    fn dormant_reply_negotiation_requires_request_and_join_user_identity() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            rooms: Vec::new(),
            active_room: room_summary("abcd", 1, "lobby"),
            users: Vec::new(),
            events: Vec::new(),
            status: "opening".into(),
        });
        let mut state = LiveChatClientState::default();
        state.set_client_instance_id(Some(ClientInstanceId::new([8; 16])));
        state.durable_requests.insert(session_id);
        state.reply_mentions_requests.insert(session_id);
        let mut transport = NoopChatTransport;
        let mut events = Vec::new();
        let accepted_body = crate::chat::protocol::with_session_accept_negotiation(
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::Array(Vec::new()),
            ]),
            &crate::chat::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REPLY_MENTIONS_CAPABILITY.into(),
                ],
            },
        )
        .expect("negotiated accept");
        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(ChatOp::SessionAccept, 1, None, accepted_body),
            &mut events,
        );
        assert!(state.reply_mentions_negotiated(session_id));
        assert_eq!(state.local_user_id(session_id), None);

        apply_frame_with_state(
            &mut client,
            Some(&mut state),
            &mut transport,
            Some(session_id),
            Frame::new(
                ChatOp::JoinAccept,
                2,
                Some(1),
                FrameBody::Fields(vec![room_value(1, "lobby"), FrameValue::U64(7)]),
            ),
            &mut events,
        );
        assert_eq!(state.local_user_id(session_id), Some(7));
        assert_eq!(client.local_user_id(session_id), Some(7));
        assert!(events.iter().any(|event| matches!(
            event,
            ChatClientEvent::LocalUserBound {
                session_id: bound_session,
                user_id: 7,
            } if *bound_session == session_id
        )));

        state.retire_session_link_state(session_id);
        assert!(!state.reply_mentions_negotiated(session_id));
        assert_eq!(state.local_user_id(session_id), None);
        assert_eq!(client.local_user_id(session_id), Some(7));
    }

    #[test]
    fn live_event_actor_display_name_is_utf8_byte_bounded() {
        let event = parse_event(
            &FrameValue::Array(vec![
                FrameValue::U64(22),
                FrameValue::U64(1),
                FrameValue::U64(7),
                FrameValue::I64(123),
                FrameValue::String("hello".into()),
                FrameValue::String("☃".repeat(CHAT_ACTOR_DISPLAY_MAX_BYTES)),
            ]),
            "server-a".into(),
            1,
        )
        .expect("event");

        let actor = event.actor_display_name.expect("bounded actor");
        assert!(actor.len() <= CHAT_ACTOR_DISPLAY_MAX_BYTES);
        assert!(actor.ends_with('…'));
    }

    #[test]
    fn parse_room_preserves_topic() {
        let room = parse_room(
            &FrameValue::Array(vec![
                FrameValue::U64(3),
                FrameValue::String("ops".into()),
                FrameValue::String("Operations desk".into()),
                FrameValue::U64(1),
            ]),
            "server-a".into(),
            true,
        )
        .expect("room");

        assert_eq!(room.name, "ops");
        assert_eq!(room.topic.as_deref(), Some("Operations desk"));
    }

    #[test]
    fn live_room_and_user_parsers_reject_oversized_operational_labels() {
        assert!(parse_room(
            &FrameValue::Array(vec![
                FrameValue::U64(3),
                FrameValue::String("r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1)),
                FrameValue::Nil,
            ]),
            "server-a".into(),
            true,
        )
        .is_none());
        assert!(parse_room(
            &FrameValue::Array(vec![
                FrameValue::U64(3),
                FrameValue::String("ops".into()),
                FrameValue::String("t".repeat(CHAT_ROOM_TOPIC_MAX_BYTES + 1)),
            ]),
            "server-a".into(),
            true,
        )
        .is_none());
        assert!(parse_user(
            &FrameValue::Array(vec![
                FrameValue::U64(7),
                FrameValue::String("u".repeat(CHAT_USER_DISPLAY_MAX_BYTES + 1)),
                FrameValue::U64(0),
                FrameValue::U64(0),
                FrameValue::Bool(false),
            ]),
            "server-a".into(),
        )
        .is_none());
    }

    #[test]
    fn parse_error_text_includes_known_error_code_label() {
        let text = parse_error_text(&FrameBody::Fields(vec![
            FrameValue::U64(ChatErrorCode::PermissionDenied as u16 as u64),
            FrameValue::String("user is muted".into()),
        ]));

        assert_eq!(text, "permission denied: user is muted");

        let expired = parse_error_text(&FrameBody::Fields(vec![FrameValue::U64(
            ChatErrorCode::DurableMutationResultExpired as u16 as u64,
        )]));
        assert_eq!(
            expired,
            "durable mutation result expired: OMENchat server returned an error"
        );
    }

    #[test]
    fn live_error_and_motd_text_are_utf8_byte_bounded() {
        let error = parse_error_text(&FrameBody::Fields(vec![
            FrameValue::U64(ChatErrorCode::PermissionDenied as u16 as u64),
            FrameValue::String("☃".repeat(CHAT_STATUS_MAX_BYTES)),
        ]));
        assert!(error.len() <= CHAT_STATUS_MAX_BYTES);
        assert!(error.ends_with('…'));

        let (mut client, session_id) = live_test_client();
        let mut events = Vec::new();
        apply_frame(
            &mut client,
            Some(session_id),
            Frame::new(
                ChatOp::SessionAccept,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String(PROTOCOL_NAME.into()),
                    FrameValue::Array(Vec::new()),
                    FrameValue::String("☃".repeat(CHAT_MOTD_MAX_BYTES)),
                ]),
            ),
            &mut events,
        );
        let motd = events.iter().find_map(|event| match event {
            ChatClientEvent::ServerMotd { motd, .. } => Some(motd),
            _ => None,
        });
        assert!(motd.is_some_and(|motd| motd.len() <= CHAT_MOTD_MAX_BYTES && motd.ends_with('…')));
    }

    fn room_value(room_id: u64, name: &str) -> FrameValue {
        FrameValue::Array(vec![
            FrameValue::U64(room_id),
            FrameValue::String(name.into()),
            FrameValue::Nil,
            FrameValue::U64(1),
        ])
    }

    fn room_value_with_topic(room_id: u64, name: &str, topic: &str) -> FrameValue {
        FrameValue::Array(vec![
            FrameValue::U64(room_id),
            FrameValue::String(name.into()),
            FrameValue::String(topic.into()),
            FrameValue::U64(2),
        ])
    }

    fn room_summary(server_id: &str, room_id: u32, name: &str) -> ChatRoomSummary {
        ChatRoomSummary {
            server_id: server_id.into(),
            room_id,
            name: name.into(),
            topic: None,
            unread: 0,
            joined: true,
        }
    }

    fn live_test_client() -> (ChatClient, ChatSessionId) {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "abcd".into(),
                destination: "abcd".into(),
                display_name: "Test Chat".into(),
            },
            active_room: room_summary("abcd", 1, "lobby"),
            rooms: vec![room_summary("abcd", 1, "lobby")],
            users: Vec::new(),
            events: Vec::new(),
            status: "joined".into(),
        }));
        (client, session_id)
    }

    fn inline_chunk_frame(
        resource_id: &str,
        total_len: usize,
        offset: usize,
        chunk: Vec<u8>,
    ) -> Frame {
        Frame::new(
            ChatOp::UploadInlineChunk,
            1,
            Some(1),
            FrameBody::Fields(vec![
                FrameValue::String(resource_id.into()),
                FrameValue::String("download.bin".into()),
                FrameValue::U64(total_len as u64),
                FrameValue::String("application/octet-stream".into()),
                FrameValue::U64(offset as u64),
                FrameValue::Bytes(chunk),
                FrameValue::Bool(false),
            ]),
        )
    }

    fn event_value(event_id: u64, actor: u64, body: &str) -> FrameValue {
        FrameValue::Array(vec![
            FrameValue::U64(event_id),
            FrameValue::U64(1),
            FrameValue::U64(actor),
            FrameValue::I64(0),
            FrameValue::String(body.into()),
        ])
    }
}
