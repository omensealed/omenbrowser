use std::collections::{BTreeMap, VecDeque};

use super::codec::{decode_frame, encode_frame};
use super::model::CHAT_RESOURCE_ID_MAX_BYTES;
use super::protocol::batch::{
    decode_compressed_values_body, decode_resource_batch_payload, decode_resource_offer_body,
    validate_resource_offer_lengths, ResourceOffer,
};
use super::protocol::{ChatOp, Frame, FrameValue};

pub const OMENCHAT_LINK_CONTEXT: u8 = 0x4f;
pub const OMENCHAT_RESOURCE_METADATA_PREFIX: &[u8] = b"omenchat-resource:";

pub const CHAT_RNS_TRANSPORT_STATUS: &str =
    "chat-client-rns uses the live-tested compatibility transport; use chat-client-rns-clean only for reticulum-rs link/resource migration testing";

pub trait ChatLinkTransport {
    fn send_frame(&mut self, frame_bytes: Vec<u8>) -> anyhow::Result<()>;
    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>>;
    fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>>;
    fn send_resource(&mut self, _resource_id: &str, _payload: Vec<u8>) -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "OMENchat transport does not support outgoing resources"
        ))
    }
    fn defer_resource_offer(
        &mut self,
        _resource_id: &str,
        _frame_bytes: Vec<u8>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedChatTransport {
    pub sent_frames: Vec<Vec<u8>>,
    pub incoming_frames: VecDeque<Vec<u8>>,
    pub resources: BTreeMap<String, Vec<u8>>,
    pub sent_resources: BTreeMap<String, Vec<u8>>,
    pub pending_resource_offers: BTreeMap<String, VecDeque<Vec<u8>>>,
}

impl CapturedChatTransport {
    pub fn push_incoming_frame(&mut self, frame: &Frame) -> anyhow::Result<()> {
        self.incoming_frames.push_back(encode_frame(frame)?);
        Ok(())
    }

    pub fn insert_resource(&mut self, resource_id: String, payload: Vec<u8>) {
        self.resources.insert(resource_id.clone(), payload);
        replay_pending_resource_offers(
            &mut self.incoming_frames,
            &mut self.pending_resource_offers,
            &resource_id,
        );
    }
}

impl ChatLinkTransport for CapturedChatTransport {
    fn send_frame(&mut self, frame_bytes: Vec<u8>) -> anyhow::Result<()> {
        self.sent_frames.push(frame_bytes);
        Ok(())
    }

    fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.incoming_frames.pop_front())
    }

    fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.resources.get(resource_id).cloned())
    }

    fn send_resource(&mut self, resource_id: &str, payload: Vec<u8>) -> anyhow::Result<()> {
        self.sent_resources.insert(resource_id.to_owned(), payload);
        Ok(())
    }

    fn defer_resource_offer(
        &mut self,
        resource_id: &str,
        frame_bytes: Vec<u8>,
    ) -> anyhow::Result<()> {
        self.pending_resource_offers
            .entry(resource_id.to_owned())
            .or_default()
            .push_back(frame_bytes);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChatLinkEvent {
    Frame(Frame),
    InlineBatch {
        op: ChatOp,
        room_id: Option<u32>,
        values: Vec<FrameValue>,
    },
    ResourceBatch {
        op: ChatOp,
        room_id: Option<u32>,
        offer: ResourceOffer,
        values: Vec<FrameValue>,
    },
    UploadResource {
        room_id: Option<u32>,
        resource_id: String,
        filename: String,
        content_type: Option<String>,
        data: Vec<u8>,
    },
}

pub fn send_chat_frame<T: ChatLinkTransport>(
    transport: &mut T,
    frame: &Frame,
) -> anyhow::Result<()> {
    transport.send_frame(encode_frame(frame)?)?;
    Ok(())
}

pub fn recv_chat_event<T: ChatLinkTransport>(
    transport: &mut T,
) -> anyhow::Result<Option<ChatLinkEvent>> {
    let Some(bytes) = transport.recv_frame()? else {
        return Ok(None);
    };
    let frame = decode_frame(&bytes)?;
    match frame.op {
        ChatOp::HistoryInline
        | ChatOp::UserListSnapshotInline
        | ChatOp::ReactionSnapshotInline
        | ChatOp::MessageRevisionSnapshotInline
        | ChatOp::ModerationAuditInline
        | ChatOp::PinSnapshot => {
            let values = decode_compressed_values_body(&frame.body)?;
            Ok(Some(ChatLinkEvent::InlineBatch {
                op: frame.op,
                room_id: frame.room_id,
                values,
            }))
        }
        ChatOp::HistoryResourceOffer
        | ChatOp::UserListSnapshotResource
        | ChatOp::ReactionSnapshotResource
        | ChatOp::MessageRevisionSnapshotResource
        | ChatOp::ModerationAuditResource => {
            let offer = decode_resource_offer_body(&frame.body)?;
            validate_resource_offer(&offer, frame.op)?;
            let Some(payload) = transport.fetch_resource(&offer.resource_id)? else {
                transport.defer_resource_offer(&offer.resource_id, bytes)?;
                return Ok(None);
            };
            let values = decode_resource_batch_payload(&offer, &payload)?;
            Ok(Some(ChatLinkEvent::ResourceBatch {
                op: frame.op,
                room_id: frame.room_id,
                offer,
                values,
            }))
        }
        ChatOp::UploadResourceOffer => {
            let Some((resource_id, filename, content_type)) = decode_upload_resource_offer(&frame)
            else {
                return Ok(Some(ChatLinkEvent::Frame(frame)));
            };
            let Some(data) = transport.fetch_resource(&resource_id)? else {
                transport.defer_resource_offer(&resource_id, bytes)?;
                return Ok(None);
            };
            Ok(Some(ChatLinkEvent::UploadResource {
                room_id: frame.room_id,
                resource_id,
                filename,
                content_type,
                data,
            }))
        }
        _ => Ok(Some(ChatLinkEvent::Frame(frame))),
    }
}

fn validate_resource_offer(offer: &ResourceOffer, op: ChatOp) -> anyhow::Result<()> {
    validate_resource_offer_lengths(offer)?;
    if offer.resource_id.is_empty() || offer.resource_id.len() > CHAT_RESOURCE_ID_MAX_BYTES {
        anyhow::bail!("OMENchat resource offer id is empty or exceeds client limits");
    }
    let purpose_matches = match op {
        ChatOp::HistoryResourceOffer => matches!(offer.purpose.as_str(), "history" | "recent"),
        ChatOp::UserListSnapshotResource => offer.purpose == "userlist",
        ChatOp::ReactionSnapshotResource => offer.purpose.starts_with("reactions:"),
        ChatOp::MessageRevisionSnapshotResource => offer.purpose.starts_with("message-revisions:"),
        ChatOp::ModerationAuditResource => offer.purpose.starts_with("moderation-audit:"),
        _ => anyhow::bail!("OMENchat operation is not a batch resource offer"),
    };
    if !purpose_matches {
        anyhow::bail!("OMENchat resource offer purpose mismatch for its operation");
    }
    Ok(())
}

fn decode_upload_resource_offer(frame: &Frame) -> Option<(String, String, Option<String>)> {
    let super::protocol::FrameBody::Fields(values) = &frame.body else {
        return None;
    };
    let resource_id = frame_value_string(values.first()?)?.trim().to_owned();
    let filename = frame_value_string(values.get(1)?)?.trim().to_owned();
    let content_type = values
        .get(3)
        .and_then(frame_value_string)
        .map(str::to_owned);
    if resource_id.is_empty() || filename.is_empty() {
        return None;
    }
    Some((resource_id, filename, content_type))
}

fn frame_value_string(value: &FrameValue) -> Option<&str> {
    match value {
        FrameValue::String(value) => Some(value),
        _ => None,
    }
}

pub(crate) fn replay_pending_resource_offers(
    incoming_frames: &mut VecDeque<Vec<u8>>,
    pending_resource_offers: &mut BTreeMap<String, VecDeque<Vec<u8>>>,
    resource_id: &str,
) {
    if let Some(mut offers) = pending_resource_offers.remove(resource_id) {
        while let Some(frame) = offers.pop_back() {
            incoming_frames.push_front(frame);
        }
    }
}

pub fn resource_metadata(resource_id: &str) -> Vec<u8> {
    let mut metadata = OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec();
    metadata.extend(resource_id.as_bytes());
    metadata
}

pub fn resource_id_from_metadata(metadata: Option<&[u8]>) -> Option<String> {
    let metadata = metadata?;
    let id = metadata.strip_prefix(OMENCHAT_RESOURCE_METADATA_PREFIX)?;
    String::from_utf8(id.to_vec())
        .ok()
        .filter(|value| !value.is_empty())
}

#[cfg(all(feature = "native-rns-net", any()))]
pub mod native {
    use std::collections::{BTreeMap, VecDeque};

    use crate::error::{AppError, AppResult};
    use crate::runtime::native::rns_net::{
        RnsNetLinkData, RnsNetPageRequestClient, RnsNetResourceEvent,
    };

    use super::{
        encode_frame, replay_pending_resource_offers, resource_id_from_metadata, resource_metadata,
        ChatLinkTransport, Frame, OMENCHAT_LINK_CONTEXT,
    };

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct NativeRnsNetChatTransport {
        pub link_id: Option<[u8; 16]>,
        incoming_frames: VecDeque<Vec<u8>>,
        resources: BTreeMap<String, Vec<u8>>,
        pending_resource_offers: BTreeMap<String, VecDeque<Vec<u8>>>,
        pub ignored_link_packets: usize,
        pub ignored_resources: usize,
    }

    impl NativeRnsNetChatTransport {
        pub fn for_link(link_id: [u8; 16]) -> Self {
            Self {
                link_id: Some(link_id),
                ..Self::default()
            }
        }

        pub fn ingest_link_data(&mut self, data: RnsNetLinkData) -> bool {
            if self
                .link_id
                .is_some_and(|expected| expected != data.link_id)
                || data.context != OMENCHAT_LINK_CONTEXT
            {
                self.ignored_link_packets = self.ignored_link_packets.saturating_add(1);
                return false;
            }
            self.link_id.get_or_insert(data.link_id);
            self.incoming_frames.push_back(data.data);
            true
        }

        pub fn ingest_resource_event(&mut self, event: RnsNetResourceEvent) -> bool {
            match event {
                RnsNetResourceEvent::Received {
                    link_id,
                    data,
                    metadata,
                } => {
                    if self.link_id.is_some_and(|expected| expected != link_id) {
                        self.ignored_resources = self.ignored_resources.saturating_add(1);
                        return false;
                    }
                    let Some(resource_id) = resource_id_from_metadata(metadata.as_deref()) else {
                        self.ignored_resources = self.ignored_resources.saturating_add(1);
                        return false;
                    };
                    self.link_id.get_or_insert(link_id);
                    self.resources.insert(resource_id.clone(), data);
                    replay_pending_resource_offers(
                        &mut self.incoming_frames,
                        &mut self.pending_resource_offers,
                        &resource_id,
                    );
                    true
                }
                RnsNetResourceEvent::Completed { .. }
                | RnsNetResourceEvent::Progress { .. }
                | RnsNetResourceEvent::Failed { .. } => false,
            }
        }
    }

    impl ChatLinkTransport for NativeRnsNetChatTransport {
        fn send_frame(&mut self, _frame_bytes: Vec<u8>) -> anyhow::Result<()> {
            Err(anyhow::anyhow!(
                "native OMENchat send_on_link requires async RnsNetPageRequestClient integration"
            ))
        }

        fn recv_frame(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.incoming_frames.pop_front())
        }

        fn fetch_resource(&mut self, resource_id: &str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.resources.get(resource_id).cloned())
        }

        fn defer_resource_offer(
            &mut self,
            resource_id: &str,
            frame_bytes: Vec<u8>,
        ) -> anyhow::Result<()> {
            self.pending_resource_offers
                .entry(resource_id.to_owned())
                .or_default()
                .push_back(frame_bytes);
            Ok(())
        }
    }

    #[derive(Clone)]
    pub struct NativeRnsNetChatSender {
        client: RnsNetPageRequestClient,
        link_id: [u8; 16],
    }

    impl NativeRnsNetChatSender {
        pub fn new(client: RnsNetPageRequestClient, link_id: [u8; 16]) -> Self {
            Self { client, link_id }
        }

        pub fn link_id(&self) -> [u8; 16] {
            self.link_id
        }

        pub async fn send_frame(&self, frame: &Frame) -> AppResult<()> {
            let frame_bytes = encode_frame(frame).map_err(|error| {
                AppError::Runtime(format!("OMENchat frame encode failed: {error}"))
            })?;
            self.client
                .send_on_link(self.link_id, frame_bytes, OMENCHAT_LINK_CONTEXT)
                .await
        }

        pub async fn send_resource_payload(
            &self,
            resource_id: &str,
            payload: Vec<u8>,
        ) -> AppResult<()> {
            self.client
                .send_resource(self.link_id, payload, Some(resource_metadata(resource_id)))
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::protocol::batch::{
        compressed_values_batch, compressed_values_body, compressed_values_payload,
        resource_offer_body,
    };
    use crate::chat::protocol::{ChatOp, Frame, FrameBody, FrameValue};

    use omenchat_protocol::fixtures::v0_6_0_1;

    #[test]
    fn client_transport_sends_encoded_frames() {
        let mut transport = CapturedChatTransport::default();
        let frame = Frame::new(ChatOp::Ping, 7, None, FrameBody::Empty);

        send_chat_frame(&mut transport, &frame).expect("send frame");

        assert_eq!(transport.sent_frames.len(), 1);
        assert_eq!(
            decode_frame(&transport.sent_frames[0]).expect("decode"),
            frame
        );
    }

    #[test]
    fn client_transport_decodes_inline_and_resource_batches() {
        let values = vec![FrameValue::Array(vec![
            FrameValue::U64(1),
            FrameValue::String("hello".into()),
        ])];
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryInline,
                1,
                Some(1),
                compressed_values_body(&values).expect("inline body"),
            ))
            .expect("push inline");
        let resource_id = "history:1:test".to_owned();
        let batch = compressed_values_batch(&values).expect("resource batch");
        transport.insert_resource(
            resource_id.clone(),
            compressed_values_payload(&values).expect("resource payload"),
        );
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryResourceOffer,
                2,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id,
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: batch.uncompressed_len,
                    compressed_len: batch.bytes.len() as u64,
                    purpose: "history".into(),
                }),
            ))
            .expect("push resource");

        assert!(matches!(
            recv_chat_event(&mut transport).expect("event"),
            Some(ChatLinkEvent::InlineBatch { values: decoded, .. }) if decoded == values
        ));
        assert!(matches!(
            recv_chat_event(&mut transport).expect("event"),
            Some(ChatLinkEvent::ResourceBatch { values: decoded, .. }) if decoded == values
        ));
    }

    #[test]
    fn client_transport_decodes_reaction_inline_and_resource_snapshots() {
        let snapshot = crate::chat::protocol::ReactionSnapshot {
            target_event_ids: vec![10],
            entries: vec![crate::chat::protocol::ReactionSnapshotEntry {
                target_event_id: 10,
                actor_user_id: 7,
                token: crate::chat::protocol::ReactionToken::Heart,
                created_at_unix: 2,
            }],
        };
        let FrameBody::Fields(values) = snapshot.into_frame_body().expect("snapshot") else {
            panic!("snapshot fields");
        };
        let batch = compressed_values_batch(&values).expect("batch");
        let resource_id = "reactions:1:test".to_owned();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ReactionSnapshotInline,
                1,
                Some(1),
                compressed_values_body(&values).expect("inline body"),
            ))
            .expect("push inline");
        transport.insert_resource(
            resource_id.clone(),
            compressed_values_payload(&values).expect("payload"),
        );
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ReactionSnapshotResource,
                2,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id,
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: batch.uncompressed_len,
                    compressed_len: batch.bytes.len() as u64,
                    purpose: "reactions:2:fixture".into(),
                }),
            ))
            .expect("push resource");

        assert!(matches!(
            recv_chat_event(&mut transport).expect("inline"),
            Some(ChatLinkEvent::InlineBatch {
                op: ChatOp::ReactionSnapshotInline,
                values: decoded,
                ..
            }) if decoded == values
        ));
        assert!(matches!(
            recv_chat_event(&mut transport).expect("resource"),
            Some(ChatLinkEvent::ResourceBatch {
                op: ChatOp::ReactionSnapshotResource,
                values: decoded,
                ..
            }) if decoded == values
        ));
    }

    #[test]
    fn client_transport_decodes_moderation_audit_inline_and_resource_pages() {
        let page = crate::chat::protocol::ModerationAuditPage {
            records: vec![crate::chat::protocol::ModerationAuditRecord {
                audit_id: 9,
                room_id: 1,
                actor_user_id: 2,
                actor_display_name_at_action: "Moderator".into(),
                target_user_id: Some(3),
                target_display_name_at_action: Some("Member".into()),
                action: crate::chat::protocol::ModerationAuditAction::Mute,
                committed_at_unix: 4,
                result_role_bits: None,
                result_status_bits: Some(2),
            }],
        };
        let values = page.into_frame_values().expect("page values");
        let batch = compressed_values_batch(&values).expect("batch");
        let resource_id = "moderation-audit:2:newest".to_owned();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ModerationAuditInline,
                1,
                Some(1),
                compressed_values_body(&values).expect("inline body"),
            ))
            .expect("push inline");
        transport.insert_resource(
            resource_id.clone(),
            compressed_values_payload(&values).expect("payload"),
        );
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::ModerationAuditResource,
                2,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id,
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: batch.uncompressed_len,
                    compressed_len: batch.bytes.len() as u64,
                    purpose: "moderation-audit:2:newest".into(),
                }),
            ))
            .expect("push resource");

        assert!(matches!(
            recv_chat_event(&mut transport).expect("inline"),
            Some(ChatLinkEvent::InlineBatch {
                op: ChatOp::ModerationAuditInline,
                values: decoded,
                ..
            }) if decoded == values
        ));
        assert!(matches!(
            recv_chat_event(&mut transport).expect("resource"),
            Some(ChatLinkEvent::ResourceBatch {
                op: ChatOp::ModerationAuditResource,
                values: decoded,
                ..
            }) if decoded == values
        ));
    }

    #[test]
    fn client_transport_decodes_dormant_message_revision_snapshots() {
        let snapshot = crate::chat::protocol::MessageRevisionSnapshot {
            target_event_ids: vec![10],
            entries: vec![crate::chat::protocol::MessageRevisionSnapshotEntry {
                target_event_id: 10,
                latest_revision_event_id: 20,
                action: crate::chat::protocol::MessageRevisionAction::Correct,
                actor_user_id: 7,
                at_unix: 2,
                replacement: Some("corrected".into()),
                revision_number: 1,
            }],
        };
        let FrameBody::Fields(values) = snapshot.into_frame_body().expect("snapshot") else {
            panic!("snapshot fields");
        };
        let batch = compressed_values_batch(&values).expect("batch");
        let resource_id = "message-revisions:1:test".to_owned();
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageRevisionSnapshotInline,
                1,
                Some(1),
                compressed_values_body(&values).expect("inline body"),
            ))
            .expect("push inline");
        transport.insert_resource(
            resource_id.clone(),
            compressed_values_payload(&values).expect("payload"),
        );
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::MessageRevisionSnapshotResource,
                2,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id,
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: batch.uncompressed_len,
                    compressed_len: batch.bytes.len() as u64,
                    purpose: "message-revisions:2:fixture".into(),
                }),
            ))
            .expect("push resource");

        assert!(matches!(
            recv_chat_event(&mut transport).expect("inline"),
            Some(ChatLinkEvent::InlineBatch {
                op: ChatOp::MessageRevisionSnapshotInline,
                values: decoded,
                ..
            }) if decoded == values
        ));
        assert!(matches!(
            recv_chat_event(&mut transport).expect("resource"),
            Some(ChatLinkEvent::ResourceBatch {
                op: ChatOp::MessageRevisionSnapshotResource,
                values: decoded,
                ..
            }) if decoded == values
        ));
    }

    #[test]
    fn client_transport_decodes_dormant_pin_snapshot_inline() {
        let snapshot = crate::chat::protocol::PinSnapshot {
            target_event_ids: vec![10],
            entries: vec![crate::chat::protocol::PinSnapshotEntry {
                target_event_id: 10,
                pin_event_id: 20,
                actor_user_id: 7,
                pinned_at_unix: 2,
            }],
        };
        let FrameBody::Fields(values) = snapshot.into_frame_body().expect("snapshot") else {
            panic!("snapshot fields");
        };
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::PinSnapshot,
                1,
                Some(1),
                compressed_values_body(&values).expect("inline body"),
            ))
            .expect("push inline");

        assert!(matches!(
            recv_chat_event(&mut transport).expect("inline"),
            Some(ChatLinkEvent::InlineBatch {
                op: ChatOp::PinSnapshot,
                values: decoded,
                ..
            }) if decoded == values
        ));
    }

    #[test]
    fn resource_offer_is_replayed_after_delayed_resource_arrives() {
        let values = vec![FrameValue::String("late history".into())];
        let resource_id = "history:1:late".to_owned();
        let batch = compressed_values_batch(&values).expect("resource batch");
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryResourceOffer,
                1,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id: resource_id.clone(),
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: batch.uncompressed_len,
                    compressed_len: batch.bytes.len() as u64,
                    purpose: "history".into(),
                }),
            ))
            .expect("push resource offer");

        assert!(recv_chat_event(&mut transport)
            .expect("missing resource is incomplete")
            .is_none());
        assert_eq!(
            transport
                .pending_resource_offers
                .get(&resource_id)
                .map(VecDeque::len),
            Some(1)
        );

        transport.insert_resource(
            resource_id.clone(),
            compressed_values_payload(&values).expect("resource payload"),
        );

        assert!(transport
            .pending_resource_offers
            .get(&resource_id)
            .is_none_or(VecDeque::is_empty));
        assert!(matches!(
            recv_chat_event(&mut transport).expect("replayed event"),
            Some(ChatLinkEvent::ResourceBatch { values: decoded, .. }) if decoded == values
        ));
    }

    #[test]
    fn invalid_resource_offer_is_rejected_before_pending_retention() {
        let mut transport = CapturedChatTransport::default();
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryResourceOffer,
                1,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id: "history:1:invalid".into(),
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: 1,
                    compressed_len: 1,
                    purpose: "userlist".into(),
                }),
            ))
            .expect("push invalid offer");

        let error = recv_chat_event(&mut transport).expect_err("purpose mismatch");

        assert!(error.to_string().contains("purpose mismatch"));
        assert!(transport.pending_resource_offers.is_empty());
    }

    #[test]
    fn recent_history_resource_purpose_matches_server_sync_contract() {
        let values = vec![FrameValue::String("recent history".into())];
        let batch = compressed_values_batch(&values).expect("resource batch");
        let resource_id = "recent:1:fixture".to_owned();
        let mut transport = CapturedChatTransport::default();
        transport.insert_resource(
            resource_id.clone(),
            compressed_values_payload(&values).expect("resource payload"),
        );
        transport
            .push_incoming_frame(&Frame::new(
                ChatOp::HistoryResourceOffer,
                1,
                Some(1),
                resource_offer_body(&ResourceOffer {
                    resource_id,
                    compression: super::super::protocol::Compression::Bzip2,
                    uncompressed_len: batch.uncompressed_len,
                    compressed_len: batch.bytes.len() as u64,
                    purpose: "recent".into(),
                }),
            ))
            .expect("push recent offer");

        assert!(matches!(
            recv_chat_event(&mut transport).expect("recent history resource"),
            Some(ChatLinkEvent::ResourceBatch {
                op: ChatOp::HistoryResourceOffer,
                values: decoded,
                ..
            }) if decoded == values
        ));
    }

    #[test]
    fn resource_payload_must_match_offer_compression_and_lengths() {
        let values = vec![FrameValue::String("bound history".into())];
        let batch = compressed_values_batch(&values).expect("resource batch");
        let payload = compressed_values_payload(&values).expect("resource payload");
        let offers = [
            ResourceOffer {
                resource_id: "history:1:compression".into(),
                compression: super::super::protocol::Compression::None,
                uncompressed_len: batch.uncompressed_len,
                compressed_len: batch.bytes.len() as u64,
                purpose: "history".into(),
            },
            ResourceOffer {
                resource_id: "history:1:uncompressed".into(),
                compression: super::super::protocol::Compression::Bzip2,
                uncompressed_len: batch.uncompressed_len + 1,
                compressed_len: batch.bytes.len() as u64,
                purpose: "history".into(),
            },
            ResourceOffer {
                resource_id: "history:1:compressed".into(),
                compression: super::super::protocol::Compression::Bzip2,
                uncompressed_len: batch.uncompressed_len,
                compressed_len: batch.bytes.len() as u64 + 1,
                purpose: "history".into(),
            },
        ];

        for offer in offers {
            let mut transport = CapturedChatTransport::default();
            transport.insert_resource(offer.resource_id.clone(), payload.clone());
            transport
                .push_incoming_frame(&Frame::new(
                    ChatOp::HistoryResourceOffer,
                    2,
                    Some(1),
                    resource_offer_body(&offer),
                ))
                .expect("push offer");

            let error = recv_chat_event(&mut transport).expect_err("metadata mismatch");
            assert!(error.to_string().contains("mismatch"), "{error}");
        }
    }

    #[test]
    fn resource_metadata_round_trips_resource_id() {
        let metadata = resource_metadata("history:1:abc");

        assert_eq!(v0_6_0_1::PROTOCOL_VERSION, 1);
        assert_eq!(v0_6_0_1::PROTOCOL_NAME, "omenchat-v0.1");
        assert!(!v0_6_0_1::SESSION_OPEN.is_empty());
        assert!(!v0_6_0_1::ROOM_MESSAGE.is_empty());
        assert!(!v0_6_0_1::HISTORY_RESOURCE_OFFER.is_empty());
        assert_eq!(OMENCHAT_LINK_CONTEXT, v0_6_0_1::LINK_CONTEXT);
        assert!(metadata.starts_with(v0_6_0_1::RESOURCE_METADATA_PREFIX));
        assert_eq!(
            resource_id_from_metadata(Some(&metadata)).as_deref(),
            Some("history:1:abc")
        );
        assert_eq!(resource_id_from_metadata(Some(b"other:abc")), None);
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn native_adapter_filters_link_context_and_resources() {
        use crate::runtime::native::rns_net::{RnsNetLinkData, RnsNetResourceEvent};

        let link_id = [9u8; 16];
        let mut adapter = native::NativeRnsNetChatTransport::for_link(link_id);
        let frame = Frame::new(ChatOp::Ping, 1, None, FrameBody::Empty);

        assert!(!adapter.ingest_link_data(RnsNetLinkData {
            link_id,
            context: 0,
            data: encode_frame(&frame).expect("encode frame"),
        }));
        assert!(adapter.ingest_link_data(RnsNetLinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&frame).expect("encode frame"),
        }));
        assert!(matches!(
            recv_chat_event(&mut adapter).expect("event"),
            Some(ChatLinkEvent::Frame(decoded)) if decoded == frame
        ));

        assert!(
            adapter.ingest_resource_event(RnsNetResourceEvent::Received {
                link_id,
                data: compressed_values_payload(&[FrameValue::String("payload".into())])
                    .expect("resource payload"),
                metadata: Some(resource_metadata("history:1")),
            })
        );
        assert!(adapter
            .fetch_resource("history:1")
            .expect("resource lookup")
            .is_some());
    }
}
