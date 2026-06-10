use std::collections::BTreeMap;

use omenbrowser_rs::messaging::lxmf_router::{
    direct_lxmf_timeout_transition, DirectLxmfRouterRecord, DirectLxmfRouterState,
};
use omenbrowser_rs::messaging::{MessageSummary, TransportMethod};

fn direct_message(fields: BTreeMap<String, String>) -> MessageSummary {
    MessageSummary {
        peer_hash: "peer".into(),
        peer_label: "Peer".into(),
        title: "title".into(),
        content: "body".into(),
        timestamp: 1.0,
        transport_method: TransportMethod::Direct,
        delivered: false,
        failed: false,
        incoming: false,
        unread: false,
        message_id: Some("packet".into()),
        fields,
        attachments: Vec::new(),
    }
}

#[test]
fn direct_router_record_classifies_submitted_and_timeout_outcomes() {
    let message = direct_message(BTreeMap::from([
        ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
        ("native_lxmf_submitted_at".into(), "10.0".into()),
    ]));

    let record = DirectLxmfRouterRecord::from_message(&message);

    assert_eq!(record.state, DirectLxmfRouterState::Submitted);
    assert_eq!(record.stale_outcome(20.0, 45.0), None);
    assert_eq!(
        record.stale_outcome(60.0, 45.0),
        Some(DirectLxmfRouterState::NoReceiptObserved)
    );
}

#[test]
fn direct_router_record_prefers_propagation_retry_when_fallback_exists() {
    let message = direct_message(BTreeMap::from([
        ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
        ("native_lxmf_submitted_at".into(), "10.0".into()),
        (
            "native_lxmf_propagation_fallback_available".into(),
            "true".into(),
        ),
        (
            "native_lxmf_propagation_node".into(),
            "fedcba98765432100123456789abcdef".into(),
        ),
    ]));

    let record = DirectLxmfRouterRecord::from_message(&message);

    assert_eq!(
        record.propagation_fallback_node.as_deref(),
        Some("fedcba98765432100123456789abcdef")
    );
    assert_eq!(
        record.stale_outcome(60.0, 45.0),
        Some(DirectLxmfRouterState::PropagationRetryReady)
    );
}

#[test]
fn direct_router_record_classifies_peer_activity_as_non_timeout_terminal() {
    let message = direct_message(BTreeMap::from([
        ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
        (
            "native_lxmf_receipt_state".into(),
            "peer_activity_after_send".into(),
        ),
        ("native_lxmf_submitted_at".into(), "10.0".into()),
    ]));

    let record = DirectLxmfRouterRecord::from_message(&message);

    assert_eq!(record.state, DirectLxmfRouterState::PeerActivityObserved);
    assert_eq!(record.stale_outcome(60.0, 45.0), None);
}

#[test]
fn direct_timeout_transition_applies_unconfirmed_fields() {
    let message = direct_message(BTreeMap::from([
        ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
        (
            "native_lxmf_proof_state".into(),
            "waiting_for_packet_proof".into(),
        ),
        ("native_lxmf_submitted_at".into(), "10.0".into()),
    ]));

    let transition = direct_lxmf_timeout_transition(&message, 60.0, 45.0).expect("transition");
    let mut fields = message.fields.clone();
    transition.apply_to_fields(&mut fields);

    assert_eq!(transition.state, DirectLxmfRouterState::NoReceiptObserved);
    assert_eq!(
        fields.get("native_lxmf_state").map(String::as_str),
        Some("submitted_unconfirmed")
    );
    assert_eq!(
        fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("no_receipt_observed")
    );
}

#[test]
fn direct_timeout_transition_requires_waiting_proof_state() {
    let message = direct_message(BTreeMap::from([
        ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
        ("native_lxmf_submitted_at".into(), "10.0".into()),
    ]));

    assert_eq!(direct_lxmf_timeout_transition(&message, 60.0, 45.0), None);
}
