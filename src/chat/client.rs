use super::descriptor::OmenChatDescriptor;
use super::model::{
    bounded_chat_text, chat_event_supports_message_revisions, chat_event_supports_pins,
    chat_event_supports_reactions, chat_message_revisions_fit_bounds, chat_pins_fit_bounds,
    chat_reactions_fit_bounds, chat_text_fits, ChatEvent, ChatMessageRevision, ChatPin,
    ChatReaction, ChatRoomSummary, ChatServerSummary, ChatUserSummary,
    CHAT_ACTOR_DISPLAY_MAX_BYTES, CHAT_MOTD_MAX_BYTES, CHAT_PIN_MAX_ROWS, CHAT_ROOM_NAME_MAX_BYTES,
    CHAT_ROOM_TOPIC_MAX_BYTES, CHAT_SERVER_DESTINATION_MAX_BYTES, CHAT_SERVER_DISPLAY_MAX_BYTES,
    CHAT_SERVER_ID_MAX_BYTES, CHAT_STATUS_MAX_BYTES, CHAT_UPLOAD_FILENAME_MAX_BYTES,
    CHAT_USER_DISPLAY_MAX_BYTES,
};
pub use super::model::{
    CHAT_CLIENT_MAX_SESSIONS, CHAT_SESSION_MAX_ROOMS, CHAT_SESSION_MAX_ROOM_BYTES,
    CHAT_SESSION_MAX_USERS, CHAT_SESSION_MAX_USER_BYTES,
};
use super::protocol::{
    EventId, MessageRevisionAction, MessageRevisionEvent, MessageRevisionSnapshot,
    MessageRevisionSnapshotEntry, ModerationAuditPage, PinAction, PinEvent, PinSnapshot,
    PinSnapshotEntry, ReactionAction, ReactionEvent, ReactionSnapshot, ReactionSnapshotEntry,
    ReactionToken, RoomId, RoomPolicyProjection, ServerId, MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS,
    REACTION_SNAPSHOT_MAX_TARGETS, ROOM_PIN_SNAPSHOT_MAX_TARGETS,
};
use super::store::ChatStore;
use std::collections::{BTreeMap, BTreeSet};

pub const CHAT_SESSION_HISTORY_MAX_EVENTS: usize = 1_024;
pub const CHAT_SESSION_HISTORY_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const CHAT_MODERATION_AUDIT_MAX_RECORDS_PER_SESSION: usize = 256;
pub const CHAT_MODERATION_AUDIT_MAX_RECORDS: usize = 1_024;
pub const CHAT_MODERATION_AUDIT_MAX_BYTES: usize = 512 * 1024;
const CHAT_MODERATION_AUDIT_RECORD_OVERHEAD_BYTES: usize = 64;

pub type ChatSessionId = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatConnectionState {
    #[default]
    Disconnected,
    Resolving,
    Connecting,
    Authenticating,
    Joined,
    Reconnecting,
    Draining,
    Failed {
        retryable: bool,
    },
}

impl ChatConnectionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Resolving => "resolving",
            Self::Connecting => "connecting",
            Self::Authenticating => "authenticating",
            Self::Joined => "joined",
            Self::Reconnecting => "reconnecting",
            Self::Draining => "draining",
            Self::Failed { retryable: true } => "failed (retryable)",
            Self::Failed { retryable: false } => "failed (terminal)",
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::Resolving
                | Self::Connecting
                | Self::Authenticating
                | Self::Reconnecting
                | Self::Disconnected
                | Self::Failed { retryable: true }
        )
    }

    /// Whether a user-initiated reconnect is currently a valid action.
    ///
    /// Transitional states remain retryable for automatic recovery, but must
    /// not expose a second manual reconnect that would compete with work that
    /// is already in flight.
    pub fn manual_reconnect_allowed(self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed { retryable: true })
    }
}

#[cfg(test)]
mod connection_state_tests {
    use super::ChatConnectionState;

