use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::error::ServerResult;
use crate::protocol::codec::{decode_frame, encode_frame};
use crate::protocol::{
    parse_session_accept_negotiation, parse_session_open_negotiation, ChatErrorCode, ChatOp,
    ClientInstanceId, DurableMutationEnvelope, Frame, FrameBody, FrameValue, RoomId,
    DURABLE_MUTATION_CAPABILITY, DURABLE_MUTATION_ENVELOPE_TAG, DURABLE_NOTICE_ACK_CAPABILITY,
    REPLY_MENTIONS_CAPABILITY,
};
use crate::session::{DurableMutationPeerContext, ServerPeer, SessionEngine};
use crate::transport::{
    release_response_resource, send_response_frame_with_context, LinkId, OmenchatTransport,
    OMENCHAT_LINK_CONTEXT,
};

const REPLAY_CACHE_MAX_ITEMS: usize = 1_024;
const REPLAY_CACHE_MAX_BYTES: usize = 4 * 1024 * 1024;
const REPLAY_CACHE_MAX_ITEMS_PER_LINK: usize = 64;
const REPLAY_CACHE_MAX_BYTES_PER_LINK: usize = 256 * 1024;
const REPLAY_CACHE_MAX_ENTRY_BYTES: usize = 64 * 1024;
const ACTIVE_LINK_MAX_ITEMS: usize = 256;
const PENDING_HANDSHAKE_MAX_ITEMS: usize = 32;
const HANDSHAKE_TIMEOUT_SECONDS: i64 = 30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OmenchatLinkEvent {
    LinkOpened {
        link_id: LinkId,
        peer: ServerPeer,
    },
    PeerIdentified {
        link_id: LinkId,
        identity_hash: [u8; 16],
    },
    LinkData {
        link_id: LinkId,
        context: u8,
        data: Vec<u8>,
    },
    ResourceReceived {
        link_id: LinkId,
        data: Vec<u8>,
        metadata: Option<Vec<u8>>,
    },
    ResourceTerminal {
        link_id: LinkId,
        direction: LiveResourceDirection,
        outcome: LiveResourceOutcome,
    },
    LinkClosed {
        link_id: LinkId,
        reason: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveResourceDirection {
    Inbound,
    Outbound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveResourceOutcome {
    Complete,
    Failed,
    Cancelled,
}

#[cfg(test)]
#[path = "live_link_soak_tests.rs"]
mod link_soak_tests;

#[cfg(test)]
#[path = "live_retry_safety_tests.rs"]
mod retry_safety_tests;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveServerStats {
    pub active_links: usize,
    pub links_opened: u64,
    pub links_closed: u64,
    pub pending_handshakes: usize,
    pub link_admission_rejected: u64,
    pub handshake_expired: u64,
    pub frames_in: u64,
    pub frames_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub resource_bytes_out: u64,
    pub resource_inbound_failed: u64,
    pub resource_outbound_complete: u64,
    pub resource_outbound_failed: u64,
    pub resource_outbound_cancelled: u64,
    pub upload_offers_released_on_resource_failure: u64,
    pub upload_offers_in: u64,
    pub upload_fetches_in: u64,
    pub upload_resources_in: u64,
    pub upload_resource_bytes_in: u64,
    pub upload_resource_offers_out: u64,
    pub upload_inline_chunks_out: u64,
    pub upload_inline_bytes_out: u64,
    pub session_requests_in: u64,
    pub room_navigation_in: u64,
    pub chat_messages_in: u64,
    pub history_requests_in: u64,
    pub pings_in: u64,
    pub commands_in: u64,
    pub resources_offered: u64,
    pub ignored_packets: u64,
    pub unknown_link_packets: u64,
    pub protocol_errors: u64,
    pub replayed_operations: u64,
    pub replay_collisions: u64,
    pub replay_cache_rejected: u64,
    pub replay_cache_items: usize,
    pub replay_cache_bytes: usize,
    pub pending_resource_items: usize,
    pub pending_resource_bytes: usize,
    pub pending_resource_rejected: u64,
    pub pending_upload_items: usize,
    pub pending_upload_identities: usize,
    pub pending_upload_rejected: u64,
    pub pending_upload_expired: u64,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
struct ReplayEntry {
    request: Vec<u8>,
    origin_responses: Vec<Frame>,
    retained_bytes: usize,
}

#[derive(Debug, Default)]
struct LinkReplayCache {
    entries: BTreeMap<(LinkId, u32), ReplayEntry>,
    insertion_order: VecDeque<(LinkId, u32)>,
    link_usage: BTreeMap<LinkId, (usize, usize)>,
    retained_bytes: usize,
}

enum ReplayLookup {
    Miss,
    Hit(Vec<Frame>),
    Collision,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FrameDispatchOutcome {
    session_accepted: bool,
    accepted_client_instance_id: Option<ClientInstanceId>,
    durable_notice_ack_accepted: bool,
    reply_mentions_accepted: bool,
    part_succeeded: bool,
    moderation_disconnect_succeeded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DurableSessionBinding {
    identity_hash: Vec<u8>,
    client_instance_id: ClientInstanceId,
    durable_notice_ack: bool,
    reply_mentions: bool,
}

impl DurableSessionBinding {
    #[cfg(test)]
    fn without_notice_ack(identity_hash: Vec<u8>, client_instance_id: ClientInstanceId) -> Self {
        Self {
            identity_hash,
            client_instance_id,
            durable_notice_ack: false,
            reply_mentions: false,
        }
    }

    #[cfg(test)]
    fn with_notice_ack(identity_hash: Vec<u8>, client_instance_id: ClientInstanceId) -> Self {
        Self {
            identity_hash,
            client_instance_id,
            durable_notice_ack: true,
            reply_mentions: false,
        }
    }
}

impl LinkReplayCache {
    fn lookup(&self, link_id: LinkId, seq: u32, request: &[u8]) -> ReplayLookup {
        let Some(entry) = self.entries.get(&(link_id, seq)) else {
            return ReplayLookup::Miss;
        };
        if entry.request == request {
            ReplayLookup::Hit(entry.origin_responses.clone())
        } else {
            ReplayLookup::Collision
        }
    }

    fn insert(
        &mut self,
        link_id: LinkId,
        seq: u32,
        mut request: Vec<u8>,
        origin_responses: Vec<Frame>,
    ) -> bool {
        request.shrink_to_fit();
        let retained_bytes = request
            .capacity()
            .saturating_add(
                origin_responses
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Frame>()),
            )
            .saturating_add(
                origin_responses
                    .iter()
                    .map(frame_body_owned_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(std::mem::size_of::<ReplayEntry>())
            .saturating_add(2usize.saturating_mul(std::mem::size_of::<(LinkId, u32)>()));
        if retained_bytes > REPLAY_CACHE_MAX_ENTRY_BYTES
            || retained_bytes > REPLAY_CACHE_MAX_BYTES_PER_LINK
            || retained_bytes > REPLAY_CACHE_MAX_BYTES
        {
            return false;
        }

        let key = (link_id, seq);
        if self.entries.contains_key(&key) {
            return false;
        }
        while self.link_usage.get(&link_id).is_some_and(|(items, bytes)| {
            *items >= REPLAY_CACHE_MAX_ITEMS_PER_LINK
                || bytes.saturating_add(retained_bytes) > REPLAY_CACHE_MAX_BYTES_PER_LINK
        }) {
            let Some(oldest) = self
                .insertion_order
                .iter()
                .copied()
                .find(|(candidate_link, _)| *candidate_link == link_id)
            else {
                return false;
            };
            self.remove(oldest);
        }
        while self.entries.len() >= REPLAY_CACHE_MAX_ITEMS
            || self.retained_bytes.saturating_add(retained_bytes) > REPLAY_CACHE_MAX_BYTES
        {
            let Some(oldest) = self.insertion_order.front().copied() else {
                return false;
            };
            self.remove(oldest);
        }

        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        let usage = self.link_usage.entry(link_id).or_default();
        usage.0 = usage.0.saturating_add(1);
        usage.1 = usage.1.saturating_add(retained_bytes);
        self.insertion_order.push_back(key);
        self.entries.insert(
            key,
            ReplayEntry {
                request,
                origin_responses,
                retained_bytes,
            },
        );
        true
    }

    fn remove_link(&mut self, link_id: LinkId) {
        while let Some(key) = self
            .insertion_order
            .iter()
            .copied()
            .find(|(candidate_link, _)| *candidate_link == link_id)
        {
            self.remove(key);
        }
    }

    fn remove(&mut self, key: (LinkId, u32)) {
        let Some(entry) = self.entries.remove(&key) else {
            self.insertion_order.retain(|candidate| candidate != &key);
            return;
        };
        self.retained_bytes = self.retained_bytes.saturating_sub(entry.retained_bytes);
        if let Some(usage) = self.link_usage.get_mut(&key.0) {
            usage.0 = usage.0.saturating_sub(1);
            usage.1 = usage.1.saturating_sub(entry.retained_bytes);
            if usage.0 == 0 {
                self.link_usage.remove(&key.0);
            }
        }
        self.insertion_order.retain(|candidate| candidate != &key);
    }
}

fn frame_body_owned_bytes(frame: &Frame) -> usize {
    match &frame.body {
        FrameBody::Empty => 0,
        FrameBody::Text(value) => value.capacity(),
        FrameBody::Fields(values) => values
            .capacity()
            .saturating_mul(std::mem::size_of::<FrameValue>())
            .saturating_add(values.iter().map(frame_value_owned_bytes).sum::<usize>()),
    }
}

fn frame_value_owned_bytes(value: &FrameValue) -> usize {
    match value {
        FrameValue::String(value) => value.capacity(),
        FrameValue::Bytes(value) => value.capacity(),
        FrameValue::Array(values) => values
            .capacity()
            .saturating_mul(std::mem::size_of::<FrameValue>())
            .saturating_add(values.iter().map(frame_value_owned_bytes).sum::<usize>()),
        FrameValue::Nil | FrameValue::Bool(_) | FrameValue::U64(_) | FrameValue::I64(_) => 0,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveLinkSummary {
    pub link_id: LinkId,
    pub identity_hash: Vec<u8>,
    pub display_name: String,
    pub room_id: Option<RoomId>,
    pub connected_at_unix: i64,
    pub traffic: LinkTrafficSummary,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkTrafficSummary {
    pub frames_in: u64,
    pub bytes_in: u64,
    pub session_requests: u64,
    pub room_navigation: u64,
    pub chat_messages: u64,
    pub history_requests: u64,
    pub pings: u64,
    pub commands: u64,
    pub upload_requests: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedLinkSummary {
    pub link_id: LinkId,
    pub identity_hash: Option<Vec<u8>>,
    pub display_name: String,
    pub room_id: Option<RoomId>,
    pub connected_at_unix: i64,
    pub closed_at_unix: i64,
    pub reason: String,
}

impl LiveServerStats {
    pub fn summary_line(&self) -> String {
        format!(
            "stats: active_links={} links_opened={} links_closed={} handshakes=pending:{} rejected:{} expired:{} frames_in={} frames_out={} traffic_in={} traffic_out={} resource_out={} resource_terminal=in_failed:{} out_complete:{} out_failed:{} out_cancelled:{} upload_offers_released:{} uploads=offers_in:{} fetches_in:{} resources_in:{} ({}) inline_out:{} ({}) resource_offers_out:{} requests=session:{} room:{} chat:{} history:{} ping:{} command:{} resources_offered={} ignored_context={} unknown_link={} protocol_errors={} replay=hits:{} collisions:{} cache_rejected:{} cache_items:{} cache_bytes:{} pending_resources=items:{} bytes:{} rejected:{} pending_uploads=items:{} identities:{} rejected:{} expired:{}",
            self.active_links,
            self.links_opened,
            self.links_closed,
            self.pending_handshakes,
            self.link_admission_rejected,
            self.handshake_expired,
            self.frames_in,
            self.frames_out,
            human_bytes(self.bytes_in),
            human_bytes(self.bytes_out),
            human_bytes(self.resource_bytes_out),
            self.resource_inbound_failed,
            self.resource_outbound_complete,
            self.resource_outbound_failed,
            self.resource_outbound_cancelled,
            self.upload_offers_released_on_resource_failure,
            self.upload_offers_in,
            self.upload_fetches_in,
            self.upload_resources_in,
            human_bytes(self.upload_resource_bytes_in),
            self.upload_inline_chunks_out,
            human_bytes(self.upload_inline_bytes_out),
            self.upload_resource_offers_out,
            self.session_requests_in,
            self.room_navigation_in,
            self.chat_messages_in,
            self.history_requests_in,
            self.pings_in,
            self.commands_in,
            self.resources_offered,
            self.ignored_packets,
            self.unknown_link_packets,
            self.protocol_errors,
            self.replayed_operations,
            self.replay_collisions,
            self.replay_cache_rejected,
            self.replay_cache_items,
            self.replay_cache_bytes,
            self.pending_resource_items,
            self.pending_resource_bytes,
            self.pending_resource_rejected,
            self.pending_upload_items,
            self.pending_upload_identities,
            self.pending_upload_rejected,
            self.pending_upload_expired,
        )
    }

    pub fn traffic_in_frames(&self) -> u64 {
        self.session_requests_in
            .saturating_add(self.room_navigation_in)
            .saturating_add(self.chat_messages_in)
            .saturating_add(self.history_requests_in)
            .saturating_add(self.pings_in)
            .saturating_add(self.commands_in)
    }
}

pub struct OmenchatLiveServer<T> {
    engine: SessionEngine,
    transport: T,
    peers: BTreeMap<LinkId, ServerPeer>,
    link_rooms: BTreeMap<LinkId, RoomId>,
    link_response_contexts: BTreeMap<LinkId, u8>,
    link_opened_at: BTreeMap<LinkId, i64>,
    link_traffic: BTreeMap<LinkId, LinkTrafficSummary>,
    identified_links: BTreeSet<LinkId>,
    session_open_links: BTreeSet<LinkId>,
    durable_sessions: BTreeMap<LinkId, DurableSessionBinding>,
    recent_closed_links: VecDeque<ClosedLinkSummary>,
    replay_cache: LinkReplayCache,
    stats: LiveServerStats,
}

impl<T: OmenchatTransport> OmenchatLiveServer<T> {
    pub fn new(engine: SessionEngine, transport: T) -> Self {
        Self {
            engine,
            transport,
            peers: BTreeMap::new(),
            link_rooms: BTreeMap::new(),
            link_response_contexts: BTreeMap::new(),
            link_opened_at: BTreeMap::new(),
            link_traffic: BTreeMap::new(),
            identified_links: BTreeSet::new(),
            session_open_links: BTreeSet::new(),
            durable_sessions: BTreeMap::new(),
            recent_closed_links: VecDeque::new(),
            replay_cache: LinkReplayCache::default(),
            stats: LiveServerStats::default(),
        }
    }

    pub fn handle_event(&mut self, event: OmenchatLinkEvent) -> ServerResult<()> {
        match event {
            OmenchatLinkEvent::LinkOpened { link_id, peer } => {
                let existing_link = self.peers.contains_key(&link_id);
                if !existing_link && !self.admit_new_link(link_id) {
                    return Ok(());
                }
                let peer = self.peer_for_link_open(link_id, peer);
                if !is_provisional_peer(&peer) {
                    self.identified_links.insert(link_id);
                }
                self.replace_duplicate_peer_links(link_id, &peer);
                self.peers.insert(link_id, peer);
                self.link_opened_at
                    .entry(link_id)
                    .or_insert_with(current_unix_secs);
                self.link_traffic.entry(link_id).or_default();
                if !existing_link {
                    self.stats.links_opened = self.stats.links_opened.saturating_add(1);
                }
                self.refresh_active_links();
                Ok(())
            }
            OmenchatLinkEvent::PeerIdentified {
                link_id,
                identity_hash,
            } => {
                let Some(existing) = self.peers.get(&link_id).cloned() else {
                    self.stats.unknown_link_packets =
                        self.stats.unknown_link_packets.saturating_add(1);
                    return Ok(());
                };
                let mut identified = existing;
                identified.identity_hash = identity_hash.to_vec();
                self.durable_sessions.remove(&link_id);
                self.identified_links.insert(link_id);
                self.replace_duplicate_peer_links(link_id, &identified);
                self.peers.insert(link_id, identified);
                self.refresh_active_links();
                Ok(())
            }
            OmenchatLinkEvent::LinkData {
                link_id,
                context,
                data,
            } => {
                let frame = decode_frame(&data).ok();
                if context != OMENCHAT_LINK_CONTEXT && frame.is_none() {
                    self.stats.ignored_packets = self.stats.ignored_packets.saturating_add(1);
                    return Ok(());
                }
                if frame.is_some() && context != 0 {
                    self.link_response_contexts.insert(link_id, context);
                }
                let peer = match self.peers.get(&link_id).cloned() {
                    Some(peer) => peer,
                    None => {
                        if !frame
                            .as_ref()
                            .map(|frame| frame.op == ChatOp::SessionOpen)
                            .unwrap_or(false)
                        {
                            self.stats.unknown_link_packets =
                                self.stats.unknown_link_packets.saturating_add(1);
                            return Ok(());
                        }
                        let provisional = ServerPeer {
                            identity_hash: link_id.to_vec(),
                            display_name: format!("link-{}", short_link_id(&link_id)),
                            lxmf_destination: None,
                        };
                        if !self.admit_new_link(link_id) {
                            return Ok(());
                        }
                        self.peers.insert(link_id, provisional.clone());
                        self.link_opened_at
                            .entry(link_id)
                            .or_insert_with(current_unix_secs);
                        self.link_traffic.entry(link_id).or_default();
                        self.stats.links_opened = self.stats.links_opened.saturating_add(1);
                        self.refresh_active_links();
                        provisional
                    }
                };
                let candidate_peer =
                    peer_from_session_open(&peer, &data).unwrap_or_else(|| peer.clone());
                self.record_link_traffic(
                    link_id,
                    data.len() as u64,
                    frame.as_ref().map(|frame| frame.op),
                );
                let joined_room_id = frame.as_ref().and_then(joined_room_id_from_frame);
                let parted_room_id = frame.as_ref().and_then(parted_room_id_from_frame);
                let active_room_peers = joined_room_id
                    .or(parted_room_id)
                    .map(|room_id| self.active_peers_for_room(link_id, room_id, &candidate_peer))
                    .unwrap_or_default();
                let moderation_disconnect = frame.as_ref().and_then(|frame| {
                    self.engine
                        .moderation_disconnect_target_for_frame(
                            &candidate_peer,
                            frame,
                            &active_room_peers,
                        )
                        .ok()
                        .flatten()
                });
                let frames_before = self.transport.frame_count();
                let resources_before = self.transport.resource_count();
                let bytes_before = self.transport.byte_count();
                let resource_bytes_before = self.transport.resource_byte_count();
                let dispatch = match self.handle_decoded_frame(
                    link_id,
                    &candidate_peer,
                    frame.clone(),
                    &active_room_peers,
                ) {
                    Ok(dispatch) => dispatch,
                    Err(error) => {
                        self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                        self.stats.last_error = Some(error.to_string());
                        return Ok(());
                    }
                };
                if dispatch.session_accepted {
                    if candidate_peer != peer {
                        self.replace_duplicate_peer_links(link_id, &candidate_peer);
                        self.peers.insert(link_id, candidate_peer.clone());
                    }
                    self.session_open_links.insert(link_id);
                    self.durable_sessions.remove(&link_id);
                    if self.identified_links.contains(&link_id) {
                        if let Some(client_instance_id) = dispatch.accepted_client_instance_id {
                            self.durable_sessions.insert(
                                link_id,
                                DurableSessionBinding {
                                    identity_hash: candidate_peer.identity_hash.clone(),
                                    client_instance_id,
                                    durable_notice_ack: dispatch.durable_notice_ack_accepted,
                                    reply_mentions: dispatch.reply_mentions_accepted,
                                },
                            );
                        }
                    }
                    self.refresh_active_links();
                }
                if dispatch.moderation_disconnect_succeeded {
                    if let Some(identity_hash) = moderation_disconnect {
                        self.disconnect_identity(&identity_hash);
                    }
                }
                if let Some(room_id) = joined_room_id {
                    self.link_rooms.insert(link_id, room_id);
                }
                if dispatch.part_succeeded {
                    if let Some(room_id) = parted_room_id {
                        if self.link_rooms.remove(&link_id).is_some() {
                            self.broadcast_userlist_for_room(room_id)?;
                        }
                    }
                }
                self.stats.frames_in = self.stats.frames_in.saturating_add(1);
                self.stats.bytes_in = self.stats.bytes_in.saturating_add(data.len() as u64);
                if let Some(frame) = frame.as_ref() {
                    self.stats.count_inbound_op(frame.op);
                }
                self.stats.frames_out = self
                    .stats
                    .frames_out
                    .saturating_add(self.transport.frame_count().saturating_sub(frames_before));
                self.stats.bytes_out = self
                    .stats
                    .bytes_out
                    .saturating_add(self.transport.byte_count().saturating_sub(bytes_before));
                self.stats.resources_offered = self.stats.resources_offered.saturating_add(
                    self.transport
                        .resource_count()
                        .saturating_sub(resources_before),
                );
                self.stats.resource_bytes_out = self.stats.resource_bytes_out.saturating_add(
                    self.transport
                        .resource_byte_count()
                        .saturating_sub(resource_bytes_before),
                );
                Ok(())
            }
            OmenchatLinkEvent::ResourceReceived {
                link_id,
                data,
                metadata,
            } => {
                let Some(peer) = self.peers.get(&link_id).cloned() else {
                    self.stats.unknown_link_packets =
                        self.stats.unknown_link_packets.saturating_add(1);
                    return Ok(());
                };
                let Some(resource_id) = resource_id_from_metadata(metadata.as_deref()) else {
                    self.stats.ignored_packets = self.stats.ignored_packets.saturating_add(1);
                    return Ok(());
                };
                if resource_id.starts_with("upload:") {
                    self.stats.upload_resources_in =
                        self.stats.upload_resources_in.saturating_add(1);
                    self.stats.upload_resource_bytes_in = self
                        .stats
                        .upload_resource_bytes_in
                        .saturating_add(data.len() as u64);
                }
                let frames_before = self.transport.frame_count();
                let bytes_before = self.transport.byte_count();
                let responses =
                    self.engine
                        .handle_upload_resource(&peer, &resource_id, data.clone())?;
                let send_result = responses.iter().try_for_each(|response| {
                    self.stats.count_outbound_op(response);
                    self.send_response_frame(link_id, response)?;
                    self.broadcast_room_event(link_id, response)
                });
                for response in &responses {
                    release_response_resource(&self.engine, response)?;
                }
                send_result?;
                self.stats.frames_out = self
                    .stats
                    .frames_out
                    .saturating_add(self.transport.frame_count().saturating_sub(frames_before));
                self.stats.bytes_in = self.stats.bytes_in.saturating_add(data.len() as u64);
                self.stats.bytes_out = self
                    .stats
                    .bytes_out
                    .saturating_add(self.transport.byte_count().saturating_sub(bytes_before));
                Ok(())
            }
            OmenchatLinkEvent::ResourceTerminal {
                link_id,
                direction,
                outcome,
            } => {
                match (direction, outcome) {
                    (LiveResourceDirection::Inbound, LiveResourceOutcome::Failed) => {
                        self.stats.resource_inbound_failed =
                            self.stats.resource_inbound_failed.saturating_add(1);
                        if let Some(identity_hash) = self
                            .peers
                            .get(&link_id)
                            .map(|peer| peer.identity_hash.clone())
                        {
                            let released =
                                self.discard_pending_uploads_for_identity(&identity_hash);
                            self.stats.upload_offers_released_on_resource_failure = self
                                .stats
                                .upload_offers_released_on_resource_failure
                                .saturating_add(released as u64);
                        }
                    }
                    (LiveResourceDirection::Outbound, LiveResourceOutcome::Complete) => {
                        self.stats.resource_outbound_complete =
                            self.stats.resource_outbound_complete.saturating_add(1);
                    }
                    (LiveResourceDirection::Outbound, LiveResourceOutcome::Failed) => {
                        self.stats.resource_outbound_failed =
                            self.stats.resource_outbound_failed.saturating_add(1);
                    }
                    (LiveResourceDirection::Outbound, LiveResourceOutcome::Cancelled) => {
                        self.stats.resource_outbound_cancelled =
                            self.stats.resource_outbound_cancelled.saturating_add(1);
                    }
                    _ => {
                        self.stats.ignored_packets = self.stats.ignored_packets.saturating_add(1);
                    }
                }
                Ok(())
            }
            OmenchatLinkEvent::LinkClosed { link_id, reason } => {
                let room_id = self.link_rooms.get(&link_id).copied();
                let identity_hash = self
                    .peers
                    .get(&link_id)
                    .map(|peer| peer.identity_hash.clone());
                self.record_closed_link(link_id, reason);
                if let Some(identity_hash) = identity_hash {
                    self.discard_pending_uploads_for_identity(&identity_hash);
                }
                if self.peers.remove(&link_id).is_some() {
                    self.stats.links_closed = self.stats.links_closed.saturating_add(1);
                }
                self.replay_cache.remove_link(link_id);
                self.sync_replay_cache_stats();
                self.link_rooms.remove(&link_id);
                self.link_response_contexts.remove(&link_id);
                self.link_opened_at.remove(&link_id);
                self.link_traffic.remove(&link_id);
                self.identified_links.remove(&link_id);
                self.session_open_links.remove(&link_id);
                self.durable_sessions.remove(&link_id);
                if let Some(room_id) = room_id {
                    self.broadcast_userlist_for_room(room_id)?;
                }
                self.refresh_active_links();
                Ok(())
            }
        }
    }

    fn handle_decoded_frame(
        &mut self,
        link_id: LinkId,
        peer: &ServerPeer,
        frame: Option<crate::protocol::Frame>,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<FrameDispatchOutcome> {
        let Some(frame) = frame else {
            return Err(crate::error::ServerError::Message(
                "OMENchat frame decode failed".into(),
            ));
        };
        let request_op = frame.op;
        let request_seq = frame.seq;
        if is_durable_mutation_envelope(&frame.body) {
            return self.handle_durable_mutation_frame(link_id, peer, frame, active_room_peers);
        }
        let requested_client_instance_id = requested_durable_client_instance(&frame);
        let notice_ack_requested =
            session_open_requests_capability(&frame, DURABLE_NOTICE_ACK_CAPABILITY);
        let reply_mentions_requested =
            session_open_requests_capability(&frame, REPLY_MENTIONS_CAPABILITY);
        let replay_candidate = is_replay_guarded_request(&frame);
        let request_fingerprint = if replay_candidate {
            Some(encode_frame(&frame).map_err(|error| {
                crate::error::ServerError::Message(format!(
                    "OMENchat replay fingerprint encode failed: {error}"
                ))
            })?)
        } else {
            None
        };
        if let Some(request_fingerprint) = request_fingerprint.as_deref() {
            match self
                .replay_cache
                .lookup(link_id, request_seq, request_fingerprint)
            {
                ReplayLookup::Hit(responses) => {
                    self.stats.replayed_operations =
                        self.stats.replayed_operations.saturating_add(1);
                    for response in responses {
                        self.stats.count_outbound_op(&response);
                        self.send_response_frame(link_id, &response)?;
                    }
                    return Ok(FrameDispatchOutcome::default());
                }
                ReplayLookup::Collision => {
                    self.stats.replay_collisions = self.stats.replay_collisions.saturating_add(1);
                    self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                    let response = replay_collision_frame(request_seq, frame.room_id);
                    self.stats.count_outbound_op(&response);
                    self.send_response_frame(link_id, &response)?;
                    return Ok(FrameDispatchOutcome::default());
                }
                ReplayLookup::Miss => {}
            }
        }

        let mut responses =
            self.engine
                .handle_frame_with_active_peers(peer, frame, active_room_peers)?;
        let reply_mentions_active = self.durable_sessions.get(&link_id).is_some_and(|binding| {
            binding.identity_hash == peer.identity_hash && binding.reply_mentions
        });
        if request_op == ChatOp::JoinRoom && reply_mentions_active {
            append_join_accept_user_id(&mut responses, self.engine.local_user_id(peer)?)?;
        }
        let dispatch = FrameDispatchOutcome {
            session_accepted: request_op == ChatOp::SessionOpen
                && responses
                    .iter()
                    .any(|response| response.op == ChatOp::SessionAccept),
            accepted_client_instance_id: accepted_durable_client_instance(
                requested_client_instance_id,
                &responses,
            ),
            durable_notice_ack_accepted: notice_ack_requested
                && responses_accept_capability(&responses, DURABLE_NOTICE_ACK_CAPABILITY),
            reply_mentions_accepted: reply_mentions_requested
                && responses_accept_capability(&responses, REPLY_MENTIONS_CAPABILITY),
            part_succeeded: request_op == ChatOp::PartRoom
                && responses.iter().any(is_successful_part_response),
            moderation_disconnect_succeeded: request_op == ChatOp::Command
                && responses.iter().any(is_successful_disconnect_response),
        };
        let mut origin_responses = Vec::with_capacity(responses.len());
        let mut broadcast_responses = Vec::new();
        for response in responses {
            if matches!(request_op, ChatOp::RoomMessage | ChatOp::RoomAction)
                && response.op == ChatOp::RoomEvent
            {
                if let Some(ack) = message_ack_from_room_event(request_seq, &response) {
                    origin_responses.push(ack);
                    broadcast_responses.push(response);
                    continue;
                }
            }
            if is_room_broadcast_response(response.op) {
                broadcast_responses.push(response.clone());
            }
            origin_responses.push(response);
        }
        if let Some(request_fingerprint) = request_fingerprint {
            if !self.replay_cache.insert(
                link_id,
                request_seq,
                request_fingerprint,
                origin_responses.clone(),
            ) {
                self.stats.replay_cache_rejected =
                    self.stats.replay_cache_rejected.saturating_add(1);
            }
            self.sync_replay_cache_stats();
        }
        let send_result = (|| -> ServerResult<()> {
            for response in &origin_responses {
                self.stats.count_outbound_op(response);
                self.send_response_frame(link_id, response)?;
            }
            for response in &broadcast_responses {
                self.broadcast_room_event(link_id, response)?;
            }
            Ok(())
        })();
        for response in origin_responses.iter().chain(&broadcast_responses) {
            release_response_resource(&self.engine, response)?;
        }
        send_result?;
        Ok(dispatch)
    }

    fn handle_durable_mutation_frame(
        &mut self,
        link_id: LinkId,
        peer: &ServerPeer,
        frame: Frame,
        active_room_peers: &[ServerPeer],
    ) -> ServerResult<FrameDispatchOutcome> {
        let envelope = match DurableMutationEnvelope::from_frame_body(&frame.body) {
            Ok(envelope) => envelope,
            Err(_) => {
                self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                let response = durable_dispatch_error_frame(
                    frame.seq,
                    frame.room_id,
                    ChatErrorCode::DurableMutationMalformed,
                    "durable mutation envelope is malformed",
                );
                self.stats.count_outbound_op(&response);
                self.send_response_frame(link_id, &response)?;
                return Ok(FrameDispatchOutcome::default());
            }
        };
        let binding = self
            .identified_links
            .contains(&link_id)
            .then(|| self.durable_sessions.get(&link_id))
            .flatten()
            .filter(|binding| binding.identity_hash == peer.identity_hash)
            .cloned();
        let Some(binding) = binding else {
            let response = durable_dispatch_error_frame(
                frame.seq,
                frame.room_id,
                ChatErrorCode::DurableMutationNotNegotiated,
                "durable mutations were not negotiated for this authenticated link",
            );
            self.stats.count_outbound_op(&response);
            self.send_response_frame(link_id, &response)?;
            return Ok(FrameDispatchOutcome::default());
        };

        let request_op = frame.op;
        let dispatch = self.engine.handle_durable_mutation_with_active_peers(
            DurableMutationPeerContext {
                peer,
                active_room_peers,
                durable_notice_ack: binding.durable_notice_ack,
                reply_mentions: binding.reply_mentions,
            },
            frame.seq,
            frame.room_id,
            request_op,
            binding.client_instance_id,
            envelope,
        )?;
        let part_succeeded =
            request_op == ChatOp::PartRoom && is_successful_part_response(&dispatch.origin);
        if let Some(identity_hash) = dispatch.disconnect_identity.as_deref() {
            self.disconnect_identity(identity_hash);
        }
        self.stats.count_outbound_op(&dispatch.origin);
        self.send_response_frame(link_id, &dispatch.origin)?;
        for broadcast in dispatch.broadcasts {
            self.broadcast_room_event(link_id, &broadcast)?;
        }
        Ok(FrameDispatchOutcome {
            part_succeeded,
            ..FrameDispatchOutcome::default()
        })
    }

    fn broadcast_room_event(
        &mut self,
        origin_link_id: LinkId,
        response: &crate::protocol::Frame,
    ) -> ServerResult<()> {
        if !is_room_broadcast_response(response.op) {
            return Ok(());
        }
        let Some(room_id) = response.room_id else {
            if response.op == ChatOp::RoomDelta {
                let link_ids = self.peer_link_ids(origin_link_id);
                for link_id in link_ids {
                    self.stats.count_outbound_op(response);
                    self.send_response_frame(link_id, response)?;
                }
            }
            return Ok(());
        };
        let link_ids = self.room_link_ids(room_id, origin_link_id);
        for link_id in link_ids {
            self.stats.count_outbound_op(response);
            self.send_response_frame(link_id, response)?;
        }
        Ok(())
    }

    fn peer_link_ids(&self, exclude_link_id: LinkId) -> Vec<LinkId> {
        self.peers
            .keys()
            .filter_map(|link_id| (*link_id != exclude_link_id).then_some(*link_id))
            .collect()
    }

    fn room_link_ids(&self, room_id: RoomId, exclude_link_id: LinkId) -> Vec<LinkId> {
        self.link_rooms
            .iter()
            .filter_map(|(link_id, active_room_id)| {
                (*link_id != exclude_link_id && *active_room_id == room_id).then_some(*link_id)
            })
            .collect()
    }

    fn broadcast_userlist_for_room(&mut self, room_id: RoomId) -> ServerResult<()> {
        let link_ids = self.room_link_ids_including(room_id);
        if link_ids.is_empty() {
            return Ok(());
        }
        let peers = self.active_peers_in_room(room_id);
        let frame = self.engine.active_userlist_frame(room_id, &peers)?;
        let send_result = link_ids
            .into_iter()
            .try_for_each(|link_id| self.send_response_frame(link_id, &frame));
        release_response_resource(&self.engine, &frame)?;
        send_result
    }

    fn send_response_frame(
        &mut self,
        link_id: LinkId,
        frame: &crate::protocol::Frame,
    ) -> ServerResult<()> {
        let context = self
            .link_response_contexts
            .get(&link_id)
            .copied()
            .unwrap_or(OMENCHAT_LINK_CONTEXT);
        send_response_frame_with_context(&self.engine, link_id, frame, &mut self.transport, context)
    }

    fn room_link_ids_including(&self, room_id: RoomId) -> Vec<LinkId> {
        self.link_rooms
            .iter()
            .filter_map(|(link_id, active_room_id)| {
                (*active_room_id == room_id).then_some(*link_id)
            })
            .collect()
    }

    fn active_peers_in_room(&self, room_id: RoomId) -> Vec<ServerPeer> {
        self.link_rooms
            .iter()
            .filter_map(|(link_id, active_room_id)| {
                (*active_room_id == room_id)
                    .then(|| self.peers.get(link_id).cloned())
                    .flatten()
            })
            .collect()
    }

    pub fn stats(&self) -> LiveServerStats {
        let mut stats = self.stats.clone();
        stats.pending_handshakes = self.pending_handshake_count();
        if let Ok((items, bytes, rejected)) = self.engine.pending_resource_metrics() {
            stats.pending_resource_items = items;
            stats.pending_resource_bytes = bytes;
            stats.pending_resource_rejected = rejected;
        }
        if let Ok((items, identities, rejected, expired)) = self.engine.pending_upload_metrics() {
            stats.pending_upload_items = items;
            stats.pending_upload_identities = identities;
            stats.pending_upload_rejected = rejected;
            stats.pending_upload_expired = expired;
        }
        stats
    }

    pub fn active_room_counts(&self) -> Vec<(RoomId, usize)> {
        let mut counts = BTreeMap::<RoomId, usize>::new();
        for room_id in self.link_rooms.values().copied() {
            *counts.entry(room_id).or_insert(0) += 1;
        }
        counts.into_iter().collect()
    }

    pub fn active_identity_counts(&self) -> Vec<(Vec<u8>, usize)> {
        let mut counts = BTreeMap::<Vec<u8>, usize>::new();
        for peer in self.peers.values() {
            *counts.entry(peer.identity_hash.clone()).or_insert(0) += 1;
        }
        counts.into_iter().collect()
    }

    pub fn active_link_summaries(&self) -> Vec<ActiveLinkSummary> {
        self.peers
            .iter()
            .map(|(link_id, peer)| ActiveLinkSummary {
                link_id: *link_id,
                identity_hash: peer.identity_hash.clone(),
                display_name: peer.display_name.clone(),
                room_id: self.link_rooms.get(link_id).copied(),
                connected_at_unix: self
                    .link_opened_at
                    .get(link_id)
                    .copied()
                    .unwrap_or_default(),
                traffic: self.link_traffic.get(link_id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    pub fn recent_closed_link_summaries(&self) -> Vec<ClosedLinkSummary> {
        self.recent_closed_links.iter().cloned().collect()
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn disconnect_identity(&mut self, identity_hash: &[u8]) -> usize {
        let links = self
            .peers
            .iter()
            .filter_map(|(link_id, peer)| {
                (peer.identity_hash.as_slice() == identity_hash).then_some(*link_id)
            })
            .collect::<Vec<_>>();
        let affected_rooms = links
            .iter()
            .filter_map(|link_id| self.link_rooms.get(link_id).copied())
            .collect::<Vec<_>>();
        self.discard_pending_uploads_for_identity(identity_hash);
        for link_id in &links {
            if let Err(error) = self.transport.close_link(*link_id) {
                self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                self.stats.last_error = Some(error.to_string());
            }
            self.record_closed_link(*link_id, Some("admin disconnect".into()));
            self.peers.remove(link_id);
            self.replay_cache.remove_link(*link_id);
            self.link_rooms.remove(link_id);
            self.link_response_contexts.remove(link_id);
            self.link_opened_at.remove(link_id);
            self.link_traffic.remove(link_id);
            self.identified_links.remove(link_id);
            self.session_open_links.remove(link_id);
            self.durable_sessions.remove(link_id);
        }
        self.stats.links_closed = self.stats.links_closed.saturating_add(links.len() as u64);
        for room_id in unique_room_ids(affected_rooms) {
            if let Err(error) = self.broadcast_userlist_for_room(room_id) {
                self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                self.stats.last_error = Some(error.to_string());
            }
        }
        self.refresh_active_links();
        links.len()
    }

    fn refresh_active_links(&mut self) {
        self.stats.active_links = self.peers.len();
        self.stats.pending_handshakes = self.pending_handshake_count();
        self.sync_replay_cache_stats();
    }

    pub fn expire_pending_handshakes(&mut self, now_unix: i64) -> usize {
        let expired = self
            .link_opened_at
            .iter()
            .filter_map(|(link_id, opened_at)| {
                (!self.handshake_complete(*link_id)
                    && now_unix.saturating_sub(*opened_at) >= HANDSHAKE_TIMEOUT_SECONDS)
                    .then_some(*link_id)
            })
            .collect::<Vec<_>>();
        for link_id in &expired {
            self.retire_link(*link_id, "authentication/session handshake timed out");
        }
        self.stats.handshake_expired = self
            .stats
            .handshake_expired
            .saturating_add(expired.len() as u64);
        self.refresh_active_links();
        expired.len()
    }

    fn admit_new_link(&mut self, link_id: LinkId) -> bool {
        let at_total_limit = self.peers.len() >= ACTIVE_LINK_MAX_ITEMS;
        let at_pending_limit = self.pending_handshake_count() >= PENDING_HANDSHAKE_MAX_ITEMS;
        if !at_total_limit && !at_pending_limit {
            return true;
        }
        self.stats.link_admission_rejected = self.stats.link_admission_rejected.saturating_add(1);
        if let Err(error) = self.transport.close_link(link_id) {
            self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
            self.stats.last_error = Some(error.to_string());
        }
        false
    }

    fn handshake_complete(&self, link_id: LinkId) -> bool {
        self.identified_links.contains(&link_id) && self.session_open_links.contains(&link_id)
    }

    fn pending_handshake_count(&self) -> usize {
        self.peers
            .keys()
            .filter(|link_id| !self.handshake_complete(**link_id))
            .count()
    }

    fn sync_replay_cache_stats(&mut self) {
        self.stats.replay_cache_items = self.replay_cache.entries.len();
        self.stats.replay_cache_bytes = self.replay_cache.retained_bytes;
    }

    fn record_link_traffic(&mut self, link_id: LinkId, bytes: u64, op: Option<ChatOp>) {
        let traffic = self.link_traffic.entry(link_id).or_default();
        traffic.frames_in = traffic.frames_in.saturating_add(1);
        traffic.bytes_in = traffic.bytes_in.saturating_add(bytes);
        match op {
            Some(ChatOp::SessionOpen) => {
                traffic.session_requests = traffic.session_requests.saturating_add(1)
            }
            Some(
                ChatOp::JoinRoom
                | ChatOp::PartRoom
                | ChatOp::RoomSubscribe
                | ChatOp::RoomUnsubscribe,
            ) => traffic.room_navigation = traffic.room_navigation.saturating_add(1),
            Some(ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice) => {
                traffic.chat_messages = traffic.chat_messages.saturating_add(1)
            }
            Some(ChatOp::HistoryBefore | ChatOp::HistoryRecent) => {
                traffic.history_requests = traffic.history_requests.saturating_add(1)
            }
            Some(ChatOp::Ping) => traffic.pings = traffic.pings.saturating_add(1),
            Some(ChatOp::Command) => traffic.commands = traffic.commands.saturating_add(1),
            Some(ChatOp::UploadOffer | ChatOp::UploadFetch) => {
                traffic.upload_requests = traffic.upload_requests.saturating_add(1)
            }
            _ => {}
        }
    }

    fn record_closed_link(&mut self, link_id: LinkId, reason: Option<String>) {
        let peer = self.peers.get(&link_id);
        let closed = ClosedLinkSummary {
            link_id,
            identity_hash: peer.map(|peer| peer.identity_hash.clone()),
            display_name: peer
                .map(|peer| peer.display_name.clone())
                .unwrap_or_else(|| "unknown".into()),
            room_id: self.link_rooms.get(&link_id).copied(),
            connected_at_unix: self
                .link_opened_at
                .get(&link_id)
                .copied()
                .unwrap_or_default(),
            closed_at_unix: current_unix_secs(),
            reason: reason.unwrap_or_else(|| "unspecified".into()),
        };
        self.recent_closed_links.push_front(closed);
        while self.recent_closed_links.len() > 12 {
            self.recent_closed_links.pop_back();
        }
    }

    fn peer_for_link_open(&self, link_id: LinkId, peer: ServerPeer) -> ServerPeer {
        let Some(existing) = self.peers.get(&link_id) else {
            return peer;
        };
        if !is_provisional_peer(existing) && is_provisional_peer(&peer) {
            return existing.clone();
        }
        let mut merged = peer;
        if is_generated_peer_name(&merged.display_name)
            && !is_generated_peer_name(&existing.display_name)
        {
            merged.display_name = existing.display_name.clone();
        }
        if merged.lxmf_destination.is_none() {
            merged.lxmf_destination = existing.lxmf_destination.clone();
        }
        merged
    }

    fn replace_duplicate_peer_links(&mut self, link_id: LinkId, peer: &ServerPeer) {
        let duplicates = self
            .peers
            .iter()
            .filter_map(|(existing_link_id, existing_peer)| {
                (*existing_link_id != link_id && existing_peer.identity_hash == peer.identity_hash)
                    .then_some(*existing_link_id)
            })
            .collect::<Vec<_>>();
        for duplicate in duplicates {
            self.retire_duplicate_peer_link(duplicate);
        }
    }

    fn retire_duplicate_peer_link(&mut self, link_id: LinkId) {
        let identity_hash = self
            .peers
            .get(&link_id)
            .map(|peer| peer.identity_hash.clone());
        self.record_closed_link(link_id, Some("duplicate identity link replaced".into()));
        if let Some(identity_hash) = identity_hash {
            self.discard_pending_uploads_for_identity(&identity_hash);
        }
        if let Err(error) = self.transport.close_link(link_id) {
            self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
            self.stats.last_error = Some(error.to_string());
        }
        if self.peers.remove(&link_id).is_some() {
            self.stats.links_closed = self.stats.links_closed.saturating_add(1);
        }
        self.replay_cache.remove_link(link_id);
        self.link_rooms.remove(&link_id);
        self.link_response_contexts.remove(&link_id);
        self.link_opened_at.remove(&link_id);
        self.link_traffic.remove(&link_id);
        self.identified_links.remove(&link_id);
        self.session_open_links.remove(&link_id);
        self.durable_sessions.remove(&link_id);
    }

    fn retire_link(&mut self, link_id: LinkId, reason: &str) {
        let identity_hash = self
            .peers
            .get(&link_id)
            .map(|peer| peer.identity_hash.clone());
        self.record_closed_link(link_id, Some(reason.into()));
        if let Some(identity_hash) = identity_hash {
            self.discard_pending_uploads_for_identity(&identity_hash);
        }
        if let Err(error) = self.transport.close_link(link_id) {
            self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
            self.stats.last_error = Some(error.to_string());
        }
        if self.peers.remove(&link_id).is_some() {
            self.stats.links_closed = self.stats.links_closed.saturating_add(1);
        }
        self.replay_cache.remove_link(link_id);
        self.link_rooms.remove(&link_id);
        self.link_response_contexts.remove(&link_id);
        self.link_opened_at.remove(&link_id);
        self.link_traffic.remove(&link_id);
        self.identified_links.remove(&link_id);
        self.session_open_links.remove(&link_id);
        self.durable_sessions.remove(&link_id);
    }

    fn discard_pending_uploads_for_identity(&mut self, identity_hash: &[u8]) -> usize {
        match self
            .engine
            .discard_pending_uploads_for_identity(identity_hash)
        {
            Ok(released) => released,
            Err(error) => {
                self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                self.stats.last_error = Some(error.to_string());
                0
            }
        }
    }

    fn active_peers_for_room(
        &self,
        current_link_id: LinkId,
        room_id: RoomId,
        current_peer: &ServerPeer,
    ) -> Vec<ServerPeer> {
        let mut active = self
            .link_rooms
            .iter()
            .filter_map(|(link_id, active_room_id)| {
                (*link_id != current_link_id && *active_room_id == room_id)
                    .then(|| self.peers.get(link_id).cloned())
                    .flatten()
            })
            .collect::<Vec<_>>();
        active.push(current_peer.clone());
        active
    }
}

fn resource_id_from_metadata(metadata: Option<&[u8]>) -> Option<String> {
    let metadata = metadata?;
    let id = metadata.strip_prefix(crate::transport::OMENCHAT_RESOURCE_METADATA_PREFIX)?;
    String::from_utf8(id.to_vec())
        .ok()
        .filter(|value| !value.is_empty())
}

impl LiveServerStats {
    fn count_inbound_op(&mut self, op: ChatOp) {
        match op {
            ChatOp::SessionOpen => {
                self.session_requests_in = self.session_requests_in.saturating_add(1)
            }
            ChatOp::JoinRoom
            | ChatOp::PartRoom
            | ChatOp::RoomSubscribe
            | ChatOp::RoomUnsubscribe => {
                self.room_navigation_in = self.room_navigation_in.saturating_add(1)
            }
            ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice => {
                self.chat_messages_in = self.chat_messages_in.saturating_add(1)
            }
            ChatOp::HistoryBefore | ChatOp::HistoryRecent => {
                self.history_requests_in = self.history_requests_in.saturating_add(1)
            }
            ChatOp::Ping => self.pings_in = self.pings_in.saturating_add(1),
            ChatOp::Command => self.commands_in = self.commands_in.saturating_add(1),
            ChatOp::UploadOffer => self.upload_offers_in = self.upload_offers_in.saturating_add(1),
            ChatOp::UploadFetch => {
                self.upload_fetches_in = self.upload_fetches_in.saturating_add(1)
            }
            _ => {}
        }
    }

    fn count_outbound_op(&mut self, frame: &crate::protocol::Frame) {
        match frame.op {
            ChatOp::UploadResourceOffer => {
                self.upload_resource_offers_out = self.upload_resource_offers_out.saturating_add(1);
            }
            ChatOp::UploadInlineChunk => {
                self.upload_inline_chunks_out = self.upload_inline_chunks_out.saturating_add(1);
                self.upload_inline_bytes_out = self
                    .upload_inline_bytes_out
                    .saturating_add(upload_inline_chunk_len(&frame.body) as u64);
            }
            _ => {}
        }
    }
}

fn upload_inline_chunk_len(body: &FrameBody) -> usize {
    match body {
        FrameBody::Fields(fields) => match fields.get(5) {
            Some(FrameValue::Bytes(bytes)) => bytes.len(),
            _ => 0,
        },
        _ => 0,
    }
}

fn message_ack_from_room_event(seq: u32, response: &Frame) -> Option<Frame> {
    if response.op != ChatOp::RoomEvent {
        return None;
    }
    let FrameBody::Fields(values) = &response.body else {
        return None;
    };
    let Some(FrameValue::Array(event_fields)) = values.first() else {
        return None;
    };
    let event_id = event_fields.first()?.clone();
    let kind = event_fields.get(1)?.clone();
    let actor_user_id = event_fields.get(2).cloned().unwrap_or(FrameValue::Nil);
    let at_unix = event_fields.get(3)?.clone();
    let actor_display_name = event_fields.get(5).cloned().unwrap_or(FrameValue::Nil);
    Some(Frame::new(
        ChatOp::MessageAck,
        seq,
        response.room_id,
        FrameBody::Fields(vec![
            event_id,
            kind,
            actor_user_id,
            at_unix,
            actor_display_name,
        ]),
    ))
}

fn append_join_accept_user_id(
    responses: &mut [Frame],
    user_id: crate::protocol::UserId,
) -> ServerResult<()> {
    let Some(response) = responses
        .iter_mut()
        .find(|response| response.op == ChatOp::JoinAccept)
    else {
        return Ok(());
    };
    let FrameBody::Fields(fields) = &mut response.body else {
        return Err(crate::error::ServerError::Message(
            "join acceptance did not contain fields".into(),
        ));
    };
    if fields.len() != 1 {
        return Err(crate::error::ServerError::Message(
            "join acceptance already contains unexpected extension fields".into(),
        ));
    }
    fields.push(FrameValue::U64(u64::from(user_id)));
    Ok(())
}

fn replay_collision_frame(seq: u32, room_id: Option<RoomId>) -> Frame {
    Frame::new(
        ChatOp::Error,
        seq,
        room_id,
        FrameBody::Fields(vec![
            FrameValue::U64(ChatErrorCode::MalformedFrame as u16 as u64),
            FrameValue::String("operation sequence was reused with different content".into()),
        ]),
    )
}

fn durable_dispatch_error_frame(
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

fn is_durable_mutation_envelope(body: &FrameBody) -> bool {
    matches!(
        body,
        FrameBody::Fields(fields)
            if matches!(
                fields.first(),
                Some(FrameValue::String(tag)) if tag == DURABLE_MUTATION_ENVELOPE_TAG
            )
    )
}

fn is_replay_guarded_request(frame: &Frame) -> bool {
    match frame.op {
        ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice | ChatOp::PartRoom => true,
        ChatOp::Command => command_name(&frame.body).is_some_and(is_mutating_command),
        _ => false,
    }
}

fn command_name(body: &FrameBody) -> Option<&str> {
    let command = match body {
        FrameBody::Text(value) => Some(value.as_str()),
        FrameBody::Fields(values) => values.iter().find_map(|value| match value {
            FrameValue::String(value) => Some(value.as_str()),
            _ => None,
        }),
        FrameBody::Empty => None,
    }?;
    command.split_whitespace().next()
}

fn is_mutating_command(command: &str) -> bool {
    matches!(
        command.to_ascii_lowercase().as_str(),
        "topic" | "create" | "kick" | "ban" | "mute" | "unmute" | "role" | "unban"
    )
}

fn command_result_name(frame: &Frame) -> Option<&str> {
    if frame.op != ChatOp::CommandResult {
        return None;
    }
    let FrameBody::Fields(values) = &frame.body else {
        return None;
    };
    values.first().and_then(|value| match value {
        FrameValue::String(value) => Some(value.as_str()),
        _ => None,
    })
}

fn is_successful_part_response(frame: &Frame) -> bool {
    command_result_name(frame) == Some("part")
}

fn is_successful_disconnect_response(frame: &Frame) -> bool {
    matches!(command_result_name(frame), Some("kick" | "ban"))
}

fn is_room_broadcast_response(op: ChatOp) -> bool {
    matches!(
        op,
        ChatOp::RoomEvent
            | ChatOp::UserDelta
            | ChatOp::RoomDelta
            | ChatOp::UserListSnapshotInline
            | ChatOp::UserListSnapshotResource
    )
}

fn unique_room_ids(room_ids: Vec<RoomId>) -> Vec<RoomId> {
    let mut unique = Vec::new();
    for room_id in room_ids {
        if !unique.contains(&room_id) {
            unique.push(room_id);
        }
    }
    unique
}

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn joined_room_id_from_frame(frame: &crate::protocol::Frame) -> Option<RoomId> {
    match frame.op {
        ChatOp::JoinRoom => Some(frame.room_id.unwrap_or(1)),
        ChatOp::RoomMessage | ChatOp::RoomAction | ChatOp::RoomNotice => frame.room_id,
        ChatOp::Command => frame.room_id,
        _ => None,
    }
}

fn parted_room_id_from_frame(frame: &crate::protocol::Frame) -> Option<RoomId> {
    if frame.op == ChatOp::PartRoom {
        frame.room_id
    } else {
        None
    }
}

fn peer_from_session_open(peer: &ServerPeer, frame_bytes: &[u8]) -> Option<ServerPeer> {
    let frame = decode_frame(frame_bytes).ok()?;
    if frame.op != ChatOp::SessionOpen {
        return None;
    }
    parse_session_open_negotiation(&frame.body).ok()?;
    let mut updated = peer.clone();
    match frame.body {
        FrameBody::Text(name) => {
            updated.display_name = sanitize_display_name(&name)?;
        }
        FrameBody::Fields(fields) => {
            if let Some(name) = fields.get(1).and_then(frame_value_string) {
                updated.display_name = sanitize_display_name(name)?;
            }
            if let Some(lxmf) = fields.get(2).and_then(frame_value_string) {
                let trimmed = lxmf.trim();
                if !trimmed.is_empty() {
                    updated.lxmf_destination = Some(trimmed.to_owned());
                }
            }
        }
        FrameBody::Empty => return None,
    }
    Some(updated)
}

fn requested_durable_client_instance(frame: &Frame) -> Option<ClientInstanceId> {
    if frame.op != ChatOp::SessionOpen {
        return None;
    }
    let negotiation = parse_session_open_negotiation(&frame.body).ok().flatten()?;
    negotiation
        .requested_capabilities
        .iter()
        .any(|capability| capability == DURABLE_MUTATION_CAPABILITY)
        .then_some(negotiation.client_instance_id)
        .flatten()
}

fn session_open_requests_capability(frame: &Frame, expected: &str) -> bool {
    if frame.op != ChatOp::SessionOpen {
        return false;
    }
    parse_session_open_negotiation(&frame.body)
        .ok()
        .flatten()
        .is_some_and(|negotiation| {
            negotiation
                .requested_capabilities
                .iter()
                .any(|capability| capability == expected)
        })
}

fn responses_accept_capability(responses: &[Frame], expected: &str) -> bool {
    responses
        .iter()
        .filter(|response| response.op == ChatOp::SessionAccept)
        .filter_map(|response| {
            parse_session_accept_negotiation(&response.body)
                .ok()
                .flatten()
        })
        .any(|negotiation| {
            negotiation
                .accepted_capabilities
                .iter()
                .any(|capability| capability == expected)
        })
}

fn accepted_durable_client_instance(
    requested: Option<ClientInstanceId>,
    responses: &[Frame],
) -> Option<ClientInstanceId> {
    let requested = requested?;
    let accepted = responses_accept_capability(responses, DURABLE_MUTATION_CAPABILITY);
    accepted.then_some(requested)
}

fn frame_value_string(value: &FrameValue) -> Option<&str> {
    match value {
        FrameValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn sanitize_display_name(name: &str) -> Option<String> {
    let cleaned = name
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(48)
        .collect::<String>();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn is_generated_peer_name(peer_name: &str) -> bool {
    peer_name.starts_with("link-") || peer_name.starts_with("peer-")
}

fn is_provisional_peer(peer: &ServerPeer) -> bool {
    peer.identity_hash.len() == 16 && peer.display_name.starts_with("link-")
}

fn short_link_id(link_id: &LinkId) -> String {
    link_id
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
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

trait TransportCounters {
    fn frame_count(&self) -> u64;
    fn resource_count(&self) -> u64;
    fn byte_count(&self) -> u64;
    fn resource_byte_count(&self) -> u64;
}

impl<T: OmenchatTransport> TransportCounters for T {
    fn frame_count(&self) -> u64 {
        self.sent_frame_count()
    }

    fn resource_count(&self) -> u64 {
        self.offered_resource_count()
    }

    fn byte_count(&self) -> u64 {
        self.sent_frame_bytes()
    }

    fn resource_byte_count(&self) -> u64 {
        self.offered_resource_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::batch::decode_compressed_values_body;
    use crate::protocol::codec::{decode_frame, encode_frame};
    use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue};
    use crate::session::{ServerPeer, SessionLimits};
    use crate::store::{OmenchatStore, ServerRoomEventKind};
    use crate::transport::{CapturedTransport, OMENCHAT_LINK_CONTEXT};

    fn assert_replayed_response(replayed: &Frame, stored: &Frame, request_seq: u32) {
        let mut expected = stored.clone();
        expected.seq = request_seq;
        assert_eq!(replayed, &expected);
    }

    #[derive(Default)]
    struct FailOnceTransport {
        captured: CapturedTransport,
        reject_next_frame: bool,
        reject_next_command_result: bool,
    }

    impl OmenchatTransport for FailOnceTransport {
        fn send_frame(&mut self, link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
            self.send_frame_with_context(link_id, frame_bytes, OMENCHAT_LINK_CONTEXT)
        }

        fn send_frame_with_context(
            &mut self,
            link_id: LinkId,
            frame_bytes: Vec<u8>,
            context: u8,
        ) -> ServerResult<()> {
            let reject_command_result = self.reject_next_command_result
                && decode_frame(&frame_bytes).is_ok_and(|frame| frame.op == ChatOp::CommandResult);
            if self.reject_next_frame || reject_command_result {
                self.reject_next_frame = false;
                self.reject_next_command_result = false;
                return Err(crate::error::ServerError::Message(
                    "injected frame delivery failure".into(),
                ));
            }
            self.captured
                .send_frame_with_context(link_id, frame_bytes, context)
        }

        fn offer_resource(
            &mut self,
            link_id: LinkId,
            resource_id: String,
            payload: Vec<u8>,
            metadata: Vec<u8>,
        ) -> ServerResult<()> {
            self.captured
                .offer_resource(link_id, resource_id, payload, metadata)
        }

        fn sent_frame_count(&self) -> u64 {
            self.captured.sent_frame_count()
        }

        fn offered_resource_count(&self) -> u64 {
            self.captured.offered_resource_count()
        }

        fn sent_frame_bytes(&self) -> u64 {
            self.captured.sent_frame_bytes()
        }

        fn offered_resource_bytes(&self) -> u64 {
            self.captured.offered_resource_bytes()
        }

        fn close_link(&mut self, link_id: LinkId) -> ServerResult<()> {
            self.captured.close_link(link_id)
        }
    }

    fn peer() -> ServerPeer {
        ServerPeer {
            identity_hash: b"peer-live".to_vec(),
            display_name: "Live Peer".into(),
            lxmf_destination: Some("lxmf-live".into()),
        }
    }

    fn temp_store_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-live-{label}-{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn replay_cache_is_per_link_item_and_byte_bounded() {
        let mut cache = LinkReplayCache::default();
        let link_id = [0x41; 16];
        for seq in 1..=REPLAY_CACHE_MAX_ITEMS_PER_LINK as u32 + 1 {
            assert!(cache.insert(
                link_id,
                seq,
                vec![seq as u8; 32],
                vec![Frame::new(
                    ChatOp::MessageAck,
                    seq,
                    Some(1),
                    FrameBody::Empty
                )],
            ));
        }

        assert_eq!(cache.entries.len(), REPLAY_CACHE_MAX_ITEMS_PER_LINK);
        assert!(cache.retained_bytes <= REPLAY_CACHE_MAX_BYTES_PER_LINK);
        assert!(matches!(
            cache.lookup(link_id, 1, &[1; 32]),
            ReplayLookup::Miss
        ));
        assert!(!cache.insert(
            link_id,
            100,
            vec![0; REPLAY_CACHE_MAX_ENTRY_BYTES + 1],
            Vec::new(),
        ));

        cache.remove_link(link_id);
        assert!(cache.entries.is_empty());
        assert!(cache.insertion_order.is_empty());
        assert!(cache.link_usage.is_empty());
        assert_eq!(cache.retained_bytes, 0);
    }

    #[test]
    fn replay_guard_covers_mutations_but_not_read_only_commands() {
        for op in [
            ChatOp::RoomMessage,
            ChatOp::RoomAction,
            ChatOp::RoomNotice,
            ChatOp::PartRoom,
        ] {
            assert!(is_replay_guarded_request(&Frame::new(
                op,
                1,
                Some(1),
                FrameBody::Empty,
            )));
        }
        for command in [
            "topic new topic",
            "create ops Operations",
            "kick Bob",
            "ban Bob",
            "mute Bob",
            "unmute Bob",
            "role Bob trusted",
            "unban Bob",
        ] {
            assert!(is_replay_guarded_request(&Frame::new(
                ChatOp::Command,
                1,
                Some(1),
                FrameBody::Text(command.into()),
            )));
        }
        assert!(!is_replay_guarded_request(&Frame::new(
            ChatOp::Command,
            1,
            Some(1),
            FrameBody::Text("rooms".into()),
        )));
        assert!(!is_replay_guarded_request(&Frame::new(
            ChatOp::HistoryRecent,
            1,
            Some(1),
            FrameBody::Empty,
        )));
    }

    #[test]
    fn live_server_replays_part_without_duplicate_event_or_userlist() {
        let path = temp_store_path("part-replay");
        let store = OmenchatStore::open(&path).expect("store");
        let room_id = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("lobby")
            .room_id;
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let alice_link = [0x53; 16];
        let bob_link = [0x54; 16];
        for (link_id, identity, name) in [
            (alice_link, b"alice-part".as_slice(), "Alice"),
            (bob_link, b"bob-part".as_slice(), "Bob"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join frame"),
            })
            .expect("join room");
        }

        let frames_before_part = live.transport().frames.len();
        let request = encode_frame(&Frame::new(
            ChatOp::PartRoom,
            8,
            Some(room_id),
            FrameBody::Empty,
        ))
        .expect("part frame");
        for _ in 0..2 {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id: alice_link,
                context: OMENCHAT_LINK_CONTEXT,
                data: request.clone(),
            })
            .expect("part attempt");
        }

        let decoded = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_part)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            decoded
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == alice_link && frame.op == ChatOp::CommandResult && frame.seq == 8
                })
                .count(),
            2
        );
        assert_eq!(
            decoded
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == bob_link && frame.op == ChatOp::RoomEvent && frame.seq == 8
                })
                .count(),
            1
        );
        assert_eq!(
            decoded
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == bob_link && frame.op == ChatOp::UserListSnapshotInline
                })
                .count(),
            1
        );
        assert!(!live.link_rooms.contains_key(&alice_link));
        assert_eq!(live.stats().replayed_operations, 1);

        let reopened = OmenchatStore::open(&path).expect("reopen store");
        let leave_events = reopened
            .latest_events(room_id, 100)
            .expect("events")
            .into_iter()
            .filter(|event| {
                matches!(
                    &event.kind,
                    ServerRoomEventKind::System { body } if body == "Alice left #lobby"
                )
            })
            .count();
        assert_eq!(leave_events, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn live_server_replays_kick_without_duplicate_event_or_disconnect() {
        let path = temp_store_path("kick-replay");
        let store = OmenchatStore::open(&path).expect("store");
        let room_id = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("lobby")
            .room_id;
        let admin = store
            .ensure_user(b"admin-kick", "Admin", None)
            .expect("admin user");
        store
            .set_user_role_bits(admin.user_id, 1 << 2)
            .expect("admin role");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                rate_commands_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let admin_link = [0x55; 16];
        let bob_link = [0x56; 16];
        for (link_id, identity, name) in [
            (admin_link, b"admin-kick".as_slice(), "Admin"),
            (bob_link, b"bob-kick".as_slice(), "Bob"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join frame"),
            })
            .expect("join room");
        }

        let request = encode_frame(&Frame::new(
            ChatOp::Command,
            9,
            Some(room_id),
            FrameBody::Text("kick Bob".into()),
        ))
        .expect("kick frame");
        for _ in 0..2 {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id: admin_link,
                context: OMENCHAT_LINK_CONTEXT,
                data: request.clone(),
            })
            .expect("kick attempt");
        }

        assert_eq!(live.transport().closed_links, vec![bob_link]);
        assert_eq!(live.stats().replayed_operations, 1);
        assert_eq!(
            live.transport()
                .frames
                .iter()
                .filter(|captured| captured.link_id == admin_link)
                .filter_map(|captured| decode_frame(&captured.bytes).ok())
                .filter(|frame| frame.op == ChatOp::CommandResult && frame.seq == 9)
                .count(),
            2
        );
        let reopened = OmenchatStore::open(&path).expect("reopen store");
        let kick_events = reopened
            .latest_events(room_id, 100)
            .expect("events")
            .into_iter()
            .filter(|event| {
                matches!(
                    &event.kind,
                    ServerRoomEventKind::System { body } if body == "Admin kicked Bob"
                )
            })
            .count();
        assert_eq!(kick_events, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn live_server_replays_topic_and_role_without_duplicate_mutations() {
        let path = temp_store_path("command-replay");
        let store = OmenchatStore::open(&path).expect("store");
        let room = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("lobby");
        let initial_revision = room.room_revision;
        let admin = store
            .ensure_user(b"admin-command", "Admin", None)
            .expect("admin user");
        store
            .set_user_role_bits(admin.user_id, 1 << 2)
            .expect("admin role");
        store
            .ensure_user(b"bob-command", "Bob", None)
            .expect("bob user");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let admin_link = [0x59; 16];
        let bob_link = [0x5a; 16];
        for (link_id, identity, name) in [
            (admin_link, b"admin-command".as_slice(), "Admin"),
            (bob_link, b"bob-command".as_slice(), "Bob"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join frame"),
            })
            .expect("join room");
        }

        let frames_before_commands = live.transport().frames.len();
        for (seq, command) in [(10, "topic Hardened topic"), (11, "role Bob trusted")] {
            let request = encode_frame(&Frame::new(
                ChatOp::Command,
                seq,
                Some(room.room_id),
                FrameBody::Text(command.into()),
            ))
            .expect("command frame");
            for _ in 0..2 {
                live.handle_event(OmenchatLinkEvent::LinkData {
                    link_id: admin_link,
                    context: OMENCHAT_LINK_CONTEXT,
                    data: request.clone(),
                })
                .expect("command attempt");
            }
        }

        let decoded = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_commands)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        for seq in [10, 11] {
            assert_eq!(
                decoded
                    .iter()
                    .filter(|(link_id, frame)| {
                        *link_id == admin_link
                            && frame.op == ChatOp::CommandResult
                            && frame.seq == seq
                    })
                    .count(),
                2
            );
        }
        assert_eq!(
            decoded
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == bob_link && frame.op == ChatOp::RoomDelta && frame.seq == 10
                })
                .count(),
            1
        );
        assert_eq!(
            decoded
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == bob_link && frame.op == ChatOp::UserDelta && frame.seq == 11
                })
                .count(),
            1
        );
        assert_eq!(live.stats().replayed_operations, 2);

        let reopened = OmenchatStore::open(&path).expect("reopen store");
        let updated_room = reopened
            .room_by_id(room.room_id)
            .expect("room query")
            .expect("room");
        assert_eq!(updated_room.topic.as_deref(), Some("Hardened topic"));
        assert_eq!(updated_room.room_revision, initial_revision + 1);
        let bob = reopened
            .users()
            .expect("users")
            .into_iter()
            .find(|user| user.display_name == "Bob")
            .expect("bob");
        assert_eq!(bob.role_bits, 1);
        let role_events = reopened
            .latest_events(room.room_id, 100)
            .expect("events")
            .into_iter()
            .filter(|event| {
                matches!(
                    &event.kind,
                    ServerRoomEventKind::System { body }
                        if body == "Admin set Bob role to trusted"
                )
            })
            .count();
        assert_eq!(role_events, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rate_limited_kick_does_not_disconnect_target() {
        let store = OmenchatStore::in_memory().expect("store");
        let admin = store
            .ensure_user(b"admin-rate", "Admin", None)
            .expect("admin user");
        store
            .set_user_role_bits(admin.user_id, 1 << 2)
            .expect("admin role");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                rate_commands_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let admin_link = [0x57; 16];
        let bob_link = [0x58; 16];
        for (link_id, identity, name) in [
            (admin_link, b"admin-rate".as_slice(), "Admin"),
            (bob_link, b"bob-rate".as_slice(), "Bob"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join frame"),
            })
            .expect("join room");
        }
        for (seq, command) in [(5, "rooms"), (6, "kick Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id: admin_link,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::Command,
                    seq,
                    Some(1),
                    FrameBody::Text(command.into()),
                ))
                .expect("command frame"),
            })
            .expect("command");
        }

        assert!(live.peers.contains_key(&bob_link));
        assert!(live.transport().closed_links.is_empty());
        assert!(live.transport().frames.iter().any(|captured| {
            captured.link_id == admin_link
                && decode_frame(&captured.bytes)
                    .map(|frame| frame.op == ChatOp::Error && frame.seq == 6)
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn live_server_replays_message_ack_without_duplicate_event_or_fanout() {
        let path = temp_store_path("message-replay");
        let store = OmenchatStore::open(&path).expect("store");
        let room_id = store
            .ensure_room("lobby", Some("Default OMENchat lobby"))
            .expect("lobby")
            .room_id;
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                rate_messages_per_minute: 1,
                ..SessionLimits::default()
            },
        );
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let alice_link = [0x51; 16];
        let bob_link = [0x52; 16];
        for (link_id, identity, name) in [
            (alice_link, b"alice-replay".as_slice(), "Alice"),
            (bob_link, b"bob-replay".as_slice(), "Bob"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join frame"),
            })
            .expect("join room");
        }

        let request = encode_frame(&Frame::new(
            ChatOp::RoomMessage,
            7,
            Some(room_id),
            FrameBody::Text("one logical message".into()),
        ))
        .expect("message frame");
        for _ in 0..2 {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id: alice_link,
                context: OMENCHAT_LINK_CONTEXT,
                data: request.clone(),
            })
            .expect("message attempt");
        }

        let alice_acks = live
            .transport()
            .frames
            .iter()
            .filter(|captured| captured.link_id == alice_link)
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .filter(|frame| frame.op == ChatOp::MessageAck && frame.seq == 7)
            .collect::<Vec<_>>();
        let bob_events = live
            .transport()
            .frames
            .iter()
            .filter(|captured| captured.link_id == bob_link)
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .filter(|frame| frame.op == ChatOp::RoomEvent && frame.seq == 7)
            .collect::<Vec<_>>();
        assert_eq!(
            alice_acks.len(),
            2,
            "alice frames: {:?}",
            live.transport()
                .frames
                .iter()
                .filter(|captured| captured.link_id == alice_link)
                .filter_map(|captured| decode_frame(&captured.bytes).ok())
                .map(|frame| (frame.op, frame.seq, frame.body))
                .collect::<Vec<_>>()
        );
        assert_eq!(alice_acks[0].body, alice_acks[1].body);
        assert_eq!(bob_events.len(), 1);
        assert_eq!(live.stats().replayed_operations, 1);
        assert_eq!(live.stats().replay_cache_items, 1);

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: alice_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::RoomMessage,
                7,
                Some(room_id),
                FrameBody::Text("different content".into()),
            ))
            .expect("collision frame"),
        })
        .expect("collision response");
        let collision = live
            .transport()
            .frames
            .iter()
            .rev()
            .find(|captured| captured.link_id == alice_link)
            .and_then(|captured| decode_frame(&captured.bytes).ok())
            .expect("collision response frame");
        assert_eq!(collision.op, ChatOp::Error);
        assert_eq!(collision.seq, 7);
        assert_eq!(live.stats().replay_collisions, 1);

        let reopened = OmenchatStore::open(&path).expect("reopen store");
        let message_events = reopened
            .latest_events(room_id, 100)
            .expect("events")
            .into_iter()
            .filter(|event| {
                matches!(
                    &event.kind,
                    ServerRoomEventKind::Message { body } if body == "one logical message"
                )
            })
            .count();
        assert_eq!(message_events, 1);

        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: alice_link,
            reason: Some("test close".into()),
        })
        .expect("close link");
        assert_eq!(live.stats().replay_cache_items, 0);
        assert_eq!(live.stats().replay_cache_bytes, 0);
        let _ = std::fs::remove_file(path);
    }

    fn user_names_from_values(values: Vec<FrameValue>) -> Vec<String> {
        values
            .iter()
            .filter_map(|value| match value {
                FrameValue::Array(fields) => fields.get(1),
                _ => None,
            })
            .filter_map(|value| match value {
                FrameValue::String(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn live_server_routes_known_link_data_and_columba_context_zero_frames() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [7u8; 16];
        let request = encode_frame(&Frame::new(
            ChatOp::JoinRoom,
            1,
            None,
            FrameBody::Text("lobby".into()),
        ))
        .expect("encode request");
        let request_len = request.len() as u64;
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: request.clone(),
        })
        .expect("unknown link ignored");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: 0,
            data: request.clone(),
        })
        .expect("columba context zero frame routed");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: request,
        })
        .expect("route request");

        assert_eq!(live.stats().active_links, 1);
        assert_eq!(live.stats().links_opened, 1);
        assert_eq!(live.stats().unknown_link_packets, 1);
        assert_eq!(live.stats().ignored_packets, 0);
        assert_eq!(live.stats().frames_in, 2);
        assert_eq!(live.stats().frames_out, 4);
        assert_eq!(live.stats().room_navigation_in, 2);
        assert_eq!(live.stats().traffic_in_frames(), 2);
        assert!(live.stats().bytes_in >= request_len);
        assert!(live.stats().bytes_out > 0);
        assert_eq!(live.transport().frames.len(), 4);
        assert!(live.stats().summary_line().contains("traffic_in="));
        assert!(live.stats().summary_line().contains("room:2"));
        assert!(live
            .transport()
            .frames
            .iter()
            .all(|frame| frame.link_id == link_id));
        assert!(live
            .transport()
            .frames
            .iter()
            .all(|frame| frame.context == OMENCHAT_LINK_CONTEXT));
    }

    #[test]
    fn live_server_recovers_unknown_link_from_session_open() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [9u8; 16];
        let session_open = encode_frame(&Frame::new(
            ChatOp::SessionOpen,
            1,
            None,
            FrameBody::Fields(vec![
                FrameValue::String("omenchat/0.1".into()),
                FrameValue::String("Clean Client".into()),
                FrameValue::String("0b09688ccb50c3ca949399fed7108f7f".into()),
            ]),
        ))
        .expect("encode session open");
        let join = encode_frame(&Frame::new(
            ChatOp::JoinRoom,
            2,
            None,
            FrameBody::Text("lobby".into()),
        ))
        .expect("encode join");
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: 0,
            data: session_open,
        })
        .expect("session open recovers unknown link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: 0,
            data: join,
        })
        .expect("join routes after recovered session");

        assert_eq!(live.stats().active_links, 1);
        assert_eq!(live.stats().links_opened, 1);
        assert_eq!(live.stats().unknown_link_packets, 0);
        assert_eq!(live.stats().frames_in, 2);
        assert!(live
            .active_link_summaries()
            .iter()
            .any(|link| link.display_name == "Clean Client"));
        assert!(live.transport().frames.iter().any(|captured| {
            captured.link_id == link_id
                && captured.context == OMENCHAT_LINK_CONTEXT
                && decode_frame(&captured.bytes)
                    .map(|frame| frame.op == ChatOp::SessionAccept)
                    .unwrap_or(false)
        }));
        assert!(live.transport().frames.iter().any(|captured| {
            captured.link_id == link_id
                && captured.context == OMENCHAT_LINK_CONTEXT
                && decode_frame(&captured.bytes)
                    .map(|frame| frame.op == ChatOp::JoinAccept)
                    .unwrap_or(false)
        }));
    }

    #[test]
    fn malformed_negotiation_does_not_complete_handshake_and_valid_retry_can_recover() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [10u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open identified link");

        let malformed = FrameBody::Fields(vec![
            FrameValue::String(crate::protocol::PROTOCOL_NAME.into()),
            FrameValue::String("Rejected Metadata".into()),
            FrameValue::Nil,
            FrameValue::Array(vec![FrameValue::String(
                crate::protocol::DURABLE_MUTATION_CAPABILITY.into(),
            )]),
            FrameValue::Bytes(vec![7; 15]),
        ]);
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::SessionOpen, 1, None, malformed))
                .expect("malformed session frame"),
        })
        .expect("malformed session response");
        assert!(!live.handshake_complete(link_id));
        assert_eq!(live.pending_handshake_count(), 1);
        assert_eq!(
            live.peers
                .get(&link_id)
                .map(|peer| peer.display_name.as_str()),
            Some("Live Peer"),
            "rejected session metadata must not replace the authenticated peer"
        );
        assert!(live.transport().frames.iter().any(|captured| {
            decode_frame(&captured.bytes).is_ok_and(|frame| {
                frame.op == ChatOp::Error
                    && matches!(
                        frame.body,
                        FrameBody::Fields(ref fields)
                            if fields.first()
                                == Some(&FrameValue::U64(
                                    ChatErrorCode::DurableMutationMalformed as u16 as u64
                                ))
                    )
            })
        }));

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::SessionOpen,
                2,
                None,
                FrameBody::Text("Accepted Metadata".into()),
            ))
            .expect("valid session frame"),
        })
        .expect("valid session retry");
        assert!(live.handshake_complete(link_id));
        assert_eq!(live.pending_handshake_count(), 0);
        assert_eq!(
            live.peers
                .get(&link_id)
                .map(|peer| peer.display_name.as_str()),
            Some("Accepted Metadata")
        );
        assert!(live.transport().frames.iter().any(|captured| {
            decode_frame(&captured.bytes)
                .is_ok_and(|frame| frame.op == ChatOp::SessionAccept && frame.seq == 2)
        }));
    }

    #[test]
    fn accepted_durable_request_binds_client_instance_to_identified_link() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [18u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open identified link");
        let client_instance_id = ClientInstanceId::new([3; 16]);
        let body = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Durable Candidate".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
                client_instance_id: Some(client_instance_id),
            },
        )
        .expect("durable request");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::SessionOpen, 1, None, body))
                .expect("session frame"),
        })
        .expect("session response");

        assert!(live.handshake_complete(link_id));
        let binding = live
            .durable_sessions
            .get(&link_id)
            .expect("accepted durable session binding");
        assert_eq!(binding.client_instance_id, client_instance_id);
        assert_eq!(binding.identity_hash, peer().identity_hash);
        assert_eq!(
            live.peers
                .get(&link_id)
                .map(|peer| peer.display_name.as_str()),
            Some("Durable Candidate")
        );
    }

    #[test]
    fn dormant_reply_binding_exposes_local_user_and_routes_rich_mutation() {
        let path = temp_store_path("rich-binding");
        let store = OmenchatStore::open(&path).expect("store");
        store.ensure_room("lobby", None).expect("lobby");
        let engine = SessionEngine::new(store);
        let link_id = [0x52; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open identified link");
        let client_instance_id = ClientInstanceId::new([0x52; 16]);
        let session_body = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Rich Client".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![
                    DURABLE_MUTATION_CAPABILITY.into(),
                    REPLY_MENTIONS_CAPABILITY.into(),
                ],
                client_instance_id: Some(client_instance_id),
            },
        )
        .expect("rich capability request");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::SessionOpen, 1, None, session_body))
                .expect("session frame"),
        })
        .expect("session response");
        let binding = live
            .durable_sessions
            .get_mut(&link_id)
            .expect("base durable binding");
        assert!(
            !binding.reply_mentions,
            "production negotiation must remain dormant"
        );
        binding.reply_mentions = true;

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                2,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join frame"),
        })
        .expect("join response");
        let join = live
            .transport()
            .frames
            .iter()
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .find(|frame| frame.op == ChatOp::JoinAccept && frame.seq == 2)
            .expect("join accept");
        let FrameBody::Fields(join_fields) = join.body else {
            panic!("join fields");
        };
        let local_user_id = match join_fields.get(1) {
            Some(FrameValue::U64(user_id)) => *user_id as crate::protocol::UserId,
            other => panic!("missing negotiated local user id: {other:?}"),
        };

        let rich_body = crate::protocol::RichMessageBody {
            body: "hello myself".into(),
            reply_to: None,
            mentioned_user_ids: vec![local_user_id],
        }
        .into_frame_body()
        .expect("rich body");
        let request_hash = crate::protocol::canonical_mutation_request_hash(
            ChatOp::RoomMessage,
            Some(1),
            &rich_body,
        )
        .expect("rich hash");
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([0x53; 16]),
            request_hash,
            body: rich_body,
        }
        .into_frame_body()
        .expect("durable envelope");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::RoomMessage, 3, Some(1), envelope))
                .expect("rich frame"),
        })
        .expect("rich response");
        assert!(live.transport().frames.iter().any(|captured| {
            decode_frame(&captured.bytes)
                .is_ok_and(|frame| frame.op == ChatOp::MessageAck && frame.seq == 3)
        }));
        drop(live);
        let store = OmenchatStore::open(&path).expect("reopen rich store");
        assert_eq!(
            store
                .latest_events(1, 10)
                .expect("rich stored event")
                .last()
                .and_then(|event| event.metadata.clone()),
            Some(crate::protocol::RichMessageEventMetadata {
                reply_to_event_id: None,
                mentioned_user_ids: vec![local_user_id],
            })
        );
        drop(store);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn durable_binding_requires_explicit_acceptance_and_is_link_scoped() {
        let client_instance_id = ClientInstanceId::new([4; 16]);
        let request_body = crate::protocol::with_session_open_negotiation(
            FrameBody::Text("Bound Client".into()),
            &crate::protocol::SessionOpenNegotiation {
                requested_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
                client_instance_id: Some(client_instance_id),
            },
        )
        .expect("request negotiation");
        let request = Frame::new(ChatOp::SessionOpen, 1, None, request_body);
        let requested = requested_durable_client_instance(&request);
        assert_eq!(requested, Some(client_instance_id));

        let legacy_accept = Frame::new(
            ChatOp::SessionAccept,
            1,
            None,
            FrameBody::Fields(vec![FrameValue::String(
                crate::protocol::PROTOCOL_NAME.into(),
            )]),
        );
        assert_eq!(
            accepted_durable_client_instance(requested, &[legacy_accept]),
            None
        );

        let negotiated_accept = crate::protocol::with_session_accept_negotiation(
            FrameBody::Fields(vec![FrameValue::String(
                crate::protocol::PROTOCOL_NAME.into(),
            )]),
            &crate::protocol::SessionAcceptNegotiation {
                accepted_capabilities: vec![DURABLE_MUTATION_CAPABILITY.into()],
            },
        )
        .expect("accept negotiation");
        let accepted = Frame::new(ChatOp::SessionAccept, 1, None, negotiated_accept);
        assert_eq!(
            accepted_durable_client_instance(requested, &[accepted]),
            Some(client_instance_id)
        );
    }

    #[test]
    fn durable_binding_is_cleared_on_identity_change_and_link_close() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [19u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open identified link");
        live.durable_sessions.insert(
            link_id,
            DurableSessionBinding::without_notice_ack(
                b"peer-live".to_vec(),
                ClientInstanceId::new([5; 16]),
            ),
        );

        live.handle_event(OmenchatLinkEvent::PeerIdentified {
            link_id,
            identity_hash: [6; 16],
        })
        .expect("replace authenticated identity");
        assert!(live.durable_sessions.is_empty());

        live.durable_sessions.insert(
            link_id,
            DurableSessionBinding::without_notice_ack(vec![6; 16], ClientInstanceId::new([7; 16])),
        );
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id,
            reason: Some("test close".into()),
        })
        .expect("close link");
        assert!(live.durable_sessions.is_empty());
    }

    #[test]
    fn live_server_ignores_context_zero_non_frames() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [7u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: 0,
            data: b"not an omenchat frame".to_vec(),
        })
        .expect("non-frame ignored");

        assert_eq!(live.stats().ignored_packets, 1);
        assert_eq!(live.stats().frames_in, 0);
        assert_eq!(live.transport().frames.len(), 0);
    }

    #[test]
    fn live_server_resource_metadata_requires_omenchat_prefix() {
        let valid = crate::transport::resource_metadata("upload:test.png");

        assert_eq!(
            resource_id_from_metadata(Some(&valid)).as_deref(),
            Some("upload:test.png")
        );
        assert_eq!(resource_id_from_metadata(Some(b"omenchat-frame:1")), None);
        assert_eq!(
            resource_id_from_metadata(Some(b"other:upload:test.png")),
            None
        );
        assert_eq!(resource_id_from_metadata(Some(b"omenchat-resource:")), None);
        assert_eq!(resource_id_from_metadata(None), None);
    }

    #[test]
    fn live_server_offers_resources_for_large_batches() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store
            .ensure_user(b"seed", "Seed", Some("lxmf-seed"))
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "large live payload".repeat(64),
                },
            )
            .expect("append event");
        let engine = SessionEngine::with_limits(
            store,
            SessionLimits {
                history_batch_size: 10,
                join_backlog_events: 10,
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );
        let link_id = [8u8; 16];
        let request = encode_frame(&Frame::new(
            ChatOp::JoinRoom,
            1,
            None,
            FrameBody::Text("lobby".into()),
        ))
        .expect("encode request");
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: request,
        })
        .expect("route request");

        assert_eq!(live.stats().resources_offered, 2);
        assert_eq!(live.transport().resources.len(), 2);
        assert!(live
            .transport()
            .resources
            .iter()
            .all(|resource| resource.link_id == link_id));
        assert_eq!(
            live.engine
                .pending_resource_metrics()
                .expect("pending resource metrics"),
            (0, 0, 0)
        );
        let stats = live.stats();
        assert_eq!(stats.pending_resource_items, 0);
        assert_eq!(stats.pending_resource_bytes, 0);
        assert_eq!(stats.pending_resource_rejected, 0);
        assert!(stats
            .summary_line()
            .contains("pending_resources=items:0 bytes:0 rejected:0"));
    }

    #[test]
    fn live_server_userlist_reports_active_room_peers_not_historical_members() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let stale = store
            .ensure_user(b"stale", "Stale", None)
            .expect("stale user");
        store
            .join_room(room.room_id, stale.user_id)
            .expect("stale join");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [11u8; 16];
        let link_b = [12u8; 16];

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_a,
            peer: ServerPeer {
                identity_hash: b"alice-id".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open alice");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join alice"),
        })
        .expect("alice join");

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_b,
            peer: ServerPeer {
                identity_hash: b"bob-id".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: None,
            },
        })
        .expect("open bob");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_b,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                2,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join bob"),
        })
        .expect("bob join");

        let userlist_frame = live
            .transport()
            .frames
            .iter()
            .rev()
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .find(|frame| frame.op == ChatOp::UserListSnapshotInline)
            .expect("userlist frame");
        let user_values = decode_compressed_values_body(&userlist_frame.body).expect("userlist");
        let names = user_values
            .iter()
            .filter_map(|value| match value {
                FrameValue::Array(fields) => fields.get(1),
                _ => None,
            })
            .filter_map(|value| match value {
                FrameValue::String(name) => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["Alice", "Bob"]);
    }

    #[test]
    fn live_server_broadcasts_userlist_when_peer_joins_room() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [19u8; 16];
        let link_b = [20u8; 16];

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_a,
            peer: ServerPeer {
                identity_hash: b"alice-id".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open alice");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join alice"),
        })
        .expect("alice join");

        let frames_before_bob = live.transport().frames.len();
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_b,
            peer: ServerPeer {
                identity_hash: b"bob-id".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: None,
            },
        })
        .expect("open bob");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_b,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                2,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join bob"),
        })
        .expect("bob join");

        let alice_userlist =
            live.transport()
                .frames
                .iter()
                .skip(frames_before_bob)
                .find(|captured| {
                    captured.link_id == link_a
                        && decode_frame(&captured.bytes)
                            .map(|frame| frame.op == ChatOp::UserListSnapshotInline)
                            .unwrap_or(false)
                });

        assert!(alice_userlist.is_some());
    }

    #[test]
    fn live_server_retains_resource_payload_through_userlist_fanout_then_releases_it() {
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                join_backlog_events: 0,
                large_batch_threshold_bytes: 1,
                ..SessionLimits::default()
            },
        );
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [21u8; 16];
        let link_b = [22u8; 16];

        for (link_id, identity, display_name, seq) in [
            (link_a, b"alice-id".as_slice(), "Alice", 1),
            (link_b, b"bob-id".as_slice(), "Bob", 2),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: display_name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            if link_id == link_b {
                live.transport_mut().resources.clear();
            }
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    seq,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join room"),
            })
            .expect("join handled");
        }

        let userlist_resources = live
            .transport()
            .resources
            .iter()
            .filter(|resource| resource.resource_id.starts_with("userlist:"))
            .collect::<Vec<_>>();
        assert_eq!(userlist_resources.len(), 2);
        assert_eq!(
            userlist_resources[0].resource_id,
            userlist_resources[1].resource_id
        );
        assert_eq!(userlist_resources[0].payload, userlist_resources[1].payload);
        assert!(userlist_resources
            .iter()
            .any(|resource| resource.link_id == link_a));
        assert!(userlist_resources
            .iter()
            .any(|resource| resource.link_id == link_b));
        assert_eq!(
            live.engine
                .pending_resource_metrics()
                .expect("pending resource metrics"),
            (0, 0, 0)
        );
    }

    #[test]
    fn live_server_broadcasts_userlist_when_peer_leaves_room() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [23u8; 16];
        let link_b = [24u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }

        let frames_before_close = live.transport().frames.len();
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: link_b,
            reason: Some("test".into()),
        })
        .expect("close bob");

        let names = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_close)
            .find(|captured| captured.link_id == link_a)
            .and_then(|captured| decode_frame(&captured.bytes).ok())
            .filter(|frame| frame.op == ChatOp::UserListSnapshotInline)
            .and_then(|frame| decode_compressed_values_body(&frame.body).ok())
            .map(user_names_from_values)
            .unwrap_or_default();

        assert_eq!(names, vec!["Alice"]);
    }

    #[test]
    fn live_server_part_room_removes_link_from_room_userlist() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [25u8; 16];
        let link_b = [26u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }

        let frames_before_part = live.transport().frames.len();
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_b,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::PartRoom, 2, Some(1), FrameBody::Empty))
                .expect("part"),
        })
        .expect("part room");

        let names = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_part)
            .filter(|captured| captured.link_id == link_a)
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .find(|frame| frame.op == ChatOp::UserListSnapshotInline)
            .and_then(|frame| decode_compressed_values_body(&frame.body).ok())
            .map(user_names_from_values)
            .unwrap_or_default();

        assert_eq!(names, vec!["Alice"]);
    }

    #[test]
    fn live_server_broadcasts_room_events_to_other_room_links() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [21u8; 16];
        let link_b = [22u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::RoomMessage,
                3,
                Some(1),
                FrameBody::Text("hello everyone".into()),
            ))
            .expect("message"),
        })
        .expect("send message");

        let room_event_links = live
            .transport()
            .frames
            .iter()
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .filter(|frame| frame.op == ChatOp::RoomEvent)
                    .map(|_| captured.link_id)
            })
            .collect::<Vec<_>>();
        let ack_links = live
            .transport()
            .frames
            .iter()
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .filter(|frame| frame.op == ChatOp::MessageAck && frame.seq == 3)
                    .map(|_| captured.link_id)
            })
            .collect::<Vec<_>>();

        assert!(ack_links.contains(&link_a));
        assert!(!ack_links.contains(&link_b));
        assert!(!room_event_links.contains(&link_a));
        assert!(room_event_links.contains(&link_b));
    }

    #[test]
    fn authenticated_durable_binding_routes_once_and_replays_original_ack() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [23u8; 16];
        let link_b = [24u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }

        let client_instance_id = ClientInstanceId::new([31; 16]);
        live.durable_sessions.insert(
            link_a,
            DurableSessionBinding::without_notice_ack(b"Alice".to_vec(), client_instance_id),
        );
        let body = FrameBody::Text("durable hello".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([32; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::RoomMessage,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().frames.len();

        for seq in [3, 4] {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id: link_a,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::RoomMessage,
                    seq,
                    Some(1),
                    envelope.clone(),
                ))
                .expect("message"),
            })
            .expect("send durable message");
        }

        let routed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let origin_acks = routed
            .iter()
            .filter(|(link_id, frame)| *link_id == link_a && frame.op == ChatOp::MessageAck)
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        let observer_events = routed
            .iter()
            .filter(|(link_id, frame)| *link_id == link_b && frame.op == ChatOp::RoomEvent)
            .count();

        assert_eq!(origin_acks.len(), 2);
        assert_replayed_response(origin_acks[1], origin_acks[0], 4);
        assert_eq!(origin_acks[0].seq, 3);
        assert_eq!(observer_events, 1);
        assert_eq!(live.replay_cache.entries.len(), 0);
    }

    #[test]
    fn durable_room_action_replay_on_replacement_link_uses_new_sequence_without_refanout() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let first_link = [25u8; 16];
        let observer_link = [26u8; 16];
        let replacement_link = [27u8; 16];

        for (link_id, name) in [(first_link, "Alice"), (observer_link, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }

        let client_instance_id = ClientInstanceId::new([33; 16]);
        live.durable_sessions.insert(
            first_link,
            DurableSessionBinding::without_notice_ack(b"Alice".to_vec(), client_instance_id),
        );
        let body = FrameBody::Text("waves after replacement".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([34; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::RoomAction,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: first_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::RoomAction,
                3,
                Some(1),
                envelope.clone(),
            ))
            .expect("first message"),
        })
        .expect("commit first message");
        let original_ack = live
            .transport()
            .frames
            .iter()
            .filter(|captured| captured.link_id == first_link)
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .find(|frame| frame.op == ChatOp::MessageAck && frame.seq == 3)
            .expect("original acknowledgement");

        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: first_link,
            reason: Some("response path lost".into()),
        })
        .expect("close first link");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_link,
            peer: ServerPeer {
                identity_hash: b"Alice".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("replacement join"),
        })
        .expect("join replacement room");
        live.durable_sessions.insert(
            replacement_link,
            DurableSessionBinding::without_notice_ack(b"Alice".to_vec(), client_instance_id),
        );
        let frames_before_replay = live.transport().frames.len();

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::RoomAction, 7, Some(1), envelope))
                .expect("replacement replay"),
        })
        .expect("replay on replacement link");

        let replayed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_replay)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let replacement_ack = replayed
            .iter()
            .find(|(link_id, frame)| *link_id == replacement_link && frame.op == ChatOp::MessageAck)
            .map(|(_, frame)| frame)
            .expect("replacement acknowledgement");
        assert_replayed_response(replacement_ack, &original_ack, 7);
        assert!(!replayed
            .iter()
            .any(|(link_id, frame)| *link_id == observer_link && frame.op == ChatOp::RoomEvent));
    }

    #[test]
    fn durable_notice_replays_to_origin_without_second_room_fanout() {
        let store = OmenchatStore::in_memory().expect("store");
        let moderator = store
            .ensure_user(b"Alice", "Alice", None)
            .expect("moderator");
        store
            .set_user_role_bits(moderator.user_id, 1 << 1)
            .expect("moderator role");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [30u8; 16];
        let link_b = [31u8; 16];
        let replacement_link = [32u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }
        live.durable_sessions.insert(
            link_a,
            DurableSessionBinding::with_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([38; 16]),
            ),
        );
        let body = FrameBody::Text("maintenance soon".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([39; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::RoomNotice,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().frames.len();

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::RoomNotice,
                9,
                Some(1),
                envelope.clone(),
            ))
            .expect("notice"),
        })
        .expect("durable notice");
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: link_a,
            reason: Some("notice acknowledgement lost".into()),
        })
        .expect("close first notice link");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_link,
            peer: ServerPeer {
                identity_hash: b"Alice".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement notice link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("replacement join"),
        })
        .expect("join replacement notice room");
        live.durable_sessions.insert(
            replacement_link,
            DurableSessionBinding::with_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([38; 16]),
            ),
        );
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::RoomNotice, 10, Some(1), envelope))
                .expect("replacement notice"),
        })
        .expect("replay durable notice");

        let routed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let origin_acks = routed
            .iter()
            .filter(|(link_id, frame)| {
                (*link_id == link_a || *link_id == replacement_link)
                    && frame.op == ChatOp::MessageAck
            })
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        assert_eq!(origin_acks.len(), 2);
        assert_replayed_response(origin_acks[1], origin_acks[0], 10);
        assert_eq!(origin_acks[0].seq, 9);
        assert!(matches!(
            &origin_acks[0].body,
            FrameBody::Fields(fields) if fields.get(1) == Some(&FrameValue::U64(3))
        ));
        assert_eq!(
            routed
                .iter()
                .filter(|(link_id, frame)| { *link_id == link_b && frame.op == ChatOp::RoomEvent })
                .count(),
            1
        );
        assert!(live.replay_cache.entries.is_empty());
    }

    #[test]
    fn durable_topic_replays_to_origin_without_second_room_delta() {
        let store = OmenchatStore::in_memory().expect("store");
        let moderator = store
            .ensure_user(b"Alice", "Alice", None)
            .expect("moderator");
        store
            .set_user_role_bits(moderator.user_id, 1 << 1)
            .expect("moderator role");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [32u8; 16];
        let link_b = [33u8; 16];
        let replacement_link = [34u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }
        live.durable_sessions.insert(
            link_a,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([40; 16]),
            ),
        );
        let body = FrameBody::Text("topic Durable lobby".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([41; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::Command,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().frames.len();

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 11, Some(1), envelope.clone()))
                .expect("topic"),
        })
        .expect("durable topic");
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: link_a,
            reason: Some("topic result lost".into()),
        })
        .expect("close first topic link");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_link,
            peer: ServerPeer {
                identity_hash: b"Alice".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement topic link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("replacement join"),
        })
        .expect("join replacement topic room");
        live.durable_sessions.insert(
            replacement_link,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([40; 16]),
            ),
        );
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 12, Some(1), envelope))
                .expect("replacement topic"),
        })
        .expect("replay durable topic");

        let routed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let origin_results = routed
            .iter()
            .filter(|(link_id, frame)| {
                (*link_id == link_a || *link_id == replacement_link)
                    && command_result_name(frame) == Some("topic")
            })
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        assert_eq!(origin_results.len(), 2);
        assert_replayed_response(origin_results[1], origin_results[0], 12);
        assert_eq!(origin_results[0].seq, 11);
        assert_eq!(
            routed
                .iter()
                .filter(|(link_id, frame)| { *link_id == link_b && frame.op == ChatOp::RoomDelta })
                .count(),
            1
        );
        assert!(live.replay_cache.entries.is_empty());
    }

    #[test]
    fn durable_create_replays_on_replacement_link_without_second_room_delta() {
        let store = OmenchatStore::in_memory().expect("store");
        let administrator = store
            .ensure_user(b"Alice", "Alice", None)
            .expect("administrator");
        store
            .set_user_role_bits(administrator.user_id, 1 << 2)
            .expect("administrator role");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let original_link = [44u8; 16];
        let observer_link = [45u8; 16];
        let replacement_link = [46u8; 16];

        for (link_id, name) in [(original_link, "Alice"), (observer_link, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }
        let client_instance_id = ClientInstanceId::new([47; 16]);
        live.durable_sessions.insert(
            original_link,
            DurableSessionBinding::without_notice_ack(b"Alice".to_vec(), client_instance_id),
        );
        let body = FrameBody::Text("create operations Operations room".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([48; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::Command,
                None,
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().frames.len();

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: original_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 21, None, envelope.clone()))
                .expect("create"),
        })
        .expect("durable create");
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: original_link,
            reason: Some("create result lost".into()),
        })
        .expect("close original link");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_link,
            peer: ServerPeer {
                identity_hash: b"Alice".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("replacement join"),
        })
        .expect("join replacement room");
        live.durable_sessions.insert(
            replacement_link,
            DurableSessionBinding::without_notice_ack(b"Alice".to_vec(), client_instance_id),
        );
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 22, None, envelope))
                .expect("replacement create"),
        })
        .expect("replay durable create");

        let routed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let origin_results = routed
            .iter()
            .filter(|(link_id, frame)| {
                (*link_id == original_link || *link_id == replacement_link)
                    && command_result_name(frame) == Some("create")
            })
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        assert_eq!(origin_results.len(), 2);
        assert_replayed_response(origin_results[1], origin_results[0], 22);
        assert_eq!(origin_results[0].seq, 21);
        assert_eq!(
            routed
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == observer_link && frame.op == ChatOp::RoomDelta
                })
                .count(),
            1
        );
        assert!(live.replay_cache.entries.is_empty());
    }

    #[test]
    fn durable_role_and_unban_replay_on_replacement_links_without_repeating_effects() {
        let store = OmenchatStore::in_memory().expect("store");
        let administrator = store
            .ensure_user(b"Alice", "Alice", None)
            .expect("administrator");
        store
            .set_user_role_bits(administrator.user_id, 1 << 2)
            .expect("administrator role");
        let banned = store
            .ensure_user(b"Charlie", "Charlie", None)
            .expect("banned target");
        store
            .set_user_status_flag(banned.user_id, 1, true)
            .expect("ban target");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [34u8; 16];
        let link_b = [35u8; 16];
        let replacement_link = [36u8; 16];
        let second_replacement_link = [37u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }
        live.durable_sessions.insert(
            link_a,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([42; 16]),
            ),
        );
        let body = FrameBody::Text("role Bob mod".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([43; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::Command,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().frames.len();

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 13, Some(1), envelope.clone()))
                .expect("role"),
        })
        .expect("durable role");
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: link_a,
            reason: Some("role result lost".into()),
        })
        .expect("close original role link");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_link,
            peer: ServerPeer {
                identity_hash: b"Alice".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement role link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("replacement join"),
        })
        .expect("join replacement role room");
        live.durable_sessions.insert(
            replacement_link,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([42; 16]),
            ),
        );
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 14, Some(1), envelope))
                .expect("replacement role"),
        })
        .expect("replay durable role");
        let unban_body = FrameBody::Text("unban Charlie".into());
        let unban_envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([44; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::Command,
                Some(1),
                &unban_body,
            )
            .expect("unban canonical hash"),
            body: unban_body,
        }
        .into_frame_body()
        .expect("durable unban envelope");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::Command,
                15,
                Some(1),
                unban_envelope.clone(),
            ))
            .expect("unban"),
        })
        .expect("durable unban");
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id: replacement_link,
            reason: Some("unban result lost".into()),
        })
        .expect("close first replacement link");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: second_replacement_link,
            peer: ServerPeer {
                identity_hash: b"Alice".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open second replacement link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: second_replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("second replacement join"),
        })
        .expect("join second replacement room");
        live.durable_sessions.insert(
            second_replacement_link,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([42; 16]),
            ),
        );
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: second_replacement_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 16, Some(1), unban_envelope))
                .expect("replacement unban"),
        })
        .expect("replay durable unban");

        let routed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let origin_results = routed
            .iter()
            .filter(|(link_id, frame)| {
                (*link_id == link_a || *link_id == replacement_link)
                    && command_result_name(frame) == Some("role")
            })
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        assert_eq!(origin_results.len(), 2);
        assert_replayed_response(origin_results[1], origin_results[0], 14);
        for op in [ChatOp::UserDelta, ChatOp::RoomEvent] {
            assert_eq!(
                routed
                    .iter()
                    .filter(|(link_id, frame)| *link_id == link_b && frame.op == op)
                    .count(),
                2
            );
        }
        let unban_results = routed
            .iter()
            .filter(|(link_id, frame)| {
                (*link_id == replacement_link || *link_id == second_replacement_link)
                    && command_result_name(frame) == Some("unban")
            })
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        assert_eq!(unban_results.len(), 2);
        assert_replayed_response(unban_results[1], unban_results[0], 16);
        assert!(live.replay_cache.entries.is_empty());
    }

    #[test]
    fn durable_kick_commit_survives_lost_response_without_disconnecting_replacement() {
        let store = OmenchatStore::in_memory().expect("store");
        let administrator = store
            .ensure_user(b"Admin", "Admin", None)
            .expect("administrator");
        store
            .set_user_role_bits(administrator.user_id, 1 << 2)
            .expect("administrator role");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, FailOnceTransport::default());
        let admin_link = [36u8; 16];
        let original_bob_link = [37u8; 16];
        let observer_link = [39u8; 16];

        for (link_id, name) in [
            (admin_link, "Admin"),
            (original_bob_link, "Bob"),
            (observer_link, "Charlie"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }
        live.durable_sessions.insert(
            admin_link,
            DurableSessionBinding::without_notice_ack(
                b"Admin".to_vec(),
                ClientInstanceId::new([44; 16]),
            ),
        );
        let body = FrameBody::Text("kick Bob".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([45; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::Command,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");

        live.transport_mut().reject_next_command_result = true;
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: admin_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 15, Some(1), envelope.clone()))
                .expect("kick"),
        })
        .expect("durable kick");
        assert_eq!(
            live.transport().captured.closed_links,
            vec![original_bob_link]
        );

        let replacement_bob_link = [38u8; 16];
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_bob_link,
            peer: ServerPeer {
                identity_hash: b"Bob".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: replacement_bob_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                2,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("replacement join"),
        })
        .expect("join replacement");

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: admin_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::Command, 16, Some(1), envelope))
                .expect("kick replay"),
        })
        .expect("replayed durable kick");
        assert_eq!(
            live.transport().captured.closed_links,
            vec![original_bob_link]
        );
        assert!(live.peers.contains_key(&replacement_bob_link));
        assert_eq!(
            live.transport()
                .captured
                .frames
                .iter()
                .filter(|captured| captured.link_id == admin_link)
                .filter_map(|captured| decode_frame(&captured.bytes).ok())
                .filter(|frame| { command_result_name(frame) == Some("kick") && frame.seq == 16 })
                .count(),
            1
        );
        assert_eq!(
            live.transport()
                .captured
                .frames
                .iter()
                .filter(|captured| captured.link_id == original_bob_link)
                .filter_map(|captured| decode_frame(&captured.bytes).ok())
                .filter(|frame| frame.op == ChatOp::RoomEvent)
                .count(),
            0
        );
        assert_eq!(
            live.transport()
                .captured
                .frames
                .iter()
                .filter(|captured| captured.link_id == observer_link)
                .filter_map(|captured| decode_frame(&captured.bytes).ok())
                .filter(|frame| frame.op == ChatOp::RoomEvent)
                .count(),
            0
        );
        assert_eq!(
            live.transport()
                .captured
                .frames
                .iter()
                .filter(|captured| captured.link_id == replacement_bob_link)
                .filter_map(|captured| decode_frame(&captured.bytes).ok())
                .filter(|frame| frame.op == ChatOp::RoomEvent)
                .count(),
            0
        );
        assert!(live.replay_cache.entries.is_empty());
    }

    #[test]
    fn durable_part_cleans_live_room_once_and_replays_without_fanout() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_a = [26u8; 16];
        let link_b = [27u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }

        live.durable_sessions.insert(
            link_a,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([34; 16]),
            ),
        );
        let body = FrameBody::Empty;
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([35; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::PartRoom,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().frames.len();

        for seq in [5, 6] {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id: link_a,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::PartRoom,
                    seq,
                    Some(1),
                    envelope.clone(),
                ))
                .expect("part"),
            })
            .expect("durable part");
        }

        let routed = live
            .transport()
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        let origin_results = routed
            .iter()
            .filter(|(link_id, frame)| *link_id == link_a && is_successful_part_response(frame))
            .map(|(_, frame)| frame)
            .collect::<Vec<_>>();
        let observer_events = routed
            .iter()
            .filter(|(link_id, frame)| *link_id == link_b && frame.op == ChatOp::RoomEvent)
            .count();
        let observer_userlists = routed
            .iter()
            .filter(|(link_id, frame)| {
                *link_id == link_b
                    && matches!(
                        frame.op,
                        ChatOp::UserListSnapshotInline | ChatOp::UserListSnapshotResource
                    )
            })
            .count();

        assert_eq!(origin_results.len(), 2);
        assert_replayed_response(origin_results[1], origin_results[0], 6);
        assert_eq!(origin_results[0].seq, 5);
        assert_eq!(observer_events, 1);
        assert_eq!(observer_userlists, 1);
        assert!(!live.link_rooms.contains_key(&link_a));
        assert_eq!(live.link_rooms.get(&link_b), Some(&1));
        assert!(live.replay_cache.entries.is_empty());
    }

    #[test]
    fn durable_part_replay_repairs_live_room_after_origin_delivery_failure() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, FailOnceTransport::default());
        let link_a = [28u8; 16];
        let link_b = [29u8; 16];

        for (link_id, name) in [(link_a, "Alice"), (link_b, "Bob")] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: name.as_bytes().to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::JoinRoom,
                    1,
                    None,
                    FrameBody::Text("lobby".into()),
                ))
                .expect("join"),
            })
            .expect("join room");
        }
        live.durable_sessions.insert(
            link_a,
            DurableSessionBinding::without_notice_ack(
                b"Alice".to_vec(),
                ClientInstanceId::new([36; 16]),
            ),
        );
        let body = FrameBody::Empty;
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([37; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::PartRoom,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        let frames_before = live.transport().captured.frames.len();

        live.transport_mut().reject_next_frame = true;
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::PartRoom, 7, Some(1), envelope.clone()))
                .expect("first part"),
        })
        .expect("commit despite delivery failure");
        assert_eq!(live.link_rooms.get(&link_a), Some(&1));

        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: link_a,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::PartRoom, 8, Some(1), envelope))
                .expect("replayed part"),
        })
        .expect("replay part");

        assert!(!live.link_rooms.contains_key(&link_a));
        let routed = live
            .transport()
            .captured
            .frames
            .iter()
            .skip(frames_before)
            .filter_map(|captured| {
                decode_frame(&captured.bytes)
                    .ok()
                    .map(|frame| (captured.link_id, frame))
            })
            .collect::<Vec<_>>();
        assert!(routed.iter().any(|(link_id, frame)| {
            *link_id == link_a && is_successful_part_response(frame) && frame.seq == 8
        }));
        assert_eq!(
            routed
                .iter()
                .filter(|(link_id, frame)| {
                    *link_id == link_b
                        && matches!(
                            frame.op,
                            ChatOp::UserListSnapshotInline | ChatOp::UserListSnapshotResource
                        )
                })
                .count(),
            1
        );
        assert!(!routed
            .iter()
            .any(|(link_id, frame)| *link_id == link_b && frame.op == ChatOp::RoomEvent));
    }

    #[test]
    fn durable_envelopes_fail_closed_without_breaking_legacy_room_messages() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let link_id = [25u8; 16];
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join"),
        })
        .expect("join room");

        let body = FrameBody::Text("not negotiated".into());
        let envelope = DurableMutationEnvelope {
            mutation_id: crate::protocol::MutationId::new([33; 16]),
            request_hash: crate::protocol::canonical_mutation_request_hash(
                ChatOp::RoomMessage,
                Some(1),
                &body,
            )
            .expect("canonical hash"),
            body,
        }
        .into_frame_body()
        .expect("durable envelope");
        for frame in [
            Frame::new(ChatOp::RoomMessage, 2, Some(1), envelope),
            Frame::new(
                ChatOp::RoomMessage,
                3,
                Some(1),
                FrameBody::Fields(vec![FrameValue::String(
                    DURABLE_MUTATION_ENVELOPE_TAG.into(),
                )]),
            ),
            Frame::new(
                ChatOp::RoomMessage,
                4,
                Some(1),
                FrameBody::Text("legacy still works".into()),
            ),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&frame).expect("frame"),
            })
            .expect("dispatch frame");
        }

        let responses = live
            .transport()
            .frames
            .iter()
            .filter(|captured| captured.link_id == link_id)
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .collect::<Vec<_>>();
        let error_code = |seq| {
            responses
                .iter()
                .find(|frame| frame.op == ChatOp::Error && frame.seq == seq)
                .and_then(|frame| match &frame.body {
                    FrameBody::Fields(fields) => fields.first(),
                    _ => None,
                })
                .and_then(|value| match value {
                    FrameValue::U64(code) => Some(*code),
                    _ => None,
                })
        };
        assert_eq!(
            error_code(2),
            Some(ChatErrorCode::DurableMutationNotNegotiated as u16 as u64)
        );
        assert_eq!(
            error_code(3),
            Some(ChatErrorCode::DurableMutationMalformed as u16 as u64)
        );
        assert!(responses
            .iter()
            .any(|frame| frame.op == ChatOp::MessageAck && frame.seq == 4));
    }

    #[test]
    fn live_server_broadcasts_global_room_delta_to_connected_peers() {
        let store = OmenchatStore::in_memory().expect("store");
        let admin = store
            .ensure_user(b"admin-id", "Admin", None)
            .expect("admin user");
        store
            .set_user_role_bits(admin.user_id, 1 << 2)
            .expect("admin role");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let admin_link = [31u8; 16];
        let observer_link = [32u8; 16];

        for (link_id, identity, name) in [
            (admin_link, b"admin-id".as_slice(), "Admin"),
            (observer_link, b"observer-id".as_slice(), "Observer"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
        }

        let frames_before_create = live.transport().frames.len();
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: admin_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::Command,
                5,
                None,
                FrameBody::Text("create #ops Operations".into()),
            ))
            .expect("create command"),
        })
        .expect("create room");

        let observer_delta = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_create)
            .find(|captured| {
                captured.link_id == observer_link
                    && decode_frame(&captured.bytes)
                        .map(|frame| frame.op == ChatOp::RoomDelta && frame.room_id.is_none())
                        .unwrap_or(false)
            });

        assert!(observer_delta.is_some());
    }

    #[test]
    fn live_server_broadcasts_user_delta_to_room_peers() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let admin = store
            .ensure_user(b"admin-id", "Admin", None)
            .expect("admin user");
        store
            .set_user_role_bits(admin.user_id, 1 << 2)
            .expect("admin role");
        store.ensure_user(b"bob-id", "Bob", None).expect("bob user");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let admin_link = [33u8; 16];
        let observer_link = [34u8; 16];

        for (link_id, identity, name) in [
            (admin_link, b"admin-id".as_slice(), "Admin"),
            (observer_link, b"observer-id".as_slice(), "Observer"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
        }
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: observer_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                1,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join observer"),
        })
        .expect("observer join");

        let frames_before_role = live.transport().frames.len();
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: admin_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::Command,
                2,
                Some(room.room_id),
                FrameBody::Text("role Bob mod".into()),
            ))
            .expect("role command"),
        })
        .expect("role update");

        let observer_delta = live
            .transport()
            .frames
            .iter()
            .skip(frames_before_role)
            .find(|captured| {
                captured.link_id == observer_link
                    && decode_frame(&captured.bytes)
                        .map(|frame| frame.op == ChatOp::UserDelta)
                        .unwrap_or(false)
            });

        assert!(observer_delta.is_some());
    }

    #[test]
    fn live_server_counts_protocol_errors_without_dropping_link() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [9u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: b"not-msgpack".to_vec(),
        })
        .expect("malformed frame is counted, not fatal");

        assert_eq!(live.stats().active_links, 1);
        assert_eq!(live.stats().protocol_errors, 1);
        assert!(live
            .stats()
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("decode"));
        assert!(live.stats().summary_line().contains("protocol_errors=1"));
    }

    #[test]
    fn live_server_counts_link_closes() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [10u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id,
            reason: Some("test".into()),
        })
        .expect("close link");

        assert_eq!(live.stats().active_links, 0);
        assert_eq!(live.stats().links_opened, 1);
        assert_eq!(live.stats().links_closed, 1);
        let closed = live.recent_closed_link_summaries();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].link_id, link_id);
        assert_eq!(closed[0].display_name, "Live Peer");
        assert_eq!(
            closed[0].identity_hash.as_deref(),
            Some(b"peer-live".as_slice())
        );
        assert_eq!(closed[0].reason, "test");
        assert!(closed[0].connected_at_unix > 0);
        assert!(closed[0].closed_at_unix >= closed[0].connected_at_unix);
    }

    #[test]
    fn live_server_link_close_releases_owned_pending_upload_offers() {
        let upload_root = std::env::temp_dir().join(format!(
            "omenchatd-live-pending-upload-{}-{}",
            std::process::id(),
            current_unix_secs()
        ));
        let _ = std::fs::remove_dir_all(&upload_root);
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_commands_per_minute: 0,
                upload_quota_bytes: 1024,
                upload_cache_root: Some(upload_root.clone()),
                ..SessionLimits::default()
            },
        );
        let link_id = [53u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        for frame in [
            Frame::new(ChatOp::JoinRoom, 1, None, FrameBody::Text("lobby".into())),
            Frame::new(
                ChatOp::UploadOffer,
                2,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("pending.bin".into()),
                    FrameValue::U64(4),
                ]),
            ),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&frame).expect("encode frame"),
            })
            .expect("handle frame");
        }
        let before = live.stats();
        assert_eq!(before.pending_upload_items, 1);
        assert_eq!(before.pending_upload_identities, 1);
        assert!(before
            .summary_line()
            .contains("pending_uploads=items:1 identities:1 rejected:0 expired:0"));

        live.handle_event(OmenchatLinkEvent::LinkClosed {
            link_id,
            reason: Some("test".into()),
        })
        .expect("close link");

        let after = live.stats();
        assert_eq!(after.pending_upload_items, 0);
        assert_eq!(after.pending_upload_identities, 0);
        let _ = std::fs::remove_dir_all(upload_root);
    }

    #[test]
    fn inbound_resource_failure_releases_peer_upload_offers_without_closing_link() {
        let upload_root = std::env::temp_dir().join(format!(
            "omenchatd-live-resource-failure-{}-{}",
            std::process::id(),
            current_unix_secs()
        ));
        let _ = std::fs::remove_dir_all(&upload_root);
        let engine = SessionEngine::with_limits(
            OmenchatStore::in_memory().expect("store"),
            SessionLimits {
                rate_commands_per_minute: 0,
                upload_quota_bytes: 1024,
                upload_cache_root: Some(upload_root.clone()),
                ..SessionLimits::default()
            },
        );
        let link_id = [54u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        for frame in [
            Frame::new(ChatOp::JoinRoom, 1, None, FrameBody::Text("lobby".into())),
            Frame::new(
                ChatOp::UploadOffer,
                2,
                Some(1),
                FrameBody::Fields(vec![
                    FrameValue::String("failed.bin".into()),
                    FrameValue::U64(4),
                ]),
            ),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&frame).expect("encode frame"),
            })
            .expect("handle frame");
        }
        assert_eq!(live.stats().pending_upload_items, 1);

        live.handle_event(OmenchatLinkEvent::ResourceTerminal {
            link_id,
            direction: LiveResourceDirection::Inbound,
            outcome: LiveResourceOutcome::Failed,
        })
        .expect("handle inbound failure");

        let stats = live.stats();
        assert_eq!(stats.active_links, 1);
        assert_eq!(stats.pending_upload_items, 0);
        assert_eq!(stats.resource_inbound_failed, 1);
        assert_eq!(stats.upload_offers_released_on_resource_failure, 1);
        assert!(live.transport().closed_links.is_empty());
        let _ = std::fs::remove_dir_all(upload_root);
    }

    #[test]
    fn outbound_resource_terminals_are_counted_after_link_cleanup() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let link_id = [55u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        for outcome in [
            LiveResourceOutcome::Complete,
            LiveResourceOutcome::Failed,
            LiveResourceOutcome::Cancelled,
        ] {
            live.handle_event(OmenchatLinkEvent::ResourceTerminal {
                link_id,
                direction: LiveResourceDirection::Outbound,
                outcome,
            })
            .expect("handle outbound terminal");
        }
        live.handle_event(OmenchatLinkEvent::ResourceTerminal {
            link_id,
            direction: LiveResourceDirection::Inbound,
            outcome: LiveResourceOutcome::Failed,
        })
        .expect("handle late inbound failure");

        let stats = live.stats();
        assert_eq!(stats.resource_inbound_failed, 1);
        assert_eq!(stats.resource_outbound_complete, 1);
        assert_eq!(stats.resource_outbound_failed, 1);
        assert_eq!(stats.resource_outbound_cancelled, 1);
        assert_eq!(stats.unknown_link_packets, 0);
        assert_eq!(stats.protocol_errors, 0);
        assert!(stats.summary_line().contains(
            "resource_terminal=in_failed:1 out_complete:1 out_failed:1 out_cancelled:1 upload_offers_released:0"
        ));
    }

    #[test]
    fn live_server_reports_active_link_summaries_for_monitoring() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [42u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");

        let summaries = live.active_link_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].link_id, link_id);
        assert_eq!(summaries[0].display_name, "Live Peer");
        assert_eq!(summaries[0].identity_hash, b"peer-live".to_vec());
        assert!(summaries[0].connected_at_unix > 0);
    }

    #[test]
    fn live_server_reports_active_link_traffic_for_monitoring() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [43u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: peer(),
        })
        .expect("open link");
        let ping = encode_frame(&Frame::new(ChatOp::Ping, 1, None, FrameBody::Empty))
            .expect("encode ping");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: ping.clone(),
        })
        .expect("ping");

        let summary = live
            .active_link_summaries()
            .into_iter()
            .find(|summary| summary.link_id == link_id)
            .expect("summary");
        assert_eq!(summary.traffic.frames_in, 1);
        assert_eq!(summary.traffic.bytes_in, ping.len() as u64);
        assert_eq!(summary.traffic.pings, 1);
    }

    #[test]
    fn live_server_disconnects_active_links_for_banned_identity() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_a = [14u8; 16];
        let link_b = [15u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_a,
            peer: ServerPeer {
                identity_hash: b"same-user".to_vec(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open alice");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_b,
            peer: ServerPeer {
                identity_hash: b"other-user".to_vec(),
                display_name: "Bob".into(),
                lxmf_destination: None,
            },
        })
        .expect("open bob");

        let closed = live.disconnect_identity(b"same-user");

        assert_eq!(closed, 1);
        assert_eq!(live.stats().active_links, 1);
        assert_eq!(live.stats().links_closed, 1);
        assert_eq!(live.transport().closed_links, vec![link_a]);
    }

    #[test]
    fn live_server_reports_active_identity_link_counts_after_duplicate_replacement() {
        let mut live = OmenchatLiveServer::new(
            SessionEngine::new(OmenchatStore::in_memory().expect("store")),
            CapturedTransport::default(),
        );
        for (link_id, identity, name) in [
            ([41u8; 16], b"same-user".as_slice(), "Alice"),
            ([42u8; 16], b"same-user".as_slice(), "Alice second link"),
            ([43u8; 16], b"other-user".as_slice(), "Bob"),
        ] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: identity.to_vec(),
                    display_name: name.into(),
                    lxmf_destination: None,
                },
            })
            .expect("open link");
        }

        let counts = live.active_identity_counts();

        assert!(counts
            .iter()
            .any(|(identity, count)| identity == b"same-user" && *count == 1));
        assert!(counts
            .iter()
            .any(|(identity, count)| identity == b"other-user" && *count == 1));
        assert_eq!(live.stats().active_links, 2);
        assert_eq!(live.stats().links_closed, 1);
        assert_eq!(live.transport().closed_links, vec![[41u8; 16]]);
        assert_eq!(
            live.recent_closed_link_summaries()[0].reason,
            "duplicate identity link replaced"
        );
    }

    #[test]
    fn duplicate_peer_retirement_clears_all_per_link_state() {
        let mut live = OmenchatLiveServer::new(
            SessionEngine::new(OmenchatStore::in_memory().expect("store")),
            CapturedTransport::default(),
        );
        let old_link = [51u8; 16];
        let replacement_link = [52u8; 16];
        let identity = b"same-user".to_vec();
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: old_link,
            peer: ServerPeer {
                identity_hash: identity.clone(),
                display_name: "Alice".into(),
                lxmf_destination: None,
            },
        })
        .expect("open old link");
        live.link_rooms.insert(old_link, 1);
        live.link_response_contexts.insert(old_link, 7);
        assert!(live
            .replay_cache
            .insert(old_link, 1, vec![1, 2, 3], Vec::new()));

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement_link,
            peer: ServerPeer {
                identity_hash: identity,
                display_name: "Alice replacement".into(),
                lxmf_destination: None,
            },
        })
        .expect("open replacement link");

        assert!(!live.peers.contains_key(&old_link));
        assert!(!live.link_rooms.contains_key(&old_link));
        assert!(!live.link_response_contexts.contains_key(&old_link));
        assert!(!live.link_opened_at.contains_key(&old_link));
        assert!(!live.link_traffic.contains_key(&old_link));
        assert!(!live
            .replay_cache
            .entries
            .keys()
            .any(|key| key.0 == old_link));
        assert!(live.peers.contains_key(&replacement_link));
        assert_eq!(live.transport().closed_links, vec![old_link]);
        assert_eq!(live.stats().active_links, 1);
        assert_eq!(live.stats().links_closed, 1);
    }

    #[test]
    fn identified_link_open_upgrades_provisional_peer_without_losing_display_name() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let link_id = [13u8; 16];
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: ServerPeer {
                identity_hash: link_id.to_vec(),
                display_name: "link-aabbccdd".into(),
                lxmf_destination: None,
            },
        })
        .expect("provisional open");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::SessionOpen,
                1,
                None,
                FrameBody::Fields(vec![
                    FrameValue::String("omenchat/0.1".into()),
                    FrameValue::String("OMENbrowser_dev".into()),
                    FrameValue::String("0b09688ccb50c3ca949399fed7108f7f".into()),
                ]),
            ))
            .expect("session open"),
        })
        .expect("session open handled");
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id,
            peer: ServerPeer {
                identity_hash: b"stable-browser-id".to_vec(),
                display_name: "peer-12345678".into(),
                lxmf_destination: None,
            },
        })
        .expect("identified open");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(
                ChatOp::JoinRoom,
                2,
                None,
                FrameBody::Text("lobby".into()),
            ))
            .expect("join"),
        })
        .expect("join handled");

        let userlist_frame = live
            .transport()
            .frames
            .iter()
            .filter_map(|captured| decode_frame(&captured.bytes).ok())
            .find(|frame| frame.op == ChatOp::UserListSnapshotInline)
            .expect("userlist frame");
        let user_values = decode_compressed_values_body(&userlist_frame.body).expect("userlist");
        let first_user = match &user_values[0] {
            FrameValue::Array(fields) => fields,
            _ => panic!("expected user array"),
        };
        assert_eq!(
            first_user.get(1),
            Some(&FrameValue::String("OMENbrowser_dev".into()))
        );
    }

    #[test]
    fn pending_handshake_admission_is_bounded_and_recovers_after_completion() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());

        for index in 0..PENDING_HANDSHAKE_MAX_ITEMS {
            let link_id = indexed_link_id(index);
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: provisional_peer(link_id),
            })
            .expect("admit pending handshake");
        }
        let rejected_link = indexed_link_id(PENDING_HANDSHAKE_MAX_ITEMS);
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: rejected_link,
            peer: provisional_peer(rejected_link),
        })
        .expect("reject excess pending handshake");
        assert_eq!(live.stats().active_links, PENDING_HANDSHAKE_MAX_ITEMS);
        assert_eq!(live.stats().pending_handshakes, PENDING_HANDSHAKE_MAX_ITEMS);
        assert_eq!(live.stats().link_admission_rejected, 1);
        assert_eq!(live.transport().closed_links, vec![rejected_link]);

        let completed_link = indexed_link_id(0);
        live.handle_event(OmenchatLinkEvent::PeerIdentified {
            link_id: completed_link,
            identity_hash: [0x91; 16],
        })
        .expect("identify peer");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: completed_link,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::SessionOpen, 1, None, FrameBody::Empty))
                .expect("session open"),
        })
        .expect("negotiate session");
        assert_eq!(
            live.stats().pending_handshakes,
            PENDING_HANDSHAKE_MAX_ITEMS - 1
        );

        let replacement = indexed_link_id(PENDING_HANDSHAKE_MAX_ITEMS + 1);
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: replacement,
            peer: provisional_peer(replacement),
        })
        .expect("admit after completed handshake");
        assert_eq!(live.stats().active_links, PENDING_HANDSHAKE_MAX_ITEMS + 1);
    }

    #[test]
    fn incomplete_handshake_expires_at_deadline_but_complete_link_survives() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        let incomplete = indexed_link_id(1);
        let complete = indexed_link_id(2);
        for link_id in [incomplete, complete] {
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: provisional_peer(link_id),
            })
            .expect("open link");
            live.link_opened_at.insert(link_id, 100);
        }
        live.handle_event(OmenchatLinkEvent::PeerIdentified {
            link_id: complete,
            identity_hash: [0x92; 16],
        })
        .expect("identify peer");
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id: complete,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&Frame::new(ChatOp::SessionOpen, 1, None, FrameBody::Empty))
                .expect("session open"),
        })
        .expect("complete session");

        assert_eq!(live.expire_pending_handshakes(129), 0);
        assert_eq!(live.expire_pending_handshakes(130), 1);
        assert!(!live.peers.contains_key(&incomplete));
        assert!(live.peers.contains_key(&complete));
        assert_eq!(live.stats().handshake_expired, 1);
        assert_eq!(live.stats().pending_handshakes, 0);
        assert_eq!(live.transport().closed_links, vec![incomplete]);
    }

    #[test]
    fn total_active_link_admission_is_bounded() {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        let mut live = OmenchatLiveServer::new(engine, CapturedTransport::default());
        for index in 0..ACTIVE_LINK_MAX_ITEMS {
            let link_id = indexed_link_id(index);
            live.handle_event(OmenchatLinkEvent::LinkOpened {
                link_id,
                peer: ServerPeer {
                    identity_hash: format!("identified-{index}").into_bytes(),
                    display_name: format!("Peer {index}"),
                    lxmf_destination: None,
                },
            })
            .expect("admit identified link");
            live.handle_event(OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                data: encode_frame(&Frame::new(
                    ChatOp::SessionOpen,
                    index as u32 + 1,
                    None,
                    FrameBody::Empty,
                ))
                .expect("session open"),
            })
            .expect("complete identified link handshake");
        }
        let excess = indexed_link_id(ACTIVE_LINK_MAX_ITEMS);
        live.handle_event(OmenchatLinkEvent::LinkOpened {
            link_id: excess,
            peer: ServerPeer {
                identity_hash: b"identified-excess".to_vec(),
                display_name: "Excess Peer".into(),
                lxmf_destination: None,
            },
        })
        .expect("reject excess active link");

        assert_eq!(live.stats().active_links, ACTIVE_LINK_MAX_ITEMS);
        assert_eq!(live.stats().link_admission_rejected, 1);
        assert_eq!(live.transport().closed_links, vec![excess]);
    }

    fn indexed_link_id(index: usize) -> LinkId {
        let mut link_id = [0u8; 16];
        link_id[..8].copy_from_slice(&(index as u64).to_be_bytes());
        link_id
    }

    fn provisional_peer(link_id: LinkId) -> ServerPeer {
        ServerPeer {
            identity_hash: link_id.to_vec(),
            display_name: format!("link-{}", short_link_id(&link_id)),
            lxmf_destination: None,
        }
    }
}
