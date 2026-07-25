use std::collections::BTreeMap;

use super::{compact_hash, current_epoch_ms, hex_lower};
use crate::operations::presentation::OperationPresentationFilter;
use crate::operations::OperationId;
use crate::runtime::{
    AnnouncePayload, LxmfDeliveryEvidence, OmenChatLinkClosed, OmenChatLinkData,
    OmenChatResourceData, OutboundStatus, PathEvent, RuntimeFacadeEvent,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkDoctorPanelState {
    pub operations_filter: OperationPresentationFilter,
    pub operations_search: String,
    pub selected_operation: Option<OperationId>,
    pub operation_select_mode: bool,
    pub operation_diagnostic_scroll: u16,
    pub recent_paths: Vec<NetworkDoctorPathRecentRow>,
    pub recent_links: Vec<NetworkDoctorLinkRecentRow>,
    pub recent_resources: Vec<NetworkDoctorResourceRecentRow>,
    pub recent_lxmf: Vec<NetworkDoctorLxmfRecentRow>,
    pub active_resources: BTreeMap<String, NetworkDoctorActiveResourceRow>,
}

pub const NETWORK_DOCTOR_OPERATION_ROWS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkDoctorPathRecentRow {
    pub epoch_ms: u64,
    pub target: String,
    pub state: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkDoctorLinkRecentRow {
    pub epoch_ms: u64,
    pub link_id: String,
    pub state: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkDoctorResourceRecentRow {
    pub epoch_ms: u64,
    pub transfer: String,
    pub state: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkDoctorActiveResourceRow {
    pub epoch_ms: u64,
    pub transfer: String,
    pub state: String,
    pub source: String,
    pub purpose: Option<String>,
    pub direction: Option<String>,
    pub peer: Option<String>,
    pub detail: String,
    pub received: Option<u64>,
    pub total: Option<u64>,
}

struct NetworkDoctorResourceUpdate {
    transfer: String,
    state: String,
    source: String,
    purpose: Option<String>,
    direction: Option<String>,
    peer: Option<String>,
    detail: String,
    received: Option<u64>,
    total: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkDoctorLxmfRecentRow {
    pub epoch_ms: u64,
    pub peer: String,
    pub state: String,
    pub detail: String,
}

impl NetworkDoctorPanelState {
    pub(super) const MAX_RECENT_ROWS: usize = 12;

    fn push_path(
        rows: &mut Vec<NetworkDoctorPathRecentRow>,
        target: impl Into<String>,
        state: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let target = target.into();
        let state = state.into();
        let detail = detail.into();
        if rows
            .iter()
            .any(|row| row.target == target && row.state == state && row.detail == detail)
        {
            return;
        }
        rows.insert(
            0,
            NetworkDoctorPathRecentRow {
                epoch_ms: current_epoch_ms(),
                target,
                state,
                detail,
            },
        );
        rows.truncate(Self::MAX_RECENT_ROWS);
    }

    fn push_link(
        rows: &mut Vec<NetworkDoctorLinkRecentRow>,
        link_id: impl Into<String>,
        state: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let link_id = link_id.into();
        let state = state.into();
        let detail = detail.into();
        if rows
            .iter()
            .any(|row| row.link_id == link_id && row.state == state && row.detail == detail)
        {
            return;
        }
        rows.insert(
            0,
            NetworkDoctorLinkRecentRow {
                epoch_ms: current_epoch_ms(),
                link_id,
                state,
                detail,
            },
        );
        rows.truncate(Self::MAX_RECENT_ROWS);
    }

    fn push_resource(
        rows: &mut Vec<NetworkDoctorResourceRecentRow>,
        transfer: impl Into<String>,
        state: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let transfer = transfer.into();
        let state = state.into();
        let detail = detail.into();
        if rows
            .iter()
            .any(|row| row.transfer == transfer && row.state == state && row.detail == detail)
        {
            return;
        }
        rows.insert(
            0,
            NetworkDoctorResourceRecentRow {
                epoch_ms: current_epoch_ms(),
                transfer,
                state,
                detail,
            },
        );
        rows.truncate(Self::MAX_RECENT_ROWS);
    }

    fn upsert_resource(&mut self, update: NetworkDoctorResourceUpdate) {
        let NetworkDoctorResourceUpdate {
            transfer,
            state,
            source,
            purpose,
            direction,
            peer,
            detail,
            received,
            total,
        } = update;
        let previous = self.active_resources.get(&transfer);
        let source = if source == "unknown" {
            previous
                .map(|row| row.source.clone())
                .unwrap_or_else(|| "unknown".into())
        } else {
            source
        };
        let purpose = purpose.or_else(|| previous.and_then(|row| row.purpose.clone()));
        let direction = direction.or_else(|| previous.and_then(|row| row.direction.clone()));
        let peer = peer.or_else(|| previous.and_then(|row| row.peer.clone()));
        let detail = Self::resource_detail_with_context(
            detail,
            &source,
            purpose.as_deref(),
            direction.as_deref(),
            peer.as_deref(),
        );
        self.active_resources.insert(
            transfer.clone(),
            NetworkDoctorActiveResourceRow {
                epoch_ms: current_epoch_ms(),
                transfer,
                state,
                source,
                purpose,
                direction,
                peer,
                detail,
                received,
                total,
            },
        );
    }

    fn resource_detail_with_context(
        detail: String,
        source: &str,
        purpose: Option<&str>,
        direction: Option<&str>,
        peer: Option<&str>,
    ) -> String {
        let mut context = Vec::new();
        if source != "unknown" {
            context.push(format!("source={source}"));
        }
        if let Some(purpose) = purpose.filter(|value| !value.is_empty()) {
            context.push(format!("purpose={purpose}"));
        }
        if let Some(direction) = direction.filter(|value| !value.is_empty()) {
            context.push(format!("direction={direction}"));
        }
        if let Some(peer) = peer.filter(|value| !value.is_empty()) {
            context.push(format!("peer={peer}"));
        }
        if context.is_empty() {
            detail
        } else {
            format!("{} | {}", context.join(" "), detail)
        }
    }

    fn push_lxmf(
        rows: &mut Vec<NetworkDoctorLxmfRecentRow>,
        peer: impl Into<String>,
        state: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let peer = peer.into();
        let state = state.into();
        let detail = detail.into();
        if rows
            .iter()
            .any(|row| row.peer == peer && row.state == state && row.detail == detail)
        {
            return;
        }
        rows.insert(
            0,
            NetworkDoctorLxmfRecentRow {
                epoch_ms: current_epoch_ms(),
                peer,
                state,
                detail,
            },
        );
        rows.truncate(Self::MAX_RECENT_ROWS);
    }

    pub(super) fn record_announce(&mut self, announce: &AnnouncePayload) {
        self.record_facade_event(&RuntimeFacadeEvent::AnnounceObserved {
            destination_hash: announce.destination_hash.clone(),
            kind: format!("{:?}", announce.kind),
            display_name: announce.display_name.clone(),
            has_ratchet: announce.has_ratchet,
        });
    }

    pub(super) fn record_path_update(&mut self, path: &PathEvent) {
        self.record_facade_event(&RuntimeFacadeEvent::PathUpdated {
            destination_hash: path.destination_hash.clone(),
            known: path.known,
            hops: path.hops,
        });
    }

    pub(super) fn record_delivery_status(&mut self, status: &OutboundStatus) {
        let state = if status.failed {
            "failed"
        } else if status.delivered {
            "delivered"
        } else {
            "updated"
        };
        Self::push_lxmf(
            &mut self.recent_lxmf,
            compact_hash(&status.peer_hash),
            state,
            format!(
                "message={} state={:?} evidence={}",
                status.message_id.as_deref().unwrap_or("none"),
                status.state,
                status.evidence.as_deref().unwrap_or("none")
            ),
        );
    }

    pub(super) fn record_lxmf_evidence(&mut self, evidence: &LxmfDeliveryEvidence) {
        Self::push_lxmf(
            &mut self.recent_lxmf,
            compact_hash(&evidence.peer_hash),
            format!("{:?}", evidence.kind).to_ascii_lowercase(),
            format!(
                "message={} detail={}",
                evidence.message_id.as_deref().unwrap_or("none"),
                evidence.detail.as_deref().unwrap_or("none")
            ),
        );
    }

    pub(super) fn record_omenchat_link_closed(&mut self, data: &OmenChatLinkClosed) {
        Self::push_link(
            &mut self.recent_links,
            hex_lower(&data.link_id),
            "closed",
            data.reason.as_deref().unwrap_or("unknown").to_string(),
        );
    }

    pub(super) fn record_omenchat_link_data(&mut self, data: &OmenChatLinkData) {
        Self::push_link(
            &mut self.recent_links,
            hex_lower(&data.link_id),
            "frame",
            format!("{} byte(s)", data.frame_bytes.len()),
        );
    }

    pub(super) fn record_omenchat_resource_data(&mut self, data: &OmenChatResourceData) {
        let transfer_id = hex_lower(&data.link_id);
        let detail = format!(
            "{} byte(s), metadata {} byte(s)",
            data.data.len(),
            data.metadata.as_ref().map_or(0, Vec::len)
        );
        self.upsert_resource(NetworkDoctorResourceUpdate {
            transfer: transfer_id.clone(),
            state: "complete".into(),
            source: "omenchat".into(),
            purpose: Some("omenchat-resource".into()),
            direction: Some("inbound".into()),
            peer: None,
            detail: detail.clone(),
            received: Some(data.data.len() as u64),
            total: Some(data.data.len() as u64),
        });
        Self::push_resource(&mut self.recent_resources, transfer_id, "complete", detail);
    }

    pub(super) fn record_facade_event(&mut self, event: &RuntimeFacadeEvent) {
        match event {
            RuntimeFacadeEvent::InterfaceUp {
                interface_id,
                detail,
            } => Self::push_path(
                &mut self.recent_paths,
                interface_id,
                "interface up",
                detail.as_deref().unwrap_or("no detail"),
            ),
            RuntimeFacadeEvent::InterfaceDown {
                interface_id,
                reason,
            } => Self::push_path(
                &mut self.recent_paths,
                interface_id,
                "interface down",
                reason.as_deref().unwrap_or("unknown"),
            ),
            RuntimeFacadeEvent::AnnounceHeard {
                destination_hash,
                app,
                aspect,
            } => Self::push_path(
                &mut self.recent_paths,
                compact_hash(destination_hash),
                "announce",
                format!("{app}.{aspect}"),
            ),
            RuntimeFacadeEvent::AnnounceObserved {
                destination_hash,
                kind,
                display_name,
                has_ratchet,
            } => Self::push_path(
                &mut self.recent_paths,
                format!("announce {kind}"),
                if *has_ratchet { "ratchet" } else { "heard" },
                format!("{} {}", compact_hash(destination_hash), display_name),
            ),
            RuntimeFacadeEvent::PathRequested { destination_hash } => Self::push_path(
                &mut self.recent_paths,
                compact_hash(destination_hash),
                "requested",
                "path request queued",
            ),
            RuntimeFacadeEvent::PathUpdated {
                destination_hash,
                known,
                hops,
            } => Self::push_path(
                &mut self.recent_paths,
                compact_hash(destination_hash),
                if *known { "known" } else { "unknown" },
                hops.map(|hops| format!("{hops} hop(s)"))
                    .unwrap_or_else(|| "hop count unavailable".into()),
            ),
            RuntimeFacadeEvent::PathFound {
                destination_hash,
                hops,
            } => Self::push_path(
                &mut self.recent_paths,
                compact_hash(destination_hash),
                "found",
                hops.map(|hops| format!("{hops} hop(s)"))
                    .unwrap_or_else(|| "hop count unavailable".into()),
            ),
            RuntimeFacadeEvent::LinkOpening { destination_hash } => Self::push_link(
                &mut self.recent_links,
                compact_hash(destination_hash),
                "opening",
                "link open requested",
            ),
            RuntimeFacadeEvent::LinkOpened {
                destination_hash,
                link_id,
            } => Self::push_link(
                &mut self.recent_links,
                link_id,
                "opened",
                format!("destination={}", compact_hash(destination_hash)),
            ),
            RuntimeFacadeEvent::LinkClosed { link_id, reason } => Self::push_link(
                &mut self.recent_links,
                link_id,
                "closed",
                reason.as_deref().unwrap_or("unknown"),
            ),
            RuntimeFacadeEvent::LinkFrameReceived { link_id, bytes } => {
                Self::push_link(
                    &mut self.recent_links,
                    link_id,
                    "frame",
                    format!("{bytes} byte(s)"),
                );
            }
            RuntimeFacadeEvent::ResourceOffered {
                link_id,
                transfer_id,
                purpose,
                bytes,
                source,
                direction,
                peer,
            } => {
                let detail = format!(
                    "link={} purpose={} size={}",
                    link_id,
                    purpose,
                    bytes
                        .map(|bytes| format!("{bytes} byte(s)"))
                        .unwrap_or_else(|| "unknown".into())
                );
                self.upsert_resource(NetworkDoctorResourceUpdate {
                    transfer: transfer_id.clone(),
                    state: "offered".into(),
                    source: source.clone().unwrap_or_else(|| "unknown".into()),
                    purpose: Some(purpose.clone()),
                    direction: direction.clone().or_else(|| Some("inbound".into())),
                    peer: peer.clone().or_else(|| Some(link_id.clone())),
                    detail: detail.clone(),
                    received: None,
                    total: *bytes,
                });
                Self::push_resource(&mut self.recent_resources, transfer_id, "offered", detail);
            }
            RuntimeFacadeEvent::ResourceProgress {
                transfer_id,
                received,
                total,
                source,
                purpose,
                direction,
                peer,
            } => {
                let detail = match (source.as_deref(), total) {
                    (Some(source), Some(total)) => {
                        format!("{source} | {received}/{total} byte(s)")
                    }
                    (Some(source), None) => {
                        format!("{source} | {received} byte(s) received")
                    }
                    (None, Some(total)) => format!("{received}/{total} byte(s)"),
                    (None, None) => format!("{received} byte(s) received"),
                };
                self.upsert_resource(NetworkDoctorResourceUpdate {
                    transfer: transfer_id.clone(),
                    state: "progress".into(),
                    source: source.clone().unwrap_or_else(|| "unknown".into()),
                    purpose: purpose.clone(),
                    direction: direction.clone(),
                    peer: peer.clone(),
                    detail: detail.clone(),
                    received: Some(*received),
                    total: *total,
                });
                Self::push_resource(&mut self.recent_resources, transfer_id, "progress", detail);
            }
            RuntimeFacadeEvent::ResourceComplete {
                transfer_id,
                bytes,
                source,
                purpose,
                direction,
                peer,
            } => {
                self.upsert_resource(NetworkDoctorResourceUpdate {
                    transfer: transfer_id.clone(),
                    state: "complete".into(),
                    source: source.clone().unwrap_or_else(|| "unknown".into()),
                    purpose: purpose.clone(),
                    direction: direction.clone(),
                    peer: peer.clone(),
                    detail: format!("{bytes} byte(s)"),
                    received: Some(*bytes),
                    total: Some(*bytes),
                });
                Self::push_resource(
                    &mut self.recent_resources,
                    transfer_id,
                    "complete",
                    format!("{bytes} byte(s)"),
                );
            }
            RuntimeFacadeEvent::ResourceFailed {
                transfer_id,
                reason,
                source,
                purpose,
                direction,
                peer,
            } => {
                self.upsert_resource(NetworkDoctorResourceUpdate {
                    transfer: transfer_id.clone(),
                    state: "failed".into(),
                    source: source.clone().unwrap_or_else(|| "unknown".into()),
                    purpose: purpose.clone(),
                    direction: direction.clone(),
                    peer: peer.clone(),
                    detail: reason.clone(),
                    received: None,
                    total: None,
                });
                Self::push_resource(&mut self.recent_resources, transfer_id, "failed", reason);
            }
            RuntimeFacadeEvent::LxmfEvent { event, detail } => Self::push_lxmf(
                &mut self.recent_lxmf,
                event,
                "event",
                detail.as_deref().unwrap_or("no detail"),
            ),
            RuntimeFacadeEvent::Diagnostic { section, message } => {
                Self::push_path(&mut self.recent_paths, section, "diagnostic", message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histories_remain_bounded_and_duplicate_suppressed() {
        let mut state = NetworkDoctorPanelState::default();
        for index in 0..=NetworkDoctorPanelState::MAX_RECENT_ROWS {
            state.record_facade_event(&RuntimeFacadeEvent::PathRequested {
                destination_hash: format!("{index:032x}"),
            });
        }
        state.record_facade_event(&RuntimeFacadeEvent::PathRequested {
            destination_hash: format!("{:032x}", NetworkDoctorPanelState::MAX_RECENT_ROWS),
        });

        assert_eq!(
            state.recent_paths.len(),
            NetworkDoctorPanelState::MAX_RECENT_ROWS
        );
        assert_eq!(
            state.recent_paths[0].target,
            compact_hash(&format!(
                "{:032x}",
                NetworkDoctorPanelState::MAX_RECENT_ROWS
            ))
        );
    }

    #[test]
    fn resource_progress_preserves_offer_context() {
        let mut state = NetworkDoctorPanelState::default();
        state.record_facade_event(&RuntimeFacadeEvent::ResourceOffered {
            link_id: "link-1".into(),
            transfer_id: "resource-1".into(),
            purpose: "history".into(),
            bytes: Some(128),
            source: Some("omenchat".into()),
            direction: Some("inbound".into()),
            peer: Some("peer-1".into()),
        });
        state.record_facade_event(&RuntimeFacadeEvent::ResourceProgress {
            transfer_id: "resource-1".into(),
            received: 64,
            total: Some(128),
            source: None,
            purpose: None,
            direction: None,
            peer: None,
        });

        let active = state
            .active_resources
            .get("resource-1")
            .expect("active resource");
        assert_eq!(active.source, "omenchat");
        assert_eq!(active.purpose.as_deref(), Some("history"));
        assert_eq!(active.direction.as_deref(), Some("inbound"));
        assert_eq!(active.peer.as_deref(), Some("peer-1"));
        assert_eq!(active.received, Some(64));
    }
}
