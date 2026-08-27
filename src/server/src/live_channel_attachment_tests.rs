use super::*;
use crate::protocol::{
    with_session_open_negotiation, ChannelAttachmentFrame, SessionOpenNegotiation,
    CHANNEL_ATTACHMENT_CAPABILITY,
};
use crate::session::SessionLimits;
use crate::store::OmenchatStore;
use crate::transport::CapturedTransport;
use sha2::{Digest, Sha256};

fn channel_server(
    label: &str,
) -> (
    OmenchatLiveServer<CapturedTransport>,
    RoomId,
    std::path::PathBuf,
) {
    let db = std::env::temp_dir().join(format!(
        "omenchatd-channel-{label}-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let uploads = db.with_extension("uploads");
    let store = OmenchatStore::open(&db).expect("store");
    let room_id = store.ensure_room("lobby", None).expect("room").room_id;
    let engine = SessionEngine::with_limits(
        store,
        SessionLimits {
            upload_cache_root: Some(uploads.clone()),
            ..SessionLimits::default()
        },
    );
    (
        OmenchatLiveServer::new(engine, CapturedTransport::default()),
        room_id,
        uploads,
    )
}

fn open_join_and_offer(
    live: &mut OmenchatLiveServer<CapturedTransport>,
    link_id: LinkId,
    room_id: RoomId,
    bytes: u64,
) -> String {
    let test_peer = ServerPeer {
        identity_hash: b"channel-peer".to_vec(),
        display_name: "Channel Peer".into(),
        lxmf_destination: None,
    };
    live.handle_event(OmenchatLinkEvent::LinkOpened {
        link_id,
        peer: test_peer,
    })
    .expect("open");
    let open = with_session_open_negotiation(
        FrameBody::Text("Channel Client".into()),
        &SessionOpenNegotiation {
            requested_capabilities: vec![CHANNEL_ATTACHMENT_CAPABILITY.into()],
            client_instance_id: None,
        },
    )
    .expect("negotiation");
    for frame in [
        Frame::new(ChatOp::SessionOpen, 1, None, open),
        Frame::new(ChatOp::JoinRoom, 2, None, FrameBody::Text("lobby".into())),
        Frame::new(
            ChatOp::UploadOffer,
            3,
            Some(room_id),
            FrameBody::Fields(vec![
                FrameValue::String("channel.bin".into()),
                FrameValue::U64(bytes),
                FrameValue::String("application/octet-stream".into()),
            ]),
        ),
    ] {
        live.handle_event(OmenchatLinkEvent::LinkData {
            link_id,
            context: OMENCHAT_LINK_CONTEXT,
            data: encode_frame(&frame).expect("encode"),
        })
        .expect("frame");
    }
    live.transport()
        .frames
        .iter()
        .rev()
        .filter_map(|captured| decode_frame(&captured.bytes).ok())
        .find_map(|frame| {
            if frame.op != ChatOp::UploadAccept {
                return None;
            }
            let FrameBody::Fields(fields) = frame.body else {
                return None;
            };
            match fields.first() {
                Some(FrameValue::String(resource_id)) => Some(resource_id.clone()),
                _ => None,
            }
        })
        .expect("accepted resource id")
}

fn channel_event(link_id: LinkId, frame: ChannelAttachmentFrame) -> OmenchatLinkEvent {
    OmenchatLinkEvent::ChannelAttachmentData {
        link_id,
        data: frame.encode(512).expect("channel frame"),
    }
}

#[test]
fn negotiated_channel_upload_commits_without_resource_dispatch() {
    let (mut live, room_id, uploads) = channel_server("channel-commit");
    let link_id = [0xc1; 16];
    let payload = b"channel attachment proof".to_vec();
    let resource_id = open_join_and_offer(&mut live, link_id, room_id, payload.len() as u64);
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Start {
            resource_id: resource_id.clone(),
            total_bytes: payload.len() as u64,
        },
    ))
    .expect("start");
    for (offset, chunk) in payload.chunks(5).enumerate() {
        live.handle_event(channel_event(
            link_id,
            ChannelAttachmentFrame::Data {
                resource_id: resource_id.clone(),
                offset: (offset * 5) as u64,
                bytes: chunk.to_vec(),
            },
        ))
        .expect("data");
    }
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Finish {
            resource_id,
            total_bytes: payload.len() as u64,
            digest: Sha256::digest(&payload).into(),
        },
    ))
    .expect("finish");
    assert!(
        live.transport().resources.is_empty(),
        "Channel path must not dispatch Resource"
    );
    assert!(live
        .transport()
        .frames
        .iter()
        .filter_map(|frame| decode_frame(&frame.bytes).ok())
        .any(|frame| frame.op == ChatOp::UploadComplete));
    let stored = std::fs::read_dir(&uploads)
        .expect("upload root")
        .flat_map(|entry| std::fs::read_dir(entry.expect("identity").path()).expect("identity dir"))
        .filter_map(Result::ok)
        .find(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .expect("published file");
    assert_eq!(
        std::fs::read(stored.path()).expect("published bytes"),
        payload
    );
    let _ = std::fs::remove_dir_all(uploads);
}

