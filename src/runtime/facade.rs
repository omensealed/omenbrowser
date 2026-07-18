use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLifecycleState {
    #[default]
    New,
    Starting,
    Running,
    Draining,
    Stopped,
    Failed,
}

impl RuntimeLifecycleState {
    pub fn accepts_new_work(self) -> bool {
        self == Self::Running
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        matches!(
            (self, next),
            (Self::New, Self::Starting | Self::Stopped | Self::Failed)
                | (
                    Self::Starting,
                    Self::Running | Self::Draining | Self::Stopped | Self::Failed
                )
                | (Self::Running, Self::Draining | Self::Failed)
                | (Self::Draining, Self::Stopped | Self::Failed)
                | (Self::Stopped, Self::Starting)
                | (
                    Self::Failed,
                    Self::Starting | Self::Draining | Self::Stopped
                )
        )
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailureCategory {
    Configuration,
    Identity,
    Interface,
    Transport,
    Rpc,
    Storage,
    Protocol,
    Shutdown,
    Internal,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeFailure {
    pub category: RuntimeFailureCategory,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technical_detail: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeLifecycleSnapshot {
    pub state: RuntimeLifecycleState,
    pub backend: crate::runtime::network::RuntimeBackendName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<RuntimeFailure>,
}

impl RuntimeLifecycleSnapshot {
    pub fn new(
        state: RuntimeLifecycleState,
        backend: crate::runtime::network::RuntimeBackendName,
    ) -> Self {
        Self {
            state,
            backend,
            failure: None,
        }
    }

    pub fn failed(
        backend: crate::runtime::network::RuntimeBackendName,
        failure: RuntimeFailure,
    ) -> Self {
        Self {
            state: RuntimeLifecycleState::Failed,
            backend,
            failure: Some(failure),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    DirectDelivery,
    OpportunisticDelivery,
    PropagatedDelivery,
    PaperUriDelivery,
    DeliveryCancellation,
    EventStream,
    History,
    ConversationListing,
    Tickets,
    Stamps,
    PropagationStatus,
    Attachments,
    SharedInstance,
    PathMetadata,
    InterfaceMutation,
    IntegratedBackend,
    RpcBackend,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapabilityAvailability {
    Supported,
    Unsupported,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapabilitySource {
    Compiled,
    Configured,
    Negotiated,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCapabilityRecord {
    pub capability: RuntimeCapability,
    pub availability: RuntimeCapabilityAvailability,
    pub source: RuntimeCapabilitySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCapabilitySnapshot {
    pub backend: crate::runtime::network::RuntimeBackendName,
    pub capabilities: Vec<RuntimeCapabilityRecord>,
}

impl Default for RuntimeCapabilitySnapshot {
    fn default() -> Self {
        Self {
            backend: crate::runtime::network::RuntimeBackendName::Auto,
            capabilities: Vec::new(),
        }
    }
}

impl RuntimeCapabilitySnapshot {
    pub fn availability(&self, capability: RuntimeCapability) -> RuntimeCapabilityAvailability {
        self.capabilities
            .iter()
            .find(|record| record.capability == capability)
            .map(|record| record.availability)
            .unwrap_or_default()
    }

    pub fn supports(&self, capability: RuntimeCapability) -> bool {
        self.availability(capability) == RuntimeCapabilityAvailability::Supported
    }
}

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
    fn lifecycle_transitions_are_explicit_and_restartable() {
        use RuntimeLifecycleState as State;

        assert!(State::New.can_transition_to(State::Starting));
        assert!(State::Starting.can_transition_to(State::Running));
        assert!(State::Running.can_transition_to(State::Draining));
        assert!(State::Draining.can_transition_to(State::Stopped));
        assert!(State::Stopped.can_transition_to(State::Starting));
        assert!(State::Failed.can_transition_to(State::Starting));
        assert!(State::Running.can_transition_to(State::Running));

        assert!(!State::New.can_transition_to(State::Running));
        assert!(!State::Running.can_transition_to(State::Stopped));
        assert!(!State::Draining.can_transition_to(State::Running));
        assert!(!State::Stopped.can_transition_to(State::Running));
        assert!(State::Running.accepts_new_work());
        assert!(!State::Draining.accepts_new_work());
        assert!(State::Stopped.is_terminal());
        assert!(State::Failed.is_terminal());
    }

    #[test]
    fn lifecycle_failure_is_structured_and_user_safe() {
        let snapshot = RuntimeLifecycleSnapshot::failed(
            crate::runtime::network::RuntimeBackendName::Reticulum,
            RuntimeFailure {
                category: RuntimeFailureCategory::Interface,
                summary: "configured interface could not start".into(),
                technical_detail: Some("tcp client rejected an empty host".into()),
                retryable: true,
            },
        );

        assert_eq!(snapshot.state, RuntimeLifecycleState::Failed);
        assert_eq!(
            snapshot.failure.as_ref().map(|failure| failure.category),
            Some(RuntimeFailureCategory::Interface)
        );
        assert!(snapshot
            .failure
            .as_ref()
            .is_some_and(|failure| failure.retryable));
    }

    #[test]
    fn capability_snapshot_defaults_missing_entries_to_unknown() {
        let snapshot = RuntimeCapabilitySnapshot {
            backend: crate::runtime::network::RuntimeBackendName::Reticulum,
            capabilities: vec![RuntimeCapabilityRecord {
                capability: RuntimeCapability::DirectDelivery,
                availability: RuntimeCapabilityAvailability::Supported,
                source: RuntimeCapabilitySource::Negotiated,
                detail: Some("backend probe".into()),
            }],
        };

        assert!(snapshot.supports(RuntimeCapability::DirectDelivery));
        assert!(!snapshot.supports(RuntimeCapability::EventStream));
        assert_eq!(
            snapshot.availability(RuntimeCapability::EventStream),
            RuntimeCapabilityAvailability::Unknown
        );
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
