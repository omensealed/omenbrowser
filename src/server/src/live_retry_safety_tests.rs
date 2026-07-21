use super::*;
use crate::protocol::codec::{decode_frame, encode_frame};
use crate::protocol::{ChatErrorCode, ChatOp, Frame, FrameBody, FrameValue};
use crate::session::{ServerPeer, SessionEngine};
use crate::store::{OmenchatStore, ServerRoomEventKind};
use crate::transport::{CapturedTransport, OMENCHAT_LINK_CONTEXT};

const FIRST_LINK: LinkId = [0x91; 16];
const RECONNECTED_LINK: LinkId = [0x92; 16];
const MUTATION_SEQ: u32 = 41;
const MUTATION_BODY: &str = "one uncertain logical mutation";

fn temp_store_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "omenchatd-retry-safety-{label}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ))
}

fn server_at(path: &std::path::Path) -> (OmenchatLiveServer<CapturedTransport>, RoomId) {
    let store = OmenchatStore::open(path).expect("store");
    let room_id = store
        .ensure_room("lobby", Some("Retry safety fixture"))
        .expect("room")
        .room_id;
    (
        OmenchatLiveServer::new(SessionEngine::new(store), CapturedTransport::default()),
        room_id,
    )
}

fn open_and_join(
    live: &mut OmenchatLiveServer<CapturedTransport>,
    link_id: LinkId,
    room_id: RoomId,
) {
    live.handle_event(OmenchatLinkEvent::LinkOpened {
        link_id,
        peer: ServerPeer {
            identity_hash: b"retry-safety-client".to_vec(),
            display_name: "Retry Safety Client".into(),
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
    assert_eq!(live.link_rooms.get(&link_id), Some(&room_id));
}

fn mutation_frame(body: &str) -> Vec<u8> {
    encode_frame(&Frame::new(
        ChatOp::RoomMessage,
        MUTATION_SEQ,
        Some(1),
        FrameBody::Text(body.into()),
    ))
    .expect("mutation frame")
}

fn send_mutation(
    live: &mut OmenchatLiveServer<CapturedTransport>,
    link_id: LinkId,
    bytes: Vec<u8>,
) {
    live.handle_event(OmenchatLinkEvent::LinkData {
        link_id,
        context: OMENCHAT_LINK_CONTEXT,
        data: bytes,
    })
    .expect("mutation dispatch");
}

fn count_messages(path: &std::path::Path, room_id: RoomId, body: &str) -> usize {
    OmenchatStore::open(path)
        .expect("reopen store")
        .latest_events(room_id, 100)
        .expect("events")
        .into_iter()
        .filter(|event| {
            matches!(&event.kind, ServerRoomEventKind::Message { body: found } if found == body)
        })
        .count()
}

fn cleanup_store(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}

#[test]
fn committed_mutation_remains_uncertain_when_the_response_is_lost() {
    let path = temp_store_path("lost-response");
    let (mut live, room_id) = server_at(&path);
    open_and_join(&mut live, FIRST_LINK, room_id);

    send_mutation(&mut live, FIRST_LINK, mutation_frame(MUTATION_BODY));
    live.transport_mut().frames.clear(); // Model loss after commit, before client receipt.

    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 1);
    assert_eq!(live.stats().replay_cache_items, 1);
    assert!(live.transport().frames.is_empty());
    drop(live);
    cleanup_store(&path);
}

#[test]
fn link_close_after_commit_discards_the_only_replay_identity() {
    let path = temp_store_path("close-after-commit");
    let (mut live, room_id) = server_at(&path);
    open_and_join(&mut live, FIRST_LINK, room_id);
    send_mutation(&mut live, FIRST_LINK, mutation_frame(MUTATION_BODY));
    live.transport_mut().frames.clear();

    live.handle_event(OmenchatLinkEvent::LinkClosed {
        link_id: FIRST_LINK,
        reason: Some("acknowledgement path lost".into()),
    })
    .expect("close link");

    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 1);
    assert_eq!(live.stats().replay_cache_items, 0);
    assert_eq!(live.stats().replay_cache_bytes, 0);
    drop(live);
    cleanup_store(&path);
}

#[test]
fn protocol_v1_resend_on_a_new_link_would_duplicate_the_committed_mutation() {
    let path = temp_store_path("new-link");
    let (mut live, room_id) = server_at(&path);
    open_and_join(&mut live, FIRST_LINK, room_id);
    let request = mutation_frame(MUTATION_BODY);
    send_mutation(&mut live, FIRST_LINK, request.clone());
    live.handle_event(OmenchatLinkEvent::LinkClosed {
        link_id: FIRST_LINK,
        reason: Some("response lost".into()),
    })
    .expect("close link");

    open_and_join(&mut live, RECONNECTED_LINK, room_id);
    send_mutation(&mut live, RECONNECTED_LINK, request);

    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 2);
    assert_eq!(live.stats().replayed_operations, 0);
    drop(live);
    cleanup_store(&path);
}

