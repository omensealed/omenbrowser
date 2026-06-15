use crate::messaging::MessageSummary;

use super::format_epoch_secs;

pub(crate) fn lxmf_message_compact_status(message: &MessageSummary) -> Option<String> {
    if message.incoming {
        return Some(if message.unread {
            "Unread incoming LXMF".into()
        } else {
            "Incoming LXMF".into()
        });
    }
    if message.failed {
        return Some(
            message
                .fields
                .get("native_lxmf_failure_reason")
                .map(|reason| format!("Failed: {reason}"))
                .unwrap_or_else(|| "Failed".into()),
        );
    }
    let direct_to_propagated = matches!(
        message
            .fields
            .get("native_lxmf_fallback")
            .map(String::as_str),
        Some("direct_to_propagated")
    );
    let propagation_transfer = message
        .fields
        .get("native_lxmf_propagation_transfer_state")
        .map(String::as_str);
    if direct_to_propagated && propagation_transfer == Some("router_deferred") {
        return Some("Direct failed; queued via propagation".into());
    }
    if let Some(transfer) = propagation_transfer {
        return Some(match transfer {
            "link_packet_sent" => "Propagation node accepted; peer unconfirmed".into(),
            "resource_completed" => "Propagation transfer complete; peer unconfirmed".into(),
            "resource_advertised" => "Propagation node accepted; peer unconfirmed".into(),
            "resource_progress" => {
                let progress = match (
                    message.fields.get("native_lxmf_resource_received"),
                    message.fields.get("native_lxmf_resource_total"),
                ) {
                    (Some(received), Some(total)) => format!(" {received}/{total}"),
                    _ => String::new(),
                };
                format!("Propagation transfer in progress{progress}")
            }
            "router_deferred" => "Propagation queued; waiting for node readiness".into(),
            "link_timeout" => "Propagation link timed out".into(),
            "resource_advertise_failed" | "resource_failed" => "Propagation transfer failed".into(),
            _ => format!("Propagation: {transfer}"),
        });
    }
    if message.delivered {
        return Some(if message.fields.contains_key("native_lxmf_state") {
            "LXMF router delivered; no retry needed".into()
        } else {
            "Delivered".into()
        });
    }
    if direct_to_propagated {
        return Some("Direct failed; queued via propagation".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("inbound_peer_message")
    ) {
        return Some("Peer activity seen after send".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("rns_packet_proof_peer_unconfirmed")
    ) {
        return Some("RNS packet proof; peer delivery unconfirmed".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("proof_received")
    ) {
        return Some("RNS packet proof received".into());
    }
    match message.fields.get("native_lxmf_state").map(String::as_str) {
        Some("submitted_to_rns_net") => Some("Submitted; waiting for proof or reply".into()),
        Some("transport_proof_received") => {
            Some("RNS transport proof; peer delivery unconfirmed".into())
        }
        Some("submitted_unconfirmed") => Some("Submitted; peer receipt unconfirmed".into()),
        Some("propagation_retry_ready") => {
            Some("Direct unconfirmed; propagation retry ready".into())
        }
        Some("propagation_node_accepted") => {
            Some("Propagation node accepted; peer unconfirmed".into())
        }
        Some("propagation_sync_no_payloads") => {
            Some("Propagation sync returned no peer payload".into())
        }
        Some("propagation_transfer_completed") => {
            Some("Propagation transfer complete; peer unconfirmed".into())
        }
        Some("queued_for_propagation") => Some("Queued for propagation".into()),
        Some("submitted_to_runtime") => Some("Submitted to runtime".into()),
        Some("failed") => Some("Failed".into()),
        Some(state) => Some(format!("LXMF: {state}")),
        None => None,
    }
}

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

pub(crate) fn lxmf_message_compact_stamp_status(message: &MessageSummary) -> Option<String> {
    if message.incoming {
        return None;
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_used")
            .map(String::as_str),
        Some("true")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_stamp_state")
            .map(String::as_str),
        Some("ticket_stamp")
    ) {
        return Some("stamp: reply ticket".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_stamp_state")
            .map(String::as_str),
        Some("direct_stamp")
    ) {
        return Some(match message.fields.get("native_lxmf_direct_stamp_cost") {
            Some(cost) => format!("stamp: direct cost {cost}"),
            None => "stamp: direct cost".into(),
        });
    }
    if let Some(cost) = message.fields.get("native_lxmf_propagation_stamp_cost") {
        return Some(format!("stamp: propagation cost {cost}"));
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_offered")
            .map(String::as_str),
        Some("true")
    ) {
        return Some("ticket: reply ticket offered".into());
    }
    None
}

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
    lines
}

fn lxmf_stamp_status_lines(message: &MessageSummary) -> Vec<String> {
    let mut lines = Vec::new();
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_offered")
            .map(String::as_str),
        Some("true")
    ) {
        lines.push("ticket: reply ticket offered".into());
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_reply_ticket_used")
            .map(String::as_str),
        Some("true")
    ) {
        lines.push("ticket: remembered reply ticket used".into());
    }
    if let Some(stamp_state) = message.fields.get("native_lxmf_stamp_state") {
        lines.push(format!("stamp: {}", lxmf_stamp_state_label(stamp_state)));
        if stamp_state == "direct_stamp" {
            push_lxmf_stamp_cost_line(
                &mut lines,
                "direct stamp",
                message.fields.get("native_lxmf_direct_stamp_cost"),
                message.fields.get("native_lxmf_direct_stamp_value"),
                message.fields.get("native_lxmf_direct_stamp_attempts"),
            );
        }
    }
    if message
        .fields
        .contains_key("native_lxmf_propagation_stamp_cost")
    {
        push_lxmf_stamp_cost_line(
            &mut lines,
            "propagation stamp",
            message.fields.get("native_lxmf_propagation_stamp_cost"),
            message.fields.get("native_lxmf_propagation_stamp_value"),
            message.fields.get("native_lxmf_propagation_stamp_attempts"),
        );
    }
    if let Some(ticket_state) = message.fields.get("native_lxmf_reply_ticket_state") {
        lines.push(format!("reply ticket: {ticket_state}"));
    }
    if let Some(expires) = message
        .fields
        .get("native_lxmf_reply_ticket_expires")
        .and_then(|value| value.parse::<f64>().ok())
    {
        lines.push(format!(
            "reply ticket expires: {}",
            format_epoch_secs(expires)
        ));
    }
    lines
}

fn push_lxmf_stamp_cost_line(
    lines: &mut Vec<String>,
    label: &str,
    cost: Option<&String>,
    value: Option<&String>,
    attempts: Option<&String>,
) {
    match (cost, value, attempts) {
        (Some(cost), Some(value), Some(attempts)) => {
            lines.push(format!(
                "{label}: cost {cost}, value {value}, attempts {attempts}"
            ));
        }
        (Some(cost), Some(value), None) => {
            lines.push(format!("{label}: cost {cost}, value {value}"));
        }
        (Some(cost), None, _) => {
            lines.push(format!("{label}: cost {cost}"));
        }
        _ => {}
    }
}

fn lxmf_stamp_state_label(state: &str) -> &str {
    match state {
        "ticket_stamp" => "reply ticket",
        "direct_stamp" => "direct cost stamp",
        "propagation_stamp" => "propagation cost stamp",
        other => other,
    }
}

pub(crate) fn desktop_message_is_retry_candidate(message: &MessageSummary) -> bool {
    if message.incoming || message.delivered {
        return false;
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_delivery_evidence_kind")
            .map(String::as_str),
        Some("inbound_peer_message")
    ) {
        return false;
    }
    if matches!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("rns_packet_proof_peer_unconfirmed")
    ) {
        return false;
    }
    if message.failed {
        return true;
    }
    if message_has_useful_direct_transfer_evidence(message) {
        return message
            .fields
            .get("native_lxmf_propagation_fallback_available")
            .map(String::as_str)
            == Some("true");
    }
    matches!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("failed" | "submitted_unconfirmed" | "propagation_retry_ready")
    ) || matches!(
        message
            .fields
            .get("native_lxmf_receipt_state")
            .map(String::as_str),
        Some("lxmf_delivery_receipt_unavailable_native_wire")
    )
}

