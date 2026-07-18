use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use omenbrowser_rs::messaging::{
    DeliveryMode, MessageStore, MessageSummary, MessagingService, OutboundComposeRequest,
    OutboundOperationIdentity, TransportMethod,
};
use omenbrowser_rs::runtime::{
    LxmfDeliveryEvidence, LxmfDeliveryEvidenceKind, MockNetworkRuntime, OutboundDeliveryState,
    OutboundStatus, RuntimeLxmfDeliveryState, RuntimeLxmfDeliveryUpdate,
};

fn temp_dir(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-integration-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn outbound_direct_message() -> MessageSummary {
    MessageSummary {
        peer_hash: "peer".into(),
        peer_label: "Peer".into(),
        title: "Subject".into(),
        content: "Body".into(),
        timestamp: 1.0,
        transport_method: TransportMethod::Direct,
        delivered: false,
        failed: false,
        incoming: false,
        unread: false,
        message_id: Some("packet-a".into()),
        fields: BTreeMap::from([("native_lxmf_state".into(), "submitted_to_rns_net".into())]),
        attachments: Vec::new(),
    }
}

fn message_with_state(
    peer_hash: &str,
    message_id: &str,
    method: TransportMethod,
    state: &str,
) -> MessageSummary {
    MessageSummary {
        peer_hash: peer_hash.into(),
        peer_label: "Peer".into(),
        title: "Subject".into(),
        content: "Body".into(),
        timestamp: 1.0,
        transport_method: method,
        delivered: false,
        failed: false,
        incoming: false,
        unread: false,
        message_id: Some(message_id.into()),
        fields: BTreeMap::from([("native_lxmf_state".into(), state.into())]),
        attachments: Vec::new(),
    }
}

fn service(name: &str) -> MessagingService {
    let store = MessageStore::new(temp_dir(name)).expect("store");
    MessagingService::new(Arc::new(MockNetworkRuntime::default()), store)
}

fn sdk_delivery_update(
    state: RuntimeLxmfDeliveryState,
    terminal: bool,
    seq_no: u64,
) -> RuntimeLxmfDeliveryUpdate {
    RuntimeLxmfDeliveryUpdate {
        message_id: "packet-a".into(),
        peer_hash: Some("peer".into()),
        previous_state: None,
        state,
        terminal,
        attempts: 2,
        reason_code: Some("router_receipt".into()),
        last_updated_ms: 42,
        event_id: format!("delivery-{seq_no}"),
        seq_no,
        cursor: format!("cursor-{seq_no}"),
    }
}

#[test]
fn sdk_delivery_update_persists_typed_terminal_state_and_rejects_regression() {
    let service = service("sdk-delivery-persistence");
    service
        .store()
        .append(outbound_direct_message())
        .expect("append message");

    assert!(service
        .apply_sdk_delivery_update(&sdk_delivery_update(
            RuntimeLxmfDeliveryState::Delivered,
            true,
            2,
        ))
        .expect("apply delivered update"));
    assert!(!service
        .apply_sdk_delivery_update(&sdk_delivery_update(
            RuntimeLxmfDeliveryState::Sent,
            false,
            3,
        ))
        .expect("reject terminal regression"));

    let thread = service.conversation("peer").expect("reload thread");
    let message = &thread.messages[0];
    assert!(message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_sdk_state")
            .map(String::as_str),
        Some("delivered")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_sdk_seq_no")
            .map(String::as_str),
        Some("2")
    );
}

#[tokio::test]
async fn expired_outbound_operation_is_rejected_before_runtime_admission() {
    let service = service("expired-operation-admission");
    let operation = OutboundOperationIdentity::generate_at(1, 1_000).expect("expired operation");

    let error = service
        .compose_with_operation(OutboundComposeRequest {
            peer_hash: "peer".into(),
            title: "Expired".into(),
            content: "Body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            attachments: Vec::new(),
            operation,
        })
        .await
        .expect_err("expired operation must not reach the runtime");

    assert!(error.to_string().contains("deadline expired"));
    assert!(service
        .store()
        .get_thread("peer")
        .expect("empty thread")
        .messages
        .is_empty());
}

#[tokio::test]
async fn messaging_service_syncs_runtime_messages_and_composes_outbound() {
    let service = service("sync-compose");

    let synced = service
        .sync_runtime_messages()
        .await
        .expect("sync messages");
    let sent = service
        .compose(
            "0123456789abcdef",
            "Hi",
            "Body",
            DeliveryMode::Propagated,
            true,
            Vec::new(),
        )
        .await
        .expect("compose");

    assert!(!synced.is_empty());
    assert_eq!(sent.transport_method, TransportMethod::Propagated);
    assert!(sent.fields.contains_key("native_lxmf_sdk_idempotency_key"));
    assert!(sent.fields.contains_key("native_lxmf_sdk_correlation_id"));
    assert_eq!(service.threads().expect("threads").len(), 1);
}

#[tokio::test]
async fn cancellation_outcome_is_explicit_and_does_not_claim_terminal_state() {
    let service = service("cancel-outcome");
    service
        .store()
        .append(outbound_direct_message())
        .expect("append message");

    let update = service
        .cancel_delivery("peer", "packet-a")
        .await
        .expect("cancel outcome");

    assert_eq!(
        update.outcome,
        omenbrowser_rs::runtime::LxmfCancelOutcome::Unsupported
    );
    let thread = service.conversation("peer").expect("reload thread");
    let message = &thread.messages[0];
    assert!(!message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_sdk_cancel_outcome")
            .map(String::as_str),
        Some("unsupported")
    );
}

#[test]
fn rns_packet_proof_does_not_mark_direct_lxmf_delivered() {
    let service = service("rns-proof-peer-unconfirmed");
    service
        .store()
        .append(outbound_direct_message())
        .expect("append message");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer".into(),
            message_id: Some("packet-a".into()),
            kind: LxmfDeliveryEvidenceKind::RnsPacketProof,
            detail: Some(
                "packet_hash:packet-a;proof_destination:peer;matched_pending:true;rtt:0.125".into(),
            ),
            rtt: Some(0.125),
            observed_at: Some(12.5),
        })
        .expect("apply evidence"));

    let thread = service.conversation("peer").expect("thread");
    let message = &thread.messages[0];
    assert!(!message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("rns_packet_proof_peer_unconfirmed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("rns_packet_proof_peer_delivery_unconfirmed")
    );
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("transport_proof_received")
    );
}

