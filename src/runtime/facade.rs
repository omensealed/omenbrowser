use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub reticulum_links: bool,
    pub reticulum_resources: bool,
    pub lxmf_direct: bool,
    pub lxmf_propagation: bool,
    pub lxmf_tickets: bool,
    pub lxmf_stamps: bool,
    pub rpc_daemon: bool,
    pub embedded_runtime: bool,
    pub gateway_mode: bool,
}

impl RuntimeCapabilities {
    pub fn clean_reticulum_lxmf() -> Self {
        Self {
            reticulum_links: cfg!(feature = "native-reticulum"),
            reticulum_resources: cfg!(feature = "native-reticulum"),
            lxmf_direct: cfg!(feature = "native-lxmf"),
            lxmf_propagation: cfg!(feature = "native-lxmf"),
            lxmf_tickets: cfg!(feature = "native-lxmf-sdk"),
            lxmf_stamps: cfg!(feature = "native-lxmf-sdk"),
            rpc_daemon: cfg!(feature = "native-rpc"),
            embedded_runtime: false,
            gateway_mode: cfg!(feature = "native-reticulum"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFacadeEvent {
    InterfaceUp {
        interface_id: String,
        detail: Option<String>,
    },
    InterfaceDown {
        interface_id: String,
        reason: Option<String>,
    },
    AnnounceHeard {
        destination_hash: String,
        app: String,
        aspect: String,
    },
    AnnounceObserved {
        destination_hash: String,
        kind: String,
        display_name: String,
        has_ratchet: bool,
    },
    PathRequested {
        destination_hash: String,
    },
    PathUpdated {
        destination_hash: String,
        known: bool,
        hops: Option<u32>,
    },
    PathFound {
        destination_hash: String,
        hops: Option<u32>,
    },
    LinkOpening {
        destination_hash: String,
    },
    LinkOpened {
        destination_hash: String,
        link_id: String,
    },
    LinkClosed {
        link_id: String,
        reason: Option<String>,
    },
    LinkFrameReceived {
        link_id: String,
        bytes: usize,
    },
    ResourceOffered {
        link_id: String,
        transfer_id: String,
        purpose: String,
        bytes: Option<u64>,
        source: Option<String>,
        direction: Option<String>,
        peer: Option<String>,
    },
    ResourceProgress {
        transfer_id: String,
        received: u64,
        total: Option<u64>,
        source: Option<String>,
        purpose: Option<String>,
        direction: Option<String>,
        peer: Option<String>,
    },
    ResourceComplete {
        transfer_id: String,
        bytes: u64,
        source: Option<String>,
        purpose: Option<String>,
        direction: Option<String>,
        peer: Option<String>,
    },
    ResourceFailed {
        transfer_id: String,
        reason: String,
        source: Option<String>,
        purpose: Option<String>,
        direction: Option<String>,
        peer: Option<String>,
    },
    LxmfEvent {
        event: String,
        detail: Option<String>,
    },
    Diagnostic {
        section: String,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LxmfAppMessageState {
    Draft,
    Queued,
    WaitingForPath,
    WaitingForTicket,
    WaitingForStamp,
    SendingDirect,
    SendingPropagated,
    DeliveredToPeer,
    DeliveredToPropagationNode,
    FailedNoPath,
    FailedStampRequired,
    FailedTicketExpired,
    FailedAttachmentTooLarge,
    FailedOther,
}

pub fn map_lxmf_status_to_app_state(status: &str) -> LxmfAppMessageState {
    match status.trim().to_ascii_lowercase().as_str() {
        "draft" => LxmfAppMessageState::Draft,
        "queued" | "pending" => LxmfAppMessageState::Queued,
        "waiting_for_path" | "path_pending" | "no_path_yet" => LxmfAppMessageState::WaitingForPath,
        "waiting_for_ticket" | "ticket_required" => LxmfAppMessageState::WaitingForTicket,
        "waiting_for_stamp" | "stamp_required" => LxmfAppMessageState::WaitingForStamp,
        "sending_direct" | "direct_send" => LxmfAppMessageState::SendingDirect,
        "sending_propagated" | "propagated_send" => LxmfAppMessageState::SendingPropagated,
        "delivered" | "delivered_to_peer" => LxmfAppMessageState::DeliveredToPeer,
        "propagation_node_accepted" | "delivered_to_propagation_node" => {
            LxmfAppMessageState::DeliveredToPropagationNode
        }
        "failed_no_path" | "no_path" => LxmfAppMessageState::FailedNoPath,
        "failed_stamp_required" => LxmfAppMessageState::FailedStampRequired,
        "failed_ticket_expired" => LxmfAppMessageState::FailedTicketExpired,
        "failed_attachment_too_large" | "attachment_too_large" => {
            LxmfAppMessageState::FailedAttachmentTooLarge
        }
        _ => LxmfAppMessageState::FailedOther,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_capabilities_default_safe() {
        let capabilities = RuntimeCapabilities::default();
        assert!(!capabilities.reticulum_links);
        assert!(!capabilities.reticulum_resources);
        assert!(!capabilities.lxmf_direct);
        assert!(!capabilities.embedded_runtime);
    }

    #[test]
    fn test_lxmf_delivery_state_mapping() {
        assert_eq!(
            map_lxmf_status_to_app_state("delivered_to_peer"),
            LxmfAppMessageState::DeliveredToPeer
        );
        assert_eq!(
            map_lxmf_status_to_app_state("propagation_node_accepted"),
            LxmfAppMessageState::DeliveredToPropagationNode
        );
        assert_eq!(
            map_lxmf_status_to_app_state("attachment_too_large"),
            LxmfAppMessageState::FailedAttachmentTooLarge
        );
        assert_eq!(
            map_lxmf_status_to_app_state("not-a-known-sdk-state"),
            LxmfAppMessageState::FailedOther
        );
    }
}