fn message_has_useful_direct_transfer_evidence(message: &MessageSummary) -> bool {
    matches!(
        message
            .fields
            .get("native_lxmf_direct_transfer_state")
            .map(String::as_str),
        Some(
            "link_packet_sent"
                | "resource_completed"
                | "resource_timeout"
                | "resource_progress"
                | "resource_advertised"
        )
    )
}

pub(crate) fn desktop_message_propagation_sync_label(
    message: &MessageSummary,
) -> Option<&'static str> {
    if message.incoming || message.delivered || message.failed {
        return None;
    }
    match message
        .fields
        .get("native_lxmf_next_action")
        .map(String::as_str)
    {
        Some("sync_propagation") => Some("Sync propagation"),
        Some("sync_propagation_again") => Some("Sync again"),
        _ => match message.fields.get("native_lxmf_state").map(String::as_str) {
            Some("propagation_node_accepted") => Some("Sync propagation"),
            Some("propagation_sync_no_payloads") => Some("Sync again"),
            _ => None,
        },
    }
}

pub(crate) struct DesktopMessageRetryLabels {
    pub(crate) prepare: &'static str,
    pub(crate) send: &'static str,
}

pub(crate) fn desktop_message_retry_labels(message: &MessageSummary) -> DesktopMessageRetryLabels {
    match message.fields.get("native_lxmf_state").map(String::as_str) {
        Some("propagation_retry_ready") => DesktopMessageRetryLabels {
            prepare: "Retry via propagation",
            send: "Send via propagation",
        },
        Some("queued_for_propagation") => DesktopMessageRetryLabels {
            prepare: "Retry propagation",
            send: "Send propagation retry",
        },
        Some("propagation_node_accepted") => DesktopMessageRetryLabels {
            prepare: "Prepare resend",
            send: "Resend via propagation",
        },
        Some("propagation_sync_no_payloads" | "propagation_transfer_completed") => {
            DesktopMessageRetryLabels {
                prepare: "Prepare resend",
                send: "Resend via propagation",
            }
        }
        Some("submitted_unconfirmed") => DesktopMessageRetryLabels {
            prepare: "Retry unconfirmed",
            send: "Send retry",
        },
        Some("submitted_to_rns_net" | "submitted_to_runtime") => DesktopMessageRetryLabels {
            prepare: "Retry pending send",
            send: "Send retry",
        },
        _ if message.failed => DesktopMessageRetryLabels {
            prepare: "Retry failed",
            send: "Send retry",
        },
        _ => DesktopMessageRetryLabels {
            prepare: "Retry this",
            send: "Send retry",
        },
    }
}