#[test]
fn rns_packet_proof_status_is_conservative_without_a_second_evidence_event() {
    let service = service("rns-proof-status-only");
    service
        .store()
        .append(outbound_direct_message())
        .expect("append message");

    assert!(service
        .update_outbound_status(&OutboundStatus {
            peer_hash: "peer".into(),
            message_id: Some("packet-a".into()),
            delivered: false,
            failed: false,
            state: OutboundDeliveryState::SubmittedToRnsNet,
            evidence: Some("rns_packet_proof".into()),
            rtt: None,
        })
        .expect("apply proof status"));

    let thread = service.conversation("peer").expect("thread");
    let message = &thread.messages[0];
    assert!(!message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("rns_packet_proof_peer_unconfirmed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("rns_packet_proof_peer_delivery_unconfirmed")
    );
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("transport_proof_received")
    );
}

#[test]
fn stale_direct_lxmf_submission_becomes_unconfirmed_not_failed() {
    let service = service("stale-direct");
    let mut message = outbound_direct_message();
    message.peer_hash = "peer-a".into();
    message.message_id = Some("packet-a".into());
    message.fields.insert(
        "native_lxmf_proof_state".into(),
        "waiting_for_packet_proof".into(),
    );
    message
        .fields
        .insert("native_lxmf_submitted_at".into(), "1.0".into());
    service.ingest_runtime_message(message).expect("stored");

    let changed = service
        .reconcile_stale_native_lxmf_direct(60.0, 45.0)
        .expect("reconcile");

    assert_eq!(changed.len(), 1);
    assert!(!changed[0].failed);
    assert_eq!(
        changed[0]
            .fields
            .get("native_lxmf_state")
            .map(String::as_str),
        Some("submitted_unconfirmed")
    );
    assert!(changed[0]
        .fields
        .get("native_lxmf_retry_guidance")
        .is_some_and(|guidance| guidance.contains("no RNS proof")));
}