    #[test]
    fn connection_state_labels_and_retryability_are_typed() {
        let cases = [
            (
                ChatConnectionState::Disconnected,
                "disconnected",
                true,
                true,
            ),
            (ChatConnectionState::Resolving, "resolving", true, false),
            (ChatConnectionState::Connecting, "connecting", true, false),
            (
                ChatConnectionState::Authenticating,
                "authenticating",
                true,
                false,
            ),
            (ChatConnectionState::Joined, "joined", false, false),
            (
                ChatConnectionState::Reconnecting,
                "reconnecting",
                true,
                false,
            ),
            (ChatConnectionState::Draining, "draining", false, false),
            (
                ChatConnectionState::Failed { retryable: true },
                "failed (retryable)",
                true,
                true,
            ),
            (
                ChatConnectionState::Failed { retryable: false },
                "failed (terminal)",
                false,
                false,
            ),
        ];
        for (state, label, retryable, manual_reconnect_allowed) in cases {
            assert_eq!(state.label(), label);
            assert_eq!(state.retryable(), retryable);
            assert_eq!(state.manual_reconnect_allowed(), manual_reconnect_allowed);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatClientRequest {
    OpenServer(OmenChatDescriptor),
    JoinRoom {
        session_id: ChatSessionId,
        room: String,
    },
    PartRoom {
        session_id: ChatSessionId,
        room: Option<String>,
    },
    SendMessage {
        session_id: ChatSessionId,
        room: String,
        body: String,
    },
    SendAction {
        session_id: ChatSessionId,
        room: String,
        body: String,
    },
    SendNotice {
        session_id: ChatSessionId,
        room: String,
        body: String,
    },
    SendUpload {
        session_id: ChatSessionId,
        room: String,
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    },
    RequestUpload {
        session_id: ChatSessionId,
        room: String,
        resource_id: String,
    },
    RefreshRooms {
        session_id: ChatSessionId,
    },
    SetTopic {
        session_id: ChatSessionId,
        topic: String,
    },
    CreateRoom {
        session_id: ChatSessionId,
        room: String,
        topic: Option<String>,
    },
    ModerateUser {
        session_id: ChatSessionId,
        action: String,
        target: String,
    },
    SyncRecent {
        session_id: ChatSessionId,
    },
    LoadOlder {
        session_id: ChatSessionId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub enum DurableMutationTerminalState {
    Conflict,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub enum DurableMutationRejectionReason {
    SlowMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatClientEvent {
    ServerOpened {
        session_id: ChatSessionId,
        server: ChatServerSummary,
    },
    RoomJoined {
        session_id: ChatSessionId,
        room: ChatRoomSummary,
        users: Vec<ChatUserSummary>,
        latest_events: Vec<ChatEvent>,
    },
    RoomsUpdated {
        session_id: ChatSessionId,
        rooms: Vec<ChatRoomSummary>,
    },
    ServerMotd {
        session_id: ChatSessionId,
        motd: String,
    },
    ServerPolicy {
        session_id: ChatSessionId,
        upload_quota_bytes: u64,
        upload_max_file_bytes: u64,
        ping_interval_seconds: u64,
    },
    UserUpdated {
        session_id: ChatSessionId,
        user: ChatUserSummary,
    },
    LocalUserBound {
        session_id: ChatSessionId,
        user_id: super::protocol::UserId,
    },
    EventAppended {
        session_id: ChatSessionId,
        event: ChatEvent,
    },
    DurableMutationAcknowledged {
        session_id: ChatSessionId,
        mutation_id: super::protocol::MutationId,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    DurableMutationTerminal {
        session_id: ChatSessionId,
        mutation_id: super::protocol::MutationId,
        state: DurableMutationTerminalState,
    },
    #[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
    DurableMutationRejected {
        session_id: ChatSessionId,
        mutation_id: super::protocol::MutationId,
        reason: DurableMutationRejectionReason,
    },
    HistoryPrepended {
        session_id: ChatSessionId,
        events: Vec<ChatEvent>,
    },
    HistorySynced {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    HistorySyncNeeded {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    ReactionDeltaApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        event: ReactionEvent,
    },
    ReactionSnapshotApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: ReactionSnapshot,
    },
    MessageRevisionDeltaApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        event: MessageRevisionEvent,
    },
    MessageRevisionSnapshotApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: MessageRevisionSnapshot,
    },
    PinDeltaApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        event: PinEvent,
    },
    PinSnapshotApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: PinSnapshot,
    },
    ModerationAuditPageApplied {
        session_id: ChatSessionId,
        room_id: RoomId,
        page: ModerationAuditPage,
    },
    ModerationAuditEnd {
        session_id: ChatSessionId,
        room_id: RoomId,
    },
    UploadAccepted {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        bytes: u64,
    },
    UploadRejected {
        session_id: ChatSessionId,
        reason: String,
        room_policy_reason: Option<super::protocol::RoomUploadRejectReason>,
    },
    UploadCompleted {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        bytes: u64,
    },
    UploadResourceAvailable {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    },
    UploadResourceProgress {
        session_id: ChatSessionId,
        resource_id: String,
        filename: String,
        received: u64,
        total: u64,
    },
    Error {
        session_id: Option<ChatSessionId>,
        message: String,
    },
}

pub(crate) fn enforce_client_event_presentation_bounds(events: &mut [ChatClientEvent]) {
    for event in events {
        match event {
            ChatClientEvent::ServerOpened { server, .. } => {
                server.display_name =
                    bounded_chat_text(server.display_name.trim(), CHAT_SERVER_DISPLAY_MAX_BYTES);
            }
            ChatClientEvent::ServerMotd { motd, .. } => {
                *motd = bounded_chat_text(motd.trim(), CHAT_MOTD_MAX_BYTES);
            }
            ChatClientEvent::EventAppended { event, .. } => {
                if let Some(actor) = event.actor_display_name.take() {
                    event.actor_display_name = Some(bounded_chat_text(
                        actor.trim(),
                        CHAT_ACTOR_DISPLAY_MAX_BYTES,
                    ));
                }
                if let super::model::ChatEventKind::Upload { filename, .. } = &mut event.kind {
                    *filename = bounded_chat_text(filename, CHAT_UPLOAD_FILENAME_MAX_BYTES);
                }
            }
            ChatClientEvent::UploadAccepted { filename, .. }
            | ChatClientEvent::UploadCompleted { filename, .. }
            | ChatClientEvent::UploadResourceProgress { filename, .. }
            | ChatClientEvent::UploadResourceAvailable { filename, .. } => {
                *filename = bounded_chat_text(filename, CHAT_UPLOAD_FILENAME_MAX_BYTES);
            }
            ChatClientEvent::UploadRejected { reason, .. }
            | ChatClientEvent::Error {
                message: reason, ..
            } => {
                *reason = bounded_chat_text(reason, CHAT_STATUS_MAX_BYTES);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatSessionView {
    pub session_id: ChatSessionId,
    pub server: ChatServerSummary,
    pub rooms: Vec<ChatRoomSummary>,
    pub active_room: ChatRoomSummary,
    pub users: Vec<ChatUserSummary>,
    pub events: Vec<ChatEvent>,
    pub status: String,
}

impl ChatSessionView {
    pub fn set_status(&mut self, status: impl AsRef<str>) {
        self.status = bounded_chat_text(status.as_ref(), CHAT_STATUS_MAX_BYTES);
    }

    pub fn retained_history_bytes(&self) -> usize {
        retained_history_bytes(&self.events)
    }

    pub fn retained_room_catalog_bytes(&self) -> usize {
        retained_room_catalog_bytes(&self.rooms)
    }

    pub fn retained_user_catalog_bytes(&self) -> usize {
        retained_user_catalog_bytes(&self.users)
    }

    pub(crate) fn enforce_catalog_bounds(&mut self) -> (usize, usize) {
        let rooms_before = self.rooms.len();
        let users_before = self.users.len();
        enforce_room_catalog_bounds(&mut self.rooms, self.active_room.room_id);
        enforce_user_catalog_bounds(&mut self.users);
        (
            rooms_before.saturating_sub(self.rooms.len()),
            users_before.saturating_sub(self.users.len()),
        )
    }

    fn enforce_presentation_bounds(&mut self) {
        self.server.display_name = bounded_chat_text(
            self.server.display_name.trim(),
            CHAT_SERVER_DISPLAY_MAX_BYTES,
        );
        self.active_room.name = self.active_room.name.trim().to_owned();
        self.active_room.server_id = self.server.server_id.clone();
        self.active_room.topic = self
            .active_room
            .topic
            .as_deref()
            .map(str::trim)
            .filter(|topic| !topic.is_empty())
            .map(|topic| bounded_chat_text(topic, CHAT_ROOM_TOPIC_MAX_BYTES));
        self.rooms
            .retain(|room| chat_text_fits(room.name.trim(), CHAT_ROOM_NAME_MAX_BYTES));
        for room in &mut self.rooms {
            room.name = room.name.trim().to_owned();
            room.topic = room
                .topic
                .as_deref()
                .map(str::trim)
                .filter(|topic| !topic.is_empty())
                .map(|topic| bounded_chat_text(topic, CHAT_ROOM_TOPIC_MAX_BYTES));
        }
        self.users
            .retain(|user| chat_text_fits(user.display_name.trim(), CHAT_USER_DISPLAY_MAX_BYTES));
        for user in &mut self.users {
            user.display_name = user.display_name.trim().to_owned();
        }
        let status = std::mem::take(&mut self.status);
        self.set_status(status);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum HistoryWindowEdge {
    Oldest,
    Newest,
}

#[derive(Clone, Debug, Default)]
pub struct ChatClient {
    next_session_id: ChatSessionId,
    sessions: Vec<ChatSessionView>,
    local_user_ids: std::collections::BTreeMap<ServerId, super::protocol::UserId>,
    mute_except_mentions: BTreeMap<ServerId, BTreeSet<RoomId>>,
    reactions: BTreeMap<
        (
            ServerId,
            RoomId,
            EventId,
            super::protocol::UserId,
            ReactionToken,
        ),
        ChatReaction,
    >,
    authoritative_reaction_targets: BTreeMap<ServerId, BTreeSet<(RoomId, EventId)>>,
    message_revisions: BTreeMap<(ServerId, RoomId, EventId), ChatMessageRevision>,
    authoritative_message_revision_targets: BTreeMap<ServerId, BTreeSet<(RoomId, EventId)>>,
    pins: BTreeMap<(ServerId, RoomId, EventId), ChatPin>,
    pin_event_cursors: BTreeMap<(ServerId, RoomId, EventId), EventId>,
    authoritative_pin_targets: BTreeMap<ServerId, BTreeSet<(RoomId, EventId)>>,
    moderation_audit_pages: BTreeMap<(ServerId, RoomId), ModerationAuditPage>,
    room_policies: BTreeMap<(ChatSessionId, RoomId), RoomPolicyProjection>,
}

impl ChatClient {
    pub fn new() -> Self {
        Self {
            next_session_id: 1,
            sessions: Vec::new(),
            local_user_ids: std::collections::BTreeMap::new(),
            mute_except_mentions: BTreeMap::new(),
            reactions: BTreeMap::new(),
            authoritative_reaction_targets: BTreeMap::new(),
            message_revisions: BTreeMap::new(),
            authoritative_message_revision_targets: BTreeMap::new(),
            pins: BTreeMap::new(),
            pin_event_cursors: BTreeMap::new(),
            authoritative_pin_targets: BTreeMap::new(),
            moderation_audit_pages: BTreeMap::new(),
            room_policies: BTreeMap::new(),
        }
    }

    pub fn reserve_session_id(&mut self) -> ChatSessionId {
        let id = self.next_session_id;
        self.next_session_id = self.next_session_id.saturating_add(1).max(1);
        id
    }

    pub fn sessions(&self) -> &[ChatSessionView] {
        &self.sessions
    }

    pub fn session(&self, session_id: ChatSessionId) -> Option<&ChatSessionView> {
        self.sessions
            .iter()
            .find(|session| session.session_id == session_id)
    }

    pub fn session_mut(&mut self, session_id: ChatSessionId) -> Option<&mut ChatSessionView> {
        self.sessions
            .iter_mut()
            .find(|session| session.session_id == session_id)
    }

    pub fn bind_local_user_id(
        &mut self,
        session_id: ChatSessionId,
        user_id: super::protocol::UserId,
    ) -> bool {
        if user_id == 0 {
            return false;
        }
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return false;
        };
        self.local_user_ids.insert(server_id, user_id);
        true
    }

    pub fn local_user_id(&self, session_id: ChatSessionId) -> Option<super::protocol::UserId> {
        self.session(session_id)
            .and_then(|session| self.local_user_ids.get(&session.server.server_id))
            .copied()
    }

    pub fn room_policy(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> Option<RoomPolicyProjection> {
        self.session(session_id)?;
        self.room_policies.get(&(session_id, room_id)).copied()
    }

    pub fn room_policy_bits(&self, session_id: ChatSessionId, room_id: RoomId) -> Option<u64> {
        self.room_policy(session_id, room_id)
            .map(RoomPolicyProjection::policy_bits)
    }

    pub fn room_slow_mode_seconds(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> Option<u32> {
        self.room_policy(session_id, room_id)
            .map(RoomPolicyProjection::slow_mode_seconds)
    }

    pub fn room_upload_policy(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> Option<super::protocol::RoomUploadPolicyProjection> {
        self.room_policy(session_id, room_id)
            .and_then(RoomPolicyProjection::upload_policy)
    }

    pub fn room_is_announcement_only(&self, session_id: ChatSessionId, room_id: RoomId) -> bool {
        self.room_policy(session_id, room_id)
            .is_some_and(RoomPolicyProjection::announcement_only)
    }

    pub fn local_user_can_publish_to_room(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> bool {
        if !self.room_is_announcement_only(session_id, room_id) {
            return true;
        }
        let Some(local_user_id) = self.local_user_id(session_id) else {
            return false;
        };
        self.session(session_id).is_some_and(|session| {
            session.users.iter().any(|user| {
                user.user_id == local_user_id
                    && user.role_bits
                        & (super::model::CHAT_ROLE_MODERATOR | super::model::CHAT_ROLE_ADMIN)
                        != 0
            })
        })
    }

    pub fn local_user_can_view_moderation_audit(&self, session_id: ChatSessionId) -> bool {
        let Some(local_user_id) = self.local_user_id(session_id) else {
            return false;
        };
        self.session(session_id).is_some_and(|session| {
            session.users.iter().any(|user| {
                user.user_id == local_user_id
                    && user.role_bits
                        & (super::model::CHAT_ROLE_MODERATOR | super::model::CHAT_ROLE_ADMIN)
                        != 0
            })
        })
    }

    pub(crate) fn replace_room_policies(
        &mut self,
        session_id: ChatSessionId,
        policies: &[(RoomId, RoomPolicyProjection)],
    ) -> bool {
        let Some(session) = self.session(session_id) else {
            return false;
        };
        let known_rooms = session
            .rooms
            .iter()
            .map(|room| room.room_id)
            .chain(std::iter::once(session.active_room.room_id))
            .collect::<BTreeSet<_>>();
        if policies.len() > CHAT_SESSION_MAX_ROOMS
            || policies
                .iter()
                .any(|(room_id, _)| !known_rooms.contains(room_id))
        {
            return false;
        }
        self.room_policies
            .retain(|(stored_session, _), _| *stored_session != session_id);
        self.room_policies.extend(
            policies
                .iter()
                .copied()
                .map(|(room_id, policy)| ((session_id, room_id), policy)),
        );
        true
    }

    pub(crate) fn update_room_policy(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        policy: RoomPolicyProjection,
    ) -> bool {
        let Some(session) = self.session(session_id) else {
            return false;
        };
        if session.active_room.room_id != room_id
            && !session.rooms.iter().any(|room| room.room_id == room_id)
        {
            return false;
        }
        let session_policy_count = self
            .room_policies
            .keys()
            .filter(|(stored_session, _)| *stored_session == session_id)
            .count();
        let key = (session_id, room_id);
        if !self.room_policies.contains_key(&key) && session_policy_count >= CHAT_SESSION_MAX_ROOMS
        {
            return false;
        }
        self.room_policies.insert(key, policy);
        true
    }

    pub(crate) fn clear_room_policies(&mut self, session_id: ChatSessionId) {
        if self.session(session_id).is_none() {
            return;
        }
        self.room_policies
            .retain(|(stored_session, _), _| *stored_session != session_id);
    }

    pub fn moderation_audit_page(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> Option<&ModerationAuditPage> {
        let server_id = &self.session(session_id)?.server.server_id;
        self.moderation_audit_pages
            .get(&(server_id.clone(), room_id))
    }

    pub(crate) fn replace_moderation_audit_page(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        page: ModerationAuditPage,
    ) -> Result<(), &'static str> {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return Err("moderation audit page did not identify an active session");
        };
        page.validate_room(room_id)
            .map_err(|_| "moderation audit page failed room validation")?;
        let key = (server_id, room_id);
        let replaced_records = self
            .moderation_audit_pages
            .get(&key)
            .map_or(0, |current| current.records.len());
        let replaced_bytes = self
            .moderation_audit_pages
            .get(&key)
            .map_or(0, moderation_audit_page_bytes);
        let retained_records = self
            .moderation_audit_pages
            .values()
            .map(|page| page.records.len())
            .sum::<usize>()
            .saturating_sub(replaced_records)
            .saturating_add(page.records.len());
        let session_records = self
            .moderation_audit_pages
            .iter()
            .filter(|((stored_server, _), _)| stored_server == &key.0)
            .map(|(_, page)| page.records.len())
            .sum::<usize>()
            .saturating_sub(replaced_records)
            .saturating_add(page.records.len());
        let retained_bytes = self
            .moderation_audit_pages
            .values()
            .map(moderation_audit_page_bytes)
            .sum::<usize>()
            .saturating_sub(replaced_bytes)
            .saturating_add(moderation_audit_page_bytes(&page));
        if session_records > CHAT_MODERATION_AUDIT_MAX_RECORDS_PER_SESSION
            || retained_records > CHAT_MODERATION_AUDIT_MAX_RECORDS
            || retained_bytes > CHAT_MODERATION_AUDIT_MAX_BYTES
        {
            self.moderation_audit_pages.remove(&key);
            return Err("moderation audit client projection exceeded its bounded retention");
        }
        self.moderation_audit_pages.insert(key, page);
        Ok(())
    }

    pub(crate) fn append_moderation_audit_page(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        before_audit_id: EventId,
        page: ModerationAuditPage,
    ) -> Result<(), &'static str> {
        page.validate_room(room_id)
            .map_err(|_| "moderation audit page failed room validation")?;
        let Some(current) = self.moderation_audit_page(session_id, room_id) else {
            return Err("moderation audit older page has no retained first page");
        };
        if current.records.last().map(|record| record.audit_id) != Some(before_audit_id) {
            return Err("moderation audit older page cursor does not match retained history");
        }
        if page
            .records
            .iter()
            .any(|record| record.audit_id >= before_audit_id)
        {
            return Err("moderation audit older page violated its exclusive cursor");
        }
        let mut combined = current.clone();
        combined.records.extend(page.records);
        combined
            .clone()
            .into_frame_values()
            .map_err(|_| "moderation audit accumulated page failed bounded ordering")?;
        self.replace_moderation_audit_page(session_id, room_id, combined)
    }

    pub(crate) fn clear_moderation_audit(&mut self, session_id: ChatSessionId) {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return;
        };
        self.moderation_audit_pages
            .retain(|(stored_server, _), _| stored_server != &server_id);
    }

    pub(crate) fn clear_moderation_audit_room(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return;
        };
        self.moderation_audit_pages.remove(&(server_id, room_id));
    }

    pub fn retained_mention_count(&self, session_id: ChatSessionId, room_id: RoomId) -> u32 {
        let Some(session) = self.session(session_id) else {
            return 0;
        };
        super::model::retained_local_mention_count(
            &session.events,
            &session.server.server_id,
            room_id,
            self.local_user_id(session_id),
        )
    }

    pub fn reactions_for_targets(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> Vec<ChatReaction> {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.as_str())
        else {
            return Vec::new();
        };
        let targets = target_event_ids.iter().copied().collect::<BTreeSet<_>>();
        let range_start = (
            server_id.to_owned(),
            room_id,
            EventId::MIN,
            super::protocol::UserId::MIN,
            ReactionToken::ThumbsUp,
        );
        let range_end = (
            server_id.to_owned(),
            room_id,
            EventId::MAX,
            super::protocol::UserId::MAX,
            ReactionToken::Question,
        );
        let mut reactions = self
            .reactions
            .range(range_start..=range_end)
            .map(|(_, reaction)| reaction)
            .filter(|reaction| targets.contains(&reaction.target_event_id))
            .cloned()
            .collect::<Vec<_>>();
        reactions.sort_unstable_by_key(|reaction| {
            (
                reaction.target_event_id,
                reaction.token.as_str(),
                reaction.actor_user_id,
            )
        });
        reactions
    }

    pub fn reaction_snapshot_complete(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> bool {
        self.session(session_id).is_some_and(|session| {
            self.authoritative_reaction_targets
                .get(&session.server.server_id)
                .is_some_and(|targets| targets.contains(&(room_id, target_event_id)))
        })
    }

    pub fn authoritative_reaction_targets(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
    ) -> BTreeSet<EventId> {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.as_str())
        else {
            return BTreeSet::new();
        };
        self.authoritative_reaction_targets
            .get(server_id)
            .into_iter()
            .flatten()
            .filter_map(|(stored_room, target)| (*stored_room == room_id).then_some(*target))
            .collect()
    }

    pub(crate) fn mark_reactions_stale(&mut self, session_id: ChatSessionId) {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return;
        };
        self.authoritative_reaction_targets.remove(&server_id);
    }

    pub(crate) fn apply_reaction_event(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event: ReactionEvent,
    ) -> Result<bool, &'static str> {
        event
            .into_frame_body()
            .map_err(|_| "reaction event is invalid")?;
        let Some(session) = self.session(session_id) else {
            return Err("reaction session is unavailable");
        };
        if !session.events.iter().any(|candidate| {
            candidate.room_id == room_id
                && candidate.event_id == event.target_event_id
                && !is_transient_local_event_id(candidate.event_id)
                && chat_event_supports_reactions(candidate)
        }) {
            return Err("reaction target is not retained");
        }
        let server_id = session.server.server_id.clone();
        let key = (
            server_id.clone(),
            room_id,
            event.target_event_id,
            event.actor_user_id,
            event.token,
        );
        let mut next = self.reactions.clone();
        let changed = match event.action {
            ReactionAction::Add => next
                .insert(
                    key,
                    ChatReaction {
                        server_id,
                        room_id,
                        target_event_id: event.target_event_id,
                        actor_user_id: event.actor_user_id,
                        token: event.token,
                        created_at_unix: event.at_unix,
                    },
                )
                .is_none(),
            ReactionAction::Remove => next.remove(&key).is_some(),
        };
        if !chat_reactions_fit_bounds(next.values()) {
            return Err("reaction state exceeds client retention limits");
        }
        self.reactions = next;
        Ok(changed)
    }

    pub(crate) fn replace_reaction_snapshot(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: &ReactionSnapshot,
    ) -> Result<(), &'static str> {
        self.replace_reaction_snapshot_with_authority(session_id, room_id, snapshot, true)
    }

    fn replace_reaction_snapshot_with_authority(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: &ReactionSnapshot,
        authoritative: bool,
    ) -> Result<(), &'static str> {
        snapshot
            .clone()
            .into_frame_body()
            .map_err(|_| "reaction snapshot is invalid")?;
        let Some(session) = self.session(session_id) else {
            return Err("reaction session is unavailable");
        };
        let target_set = snapshot
            .target_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if !target_set.iter().all(|target_event_id| {
            session.events.iter().any(|event| {
                event.room_id == room_id
                    && event.event_id == *target_event_id
                    && !is_transient_local_event_id(event.event_id)
                    && chat_event_supports_reactions(event)
            })
        }) {
            return Err("reaction snapshot target is not retained");
        }
        let server_id = session.server.server_id.clone();
        let mut next = self.reactions.clone();
        next.retain(|(stored_server, stored_room, target, ..), _| {
            stored_server != &server_id || *stored_room != room_id || !target_set.contains(target)
        });
        for entry in &snapshot.entries {
            let reaction = ChatReaction {
                server_id: server_id.clone(),
                room_id,
                target_event_id: entry.target_event_id,
                actor_user_id: entry.actor_user_id,
                token: entry.token,
                created_at_unix: entry.created_at_unix,
            };
            next.insert(
                (
                    server_id.clone(),
                    room_id,
                    entry.target_event_id,
                    entry.actor_user_id,
                    entry.token,
                ),
                reaction,
            );
        }
        if !chat_reactions_fit_bounds(next.values()) {
            return Err("reaction snapshot exceeds client retention limits");
        }
        self.reactions = next;
        if authoritative {
            self.prune_reaction_state_for_server(&server_id);
            self.authoritative_reaction_targets
                .entry(server_id)
                .or_default()
                .extend(
                    snapshot
                        .target_event_ids
                        .iter()
                        .map(|target| (room_id, *target)),
                );
        }
        Ok(())
    }

    pub fn message_revision_for_target(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> Option<&ChatMessageRevision> {
        let server_id = self.session(session_id)?.server.server_id.as_str();
        self.message_revisions
            .get(&(server_id.to_owned(), room_id, target_event_id))
    }

    pub fn message_revision_target_authoritative(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> bool {
        self.session(session_id).is_some_and(|session| {
            self.authoritative_message_revision_targets
                .get(&session.server.server_id)
                .is_some_and(|targets| targets.contains(&(room_id, target_event_id)))
        })
    }

    pub fn message_revision_snapshot_complete(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> bool {
        self.message_revision_target_authoritative(session_id, room_id, target_event_id)
    }

    fn mark_message_revision_target_authoritative(
        &mut self,
        server_id: ServerId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> bool {
        self.authoritative_message_revision_targets
            .entry(server_id)
            .or_default()
            .insert((room_id, target_event_id))
    }

    pub(crate) fn mark_message_revisions_stale(&mut self, session_id: ChatSessionId) {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return;
        };
        self.authoritative_message_revision_targets
            .remove(&server_id);
    }

    pub(crate) fn apply_message_revision_event(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event: MessageRevisionEvent,
    ) -> Result<bool, &'static str> {
        event
            .clone()
            .into_frame_body()
            .map_err(|_| "message revision event is invalid")?;
        let Some(session) = self.session(session_id) else {
            return Err("message revision session is unavailable");
        };
        if !session.events.iter().any(|candidate| {
            candidate.room_id == room_id
                && candidate.event_id == event.target_event_id
                && !is_transient_local_event_id(candidate.event_id)
                && chat_event_supports_message_revisions(candidate)
        }) {
            return Err("message revision target is not retained");
        }
        let server_id = session.server.server_id.clone();
        let key = (server_id.clone(), room_id, event.target_event_id);
        let revision = ChatMessageRevision {
            server_id: server_id.clone(),
            room_id,
            target_event_id: event.target_event_id,
            latest_revision_event_id: event.revision_event_id,
            action: event.action,
            actor_user_id: event.actor_user_id,
            replacement_body: event.replacement,
            at_unix: event.at_unix,
            revision_number: event.revision_number,
        };
        if let Some(current) = self.message_revisions.get(&key) {
            if current == &revision {
                return Ok(self.mark_message_revision_target_authoritative(
                    server_id,
                    room_id,
                    event.target_event_id,
                ));
            }
            if revision.latest_revision_event_id <= current.latest_revision_event_id
                || revision.revision_number <= current.revision_number
                || current.action == MessageRevisionAction::Tombstone
            {
                return Err("message revision event is stale or conflicts with retained state");
            }
        }
        let mut next = self.message_revisions.clone();
        next.insert(key, revision);
        if !chat_message_revisions_fit_bounds(next.values()) {
            return Err("message revision state exceeds client retention limits");
        }
        self.message_revisions = next;
        self.mark_message_revision_target_authoritative(server_id, room_id, event.target_event_id);
        Ok(true)
    }

    pub(crate) fn replace_message_revision_snapshot(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: &MessageRevisionSnapshot,
    ) -> Result<(), &'static str> {
        let result = self
            .replace_message_revision_snapshot_with_authority(session_id, room_id, snapshot, true);
        if result.is_err() {
            self.mark_message_revisions_stale(session_id);
        }
        result
    }

    fn replace_message_revision_snapshot_with_authority(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: &MessageRevisionSnapshot,
        authoritative: bool,
    ) -> Result<(), &'static str> {
        snapshot
            .clone()
            .into_frame_body()
            .map_err(|_| "message revision snapshot is invalid")?;
        let Some(session) = self.session(session_id) else {
            return Err("message revision session is unavailable");
        };
        let target_set = snapshot
            .target_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if !target_set.iter().all(|target_event_id| {
            session.events.iter().any(|event| {
                event.room_id == room_id
                    && event.event_id == *target_event_id
                    && !is_transient_local_event_id(event.event_id)
                    && chat_event_supports_message_revisions(event)
            })
        }) {
            return Err("message revision snapshot target is not retained");
        }
        let server_id = session.server.server_id.clone();
        let mut next = self.message_revisions.clone();
        next.retain(|(stored_server, stored_room, target), _| {
            stored_server != &server_id || *stored_room != room_id || !target_set.contains(target)
        });
        for entry in &snapshot.entries {
            next.insert(
                (server_id.clone(), room_id, entry.target_event_id),
                ChatMessageRevision {
                    server_id: server_id.clone(),
                    room_id,
                    target_event_id: entry.target_event_id,
                    latest_revision_event_id: entry.latest_revision_event_id,
                    action: entry.action,
                    actor_user_id: entry.actor_user_id,
                    replacement_body: entry.replacement.clone(),
                    at_unix: entry.at_unix,
                    revision_number: entry.revision_number,
                },
            );
        }
        if !chat_message_revisions_fit_bounds(next.values()) {
            return Err("message revision snapshot exceeds client retention limits");
        }
        self.message_revisions = next;
        if authoritative {
            self.prune_message_revision_state_for_server(&server_id);
            self.authoritative_message_revision_targets
                .entry(server_id)
                .or_default()
                .extend(
                    snapshot
                        .target_event_ids
                        .iter()
                        .map(|target| (room_id, *target)),
                );
        }
        Ok(())
    }

    pub fn pin_for_target(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> Option<&ChatPin> {
        let server_id = self.session(session_id)?.server.server_id.as_str();
        self.pins
            .get(&(server_id.to_owned(), room_id, target_event_id))
    }

    pub fn pin_target_authoritative(
        &self,
        session_id: ChatSessionId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> bool {
        self.session(session_id).is_some_and(|session| {
            self.authoritative_pin_targets
                .get(&session.server.server_id)
                .is_some_and(|targets| targets.contains(&(room_id, target_event_id)))
        })
    }

    pub(crate) fn mark_pins_stale(&mut self, session_id: ChatSessionId) {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return;
        };
        self.authoritative_pin_targets.remove(&server_id);
        self.pin_event_cursors
            .retain(|(stored_server, ..), _| stored_server != &server_id);
    }

    pub(crate) fn mark_pin_room_stale(&mut self, session_id: ChatSessionId, room_id: RoomId) {
        let Some(server_id) = self
            .session(session_id)
            .map(|session| session.server.server_id.clone())
        else {
            return;
        };
        if let Some(targets) = self.authoritative_pin_targets.get_mut(&server_id) {
            targets.retain(|(stored_room, _)| *stored_room != room_id);
            if targets.is_empty() {
                self.authoritative_pin_targets.remove(&server_id);
            }
        }
        self.pin_event_cursors
            .retain(|(stored_server, stored_room, _), _| {
                stored_server != &server_id || *stored_room != room_id
            });
    }

    pub(crate) fn apply_pin_event(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        event: PinEvent,
    ) -> Result<bool, &'static str> {
        event
            .into_frame_body()
            .map_err(|_| "pin event is invalid")?;
        let Some(session) = self.session(session_id) else {
            return Err("pin session is unavailable");
        };
        if !session.events.iter().any(|candidate| {
            candidate.room_id == room_id
                && candidate.event_id == event.target_event_id
                && !is_transient_local_event_id(candidate.event_id)
                && chat_event_supports_pins(candidate)
        }) {
            return Err("pin target is not retained");
        }
        let server_id = session.server.server_id.clone();
        let key = (server_id.clone(), room_id, event.target_event_id);
        if let Some(current_event_id) = self.pin_event_cursors.get(&key) {
            if event.pin_event_id < *current_event_id {
                return Err("pin event is stale");
            }
            if event.pin_event_id == *current_event_id {
                let matches_current = match event.action {
                    PinAction::Pin => self.pins.get(&key).is_some_and(|pin| {
                        pin.pin_event_id == event.pin_event_id
                            && pin.actor_user_id == event.actor_user_id
                            && pin.pinned_at_unix == event.at_unix
                    }),
                    PinAction::Unpin => !self.pins.contains_key(&key),
                };
                if !matches_current {
                    return Err("pin event conflicts with retained state");
                }
                self.mark_pin_target_authoritative(server_id, room_id, event.target_event_id)?;
                return Ok(false);
            }
        }
        let mut next = self.pins.clone();
        let changed = match event.action {
            PinAction::Pin => {
                next.insert(
                    key.clone(),
                    ChatPin {
                        server_id: server_id.clone(),
                        room_id,
                        target_event_id: event.target_event_id,
                        pin_event_id: event.pin_event_id,
                        actor_user_id: event.actor_user_id,
                        pinned_at_unix: event.at_unix,
                    },
                );
                true
            }
            PinAction::Unpin => next.remove(&key).is_some(),
        };
        if !chat_pins_fit_bounds(next.values()) {
            return Err("pin state exceeds client retention limits");
        }
        let mut next_cursors = self.pin_event_cursors.clone();
        next_cursors.insert(key, event.pin_event_id);
        self.ensure_pin_authority_capacity(&server_id, room_id, event.target_event_id)?;
        self.pins = next;
        self.pin_event_cursors = next_cursors;
        self.authoritative_pin_targets
            .entry(server_id)
            .or_default()
            .insert((room_id, event.target_event_id));
        Ok(changed)
    }

    pub(crate) fn replace_pin_snapshot(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: &PinSnapshot,
    ) -> Result<(), &'static str> {
        let result = self.replace_pin_snapshot_with_authority(session_id, room_id, snapshot, true);
        if result.is_err() {
            self.mark_pins_stale(session_id);
        }
        result
    }

    fn replace_pin_snapshot_with_authority(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        snapshot: &PinSnapshot,
        authoritative: bool,
    ) -> Result<(), &'static str> {
        snapshot
            .clone()
            .into_frame_body()
            .map_err(|_| "pin snapshot is invalid")?;
        let Some(session) = self.session(session_id) else {
            return Err("pin session is unavailable");
        };
        let target_set = snapshot
            .target_event_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if !target_set.iter().all(|target_event_id| {
            session.events.iter().any(|event| {
                event.room_id == room_id
                    && event.event_id == *target_event_id
                    && !is_transient_local_event_id(event.event_id)
                    && chat_event_supports_pins(event)
            })
        }) {
            return Err("pin snapshot target is not retained");
        }
        let server_id = session.server.server_id.clone();
        let mut next = self.pins.clone();
        next.retain(|(stored_server, stored_room, target), _| {
            stored_server != &server_id || *stored_room != room_id || !target_set.contains(target)
        });
        for entry in &snapshot.entries {
            next.insert(
                (server_id.clone(), room_id, entry.target_event_id),
                ChatPin {
                    server_id: server_id.clone(),
                    room_id,
                    target_event_id: entry.target_event_id,
                    pin_event_id: entry.pin_event_id,
                    actor_user_id: entry.actor_user_id,
                    pinned_at_unix: entry.pinned_at_unix,
                },
            );
        }
        if !chat_pins_fit_bounds(next.values()) {
            return Err("pin snapshot exceeds client retention limits");
        }
        if authoritative {
            let existing_without_targets = self
                .authoritative_pin_targets
                .values()
                .map(BTreeSet::len)
                .sum::<usize>()
                .saturating_sub(
                    self.authoritative_pin_targets
                        .get(&server_id)
                        .into_iter()
                        .flatten()
                        .filter(|(stored_room, target)| {
                            *stored_room == room_id && target_set.contains(target)
                        })
                        .count(),
                );
            if existing_without_targets.saturating_add(target_set.len()) > CHAT_PIN_MAX_ROWS {
                return Err("pin authority exceeds client retention limits");
            }
        }
        let mut next_cursors = self.pin_event_cursors.clone();
        next_cursors.retain(|(stored_server, stored_room, target), _| {
            stored_server != &server_id || *stored_room != room_id || !target_set.contains(target)
        });
        for entry in &snapshot.entries {
            next_cursors.insert(
                (server_id.clone(), room_id, entry.target_event_id),
                entry.pin_event_id,
            );
        }
        self.pins = next;
        self.pin_event_cursors = next_cursors;
        if authoritative {
            self.prune_pin_state_for_server(&server_id);
            let targets = self.authoritative_pin_targets.entry(server_id).or_default();
            targets.retain(|(stored_room, target)| {
                *stored_room != room_id || !target_set.contains(target)
            });
            targets.extend(target_set.into_iter().map(|target| (room_id, target)));
        }
        Ok(())
    }

    fn ensure_pin_authority_capacity(
        &self,
        server_id: &ServerId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> Result<(), &'static str> {
        let already_present = self
            .authoritative_pin_targets
            .get(server_id)
            .is_some_and(|targets| targets.contains(&(room_id, target_event_id)));
        let total = self
            .authoritative_pin_targets
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        if !already_present && total >= CHAT_PIN_MAX_ROWS {
            return Err("pin authority exceeds client retention limits");
        }
        Ok(())
    }

    fn mark_pin_target_authoritative(
        &mut self,
        server_id: ServerId,
        room_id: RoomId,
        target_event_id: EventId,
    ) -> Result<bool, &'static str> {
        self.ensure_pin_authority_capacity(&server_id, room_id, target_event_id)?;
        Ok(self
            .authoritative_pin_targets
            .entry(server_id)
            .or_default()
            .insert((room_id, target_event_id)))
    }

    pub fn room_mute_except_mentions(&self, session_id: ChatSessionId, room_id: RoomId) -> bool {
        self.session(session_id).is_some_and(|session| {
            self.mute_except_mentions
                .get(&session.server.server_id)
                .is_some_and(|rooms| rooms.contains(&room_id))
        })
    }

    pub fn set_room_mute_except_mentions(
        &mut self,
        session_id: ChatSessionId,
        room_id: RoomId,
        enabled: bool,
    ) -> bool {
        let Some(session) = self.session(session_id) else {
            return false;
        };
        if session.active_room.room_id != room_id
            && !session.rooms.iter().any(|room| room.room_id == room_id)
        {
            return false;
        }
        let server_id = session.server.server_id.clone();
        if enabled {
            let rooms = self.mute_except_mentions.entry(server_id).or_default();
            if !rooms.contains(&room_id) && rooms.len() >= CHAT_SESSION_MAX_ROOMS {
                return false;
            }
            rooms.insert(room_id);
        } else {
            let remove_server =
                self.mute_except_mentions
                    .get_mut(&server_id)
                    .is_some_and(|rooms| {
                        rooms.remove(&room_id);
                        rooms.is_empty()
                    });
            if remove_server {
                self.mute_except_mentions.remove(&server_id);
            }
        }
        true
    }

    pub fn event_allows_unread(&self, session_id: ChatSessionId, event: &ChatEvent) -> bool {
        if !self.room_mute_except_mentions(session_id, event.room_id) {
            return true;
        }
        let Some(local_user_id) = self.local_user_id(session_id) else {
            return false;
        };
        event.message_metadata().is_some_and(|metadata| {
            metadata
                .mentioned_user_ids
                .binary_search(&local_user_id)
                .is_ok()
        })
    }

    pub(crate) fn enforce_status_bounds(&mut self) {
        for session in &mut self.sessions {
            let status = std::mem::take(&mut session.status);
            session.set_status(status);
        }
    }

    pub fn push_session(&mut self, mut session: ChatSessionView) -> bool {
        if self.sessions.len() >= CHAT_CLIENT_MAX_SESSIONS
            || !chat_text_fits(&session.server.server_id, CHAT_SERVER_ID_MAX_BYTES)
            || !chat_text_fits(
                &session.server.destination,
                CHAT_SERVER_DESTINATION_MAX_BYTES,
            )
            || session.active_room.name.trim().is_empty()
            || !chat_text_fits(session.active_room.name.trim(), CHAT_ROOM_NAME_MAX_BYTES)
        {
            return false;
        }
        enforce_history_window(&mut session.events, HistoryWindowEdge::Newest, None);
        session.enforce_presentation_bounds();
        session.enforce_catalog_bounds();
        self.sessions.push(session);
        true
    }

    pub fn remove_session(&mut self, session_id: ChatSessionId) -> Option<ChatSessionView> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.session_id == session_id)?;
        let removed = self.sessions.remove(index);
        if !self
            .sessions
            .iter()
            .any(|session| session.server.server_id == removed.server.server_id)
        {
            self.local_user_ids.remove(&removed.server.server_id);
            self.mute_except_mentions.remove(&removed.server.server_id);
            self.reactions
                .retain(|(server_id, ..), _| server_id != &removed.server.server_id);
            self.authoritative_reaction_targets
                .remove(&removed.server.server_id);
            self.message_revisions
                .retain(|(server_id, ..), _| server_id != &removed.server.server_id);
            self.authoritative_message_revision_targets
                .remove(&removed.server.server_id);
            self.pins
                .retain(|(server_id, ..), _| server_id != &removed.server.server_id);
            self.pin_event_cursors
                .retain(|(server_id, ..), _| server_id != &removed.server.server_id);
            self.authoritative_pin_targets
                .remove(&removed.server.server_id);
            self.moderation_audit_pages
                .retain(|(server_id, _), _| server_id != &removed.server.server_id);
        } else {
            self.prune_message_revision_state_for_server(&removed.server.server_id);
            self.prune_pin_state_for_server(&removed.server.server_id);
        }
        self.room_policies
            .retain(|(stored_session, _), _| *stored_session != session_id);
        Some(removed)
    }

