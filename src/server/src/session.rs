use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::error::{ServerError, ServerResult};
use crate::protocol::batch::{
    compressed_values_body, compressed_values_payload, encoded_compressed_len, resource_offer_body,
    ResourceOffer,
};
use crate::protocol::codec::{decode_frame, encode_frame};
use crate::protocol::{
    append_rich_message_event_metadata, canonical_mutation_request_hash,
    parse_session_open_negotiation, with_session_accept_negotiation, ChatErrorCode, ChatOp,
    ClientInstanceId, Compression, DurableMutationEnvelope, Frame, FrameBody, FrameValue,
    MessageRevisionAck, MessageRevisionRequest, MessageRevisionSnapshot, ModerationAuditAction,
    ModerationAuditRequest, PinAck, PinRequest, ReactionAck, ReactionRequest, ReactionSnapshot,
    RichMessageBody, RichMessageEventMetadata, RoomCatalogEntry, RoomCatalogShape, RoomId,
    SessionAcceptNegotiation, UserId, ANNOUNCEMENT_ROOMS_CAPABILITY, DURABLE_MUTATION_CAPABILITY,
    DURABLE_NOTICE_ACK_CAPABILITY, MESSAGE_REVISIONS_CAPABILITY,
    MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS, MODERATION_AUDIT_CAPABILITY, PROTOCOL_NAME,
    REACTIONS_CAPABILITY, REACTION_SNAPSHOT_MAX_TARGETS, REPLY_MENTIONS_BODY_TAG,
    REPLY_MENTIONS_CAPABILITY, ROOM_PINS_CAPABILITY, ROOM_PIN_SNAPSHOT_MAX_TARGETS,
    ROOM_SLOW_MODE_CAPABILITY,
};
use crate::store::durable_replay::{
    DurableMutationEffectCommit, DurableMutationEffectPlan, DurableMutationKey,
    DurableRoomEventCommit, DurableRoomEventPlan,
};
use crate::store::message_revisions::{MessageRevisionActorPolicy, MessageRevisionMutationResult};
use crate::store::moderation_audit::ModerationAuditAdmission;
use crate::store::pins::PinMutationResult;
use crate::store::reactions::ReactionMutationResult;
use crate::store::slow_mode::{
    admit_room_publication, room_slow_mode_seconds, SlowModeAdmission, SlowModeRoomPublication,
};
use crate::store::{
    normalize_room_name, OmenchatStore, RoomContentMutationAdmission, ServerRoom, ServerRoomEvent,
    ServerRoomEventKind, ServerUser,
};
use crate::upload::{
    plan_upload_with_index, store_upload_with_policy_indexed_and_commit, UploadPolicy,
    UploadQuotaDecision,
};

mod slow_mode;

use slow_mode::{SlowModeMonotonicAdmission, SlowModeOwner, SlowModeReservation};

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
const REPLY_MENTIONS_SERVER_ENABLED: bool = true;
const REACTIONS_SERVER_ENABLED: bool = true;
const MESSAGE_REVISIONS_SERVER_ENABLED: bool = true;
const ROOM_PINS_SERVER_ENABLED: bool = true;
const MODERATION_AUDIT_SERVER_ENABLED: bool = cfg!(feature = "omenchat-moderation-audit");

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
    slow_mode: SlowModeOwner,
    slow_mode_enforcement_enabled: bool,
    slow_mode_capability_enabled: bool,
    moderation_audit_enabled: bool,
    announcement_rooms_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DurableMutationDispatch {
    pub origin: Frame,
    pub broadcasts: Vec<Frame>,
    pub disconnect_identity: Option<Vec<u8>>,
    pub pruned: usize,
}

pub(crate) struct DurableMutationPeerContext<'a> {
    pub peer: &'a ServerPeer,
    pub active_room_peers: &'a [ServerPeer],
    pub durable_notice_ack: bool,
    pub reply_mentions: bool,
    pub reactions: bool,
    pub message_revisions: bool,
    pub pins: bool,
}

#[derive(Clone, Copy)]
struct DurableRoomOperation {
    op: ChatOp,
    notice_ack: bool,
    reply_mentions: bool,
}

struct RoomPublicationAdmission {
    rate: Option<RateReservation>,
    slow_mode: Option<SlowModeReservation>,
}

impl RoomPublicationAdmission {
    fn commit(self) {
        if let Some(rate) = self.rate {
            rate.commit();
        }
        if let Some(slow_mode) = self.slow_mode {
            slow_mode.commit();
        }
    }
}

struct DurableCommandEffect {
    broadcasts: Vec<Frame>,
    admission: Option<RateReservation>,
    disconnect_identity: Option<Vec<u8>>,
}

struct DurableReactionEffect {
    broadcast: Option<Frame>,
    admission: Option<RateReservation>,
}

struct DurableMessageRevisionEffect {
    broadcast: Frame,
    admission: Option<RateReservation>,
}

struct DurablePinEffect {
    broadcast: Option<Frame>,
    admission: Option<RateReservation>,
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
    #[cfg(test)]
    pub(crate) fn pin_row_counts(&self) -> ServerResult<(i64, i64)> {
        self.store.pin_row_counts()
    }

