use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Notify};

use crate::browser::{BrowserPage, DownloadedFile, PageSource};
use crate::directory::DirectoryKind;
use crate::error::AppResult;
use crate::identity::IdentityProfile;
use crate::interfaces::ReticulumInterfaceProfile;
use crate::messaging::{
    AttachmentSummary, DeliveryMode, MessageEnvelope, MessageSummary, TransportMethod,
};
use crate::runtime::facade::{
    RuntimeCapability, RuntimeCapabilityAvailability, RuntimeCapabilityRecord,
    RuntimeCapabilitySnapshot, RuntimeCapabilitySource, RuntimeLifecycleSnapshot,
    RuntimeLifecycleState,
};
use crate::runtime::RuntimeBusEvent;
use crate::storage::files::{atomic_write_new_bounded, next_available_download_path};

const SAMPLE_INDEX: &str = r#"#!c=60
>Welcome to OMENbrowser
Install one app, create an identity, and start exploring.

>>Highlights
`F0af`[Browse sample gallery`mock.page:/page/gallery.mu]`f
`F0af`[Open MicronPlus demo`mock.page:/page/micronplus.mu]`f
`F0af`[Open LXMF compose flow`lxmf@0123456789abcdef]`f

>>img2micron check
`Ff00`B00f▀`F0f0`Bf00▀`f`b

>>True-color check
`FTff0000`BT220000▀`FTff8800`BT332200▀`FTffee00`BT333300▀`FT66dd66`BT113311▀`FT33bbff`BT112244▀`FTaa88ff`BT221133▀`f`b
"#;

const SAMPLE_GALLERY: &str = r#">Micron Gallery
This page preserves spacing, color, and half-block art.

`Ff00`B00f▀`F0f0`Bf00▀`F00f`Bff0▀`f`b

`B444`<16|nickname`mesh friend>`b
`[Submit`mock.page:/page/index.mu`nickname]
"#;

const SAMPLE_MICRONPLUS_FALLBACK: &str = r#"#!c=0
>MicronPlus Demo
This mock page behaves like a node that detects MicronPlus support.

`Ff8fMicronPlus is bundled with OMENbrowser as a first-party plugin.`f
`Ff80Enable the `!micronplus-textui`! plugin in the Plugins tab, then reload this page.`f

`F888Without MicronPlus, the page falls back to this plain explanation instead of loading the live demo.`f
"#;

const SAMPLE_MICRONPLUS: &str = r#"#!c=0
>MicronPlus Demo
This mock page detected MicronPlus and loaded the richer layout.

[window title="MicronPlus Demo"]
[columns]
[column weight=3]
`F777This panel uses MicronPlus layout tags inside OMENbrowser.`f
[status text="MicronPlus detected" style="success"]
[live id="demo_feed" src=":/page/micronplus-feed.mu" refresh=2 loop=4 fields="message"]
[input name="message" submit=enter action="p:demo_feed:demo_log" fields="message"]
[button label="Refresh demo" action="p:demo_feed:demo_log" fields="message"]
[/column]
[column weight=2]
[scrollbox height=6]
`F6cfScrollbox sample`f
`FTff8800`BT221100True-color sample`f`b
Line 1
Line 2
[/scrollbox]
[log height=5 max=4 id="demo_log"]
demo: plugin active
demo: partial refresh available
demo: static columns available
demo: scrollbox/log containers available
[/log]
[/column]
[/columns]
[/window]
"#;

const SAMPLE_MICRONPLUS_FEED: &str = r#">Demo Feed
MicronPlus partial refresh is active.
"#;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DestinationId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeBackendName {
    Auto,
    Mock,
    Reticulum,
    Bridge(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkStatus {
    pub connected: bool,
    pub backend: RuntimeBackendName,
    pub active_identity: Option<IdentityProfile>,
    pub message: String,
}

pub type RuntimeStatus = NetworkStatus;

#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            if self.is_cancelled() {
                return;
            }
            notified.await;
            if self.is_cancelled() {
                return;
            }
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutboundStatus {
    pub peer_hash: String,
    pub message_id: Option<String>,
    pub delivered: bool,
    pub failed: bool,
    #[serde(default)]
    pub state: OutboundDeliveryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt: Option<f64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LxmfCancelOutcome {
    Accepted,
    AlreadyTerminal,
    NotFound,
    TooLateToCancel,
    Unsupported,
}

impl LxmfCancelOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AlreadyTerminal => "already_terminal",
            Self::NotFound => "not_found",
            Self::TooLateToCancel => "too_late_to_cancel",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LxmfDeliveryEvidence {
    pub peer_hash: String,
    pub message_id: Option<String>,
    pub kind: LxmfDeliveryEvidenceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<f64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LxmfDeliveryEvidenceKind {
    PacketSubmitted,
    RnsPacketProof,
    PropagationNodeAccepted,
    PropagationNodeFailed,
    PropagationSyncNoPayloads,
    LxmfRouterDelivered,
    LxmfRouterFailed,
    InboundPeerMessage,
    NoReceiptObserved,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutboundDeliveryState {
    #[default]
    Unknown,
    SubmittedToRuntime,
    SubmittedToRnsNet,
    Delivered,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AnnouncePayload {
    pub destination_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_hash: Option<String>,
    pub display_name: String,
    pub kind: DirectoryKind,
    pub associated_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_associated_hash: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_ratchet: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lxmf_stamp_cost: Option<u8>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum RuntimeEvent {
    Delivery(MessageSummary),
    OutboundStatus(OutboundStatus),
    Announce(AnnouncePayload),
    Debug(String),
    Status(NetworkStatus),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceStats {
    pub available: bool,
    pub reason: Option<String>,
    pub interfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<InterfaceSample>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_health: Option<Box<TransportHealthSnapshot>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransportHealthSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbound_queue: Option<InboundQueueHealthSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traffic: Option<TransportTrafficSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracked_links: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_links: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_links: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboundQueueHealthSnapshot {
    pub total_items: u64,
    pub classes: Vec<InboundQueueClassSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboundQueueClassSnapshot {
    pub class: String,
    pub items: u64,
    pub capacity: u64,
    pub dropped: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransportTrafficSnapshot {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub announce_rx_count: u64,
    pub announce_tx_count: u64,
    pub path_request_rx_count: u64,
    pub path_request_tx_count: u64,
    pub protocol_violations: u64,
    pub ifac_violations: u64,
    pub packet_filter_hits: u64,
    pub announce_burst_active: bool,
    pub path_request_burst_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceSample {
    pub profile_id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub state: InterfaceSampleState,
    pub enabled: bool,
    pub supported: bool,
    pub attached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceSampleState {
    #[default]
    Unknown,
    Disabled,
    Unsupported,
    Configured,
    Attached,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub announce_counts: BTreeMap<String, u32>,
    pub pending_announces: u32,
    pub known_destinations: u32,
    #[serde(default)]
    pub ratchet_announces: u32,
    #[serde(default)]
    pub path_table_available: bool,
    pub path_table_count: u32,
    #[serde(default)]
    pub request_failure_metrics_available: bool,
    pub request_failures: u32,
    pub active_propagation_node: Option<String>,
    pub connected_to_shared_instance: bool,
    pub is_shared_instance: bool,
    #[serde(default)]
    pub shared_instance_status_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfSdkRpcProbeSnapshot {
    pub endpoint: Option<String>,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_contract_version: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_stream_position: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_messages: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_flight_messages: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryCandidate {
    pub destination_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_hash: Option<String>,
    pub display_name: String,
    pub kind: DirectoryKind,
    pub associated_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_associated_hash: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_ratchet: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lxmf_stamp_cost: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DestinationInspection {
    pub destination_hash: String,
    pub valid_length: bool,
    pub has_path: bool,
    pub hops: Option<u32>,
    pub first_hop_timeout: Option<f64>,
    pub known_identity: bool,
    pub known_app_data: bool,
    pub propagation_usable: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageFetchProbeReport {
    pub backend: RuntimeBackendName,
    pub url: String,
    pub destination_hash: Option<String>,
    pub path: Option<String>,
    pub execute_request: bool,
    pub ready_to_request: bool,
    pub steps: Vec<PageFetchProbeStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageFetchProbeStep {
    pub stage: PageFetchProbeStage,
    pub ok: bool,
    pub detail: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub trace: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PageFetchProbeStage {
    AddressParse,
    RuntimeSetup,
    DestinationIdentity,
    PathDiscovery,
    LinkSetup,
    RequestSend,
    ResponseWait,
    ResponseDecode,
}

impl PageFetchProbeStep {
    pub fn ok(stage: PageFetchProbeStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            ok: true,
            detail: detail.into(),
            trace: BTreeMap::new(),
        }
    }

    pub fn failed(stage: PageFetchProbeStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            ok: false,
            detail: detail.into(),
            trace: BTreeMap::new(),
        }
    }

    pub fn with_trace(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.trace.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfDeliveryProbeReport {
    pub backend: RuntimeBackendName,
    pub peer_hash: String,
    pub execute_send: bool,
    pub ready_to_send: bool,
    pub steps: Vec<LxmfDeliveryProbeStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfDeliveryProbeStep {
    pub stage: LxmfDeliveryProbeStage,
    pub ok: bool,
    pub detail: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub trace: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LxmfDeliveryProbeStage {
    RuntimeSetup,
    SourceIdentity,
    PeerIdentity,
    PathDiscovery,
    PacketBuild,
    SendPacket,
}

impl LxmfDeliveryProbeStep {
    pub fn ok(stage: LxmfDeliveryProbeStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            ok: true,
            detail: detail.into(),
            trace: BTreeMap::new(),
        }
    }

    pub fn failed(stage: LxmfDeliveryProbeStage, detail: impl Into<String>) -> Self {
        Self {
            stage,
            ok: false,
            detail: detail.into(),
            trace: BTreeMap::new(),
        }
    }

    pub fn with_trace(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.trace.insert(key.into(), value.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropagationStatus {
    pub selected: bool,
    pub destination_hash: Option<String>,
    pub has_path: bool,
    pub known_app_data: bool,
    pub link_state: String,
    pub transfer_state: String,
}

pub const LXMF_INVITATION_CAPABILITY_PROBE_DEADLINE_MS: u64 = 15_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvitationCapabilityProbeOutcome {
    Supported,
    Unsupported,
    Unknown,
    Conflict,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PropagationDebugSnapshot {
    pub selected_node: Option<String>,
    pub router_state: String,
    pub pending_outbound_ids: Vec<String>,
    pub pending_deferred_ids: Vec<String>,
    pub failed_outbound_ids: Vec<String>,
    pub link_state: String,
    pub message: Option<PropagationMessageSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PropagationMessageSnapshot {
    pub origin: String,
    pub message_id: String,
    pub state: Option<String>,
    pub desired_method: Option<String>,
    pub method: Option<String>,
    pub representation: Option<String>,
    pub progress: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatLinkOpened {
    pub destination_hash: String,
    pub link_id: [u8; 16],
    pub rtt_millis: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatLinkData {
    pub link_id: [u8; 16],
    pub frame_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatLinkClosed {
    pub link_id: [u8; 16],
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmenChatResourceData {
    pub link_id: [u8; 16],
    pub data: Vec<u8>,
    pub metadata: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceProgressEvent {
    pub transfer_id: String,
    pub received: u64,
    pub total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceLifecycleEvent {
    pub transfer_id: String,
    pub state: ResourceLifecycleState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLifecycleState {
    Offered,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfCorrelationRecovery {
    pub direct_recovered: usize,
    pub propagated_recovered: usize,
}

pub const LXMF_HISTORY_PAGE_MAX_ITEMS: usize = 128;
pub const LXMF_HISTORY_PAGE_MAX_BYTES: usize = 4 * 1024 * 1024;
pub const LXMF_HISTORY_CURSOR_MAX_BYTES: usize = 4 * 1024;
pub const LXMF_HISTORY_RECOVERY_MAX_PAGES: usize = 4;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfHistoryRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfHistoryRecord {
    pub message_id: String,
    pub source: String,
    pub destination: String,
    pub title: String,
    pub content: String,
    pub timestamp: i64,
    pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_status: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LxmfHistoryPage {
    pub messages: Vec<LxmfHistoryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl LxmfHistoryRequest {
    pub fn bounded(
        peer_hash: Option<String>,
        cursor: Option<String>,
        limit: usize,
    ) -> AppResult<Self> {
        if peer_hash.as_ref().is_some_and(|peer| {
            peer.is_empty() || peer.len() > 256 || peer.chars().any(char::is_control)
        }) || cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > LXMF_HISTORY_CURSOR_MAX_BYTES)
        {
            return Err(crate::error::AppError::Runtime(
                "LXMF history request exceeds its scalar safety limits".into(),
            ));
        }
        Ok(Self {
            peer_hash,
            cursor,
            limit: limit.clamp(1, LXMF_HISTORY_PAGE_MAX_ITEMS),
        })
    }
}

impl LxmfHistoryPage {
    pub fn validate(self) -> AppResult<Self> {
        if self.messages.len() > LXMF_HISTORY_PAGE_MAX_ITEMS
            || self
                .next_cursor
                .as_ref()
                .is_some_and(|cursor| cursor.len() > LXMF_HISTORY_CURSOR_MAX_BYTES)
        {
            return Err(crate::error::AppError::Runtime(
                "LXMF history page exceeds its item or cursor limit".into(),
            ));
        }
        let encoded_bytes = serde_json::to_vec(&self)
            .map_err(|error| crate::error::AppError::Runtime(error.to_string()))?
            .len();
        if encoded_bytes > LXMF_HISTORY_PAGE_MAX_BYTES {
            return Err(crate::error::AppError::Runtime(format!(
                "LXMF history page exceeds the {LXMF_HISTORY_PAGE_MAX_BYTES} byte limit"
            )));
        }
        Ok(self)
    }
}

#[async_trait]
pub trait NetworkRuntime: Send + Sync {
    fn subscribe_events(&self) -> Option<broadcast::Receiver<RuntimeBusEvent>> {
        None
    }

    async fn start_runtime(
        &self,
        identity: Option<IdentityProfile>,
        interfaces: Vec<ReticulumInterfaceProfile>,
    ) -> AppResult<()> {
        let _ = (identity, interfaces);
        Ok(())
    }

    async fn stop_runtime(&self) -> AppResult<()> {
        Ok(())
    }

    async fn lifecycle_snapshot(&self) -> RuntimeLifecycleSnapshot {
        let status = self.status().await;
        RuntimeLifecycleSnapshot::new(
            if status.connected {
                RuntimeLifecycleState::Running
            } else {
                RuntimeLifecycleState::Stopped
            },
            status.backend,
        )
    }

    async fn capability_snapshot(&self) -> RuntimeCapabilitySnapshot {
        RuntimeCapabilitySnapshot {
            backend: self.status().await.backend,
            capabilities: Vec::new(),
        }
    }

    async fn status(&self) -> NetworkStatus;
    async fn attach_identity(&self, identity: IdentityProfile) -> AppResult<()>;
    async fn announce_identity(&self) -> AppResult<bool>;

    async fn probe_lxmf_invitation_capability(
        &self,
        peer_hash: &str,
        cancel: CancellationToken,
    ) -> AppResult<InvitationCapabilityProbeOutcome> {
        let _ = (peer_hash, cancel);
        Err(crate::error::AppError::Unsupported(
            "LXMF invitation capability probing is unavailable for this backend".into(),
        ))
    }

    async fn set_identify_on_connect_destinations(
        &self,
        destination_hashes: BTreeSet<String>,
    ) -> AppResult<()> {
        let _ = destination_hashes;
        Ok(())
    }

    async fn fetch_page(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
    ) -> AppResult<BrowserPage>;

    async fn fetch_page_with_operation(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
        operation_id: Option<String>,
    ) -> AppResult<BrowserPage> {
        let _ = operation_id;
        self.fetch_page(url, request_data, cancel).await
    }

    async fn download_file(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
    ) -> AppResult<DownloadedFile>;

    async fn download_file_with_operation(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
        operation_id: Option<String>,
    ) -> AppResult<DownloadedFile> {
        let _ = operation_id;
        self.download_file(url, downloads_dir, cancel).await
    }

    async fn list_messages(&self) -> AppResult<Vec<MessageSummary>>;
    async fn lxmf_history_page(&self, request: LxmfHistoryRequest) -> AppResult<LxmfHistoryPage> {
        let _ = request;
        Err(crate::error::AppError::Unsupported(
            "runtime does not expose typed LXMF history".into(),
        ))
    }
    async fn send_message(&self, envelope: MessageEnvelope) -> AppResult<MessageSummary>;
    async fn cancel_lxmf_delivery(&self, message_id: &str) -> AppResult<LxmfCancelOutcome> {
        let _ = message_id;
        Ok(LxmfCancelOutcome::Unsupported)
    }
    async fn create_contact(&self, peer_hash: &str, label: &str) -> AppResult<()>;

    async fn recover_lxmf_correlation(
        &self,
        messages: Vec<MessageSummary>,
    ) -> AppResult<LxmfCorrelationRecovery> {
        let _ = messages;
        Ok(LxmfCorrelationRecovery::default())
    }

    async fn set_outbound_propagation_node(&self, hash: Option<String>) -> AppResult<()>;
    async fn get_outbound_propagation_node(&self) -> AppResult<Option<String>>;
    async fn sync_propagation_messages(&self, limit: Option<u32>) -> AppResult<()>;

    async fn request_path(
        &self,
        destination_hash: &str,
        reason: &str,
        sibling_aspects: bool,
    ) -> AppResult<bool>;
    async fn warm_paths(
        &self,
        hashes: &[String],
        max_requests: u32,
        cooldown_secs: u64,
    ) -> AppResult<u32>;
    async fn preload_known_destinations(&self, path: &Path) -> AppResult<usize> {
        let _ = path;
        Err(crate::error::AppError::Unsupported(
            "runtime does not support preloading known destinations".into(),
        ))
    }

    async fn interface_stats(&self) -> AppResult<InterfaceStats>;
    async fn network_snapshot(&self) -> AppResult<NetworkSnapshot>;
    async fn directory_candidates(
        &self,
        limit: Option<usize>,
        include_propagation_usable: bool,
    ) -> AppResult<Vec<DirectoryCandidate>>;
    async fn inspect_destination(
        &self,
        destination_hash: &str,
        include_propagation_usable: bool,
    ) -> AppResult<DestinationInspection>;
    async fn propagation_status(&self) -> AppResult<PropagationStatus>;
    async fn propagation_debug_snapshot(
        &self,
        message_id: Option<String>,
    ) -> AppResult<PropagationDebugSnapshot> {
        let status = self.propagation_status().await?;
        Ok(PropagationDebugSnapshot {
            selected_node: status.destination_hash,
            router_state: status.transfer_state,
            pending_outbound_ids: Vec::new(),
            pending_deferred_ids: Vec::new(),
            failed_outbound_ids: Vec::new(),
            link_state: status.link_state,
            message: message_id.map(|message_id| PropagationMessageSnapshot {
                origin: "-".into(),
                message_id,
                state: None,
                desired_method: None,
                method: None,
                representation: None,
                progress: None,
            }),
        })
    }

    async fn native_lxmf_sdk_rpc_probe(&self) -> AppResult<LxmfSdkRpcProbeSnapshot> {
        Ok(LxmfSdkRpcProbeSnapshot {
            endpoint: None,
            state: "disabled".into(),
            runtime_id: None,
            active_contract_version: None,
            event_stream_position: None,
            config_revision: None,
            queued_messages: None,
            in_flight_messages: None,
            detail: Some("native LXMF SDK/RPC feature is not enabled".into()),
        })
    }

    async fn probe_page_fetch(
        &self,
        url: &str,
        execute_request: bool,
    ) -> AppResult<PageFetchProbeReport> {
        Ok(PageFetchProbeReport {
            backend: self.status().await.backend,
            url: url.into(),
            destination_hash: None,
            path: None,
            execute_request,
            ready_to_request: false,
            steps: vec![PageFetchProbeStep::failed(
                PageFetchProbeStage::RuntimeSetup,
                "this runtime does not expose a page-fetch interop probe",
            )],
        })
    }

    async fn probe_lxmf_delivery(
        &self,
        peer_hash: &str,
        execute_send: bool,
    ) -> AppResult<LxmfDeliveryProbeReport> {
        Ok(LxmfDeliveryProbeReport {
            backend: self.status().await.backend,
            peer_hash: peer_hash.into(),
            execute_send,
            ready_to_send: false,
            steps: vec![LxmfDeliveryProbeStep::failed(
                LxmfDeliveryProbeStage::RuntimeSetup,
                "this runtime does not expose an LXMF delivery interop probe",
            )],
        })
    }

    async fn open_omenchat_link(
        &self,
        destination_hash: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<OmenChatLinkOpened> {
        let _ = (destination_hash, timeout, cancel);
        Err(crate::error::AppError::Unsupported(
            "runtime does not support OMENchat links".into(),
        ))
    }

    async fn send_omenchat_frame(&self, link_id: [u8; 16], frame_bytes: Vec<u8>) -> AppResult<()> {
        let _ = (link_id, frame_bytes);
        Err(crate::error::AppError::Unsupported(
            "runtime does not support OMENchat link frames".into(),
        ))
    }

    async fn send_omenchat_resource(
        &self,
        link_id: [u8; 16],
        resource_id: String,
        payload: Vec<u8>,
    ) -> AppResult<()> {
        let _ = (link_id, resource_id, payload);
        Err(crate::error::AppError::Unsupported(
            "runtime does not support OMENchat link resources".into(),
        ))
    }

    async fn close_omenchat_link(&self, link_id: [u8; 16]) -> AppResult<bool> {
        let _ = link_id;
        Err(crate::error::AppError::Unsupported(
            "runtime does not support OMENchat link teardown".into(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct MockNetworkRuntime {
    state: Arc<Mutex<MockRuntimeState>>,
}

#[derive(Clone, Debug)]
struct MockRuntimeState {
    lifecycle: RuntimeLifecycleState,
    active_identity: Option<IdentityProfile>,
    preferred_propagation_node_hash: Option<String>,
    messages: Vec<MessageSummary>,
}

impl Default for MockNetworkRuntime {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockRuntimeState {
                lifecycle: RuntimeLifecycleState::Running,
                active_identity: None,
                preferred_propagation_node_hash: None,
                messages: vec![welcome_message()],
            })),
        }
    }
}

impl MockNetworkRuntime {
    fn require_running(&self, operation: &str) -> AppResult<()> {
        let lifecycle = self
            .state
            .lock()
            .expect("mock runtime mutex poisoned")
            .lifecycle;
        if lifecycle.accepts_new_work() {
            Ok(())
        } else {
            Err(crate::error::AppError::Runtime(format!(
                "mock runtime cannot {operation} while lifecycle is {lifecycle:?}"
            )))
        }
    }
}

#[async_trait]
impl NetworkRuntime for MockNetworkRuntime {
    async fn start_runtime(
        &self,
        identity: Option<IdentityProfile>,
        interfaces: Vec<ReticulumInterfaceProfile>,
    ) -> AppResult<()> {
        let _ = interfaces;
        let mut state = self.state.lock().expect("mock runtime mutex poisoned");
        if state.lifecycle == RuntimeLifecycleState::Running {
            if identity.is_none() || state.active_identity.as_ref() == identity.as_ref() {
                return Ok(());
            }
            if state.active_identity.is_none() {
                state.active_identity = identity;
                return Ok(());
            }
            return Err(crate::error::AppError::Runtime(
                "mock runtime is already running with a different identity".into(),
            ));
        }
        if matches!(
            state.lifecycle,
            RuntimeLifecycleState::Starting | RuntimeLifecycleState::Draining
        ) {
            return Err(crate::error::AppError::Runtime(format!(
                "mock runtime cannot start while lifecycle is {:?}",
                state.lifecycle
            )));
        }
        state.lifecycle = RuntimeLifecycleState::Starting;
        if let Some(identity) = identity {
            state.active_identity = Some(identity);
        }
        state.lifecycle = RuntimeLifecycleState::Running;
        Ok(())
    }

    async fn stop_runtime(&self) -> AppResult<()> {
        let mut state = self.state.lock().expect("mock runtime mutex poisoned");
        if state.lifecycle == RuntimeLifecycleState::Stopped {
            return Ok(());
        }
        state.lifecycle = RuntimeLifecycleState::Draining;
        state.lifecycle = RuntimeLifecycleState::Stopped;
        Ok(())
    }

    async fn lifecycle_snapshot(&self) -> RuntimeLifecycleSnapshot {
        RuntimeLifecycleSnapshot::new(
            self.state
                .lock()
                .expect("mock runtime mutex poisoned")
                .lifecycle,
            RuntimeBackendName::Mock,
        )
    }

    async fn capability_snapshot(&self) -> RuntimeCapabilitySnapshot {
        let supported = [
            RuntimeCapability::DirectDelivery,
            RuntimeCapability::PropagatedDelivery,
            RuntimeCapability::History,
            RuntimeCapability::Attachments,
        ]
        .into_iter()
        .map(|capability| RuntimeCapabilityRecord {
            capability,
            availability: RuntimeCapabilityAvailability::Supported,
            source: RuntimeCapabilitySource::Configured,
            detail: Some("mock simulation only".into()),
        });
        let unsupported = [
            RuntimeCapability::IntegratedBackend,
            RuntimeCapability::RpcBackend,
            RuntimeCapability::SharedInstance,
            RuntimeCapability::InterfaceMutation,
            RuntimeCapability::AuthenticatedLxmfSourceEvidence,
        ]
        .into_iter()
        .map(|capability| RuntimeCapabilityRecord {
            capability,
            availability: RuntimeCapabilityAvailability::Unsupported,
            source: RuntimeCapabilitySource::Configured,
            detail: Some("not provided by the mock backend".into()),
        });
        RuntimeCapabilitySnapshot {
            backend: RuntimeBackendName::Mock,
            capabilities: supported.chain(unsupported).collect(),
        }
    }

    async fn status(&self) -> NetworkStatus {
        let state = self.state.lock().expect("mock runtime mutex poisoned");
        NetworkStatus {
            connected: state.active_identity.is_some(),
            backend: RuntimeBackendName::Mock,
            active_identity: state.active_identity.clone(),
            message: "Mock runtime active".into(),
        }
    }

    async fn attach_identity(&self, identity: IdentityProfile) -> AppResult<()> {
        self.state
            .lock()
            .expect("mock runtime mutex poisoned")
            .active_identity = Some(identity);
        Ok(())
    }

    async fn announce_identity(&self) -> AppResult<bool> {
        self.require_running("announce an identity")?;
        Ok(self
            .state
            .lock()
            .expect("mock runtime mutex poisoned")
            .active_identity
            .is_some())
    }

    async fn fetch_page(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
    ) -> AppResult<BrowserPage> {
        self.require_running("fetch a page")?;
        if cancel.is_cancelled() {
            return Err(crate::error::AppError::Runtime("request cancelled".into()));
        }
        if !is_mock_browser_url(url) {
            return Err(crate::error::AppError::Runtime(format!(
                "mock runtime cannot load real NomadNet address {url}; switch to Reticulum backend and start native networking"
            )));
        }
        let saved = request_data
            .as_ref()
            .filter(|data| !data.is_empty())
            .map(|data| {
                let fields = data
                    .iter()
                    .map(|(key, value)| format!("{key}: `{value}`"))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("\n\n{fields}")
            })
            .unwrap_or_default();

        let (title, mut markup) = if url.contains("micronplus-feed") {
            let mut markup = SAMPLE_MICRONPLUS_FEED.to_string();
            if let Some(message) = request_data
                .as_ref()
                .and_then(|data| data.get("field_message"))
            {
                markup.push_str(&format!("\nLast message: `{message}`"));
            }
            ("MicronPlus Feed", markup)
        } else if url.contains("micronplus") {
            let enabled = request_data
                .as_ref()
                .and_then(|data| data.get("var_micronplus_plugin_enabled"))
                .is_some_and(|value| value == "1");
            (
                "MicronPlus Demo",
                if enabled {
                    SAMPLE_MICRONPLUS.to_string()
                } else {
                    SAMPLE_MICRONPLUS_FALLBACK.to_string()
                },
            )
        } else if url.contains("gallery") {
            let mut markup = SAMPLE_GALLERY.to_string();
            if let Some(nickname) = request_data
                .as_ref()
                .and_then(|data| data.get("field_nickname"))
            {
                markup.push_str(&format!("\n\nSaved nickname: `{nickname}`"));
            }
            ("Micron Gallery", markup)
        } else {
            ("Welcome", SAMPLE_INDEX.to_string())
        };
        markup.push_str(&saved);

        Ok(BrowserPage {
            url: url.into(),
            markup,
            title: title.into(),
            source: PageSource::Mock,
            metadata: BTreeMap::new(),
            request_data,
        })
    }

    async fn download_file(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
    ) -> AppResult<DownloadedFile> {
        self.require_running("download a file")?;
        if cancel.is_cancelled() {
            return Err(crate::error::AppError::Runtime("download cancelled".into()));
        }
        let path = next_available_download_path(downloads_dir, "mock-download.txt")?;
        atomic_write_new_bounded(
            path.clone(),
            format!("Downloaded from {url}\n").into_bytes(),
        )
        .await?;
        Ok(DownloadedFile {
            url: url.into(),
            path,
            content_type: "text/plain".into(),
        })
    }

    async fn list_messages(&self) -> AppResult<Vec<MessageSummary>> {
        Ok(self
            .state
            .lock()
            .expect("mock runtime mutex poisoned")
            .messages
            .clone())
    }

    async fn send_message(&self, envelope: MessageEnvelope) -> AppResult<MessageSummary> {
        self.require_running("send a message")?;
        if envelope
            .operation
            .as_ref()
            .is_some_and(|operation| operation.remaining_ttl_ms().is_none())
        {
            return Err(crate::error::AppError::Runtime(
                "LXMF send deadline expired before mock runtime admission".into(),
            ));
        }
        let attachments = envelope
            .attachments
            .iter()
            .filter_map(attachment_summary)
            .collect::<Vec<_>>();
        let message = MessageSummary {
            peer_hash: envelope.peer_hash.clone(),
            peer_label: envelope.peer_hash.chars().take(8).collect(),
            title: envelope.title,
            content: envelope.body,
            timestamp: unix_timestamp(),
            transport_method: match envelope.delivery_mode {
                DeliveryMode::Direct => TransportMethod::Direct,
                DeliveryMode::Propagated => TransportMethod::Propagated,
            },
            delivered: true,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some(format!("mock-{}", unix_timestamp_millis())),
            fields: BTreeMap::new(),
            attachments,
        };
        self.state
            .lock()
            .expect("mock runtime mutex poisoned")
            .messages
            .insert(0, message.clone());
        Ok(message)
    }

    async fn probe_lxmf_delivery(
        &self,
        peer_hash: &str,
        execute_send: bool,
    ) -> AppResult<LxmfDeliveryProbeReport> {
        Ok(LxmfDeliveryProbeReport {
            backend: RuntimeBackendName::Mock,
            peer_hash: peer_hash.into(),
            execute_send,
            ready_to_send: false,
            steps: vec![LxmfDeliveryProbeStep::failed(
                LxmfDeliveryProbeStage::RuntimeSetup,
                "mock runtime does not perform native LXMF delivery",
            )],
        })
    }

    async fn create_contact(&self, _peer_hash: &str, _label: &str) -> AppResult<()> {
        Ok(())
    }

    async fn set_outbound_propagation_node(&self, hash: Option<String>) -> AppResult<()> {
        self.state
            .lock()
            .expect("mock runtime mutex poisoned")
            .preferred_propagation_node_hash = hash;
        Ok(())
    }

    async fn get_outbound_propagation_node(&self) -> AppResult<Option<String>> {
        Ok(self
            .state
            .lock()
            .expect("mock runtime mutex poisoned")
            .preferred_propagation_node_hash
            .clone())
    }

    async fn sync_propagation_messages(&self, _limit: Option<u32>) -> AppResult<()> {
        self.require_running("synchronize propagation messages")?;
        Ok(())
    }

    async fn request_path(
        &self,
        destination_hash: &str,
        _reason: &str,
        _sibling_aspects: bool,
    ) -> AppResult<bool> {
        self.require_running("request a path")?;
        Ok(is_mock_known_destination(destination_hash))
    }

    async fn warm_paths(
        &self,
        hashes: &[String],
        max_requests: u32,
        _cooldown_secs: u64,
    ) -> AppResult<u32> {
        self.require_running("warm paths")?;
        Ok(hashes
            .iter()
            .filter(|hash| is_mock_known_destination(hash))
            .take(max_requests as usize)
            .count() as u32)
    }

    async fn preload_known_destinations(&self, _path: &Path) -> AppResult<usize> {
        Ok(0)
    }

    async fn interface_stats(&self) -> AppResult<InterfaceStats> {
        Ok(InterfaceStats {
            available: false,
            reason: Some("rnstatus is only available with the Reticulum backend".into()),
            interfaces: Vec::new(),
            samples: Vec::new(),
            transport_health: None,
        })
    }

    async fn network_snapshot(&self) -> AppResult<NetworkSnapshot> {
        let active_propagation_node = self
            .state
            .lock()
            .expect("mock runtime mutex poisoned")
            .preferred_propagation_node_hash
            .clone();
        Ok(NetworkSnapshot {
            announce_counts: BTreeMap::from([
                ("node".into(), 1),
                ("peer".into(), 1),
                ("propagation".into(), 1),
            ]),
            pending_announces: 0,
            known_destinations: 2,
            ratchet_announces: 0,
            path_table_available: false,
            path_table_count: 0,
            request_failure_metrics_available: false,
            request_failures: 0,
            active_propagation_node,
            connected_to_shared_instance: false,
            is_shared_instance: false,
            shared_instance_status_available: false,
        })
    }

    async fn directory_candidates(
        &self,
        limit: Option<usize>,
        _include_propagation_usable: bool,
    ) -> AppResult<Vec<DirectoryCandidate>> {
        let mut candidates = mock_directory_candidates();
        if let Some(limit) = limit {
            candidates.truncate(limit);
        }
        Ok(candidates)
    }

    async fn inspect_destination(
        &self,
        destination_hash: &str,
        include_propagation_usable: bool,
    ) -> AppResult<DestinationInspection> {
        let has_path = destination_hash == "0123456789abcdef"
            || destination_hash == "0123456789abcdef0123456789abcdef";
        Ok(DestinationInspection {
            destination_hash: destination_hash.into(),
            valid_length: true,
            has_path,
            hops: has_path.then_some(1),
            first_hop_timeout: has_path.then_some(1.0),
            known_identity: has_path,
            known_app_data: has_path,
            propagation_usable: include_propagation_usable.then_some(false),
        })
    }

    async fn probe_page_fetch(
        &self,
        url: &str,
        execute_request: bool,
    ) -> AppResult<PageFetchProbeReport> {
        Ok(PageFetchProbeReport {
            backend: RuntimeBackendName::Mock,
            url: url.into(),
            destination_hash: None,
            path: None,
            execute_request,
            ready_to_request: false,
            steps: vec![PageFetchProbeStep::failed(
                PageFetchProbeStage::RuntimeSetup,
                "mock runtime does not perform native Reticulum page fetches",
            )],
        })
    }

    async fn propagation_status(&self) -> AppResult<PropagationStatus> {
        let destination_hash = self
            .state
            .lock()
            .expect("mock runtime mutex poisoned")
            .preferred_propagation_node_hash
            .clone();
        let has_path = destination_hash.as_deref() == Some("fedcba9876543210");
        Ok(PropagationStatus {
            selected: destination_hash.is_some(),
            destination_hash,
            has_path,
            known_app_data: has_path,
            link_state: if has_path { "active" } else { "none" }.into(),
            transfer_state: "idle".into(),
        })
    }
}

fn is_mock_browser_url(url: &str) -> bool {
    url.starts_with("mock.node:")
        || url.starts_with("mock.page:")
        || url.starts_with("mock:")
        || url.starts_with("demo:")
        || url.contains("micronplus")
        || url.contains("gallery")
}

fn welcome_message() -> MessageSummary {
    MessageSummary {
        peer_hash: "0123456789abcdef".into(),
        peer_label: "Welcome Bot".into(),
        title: "Welcome to LXMF".into(),
        content: "Mock runtime is active. Install Reticulum and LXMF to go live.".into(),
        timestamp: unix_timestamp(),
        transport_method: TransportMethod::Direct,
        delivered: true,
        failed: false,
        incoming: true,
        unread: true,
        message_id: Some("mock-welcome".into()),
        fields: BTreeMap::new(),
        attachments: Vec::new(),
    }
}

fn mock_directory_candidates() -> Vec<DirectoryCandidate> {
    vec![
        DirectoryCandidate {
            destination_hash: "mock.node".into(),
            identity_hash: None,
            display_name: "Mock Node".into(),
            kind: DirectoryKind::Node,
            associated_hash: Some("0123456789abcdef".into()),
            node_associated_hash: None,
            has_ratchet: false,
            lxmf_stamp_cost: None,
        },
        DirectoryCandidate {
            destination_hash: "0123456789abcdef".into(),
            identity_hash: None,
            display_name: "Welcome Bot".into(),
            kind: DirectoryKind::Peer,
            associated_hash: Some("mock.node".into()),
            node_associated_hash: None,
            has_ratchet: false,
            lxmf_stamp_cost: None,
        },
        DirectoryCandidate {
            destination_hash: "fedcba9876543210".into(),
            identity_hash: None,
            display_name: "Mock Propagation Node".into(),
            kind: DirectoryKind::Propagation,
            associated_hash: Some("0123456789abcdef".into()),
            node_associated_hash: Some("mock.node".into()),
            has_ratchet: false,
            lxmf_stamp_cost: None,
        },
    ]
}

fn attachment_summary(path: &PathBuf) -> Option<AttachmentSummary> {
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_file().then(|| AttachmentSummary {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment")
            .into(),
        size: metadata.len(),
        path: Some(path.clone()),
    })
}

fn is_mock_known_destination(destination_hash: &str) -> bool {
    matches!(
        destination_hash,
        "0123456789abcdef" | "0123456789abcdef0123456789abcdef" | "mock.node" | "fedcba9876543210"
    )
}

fn unix_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn unix_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> IdentityProfile {
        IdentityProfile {
            label: "mock".into(),
            path: "mock://identity".into(),
            hash_hex: "abcd".into(),
            managed: true,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omenbrowser-rs-runtime-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn runtime_status_round_trips_json() {
        let status = NetworkStatus {
            connected: true,
            backend: RuntimeBackendName::Mock,
            active_identity: Some(identity()),
            message: "ready".into(),
        };

        let json = serde_json::to_string(&status).expect("serialize runtime status");
        let decoded: NetworkStatus =
            serde_json::from_str(&json).expect("deserialize runtime status");

        assert_eq!(decoded, status);
    }

    #[test]
    fn page_fetch_probe_report_round_trips_json() {
        let report = PageFetchProbeReport {
            backend: RuntimeBackendName::Reticulum,
            url: "00112233445566778899aabbccddeeff:/".into(),
            destination_hash: Some("00112233445566778899aabbccddeeff".into()),
            path: Some("/".into()),
            execute_request: false,
            ready_to_request: false,
            steps: vec![PageFetchProbeStep::failed(
                PageFetchProbeStage::DestinationIdentity,
                "destination signing key is not known",
            )
            .with_trace("destination", "00112233445566778899aabbccddeeff")
            .with_trace("source", "known_destinations")],
        };

        let json = serde_json::to_string(&report).expect("serialize probe");
        let decoded: PageFetchProbeReport = serde_json::from_str(&json).expect("deserialize probe");

        assert_eq!(decoded, report);
        assert!(json.contains("destination_identity"));
        assert!(json.contains("\"trace\""));
        assert!(json.contains("known_destinations"));
    }

    #[test]
    fn announce_payload_and_directory_candidate_accept_legacy_json_without_node_association() {
        let announce: AnnouncePayload = serde_json::from_value(serde_json::json!({
            "destination_hash": "prop",
            "display_name": "Propagation",
            "kind": "propagation",
            "associated_hash": "peer"
        }))
        .expect("deserialize legacy announce payload");

        assert_eq!(announce.node_associated_hash, None);
        assert_eq!(announce.lxmf_stamp_cost, None);

        let candidate: DirectoryCandidate = serde_json::from_value(serde_json::json!({
            "destination_hash": "prop",
            "display_name": "Propagation",
            "kind": "propagation",
            "associated_hash": "peer"
        }))
        .expect("deserialize legacy directory candidate");

        assert_eq!(candidate.node_associated_hash, None);
        assert_eq!(candidate.lxmf_stamp_cost, None);
    }

    #[tokio::test]
    async fn mock_status_tracks_attached_identity() {
        let runtime = MockNetworkRuntime::default();
        assert!(!runtime.status().await.connected);

        runtime
            .attach_identity(identity())
            .await
            .expect("attach identity");
        let status = runtime.status().await;

        assert!(status.connected);
        assert_eq!(
            status
                .active_identity
                .as_ref()
                .map(|identity| identity.label.as_str()),
            Some("mock")
        );
    }

    #[tokio::test]
    async fn mock_lifecycle_tracks_start_and_orderly_stop() {
        let runtime = MockNetworkRuntime::default();
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Running
        );

        runtime
            .start_runtime(Some(identity()), Vec::new())
            .await
            .expect("start mock runtime");
        runtime
            .start_runtime(Some(identity()), Vec::new())
            .await
            .expect("repeat identical mock start");
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Running
        );

        runtime.stop_runtime().await.expect("stop mock runtime");
        runtime
            .stop_runtime()
            .await
            .expect("repeat mock stop is idempotent");
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Stopped
        );
        assert!(runtime
            .fetch_page("mock.node:/", None, CancellationToken::new())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn mock_duplicate_start_rejects_conflicting_identity() {
        let runtime = MockNetworkRuntime::default();
        runtime
            .start_runtime(Some(identity()), Vec::new())
            .await
            .expect("attach mock identity");
        let mut other = identity();
        other.hash_hex = "different".into();

        let error = runtime
            .start_runtime(Some(other), Vec::new())
            .await
            .expect_err("conflicting mock restart must fail");
        assert!(error.to_string().contains("different identity"));
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Running
        );
    }

    #[tokio::test]
    async fn mock_capabilities_are_explicit_and_backend_scoped() {
        let runtime = MockNetworkRuntime::default();
        let snapshot = runtime.capability_snapshot().await;

        assert_eq!(snapshot.backend, RuntimeBackendName::Mock);
        assert!(snapshot.supports(RuntimeCapability::DirectDelivery));
        assert!(snapshot.supports(RuntimeCapability::History));
        assert_eq!(
            snapshot.availability(RuntimeCapability::RpcBackend),
            RuntimeCapabilityAvailability::Unsupported
        );
        assert_eq!(
            snapshot.availability(RuntimeCapability::EventStream),
            RuntimeCapabilityAvailability::Unknown
        );
    }

    #[tokio::test]
    async fn mock_fetch_page_returns_sample_pages_and_request_data() {
        let runtime = MockNetworkRuntime::default();
        let request_data = BTreeMap::from([("field_nickname".into(), "mesh friend".into())]);

        let page = runtime
            .fetch_page(
                "mock.node:/page/gallery.mu",
                Some(request_data.clone()),
                CancellationToken::new(),
            )
            .await
            .expect("fetch mock page");

        assert_eq!(page.title, "Micron Gallery");
        assert!(page.markup.contains("Saved nickname"));
        assert_eq!(page.request_data, Some(request_data));
    }

    #[tokio::test]
    async fn mock_download_writes_non_overwriting_file() {
        let runtime = MockNetworkRuntime::default();
        let dir = temp_dir("download");
        std::fs::write(dir.join("mock-download.txt"), b"old").expect("seed download");

        let downloaded = runtime
            .download_file("mock.node:/file", &dir, CancellationToken::new())
            .await
            .expect("download file");

        assert_eq!(
            downloaded.path.file_name().and_then(|name| name.to_str()),
            Some("mock-download-1.txt")
        );
        assert_eq!(downloaded.content_type, "text/plain");
    }

    #[tokio::test]
    async fn mock_messages_and_sends_are_stateful() {
        let runtime = MockNetworkRuntime::default();
        let before = runtime.list_messages().await.expect("list messages").len();

        let sent = runtime
            .send_message(MessageEnvelope {
                peer_hash: "0123456789abcdef".into(),
                title: "Hi".into(),
                body: "Body".into(),
                delivery_mode: DeliveryMode::Propagated,
                include_ticket: true,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            })
            .await
            .expect("send mock message");
        let messages = runtime.list_messages().await.expect("list messages");

        assert_eq!(sent.transport_method, TransportMethod::Propagated);
        assert_eq!(messages.len(), before + 1);
        assert!(!messages[0].incoming);
    }

    #[tokio::test]
    async fn mock_directory_path_and_propagation_behaviors_work() {
        let runtime = MockNetworkRuntime::default();
        let candidates = runtime
            .directory_candidates(None, true)
            .await
            .expect("directory candidates");

        assert_eq!(candidates.len(), 3);
        assert!(runtime
            .request_path("mock.node", "test", false)
            .await
            .expect("request path"));
        assert_eq!(
            runtime
                .warm_paths(
                    &[
                        "mock.node".into(),
                        "unknown".into(),
                        "fedcba9876543210".into()
                    ],
                    10,
                    0,
                )
                .await
                .expect("warm paths"),
            2
        );

        runtime
            .set_outbound_propagation_node(Some("fedcba9876543210".into()))
            .await
            .expect("set propagation");
        let status = runtime
            .propagation_status()
            .await
            .expect("propagation status");

        assert!(status.selected);
        assert!(status.has_path);
    }

    #[tokio::test]
    async fn mock_snapshot_and_inspection_are_deterministic() {
        let runtime = MockNetworkRuntime::default();
        let stats = runtime.interface_stats().await.expect("interface stats");
        let snapshot = runtime.network_snapshot().await.expect("network snapshot");
        let inspection = runtime
            .inspect_destination("0123456789abcdef", true)
            .await
            .expect("inspect destination");

        assert!(!stats.available);
        assert!(stats.transport_health.is_none());
        assert_eq!(snapshot.announce_counts.get("node"), Some(&1));
        assert!(inspection.has_path);
        assert_eq!(inspection.hops, Some(1));
    }

    #[test]
    fn lxmf_history_request_clamps_items_and_rejects_oversized_cursor() {
        let request =
            LxmfHistoryRequest::bounded(None, None, usize::MAX).expect("bounded history request");
        assert_eq!(request.limit, LXMF_HISTORY_PAGE_MAX_ITEMS);
        assert!(LxmfHistoryRequest::bounded(
            None,
            Some("x".repeat(LXMF_HISTORY_CURSOR_MAX_BYTES + 1)),
            1,
        )
        .is_err());
    }

    #[test]
    fn lxmf_history_page_rejects_item_and_byte_overload() {
        let record = LxmfHistoryRecord {
            message_id: "message".into(),
            source: "peer".into(),
            destination: "local".into(),
            title: String::new(),
            content: String::new(),
            timestamp: 1,
            direction: "in".into(),
            receipt_status: None,
        };
        assert!(LxmfHistoryPage {
            messages: vec![record.clone(); LXMF_HISTORY_PAGE_MAX_ITEMS + 1],
            next_cursor: None,
        }
        .validate()
        .is_err());
        assert!(LxmfHistoryPage {
            messages: vec![LxmfHistoryRecord {
                content: "x".repeat(LXMF_HISTORY_PAGE_MAX_BYTES),
                ..record
            }],
            next_cursor: None,
        }
        .validate()
        .is_err());
    }
}