    fn prune_reaction_state_for_server(&mut self, server_id: &ServerId) {
        let retained = self
            .sessions
            .iter()
            .filter(|session| session.server.server_id == *server_id)
            .flat_map(|session| {
                session
                    .events
                    .iter()
                    .map(|event| (event.room_id, event.event_id))
            })
            .collect::<BTreeSet<_>>();
        self.reactions
            .retain(|(stored_server, room_id, target, ..), _| {
                stored_server != server_id || retained.contains(&(*room_id, *target))
            });
        if let Some(targets) = self.authoritative_reaction_targets.get_mut(server_id) {
            targets.retain(|target| retained.contains(target));
            if targets.is_empty() {
                self.authoritative_reaction_targets.remove(server_id);
            }
        }
    }

    fn prune_message_revision_state_for_server(&mut self, server_id: &ServerId) {
        if self.message_revisions.is_empty()
            && self.authoritative_message_revision_targets.is_empty()
        {
            return;
        }
        let retained = self
            .sessions
            .iter()
            .filter(|session| session.server.server_id == *server_id)
            .flat_map(|session| {
                session
                    .events
                    .iter()
                    .filter(|event| chat_event_supports_message_revisions(event))
                    .map(|event| (event.room_id, event.event_id))
            })
            .collect::<BTreeSet<_>>();
        self.message_revisions
            .retain(|(stored_server, room_id, target), _| {
                stored_server != server_id || retained.contains(&(*room_id, *target))
            });
        if let Some(targets) = self
            .authoritative_message_revision_targets
            .get_mut(server_id)
        {
            targets.retain(|target| retained.contains(target));
            if targets.is_empty() {
                self.authoritative_message_revision_targets
                    .remove(server_id);
            }
        }
    }

