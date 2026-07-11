use std::collections::BTreeMap;

use crate::app::{
    MonitoringPanelState, NetworkDoctorActiveResourceRow, NetworkDoctorLinkRecentRow,
    NetworkDoctorLxmfRecentRow, NetworkDoctorPathRecentRow, NetworkDoctorResourceRecentRow,
};
use crate::messaging::conversation::Conversation;
use crate::messaging::DeliveryMode;
use crate::runtime::network::InterfaceSampleState;
use crate::runtime::RuntimeStatus;

use super::super::{human_bytes, RecentActivityRow};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct NetworkDoctorHealthSummary {
    pub(super) runtime_line: String,
    pub(super) interface_line: String,
    pub(super) traffic_line: String,
    pub(super) attention_lines: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct NetworkDoctorMessagingSummary {
    pub(super) conversation_line: String,
    pub(super) delivery_line: String,
    pub(super) ticket_line: String,
    pub(super) resource_line: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NetworkDoctorRow {
    label: String,
    state: String,
    detail: String,
}

impl RecentActivityRow for NetworkDoctorPathRecentRow {
    fn epoch_ms(&self) -> u64 {
        self.epoch_ms
    }

    fn columns(&self) -> (&str, &str, &str) {
        (&self.target, &self.state, &self.detail)
    }
}

impl RecentActivityRow for NetworkDoctorLinkRecentRow {
    fn epoch_ms(&self) -> u64 {
        self.epoch_ms
    }

    fn columns(&self) -> (&str, &str, &str) {
        (&self.link_id, &self.state, &self.detail)
    }
}

impl RecentActivityRow for NetworkDoctorResourceRecentRow {
    fn epoch_ms(&self) -> u64 {
        self.epoch_ms
    }

    fn columns(&self) -> (&str, &str, &str) {
        (&self.transfer, &self.state, &self.detail)
    }
}

impl RecentActivityRow for NetworkDoctorActiveResourceRow {
    fn epoch_ms(&self) -> u64 {
        self.epoch_ms
    }

    fn columns(&self) -> (&str, &str, &str) {
        (&self.transfer, &self.state, &self.detail)
    }
}

impl RecentActivityRow for NetworkDoctorLxmfRecentRow {
    fn epoch_ms(&self) -> u64 {
        self.epoch_ms
    }

    fn columns(&self) -> (&str, &str, &str) {
        (&self.peer, &self.state, &self.detail)
    }
}

pub(super) fn network_doctor_active_resource_rows(
    rows: &BTreeMap<String, NetworkDoctorActiveResourceRow>,
) -> Vec<NetworkDoctorActiveResourceRow> {
    let mut rows = rows.values().cloned().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .epoch_ms
            .cmp(&left.epoch_ms)
            .then_with(|| left.transfer.cmp(&right.transfer))
    });
    rows.truncate(12);
    rows
}

impl NetworkDoctorRow {
    fn new(label: impl Into<String>, state: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: state.into(),
            detail: detail.into(),
        }
    }

    pub(super) fn display_line(&self) -> String {
        format!("{} | {} | {}", self.label, self.state, self.detail)
    }
}

