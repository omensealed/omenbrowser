use super::*;
use crate::messaging::conversation::MessageSendState;
use crate::messaging::NativeLxmfReplyTicket;
use crate::runtime::network::InterfaceSample;
use crate::runtime::{InterfaceStats, NetworkSnapshot, RuntimeBackendName};
use std::collections::BTreeMap;

fn runtime_status(connected: bool) -> RuntimeStatus {
    RuntimeStatus {
        connected,
        backend: RuntimeBackendName::Reticulum,
        active_identity: None,
        message: "ready".into(),
    }
}

fn sample(name: &str, state: InterfaceSampleState, attached: bool) -> InterfaceSample {
    InterfaceSample {
        profile_id: name.into(),
        name: name.into(),
        kind: "tcp_client".into(),
        state,
        enabled: true,
        supported: true,
        attached,
        endpoint: Some("127.0.0.1:4242".into()),
        detail: None,
    }
}

#[test]
fn network_doctor_health_summary_reports_clean_snapshot() {
    let monitoring = MonitoringPanelState {
        last_interface_stats: Some(InterfaceStats {
            available: true,
            reason: None,
            interfaces: Vec::new(),
            samples: vec![sample("gateway", InterfaceSampleState::Attached, true)],
        }),
        estimated_inbound_bytes: 1024,
        estimated_outbound_bytes: 2048,
        inbound_messages: 1,
        outbound_lxmf_sends: 1,
        ..MonitoringPanelState::default()
    };

    let summary = network_doctor_health_summary(&runtime_status(true), &monitoring, 0);

    assert!(summary.runtime_line.contains("connected"));
    assert!(summary.interface_line.contains("1 attached / 1 enabled"));
    assert!(summary.traffic_line.contains("2 ops"));
    assert_eq!(
        summary.attention_lines,
        vec!["attention: no obvious blocker in passive snapshot"]
    );
}

#[test]
fn network_doctor_health_summary_flags_blockers() {
    let monitoring = MonitoringPanelState {
        runtime_errors: 2,
        last_interface_stats: Some(InterfaceStats {
            available: true,
            reason: None,
            interfaces: Vec::new(),
            samples: vec![sample("gateway", InterfaceSampleState::Configured, false)],
        }),
        ..MonitoringPanelState::default()
    };

    let summary = network_doctor_health_summary(&runtime_status(false), &monitoring, 3);

    assert!(summary.interface_line.contains("0 attached"));
    assert!(summary
        .attention_lines
        .iter()
        .any(|line| line.contains("runtime is not connected")));
    assert!(summary
        .attention_lines
        .iter()
        .any(|line| line.contains("2 runtime error")));
    assert!(summary
        .attention_lines
        .iter()
        .any(|line| line.contains("3 LXMF conversation")));
}

#[test]
fn network_doctor_messaging_summary_counts_conversation_state() {
    let mut direct = Conversation::new(1, "peer-a", "Peer A");
    direct.thread.unread_count = 2;
    direct.include_ticket = true;
    direct.attachments.push("photo.jpg".into());

    let mut propagated = Conversation::new(2, "peer-b", "Peer B");
    propagated.delivery_mode = DeliveryMode::Propagated;
    propagated.pending_send = Some(MessageSendState { generation: 7 });
    propagated.thread.lxmf_reply_ticket = Some(NativeLxmfReplyTicket {
        ticket: vec![1, 2, 3],
        expires: 42.0,
    });

    let monitoring = MonitoringPanelState {
        outbound_lxmf_sends: 3,
        outbound_propagation_syncs: 4,
        inbound_messages: 5,
        lxmf_evidence_updates: 6,
        outbound_file_downloads: 7,
        inbound_downloads: 8,
        ..MonitoringPanelState::default()
    };

    let summary = network_doctor_messaging_summary(&[direct, propagated], &monitoring);

    assert!(summary
        .conversation_line
        .contains("2 total / 1 direct / 1 propagated / 2 unread"));
    assert!(summary
        .delivery_line
        .contains("1 pending / 3 sends / 4 propagation syncs / 5 inbound"));
    assert!(summary
        .ticket_line
        .contains("1 remembered / 1 requested / 6 evidence updates"));
    assert!(summary
        .resource_line
        .contains("1 staged LXMF attachment(s) / 7 browser downloads / 8 received downloads"));
}