    fn prune_pin_state_for_server(&mut self, server_id: &ServerId) {
        if self.pins.is_empty()
            && self.pin_event_cursors.is_empty()
            && self.authoritative_pin_targets.is_empty()
        {
            return;
        }
        let retained = self
            .sessions
            .iter()
            .filter(|session| session.server.server_id == *server_id)
            .flat_map(|session| {
                session
                    .events
                    .iter()
                    .filter(|event| chat_event_supports_pins(event))
                    .map(|event| (event.room_id, event.event_id))
            })
            .collect::<BTreeSet<_>>();
        self.pins.retain(|(stored_server, room_id, target), _| {
            stored_server != server_id || retained.contains(&(*room_id, *target))
        });
        self.pin_event_cursors
            .retain(|(stored_server, room_id, target), _| {
                stored_server != server_id || retained.contains(&(*room_id, *target))
            });
        if let Some(targets) = self.authoritative_pin_targets.get_mut(server_id) {
            targets.retain(|target| retained.contains(target));
            if targets.is_empty() {
                self.authoritative_pin_targets.remove(server_id);
            }
        }
    }

    pub fn persist_session<S: ChatStore>(
        &self,
        store: &mut S,
        session_id: ChatSessionId,
    ) -> anyhow::Result<()> {
        let Some(session) = self.session(session_id) else {
            return Ok(());
        };
        store.save_server(session.server.clone())?;
        if let Some(user_id) = self.local_user_id(session_id) {
            store.set_local_user_id(&session.server.server_id, Some(user_id))?;
        }
        for room in &session.rooms {
            store.save_room(room.clone())?;
        }
        store.save_room(session.active_room.clone())?;
        store.set_active_room(&session.server.server_id, session.active_room.room_id)?;
        if session.active_room.joined {
            store.replace_userlist(
                &session.server.server_id,
                session.active_room.room_id,
                session.users.clone(),
            )?;
        } else {
            store.replace_userlist(
                &session.server.server_id,
                session.active_room.room_id,
                Vec::new(),
            )?;
        }
        store.append_events(
            session
                .events
                .iter()
                .filter(|event| !is_transient_local_event_id(event.event_id))
                .cloned()
                .collect(),
        )?;
        for room_id in session
            .events
            .iter()
            .map(|event| event.room_id)
            .collect::<BTreeSet<_>>()
        {
            let target_event_ids = session
                .events
                .iter()
                .filter(|event| {
                    event.room_id == room_id
                        && !is_transient_local_event_id(event.event_id)
                        && chat_event_supports_reactions(event)
                })
                .map(|event| event.event_id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for targets in target_event_ids.chunks(REACTION_SNAPSHOT_MAX_TARGETS) {
                let entries = self
                    .reactions_for_targets(session_id, room_id, targets)
                    .into_iter()
                    .map(|reaction| ReactionSnapshotEntry {
                        target_event_id: reaction.target_event_id,
                        actor_user_id: reaction.actor_user_id,
                        token: reaction.token,
                        created_at_unix: reaction.created_at_unix,
                    })
                    .collect();
                store.replace_reaction_snapshot(
                    &session.server.server_id,
                    room_id,
                    ReactionSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries,
                    },
                )?;
            }
        }
        for room_id in session
            .events
            .iter()
            .map(|event| event.room_id)
            .collect::<BTreeSet<_>>()
        {
            let target_event_ids = session
                .events
                .iter()
                .filter(|event| {
                    event.room_id == room_id
                        && !is_transient_local_event_id(event.event_id)
                        && chat_event_supports_message_revisions(event)
                })
                .map(|event| event.event_id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for targets in target_event_ids.chunks(MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS) {
                let entries = targets
                    .iter()
                    .filter_map(|target| {
                        self.message_revision_for_target(session_id, room_id, *target)
                    })
                    .map(|revision| MessageRevisionSnapshotEntry {
                        target_event_id: revision.target_event_id,
                        latest_revision_event_id: revision.latest_revision_event_id,
                        action: revision.action,
                        actor_user_id: revision.actor_user_id,
                        at_unix: revision.at_unix,
                        replacement: revision.replacement_body.clone(),
                        revision_number: revision.revision_number,
                    })
                    .collect();
                store.replace_message_revision_snapshot(
                    &session.server.server_id,
                    room_id,
                    MessageRevisionSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries,
                    },
                )?;
            }
        }
        for room_id in session
            .events
            .iter()
            .map(|event| event.room_id)
            .collect::<BTreeSet<_>>()
        {
            let target_event_ids = session
                .events
                .iter()
                .filter(|event| {
                    event.room_id == room_id
                        && !is_transient_local_event_id(event.event_id)
                        && chat_event_supports_pins(event)
                })
                .map(|event| event.event_id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for targets in target_event_ids.chunks(
                ROOM_PIN_SNAPSHOT_MAX_TARGETS.min(super::protocol::ROOM_PIN_SNAPSHOT_MAX_ENTRIES),
            ) {
                let entries = targets
                    .iter()
                    .filter_map(|target| self.pin_for_target(session_id, room_id, *target))
                    .map(|pin| PinSnapshotEntry {
                        target_event_id: pin.target_event_id,
                        pin_event_id: pin.pin_event_id,
                        actor_user_id: pin.actor_user_id,
                        pinned_at_unix: pin.pinned_at_unix,
                    })
                    .collect();
                store.replace_pin_snapshot(
                    &session.server.server_id,
                    room_id,
                    PinSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries,
                    },
                )?;
            }
        }
        Ok(())
    }

    pub fn prepend_history_events(
        &mut self,
        session_id: ChatSessionId,
        events: Vec<ChatEvent>,
    ) -> usize {
        if events.is_empty() {
            return 0;
        }
        let Some(session) = self.session_mut(session_id) else {
            return 0;
        };
        let existing = session
            .events
            .iter()
            .map(|event| (event.room_id, event.event_id))
            .collect::<BTreeSet<_>>();
        let mut candidates = BTreeSet::new();
        for event in events {
            candidates.insert((event.room_id, event.event_id));
            if !session.events.iter().any(|existing| {
                existing.room_id == event.room_id && existing.event_id == event.event_id
            }) {
                session.events.push(event);
            }
        }
        session
            .events
            .sort_by_key(|event| (event.room_id, event.event_id));
        enforce_history_window(&mut session.events, HistoryWindowEdge::Oldest, None);
        let retained = session
            .events
            .iter()
            .map(|event| (event.room_id, event.event_id))
            .collect::<BTreeSet<_>>();
        let added = candidates
            .difference(&existing)
            .filter(|key| retained.contains(key))
            .count();
        if added > 0 {
            session.status = format!("loaded {added} older cached event(s)");
        }
        let server_id = session.server.server_id.clone();
        self.prune_message_revision_state_for_server(&server_id);
        self.prune_pin_state_for_server(&server_id);
        added
    }