pub(super) fn network_doctor_health_summary(
    runtime: &RuntimeStatus,
    monitoring: &MonitoringPanelState,
    pending_messages: usize,
) -> NetworkDoctorHealthSummary {
    let runtime_line = format!(
        "runtime: {:?} | {} | {}",
        runtime.backend,
        if runtime.connected {
            "connected"
        } else {
            "not connected"
        },
        runtime.message
    );
    let interface_line = monitoring
        .last_interface_stats
        .as_ref()
        .map(|stats| {
            if !stats.available {
                return format!(
                    "interfaces: unavailable{}",
                    stats
                        .reason
                        .as_ref()
                        .map(|reason| format!(" ({reason})"))
                        .unwrap_or_default()
                );
            }
            let attached = stats
                .samples
                .iter()
                .filter(|sample| sample.attached || sample.state == InterfaceSampleState::Attached)
                .count();
            let enabled = stats.samples.iter().filter(|sample| sample.enabled).count();
            let configured = stats
                .samples
                .iter()
                .filter(|sample| sample.state == InterfaceSampleState::Configured)
                .count();
            let disabled = stats
                .samples
                .iter()
                .filter(|sample| sample.state == InterfaceSampleState::Disabled || !sample.enabled)
                .count();
            format!(
                "interfaces: {attached} attached / {enabled} enabled / {disabled} disabled / {configured} configured"
            )
        })
        .unwrap_or_else(|| "interfaces: no sample yet".into());
    let traffic_ops = monitoring
        .outbound_page_requests
        .saturating_add(monitoring.outbound_partial_refreshes)
        .saturating_add(monitoring.outbound_file_downloads)
        .saturating_add(monitoring.outbound_path_requests)
        .saturating_add(monitoring.outbound_path_warmups)
        .saturating_add(monitoring.outbound_lxmf_sends)
        .saturating_add(monitoring.outbound_propagation_syncs)
        .saturating_add(monitoring.outbound_diagnostics)
        .saturating_add(monitoring.outbound_status_updates)
        .saturating_add(monitoring.inbound_page_responses)
        .saturating_add(monitoring.inbound_downloads)
        .saturating_add(monitoring.announces_received)
        .saturating_add(monitoring.path_updates_received)
        .saturating_add(monitoring.inbound_messages)
        .saturating_add(monitoring.lxmf_evidence_updates)
        .saturating_add(monitoring.propagation_sync_events);
    let traffic_line = format!(
        "traffic: {traffic_ops} ops | {} rx / {} tx",
        human_bytes(monitoring.estimated_inbound_bytes),
        human_bytes(monitoring.estimated_outbound_bytes)
    );

    let mut attention_lines = Vec::new();
    if !runtime.connected {
        attention_lines.push("attention: runtime is not connected".into());
    }
    if monitoring.runtime_errors > 0 {
        attention_lines.push(format!(
            "attention: {} runtime error(s) recorded",
            monitoring.runtime_errors
        ));
    }
    if pending_messages > 0 {
        attention_lines.push(format!(
            "attention: {pending_messages} LXMF conversation(s) have pending sends"
        ));
    }
    if let Some(stats) = &monitoring.last_interface_stats {
        let attached = stats
            .samples
            .iter()
            .any(|sample| sample.attached || sample.state == InterfaceSampleState::Attached);
        if stats.available && runtime.connected && !attached {
            attention_lines.push(
                "attention: runtime is connected but no attached interface is reported".into(),
            );
        }
    }
    if attention_lines.is_empty() {
        attention_lines.push("attention: no obvious blocker in passive snapshot".into());
    }

    NetworkDoctorHealthSummary {
        runtime_line,
        interface_line,
        traffic_line,
        attention_lines,
    }
}

pub(super) fn network_doctor_path_rows(monitoring: &MonitoringPanelState) -> Vec<NetworkDoctorRow> {
    let Some(snapshot) = monitoring.last_network_snapshot.as_ref() else {
        return vec![NetworkDoctorRow::new(
            "network snapshot",
            "unavailable",
            "run Diagnostics or wait for runtime status to populate path/discovery details",
        )];
    };

    let mut rows = vec![
        NetworkDoctorRow::new(
            "announces",
            if snapshot.pending_announces > 0 {
                "pending"
            } else {
                "idle"
            },
            format!(
                "{} pending / {} known destination(s) / {} ratchet announce(s)",
                snapshot.pending_announces, snapshot.known_destinations, snapshot.ratchet_announces
            ),
        ),
        NetworkDoctorRow::new(
            "path table",
            if snapshot.path_table_count > 0 {
                "available"
            } else {
                "empty"
            },
            format!(
                "{} cached path(s) / {} path update event(s)",
                snapshot.path_table_count, monitoring.path_updates_received
            ),
        ),
        NetworkDoctorRow::new(
            "path requests",
            if snapshot.request_failures > 0 {
                "attention"
            } else {
                "ok"
            },
            format!(
                "{} request(s) / {} warmup(s) / {} failure(s)",
                monitoring.outbound_path_requests,
                monitoring.outbound_path_warmups,
                snapshot.request_failures
            ),
        ),
        NetworkDoctorRow::new(
            "runtime instance",
            if snapshot.connected_to_shared_instance {
                "shared"
            } else if snapshot.is_shared_instance {
                "serving"
            } else {
                "managed"
            },
            format!(
                "connected_to_shared={} / is_shared={}",
                snapshot.connected_to_shared_instance, snapshot.is_shared_instance
            ),
        ),
    ];

    rows.push(NetworkDoctorRow::new(
        "propagation node",
        if snapshot.active_propagation_node.is_some() {
            "selected"
        } else {
            "unset"
        },
        snapshot
            .active_propagation_node
            .as_deref()
            .unwrap_or("no active propagation node")
            .to_string(),
    ));

    if !snapshot.announce_counts.is_empty() {
        let mut counts = snapshot
            .announce_counts
            .iter()
            .map(|(aspect, count)| format!("{aspect}={count}"))
            .collect::<Vec<_>>();
        counts.sort();
        rows.push(NetworkDoctorRow::new(
            "announce aspects",
            "observed",
            counts.join(", "),
        ));
    }

    rows
}