#[test]
fn protocol_v1_client_restart_has_no_durable_instance_identity() {
    let path = temp_store_path("client-restart");
    let (mut live, room_id) = server_at(&path);
    open_and_join(&mut live, FIRST_LINK, room_id);
    send_mutation(&mut live, FIRST_LINK, mutation_frame(MUTATION_BODY));
    live.handle_event(OmenchatLinkEvent::LinkClosed {
        link_id: FIRST_LINK,
        reason: Some("client process stopped before acknowledgement".into()),
    })
    .expect("close link");

    // A restarted v1 client can reproduce the same seq and bytes, but the new
    // Link is the only available execution identity.
    open_and_join(&mut live, RECONNECTED_LINK, room_id);
    send_mutation(&mut live, RECONNECTED_LINK, mutation_frame(MUTATION_BODY));

    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 2);
    drop(live);
    cleanup_store(&path);
}

#[test]
fn protocol_v1_server_restart_forgets_committed_mutation_replay_state() {
    let path = temp_store_path("server-restart");
    let (mut before_restart, room_id) = server_at(&path);
    open_and_join(&mut before_restart, FIRST_LINK, room_id);
    send_mutation(
        &mut before_restart,
        FIRST_LINK,
        mutation_frame(MUTATION_BODY),
    );
    drop(before_restart);

    let (mut after_restart, reopened_room_id) = server_at(&path);
    assert_eq!(reopened_room_id, room_id);
    open_and_join(&mut after_restart, RECONNECTED_LINK, room_id);
    send_mutation(
        &mut after_restart,
        RECONNECTED_LINK,
        mutation_frame(MUTATION_BODY),
    );

    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 2);
    drop(after_restart);
    cleanup_store(&path);
}

#[test]
fn exact_same_link_duplicate_returns_the_original_result_once() {
    let path = temp_store_path("exact-duplicate");
    let (mut live, room_id) = server_at(&path);
    open_and_join(&mut live, FIRST_LINK, room_id);
    let request = mutation_frame(MUTATION_BODY);

    send_mutation(&mut live, FIRST_LINK, request.clone());
    send_mutation(&mut live, FIRST_LINK, request);

    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 1);
    assert_eq!(live.stats().replayed_operations, 1);
    drop(live);
    cleanup_store(&path);
}

#[test]
fn same_link_sequence_reuse_with_different_content_is_a_conflict() {
    let path = temp_store_path("content-conflict");
    let (mut live, room_id) = server_at(&path);
    open_and_join(&mut live, FIRST_LINK, room_id);
    send_mutation(&mut live, FIRST_LINK, mutation_frame(MUTATION_BODY));
    send_mutation(
        &mut live,
        FIRST_LINK,
        mutation_frame("different content under the same v1 sequence"),
    );

    let conflict = live
        .transport()
        .frames
        .iter()
        .rev()
        .find(|captured| captured.link_id == FIRST_LINK)
        .and_then(|captured| decode_frame(&captured.bytes).ok())
        .expect("conflict response");
    assert_eq!(conflict.op, ChatOp::Error);
    let FrameBody::Fields(values) = &conflict.body else {
        panic!("expected structured conflict body");
    };
    assert_eq!(
        values.first(),
        Some(&FrameValue::U64(
            ChatErrorCode::MalformedFrame as u16 as u64
        ))
    );
    assert_eq!(count_messages(&path, room_id, MUTATION_BODY), 1);
    assert_eq!(live.stats().replay_collisions, 1);
    drop(live);
    cleanup_store(&path);
}