    pub(crate) fn append_event_bounded(
        &mut self,
        session_id: ChatSessionId,
        event: ChatEvent,
        sort_after: bool,
        edge: HistoryWindowEdge,
    ) -> bool {
        let key = (event.room_id, event.event_id);
        let Some(session) = self.session_mut(session_id) else {
            return false;
        };
        if session
            .events
            .iter()
            .any(|existing| (existing.room_id, existing.event_id) == key)
        {
            return false;
        }
        session.events.push(event);
        if sort_after {
            session
                .events
                .sort_by_key(|event| (event.room_id, event.event_id));
        }
        enforce_history_window(&mut session.events, edge, Some(key));
        let retained = session
            .events
            .iter()
            .any(|event| (event.room_id, event.event_id) == key);
        let server_id = session.server.server_id.clone();
        self.prune_message_revision_state_for_server(&server_id);
        self.prune_pin_state_for_server(&server_id);
        retained
    }

    pub fn load_cached_history_before<S: ChatStore>(
        &mut self,
        store: &S,
        session_id: ChatSessionId,
        limit: usize,
    ) -> anyhow::Result<usize> {
        let Some(session) = self.session(session_id) else {
            return Ok(0);
        };
        let server_id = session.server.server_id.clone();
        let room_id = session.active_room.room_id;
        let before = session
            .events
            .iter()
            .filter(|event| event.room_id == room_id)
            .map(|event| event.event_id)
            .min()
            .unwrap_or(EventId::MAX);
        let events = store.events_before(&server_id, room_id, before, limit)?;
        Ok(self.prepend_history_events(session_id, events))
    }

    pub fn load_cached_room_history<S: ChatStore>(
        &mut self,
        store: &S,
        session_id: ChatSessionId,
        limit: usize,
    ) -> anyhow::Result<usize> {
        let Some(session) = self.session(session_id) else {
            return Ok(0);
        };
        let server_id = session.server.server_id.clone();
        let room_id = session.active_room.room_id;
        let events = store.latest_events(&server_id, room_id, limit)?;
        Ok(self.prepend_history_events(session_id, events))
    }

    pub fn restore_from_store<S: ChatStore>(
        &mut self,
        store: &S,
        event_limit: usize,
    ) -> anyhow::Result<usize> {
        let mut restored = 0;
        for server in store.saved_servers()? {
            if self.sessions.len() >= CHAT_CLIENT_MAX_SESSIONS {
                break;
            }
            if !is_restorable_server_destination(&server.destination) {
                continue;
            }
            let rooms = store.rooms_for_server(&server.server_id)?;
            let Some(room) =
                restore_room_for_server(store, &server.server_id, &rooms, event_limit)?
            else {
                continue;
            };
            let users = store.users_for_room(&server.server_id, room.room_id)?;
            let mut events = Vec::new();
            for known_room in &rooms {
                events.extend(store.latest_events(
                    &server.server_id,
                    known_room.room_id,
                    event_limit,
                )?);
            }
            events.sort_by_key(|event| (event.room_id, event.event_id));
            events.dedup_by_key(|event| (event.room_id, event.event_id));
            let session_id = self.reserve_session_id();
            let local_user_id = store.local_user_id(&server.server_id)?;
            let server_id = server.server_id.clone();
            let mut muted_rooms = Vec::new();
            for known_room in &rooms {
                if store.room_mute_except_mentions(&server_id, known_room.room_id)? {
                    muted_rooms.push(known_room.room_id);
                }
            }
            if !self.push_session(ChatSessionView {
                session_id,
                server,
                rooms,
                active_room: room,
                users,
                events,
                status: "restored from local cache".into(),
            }) {
                break;
            }
            if let Some(user_id) = local_user_id {
                self.local_user_ids.insert(server_id.clone(), user_id);
            }
            for room_id in muted_rooms {
                self.mute_except_mentions
                    .entry(server_id.clone())
                    .or_default()
                    .insert(room_id);
            }
            let retained_by_room = self
                .session(session_id)
                .map(|session| {
                    let mut retained = BTreeMap::<RoomId, Vec<EventId>>::new();
                    for event in &session.events {
                        if !is_transient_local_event_id(event.event_id)
                            && chat_event_supports_reactions(event)
                        {
                            retained
                                .entry(event.room_id)
                                .or_default()
                                .push(event.event_id);
                        }
                    }
                    retained
                })
                .unwrap_or_default();
            for (room_id, mut target_event_ids) in retained_by_room {
                target_event_ids.sort_unstable();
                target_event_ids.dedup();
                for targets in target_event_ids.chunks(REACTION_SNAPSHOT_MAX_TARGETS) {
                    let reactions = store.reactions_for_targets(&server_id, room_id, targets)?;
                    let snapshot = ReactionSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries: reactions
                            .into_iter()
                            .map(|reaction| ReactionSnapshotEntry {
                                target_event_id: reaction.target_event_id,
                                actor_user_id: reaction.actor_user_id,
                                token: reaction.token,
                                created_at_unix: reaction.created_at_unix,
                            })
                            .collect(),
                    };
                    self.replace_reaction_snapshot_with_authority(
                        session_id, room_id, &snapshot, false,
                    )
                    .map_err(anyhow::Error::msg)?;
                }
            }
            let retained_revision_targets = self
                .session(session_id)
                .map(|session| {
                    let mut retained = BTreeMap::<RoomId, Vec<EventId>>::new();
                    for event in &session.events {
                        if !is_transient_local_event_id(event.event_id)
                            && chat_event_supports_message_revisions(event)
                        {
                            retained
                                .entry(event.room_id)
                                .or_default()
                                .push(event.event_id);
                        }
                    }
                    retained
                })
                .unwrap_or_default();
            for (room_id, mut target_event_ids) in retained_revision_targets {
                target_event_ids.sort_unstable();
                target_event_ids.dedup();
                for targets in target_event_ids.chunks(MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS) {
                    let revisions =
                        store.message_revisions_for_targets(&server_id, room_id, targets)?;
                    let snapshot = MessageRevisionSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries: revisions
                            .into_iter()
                            .map(|revision| MessageRevisionSnapshotEntry {
                                target_event_id: revision.target_event_id,
                                latest_revision_event_id: revision.latest_revision_event_id,
                                action: revision.action,
                                actor_user_id: revision.actor_user_id,
                                at_unix: revision.at_unix,
                                replacement: revision.replacement_body,
                                revision_number: revision.revision_number,
                            })
                            .collect(),
                    };
                    self.replace_message_revision_snapshot_with_authority(
                        session_id, room_id, &snapshot, false,
                    )
                    .map_err(anyhow::Error::msg)?;
                }
            }
            let retained_pin_targets = self
                .session(session_id)
                .map(|session| {
                    let mut retained = BTreeMap::<RoomId, Vec<EventId>>::new();
                    for event in &session.events {
                        if !is_transient_local_event_id(event.event_id)
                            && chat_event_supports_pins(event)
                        {
                            retained
                                .entry(event.room_id)
                                .or_default()
                                .push(event.event_id);
                        }
                    }
                    retained
                })
                .unwrap_or_default();
            for (room_id, mut target_event_ids) in retained_pin_targets {
                target_event_ids.sort_unstable();
                target_event_ids.dedup();
                for targets in target_event_ids.chunks(
                    ROOM_PIN_SNAPSHOT_MAX_TARGETS
                        .min(super::protocol::ROOM_PIN_SNAPSHOT_MAX_ENTRIES),
                ) {
                    let pins = store.pins_for_targets(&server_id, room_id, targets)?;
                    let snapshot = PinSnapshot {
                        target_event_ids: targets.to_vec(),
                        entries: pins
                            .into_iter()
                            .map(|pin| PinSnapshotEntry {
                                target_event_id: pin.target_event_id,
                                pin_event_id: pin.pin_event_id,
                                actor_user_id: pin.actor_user_id,
                                pinned_at_unix: pin.pinned_at_unix,
                            })
                            .collect(),
                    };
                    self.replace_pin_snapshot_with_authority(session_id, room_id, &snapshot, false)
                        .map_err(anyhow::Error::msg)?;
                }
            }
            restored += 1;
        }
        Ok(restored)
    }
}

fn moderation_audit_page_bytes(page: &ModerationAuditPage) -> usize {
    page.records.iter().fold(0usize, |bytes, record| {
        bytes
            .saturating_add(CHAT_MODERATION_AUDIT_RECORD_OVERHEAD_BYTES)
            .saturating_add(record.actor_display_name_at_action.len())
            .saturating_add(
                record
                    .target_display_name_at_action
                    .as_deref()
                    .map_or(0, str::len),
            )
    })
}

fn retained_room_catalog_bytes(rooms: &[ChatRoomSummary]) -> usize {
    rooms
        .iter()
        .map(|room| {
            std::mem::size_of::<ChatRoomSummary>()
                .saturating_add(room.server_id.capacity())
                .saturating_add(room.name.capacity())
                .saturating_add(room.topic.as_ref().map_or(0, String::capacity))
        })
        .fold(0, usize::saturating_add)
}

fn retained_user_catalog_bytes(users: &[ChatUserSummary]) -> usize {
    users
        .iter()
        .map(|user| {
            std::mem::size_of::<ChatUserSummary>()
                .saturating_add(user.server_id.capacity())
                .saturating_add(user.display_name.capacity())
        })
        .fold(0, usize::saturating_add)
}

pub(crate) fn enforce_room_catalog_bounds(
    rooms: &mut Vec<ChatRoomSummary>,
    active_room_id: RoomId,
) -> usize {
    let before = rooms.len();
    let mut retained_bytes = retained_room_catalog_bytes(rooms);
    if rooms.len() <= CHAT_SESSION_MAX_ROOMS && retained_bytes <= CHAT_SESSION_MAX_ROOM_BYTES {
        return 0;
    }
    let mut ranked = (0..rooms.len()).collect::<Vec<_>>();
    ranked.sort_by_key(|index| {
        let room = &rooms[*index];
        (
            room.room_id == active_room_id,
            room.joined,
            room.unread > 0,
            std::cmp::Reverse(*index),
        )
    });
    let mut remove = vec![false; rooms.len()];
    let mut retained_items = rooms.len();
    for index in ranked {
        if retained_items <= CHAT_SESSION_MAX_ROOMS && retained_bytes <= CHAT_SESSION_MAX_ROOM_BYTES
        {
            break;
        }
        remove[index] = true;
        retained_items = retained_items.saturating_sub(1);
        retained_bytes = retained_bytes.saturating_sub(retained_room_catalog_bytes(
            std::slice::from_ref(&rooms[index]),
        ));
    }
    let mut index = 0;
    rooms.retain(|_| {
        let keep = !remove[index];
        index += 1;
        keep
    });
    before.saturating_sub(rooms.len())
}

pub(crate) fn enforce_user_catalog_bounds(users: &mut Vec<ChatUserSummary>) -> usize {
    let before = users.len();
    let mut retained_bytes = retained_user_catalog_bytes(users);
    while users.len() > CHAT_SESSION_MAX_USERS || retained_bytes > CHAT_SESSION_MAX_USER_BYTES {
        let Some(user) = users.pop() else {
            break;
        };
        retained_bytes =
            retained_bytes.saturating_sub(retained_user_catalog_bytes(std::slice::from_ref(&user)));
    }
    before.saturating_sub(users.len())
}

fn retained_history_bytes(events: &[ChatEvent]) -> usize {
    events
        .iter()
        .map(chat_event_retained_bytes)
        .fold(0, usize::saturating_add)
}

fn chat_event_retained_bytes(event: &ChatEvent) -> usize {
    let kind_bytes = match &event.kind {
        super::model::ChatEventKind::Message { body }
        | super::model::ChatEventKind::Action { body }
        | super::model::ChatEventKind::Notice { body }
        | super::model::ChatEventKind::System { body } => body.capacity(),
        super::model::ChatEventKind::RichMessage { body, metadata } => {
            body.capacity().saturating_add(
                metadata
                    .mentioned_user_ids
                    .capacity()
                    .saturating_mul(std::mem::size_of::<u32>()),
            )
        }
        super::model::ChatEventKind::Upload {
            resource_id,
            filename,
            ..
        } => resource_id.capacity().saturating_add(filename.capacity()),
    };
    std::mem::size_of::<ChatEvent>()
        .saturating_add(event.server_id.capacity())
        .saturating_add(
            event
                .actor_display_name
                .as_ref()
                .map_or(0, String::capacity),
        )
        .saturating_add(kind_bytes)
}

