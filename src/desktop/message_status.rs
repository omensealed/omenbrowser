use crate::messaging::MessageSummary;

use super::format_epoch_secs;
use super::message_detail::append_lxmf_message_detail_status_lines;
use super::message_labels::{
    lxmf_fallback_label, lxmf_propagation_transfer_label, lxmf_state_label,
};
use super::message_stamp::lxmf_stamp_status_lines;

pub(crate) fn lxmf_message_status_lines(message: &MessageSummary) -> Vec<String> {
    let mut lines = Vec::new();
    let state = message.fields.get("native_lxmf_state");
    if let Some(state) = state {
        lines.push(format!("LXMF state: {}", lxmf_state_label(state)));
    } else if message
        .fields
        .keys()
        .any(|key| key.starts_with("native_lxmf_"))
    {
        lines.push("LXMF state: evidence recorded".into());
    } else {
        return Vec::new();
    }
    lines.extend(lxmf_stamp_status_lines(message));
    if let Some(packet_hash) = message.fields.get("native_lxmf_packet_hash") {
        lines.push(format!("packet: {packet_hash}"));
    }
    if let Some(message_id) = message.fields.get("native_lxmf_message_id") {
        lines.push(format!("message id: {message_id}"));
    }
    if let Some(node) = message.fields.get("native_lxmf_propagation_node") {
        lines.push(format!("propagation node: {node}"));
    }
    if let Some(transfer) = message.fields.get("native_lxmf_propagation_transfer_state") {
        lines.push(format!(
            "propagation transfer: {}",
            lxmf_propagation_transfer_label(transfer)
        ));
    }
    if let Some(fallback) = message.fields.get("native_lxmf_fallback") {
        lines.push(format!("fallback: {}", lxmf_fallback_label(fallback)));
    }
    if let Some(link_id) = message
        .fields
        .get("native_lxmf_propagation_link_id")
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("propagation link: {link_id}"));
    }
    if let (Some(received), Some(total)) = (
        message.fields.get("native_lxmf_resource_received"),
        message.fields.get("native_lxmf_resource_total"),
    ) {
        lines.push(format!("resource: {received}/{total} parts"));
    }
    if let Some(submitted_at) = message
        .fields
        .get("native_lxmf_submitted_at")
        .and_then(|value| value.parse::<f64>().ok())
    {
        lines.push(format!("submitted: {}", format_epoch_secs(submitted_at)));
    }
    append_lxmf_message_detail_status_lines(&mut lines, message, state);
    lines
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::messaging::{MessageSummary, TransportMethod};

    use super::super::message_compact::lxmf_message_compact_status;
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
    fn lxmf_message_status_lines_show_submission_and_proof_wait() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
            ("native_lxmf_packet_hash".into(), "packet-b".into()),
            (
                "native_lxmf_retry_guidance".into(),
                "wait for packet proof".into(),
            ),
        ]));

        let lines = lxmf_message_status_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "LXMF state: submitted; waiting for proof or peer activity"));
        assert!(lines.iter().any(|line| line == "packet: packet-b"));
        assert!(lines
            .iter()
            .any(|line| line == "proof: waiting for packet proof"));
        assert!(lines
            .iter()
            .any(|line| line == "next: wait for packet proof"));
    }

    #[test]
    fn lxmf_message_status_lines_show_failure_reason() {
        let mut message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "failed".into()),
            (
                "native_lxmf_failure_reason".into(),
                "LXMF peer path is not known".into(),
            ),
            (
                "native_lxmf_retry_guidance".into(),
                "request path and retry".into(),
            ),
        ]));
        message.failed = true;

        let lines = lxmf_message_status_lines(&message);

        assert!(lines.iter().any(|line| line == "LXMF state: failed"));
        assert!(lines
            .iter()
            .any(|line| line == "failure: LXMF peer path is not known"));
        assert!(lines
            .iter()
            .any(|line| line == "next: request path and retry"));
    }

    #[test]
    fn lxmf_message_status_lines_show_direct_to_propagated_fallback() {
        let mut message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "queued_for_propagation".into()),
            ("native_lxmf_fallback".into(), "direct_to_propagated".into()),
            (
                "native_lxmf_failure_reason".into(),
                "direct path missing".into(),
            ),
            (
                "native_lxmf_propagation_transfer_state".into(),
                "router_deferred".into(),
            ),
        ]));
        message.transport_method = TransportMethod::Propagated;

        let lines = lxmf_message_status_lines(&message);

        assert_eq!(
            lxmf_message_compact_status(&message).as_deref(),
            Some("Direct failed; queued via propagation")
        );
        assert!(lines
            .iter()
            .any(|line| line == "fallback: direct send failed; queued via propagation"));
        assert!(lines.iter().any(|line| {
            line == "propagation transfer: queued; waiting for propagation node readiness"
        }));
        assert!(lines
            .iter()
            .any(|line| line == "failure: direct path missing"));
    }

    #[test]
    fn lxmf_message_status_lines_show_ticket_evidence_without_state() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_reply_ticket_offered".into(), "true".into()),
            ("native_lxmf_reply_ticket_used".into(), "true".into()),
            ("native_lxmf_stamp_state".into(), "ticket_stamp".into()),
            ("native_lxmf_reply_ticket_state".into(), "valid".into()),
            (
                "native_lxmf_reply_ticket_expires".into(),
                "1782921166.557".into(),
            ),
        ]));

        let lines = lxmf_message_status_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "LXMF state: evidence recorded"));
        assert!(lines
            .iter()
            .any(|line| line == "ticket: reply ticket offered"));
        assert!(lines
            .iter()
            .any(|line| line == "ticket: remembered reply ticket used"));
        assert!(lines.iter().any(|line| line == "stamp: reply ticket"));
        assert!(lines.iter().any(|line| line == "reply ticket: valid"));
        assert!(lines
            .iter()
            .any(|line| line.starts_with("reply ticket expires: ")));
    }

    #[test]
    fn lxmf_message_status_lines_show_direct_stamp_cost_evidence() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_stamp_state".into(), "direct_stamp".into()),
            ("native_lxmf_direct_stamp_cost".into(), "8".into()),
            ("native_lxmf_direct_stamp_value".into(), "10".into()),
            ("native_lxmf_direct_stamp_attempts".into(), "42".into()),
        ]));

        let lines = lxmf_message_status_lines(&message);

        assert!(lines.iter().any(|line| line == "stamp: direct cost stamp"));
        assert!(lines
            .iter()
            .any(|line| line == "direct stamp: cost 8, value 10, attempts 42"));
    }
}
