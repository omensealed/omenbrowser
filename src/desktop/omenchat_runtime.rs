#[cfg(feature = "chat-client")]
use crate::chat::client::is_restorable_server_destination;
#[cfg(feature = "chat-client")]
use crate::chat::protocol::RoomId;
#[cfg(feature = "chat-client")]
use std::collections::HashMap;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::collections::{BTreeMap, VecDeque};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use std::time::Duration;

#[cfg(feature = "chat-client")]
use crate::chat::{ChatClientRequest, ChatSessionId, ChatSessionView};

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::app::current_epoch_ms;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::chat::rns::{resource_id_from_metadata, ChatLinkTransport};
#[cfg(feature = "chat-client")]
use crate::chat::store::{ChatStore, SqliteChatStore};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::desktop::OMENCHAT_HEARTBEAT_IDLE_MS;
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use crate::desktop::{Message, OMENCHAT_PATH_RECONNECT_DELAY_MS};
#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
use iced::Task;

#[cfg(feature = "chat-client")]
pub(in crate::desktop) fn request_session_id(request: &ChatClientRequest) -> Option<ChatSessionId> {
    match request {
        ChatClientRequest::OpenServer(_) => None,
        ChatClientRequest::JoinRoom { session_id, .. }
        | ChatClientRequest::PartRoom { session_id, .. }
        | ChatClientRequest::SendMessage { session_id, .. }
        | ChatClientRequest::SendAction { session_id, .. }
        | ChatClientRequest::SendNotice { session_id, .. }
        | ChatClientRequest::SendUpload { session_id, .. }
        | ChatClientRequest::RequestUpload { session_id, .. }
        | ChatClientRequest::RefreshRooms { session_id }
        | ChatClientRequest::SetTopic { session_id, .. }
        | ChatClientRequest::CreateRoom { session_id, .. }
        | ChatClientRequest::ModerateUser { session_id, .. }
        | ChatClientRequest::SyncRecent { session_id }
        | ChatClientRequest::LoadOlder { session_id } => Some(*session_id),
    }
}

#[cfg(feature = "chat-client")]
pub(in crate::desktop) fn prune_unrestorable_omenchat_servers(store: &mut SqliteChatStore) {
    let Ok(servers) = store.saved_servers() else {
        return;
    };
    for server in servers {
        if is_restorable_server_destination(&server.destination) {
            continue;
        }
        if let Err(error) = store.delete_server(&server.server_id) {
            tracing::warn!(
                "failed to prune unrestorable OMENchat server {}: {error}",
                server.server_id
            );
        }
    }
}