#[test]
fn network_doctor_path_rows_use_network_snapshot() {
    let monitoring = MonitoringPanelState {
        outbound_path_requests: 2,
        outbound_path_warmups: 3,
        path_updates_received: 4,
        last_network_snapshot: Some(NetworkSnapshot {
            announce_counts: BTreeMap::from([
                ("lxmf.delivery".into(), 2),
                ("nomadnetwork.node".into(), 1),
            ]),
            pending_announces: 1,
            known_destinations: 5,
            ratchet_announces: 6,
            path_table_count: 7,
            request_failures: 8,
            active_propagation_node: Some("abcd".into()),
            connected_to_shared_instance: true,
            is_shared_instance: false,
        }),
        ..MonitoringPanelState::default()
    };

    let rows = network_doctor_path_rows(&monitoring);

    assert!(rows
        .iter()
        .any(|row| row.display_line().contains("announces | pending")));
    assert!(rows
        .iter()
        .any(|row| row.display_line().contains("path table | available")));
    assert!(rows
        .iter()
        .any(|row| row.display_line().contains("8 failure")));
    assert!(rows.iter().any(|row| row
        .display_line()
        .contains("propagation node | selected | abcd")));
    assert!(rows
        .iter()
        .any(|row| row.display_line().contains("lxmf.delivery=2")));
}

#[test]
fn network_doctor_transfer_rows_report_idle_and_pending_states() {
    let idle = network_doctor_transfer_rows(&[], &MonitoringPanelState::default());
    assert!(idle
        .iter()
        .any(|row| row.display_line() == "browser downloads | idle | 0 requested / 0 received"));
    assert!(idle.iter().any(|row| row
        .display_line()
        .contains("reticulum resources | typed events")));

    let mut conversation = Conversation::new(1, "peer", "Peer");
    conversation.attachments.push("one.bin".into());
    conversation.pending_send = Some(MessageSendState { generation: 9 });
    let monitoring = MonitoringPanelState {
        outbound_file_downloads: 1,
        inbound_downloads: 2,
        outbound_lxmf_sends: 3,
        inbound_messages: 4,
        lxmf_evidence_updates: 5,
        ..MonitoringPanelState::default()
    };
    let active = network_doctor_transfer_rows(&[conversation], &monitoring);

    assert!(
        active
            .iter()
            .any(|row| row.display_line()
                == "browser downloads | observed | 1 requested / 2 received")
    );
    assert!(active.iter().any(|row| row
        .display_line()
        .contains("lxmf attachments | staged | 1 staged")));
    assert!(active.iter().any(|row| row
        .display_line()
        .contains("lxmf delivery | pending | 1 pending / 3 sends / 4 inbound / 5 evidence")));
}

#[test]
fn network_doctor_active_resource_rows_sort_newest_first() {
    let rows = BTreeMap::from([
        (
            "old".into(),
            NetworkDoctorActiveResourceRow {
                epoch_ms: 100,
                transfer: "old".into(),
                state: "progress".into(),
                source: "omenchat".into(),
                purpose: Some("omenchat-resource".into()),
                direction: Some("inbound".into()),
                peer: Some("link-old".into()),
                detail: "omenchat | 1/4 byte(s)".into(),
                received: Some(1),
                total: Some(4),
            },
        ),
        (
            "new".into(),
            NetworkDoctorActiveResourceRow {
                epoch_ms: 200,
                transfer: "new".into(),
                state: "complete".into(),
                source: "nomadnet-page".into(),
                purpose: Some("nomadnet-page".into()),
                direction: Some("inbound".into()),
                peer: None,
                detail: "4 byte(s)".into(),
                received: Some(4),
                total: Some(4),
            },
        ),
    ]);

    let rows = network_doctor_active_resource_rows(&rows);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].transfer, "new");
    assert_eq!(rows[1].transfer, "old");
}

#[test]
fn recent_activity_rows_render_from_typed_state_rows() {
    let path = NetworkDoctorPathRecentRow {
        epoch_ms: 100,
        target: "target".into(),
        state: "state".into(),
        detail: "detail".into(),
    };
    let link = NetworkDoctorLinkRecentRow {
        epoch_ms: 100,
        link_id: "link".into(),
        state: "open".into(),
        detail: "ready".into(),
    };
    let resource = NetworkDoctorResourceRecentRow {
        epoch_ms: 100,
        transfer: "res".into(),
        state: "complete".into(),
        detail: "4 byte(s)".into(),
    };
    let lxmf = NetworkDoctorLxmfRecentRow {
        epoch_ms: 100,
        peer: "peer".into(),
        state: "event".into(),
        detail: "delivered".into(),
    };

    assert_eq!(path.display_line(), "target | state | detail");
    assert_eq!(link.display_line(), "link | open | ready");
    assert_eq!(resource.display_line(), "res | complete | 4 byte(s)");
    assert_eq!(lxmf.display_line(), "peer | event | delivered");
}
