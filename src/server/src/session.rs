use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{ServerError, ServerResult};
use crate::protocol::batch::{
    compressed_values_body, compressed_values_payload, encoded_compressed_len, resource_offer_body,
    ResourceOffer,
};
use crate::protocol::codec::{decode_frame, encode_frame};
use crate::protocol::{
    canonical_mutation_request_hash, parse_session_open_negotiation, ChatErrorCode, ChatOp,
    ClientInstanceId, Compression, DurableMutationEnvelope, Frame, FrameBody, FrameValue, RoomId,
    PROTOCOL_NAME,
};
use crate::store::durable_replay::{
    DurableMutationKey, DurableRoomEventCommit, DurableRoomEventPlan,
};
use crate::store::{OmenchatStore, ServerRoom, ServerRoomEvent, ServerRoomEventKind, ServerUser};
use crate::upload::{
    plan_upload_with_index, store_upload_with_policy_indexed_and_commit, UploadPolicy,
    UploadQuotaDecision,
};

const STATUS_BANNED: u32 = 1;
const STATUS_MUTED: u32 = 1 << 1;
const ROLE_TRUSTED: u64 = 1;
const ROLE_MODERATOR: u64 = 1 << 1;
const ROLE_ADMIN: u64 = 1 << 2;
const LINK_INLINE_HISTORY_TARGET_BYTES: usize = 384;
const UPLOAD_INLINE_CHUNK_BYTES: usize = 256;
const UPLOAD_INLINE_MAX_BYTES: usize = 16 * 1024;
const PENDING_RESOURCE_MAX_ITEMS: usize = 64;
const PENDING_RESOURCE_MAX_BYTES: usize = 16 * 1024 * 1024;
const PENDING_RESOURCE_MAX_ENTRY_BYTES: usize = 4 * 1024 * 1024;
const PENDING_UPLOAD_MAX_ITEMS: usize = 256;
const PENDING_UPLOAD_MAX_ITEMS_PER_IDENTITY: usize = 8;
const PENDING_UPLOAD_TTL_SECONDS: u64 = 6 * 60 * 60;
const UPLOAD_FILENAME_MAX_BYTES: usize = 255;
const UPLOAD_CONTENT_TYPE_MAX_BYTES: usize = 255;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerPeer {
    pub identity_hash: Vec<u8>,
    pub display_name: String,
    pub lxmf_destination: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLimits {
    pub history_batch_size: usize,
    pub join_backlog_events: usize,
    pub large_batch_threshold_bytes: usize,
    pub max_message_bytes: usize,
    pub rate_messages_per_minute: usize,
    pub rate_commands_per_minute: usize,
    pub upload_quota_bytes: u64,
    pub upload_max_file_bytes: u64,
    pub upload_cache_root: Option<PathBuf>,
    pub ping_interval_seconds: u64,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            history_batch_size: 50,
            join_backlog_events: 50,
            large_batch_threshold_bytes: 4096,
            max_message_bytes: 2048,
            rate_messages_per_minute: 20,
            rate_commands_per_minute: 12,
            upload_quota_bytes: 50 * 1024 * 1024,
            upload_max_file_bytes: 512 * 1024,
            upload_cache_root: None,
            ping_interval_seconds: 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RateKind {
    Message,
    Command,
}

#[derive(Clone, Debug, Default)]
struct RateBucket {
    window_start: u64,
    count: usize,
}

type RateKey = (Vec<u8>, RateKind);
type RateBuckets = Arc<Mutex<BTreeMap<RateKey, RateBucket>>>;

struct RateReservation {
    buckets: RateBuckets,
    key: RateKey,
    window_start: u64,
    active: bool,
}

impl RateReservation {
    fn commit(mut self) {
        self.active = false;
    }
}

impl Drop for RateReservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Ok(mut buckets) = self.buckets.lock() else {
            return;
        };
        let Some(bucket) = buckets.get_mut(&self.key) else {
            return;
        };
        if bucket.window_start == self.window_start {
            bucket.count = bucket.count.saturating_sub(1);
        }
    }
}

enum RateAdmission {
    Admitted(Option<RateReservation>),
    Rejected,
}

#[derive(Debug, Default)]
struct PendingResourceStore {
    entries: BTreeMap<String, Vec<u8>>,
    retained_bytes: usize,
    rejected: u64,
}

impl PendingResourceStore {
    fn insert(&mut self, resource_id: String, payload: Vec<u8>) -> ServerResult<()> {
        let payload_bytes = payload.len();
        let existing = self.entries.get(&resource_id);
        let replaced_bytes = existing.map(Vec::len).unwrap_or_default();
        let projected_items = self.entries.len() + usize::from(existing.is_none());
        let projected_bytes = self
            .retained_bytes
            .saturating_sub(replaced_bytes)
            .saturating_add(payload_bytes);
        if payload_bytes > PENDING_RESOURCE_MAX_ENTRY_BYTES
            || projected_items > PENDING_RESOURCE_MAX_ITEMS
            || projected_bytes > PENDING_RESOURCE_MAX_BYTES
        {
            self.rejected = self.rejected.saturating_add(1);
            return Err(ServerError::Message(format!(
                "pending resource admission rejected: items={projected_items}/{PENDING_RESOURCE_MAX_ITEMS} bytes={projected_bytes}/{PENDING_RESOURCE_MAX_BYTES} entry_bytes={payload_bytes}/{PENDING_RESOURCE_MAX_ENTRY_BYTES}"
            )));
        }
        self.entries.insert(resource_id, payload);
        self.retained_bytes = projected_bytes;
        Ok(())
    }

    fn get(&self, resource_id: &str) -> Option<Vec<u8>> {
        self.entries.get(resource_id).cloned()
    }

    fn take(&mut self, resource_id: &str) -> Option<Vec<u8>> {
        let payload = self.entries.remove(resource_id)?;
        self.retained_bytes = self.retained_bytes.saturating_sub(payload.len());
        Some(payload)
    }
}