fn enforce_history_window(
    events: &mut Vec<ChatEvent>,
    edge: HistoryWindowEdge,
    protected: Option<(RoomId, EventId)>,
) {
    let mut retained_bytes = retained_history_bytes(events);
    if events.len() <= CHAT_SESSION_HISTORY_MAX_EVENTS
        && retained_bytes <= CHAT_SESSION_HISTORY_MAX_BYTES
    {
        return;
    }
    let mut ranked = (0..events.len()).collect::<Vec<_>>();
    ranked.sort_by_key(|index| {
        let event = &events[*index];
        (event.at_unix, event.room_id, event.event_id)
    });
    if matches!(edge, HistoryWindowEdge::Oldest) {
        ranked.reverse();
    }
    if let Some(protected) = protected {
        ranked.sort_by_key(|index| {
            let event = &events[*index];
            (event.room_id, event.event_id) == protected
        });
    }
    let mut remove = vec![false; events.len()];
    let mut retained_items = events.len();
    for index in ranked {
        if retained_items <= CHAT_SESSION_HISTORY_MAX_EVENTS
            && retained_bytes <= CHAT_SESSION_HISTORY_MAX_BYTES
        {
            break;
        }
        remove[index] = true;
        retained_items = retained_items.saturating_sub(1);
        retained_bytes = retained_bytes.saturating_sub(chat_event_retained_bytes(&events[index]));
    }
    let mut index = 0;
    events.retain(|_| {
        let keep = !remove[index];
        index += 1;
        keep
    });
}

fn is_transient_local_event_id(event_id: EventId) -> bool {
    event_id > u64::MAX.saturating_sub(1_000_000)
}

fn restore_room_for_server<S: ChatStore>(
    store: &S,
    server_id: &ServerId,
    rooms: &[ChatRoomSummary],
    event_limit: usize,
) -> anyhow::Result<Option<ChatRoomSummary>> {
    if let Some(active_room_id) = store.active_room_id(server_id)? {
        if let Some(room) = rooms
            .iter()
            .find(|room| room.room_id == active_room_id)
            .cloned()
        {
            return Ok(Some(room));
        }
    }
    if let Some(room) = rooms.iter().find(|room| room.joined).cloned() {
        return Ok(Some(room));
    }
    for room in rooms {
        if !store
            .latest_events(server_id, room.room_id, event_limit)?
            .is_empty()
        {
            let mut room = room.clone();
            room.joined = true;
            return Ok(Some(room));
        }
    }
    Ok(rooms.first().cloned().map(|mut room| {
        room.joined = true;
        room
    }))
}

