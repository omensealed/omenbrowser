use crate::error::ServerResult;
use crate::protocol::batch::decode_resource_offer_body;
use crate::protocol::codec::{decode_frame, encode_frame};
use crate::protocol::{ChatErrorCode, ChatOp, Frame, FrameBody, FrameValue};
use crate::session::{ServerPeer, SessionEngine};

pub type LinkId = [u8; 16];

pub const OMENCHAT_LINK_CONTEXT: u8 = 0x4f;
pub const OMENCHAT_RESOURCE_METADATA_PREFIX: &[u8] = b"omenchat-resource:";

pub trait OmenchatTransport {
    fn send_frame(&mut self, link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()>;
    fn send_frame_with_context(
        &mut self,
        link_id: LinkId,
        frame_bytes: Vec<u8>,
        _context: u8,
    ) -> ServerResult<()> {
        self.send_frame(link_id, frame_bytes)
    }
    fn offer_resource(
        &mut self,
        link_id: LinkId,
        resource_id: String,
        payload: Vec<u8>,
        metadata: Vec<u8>,
    ) -> ServerResult<()>;
    fn sent_frame_count(&self) -> u64;
    fn offered_resource_count(&self) -> u64;
    fn sent_frame_bytes(&self) -> u64;
    fn offered_resource_bytes(&self) -> u64;
    fn close_link(&mut self, _link_id: LinkId) -> ServerResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedTransport {
    pub frames: Vec<CapturedFrame>,
    pub resources: Vec<CapturedResource>,
    pub closed_links: Vec<LinkId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedFrame {
    pub link_id: LinkId,
    pub bytes: Vec<u8>,
    pub context: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedResource {
    pub link_id: LinkId,
    pub resource_id: String,
    pub payload: Vec<u8>,
    pub metadata: Vec<u8>,
}

impl OmenchatTransport for CapturedTransport {
    fn send_frame(&mut self, link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
        self.frames.push(CapturedFrame {
            link_id,
            bytes: frame_bytes,
            context: OMENCHAT_LINK_CONTEXT,
        });
        Ok(())
    }

    fn send_frame_with_context(
        &mut self,
        link_id: LinkId,
        frame_bytes: Vec<u8>,
        context: u8,
    ) -> ServerResult<()> {
        self.frames.push(CapturedFrame {
            link_id,
            bytes: frame_bytes,
            context,
        });
        Ok(())
    }

    fn offer_resource(
        &mut self,
        link_id: LinkId,
        resource_id: String,
        payload: Vec<u8>,
        metadata: Vec<u8>,
    ) -> ServerResult<()> {
        self.resources.push(CapturedResource {
            link_id,
            resource_id,
            payload,
            metadata,
        });
        Ok(())
    }

    fn sent_frame_count(&self) -> u64 {
        self.frames.len() as u64
    }

    fn offered_resource_count(&self) -> u64 {
        self.resources.len() as u64
    }

    fn sent_frame_bytes(&self) -> u64 {
        self.frames
            .iter()
            .map(|frame| frame.bytes.len() as u64)
            .sum()
    }

    fn offered_resource_bytes(&self) -> u64 {
        self.resources
            .iter()
            .map(|resource| resource.payload.len() as u64)
            .sum()
    }

    fn close_link(&mut self, link_id: LinkId) -> ServerResult<()> {
        self.closed_links.push(link_id);
        Ok(())
    }
}

pub fn resource_metadata(resource_id: &str) -> Vec<u8> {
    let mut metadata = OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec();
    metadata.extend(resource_id.as_bytes());
    metadata
}

pub fn handle_link_frame<T: OmenchatTransport>(
    engine: &SessionEngine,
    link_id: LinkId,
    peer: &ServerPeer,
    frame_bytes: &[u8],
    transport: &mut T,
) -> ServerResult<()> {
    handle_link_frame_with_active_peers(engine, link_id, peer, frame_bytes, transport, &[])
}

pub fn handle_link_frame_with_active_peers<T: OmenchatTransport>(
    engine: &SessionEngine,
    link_id: LinkId,
    peer: &ServerPeer,
    frame_bytes: &[u8],
    transport: &mut T,
    active_room_peers: &[ServerPeer],
) -> ServerResult<()> {
    let frame = decode_frame(frame_bytes).map_err(|error| {
        crate::error::ServerError::Message(format!("OMENchat frame decode failed: {error}"))
    })?;
    let responses = engine.handle_frame_with_active_peers(peer, frame, active_room_peers)?;
    let send_result = responses
        .iter()
        .try_for_each(|response| send_response_frame(engine, link_id, response, transport));
    for response in &responses {
        release_response_resource(engine, response)?;
    }
    send_result
}

pub fn send_response_frame<T: OmenchatTransport>(
    engine: &SessionEngine,
    link_id: LinkId,
    response: &Frame,
    transport: &mut T,
) -> ServerResult<()> {
    send_preflighted_response(engine, link_id, response, transport, None)
}

pub fn send_response_frame_with_context<T: OmenchatTransport>(
    engine: &SessionEngine,
    link_id: LinkId,
    response: &Frame,
    transport: &mut T,
    context: u8,
) -> ServerResult<()> {
    send_preflighted_response(engine, link_id, response, transport, Some(context))
}

struct PreparedResponseResource {
    resource_id: String,
    payload: Vec<u8>,
    metadata: Vec<u8>,
}

fn send_preflighted_response<T: OmenchatTransport>(
    engine: &SessionEngine,
    link_id: LinkId,
    response: &Frame,
    transport: &mut T,
    context: Option<u8>,
) -> ServerResult<()> {
    if !response_carries_resource(response.op) {
        return send_encoded_frame(link_id, response, transport, context);
    }

    let Some(prepared) = prepare_response_resource(engine, response)? else {
        return send_resource_unavailable(link_id, response, transport, context);
    };
    send_encoded_frame(link_id, response, transport, context)?;
    transport.offer_resource(
        link_id,
        prepared.resource_id,
        prepared.payload,
        prepared.metadata,
    )
}

fn prepare_response_resource(
    engine: &SessionEngine,
    response: &Frame,
) -> ServerResult<Option<PreparedResponseResource>> {
    let resource_id = if response.op == ChatOp::UploadResourceOffer {
        upload_resource_id_from_offer(response)
    } else {
        Some(
            decode_resource_offer_body(&response.body)
                .map_err(|error| {
                    crate::error::ServerError::Message(format!(
                        "OMENchat resource offer decode failed: {error}"
                    ))
                })?
                .resource_id,
        )
    };
    let Some(resource_id) = resource_id else {
        return Ok(None);
    };
    let Some(payload) = engine.resource_payload(&resource_id)? else {
        return Ok(None);
    };
    let metadata = resource_metadata(&resource_id);
    Ok(Some(PreparedResponseResource {
        resource_id,
        payload,
        metadata,
    }))
}

fn send_encoded_frame<T: OmenchatTransport>(
    link_id: LinkId,
    frame: &Frame,
    transport: &mut T,
    context: Option<u8>,
) -> ServerResult<()> {
    let encoded = encode_frame(frame).map_err(|error| {
        crate::error::ServerError::Message(format!("OMENchat frame encode failed: {error}"))
    })?;
    if let Some(context) = context {
        transport.send_frame_with_context(link_id, encoded, context)
    } else {
        transport.send_frame(link_id, encoded)
    }
}

fn send_resource_unavailable<T: OmenchatTransport>(
    link_id: LinkId,
    response: &Frame,
    transport: &mut T,
    context: Option<u8>,
) -> ServerResult<()> {
    let error = Frame::new(
        ChatOp::Error,
        response.seq,
        response.room_id,
        FrameBody::Fields(vec![
            FrameValue::U64(ChatErrorCode::ResourceUnavailable as u16 as u64),
            FrameValue::String(
                "Resource unavailable on the current Reticulum compatibility train".into(),
            ),
        ]),
    );
    send_encoded_frame(link_id, &error, transport, context)
}

fn response_carries_resource(op: ChatOp) -> bool {
    matches!(
        op,
        ChatOp::HistoryResourceOffer
            | ChatOp::UserListSnapshotResource
            | ChatOp::ReactionSnapshotResource
            | ChatOp::MessageRevisionSnapshotResource
            | ChatOp::ModerationAuditResource
            | ChatOp::UploadResourceOffer
    )
}

pub(crate) fn release_response_resource(
    engine: &SessionEngine,
    response: &Frame,
) -> ServerResult<()> {
    let resource_id = match response.op {
        ChatOp::UploadResourceOffer => upload_resource_id_from_offer(response),
        ChatOp::HistoryResourceOffer
        | ChatOp::UserListSnapshotResource
        | ChatOp::ReactionSnapshotResource
        | ChatOp::MessageRevisionSnapshotResource
        | ChatOp::ModerationAuditResource => Some(
            decode_resource_offer_body(&response.body)
                .map_err(|error| {
                    crate::error::ServerError::Message(format!(
                        "OMENchat resource release decode failed: {error}"
                    ))
                })?
                .resource_id,
        ),
        _ => None,
    };
    if let Some(resource_id) = resource_id {
        let _ = engine.take_resource_payload(&resource_id)?;
    }
    Ok(())
}

fn upload_resource_id_from_offer(response: &Frame) -> Option<String> {
    let crate::protocol::FrameBody::Fields(values) = &response.body else {
        return None;
    };
    match values.first()? {
        crate::protocol::FrameValue::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_owned())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ServerError;

    mod v0_6_0_1 {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/omenchat/v0_6_0_1_wire.rs"
        ));
    }

    #[test]
    fn transport_labels_match_v0_6_0_1_wire_contract() {
        assert_eq!(v0_6_0_1::PROTOCOL_VERSION, 1);
        assert_eq!(v0_6_0_1::PROTOCOL_NAME, "omenchat-v0.1");
        assert!(!v0_6_0_1::SESSION_OPEN.is_empty());
        assert!(!v0_6_0_1::ROOM_MESSAGE.is_empty());
        assert!(!v0_6_0_1::HISTORY_RESOURCE_OFFER.is_empty());
        assert_eq!(OMENCHAT_LINK_CONTEXT, v0_6_0_1::LINK_CONTEXT);
        assert_eq!(
            OMENCHAT_RESOURCE_METADATA_PREFIX,
            v0_6_0_1::RESOURCE_METADATA_PREFIX
        );
        assert_eq!(
            resource_metadata("history:7:fixture"),
            b"omenchat-resource:history:7:fixture"
        );
    }

    #[test]
    fn moderation_audit_resource_uses_the_payload_bridge() {
        assert!(response_carries_resource(ChatOp::ModerationAuditResource));
        assert!(!response_carries_resource(ChatOp::ModerationAuditInline));
    }
    use crate::protocol::batch::decode_compressed_values_payload;
    use crate::protocol::codec::encode_frame;
    use crate::protocol::{ChatOp, Frame, FrameBody};
    use crate::session::{ServerPeer, SessionLimits};
    use crate::store::{OmenchatStore, ServerRoomEventKind};

    fn peer() -> ServerPeer {
        ServerPeer {
            identity_hash: b"peer-a".to_vec(),
            display_name: "Alice".into(),
            lxmf_destination: Some("lxmf-a".into()),
        }
    }

    struct RejectingTransport;

    impl OmenchatTransport for RejectingTransport {
        fn send_frame(&mut self, _link_id: LinkId, _frame_bytes: Vec<u8>) -> ServerResult<()> {
            Err(ServerError::Message("injected frame rejection".into()))
        }

        fn offer_resource(
            &mut self,
            _link_id: LinkId,
            _resource_id: String,
            _payload: Vec<u8>,
            _metadata: Vec<u8>,
        ) -> ServerResult<()> {
            Err(ServerError::Message("injected resource rejection".into()))
        }

        fn sent_frame_count(&self) -> u64 {
            0
        }

        fn offered_resource_count(&self) -> u64 {
            0
        }

        fn sent_frame_bytes(&self) -> u64 {
            0
        }

        fn offered_resource_bytes(&self) -> u64 {
            0
        }
    }

    #[derive(Default)]
    struct ResourceRejectingTransport {
        captured: CapturedTransport,
        offer_attempts: usize,
    }

    impl OmenchatTransport for ResourceRejectingTransport {
        fn send_frame(&mut self, link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
            self.captured.send_frame(link_id, frame_bytes)
        }

        fn send_frame_with_context(
            &mut self,
            link_id: LinkId,
            frame_bytes: Vec<u8>,
            context: u8,
        ) -> ServerResult<()> {
            self.captured
                .send_frame_with_context(link_id, frame_bytes, context)
        }

        fn offer_resource(
            &mut self,
            _link_id: LinkId,
            _resource_id: String,
            _payload: Vec<u8>,
            _metadata: Vec<u8>,
        ) -> ServerResult<()> {
            self.offer_attempts = self.offer_attempts.saturating_add(1);
            Err(ServerError::Message("injected resource rejection".into()))
        }

        fn sent_frame_count(&self) -> u64 {
            self.captured.sent_frame_count()
        }

        fn offered_resource_count(&self) -> u64 {
            0
        }

        fn sent_frame_bytes(&self) -> u64 {
            self.captured.sent_frame_bytes()
        }

        fn offered_resource_bytes(&self) -> u64 {
            0
        }
    }

    fn test_resource_offer(resource_id: String, payload_len: usize) -> Frame {
        use crate::protocol::batch::{resource_offer_body, ResourceOffer};
        use crate::protocol::Compression;

        Frame::new(
            ChatOp::HistoryResourceOffer,
            99,
            Some(1),
            resource_offer_body(&ResourceOffer {
                resource_id,
                compression: Compression::None,
                uncompressed_len: payload_len as u64,
                compressed_len: payload_len as u64,
                purpose: "history".into(),
            }),
        )
    }

    #[test]
    fn link_bridge_emits_frames_and_resource_payloads() {
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
                    body: "transport resource payload".repeat(64),
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
        let request = encode_frame(&Frame::new(
            ChatOp::JoinRoom,
            1,
            None,
            FrameBody::Text("lobby".into()),
        ))
        .expect("encode request");
        let mut transport = CapturedTransport::default();

        let link_id = [3u8; 16];
        handle_link_frame(&engine, link_id, &peer(), &request, &mut transport)
            .expect("handle link frame");

        assert_eq!(transport.frames.len(), 3);
        assert_eq!(transport.resources.len(), 2);
        assert!(transport
            .frames
            .iter()
            .all(|frame| frame.link_id == link_id));
        assert!(transport
            .resources
            .iter()
            .all(|resource| resource.link_id == link_id));
        assert_eq!(
            transport.resources[1].metadata,
            resource_metadata(&transport.resources[1].resource_id)
        );
        let decoded = decode_compressed_values_payload(&transport.resources[1].payload)
            .expect("decode resource payload");
        assert_eq!(decoded.len(), 1);
        for resource in &transport.resources {
            assert!(engine
                .resource_payload(&resource.resource_id)
                .expect("resource lookup")
                .is_none());
        }
    }

    #[test]
    fn link_bridge_releases_generated_resources_when_transport_rejects() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .room_by_name("lobby")
            .expect("room query")
            .expect("room");
        let user = store.ensure_user(b"seed", "Seed", None).expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "rejected resource payload".repeat(64),
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
        let request = encode_frame(&Frame::new(
            ChatOp::JoinRoom,
            1,
            None,
            FrameBody::Text("lobby".into()),
        ))
        .expect("encode request");

        let error = handle_link_frame(
            &engine,
            [4u8; 16],
            &peer(),
            &request,
            &mut RejectingTransport,
        )
        .expect_err("transport rejection");

        assert!(error.to_string().contains("injected frame rejection"));
        assert_eq!(
            engine
                .pending_resource_metrics()
                .expect("pending resource metrics"),
            (0, 0, 0)
        );
    }

    #[test]
    fn resource_above_legacy_split_boundary_dispatches_once() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let resource_id = "history:split-exposure".to_owned();
        let metadata = resource_metadata(&resource_id);
        let payload_len = 1_048_575usize
            .checked_sub(3 + metadata.len())
            .expect("metadata fits")
            + 1;
        engine
            .store_test_pending_resource(resource_id.clone(), vec![0x5a; payload_len])
            .expect("retain exposure payload");
        let frame = test_resource_offer(resource_id, payload_len);
        let mut transport = CapturedTransport::default();

        send_response_frame(&engine, [0x99; 16], &frame, &mut transport)
            .expect("split Resource dispatch");

        assert_eq!(transport.resources.len(), 1);
        assert_eq!(transport.resources[0].payload.len(), payload_len);
        assert_eq!(transport.frames.len(), 1);
        let returned = decode_frame(&transport.frames[0].bytes).expect("decode returned frame");
        assert_eq!(returned.op, ChatOp::HistoryResourceOffer);
        assert_eq!(
            engine
                .pending_resource_metrics()
                .expect("pending resource metrics"),
            (1, payload_len, 0),
            "payload remains retained until terminal cleanup"
        );
        release_response_resource(&engine, &frame).expect("release split payload");
    }