#[cfg(feature = "chat-client")]
pub(in crate::desktop) fn omenchat_event_counts_by_room(
    sessions: &[ChatSessionView],
) -> HashMap<(ChatSessionId, RoomId), usize> {
    let mut counts = HashMap::new();
    for session in sessions {
        counts
            .entry((session.session_id, session.active_room.room_id))
            .or_insert(0);
        for event in &session.events {
            *counts
                .entry((session.session_id, event.room_id))
                .or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(all(test, feature = "chat-client"))]
#[path = "omenchat_runtime_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(feature = "chat-client-rns", feature = "chat-client-rns-clean")
))]
#[path = "omenchat_runtime_live_transport_tests.rs"]
mod live_transport_tests;

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn delayed_omenchat_reconnect_if_disconnected_task(
    session_id: ChatSessionId,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(OMENCHAT_PATH_RECONNECT_DELAY_MS)).await;
            session_id
        },
        |session_id| {
            Message::OmenChat(super::OmenChatMessage::ReconnectSessionIfDisconnected(
                session_id,
            ))
        },
    )
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
#[derive(Clone, Debug, Default)]
pub(in crate::desktop) struct DesktopOmenChatTransport {
    pub(in crate::desktop) link_id: [u8; 16],
    pub(in crate::desktop) incoming_frames: VecDeque<Vec<u8>>,
    pub(in crate::desktop) incoming_frame_bytes: usize,
    pub(in crate::desktop) resources: BTreeMap<String, Vec<u8>>,
    pub(in crate::desktop) resource_order: VecDeque<String>,
    pub(in crate::desktop) resource_cached_bytes: usize,
    pub(in crate::desktop) pending_resource_offers: BTreeMap<String, VecDeque<Vec<u8>>>,
    pub(in crate::desktop) pending_resource_offer_bytes: usize,
    pub(in crate::desktop) rejected_resources: u64,
    pub(in crate::desktop) rejected_resource_offers: u64,
    pub(in crate::desktop) rejected_incoming_frames: u64,
    pub(in crate::desktop) rejected_outgoing_frames: u64,
    pub(in crate::desktop) rejected_outgoing_resources: u64,
    pub(in crate::desktop) outgoing_frames: Vec<Vec<u8>>,
    pub(in crate::desktop) outgoing_frame_bytes: usize,
    pub(in crate::desktop) outgoing_resources: Vec<(String, Vec<u8>)>,
    pub(in crate::desktop) outgoing_resource_bytes: usize,
    pub(in crate::desktop) last_rx_epoch_ms: u64,
    pub(in crate::desktop) last_tx_epoch_ms: u64,
    pub(in crate::desktop) last_ping_epoch_ms: u64,
    pub(in crate::desktop) last_pong_epoch_ms: u64,
    pub(in crate::desktop) last_ping_rtt_ms: Option<u64>,
    pub(in crate::desktop) connected_since_epoch_ms: u64,
    pub(in crate::desktop) frames_in: u64,
    pub(in crate::desktop) frames_out: u64,
    pub(in crate::desktop) bytes_in: u64,
    pub(in crate::desktop) bytes_out: u64,
    pub(in crate::desktop) resource_bytes_in: u64,
    pub(in crate::desktop) resources_in: u64,
    pub(in crate::desktop) history_frames_in: u64,
    pub(in crate::desktop) history_frames_out: u64,
    pub(in crate::desktop) room_events_in: u64,
    pub(in crate::desktop) chat_frames_out: u64,
    pub(in crate::desktop) userlist_frames_in: u64,
    pub(in crate::desktop) resource_offers_in: u64,
    pub(in crate::desktop) upload_fetches_out: u64,
    pub(in crate::desktop) upload_resource_offers_in: u64,
    pub(in crate::desktop) upload_inline_chunks_in: u64,
    pub(in crate::desktop) upload_inline_bytes_in: u64,
    pub(in crate::desktop) upload_resources_in: u64,
    pub(in crate::desktop) upload_resource_bytes_in: u64,
    pub(in crate::desktop) pings_in: u64,
    pub(in crate::desktop) pings_out: u64,
    pub(in crate::desktop) pongs_in: u64,
    pub(in crate::desktop) pongs_out: u64,
    pub(in crate::desktop) last_rx_frame: Option<String>,
    pub(in crate::desktop) last_tx_frame: Option<String>,
    pub(in crate::desktop) awaiting_pong: bool,
    pub(in crate::desktop) heartbeat_idle_ms: u64,
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl DesktopOmenChatTransport {
    pub(in crate::desktop) fn new(link_id: [u8; 16], now_ms: u64) -> Self {
        Self {
            link_id,
            last_rx_epoch_ms: now_ms,
            last_tx_epoch_ms: now_ms,
            connected_since_epoch_ms: now_ms,
            heartbeat_idle_ms: OMENCHAT_HEARTBEAT_IDLE_MS,
            ..Self::default()
        }
    }

    pub(in crate::desktop) fn push_incoming_frame(&mut self, bytes: Vec<u8>, now_ms: u64) -> bool {
        self.frames_in = self.frames_in.saturating_add(1);
        self.bytes_in = self.bytes_in.saturating_add(bytes.len() as u64);
        self.last_rx_epoch_ms = now_ms;
        if !self.incoming_frame_has_capacity(bytes.len()) {
            self.rejected_incoming_frames = self.rejected_incoming_frames.saturating_add(1);
            return false;
        }
        let op = self.note_incoming_frame(&bytes);
        if matches!(op, Some(crate::chat::protocol::ChatOp::Pong)) {
            self.last_pong_epoch_ms = now_ms;
            if self.last_ping_epoch_ms > 0 {
                self.last_ping_rtt_ms = Some(now_ms.saturating_sub(self.last_ping_epoch_ms));
            }
            self.awaiting_pong = false;
        } else if op.is_some() {
            self.awaiting_pong = false;
        }
        self.incoming_frame_bytes = self.incoming_frame_bytes.saturating_add(bytes.len());
        self.incoming_frames.push_back(bytes);
        true
    }

    fn incoming_frame_has_capacity(&self, bytes: usize) -> bool {
        bytes <= crate::chat::codec::MAX_FRAME_BYTES
            && self.incoming_frames.len() < super::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_ITEMS
            && self.incoming_frame_bytes.saturating_add(bytes)
                <= super::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_BYTES
    }

    fn push_replayed_incoming_frame(&mut self, bytes: Vec<u8>) {
        if !self.incoming_frame_has_capacity(bytes.len()) {
            self.rejected_incoming_frames = self.rejected_incoming_frames.saturating_add(1);
            return;
        }
        self.incoming_frame_bytes = self.incoming_frame_bytes.saturating_add(bytes.len());
        self.incoming_frames.push_front(bytes);
    }

    pub(in crate::desktop) fn push_resource(
        &mut self,
        metadata: Option<Vec<u8>>,
        data: Vec<u8>,
        now_ms: u64,
    ) -> bool {
        self.last_rx_epoch_ms = now_ms;
        self.awaiting_pong = false;
        self.resources_in = self.resources_in.saturating_add(1);
        self.resource_bytes_in = self.resource_bytes_in.saturating_add(data.len() as u64);
        if data.len() > super::OMENCHAT_RESOURCE_MAX_BYTES {
            self.rejected_resources = self.rejected_resources.saturating_add(1);
            return false;
        }
        let inferred_resource_id = if metadata.is_none() && self.pending_resource_offer_count() == 1
        {
            self.pending_resource_offers.keys().next().cloned()
        } else {
            None
        };
        if let Some(resource_id) =
            resource_id_from_metadata(metadata.as_deref()).or(inferred_resource_id)
        {
            if resource_id.starts_with("upload:") {
                self.upload_resources_in = self.upload_resources_in.saturating_add(1);
                self.upload_resource_bytes_in = self
                    .upload_resource_bytes_in
                    .saturating_add(data.len() as u64);
            }
            if let Some(previous) = self.resources.remove(&resource_id) {
                self.resource_cached_bytes =
                    self.resource_cached_bytes.saturating_sub(previous.len());
                self.resource_order.retain(|stored| stored != &resource_id);
            }
            while self.resources.len() >= super::OMENCHAT_RESOURCE_CACHE_MAX_ITEMS
                || self.resource_cached_bytes.saturating_add(data.len())
                    > super::OMENCHAT_RESOURCE_CACHE_MAX_BYTES
            {
                let Some(oldest) = self.resource_order.pop_front() else {
                    self.rejected_resources = self.rejected_resources.saturating_add(1);
                    return false;
                };
                if let Some(removed) = self.resources.remove(&oldest) {
                    self.resource_cached_bytes =
                        self.resource_cached_bytes.saturating_sub(removed.len());
                }
            }
            self.resource_cached_bytes = self.resource_cached_bytes.saturating_add(data.len());
            self.resource_order.push_back(resource_id.clone());
            self.resources.insert(resource_id.clone(), data);
            if let Some(offers) = self.pending_resource_offers.get(&resource_id) {
                let released = offers
                    .iter()
                    .fold(0usize, |total, frame| total.saturating_add(frame.len()));
                self.pending_resource_offer_bytes =
                    self.pending_resource_offer_bytes.saturating_sub(released);
            }
            if let Some(mut offers) = self.pending_resource_offers.remove(&resource_id) {
                while let Some(frame) = offers.pop_back() {
                    self.push_replayed_incoming_frame(frame);
                }
            }
            return true;
        }
        self.rejected_resources = self.rejected_resources.saturating_add(1);
        false
    }

    pub(in crate::desktop) fn take_outgoing_frames(&mut self) -> Vec<Vec<u8>> {
        let frames = std::mem::take(&mut self.outgoing_frames);
        self.outgoing_frame_bytes = 0;
        if !frames.is_empty() {
            self.last_tx_epoch_ms = current_epoch_ms();
        }
        frames
    }

    pub(in crate::desktop) fn take_outgoing_resources(&mut self) -> Vec<(String, Vec<u8>)> {
        let resources = std::mem::take(&mut self.outgoing_resources);
        self.outgoing_resource_bytes = 0;
        if !resources.is_empty() {
            self.last_tx_epoch_ms = current_epoch_ms();
        }
        resources
    }

    pub(in crate::desktop) fn pending_resource_offer_count(&self) -> usize {
        self.pending_resource_offers
            .values()
            .map(VecDeque::len)
            .sum()
    }

    pub(in crate::desktop) fn clear_pending_resource_offers(&mut self) -> usize {
        let released = self.pending_resource_offer_count();
        self.pending_resource_offers.clear();
        self.pending_resource_offer_bytes = 0;
        released
    }

    fn note_incoming_frame(&mut self, bytes: &[u8]) -> Option<crate::chat::protocol::ChatOp> {
        let Ok(frame) = crate::chat::codec::decode_frame(bytes) else {
            self.last_rx_frame = Some("decode error".into());
            return None;
        };
        self.last_rx_frame = Some(omenchat_monitor_frame_label(frame.op));
        match frame.op {
            crate::chat::protocol::ChatOp::HistoryInline
            | crate::chat::protocol::ChatOp::HistoryResourceOffer
            | crate::chat::protocol::ChatOp::HistoryEnd
            | crate::chat::protocol::ChatOp::HistoryCurrent => {
                self.history_frames_in = self.history_frames_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::RoomEvent => {
                self.room_events_in = self.room_events_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UserListSnapshotInline
            | crate::chat::protocol::ChatOp::UserListSnapshotResource => {
                self.userlist_frames_in = self.userlist_frames_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UploadResourceOffer => {
                self.upload_resource_offers_in = self.upload_resource_offers_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UploadInlineChunk => {
                self.upload_inline_chunks_in = self.upload_inline_chunks_in.saturating_add(1);
                if let crate::chat::protocol::FrameBody::Fields(fields) = &frame.body {
                    if let Some(crate::chat::protocol::FrameValue::Bytes(chunk)) = fields.get(5) {
                        self.upload_inline_bytes_in = self
                            .upload_inline_bytes_in
                            .saturating_add(chunk.len() as u64);
                    }
                }
            }
            crate::chat::protocol::ChatOp::Pong => {
                self.pongs_in = self.pongs_in.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::Ping => {
                self.pings_in = self.pings_in.saturating_add(1);
            }
            _ => {}
        }
        if matches!(
            frame.op,
            crate::chat::protocol::ChatOp::HistoryResourceOffer
                | crate::chat::protocol::ChatOp::UserListSnapshotResource
                | crate::chat::protocol::ChatOp::UploadResourceOffer
        ) {
            self.resource_offers_in = self.resource_offers_in.saturating_add(1);
        }
        Some(frame.op)
    }

    fn note_outgoing_frame(&mut self, bytes: &[u8]) {
        let Ok(frame) = crate::chat::codec::decode_frame(bytes) else {
            self.last_tx_frame = Some("decode error".into());
            return;
        };
        self.last_tx_frame = Some(omenchat_monitor_frame_label(frame.op));
        match frame.op {
            crate::chat::protocol::ChatOp::HistoryBefore
            | crate::chat::protocol::ChatOp::HistoryRecent => {
                self.history_frames_out = self.history_frames_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::RoomMessage
            | crate::chat::protocol::ChatOp::RoomAction
            | crate::chat::protocol::ChatOp::RoomNotice
            | crate::chat::protocol::ChatOp::Command => {
                self.chat_frames_out = self.chat_frames_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::Ping => {
                self.pings_out = self.pings_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::UploadFetch => {
                self.upload_fetches_out = self.upload_fetches_out.saturating_add(1);
            }
            crate::chat::protocol::ChatOp::Pong => {
                self.pongs_out = self.pongs_out.saturating_add(1);
            }
            _ => {}
        }
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
impl ChatLinkTransport for DesktopOmenChatTransport {
    fn send_frame(&mut self, frame_bytes: Vec<u8>) -> anyhow::Result<()> {
        let next_bytes = self
            .outgoing_frame_bytes
            .checked_add(frame_bytes.len())
            .ok_or_else(|| anyhow::anyhow!("OMENchat outgoing frame byte overflow"))?;
        if frame_bytes.len() > crate::chat::codec::MAX_FRAME_BYTES
            || self.outgoing_frames.len() >= super::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_ITEMS
            || next_bytes > super::OMENCHAT_TRANSPORT_FRAME_QUEUE_MAX_BYTES
        {
            self.rejected_outgoing_frames = self.rejected_outgoing_frames.saturating_add(1);
            anyhow::bail!("OMENchat outgoing frame queue budget exceeded");
        }
        self.frames_out = self.frames_out.saturating_add(1);
        self.bytes_out = self.bytes_out.saturating_add(frame_bytes.len() as u64);
        self.note_outgoing_frame(&frame_bytes);
        self.outgoing_frame_bytes = next_bytes;
        self.outgoing_frames.push(frame_bytes);
        Ok(())
    }

    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        let frame = self.incoming_frames.pop_front();
        if let Some(bytes) = frame.as_ref() {
            self.incoming_frame_bytes = self.incoming_frame_bytes.saturating_sub(bytes.len());
        }
        Ok(frame)
    }

    fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let resource = self.resources.remove(resource_id);
        if let Some(bytes) = resource.as_ref() {
            self.resource_cached_bytes = self.resource_cached_bytes.saturating_sub(bytes.len());
            self.resource_order.retain(|stored| stored != resource_id);
        }
        Ok(resource)
    }

    fn send_resource(&mut self, resource_id: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        let metadata_len = crate::chat::rns::resource_metadata(resource_id).len();
        if !crate::resource_compat::metadata_bearing_resource_is_unsplit_safe(
            payload.len(),
            metadata_len,
        ) {
            self.rejected_outgoing_resources = self.rejected_outgoing_resources.saturating_add(1);
            anyhow::bail!(
                "OMENchat Resource exceeds the safe Reticulum 0.9.7 single-segment limit"
            );
        }
        let next_bytes = self
            .outgoing_resource_bytes
            .checked_add(payload.len())
            .ok_or_else(|| anyhow::anyhow!("OMENchat outgoing Resource byte overflow"))?;
        if resource_id.len() > super::OMENCHAT_TRANSPORT_RESOURCE_ID_MAX_BYTES
            || payload.len() > super::OMENCHAT_RESOURCE_MAX_BYTES
            || self.outgoing_resources.len() >= super::OMENCHAT_TRANSPORT_RESOURCE_QUEUE_MAX_ITEMS
            || next_bytes > super::OMENCHAT_TRANSPORT_RESOURCE_QUEUE_MAX_BYTES
        {
            self.rejected_outgoing_resources = self.rejected_outgoing_resources.saturating_add(1);
            anyhow::bail!("OMENchat outgoing Resource queue budget exceeded");
        }
        self.bytes_out = self.bytes_out.saturating_add(payload.len() as u64);
        self.outgoing_resource_bytes = next_bytes;
        self.outgoing_resources
            .push((resource_id.to_owned(), payload));
        Ok(())
    }

    fn defer_resource_offer(
        &mut self,
        resource_id: &str,
        frame_bytes: Vec<u8>,
    ) -> anyhow::Result<()> {
        let next_bytes = self
            .pending_resource_offer_bytes
            .checked_add(frame_bytes.len())
            .ok_or_else(|| anyhow::anyhow!("OMENchat pending Resource offer byte overflow"))?;
        if frame_bytes.len() > crate::chat::codec::MAX_FRAME_BYTES
            || self.pending_resource_offer_count()
                >= super::OMENCHAT_PENDING_RESOURCE_OFFER_MAX_ITEMS
            || next_bytes > super::OMENCHAT_PENDING_RESOURCE_OFFER_MAX_BYTES
        {
            self.rejected_resource_offers = self.rejected_resource_offers.saturating_add(1);
            anyhow::bail!("OMENchat pending Resource offer budget exceeded");
        }
        self.pending_resource_offer_bytes = next_bytes;
        self.pending_resource_offers
            .entry(resource_id.to_owned())
            .or_default()
            .push_back(frame_bytes);
        Ok(())
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_live_open_error_status(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("request_path") || lower.contains("path to") && lower.contains("not known") {
        "path missing: request path was queued; wait for the server announce/path, then reconnect"
            .into()
    } else if lower.contains("no known identity key") {
        "path/key missing: request path and wait for the server announce, then reopen this OMENchat link"
            .into()
    } else if lower.contains("timed out") && lower.contains("link") {
        "link establishment timed out: path exists but the server did not complete the Link handshake"
            .into()
    } else if lower.contains("timed out") {
        "server response timed out: Link opened, but the server did not answer before the wait limit"
            .into()
    } else if lower.contains("runtime is not running") || lower.contains("runtime is not started") {
        "Reticulum runtime is not running; start/connect the runtime, then reopen this OMENchat link"
            .into()
    } else {
        format!("live link failed: {error}")
    }
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_close_reason_is_timeout(reason: Option<&str>) -> bool {
    reason
        .map(str::trim)
        .is_some_and(|reason| reason.eq_ignore_ascii_case("timeout"))
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_close_reason_allows_quick_reconnect(
    reason: Option<&str>,
) -> bool {
    let Some(reason) = reason.map(str::trim) else {
        return false;
    };
    reason.eq_ignore_ascii_case("timeout")
        || reason.eq_ignore_ascii_case("destinationclosed")
        || reason.eq_ignore_ascii_case("initiatorclosed")
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(any(feature = "chat-client-rns", feature = "chat-client-rns-clean"))]
pub(in crate::desktop) fn omenchat_monitor_frame_label(
    op: crate::chat::protocol::ChatOp,
) -> String {
    match op {
        crate::chat::protocol::ChatOp::SessionOpen => "session open",
        crate::chat::protocol::ChatOp::SessionAccept => "session accepted",
        crate::chat::protocol::ChatOp::SessionReject => "session rejected",
        crate::chat::protocol::ChatOp::JoinRoom => "join room",
        crate::chat::protocol::ChatOp::JoinAccept => "join accepted",
        crate::chat::protocol::ChatOp::PartRoom => "part room",
        crate::chat::protocol::ChatOp::RoomSubscribe => "room subscribe",
        crate::chat::protocol::ChatOp::RoomUnsubscribe => "room unsubscribe",
        crate::chat::protocol::ChatOp::RoomMessage => "room message",
        crate::chat::protocol::ChatOp::RoomAction => "room action",
        crate::chat::protocol::ChatOp::RoomNotice => "room notice",
        crate::chat::protocol::ChatOp::RoomEvent => "room event",
        crate::chat::protocol::ChatOp::MessageAck => "message ack",
        crate::chat::protocol::ChatOp::RoomReaction => "room reaction",
        crate::chat::protocol::ChatOp::ReactionAck => "reaction ack",
        crate::chat::protocol::ChatOp::ReactionEvent => "reaction event",
        crate::chat::protocol::ChatOp::ReactionSnapshotInline => "reaction snapshot inline",
        crate::chat::protocol::ChatOp::ReactionSnapshotResource => "reaction snapshot resource",
        crate::chat::protocol::ChatOp::UserListSnapshotInline => "userlist inline",
        crate::chat::protocol::ChatOp::UserListSnapshotResource => "userlist resource",
        crate::chat::protocol::ChatOp::UserDelta => "user delta",
        crate::chat::protocol::ChatOp::RoomDelta => "room delta",
        crate::chat::protocol::ChatOp::RoleDelta => "role delta",
        crate::chat::protocol::ChatOp::RoomMessageRevision => "message revision",
        crate::chat::protocol::ChatOp::MessageRevisionAck => "message revision ack",
        crate::chat::protocol::ChatOp::MessageRevisionEvent => "message revision event",
        crate::chat::protocol::ChatOp::MessageRevisionSnapshotInline => {
            "message revision snapshot inline"
        }
        crate::chat::protocol::ChatOp::MessageRevisionSnapshotResource => {
            "message revision snapshot resource"
        }
        crate::chat::protocol::ChatOp::HistoryBefore => "history before",
        crate::chat::protocol::ChatOp::HistoryInline => "history inline",
        crate::chat::protocol::ChatOp::HistoryResourceOffer => "history resource",
        crate::chat::protocol::ChatOp::HistoryEnd => "history end",
        crate::chat::protocol::ChatOp::HistoryRecent => "history recent",
        crate::chat::protocol::ChatOp::HistoryCurrent => "history current",
        crate::chat::protocol::ChatOp::RoomPin => "room pin",
        crate::chat::protocol::ChatOp::PinAck => "pin ack",
        crate::chat::protocol::ChatOp::PinEvent => "pin event",
        crate::chat::protocol::ChatOp::PinSnapshot => "pin snapshot",
        crate::chat::protocol::ChatOp::Command => "command",
        crate::chat::protocol::ChatOp::CommandResult => "command result",
        crate::chat::protocol::ChatOp::ModerationAuditBefore => "moderation audit before",
        crate::chat::protocol::ChatOp::ModerationAuditInline => "moderation audit inline",
        crate::chat::protocol::ChatOp::ModerationAuditResource => "moderation audit resource",
        crate::chat::protocol::ChatOp::ModerationAuditEnd => "moderation audit end",
        crate::chat::protocol::ChatOp::ContactRequest => "contact request",
        crate::chat::protocol::ChatOp::ContactOffer => "contact offer",
        crate::chat::protocol::ChatOp::ContactAccept => "contact accepted",
        crate::chat::protocol::ChatOp::ContactReject => "contact rejected",
        crate::chat::protocol::ChatOp::UploadOffer => "upload offer",
        crate::chat::protocol::ChatOp::UploadAccept => "upload accepted",
        crate::chat::protocol::ChatOp::UploadReject => "upload rejected",
        crate::chat::protocol::ChatOp::UploadComplete => "upload complete",
        crate::chat::protocol::ChatOp::UploadFetch => "upload fetch",
        crate::chat::protocol::ChatOp::UploadResourceOffer => "upload resource",
        crate::chat::protocol::ChatOp::UploadInlineChunk => "upload chunk",
        crate::chat::protocol::ChatOp::Ping => "ping",
        crate::chat::protocol::ChatOp::Pong => "pong",
        crate::chat::protocol::ChatOp::Error => "error",
    }
    .into()
}