pub(super) fn network_doctor_transfer_rows(
    conversations: &[Conversation],
    monitoring: &MonitoringPanelState,
) -> Vec<NetworkDoctorRow> {
    let staged_attachments: usize = conversations
        .iter()
        .map(|conversation| conversation.attachments.len())
        .sum();
    let pending_sends = conversations
        .iter()
        .filter(|conversation| conversation.pending_send.is_some())
        .count();
    let browser_downloads = monitoring
        .outbound_file_downloads
        .saturating_add(monitoring.inbound_downloads);
    let lxmf_activity = monitoring
        .outbound_lxmf_sends
        .saturating_add(monitoring.inbound_messages)
        .saturating_add(monitoring.lxmf_evidence_updates)
        .saturating_add(monitoring.propagation_sync_events);

    vec![
        NetworkDoctorRow::new(
            "browser downloads",
            if browser_downloads > 0 {
                "observed"
            } else {
                "idle"
            },
            format!(
                "{} requested / {} received",
                monitoring.outbound_file_downloads, monitoring.inbound_downloads
            ),
        ),
        NetworkDoctorRow::new(
            "lxmf attachments",
            if staged_attachments > 0 {
                "staged"
            } else {
                "idle"
            },
            format!("{staged_attachments} staged attachment(s) across conversations"),
        ),
        NetworkDoctorRow::new(
            "lxmf delivery",
            if pending_sends > 0 {
                "pending"
            } else if lxmf_activity > 0 {
                "observed"
            } else {
                "idle"
            },
            format!(
                "{} pending / {} sends / {} inbound / {} evidence update(s)",
                pending_sends,
                monitoring.outbound_lxmf_sends,
                monitoring.inbound_messages,
                monitoring.lxmf_evidence_updates
            ),
        ),
        NetworkDoctorRow::new(
            "reticulum resources",
            "typed events",
            "active transfer state is populated by runtime resource progress/completion events",
        ),
    ]
}

pub(super) fn network_doctor_messaging_summary(
    conversations: &[Conversation],
    monitoring: &MonitoringPanelState,
) -> NetworkDoctorMessagingSummary {
    let direct = conversations
        .iter()
        .filter(|conversation| matches!(conversation.delivery_mode, DeliveryMode::Direct))
        .count();
    let propagated = conversations
        .iter()
        .filter(|conversation| matches!(conversation.delivery_mode, DeliveryMode::Propagated))
        .count();
    let pending = conversations
        .iter()
        .filter(|conversation| conversation.pending_send.is_some())
        .count();
    let unread: u32 = conversations
        .iter()
        .map(|conversation| conversation.thread.unread_count)
        .sum();
    let staged_attachments: usize = conversations
        .iter()
        .map(|conversation| conversation.attachments.len())
        .sum();
    let conversations_with_reply_ticket = conversations
        .iter()
        .filter(|conversation| conversation.thread.lxmf_reply_ticket.is_some())
        .count();
    let conversations_requesting_ticket = conversations
        .iter()
        .filter(|conversation| conversation.include_ticket)
        .count();

    NetworkDoctorMessagingSummary {
        conversation_line: format!(
            "conversations: {} total / {direct} direct / {propagated} propagated / {unread} unread",
            conversations.len()
        ),
        delivery_line: format!(
            "delivery: {pending} pending / {} sends / {} propagation syncs / {} inbound",
            monitoring.outbound_lxmf_sends,
            monitoring.outbound_propagation_syncs,
            monitoring.inbound_messages
        ),
        ticket_line: format!(
            "tickets: {conversations_with_reply_ticket} remembered / {conversations_requesting_ticket} requested / {} evidence updates",
            monitoring.lxmf_evidence_updates
        ),
        resource_line: format!(
            "resources: {staged_attachments} staged LXMF attachment(s) / {} browser downloads / {} received downloads",
            monitoring.outbound_file_downloads,
            monitoring.inbound_downloads
        ),
    }
}

#[cfg(test)]
#[path = "network_doctor_tests.rs"]
mod tests;