#[test]
fn direct_resource_status_does_not_revert_to_packet_proof_timeout() {
    let service = service("direct-resource-advertised");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "direct-a",
            TransportMethod::Direct,
            "submitted_to_rns_net",
        ))
        .expect("stored");

    assert!(service
        .update_outbound_status(&OutboundStatus {
            peer_hash: "peer-a".into(),
            message_id: Some("direct-a".into()),
            delivered: false,
            failed: false,
            state: OutboundDeliveryState::SubmittedToRnsNet,
            evidence: Some(
                "direct_transfer_state:resource_advertised;\
                 direct_link_id:010203;\
                 submitted_at:12.500"
                    .replace(char::is_whitespace, ""),
            ),
            rtt: None,
        })
        .expect("update status"));

    let changed = service
        .reconcile_stale_native_lxmf_direct(80.0, 45.0)
        .expect("reconcile");
    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];

    assert!(changed.is_empty());
    assert_eq!(
        message
            .fields
            .get("native_lxmf_direct_transfer_state")
            .map(String::as_str),
        Some("resource_advertised")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("resource_advertised")
    );
    assert!(message
        .fields
        .get("native_lxmf_retry_guidance")
        .is_some_and(|guidance| guidance.contains("direct resource is in progress")));
}

#[test]
fn direct_link_packet_status_remains_peer_unconfirmed_without_router_callback() {
    let service = service("direct-link-packet-sent");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "direct-a",
            TransportMethod::Direct,
            "submitted_to_rns_net",
        ))
        .expect("stored");

    assert!(service
        .update_outbound_status(&OutboundStatus {
            peer_hash: "peer-a".into(),
            message_id: Some("direct-a".into()),
            delivered: false,
            failed: false,
            state: OutboundDeliveryState::SubmittedToRnsNet,
            evidence: Some(
                "direct_transfer_state:link_packet_sent;\
                 direct_link_id:010203;\
                 receipt_state:direct_link_packet_sent_peer_unconfirmed;\
                 delivery_state:peer_delivery_unconfirmed;\
                 submitted_at:12.500"
                    .replace(char::is_whitespace, ""),
            ),
            rtt: None,
        })
        .expect("update status"));

    let changed = service
        .reconcile_stale_native_lxmf_direct(80.0, 45.0)
        .expect("reconcile");
    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];

    assert!(changed.is_empty());
    assert!(!message.delivered);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_direct_transfer_state")
            .map(String::as_str),
        Some("link_packet_sent")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("direct_link_packet_sent_peer_unconfirmed")
    );
    assert!(message
        .fields
        .get("native_lxmf_retry_guidance")
        .is_some_and(|guidance| guidance.contains("wait for LXMF router evidence")));
}

#[test]
fn stale_propagated_lxmf_router_deferred_becomes_router_timeout() {
    let service = service("stale-propagated");
    let mut message = message_with_state(
        "peer-a",
        "prop-a",
        TransportMethod::Propagated,
        "queued_for_propagation",
    );
    message.fields.insert(
        "native_lxmf_propagation_transfer_state".into(),
        "router_deferred".into(),
    );
    message
        .fields
        .insert("native_lxmf_submitted_at".into(), "1.0".into());
    service.ingest_runtime_message(message).expect("stored");

    let changed = service
        .reconcile_stale_native_lxmf_propagated(60.0, 45.0)
        .expect("reconcile");

    assert_eq!(changed.len(), 1);
    assert_eq!(
        changed[0]
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("router_timeout")
    );
}

#[test]
fn no_receipt_evidence_sets_direct_retry_guidance_without_terminal_status() {
    let service = service("no-receipt-evidence");
    let mut message = outbound_direct_message();
    message.peer_hash = "peer-a".into();
    message.message_id = Some("packet-a".into());
    service.ingest_runtime_message(message).expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("packet-a".into()),
            kind: LxmfDeliveryEvidenceKind::NoReceiptObserved,
            detail: Some(
                "packet_hash:packet-a;direct_timeout_age_secs:45.0;fallback_ready:true;\
                 propagation_node:fedcba98765432100123456789abcdef;\
                 peer_activity_observed:false;proof_state:proof_not_observed"
                    .replace(char::is_whitespace, ""),
            ),
            rtt: None,
            observed_at: Some(12.5),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(!message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("no_receipt_observed")
    );
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("propagation_retry_ready")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_node")
            .map(String::as_str),
        Some("fedcba98765432100123456789abcdef")
    );
}