    pub fn new(store: OmenchatStore) -> Self {
        Self {
            store,
            limits: SessionLimits::default(),
            server_motd: Some("Welcome to OMENchat".into()),
            pending_resources: Arc::new(Mutex::new(PendingResourceStore::default())),
            pending_uploads: Arc::new(Mutex::new(PendingUploadStore::default())),
            rate_buckets: Arc::new(Mutex::new(BTreeMap::new())),
            slow_mode: SlowModeOwner::default(),
            slow_mode_enforcement_enabled: cfg!(feature = "omenchat-slow-mode"),
            slow_mode_capability_enabled: cfg!(feature = "omenchat-slow-mode"),
            moderation_audit_enabled: MODERATION_AUDIT_SERVER_ENABLED,
            announcement_rooms_enabled: cfg!(feature = "omenchat-announcement-rooms"),
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
            slow_mode: SlowModeOwner::default(),
            slow_mode_enforcement_enabled: cfg!(feature = "omenchat-slow-mode"),
            slow_mode_capability_enabled: cfg!(feature = "omenchat-slow-mode"),
            moderation_audit_enabled: MODERATION_AUDIT_SERVER_ENABLED,
            announcement_rooms_enabled: cfg!(feature = "omenchat-announcement-rooms"),
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
            slow_mode: SlowModeOwner::default(),
            slow_mode_enforcement_enabled: cfg!(feature = "omenchat-slow-mode"),
            slow_mode_capability_enabled: cfg!(feature = "omenchat-slow-mode"),
            moderation_audit_enabled: MODERATION_AUDIT_SERVER_ENABLED,
            announcement_rooms_enabled: cfg!(feature = "omenchat-announcement-rooms"),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_moderation_audit(store: OmenchatStore, limits: SessionLimits) -> Self {
        let mut engine = Self::with_limits(store, limits);
        engine.moderation_audit_enabled = true;
        engine
    }

    #[cfg(test)]
    pub(crate) fn with_test_announcement_rooms(store: OmenchatStore) -> Self {
        let mut engine = Self::new(store);
        engine.announcement_rooms_enabled = true;
        engine
    }

    #[cfg(test)]
    pub(crate) fn with_test_slow_mode(store: OmenchatStore) -> Self {
        let mut engine = Self::new(store);
        engine.slow_mode_enforcement_enabled = true;
        engine.slow_mode_capability_enabled = true;
        engine
    }

    pub fn handle_frame(&self, peer: &ServerPeer, frame: Frame) -> ServerResult<Vec<Frame>> {
        self.handle_frame_with_active_peers(peer, frame, &[])
    }

    pub(crate) fn local_user_id(&self, peer: &ServerPeer) -> ServerResult<UserId> {
        self.ensure_peer(peer).map(|user| user.user_id)
    }

    pub(crate) fn shape_room_frame_for_catalog_shape(
        &self,
        frame: &Frame,
        room_catalog_shape: RoomCatalogShape,
    ) -> ServerResult<Frame> {
        let mut shaped = frame.clone();
        match shaped.op {
            ChatOp::JoinAccept | ChatOp::RoomDelta => {
                let FrameBody::Fields(fields) = &mut shaped.body else {
                    return Err(ServerError::Message(format!(
                        "{:?} response did not contain fields",
                        shaped.op
                    )));
                };
                let room = fields.first_mut().ok_or_else(|| {
                    ServerError::Message(format!("{:?} response omitted its room", shaped.op))
                })?;
                *room = self.authoritative_room_value(room, room_catalog_shape)?;
            }
            ChatOp::CommandResult => {
                let FrameBody::Fields(fields) = &mut shaped.body else {
                    return Ok(shaped);
                };
                let Some(FrameValue::String(command)) = fields.first() else {
                    return Ok(shaped);
                };
                let command = command.clone();
                match command.as_str() {
                    "rooms" => {
                        let Some(FrameValue::Array(rooms)) = fields.get_mut(1) else {
                            return Err(ServerError::Message(
                                "rooms command result omitted its catalog".into(),
                            ));
                        };
                        for room in rooms {
                            *room = self.authoritative_room_value(room, room_catalog_shape)?;
                        }
                    }
                    "create" | "topic" | "part" => {
                        let room = fields.get_mut(1).ok_or_else(|| {
                            ServerError::Message(format!(
                                "{command} command result omitted its room"
                            ))
                        })?;
                        *room = self.authoritative_room_value(room, room_catalog_shape)?;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(shaped)
    }

    #[cfg(feature = "omenchat-slow-mode-qualification")]
    pub(crate) fn set_slow_mode_for_qualification(
        &self,
        room_id: RoomId,
        slow_mode_seconds: u32,
    ) -> ServerResult<Option<Frame>> {
        let update = self
            .store
            .update_room_slow_mode_seconds(room_id, slow_mode_seconds)?;
        if update.previous_seconds == update.room.slow_mode_seconds {
            return Ok(None);
        }
        let seq = u32::try_from(update.room.room_revision).map_err(|_| {
            ServerError::Message(
                "room revision exceeds the qualification frame sequence boundary".into(),
            )
        })?;
        Ok(Some(Frame::new(
            ChatOp::RoomDelta,
            seq,
            Some(room_id),
            FrameBody::Fields(vec![room_to_value(&update.room)]),
        )))
    }

    fn authoritative_room_value(
        &self,
        value: &FrameValue,
        room_catalog_shape: RoomCatalogShape,
    ) -> ServerResult<FrameValue> {
        let FrameValue::Array(fields) = value else {
            return Err(ServerError::Message(
                "server-generated room value was not an array".into(),
            ));
        };
        let Some(FrameValue::U64(room_id)) = fields.first() else {
            return Err(ServerError::Message(
                "server-generated room value omitted its id".into(),
            ));
        };
        let room_id = u32::try_from(*room_id)
            .map_err(|_| ServerError::Message("server-generated room id exceeded u32".into()))?;
        let room = self
            .store
            .room_by_id(room_id)?
            .ok_or_else(|| ServerError::Message(format!("room {room_id} disappeared")))?;
        room_to_value_for_shape(&room, room_catalog_shape)
    }

    pub fn handle_frame_with_active_peers(
        &self,
        peer: &ServerPeer,
        frame: Frame,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<Vec<Frame>> {
        self.handle_frame_with_active_peers_and_moderation_audit(
            peer,
            frame,
            active_room_peers,
            false,
        )
    }

    pub(crate) fn handle_frame_with_active_peers_and_moderation_audit(
        &self,
        peer: &ServerPeer,
        frame: Frame,
        active_room_peers: &[ServerPeer],
        moderation_audit_negotiated: bool,
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
            ChatOp::ModerationAuditBefore
                if self.moderation_audit_enabled && moderation_audit_negotiated =>
            {
                self.handle_moderation_audit_before(peer, frame.seq, frame.room_id, frame.body)
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

    fn handle_moderation_audit_before(
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
                ChatErrorCode::MalformedFrame,
                "moderation audit request requires a room",
            )]);
        };
        let request = match ModerationAuditRequest::from_frame_body(&body) {
            Ok(request) => request,
            Err(error) => {
                return Ok(vec![self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::MalformedFrame,
                    &format!("invalid moderation audit request: {error}"),
                )])
            }
        };
        let Some(actor) = self.ensure_allowed_peer(peer, seq, Some(room_id))? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "user is banned",
            )]);
        };
        if actor.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "moderation audit requires moderator or administrator role",
            )]);
        }
        if !self.store.room_has_member(room_id, actor.user_id)? {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "moderation audit requires current room membership",
            )]);
        }
        if let Some(error) =
            self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Command)?
        {
            return Ok(vec![error]);
        }

        let page =
            self.store
                .moderation_audit_page(room_id, request.before_audit_id, request.limit)?;
        let reached_end = page.records.len() < usize::from(request.limit);
        let values = page.into_frame_values().map_err(|error| {
            ServerError::Message(format!("moderation audit page encode failed: {error}"))
        })?;
        let mut responses = Vec::with_capacity(2);
        if !values.is_empty() {
            let cursor = request
                .before_audit_id
                .map_or_else(|| "newest".into(), |audit_id| audit_id.to_string());
            let purpose = format!("moderation-audit:{seq}:{cursor}");
            let (op, body) = if cfg!(feature = "omenchat-moderation-audit-resource-qualification") {
                (
                    ChatOp::ModerationAuditResource,
                    self.resource_batch_body(room_id, &purpose, &values)?,
                )
            } else {
                (
                    self.batch_op(
                        ChatOp::ModerationAuditInline,
                        ChatOp::ModerationAuditResource,
                        &values,
                    )?,
                    self.batch_body(room_id, &purpose, &values)?,
                )
            };
            responses.push(Frame::new(op, seq, Some(room_id), body));
        }
        if reached_end {
            responses.push(Frame::new(
                ChatOp::ModerationAuditEnd,
                seq,
                Some(room_id),
                FrameBody::Empty,
            ));
        }
        Ok(responses)
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

    pub fn reaction_snapshot_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        target_event_ids: &[u64],
    ) -> ServerResult<Frame> {
        let snapshot = self.store.reaction_snapshot(room_id, target_event_ids)?;
        let fingerprint = reaction_snapshot_fingerprint(&snapshot);
        let FrameBody::Fields(values) = snapshot.clone().into_frame_body().map_err(|error| {
            ServerError::Message(format!("reaction snapshot encode failed: {error}"))
        })?
        else {
            return Err(ServerError::Message(
                "reaction snapshot did not produce a fields body".into(),
            ));
        };
        let purpose = format!("reactions:{seq}:{fingerprint:016x}");
        Ok(Frame::new(
            self.batch_op(
                ChatOp::ReactionSnapshotInline,
                ChatOp::ReactionSnapshotResource,
                &values,
            )?,
            seq,
            Some(room_id),
            self.batch_body(room_id, &purpose, &values)?,
        ))
    }

    pub fn latest_reaction_snapshot_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        request_op: ChatOp,
    ) -> ServerResult<Frame> {
        let limit = match request_op {
            ChatOp::JoinRoom => self.limits.join_backlog_events,
            ChatOp::HistoryRecent => self.limits.history_batch_size,
            _ => {
                return Err(ServerError::Message(
                    "reaction snapshot request does not identify a recent-history boundary".into(),
                ))
            }
        };
        let mut target_event_ids = self
            .store
            .latest_events(room_id, limit.min(REACTION_SNAPSHOT_MAX_TARGETS))?
            .into_iter()
            .map(|event| event.event_id)
            .collect::<Vec<_>>();
        target_event_ids.sort_unstable();
        target_event_ids.dedup();
        self.reaction_snapshot_frame(seq, room_id, &target_event_ids)
    }

    pub fn message_revision_snapshot_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        target_event_ids: &[u64],
    ) -> ServerResult<Frame> {
        let snapshot = self
            .store
            .message_revision_snapshot(room_id, target_event_ids)?;
        let fingerprint = message_revision_snapshot_fingerprint(&snapshot);
        let FrameBody::Fields(values) = snapshot.clone().into_frame_body().map_err(|error| {
            ServerError::Message(format!("message revision snapshot encode failed: {error}"))
        })?
        else {
            return Err(ServerError::Message(
                "message revision snapshot did not produce a fields body".into(),
            ));
        };
        let purpose = format!("message-revisions:{seq}:{fingerprint:016x}");
        Ok(Frame::new(
            self.batch_op(
                ChatOp::MessageRevisionSnapshotInline,
                ChatOp::MessageRevisionSnapshotResource,
                &values,
            )?,
            seq,
            Some(room_id),
            self.batch_body(room_id, &purpose, &values)?,
        ))
    }

    pub fn latest_message_revision_snapshot_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        request_op: ChatOp,
    ) -> ServerResult<Frame> {
        let limit =
            match request_op {
                ChatOp::JoinRoom => self.limits.join_backlog_events,
                ChatOp::HistoryRecent => self.limits.history_batch_size,
                _ => return Err(ServerError::Message(
                    "message revision snapshot request does not identify a recent-history boundary"
                        .into(),
                )),
            };
        let mut target_event_ids = self
            .store
            .latest_events(room_id, limit.min(MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS))?
            .into_iter()
            .filter_map(|event| {
                matches!(event.kind, ServerRoomEventKind::Message { .. }).then_some(event.event_id)
            })
            .collect::<Vec<_>>();
        target_event_ids.sort_unstable();
        target_event_ids.dedup();
        self.message_revision_snapshot_frame(seq, room_id, &target_event_ids)
    }

    pub fn pin_snapshot_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        target_event_ids: &[u64],
    ) -> ServerResult<Frame> {
        let snapshot = self.store.pin_snapshot(room_id, target_event_ids)?;
        let FrameBody::Fields(values) = snapshot.into_frame_body().map_err(|error| {
            ServerError::Message(format!("pin snapshot encode failed: {error}"))
        })?
        else {
            return Err(ServerError::Message(
                "pin snapshot did not encode as fields".into(),
            ));
        };
        let body = compressed_values_body(&values).map_err(|error| {
            ServerError::Message(format!("pin snapshot encode failed: {error}"))
        })?;
        Ok(Frame::new(ChatOp::PinSnapshot, seq, Some(room_id), body))
    }

    pub fn latest_pin_snapshot_frame(
        &self,
        seq: u32,
        room_id: RoomId,
        request_op: ChatOp,
    ) -> ServerResult<Frame> {
        let limit = match request_op {
            ChatOp::JoinRoom => self.limits.join_backlog_events,
            ChatOp::HistoryRecent => self.limits.history_batch_size,
            _ => {
                return Err(ServerError::Message(
                    "pin snapshot request does not identify a recent-history boundary".into(),
                ))
            }
        };
        let mut target_event_ids = self
            .store
            .latest_events(room_id, limit.min(ROOM_PIN_SNAPSHOT_MAX_TARGETS))?
            .into_iter()
            .map(|event| event.event_id)
            .collect::<Vec<_>>();
        target_event_ids.sort_unstable();
        target_event_ids.dedup();
        self.pin_snapshot_frame(seq, room_id, &target_event_ids)
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
        let negotiation = match parse_session_open_negotiation(&body) {
            Ok(negotiation) => negotiation,
            Err(_) => {
                return Ok(vec![self.error_frame(
                    seq,
                    None,
                    ChatErrorCode::DurableMutationMalformed,
                    "invalid session capability negotiation",
                )]);
            }
        };
        let durable_requested = negotiation.as_ref().is_some_and(|negotiation| {
            negotiation.client_instance_id.is_some()
                && negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == DURABLE_MUTATION_CAPABILITY)
        });
        let slow_mode_requested = self.slow_mode_capability_enabled
            && durable_requested
            && negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == ROOM_SLOW_MODE_CAPABILITY)
            });
        let announcement_rooms_requested = self.announcement_rooms_enabled
            && negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == ANNOUNCEMENT_ROOMS_CAPABILITY)
            });
        let room_catalog_shape = if slow_mode_requested {
            RoomCatalogShape::SlowMode
        } else if announcement_rooms_requested {
            RoomCatalogShape::PolicyBits
        } else {
            RoomCatalogShape::Legacy
        };
        let rooms = self
            .store
            .list_rooms()?
            .into_iter()
            .map(|room| room_to_value_for_shape(&room, room_catalog_shape))
            .collect::<ServerResult<Vec<_>>>()?;
        let mut response_body = FrameBody::Fields(vec![
            FrameValue::String(PROTOCOL_NAME.into()),
            FrameValue::Array(rooms),
            self.server_motd
                .clone()
                .map(FrameValue::String)
                .unwrap_or(FrameValue::Nil),
            FrameValue::U64(self.limits.upload_quota_bytes),
            FrameValue::U64(self.limits.ping_interval_seconds.clamp(5, 600)),
            FrameValue::U64(self.limits.upload_max_file_bytes),
        ]);
        let moderation_audit_requested = self.moderation_audit_enabled
            && negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == MODERATION_AUDIT_CAPABILITY)
            });
        if durable_requested
            || moderation_audit_requested
            || announcement_rooms_requested
            || slow_mode_requested
        {
            let mut accepted_capabilities = Vec::new();
            if moderation_audit_requested {
                accepted_capabilities.push(MODERATION_AUDIT_CAPABILITY.into());
            }
            if announcement_rooms_requested {
                accepted_capabilities.push(ANNOUNCEMENT_ROOMS_CAPABILITY.into());
            }
            if slow_mode_requested {
                accepted_capabilities.push(ROOM_SLOW_MODE_CAPABILITY.into());
            }
            if durable_requested {
                accepted_capabilities.push(DURABLE_MUTATION_CAPABILITY.into());
            }
            let notice_ack_requested = negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == DURABLE_NOTICE_ACK_CAPABILITY)
            });
            if durable_requested && notice_ack_requested {
                accepted_capabilities.push(DURABLE_NOTICE_ACK_CAPABILITY.into());
            }
            let reply_mentions_requested = negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == REPLY_MENTIONS_CAPABILITY)
            });
            if durable_requested && reply_mentions_requested && REPLY_MENTIONS_SERVER_ENABLED {
                accepted_capabilities.push(REPLY_MENTIONS_CAPABILITY.into());
            }
            let reactions_requested = negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == REACTIONS_CAPABILITY)
            });
            if durable_requested && reactions_requested && REACTIONS_SERVER_ENABLED {
                accepted_capabilities.push(REACTIONS_CAPABILITY.into());
            }
            let message_revisions_requested = negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == MESSAGE_REVISIONS_CAPABILITY)
            });
            if durable_requested && message_revisions_requested && MESSAGE_REVISIONS_SERVER_ENABLED
            {
                accepted_capabilities.push(MESSAGE_REVISIONS_CAPABILITY.into());
            }
            let pins_requested = negotiation.as_ref().is_some_and(|negotiation| {
                negotiation
                    .requested_capabilities
                    .iter()
                    .any(|capability| capability == ROOM_PINS_CAPABILITY)
            });
            if durable_requested && pins_requested && ROOM_PINS_SERVER_ENABLED {
                accepted_capabilities.push(ROOM_PINS_CAPABILITY.into());
            }
            response_body = with_session_accept_negotiation(
                response_body,
                &SessionAcceptNegotiation {
                    accepted_capabilities,
                },
            )
            .map_err(|error| {
                ServerError::Message(format!("session capability response failed: {error}"))
            })?;
        }
        Ok(vec![Frame::new(
            ChatOp::SessionAccept,
            seq,
            None,
            response_body,
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
                FrameBody::Fields(vec![event_to_value(&event)?]),
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
                FrameBody::Fields(vec![event_to_value(&event)?]),
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
                FrameBody::Fields(vec![event_to_value(&event)?]),
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
            .collect::<ServerResult<Vec<_>>>()?;

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
                FrameBody::Fields(vec![event_to_value(&event)?]),
            ),
        ])
    }

    /// Executes one already-negotiated durable mutation. Capability acceptance
    /// remains disabled until every operation promised by the capability has a
    /// durable implementation.
    pub fn handle_durable_mutation(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        op: ChatOp,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        self.handle_durable_mutation_with_active_peers(
            DurableMutationPeerContext {
                peer,
                active_room_peers: &[],
                durable_notice_ack: true,
                reply_mentions: false,
                reactions: false,
                message_revisions: false,
                pins: false,
            },
            seq,
            room_id,
            op,
            client_instance_id,
            envelope,
        )
    }

    pub(crate) fn handle_durable_mutation_with_active_peers(
        &self,
        peers: DurableMutationPeerContext<'_>,
        seq: u32,
        room_id: Option<RoomId>,
        op: ChatOp,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        let peer = peers.peer;
        match op {
            ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice => self
                .handle_durable_room_text_with_notice_ack(
                    peer,
                    seq,
                    room_id,
                    DurableRoomOperation {
                        op,
                        notice_ack: peers.durable_notice_ack,
                        reply_mentions: peers.reply_mentions,
                    },
                    client_instance_id,
                    envelope,
                ),
            ChatOp::PartRoom => {
                self.handle_durable_part_room(peer, seq, room_id, client_instance_id, envelope)
            }
            ChatOp::Command => self.handle_durable_command(
                peer,
                seq,
                room_id,
                client_instance_id,
                envelope,
                peers.active_room_peers,
            ),
            ChatOp::RoomReaction if peers.reactions => {
                self.handle_durable_reaction(peer, seq, room_id, client_instance_id, envelope)
            }
            ChatOp::RoomReaction => Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationNotNegotiated,
                "reactions were not negotiated for this link",
            )),
            ChatOp::RoomMessageRevision if peers.message_revisions => self
                .handle_durable_message_revision(peer, seq, room_id, client_instance_id, envelope),
            ChatOp::RoomMessageRevision => Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationNotNegotiated,
                "message revisions were not negotiated for this link",
            )),
            ChatOp::RoomPin if peers.pins => {
                self.handle_durable_pin(peer, seq, room_id, client_instance_id, envelope)
            }
            ChatOp::RoomPin => Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationNotNegotiated,
                "room pins were not negotiated for this link",
            )),
            _ => Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationMalformed,
                "durable room operation is unsupported",
            )),
        }
    }

    /// Executes one already-negotiated durable room message, action, or notice.
    pub fn handle_durable_room_text(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        op: ChatOp,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        self.handle_durable_room_text_with_notice_ack(
            peer,
            seq,
            room_id,
            DurableRoomOperation {
                op,
                notice_ack: true,
                reply_mentions: false,
            },
            client_instance_id,
            envelope,
        )
    }

    fn handle_durable_room_text_with_notice_ack(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        operation: DurableRoomOperation,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        let op = operation.op;
        if !matches!(
            op,
            ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice
        ) {
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
        let rich_shape_tagged = matches!(
            &envelope.body,
            FrameBody::Fields(fields)
                if matches!(
                    fields.get(1),
                    Some(FrameValue::String(tag)) if tag == REPLY_MENTIONS_BODY_TAG
                )
        );
        if rich_shape_tagged && !operation.reply_mentions {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationNotNegotiated,
                "reply and mention metadata was not negotiated for this link",
            ));
        }
        let (body, metadata) = if op == ChatOp::RoomMessage
            && operation.reply_mentions
            && matches!(&envelope.body, FrameBody::Fields(_))
        {
            let rich = match RichMessageBody::from_frame_body(&envelope.body) {
                Ok(rich) => rich,
                Err(_) => {
                    return Ok(self.durable_error_dispatch(
                        seq,
                        Some(room_id),
                        ChatErrorCode::DurableMutationMalformed,
                        "rich room message body is malformed",
                    ))
                }
            };
            if rich
                .reply_to
                .is_some_and(|reference| reference.room_id != room_id)
            {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "reply reference belongs to a different room",
                ));
            }
            (
                rich.body,
                Some(RichMessageEventMetadata {
                    reply_to_event_id: rich.reply_to.map(|reference| reference.event_id),
                    mentioned_user_ids: rich.mentioned_user_ids,
                }),
            )
        } else {
            let Some(body) = body_string(&envelope.body).filter(|body| !body.trim().is_empty())
            else {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable message body is empty",
                ));
            };
            (body, None)
        };
        if body.trim().is_empty() {
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
        let event_kind = match op {
            ChatOp::RoomMessage => ServerRoomEventKind::Message { body },
            ChatOp::RoomAction => ServerRoomEventKind::Action { body },
            ChatOp::RoomNotice => ServerRoomEventKind::Notice { body },
            _ => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable room operation is unsupported",
                ))
            }
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
                if let Some(result_frame) = self.durable_room_policy_rejection(
                    transaction,
                    seq,
                    room_id,
                    user.role_bits,
                    "publishing messages",
                )? {
                    return Ok(DurableRoomEventPlan::Response { result_frame });
                }
                if op == ChatOp::RoomNotice && user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
                    return Ok(DurableRoomEventPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "room notices require moderator or admin role",
                        ))?,
                    });
                }
                if let Some(metadata) = metadata.as_ref() {
                    if !OmenchatStore::durable_room_has_member(transaction, room_id, user.user_id)?
                    {
                        return Ok(DurableRoomEventPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::NotJoined,
                                "join the room before sending a rich message",
                            ))?,
                        });
                    }
                    if let Some(reply_to_event_id) = metadata.reply_to_event_id {
                        if !OmenchatStore::durable_room_event_exists(
                            transaction,
                            room_id,
                            reply_to_event_id,
                        )? {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::HistoryUnavailable,
                                    "reply target is unavailable in this room",
                                ))?,
                            });
                        }
                    }
                    for mentioned_user_id in &metadata.mentioned_user_ids {
                        if !OmenchatStore::durable_room_has_member(
                            transaction,
                            room_id,
                            *mentioned_user_id,
                        )? {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::UserNotFound,
                                    "mentioned user is not a current room member",
                                ))?,
                            });
                        }
                    }
                }
                let rate = match self.reserve_rate(peer, RateKind::Message)? {
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
                let slow_mode = if self.slow_mode_enforcement_enabled
                    && op != ChatOp::RoomNotice
                    && user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0
                {
                    let Some(seconds) = room_slow_mode_seconds(transaction, room_id)? else {
                        return Ok(DurableRoomEventPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RoomNotFound,
                                "room not found",
                            ))?,
                        });
                    };
                    let reservation = match self.slow_mode.reserve(
                        room_id,
                        user.user_id,
                        seconds,
                        Instant::now(),
                    )? {
                        SlowModeMonotonicAdmission::Disabled => None,
                        SlowModeMonotonicAdmission::Admitted(reservation) => Some(reservation),
                        SlowModeMonotonicAdmission::Rejected {
                            retry_after_seconds,
                        } => {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::SlowModeActive,
                                    &format!(
                                        "room slow mode is active; retry in {retry_after_seconds}s"
                                    ),
                                ))?,
                            })
                        }
                        SlowModeMonotonicAdmission::Saturated => {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::SlowModeActive,
                                    "room slow-mode admission is saturated",
                                ))?,
                            })
                        }
                    };
                    match admit_room_publication(
                        transaction,
                        room_id,
                        user.user_id,
                        i64::try_from(unix_seconds()).unwrap_or(i64::MAX),
                    )? {
                        SlowModeAdmission::Disabled => None,
                        SlowModeAdmission::Admitted { .. } => reservation,
                        SlowModeAdmission::Rejected {
                            retry_after_seconds,
                        } => {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::SlowModeActive,
                                    &format!(
                                        "room slow mode is active; retry in {retry_after_seconds}s"
                                    ),
                                ))?,
                            })
                        }
                        SlowModeAdmission::RoomNotFound => {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::RoomNotFound,
                                    "room not found",
                                ))?,
                            })
                        }
                        SlowModeAdmission::Saturated => {
                            return Ok(DurableRoomEventPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    Some(room_id),
                                    ChatErrorCode::SlowModeActive,
                                    "room slow-mode admission is saturated",
                                ))?,
                            })
                        }
                    }
                } else {
                    None
                };
                let admission = RoomPublicationAdmission { rate, slow_mode };
                OmenchatStore::join_durable_room(transaction, room_id, user.user_id)?;
                match metadata.clone() {
                    Some(metadata) => Ok(DurableRoomEventPlan::RichEvent {
                        actor_user_id: Some(user.user_id),
                        kind: event_kind,
                        metadata,
                        admission,
                    }),
                    None => Ok(DurableRoomEventPlan::Event {
                        actor_user_id: Some(user.user_id),
                        kind: event_kind,
                        admission,
                    }),
                }
            },
            |event| {
                let result = if op == ChatOp::RoomNotice && !operation.notice_ack {
                    Frame::new(
                        ChatOp::RoomEvent,
                        seq,
                        Some(room_id),
                        FrameBody::Fields(vec![event_to_value(event)?]),
                    )
                } else {
                    message_ack_for_event(seq, event)
                };
                self.encode_durable_result(result)
            },
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
                admission.commit();
                Ok(DurableMutationDispatch {
                    origin: decode_durable_result(&result_frame)?,
                    broadcasts: vec![Frame::new(
                        ChatOp::RoomEvent,
                        seq,
                        Some(room_id),
                        FrameBody::Fields(vec![event_to_value(&event)?]),
                    )],
                    disconnect_identity: None,
                    pruned,
                })
            }
            DurableRoomEventCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned,
            }),
            DurableRoomEventCommit::Replayed { result_frame } => Ok(DurableMutationDispatch {
                origin: decode_durable_replay_result(&result_frame, seq)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
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

    fn handle_durable_reaction(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        let Some(room_id) = room_id else {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "durable reaction has no room id",
            ));
        };
        let canonical_hash = match canonical_mutation_request_hash(
            ChatOp::RoomReaction,
            Some(room_id),
            &envelope.body,
        ) {
            Ok(hash) => hash,
            Err(_) => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable reaction body exceeds canonical bounds",
                ))
            }
        };
        if canonical_hash != envelope.request_hash {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable reaction hash does not match its canonical body",
            ));
        }
        let request = match ReactionRequest::from_frame_body(&envelope.body) {
            Ok(request) if i64::try_from(request.target_event_id).is_ok() => request,
            _ => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable reaction request is malformed",
                ))
            }
        };
        let key = DurableMutationKey {
            identity_hash: &peer.identity_hash,
            client_instance_id,
            mutation_id: envelope.mutation_id,
        };
        let commit = self.store.commit_durable_mutation_effect_result(
            key,
            envelope.request_hash,
            |transaction| {
                let Some(user) = OmenchatStore::ensure_durable_room_user(
                    transaction,
                    room_id,
                    &peer.identity_hash,
                    &peer.display_name,
                    peer.lxmf_destination.as_deref(),
                )?
                else {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RoomNotFound,
                            "room not found",
                        ))?,
                    });
                };
                if user.status_bits & STATUS_BANNED != 0 {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "user is banned",
                        ))?,
                    });
                }
                if user.status_bits & STATUS_MUTED != 0 {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "user is muted",
                        ))?,
                    });
                }
                if !OmenchatStore::durable_room_has_member(transaction, room_id, user.user_id)? {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::NotJoined,
                            "join the room before reacting",
                        ))?,
                    });
                }
                if let Some(result_frame) = self.durable_room_policy_rejection(
                    transaction,
                    seq,
                    room_id,
                    user.role_bits,
                    "publishing reactions",
                )? {
                    return Ok(DurableMutationEffectPlan::Response { result_frame });
                }
                let admission = match self.reserve_rate(peer, RateKind::Command)? {
                    RateAdmission::Admitted(admission) => admission,
                    RateAdmission::Rejected => {
                        return Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RateLimited,
                                "reaction rate limit exceeded",
                            ))?,
                        })
                    }
                };
                match OmenchatStore::apply_reaction_mutation(
                    transaction,
                    room_id,
                    user.user_id,
                    request,
                )? {
                    ReactionMutationResult::TargetUnavailable => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::HistoryUnavailable,
                                "reaction target is unavailable in this room",
                            ))?,
                        })
                    }
                    ReactionMutationResult::Saturated => Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RateLimited,
                            "reaction state retention limit reached",
                        ))?,
                    }),
                    ReactionMutationResult::Unchanged => {
                        let ack = ReactionAck {
                            target_event_id: request.target_event_id,
                            actor_user_id: user.user_id,
                            token: request.token,
                            action: request.action,
                            changed: false,
                            reaction_event_id: None,
                        };
                        Ok(DurableMutationEffectPlan::Effect {
                            result_frame: self.encode_durable_result(Frame::new(
                                ChatOp::ReactionAck,
                                seq,
                                Some(room_id),
                                ack.into_frame_body().map_err(|error| {
                                    ServerError::Message(format!(
                                        "reaction acknowledgement encode failed: {error}"
                                    ))
                                })?,
                            ))?,
                            effect: DurableReactionEffect {
                                broadcast: None,
                                admission,
                            },
                        })
                    }
                    ReactionMutationResult::Changed(event) => {
                        let ack = ReactionAck {
                            target_event_id: event.target_event_id,
                            actor_user_id: event.actor_user_id,
                            token: event.token,
                            action: event.action,
                            changed: true,
                            reaction_event_id: Some(event.reaction_event_id),
                        };
                        let broadcast = Frame::new(
                            ChatOp::ReactionEvent,
                            seq,
                            Some(room_id),
                            event.into_frame_body().map_err(|error| {
                                ServerError::Message(format!(
                                    "reaction event encode failed: {error}"
                                ))
                            })?,
                        );
                        Ok(DurableMutationEffectPlan::Effect {
                            result_frame: self.encode_durable_result(Frame::new(
                                ChatOp::ReactionAck,
                                seq,
                                Some(room_id),
                                ack.into_frame_body().map_err(|error| {
                                    ServerError::Message(format!(
                                        "reaction acknowledgement encode failed: {error}"
                                    ))
                                })?,
                            ))?,
                            effect: DurableReactionEffect {
                                broadcast: Some(broadcast),
                                admission,
                            },
                        })
                    }
                }
            },
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
            DurableMutationEffectCommit::Stored {
                result_frame,
                effect,
                pruned,
            } => {
                if let Some(admission) = effect.admission {
                    admission.commit();
                }
                Ok(DurableMutationDispatch {
                    origin: decode_durable_result(&result_frame)?,
                    broadcasts: effect.broadcast.into_iter().collect(),
                    disconnect_identity: None,
                    pruned,
                })
            }
            DurableMutationEffectCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned,
            }),
            DurableMutationEffectCommit::Replayed { result_frame } => Ok(DurableMutationDispatch {
                origin: decode_durable_replay_result(&result_frame, seq)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned: 0,
            }),
            DurableMutationEffectCommit::Conflict => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationConflict,
                "durable mutation id was reused with different content",
            )),
            DurableMutationEffectCommit::Expired => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationResultExpired,
                "durable client instance has expired replay state",
            )),
        }
    }

    fn handle_durable_pin(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        let Some(room_id) = room_id else {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "durable pin mutation has no room id",
            ));
        };
        let canonical_hash =
            match canonical_mutation_request_hash(ChatOp::RoomPin, Some(room_id), &envelope.body) {
                Ok(hash) => hash,
                Err(_) => {
                    return Ok(self.durable_error_dispatch(
                        seq,
                        Some(room_id),
                        ChatErrorCode::DurableMutationMalformed,
                        "durable pin body exceeds canonical bounds",
                    ))
                }
            };
        if canonical_hash != envelope.request_hash {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable pin hash does not match its canonical body",
            ));
        }
        let request = match PinRequest::from_frame_body(&envelope.body) {
            Ok(request) if i64::try_from(request.target_event_id).is_ok() => request,
            _ => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable pin request is malformed",
                ))
            }
        };
        let key = DurableMutationKey {
            identity_hash: &peer.identity_hash,
            client_instance_id,
            mutation_id: envelope.mutation_id,
        };
        let commit = self.store.commit_durable_mutation_effect_result(
            key,
            envelope.request_hash,
            |transaction| {
                let Some(user) = OmenchatStore::ensure_durable_room_user(
                    transaction,
                    room_id,
                    &peer.identity_hash,
                    &peer.display_name,
                    peer.lxmf_destination.as_deref(),
                )?
                else {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RoomNotFound,
                            "room not found",
                        ))?,
                    });
                };
                if user.status_bits & STATUS_BANNED != 0 {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "user is banned",
                        ))?,
                    });
                }
                if !OmenchatStore::durable_room_has_member(transaction, room_id, user.user_id)? {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::NotJoined,
                            "join the room before changing pins",
                        ))?,
                    });
                }
                if user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "moderator or administrator role is required to change pins",
                        ))?,
                    });
                }
                let admission = match self.reserve_rate(peer, RateKind::Command)? {
                    RateAdmission::Admitted(admission) => admission,
                    RateAdmission::Rejected => {
                        return Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RateLimited,
                                "pin mutation rate limit exceeded",
                            ))?,
                        })
                    }
                };
                match OmenchatStore::apply_pin_mutation(
                    transaction,
                    room_id,
                    user.user_id,
                    request,
                )? {
                    PinMutationResult::TargetUnavailable => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::HistoryUnavailable,
                                "pin target is unavailable in this room",
                            ))?,
                        })
                    }
                    PinMutationResult::Saturated => Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RateLimited,
                            "pin state retention limit reached",
                        ))?,
                    }),
                    PinMutationResult::Unchanged => {
                        let ack = PinAck {
                            target_event_id: request.target_event_id,
                            action: request.action,
                            actor_user_id: user.user_id,
                            changed: false,
                            pin_event_id: None,
                        };
                        Ok(DurableMutationEffectPlan::Effect {
                            result_frame: self.encode_durable_result(Frame::new(
                                ChatOp::PinAck,
                                seq,
                                Some(room_id),
                                ack.into_frame_body().map_err(|error| {
                                    ServerError::Message(format!(
                                        "pin acknowledgement encode failed: {error}"
                                    ))
                                })?,
                            ))?,
                            effect: DurablePinEffect {
                                broadcast: None,
                                admission,
                            },
                        })
                    }
                    PinMutationResult::Changed(event) => {
                        let ack = PinAck {
                            target_event_id: event.target_event_id,
                            action: event.action,
                            actor_user_id: event.actor_user_id,
                            changed: true,
                            pin_event_id: Some(event.pin_event_id),
                        };
                        let broadcast = Frame::new(
                            ChatOp::PinEvent,
                            seq,
                            Some(room_id),
                            event.into_frame_body().map_err(|error| {
                                ServerError::Message(format!("pin event encode failed: {error}"))
                            })?,
                        );
                        Ok(DurableMutationEffectPlan::Effect {
                            result_frame: self.encode_durable_result(Frame::new(
                                ChatOp::PinAck,
                                seq,
                                Some(room_id),
                                ack.into_frame_body().map_err(|error| {
                                    ServerError::Message(format!(
                                        "pin acknowledgement encode failed: {error}"
                                    ))
                                })?,
                            ))?,
                            effect: DurablePinEffect {
                                broadcast: Some(broadcast),
                                admission,
                            },
                        })
                    }
                }
            },
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
            DurableMutationEffectCommit::Stored {
                result_frame,
                effect,
                pruned,
            } => {
                if let Some(admission) = effect.admission {
                    admission.commit();
                }
                Ok(DurableMutationDispatch {
                    origin: decode_durable_result(&result_frame)?,
                    broadcasts: effect.broadcast.into_iter().collect(),
                    disconnect_identity: None,
                    pruned,
                })
            }
            DurableMutationEffectCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned,
            }),
            DurableMutationEffectCommit::Replayed { result_frame } => Ok(DurableMutationDispatch {
                origin: decode_durable_replay_result(&result_frame, seq)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned: 0,
            }),
            DurableMutationEffectCommit::Conflict => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationConflict,
                "durable mutation id was reused with different content",
            )),
            DurableMutationEffectCommit::Expired => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationResultExpired,
                "durable client instance has expired replay state",
            )),
        }
    }

    fn handle_durable_message_revision(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        let Some(room_id) = room_id else {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "durable message revision has no room id",
            ));
        };
        let canonical_hash = match canonical_mutation_request_hash(
            ChatOp::RoomMessageRevision,
            Some(room_id),
            &envelope.body,
        ) {
            Ok(hash) => hash,
            Err(_) => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable message revision body exceeds canonical bounds",
                ))
            }
        };
        if canonical_hash != envelope.request_hash {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable message revision hash does not match its canonical body",
            ));
        }
        let request = match MessageRevisionRequest::from_frame_body(&envelope.body) {
            Ok(request)
                if i64::try_from(request.target_event_id).is_ok()
                    && request.replacement.as_ref().is_none_or(|replacement| {
                        replacement.len() <= self.limits.max_message_bytes
                    }) =>
            {
                request
            }
            _ => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    Some(room_id),
                    ChatErrorCode::DurableMutationMalformed,
                    "durable message revision request is malformed or too large",
                ))
            }
        };
        let key = DurableMutationKey {
            identity_hash: &peer.identity_hash,
            client_instance_id,
            mutation_id: envelope.mutation_id,
        };
        let commit = self.store.commit_durable_mutation_effect_result(
            key,
            envelope.request_hash,
            |transaction| {
                let Some(user) = OmenchatStore::ensure_durable_room_user(
                    transaction,
                    room_id,
                    &peer.identity_hash,
                    &peer.display_name,
                    peer.lxmf_destination.as_deref(),
                )?
                else {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RoomNotFound,
                            "room not found",
                        ))?,
                    });
                };
                if user.status_bits & STATUS_BANNED != 0 {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::PermissionDenied,
                            "user is banned",
                        ))?,
                    });
                }
                if !OmenchatStore::durable_room_has_member(transaction, room_id, user.user_id)? {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::NotJoined,
                            "join the room before revising a message",
                        ))?,
                    });
                }
                if let Some(result_frame) = self.durable_room_policy_rejection(
                    transaction,
                    seq,
                    room_id,
                    user.role_bits,
                    "revising messages",
                )? {
                    return Ok(DurableMutationEffectPlan::Response { result_frame });
                }
                let admission = match self.reserve_rate(peer, RateKind::Command)? {
                    RateAdmission::Admitted(admission) => admission,
                    RateAdmission::Rejected => {
                        return Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RateLimited,
                                "message revision rate limit exceeded",
                            ))?,
                        })
                    }
                };
                let policy = MessageRevisionActorPolicy {
                    is_moderator: user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) != 0,
                    is_muted: user.status_bits & STATUS_MUTED != 0,
                };
                match OmenchatStore::apply_message_revision_mutation(
                    transaction,
                    room_id,
                    user.user_id,
                    Some(&user.display_name),
                    policy,
                    request.clone(),
                    self.limits.max_message_bytes,
                )? {
                    MessageRevisionMutationResult::Changed(mutation) => {
                        let event = mutation.event;
                        let ack = MessageRevisionAck {
                            target_event_id: event.target_event_id,
                            action: event.action,
                            actor_user_id: event.actor_user_id,
                            changed: true,
                            revision_event_id: Some(event.revision_event_id),
                            revision_number: event.revision_number,
                        };
                        let broadcast = Frame::new(
                            ChatOp::MessageRevisionEvent,
                            seq,
                            Some(room_id),
                            event.into_frame_body().map_err(|error| {
                                ServerError::Message(format!(
                                    "message revision event encode failed: {error}"
                                ))
                            })?,
                        );
                        Ok(DurableMutationEffectPlan::Effect {
                            result_frame: self.encode_durable_result(Frame::new(
                                ChatOp::MessageRevisionAck,
                                seq,
                                Some(room_id),
                                ack.into_frame_body().map_err(|error| {
                                    ServerError::Message(format!(
                                        "message revision acknowledgement encode failed: {error}"
                                    ))
                                })?,
                            ))?,
                            effect: DurableMessageRevisionEffect {
                                broadcast,
                                admission,
                            },
                        })
                    }
                    MessageRevisionMutationResult::Unchanged => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::DurableMutationConflict,
                                "message revision does not change effective text",
                            ))?,
                        })
                    }
                    MessageRevisionMutationResult::TargetUnavailable
                    | MessageRevisionMutationResult::AlreadyTombstoned => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::HistoryUnavailable,
                                "message revision target is unavailable in this room",
                            ))?,
                        })
                    }
                    MessageRevisionMutationResult::PermissionDenied => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::PermissionDenied,
                                "message revision is not permitted",
                            ))?,
                        })
                    }
                    MessageRevisionMutationResult::CorrectionLimitReached => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RateLimited,
                                "message correction limit reached",
                            ))?,
                        })
                    }
                    MessageRevisionMutationResult::Saturated => {
                        Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                Some(room_id),
                                ChatErrorCode::RateLimited,
                                "message revision retention limit reached",
                            ))?,
                        })
                    }
                }
            },
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
            DurableMutationEffectCommit::Stored {
                result_frame,
                effect,
                pruned,
            } => {
                if let Some(admission) = effect.admission {
                    admission.commit();
                }
                Ok(DurableMutationDispatch {
                    origin: decode_durable_result(&result_frame)?,
                    broadcasts: vec![effect.broadcast],
                    disconnect_identity: None,
                    pruned,
                })
            }
            DurableMutationEffectCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned,
            }),
            DurableMutationEffectCommit::Replayed { result_frame } => Ok(DurableMutationDispatch {
                origin: decode_durable_replay_result(&result_frame, seq)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned: 0,
            }),
            DurableMutationEffectCommit::Conflict => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationConflict,
                "durable mutation id was reused with different content",
            )),
            DurableMutationEffectCommit::Expired => Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationResultExpired,
                "durable client instance has expired replay state",
            )),
        }
    }

    fn handle_durable_part_room(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
    ) -> ServerResult<DurableMutationDispatch> {
        let Some(room_id) = room_id else {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "durable part has no room id",
            ));
        };
        let canonical_hash = match canonical_mutation_request_hash(
            ChatOp::PartRoom,
            Some(room_id),
            &envelope.body,
        ) {
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
        if canonical_hash != envelope.request_hash || envelope.body != FrameBody::Empty {
            return Ok(self.durable_error_dispatch(
                seq,
                Some(room_id),
                ChatErrorCode::DurableMutationMalformed,
                "durable part request is malformed",
            ));
        }
        let key = DurableMutationKey {
            identity_hash: &peer.identity_hash,
            client_instance_id,
            mutation_id: envelope.mutation_id,
        };
        let commit = self.store.commit_durable_room_event_result(
            key,
            envelope.request_hash,
            room_id,
            |transaction| {
                let Some(room) = OmenchatStore::durable_room(transaction, room_id)? else {
                    return Ok(DurableRoomEventPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            Some(room_id),
                            ChatErrorCode::RoomNotFound,
                            "room not found",
                        ))?,
                    });
                };
                let Some(user) = OmenchatStore::ensure_durable_room_user(
                    transaction,
                    room_id,
                    &peer.identity_hash,
                    &peer.display_name,
                    peer.lxmf_destination.as_deref(),
                )?
                else {
                    return Err(ServerError::Message(
                        "durable part room disappeared during transaction".into(),
                    ));
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
                OmenchatStore::leave_durable_room(transaction, room_id, user.user_id)?;
                let result_frame = self.encode_durable_result(Frame::new(
                    ChatOp::CommandResult,
                    seq,
                    Some(room_id),
                    FrameBody::Fields(vec![
                        FrameValue::String("part".into()),
                        room_to_value(&room),
                    ]),
                ))?;
                Ok(DurableRoomEventPlan::EventWithResult {
                    actor_user_id: Some(user.user_id),
                    kind: ServerRoomEventKind::System {
                        body: format!("{} left #{}", user.display_name, room.name),
                    },
                    admission: (),
                    result_frame,
                })
            },
            |_| {
                Err(ServerError::Message(
                    "durable part unexpectedly requested event-derived result encoding".into(),
                ))
            },
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
                pruned,
                ..
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: vec![Frame::new(
                    ChatOp::RoomEvent,
                    seq,
                    Some(room_id),
                    FrameBody::Fields(vec![event_to_value(&event)?]),
                )],
                disconnect_identity: None,
                pruned,
            }),
            DurableRoomEventCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned,
            }),
            DurableRoomEventCommit::Replayed { result_frame } => Ok(DurableMutationDispatch {
                origin: decode_durable_replay_result(&result_frame, seq)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
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

    fn handle_durable_command(
        &self,
        peer: &ServerPeer,
        seq: u32,
        room_id: Option<RoomId>,
        client_instance_id: ClientInstanceId,
        envelope: DurableMutationEnvelope,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<DurableMutationDispatch> {
        let canonical_hash =
            match canonical_mutation_request_hash(ChatOp::Command, room_id, &envelope.body) {
                Ok(hash) => hash,
                Err(_) => {
                    return Ok(self.durable_error_dispatch(
                        seq,
                        room_id,
                        ChatErrorCode::DurableMutationMalformed,
                        "durable command body exceeds canonical bounds",
                    ))
                }
            };
        if canonical_hash != envelope.request_hash {
            return Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationMalformed,
                "durable command hash does not match its canonical body",
            ));
        }
        let Some(command) =
            body_string(&envelope.body).filter(|command| !command.trim().is_empty())
        else {
            return Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationMalformed,
                "durable command body is empty",
            ));
        };
        let command = command.trim();
        let (command_name, command_rest) = command
            .split_once(char::is_whitespace)
            .unwrap_or((command, ""));
        let command_name = command_name.to_ascii_lowercase();
        if !matches!(
            command_name.as_str(),
            "topic" | "create" | "kick" | "ban" | "mute" | "unmute" | "role" | "unban"
        ) {
            return Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationMalformed,
                "durable command is not implemented",
            ));
        }
        let normalized_create_name = (command_name == "create").then(|| {
            let (name, _) = command_rest
                .split_once(char::is_whitespace)
                .unwrap_or((command_rest, ""));
            normalize_room_name(name)
        });
        if normalized_create_name
            .as_ref()
            .is_some_and(String::is_empty)
        {
            return Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationMalformed,
                "create command requires a room name",
            ));
        }
        if command_name == "topic" && room_id.is_none() {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "topic command requires an active room",
            ));
        }
        let moderation_command =
            matches!(command_name.as_str(), "kick" | "ban" | "mute" | "unmute");
        if moderation_command && room_id.is_none() {
            return Ok(self.durable_error_dispatch(
                seq,
                None,
                ChatErrorCode::DurableMutationMalformed,
                "moderation command requires an active room",
            ));
        }
        let role_change = if command_name == "role" {
            let (target, role_label) = command_rest
                .trim()
                .split_once(char::is_whitespace)
                .unwrap_or((command_rest.trim(), ""));
            let Some(role_bits) = role_bits_from_label(role_label) else {
                return Ok(self.durable_error_dispatch(
                    seq,
                    room_id,
                    ChatErrorCode::DurableMutationMalformed,
                    "usage: role <user> <standard|trusted|mod|admin>",
                ));
            };
            Some((target.to_owned(), role_bits))
        } else {
            None
        };

        let key = DurableMutationKey {
            identity_hash: &peer.identity_hash,
            client_instance_id,
            mutation_id: envelope.mutation_id,
        };
        let history_retention = self.store.room_history_retention();
        let commit = self.store.commit_durable_mutation_effect_result(
            key,
            envelope.request_hash,
            |transaction| {
                let user = OmenchatStore::ensure_durable_user(
                    transaction,
                    &peer.identity_hash,
                    &peer.display_name,
                    peer.lxmf_destination.as_deref(),
                )?;
                if user.status_bits & STATUS_BANNED != 0 {
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            room_id,
                            ChatErrorCode::PermissionDenied,
                            "user is banned",
                        ))?,
                    });
                }
                let required_role = match command_name.as_str() {
                    "create" | "role" | "unban" => ROLE_ADMIN,
                    _ => ROLE_MODERATOR | ROLE_ADMIN,
                };
                if user.role_bits & required_role == 0 {
                    let message = match command_name.as_str() {
                        "create" => "room creation requires admin role",
                        "role" => "role changes require admin role",
                        "unban" => "unban requires admin role",
                        "kick" | "ban" | "mute" | "unmute" => {
                            "moderation command requires moderator or admin role"
                        }
                        _ => "topic changes require moderator or admin role",
                    };
                    return Ok(DurableMutationEffectPlan::Response {
                        result_frame: self.encode_durable_result(self.error_frame(
                            seq,
                            room_id,
                            ChatErrorCode::PermissionDenied,
                            message,
                        ))?,
                    });
                }
                let admission = match self.reserve_rate(peer, RateKind::Command)? {
                    RateAdmission::Admitted(admission) => admission,
                    RateAdmission::Rejected => {
                        return Ok(DurableMutationEffectPlan::Response {
                            result_frame: self.encode_durable_result(self.error_frame(
                                seq,
                                room_id,
                                ChatErrorCode::RateLimited,
                                "command rate limit exceeded",
                            ))?,
                        })
                    }
                };
                let (result_room_id, result_value, broadcasts, disconnect_identity) =
                    if command_name == "topic" {
                        let active_room_id = room_id.ok_or_else(|| {
                            ServerError::Message("durable topic room id was not prepared".into())
                        })?;
                        let topic = command_rest.trim();
                        let Some(room) = OmenchatStore::update_durable_room_topic(
                            transaction,
                            active_room_id,
                            (!topic.is_empty()).then_some(topic),
                        )?
                        else {
                            return Ok(DurableMutationEffectPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    room_id,
                                    ChatErrorCode::RoomNotFound,
                                    "room not found",
                                ))?,
                            });
                        };
                        let broadcast = Frame::new(
                            ChatOp::RoomDelta,
                            seq,
                            Some(room.room_id),
                            FrameBody::Fields(vec![room_to_value(&room)]),
                        );
                        (
                            Some(room.room_id),
                            room_to_value(&room),
                            vec![broadcast],
                            None,
                        )
                    } else if command_name == "create" {
                        let (_, topic) = command_rest
                            .split_once(char::is_whitespace)
                            .unwrap_or((command_rest, ""));
                        let room = OmenchatStore::create_durable_room(
                            transaction,
                            normalized_create_name.as_deref().ok_or_else(|| {
                                ServerError::Message("durable create name was not prepared".into())
                            })?,
                            (!topic.trim().is_empty()).then_some(topic.trim()),
                        )?;
                        let broadcast = Frame::new(
                            ChatOp::RoomDelta,
                            seq,
                            None,
                            FrameBody::Fields(vec![room_to_value(&room)]),
                        );
                        (None, room_to_value(&room), vec![broadcast], None)
                    } else if moderation_command {
                        let active_room_id = room_id.ok_or_else(|| {
                            ServerError::Message(
                                "durable moderation room id was not prepared".into(),
                            )
                        })?;
                        let Some(target_peer) =
                            resolve_active_peer_target(active_room_peers, command_rest.trim())
                        else {
                            return Ok(DurableMutationEffectPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    room_id,
                                    ChatErrorCode::UserNotFound,
                                    "target user is not active in this room",
                                ))?,
                            });
                        };
                        if target_peer.identity_hash == peer.identity_hash {
                            return Ok(DurableMutationEffectPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    room_id,
                                    ChatErrorCode::PermissionDenied,
                                    "cannot moderate your own active session",
                                ))?,
                            });
                        }
                        let target_user = OmenchatStore::ensure_durable_user(
                            transaction,
                            &target_peer.identity_hash,
                            &target_peer.display_name,
                            target_peer.lxmf_destination.as_deref(),
                        )?;
                        if target_user.role_bits & ROLE_ADMIN != 0
                            && user.role_bits & ROLE_ADMIN == 0
                        {
                            return Ok(DurableMutationEffectPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    room_id,
                                    ChatErrorCode::PermissionDenied,
                                    "moderators cannot moderate admins",
                                ))?,
                            });
                        }
                        let changed_user = match command_name.as_str() {
                            "ban" => OmenchatStore::set_durable_user_status_flag(
                                transaction,
                                target_user.user_id,
                                STATUS_BANNED,
                                true,
                            )?,
                            "mute" => OmenchatStore::set_durable_user_status_flag(
                                transaction,
                                target_user.user_id,
                                STATUS_MUTED,
                                true,
                            )?,
                            "unmute" => OmenchatStore::set_durable_user_status_flag(
                                transaction,
                                target_user.user_id,
                                STATUS_MUTED,
                                false,
                            )?,
                            _ => target_user,
                        };
                        let audit_action = match command_name.as_str() {
                            "ban" => ModerationAuditAction::Ban,
                            "mute" => ModerationAuditAction::Mute,
                            "unmute" => ModerationAuditAction::Unmute,
                            _ => ModerationAuditAction::Kick,
                        };
                        let result_status_bits = (audit_action != ModerationAuditAction::Kick)
                            .then_some(changed_user.status_bits);
                        if OmenchatStore::append_durable_moderation_audit(
                            transaction,
                            active_room_id,
                            &user,
                            &changed_user,
                            audit_action,
                            None,
                            result_status_bits,
                        )? == ModerationAuditAdmission::Saturated
                        {
                            return Err(ServerError::Message(
                                "moderation audit retention is saturated; mutation rolled back"
                                    .into(),
                            ));
                        }
                        let event = OmenchatStore::append_durable_room_event(
                            transaction,
                            active_room_id,
                            Some(user.user_id),
                            ServerRoomEventKind::System {
                                body: format!(
                                    "{} {} {}",
                                    user.display_name,
                                    moderation_past_tense(&command_name),
                                    changed_user.display_name
                                ),
                            },
                            history_retention,
                        )?;
                        let broadcasts = vec![
                            user_delta_frame(seq, room_id, &changed_user),
                            Frame::new(
                                ChatOp::RoomEvent,
                                seq,
                                room_id,
                                FrameBody::Fields(vec![event_to_value(&event)?]),
                            ),
                        ];
                        let disconnect_identity = matches!(command_name.as_str(), "kick" | "ban")
                            .then(|| target_peer.identity_hash.clone());
                        (
                            room_id,
                            user_to_value(&changed_user),
                            broadcasts,
                            disconnect_identity,
                        )
                    } else {
                        let (target, role_bits) = role_change
                            .as_ref()
                            .map(|(target, role_bits)| (target.as_str(), Some(*role_bits)))
                            .unwrap_or((command_rest.trim(), None));
                        let Some(target_user) = resolve_known_user_target(
                            &OmenchatStore::durable_users(transaction)?,
                            target,
                        ) else {
                            return Ok(DurableMutationEffectPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    room_id,
                                    ChatErrorCode::UserNotFound,
                                    "target user is unknown",
                                ))?,
                            });
                        };
                        if target_user.identity_hash == peer.identity_hash {
                            let message = if command_name == "role" {
                                "cannot change your own active session role"
                            } else {
                                "cannot unban your own active session"
                            };
                            return Ok(DurableMutationEffectPlan::Response {
                                result_frame: self.encode_durable_result(self.error_frame(
                                    seq,
                                    room_id,
                                    ChatErrorCode::PermissionDenied,
                                    message,
                                ))?,
                            });
                        }
                        let changed_user = if let Some(role_bits) = role_bits {
                            OmenchatStore::set_durable_user_role_bits(
                                transaction,
                                target_user.user_id,
                                role_bits,
                            )?
                        } else {
                            OmenchatStore::set_durable_user_status_flag(
                                transaction,
                                target_user.user_id,
                                STATUS_BANNED,
                                false,
                            )?
                        };
                        if let Some(active_room_id) = room_id {
                            let (audit_action, result_role_bits, result_status_bits) =
                                if role_bits.is_some() {
                                    (
                                        ModerationAuditAction::RoleChange,
                                        Some(changed_user.role_bits),
                                        None,
                                    )
                                } else {
                                    (
                                        ModerationAuditAction::Unban,
                                        None,
                                        Some(changed_user.status_bits),
                                    )
                                };
                            if OmenchatStore::append_durable_moderation_audit(
                                transaction,
                                active_room_id,
                                &user,
                                &changed_user,
                                audit_action,
                                result_role_bits,
                                result_status_bits,
                            )? == ModerationAuditAdmission::Saturated
                            {
                                return Err(ServerError::Message(
                                    "moderation audit retention is saturated; mutation rolled back"
                                        .into(),
                                ));
                            }
                        }
                        let mut broadcasts = vec![user_delta_frame(seq, room_id, &changed_user)];
                        if let Some(active_room_id) = room_id {
                            let event_body = if let Some(role_bits) = role_bits {
                                format!(
                                    "{} set {} role to {}",
                                    user.display_name,
                                    changed_user.display_name,
                                    role_label_from_bits(role_bits)
                                )
                            } else {
                                format!(
                                    "{} unbanned {}",
                                    user.display_name, changed_user.display_name
                                )
                            };
                            let event = OmenchatStore::append_durable_room_event(
                                transaction,
                                active_room_id,
                                Some(user.user_id),
                                ServerRoomEventKind::System { body: event_body },
                                history_retention,
                            )?;
                            broadcasts.push(Frame::new(
                                ChatOp::RoomEvent,
                                seq,
                                Some(active_room_id),
                                FrameBody::Fields(vec![event_to_value(&event)?]),
                            ));
                        }
                        (room_id, user_to_value(&changed_user), broadcasts, None)
                    };
                let result_frame = self.encode_durable_result(Frame::new(
                    ChatOp::CommandResult,
                    seq,
                    result_room_id,
                    FrameBody::Fields(vec![FrameValue::String(command_name.clone()), result_value]),
                ))?;
                Ok(DurableMutationEffectPlan::Effect {
                    result_frame,
                    effect: DurableCommandEffect {
                        broadcasts,
                        admission,
                        disconnect_identity,
                    },
                })
            },
        );
        let commit = match commit {
            Ok(commit) => commit,
            Err(ServerError::Sqlite(error)) if sqlite_is_busy(&error) => {
                return Ok(self.durable_error_dispatch(
                    seq,
                    room_id,
                    ChatErrorCode::DurableMutationStoreBusy,
                    "durable mutation store is busy",
                ))
            }
            Err(error) => return Err(error),
        };
        self.durable_effect_dispatch(commit, seq, room_id)
    }

    fn durable_effect_dispatch(
        &self,
        commit: DurableMutationEffectCommit<DurableCommandEffect>,
        seq: u32,
        room_id: Option<RoomId>,
    ) -> ServerResult<DurableMutationDispatch> {
        match commit {
            DurableMutationEffectCommit::Stored {
                result_frame,
                effect,
                pruned,
            } => {
                if let Some(admission) = effect.admission {
                    admission.commit();
                }
                Ok(DurableMutationDispatch {
                    origin: decode_durable_result(&result_frame)?,
                    broadcasts: effect.broadcasts,
                    disconnect_identity: effect.disconnect_identity,
                    pruned,
                })
            }
            DurableMutationEffectCommit::StoredResponse {
                result_frame,
                pruned,
            } => Ok(DurableMutationDispatch {
                origin: decode_durable_result(&result_frame)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned,
            }),
            DurableMutationEffectCommit::Replayed { result_frame } => Ok(DurableMutationDispatch {
                origin: decode_durable_replay_result(&result_frame, seq)?,
                broadcasts: Vec::new(),
                disconnect_identity: None,
                pruned: 0,
            }),
            DurableMutationEffectCommit::Conflict => Ok(self.durable_error_dispatch(
                seq,
                room_id,
                ChatErrorCode::DurableMutationConflict,
                "durable mutation id was reused with different content",
            )),
            DurableMutationEffectCommit::Expired => Ok(self.durable_error_dispatch(
                seq,
                room_id,
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
    ) -> DurableMutationDispatch {
        DurableMutationDispatch {
            origin: self.error_frame(seq, room_id, code, message),
            broadcasts: Vec::new(),
            disconnect_identity: None,
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
        let Some(room) = self.store.room_by_id(room_id)? else {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room not found",
            )]);
        };
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
        if let Some(error) =
            self.reject_room_content_policy(seq, room_id, user.role_bits, "publishing messages")?
        {
            return Ok(vec![error]);
        }
        if !self.slow_mode_enforcement_enabled
            || user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) != 0
        {
            if let Some(error) =
                self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Message)?
            {
                return Ok(vec![error]);
            }
            self.store.join_room(room_id, user.user_id)?;
            let event = self
                .store
                .append_event(room_id, Some(user.user_id), event_kind(body))?;
            return Ok(vec![Frame::new(
                ChatOp::RoomEvent,
                seq,
                Some(room_id),
                FrameBody::Fields(vec![event_to_value(&event)?]),
            )]);
        }
        let rate = match self.reserve_rate(peer, RateKind::Message)? {
            RateAdmission::Admitted(admission) => admission,
            RateAdmission::Rejected => {
                return Ok(vec![self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::RateLimited,
                    "message rate limit exceeded",
                )])
            }
        };
        let slow_mode = match self.slow_mode.reserve(
            room_id,
            user.user_id,
            room.slow_mode_seconds,
            Instant::now(),
        )? {
            SlowModeMonotonicAdmission::Disabled => None,
            SlowModeMonotonicAdmission::Admitted(reservation) => Some(reservation),
            SlowModeMonotonicAdmission::Rejected {
                retry_after_seconds,
            } => {
                return Ok(vec![self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::SlowModeActive,
                    &format!("room slow mode is active; retry in {retry_after_seconds}s"),
                )])
            }
            SlowModeMonotonicAdmission::Saturated => {
                return Ok(vec![self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::SlowModeActive,
                    "room slow-mode admission is saturated",
                )])
            }
        };
        let admission = RoomPublicationAdmission { rate, slow_mode };
        match self.store.commit_room_publication_with_slow_mode(
            room_id,
            user.user_id,
            event_kind(body),
            i64::try_from(unix_seconds()).unwrap_or(i64::MAX),
        )? {
            SlowModeRoomPublication::Stored {
                event,
                admission: persisted,
            } => {
                if matches!(persisted, SlowModeAdmission::Admitted { .. }) {
                    admission.commit();
                } else if let Some(rate) = admission.rate {
                    rate.commit();
                }
                Ok(vec![Frame::new(
                    ChatOp::RoomEvent,
                    seq,
                    Some(room_id),
                    FrameBody::Fields(vec![event_to_value(&event)?]),
                )])
            }
            SlowModeRoomPublication::Rejected {
                retry_after_seconds,
            } => Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::SlowModeActive,
                &format!("room slow mode is active; retry in {retry_after_seconds}s"),
            )]),
            SlowModeRoomPublication::RoomNotFound => Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room not found",
            )]),
            SlowModeRoomPublication::Saturated => Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::SlowModeActive,
                "room slow-mode admission is saturated",
            )]),
        }
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
        if let Some(error) =
            self.reject_room_content_policy(seq, room_id, user.role_bits, "publishing notices")?
        {
            return Ok(vec![error]);
        }
        if user.role_bits & (ROLE_MODERATOR | ROLE_ADMIN) == 0 {
            return Ok(vec![self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::PermissionDenied,
                "room notices require moderator or admin role",
            )]);
        }
        if let Some(error) =
            self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Message)?
        {
            return Ok(vec![error]);
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
            FrameBody::Fields(vec![event_to_value(&event)?]),
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
            .collect::<ServerResult<Vec<_>>>()?;
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

        let values = events
            .iter()
            .map(event_to_value)
            .collect::<ServerResult<Vec<_>>>()?;
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
        if let Some(error) =
            self.reject_room_content_policy(seq, room_id, user.role_bits, "publishing uploads")?
        {
            return Ok(vec![error]);
        }
        if let Some(error) =
            self.reject_if_rate_limited(peer, seq, Some(room_id), RateKind::Command)?
        {
            return Ok(vec![error]);
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
        let user = self.ensure_peer(peer)?;
        if let Some(error) = self.reject_room_content_policy(
            0,
            upload.room_id,
            user.role_bits,
            "publishing uploads",
        )? {
            return Ok(vec![error]);
        }
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
                FrameBody::Fields(vec![event_to_value(&event)?]),
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

    fn reject_room_content_policy(
        &self,
        seq: u32,
        room_id: RoomId,
        actor_role_bits: u64,
        operation: &str,
    ) -> ServerResult<Option<Frame>> {
        match self
            .store
            .room_content_mutation_admission(room_id, actor_role_bits)?
        {
            RoomContentMutationAdmission::Allowed => Ok(None),
            RoomContentMutationAdmission::RoomNotFound => Ok(Some(self.error_frame(
                seq,
                Some(room_id),
                ChatErrorCode::RoomNotFound,
                "room not found",
            ))),
            RoomContentMutationAdmission::AnnouncementRestricted => {
                Ok(Some(self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::RoomPolicyRestricted,
                    &format!(
                        "{operation} is restricted to moderators and administrators in this announcement room"
                    ),
                )))
            }
        }
    }

    fn durable_room_policy_rejection(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        seq: u32,
        room_id: RoomId,
        actor_role_bits: u64,
        operation: &str,
    ) -> ServerResult<Option<Vec<u8>>> {
        match OmenchatStore::durable_room_content_mutation_admission(
            transaction,
            room_id,
            actor_role_bits,
        )? {
            RoomContentMutationAdmission::Allowed => Ok(None),
            RoomContentMutationAdmission::RoomNotFound => {
                self.encode_durable_result(self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::RoomNotFound,
                    "room not found",
                ))
                .map(Some)
            }
            RoomContentMutationAdmission::AnnouncementRestricted => self
                .encode_durable_result(self.error_frame(
                    seq,
                    Some(room_id),
                    ChatErrorCode::RoomPolicyRestricted,
                    &format!(
                        "{operation} is restricted to moderators and administrators in this announcement room"
                    ),
                ))
                .map(Some),
        }
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

fn room_to_value_for_shape(
    room: &ServerRoom,
    room_catalog_shape: RoomCatalogShape,
) -> ServerResult<FrameValue> {
    RoomCatalogEntry {
        room_id: room.room_id,
        name: room.name.clone(),
        topic: room.topic.clone(),
        room_revision: room.room_revision,
        policy_bits: room.policy_bits,
        slow_mode_seconds: room.slow_mode_seconds,
    }
    .into_frame_value_for_shape(room_catalog_shape)
    .map_err(|error| ServerError::Message(format!("stored room cannot be encoded: {error}")))
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

fn event_to_value(event: &ServerRoomEvent) -> ServerResult<FrameValue> {
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
    if let Some(metadata) = event.metadata.as_ref() {
        append_rich_message_event_metadata(&mut fields, metadata).map_err(|error| {
            ServerError::Message(format!("stored rich message metadata is invalid: {error}"))
        })?;
    }
    Ok(FrameValue::Array(fields))
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

fn decode_durable_replay_result(bytes: &[u8], request_seq: u32) -> ServerResult<Frame> {
    let mut frame = decode_durable_result(bytes)?;
    frame.seq = request_seq;
    Ok(frame)
}

fn reaction_snapshot_fingerprint(snapshot: &ReactionSnapshot) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    snapshot.target_event_ids.hash(&mut hasher);
    for entry in &snapshot.entries {
        entry.target_event_id.hash(&mut hasher);
        entry.actor_user_id.hash(&mut hasher);
        entry.token.as_str().hash(&mut hasher);
        entry.created_at_unix.hash(&mut hasher);
    }
    hasher.finish()
}

fn message_revision_snapshot_fingerprint(snapshot: &MessageRevisionSnapshot) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    snapshot.target_event_ids.hash(&mut hasher);
    for entry in &snapshot.entries {
        entry.target_event_id.hash(&mut hasher);
        entry.latest_revision_event_id.hash(&mut hasher);
        (entry.action as u8).hash(&mut hasher);
        entry.actor_user_id.hash(&mut hasher);
        entry.at_unix.hash(&mut hasher);
        entry.replacement.hash(&mut hasher);
        entry.revision_number.hash(&mut hasher);
    }
    hasher.finish()
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

    fn assert_replayed_response(replayed: &Frame, stored: &Frame, request_seq: u32) {
        let mut expected = stored.clone();
        expected.seq = request_seq;
        assert_eq!(replayed, &expected);
    }
    use crate::protocol::batch::{
        decode_compressed_values_body, decode_compressed_values_payload, decode_resource_offer_body,
    };
    use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue};
    use crate::store::OmenchatStore;

    fn expected_moderation_audit_page_op() -> ChatOp {
        if cfg!(feature = "omenchat-moderation-audit-resource-qualification") {
            ChatOp::ModerationAuditResource
        } else {
            ChatOp::ModerationAuditInline
        }
    }

    fn moderation_audit_values(engine: &SessionEngine, frame: &Frame) -> Vec<FrameValue> {
        match frame.op {
            ChatOp::ModerationAuditInline => {
                decode_compressed_values_body(&frame.body).expect("inline audit values")
            }
            ChatOp::ModerationAuditResource => {
                let offer = decode_resource_offer_body(&frame.body).expect("audit Resource offer");
                let payload = engine
                    .resource_payload(&offer.resource_id)
                    .expect("audit Resource lookup")
                    .expect("audit Resource payload");
                decode_compressed_values_payload(&payload).expect("audit Resource values")
            }
            op => panic!("unexpected moderation audit page operation: {op:?}"),
        }
    }

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
        durable_envelope_body(op, room_id, mutation_marker, FrameBody::Text(body.into()))
    }

    fn durable_envelope_body(
        op: ChatOp,
        room_id: RoomId,
        mutation_marker: u8,
        body: FrameBody,
    ) -> DurableMutationEnvelope {
        durable_envelope_optional_room(op, Some(room_id), mutation_marker, body)
    }

    fn durable_envelope_optional_room(
        op: ChatOp,
        room_id: Option<RoomId>,
        mutation_marker: u8,
        body: FrameBody,
    ) -> DurableMutationEnvelope {
        DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([mutation_marker; 16]),
            request_hash: canonical_mutation_request_hash(op, room_id, &body)
                .expect("canonical hash"),
            body,
        }
    }

    fn rich_message_envelope(
        room_id: RoomId,
        mutation_marker: u8,
        reply_to: Option<crate::protocol::ReplyReference>,
        mentioned_user_ids: Vec<UserId>,
    ) -> DurableMutationEnvelope {
        durable_envelope_body(
            ChatOp::RoomMessage,
            room_id,
            mutation_marker,
            RichMessageBody {
                body: "rich message".into(),
                reply_to,
                mentioned_user_ids,
            }
            .into_frame_body()
            .expect("bounded rich message"),
        )
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
    fn durable_capability_request_is_explicitly_accepted() {
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
        assert_eq!(fields.len(), 7);
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![crate::protocol::DURABLE_MUTATION_CAPABILITY.into()],
            }))
        );
    }

    #[test]
    fn message_revisions_capability_requires_explicit_durable_request() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::MESSAGE_REVISIONS_CAPABILITY.into(),
                ],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([14; 16])),
            },
        )
        .expect("message revision capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 2, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::MESSAGE_REVISIONS_CAPABILITY.into(),
                ],
            }))
        );
    }

    #[test]
    fn pin_capability_requires_explicit_durable_request() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::ROOM_PINS_CAPABILITY.into(),
                ],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([15; 16])),
            },
        )
        .expect("dormant pin capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 2, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::ROOM_PINS_CAPABILITY.into(),
                ],
            }))
        );

        let pin_only = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![crate::protocol::ROOM_PINS_CAPABILITY.into()],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([16; 16])),
            },
        )
        .expect("pin-only capability request");
        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 3, None, pin_only))
            .expect("pin-only session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(None)
        );
    }

    #[test]
    fn moderation_audit_capability_follows_product_feature() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::MODERATION_AUDIT_CAPABILITY.into(),
                ],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([17; 16])),
            },
        )
        .expect("dormant moderation audit capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 4, None, request))
            .expect("session open");
        let expected = if cfg!(feature = "omenchat-moderation-audit") {
            vec![
                crate::protocol::MODERATION_AUDIT_CAPABILITY.into(),
                crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
            ]
        } else {
            vec![crate::protocol::DURABLE_MUTATION_CAPABILITY.into()]
        };
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: expected,
            }))
        );

        let audit_only = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![crate::protocol::MODERATION_AUDIT_CAPABILITY.into()],
                client_instance_id: None,
            },
        )
        .expect("audit-only capability request");
        let response = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::SessionOpen, 5, None, audit_only),
            )
            .expect("audit-only session open");
        let expected = cfg!(feature = "omenchat-moderation-audit").then(|| {
            crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![crate::protocol::MODERATION_AUDIT_CAPABILITY.into()],
            }
        });
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(expected)
        );
    }

    #[test]
    #[cfg(not(feature = "omenchat-announcement-rooms"))]
    fn announcement_rooms_capability_remains_inactive_without_feature() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::ANNOUNCEMENT_ROOMS_CAPABILITY.into(),
                ],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([18; 16])),
            },
        )
        .expect("inactive announcement-room capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 6, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![crate::protocol::DURABLE_MUTATION_CAPABILITY.into()],
            }))
        );

        let policy_only = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![crate::protocol::ANNOUNCEMENT_ROOMS_CAPABILITY.into()],
                client_instance_id: None,
            },
        )
        .expect("announcement-room-only capability request");
        let response = engine
            .handle_frame(
                &peer(),
                Frame::new(ChatOp::SessionOpen, 7, None, policy_only),
            )
            .expect("announcement-room-only session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(None)
        );
    }

    #[test]
    #[cfg(feature = "omenchat-announcement-rooms")]
    fn announcement_room_product_feature_enables_the_normal_engine() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        assert!(engine.announcement_rooms_enabled);
    }

    #[test]
    fn test_enabled_announcement_rooms_accepts_only_an_explicit_request_and_encodes_policy() {
        let engine =
            SessionEngine::with_test_announcement_rooms(OmenchatStore::in_memory().expect("store"));
        let announcement_room = engine
            .store
            .set_room_announcement_policy(1, true)
            .expect("announcement policy");
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![crate::protocol::ANNOUNCEMENT_ROOMS_CAPABILITY.into()],
                client_instance_id: None,
            },
        )
        .expect("announcement capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 8, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![crate::protocol::ANNOUNCEMENT_ROOMS_CAPABILITY.into(),],
            }))
        );
        let FrameBody::Fields(fields) = &response[0].body else {
            panic!("session acceptance must contain fields");
        };
        let Some(FrameValue::Array(rooms)) = fields.get(1) else {
            panic!("session acceptance must contain a room catalog");
        };
        assert_eq!(rooms.len(), 1);
        assert_eq!(
            crate::protocol::RoomCatalogEntry::from_frame_value(&rooms[0], true)
                .expect("negotiated room"),
            crate::protocol::RoomCatalogEntry {
                room_id: 1,
                name: "lobby".into(),
                topic: announcement_room.topic,
                room_revision: announcement_room.room_revision,
                policy_bits: crate::protocol::ROOM_POLICY_ANNOUNCEMENT,
                slow_mode_seconds: announcement_room.slow_mode_seconds,
            }
        );
    }

    #[test]
    fn slow_mode_product_feature_requires_durable_mutations_and_encodes_exact_shape() {
        let engine = SessionEngine::with_test_slow_mode(OmenchatStore::in_memory().expect("store"));
        engine
            .store
            .set_room_slow_mode_seconds(1, 30)
            .expect("slow mode");
        let request =
            |requested_capabilities: Vec<String>,
             client_instance_id: Option<crate::protocol::ClientInstanceId>| {
                crate::protocol::with_session_open_negotiation(
                    FrameBody::Text("Alice".into()),
                    &crate::protocol::SessionOpenNegotiation {
                        requested_capabilities,
                        client_instance_id,
                    },
                )
                .expect("capability request")
            };

        let accepted = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::SessionOpen,
                    9,
                    None,
                    request(
                        vec![
                            crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                            crate::protocol::ROOM_SLOW_MODE_CAPABILITY.into(),
                        ],
                        Some(crate::protocol::ClientInstanceId::new([19; 16])),
                    ),
                ),
            )
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&accepted[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    crate::protocol::ROOM_SLOW_MODE_CAPABILITY.into(),
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                ],
            }))
        );
        let FrameBody::Fields(fields) = &accepted[0].body else {
            panic!("session acceptance fields");
        };
        let Some(FrameValue::Array(rooms)) = fields.get(1) else {
            panic!("session acceptance room catalog");
        };
        let room = crate::protocol::RoomCatalogEntry::from_frame_value_for_shape(
            rooms.first().expect("lobby"),
            crate::protocol::RoomCatalogShape::SlowMode,
        )
        .expect("six-field slow-mode room");
        assert_eq!(room.slow_mode_seconds, 30);

        let configured_engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let response = configured_engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::SessionOpen,
                    10,
                    None,
                    request(
                        vec![
                            crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                            crate::protocol::ROOM_SLOW_MODE_CAPABILITY.into(),
                        ],
                        Some(crate::protocol::ClientInstanceId::new([20; 16])),
                    ),
                ),
            )
            .expect("session open with configured capability");
        let negotiation = crate::protocol::parse_session_accept_negotiation(&response[0].body)
            .expect("session acceptance negotiation")
            .expect("explicit negotiation");
        assert_eq!(
            negotiation
                .accepted_capabilities
                .iter()
                .any(|capability| capability == crate::protocol::ROOM_SLOW_MODE_CAPABILITY),
            cfg!(feature = "omenchat-slow-mode")
        );
        let FrameBody::Fields(fields) = &response[0].body else {
            panic!("session acceptance fields");
        };
        let Some(FrameValue::Array(rooms)) = fields.get(1) else {
            panic!("room catalog");
        };
        let expected_shape = if cfg!(feature = "omenchat-slow-mode") {
            crate::protocol::RoomCatalogShape::SlowMode
        } else {
            crate::protocol::RoomCatalogShape::Legacy
        };
        crate::protocol::RoomCatalogEntry::from_frame_value_for_shape(
            rooms.first().expect("lobby"),
            expected_shape,
        )
        .expect("feature-selected room shape");
    }

    #[test]
    fn announcement_room_rejects_legacy_member_content_without_rate_or_queue_side_effects() {
        let root = temp_upload_root("announcement-policy");
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_messages_per_minute: 1,
                rate_commands_per_minute: 1,
                upload_quota_bytes: 1024,
                upload_cache_root: Some(root.clone()),
                ..SessionLimits::default()
            },
        );
        let room = engine
            .store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = engine.ensure_peer(&peer()).expect("user");
        engine
            .store
            .join_room(room.room_id, user.user_id)
            .expect("join");
        engine
            .store
            .set_room_announcement_policy(room.room_id, true)
            .expect("announcement policy");

        for (seq, op, body) in [
            (1, ChatOp::RoomMessage, FrameBody::Text("message".into())),
            (2, ChatOp::RoomAction, FrameBody::Text("action".into())),
            (3, ChatOp::RoomNotice, FrameBody::Text("notice".into())),
            (
                4,
                ChatOp::UploadOffer,
                FrameBody::Fields(vec![
                    FrameValue::String("blocked.bin".into()),
                    FrameValue::U64(1),
                ]),
            ),
        ] {
            let response = engine
                .handle_frame(&peer(), Frame::new(op, seq, Some(room.room_id), body))
                .expect("policy response");
            assert_eq!(response.len(), 1);
            assert_eq!(response[0].op, ChatOp::Error);
            assert_eq!(
                frame_error_code(&response[0]),
                Some(ChatErrorCode::RoomPolicyRestricted as u16 as u64)
            );
        }
        assert!(engine
            .store
            .latest_events(room.room_id, 10)
            .expect("events")
            .is_empty());
        assert_eq!(
            engine
                .pending_upload_metrics()
                .expect("pending upload metrics"),
            (0, 0, 0, 0)
        );

        engine
            .store
            .set_room_announcement_policy(room.room_id, false)
            .expect("ordinary policy");
        let accepted = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomMessage,
                    5,
                    Some(room.room_id),
                    FrameBody::Text("accepted".into()),
                ),
            )
            .expect("ordinary message");
        assert_eq!(accepted[0].op, ChatOp::RoomEvent);
        let upload = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::UploadOffer,
                    6,
                    Some(room.room_id),
                    FrameBody::Fields(vec![
                        FrameValue::String("accepted.bin".into()),
                        FrameValue::U64(1),
                    ]),
                ),
            )
            .expect("ordinary upload");
        assert_eq!(upload[0].op, ChatOp::UploadAccept);
        let resource_id = frame_body_values(&upload[0].body)
            .and_then(|values| values.first())
            .and_then(frame_value_string)
            .expect("accepted resource id")
            .to_owned();
        engine
            .store
            .set_room_announcement_policy(room.room_id, true)
            .expect("policy changed before resource");
        let rejected_resource = engine
            .handle_upload_resource(&peer(), &resource_id, vec![1])
            .expect("resource policy response");
        assert_eq!(rejected_resource[0].op, ChatOp::Error);
        assert_eq!(
            frame_error_code(&rejected_resource[0]),
            Some(ChatErrorCode::RoomPolicyRestricted as u16 as u64)
        );
        assert!(engine
            .store
            .upload_file(&resource_id)
            .expect("upload row")
            .is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn announcement_room_durable_policy_is_transactional_replay_safe_and_role_aware() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let room = engine
            .store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        engine
            .store
            .set_room_announcement_policy(room.room_id, true)
            .expect("announcement policy");
        let client_instance_id = ClientInstanceId::new([72; 16]);
        let envelope = durable_envelope(
            ChatOp::RoomMessage,
            room.room_id,
            73,
            "blocked durable message",
        );
        let rejected = engine
            .handle_durable_mutation(
                &peer(),
                10,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope.clone(),
            )
            .expect("policy rejection");
        assert_eq!(
            frame_error_code(&rejected.origin),
            Some(ChatErrorCode::RoomPolicyRestricted as u16 as u64)
        );
        assert!(rejected.broadcasts.is_empty());
        assert!(engine
            .store
            .latest_events(room.room_id, 10)
            .expect("events")
            .is_empty());

        let user = engine.ensure_peer(&peer()).expect("user");
        engine
            .store
            .set_user_role_bits(user.user_id, ROLE_MODERATOR)
            .expect("moderator");
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                11,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope,
            )
            .expect("exact replay");
        assert_replayed_response(&replayed.origin, &rejected.origin, 11);
        assert!(replayed.broadcasts.is_empty());

        let accepted = engine
            .handle_durable_mutation(
                &peer(),
                12,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(
                    ChatOp::RoomMessage,
                    room.room_id,
                    74,
                    "moderator publication",
                ),
            )
            .expect("moderator publication");
        assert_eq!(accepted.origin.op, ChatOp::MessageAck);
        assert_eq!(accepted.broadcasts.len(), 1);
        let target_event_id =
            engine.store.latest_events(room.room_id, 1).expect("events")[0].event_id;

        engine
            .store
            .set_user_role_bits(user.user_id, 0)
            .expect("standard member");
        for (seq, op, marker, body) in [
            (
                13,
                ChatOp::RoomReaction,
                75,
                ReactionRequest {
                    target_event_id,
                    token: crate::protocol::ReactionToken::Heart,
                    action: crate::protocol::ReactionAction::Add,
                }
                .into_frame_body()
                .expect("reaction"),
            ),
            (
                14,
                ChatOp::RoomMessageRevision,
                76,
                MessageRevisionRequest {
                    target_event_id,
                    action: crate::protocol::MessageRevisionAction::Correct,
                    replacement: Some("blocked correction".into()),
                }
                .into_frame_body()
                .expect("revision"),
            ),
        ] {
            let dispatch = engine
                .handle_durable_mutation_with_active_peers(
                    DurableMutationPeerContext {
                        peer: &peer(),
                        active_room_peers: &[],
                        durable_notice_ack: true,
                        reply_mentions: false,
                        reactions: true,
                        message_revisions: true,
                        pins: false,
                    },
                    seq,
                    Some(room.room_id),
                    op,
                    client_instance_id,
                    durable_envelope_body(op, room.room_id, marker, body),
                )
                .expect("policy rejection");
            assert_eq!(
                frame_error_code(&dispatch.origin),
                Some(ChatErrorCode::RoomPolicyRestricted as u16 as u64)
            );
            assert!(dispatch.broadcasts.is_empty());
        }
        assert_eq!(
            engine.store.reaction_row_counts().expect("reaction counts"),
            (0, 0)
        );
        assert_eq!(
            engine
                .store
                .message_revision_row_counts()
                .expect("revision counts"),
            (0, 0)
        );
    }

    #[test]
    fn test_moderation_audit_capability_is_independent_of_durable_mutations() {
        let engine = SessionEngine::with_test_moderation_audit(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits::default(),
        );
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![crate::protocol::MODERATION_AUDIT_CAPABILITY.into()],
                client_instance_id: None,
            },
        )
        .expect("moderation audit capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 6, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![crate::protocol::MODERATION_AUDIT_CAPABILITY.into()],
            }))
        );
    }

    #[test]
    fn moderation_audit_paging_is_authorized_exclusive_and_bounded() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let actor = store
            .ensure_user(&peer().identity_hash, "Alice", Some("lxmf-a"))
            .expect("actor");
        store
            .set_user_role_bits(actor.user_id, ROLE_ADMIN)
            .expect("admin");
        store
            .join_room(room.room_id, actor.user_id)
            .expect("actor join");
        store.ensure_user(b"peer-b", "Bob", None).expect("target");
        let engine = SessionEngine::with_test_moderation_audit(
            store,
            SessionLimits {
                large_batch_threshold_bytes: usize::MAX,
                ..SessionLimits::default()
            },
        );
        let client_instance_id = ClientInstanceId::new([18; 16]);
        for marker in 1..=3 {
            let envelope = durable_envelope(ChatOp::Command, room.room_id, marker, "role Bob mod");
            let response = engine
                .handle_durable_mutation(
                    &peer(),
                    u32::from(marker),
                    Some(room.room_id),
                    ChatOp::Command,
                    client_instance_id,
                    envelope,
                )
                .expect("durable role command");
            assert_eq!(response.origin.op, ChatOp::CommandResult);
        }

        let first = engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    20,
                    Some(room.room_id),
                    ModerationAuditRequest {
                        before_audit_id: None,
                        limit: 2,
                    }
                    .into_frame_body()
                    .expect("request"),
                ),
                &[],
                true,
            )
            .expect("first page");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].op, expected_moderation_audit_page_op());
        let first_values = moderation_audit_values(&engine, &first[0]);
        let first_page = crate::protocol::ModerationAuditPage::from_frame_values(&first_values)
            .expect("first page");
        assert_eq!(first_page.records.len(), 2);
        assert!(
            first_page.records[0].audit_id > first_page.records[1].audit_id,
            "pages must be newest-first"
        );

        let cursor = first_page.records[1].audit_id;
        let second = engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    21,
                    Some(room.room_id),
                    ModerationAuditRequest {
                        before_audit_id: Some(cursor),
                        limit: 2,
                    }
                    .into_frame_body()
                    .expect("cursor request"),
                ),
                &[],
                true,
            )
            .expect("second page");
        assert_eq!(
            second.iter().map(|frame| frame.op).collect::<Vec<_>>(),
            vec![
                expected_moderation_audit_page_op(),
                ChatOp::ModerationAuditEnd,
            ]
        );
        let second_values = moderation_audit_values(&engine, &second[0]);
        let second_page = crate::protocol::ModerationAuditPage::from_frame_values(&second_values)
            .expect("second page");
        assert_eq!(second_page.records.len(), 1);
        assert!(second_page.records[0].audit_id < cursor);

        engine
            .store
            .set_user_role_bits(actor.user_id, 0)
            .expect("remove role");
        let denied = engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    22,
                    Some(room.room_id),
                    ModerationAuditRequest {
                        before_audit_id: None,
                        limit: 2,
                    }
                    .into_frame_body()
                    .expect("denied request"),
                ),
                &[],
                true,
            )
            .expect("role loss response");
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].op, ChatOp::Error);
        assert!(matches!(
            &denied[0].body,
            FrameBody::Fields(fields)
                if fields.first()
                    == Some(&FrameValue::U64(ChatErrorCode::PermissionDenied as u16 as u64))
        ));

        engine
            .store
            .set_user_role_bits(actor.user_id, ROLE_ADMIN)
            .expect("restore role");
        engine
            .store
            .leave_room(room.room_id, actor.user_id)
            .expect("leave room");
        let denied = engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    23,
                    Some(room.room_id),
                    ModerationAuditRequest {
                        before_audit_id: None,
                        limit: 2,
                    }
                    .into_frame_body()
                    .expect("membership-loss request"),
                ),
                &[],
                true,
            )
            .expect("membership loss response");
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].op, ChatOp::Error);
    }

    #[test]
    fn moderation_audit_resource_matches_inline_and_malformed_requests_fail_closed() {
        fn seeded_engine(threshold: usize) -> (SessionEngine, RoomId) {
            let store = OmenchatStore::in_memory().expect("store");
            let room = store
                .room_by_name("lobby")
                .expect("room query")
                .expect("room");
            let actor = store
                .ensure_user(&peer().identity_hash, "Alice", Some("lxmf-a"))
                .expect("actor");
            store
                .set_user_role_bits(actor.user_id, ROLE_ADMIN)
                .expect("admin");
            store.join_room(room.room_id, actor.user_id).expect("join");
            store.ensure_user(b"peer-b", "Bob", None).expect("target");
            let engine = SessionEngine::with_test_moderation_audit(
                store,
                SessionLimits {
                    large_batch_threshold_bytes: threshold,
                    ..SessionLimits::default()
                },
            );
            engine
                .handle_durable_mutation(
                    &peer(),
                    1,
                    Some(room.room_id),
                    ChatOp::Command,
                    ClientInstanceId::new([19; 16]),
                    durable_envelope(ChatOp::Command, room.room_id, 1, "role Bob mod"),
                )
                .expect("seed audit");
            (engine, room.room_id)
        }

        let request = ModerationAuditRequest {
            before_audit_id: None,
            limit: 2,
        }
        .into_frame_body()
        .expect("request");
        let (inline_engine, room_id) = seeded_engine(usize::MAX);
        let inline = inline_engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    30,
                    Some(room_id),
                    request.clone(),
                ),
                &[],
                true,
            )
            .expect("inline");
        let inline_values = moderation_audit_values(&inline_engine, &inline[0]);

        let (resource_engine, resource_room_id) = seeded_engine(1);
        let resource = resource_engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    30,
                    Some(resource_room_id),
                    request,
                ),
                &[],
                true,
            )
            .expect("resource");
        assert_eq!(resource[0].op, ChatOp::ModerationAuditResource);
        let offer = decode_resource_offer_body(&resource[0].body).expect("offer");
        assert!(offer.purpose.starts_with("moderation-audit:30:"));
        let payload = resource_engine
            .resource_payload(&offer.resource_id)
            .expect("resource lookup")
            .expect("resource payload");
        let resource_values = decode_compressed_values_payload(&payload).expect("resource values");
        let normalize_committed_at = |mut values: Vec<FrameValue>| {
            for value in &mut values {
                let FrameValue::Array(fields) = value else {
                    panic!("moderation audit value must be an array");
                };
                assert!(matches!(fields.get(7), Some(FrameValue::U64(_))));
                fields[7] = FrameValue::U64(0);
            }
            values
        };
        assert_eq!(
            normalize_committed_at(inline_values),
            normalize_committed_at(resource_values)
        );

        let malformed = resource_engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    31,
                    Some(resource_room_id),
                    FrameBody::Empty,
                ),
                &[],
                true,
            )
            .expect("malformed response");
        assert_eq!(malformed.len(), 1);
        assert_eq!(malformed[0].op, ChatOp::Error);

        let oversized = resource_engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    32,
                    Some(resource_room_id),
                    FrameBody::Fields(vec![
                        FrameValue::String(
                            crate::protocol::MODERATION_AUDIT_REQUEST_BODY_TAG.into(),
                        ),
                        FrameValue::Nil,
                        FrameValue::U64(
                            (crate::protocol::MODERATION_AUDIT_PAGE_MAX_ENTRIES + 1) as u64,
                        ),
                    ]),
                ),
                &[],
                true,
            )
            .expect("oversized response");
        assert_eq!(oversized.len(), 1);
        assert_eq!(oversized[0].op, ChatOp::Error);

        let not_negotiated = resource_engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                Frame::new(
                    ChatOp::ModerationAuditBefore,
                    33,
                    Some(resource_room_id),
                    ModerationAuditRequest {
                        before_audit_id: None,
                        limit: 1,
                    }
                    .into_frame_body()
                    .expect("request"),
                ),
                &[],
                false,
            )
            .expect("not negotiated response");
        assert_eq!(not_negotiated.len(), 1);
        assert_eq!(not_negotiated[0].op, ChatOp::Error);
    }

    #[test]
    fn moderation_audit_page_survives_server_restart_and_duplicate_reads_are_stable() {
        let path = temp_store_path("moderation-audit-restart");
        let room_id;
        {
            let store = OmenchatStore::open(&path).expect("store");
            let room = store
                .ensure_room("lobby", Some("Default OMENchat lobby"))
                .expect("room");
            room_id = room.room_id;
            let actor = store
                .ensure_user(&peer().identity_hash, "Alice", Some("lxmf-a"))
                .expect("actor");
            store
                .set_user_role_bits(actor.user_id, ROLE_ADMIN)
                .expect("admin");
            store.join_room(room_id, actor.user_id).expect("actor join");
            store.ensure_user(b"peer-b", "Bob", None).expect("target");
            let engine = SessionEngine::with_test_moderation_audit(store, SessionLimits::default());
            let committed = engine
                .handle_durable_mutation(
                    &peer(),
                    1,
                    Some(room_id),
                    ChatOp::Command,
                    ClientInstanceId::new([20; 16]),
                    durable_envelope(ChatOp::Command, room_id, 1, "role Bob mod"),
                )
                .expect("committed moderation");
            assert_eq!(committed.origin.op, ChatOp::CommandResult);
        }

        let engine = SessionEngine::with_test_moderation_audit(
            OmenchatStore::open(&path).expect("reopened store"),
            SessionLimits::default(),
        );
        let request = Frame::new(
            ChatOp::ModerationAuditBefore,
            40,
            Some(room_id),
            ModerationAuditRequest {
                before_audit_id: None,
                limit: 10,
            }
            .into_frame_body()
            .expect("request"),
        );
        let first = engine
            .handle_frame_with_active_peers_and_moderation_audit(
                &peer(),
                request.clone(),
                &[],
                true,
            )
            .expect("first read");
        let duplicate = engine
            .handle_frame_with_active_peers_and_moderation_audit(&peer(), request, &[], true)
            .expect("duplicate read");
        assert_eq!(duplicate, first);
        assert_eq!(
            first.iter().map(|frame| frame.op).collect::<Vec<_>>(),
            vec![
                expected_moderation_audit_page_op(),
                ChatOp::ModerationAuditEnd,
            ]
        );
        let values = moderation_audit_values(&engine, &first[0]);
        let page = crate::protocol::ModerationAuditPage::from_frame_values(&values).expect("page");
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].action, ModerationAuditAction::RoleChange);

        drop(engine);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    }

    #[test]
    fn dormant_pin_executor_is_transactional_role_scoped_and_replays_after_restart() {
        let path = temp_store_path("pin-replay");
        let (room_id, target_event_id, actor_user_id) = {
            let store = OmenchatStore::open(&path).expect("store");
            let room = store.ensure_room("lobby", None).expect("room");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("user");
            store.join_room(room.room_id, user.user_id).expect("join");
            store
                .set_user_role_bits(user.user_id, ROLE_MODERATOR)
                .expect("moderator role");
            let target = store
                .append_event(
                    room.room_id,
                    Some(user.user_id),
                    ServerRoomEventKind::Message {
                        body: "pin target".into(),
                    },
                )
                .expect("target");
            (room.room_id, target.event_id, user.user_id)
        };
        let request = PinRequest {
            target_event_id,
            action: crate::protocol::PinAction::Pin,
        };
        let envelope = durable_envelope_body(
            ChatOp::RoomPin,
            room_id,
            75,
            request.into_frame_body().expect("pin request"),
        );
        let client_instance_id = ClientInstanceId::new([74; 16]);
        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let rejected = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: true,
                    pins: false,
                },
                9,
                Some(room_id),
                ChatOp::RoomPin,
                client_instance_id,
                envelope.clone(),
            )
            .expect("unbound pin");
        assert_eq!(
            frame_error_code(&rejected.origin),
            Some(ChatErrorCode::DurableMutationNotNegotiated as u16 as u64)
        );
        assert_eq!(
            engine.store.pin_row_counts().expect("empty pin rows"),
            (0, 0)
        );

        let stored = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: true,
                    pins: true,
                },
                10,
                Some(room_id),
                ChatOp::RoomPin,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored pin");
        assert_eq!(stored.origin.op, ChatOp::PinAck);
        let stored_ack = PinAck::from_frame_body(&stored.origin.body).expect("pin ack");
        assert_eq!(stored_ack.actor_user_id, actor_user_id);
        assert!(stored_ack.changed);
        assert_eq!(stored.broadcasts.len(), 1);
        assert_eq!(stored.broadcasts[0].op, ChatOp::PinEvent);
        crate::protocol::PinEvent::from_frame_body(&stored.broadcasts[0].body).expect("pin event");
        let snapshot = engine
            .pin_snapshot_frame(10, room_id, &[target_event_id])
            .expect("pin snapshot");
        assert_eq!(snapshot.op, ChatOp::PinSnapshot);
        let values = crate::protocol::batch::decode_compressed_values_body(&snapshot.body)
            .expect("compressed snapshot body");
        let snapshot = crate::protocol::PinSnapshot::from_frame_body(&FrameBody::Fields(values))
            .expect("snapshot body");
        assert_eq!(snapshot.target_event_ids, vec![target_event_id]);
        assert_eq!(snapshot.entries.len(), 1);
        drop(engine);

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: true,
                    pins: true,
                },
                11,
                Some(room_id),
                ChatOp::RoomPin,
                client_instance_id,
                envelope,
            )
            .expect("restart replay");
        assert_replayed_response(&replayed.origin, &stored.origin, 11);
        assert!(replayed.broadcasts.is_empty());
        assert_eq!(engine.store.pin_row_counts().expect("pin rows"), (1, 1));

        let conflict = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: true,
                    pins: true,
                },
                12,
                Some(room_id),
                ChatOp::RoomPin,
                client_instance_id,
                durable_envelope_body(
                    ChatOp::RoomPin,
                    room_id,
                    75,
                    PinRequest {
                        action: crate::protocol::PinAction::Unpin,
                        ..request
                    }
                    .into_frame_body()
                    .expect("conflicting pin request"),
                ),
            )
            .expect("pin conflict");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());
        assert_eq!(engine.store.pin_row_counts().expect("pin rows"), (1, 1));

        drop(engine);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn dormant_pin_executor_denies_joined_non_moderator_without_state() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room lookup")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let target = store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "pin target".into(),
                },
            )
            .expect("target");
        let engine = SessionEngine::new(store);
        let dispatch = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: true,
                    pins: true,
                },
                20,
                Some(room.room_id),
                ChatOp::RoomPin,
                ClientInstanceId::new([76; 16]),
                durable_envelope_body(
                    ChatOp::RoomPin,
                    room.room_id,
                    77,
                    PinRequest {
                        target_event_id: target.event_id,
                        action: crate::protocol::PinAction::Pin,
                    }
                    .into_frame_body()
                    .expect("pin request"),
                ),
            )
            .expect("denied pin");
        assert_eq!(
            frame_error_code(&dispatch.origin),
            Some(ChatErrorCode::PermissionDenied as u16 as u64)
        );
        assert!(dispatch.broadcasts.is_empty());
        assert_eq!(
            engine.store.pin_row_counts().expect("empty pin rows"),
            (0, 0)
        );
    }

    #[test]
    fn dormant_message_revision_executor_replays_across_restart_without_refanout() {
        let path = temp_store_path("message-revision-replay");
        let (room_id, target_event_id) = {
            let store = OmenchatStore::open(&path).expect("store");
            let room = store.ensure_room("lobby", None).expect("room");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("user");
            store.join_room(room.room_id, user.user_id).expect("join");
            let target = store
                .append_event(
                    room.room_id,
                    Some(user.user_id),
                    ServerRoomEventKind::Message {
                        body: "original".into(),
                    },
                )
                .expect("target");
            (room.room_id, target.event_id)
        };
        let client_instance_id = ClientInstanceId::new([71; 16]);
        let request = crate::protocol::MessageRevisionRequest {
            target_event_id,
            action: crate::protocol::MessageRevisionAction::Correct,
            replacement: Some("corrected".into()),
        };
        let envelope = durable_envelope_body(
            ChatOp::RoomMessageRevision,
            room_id,
            72,
            request.clone().into_frame_body().expect("revision request"),
        );
        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let stored = engine
            .handle_durable_message_revision(
                &peer(),
                10,
                Some(room_id),
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored revision");
        assert_eq!(stored.origin.op, ChatOp::MessageRevisionAck);
        let stored_ack = crate::protocol::MessageRevisionAck::from_frame_body(&stored.origin.body)
            .expect("revision acknowledgement");
        assert!(stored_ack.changed);
        assert_eq!(stored_ack.revision_number, 1);
        assert_eq!(stored.broadcasts.len(), 1);
        assert_eq!(stored.broadcasts[0].op, ChatOp::MessageRevisionEvent);
        crate::protocol::MessageRevisionEvent::from_frame_body(&stored.broadcasts[0].body)
            .expect("revision event");
        let inline = engine
            .message_revision_snapshot_frame(10, room_id, &[target_event_id])
            .expect("inline revision snapshot");
        assert_eq!(inline.op, ChatOp::MessageRevisionSnapshotInline);
        let inline_values =
            decode_compressed_values_body(&inline.body).expect("inline snapshot values");
        let inline_snapshot = crate::protocol::MessageRevisionSnapshot::from_frame_body(
            &FrameBody::Fields(inline_values),
        )
        .expect("inline revision snapshot");
        assert_eq!(inline_snapshot.entries.len(), 1);
        drop(engine);

        let engine = SessionEngine::with_limits(
            OmenchatStore::open(&path).expect("reopened store"),
            SessionLimits {
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );
        let replayed = engine
            .handle_durable_message_revision(
                &peer(),
                11,
                Some(room_id),
                client_instance_id,
                envelope,
            )
            .expect("restart replay");
        assert_replayed_response(&replayed.origin, &stored.origin, 11);
        assert!(replayed.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .message_revision_row_counts()
                .expect("revision counts"),
            (1, 1)
        );

        let conflict = engine
            .handle_durable_message_revision(
                &peer(),
                12,
                Some(room_id),
                client_instance_id,
                durable_envelope_body(
                    ChatOp::RoomMessageRevision,
                    room_id,
                    72,
                    crate::protocol::MessageRevisionRequest {
                        replacement: Some("different".into()),
                        ..request
                    }
                    .into_frame_body()
                    .expect("conflicting revision"),
                ),
            )
            .expect("revision conflict");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());

        let resource = engine
            .message_revision_snapshot_frame(13, room_id, &[target_event_id])
            .expect("resource revision snapshot");
        assert_eq!(resource.op, ChatOp::MessageRevisionSnapshotResource);
        let mut transport = crate::transport::CapturedTransport::default();
        crate::transport::send_response_frame(&engine, [0x71; 16], &resource, &mut transport)
            .expect("dispatch revision snapshot resource");
        assert_eq!(transport.frames.len(), 1);
        assert_eq!(transport.resources.len(), 1);

        drop(engine);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn message_revision_result_encoding_failure_rolls_back_effect_and_replay() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room lookup")
            .expect("lobby");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let target = store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "original".into(),
                },
            )
            .expect("target");
        let engine = SessionEngine::new(store);
        let request = crate::protocol::MessageRevisionRequest {
            target_event_id: target.event_id,
            action: crate::protocol::MessageRevisionAction::Correct,
            replacement: Some("corrected".into()),
        };
        let envelope = durable_envelope_body(
            ChatOp::RoomMessageRevision,
            room.room_id,
            74,
            request.into_frame_body().expect("revision body"),
        );
        let oversized_display_peer = ServerPeer {
            display_name: "x".repeat(
                crate::protocol::MESSAGE_REVISION_MAX_ACTOR_DISPLAY_BYTES.saturating_add(1),
            ),
            ..peer()
        };
        let error = engine
            .handle_durable_message_revision(
                &oversized_display_peer,
                20,
                Some(room.room_id),
                ClientInstanceId::new([73; 16]),
                envelope.clone(),
            )
            .expect_err("event codec failure")
            .to_string();
        assert!(error.contains("message revision event encode failed"));
        assert_eq!(
            engine
                .store
                .message_revision_row_counts()
                .expect("rolled-back revision counts"),
            (0, 0)
        );

        let stored = engine
            .handle_durable_message_revision(
                &peer(),
                21,
                Some(room.room_id),
                ClientInstanceId::new([73; 16]),
                envelope,
            )
            .expect("retry after rolled-back codec failure");
        assert_eq!(stored.origin.op, ChatOp::MessageRevisionAck);
        assert_eq!(stored.broadcasts.len(), 1);
        assert_eq!(
            engine
                .store
                .message_revision_row_counts()
                .expect("committed revision counts"),
            (1, 1)
        );
    }

    #[test]
    fn durable_notice_ack_requires_explicit_additional_capability() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::DURABLE_NOTICE_ACK_CAPABILITY.into(),
                ],
                client_instance_id: Some(crate::protocol::ClientInstanceId::new([10; 16])),
            },
        )
        .expect("notice acknowledgement request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 2, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
                    crate::protocol::DURABLE_NOTICE_ACK_CAPABILITY.into(),
                ],
            }))
        );
    }

    #[test]
    fn reply_mentions_capability_is_accepted_only_when_explicitly_requested() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let request = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REPLY_MENTIONS_CAPABILITY.into(),
                ],
                client_instance_id: Some(ClientInstanceId::new([12; 16])),
            },
        )
        .expect("reply capability request");

        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 3, None, request))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REPLY_MENTIONS_CAPABILITY.into(),
                ],
            }))
        );

        let legacy = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::SessionOpen,
                    4,
                    None,
                    FrameBody::Text("Alice".into()),
                ),
            )
            .expect("legacy session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&legacy[0].body),
            Ok(None)
        );
    }

    #[test]
    fn reactions_capability_is_accepted_only_when_requested_and_rejects_unbound_mutations() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let target = store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "target".into(),
                },
            )
            .expect("target");
        let engine = SessionEngine::new(store);
        let open = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REACTIONS_CAPABILITY.into(),
                ],
                client_instance_id: Some(ClientInstanceId::new([41; 16])),
            },
        )
        .expect("reactions capability request");
        let response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 1, None, open))
            .expect("session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&response[0].body),
            Ok(Some(SessionAcceptNegotiation {
                accepted_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REACTIONS_CAPABILITY.into(),
                ],
            }))
        );

        let base_only = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Alice".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
                client_instance_id: Some(ClientInstanceId::new([40; 16])),
            },
        )
        .expect("base capability request");
        let base_response = engine
            .handle_frame(&peer(), Frame::new(ChatOp::SessionOpen, 2, None, base_only))
            .expect("base session open");
        assert_eq!(
            crate::protocol::parse_session_accept_negotiation(&base_response[0].body),
            Ok(Some(SessionAcceptNegotiation {
                accepted_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
            }))
        );

        let body = crate::protocol::ReactionRequest {
            target_event_id: target.event_id,
            token: crate::protocol::ReactionToken::Heart,
            action: crate::protocol::ReactionAction::Add,
        }
        .into_frame_body()
        .expect("reaction body");
        let rejected = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: false,
                    message_revisions: false,
                    pins: false,
                },
                3,
                Some(room.room_id),
                ChatOp::RoomReaction,
                ClientInstanceId::new([41; 16]),
                durable_envelope_body(ChatOp::RoomReaction, room.room_id, 42, body),
            )
            .expect("unnegotiated reaction");
        assert_eq!(
            frame_error_code(&rejected.origin),
            Some(ChatErrorCode::DurableMutationNotNegotiated as u16 as u64)
        );
        assert!(rejected.broadcasts.is_empty());
        assert_eq!(
            engine.store.reaction_row_counts().expect("reaction counts"),
            (0, 0)
        );
    }

    #[test]
    fn durable_reaction_commit_replay_conflict_and_snapshots_survive_restart() {
        let path = temp_store_path("reaction-replay");
        let (room_id, target_event_id) = {
            let store = OmenchatStore::open(&path).expect("store");
            let room = store.ensure_room("lobby", None).expect("room");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("user");
            store.join_room(room.room_id, user.user_id).expect("join");
            let target = store
                .append_event(
                    room.room_id,
                    Some(user.user_id),
                    ServerRoomEventKind::Message {
                        body: "target".into(),
                    },
                )
                .expect("target");
            (room.room_id, target.event_id)
        };
        let client_instance_id = ClientInstanceId::new([43; 16]);
        let request = crate::protocol::ReactionRequest {
            target_event_id,
            token: crate::protocol::ReactionToken::Heart,
            action: crate::protocol::ReactionAction::Add,
        };
        let envelope = durable_envelope_body(
            ChatOp::RoomReaction,
            room_id,
            44,
            request.into_frame_body().expect("reaction body"),
        );
        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let stored = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: false,
                    pins: false,
                },
                10,
                Some(room_id),
                ChatOp::RoomReaction,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored reaction");
        assert_eq!(stored.origin.op, ChatOp::ReactionAck);
        assert!(
            crate::protocol::ReactionAck::from_frame_body(&stored.origin.body)
                .expect("ack")
                .changed
        );
        assert_eq!(stored.broadcasts.len(), 1);
        assert_eq!(stored.broadcasts[0].op, ChatOp::ReactionEvent);
        crate::protocol::ReactionEvent::from_frame_body(&stored.broadcasts[0].body)
            .expect("reaction event");
        let inline = engine
            .reaction_snapshot_frame(10, room_id, &[target_event_id])
            .expect("inline reaction snapshot");
        assert_eq!(inline.op, ChatOp::ReactionSnapshotInline);
        let inline_values =
            decode_compressed_values_body(&inline.body).expect("inline snapshot values");
        let inline_snapshot =
            crate::protocol::ReactionSnapshot::from_frame_body(&FrameBody::Fields(inline_values))
                .expect("inline reaction snapshot");
        assert_eq!(inline_snapshot.target_event_ids, vec![target_event_id]);
        assert_eq!(inline_snapshot.entries.len(), 1);
        drop(engine);

        let engine = SessionEngine::with_limits(
            OmenchatStore::open(&path).expect("reopened store"),
            SessionLimits {
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );
        let replayed = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: false,
                    pins: false,
                },
                11,
                Some(room_id),
                ChatOp::RoomReaction,
                client_instance_id,
                envelope.clone(),
            )
            .expect("restart replay");
        assert_replayed_response(&replayed.origin, &stored.origin, 11);
        assert!(replayed.broadcasts.is_empty());

        let no_change = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: false,
                    pins: false,
                },
                12,
                Some(room_id),
                ChatOp::RoomReaction,
                client_instance_id,
                durable_envelope_body(
                    ChatOp::RoomReaction,
                    room_id,
                    45,
                    request.into_frame_body().expect("reaction body"),
                ),
            )
            .expect("idempotent logical add");
        assert!(
            !crate::protocol::ReactionAck::from_frame_body(&no_change.origin.body)
                .expect("no-change ack")
                .changed
        );
        assert!(no_change.broadcasts.is_empty());

        let conflicting = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: true,
                    message_revisions: false,
                    pins: false,
                },
                13,
                Some(room_id),
                ChatOp::RoomReaction,
                client_instance_id,
                durable_envelope_body(
                    ChatOp::RoomReaction,
                    room_id,
                    44,
                    crate::protocol::ReactionRequest {
                        token: crate::protocol::ReactionToken::Laugh,
                        ..request
                    }
                    .into_frame_body()
                    .expect("conflicting body"),
                ),
            )
            .expect("conflicting reaction");
        assert_eq!(
            frame_error_code(&conflicting.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );

        let snapshot_frame = engine
            .reaction_snapshot_frame(14, room_id, &[target_event_id])
            .expect("reaction snapshot");
        assert_eq!(snapshot_frame.op, ChatOp::ReactionSnapshotResource);
        let mut transport = crate::transport::CapturedTransport::default();
        crate::transport::send_response_frame(&engine, [0x45; 16], &snapshot_frame, &mut transport)
            .expect("dispatch reaction snapshot resource");
        assert_eq!(transport.frames.len(), 1);
        assert_eq!(transport.resources.len(), 1);
        let offer = decode_resource_offer_body(&snapshot_frame.body).expect("resource offer");
        assert_eq!(transport.resources[0].resource_id, offer.resource_id);
        let payload = engine
            .resource_payload(&offer.resource_id)
            .expect("resource lookup")
            .expect("resource payload");
        let values = decode_compressed_values_payload(&payload).expect("reaction snapshot payload");
        let snapshot =
            crate::protocol::ReactionSnapshot::from_frame_body(&FrameBody::Fields(values))
                .expect("reaction snapshot");
        assert_eq!(snapshot.target_event_ids, vec![target_event_id]);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(
            snapshot.entries[0].token,
            crate::protocol::ReactionToken::Heart
        );
        assert_eq!(
            engine.store.reaction_row_counts().expect("reaction counts"),
            (1, 1)
        );
        drop(engine);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn durable_reaction_requires_membership_live_target_and_unmuted_actor() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        let target = store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "target".into(),
                },
            )
            .expect("target");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([46; 16]);
        let dispatch = |mutation_marker, target_event_id| {
            engine
                .handle_durable_mutation_with_active_peers(
                    DurableMutationPeerContext {
                        peer: &peer(),
                        active_room_peers: &[],
                        durable_notice_ack: true,
                        reply_mentions: true,
                        reactions: true,
                        message_revisions: false,
                        pins: false,
                    },
                    u32::from(mutation_marker),
                    Some(room.room_id),
                    ChatOp::RoomReaction,
                    client_instance_id,
                    durable_envelope_body(
                        ChatOp::RoomReaction,
                        room.room_id,
                        mutation_marker,
                        crate::protocol::ReactionRequest {
                            target_event_id,
                            token: crate::protocol::ReactionToken::Heart,
                            action: crate::protocol::ReactionAction::Add,
                        }
                        .into_frame_body()
                        .expect("reaction body"),
                    ),
                )
                .expect("reaction dispatch")
        };

        let not_joined = dispatch(47, target.event_id);
        assert_eq!(
            frame_error_code(&not_joined.origin),
            Some(ChatErrorCode::NotJoined as u16 as u64)
        );
        engine
            .store
            .join_room(room.room_id, user.user_id)
            .expect("join");
        let unavailable = dispatch(48, target.event_id + 1);
        assert_eq!(
            frame_error_code(&unavailable.origin),
            Some(ChatErrorCode::HistoryUnavailable as u16 as u64)
        );
        engine
            .store
            .set_user_status_flag(user.user_id, STATUS_MUTED, true)
            .expect("mute");
        let muted = dispatch(49, target.event_id);
        assert_eq!(
            frame_error_code(&muted.origin),
            Some(ChatErrorCode::PermissionDenied as u16 as u64)
        );
        assert_eq!(
            engine.store.reaction_row_counts().expect("reaction counts"),
            (0, 0)
        );
    }

    #[test]
    fn base_durable_capability_preserves_legacy_notice_origin_response() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("moderator");
        store
            .set_user_role_bits(user.user_id, ROLE_MODERATOR)
            .expect("moderator role");
        let engine = SessionEngine::new(store);
        let result = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: false,
                    reply_mentions: false,
                    reactions: false,
                    message_revisions: false,
                    pins: false,
                },
                3,
                Some(room.room_id),
                ChatOp::RoomNotice,
                ClientInstanceId::new([11; 16]),
                durable_envelope(ChatOp::RoomNotice, room.room_id, 12, "legacy response"),
            )
            .expect("base durable notice");

        assert_eq!(result.origin.op, ChatOp::RoomEvent);
        assert_eq!(
            result.broadcasts.first().map(|frame| frame.op),
            Some(ChatOp::RoomEvent)
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
            stored.broadcasts.first().map(|frame| frame.op),
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
        assert_replayed_response(&replayed.origin, &stored.origin, 12);
        assert!(replayed.broadcasts.is_empty());

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
        assert!(rate_limited.broadcasts.is_empty());
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
    fn test_only_durable_slow_mode_replays_before_admission_and_survives_restart() {
        let path = temp_store_path("durable-slow-mode");
        let (room_id, user_id) = {
            let store = OmenchatStore::open(&path).expect("store");
            let room = store.ensure_room("lobby", None).expect("room");
            store
                .set_room_slow_mode_seconds(room.room_id, 30)
                .expect("slow mode");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("user");
            (room.room_id, user.user_id)
        };
        let client_instance_id = ClientInstanceId::new([41; 16]);
        let first = durable_envelope(ChatOp::RoomMessage, room_id, 41, "first");
        let engine = SessionEngine::with_test_slow_mode(OmenchatStore::open(&path).expect("store"));
        let stored = engine
            .handle_durable_room_text(
                &peer(),
                41,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                first.clone(),
            )
            .expect("first durable message");
        assert_eq!(stored.origin.op, ChatOp::MessageAck);
        assert_eq!(stored.broadcasts.len(), 1);
        assert_eq!(
            engine
                .store
                .slow_mode_admission_count()
                .expect("admission count"),
            1
        );

        let replayed = engine
            .handle_durable_room_text(
                &peer(),
                42,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                first.clone(),
            )
            .expect("exact replay");
        assert_replayed_response(&replayed.origin, &stored.origin, 42);
        assert!(replayed.broadcasts.is_empty());

        let conflict = engine
            .handle_durable_room_text(
                &peer(),
                43,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room_id, 41, "different"),
            )
            .expect("hash conflict");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );

        engine
            .store
            .leave_room(room_id, user_id)
            .expect("leave room");
        let rejected = engine
            .handle_durable_room_text(
                &peer(),
                44,
                Some(room_id),
                ChatOp::RoomAction,
                client_instance_id,
                durable_envelope(ChatOp::RoomAction, room_id, 42, "waves"),
            )
            .expect("cooldown rejection");
        assert_eq!(
            frame_error_code(&rejected.origin),
            Some(ChatErrorCode::SlowModeActive as u16 as u64)
        );
        assert!(rejected.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .latest_events(room_id, 10)
                .expect("events")
                .len(),
            1
        );
        drop(engine);

        let restarted =
            SessionEngine::with_test_slow_mode(OmenchatStore::open(&path).expect("reopened store"));
        let after_restart = restarted
            .handle_durable_room_text(
                &peer(),
                45,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room_id, 43, "after restart"),
            )
            .expect("restart rejection");
        assert_eq!(
            frame_error_code(&after_restart.origin),
            Some(ChatErrorCode::SlowModeActive as u16 as u64)
        );
        assert_eq!(
            restarted
                .store
                .latest_events(room_id, 10)
                .expect("events after restart")
                .len(),
            1
        );
        drop(restarted);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn test_only_slow_mode_rejections_do_not_consume_admission_and_roles_bypass() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        store
            .set_room_slow_mode_seconds(room.room_id, 30)
            .expect("slow mode");
        store
            .set_room_announcement_policy(room.room_id, true)
            .expect("announcement room");
        let mut engine = SessionEngine::with_test_slow_mode(store);
        engine.announcement_rooms_enabled = true;
        let client_instance_id = ClientInstanceId::new([42; 16]);

        let policy_rejected = engine
            .handle_durable_room_text(
                &peer(),
                50,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room.room_id, 50, "blocked"),
            )
            .expect("policy rejection");
        assert_eq!(
            frame_error_code(&policy_rejected.origin),
            Some(ChatErrorCode::RoomPolicyRestricted as u16 as u64)
        );
        assert_eq!(
            engine
                .store
                .slow_mode_admission_count()
                .expect("admission count"),
            0
        );

        engine
            .store
            .set_room_announcement_policy(room.room_id, false)
            .expect("ordinary room");
        let malformed = engine
            .handle_durable_room_text(
                &peer(),
                51,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room.room_id, 51, ""),
            )
            .expect("malformed message");
        assert_eq!(
            frame_error_code(&malformed.origin),
            Some(ChatErrorCode::DurableMutationMalformed as u16 as u64)
        );
        assert_eq!(
            engine
                .store
                .slow_mode_admission_count()
                .expect("admission count"),
            0
        );

        let first = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomMessage,
                    52,
                    Some(room.room_id),
                    FrameBody::Text("legacy first".into()),
                ),
            )
            .expect("legacy first");
        assert_eq!(first[0].op, ChatOp::RoomEvent);
        let second = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::RoomAction,
                    53,
                    Some(room.room_id),
                    FrameBody::Text("legacy second".into()),
                ),
            )
            .expect("legacy second");
        assert_eq!(
            frame_error_code(&second[0]),
            Some(ChatErrorCode::SlowModeActive as u16 as u64)
        );

        drop(engine);
        let store = OmenchatStore::in_memory().expect("moderator store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        store
            .set_room_slow_mode_seconds(room.room_id, 30)
            .expect("slow mode");
        let moderator = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("moderator");
        store
            .set_user_role_bits(moderator.user_id, ROLE_MODERATOR)
            .expect("moderator role");
        let engine = SessionEngine::with_test_slow_mode(store);
        for seq in 54..=55 {
            let response = engine
                .handle_frame(
                    &peer(),
                    Frame::new(
                        ChatOp::RoomMessage,
                        seq,
                        Some(room.room_id),
                        FrameBody::Text(format!("moderator {seq}")),
                    ),
                )
                .expect("moderator message");
            assert_eq!(response[0].op, ChatOp::RoomEvent);
        }
        assert_eq!(
            engine
                .store
                .slow_mode_admission_count()
                .expect("moderator admission count"),
            0
        );
    }

    #[test]
    #[cfg(not(feature = "omenchat-slow-mode"))]
    fn dormant_slow_mode_setting_does_not_change_production_session_behavior() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        store
            .set_room_slow_mode_seconds(room.room_id, 30)
            .expect("dormant slow mode");
        let engine = SessionEngine::new(store);
        for seq in 60..=61 {
            let response = engine
                .handle_frame(
                    &peer(),
                    Frame::new(
                        ChatOp::RoomMessage,
                        seq,
                        Some(room.room_id),
                        FrameBody::Text(format!("production path {seq}")),
                    ),
                )
                .expect("production message");
            assert_eq!(response[0].op, ChatOp::RoomEvent);
        }
        assert_eq!(
            engine
                .store
                .slow_mode_admission_count()
                .expect("admission count"),
            0
        );
    }

    #[test]
    fn test_only_disable_bypasses_and_reenable_preserves_prior_deadline() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        store
            .set_room_slow_mode_seconds(room.room_id, 30)
            .expect("slow mode");
        let engine = SessionEngine::with_test_slow_mode(store);
        let client_instance_id = ClientInstanceId::new([43; 16]);
        let first = engine
            .handle_durable_room_text(
                &peer(),
                70,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room.room_id, 70, "enabled"),
            )
            .expect("enabled message");
        assert_eq!(first.origin.op, ChatOp::MessageAck);

        engine
            .store
            .set_room_slow_mode_seconds(room.room_id, 0)
            .expect("disable slow mode");
        let disabled = engine
            .handle_durable_room_text(
                &peer(),
                71,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room.room_id, 71, "disabled"),
            )
            .expect("disabled message");
        assert_eq!(disabled.origin.op, ChatOp::MessageAck);

        engine
            .store
            .set_room_slow_mode_seconds(room.room_id, 30)
            .expect("reenable slow mode");
        let reenabled = engine
            .handle_durable_room_text(
                &peer(),
                72,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                durable_envelope(ChatOp::RoomMessage, room.room_id, 72, "reenabled"),
            )
            .expect("reenabled rejection");
        assert_eq!(
            frame_error_code(&reenabled.origin),
            Some(ChatErrorCode::SlowModeActive as u16 as u64)
        );
        assert_eq!(
            engine
                .store
                .latest_events(room.room_id, 10)
                .expect("events")
                .len(),
            2
        );
    }

    #[test]
    fn rich_message_metadata_survives_fanout_history_resource_and_restart_replay() {
        let path = temp_store_path("rich-message-recovery");
        let (room_id, original_event_id, alice_user_id, bob_user_id) = {
            let store = OmenchatStore::open(&path).expect("store");
            let room = store.ensure_room("lobby", None).expect("room");
            let alice = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("alice");
            let bob = store.ensure_user(b"peer-b", "Bob", None).expect("bob");
            store
                .join_room(room.room_id, alice.user_id)
                .expect("join alice");
            store
                .join_room(room.room_id, bob.user_id)
                .expect("join bob");
            let original = store
                .append_event(
                    room.room_id,
                    Some(bob.user_id),
                    ServerRoomEventKind::Message {
                        body: "original".into(),
                    },
                )
                .expect("original event");
            (room.room_id, original.event_id, alice.user_id, bob.user_id)
        };
        let envelope = rich_message_envelope(
            room_id,
            31,
            Some(crate::protocol::ReplyReference {
                room_id,
                event_id: original_event_id,
            }),
            vec![bob_user_id],
        );
        let client_instance_id = ClientInstanceId::new([31; 16]);
        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("store"));
        let stored = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: false,
                    message_revisions: false,
                    pins: false,
                },
                31,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope.clone(),
            )
            .expect("rich message");
        assert_eq!(stored.origin.op, ChatOp::MessageAck);
        assert_eq!(stored.broadcasts.len(), 1);
        let FrameBody::Fields(broadcast_values) = &stored.broadcasts[0].body else {
            panic!("rich event broadcast");
        };
        let broadcast_event = broadcast_values[0].clone();
        let FrameValue::Array(event_fields) = &broadcast_event else {
            panic!("rich event fields");
        };
        assert_eq!(event_fields.len(), 8);
        assert_eq!(
            event_fields.get(6),
            Some(&FrameValue::U64(original_event_id))
        );
        assert_eq!(
            event_fields.get(7),
            Some(&FrameValue::Array(vec![FrameValue::U64(u64::from(
                bob_user_id
            ))]))
        );
        let events = engine
            .store
            .latest_events(room_id, 10)
            .expect("stored events");
        assert_eq!(
            events.last().and_then(|event| event.metadata.clone()),
            Some(RichMessageEventMetadata {
                reply_to_event_id: Some(original_event_id),
                mentioned_user_ids: vec![bob_user_id],
            })
        );

        let inline = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    32,
                    Some(room_id),
                    FrameBody::Fields(vec![FrameValue::U64(u64::MAX)]),
                ),
            )
            .expect("inline history");
        let inline_values = inline
            .iter()
            .filter(|frame| frame.op == ChatOp::HistoryInline)
            .flat_map(|frame| {
                decode_compressed_values_body(&frame.body).expect("inline history values")
            })
            .collect::<Vec<_>>();
        let rich_inline = inline_values
            .iter()
            .find_map(|value| match value {
                FrameValue::Array(fields)
                    if fields.first() == event_fields.first()
                        && fields.get(4) == event_fields.get(4) =>
                {
                    Some(fields)
                }
                _ => None,
            })
            .expect("rich inline event");
        assert_eq!(rich_inline.get(6), event_fields.get(6));
        assert_eq!(rich_inline.get(7), event_fields.get(7));
        drop(engine);

        let engine = SessionEngine::with_limits(
            OmenchatStore::open(&path).expect("reopened store"),
            SessionLimits {
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );
        let replayed = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: false,
                    message_revisions: false,
                    pins: false,
                },
                33,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                envelope.clone(),
            )
            .expect("restart replay");
        assert_replayed_response(&replayed.origin, &stored.origin, 33);
        assert!(replayed.broadcasts.is_empty());

        let resource = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    34,
                    Some(room_id),
                    FrameBody::Fields(vec![FrameValue::U64(u64::MAX)]),
                ),
            )
            .expect("resource history");
        assert_eq!(resource[0].op, ChatOp::HistoryResourceOffer);
        let offer = decode_resource_offer_body(&resource[0].body).expect("resource offer");
        let payload = engine
            .resource_payload(&offer.resource_id)
            .expect("resource lookup")
            .expect("resource payload");
        let resource_values =
            decode_compressed_values_payload(&payload).expect("resource history values");
        let rich_resource = resource_values
            .iter()
            .find_map(|value| match value {
                FrameValue::Array(fields)
                    if fields.first() == event_fields.first()
                        && fields.get(4) == event_fields.get(4) =>
                {
                    Some(fields)
                }
                _ => None,
            })
            .expect("rich resource event");
        assert_eq!(rich_resource.get(6), event_fields.get(6));
        assert_eq!(rich_resource.get(7), event_fields.get(7));

        let conflicting = rich_message_envelope(
            room_id,
            31,
            Some(crate::protocol::ReplyReference {
                room_id,
                event_id: original_event_id,
            }),
            vec![alice_user_id],
        );
        let conflict = engine
            .handle_durable_mutation_with_active_peers(
                DurableMutationPeerContext {
                    peer: &peer(),
                    active_room_peers: &[],
                    durable_notice_ack: true,
                    reply_mentions: true,
                    reactions: false,
                    message_revisions: false,
                    pins: false,
                },
                35,
                Some(room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                conflicting,
            )
            .expect("rich conflict");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert_eq!(
            engine
                .store
                .latest_events(room_id, 10)
                .expect("events after replay")
                .len(),
            2
        );

        drop(engine);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn rich_message_validation_fails_closed_without_event_insertion() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let alice = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("alice");
        let bob = store.ensure_user(b"peer-b", "Bob", None).expect("bob");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([32; 16]);
        let alice_peer = peer();
        let context = |reply_mentions| DurableMutationPeerContext {
            peer: &alice_peer,
            active_room_peers: &[],
            durable_notice_ack: true,
            reply_mentions,
            reactions: false,
            message_revisions: false,
            pins: false,
        };

        let not_negotiated = engine
            .handle_durable_mutation_with_active_peers(
                context(false),
                40,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                rich_message_envelope(room.room_id, 40, None, vec![alice.user_id]),
            )
            .expect("not negotiated");
        assert_eq!(
            frame_error_code(&not_negotiated.origin),
            Some(ChatErrorCode::DurableMutationNotNegotiated as u16 as u64)
        );

        let not_joined = engine
            .handle_durable_mutation_with_active_peers(
                context(true),
                41,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                rich_message_envelope(room.room_id, 41, None, vec![alice.user_id]),
            )
            .expect("sender not joined");
        assert_eq!(
            frame_error_code(&not_joined.origin),
            Some(ChatErrorCode::NotJoined as u16 as u64)
        );
        engine
            .store
            .join_room(room.room_id, alice.user_id)
            .expect("join alice");

        let missing_reply = engine
            .handle_durable_mutation_with_active_peers(
                context(true),
                42,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                rich_message_envelope(
                    room.room_id,
                    42,
                    Some(crate::protocol::ReplyReference {
                        room_id: room.room_id,
                        event_id: 999,
                    }),
                    Vec::new(),
                ),
            )
            .expect("missing reply");
        assert_eq!(
            frame_error_code(&missing_reply.origin),
            Some(ChatErrorCode::HistoryUnavailable as u16 as u64)
        );

        let missing_member_envelope =
            rich_message_envelope(room.room_id, 43, None, vec![bob.user_id]);
        let missing_member = engine
            .handle_durable_mutation_with_active_peers(
                context(true),
                43,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                missing_member_envelope.clone(),
            )
            .expect("missing mentioned member");
        assert_eq!(
            frame_error_code(&missing_member.origin),
            Some(ChatErrorCode::UserNotFound as u16 as u64)
        );
        engine
            .store
            .join_room(room.room_id, bob.user_id)
            .expect("join bob after rejected mutation");
        let replayed_rejection = engine
            .handle_durable_mutation_with_active_peers(
                context(true),
                46,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                missing_member_envelope,
            )
            .expect("replayed validation result");
        assert_replayed_response(&replayed_rejection.origin, &missing_member.origin, 46);
        assert!(replayed_rejection.broadcasts.is_empty());

        let cross_room = engine
            .handle_durable_mutation_with_active_peers(
                context(true),
                44,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                rich_message_envelope(
                    room.room_id,
                    44,
                    Some(crate::protocol::ReplyReference {
                        room_id: room.room_id + 1,
                        event_id: 1,
                    }),
                    Vec::new(),
                ),
            )
            .expect("cross-room reply");
        assert_eq!(
            frame_error_code(&cross_room.origin),
            Some(ChatErrorCode::DurableMutationMalformed as u16 as u64)
        );

        let deleted = engine
            .store
            .append_event(
                room.room_id,
                Some(alice.user_id),
                ServerRoomEventKind::Message {
                    body: "deleted".into(),
                },
            )
            .expect("deleted target");
        engine
            .store
            .mark_event_deleted_for_test(room.room_id, deleted.event_id)
            .expect("mark target deleted");
        let deleted_reply = engine
            .handle_durable_mutation_with_active_peers(
                context(true),
                45,
                Some(room.room_id),
                ChatOp::RoomMessage,
                client_instance_id,
                rich_message_envelope(
                    room.room_id,
                    45,
                    Some(crate::protocol::ReplyReference {
                        room_id: room.room_id,
                        event_id: deleted.event_id,
                    }),
                    Vec::new(),
                ),
            )
            .expect("deleted reply");
        assert_eq!(
            frame_error_code(&deleted_reply.origin),
            Some(ChatErrorCode::HistoryUnavailable as u16 as u64)
        );
        assert!(engine
            .store
            .latest_events(room.room_id, 10)
            .expect("visible events")
            .is_empty());
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
        let stored = engine
            .handle_durable_room_text(
                &peer(),
                21,
                Some(room.room_id),
                ChatOp::RoomAction,
                client_instance_id,
                first.clone(),
            )
            .expect("first action");
        let replayed = engine
            .handle_durable_room_text(
                &peer(),
                22,
                Some(room.room_id),
                ChatOp::RoomAction,
                client_instance_id,
                first,
            )
            .expect("exact action replay");
        assert_replayed_response(&replayed.origin, &stored.origin, 22);
        assert!(replayed.broadcasts.is_empty());

        let conflict = engine
            .handle_durable_room_text(
                &peer(),
                23,
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
        assert!(conflict.broadcasts.is_empty());

        let mut malformed = durable_envelope(ChatOp::RoomAction, room.room_id, 4, "invalid hash");
        malformed.request_hash = crate::protocol::RequestHash::new([0; 32]);
        let malformed = engine
            .handle_durable_room_text(
                &peer(),
                24,
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
        assert_replayed_response(&replayed.origin, &rejected.origin, 32);
        assert!(replayed.broadcasts.is_empty());
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
            assert!(!original.broadcasts.is_empty());
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
        assert_replayed_response(&replayed.origin, &original, 42);
        assert!(replayed.broadcasts.is_empty());
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
    fn durable_room_action_replays_after_server_restart_without_new_event() {
        let path = temp_store_path("durable-action-restart");
        let client_instance_id = ClientInstanceId::new([21; 16]);
        let (room_id, envelope, original) = {
            let store = OmenchatStore::open(&path).expect("persistent store");
            let room = store.ensure_room("lobby", None).expect("room");
            let envelope = durable_envelope(ChatOp::RoomAction, room.room_id, 16, "waves once");
            let engine = SessionEngine::new(store);
            let original = engine
                .handle_durable_room_text(
                    &peer(),
                    141,
                    Some(room.room_id),
                    ChatOp::RoomAction,
                    client_instance_id,
                    envelope.clone(),
                )
                .expect("stored action before restart");
            assert!(!original.broadcasts.is_empty());
            (room.room_id, envelope, original.origin)
        };

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed = engine
            .handle_durable_room_text(
                &peer(),
                142,
                Some(room_id),
                ChatOp::RoomAction,
                client_instance_id,
                envelope,
            )
            .expect("replayed action after restart");
        assert_replayed_response(&replayed.origin, &original, 142);
        assert!(replayed.broadcasts.is_empty());
        let events = engine
            .store
            .latest_events(room_id, 10)
            .expect("action events");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].kind,
            ServerRoomEventKind::Action { body } if body == "waves once"
        ));
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
    fn durable_part_removes_membership_and_replays_without_second_event() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([12; 16]);
        let envelope = durable_envelope_body(ChatOp::PartRoom, room.room_id, 7, FrameBody::Empty);

        let stored = engine
            .handle_durable_mutation(
                &peer(),
                51,
                Some(room.room_id),
                ChatOp::PartRoom,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable part");
        assert_eq!(stored.origin.op, ChatOp::CommandResult);
        assert_eq!(stored.origin.seq, 51);
        assert_eq!(
            stored.origin.body,
            FrameBody::Fields(vec![
                FrameValue::String("part".into()),
                room_to_value(&room),
            ])
        );
        assert_eq!(
            stored.broadcasts.first().map(|frame| frame.op),
            Some(ChatOp::RoomEvent)
        );
        assert!(!engine
            .store
            .room_has_member(room.room_id, user.user_id)
            .expect("membership"));

        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                52,
                Some(room.room_id),
                ChatOp::PartRoom,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable part");
        assert_replayed_response(&replayed.origin, &stored.origin, 52);
        assert!(replayed.broadcasts.is_empty());
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
    fn durable_notice_replays_once_and_preserves_moderator_decision() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store
            .set_user_role_bits(user.user_id, ROLE_MODERATOR)
            .expect("moderator");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                rate_messages_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let client_instance_id = ClientInstanceId::new([13; 16]);
        let envelope = durable_envelope(ChatOp::RoomNotice, room.room_id, 8, "maintenance soon");

        let stored = engine
            .handle_durable_mutation(
                &peer(),
                61,
                Some(room.room_id),
                ChatOp::RoomNotice,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable notice");
        assert_eq!(stored.origin.op, ChatOp::MessageAck);
        assert!(matches!(
            &stored.origin.body,
            FrameBody::Fields(fields) if fields.get(1) == Some(&FrameValue::U64(3))
        ));
        assert_eq!(
            stored.broadcasts.first().map(|frame| frame.op),
            Some(ChatOp::RoomEvent)
        );
        assert_eq!(
            stored.broadcasts.first().map(|frame| frame.seq),
            Some(stored.origin.seq)
        );

        engine
            .store
            .set_user_role_bits(user.user_id, 0)
            .expect("remove moderator role");
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                62,
                Some(room.room_id),
                ChatOp::RoomNotice,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable notice");
        assert_replayed_response(&replayed.origin, &stored.origin, 62);
        assert!(replayed.broadcasts.is_empty());

        let conflict = engine
            .handle_durable_mutation(
                &peer(),
                63,
                Some(room.room_id),
                ChatOp::RoomNotice,
                client_instance_id,
                durable_envelope(
                    ChatOp::RoomNotice,
                    room.room_id,
                    8,
                    "different notice content",
                ),
            )
            .expect("conflicting durable notice");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());

        let denied = engine
            .handle_durable_mutation(
                &peer(),
                64,
                Some(room.room_id),
                ChatOp::RoomNotice,
                client_instance_id,
                durable_envelope(ChatOp::RoomNotice, room.room_id, 9, "second notice"),
            )
            .expect("denied durable notice");
        assert_eq!(
            frame_error_code(&denied.origin),
            Some(ChatErrorCode::PermissionDenied as u16 as u64)
        );
        assert!(denied.broadcasts.is_empty());
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
    fn durable_notice_replays_after_server_restart_without_new_event() {
        let path = temp_store_path("durable-notice-restart");
        let client_instance_id = ClientInstanceId::new([22; 16]);
        let (room_id, envelope, original) = {
            let store = OmenchatStore::open(&path).expect("persistent store");
            let room = store.ensure_room("lobby", None).expect("room");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("moderator");
            store
                .set_user_role_bits(user.user_id, ROLE_MODERATOR)
                .expect("moderator role");
            let envelope = durable_envelope(ChatOp::RoomNotice, room.room_id, 17, "restart notice");
            let engine = SessionEngine::new(store);
            let original = engine
                .handle_durable_mutation(
                    &peer(),
                    151,
                    Some(room.room_id),
                    ChatOp::RoomNotice,
                    client_instance_id,
                    envelope.clone(),
                )
                .expect("stored notice before restart");
            assert_eq!(original.origin.op, ChatOp::MessageAck);
            assert!(!original.broadcasts.is_empty());
            (room.room_id, envelope, original.origin)
        };

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                152,
                Some(room_id),
                ChatOp::RoomNotice,
                client_instance_id,
                envelope,
            )
            .expect("replayed notice after restart");
        assert_replayed_response(&replayed.origin, &original, 152);
        assert!(replayed.broadcasts.is_empty());
        let events = engine
            .store
            .latest_events(room_id, 10)
            .expect("notice events");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].kind,
            ServerRoomEventKind::Notice { body } if body == "restart notice"
        ));
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
    fn durable_topic_updates_once_and_replays_original_result() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store
            .set_user_role_bits(user.user_id, ROLE_MODERATOR)
            .expect("moderator");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([14; 16]);
        let envelope = durable_envelope_body(
            ChatOp::Command,
            room.room_id,
            10,
            FrameBody::Text("topic Durable topic".into()),
        );

        let stored = engine
            .handle_durable_mutation(
                &peer(),
                71,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable topic");
        assert_eq!(stored.origin.op, ChatOp::CommandResult);
        assert_eq!(
            stored.broadcasts.first().map(|frame| frame.op),
            Some(ChatOp::RoomDelta)
        );
        let updated = engine
            .store
            .room_by_id(room.room_id)
            .expect("updated room")
            .expect("room");
        assert_eq!(updated.topic.as_deref(), Some("Durable topic"));
        assert_eq!(updated.room_revision, room.room_revision + 1);

        engine
            .store
            .set_user_role_bits(user.user_id, 0)
            .expect("remove moderator role");
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                72,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable topic");
        assert_replayed_response(&replayed.origin, &stored.origin, 72);
        assert!(replayed.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .room_by_id(room.room_id)
                .expect("replayed room")
                .expect("room")
                .room_revision,
            updated.room_revision
        );

        let conflict = engine
            .handle_durable_mutation(
                &peer(),
                73,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                durable_envelope_body(
                    ChatOp::Command,
                    room.room_id,
                    10,
                    FrameBody::Text("topic Different content".into()),
                ),
            )
            .expect("conflicting durable topic");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .room_by_id(room.room_id)
                .expect("conflicted room")
                .expect("room")
                .room_revision,
            updated.room_revision
        );
    }

    #[test]
    fn durable_topic_replays_after_server_restart_without_second_update() {
        let path = temp_store_path("durable-topic-restart");
        let client_instance_id = ClientInstanceId::new([23; 16]);
        let (room_id, envelope, original, committed_revision) = {
            let store = OmenchatStore::open(&path).expect("persistent store");
            let room = store.ensure_room("lobby", None).expect("room");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("moderator");
            store
                .set_user_role_bits(user.user_id, ROLE_MODERATOR)
                .expect("moderator role");
            let envelope = durable_envelope_body(
                ChatOp::Command,
                room.room_id,
                18,
                FrameBody::Text("topic Restart durable topic".into()),
            );
            let engine = SessionEngine::new(store);
            let original = engine
                .handle_durable_mutation(
                    &peer(),
                    161,
                    Some(room.room_id),
                    ChatOp::Command,
                    client_instance_id,
                    envelope.clone(),
                )
                .expect("stored topic before restart");
            assert_eq!(original.origin.op, ChatOp::CommandResult);
            assert_eq!(
                original.broadcasts.first().map(|frame| frame.op),
                Some(ChatOp::RoomDelta)
            );
            let updated = engine
                .store
                .room_by_id(room.room_id)
                .expect("updated room")
                .expect("room");
            assert_eq!(updated.topic.as_deref(), Some("Restart durable topic"));
            engine
                .store
                .set_user_role_bits(user.user_id, 0)
                .expect("remove moderator role");
            (
                room.room_id,
                envelope,
                original.origin,
                updated.room_revision,
            )
        };

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                162,
                Some(room_id),
                ChatOp::Command,
                client_instance_id,
                envelope,
            )
            .expect("replayed topic after restart");
        assert_replayed_response(&replayed.origin, &original, 162);
        assert!(replayed.broadcasts.is_empty());
        let room = engine
            .store
            .room_by_id(room_id)
            .expect("replayed room")
            .expect("room");
        assert_eq!(room.topic.as_deref(), Some("Restart durable topic"));
        assert_eq!(room.room_revision, committed_revision);
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
    fn durable_create_creates_once_and_replays_after_admin_role_changes() {
        let store = OmenchatStore::in_memory().expect("store");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("user");
        store
            .set_user_role_bits(user.user_id, ROLE_ADMIN)
            .expect("admin");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([15; 16]);
        let envelope = durable_envelope_optional_room(
            ChatOp::Command,
            None,
            11,
            FrameBody::Text("create operations Operations room".into()),
        );

        let stored = engine
            .handle_durable_mutation(
                &peer(),
                81,
                None,
                ChatOp::Command,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable create");
        assert_eq!(stored.origin.op, ChatOp::CommandResult);
        assert_eq!(stored.origin.room_id, None);
        assert_eq!(
            stored.broadcasts.first().map(|frame| frame.op),
            Some(ChatOp::RoomDelta)
        );
        let created = engine
            .store
            .room_by_name("operations")
            .expect("created room")
            .expect("room");
        assert_eq!(created.topic.as_deref(), Some("Operations room"));

        engine
            .store
            .set_user_role_bits(user.user_id, 0)
            .expect("remove admin role");
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                82,
                None,
                ChatOp::Command,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable create");
        assert_replayed_response(&replayed.origin, &stored.origin, 82);
        assert!(replayed.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .room_by_name("operations")
                .expect("replayed room")
                .expect("room")
                .room_revision,
            created.room_revision
        );

        let conflict = engine
            .handle_durable_mutation(
                &peer(),
                83,
                None,
                ChatOp::Command,
                client_instance_id,
                durable_envelope_optional_room(
                    ChatOp::Command,
                    None,
                    11,
                    FrameBody::Text("create operations Different content".into()),
                ),
            )
            .expect("conflicting durable create");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .room_by_name("operations")
                .expect("conflicted room")
                .expect("room")
                .room_revision,
            created.room_revision
        );
    }

    #[test]
    fn durable_create_replays_after_server_restart_without_second_room_mutation() {
        let path = temp_store_path("durable-create-restart");
        let client_instance_id = ClientInstanceId::new([24; 16]);
        let (envelope, original, room_id, committed_revision) = {
            let store = OmenchatStore::open(&path).expect("persistent store");
            let user = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("administrator");
            store
                .set_user_role_bits(user.user_id, ROLE_ADMIN)
                .expect("administrator role");
            let envelope = durable_envelope_optional_room(
                ChatOp::Command,
                None,
                19,
                FrameBody::Text("create restart Restart room".into()),
            );
            let engine = SessionEngine::new(store);
            let original = engine
                .handle_durable_mutation(
                    &peer(),
                    171,
                    None,
                    ChatOp::Command,
                    client_instance_id,
                    envelope.clone(),
                )
                .expect("stored create before restart");
            assert_eq!(original.origin.op, ChatOp::CommandResult);
            assert_eq!(
                original.broadcasts.first().map(|frame| frame.op),
                Some(ChatOp::RoomDelta)
            );
            let room = engine
                .store
                .room_by_name("restart")
                .expect("created room")
                .expect("room");
            engine
                .store
                .set_user_role_bits(user.user_id, 0)
                .expect("remove administrator role");
            (envelope, original.origin, room.room_id, room.room_revision)
        };

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                172,
                None,
                ChatOp::Command,
                client_instance_id,
                envelope,
            )
            .expect("replayed create after restart");
        assert_replayed_response(&replayed.origin, &original, 172);
        assert!(replayed.broadcasts.is_empty());
        let room = engine
            .store
            .room_by_name("restart")
            .expect("replayed room")
            .expect("room");
        assert_eq!(room.room_id, room_id);
        assert_eq!(room.room_revision, committed_revision);
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
    fn durable_role_changes_once_and_replays_without_broadcasts() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let actor = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("actor");
        store
            .set_user_role_bits(actor.user_id, ROLE_ADMIN)
            .expect("admin");
        let target = store.ensure_user(b"peer-b", "Bob", None).expect("target");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([16; 16]);
        let envelope = durable_envelope_body(
            ChatOp::Command,
            room.room_id,
            12,
            FrameBody::Text("role Bob mod".into()),
        );

        let stored = engine
            .handle_durable_mutation(
                &peer(),
                91,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable role");
        assert_eq!(stored.origin.op, ChatOp::CommandResult);
        assert_eq!(
            stored
                .broadcasts
                .iter()
                .map(|frame| frame.op)
                .collect::<Vec<_>>(),
            vec![ChatOp::UserDelta, ChatOp::RoomEvent]
        );
        assert_eq!(
            engine
                .store
                .user_by_identity(b"peer-b")
                .expect("target query")
                .expect("target")
                .role_bits,
            ROLE_TRUSTED | ROLE_MODERATOR
        );
        assert_eq!(
            engine
                .store
                .latest_events(room.room_id, 10)
                .expect("events")
                .len(),
            1
        );
        let audit = engine
            .store
            .moderation_audit_page(room.room_id, None, 10)
            .expect("role audit");
        assert_eq!(audit.records.len(), 1);
        assert_eq!(audit.records[0].action, ModerationAuditAction::RoleChange);
        assert_eq!(
            audit.records[0].result_role_bits,
            Some(ROLE_TRUSTED | ROLE_MODERATOR)
        );

        engine
            .store
            .set_user_role_bits(actor.user_id, 0)
            .expect("remove admin role");
        engine
            .store
            .set_user_role_bits(target.user_id, 0)
            .expect("change target after commit");
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                92,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable role");
        assert_replayed_response(&replayed.origin, &stored.origin, 92);
        assert!(replayed.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .user_by_identity(b"peer-b")
                .expect("replayed target query")
                .expect("target")
                .role_bits,
            0
        );
        assert_eq!(
            engine
                .store
                .latest_events(room.room_id, 10)
                .expect("replayed events")
                .len(),
            1
        );
        assert_eq!(
            engine
                .store
                .moderation_audit_page(room.room_id, None, 10)
                .expect("replayed role audit")
                .records
                .len(),
            1
        );

        let conflict = engine
            .handle_durable_mutation(
                &peer(),
                93,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                durable_envelope_body(
                    ChatOp::Command,
                    room.room_id,
                    12,
                    FrameBody::Text("role Bob admin".into()),
                ),
            )
            .expect("conflicting durable role");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());
        assert_eq!(
            engine
                .store
                .user_by_identity(b"peer-b")
                .expect("conflicted target query")
                .expect("target")
                .role_bits,
            0
        );
    }

    #[test]
    fn durable_unban_changes_once_and_replays_without_broadcasts() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let actor = store
            .ensure_user(&peer().identity_hash, "Alice", None)
            .expect("actor");
        store
            .set_user_role_bits(actor.user_id, ROLE_ADMIN)
            .expect("admin");
        let target = store.ensure_user(b"peer-b", "Bob", None).expect("target");
        store
            .set_user_status_flag(target.user_id, STATUS_BANNED, true)
            .expect("ban target");
        let engine = SessionEngine::new(store);
        let client_instance_id = ClientInstanceId::new([17; 16]);
        let envelope = durable_envelope_body(
            ChatOp::Command,
            room.room_id,
            13,
            FrameBody::Text("unban Bob".into()),
        );

        let stored = engine
            .handle_durable_mutation(
                &peer(),
                101,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                envelope.clone(),
            )
            .expect("stored durable unban");
        assert_eq!(
            stored
                .broadcasts
                .iter()
                .map(|frame| frame.op)
                .collect::<Vec<_>>(),
            vec![ChatOp::UserDelta, ChatOp::RoomEvent]
        );
        assert_eq!(
            engine
                .store
                .user_by_identity(b"peer-b")
                .expect("target query")
                .expect("target")
                .status_bits
                & STATUS_BANNED,
            0
        );
        let audit = engine
            .store
            .moderation_audit_page(room.room_id, None, 10)
            .expect("unban audit");
        assert_eq!(audit.records.len(), 1);
        assert_eq!(audit.records[0].action, ModerationAuditAction::Unban);
        assert_eq!(audit.records[0].result_status_bits, Some(0));

        engine
            .store
            .set_user_role_bits(actor.user_id, 0)
            .expect("remove admin role");
        engine
            .store
            .set_user_status_flag(target.user_id, STATUS_BANNED, true)
            .expect("re-ban after commit");
        let replayed = engine
            .handle_durable_mutation(
                &peer(),
                102,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                envelope,
            )
            .expect("replayed durable unban");
        assert_replayed_response(&replayed.origin, &stored.origin, 102);
        assert!(replayed.broadcasts.is_empty());
        assert_ne!(
            engine
                .store
                .user_by_identity(b"peer-b")
                .expect("replayed target query")
                .expect("target")
                .status_bits
                & STATUS_BANNED,
            0
        );
        assert_eq!(
            engine
                .store
                .latest_events(room.room_id, 10)
                .expect("events")
                .len(),
            1
        );
        assert_eq!(
            engine
                .store
                .moderation_audit_page(room.room_id, None, 10)
                .expect("replayed unban audit")
                .records
                .len(),
            1
        );

        let conflict = engine
            .handle_durable_mutation(
                &peer(),
                103,
                Some(room.room_id),
                ChatOp::Command,
                client_instance_id,
                durable_envelope_body(
                    ChatOp::Command,
                    room.room_id,
                    13,
                    FrameBody::Text("unban 2".into()),
                ),
            )
            .expect("conflicting durable unban");
        assert_eq!(
            frame_error_code(&conflict.origin),
            Some(ChatErrorCode::DurableMutationConflict as u16 as u64)
        );
        assert!(conflict.broadcasts.is_empty());
        assert_ne!(
            engine
                .store
                .user_by_identity(b"peer-b")
                .expect("conflicted target query")
                .expect("target")
                .status_bits
                & STATUS_BANNED,
            0
        );
    }

    #[test]
    fn durable_role_and_unban_replay_after_server_restart_without_second_mutation() {
        let path = temp_store_path("durable-role-unban-restart");
        let client_instance_id = ClientInstanceId::new([25; 16]);
        let (room_id, role_envelope, unban_envelope, role_result, unban_result) = {
            let store = OmenchatStore::open(&path).expect("persistent store");
            let room = store.ensure_room("lobby", None).expect("room");
            let actor = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("administrator");
            store
                .set_user_role_bits(actor.user_id, ROLE_ADMIN)
                .expect("administrator role");
            let target = store.ensure_user(b"peer-b", "Bob", None).expect("target");
            store
                .set_user_status_flag(target.user_id, STATUS_BANNED, true)
                .expect("ban target");
            let role_envelope = durable_envelope_body(
                ChatOp::Command,
                room.room_id,
                20,
                FrameBody::Text("role Bob mod".into()),
            );
            let unban_envelope = durable_envelope_body(
                ChatOp::Command,
                room.room_id,
                21,
                FrameBody::Text("unban Bob".into()),
            );
            let engine = SessionEngine::new(store);
            let role_result = engine
                .handle_durable_mutation(
                    &peer(),
                    181,
                    Some(room.room_id),
                    ChatOp::Command,
                    client_instance_id,
                    role_envelope.clone(),
                )
                .expect("stored role before restart");
            let unban_result = engine
                .handle_durable_mutation(
                    &peer(),
                    182,
                    Some(room.room_id),
                    ChatOp::Command,
                    client_instance_id,
                    unban_envelope.clone(),
                )
                .expect("stored unban before restart");
            assert_eq!(role_result.broadcasts.len(), 2);
            assert_eq!(unban_result.broadcasts.len(), 2);
            engine
                .store
                .set_user_role_bits(actor.user_id, 0)
                .expect("remove administrator role");
            engine
                .store
                .set_user_role_bits(target.user_id, 0)
                .expect("change target role after commit");
            engine
                .store
                .set_user_status_flag(target.user_id, STATUS_BANNED, true)
                .expect("re-ban target after commit");
            (
                room.room_id,
                role_envelope,
                unban_envelope,
                role_result.origin,
                unban_result.origin,
            )
        };

        let engine = SessionEngine::new(OmenchatStore::open(&path).expect("reopened store"));
        let replayed_role = engine
            .handle_durable_mutation(
                &peer(),
                183,
                Some(room_id),
                ChatOp::Command,
                client_instance_id,
                role_envelope,
            )
            .expect("replayed role after restart");
        let replayed_unban = engine
            .handle_durable_mutation(
                &peer(),
                184,
                Some(room_id),
                ChatOp::Command,
                client_instance_id,
                unban_envelope,
            )
            .expect("replayed unban after restart");
        assert_replayed_response(&replayed_role.origin, &role_result, 183);
        assert_replayed_response(&replayed_unban.origin, &unban_result, 184);
        assert!(replayed_role.broadcasts.is_empty());
        assert!(replayed_unban.broadcasts.is_empty());
        let target = engine
            .store
            .user_by_identity(b"peer-b")
            .expect("target query")
            .expect("target");
        assert_eq!(target.role_bits, 0);
        assert_ne!(target.status_bits & STATUS_BANNED, 0);
        assert_eq!(
            engine
                .store
                .latest_events(room_id, 10)
                .expect("events")
                .len(),
            2
        );
        let audit = engine
            .store
            .moderation_audit_page(room_id, None, 10)
            .expect("restart audit");
        assert_eq!(audit.records.len(), 2);
        assert_eq!(audit.records[0].action, ModerationAuditAction::Unban);
        assert_eq!(audit.records[1].action, ModerationAuditAction::RoleChange);
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
    fn durable_active_peer_moderation_executes_once_for_each_action() {
        for (index, action) in ["kick", "ban", "mute", "unmute"].into_iter().enumerate() {
            let store = OmenchatStore::in_memory().expect("store");
            let room = store
                .room_by_name("lobby")
                .expect("room query")
                .expect("room");
            let actor = store
                .ensure_user(&peer().identity_hash, "Alice", None)
                .expect("actor");
            store
                .set_user_role_bits(actor.user_id, ROLE_MODERATOR)
                .expect("moderator");
            let target_peer = ServerPeer {
                identity_hash: b"peer-b".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: None,
            };
            let target = store
                .ensure_user(&target_peer.identity_hash, "Bob", None)
                .expect("target");
            if action == "unmute" {
                store
                    .set_user_status_flag(target.user_id, STATUS_MUTED, true)
                    .expect("initial mute");
            }
            let engine = SessionEngine::new(store);
            let client_instance_id = ClientInstanceId::new([18 + index as u8; 16]);
            let envelope = durable_envelope_body(
                ChatOp::Command,
                room.room_id,
                20 + index as u8,
                FrameBody::Text(format!("{action} Bob")),
            );

            let stored = engine
                .handle_durable_mutation_with_active_peers(
                    DurableMutationPeerContext {
                        peer: &peer(),
                        active_room_peers: &[peer(), target_peer.clone()],
                        durable_notice_ack: true,
                        reply_mentions: false,
                        reactions: false,
                        message_revisions: false,
                        pins: false,
                    },
                    110 + index as u32,
                    Some(room.room_id),
                    ChatOp::Command,
                    client_instance_id,
                    envelope.clone(),
                )
                .expect("stored durable moderation");
            assert_eq!(stored.origin.op, ChatOp::CommandResult, "{action}");
            assert_eq!(
                stored
                    .broadcasts
                    .iter()
                    .map(|frame| frame.op)
                    .collect::<Vec<_>>(),
                vec![ChatOp::UserDelta, ChatOp::RoomEvent],
                "{action}"
            );
            assert_eq!(
                stored.disconnect_identity.as_deref(),
                matches!(action, "kick" | "ban").then_some(target_peer.identity_hash.as_slice()),
                "{action}"
            );
            let changed = engine
                .store
                .user_by_identity(&target_peer.identity_hash)
                .expect("target query")
                .expect("target");
            match action {
                "ban" => assert_ne!(changed.status_bits & STATUS_BANNED, 0),
                "mute" => assert_ne!(changed.status_bits & STATUS_MUTED, 0),
                "unmute" => assert_eq!(changed.status_bits & STATUS_MUTED, 0),
                _ => assert_eq!(changed.status_bits, target.status_bits),
            }
            assert_eq!(
                engine
                    .store
                    .latest_events(room.room_id, 10)
                    .expect("events")
                    .len(),
                1,
                "{action}"
            );
            let audit = engine
                .store
                .moderation_audit_page(room.room_id, None, 10)
                .expect("moderation audit");
            assert_eq!(audit.records.len(), 1, "{action}");
            assert_eq!(
                audit.records[0].action,
                match action {
                    "ban" => ModerationAuditAction::Ban,
                    "mute" => ModerationAuditAction::Mute,
                    "unmute" => ModerationAuditAction::Unmute,
                    _ => ModerationAuditAction::Kick,
                },
                "{action}"
            );
            assert_eq!(audit.records[0].actor_display_name_at_action, "Alice");
            assert_eq!(
                audit.records[0].target_display_name_at_action.as_deref(),
                Some("Bob")
            );

            engine
                .store
                .set_user_role_bits(actor.user_id, 0)
                .expect("remove moderator role");
            engine
                .store
                .set_user_status_flag(target.user_id, STATUS_BANNED, false)
                .expect("clear ban after commit");
            engine
                .store
                .set_user_status_flag(target.user_id, STATUS_MUTED, action == "unmute")
                .expect("change mute after commit");
            let state_before_replay = engine
                .store
                .user_by_identity(&target_peer.identity_hash)
                .expect("state before replay")
                .expect("target");
            let replayed = engine
                .handle_durable_mutation_with_active_peers(
                    DurableMutationPeerContext {
                        peer: &peer(),
                        active_room_peers: &[],
                        durable_notice_ack: true,
                        reply_mentions: false,
                        reactions: false,
                        message_revisions: false,
                        pins: false,
                    },
                    120 + index as u32,
                    Some(room.room_id),
                    ChatOp::Command,
                    client_instance_id,
                    envelope,
                )
                .expect("replayed durable moderation");
            let mut expected = stored.origin.clone();
            expected.seq = 120 + index as u32;
            assert_eq!(replayed.origin, expected, "{action}");
            assert!(replayed.broadcasts.is_empty(), "{action}");
            assert!(replayed.disconnect_identity.is_none(), "{action}");
            assert_eq!(
                engine
                    .store
                    .user_by_identity(&target_peer.identity_hash)
                    .expect("state after replay")
                    .expect("target"),
                state_before_replay,
                "{action}"
            );
            assert_eq!(
                engine
                    .store
                    .latest_events(room.room_id, 10)
                    .expect("replayed events")
                    .len(),
                1,
                "{action}"
            );
            assert_eq!(
                engine
                    .store
                    .moderation_audit_page(room.room_id, None, 10)
                    .expect("replayed moderation audit")
                    .records
                    .len(),
                1,
                "{action}"
            );
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
    fn retained_history_resource_and_paging_expose_only_surviving_events() {
        let store = OmenchatStore::in_memory()
            .expect("store")
            .with_room_history_retention(crate::store::RoomHistoryRetentionPolicy {
                enabled: true,
                max_age_days: 3_650,
                max_events_per_room: 3,
                max_bytes_per_room: u64::MAX,
            });
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(&peer().identity_hash, "Alice", Some("lxmf-a"))
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        for body in ["one", "two", "three", "four"] {
            store
                .append_event(
                    room.room_id,
                    Some(user.user_id),
                    ServerRoomEventKind::Message { body: body.into() },
                )
                .expect("append retained event");
        }
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                history_batch_size: 10,
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );

        let history = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    1,
                    Some(room.room_id),
                    FrameBody::Fields(vec![FrameValue::U64(u64::MAX)]),
                ),
            )
            .expect("retained resource history");
        assert_eq!(history[0].op, ChatOp::HistoryResourceOffer);
        let offer = decode_resource_offer_body(&history[0].body).expect("resource offer");
        let payload = engine
            .resource_payload(&offer.resource_id)
            .expect("resource lookup")
            .expect("resource payload");
        let values = decode_compressed_values_payload(&payload).expect("retained resource payload");
        let event_ids = values
            .iter()
            .filter_map(|value| match value {
                FrameValue::Array(fields) => fields.first(),
                _ => None,
            })
            .filter_map(|value| match value {
                FrameValue::U64(event_id) => Some(*event_id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(event_ids, vec![2, 3, 4]);

        let before_oldest = engine
            .handle_frame(
                &peer(),
                Frame::new(
                    ChatOp::HistoryBefore,
                    2,
                    Some(room.room_id),
                    FrameBody::Fields(vec![FrameValue::U64(2)]),
                ),
            )
            .expect("page before oldest retained event");
        assert_eq!(before_oldest[0].op, ChatOp::HistoryEnd);
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
