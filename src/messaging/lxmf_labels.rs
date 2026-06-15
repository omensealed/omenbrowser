pub(crate) fn delivery_evidence(kind: &str) -> &'static str {
    match kind {
        "packet_submitted" => "packet submitted to Reticulum transport",
        "rns_packet_proof" => "RNS packet proof observed",
        "propagation_node_accepted" => "propagation node accepted payload",
        "propagation_node_failed" => "propagation node transfer failed",
        "propagation_sync_no_payloads" => "propagation sync found no new peer payload",
        "lxmf_router_delivered" => "LXMF router delivered callback",
        "lxmf_router_failed" => "LXMF router failed callback",
        "inbound_peer_message" => "inbound LXMF activity from this peer after send",
        "no_receipt_observed" => "no LXMF receipt observed by native wire path",
        _ => "unknown delivery evidence",
    }
}

pub(crate) fn diagnostics_delivery_evidence(kind: &str) -> &'static str {
    match kind {
        "rns_packet_proof" => "RNS packet proof observed; peer delivery unconfirmed",
        _ => delivery_evidence(kind),
    }
}

pub(crate) fn state(state: &str) -> &'static str {
    match state {
        "submitted_to_runtime" => "submitted to runtime",
        "submitted_to_rns_net" => "submitted; waiting for proof or peer activity",
        "submitted_unconfirmed" => "submitted; peer receipt unconfirmed",
        "transport_proof_received" => "RNS transport proof observed; peer unconfirmed",
        "peer_activity_observed" => "peer activity observed after send",
        "queued_for_propagation" => "queued for propagation",
        "propagation_retry_ready" => "direct unconfirmed; propagation retry ready",
        "propagation_node_accepted" => "propagation node accepted payload",
        "propagation_sync_no_payloads" => "propagation sync found no new peer payload",
        "propagation_transfer_completed" => "propagation transfer complete; peer unconfirmed",
        "delivered" => "LXMF router delivered",
        "failed" => "failed",
        "unknown" => "unknown",
        _ => "unrecognized native LXMF state",
    }
}

pub(crate) fn proof_state(state: &str) -> &'static str {
    match state {
        "waiting_for_packet_proof" => "waiting for packet proof",
        "rns_packet_proof_peer_unconfirmed" => {
            "RNS packet proof observed; peer delivery unconfirmed"
        }
        "proof_received" => "RNS packet proof received",
        "proof_not_observed" => "no packet proof observed",
        "peer_delivery_unconfirmed" => "peer delivery unconfirmed",
        "peer_activity_observed" => "peer activity observed",
        "lxmf_router_callback" => "LXMF router callback received",
        "link_packet_sent" => "direct link packet sent",
        "resource_completed" => "resource transfer completed",
        "resource_timeout" => "resource transfer timed out; peer unconfirmed",
        "resource_progress" => "resource transfer in progress",
        "resource_advertised" => "resource transfer advertised",
        "propagation_resource_in_progress" => "propagation resource in progress",
        "propagation_queued" => "propagation queued",
        "failed" => "failed",
        "unknown" => "unknown",
        _ => "unrecognized native proof state",
    }
}

pub(crate) fn receipt_state(state: &str) -> &'static str {
    match state {
        "packet_submitted" => "packet submitted; receipt pending",
        "lxmf_delivery_receipt_unavailable_native_wire" => {
            "native wire has no confirmed LXMF peer receipt"
        }
        "rns_packet_proof_peer_delivery_unconfirmed" => {
            "RNS packet proof observed; peer delivery unconfirmed"
        }
        "direct_link_packet_sent_peer_unconfirmed" => {
            "direct link packet sent; peer delivery unconfirmed"
        }
        "direct_resource_completed_peer_unconfirmed" => {
            "direct resource completed; peer delivery unconfirmed"
        }
        "direct_resource_timeout" => "direct resource timed out; peer delivery unconfirmed",
        "direct_resource_in_progress" => "direct resource transfer in progress",
        "direct_resource_state_unknown" => "direct resource state unknown",
        "propagation_node_accepted" | "propagation_node_accepted_peer_unconfirmed" => {
            "propagation node accepted payload; peer delivery unconfirmed"
        }
        "propagation_resource_in_progress" => "propagation resource transfer in progress",
        "propagation_resource_failed" | "propagation_node_failed" => "propagation transfer failed",
        "propagation_queued" => "propagation queued",
        "propagation_sync_no_peer_payload" => "propagation sync found no new peer payload",
        "peer_activity_after_send" => "peer activity observed after send",
        "lxmf_delivered" => "LXMF router delivered",
        "lxmf_failed" => "LXMF router failed",
        _ => "unrecognized native receipt state",
    }
}

pub(crate) fn fallback(fallback: &str) -> &'static str {
    match fallback {
        "direct_to_propagated" => "direct send failed; queued via propagation",
        _ => "unrecognized fallback",
    }
}

pub(crate) fn propagation_transfer(transfer: &str) -> &'static str {
    match transfer {
        "link_packet_sent" => "link packet sent to propagation node; peer unconfirmed",
        "resource_completed" => "resource transfer complete; peer unconfirmed",
        "resource_advertised" => "resource offered to propagation node; peer unconfirmed",
        "resource_progress" => "resource transfer in progress",
        "router_deferred" => "queued; waiting for propagation node readiness",
        "link_timeout" => "link timed out",
        "resource_advertise_failed" | "resource_failed" => "resource transfer failed",
        _ => "unrecognized propagation transfer state",
    }
}