#[test]
fn propagation_node_acceptance_keeps_peer_delivery_unconfirmed() {
    let service = service("propagation-accepted");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "transient-a",
            TransportMethod::Propagated,
            "queued_for_propagation",
        ))
        .expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("transient-a".into()),
            kind: LxmfDeliveryEvidenceKind::PropagationNodeAccepted,
            detail: Some(
                "propagation_transfer_state:resource_completed;\
                 propagation_link_id:010203;\
                 propagation_node:fedcba98765432100123456789abcdef;\
                 receipt_state:propagation_node_accepted;\
                 delivery_state:peer_delivery_unconfirmed"
                    .replace(char::is_whitespace, ""),
            ),
            rtt: None,
            observed_at: Some(12.5),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(!message.delivered);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("propagation_node_accepted")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("peer_delivery_unconfirmed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_next_action")
            .map(String::as_str),
        Some("sync_propagation")
    );
}

#[test]
fn lxmf_router_delivered_evidence_marks_message_delivered() {
    let service = service("router-delivered-evidence");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "message-a",
            TransportMethod::Direct,
            "submitted_to_rns_net",
        ))
        .expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("message-a".into()),
            kind: LxmfDeliveryEvidenceKind::LxmfRouterDelivered,
            detail: Some("delivery_state:delivered".into()),
            rtt: None,
            observed_at: Some(12.5),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("delivered")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("lxmf_delivered")
    );
}

#[test]
fn inbound_peer_evidence_updates_state_without_claiming_delivery() {
    let service = service("inbound-peer-evidence");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "message-a",
            TransportMethod::Direct,
            "submitted_unconfirmed",
        ))
        .expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("message-a".into()),
            kind: LxmfDeliveryEvidenceKind::InboundPeerMessage,
            detail: Some(
                "peer_activity_observed:true;observed_peer_hash:peer-a;observed_at:12.500".into(),
            ),
            rtt: None,
            observed_at: Some(12.5),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(!message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("peer_activity_observed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("peer_activity_after_send")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("inbound_peer_message")
    );
    assert!(message
        .fields
        .get("native_lxmf_retry_guidance")
        .is_some_and(|guidance| guidance.contains("do not retry")));
}

#[test]
fn lxmf_router_failed_evidence_marks_message_failed() {
    let service = service("router-failed-evidence");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "message-a",
            TransportMethod::Direct,
            "submitted_to_rns_net",
        ))
        .expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("message-a".into()),
            kind: LxmfDeliveryEvidenceKind::LxmfRouterFailed,
            detail: Some("delivery_state:failed".into()),
            rtt: None,
            observed_at: Some(12.5),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(!message.delivered);
    assert!(message.failed);
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("failed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("lxmf_failed")
    );
}

#[test]
fn propagation_sync_without_payload_sets_backoff_and_sync_again_action() {
    let service = service("propagation-sync-empty");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "transient-a",
            TransportMethod::Propagated,
            "propagation_node_accepted",
        ))
        .expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("transient-a".into()),
            kind: LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads,
            detail: Some(
                "propagation_transfer_state:resource_completed;requested:0;decoded:0;haves:2"
                    .into(),
            ),
            rtt: None,
            observed_at: Some(100.0),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("propagation_node_accepted")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_sync_evidence_kind")
            .map(String::as_str),
        Some("propagation_sync_no_payloads")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_sync_state")
            .map(String::as_str),
        Some("propagation_sync_no_payloads")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_sync_transfer_state")
            .map(String::as_str),
        Some("resource_completed")
    );
    assert!(!message
        .fields
        .contains_key("native_lxmf_delivery_evidence_kind"));
    assert_eq!(
        message
            .fields
            .get("native_lxmf_next_action")
            .map(String::as_str),
        Some("sync_propagation_again")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_retry_after_epoch_secs")
            .map(String::as_str),
        Some("130.000")
    );
}