pub fn is_restorable_server_destination(destination: &str) -> bool {
    let destination = destination.trim();
    destination.len() >= 32 && destination.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::model::ChatEventKind;
    use crate::chat::store::SqliteChatStore;

    const TEST_DESTINATION: &str = "abcd1234abcd1234abcd1234abcd1234";

    fn bounded_history_event(event_id: u64, body_bytes: usize) -> ChatEvent {
        ChatEvent {
            server_id: "server-a".into(),
            room_id: 1,
            event_id,
            actor_user_id: Some(1),
            actor_display_name: Some("Alice".into()),
            at_unix: event_id as i64,
            kind: ChatEventKind::Message {
                body: "x".repeat(body_bytes),
            },
        }
    }

    fn bounded_history_session(
        session_id: ChatSessionId,
        events: Vec<ChatEvent>,
    ) -> ChatSessionView {
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events,
            status: String::new(),
        }
    }

    #[test]
    fn room_policy_projection_is_catalog_bounded_and_session_owned() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(session_id, Vec::new())));
        let policy =
            RoomPolicyProjection::new(super::super::protocol::ROOM_POLICY_ANNOUNCEMENT, 30)
                .expect("bounded policy");

        assert!(client.update_room_policy(session_id, 1, policy));
        assert_eq!(client.room_policy(session_id, 1), Some(policy));
        assert_eq!(client.room_slow_mode_seconds(session_id, 1), Some(30));
        assert_eq!(client.room_upload_policy(session_id, 1), None);
        assert!(!client.update_room_policy(session_id, 2, policy));

        let oversized = vec![(1, policy); CHAT_SESSION_MAX_ROOMS + 1];
        assert!(!client.replace_room_policies(session_id, &oversized));
        assert_eq!(client.room_policy(session_id, 1), Some(policy));

        assert!(client.remove_session(session_id).is_some());
        assert_eq!(client.room_policy(session_id, 1), None);
    }

    #[test]
    #[ignore = "explicit isolated moderation-audit client projection measurement"]
    fn moderation_audit_projection_measurement() {
        use crate::chat::protocol::{
            ModerationAuditAction, ModerationAuditRecord, MODERATION_AUDIT_PAGE_MAX_ENTRIES,
        };
        use std::time::Instant;

        let server_count =
            CHAT_MODERATION_AUDIT_MAX_RECORDS / CHAT_MODERATION_AUDIT_MAX_RECORDS_PER_SESSION;
        let pages_per_server =
            CHAT_MODERATION_AUDIT_MAX_RECORDS_PER_SESSION / MODERATION_AUDIT_PAGE_MAX_ENTRIES;
        assert_eq!(
            server_count * pages_per_server * MODERATION_AUDIT_PAGE_MAX_ENTRIES,
            CHAT_MODERATION_AUDIT_MAX_RECORDS
        );
        let mut client = ChatClient::new();
        let mut admission_micros = Vec::with_capacity(server_count * pages_per_server);

        for server_index in 0..server_count {
            let session_id = client.reserve_session_id();
            let mut session = bounded_history_session(session_id, Vec::new());
            session.server.server_id = format!("measurement-server-{server_index}");
            session.active_room.server_id = session.server.server_id.clone();
            session.rooms[0].server_id = session.server.server_id.clone();
            assert!(client.push_session(session));
            for page_index in 0..pages_per_server {
                let room_id = page_index as RoomId + 1;
                let page = ModerationAuditPage {
                    records: (0..MODERATION_AUDIT_PAGE_MAX_ENTRIES)
                        .map(|record_index| ModerationAuditRecord {
                            audit_id: (page_index * MODERATION_AUDIT_PAGE_MAX_ENTRIES
                                + record_index
                                + 1) as EventId,
                            room_id,
                            actor_user_id: 2,
                            actor_display_name_at_action: "Moderator".into(),
                            target_user_id: Some(3),
                            target_display_name_at_action: Some("Target".into()),
                            action: ModerationAuditAction::Kick,
                            committed_at_unix: 1_700_000_000 + record_index as i64,
                            result_role_bits: None,
                            result_status_bits: None,
                        })
                        .collect(),
                };
                let started = Instant::now();
                client
                    .replace_moderation_audit_page(session_id, room_id, page)
                    .expect("measurement admission");
                admission_micros.push(started.elapsed().as_micros());
            }
        }

        let retained_records = client
            .moderation_audit_pages
            .values()
            .map(|page| page.records.len())
            .sum::<usize>();
        let retained_bytes = client
            .moderation_audit_pages
            .values()
            .map(moderation_audit_page_bytes)
            .sum::<usize>();
        let overflow_session_id = client.reserve_session_id();
        let mut overflow_session = bounded_history_session(overflow_session_id, Vec::new());
        overflow_session.server.server_id = "measurement-overflow".into();
        overflow_session.active_room.server_id = overflow_session.server.server_id.clone();
        overflow_session.rooms[0].server_id = overflow_session.server.server_id.clone();
        assert!(client.push_session(overflow_session));
        let overflow_page = ModerationAuditPage {
            records: vec![ModerationAuditRecord {
                audit_id: 1,
                room_id: 1,
                actor_user_id: 2,
                actor_display_name_at_action: "Moderator".into(),
                target_user_id: Some(3),
                target_display_name_at_action: Some("Target".into()),
                action: ModerationAuditAction::Kick,
                committed_at_unix: 1_700_000_000,
                result_role_bits: None,
                result_status_bits: None,
            }],
        };
        assert!(client
            .replace_moderation_audit_page(overflow_session_id, 1, overflow_page)
            .is_err());
        assert!(client
            .moderation_audit_page(overflow_session_id, 1)
            .is_none());

        let percentile = |samples: &mut Vec<u128>, percent: usize| {
            samples.sort_unstable();
            let index = samples
                .len()
                .saturating_mul(percent)
                .saturating_add(99)
                .checked_div(100)
                .unwrap_or(0)
                .saturating_sub(1)
                .min(samples.len().saturating_sub(1));
            samples[index]
        };
        let admission_max = admission_micros.iter().copied().max().unwrap_or(0);
        let admission_p50 = percentile(&mut admission_micros.clone(), 50);
        let admission_p95 = percentile(&mut admission_micros, 95);

        assert_eq!(retained_records, CHAT_MODERATION_AUDIT_MAX_RECORDS);
        assert!(retained_bytes <= CHAT_MODERATION_AUDIT_MAX_BYTES);
        println!(
            "MODERATION_AUDIT_PROJECTION_MEASUREMENT servers={server_count} pages={} records={retained_records} retained_bytes={retained_bytes} admission_p50_us={admission_p50} admission_p95_us={admission_p95} admission_max_us={admission_max}",
            server_count * pages_per_server
        );
    }

    #[test]
    fn client_session_history_is_item_bounded_and_keeps_recent_edge_on_restore() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let excess = 10_u64;
        let events = (1..=(CHAT_SESSION_HISTORY_MAX_EVENTS as u64 + excess))
            .map(|event_id| bounded_history_event(event_id, 1))
            .collect();

        client.push_session(bounded_history_session(session_id, events));

        let session = client.session(session_id).expect("bounded session");
        assert_eq!(session.events.len(), CHAT_SESSION_HISTORY_MAX_EVENTS);
        assert_eq!(session.events.first().map(|event| event.event_id), Some(11));
        assert_eq!(
            session.events.last().map(|event| event.event_id),
            Some(CHAT_SESSION_HISTORY_MAX_EVENTS as u64 + excess)
        );
    }

    #[test]
    fn client_load_older_keeps_old_edge_without_losing_persisted_pagination_floor() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let first_recent = 1_000_u64;
        let events = (first_recent..first_recent + CHAT_SESSION_HISTORY_MAX_EVENTS as u64)
            .map(|event_id| bounded_history_event(event_id, 1))
            .collect();
        client.push_session(bounded_history_session(session_id, events));
        let older = (1..=10)
            .map(|event_id| bounded_history_event(event_id, 1))
            .collect();

        assert_eq!(client.prepend_history_events(session_id, older), 10);

        let session = client.session(session_id).expect("bounded session");
        assert_eq!(session.events.len(), CHAT_SESSION_HISTORY_MAX_EVENTS);
        assert!(session.events.iter().all(|event| event.event_id <= 10
            || event.event_id < first_recent + CHAT_SESSION_HISTORY_MAX_EVENTS as u64 - 10));
        assert_eq!(
            session.events.iter().map(|event| event.event_id).min(),
            Some(1)
        );
    }

    #[test]
    fn client_session_history_is_owned_byte_bounded() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let body_bytes = CHAT_SESSION_HISTORY_MAX_BYTES / 3;
        let events = (1..=4)
            .map(|event_id| bounded_history_event(event_id, body_bytes))
            .collect();

        client.push_session(bounded_history_session(session_id, events));

        let session = client.session(session_id).expect("bounded session");
        assert!(session.retained_history_bytes() <= CHAT_SESSION_HISTORY_MAX_BYTES);
        assert_eq!(session.events.last().map(|event| event.event_id), Some(4));
        assert!(session.events.len() < 4);
    }

    #[test]
    fn client_recent_append_retains_new_event_despite_old_remote_timestamp() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let events = (1..=CHAT_SESSION_HISTORY_MAX_EVENTS as u64)
            .map(|event_id| bounded_history_event(event_id, 1))
            .collect();
        client.push_session(bounded_history_session(session_id, events));
        let mut skewed = bounded_history_event(10_000, 1);
        skewed.at_unix = -10_000;

        assert!(client.append_event_bounded(session_id, skewed, false, HistoryWindowEdge::Newest,));

        let session = client.session(session_id).expect("bounded session");
        assert_eq!(session.events.len(), CHAT_SESSION_HISTORY_MAX_EVENTS);
        assert!(session.events.iter().any(|event| event.event_id == 10_000));
    }

    #[test]
    fn client_refuses_session_overload_without_evicting_open_sessions() {
        let mut client = ChatClient::new();
        for _ in 0..CHAT_CLIENT_MAX_SESSIONS {
            let session_id = client.reserve_session_id();
            assert!(client.push_session(bounded_history_session(session_id, Vec::new())));
        }
        let existing_ids = client
            .sessions()
            .iter()
            .map(|session| session.session_id)
            .collect::<Vec<_>>();
        let overflow_id = client.reserve_session_id();

        assert!(!client.push_session(bounded_history_session(overflow_id, Vec::new())));
        assert_eq!(client.sessions().len(), CHAT_CLIENT_MAX_SESSIONS);
        assert_eq!(
            client
                .sessions()
                .iter()
                .map(|session| session.session_id)
                .collect::<Vec<_>>(),
            existing_ids
        );
    }

    #[test]
    fn client_session_admission_bounds_presentation_metadata() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let mut session = bounded_history_session(session_id, Vec::new());
        session.server.display_name = "☃".repeat(CHAT_SERVER_DISPLAY_MAX_BYTES);
        session.active_room.topic = Some("t".repeat(CHAT_ROOM_TOPIC_MAX_BYTES + 1));
        session.rooms.push(ChatRoomSummary {
            server_id: "server-a".into(),
            room_id: 2,
            name: "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1),
            topic: None,
            unread: 0,
            joined: false,
        });
        session.users.push(ChatUserSummary {
            server_id: "server-a".into(),
            user_id: 2,
            display_name: "u".repeat(CHAT_USER_DISPLAY_MAX_BYTES + 1),
            role_bits: 0,
            status_bits: 0,
            lxmf_available: false,
        });
        session.status = "s".repeat(CHAT_STATUS_MAX_BYTES + 1);

        assert!(client.push_session(session));

        let session = client.session(session_id).expect("bounded metadata");
        assert!(session.server.display_name.len() <= CHAT_SERVER_DISPLAY_MAX_BYTES);
        assert!(session.server.display_name.ends_with('…'));
        assert!(session
            .active_room
            .topic
            .as_ref()
            .is_some_and(|topic| topic.len() <= CHAT_ROOM_TOPIC_MAX_BYTES));
        assert_eq!(session.rooms.len(), 1);
        assert!(session.users.is_empty());
        assert!(session.status.len() <= CHAT_STATUS_MAX_BYTES);
        assert!(session.status.ends_with('…'));
    }

    #[test]
    fn client_session_admission_rejects_oversized_operational_identifiers() {
        let mut client = ChatClient::new();
        let mut oversized_room = bounded_history_session(1, Vec::new());
        oversized_room.active_room.name = "r".repeat(CHAT_ROOM_NAME_MAX_BYTES + 1);
        assert!(!client.push_session(oversized_room));

        let mut oversized_server = bounded_history_session(2, Vec::new());
        oversized_server.server.server_id = "s".repeat(CHAT_SERVER_ID_MAX_BYTES + 1);
        assert!(!client.push_session(oversized_server));
        assert!(client.sessions().is_empty());
    }

    #[test]
    fn client_room_and_user_catalogs_are_item_bounded() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let mut session = bounded_history_session(session_id, Vec::new());
        session.rooms = (1..=CHAT_SESSION_MAX_ROOMS as u32 + 1)
            .map(|room_id| ChatRoomSummary {
                server_id: "server-a".into(),
                room_id,
                name: format!("room-{room_id:04}"),
                topic: None,
                unread: 0,
                joined: room_id == 1,
            })
            .collect();
        session.users = (1..=CHAT_SESSION_MAX_USERS as u32 + 1)
            .map(|user_id| ChatUserSummary {
                server_id: "server-a".into(),
                user_id,
                display_name: format!("user-{user_id:04}"),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            })
            .collect();

        assert!(client.push_session(session));

        let session = client.session(session_id).expect("bounded catalogs");
        assert_eq!(session.rooms.len(), CHAT_SESSION_MAX_ROOMS);
        assert_eq!(session.users.len(), CHAT_SESSION_MAX_USERS);
        assert!(session
            .rooms
            .iter()
            .any(|room| room.room_id == session.active_room.room_id));
    }

    #[test]
    fn client_room_and_user_catalogs_are_owned_byte_bounded() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        let mut session = bounded_history_session(session_id, Vec::new());
        session.rooms.extend((2..=4).map(|room_id| ChatRoomSummary {
            server_id: "server-a".into(),
            room_id,
            name: "r".repeat(CHAT_SESSION_MAX_ROOM_BYTES / 2),
            topic: None,
            unread: 0,
            joined: false,
        }));
        session.users = (1..=4)
            .map(|user_id| ChatUserSummary {
                server_id: "server-a".into(),
                user_id,
                display_name: "u".repeat(CHAT_SESSION_MAX_USER_BYTES / 2),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: false,
            })
            .collect();

        assert!(client.push_session(session));

        let session = client.session(session_id).expect("bounded catalogs");
        assert!(session.retained_room_catalog_bytes() <= CHAT_SESSION_MAX_ROOM_BYTES);
        assert!(session.retained_user_catalog_bytes() <= CHAT_SESSION_MAX_USER_BYTES);
        assert!(session
            .rooms
            .iter()
            .any(|room| room.room_id == session.active_room.room_id));
    }

    #[test]
    fn client_persists_and_restores_sessions() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: TEST_DESTINATION.into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: Some("Lobby topic".into()),
                unread: 0,
                joined: true,
            },
            rooms: vec![
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: Some("Lobby topic".into()),
                    unread: 0,
                    joined: true,
                },
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 2,
                    name: "support".into(),
                    topic: Some("Support topic".into()),
                    unread: 0,
                    joined: false,
                },
            ],
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 1,
                display_name: "Alice".into(),
                role_bits: 1,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 1,
                actor_user_id: Some(1),
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "hello".into(),
                },
            }],
            status: "test".into(),
        });
        assert!(client.bind_local_user_id(session_id, 1));
        assert!(client.set_room_mute_except_mentions(session_id, 2, true));
        store
            .append_events(vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 2,
                event_id: 1,
                actor_user_id: Some(1),
                actor_display_name: Some("Alice".into()),
                at_unix: 1,
                kind: ChatEventKind::Message {
                    body: "support cache".into(),
                },
            }])
            .expect("support event");
        client
            .persist_session(&mut store, session_id)
            .expect("persist session");
        store
            .set_room_mute_except_mentions(&"server-a".into(), 2, true)
            .expect("persist room notification policy");

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(restored.local_user_id(session.session_id), Some(1));
        assert!(restored.room_mute_except_mentions(session.session_id, 2));
        assert!(!restored.room_mute_except_mentions(session.session_id, 1));
        assert_eq!(session.server.display_name, "Server A");
        assert_eq!(session.active_room.name, "lobby");
        assert!(session.active_room.joined);
        assert_eq!(session.rooms.len(), 2);
        assert_eq!(session.users[0].display_name, "Alice");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| (event.room_id, event.event_id))
                .collect::<Vec<_>>(),
            vec![(1, 1), (2, 1)]
        );
    }

    #[test]
    fn client_reactions_are_authoritative_bounded_and_restart_safe() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(
            session_id,
            vec![bounded_history_event(1, 5)]
        )));

        let add = ReactionEvent {
            reaction_event_id: 2,
            target_event_id: 1,
            actor_user_id: 7,
            token: ReactionToken::Heart,
            action: ReactionAction::Add,
            at_unix: 10,
        };
        assert_eq!(client.apply_reaction_event(session_id, 1, add), Ok(true));
        assert_eq!(client.apply_reaction_event(session_id, 1, add), Ok(false));
        assert!(client
            .apply_reaction_event(
                session_id,
                1,
                ReactionEvent {
                    target_event_id: 99,
                    ..add
                }
            )
            .is_err());

        client
            .replace_reaction_snapshot(
                session_id,
                1,
                &ReactionSnapshot {
                    target_event_ids: vec![1],
                    entries: vec![
                        ReactionSnapshotEntry {
                            target_event_id: 1,
                            actor_user_id: 8,
                            token: ReactionToken::Celebrate,
                            created_at_unix: 11,
                        },
                        ReactionSnapshotEntry {
                            target_event_id: 1,
                            actor_user_id: 7,
                            token: ReactionToken::ThumbsUp,
                            created_at_unix: 12,
                        },
                    ],
                },
            )
            .expect("authoritative snapshot");
        let retained = client.reactions_for_targets(session_id, 1, &[1]);
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0].actor_user_id, 8);
        assert_eq!(retained[0].token, ReactionToken::Celebrate);
        assert_eq!(retained[1].actor_user_id, 7);
        assert_eq!(retained[1].token, ReactionToken::ThumbsUp);
        assert!(client.reaction_snapshot_complete(session_id, 1, 1));

        client
            .persist_session(&mut store, session_id)
            .expect("persist reaction state");
        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore reaction state"),
            1
        );
        let restored_session = restored.sessions()[0].session_id;
        assert_eq!(
            restored.reactions_for_targets(restored_session, 1, &[1]),
            retained
        );
        assert!(!restored.reaction_snapshot_complete(restored_session, 1, 1));
        restored.remove_session(restored_session);
        assert!(restored
            .reactions_for_targets(restored_session, 1, &[1])
            .is_empty());
    }

    #[test]
    fn client_message_revisions_are_dormant_ordered_and_restart_safe() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(
            session_id,
            vec![bounded_history_event(1, 5), bounded_history_event(2, 6)]
        )));
        let correction = MessageRevisionEvent {
            revision_event_id: 2,
            target_event_id: 1,
            action: MessageRevisionAction::Correct,
            actor_user_id: 7,
            at_unix: 10,
            replacement: Some("corrected".into()),
            revision_number: 1,
            actor_display_name: Some("Alice".into()),
        };
        assert_eq!(
            client.apply_message_revision_event(session_id, 1, correction.clone()),
            Ok(true)
        );
        assert!(client.message_revision_target_authoritative(session_id, 1, 1));
        assert!(!client.message_revision_target_authoritative(session_id, 1, 2));
        client.mark_message_revisions_stale(session_id);
        assert!(!client.message_revision_target_authoritative(session_id, 1, 1));
        assert!(client
            .apply_message_revision_event(
                session_id,
                1,
                MessageRevisionEvent {
                    revision_event_id: 1,
                    target_event_id: 1,
                    action: MessageRevisionAction::Correct,
                    actor_user_id: 7,
                    at_unix: 11,
                    replacement: Some("stale".into()),
                    revision_number: 1,
                    actor_display_name: None,
                },
            )
            .is_err());
        assert!(!client.message_revision_target_authoritative(session_id, 1, 1));
        assert_eq!(
            client.apply_message_revision_event(session_id, 1, correction.clone()),
            Ok(true)
        );
        assert!(client.message_revision_target_authoritative(session_id, 1, 1));
        assert_eq!(
            client.apply_message_revision_event(session_id, 1, correction),
            Ok(false)
        );
        client
            .replace_message_revision_snapshot(
                session_id,
                1,
                &MessageRevisionSnapshot {
                    target_event_ids: vec![1],
                    entries: vec![MessageRevisionSnapshotEntry {
                        target_event_id: 1,
                        latest_revision_event_id: 3,
                        action: MessageRevisionAction::Tombstone,
                        actor_user_id: 8,
                        at_unix: 12,
                        replacement: None,
                        revision_number: 2,
                    }],
                },
            )
            .expect("authoritative tombstone");
        assert_eq!(
            client
                .message_revision_for_target(session_id, 1, 1)
                .expect("revision")
                .action,
            MessageRevisionAction::Tombstone
        );
        assert!(client.message_revision_snapshot_complete(session_id, 1, 1));
        assert!(client
            .replace_message_revision_snapshot(
                session_id,
                1,
                &MessageRevisionSnapshot {
                    target_event_ids: vec![99],
                    entries: Vec::new(),
                },
            )
            .is_err());
        assert!(!client.message_revision_snapshot_complete(session_id, 1, 1));
        assert_eq!(
            client
                .message_revision_for_target(session_id, 1, 1)
                .expect("prior revision retained")
                .action,
            MessageRevisionAction::Tombstone
        );
        assert!(matches!(
            client.session(session_id).expect("session").events[0].kind,
            ChatEventKind::Message { .. }
        ));

        client
            .persist_session(&mut store, session_id)
            .expect("persist message revision");
        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore message revision"),
            1
        );
        let restored_session = restored.sessions()[0].session_id;
        assert_eq!(
            restored
                .message_revision_for_target(restored_session, 1, 1)
                .expect("restored revision")
                .action,
            MessageRevisionAction::Tombstone
        );
        assert!(!restored.message_revision_snapshot_complete(restored_session, 1, 1));
        restored.remove_session(restored_session);
        assert!(restored
            .message_revision_for_target(restored_session, 1, 1)
            .is_none());
    }

    #[test]
    fn client_pins_are_bounded_authoritative_and_restart_stale() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(
            session_id,
            vec![bounded_history_event(1, 5), bounded_history_event(2, 5)]
        )));
        let pin = PinEvent {
            pin_event_id: 10,
            target_event_id: 1,
            action: PinAction::Pin,
            actor_user_id: 7,
            at_unix: 10,
        };
        assert_eq!(client.apply_pin_event(session_id, 1, pin), Ok(true));
        assert!(client.pin_target_authoritative(session_id, 1, 1));
        assert_eq!(
            client
                .pin_for_target(session_id, 1, 1)
                .expect("retained pin")
                .pin_event_id,
            10
        );
        assert_eq!(client.apply_pin_event(session_id, 1, pin), Ok(false));
        assert!(client
            .apply_pin_event(
                session_id,
                1,
                PinEvent {
                    actor_user_id: 8,
                    ..pin
                },
            )
            .is_err());
        assert_eq!(
            client.apply_pin_event(
                session_id,
                1,
                PinEvent {
                    pin_event_id: 11,
                    actor_user_id: 8,
                    at_unix: 11,
                    ..pin
                },
            ),
            Ok(true)
        );

        client.mark_pins_stale(session_id);
        assert!(!client.pin_target_authoritative(session_id, 1, 1));
        assert!(client.pin_for_target(session_id, 1, 1).is_some());
        client
            .replace_pin_snapshot(
                session_id,
                1,
                &PinSnapshot {
                    target_event_ids: vec![1, 2],
                    entries: vec![PinSnapshotEntry {
                        target_event_id: 2,
                        pin_event_id: 12,
                        actor_user_id: 8,
                        pinned_at_unix: 12,
                    }],
                },
            )
            .expect("authoritative pin snapshot");
        assert!(client.pin_for_target(session_id, 1, 1).is_none());
        assert!(client.pin_for_target(session_id, 1, 2).is_some());
        assert!(client.pin_target_authoritative(session_id, 1, 1));
        assert!(client.pin_target_authoritative(session_id, 1, 2));
        client.mark_pin_room_stale(session_id, 1);
        assert!(!client.pin_target_authoritative(session_id, 1, 1));
        assert!(!client.pin_target_authoritative(session_id, 1, 2));
        assert!(client.pin_for_target(session_id, 1, 2).is_some());

        assert!(client
            .replace_pin_snapshot(
                session_id,
                1,
                &PinSnapshot {
                    target_event_ids: vec![99],
                    entries: Vec::new(),
                },
            )
            .is_err());
        assert!(!client.pin_target_authoritative(session_id, 1, 2));
        assert!(client.pin_for_target(session_id, 1, 2).is_some());

        client
            .persist_session(&mut store, session_id)
            .expect("persist pins");
        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore pins"),
            1
        );
        let restored_session = restored.sessions()[0].session_id;
        assert!(restored.pin_for_target(restored_session, 1, 2).is_some());
        assert!(!restored.pin_target_authoritative(restored_session, 1, 2));
    }

    #[test]
    fn message_revision_projection_follows_retained_session_history() {
        let mut client = ChatClient::new();
        let first_session = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(
            first_session,
            vec![bounded_history_event(1, 5)]
        )));
        assert_eq!(
            client.apply_message_revision_event(
                first_session,
                1,
                MessageRevisionEvent {
                    revision_event_id: 2,
                    target_event_id: 1,
                    action: MessageRevisionAction::Correct,
                    actor_user_id: 7,
                    at_unix: 2,
                    replacement: Some("corrected".into()),
                    revision_number: 1,
                    actor_display_name: None,
                },
            ),
            Ok(true)
        );
        let second_session = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(
            second_session,
            vec![bounded_history_event(2, 5)]
        )));

        client.remove_session(first_session).expect("remove first");

        assert!(client
            .message_revision_for_target(second_session, 1, 1)
            .is_none());
        assert!(client.message_revisions.is_empty());
    }

    #[test]
    fn reaction_snapshot_evidence_and_rows_follow_retained_history() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(
            session_id,
            vec![bounded_history_event(1, 5)]
        )));
        client
            .replace_reaction_snapshot(
                session_id,
                1,
                &ReactionSnapshot {
                    target_event_ids: vec![1],
                    entries: vec![ReactionSnapshotEntry {
                        target_event_id: 1,
                        actor_user_id: 7,
                        token: ReactionToken::Heart,
                        created_at_unix: 1,
                    }],
                },
            )
            .expect("first snapshot");
        assert_eq!(
            client.authoritative_reaction_targets(session_id, 1),
            BTreeSet::from([1])
        );

        client.session_mut(session_id).expect("session").events = vec![bounded_history_event(2, 5)];
        client
            .replace_reaction_snapshot(
                session_id,
                1,
                &ReactionSnapshot {
                    target_event_ids: vec![2],
                    entries: Vec::new(),
                },
            )
            .expect("replacement target snapshot");
        assert_eq!(
            client.authoritative_reaction_targets(session_id, 1),
            BTreeSet::from([2])
        );
        assert!(client.reactions_for_targets(session_id, 1, &[1]).is_empty());
    }

    #[test]
    fn mute_except_mentions_requires_exact_bound_numeric_mention_for_unread() {
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        assert!(client.push_session(bounded_history_session(session_id, Vec::new())));
        assert!(client.bind_local_user_id(session_id, 7));
        assert!(client.set_room_mute_except_mentions(session_id, 1, true));

        let event = |mentioned_user_ids| ChatEvent {
            server_id: "server-a".into(),
            room_id: 1,
            event_id: 1,
            actor_user_id: Some(2),
            actor_display_name: Some("Peer".into()),
            at_unix: 1,
            kind: ChatEventKind::RichMessage {
                body: "@tester".into(),
                metadata: super::super::model::ChatMessageMetadata {
                    reply_to_event_id: None,
                    mentioned_user_ids,
                },
            },
        };
        assert!(!client.event_allows_unread(session_id, &event(Vec::new())));
        assert!(!client.event_allows_unread(session_id, &event(vec![6, 8])));
        assert!(client.event_allows_unread(session_id, &event(vec![7])));

        assert!(client.set_room_mute_except_mentions(session_id, 1, false));
        assert!(client.event_allows_unread(session_id, &event(Vec::new())));
    }

    #[test]
    fn client_persistence_skips_transient_local_echo_events() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: "server-a".into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: Vec::new(),
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: Some(1),
                    actor_display_name: Some("Alice".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "stored".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: u64::MAX - 1,
                    actor_user_id: Some(1),
                    actor_display_name: Some("Alice".into()),
                    at_unix: 2,
                    kind: ChatEventKind::Message {
                        body: "pending local echo".into(),
                    },
                },
            ],
            status: "test".into(),
        });

        client
            .persist_session(&mut store, session_id)
            .expect("persist session");

        let events = store
            .latest_events(&"server-a".into(), 1, 50)
            .expect("stored events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, 1);
        assert_eq!(
            events[0].kind,
            ChatEventKind::Message {
                body: "stored".into()
            }
        );
    }

    #[test]
    fn client_restore_ignores_unrestorable_dev_servers() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        for (server_id, destination, display_name) in [
            ("mock-server", "mockchatdestination", "Mock OMENchat"),
            ("pending-server", "pending-omenchat-1", "New Chat"),
            ("real-server", TEST_DESTINATION, "Real OMENchat"),
        ] {
            store
                .save_server(ChatServerSummary {
                    server_id: server_id.into(),
                    destination: destination.into(),
                    display_name: display_name.into(),
                })
                .expect("save server");
            store
                .save_room(ChatRoomSummary {
                    server_id: server_id.into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                })
                .expect("save room");
        }

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.server.display_name, "Real OMENchat");
        assert_eq!(session.server.destination, TEST_DESTINATION);
    }

    #[test]
    fn client_persistence_preserves_parted_active_room_state() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: TEST_DESTINATION.into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: false,
            },
            rooms: vec![ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            }],
            users: vec![ChatUserSummary {
                server_id: "server-a".into(),
                user_id: 1,
                display_name: "Alice".into(),
                role_bits: 0,
                status_bits: 0,
                lxmf_available: true,
            }],
            events: Vec::new(),
            status: "left #lobby".into(),
        });

        client
            .persist_session(&mut store, session_id)
            .expect("persist parted session");

        let rooms = store
            .rooms_for_server(&"server-a".into())
            .expect("stored rooms");
        assert_eq!(rooms.len(), 1);
        assert!(!rooms[0].joined);
        assert!(store
            .users_for_room(&"server-a".into(), 1)
            .expect("stored userlist")
            .is_empty());
    }

    #[test]
    fn client_persistence_preserves_joined_rooms_and_active_room() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server: ChatServerSummary {
                server_id: "server-a".into(),
                destination: TEST_DESTINATION.into(),
                display_name: "Server A".into(),
            },
            active_room: ChatRoomSummary {
                server_id: "server-a".into(),
                room_id: 2,
                name: "support".into(),
                topic: None,
                unread: 0,
                joined: true,
            },
            rooms: vec![
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 1,
                    name: "lobby".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                },
                ChatRoomSummary {
                    server_id: "server-a".into(),
                    room_id: 2,
                    name: "support".into(),
                    topic: None,
                    unread: 0,
                    joined: true,
                },
            ],
            users: Vec::new(),
            events: Vec::new(),
            status: "joined support".into(),
        });

        client
            .persist_session(&mut store, session_id)
            .expect("persist switched session");

        let rooms = store
            .rooms_for_server(&"server-a".into())
            .expect("stored rooms");
        assert_eq!(
            store
                .active_room_id(&"server-a".into())
                .expect("active room"),
            Some(2)
        );
        assert_eq!(
            rooms
                .iter()
                .find(|room| room.name == "lobby")
                .map(|room| room.joined),
            Some(true)
        );
        assert_eq!(
            rooms
                .iter()
                .find(|room| room.name == "support")
                .map(|room| room.joined),
            Some(true)
        );
    }

    #[test]
    fn client_restore_prefers_persisted_active_room_over_joined_order() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        store.save_server(server.clone()).expect("server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("lobby");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 2,
                name: "help".into(),
                topic: None,
                unread: 0,
                joined: true,
            })
            .expect("help");
        store
            .set_active_room(&server.server_id, 2)
            .expect("active room");

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.active_room.name, "help");
    }

    #[test]
    fn client_restore_prefers_room_with_cached_events_when_join_marker_is_missing() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        store.save_server(server.clone()).expect("server");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 1,
                name: "lobby".into(),
                topic: None,
                unread: 0,
                joined: false,
            })
            .expect("lobby");
        store
            .save_room(ChatRoomSummary {
                server_id: server.server_id.clone(),
                room_id: 2,
                name: "support".into(),
                topic: None,
                unread: 0,
                joined: false,
            })
            .expect("support");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: 2,
                event_id: 7,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "cached support event".into(),
                },
            }])
            .expect("events");

        let mut restored = ChatClient::new();
        assert_eq!(
            restored
                .restore_from_store(&store, 50)
                .expect("restore sessions"),
            1
        );
        let session = restored.sessions().first().expect("restored session");
        assert_eq!(session.active_room.name, "support");
        assert_eq!(session.events.len(), 1);
    }

    #[test]
    fn client_loads_older_history_from_cached_store() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(room.clone()).expect("room");
        store
            .append_events(vec![
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: room.room_id,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "older one".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: room.room_id,
                    event_id: 2,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "older two".into(),
                    },
                },
            ])
            .expect("events");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 3,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "newest".into(),
                },
            }],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_history_before(&store, session_id, 50)
                .expect("load cached history"),
            2
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(session.status, "loaded 2 older cached event(s)");
    }

    #[test]
    fn client_loads_active_room_history_from_cached_store_after_room_switch() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(room.clone()).expect("room");
        store
            .append_events(vec![ChatEvent {
                server_id: server.server_id.clone(),
                room_id: room.room_id,
                event_id: 9,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "cached help event".into(),
                },
            }])
            .expect("events");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: Vec::new(),
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_room_history(&store, session_id, 50)
                .expect("load active room history"),
            1
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].event_id, 9);
    }

    #[test]
    fn client_cached_room_history_merge_does_not_drop_live_room_events() {
        let store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let room = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![room.clone()],
            active_room: room,
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 1,
                event_id: 42,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 42,
                kind: ChatEventKind::Message {
                    body: "live history row".into(),
                },
            }],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_room_history(&store, session_id, 50)
                .expect("load active room history"),
            0
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(session.events.len(), 1);
        assert_eq!(session.events[0].event_id, 42);
    }

    #[test]
    fn client_treats_cached_event_ids_as_room_scoped() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let lobby = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        let help = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 0,
            joined: false,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(lobby.clone()).expect("lobby");
        store.save_room(help.clone()).expect("help");
        store
            .append_events(vec![
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "lobby one".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: None,
                    at_unix: 0,
                    kind: ChatEventKind::Message {
                        body: "help one".into(),
                    },
                },
            ])
            .expect("events");
        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![lobby.clone(), help.clone()],
            active_room: lobby,
            users: Vec::new(),
            events: vec![ChatEvent {
                server_id: "server-a".into(),
                room_id: 2,
                event_id: 1,
                actor_user_id: None,
                actor_display_name: None,
                at_unix: 0,
                kind: ChatEventKind::Message {
                    body: "stale help row".into(),
                },
            }],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_room_history(&store, session_id, 50)
                .expect("load lobby history"),
            1
        );

        let session = client.session(session_id).expect("session");
        let mut event_keys = session
            .events
            .iter()
            .map(|event| (event.room_id, event.event_id))
            .collect::<Vec<_>>();
        event_keys.sort_unstable();
        assert_eq!(event_keys, vec![(1, 1), (2, 1)]);
    }

    #[test]
    fn client_load_older_history_uses_active_room_event_floor() {
        let mut store = SqliteChatStore::in_memory().expect("store");
        let server = ChatServerSummary {
            server_id: "server-a".into(),
            destination: TEST_DESTINATION.into(),
            display_name: "Server A".into(),
        };
        let lobby = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 1,
            name: "lobby".into(),
            topic: None,
            unread: 0,
            joined: false,
        };
        let help = ChatRoomSummary {
            server_id: server.server_id.clone(),
            room_id: 2,
            name: "help".into(),
            topic: None,
            unread: 0,
            joined: true,
        };
        store.save_server(server.clone()).expect("server");
        store.save_room(lobby.clone()).expect("lobby");
        store.save_room(help.clone()).expect("help");
        store
            .append_events(vec![
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 8,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 8,
                    kind: ChatEventKind::Message {
                        body: "older help".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 9,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 9,
                    kind: ChatEventKind::Message {
                        body: "also older help".into(),
                    },
                },
                ChatEvent {
                    server_id: server.server_id.clone(),
                    room_id: 2,
                    event_id: 10,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 10,
                    kind: ChatEventKind::Message {
                        body: "loaded help".into(),
                    },
                },
            ])
            .expect("events");

        let mut client = ChatClient::new();
        let session_id = client.reserve_session_id();
        client.push_session(ChatSessionView {
            session_id,
            server,
            rooms: vec![lobby.clone(), help.clone()],
            active_room: help,
            users: Vec::new(),
            events: vec![
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 1,
                    event_id: 1,
                    actor_user_id: None,
                    actor_display_name: Some("Bob".into()),
                    at_unix: 1,
                    kind: ChatEventKind::Message {
                        body: "old lobby should not set help floor".into(),
                    },
                },
                ChatEvent {
                    server_id: "server-a".into(),
                    room_id: 2,
                    event_id: 10,
                    actor_user_id: None,
                    actor_display_name: Some("Alice".into()),
                    at_unix: 10,
                    kind: ChatEventKind::Message {
                        body: "loaded help".into(),
                    },
                },
            ],
            status: String::new(),
        });

        assert_eq!(
            client
                .load_cached_history_before(&store, session_id, 50)
                .expect("load older active room history"),
            2
        );
        let session = client.session(session_id).expect("session");
        assert_eq!(
            session
                .events
                .iter()
                .filter(|event| event.room_id == 2)
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![8, 9, 10]
        );
    }
}
