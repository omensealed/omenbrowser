use crate::messaging::MessageSummary;

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::messaging::{MessageSummary, TransportMethod};

    use super::*;

    fn message_with_fields(fields: BTreeMap<String, String>) -> MessageSummary {
        MessageSummary {
            peer_hash: "peer".into(),
            peer_label: "Peer".into(),
            title: "hello".into(),
            content: "body".into(),
            timestamp: 1.0,
            transport_method: TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("msg-1".into()),
            fields,
            attachments: Vec::new(),
        }
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
