use crate::messaging::MessageSummary;

use super::format_epoch_secs;
use super::message_labels::{
    lxmf_delivery_evidence_label, lxmf_proof_state_label, lxmf_receipt_state_label,
};

pub(crate) fn append_lxmf_message_detail_status_lines(
    lines: &mut Vec<String>,
    message: &MessageSummary,
    state: Option<&String>,
) {
    if let Some(proof_state) = message.fields.get("native_lxmf_proof_state") {
        lines.push(format!("proof: {}", lxmf_proof_state_label(proof_state)));
    } else if state.map(String::as_str) == Some("submitted_to_rns_net") {
        lines.push("proof: waiting for packet proof".into());
    }
    if let Some(receipt_state) = message.fields.get("native_lxmf_receipt_state") {
        lines.push(format!(
            "receipt: {}",
            lxmf_receipt_state_label(receipt_state)
        ));
    }
    if let Some(kind) = message.fields.get("native_lxmf_delivery_evidence_kind") {
        lines.push(format!(
            "delivery evidence: {}",
            lxmf_delivery_evidence_label(kind)
        ));
    }
    if let Some(kind) = message
        .fields
        .get("native_lxmf_propagation_sync_evidence_kind")
    {
        lines.push(format!(
            "propagation sync evidence: {}",
            lxmf_delivery_evidence_label(kind)
        ));
    }
    if let Some(receipt) = message
        .fields
        .get("native_lxmf_propagation_sync_receipt_state")
    {
        lines.push(format!(
            "propagation sync receipt: {}",
            lxmf_receipt_state_label(receipt)
        ));
    }
    if let Some(detail) = message.fields.get("native_lxmf_propagation_sync_detail") {
        lines.push(format!("propagation sync detail: {detail}"));
    }
    if let Some(observed_at) = message
        .fields
        .get("native_lxmf_propagation_sync_observed_at")
        .and_then(|value| value.parse::<f64>().ok())
    {
        lines.push(format!(
            "propagation sync observed: {}",
            format_epoch_secs(observed_at)
        ));
    }
    if let Some(detail) = message.fields.get("native_lxmf_delivery_evidence_detail") {
        lines.push(format!("delivery detail: {detail}"));
    }
    if let Some(observed_at) = message
        .fields
        .get("native_lxmf_delivery_evidence_observed_at")
        .and_then(|value| value.parse::<f64>().ok())
    {
        lines.push(format!(
            "evidence observed: {}",
            format_epoch_secs(observed_at)
        ));
    }
    if let Some(rtt) = message.fields.get("native_lxmf_rtt") {
        lines.push(format!("proof RTT: {rtt}s"));
    }
    if let Some(reason) = message.fields.get("native_lxmf_failure_reason") {
        lines.push(format!("failure: {reason}"));
    }
    if let Some(reason) = message.fields.get("native_lxmf_uncertain_reason") {
        lines.push(format!("unconfirmed: {reason}"));
    }
    if let Some(next_action) = message.fields.get("native_lxmf_next_action") {
        lines.push(format!("next action: {next_action}"));
    }
    if let Some(attempt) = message.fields.get("native_lxmf_retry_attempt") {
        lines.push(format!("retry attempt: {attempt}"));
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

    fn detail_lines(message: &MessageSummary) -> Vec<String> {
        let mut lines = Vec::new();
        append_lxmf_message_detail_status_lines(
            &mut lines,
            message,
            message.fields.get("native_lxmf_state"),
        );
        lines
    }

    #[test]
    fn lxmf_message_detail_status_lines_show_submission_and_proof_wait() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
            (
                "native_lxmf_retry_guidance".into(),
                "wait for packet proof".into(),
            ),
        ]));

        let lines = detail_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "proof: waiting for packet proof"));
        assert!(lines
            .iter()
            .any(|line| line == "next: wait for packet proof"));
    }

    #[test]
    fn lxmf_message_detail_status_lines_show_unconfirmed_receipt_state() {
        let message = message_with_fields(BTreeMap::from([
            (
                "native_lxmf_proof_state".into(),
                "proof_not_observed".into(),
            ),
            (
                "native_lxmf_receipt_state".into(),
                "lxmf_delivery_receipt_unavailable_native_wire".into(),
            ),
            (
                "native_lxmf_uncertain_reason".into(),
                "native direct send lacks LXMF router callback parity".into(),
            ),
        ]));

        let lines = detail_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "proof: no packet proof observed"));
        assert!(lines
            .iter()
            .any(|line| line == "receipt: native wire has no confirmed LXMF peer receipt"));
        assert!(lines.iter().any(|line| {
            line == "unconfirmed: native direct send lacks LXMF router callback parity"
        }));
    }

    #[test]
    fn lxmf_message_detail_status_lines_show_delivery_evidence_detail() {
        let message = message_with_fields(BTreeMap::from([
            (
                "native_lxmf_delivery_evidence_kind".into(),
                "inbound_peer_message".into(),
            ),
            (
                "native_lxmf_delivery_evidence_detail".into(),
                "propagation sync received LXMF from peer with pending direct outbound".into(),
            ),
            (
                "native_lxmf_delivery_evidence_observed_at".into(),
                "123.456".into(),
            ),
        ]));

        let lines = detail_lines(&message);

        assert!(lines.iter().any(|line| {
            line == "delivery evidence: inbound LXMF activity from this peer after send"
        }));
        assert!(lines.iter().any(|line| {
            line == "delivery detail: propagation sync received LXMF from peer with pending direct outbound"
        }));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("evidence observed: ")));
    }

    #[test]
    fn lxmf_message_detail_status_lines_separate_propagation_sync_from_delivery_evidence() {
        let mut message = message_with_fields(BTreeMap::from([
            (
                "native_lxmf_state".into(),
                "propagation_node_accepted".into(),
            ),
            (
                "native_lxmf_receipt_state".into(),
                "propagation_node_accepted_peer_unconfirmed".into(),
            ),
            (
                "native_lxmf_propagation_sync_evidence_kind".into(),
                "propagation_sync_no_payloads".into(),
            ),
            (
                "native_lxmf_propagation_sync_receipt_state".into(),
                "propagation_sync_no_peer_payload".into(),
            ),
            (
                "native_lxmf_propagation_sync_detail".into(),
                "requested:0;decoded:0;haves:2".into(),
            ),
        ]));
        message.transport_method = TransportMethod::Propagated;

        let lines = detail_lines(&message);
        assert!(lines.iter().any(|line| {
            line == "propagation sync evidence: propagation sync found no new peer payload"
        }));
        assert!(lines
            .iter()
            .any(|line| line == "propagation sync detail: requested:0;decoded:0;haves:2"));
        assert!(!lines
            .iter()
            .any(|line| line.starts_with("delivery evidence:")));
    }
}