#[test]
fn propagation_sync_no_payload_preserves_handoff_transfer_field() {
    let service = service("propagation-sync-preserves-transfer-handoff");
    let mut message = message_with_state(
        "peer-a",
        "transient-transfer-a",
        TransportMethod::Propagated,
        "queued_for_propagation",
    );
    message.fields.insert(
        "native_lxmf_propagation_transfer_state".into(),
        "resource_advertised".into(),
    );
    service.ingest_runtime_message(message).expect("stored");

    assert!(service
        .apply_lxmf_delivery_evidence(&LxmfDeliveryEvidence {
            peer_hash: "peer-a".into(),
            message_id: Some("transient-transfer-a".into()),
            kind: LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads,
            detail: Some(
                "propagation_transfer_state:resource_completed;requested:0;decoded:0;haves:2"
                    .into(),
            ),
            rtt: None,
            observed_at: Some(100.0),
        })
        .expect("apply evidence"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("propagation_node_accepted")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("propagation_node_accepted_peer_unconfirmed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("resource_advertised")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_sync_evidence_kind")
            .map(String::as_str),
        Some("propagation_sync_no_payloads")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_sync_state")
            .map(String::as_str),
        Some("propagation_sync_no_payloads")
    );
    assert!(!message
        .fields
        .contains_key("native_lxmf_delivery_evidence_kind"));
    assert_eq!(
        message
            .fields
            .get("native_lxmf_next_action")
            .map(String::as_str),
        Some("sync_propagation_again")
    );
}

#[test]
fn propagated_resource_completion_updates_transfer_fields_without_peer_delivery() {
    let service = service("resource-completed");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "prop-a",
            TransportMethod::Propagated,
            "queued_for_propagation",
        ))
        .expect("stored");

    assert!(service
        .update_outbound_status(&OutboundStatus {
            peer_hash: "peer-a".into(),
            message_id: Some("prop-a".into()),
            delivered: false,
            failed: false,
            state: OutboundDeliveryState::SubmittedToRnsNet,
            evidence: Some(
                "propagation_transfer_state:resource_completed;\
                 propagation_link_id:010203;\
                 resource_received:4;\
                 resource_total:4;\
                 submitted_at:12.500"
                    .replace(char::is_whitespace, ""),
            ),
            rtt: None,
        })
        .expect("update status"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(!message.delivered);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("resource_completed")
    );
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("propagation_transfer_completed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("propagation_node_accepted_peer_unconfirmed")
    );
}

#[test]
fn propagated_link_packet_updates_transfer_fields_without_peer_delivery() {
    let service = service("propagation-link-packet");
    service
        .ingest_runtime_message(message_with_state(
            "peer-a",
            "prop-link-a",
            TransportMethod::Propagated,
            "queued_for_propagation",
        ))
        .expect("stored");

    assert!(service
        .update_outbound_status(&OutboundStatus {
            peer_hash: "peer-a".into(),
            message_id: Some("prop-link-a".into()),
            delivered: false,
            failed: false,
            state: OutboundDeliveryState::SubmittedToRnsNet,
            evidence: Some(
                "propagation_transfer_state:link_packet_sent;\
                 propagation_link_id:010203;\
                 propagation_node:fedcba98765432100123456789abcdef;\
                 receipt_state:propagation_node_accepted;\
                 delivery_state:peer_delivery_unconfirmed;\
                 submitted_at:12.500"
                    .replace(char::is_whitespace, ""),
            ),
            rtt: None,
        })
        .expect("update status"));

    let stored = service.conversation("peer-a").expect("conversation");
    let message = &stored.messages[0];
    assert!(!message.delivered);
    assert!(!message.failed);
    assert_eq!(
        message
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .map(String::as_str),
        Some("link_packet_sent")
    );
    assert_eq!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("propagation_transfer_completed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("propagation_node_accepted_peer_unconfirmed")
    );
    assert_eq!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("peer_delivery_unconfirmed")
    );
}
