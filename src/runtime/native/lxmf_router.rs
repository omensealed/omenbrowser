use std::collections::BTreeMap;

use crate::messaging::MessageSummary;
use crate::runtime::network::{
    LxmfDeliveryEvidence, LxmfDeliveryEvidenceKind, OutboundDeliveryState, OutboundStatus,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PendingDirectLxmf {
    pub peer_hash: String,
    pub submitted_at: f64,
    pub packet_proof_observed_at: Option<f64>,
    pub peer_activity_observed_at: Option<f64>,
    pub propagation_fallback_node: Option<String>,
    pub no_receipt_observed_at: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DirectLxmfTimeoutEvent {
    pub peer_hash: String,
    pub message_id: String,
    pub submitted_at: f64,
    pub observed_at: f64,
    pub propagation_fallback_node: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeDirectLxmfRouter {
    pending: BTreeMap<String, PendingDirectLxmf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropagatedLxmfRouterEvent {
    pub status: OutboundStatus,
    pub evidence: LxmfDeliveryEvidence,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativePropagatedLxmfRouter;

#[derive(Clone, Debug, PartialEq)]
pub struct PropagatedNodeAccepted<'a> {
    pub peer_hash: &'a str,
    pub message_id: &'a str,
    pub propagation_node: &'a str,
    pub submitted_at: f64,
    pub transfer_state: &'a str,
    pub link_id: Option<&'a str>,
    pub representation: Option<&'a str>,
    pub observed_at: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropagatedNodeFailed<'a> {
    pub peer_hash: &'a str,
    pub message_id: &'a str,
    pub propagation_node: &'a str,
    pub submitted_at: f64,
    pub transfer_state: &'a str,
    pub link_id: Option<&'a str>,
    pub failure_reason: &'a str,
    pub observed_at: f64,
}

impl NativePropagatedLxmfRouter {
    pub fn propagation_node_accepted(
        event: PropagatedNodeAccepted<'_>,
    ) -> PropagatedLxmfRouterEvent {
        let mut detail = format!(
            "propagation_transfer_state:{};propagation_node:{};submitted_at:{:.3};receipt_state:propagation_node_accepted;delivery_state:peer_delivery_unconfirmed",
            event.transfer_state, event.propagation_node, event.submitted_at
        );
        if let Some(link_id) = event.link_id {
            detail.push_str(";propagation_link_id:");
            detail.push_str(link_id);
        }
        if let Some(representation) = event.representation {
            detail.push_str(";propagation_representation:");
            detail.push_str(representation);
        }
        PropagatedLxmfRouterEvent {
            status: OutboundStatus {
                peer_hash: event.peer_hash.into(),
                message_id: Some(event.message_id.into()),
                delivered: false,
                failed: false,
                state: OutboundDeliveryState::SubmittedToRnsNet,
                evidence: Some(detail.clone()),
                rtt: None,
            },
            evidence: LxmfDeliveryEvidence {
                peer_hash: event.peer_hash.into(),
                message_id: Some(event.message_id.into()),
                kind: LxmfDeliveryEvidenceKind::PropagationNodeAccepted,
                detail: Some(detail),
                rtt: None,
                observed_at: Some(event.observed_at),
            },
        }
    }

    pub fn propagation_node_failed(event: PropagatedNodeFailed<'_>) -> PropagatedLxmfRouterEvent {
        let mut detail = format!(
            "propagation_transfer_state:{};propagation_node:{};submitted_at:{:.3};failure_reason:{};receipt_state:propagation_node_failed;delivery_state:failed",
            event.transfer_state, event.propagation_node, event.submitted_at, event.failure_reason
        );
        if let Some(link_id) = event.link_id {
            detail.push_str(";propagation_link_id:");
            detail.push_str(link_id);
        }
        PropagatedLxmfRouterEvent {
            status: OutboundStatus {
                peer_hash: event.peer_hash.into(),
                message_id: Some(event.message_id.into()),
                delivered: false,
                failed: true,
                state: OutboundDeliveryState::Failed,
                evidence: Some(detail.clone()),
                rtt: None,
            },
            evidence: LxmfDeliveryEvidence {
                peer_hash: event.peer_hash.into(),
                message_id: Some(event.message_id.into()),
                kind: LxmfDeliveryEvidenceKind::PropagationNodeFailed,
                detail: Some(detail),
                rtt: None,
                observed_at: Some(event.observed_at),
            },
        }
    }
}

impl NativeDirectLxmfRouter {
    pub fn insert_submission(
        &mut self,
        message_id: String,
        peer_hash: String,
        submitted_at: f64,
        propagation_fallback_node: Option<String>,
    ) {
        self.pending.insert(
            message_id,
            PendingDirectLxmf {
                peer_hash,
                submitted_at,
                packet_proof_observed_at: None,
                peer_activity_observed_at: None,
                propagation_fallback_node,
                no_receipt_observed_at: None,
            },
        );
    }

    pub fn recover_direct_correlations(&mut self, messages: &[MessageSummary]) -> usize {
        let mut recovered = 0usize;
        for message in messages {
            if !message_can_recover_direct(message) {
                continue;
            }
            let Some(message_id) = message_runtime_id(message) else {
                continue;
            };
            if self.pending.contains_key(&message_id) {
                continue;
            }
            self.pending.insert(
                message_id,
                PendingDirectLxmf {
                    peer_hash: message.peer_hash.clone(),
                    submitted_at: message_submitted_at(message).unwrap_or(message.timestamp),
                    packet_proof_observed_at: message_packet_proof_observed(message)
                        .then_some(message.timestamp),
                    peer_activity_observed_at: message_peer_activity_observed(message)
                        .then_some(message.timestamp),
                    propagation_fallback_node: message_propagation_fallback_node(message),
                    no_receipt_observed_at: message_no_receipt_observed_at(message),
                },
            );
            recovered += 1;
        }
        recovered
    }

    pub fn proof_status_for_packet(
        &mut self,
        packet_hash: String,
        destination_hash: String,
        rtt: f64,
    ) -> (OutboundStatus, bool) {
        let pending = self.pending.get_mut(&packet_hash);
        let matched_pending = pending.is_some();
        let peer_hash = pending
            .as_ref()
            .map(|pending| pending.peer_hash.clone())
            .unwrap_or(destination_hash);
        if let Some(pending) = pending {
            pending.packet_proof_observed_at = Some(pending.submitted_at + rtt.max(0.0));
        }

        (
            OutboundStatus {
                peer_hash,
                message_id: Some(packet_hash),
                delivered: false,
                failed: false,
                state: OutboundDeliveryState::SubmittedToRnsNet,
                evidence: Some("rns_packet_proof".into()),
                rtt: Some(rtt),
            },
            matched_pending,
        )
    }

    pub fn inbound_peer_evidence(
        &mut self,
        message: &MessageSummary,
        detail: &str,
        observed_at: f64,
    ) -> Vec<LxmfDeliveryEvidence> {
        if message.peer_hash.is_empty() {
            return Vec::new();
        }
        self.inbound_peer_evidence_for_hashes(
            std::slice::from_ref(&message.peer_hash),
            &message.peer_hash,
            detail,
            observed_at,
        )
    }

    pub fn inbound_peer_evidence_for_hashes(
        &mut self,
        peer_hashes: &[String],
        observed_peer_hash: &str,
        detail: &str,
        observed_at: f64,
    ) -> Vec<LxmfDeliveryEvidence> {
        if peer_hashes.is_empty() {
            return Vec::new();
        }
        self.pending
            .iter_mut()
            .filter(|(_, pending)| {
                peer_hashes
                    .iter()
                    .any(|peer_hash| pending.peer_hash.eq_ignore_ascii_case(peer_hash))
            })
            .map(|(message_id, pending)| {
                pending.peer_activity_observed_at =
                    Some(pending.peer_activity_observed_at.unwrap_or(observed_at));
                LxmfDeliveryEvidence {
                    peer_hash: pending.peer_hash.clone(),
                    message_id: Some(message_id.clone()),
                    kind: LxmfDeliveryEvidenceKind::InboundPeerMessage,
                    detail: Some(format!(
                        "{detail};packet_hash:{message_id};observed_peer_hash:{observed_peer_hash};peer_activity_observed:true;observed_at:{observed_at:.3}"
                    )),
                    rtt: None,
                    observed_at: Some(observed_at),
                }
            })
            .collect()
    }

    pub fn reconcile_timeouts(
        &mut self,
        now: f64,
        timeout_seconds: f64,
    ) -> Vec<DirectLxmfTimeoutEvent> {
        self.pending
            .iter_mut()
            .filter_map(|(message_id, pending)| {
                if pending.packet_proof_observed_at.is_some()
                    || pending.peer_activity_observed_at.is_some()
                    || pending.no_receipt_observed_at.is_some()
                    || now < pending.submitted_at + timeout_seconds
                {
                    return None;
                }
                pending.no_receipt_observed_at = Some(now);
                Some(DirectLxmfTimeoutEvent {
                    peer_hash: pending.peer_hash.clone(),
                    message_id: message_id.clone(),
                    submitted_at: pending.submitted_at,
                    observed_at: now,
                    propagation_fallback_node: pending.propagation_fallback_node.clone(),
                })
            })
            .collect()
    }

    pub fn summary(&self, now: f64) -> String {
        let count = self.pending.len();
        if count == 0 {
            return "pending_lxmf_direct=0".into();
        }
        let peer_activity_observed = self
            .pending
            .values()
            .filter(|pending| pending.peer_activity_observed_at.is_some())
            .count();
        let packet_proof_observed = self
            .pending
            .values()
            .filter(|pending| pending.packet_proof_observed_at.is_some())
            .count();
        let oldest_age_secs = self
            .pending
            .values()
            .map(|pending| (now - pending.submitted_at).max(0.0))
            .fold(0.0, f64::max);
        format!(
            "pending_lxmf_direct={count} packet_proof_observed_for_pending_direct={packet_proof_observed} peer_activity_observed_for_pending_direct={peer_activity_observed} oldest_pending_lxmf_direct_age_secs={oldest_age_secs:.1}"
        )
    }

    #[cfg(test)]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub fn pending(&self, message_id: &str) -> Option<&PendingDirectLxmf> {
        self.pending.get(message_id)
    }
}

fn message_can_recover_direct(message: &MessageSummary) -> bool {
    if message.incoming || message.delivered || message.failed {
        return false;
    }
    if !matches!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("submitted_to_rns_net") | Some("submitted_to_runtime") | Some("submitted_unconfirmed")
    ) {
        return false;
    }
    let waiting_or_proof_observed =
        message
            .fields
            .get("native_lxmf_proof_state")
            .is_none_or(|value| {
                matches!(
                    value.as_str(),
                    "waiting_for_packet_proof" | "rns_packet_proof_peer_unconfirmed"
                )
            });
    let terminal_receipt = message
        .fields
        .get("native_lxmf_receipt_state")
        .is_some_and(|value| value == "lxmf_delivered" || value == "lxmf_failed");
    waiting_or_proof_observed && !terminal_receipt
}

fn message_runtime_id(message: &MessageSummary) -> Option<String> {
    message
        .fields
        .get("native_lxmf_packet_hash")
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| message.message_id.clone())
}

fn message_submitted_at(message: &MessageSummary) -> Option<f64> {
    message
        .fields
        .get("native_lxmf_submitted_at")
        .and_then(|value| value.parse::<f64>().ok())
}

fn message_peer_activity_observed(message: &MessageSummary) -> bool {
    message
        .fields
        .get("native_lxmf_peer_activity_observed")
        .is_some_and(|value| value == "true")
        || message
            .fields
            .get("native_lxmf_receipt_state")
            .is_some_and(|value| value == "peer_activity_after_send")
}

fn message_packet_proof_observed(message: &MessageSummary) -> bool {
    message
        .fields
        .get("native_lxmf_proof_state")
        .is_some_and(|value| value == "rns_packet_proof_peer_unconfirmed")
        || message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .is_some_and(|value| value == "rns_packet_proof")
}

fn message_propagation_fallback_node(message: &MessageSummary) -> Option<String> {
    message
        .fields
        .get("native_lxmf_propagation_fallback_available")
        .filter(|value| value.as_str() == "true")
        .and_then(|_| message.fields.get("native_lxmf_propagation_node"))
        .filter(|value| !value.is_empty())
        .cloned()
}

fn message_no_receipt_observed_at(message: &MessageSummary) -> Option<f64> {
    let observed = message
        .fields
        .get("native_lxmf_delivery_evidence_kind")
        .is_some_and(|value| value == "no_receipt_observed")
        || message
            .fields
            .get("native_lxmf_proof_state")
            .is_some_and(|value| value == "proof_not_observed");
    if !observed {
        return None;
    }
    message
        .fields
        .get("native_lxmf_delivery_evidence_observed_at")
        .and_then(|value| value.parse::<f64>().ok())
        .or(Some(message.timestamp))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::messaging::{MessageSummary, TransportMethod};

    use super::*;

    fn direct_message(fields: BTreeMap<String, String>) -> MessageSummary {
        MessageSummary {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            peer_label: "Peer".into(),
            title: "Title".into(),
            content: "Body".into(),
            timestamp: 10.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("packet-a".into()),
            fields,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn proof_status_matches_pending_and_keeps_correlation_open() {
        let mut router = NativeDirectLxmfRouter::default();
        router.insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            10.0,
            None,
        );

        let (status, matched) =
            router.proof_status_for_packet("packet-a".into(), "fallback".into(), 0.25);

        assert!(matched);
        assert_eq!(status.peer_hash, "00112233445566778899aabbccddeeff");
        assert_eq!(status.message_id.as_deref(), Some("packet-a"));
        assert!(!status.delivered);
        assert_eq!(status.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert_eq!(router.pending_len(), 1);
        assert!(router
            .pending("packet-a")
            .and_then(|pending| pending.packet_proof_observed_at)
            .is_some());
        assert!(router.reconcile_timeouts(60.0, 45.0).is_empty());
    }

    #[test]
    fn packet_proof_does_not_prevent_later_peer_activity_evidence() {
        let mut router = NativeDirectLxmfRouter::default();
        router.insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            10.0,
            None,
        );
        let _ = router.proof_status_for_packet("packet-a".into(), "fallback".into(), 0.25);
        let message = direct_message(BTreeMap::new());

        let evidence = router.inbound_peer_evidence(&message, "peer activity", 20.0);

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].message_id.as_deref(), Some("packet-a"));
        assert!(router
            .pending("packet-a")
            .and_then(|pending| pending.peer_activity_observed_at)
            .is_some());
    }

    #[test]
    fn inbound_peer_activity_marks_matching_pending_rows_only() {
        let mut router = NativeDirectLxmfRouter::default();
        router.insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            10.0,
            None,
        );
        router.insert_submission(
            "packet-b".into(),
            "ffffffffffffffffffffffffffffffff".into(),
            10.0,
            None,
        );
        let message = direct_message(BTreeMap::new());

        let evidence = router.inbound_peer_evidence(&message, "peer activity", 20.0);

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].message_id.as_deref(), Some("packet-a"));
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("packet_hash:packet-a")));
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("peer_activity_observed:true")));
        assert!(router
            .pending("packet-a")
            .and_then(|pending| pending.peer_activity_observed_at)
            .is_some());
        assert!(router
            .pending("packet-b")
            .and_then(|pending| pending.peer_activity_observed_at)
            .is_none());
    }

    #[test]
    fn recovers_waiting_direct_messages_from_store_rows() {
        let mut router = NativeDirectLxmfRouter::default();
        let message = direct_message(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
            (
                "native_lxmf_proof_state".into(),
                "waiting_for_packet_proof".into(),
            ),
            ("native_lxmf_packet_hash".into(), "packet-z".into()),
            ("native_lxmf_submitted_at".into(), "123.5".into()),
        ]));

        assert_eq!(router.recover_direct_correlations(&[message]), 1);
        assert_eq!(router.pending_len(), 1);
        assert_eq!(
            router
                .pending("packet-z")
                .map(|pending| pending.submitted_at),
            Some(123.5)
        );
    }

    #[test]
    fn recovers_packet_proof_correlations_from_store_rows() {
        let mut router = NativeDirectLxmfRouter::default();
        let message = direct_message(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
            (
                "native_lxmf_proof_state".into(),
                "rns_packet_proof_peer_unconfirmed".into(),
            ),
            (
                "native_lxmf_delivery_evidence_kind".into(),
                "rns_packet_proof".into(),
            ),
            ("native_lxmf_packet_hash".into(), "packet-z".into()),
            ("native_lxmf_submitted_at".into(), "123.5".into()),
        ]));

        assert_eq!(router.recover_direct_correlations(&[message]), 1);
        assert!(router
            .pending("packet-z")
            .and_then(|pending| pending.packet_proof_observed_at)
            .is_some());
        assert!(router.reconcile_timeouts(200.0, 45.0).is_empty());
    }

    #[test]
    fn reconcile_timeouts_emits_once_and_keeps_pending_for_late_proof() {
        let mut router = NativeDirectLxmfRouter::default();
        router.insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            10.0,
            Some("fedcba98765432100123456789abcdef".into()),
        );

        assert!(router.reconcile_timeouts(50.0, 45.0).is_empty());
        let events = router.reconcile_timeouts(60.0, 45.0);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message_id, "packet-a");
        assert_eq!(
            events[0].propagation_fallback_node.as_deref(),
            Some("fedcba98765432100123456789abcdef")
        );
        assert!(router.reconcile_timeouts(61.0, 45.0).is_empty());
        assert_eq!(router.pending_len(), 1);
    }

    #[test]
    fn reconcile_timeouts_skips_rows_with_peer_activity() {
        let mut router = NativeDirectLxmfRouter::default();
        router.insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            10.0,
            None,
        );
        let message = direct_message(BTreeMap::new());
        let _ = router.inbound_peer_evidence(&message, "peer activity", 20.0);

        assert!(router.reconcile_timeouts(60.0, 45.0).is_empty());
    }

    #[test]
    fn propagated_router_node_acceptance_is_peer_unconfirmed_not_delivered() {
        let event = NativePropagatedLxmfRouter::propagation_node_accepted(PropagatedNodeAccepted {
            peer_hash: "00112233445566778899aabbccddeeff",
            message_id: "message-a",
            propagation_node: "fedcba98765432100123456789abcdef",
            submitted_at: 10.5,
            transfer_state: "link_packet_sent",
            link_id: Some("01010101010101010101010101010101"),
            representation: Some("link_packet"),
            observed_at: 12.0,
        });

        assert_eq!(event.status.peer_hash, "00112233445566778899aabbccddeeff");
        assert_eq!(event.status.message_id.as_deref(), Some("message-a"));
        assert!(!event.status.delivered);
        assert!(!event.status.failed);
        assert_eq!(event.status.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert_eq!(
            event.evidence.kind,
            LxmfDeliveryEvidenceKind::PropagationNodeAccepted
        );
        let detail = event.evidence.detail.as_deref().unwrap_or_default();
        assert!(detail.contains("receipt_state:propagation_node_accepted"));
        assert!(detail.contains("delivery_state:peer_delivery_unconfirmed"));
        assert!(detail.contains("propagation_representation:link_packet"));
    }

    #[test]
    fn propagated_router_node_failure_is_terminal_failure() {
        let event = NativePropagatedLxmfRouter::propagation_node_failed(PropagatedNodeFailed {
            peer_hash: "00112233445566778899aabbccddeeff",
            message_id: "message-a",
            propagation_node: "fedcba98765432100123456789abcdef",
            submitted_at: 10.5,
            transfer_state: "resource_failed",
            link_id: Some("01010101010101010101010101010101"),
            failure_reason: "resource transfer timeout",
            observed_at: 12.0,
        });

        assert_eq!(event.status.state, OutboundDeliveryState::Failed);
        assert!(!event.status.delivered);
        assert!(event.status.failed);
        assert_eq!(
            event.evidence.kind,
            LxmfDeliveryEvidenceKind::PropagationNodeFailed
        );
        let detail = event.status.evidence.as_deref().unwrap_or_default();
        assert!(detail.contains("receipt_state:propagation_node_failed"));
        assert!(detail.contains("failure_reason:resource transfer timeout"));
    }
}