pub struct SessionEngine {
    store: OmenchatStore,
    limits: SessionLimits,
    server_motd: Option<String>,
    pending_resources: Arc<Mutex<PendingResourceStore>>,
    pending_uploads: Arc<Mutex<PendingUploadStore>>,
    rate_buckets: RateBuckets,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DurableRoomTextDispatch {
    pub origin: Frame,
    pub broadcast: Option<Frame>,
    pub pruned: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingUpload {
    identity_hash: Vec<u8>,
    room_id: RoomId,
    user_id: u32,
    filename: String,
    content_type: Option<String>,
    incoming_bytes: u64,
    accepted_at: u64,
}

#[derive(Debug, Default)]
struct PendingUploadStore {
    entries: BTreeMap<String, PendingUpload>,
    rejected: u64,
    expired: u64,
}

enum PendingUploadTake {
    Found(PendingUpload),
    NotFound,
    IdentityMismatch,
}

impl PendingUploadStore {
    fn insert(&mut self, resource_id: String, upload: PendingUpload, now: u64) -> bool {
        self.purge_expired(now);
        if let Some(existing) = self.entries.get(&resource_id) {
            if existing.identity_hash != upload.identity_hash {
                self.rejected = self.rejected.saturating_add(1);
                return false;
            }
            self.entries.insert(resource_id, upload);
            return true;
        }
        let identity_items = self
            .entries
            .values()
            .filter(|entry| entry.identity_hash == upload.identity_hash)
            .count();
        if self.entries.len() >= PENDING_UPLOAD_MAX_ITEMS
            || identity_items >= PENDING_UPLOAD_MAX_ITEMS_PER_IDENTITY
        {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.entries.insert(resource_id, upload);
        true
    }

    fn take_for_identity(
        &mut self,
        resource_id: &str,
        identity_hash: &[u8],
        now: u64,
    ) -> PendingUploadTake {
        self.purge_expired(now);
        let Some(upload) = self.entries.get(resource_id) else {
            return PendingUploadTake::NotFound;
        };
        if upload.identity_hash.as_slice() != identity_hash {
            return PendingUploadTake::IdentityMismatch;
        }
        match self.entries.remove(resource_id) {
            Some(upload) => PendingUploadTake::Found(upload),
            None => PendingUploadTake::NotFound,
        }
    }

    fn remove_identity(&mut self, identity_hash: &[u8], now: u64) -> usize {
        self.purge_expired(now);
        let before = self.entries.len();
        self.entries
            .retain(|_, upload| upload.identity_hash.as_slice() != identity_hash);
        before.saturating_sub(self.entries.len())
    }

    fn metrics(&mut self, now: u64) -> (usize, usize, u64, u64) {
        self.purge_expired(now);
        let mut identities = BTreeMap::new();
        for upload in self.entries.values() {
            identities.insert(upload.identity_hash.as_slice(), ());
        }
        (
            self.entries.len(),
            identities.len(),
            self.rejected,
            self.expired,
        )
    }

    fn purge_expired(&mut self, now: u64) {
        let before = self.entries.len();
        self.entries.retain(|_, upload| {
            now.saturating_sub(upload.accepted_at) < PENDING_UPLOAD_TTL_SECONDS
        });
        self.expired = self
            .expired
            .saturating_add(before.saturating_sub(self.entries.len()) as u64);
    }
}

impl SessionEngine {
    pub fn new(store: OmenchatStore) -> Self {
        Self {
            store,
            limits: SessionLimits::default(),
            server_motd: Some("Welcome to OMENchat".into()),
            pending_resources: Arc::new(Mutex::new(PendingResourceStore::default())),
            pending_uploads: Arc::new(Mutex::new(PendingUploadStore::default())),
            rate_buckets: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_limits(store: OmenchatStore, limits: SessionLimits) -> Self {
        Self {
            store,
            limits,
            server_motd: Some("Welcome to OMENchat".into()),
            pending_resources: Arc::new(Mutex::new(PendingResourceStore::default())),
            pending_uploads: Arc::new(Mutex::new(PendingUploadStore::default())),
            rate_buckets: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn with_limits_and_motd(
        store: OmenchatStore,
        limits: SessionLimits,
        motd: Option<String>,
    ) -> Self {
        Self {
            store,
            limits,
            server_motd: motd.and_then(|motd| {
                let motd = motd.trim().to_owned();
                (!motd.is_empty()).then_some(motd)
            }),
            pending_resources: Arc::new(Mutex::new(PendingResourceStore::default())),
            pending_uploads: Arc::new(Mutex::new(PendingUploadStore::default())),
            rate_buckets: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn handle_frame(&self, peer: &ServerPeer, frame: Frame) -> ServerResult<Vec<Frame>> {
        self.handle_frame_with_active_peers(peer, frame, &[])
    }

    pub fn handle_frame_with_active_peers(
        &self,
        peer: &ServerPeer,
        frame: Frame,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<Frame>> {
        match frame.op {
            ChatOp::SessionOpen => self.handle_session_open(peer, frame.seq, frame.body),
            ChatOp::JoinRoom => {
                self.handle_join_room(peer, frame.seq, frame.body, active_room_peers)
            }
            ChatOp::PartRoom => self.handle_part_room(peer, frame.seq, frame.room_id),
            ChatOp::RoomMessage => {
                self.handle_room_text(peer, frame.seq, frame.room_id, frame.body, |body| {
                    ServerRoomEventKind::Message { body }
                })
            }
            ChatOp::RoomAction => {
                self.handle_room_text(peer, frame.seq, frame.room_id, frame.body, |body| {
                    ServerRoomEventKind::Action { body }
                })
            }
            ChatOp::RoomNotice => {
                self.handle_room_notice(peer, frame.seq, frame.room_id, frame.body)
            }
            ChatOp::HistoryBefore => {
                self.handle_history_before(peer, frame.seq, frame.room_id, frame.body)
            }
            ChatOp::HistoryRecent => {
                self.handle_history_recent(peer, frame.seq, frame.room_id, frame.body)
            }
            ChatOp::UploadOffer => {
                self.handle_upload_offer(peer, frame.seq, frame.room_id, frame.body)
            }
            ChatOp::UploadFetch => {
                self.handle_upload_fetch(peer, frame.seq, frame.room_id, frame.body)
            }
            ChatOp::Command => self.handle_command(
                peer,
                frame.seq,
                frame.room_id,
                frame.body,
                active_room_peers,
            ),
            ChatOp::Ping => Ok(vec![Frame::new(
                ChatOp::Pong,
                frame.seq,
                frame.room_id,
                FrameBody::Empty,
            )]),
            _ => Ok(vec![self.error_frame(
                frame.seq,
                frame.room_id,
                ChatErrorCode::MalformedFrame,
                "unsupported server op",
            )]),
        }
    }

    pub fn moderation_disconnect_target_for_frame(
        &self,
        peer: &ServerPeer,
        frame: &Frame,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Option<Vec<u8>>> {
        if frame.op != ChatOp::Command {
            return Ok(None);
        }
        let Some(command) = body_string(&frame.body) else {
            return Ok(None);
        };
        let command = command.trim();
        let (command_name, target) = command
            .split_once(char::is_whitespace)
            .unwrap_or((command, ""));
        if !matches!(command_name.to_ascii_lowercase().as_str(), "kick" | "ban") {
            return Ok(None);
        }
        let Some(actor) = self.ensure_allowed_peer(peer, frame.seq, frame.room_id)? else {
            return Ok(None);
        };
        if actor.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
            return Ok(None);
        }
        let Some(target_peer) = resolve_active_peer_target(active_room_peers, target) else {
            return Ok(None);
        };
        if target_peer.identity_hash == peer.identity_hash {
            return Ok(None);
        }
        let target_user = self.ensure_peer(&target_peer)?;
        if target_user.role_bits & ROLE_ADMIN != 0 && actor.role_bits & ROLE_ADMIN == 0 {
            return Ok(None);
        }
        Ok(Some(target_peer.identity_hash))
    }

    pub fn active_userlist_frame(
        &self,
        room_id: RoomId,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Frame> {
        let users = self.user_values_for_active_peers(room_id, active_room_peers)?;
        Ok(Frame::new(
            self.batch_op(
                ChatOp::UserListSnapshotInline,
                ChatOp::UserListSnapshotResource,
                &users,
            )?,
            0,
            Some(room_id),
            self.batch_body(room_id, "userlist", &users)?,
        ))
    }

    fn handle_session_open(
        &self,
        peer: &ServerPeer,
        seq: u32,
        body: FrameBody,
    ) -> ServerResult<Vec<Frame>> {
        if let Some(error) = self.reject_if_banned(peer, seq, None)? {
            return Ok(vec![error]);
        }
        if parse_session_open_negotiation(&body).is_err() {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "invalid session capability negotiation",
            )]);
        }
        let rooms = self
            .store
            .list_rooms()?
            .into_iter()
            .map(|room| room_to_value(&room))
            .collect::<Vec<_>>();
        Ok(vec![Frame::new(
            ChatOp::SessionAccept,
            seq,
            None,
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::Array(rooms),
                self.server_motd
                    .clone()
                    .map(FrameValue::String)
                    .unwrap_or(FrameValue::Nil),
                FrameValue::U64(self.limits.upload_quota_bytes),
                FrameValue::U64(self.limits.ping_interval_seconds.clamp(5, 600)),
                FrameValue::U64(self.limits.upload_max_file_bytes),
            ]),
        )])
    }

    fn handle_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<Frame>> {
        if let Some(error) = self.reject_if_banned(peer, seq, room_id)? {
            return Ok(vec![error]);
        }
        if let Some(error) = self.reject_if_rate_limited(peer, seq, room_id, RateKind::Command)? {
            return Ok(vec![error]);
        }
        let Some(command) = body_string(&body) else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::MalformedFrame,
                "command frame did not include a command",
            )]);
        };
        let command = command.trim();
        let (command_name, command_rest) = command
            .split_once(char::is_whitespace)
            .unwrap_or((command, ""));
        match command_name.to_ascii_lowercase().as_str() {
            "rooms" => {
                let rooms = self
                    .store
                    .list_rooms()?
                    .into_iter()
                    .map(|room| room_to_value(&room))
                    .collect::<Vec<_>>();
                Ok(vec![Frame::new(
                    ChatOp::CommandResult,
                    seq,
                    room_id,
                    FrameBody::Fields(vec![
                        FrameValue::String("rooms".into()),
                        FrameValue::Array(rooms),
                    ]),
                )])
            }
            "topic" => self.handle_topic_command(peer, seq, room_id, command_rest.trim()),
            "create" => self.handle_create_room_command(peer, seq, command_rest.trim()),
            "kick" | "ban" | "mute" | "unmute" => self.handle_moderation_command(
                peer,
                seq,
                room_id,
                command_name,
                command_rest.trim(),
                active_room_peers,
            ),
            "role" => self.handle_role_command(peer, seq, room_id, command_rest.trim()),
            "unban" => self.handle_unban_command(peer, seq, room_id, command_rest.trim()),
            _ => Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::MalformedFrame,
                "unsupported command",
            )]),
        }
    }

    fn handle_topic_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        topic: &str,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::MalformedFrame,
                "topic command requires an active room",
            )]);
        };
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "topic changes require moderator or admin role",
            )]);
        }
        let topic = topic.trim();
        let room = self
            .store
            .update_room_topic(room_id, (!topic.is_empty()).then_some(topic))?;
        Ok(vec![
            Frame::new(
                ChatOp::CommandResult,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![
                    FrameValue::String("topic".into()),
                    room_to_value(&room),
                ]),
            ),
            Frame::new(
                ChatOp::RoomDelta,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![room_to_value(&room)]),
            ),
        ])
    }

    fn handle_create_room_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        command_rest: &str,
    ) -> ServerResult<Vec<Frame>> {
        let Some(user) = self.ensure_allowed_peer(peer, seq, None)? else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if user.role_bits & ROLE_ADMIN == 0 {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::PermissionDenied,
                "room creation requires admin role",
            )]);
        }
        let (name, topic) = command_rest
            .split_once(char::is_whitespace)
            .unwrap_or((command_rest, ""));
        if name.trim().is_empty() {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::MalformedFrame,
                "create command requires a room name",
            )]);
        }
        let topic = topic.trim();
        let room = self
            .store
            .create_room(name, (!topic.is_empty()).then_some(topic))?;
        Ok(vec![
            Frame::new(
                ChatOp::CommandResult,
                seq,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("create".into()),
                    room_to_value(&room),
                ]),
            ),
            Frame::new(
                ChatOp::RoomDelta,
                seq,
                None,
                FrameBody::Fields(vec![room_to_value(&room)]),
            ),
        ])
    }

    fn handle_moderation_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        action: &str,
        target: &str,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<Frame>> {
        let Some(active_room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::MalformedFrame,
                "moderation command requires an active room",
            )]);
        };
        let Some(actor) = self.ensure_allowed_peer(peer, seq, room_id)? else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if actor.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "moderation command requires moderator or admin role",
            )]);
        }
        let Some(target_peer) = resolve_active_peer_target(active_room_peers, target) else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::UserNotFound,
                "target user is not active in this room",
            )]);
        };
        if target_peer.identity_hash == peer.identity_hash {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "cannot moderate your own active session",
            )]);
        }
        let target_user = self.ensure_peer(&target_peer)?;
        if target_user.role_bits & ROLE_ADMIN != 0 && actor.role_bits & ROLE_ADMIN == 0 {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "moderators cannot moderate admins",
            )]);
        }
        let command = action.to_ascii_lowercase();
        let user = match command.as_str() {
            "ban" => self
                .store
                .set_user_status_flag(target_user.user_id, STATUS_BANNED, true)?,
            "mute" => self
                .store
                .set_user_status_flag(target_user.user_id, STATUS_MUTED, true)?,
            "unmute" => {
                self.store
                    .set_user_status_flag(target_user.user_id, STATUS_MUTED, false)?
            }
            _ => target_user,
        };
        let event = self.store.append_event(
            active_room_id,
            Some(actor.user_id),
            ServerRoomEventKind::System {
                body: format!(
                    "{} {} {}",
                    actor.display_name,
                    moderation_past_tense(&command),
                    user.display_name
                ),
            },
        )?;
        Ok(vec![
            Frame::new(
                ChatOp::CommandResult,
                seq,
                room_id,
                FrameBody::Fields(vec![FrameValue::String(command), user_to_value(&user)]),
            ),
            user_delta_frame(seq, room_id, &user),
            Frame::new(
                ChatOp::RoomEvent,
                seq,
                room_id,
                FrameBody::Fields(vec![event_to_value(&event)]),
            ),
        ])
    }

    fn handle_unban_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        target: &str,
    ) -> ServerResult<Vec<Frame>> {
        let Some(actor) = self.ensure_allowed_peer(peer, seq, room_id)? else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if actor.role_bits & ROLE_ADMIN == 0 {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "unban requires admin role",
            )]);
        }
        let Some(target_user) = resolve_known_user_target(&self.store.users()?, target) else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::UserNotFound,
                "target user is unknown",
            )]);
        };
        if target_user.identity_hash == peer.identity_hash {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "cannot unban your own active session",
            )]);
        }
        let user = self
            .store
            .set_user_status_flag(target_user.user_id, STATUS_BANNED, false)?;
        let mut frames = vec![Frame::new(
            ChatOp::CommandResult,
            seq,
            room_id,
            FrameBody::Fields(vec![
                FrameValue::String("unban".into()),
                user_to_value(&user),
            ]),
        )];
        frames.push(user_delta_frame(seq, room_id, &user));
        if let Some(room_id) = room_id {
            let event = self.store.append_event(
                room_id,
                Some(actor.user_id),
                ServerRoomEventKind::System {
                    body: format!("{} unbanned {}", actor.display_name, user.display_name),
                },
            )?;
            frames.push(Frame::new(
                ChatOp::RoomEvent,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![event_to_value(&event)]),
            ));
        }
        Ok(frames)
    }

    fn handle_role_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        command_rest: &str,
    ) -> ServerResult<Vec<Frame>> {
        let Some(actor) = self.ensure_allowed_peer(peer, seq, room_id)? else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if actor.role_bits & ROLE_ADMIN == 0 {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "role changes require admin role",
            )]);
        }
        let (target, role_label) = command_rest
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((command_rest.trim(), ""));
        let Some(role_bits) = role_bits_from_label(role_label) else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::MalformedFrame,
                "usage: role <user> <standard|trusted|mod|admin>",
            )]);
        };
        let Some(target_user) = resolve_known_user_target(&self.store.users()?, target) else {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::UserNotFound,
                "target user is unknown",
            )]);
        };
        if target_user.identity_hash == peer.identity_hash {
            return Ok(vec![self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "cannot change your own active session role",
            )]);
        }
        let user = self
            .store
            .set_user_role_bits(target_user.user_id, role_bits)?;
        let mut frames = vec![Frame::new(
            ChatOp::CommandResult,
            seq,
            room_id,
            FrameBody::Fields(vec![
                FrameValue::String("role".into()),
                user_to_value(&user),
            ]),
        )];
        frames.push(user_delta_frame(seq, room_id, &user));
        if let Some(room_id) = room_id {
            let event = self.store.append_event(
                room_id,
                Some(actor.user_id),
                ServerRoomEventKind::System {
                    body: format!(
                        "{} set {} role to {}",
                        actor.display_name,
                        user.display_name,
                        role_label_from_bits(role_bits)
                    ),
                },
            )?;
            frames.push(Frame::new(
                ChatOp::RoomEvent,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![event_to_value(&event)]),
            ));
        }
        Ok(frames)
    }

    fn handle_join_room(
        &self,
        peer: &ServerPeer,
        seq: u32,
        body: FrameBody,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<Frame>> {
        let Some(user) = self.ensure_allowed_peer(peer, seq, None)? else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        let room_name = body_string(&body).unwrap_or_else(|| "lobby".into());
        let Some(room) = self.store.room_by_name(&room_name)? else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::RoomNotFound,
                "room not found",
            )]);
        };
        self.store.join_room(room.room_id, user.user_id)?;

        let users = self.user_values_for_join(room.room_id, user, active_room_peers)?;
        let events = self
            .store
            .latest_events(room.room_id, self.limits.join_backlog_events)?
            .into_iter()
            .map(|event| event_to_value(&event))
            .collect::<Vec<_>>();

        let mut frames = vec![
            Frame::new(
                ChatOp::JoinAccept,
                seq,
                Some(room.room_id),
                FrameBody::Fields(vec![room_to_value(&room)]),
            ),
            Frame::new(
                self.batch_op(
                    ChatOp::UserListSnapshotInline,
                    ChatOp::UserListSnapshotResource,
                    &users,
                )?,
                seq,
                Some(room.room_id),
                self.batch_body(room.room_id, "userlist", &users)?,
            ),
        ];
        frames.extend(self.history_batch_frames(seq, room.room_id, "history", &events)?);
        Ok(frames)
    }

    fn handle_part_room(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::RoomNotFound,
                "part has no room id",
            )]);
        };
        let Some(room) = self.store.room_by_id(room_id)? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room not found",
            )]);
        };
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        self.store.leave_room(room_id, user.user_id)?;
        let event = self.store.append_event(
            room_id,
            Some(user.user_id),
            ServerRoomEventKind::System {
                body: format!("{} left #{}", user.display_name, room.name),
            },
        )?;
        Ok(vec![
            Frame::new(
                ChatOp::CommandResult,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![
                    FrameValue::String("part".into()),
                    room_to_value(&room),
                ]),
            ),
            Frame::new(
                ChatOp::RoomEvent,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![event_to_value(&event)]),
            ),
        ])
    }

    /// Executes one already-negotiated durable room message or action. Live
    /// dispatch does not call this boundary until capability acceptance is
    /// enabled and bound to an authenticated Link.
    pub fn handle_durable_room_text(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        op: ChatOp,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableRoomTextDispatch> {
        if !matches!(op, ChatOp::RoomMessage | ChatOp::RoomAction) {
            return Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationMalformed,
                "durable room operation is unsupported",
            ));
        }
        let Some(room_id) = room_id else {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "durable room operation has no room id",
            ));
        };
        let canonical_hash =
            match canonical_mutation_request_hash(op, Some(room_id), &envelope.body) {
                Ok(hash) => hash,
                Err(_) => {
                    return Ok(self.durable_error_dispatch(
                        seq,
                        Some(room_id),
                        ChatErrorCode::DurableMutationMalformed,
                        "durable request body exceeds canonical bounds",
                    ))
                }
            };
        if canonical_hash != envelope.request_hash {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable request hash does not match its canonical body",
            ));
        }
        let Some(body) = body_string(&envelope.body).filter(|body| !body.trim().is_empty()) else {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable message body is empty",
            ));
        };
        if body.len() > self.limits.max_message_bytes {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable message body exceeds server limit",
            ));
        }
        let mutation_id = envelope.mutation_id;
        let request_hash = envelope.request_hash;
        let event_kind = if op == ChatOp::RoomMessage {
            ServerRoomEventKind::Message { body }
        } else {
            ServerRoomEventKind::Action { body }
        };
        let key = DurableMutationKey {
            identity_hash: &peer.identity_hash,
            client_instance_id,
            mutation_id,
        };
        let commit = self.store.commit_durable_room_event_result(
            key,
            request_hash,
            room_id,
            |transaction| {
                let Some(user) = OmenchatStore::ensure_durable_room_user(
                    transaction,
                    room_id,
                    &peer.identity_hash,
                    &peer.display_name,
                    peer.lxmf_destination.as_deref(),
                )?
                else {
                    return Ok(DurableRoomEventPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RoomNotFound,
                            "room not found",
                        ))?,
                    });
                };
                if user.status_bits & STATUS_BANNED != 0 {
                    return Ok(DurableRoomEventPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "user is banned",
                        ))?,
                    });
                }
                if user.status_bits & STATUS_MUTED != 0 {
                    return Ok(DurableRoomEventPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "user is muted",
                        ))?,
                    });
                }
                let admission = match self.reserve_rate(peer, RateKind::Message)? {
                    RateAdmission::Admitted(admission) => admission,
                    RateAdmission::Rejected => {
                        return Ok(DurableRoomEventPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RateLimited,
                                "message rate limit exceeded",
                            ))?,
                        })
                    }
                };
                OmenchatStore::join_durable_room(transaction, room_id, user.user_id)?;
                Ok(DurableRoomEventPlan::Event {
                    actor_user_id: Some(user.user_id),
                    kind: event_kind,
                    admission,
                })
            },
            |event| self.encode_durable_result(message_ack_for_event(seq, event)),
        );
        let commit = match commit {
            Ok(commit) => commit,
            Err(ServerError::Sqlite(error)) if sqlite_is_busy(&error) => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationStoreBusy,
                    "durable mutation store is busy",
                ))
            }
            Err(error) => return Err(error),
        };
        match commit {
            DurableRoomEventCommit::Stored {
                result_frame,
                event,
                admission,
                pruned,
            } => {
                if let Some(admission) = admission {
                    admission.commit();
                }
                Ok(DurableRoomTextDispatch {
                    origin: decode_durable_result(&result_frame)?,
                    broadcast: Some(Frame::new(
                        ChatOp::RoomEvent,
                        seq,
                        Some(room_id),
                        FrameBody::Fields(vec![event_to_value(&event)]),
                    )),
                    pruned,
                })
            }
            DurableRoomEventCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableRoomTextDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcast: None,
                pruned,
            }),
            DurableRoomEventCommit::Replayed { result_frame } => Ok(DurableRoomTextDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcast: None,
                pruned: 0,
            }),
            DurableRoomEventCommit::Conflict => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationConflict,
                "durable mutation id was reused with different content",
            )),
            DurableRoomEventCommit::Expired => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationResultExpired,
                "durable client instance has expired replay state",
            )),
        }
    }

    fn encode_durable_result(&self, frame: Frame) -> ServerResult<Vec<u8>> {
        encode_frame(&frame).map_err(|error| {
            ServerError::Message(format!("durable origin response encode failed: {error}"))
        })
    }

    fn durable_error_dispatch(
        &self,
        seq: u32,
        room_id: Option<RoomId>,
        code: ChatErrorCode,
        message: &str,
    ) -> DurableRoomTextDispatch {
        DurableRoomTextDispatch {
            origin: self.error_frame(seq, room_id, code, message),
            broadcast: None,
            pruned: 0,
        }
    }

    fn handle_room_text(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
        event_kind: impl FnOnce(String) -> ServerRoomEventKind,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::RoomNotFound,
                "message has no room id",
            )]);
        };
        if self.store.room_by_id(room_id)?.is_none() {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room not found",
            )]);
        }
        let Some(body) = body_string(&body).filter(|body| !body.trim().is_empty()) else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::MalformedFrame,
                "message body is empty",
            )]);
        };
        if body.len() > self.limits.max_message_bytes {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::MalformedFrame,
                "message body exceeds server limit",
            )]);
        }
        if let Some(error) =
            self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Message)?
        {
            return Ok(vec![error]);
        }
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if user.status_bits & STATUS_MUTED != 0 {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is muted",
            )]);
        }
        self.store.join_room(room_id, user.user_id)?;
        let event = self
            .store
            .append_event(room_id, Some(user.user_id), event_kind(body))?;
        Ok(vec![Frame::new(
            ChatOp::RoomEvent,
            seq,
            Some(room_id),
            FrameBody::Fields(vec![event_to_value(&event)]),
        )])
    }

    fn handle_room_notice(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::RoomNotFound,
                "notice has no room id",
            )]);
        };
        if self.store.room_by_id(room_id)?.is_none() {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room not found",
            )]);
        }
        let Some(body) = body_string(&body).filter(|body| !body.trim().is_empty()) else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::MalformedFrame,
                "notice body is empty",
            )]);
        };
        if body.len() > self.limits.max_message_bytes {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::MalformedFrame,
                "notice body exceeds server limit",
            )]);
        }
        if let Some(error) =
            self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Message)?
        {
            return Ok(vec![error]);
        }
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if user.status_bits & STATUS_MUTED != 0 {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is muted",
            )]);
        }
        if user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "room notices require moderator or admin role",
            )]);
        }
        self.store.join_room(room_id, user.user_id)?;
        let event = self.store.append_event(
            room_id,
            Some(user.user_id),
            ServerRoomEventKind::Notice { body },
        )?;
        Ok(vec![Frame::new(
            ChatOp::RoomEvent,
            seq,
            Some(room_id),
            FrameBody::Fields(vec![event_to_value(&event)]),
        )])
    }

    fn user_values_for_join(
        &self,
        room_id: RoomId,
        joined_user: ServerUser,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<FrameValue>> {
        if active_room_peers.is_empty() {
            return self
                .store
                .users_for_room(room_id)?
                .into_iter()
                .map(|user| Ok(user_to_value(&user)))
                .collect();
        }

        let mut users = self.user_values_for_active_peers(room_id, active_room_peers)?;
        if !users.iter().any(|value| {
            matches!(
                value,
                FrameValue::Array(fields)
                    if fields.first() == Some(&FrameValue::U64(joined_user.user_id as u64))
            )
        }) {
            users.push(user_to_value(&joined_user));
        }
        Ok(users)
    }

    fn user_values_for_active_peers(
        &self,
        room_id: RoomId,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<FrameValue>> {
        let mut users = Vec::new();
        let mut seen_hashes = Vec::<Vec<u8>>::new();
        for peer in active_room_peers {
            if peer.identity_hash.is_empty() || seen_hashes.contains(&peer.identity_hash) {
                continue;
            }
            seen_hashes.push(peer.identity_hash.clone());
            let user = self.ensure_peer(peer)?;
            self.store.join_room(room_id, user.user_id)?;
            users.push(user);
        }
        users.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.user_id.cmp(&right.user_id))
        });
        Ok(users.iter().map(user_to_value).collect())
    }

    fn handle_history_before(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::RoomNotFound,
                "history request has no room id",
            )]);
        };
        if self.store.room_by_id(room_id)?.is_none() {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room was not found",
            )]);
        }
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if !self.store.room_has_member(room_id, user.user_id)? {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::NotJoined,
                "join the room before requesting history",
            )]);
        }
        let before_event_id = body_u64(&body)
            .unwrap_or(i64::MAX as u64)
            .min(i64::MAX as u64);
        let events = self
            .store
            .events_before(room_id, before_event_id, self.limits.history_batch_size)?
            .into_iter()
            .map(|event| event_to_value(&event))
            .collect::<Vec<_>>();
        if events.is_empty() {
            return Ok(vec![Frame::new(
                ChatOp::HistoryEnd,
                seq,
                Some(room_id),
                FrameBody::Empty,
            )]);
        }
        self.history_batch_frames(seq, room_id, "history", &events)
    }

    fn handle_history_recent(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::RoomNotFound,
                "recent history request has no room id",
            )]);
        };
        if self.store.room_by_id(room_id)?.is_none() {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room was not found",
            )]);
        }
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if !self.store.room_has_member(room_id, user.user_id)? {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::NotJoined,
                "join the room before syncing history",
            )]);
        }

        let events = self
            .store
            .latest_events(room_id, self.limits.history_batch_size)?;
        let local = history_fingerprint_body(&body);
        let server = server_history_fingerprint(&events);
        if local == server {
            return Ok(vec![Frame::new(
                ChatOp::HistoryCurrent,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![
                    FrameValue::U64(server.first_event_id),
                    FrameValue::U64(server.last_event_id),
                    FrameValue::U64(server.event_count),
                    FrameValue::U64(server.checksum),
                ]),
            )]);
        }

        let values = events.iter().map(event_to_value).collect::<Vec<_>>();
        if values.is_empty() {
            return Ok(vec![Frame::new(
                ChatOp::HistoryCurrent,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![
                    FrameValue::U64(0),
                    FrameValue::U64(0),
                    FrameValue::U64(0),
                    FrameValue::U64(server.checksum),
                ]),
            )]);
        }
        self.history_batch_frames(seq, room_id, "recent", &values)
    }

    fn handle_upload_offer(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.upload_reject_frame(
                seq,
                None,
                "upload offer has no room id",
                self.limits.upload_quota_bytes,
                0,
            )]);
        };
        if self.store.room_by_id(room_id)?.is_none() {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "room not found",
                self.limits.upload_quota_bytes,
                0,
            )]);
        }
        if let Some(error) =
            self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Command)?
        {
            return Ok(vec![error]);
        }
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "user is banned",
                self.limits.upload_quota_bytes,
                0,
            )]);
        };
        if user.status_bits & STATUS_MUTED != 0 {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "user is muted",
                self.limits.upload_quota_bytes,
                0,
            )]);
        }
        if !self.store.room_has_member(room_id, user.user_id)? {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "join the room before uploading",
                self.limits.upload_quota_bytes,
                0,
            )]);
        }

        let Some(offer) = upload_offer_body(&body) else {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "upload offer must include filename and byte length",
                self.limits.upload_quota_bytes,
                0,
            )]);
        };
        if offer.filename.len() > UPLOAD_FILENAME_MAX_BYTES
            || offer
                .content_type
                .as_ref()
                .is_some_and(|value| value.len() > UPLOAD_CONTENT_TYPE_MAX_BYTES)
        {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "upload metadata exceeds server limit",
                self.limits.upload_quota_bytes,
                offer.incoming_bytes,
            )]);
        }
        if offer.incoming_bytes == 0 {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "upload offer byte length is empty",
                self.limits.upload_quota_bytes,
                0,
            )]);
        }
        if offer.incoming_bytes > self.limits.upload_max_file_bytes {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "upload exceeds server file size limit",
                self.limits.upload_max_file_bytes,
                offer.incoming_bytes,
            )]);
        }
        let Some(cache_root) = self.limits.upload_cache_root.clone() else {
            return Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "upload cache is unavailable",
                self.limits.upload_quota_bytes,
                offer.incoming_bytes,
            )]);
        };
        let policy = UploadPolicy {
            cache_root,
            quota_bytes: self.limits.upload_quota_bytes,
        };
        let identity_dir =
            crate::upload::upload_identity_dir_for_root(&policy.cache_root, &peer.identity_hash);
        let indexed = self.store.plan_upload_from_index(
            user.user_id,
            &identity_dir,
            offer.incoming_bytes,
            policy.quota_bytes,
        )?;
        match plan_upload_with_index(&policy, &peer.identity_hash, offer.incoming_bytes, indexed) {
            UploadQuotaDecision::Disabled => Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "uploads are disabled by server policy",
                self.limits.upload_quota_bytes,
                offer.incoming_bytes,
            )]),
            UploadQuotaDecision::TooLarge {
                quota_bytes,
                incoming_bytes,
            } => Ok(vec![self.upload_reject_frame(
                seq,
                Some(room_id),
                "upload exceeds server quota",
                quota_bytes,
                incoming_bytes,
            )]),
            UploadQuotaDecision::Accepted(plan) => {
                let resource_id = upload_resource_id(room_id, user.user_id, seq, &offer);
                let accepted_at = unix_seconds();
                let accepted = self
                    .pending_uploads
                    .lock()
                    .map_err(|_| ServerError::Message("pending upload lock poisoned".into()))?
                    .insert(
                        resource_id.clone(),
                        PendingUpload {
                            identity_hash: peer.identity_hash.clone(),
                            room_id,
                            user_id: user.user_id,
                            filename: offer.filename,
                            content_type: offer.content_type,
                            incoming_bytes: plan.incoming_bytes,
                            accepted_at,
                        },
                        accepted_at,
                    );
                if !accepted {
                    return Ok(vec![self.upload_reject_frame(
                        seq,
                        Some(room_id),
                        "too many pending upload offers",
                        plan.quota_bytes,
                        plan.incoming_bytes,
                    )]);
                }
                Ok(vec![Frame::new(
                    ChatOp::UploadAccept,
                    seq,
                    Some(room_id),
                    FrameBody::Fields(vec![
                        FrameValue::String(resource_id),
                        FrameValue::U64(plan.quota_bytes),
                        FrameValue::U64(plan.incoming_bytes),
                        FrameValue::U64(plan.evict.len() as u64),
                    ]),
                )])
            }
        }
    }

    pub fn handle_upload_resource(
        &self,
        peer: &ServerPeer,
        resource_id: &str,
        data: Vec<u8>,
    ) -> ServerResult<Vec<Frame>> {
        let pending = self
            .pending_uploads
            .lock()
            .map_err(|_| ServerError::Message("pending upload lock poisoned".into()))?
            .take_for_identity(resource_id, &peer.identity_hash, unix_seconds());
        let upload = match pending {
            PendingUploadTake::Found(upload) => upload,
            PendingUploadTake::NotFound => {
                return Ok(vec![self.upload_reject_frame(
                    0,
                    None,
                    "unknown or expired upload resource",
                    self.limits.upload_quota_bytes,
                    data.len() as u64,
                )]);
            }
            PendingUploadTake::IdentityMismatch => {
                return Ok(vec![self.upload_reject_frame(
                    0,
                    None,
                    "upload resource identity mismatch",
                    self.limits.upload_quota_bytes,
                    data.len() as u64,
                )]);
            }
        };
        if data.len() as u64 != upload.incoming_bytes {
            return Ok(vec![self.upload_reject_frame(
                0,
                Some(upload.room_id),
                "upload resource size mismatch",
                self.limits.upload_quota_bytes,
                data.len() as u64,
            )]);
        }
        let Some(cache_root) = self.limits.upload_cache_root.clone() else {
            return Ok(vec![self.upload_reject_frame(
                0,
                Some(upload.room_id),
                "upload cache is unavailable",
                self.limits.upload_quota_bytes,
                upload.incoming_bytes,
            )]);
        };
        let policy = UploadPolicy {
            cache_root,
            quota_bytes: self.limits.upload_quota_bytes,
        };
        let _user = self.ensure_peer(peer)?;
        let identity_dir =
            crate::upload::upload_identity_dir_for_root(&policy.cache_root, &peer.identity_hash);
        let stored = store_upload_with_policy_indexed_and_commit(
            &policy,
            &peer.identity_hash,
            &upload.filename,
            &data,
            |incoming_bytes| {
                let indexed = self.store.plan_upload_from_index(
                    upload.user_id,
                    &identity_dir,
                    incoming_bytes,
                    policy.quota_bytes,
                )?;
                Ok(plan_upload_with_index(
                    &policy,
                    &peer.identity_hash,
                    incoming_bytes,
                    indexed,
                ))
            },
            |pending| {
                self.store
                    .record_upload_file(crate::store::RecordUploadFile {
                        resource_id,
                        room_id: upload.room_id,
                        actor_user_id: upload.user_id,
                        filename: &upload.filename,
                        content_type: upload.content_type.as_deref(),
                        byte_len: pending.bytes,
                        path: &pending.path,
                    })
            },
        )?;
        if let Err(error) =
            self.store
                .remove_evicted_upload_records(upload.user_id, resource_id, &stored.evicted)
        {
            // The file replacement is already durable. Retaining stale rows
            // over-counts quota and is safer than reporting failure after a
            // successful client-visible upload. Force reconciliation before
            // the next admission and emit an operator-visible diagnostic.
            self.store.invalidate_upload_ledger(upload.user_id);
            eprintln!(
                "omenchatd upload ledger cleanup failed after durable replacement; next upload will require reconciliation: {error}"
            );
        }
        let event = self.store.append_event(
            upload.room_id,
            Some(upload.user_id),
            ServerRoomEventKind::Upload {
                resource_id: resource_id.to_owned(),
                filename: upload.filename.clone(),
                bytes: stored.bytes,
            },
        )?;
        Ok(vec![
            Frame::new(
                ChatOp::UploadComplete,
                0,
                Some(upload.room_id),
                FrameBody::Fields(vec![
                    FrameValue::String(resource_id.to_owned()),
                    FrameValue::String(upload.filename),
                    FrameValue::U64(stored.bytes),
                    FrameValue::U64(stored.evicted.len() as u64),
                ]),
            ),
            Frame::new(
                ChatOp::RoomEvent,
                0,
                Some(upload.room_id),
                FrameBody::Fields(vec![event_to_value(&event)]),
            ),
        ])
    }

    fn handle_upload_fetch(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        body: FrameBody,
    ) -> ServerResult<Vec<Frame>> {
        let Some(room_id) = room_id else {
            return Ok(vec![self.error_frame(
                seq,
                None,
                ChatErrorCode::NotJoined,
                "upload fetch has no room id",
            )]);
        };
        let Some(resource_id) = frame_body_values(&body)
            .and_then(|values| values.first())
            .and_then(frame_value_string)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::MalformedFrame,
                "upload fetch must include a resource id",
            )]);
        };
        let Some(user) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if !self.store.room_has_member(room_id, user.user_id)? {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::NotJoined,
                "join the room before fetching uploads",
            )]);
        }
        let Some(upload) = self.store.upload_file(&resource_id)? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::ResourceUnavailable,
                "upload resource is unavailable",
            )]);
        };
        if upload.room_id != room_id {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::NotJoined,
                "upload resource belongs to another room",
            )]);
        }
        let bytes = std::fs::read(&upload.path)?;
        if bytes.len() as u64 != upload.byte_len {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::ResourceUnavailable,
                "upload resource size changed on disk",
            )]);
        }
        if bytes.len() <= UPLOAD_INLINE_MAX_BYTES {
            return Ok(upload_inline_chunk_frames(
                seq,
                room_id,
                resource_id,
                upload.filename,
                upload.content_type,
                bytes,
            ));
        }
        self.store_pending_resource(resource_id.clone(), bytes)?;
        Ok(vec![Frame::new(
            ChatOp::UploadResourceOffer,
            seq,
            Some(room_id),
            FrameBody::Fields(vec![
                FrameValue::String(resource_id),
                FrameValue::String(upload.filename),
                FrameValue::U64(upload.byte_len),
                upload
                    .content_type
                    .map(FrameValue::String)
                    .unwrap_or(FrameValue::Nil),
            ]),
        )])
    }

    fn ensure_peer(&self, peer: &ServerPeer) -> ServerResult<ServerUser> {
        if peer.identity_hash.is_empty() {
            return Err(ServerError::Message("peer identity hash is empty".into()));
        }
        self.store.ensure_user(
            &peer.identity_hash,
            &peer.display_name,
            peer.lxmf_destination.as_deref(),
        )
    }

    fn ensure_allowed_peer(
        &self,
        peer: &ServerPeer,
        _seq: u32,
        _room_id: Option<RoomId>,
    ) -> ServerResult<Option<ServerUser>> {
        let user = self.ensure_peer(peer)?;
        if user.status_bits & STATUS_BANNED != 0 {
            return Ok(None);
        }
        Ok(Some(user))
    }

    fn reject_if_banned(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
    ) -> ServerResult<Option<Frame>> {
        let user = self.ensure_peer(peer)?;
        if user.status_bits & STATUS_BANNED != 0 {
            return Ok(Some(self.error_frame(
                seq,
                room_id,
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )));
        }
        Ok(None)
    }

    fn reject_if_rate_limited(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        kind: RateKind,
    ) -> ServerResult<Option<Frame>> {
        match self.reserve_rate(peer, kind)? {
            RateAdmission::Admitted(reservation) => {
                if let Some(reservation) = reservation {
                    reservation.commit();
                }
                Ok(None)
            }
            RateAdmission::Rejected => {
                let message = match kind {
                    RateKind::Message => "message rate limit exceeded",
                    RateKind::Command => "command rate limit exceeded",
                };
                Ok(Some(self.error_frame(
                    seq,
                    room_id,
                    ChatErrorCode::RateLimited,
                    message,
                )))
            }
        }
    }

    fn reserve_rate(&self, peer: &ServerPeer, kind: RateKind) -> ServerResult<RateAdmission> {
        let limit = match kind {
            RateKind::Message => self.limits.rate_messages_per_minute,
            RateKind::Command => self.limits.rate_commands_per_minute,
        };
        if limit == 0 {
            return Ok(RateAdmission::Admitted(None));
        }

        let now = unix_seconds();
        let mut buckets = self
            .rate_buckets
            .lock()
            .map_err(|_| ServerError::Message("rate limiter lock poisoned".into()))?;
        let key = (peer.identity_hash.clone(), kind);
        let bucket = buckets.entry(key.clone()).or_insert_with(|| RateBucket {
            window_start: now,
            count: 0,
        });
        if now.saturating_sub(bucket.window_start) >= 60 {
            bucket.window_start = now;
            bucket.count = 0;
        }
        if bucket.count >= limit {
            return Ok(RateAdmission::Rejected);
        }
        bucket.count += 1;
        let window_start = bucket.window_start;
        drop(buckets);
        Ok(RateAdmission::Admitted(Some(RateReservation {
            buckets: Arc::clone(&self.rate_buckets),
            key,
            window_start,
            active: true,
        })))
    }

    fn error_frame(
        &self,
        seq: u32,
        room_id: Option<RoomId>,
        code: ChatErrorCode,
        message: &str,
    ) -> Frame {
        Frame::new(
            ChatOp::Error,
            seq,
            room_id,
            FrameBody::Fields(vec![
                FrameValue::U64(code as u16 as u64),
                FrameValue::String(message.into()),
            ]),
        )
    }

    fn upload_reject_frame(
        &self,
        seq: u32,
        room_id: Option<RoomId>,
        reason: &str,
        quota_bytes: u64,
        incoming_bytes: u64,
    ) -> Frame {
        Frame::new(
            ChatOp::UploadReject,
            seq,
            room_id,
            FrameBody::Fields(vec![
                FrameValue::String(reason.into()),
                FrameValue::U64(quota_bytes),
                FrameValue::U64(incoming_bytes),
            ]),
        )
    }

    fn batch_op(
        &self,
        inline_op: ChatOp,
        resource_op: ChatOp,
        values: &[FrameValue],
    ) -> ServerResult<ChatOp> {
        let (_, compressed_len) = encoded_compressed_len(values)
            .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
        if compressed_len > self.limits.large_batch_threshold_bytes {
            Ok(resource_op)
        } else {
            Ok(inline_op)
        }
    }

    fn batch_body(
        &self,
        room_id: RoomId,
        purpose: &str,
        values: &[FrameValue],
    ) -> ServerResult<FrameBody> {
        let (uncompressed_len, compressed_len) = encoded_compressed_len(values)
            .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
        if compressed_len > self.limits.large_batch_threshold_bytes {
            let resource_id = format!("{purpose}:{room_id}:{uncompressed_len}:{compressed_len}");
            let payload = compressed_values_payload(values)
                .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
            self.store_pending_resource(resource_id.clone(), payload)?;
            Ok(resource_offer_body(&ResourceOffer {
                resource_id,
                compression: Compression::Bzip2,
                uncompressed_len: uncompressed_len as u64,
                compressed_len: compressed_len as u64,
                purpose: purpose.into(),
            }))
        } else {
            compressed_values_body(values)
                .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))
        }
    }

    fn resource_batch_body(
        &self,
        room_id: RoomId,
        purpose: &str,
        values: &[FrameValue],
    ) -> ServerResult<FrameBody> {
        let (uncompressed_len, compressed_len) = encoded_compressed_len(values)
            .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
        let resource_id = format!("{purpose}:{room_id}:{uncompressed_len}:{compressed_len}");
        let payload = compressed_values_payload(values)
            .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
        self.store_pending_resource(resource_id.clone(), payload)?;
        Ok(resource_offer_body(&ResourceOffer {
            resource_id,
            compression: Compression::Bzip2,
            uncompressed_len: uncompressed_len as u64,
            compressed_len: compressed_len as u64,
            purpose: purpose.into(),
        }))
    }

    fn history_batch_frames(
        &self,
        seq: u32,
        room_id: RoomId,
        purpose: &str,
        values: &[FrameValue],
    ) -> ServerResult<Vec<Frame>> {
        if values.is_empty() {
            return Ok(Vec::new());
        }
        if self.limits.large_batch_threshold_bytes < LINK_INLINE_HISTORY_TARGET_BYTES {
            return Ok(vec![Frame::new(
                ChatOp::HistoryResourceOffer,
                seq,
                Some(room_id),
                self.resource_batch_body(room_id, purpose, values)?,
            )]);
        }

        let inline_target = self
            .limits
            .large_batch_threshold_bytes
            .clamp(1, LINK_INLINE_HISTORY_TARGET_BYTES);
        let mut frames = Vec::new();
        let mut chunk = Vec::new();

        for value in values {
            let mut candidate = chunk.clone();
            candidate.push(value.clone());
            let (_, candidate_len) = encoded_compressed_len(&candidate)
                .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
            if !chunk.is_empty() && candidate_len > inline_target {
                frames.push(self.history_batch_frame(seq, room_id, purpose, &chunk)?);
                chunk.clear();
            }
            chunk.push(value.clone());
        }

        if !chunk.is_empty() {
            frames.push(self.history_batch_frame(seq, room_id, purpose, &chunk)?);
        }

        Ok(frames)
    }

    fn history_batch_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        purpose: &str,
        values: &[FrameValue],
    ) -> ServerResult<Frame> {
        let (_, compressed_len) = encoded_compressed_len(values)
            .map_err(|error| ServerError::Message(format!("batch encode failed: {error}")))?;
        let inline_target = self
            .limits
            .large_batch_threshold_bytes
            .clamp(1, LINK_INLINE_HISTORY_TARGET_BYTES);
        if compressed_len <= inline_target {
            Ok(Frame::new(
                ChatOp::HistoryInline,
                seq,
                Some(room_id),
                compressed_values_body(values).map_err(|error| {
                    ServerError::Message(format!("batch encode failed: {error}"))
                })?,
            ))
        } else {
            Ok(Frame::new(
                ChatOp::HistoryResourceOffer,
                seq,
                Some(room_id),
                self.resource_batch_body(room_id, purpose, values)?,
            ))
        }
    }

    pub fn resource_payload(&self, resource_id: &str) -> ServerResult<Option<Vec<u8>>> {
        Ok(self
            .pending_resources
            .lock()
            .map_err(|_| ServerError::Message("pending resource lock poisoned".into()))?
            .get(resource_id))
    }

    pub fn take_resource_payload(&self, resource_id: &str) -> ServerResult<Option<Vec<u8>>> {
        Ok(self
            .pending_resources
            .lock()
            .map_err(|_| ServerError::Message("pending resource lock poisoned".into()))?
            .take(resource_id))
    }

    pub(crate) fn pending_resource_metrics(&self) -> ServerResult<(usize, usize, u64)> {
        let pending = self
            .pending_resources
            .lock()
            .map_err(|_| ServerError::Message("pending resource lock poisoned".into()))?;
        Ok((
            pending.entries.len(),
            pending.retained_bytes,
            pending.rejected,
        ))
    }

    pub(crate) fn pending_upload_metrics(&self) -> ServerResult<(usize, usize, u64, u64)> {
        Ok(self
            .pending_uploads
            .lock()
            .map_err(|_| ServerError::Message("pending upload lock poisoned".into()))?
            .metrics(unix_seconds()))
    }

    pub(crate) fn discard_pending_uploads_for_identity(
        &self,
        identity_hash: &[u8],
    ) -> ServerResult<usize> {
        Ok(self
            .pending_uploads
            .lock()
            .map_err(|_| ServerError::Message("pending upload lock poisoned".into()))?
            .remove_identity(identity_hash, unix_seconds()))
    }

    fn store_pending_resource(&self, resource_id: String, payload: Vec<u8>) -> ServerResult<()> {
        self.pending_resources
            .lock()
            .map_err(|_| ServerError::Message("pending resource lock poisoned".into()))?
            .insert(resource_id, payload)
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn body_string(body: &FrameBody) -> Option<String> {
    match body {
        FrameBody::Text(value) => Some(value.clone()),
        FrameBody::Fields(values) => values.iter().find_map(|value| match value {
            FrameValue::String(value) => Some(value.clone()),
            _ => None,
        }),
        FrameBody::Empty => None,
    }
}

fn body_u64(body: &FrameBody) -> Option<u64> {
    match body {
        FrameBody::Fields(values) => values.iter().find_map(|value| match value {
            FrameValue::U64(value) => Some(*value),
            FrameValue::I64(value) if *value >= 0 => Some(*value as u64),
            _ => None,
        }),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UploadOfferBody {
    filename: String,
    incoming_bytes: u64,
    content_type: Option<String>,
}

fn upload_offer_body(body: &FrameBody) -> Option<UploadOfferBody> {
    let FrameBody::Fields(values) = body else {
        return None;
    };
    let filename = values.first().and_then(frame_value_string)?;
    let incoming_bytes = values.get(1).and_then(frame_value_u64)?;
    let content_type = values.get(2).and_then(frame_value_string);
    Some(UploadOfferBody {
        filename,
        incoming_bytes,
        content_type,
    })
}

fn upload_resource_id(room_id: RoomId, user_id: u32, seq: u32, offer: &UploadOfferBody) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in offer.filename.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if let Some(content_type) = &offer.content_type {
        for byte in content_type.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash ^= offer.incoming_bytes;
    hash = hash.wrapping_mul(0x100000001b3);
    format!("upload:{room_id}:{user_id}:{seq}:{hash:016x}")
}

fn upload_inline_chunk_frames(
    seq: u32,
    room_id: RoomId,
    resource_id: String,
    filename: String,
    content_type: Option<String>,
    bytes: Vec<u8>,
) -> Vec<Frame> {
    let total_len = bytes.len() as u64;
    let mut frames = Vec::new();
    for (index, chunk) in bytes.chunks(UPLOAD_INLINE_CHUNK_BYTES).enumerate() {
        let offset = (index * UPLOAD_INLINE_CHUNK_BYTES) as u64;
        let done = offset + chunk.len() as u64 >= total_len;
        frames.push(Frame::new(
            ChatOp::UploadInlineChunk,
            seq,
            Some(room_id),
            FrameBody::Fields(vec![
                FrameValue::String(resource_id.clone()),
                FrameValue::String(filename.clone()),
                FrameValue::U64(total_len),
                content_type
                    .clone()
                    .map(FrameValue::String)
                    .unwrap_or(FrameValue::Nil),
                FrameValue::U64(offset),
                FrameValue::Bytes(chunk.to_vec()),
                FrameValue::Bool(done),
            ]),
        ));
    }
    if frames.is_empty() {
        frames.push(Frame::new(
            ChatOp::UploadInlineChunk,
            seq,
            Some(room_id),
            FrameBody::Fields(vec![
                FrameValue::String(resource_id),
                FrameValue::String(filename),
                FrameValue::U64(0),
                content_type
                    .map(FrameValue::String)
                    .unwrap_or(FrameValue::Nil),
                FrameValue::U64(0),
                FrameValue::Bytes(Vec::new()),
                FrameValue::Bool(true),
            ]),
        ));
    }
    frames
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HistoryFingerprint {
    first_event_id: u64,
    last_event_id: u64,
    event_count: u64,
    checksum: u64,
}

fn history_fingerprint_body(body: &FrameBody) -> HistoryFingerprint {
    let values = match body {
        FrameBody::Fields(values) => values,
        _ => return HistoryFingerprint::default(),
    };
    HistoryFingerprint {
        first_event_id: values.first().and_then(frame_value_u64).unwrap_or(0),
        last_event_id: values.get(1).and_then(frame_value_u64).unwrap_or(0),
        event_count: values.get(2).and_then(frame_value_u64).unwrap_or(0),
        checksum: values.get(3).and_then(frame_value_u64).unwrap_or(0),
    }
}

fn frame_value_string(value: &FrameValue) -> Option<String> {
    match value {
        FrameValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn frame_body_values(body: &FrameBody) -> Option<&[FrameValue]> {
    match body {
        FrameBody::Fields(values) => Some(values),
        _ => None,
    }
}

fn frame_value_u64(value: &FrameValue) -> Option<u64> {
    match value {
        FrameValue::U64(value) => Some(*value),
        FrameValue::I64(value) if *value >= 0 => Some(*value as u64),
        _ => None,
    }
}

fn server_history_fingerprint(events: &[ServerRoomEvent]) -> HistoryFingerprint {
    let mut checksum = 0xcbf29ce484222325_u64;
    for event in events {
        checksum = fnv_mix_u64(checksum, event.event_id);
        checksum = fnv_mix_u64(checksum, event.room_id as u64);
        checksum = fnv_mix_u64(checksum, event.actor_user_id.unwrap_or_default() as u64);
        checksum = fnv_mix_u64(checksum, event.at_unix as u64);
        checksum = fnv_mix_bytes(checksum, event.actor_display_name.as_deref().unwrap_or(""));
        match &event.kind {
            ServerRoomEventKind::Message { body } => {
                checksum = fnv_mix_u64(checksum, 1);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ServerRoomEventKind::Action { body } => {
                checksum = fnv_mix_u64(checksum, 2);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ServerRoomEventKind::Notice { body } => {
                checksum = fnv_mix_u64(checksum, 3);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ServerRoomEventKind::System { body } => {
                checksum = fnv_mix_u64(checksum, 4);
                checksum = fnv_mix_bytes(checksum, body);
            }
            ServerRoomEventKind::Upload {
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
    HistoryFingerprint {
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

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn resolve_active_peer_target(peers: &[ServerPeer], target: &str) -> Option<ServerPeer> {
    let target = target.trim().trim_start_matches('@');
    if target.is_empty() {
        return None;
    }
    let target_lower = target.to_ascii_lowercase();
    peers
        .iter()
        .find(|peer| {
            peer.display_name.eq_ignore_ascii_case(target)
                || identity_hex_starts_with(&peer.identity_hash, &target_lower)
        })
        .cloned()
}

fn resolve_known_user_target(users: &[ServerUser], target: &str) -> Option<ServerUser> {
    let target = target.trim().trim_start_matches('@');
    if target.is_empty() {
        return None;
    }
    let target_lower = target.to_ascii_lowercase();
    users
        .iter()
        .find(|user| {
            user.display_name.eq_ignore_ascii_case(target)
                || user.user_id.to_string() == target
                || identity_hex_starts_with(&user.identity_hash, &target_lower)
        })
        .cloned()
}

fn moderation_past_tense(command: &str) -> &'static str {
    match command {
        "ban" => "banned",
        "mute" => "muted",
        "unmute" => "unmuted",
        _ => "kicked",
    }
}

fn role_bits_from_label(label: &str) -> Option<u64> {
    match label.trim().to_ascii_lowercase().as_str() {
        "standard" | "user" | "none" => Some(0),
        "trusted" | "trust" => Some(ROLE_TRUSTED),
        "mod" | "moderator" => Some(ROLE_TRUSTED | ROLE_MODERATOR),
        "admin" | "administrator" => Some(ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN),
        _ => None,
    }
}

fn role_label_from_bits(role_bits: u64) -> &'static str {
    if role_bits & ROLE_ADMIN != 0 {
        "admin"
    } else if role_bits & ROLE_MODERATOR != 0 {
        "mod"
    } else if role_bits & ROLE_TRUSTED != 0 {
        "trusted"
    } else {
        "standard"
    }
}

fn identity_hex_starts_with(identity_hash: &[u8], target: &str) -> bool {
    let mut rendered = String::with_capacity(identity_hash.len() * 2);
    for byte in identity_hash {
        use std::fmt::Write;
        let _ = write!(&mut rendered, "{byte:02x}");
    }
    rendered.starts_with(target)
}

fn room_to_value(room: &ServerRoom) -> FrameValue {
    FrameValue::Array(vec![
        FrameValue::U64(room.room_id as u64),
        FrameValue::String(room.name.clone()),
        room.topic
            .clone()
            .map(FrameValue::String)
            .unwrap_or(FrameValue::Nil),
        FrameValue::U64(room.room_revision),
    ])
}

fn user_to_value(user: &ServerUser) -> FrameValue {
    FrameValue::Array(vec![
        FrameValue::U64(user.user_id as u64),
        FrameValue::String(user.display_name.clone()),
        FrameValue::U64(user.role_bits),
        FrameValue::U64(user.status_bits as u64),
        FrameValue::Bool(user.lxmf_destination.is_some()),
    ])
}

fn user_delta_frame(seq: u32, room_id: Option<RoomId>, user: &ServerUser) -> Frame {
    Frame::new(
        ChatOp::UserDelta,
        seq,
        room_id,
        FrameBody::Fields(vec![user_to_value(user)]),
    )
}

fn event_to_value(event: &ServerRoomEvent) -> FrameValue {
    let (kind, body) = match &event.kind {
        ServerRoomEventKind::Message { body } => (1_u64, body.clone()),
        ServerRoomEventKind::Action { body } => (2, body.clone()),
        ServerRoomEventKind::Notice { body } => (3, body.clone()),
        ServerRoomEventKind::System { body } => (4, body.clone()),
        ServerRoomEventKind::Upload {
            filename, bytes, ..
        } => (
            5,
            format!("uploaded {} ({})", filename, human_bytes(*bytes)),
        ),
    };
    let mut fields = vec![
        FrameValue::U64(event.event_id),
        FrameValue::U64(kind),
        event
            .actor_user_id
            .map(|user_id| FrameValue::U64(user_id as u64))
            .unwrap_or(FrameValue::Nil),
        FrameValue::I64(event.at_unix),
        FrameValue::String(body),
        event
            .actor_display_name
            .clone()
            .map(FrameValue::String)
            .unwrap_or(FrameValue::Nil),
    ];
    if let ServerRoomEventKind::Upload {
        resource_id,
        filename,
        bytes,
    } = &event.kind
    {
        fields.push(FrameValue::String(resource_id.clone()));
        fields.push(FrameValue::String(filename.clone()));
        fields.push(FrameValue::U64(*bytes));
    }
    FrameValue::Array(fields)
}

fn message_ack_for_event(seq: u32, event: &ServerRoomEvent) -> Frame {
    let kind = match &event.kind {
        ServerRoomEventKind::Message { .. } => 1,
        ServerRoomEventKind::Action { .. } => 2,
        ServerRoomEventKind::Notice { .. } => 3,
        ServerRoomEventKind::System { .. } => 4,
        ServerRoomEventKind::Upload { .. } => 5,
    };
    Frame::new(
        ChatOp::MessageAck,
        seq,
        Some(event.room_id),
        FrameBody::Fields(vec![
            FrameValue::U64(event.event_id),
            FrameValue::U64(kind),
            event
                .actor_user_id
                .map(|user_id| FrameValue::U64(user_id as u64))
                .unwrap_or(FrameValue::Nil),
            FrameValue::I64(event.at_unix),
            event
                .actor_display_name
                .clone()
                .map(FrameValue::String)
                .unwrap_or(FrameValue::Nil),
        ]),
    )
}

fn decode_durable_result(bytes: &[u8]) -> ServerResult<Frame> {
    decode_frame(bytes).map_err(|error| {
        ServerError::Message(format!(
            "stored durable origin response is invalid: {error}"
        ))
    })
}

fn sqlite_is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::batch::{
        decode_compressed_values_body, decode_compressed_values_payload, decode_resource_offer_body,
    };
    use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue};
    use crate::store::OmenchatStore;

    fn peer() -> ServerPeer {
        ServerPeer {
            identity_hash: b"peer-a".to_vec(),
            display_name: "Alice".into(),
            lxmf_destination: Some("lxmf-a".into()),
        }
    }

    fn durable_envelope(
        op: ChatOp,
        room_id: RoomId,
        mutation_marker: u8,
        body: &str,
    ) -> DurableMutationEnvelope {
        let body = FrameBody::Text(body.into());
        DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([mutation_marker; 16]),
            request_hash: canonical_mutation_request_hash(op, Some(room_id), &body)
                .expect("canonical hash"),
            body,
        }
    }

    fn frame_error_code(frame: &Frame) -> Option<u64> {
        let FrameBody::Fields(fields) = &frame.body else {
            return None;
        };
        match fields.first() {
            Some(FrameValue::U64(code)) => Some(*code),
            _ => None,
        }
    }

    fn temp_store_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-session-{label}-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    fn temp_upload_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-session-upload-{label}-{}",
            std::process::id()
        ))
    }

    fn join_lobby(engine: &SessionEngine, peer: &ServerPeer) {
        engine
            .handle_frame(
                peer,
                Frame::new(ChatOp::SessionOpen, 1, None, FrameBody::Empty),
            )
            .expect("session open");
        engine
            .handle_frame(
                peer,
                Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
            )
            .expect("join lobby");
    }

    #[test]
    fn pending_resource_store_enforces_item_and_entry_budgets() {
        let mut store = PendingResourceStore::default();
        for index in 0..PENDING_RESOURCE_MAX_ITEMS {
            store
                .insert(format!("resource-{index}"), vec![index as u8])
                .expect("bounded item admission");
        }
        let item_error = store
            .insert("one-too-many".into(), vec![1])
            .expect_err("item overflow must fail");
        assert!(item_error.to_string().contains("admission rejected"));
        assert_eq!(store.entries.len(), PENDING_RESOURCE_MAX_ITEMS);
        assert_eq!(store.retained_bytes, PENDING_RESOURCE_MAX_ITEMS);

        let removed = store.take("resource-0").expect("remove retained payload");
        assert_eq!(removed, vec![0]);
        store
            .insert("replacement-slot".into(), vec![2])
            .expect("released item capacity");
        assert_eq!(store.entries.len(), PENDING_RESOURCE_MAX_ITEMS);

        let mut entry_store = PendingResourceStore::default();
        let entry_error = entry_store
            .insert(
                "oversized".into(),
                vec![0; PENDING_RESOURCE_MAX_ENTRY_BYTES + 1],
            )
            .expect_err("oversized entry must fail");
        assert!(entry_error.to_string().contains("admission rejected"));
        assert!(entry_store.entries.is_empty());
        assert_eq!(entry_store.retained_bytes, 0);
        assert_eq!(entry_store.rejected, 1);
    }

    #[test]
    fn pending_resource_store_enforces_global_bytes_and_replacement_accounting() {
        let mut store = PendingResourceStore::default();
        for index in 0..4 {
            store
                .insert(
                    format!("full-{index}"),
                    vec![index as u8; PENDING_RESOURCE_MAX_ENTRY_BYTES],
                )
                .expect("exact global byte budget");
        }
        assert_eq!(store.retained_bytes, PENDING_RESOURCE_MAX_BYTES);
        store
            .insert("full-0".into(), vec![9; PENDING_RESOURCE_MAX_ENTRY_BYTES])
            .expect("same-sized replacement");
        assert_eq!(store.retained_bytes, PENDING_RESOURCE_MAX_BYTES);
        store
            .insert("full-0".into(), vec![9])
            .expect("smaller replacement");
        assert_eq!(
            store.retained_bytes,
            PENDING_RESOURCE_MAX_BYTES - PENDING_RESOURCE_MAX_ENTRY_BYTES + 1
        );
        store
            .insert(
                "refill".into(),
                vec![8; PENDING_RESOURCE_MAX_ENTRY_BYTES - 1],
            )
            .expect("released byte capacity");
        assert_eq!(store.retained_bytes, PENDING_RESOURCE_MAX_BYTES);
        store
            .insert("overflow".into(), vec![1])
            .expect_err("global byte overflow must fail");
        assert_eq!(store.retained_bytes, PENDING_RESOURCE_MAX_BYTES);
        assert!(!store.entries.contains_key("overflow"));
        assert_eq!(store.rejected, 1);
    }

    fn pending_upload(identity: &[u8], accepted_at: u64) -> PendingUpload {
        PendingUpload {
            identity_hash: identity.to_vec(),
            room_id: 1,
            user_id: 1,
            filename: "upload.bin".into(),
            content_type: Some("application/octet-stream".into()),
            incoming_bytes: 1,
            accepted_at,
        }
    }

    #[test]
    fn pending_upload_store_enforces_global_and_per_identity_fairness() {
        let now = 100_000;
        let mut per_identity = PendingUploadStore::default();
        for index in 0..PENDING_UPLOAD_MAX_ITEMS_PER_IDENTITY {
            assert!(per_identity.insert(
                format!("alice-{index}"),
                pending_upload(b"alice", now),
                now,
            ));
        }
        assert!(!per_identity.insert("alice-overflow".into(), pending_upload(b"alice", now), now,));
        assert!(per_identity.insert("bob".into(), pending_upload(b"bob", now), now,));
        assert!(per_identity.insert("alice-0".into(), pending_upload(b"alice", now + 1), now + 1,));
        assert_eq!(
            per_identity.metrics(now + 1),
            (PENDING_UPLOAD_MAX_ITEMS_PER_IDENTITY + 1, 2, 1, 0)
        );

        let mut global = PendingUploadStore::default();
        for index in 0..PENDING_UPLOAD_MAX_ITEMS {
            assert!(global.insert(
                format!("global-{index}"),
                pending_upload(format!("identity-{index}").as_bytes(), now),
                now,
            ));
        }
        assert!(!global.insert(
            "global-overflow".into(),
            pending_upload(b"another-identity", now),
            now,
        ));
        assert_eq!(global.metrics(now), (PENDING_UPLOAD_MAX_ITEMS, 256, 1, 0));
    }

    #[test]
    fn pending_upload_store_preserves_owner_on_mismatch_and_expires_stale_offers() {
        let now = 200_000;
        let mut store = PendingUploadStore::default();
        assert!(store.insert("owned".into(), pending_upload(b"alice", now), now,));
        assert!(matches!(
            store.take_for_identity("owned", b"mallory", now),
            PendingUploadTake::IdentityMismatch
        ));
        assert_eq!(store.metrics(now), (1, 1, 0, 0));
        assert!(matches!(
            store.take_for_identity("owned", b"alice", now),
            PendingUploadTake::Found(_)
        ));
        assert_eq!(store.metrics(now), (0, 0, 0, 0));

        assert!(store.insert("expired".into(), pending_upload(b"alice", now), now,));
        assert_eq!(
            store.metrics(now + PENDING_UPLOAD_TTL_SECONDS),
            (0, 0, 0, 1)
        );
        assert!(matches!(
            store.take_for_identity("expired", b"alice", now + PENDING_UPLOAD_TTL_SECONDS),
            PendingUploadTake::NotFound
        ));
    }

    #[test]
    fn upload_offer_rejects_metadata_and_pending_identity_overload() {
        let root = temp_upload_root("pending-admission");
        let _ = std::fs::remove_dir_all(&root);
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_commands_per_minute: 0,
                upload_quota_bytes: 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        join_lobby(&engine, &peer);

        let oversized = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    10,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("x".repeat(UPLOAD_FILENAME_MAX_BYTES + 1)),
                        FrameValue::U64(1),
                    ]),
                ),
            )
            .expect("oversized metadata response");
        assert_eq!(oversized[0].op, ChatOp::UploadReject);

        for index in 0..PENDING_UPLOAD_MAX_ITEMS_PER_IDENTITY {
            let response = engine
                .handle_frame(
                    &peer,
                    Frame::new(
                        ChatOp::UploadOffer,
                        20 + index as u32,
                        Some(1),
                        FrameBody::Fields(vec![
                            FrameValue::String(format!("upload-{index}.bin")),
                            FrameValue::U64(1),
                        ]),
                    ),
                )
                .expect("pending upload offer");
            assert_eq!(response[0].op, ChatOp::UploadAccept);
        }
        let rejected = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    99,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("one-too-many.bin".into()),
                        FrameValue::U64(1),
                    ]),
                ),
            )
            .expect("overload response");
        assert_eq!(rejected[0].op, ChatOp::UploadReject);
        assert_eq!(
            engine
                .pending_upload_metrics()
                .expect("pending upload metrics"),
            (PENDING_UPLOAD_MAX_ITEMS_PER_IDENTITY, 1, 1, 0)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upload_offer_accepts_when_quota_policy_allows_it() {
        let root = temp_upload_root("accept");
        let _ = std::fs::remove_dir_all(&root);
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                upload_quota_bytes: 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        join_lobby(&engine, &peer);

        let response = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(512),
                        FrameValue::String("image/png".into()),
                    ]),
                ),
            )
            .expect("upload offer");

        assert_eq!(response[0].op, ChatOp::UploadAccept);
        let FrameBody::Fields(fields) = &response[0].body else {
            panic!("upload accept fields");
        };
        assert!(
            matches!(fields.first(), Some(FrameValue::String(value)) if value.starts_with("upload:1:"))
        );
        assert_eq!(fields.get(1), Some(&FrameValue::U64(1024)));
        assert_eq!(fields.get(2), Some(&FrameValue::U64(512)));
        assert_eq!(fields.get(3), Some(&FrameValue::U64(0)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn accepted_upload_resource_is_stored_and_announced_to_room() {
        let root = temp_upload_root("complete");
        let _ = std::fs::remove_dir_all(&root);
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                upload_quota_bytes: 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        join_lobby(&engine, &peer);
        let accepted = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(4),
                        FrameValue::String("image/png".into()),
                    ]),
                ),
            )
            .expect("upload offer");
        let FrameBody::Fields(fields) = &accepted[0].body else {
            panic!("upload accept fields");
        };
        let FrameValue::String(resource_id) = &fields[0] else {
            panic!("resource id");
        };

        let mismatched = engine
            .handle_upload_resource(
                &ServerPeer {
                    identity_hash: b"other-peer".to_vec(),
                    display_name: "Mallory".into(),
                    lxmf_destination: None,
                },
                resource_id,
                b"data".to_vec(),
            )
            .expect("identity mismatch response");
        assert_eq!(mismatched[0].op, ChatOp::UploadReject);
        assert_eq!(
            engine
                .pending_upload_metrics()
                .expect("pending upload metrics"),
            (1, 1, 0, 0)
        );

        let complete = engine
            .handle_upload_resource(&peer, resource_id, b"data".to_vec())
            .expect("upload resource");

        assert_eq!(complete[0].op, ChatOp::UploadComplete);
        assert_eq!(complete[1].op, ChatOp::RoomEvent);
        let identity_dir = root.join("706565722d61");
        assert!(identity_dir.join("image.png").exists());
        assert!(matches!(
            &complete[1].body,
            FrameBody::Fields(fields)
                if matches!(&fields[0], FrameValue::Array(event_fields)
                    if event_fields.get(1) == Some(&FrameValue::U64(5))
                        && event_fields.get(4) == Some(&FrameValue::String("uploaded image.png (4 B)".into()))
                        && event_fields.get(7) == Some(&FrameValue::String("image.png".into()))
                        && event_fields.get(8) == Some(&FrameValue::U64(4)))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stored_upload_resource_can_be_fetched_by_joined_room_member() {
        let root = temp_upload_root("fetch");
        let _ = std::fs::remove_dir_all(&root);
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                upload_quota_bytes: 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        join_lobby(&engine, &peer);
        let accepted = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(4),
                        FrameValue::String("image/png".into()),
                    ]),
                ),
            )
            .expect("upload offer");
        let FrameBody::Fields(fields) = &accepted[0].body else {
            panic!("upload accept fields");
        };
        let FrameValue::String(resource_id) = &fields[0] else {
            panic!("resource id");
        };
        engine
            .handle_upload_resource(&peer, resource_id, b"data".to_vec())
            .expect("upload resource");

        let fetched = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadFetch,
                    4,
                    Some(1),
                    FrameBody::Fields(vec![FrameValue::String(resource_id.clone())]),
                ),
            )
            .expect("upload fetch");

        assert_eq!(fetched[0].op, ChatOp::UploadInlineChunk);
        assert!(matches!(
            &fetched[0].body,
            FrameBody::Fields(fields)
                if fields.first() == Some(&FrameValue::String(resource_id.clone()))
                    && fields.get(1) == Some(&FrameValue::String("image.png".into()))
                    && fields.get(2) == Some(&FrameValue::U64(4))
                    && fields.get(3) == Some(&FrameValue::String("image/png".into()))
                    && fields.get(4) == Some(&FrameValue::U64(0))
                    && fields.get(5) == Some(&FrameValue::Bytes(b"data".to_vec()))
                    && fields.get(6) == Some(&FrameValue::Bool(true))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stored_image_sized_upload_fetch_uses_resource_offer() {
        let root = temp_upload_root("fetch-resource");
        let _ = std::fs::remove_dir_all(&root);
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                upload_quota_bytes: 512 * 1024,
                upload_max_file_bytes: 512 * 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        join_lobby(&engine, &peer);
        let payload = vec![0x51; UPLOAD_INLINE_MAX_BYTES + 1];
        let accepted = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(payload.len() as u64),
                        FrameValue::String("image/png".into()),
                    ]),
                ),
            )
            .expect("upload offer");
        let FrameBody::Fields(fields) = &accepted[0].body else {
            panic!("upload accept fields");
        };
        let FrameValue::String(resource_id) = &fields[0] else {
            panic!("resource id");
        };
        engine
            .handle_upload_resource(&peer, resource_id, payload.clone())
            .expect("upload resource");

        let fetched = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadFetch,
                    4,
                    Some(1),
                    FrameBody::Fields(vec![FrameValue::String(resource_id.clone())]),
                ),
            )
            .expect("upload fetch");

        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].op, ChatOp::UploadResourceOffer);
        assert!(matches!(
            &fetched[0].body,
            FrameBody::Fields(fields)
                if fields.first() == Some(&FrameValue::String(resource_id.clone()))
                    && fields.get(1) == Some(&FrameValue::String("image.png".into()))
                    && fields.get(2) == Some(&FrameValue::U64(payload.len() as u64))
                    && fields.get(3) == Some(&FrameValue::String("image/png".into()))
        ));
        assert_eq!(
            engine
                .resource_payload(resource_id)
                .expect("resource payload")
                .as_deref(),
            Some(payload.as_slice())
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upload_offer_rejects_disabled_quota_and_max_file_violations() {
        let root = temp_upload_root("reject");
        let _ = std::fs::remove_dir_all(&root);
        let peer = peer();
        let disabled = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                upload_quota_bytes: 0,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        join_lobby(&disabled, &peer);

        let disabled_response = disabled
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(1),
                    ]),
                ),
            )
            .expect("disabled upload offer");
        assert_eq!(disabled_response[0].op, ChatOp::UploadReject);
        assert!(matches!(
            &disabled_response[0].body,
            FrameBody::Fields(fields)
                if fields.first() == Some(&FrameValue::String("uploads are disabled by server policy".into()))
        ));

        let over_quota = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                upload_quota_bytes: 10,
                upload_max_file_bytes: 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        join_lobby(&over_quota, &peer);
        let over_quota_response = over_quota
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(11),
                    ]),
                ),
            )
            .expect("oversized upload offer");
        assert_eq!(over_quota_response[0].op, ChatOp::UploadReject);
        assert!(matches!(
            &over_quota_response[0].body,
            FrameBody::Fields(fields)
                if fields.first() == Some(&FrameValue::String("upload exceeds server quota".into()))
                    && fields.get(1) == Some(&FrameValue::U64(10))
                    && fields.get(2) == Some(&FrameValue::U64(11))
        ));

        let over_file_limit = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                upload_quota_bytes: 50 * 1024 * 1024,
                upload_max_file_bytes: 10,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        join_lobby(&over_file_limit, &peer);
        let over_file_limit_response = over_file_limit
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::UploadOffer,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![
                        FrameValue::String("image.png".into()),
                        FrameValue::U64(11),
                    ]),
                ),
            )
            .expect("over file limit upload offer");
        assert_eq!(over_file_limit_response[0].op, ChatOp::UploadReject);
        assert!(matches!(
            &over_file_limit_response[0].body,
            FrameBody::Fields(fields)
                if fields.first() == Some(&FrameValue::String("upload exceeds server file size limit".into()))
                    && fields.get(1) == Some(&FrameValue::U64(10))
                    && fields.get(2) == Some(&FrameValue::U64(11))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn session_open_join_message_and_history_flow() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                history_batch_size: 10,
                join_backlog_events: 10,
                large_batch_threshold_bytes: 4096,
                ..SessionLimits::default()
            },
        );
        let peer = peer();

        let opened = engine
            .handle_frame(
                &peer,
                Frame::new(ChatOp::SessionOpen, 1, None, FrameBody::Empty),
            )
            .expect("session open");
        assert_eq!(opened[0].op, ChatOp::SessionAccept);
        let FrameBody::Fields(opened_fields) = &opened[0].body else {
            panic!("session accept fields");
        };
        assert_eq!(
            opened_fields.get(2),
            Some(&FrameValue::String("Welcome to OMENchat".into()))
        );
        assert_eq!(
            opened_fields.get(3),
            Some(&FrameValue::U64(50 * 1024 * 1024))
        );
        assert_eq!(opened_fields.get(4), Some(&FrameValue::U64(30)));
        assert_eq!(opened_fields.get(5), Some(&FrameValue::U64(512 * 1024)));

        let joined = engine
            .handle_frame(
                &peer,
                Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
            )
            .expect("join room");
        assert_eq!(joined[0].op, ChatOp::JoinAccept);
        assert_eq!(joined[1].op, ChatOp::UserListSnapshotInline);
        assert_eq!(joined.len(), 2);

        let room_id = joined[0].room_id.expect("room id");
        let message = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::RoomMessage,
                    3,
                    Some(room_id),
                    FrameBody::Text("hello room".into()),
                ),
            )
            .expect("message");
        assert_eq!(message[0].op, ChatOp::RoomEvent);

        let action = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::RoomAction,
                    4,
                    Some(room_id),
                    FrameBody::Text("waves".into()),
                ),
            )
            .expect("action");
        assert_eq!(action[0].op, ChatOp::RoomEvent);

        let history = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::HistoryBefore,
                    5,
                    Some(room_id),
                    FrameBody::Fields(vec![FrameValue::U64(999)]),
                ),
            )
            .expect("history");
        assert_eq!(history[0].op, ChatOp::HistoryInline);

        let history_end = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::HistoryBefore,
                    6,
                    Some(room_id),
                    FrameBody::Fields(vec![FrameValue::U64(1)]),
                ),
            )
            .expect("history end");
        assert_eq!(history_end[0].op, ChatOp::HistoryEnd);
    }

    #[test]
    fn unsupported_durable_capability_request_keeps_legacy_session_accept() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Fields(vec![
                FrameValue::String(PROTOCOL_NAME.into()),
                FrameValue::String("Alice".into()),
                FrameValue::String("lxmf-a".into()),
            ]),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![crate::protocol::DURABLE_MUTATION_CAPABILITY.into()],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([9; 16])),
            },
        )
        .expect("extended session open");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 1, None, request))
            .expect("session open");
        assert_eq!(response.len(), 1);
        assert_eq!(response[0].op, ChatOp::SessionAccept);
        let FrameBody::Fields(fields) = &response[0].body else {
            panic!("session accept fields");
        };
        assert_eq!(fields.len(), 6);
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(None)
        );
    }

    #[test]
    fn unknown_capability_request_keeps_legacy_session_accept() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec!["future-unknown-capability".into()],
                client_instance_id: None,
            },
        )
        .expect("extended session open");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 2, None, request))
            .expect("session open");
        assert_eq!(response.len(), 1);
        assert_eq!(response[0].op, ChatOp::SessionAccept);
        assert_eq!(response[0].seq, 2);
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(None)
        );
    }

    #[test]
    fn malformed_capability_request_is_rejected_without_session_accept() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::String("Alice".into()),
            FrameValue::Nil,
            FrameValue::Array(vec![FrameValue::String(
                crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
            )]),
            FrameValue::Bytes(vec![9; 15]),
        ]);

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 3, None, request))
            .expect("malformed negotiation response");
        assert_eq!(response.len(), 1);
        assert_eq!(response[0].op, ChatOp::Error);
        assert_eq!(response[0].seq, 3);
        assert_eq!(
            response[0].body,
            FrameBody::Fields(vec![
                FrameValue::U64(ChatErrorCode::DurableMutationMalformed as u16 as u64),
                FrameValue::String("invalid session capability negotiation".into()),
            ])
        );
    }

    #[test]
    fn history_before_requires_room_membership() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        store
            .append_event(
                room.room_id,
                None,
                ServerRoomEventKind::System {
                    body: "private history".into(),
                },
            )
            .expect("append event");
        let engine = SessionEngine::new(store);

        let rejected = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    8,
                    Some(room.room_id),
                    FrameBody::Fields(vec![FrameValue::U64(999)]),
                ),
            )
            .expect("history");

        assert_eq!(rejected[0].op, ChatOp::Error);
        assert_eq!(
            rejected[0].body,
            FrameBody::Fields(vec![
                FrameValue::U64(ChatErrorCode::NotJoined as u16 as u64),
                FrameValue::String("join the room before requesting history".into()),
            ])
        );
    }

    #[test]
    fn history_recent_returns_current_when_client_fingerprint_matches() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                history_batch_size: 10,
                join_backlog_events: 10,
                large_batch_threshold_bytes: 4096,
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        let joined = engine
            .handle_frame(
                &peer,
                Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
            )
            .expect("join room");
        let room_id = joined[0].room_id.expect("room id");
        engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::RoomMessage,
                    3,
                    Some(room_id),
                    FrameBody::Text("hello room".into()),
                ),
            )
            .expect("message");
        let recent = engine
            .store
            .latest_events(room_id, 10)
            .expect("latest history");
        let fingerprint = server_history_fingerprint(&recent);

        let synced = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::HistoryRecent,
                    4,
                    Some(room_id),
                    FrameBody::Fields(vec![
                        FrameValue::U64(fingerprint.first_event_id),
                        FrameValue::U64(fingerprint.last_event_id),
                        FrameValue::U64(fingerprint.event_count),
                        FrameValue::U64(fingerprint.checksum),
                    ]),
                ),
            )
            .expect("recent sync");

        assert_eq!(synced[0].op, ChatOp::HistoryCurrent);
    }

    #[test]
    fn history_recent_returns_bounded_backlog_when_client_fingerprint_differs() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                history_batch_size: 2,
                join_backlog_events: 2,
                large_batch_threshold_bytes: 4096,
                ..SessionLimits::default()
            },
        );
        let peer = peer();
        let joined = engine
            .handle_frame(
                &peer,
                Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
            )
            .expect("join room");
        let room_id = joined[0].room_id.expect("room id");
        for (seq, body) in [(3, "one"), (4, "two"), (5, "three")] {
            engine
                .handle_frame(
                    &peer,
                    Frame::new(
                        ChatOp::RoomMessage,
                        seq,
                        Some(room_id),
                        FrameBody::Text(body.into()),
                    ),
                )
                .expect("message");
        }

        let history = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::HistoryRecent,
                    6,
                    Some(room_id),
                    FrameBody::Fields(vec![
                        FrameValue::U64(1),
                        FrameValue::U64(1),
                        FrameValue::U64(1),
                        FrameValue::U64(1),
                    ]),
                ),
            )
            .expect("recent sync");

        assert_eq!(history[0].op, ChatOp::HistoryInline);
        let values = decode_compressed_values_body(&history[0].body).expect("history payload");
        let event_ids = values
            .iter()
            .filter_map(|value| match value {
                FrameValue::Array(fields) => fields.first(),
                _ => None,
            })
            .filter_map(|value| match value {
                FrameValue::U64(value) => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(event_ids, vec![2, 3]);
    }

    #[test]
    fn part_room_removes_membership_and_returns_room_result() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let engine = SessionEngine::new(store);

        let parted = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::PartRoom, 7, Some(room.room_id), FrameBody::Empty),
            )
            .expect("part");

        assert_eq!(parted[0].op, ChatOp::CommandResult);
        assert_eq!(parted[1].op, ChatOp::RoomEvent);
        let FrameBody::Fields(values) = &parted[0].body else {
            panic!("part should return command result fields");
        };
        assert_eq!(values.first(), Some(&FrameValue::String("part".into())));
        let users = engine
            .store
            .users_for_room(room.room_id)
            .expect("room users");
        assert!(users.is_empty());
    }

    #[test]
    fn command_rooms_returns_current_room_catalog() {
        let store = OmenchatStore::in_memory().expect("store");
        store
            .ensure_room("ops", Some("Operations"))
            .expect("add room");
        let engine = SessionEngine::new(store);

        let response = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::Command, 9, None, FrameBody::Text("rooms".into())),
            )
            .expect("rooms command");

        assert_eq!(response.len(), 1);
        assert_eq!(response[0].op, ChatOp::CommandResult);
        let FrameBody::Fields(values) = &response[0].body else {
            panic!("rooms command should return fields");
        };
        assert_eq!(values.first(), Some(&FrameValue::String("rooms".into())));
        let Some(FrameValue::Array(rooms)) = values.get(1) else {
            panic!("rooms command should return room array");
        };
        assert!(rooms.iter().any(|room| {
            matches!(
                room,
                FrameValue::Array(fields)
                    if fields.get(1) == Some(&FrameValue::String("lobby".into()))
            )
        }));
        assert!(rooms.iter().any(|room| {
            matches!(
                room,
                FrameValue::Array(fields)
                    if fields.get(1) == Some(&FrameValue::String("ops".into()))
            )
        }));
    }

    #[test]
    fn command_topic_requires_mod_or_admin_and_updates_room() {
        let path = temp_store_path("topic");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let user = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("user");
        drop(store);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    10,
                    Some(room.room_id),
                    FrameBody::Text("topic New Topic".into()),
                ),
            )
            .expect("topic denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_MODERATOR as i64, user.user_id as i64),
            )
            .expect("moderator role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let updated = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    11,
                    Some(room.room_id),
                    FrameBody::Text("topic New Topic".into()),
                ),
            )
            .expect("topic update");
        assert_eq!(updated[0].op, ChatOp::CommandResult);
        assert_eq!(updated[1].op, ChatOp::RoomDelta);
        assert_eq!(updated[1].room_id, Some(room.room_id));
        let FrameBody::Fields(values) = &updated[0].body else {
            panic!("topic command should return fields");
        };
        assert_eq!(values.first(), Some(&FrameValue::String("topic".into())));
        assert!(matches!(
            values.get(1),
            Some(FrameValue::Array(fields))
                if fields.get(2) == Some(&FrameValue::String("New Topic".into()))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn command_create_requires_admin_and_returns_created_room() {
        let path = temp_store_path("create-room");
        let store = OmenchatStore::open(&path).expect("store");
        store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let user = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("user");
        drop(store);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    12,
                    None,
                    FrameBody::Text("create #ops Operations Desk".into()),
                ),
            )
            .expect("create denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_MODERATOR as i64, user.user_id as i64),
            )
            .expect("moderator role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let mod_denied = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    13,
                    None,
                    FrameBody::Text("create #ops Operations Desk".into()),
                ),
            )
            .expect("moderator create denied");
        assert_eq!(mod_denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_ADMIN as i64, user.user_id as i64),
            )
            .expect("admin role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let created = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    14,
                    None,
                    FrameBody::Text("create #ops Operations Desk".into()),
                ),
            )
            .expect("create room");
        assert_eq!(created[0].op, ChatOp::CommandResult);
        assert_eq!(created[1].op, ChatOp::RoomDelta);
        assert_eq!(created[1].room_id, None);
        let FrameBody::Fields(values) = &created[0].body else {
            panic!("create command should return fields");
        };
        assert_eq!(values.first(), Some(&FrameValue::String("create".into())));
        assert!(matches!(
            values.get(1),
            Some(FrameValue::Array(fields))
                if fields.get(1) == Some(&FrameValue::String("ops".into()))
                    && fields.get(2) == Some(&FrameValue::String("Operations Desk".into()))
        ));
        assert!(matches!(
            &created[1].body,
            FrameBody::Fields(values)
                if matches!(values.first(), Some(FrameValue::Array(fields))
                    if fields.get(1) == Some(&FrameValue::String("ops".into())))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn command_kick_and_ban_require_mod_or_admin() {
        let path = temp_store_path("moderation");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let actor = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("actor");
        store
            .ensure_user(b"peer-b", "Bob", Some("lxmf-b"))
            .expect("target");
        drop(store);

        let active = vec![
            peer(),
            ServerPeer {
                identity_hash: b"peer-b".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: Some("lxmf-b".into()),
            },
        ];
        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame_with_active_peers(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    15,
                    Some(room.room_id),
                    FrameBody::Text("kick Bob".into()),
                ),
                &active,
            )
            .expect("kick denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_MODERATOR as i64, actor.user_id as i64),
            )
            .expect("moderator role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let kicked = engine
            .handle_frame_with_active_peers(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    16,
                    Some(room.room_id),
                    FrameBody::Text("kick Bob".into()),
                ),
                &active,
            )
            .expect("kick");
        assert_eq!(kicked[0].op, ChatOp::CommandResult);
        assert_eq!(kicked[1].op, ChatOp::UserDelta);
        assert_eq!(kicked[2].op, ChatOp::RoomEvent);
        let FrameBody::Fields(values) = &kicked[2].body else {
            panic!("kick should append a room event");
        };
        assert!(matches!(
            values.first(),
            Some(FrameValue::Array(fields))
                if fields.get(4) == Some(&FrameValue::String("Alice kicked Bob".into()))
        ));

        let banned = engine
            .handle_frame_with_active_peers(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    17,
                    Some(room.room_id),
                    FrameBody::Text("ban Bob".into()),
                ),
                &active,
            )
            .expect("ban");
        assert_eq!(banned[0].op, ChatOp::CommandResult);
        assert_eq!(banned[1].op, ChatOp::UserDelta);
        assert_eq!(banned[2].op, ChatOp::RoomEvent);
        let store = OmenchatStore::open(&path).expect("store");
        let bob = store
            .user_by_identity(b"peer-b")
            .expect("bob query")
            .expect("bob");
        assert_ne!(bob.status_bits & STATUS_BANNED, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn command_unban_requires_admin_and_known_target() {
        let path = temp_store_path("unban");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let actor = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("actor");
        let bob = store
            .ensure_user(b"peer-b", "Bob", Some("lxmf-b"))
            .expect("target");
        store
            .set_user_status_flag(bob.user_id, STATUS_BANNED, true)
            .expect("ban bob");
        drop(store);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    18,
                    Some(room.room_id),
                    FrameBody::Text("unban Bob".into()),
                ),
            )
            .expect("unban denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_ADMIN as i64, actor.user_id as i64),
            )
            .expect("admin role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let unbanned = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    19,
                    Some(room.room_id),
                    FrameBody::Text("unban Bob".into()),
                ),
            )
            .expect("unban");
        assert_eq!(unbanned[0].op, ChatOp::CommandResult);
        assert_eq!(unbanned[1].op, ChatOp::UserDelta);
        assert_eq!(unbanned[2].op, ChatOp::RoomEvent);
        let FrameBody::Fields(values) = &unbanned[2].body else {
            panic!("unban should append a room event");
        };
        assert!(matches!(
            values.first(),
            Some(FrameValue::Array(fields))
                if fields.get(4) == Some(&FrameValue::String("Alice unbanned Bob".into()))
        ));

        let store = OmenchatStore::open(&path).expect("store");
        let bob = store
            .user_by_identity(b"peer-b")
            .expect("bob query")
            .expect("bob");
        assert_eq!(bob.status_bits & STATUS_BANNED, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn command_mute_and_unmute_require_mod_or_admin() {
        let path = temp_store_path("mute-command");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let actor = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("actor");
        store
            .ensure_user(b"peer-b", "Bob", Some("lxmf-b"))
            .expect("target");
        drop(store);

        let active = vec![
            peer(),
            ServerPeer {
                identity_hash: b"peer-b".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: Some("lxmf-b".into()),
            },
        ];
        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame_with_active_peers(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    20,
                    Some(room.room_id),
                    FrameBody::Text("mute Bob".into()),
                ),
                &active,
            )
            .expect("mute denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_MODERATOR as i64, actor.user_id as i64),
            )
            .expect("moderator role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let muted = engine
            .handle_frame_with_active_peers(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    21,
                    Some(room.room_id),
                    FrameBody::Text("mute Bob".into()),
                ),
                &active,
            )
            .expect("mute");
        assert_eq!(muted[0].op, ChatOp::CommandResult);
        assert_eq!(muted[1].op, ChatOp::UserDelta);
        assert_eq!(muted[2].op, ChatOp::RoomEvent);
        let store = OmenchatStore::open(&path).expect("store");
        let bob = store
            .user_by_identity(b"peer-b")
            .expect("bob query")
            .expect("bob");
        assert_ne!(bob.status_bits & STATUS_MUTED, 0);
        drop(store);

        let unmuted = engine
            .handle_frame_with_active_peers(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    22,
                    Some(room.room_id),
                    FrameBody::Text("unmute Bob".into()),
                ),
                &active,
            )
            .expect("unmute");
        assert_eq!(unmuted[0].op, ChatOp::CommandResult);
        assert_eq!(unmuted[1].op, ChatOp::UserDelta);
        assert_eq!(unmuted[2].op, ChatOp::RoomEvent);
        let FrameBody::Fields(values) = &unmuted[2].body else {
            panic!("unmute should append a room event");
        };
        assert!(matches!(
            values.first(),
            Some(FrameValue::Array(fields))
                if fields.get(4) == Some(&FrameValue::String("Alice unmuted Bob".into()))
        ));
        let store = OmenchatStore::open(&path).expect("store");
        let bob = store
            .user_by_identity(b"peer-b")
            .expect("bob query")
            .expect("bob");
        assert_eq!(bob.status_bits & STATUS_MUTED, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn command_role_requires_admin_and_updates_known_target() {
        let path = temp_store_path("role-command");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let actor = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("actor");
        store
            .ensure_user(b"peer-b", "Bob", Some("lxmf-b"))
            .expect("target");
        drop(store);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    23,
                    Some(room.room_id),
                    FrameBody::Text("role Bob mod".into()),
                ),
            )
            .expect("role denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (
                    (ROLE_TRUSTED | ROLE_MODERATOR | ROLE_ADMIN) as i64,
                    actor.user_id as i64,
                ),
            )
            .expect("admin role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let updated = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::Command,
                    24,
                    Some(room.room_id),
                    FrameBody::Text("role Bob mod".into()),
                ),
            )
            .expect("role update");
        assert_eq!(updated[0].op, ChatOp::CommandResult);
        assert_eq!(updated[1].op, ChatOp::UserDelta);
        assert_eq!(updated[2].op, ChatOp::RoomEvent);
        let FrameBody::Fields(values) = &updated[2].body else {
            panic!("role should append a room event");
        };
        assert!(matches!(
            values.first(),
            Some(FrameValue::Array(fields))
                if fields.get(4) == Some(&FrameValue::String("Alice set Bob role to mod".into()))
        ));

        let store = OmenchatStore::open(&path).expect("store");
        let bob = store
            .user_by_identity(b"peer-b")
            .expect("bob query")
            .expect("bob");
        assert_eq!(bob.role_bits, ROLE_TRUSTED | ROLE_MODERATOR);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn room_notice_requires_mod_or_admin() {
        let path = temp_store_path("room-notice");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let actor = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("actor");
        drop(store);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let denied = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomNotice,
                    25,
                    Some(room.room_id),
                    FrameBody::Text("maintenance soon".into()),
                ),
            )
            .expect("notice denied");
        assert_eq!(denied[0].op, ChatOp::Error);

        drop(engine);
        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_MODERATOR as i64, actor.user_id as i64),
            )
            .expect("moderator role");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let notice = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomNotice,
                    26,
                    Some(room.room_id),
                    FrameBody::Text("maintenance soon".into()),
                ),
            )
            .expect("notice");
        assert_eq!(notice[0].op, ChatOp::RoomEvent);
        let FrameBody::Fields(values) = &notice[0].body else {
            panic!("notice should append a room event");
        };
        assert!(matches!(
            values.first(),
            Some(FrameValue::Array(fields))
                if fields.get(1) == Some(&FrameValue::U64(3))
                    && fields.get(4) == Some(&FrameValue::String("maintenance soon".into()))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn room_message_and_notice_reject_oversized_bodies() {
        let path = temp_store_path("message-size");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let actor = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("actor");
        drop(store);

        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET role_bits = ?1 WHERE user_id = ?2",
                (ROLE_MODERATOR as i64, actor.user_id as i64),
            )
            .expect("moderator role");
        drop(connection);

        let engine = SessionEngine::with_limits(
            OmenchatStore::open(&path).expect("store"),
            SessionLimits {
                max_message_bytes: 4,
                ..SessionLimits::default()
            },
        );
        let oversized_message = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomMessage,
                    27,
                    Some(room.room_id),
                    FrameBody::Text("12345".into()),
                ),
            )
            .expect("oversized message");
        assert_eq!(oversized_message[0].op, ChatOp::Error);

        let oversized_notice = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomNotice,
                    28,
                    Some(room.room_id),
                    FrameBody::Text("12345".into()),
                ),
            )
            .expect("oversized notice");
        assert_eq!(oversized_notice[0].op, ChatOp::Error);

        let allowed = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomMessage,
                    29,
                    Some(room.room_id),
                    FrameBody::Text("1234".into()),
                ),
            )
            .expect("allowed message");
        assert_eq!(allowed[0].op, ChatOp::RoomEvent);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn room_message_rate_limit_rejects_excess_messages() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                rate_messages_per_minute: 2,
                ..SessionLimits::default()
            },
        );
        let peer = peer();

        for seq in 1..=2 {
            let response = engine
                .handle_frame(
                    &peer,
                    Frame::new(
                        ChatOp::RoomMessage,
                        seq,
                        Some(room.room_id),
                        FrameBody::Text(format!("allowed {seq}")),
                    ),
                )
                .expect("message");
            assert_eq!(response[0].op, ChatOp::RoomEvent);
        }

        let rejected = engine
            .handle_frame(
                &peer,
                Frame::new(
                    ChatOp::RoomMessage,
                    3,
                    Some(room.room_id),
                    FrameBody::Text("blocked".into()),
                ),
            )
            .expect("message");
        assert_eq!(rejected[0].op, ChatOp::Error);
        assert_eq!(
            rejected[0].body,
            FrameBody::Fields(vec![
                FrameValue::U64(ChatErrorCode::RateLimited as u16 as u64),
                FrameValue::String("message rate limit exceeded".into()),
            ])
        );
    }

    #[test]
    fn durable_room_text_replays_exact_ack_without_rate_or_broadcast_repetition() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                rate_messages_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let client_instance_id = ClientInstanceId::new([8; 16]);
        let envelope = durable_envelope(ChatOp::RoomMessage, room.room_id, 1, "once");

        let stored = engine
            .handle_durable_room_text(
                &peer(),
                11,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable message");
        assert_eq!(stored.origin.op, ChatOp::MessageAck);
        assert_eq!(stored.origin.seq, 11);
        assert_eq!(
            stored.broadcast.as_ref().map(|frame| frame.op),
            Some(ChatOp::RoomEvent)
        );

        let replayed = engine
            .handle_durable_room_text(
                &peer(),
                12,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable message");
        assert_eq!(replayed.origin, stored.origin);
        assert!(replayed.broadcast.is_none());

        let rate_limited = engine
            .handle_durable_room_text(
                &peer(),
                13,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room.room_id, 2, "new mutation"),
            )
            .expect("second logical mutation");
        assert_eq!(rate_limited.origin.op, ChatOp::Error);
        assert_eq!(
            frame_error_code(&rate_limited.origin),
            Some(ChatErrorCode::RateLimited as u16 as u64)
        );
        assert!(rate_limited.broadcast.is_none());
        assert_eq!(
            engine
                .store
                .latest_events(room.room_id, 10)
                .expect("events")
                .len(),
            1
        );
    }

    #[test]
    fn durable_room_text_rejects_hash_conflict_and_malformed_hash_without_mutation() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([9; 16]);
        let first = durable_envelope(ChatOp::RoomAction, room.room_id, 3, "waves");
        engine
            .handle_durable_room_text(
                &peer(),
                21,
                Some(room.room_id),
                ChatOp::RoomAction,
                client_instance_id,
                first,
            )
            .expect("first action");

        let conflict = engine
            .handle_durable_room_text(
                &peer(),
                22,
                Some(room.room_id),
                ChatOp::RoomAction,
                client_instance_id,
                durable_envelope(ChatOp::RoomAction, room.room_id, 3, "different"),
            )
            .expect("conflict");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcast.is_none());

        let mut malformed = durable_envelope(ChatOp::RoomAction, room.room_id, 4, "invalid hash");
        malformed.request_hash = crate::protocol::RequestHash::new([0; 32]);
        let malformed = engine
            .handle_durable_room_text(
                &peer(),
                23,
                Some(room.room_id),
                ChatOp::RoomAction,
                client_instance_id,
                malformed,
            )
            .expect("malformed hash");
        assert_eq!(
            frame_error_code(&malformed.origin),
            Some(ChatErrorCode::DurableMutationMalformed as u16 as u64)
        );
        assert_eq!(
            engine
                .store
                .latest_events(room.room_id, 10)
                .expect("events")
                .len(),
            1
        );
    }

    #[test]
    fn durable_room_text_replays_original_permission_result_after_policy_changes() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store
            .set_user_status_flag(user.user_id, STATUS_MUTED, true)
            .expect("mute");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([10; 16]);
        let envelope = durable_envelope(ChatOp::RoomMessage, room.room_id, 5, "blocked once");
        let rejected = engine
            .handle_durable_room_text(
                &peer(),
                31,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored rejection");
        assert_eq!(
            frame_error_code(&rejected.origin),
            Some(ChatErrorCode::PermissionDenied as u16 as u64)
        );
        engine
            .store
            .set_user_status_flag(user.user_id, STATUS_MUTED, false)
            .expect("unmute");

        let replayed = engine
            .handle_durable_room_text(
                &peer(),
                32,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope,
            )
            .expect("replayed rejection");
        assert_eq!(replayed.origin, rejected.origin);
        assert!(replayed.broadcast.is_none());
        assert!(engine
            .store
            .latest_events(room.room_id, 10)
            .expect("events")
            .is_empty());
    }

    #[test]
    fn durable_room_text_replays_after_server_restart_without_new_event() {
        let path = temp_store_path("durable-restart");
        let client_instance_id = ClientInstanceId::new([11; 16]);
        let (room_id, envelope, original) = {
            let store = OmenchatStore::open(&path).expect("persistent store");
            let room = store.ensure_room("lobby", None).expect("room");
            let envelope = durable_envelope(ChatOp::RoomMessage, room.room_id, 6, "restart once");
            let engine = SessionEngine::new(store);
            let original = engine
                .handle_durable_room_text(
                    &peer(),
                    41,
                    Some(room.room_id),
                    ChatOp::RoomMessage,
                    client_instance_id,
                    envelope.clone(),
                )
                .expect("stored before restart");
            assert!(original.broadcast.is_some());
            (room.room_id, envelope, original.origin)
        };

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed = engine
            .handle_durable_room_text(
                &peer(),
                42,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope,
            )
            .expect("replayed after restart");
        assert_eq!(replayed.origin, original);
        assert!(replayed.broadcast.is_none());
        assert_eq!(
            engine
                .store
                .latest_events(room_id, 10)
                .expect("events")
                .len(),
            1
        );
        drop(engine);
        for candidate in [
            path.clone(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn rate_reservation_rolls_back_uncommitted_admission() {
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_messages_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let peer = peer();

        let reservation = match engine
            .reserve_rate(&peer, RateKind::Message)
            .expect("first admission")
        {
            RateAdmission::Admitted(Some(reservation)) => reservation,
            _ => panic!("limited admission must return a reservation"),
        };
        drop(reservation);

        let replacement = match engine
            .reserve_rate(&peer, RateKind::Message)
            .expect("replacement admission")
        {
            RateAdmission::Admitted(Some(reservation)) => reservation,
            _ => panic!("rolled-back admission must release its capacity"),
        };
        replacement.commit();
        assert!(matches!(
            engine
                .reserve_rate(&peer, RateKind::Message)
                .expect("admission after commit"),
            RateAdmission::Rejected
        ));
    }

    #[test]
    fn disabled_rate_limit_needs_no_reservation() {
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_messages_per_minute: 0,
                ..SessionLimits::default()
            },
        );
        assert!(matches!(
            engine
                .reserve_rate(&peer(), RateKind::Message)
                .expect("unlimited admission"),
            RateAdmission::Admitted(None)
        ));
    }

    #[test]
    fn command_rate_limit_rejects_excess_commands() {
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_commands_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let peer = peer();

        let first = engine
            .handle_frame(
                &peer,
                Frame::new(ChatOp::Command, 1, None, FrameBody::Text("rooms".into())),
            )
            .expect("first command");
        assert_eq!(first[0].op, ChatOp::CommandResult);

        let rejected = engine
            .handle_frame(
                &peer,
                Frame::new(ChatOp::Command, 2, None, FrameBody::Text("rooms".into())),
            )
            .expect("second command");
        assert_eq!(rejected[0].op, ChatOp::Error);
        assert_eq!(
            rejected[0].body,
            FrameBody::Fields(vec![
                FrameValue::U64(ChatErrorCode::RateLimited as u16 as u64),
                FrameValue::String("command rate limit exceeded".into()),
            ])
        );
    }

    #[test]
    fn large_join_batches_return_resource_offers() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(b"seed", "Seed", Some("lxmf-seed"))
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        for index in 0..8 {
            store
                .append_event(
                    room.room_id,
                    Some(user.user_id),
                    ServerRoomEventKind::Message {
                        body: format!("large history payload {index} {}", "x".repeat(256)),
                    },
                )
                .expect("append event");
        }
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                history_batch_size: 10,
                join_backlog_events: 10,
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );

        let joined = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
            )
            .expect("join room");

        assert_eq!(joined[1].op, ChatOp::UserListSnapshotResource);
        assert_eq!(joined[2].op, ChatOp::HistoryResourceOffer);

        let offer = decode_resource_offer_body(&joined[2].body).expect("resource offer");
        let payload = engine
            .resource_payload(&offer.resource_id)
            .expect("resource lookup")
            .expect("resource payload");
        let decoded = decode_compressed_values_payload(&payload).expect("resource payload decode");
        assert_eq!(decoded.len(), 8);
        assert!(engine
            .take_resource_payload(&offer.resource_id)
            .expect("take resource")
            .is_some());
        assert!(engine
            .resource_payload(&offer.resource_id)
            .expect("resource removed")
            .is_none());
    }

    #[test]
    fn banned_users_are_denied_session_and_room_actions() {
        let path = temp_store_path("banned");
        let store = OmenchatStore::open(&path).expect("store");
        store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let user = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("user");
        drop(store);

        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET status_bits = ?1 WHERE user_id = ?2",
                (STATUS_BANNED as i64, user.user_id as i64),
            )
            .expect("ban");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let opened = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::SessionOpen, 1, None, FrameBody::Empty),
            )
            .expect("session open");
        assert_eq!(opened[0].op, ChatOp::Error);

        let joined = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
            )
            .expect("join room");
        assert_eq!(joined[0].op, ChatOp::Error);

        let history = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    3,
                    Some(1),
                    FrameBody::Fields(vec![FrameValue::U64(999)]),
                ),
            )
            .expect("history");
        assert_eq!(history[0].op, ChatOp::Error);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn muted_users_can_join_and_read_but_not_send_messages() {
        let path = temp_store_path("muted");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("room");
        let user = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("user");
        store
            .append_event(
                room.room_id,
                None,
                ServerRoomEventKind::System {
                    body: "before mute".into(),
                },
            )
            .expect("history seed");
        drop(store);

        let connection = rusqlite::Connection::open(&path).expect("db");
        connection
            .execute(
                "UPDATE users SET status_bits = ?1 WHERE user_id = ?2",
                (STATUS_MUTED as i64, user.user_id as i64),
            )
            .expect("mute");
        drop(connection);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let joined = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::JoinRoom, 1, None, FrameBody::Text("lobby".into())),
            )
            .expect("join");
        assert_eq!(joined[0].op, ChatOp::JoinAccept);

        let history = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    2,
                    Some(room.room_id),
                    FrameBody::Fields(vec![FrameValue::U64(999)]),
                ),
            )
            .expect("history");
        assert_eq!(history[0].op, ChatOp::HistoryInline);

        let message = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomMessage,
                    3,
                    Some(room.room_id),
                    FrameBody::Text("blocked".into()),
                ),
            )
            .expect("message");
        assert_eq!(message[0].op, ChatOp::Error);
        let _ = std::fs::remove_file(path);
    }
}
