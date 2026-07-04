use std::collections::{BTreeMap, VecDeque};

use crate::error::ServerResult;
use crate::protocol::codec::decode_frame;
use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue, RoomId};
use crate::session::{ServerPeer, SessionEngine};
use crate::transport::{
    send_response_frame_with_context, LinkId, OmenchatTransport, OMENCHAT_LINK_CONTEXT,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OmenchatLinkEvent {
    LinkOpened {
        link_id: LinkId,
        peer: ServerPeer,
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
    LinkClosed {
        link_id: LinkId,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveServerStats {
    pub active_links: usize,
    pub links_opened: u64,
    pub links_closed: u64,
    pub frames_in: u64,
    pub frames_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub resource_bytes_out: u64,
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
    pub last_error: Option<String>,
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
            "stats: active_links={} links_opened={} links_closed={} frames_in={} frames_out={} traffic_in={} traffic_out={} resource_out={} uploads=offers_in:{} fetches_in:{} resources_in:{} ({}) inline_out:{} ({}) resource_offers_out:{} requests=session:{} room:{} chat:{} history:{} ping:{} command:{} resources_offered={} ignored_context={} unknown_link={} protocol_errors={}",
            self.active_links,
            self.links_opened,
            self.links_closed,
            self.frames_in,
            self.frames_out,
            human_bytes(self.bytes_in),
            human_bytes(self.bytes_out),
            human_bytes(self.resource_bytes_out),
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
            self.protocol_errors
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
    recent_closed_links: VecDeque<ClosedLinkSummary>,
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
            recent_closed_links: VecDeque::new(),
            stats: LiveServerStats::default(),
        }
    }

    pub fn handle_event(&mut self, event: OmenchatLinkEvent) -> ServerResult<()> {
        match event {
            OmenchatLinkEvent::LinkOpened { link_id, peer } => {
                let existing_link = self.peers.contains_key(&link_id);
                let peer = self.peer_for_link_open(link_id, peer);
                let duplicate_links = self
                    .peers
                    .iter()
                    .filter_map(|(existing_link_id, existing_peer)| {
                        (*existing_link_id != link_id
                            && existing_peer.identity_hash == peer.identity_hash)
                            .then_some(*existing_link_id)
                    })
                    .collect::<Vec<_>>();
                for duplicate_link in duplicate_links {
                    self.record_closed_link(
                        duplicate_link,
                        Some("duplicate identity link replaced".into()),
                    );
                    self.peers.remove(&duplicate_link);
                    self.link_rooms.remove(&duplicate_link);
                    self.link_opened_at.remove(&duplicate_link);
                    self.link_traffic.remove(&duplicate_link);
                }
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
                let mut peer = match self.peers.get(&link_id).cloned() {
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
                        let peer =
                            peer_from_session_open(&provisional, &data).unwrap_or(provisional);
                        self.peers.insert(link_id, peer.clone());
                        self.link_opened_at
                            .entry(link_id)
                            .or_insert_with(current_unix_secs);
                        self.link_traffic.entry(link_id).or_default();
                        self.stats.links_opened = self.stats.links_opened.saturating_add(1);
                        self.refresh_active_links();
                        peer
                    }
                };
                if let Some(updated_peer) = peer_from_session_open(&peer, &data) {
                    peer = updated_peer;
                    self.replace_duplicate_peer_links(link_id, &peer);
                    self.peers.insert(link_id, peer.clone());
                    self.refresh_active_links();
                }
                self.record_link_traffic(
                    link_id,
                    data.len() as u64,
                    frame.as_ref().map(|frame| frame.op),
                );
                let joined_room_id = frame.as_ref().and_then(joined_room_id_from_frame);
                let parted_room_id = frame.as_ref().and_then(parted_room_id_from_frame);
                let active_room_peers = joined_room_id
                    .or(parted_room_id)
                    .map(|room_id| self.active_peers_for_room(link_id, room_id, &peer))
                    .unwrap_or_default();
                let moderation_disconnect = frame.as_ref().and_then(|frame| {
                    self.engine
                        .moderation_disconnect_target_for_frame(&peer, frame, &active_room_peers)
                        .ok()
                        .flatten()
                });
                let frames_before = self.transport.frame_count();
                let resources_before = self.transport.resource_count();
                let bytes_before = self.transport.byte_count();
                let resource_bytes_before = self.transport.resource_byte_count();
                if let Err(error) =
                    self.handle_decoded_frame(link_id, &peer, frame.clone(), &active_room_peers)
                {
                    self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                    self.stats.last_error = Some(error.to_string());
                    return Ok(());
                }
                if let Some(identity_hash) = moderation_disconnect {
                    self.disconnect_identity(&identity_hash);
                }
                if let Some(room_id) = joined_room_id {
                    self.link_rooms.insert(link_id, room_id);
                }
                if let Some(room_id) = parted_room_id {
                    self.link_rooms.remove(&link_id);
                    self.broadcast_userlist_for_room(room_id)?;
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
                for response in
                    self.engine
                        .handle_upload_resource(&peer, &resource_id, data.clone())?
                {
                    self.stats.count_outbound_op(&response);
                    self.send_response_frame(link_id, &response)?;
                    self.broadcast_room_event(link_id, &response)?;
                }
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
            OmenchatLinkEvent::LinkClosed { link_id, reason } => {
                let room_id = self.link_rooms.get(&link_id).copied();
                self.record_closed_link(link_id, reason);
                if self.peers.remove(&link_id).is_some() {
                    self.stats.links_closed = self.stats.links_closed.saturating_add(1);
                }
                self.link_rooms.remove(&link_id);
                self.link_response_contexts.remove(&link_id);
                self.link_opened_at.remove(&link_id);
                self.link_traffic.remove(&link_id);
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
    ) -> ServerResult<()> {
        let Some(frame) = frame else {
            return Err(crate::error::ServerError::Message(
                "OMENchat frame decode failed".into(),
            ));
        };
        let request_op = frame.op;
        let request_seq = frame.seq;
        for response in
            self.engine
                .handle_frame_with_active_peers(peer, frame, active_room_peers)?
        {
            if matches!(request_op, ChatOp::RoomMessage | ChatOp::RoomAction)
                && response.op == ChatOp::RoomEvent
            {
                if let Some(ack) = message_ack_from_room_event(request_seq, &response) {
                    self.stats.count_outbound_op(&ack);
                    self.send_response_frame(link_id, &ack)?;
                    self.broadcast_room_event(link_id, &response)?;
                    continue;
                }
            }
            self.stats.count_outbound_op(&response);
            self.send_response_frame(link_id, &response)?;
            self.broadcast_room_event(link_id, &response)?;
        }
        Ok(())
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
        for link_id in link_ids {
            self.send_response_frame(link_id, &frame)?;
        }
        Ok(())
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

    pub fn stats(&self) -> &LiveServerStats {
        &self.stats
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
        for link_id in &links {
            if let Err(error) = self.transport.close_link(*link_id) {
                self.stats.protocol_errors = self.stats.protocol_errors.saturating_add(1);
                self.stats.last_error = Some(error.to_string());
            }
            self.record_closed_link(*link_id, Some("admin disconnect".into()));
            self.peers.remove(link_id);
            self.link_rooms.remove(link_id);
            self.link_response_contexts.remove(link_id);
            self.link_opened_at.remove(link_id);
            self.link_traffic.remove(link_id);
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
            self.record_closed_link(duplicate, Some("duplicate identity link replaced".into()));
            self.peers.remove(&duplicate);
            self.link_rooms.remove(&duplicate);
            self.link_response_contexts.remove(&duplicate);
            self.link_opened_at.remove(&duplicate);
            self.link_traffic.remove(&duplicate);
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

    fn peer() -> ServerPeer {
        ServerPeer {
            identity_hash: b"peer-live".to_vec(),
            display_name: "Live Peer".into(),
            lxmf_destination: Some("lxmf-live".into()),
        }
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
}