    #[test]
    fn exact_boundary_resource_dispatches_once_with_context() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let resource_id = "history:exact-boundary".to_owned();
        let metadata = resource_metadata(&resource_id);
        // Keep this test available in the storage-only feature closure used by
        // the crash-recovery lane, where the optional transport crate is absent.
        let payload_len = 1_048_575usize
            .checked_sub(3 + metadata.len())
            .expect("metadata fits");
        engine
            .store_test_pending_resource(resource_id.clone(), vec![0x33; payload_len])
            .expect("retain boundary payload");
        let frame = test_resource_offer(resource_id, payload_len);
        let mut transport = CapturedTransport::default();

        send_response_frame_with_context(&engine, [0x42; 16], &frame, &mut transport, 0x7a)
            .expect("dispatch exact boundary");

        assert_eq!(transport.frames.len(), 1);
        assert_eq!(transport.frames[0].context, 0x7a);
        assert_eq!(transport.resources.len(), 1);
        assert_eq!(transport.resources[0].payload.len(), payload_len);
        assert_eq!(
            engine
                .pending_resource_metrics()
                .expect("pending resource metrics"),
            (1, payload_len, 0)
        );
        release_response_resource(&engine, &frame).expect("release boundary payload");
        assert_eq!(
            engine
                .pending_resource_metrics()
                .expect("released pending resource metrics"),
            (0, 0, 0)
        );
    }

    #[test]
    fn resource_dispatch_failure_does_not_retain_or_retry_payload() {
        let engine = SessionEngine::new(OmenchatStore::in_memory().expect("store"));
        let resource_id = "history:dispatch-failure".to_owned();
        engine
            .store_test_pending_resource(resource_id.clone(), vec![0x44; 32])
            .expect("retain payload");
        let frame = test_resource_offer(resource_id, 32);
        let mut transport = ResourceRejectingTransport::default();

        let error = send_response_frame(&engine, [0x43; 16], &frame, &mut transport)
            .expect_err("injected dispatch failure");

        assert!(error.to_string().contains("injected resource rejection"));
        assert_eq!(transport.captured.frames.len(), 1);
        assert_eq!(transport.offer_attempts, 1);
        assert_eq!(transport.offered_resource_count(), 0);
        release_response_resource(&engine, &frame).expect("release failed payload");
        assert_eq!(
            engine
                .pending_resource_metrics()
                .expect("pending resource metrics"),
            (0, 0, 0)
        );
    }
}
