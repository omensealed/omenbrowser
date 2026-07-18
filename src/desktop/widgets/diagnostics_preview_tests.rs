use super::*;

const FIXTURE_RETICULUM_HASH: &str = "00112233445566778899aabbccddeeff";

fn fixture_browser_node_url() -> String {
    format!("{FIXTURE_RETICULUM_HASH}:/page/index.mu")
}

fn pretty_json_lines(value: serde_json::Value) -> Vec<String> {
    serde_json::to_string_pretty(&value)
        .expect("json")
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn diagnostics_summary_extracts_classification_next_step() {
    let lines = pretty_json_lines(serde_json::json!({
        "report": "native_network_smoke_test",
        "classification": {
            "outcome": "blocked",
            "stage": "destination_identity",
            "detail": "destination identity is not known",
            "next_step": "preload known_destinations"
        }
    }));

    let summary = diagnostics_preview_report_summary(&lines).expect("summary");
    assert_eq!(summary.report, "native_network_smoke_test");
    assert_eq!(summary.outcome, "blocked");
    assert_eq!(summary.stage, "destination_identity");
    assert_eq!(summary.next_step, "preload known_destinations");
}

#[test]
fn diagnostics_summary_extracts_page_probe_failure_stage() {
    let lines = pretty_json_lines(serde_json::json!({
        "url": fixture_browser_node_url(),
        "ready_to_request": false,
        "steps": [
            {"stage": "address_parse", "ok": true, "detail": "parsed"},
            {"stage": "path_discovery", "ok": false, "detail": "path unknown"}
        ]
    }));

    let summary = diagnostics_preview_report_summary(&lines).expect("summary");
    assert_eq!(summary.outcome, "blocked");
    assert_eq!(summary.stage, "path_discovery");
    assert_eq!(summary.detail, "path unknown");
}

#[test]
fn diagnostics_live_fetch_card_extracts_success_metadata() {
    let lines = pretty_json_lines(serde_json::json!({
        "report": "native_network_smoke_test",
        "classification": {
            "outcome": "pass",
            "stage": "live_fetch",
            "next_step": "open browser"
        },
        "live_fetch": {
            "ok": true,
            "stage_hint": "response_decode",
            "url": fixture_browser_node_url(),
            "title": "Node Home",
            "markup_bytes": 128,
            "markup_lines": 6,
            "metadata": {
                "native_request_backend": "reticulum_transport",
                "native_request_primitive": "request-resource"
            }
        }
    }));

    let card = diagnostics_preview_live_fetch_card(&lines).expect("live fetch card");
    assert_eq!(card.outcome, "pass");
    assert_eq!(card.stage_hint, "response_decode");
    assert_eq!(card.request_backend, "reticulum_transport/request-resource");
    assert_eq!(card.response_size, "128 bytes, 6 lines");
    assert_eq!(card.first_failed_stage, "live_fetch");
}

#[test]
fn diagnostics_live_fetch_card_extracts_failed_probe_stage() {
    let lines = pretty_json_lines(serde_json::json!({
        "report": "native_network_smoke_test",
        "classification": {
            "outcome": "blocked",
            "stage": "path_discovery",
            "next_step": "warm path"
        },
        "live_page_probe": {
            "ok": true,
            "report": {
                "steps": [
                    {"stage": "address_parse", "ok": true, "detail": "parsed"},
                    {"stage": "path_discovery", "ok": false, "detail": "queued request_path"}
                ]
            }
        },
        "live_fetch": {
            "ok": false,
            "status": "blocked",
            "error": "live fetch preflight did not report ready_to_request",
            "stage_hint": "path_discovery"
        }
    }));

    let card = diagnostics_preview_live_fetch_card(&lines).expect("live fetch card");
    assert_eq!(card.outcome, "blocked");
    assert_eq!(card.request_backend, "not reached");
    assert_eq!(card.response_size, "no response body");
    assert_eq!(card.first_failed_stage, "path_discovery");
    assert_eq!(card.next_step, "warm path");
}

#[test]
fn diagnostics_lxmf_delivery_card_extracts_proof_and_inbound_evidence() {
    let lines = pretty_json_lines(serde_json::json!({
        "report": "native_lxmf_live_interop",
        "classification": {
            "outcome": "pass",
            "reason": "explicit send produced matching LXMF/RNS evidence",
            "next_step": "capture report",
            "proof_match_state": "matched_packet_proof",
            "inbound_reply_match_state": "matched_peer_reply"
        },
        "readiness_probe": {
            "ready_to_send": true,
            "steps": [
                {"stage": "runtime_setup", "ok": true, "detail": "runtime ready"}
            ]
        },
        "send": {
            "requested": true,
            "ok": true,
            "message_id": "packet-1",
            "native_lxmf_state": "submitted_to_rns_net"
        },
        "wait": {
            "status": "observed",
            "proof_match_state": "matched_packet_proof",
            "inbound_reply_match_state": "matched_peer_reply",
            "inbound_messages": 1,
            "delivery_updates": 2,
            "packet_proofs": 1
        }
    }));

    let card = diagnostics_preview_lxmf_delivery_card(&lines).expect("lxmf card");
    assert_eq!(card.outcome, "pass");
    assert!(card.send_state.contains("submitted_to_rns_net"));
    assert_eq!(card.proof_state, "matched_packet_proof");
    assert_eq!(card.inbound_state, "matched_peer_reply");
    assert_eq!(
        card.event_counts,
        "inbound=1, delivery_updates=2, packet_proofs=1"
    );
    assert_eq!(card.readiness_stage, "ready or not requested");
}

#[test]
fn diagnostics_lxmf_delivery_card_extracts_nested_blocker() {
    let lines = pretty_json_lines(serde_json::json!({
        "report": "native_network_smoke_test",
        "lxmf_live_interop": {
            "report": "native_lxmf_live_interop",
            "classification": {
                "outcome": "blocked",
                "reason": "target peer is not ready for direct LXMF send",
                "next_step": "request peer path"
            },
            "readiness_probe": {
                "ready_to_send": false,
                "steps": [
                    {"stage": "path_discovery", "ok": false, "detail": "queued request_path"}
                ]
            },
            "send": {
                "requested": true,
                "ok": false,
                "skipped": "LXMF delivery probe did not report ready_to_send"
            },
            "wait": {
                "status": "timeout",
                "proof_match_state": "no_matching_packet_proof",
                "inbound_reply_match_state": "no_matching_peer_reply",
                "inbound_messages": 0,
                "delivery_updates": 0,
                "packet_proofs": 0
            }
        }
    }));

    let card = diagnostics_preview_lxmf_delivery_card(&lines).expect("lxmf card");
    assert_eq!(card.outcome, "blocked");
    assert!(card.send_state.contains("ready_to_send"));
    assert_eq!(card.readiness_stage, "path_discovery: queued request_path");
    assert_eq!(card.next_step, "request peer path");
}

#[test]
fn diagnostics_propagation_sync_card_extracts_status_and_event_counts() {
    let lines = pretty_json_lines(serde_json::json!({
        "report": "native_lxmf_propagation_diagnostics",
        "selected_node": FIXTURE_RETICULUM_HASH,
        "sync": {
            "ok": true,
            "error": null
        },
        "before": {
            "has_path": true,
            "known_app_data": true,
            "link_state": "path_known",
            "transfer_state": "idle"
        },
        "after": {
            "has_path": true,
            "known_app_data": true,
            "link_state": "link_established",
            "transfer_state": "complete"
        },
        "sync_events": [
            {"kind": "propagation_sync", "stage": "list_response", "status": "complete", "detail": "received list"},
            {"kind": "propagation_status", "transfer_state": "list_request_sent"},
            {"kind": "propagation_status", "transfer_state": "complete"},
            {"kind": "debug", "message": "native LXMF propagation sync complete"}
        ],
        "blocker": "no propagation blocker reported",
        "next_step": "try propagation sync again or inspect runtime logs"
    }));

    let card = diagnostics_preview_propagation_sync_card(&lines).expect("propagation card");

    assert_eq!(card.outcome, "complete");
    assert!(card.before.contains("path=true"));
    assert!(card.after.contains("transfer=complete"));
    assert_eq!(
        card.events,
        "structured=1, status=2, debug=1, messages=0, total=4"
    );
    assert!(card
        .event_lines
        .iter()
        .any(|line| line.contains("native LXMF propagation sync complete")));
    assert_eq!(card.blocker, "no propagation blocker reported");
}

#[test]
fn diagnostics_stage_cards_extract_preflight_and_smoke_stages() {
    let preflight = pretty_json_lines(serde_json::json!({
        "report": "native_network_preflight",
        "stages": [
            {
                "stage": "backend",
                "outcome": "pass",
                "detail": "Auto",
                "next_step": "continue"
            }
        ]
    }));
    let cards = diagnostics_preview_stage_cards(&preflight);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].kind, "preflight");
    assert_eq!(cards[0].stage, "backend");
    assert_eq!(cards[0].status, "pass");

    let smoke = pretty_json_lines(serde_json::json!({
        "report": "native_network_smoke_test",
        "verdicts": {
            "page_fetch": {
                "status": "blocked",
                "detail": "path unknown",
                "next_action": "request path"
            }
        }
    }));
    let cards = diagnostics_preview_stage_cards(&smoke);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].kind, "smoke");
    assert_eq!(cards[0].stage, "page_fetch");
    assert_eq!(cards[0].next_step, "request path");
}