#[test]
fn channel_offset_failure_and_link_close_remove_private_stage() {
    let (mut live, room_id, uploads) = channel_server("channel-cleanup");
    let link_id = [0xc2; 16];
    let resource_id = open_join_and_offer(&mut live, link_id, room_id, 4);
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Start {
            resource_id: resource_id.clone(),
            total_bytes: 4,
        },
    ))
    .expect("start");
    assert!(live
        .handle_event(channel_event(
            link_id,
            ChannelAttachmentFrame::Data {
                resource_id: resource_id.clone(),
                offset: 1,
                bytes: vec![1],
            }
        ))
        .is_err());
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Start {
            resource_id,
            total_bytes: 4,
        },
    ))
    .expect("restart stage");
    live.handle_event(OmenchatLinkEvent::LinkClosed {
        link_id,
        reason: Some("test cancellation".into()),
    })
    .expect("close");
    let staged = std::fs::read_dir(&uploads)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            std::fs::read_dir(entry.path())
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
        })
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
        .count();
    assert_eq!(staged, 0);
    let _ = std::fs::remove_dir_all(uploads);
}

#[test]
fn channel_digest_failure_removes_private_stage_without_publication() {
    let (mut live, room_id, uploads) = channel_server("channel-digest");
    let link_id = [0xc3; 16];
    let payload = b"digest failure";
    let resource_id = open_join_and_offer(&mut live, link_id, room_id, payload.len() as u64);
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Start {
            resource_id: resource_id.clone(),
            total_bytes: payload.len() as u64,
        },
    ))
    .expect("start");
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Data {
            resource_id: resource_id.clone(),
            offset: 0,
            bytes: payload.to_vec(),
        },
    ))
    .expect("data");
    live.handle_event(channel_event(
        link_id,
        ChannelAttachmentFrame::Finish {
            resource_id,
            total_bytes: payload.len() as u64,
            digest: [0; 32],
        },
    ))
    .expect("digest rejection is a handled protocol outcome");
    let files = std::fs::read_dir(&uploads)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            std::fs::read_dir(entry.path())
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
        })
        .count();
    assert_eq!(files, 0);
    let _ = std::fs::remove_dir_all(uploads);
}

#[test]
fn duplicate_link_retirement_preserves_replacement_identity_pending_upload() {
    let (mut live, room_id, uploads) = channel_server("channel-replacement");
    let old_link = [0xc4; 16];
    let replacement_link = [0xc5; 16];
    let _resource_id = open_join_and_offer(&mut live, old_link, room_id, 4);
    assert_eq!(live.engine.pending_upload_metrics().expect("metrics").0, 1);

    live.handle_event(OmenchatLinkEvent::LinkOpened {
        link_id: replacement_link,
        peer: ServerPeer {
            identity_hash: b"channel-peer".to_vec(),
            display_name: "Channel Peer".into(),
            lxmf_destination: None,
        },
    })
    .expect("replacement link");
    assert_eq!(live.engine.pending_upload_metrics().expect("metrics").0, 1);

    live.handle_event(OmenchatLinkEvent::LinkClosed {
        link_id: old_link,
        reason: Some("late old-Link close".into()),
    })
    .expect("late close");
    assert_eq!(live.engine.pending_upload_metrics().expect("metrics").0, 1);

    live.handle_event(OmenchatLinkEvent::LinkClosed {
        link_id: replacement_link,
        reason: Some("current Link close".into()),
    })
    .expect("current close");
    assert_eq!(live.engine.pending_upload_metrics().expect("metrics").0, 0);
    let _ = std::fs::remove_dir_all(uploads);
}
