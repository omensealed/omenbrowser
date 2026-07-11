use crate::messaging::MessageSummary;

use super::format_epoch_secs;
use super::message_labels::{
    lxmf_delivery_evidence_label, lxmf_propagation_transfer_label, lxmf_receipt_state_label,
    lxmf_state_label,
};
use super::message_stamp::lxmf_stamp_status_lines;

pub(crate) fn lxmf_message_secondary_status_lines(message: &MessageSummary) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(lxmf_stamp_status_lines(message));
    if let Some(packet_hash) = message.fields.get("native_lxmf_packet_hash") {
        lines.push(format!("packet: {packet_hash}"));
    }
    if let Some(transfer) = message.fields.get("native_lxmf_propagation_transfer_state") {
        lines.push(format!(
            "propagation: {}",
            lxmf_propagation_transfer_label(transfer)
        ));
    }
    if let Some(state) = message.fields.get("native_lxmf_propagation_state") {
        lines.push(format!("propagation state: {}", lxmf_state_label(state)));
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_fallback")
            .map(String::as_str),
        Some("direct_to_propagated")
    ) {
        lines.push("fallback: direct send failed; queued via propagation".into());
    }
    if let Some(kind) = message.fields.get("native_lxmf_delivery_evidence_kind") {
        lines.push(format!("evidence: {}", lxmf_delivery_evidence_label(kind)));
    }
    if let Some(kind) = message
        .fields
        .get("native_lxmf_propagation_sync_evidence_kind")
    {
        lines.push(format!(
            "sync evidence: {}",
            lxmf_delivery_evidence_label(kind)
        ));
    }
    if let Some(receipt_state) = message
        .fields
        .get("native_lxmf_propagation_sync_receipt_state")
    {
        lines.push(format!(
            "sync receipt: {}",
            lxmf_receipt_state_label(receipt_state)
        ));
    }
    if let Some(guidance) = message.fields.get("native_lxmf_propagation_sync_guidance") {
        lines.push(format!("sync next: {guidance}"));
    }
    if let Some(receipt_state) = message.fields.get("native_lxmf_receipt_state") {
        lines.push(format!(
            "receipt: {}",
            lxmf_receipt_state_label(receipt_state)
        ));
    }
    if let Some(rtt) = message.fields.get("native_lxmf_rtt") {
        lines.push(format!("proof RTT: {rtt}s"));
    }
    if let Some(guidance) = message.fields.get("native_lxmf_retry_guidance") {
        lines.push(format!("next: {guidance}"));
    }
    if let Some(retry_after) = message
        .fields
        .get("native_lxmf_retry_after_epoch_secs")
        .and_then(|value| value.parse::<f64>().ok())
    {
        lines.push(format!("retry after: {}", format_epoch_secs(retry_after)));
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::messaging::{MessageSummary, TransportMethod};

    use super::*;

    fn message_with_fields(fields: BTreeMap<String, String>) -> MessageSummary {
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
            message_id: Some("packet-1".into()),
            fields,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn lxmf_message_secondary_status_lines_use_human_propagation_labels() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_fallback".into(), "direct_to_propagated".into()),
            (
                "native_lxmf_propagation_transfer_state".into(),
                "router_deferred".into(),
            ),
            (
                "native_lxmf_propagation_state".into(),
                "queued_for_propagation".into(),
            ),
        ]));

        let lines = lxmf_message_secondary_status_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "propagation: queued; waiting for propagation node readiness"));
        assert!(lines
            .iter()
            .any(|line| line == "propagation state: queued for propagation"));
        assert!(lines
            .iter()
            .any(|line| line == "fallback: direct send failed; queued via propagation"));
    }

    #[test]
    fn lxmf_message_secondary_status_lines_keep_details_compact() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_packet_hash".into(), "packet-1".into()),
            (
                "native_lxmf_delivery_evidence_kind".into(),
                "rns_packet_proof".into(),
            ),
            ("native_lxmf_rtt".into(), "0.125".into()),
        ]));

        let lines = lxmf_message_secondary_status_lines(&message);

        assert_eq!(lines[0], "packet: packet-1");
        assert!(lines
            .iter()
            .any(|line| line == "evidence: RNS packet proof observed"));
        assert!(lines.iter().any(|line| line == "proof RTT: 0.125s"));
    }

    #[test]
    fn lxmf_message_secondary_status_lines_separate_propagation_sync_from_delivery_evidence() {
        let mut message = message_with_fields(BTreeMap::from([
            (
                "native_lxmf_propagation_sync_evidence_kind".into(),
                "propagation_sync_no_payloads".into(),
            ),
            (
                "native_lxmf_propagation_sync_receipt_state".into(),
                "propagation_sync_no_peer_payload".into(),
            ),
        ]));
        message.transport_method = TransportMethod::Propagated;

        let lines = lxmf_message_secondary_status_lines(&message);
        assert!(lines
            .iter()
            .any(|line| line == "sync evidence: propagation sync found no new peer payload"));
        assert!(lines
            .iter()
            .any(|line| line == "sync receipt: propagation sync found no new peer payload"));
        assert!(!lines.iter().any(|line| line.starts_with("evidence:")));
    }
}
