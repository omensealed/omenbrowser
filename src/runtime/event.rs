use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::browser::{BrowserPage, DownloadedFile};
use crate::directory::DirectoryKind;
use crate::messaging::{MessageSummary, TransportMethod};
use crate::runtime::network::{
    AnnouncePayload, InterfaceStats, LxmfDeliveryEvidence, NetworkStatus, OmenChatLinkClosed,
    OmenChatLinkData, OmenChatResourceData, OutboundStatus, PageFetchProbeReport,
    PropagationStatus,
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
    Debug(String),
    Error(String),
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