fn lxmf_delivery_evidence_label(kind: &str) -> &'static str {
    crate::messaging::lxmf_labels::delivery_evidence(kind)
}

fn lxmf_state_label(state: &str) -> &'static str {
    crate::messaging::lxmf_labels::state(state)
}

fn lxmf_proof_state_label(state: &str) -> &'static str {
    crate::messaging::lxmf_labels::proof_state(state)
}

fn lxmf_receipt_state_label(state: &str) -> &'static str {
    crate::messaging::lxmf_labels::receipt_state(state)
}

fn lxmf_fallback_label(fallback: &str) -> &'static str {
    crate::messaging::lxmf_labels::fallback(fallback)
}

fn lxmf_propagation_transfer_label(transfer: &str) -> &'static str {
    crate::messaging::lxmf_labels::propagation_transfer(transfer)
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
    fn lxmf_message_status_lines_show_unconfirmed_receipt_state() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_unconfirmed".into()),
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

        let lines = lxmf_message_status_lines(&message);

        assert!(lines
            .iter()
            .any(|line| line == "LXMF state: submitted; peer receipt unconfirmed"));
        assert!(lines
            .iter()
            .any(|line| line == "proof: no packet proof observed"));
        assert!(lines
            .iter()
            .any(|line| { line == "receipt: native wire has no confirmed LXMF peer receipt" }));
        assert!(lines.iter().any(|line| {
            line == "unconfirmed: native direct send lacks LXMF router callback parity"
        }));
    }

    #[test]
    fn lxmf_message_status_lines_show_delivery_evidence_detail() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
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

        let lines = lxmf_message_status_lines(&message);

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
    fn lxmf_message_compact_status_prefers_human_messenger_states() {
        let mut message = message_with_fields(BTreeMap::from([(
            "native_lxmf_state".into(),
            "submitted_to_rns_net".into(),
        )]));

        assert_eq!(
            lxmf_message_compact_status(&message).as_deref(),
            Some("Submitted; waiting for proof or reply")
        );

        message.fields.insert(
            "native_lxmf_delivery_evidence_kind".into(),
            "inbound_peer_message".into(),
        );
        assert_eq!(
            lxmf_message_compact_status(&message).as_deref(),
            Some("Peer activity seen after send")
        );

        message.fields.remove("native_lxmf_delivery_evidence_kind");
        message.fields.insert(
            "native_lxmf_proof_state".into(),
            "rns_packet_proof_peer_unconfirmed".into(),
        );
        assert_eq!(
            lxmf_message_compact_status(&message).as_deref(),
            Some("RNS packet proof; peer delivery unconfirmed")
        );

        message.failed = true;
        message
            .fields
            .insert("native_lxmf_failure_reason".into(), "path unknown".into());
        assert_eq!(
            lxmf_message_compact_status(&message).as_deref(),
            Some("Failed: path unknown")
        );
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
    fn lxmf_message_secondary_status_lines_show_ticket_summary() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_reply_ticket_used".into(), "true".into()),
            ("native_lxmf_stamp_state".into(), "ticket_stamp".into()),
        ]));

        let lines = lxmf_message_secondary_status_lines(&message);

        assert_eq!(lines[0], "ticket: remembered reply ticket used");
        assert_eq!(lines[1], "stamp: reply ticket");
    }

    #[test]
    fn lxmf_message_compact_stamp_status_summarizes_reply_ticket() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_reply_ticket_used".into(), "true".into()),
            ("native_lxmf_stamp_state".into(), "ticket_stamp".into()),
        ]));

        assert_eq!(
            lxmf_message_compact_stamp_status(&message).as_deref(),
            Some("stamp: reply ticket")
        );
    }

    #[test]
    fn lxmf_message_compact_stamp_status_summarizes_direct_cost() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_stamp_state".into(), "direct_stamp".into()),
            ("native_lxmf_direct_stamp_cost".into(), "8".into()),
        ]));

        assert_eq!(
            lxmf_message_compact_stamp_status(&message).as_deref(),
            Some("stamp: direct cost 8")
        );
    }

    #[test]
    fn lxmf_message_compact_stamp_status_summarizes_propagation_cost() {
        let message = message_with_fields(BTreeMap::from([(
            "native_lxmf_propagation_stamp_cost".into(),
            "16".into(),
        )]));

        assert_eq!(
            lxmf_message_compact_stamp_status(&message).as_deref(),
            Some("stamp: propagation cost 16")
        );
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

    #[test]
    fn lxmf_message_secondary_status_lines_show_propagation_stamp_evidence() {
        let message = message_with_fields(BTreeMap::from([
            ("native_lxmf_propagation_stamp_cost".into(), "16".into()),
            ("native_lxmf_propagation_stamp_value".into(), "17".into()),
            (
                "native_lxmf_propagation_stamp_attempts".into(),
                "654".into(),
            ),
        ]));

        let lines = lxmf_message_secondary_status_lines(&message);

        assert_eq!(
            lines[0],
            "propagation stamp: cost 16, value 17, attempts 654"
        );
    }

    #[test]
    fn lxmf_message_status_lines_separate_propagation_sync_from_delivery_evidence() {
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
        message.message_id = Some("transient-1".into());

        let secondary = lxmf_message_secondary_status_lines(&message);
        assert!(secondary
            .iter()
            .any(|line| line == "sync evidence: propagation sync found no new peer payload"));
        assert!(secondary
            .iter()
            .any(|line| line == "sync receipt: propagation sync found no new peer payload"));
        assert!(!secondary.iter().any(|line| line.starts_with("evidence:")));

        let details = lxmf_message_status_lines(&message);
        assert!(details.iter().any(|line| {
            line == "propagation sync evidence: propagation sync found no new peer payload"
        }));
        assert!(details
            .iter()
            .any(|line| line == "propagation sync detail: requested:0;decoded:0;haves:2"));
        assert!(!details
            .iter()
            .any(|line| line.starts_with("delivery evidence:")));
    }

    #[test]
    fn desktop_message_is_retry_candidate_matches_failed_and_unconfirmed() {
        let mut message = message_with_fields(BTreeMap::from([(
            "native_lxmf_state".into(),
            "submitted_unconfirmed".into(),
        )]));

        assert!(desktop_message_is_retry_candidate(&message));

        message.delivered = true;
        assert!(!desktop_message_is_retry_candidate(&message));

        message.delivered = false;
        message.incoming = true;
        assert!(!desktop_message_is_retry_candidate(&message));
    }

    #[test]
    fn desktop_message_retry_candidate_avoids_pending_and_sync_first_states() {
        let mut message = message_with_fields(BTreeMap::from([(
            "native_lxmf_state".into(),
            "submitted_to_rns_net".into(),
        )]));

        assert!(!desktop_message_is_retry_candidate(&message));

        message
            .fields
            .insert("native_lxmf_state".into(), "submitted_to_runtime".into());
        assert!(!desktop_message_is_retry_candidate(&message));

        message
            .fields
            .insert("native_lxmf_state".into(), "queued_for_propagation".into());
        assert!(!desktop_message_is_retry_candidate(&message));

        message.fields.insert(
            "native_lxmf_state".into(),
            "propagation_sync_no_payloads".into(),
        );
        assert!(!desktop_message_is_retry_candidate(&message));
        assert_eq!(
            desktop_message_propagation_sync_label(&message),
            Some("Sync again")
        );
    }

    #[test]
    fn desktop_message_is_not_retry_candidate_after_native_proof_or_peer_activity() {
        let mut message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
            (
                "native_lxmf_proof_state".into(),
                "rns_packet_proof_peer_unconfirmed".into(),
            ),
        ]));

        assert!(!desktop_message_is_retry_candidate(&message));

        message.fields.remove("native_lxmf_proof_state");
        message.fields.insert(
            "native_lxmf_delivery_evidence_kind".into(),
            "inbound_peer_message".into(),
        );
        assert!(!desktop_message_is_retry_candidate(&message));
    }

    #[test]
    fn desktop_message_retry_candidate_avoids_direct_transfer_evidence_without_fallback() {
        let mut message = message_with_fields(BTreeMap::from([
            ("native_lxmf_state".into(), "submitted_unconfirmed".into()),
            (
                "native_lxmf_direct_transfer_state".into(),
                "link_packet_sent".into(),
            ),
            (
                "native_lxmf_receipt_state".into(),
                "direct_link_packet_sent_peer_unconfirmed".into(),
            ),
        ]));

        assert!(!desktop_message_is_retry_candidate(&message));

        message.fields.insert(
            "native_lxmf_direct_transfer_state".into(),
            "resource_timeout".into(),
        );
        assert!(!desktop_message_is_retry_candidate(&message));

        message.fields.insert(
            "native_lxmf_propagation_fallback_available".into(),
            "true".into(),
        );
        assert!(desktop_message_is_retry_candidate(&message));
    }

    #[test]
    fn lxmf_message_compact_status_reports_router_delivery_without_peer_receipt_warning() {
        let mut message = message_with_fields(BTreeMap::from([(
            "native_lxmf_state".into(),
            "delivered".into(),
        )]));
        message.delivered = true;

        assert_eq!(
            lxmf_message_compact_status(&message).as_deref(),
            Some("LXMF router delivered; no retry needed")
        );
    }

    #[test]
    fn desktop_message_retry_labels_call_out_propagation_retry_ready() {
        let mut message = message_with_fields(BTreeMap::from([(
            "native_lxmf_state".into(),
            "propagation_retry_ready".into(),
        )]));

        let labels = desktop_message_retry_labels(&message);
        assert_eq!(labels.prepare, "Retry via propagation");
        assert_eq!(labels.send, "Send via propagation");

        message
            .fields
            .insert("native_lxmf_state".into(), "submitted_unconfirmed".into());
        let labels = desktop_message_retry_labels(&message);
        assert_eq!(labels.prepare, "Retry unconfirmed");
        assert_eq!(labels.send, "Send retry");

        message.fields.insert(
            "native_lxmf_state".into(),
            "propagation_sync_no_payloads".into(),
        );
        assert!(!desktop_message_is_retry_candidate(&message));
        let labels = desktop_message_retry_labels(&message);
        assert_eq!(labels.prepare, "Prepare resend");
        assert_eq!(labels.send, "Resend via propagation");
    }

    #[test]
    fn desktop_message_propagation_sync_label_matches_next_action() {
        let mut message = message_with_fields(BTreeMap::from([(
            "native_lxmf_next_action".into(),
            "sync_propagation".into(),
        )]));
        message.transport_method = TransportMethod::Propagated;

        assert_eq!(
            desktop_message_propagation_sync_label(&message),
            Some("Sync propagation")
        );

        message.fields.insert(
            "native_lxmf_next_action".into(),
            "sync_propagation_again".into(),
        );
        assert_eq!(
            desktop_message_propagation_sync_label(&message),
            Some("Sync again")
        );

        message.failed = true;
        assert_eq!(desktop_message_propagation_sync_label(&message), None);
    }
}
