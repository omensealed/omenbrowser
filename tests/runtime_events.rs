use std::collections::BTreeMap;

use omenbrowser_rs::directory::DirectoryKind;
use omenbrowser_rs::messaging::{MessageSummary, TransportMethod};
use omenbrowser_rs::runtime::event::{
    AppEvent, BrowserBusEvent, MessageBusEvent, PropagationSyncEvent, PropagationSyncEventStatus,
    PropagationSyncStage, RuntimeBusEvent,
};
use omenbrowser_rs::runtime::network::{
    AnnouncePayload, PageFetchProbeReport, PageFetchProbeStage, PageFetchProbeStep,
    RuntimeBackendName, RuntimeStatus,
};

const FIXTURE_DESTINATION_HASH: &str = "00112233445566778899aabbccddeeff";
const FIXTURE_DESTINATION_URL: &str = "00112233445566778899aabbccddeeff:/";

#[test]
fn app_event_represents_runtime_browser_message_and_partial_paths() {
    let status = AppEvent::Runtime(RuntimeBusEvent::StatusChanged(RuntimeStatus {
        connected: false,
        backend: RuntimeBackendName::Mock,
        active_identity: None,
        message: "mock".into(),
    }));
    let browser = AppEvent::Browser(BrowserBusEvent::PartialRefreshResult {
        tab_id: 1,
        generation: 2,
        slot: "main".into(),
        result: Ok(">partial".into()),
    });
    let message = AppEvent::Message(MessageBusEvent::SyncResult {
        generation: 3,
        result: Ok(vec![MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "hello".into(),
            content: "body".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: true,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("m1".into()),
            fields: BTreeMap::new(),
            attachments: Vec::new(),
        }]),
    });
    let announce = AppEvent::Runtime(RuntimeBusEvent::Announce(AnnouncePayload {
        destination_hash: "hash".into(),
        display_name: "Node".into(),
        kind: DirectoryKind::Node,
        associated_hash: None,
        node_associated_hash: None,
        has_ratchet: false,
        lxmf_stamp_cost: None,
    }));
    let probe = AppEvent::Runtime(RuntimeBusEvent::PageFetchProbe(PageFetchProbeReport {
        backend: RuntimeBackendName::Reticulum,
        url: FIXTURE_DESTINATION_URL.into(),
        destination_hash: Some(FIXTURE_DESTINATION_HASH.into()),
        path: Some("/".into()),
        execute_request: false,
        ready_to_request: false,
        steps: vec![PageFetchProbeStep::failed(
            PageFetchProbeStage::PathDiscovery,
            "path request queued",
        )],
    }));

    assert!(matches!(status, AppEvent::Runtime(_)));
    assert!(matches!(browser, AppEvent::Browser(_)));
    assert!(matches!(message, AppEvent::Message(_)));
    assert!(matches!(announce, AppEvent::Runtime(_)));
    assert!(matches!(probe, AppEvent::Runtime(_)));
}

#[test]
fn propagation_sync_event_round_trips_with_stage_counts() {
    let event = RuntimeBusEvent::PropagationSync(PropagationSyncEvent {
        stage: PropagationSyncStage::GetResponse,
        status: PropagationSyncEventStatus::Complete,
        destination_hash: Some(FIXTURE_DESTINATION_HASH.into()),
        detail: "received propagation payload response".into(),
        counts: BTreeMap::from([("payloads".into(), 2usize), ("decoded".into(), 1usize)]),
    });

    let encoded = serde_json::to_string(&event).expect("encode event");
    let decoded: RuntimeBusEvent = serde_json::from_str(&encoded).expect("decode event");

    assert_eq!(decoded, event);
    assert!(encoded.contains("get_response"));
    assert!(encoded.contains("payloads"));
}
