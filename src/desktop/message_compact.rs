use crate::messaging::MessageSummary;

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
}
