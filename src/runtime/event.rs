use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::browser::{BrowserPage, DownloadedFile};
use crate::directory::DirectoryKind;
use crate::messaging::{MessageSummary, TransportMethod};
use crate::runtime::network::{
    AnnouncePayload, InterfaceStats, LxmfDeliveryEvidence, LxmfHistoryPage, NetworkStatus,
    OmenChatLinkClosed, OmenChatLinkData, OmenChatResourceData, OutboundStatus,
    PageFetchProbeReport, PropagationStatus, ResourceLifecycleEvent, ResourceProgressEvent,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RuntimeBusEvent {
    StatusChanged(NetworkStatus),
    Announce(AnnouncePayload),
    PathUpdated(PathEvent),
    MessageReceived(MessageSummary),
    MessageDeliveryUpdated(OutboundStatus),
    LxmfDeliveryEvidence(LxmfDeliveryEvidence),
    PropagationStatus(PropagationStatus),
    PropagationSync(PropagationSyncEvent),
    InterfaceStats(InterfaceStats),
    PageFetchProbe(PageFetchProbeReport),
    OmenChatLinkClosed(OmenChatLinkClosed),
    OmenChatLinkData(OmenChatLinkData),
    OmenChatResourceData(OmenChatResourceData),
    ResourceProgress(ResourceProgressEvent),
    ResourceLifecycle(ResourceLifecycleEvent),
    SdkRpcEvent(RuntimeSdkRpcEvent),
    SdkDeliveryUpdated(RuntimeLxmfDeliveryUpdate),
    LxmfHistoryRecovered(LxmfHistoryPage),
    StreamGap(RuntimeEventGap),
    StreamRecovered(RuntimeEventRecovery),
    Debug(String),
    Error(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventSource {
    IntegratedBroadcast,
    SdkRpc,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventGapReason {
    SourceLag,
    DownstreamByteBudget,
    UpstreamStreamGap,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeEventGap {
    pub source: RuntimeEventSource,
    pub reason: RuntimeEventGapReason,
    pub dropped_count: u64,
    pub last_cursor: u64,
    pub next_cursor: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeEventRecovery {
    pub source: RuntimeEventSource,
    pub cursor: u64,
    pub status_recovered: bool,
    pub interfaces_recovered: bool,
    pub network_snapshot_recovered: bool,
    pub propagation_recovered: bool,
    pub directory_entries_recovered: usize,
    pub messages_recovered: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuntimeSdkRpcEvent {
    pub event_id: String,
    pub runtime_id: String,
    pub stream_id: String,
    pub seq_no: u64,
    pub contract_version: u16,
    pub ts_ms: u64,
    pub event_type: String,
    pub severity: String,
    pub source_component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub payload: serde_json::Value,
    pub cursor: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeLxmfDeliveryState {
    Queued,
    Dispatching,
    InFlight,
    Sent,
    Delivered,
    Failed,
    Cancelled,
    Expired,
    Rejected,
    Unknown,
}

impl RuntimeLxmfDeliveryState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Delivered | Self::Failed | Self::Cancelled | Self::Expired | Self::Rejected
        )
    }

    pub fn is_failure_terminal(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Cancelled | Self::Expired | Self::Rejected
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Dispatching => "dispatching",
            Self::InFlight => "in_flight",
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Rejected => "rejected",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeLxmfDeliveryUpdate {
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_state: Option<RuntimeLxmfDeliveryState>,
    pub state: RuntimeLxmfDeliveryState,
    pub terminal: bool,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub last_updated_ms: u64,
    pub event_id: String,
    pub seq_no: u64,
    pub cursor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PropagationSyncStage {
    SelectNode,
    PathCheck,
    AppDataCheck,
    IdentityLoad,
    LinkEstablish,
    LinkIdentify,
    CacheLoad,
    ListRequest,
    ListResponse,
    GetRequest,
    GetResponse,
    Decode,
    AckRequest,
    Complete,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PropagationSyncEventStatus {
    Started,
    Progress,
    Complete,
    Blocked,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropagationSyncEvent {
    pub stage: PropagationSyncStage,
    pub status: PropagationSyncEventStatus,
    pub destination_hash: Option<String>,
    pub detail: String,
    pub counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathEvent {
    pub destination_hash: String,
    pub known: bool,
    pub hops: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BrowserBusEvent {
    PageLoaded {
        tab_id: u64,
        generation: u64,
        result: Result<BrowserPage, String>,
    },
    DownloadFinished {
        tab_id: u64,
        generation: u64,
        result: Result<DownloadedFile, String>,
    },
    PartialRefreshTick {
        tab_id: u64,
        generation: u64,
        slot: String,
    },
    PartialRefreshResult {
        tab_id: u64,
        generation: u64,
        slot: String,
        result: Result<String, String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MessageBusEvent {
    SendResult {
        conversation_id: u64,
        generation: u64,
        result: Result<MessageSummary, String>,
    },
    SyncResult {
        generation: u64,
        result: Result<Vec<MessageSummary>, String>,
    },
    DeliveryReceipt(OutboundStatus),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DirectoryBusEvent {
    AnnounceReceived {
        destination_hash: String,
        display_name: String,
        kind: DirectoryKind,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TickKind {
    Render,
    RuntimeStatus,
    PartialRefresh,
    MessageSync,
    Diagnostics,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum AppEvent {
    Input(String),
    Tick(TickKind),
    Browser(BrowserBusEvent),
    Message(MessageBusEvent),
    Runtime(RuntimeBusEvent),
    Directory(DirectoryBusEvent),
    Plugin(String),
    Diagnostics(String),
    InterfaceChanged,
    Log(String),
    Shutdown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuntimeDebugMessage {
    pub backend: String,
    pub message: String,
}

pub fn transport_label(method: &TransportMethod) -> &'static str {
    match method {
        TransportMethod::Direct => "direct",
        TransportMethod::Propagated => "propagated",
        TransportMethod::Unknown(_) => "unknown",
    }
}
