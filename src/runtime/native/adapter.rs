#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
#[cfg(feature = "native-lxmf")]
use std::path::PathBuf;
#[cfg(feature = "native-lxmf")]
use std::sync::LazyLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(all(feature = "native-rns-net", any()))]
use std::time::Instant;

use async_trait::async_trait;
#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
use rand_core::OsRng;
#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
use rns_transport::resource::ResourceEventKind;
#[cfg(feature = "native-lxmf")]
use sha2::{Digest, Sha256};
#[cfg(not(all(feature = "native-rns-net", any())))]
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{broadcast, watch, Semaphore};

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATED_TERMINAL_RETENTION_SECS: f64 = 3600.0;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATED_TRANSFER_TIMEOUT_SECS: f64 = 45.0;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATION_PATH_WAIT_ATTEMPTS: usize = 30;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATION_PATH_WAIT_STEP: Duration = Duration::from_millis(100);
#[cfg(all(feature = "native-rns-net", any()))]
const NATIVE_LXMF_DIRECT_PROOF_TIMEOUT_SECS: f64 = 45.0;
#[cfg(all(feature = "native-rns-net", any()))]
const NATIVE_LXMF_DIRECT_ROUTER_TICK_SECS: u64 = 5;
#[cfg(all(feature = "native-rns-net", any()))]
const NATIVE_KNOWN_DESTINATIONS_MAX_AGE_SECS: f64 = 6.0 * 60.0 * 60.0;
#[cfg(all(feature = "native-rns-net", any()))]
const NATIVE_KNOWN_DESTINATIONS_MAX_SAVED: usize = 4096;
#[cfg(all(feature = "native-rns-net", any()))]
const NATIVE_KNOWN_DESTINATIONS_SAVE_INTERVAL_SECS: u64 = 30;
#[cfg(not(all(feature = "native-rns-net", any())))]
const RETICULUM_PATH_TABLE_SAVE_INTERVAL_SECS: u64 = 30;
#[cfg(not(all(feature = "native-rns-net", any())))]
const CLEAN_DESTINATION_CACHE_MAX_ITEMS: usize = 256;
#[cfg(not(all(feature = "native-rns-net", any())))]
const CLEAN_DESTINATION_APP_DATA_MAX_ITEM_BYTES: usize = 4 * 1024;
#[cfg(not(all(feature = "native-rns-net", any())))]
const CLEAN_DESTINATION_APP_DATA_MAX_TOTAL_BYTES: usize = 256 * 1024;
#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
const CLEAN_DIRECT_POLICY_DISCOVERY_MAX_WAIT: Duration = Duration::from_secs(5);
#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
const CLEAN_PROPAGATION_SENDER_PATH_REQUEST_MAX: usize = 32;
const OMENCHAT_LINK_CONTEXT: u8 = 0x4f;
#[cfg(all(feature = "native-rns-net", any()))]
const OMENCHAT_LINK_PATH_WAIT_ATTEMPTS: usize = 40;
#[cfg(all(feature = "native-rns-net", any()))]
const OMENCHAT_LINK_PATH_WAIT_STEP: Duration = Duration::from_millis(250);
#[cfg(not(all(feature = "native-rns-net", any())))]
const OMENCHAT_CLEAN_LINK_PATH_WAIT_STEP: Duration = Duration::from_millis(250);
#[cfg(not(all(feature = "native-rns-net", any())))]
const OMENCHAT_CLEAN_LINK_GATE_STRIPES: usize = 32;
const OMENCHAT_RESOURCE_METADATA_PREFIX: &[u8] = b"omenchat-resource:";
#[cfg(not(all(feature = "native-rns-net", any())))]
const OMENCHAT_FRAME_RESOURCE_METADATA: &[u8] = b"omenchat-frame:";
#[cfg(not(all(feature = "native-rns-net", any())))]
const OMENCHAT_CLEAN_RESOURCE_MAX_BYTES: usize = 8 * 1024 * 1024;
#[cfg(feature = "native-lxmf")]
const PROPAGATION_STAMP_BLOCKING_JOBS: usize = 2;
#[cfg(feature = "native-lxmf")]
static PROPAGATION_STAMP_BLOCKING_GATE: Semaphore =
    Semaphore::const_new(PROPAGATION_STAMP_BLOCKING_JOBS);
#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
const DIRECT_STAMP_BLOCKING_JOBS: usize = 2;
#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
static DIRECT_STAMP_BLOCKING_GATE: Semaphore = Semaphore::const_new(DIRECT_STAMP_BLOCKING_JOBS);
#[cfg(feature = "native-lxmf")]
const NATIVE_LXMF_DECODE_BLOCKING_JOBS: usize = 2;
#[cfg(feature = "native-lxmf")]
static NATIVE_LXMF_DECODE_BLOCKING_GATE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(NATIVE_LXMF_DECODE_BLOCKING_JOBS)));
#[cfg(not(all(feature = "native-rns-net", any())))]
fn clean_omenchat_resource_limit(metadata: Option<&[u8]>) -> Option<usize> {
    let metadata = metadata?;
    if metadata.starts_with(OMENCHAT_FRAME_RESOURCE_METADATA) {
        Some(crate::protocol_limits::OMENCHAT_FRAME_MAX_BYTES)
    } else if metadata.starts_with(OMENCHAT_RESOURCE_METADATA_PREFIX) {
        Some(OMENCHAT_CLEAN_RESOURCE_MAX_BYTES)
    } else {
        None
    }
}
#[cfg(not(all(feature = "native-rns-net", any())))]
const OMENCHAT_RNS_APP_NAME: &str = "omenchat";
#[cfg(not(all(feature = "native-rns-net", any())))]
const OMENCHAT_NODE_ASPECT: &str = "node";
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_LINK_PACKET_MDU: usize = 431;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_OPPORTUNISTIC_PACKET_MDU: usize = rns_core::constants::ENCRYPTED_MDU;

use crate::browser::{BrowserPage, DownloadedFile};
use crate::directory::DirectoryKind;
use crate::error::{AppError, AppResult};
use crate::identity::IdentityProfile;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
use crate::messaging::DeliveryMode;
#[cfg(feature = "native-lxmf-sdk")]
use crate::messaging::TransportMethod;
use crate::messaging::{AttachmentSummary, MessageEnvelope, MessageSummary};
use crate::runtime::facade::{
    RuntimeCapability, RuntimeCapabilityAvailability, RuntimeCapabilityRecord,
    RuntimeCapabilitySnapshot, RuntimeCapabilitySource, RuntimeFailure, RuntimeFailureCategory,
    RuntimeLifecycleSnapshot, RuntimeLifecycleState,
};
#[cfg(all(feature = "native-rns-net", any()))]
use crate::runtime::native::announce::display_name_for_kind;
use crate::runtime::native::announce::{payload_from_announce_event, NativeAnnounceState};
#[cfg(all(feature = "native-rns-net", any()))]
use crate::runtime::native::identity::load_rns_net_proof_signing_key_file;
use crate::runtime::native::identity::{
    load_private_identity_file, load_transport_private_identity_file, NativeIdentitySummary,
};
use crate::runtime::native::interface::{
    plan_interfaces, validate_startup_plans, NativeInterfacePlan,
};
#[cfg(feature = "native-lxmf")]
use crate::runtime::native::lxmf_router::NativeDirectLxmfRouter;
#[cfg(all(feature = "native-rns-net", any()))]
use crate::runtime::native::lxmf_router::{
    DirectLxmfTimeoutEvent, NativePropagatedLxmfRouter, PropagatedNodeAccepted,
    PropagatedNodeFailed,
};
#[cfg(not(all(feature = "native-rns-net", any())))]
use crate::runtime::native::request::native_reticulum09_capability_report;
#[cfg(not(all(feature = "native-rns-net", any())))]
use crate::runtime::native::request::{
    send_reticulum_link_identify, single_output_destination_desc, NativeLinkRequestFrame,
    NativeLinkResponseFrame, NativePageFetchContext,
};
use crate::runtime::native::request::{
    NativeFetchPlan, NativePageTransportClient, ReticulumPageTransportClient,
};
#[cfg(all(feature = "native-rns-net", any()))]
use crate::runtime::native::rns_net::{
    RnsNetAnnounceKey, RnsNetDestinationKeyStore, RnsNetDestinationKeys, RnsNetLinkData,
    RnsNetLocalDelivery, RnsNetPageCallbacks, RnsNetPageRequestClient, RnsNetPathUpdate,
    RnsNetProof, RnsNetResourceEvent,
};
#[cfg(all(feature = "native-rns-net", any()))]
use crate::runtime::native::NativePageFetchFailureStage;
use crate::runtime::native::{NativeRuntimeConfig, NativeRuntimeError};
#[cfg(feature = "native-lxmf-sdk")]
use crate::runtime::native_lxmf::client::{
    build_sdk_send_plan, build_sdk_wire_delivery_from_envelope_with_issued_ticket,
    build_sdk_wire_delivery_from_envelope_with_policy, NativeLxmfSdkSender,
    NativeLxmfSdkSenderState, NativeLxmfSdkWireDelivery, NativeLxmfSdkWireSubmitter,
    RpcNativeLxmfSdkSender,
};
#[cfg(feature = "native-lxmf-sdk")]
use crate::runtime::native_lxmf::event_stream::{
    NativeLxmfSdkEventStreamSnapshot, NativeLxmfSdkEventStreamState, NativeLxmfSdkEventWorker,
};
#[cfg(feature = "native-lxmf-sdk")]
use crate::runtime::native_lxmf::tickets::{NativeLxmfTicketIssueState, NativeLxmfTicketIssuer};
use crate::runtime::network::{
    AnnouncePayload, CancellationToken, DestinationInspection, DirectoryCandidate, InterfaceSample,
    InterfaceSampleState, InterfaceStats, LxmfCancelOutcome, LxmfCorrelationRecovery,
    LxmfDeliveryProbeReport, LxmfDeliveryProbeStage, LxmfDeliveryProbeStep, LxmfHistoryPage,
    LxmfHistoryRequest, LxmfSdkRpcProbeSnapshot, NetworkRuntime, NetworkSnapshot, NetworkStatus,
    OmenChatLinkClosed, OmenChatLinkData, OmenChatLinkOpened, OmenChatResourceData,
    PageFetchProbeReport, PageFetchProbeStage, PageFetchProbeStep, PropagationDebugSnapshot,
    PropagationMessageSnapshot, PropagationStatus, ResourceLifecycleEvent, ResourceLifecycleState,
    ResourceProgressEvent, RuntimeBackendName,
};
#[cfg(feature = "native-lxmf")]
use crate::runtime::network::{
    LxmfDeliveryEvidence, LxmfDeliveryEvidenceKind, OutboundDeliveryState, OutboundStatus,
};
#[cfg(all(feature = "native-rns-net", any()))]
use crate::runtime::PathEvent;
use crate::runtime::RuntimeBusEvent;
#[cfg(feature = "native-lxmf")]
use crate::runtime::{PropagationSyncEvent, PropagationSyncEventStatus, PropagationSyncStage};
use crate::storage::files::{atomic_write_new_bounded, next_available_download_path};
#[cfg(feature = "native-lxmf")]
use crate::storage::transient_ids::{
    DeliveredTransientIdStore, LXMF_LOCAL_DELIVERY_CACHE_MAX_AGE_SECS,
};

#[derive(Clone)]
pub struct NativeNetworkRuntime {
    config: NativeRuntimeConfig,
    state: Arc<Mutex<NativeRuntimeState>>,
    lifecycle: Arc<Mutex<RuntimeLifecycleSnapshot>>,
    transport: Arc<Mutex<Option<NativeTransportHandle>>>,
    announces: Arc<Mutex<NativeAnnounceState>>,
    inbound_messages: Arc<Mutex<Vec<MessageSummary>>>,
    outbound_propagation_node: Arc<Mutex<Option<String>>>,
    identify_on_connect_destinations: Arc<Mutex<BTreeSet<String>>>,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    #[cfg(feature = "native-lxmf-sdk")]
    sdk_rpc_event_worker: Arc<NativeLxmfSdkEventWorker>,
    #[cfg(feature = "native-lxmf-sdk")]
    ticket_issuer: NativeLxmfTicketIssuer,
    page_transport: Arc<dyn NativePageTransportClient>,
    #[cfg(all(feature = "native-rns-net", any()))]
    rns_net: Arc<Mutex<Option<NativeRnsNetHandle>>>,
    #[cfg(feature = "native-lxmf")]
    pending_lxmf_proofs: PendingLxmfProofs,
    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    pending_direct_lxmf_resources: PendingDirectLxmfResources,
    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    pending_propagated_lxmf: PendingPropagatedLxmf,
    active_omenchat_links: Arc<Mutex<BTreeSet<[u8; 16]>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_omenchat_links: Arc<Mutex<BTreeMap<[u8; 16], CleanOmenChatLink>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_omenchat_link_coordinator: CleanOmenChatLinkCoordinator,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_recent_omenchat_announces: Arc<Mutex<BTreeMap<String, CleanOmenChatAnnounce>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_destination_app_data: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_local_lxmf: Arc<Mutex<CleanLocalLxmfState>>,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
#[derive(Clone)]
struct CleanOmenChatLink {
    destination_hash: rns_transport::hash::AddressHash,
    link_id: rns_transport::hash::AddressHash,
    transport: Arc<reticulum_rs::runtime::Transport>,
    link: Arc<AsyncMutex<rns_transport::destination::link::Link>>,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
#[derive(Clone, Debug)]
struct CleanOmenChatLinkCoordinator {
    gates: Arc<[AsyncMutex<()>; OMENCHAT_CLEAN_LINK_GATE_STRIPES]>,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
impl Default for CleanOmenChatLinkCoordinator {
    fn default() -> Self {
        Self {
            gates: Arc::new(std::array::from_fn(|_| AsyncMutex::new(()))),
        }
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
impl CleanOmenChatLinkCoordinator {
    fn stripe(destination: &rns_transport::hash::AddressHash) -> usize {
        usize::from(destination.as_slice()[0]) % OMENCHAT_CLEAN_LINK_GATE_STRIPES
    }

    async fn lock<'a>(
        &'a self,
        destination: &rns_transport::hash::AddressHash,
        cancel: &CancellationToken,
    ) -> AppResult<tokio::sync::MutexGuard<'a, ()>> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let gate = &self.gates[Self::stripe(destination)];
        tokio::select! {
            guard = gate.lock() => Ok(guard),
            _ = cancel.cancelled() => Err(AppError::from(NativeRuntimeError::Cancelled)),
        }
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
#[derive(Clone, Copy, Debug)]
struct CleanOmenChatAnnounce {
    observed_at: tokio::time::Instant,
    hops: u8,
    iface: rns_transport::hash::AddressHash,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
#[derive(Clone, Debug, Default)]
struct CleanLocalLxmfState {
    announced: bool,
    destination_hash: Option<String>,
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
struct CleanLxmfPropagationLink {
    destination_hash: rns_transport::hash::AddressHash,
    link: Arc<AsyncMutex<rns_transport::destination::link::Link>>,
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
#[derive(Clone)]
struct CleanReticulumLxmfWireSubmitter {
    transport: Arc<reticulum_rs::runtime::Transport>,
    storage_path: std::path::PathBuf,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    outbound_propagation_node: Arc<Mutex<Option<String>>>,
    destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
    destination_app_data: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    pending_lxmf_proofs: PendingLxmfProofs,
    propagation_link: Arc<AsyncMutex<Option<CleanLxmfPropagationLink>>>,
    propagation_send_gate: Arc<AsyncMutex<()>>,
    timeout: Duration,
    runtime: tokio::runtime::Handle,
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
struct CleanLxmfSubmitterState {
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    outbound_propagation_node: Arc<Mutex<Option<String>>>,
    destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
    destination_app_data: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    pending_lxmf_proofs: PendingLxmfProofs,
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CleanLxmfSubmitOutcome {
    route: &'static str,
    receipt_hash: Option<String>,
    resource_hash: Option<String>,
    propagation_stamp: Option<crate::runtime::native_lxmf::codec::GeneratedPropagationStamp>,
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
struct CleanLxmfReceiptHandler {
    pending_lxmf_proofs: PendingLxmfProofs,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
impl rns_transport::transport::ReceiptHandler for CleanLxmfReceiptHandler {
    fn on_receipt(&self, receipt: &rns_transport::transport::DeliveryReceipt) {
        let receipt_hash = hex_encode(&receipt.message_id);
        let (status, matched_pending, first_observation) = self
            .pending_lxmf_proofs
            .lock()
            .expect("native LXMF proof map lock")
            .receipt_status_for_packet(receipt_hash.clone(), String::new(), 0.0);
        if !matched_pending {
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 receipt ignored receipt_hash={receipt_hash} reason=no_pending_correlation"
            )));
            return;
        }
        if !first_observation {
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 duplicate receipt ignored receipt_hash={receipt_hash} reason=already_observed"
            )));
            return;
        }
        let peer_hash = status.peer_hash.clone();
        let message_id = status.message_id.clone();
        let _ = self
            .event_tx
            .send(RuntimeBusEvent::MessageDeliveryUpdated(status));
        let _ = self.event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
            LxmfDeliveryEvidence {
                peer_hash: peer_hash.clone(),
                message_id: message_id.clone(),
                kind: LxmfDeliveryEvidenceKind::RnsPacketProof,
                detail: Some(format!(
                    "receipt_hash:{receipt_hash};matched_pending:true;source:reticulum_rs_0_9_receipt_handler"
                )),
                rtt: None,
                observed_at: Some(native_unix_timestamp()),
            },
        ));
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 receipt correlated peer={} message_id={} receipt_hash={} delivery=peer_unconfirmed",
            peer_hash,
            message_id.as_deref().unwrap_or("none"),
            receipt_hash
        )));
    }
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
fn clean_lxmf_submission_terminal_flags(accepted: bool) -> (bool, bool) {
    (false, !accepted)
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
fn clean_direct_stamp_cost(
    app_data: Option<&[u8]>,
    reply_ticket: Option<&crate::messaging::NativeLxmfReplyTicket>,
    now: f64,
) -> AppResult<Option<u8>> {
    let valid_reply_ticket = match reply_ticket {
        Some(ticket)
            if ticket.ticket.len() == crate::runtime::native_lxmf::tickets::LXMF_TICKET_BYTES
                && ticket.expires.is_finite()
                && ticket.expires > now =>
        {
            true
        }
        Some(_) => {
            return Err(AppError::Runtime(
                "LXMF reply ticket is expired or invalid".into(),
            ));
        }
        None => false,
    };
    match crate::runtime::native_lxmf::codec::delivery_announce_direct_stamp_policy(
        app_data,
        valid_reply_ticket,
    ) {
        crate::runtime::native_lxmf::codec::DirectStampPolicy::Required { cost }
            if cost <= crate::runtime::native_lxmf::codec::CLEAN_DIRECT_STAMP_MAX_COST =>
        {
            Ok(Some(cost))
        }
        crate::runtime::native_lxmf::codec::DirectStampPolicy::Required { cost } => {
            Err(AppError::Unsupported(format!(
                "LXMF peer requires direct stamp cost {cost}, above the automatic safety ceiling {}",
                crate::runtime::native_lxmf::codec::CLEAN_DIRECT_STAMP_MAX_COST
            )))
        }
        crate::runtime::native_lxmf::codec::DirectStampPolicy::Unsupported => {
            Err(AppError::Unsupported(
                "LXMF peer announced an unsupported direct stamp policy".into(),
            ))
        }
        crate::runtime::native_lxmf::codec::DirectStampPolicy::Unknown
        | crate::runtime::native_lxmf::codec::DirectStampPolicy::NotRequired
        | crate::runtime::native_lxmf::codec::DirectStampPolicy::TicketAccepted => Ok(None),
    }
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
async fn clean_wait_for_direct_policy_announce(
    cache: &Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    destination: &str,
    events: &mut broadcast::Receiver<RuntimeBusEvent>,
    timeout: Duration,
    shutdown: &tokio_util::sync::CancellationToken,
) -> AppResult<Option<Vec<u8>>> {
    if let Some(app_data) = cache
        .lock()
        .expect("native clean destination app-data cache lock")
        .get(destination)
        .cloned()
    {
        return Ok(Some(app_data));
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let received = tokio::select! {
            _ = shutdown.cancelled() => {
                return Err(AppError::Runtime(
                    "LXMF direct stamp policy discovery cancelled during shutdown".into(),
                ));
            }
            _ = tokio::time::sleep_until(deadline) => return Ok(None),
            received = events.recv() => received,
        };
        match received {
            Ok(RuntimeBusEvent::Announce(payload))
                if payload.destination_hash.eq_ignore_ascii_case(destination) =>
            {
                return cache
                    .lock()
                    .expect("native clean destination app-data cache lock")
                    .get(destination)
                    .cloned()
                    .map(Some)
                    .ok_or_else(|| {
                        AppError::Unsupported(
                            "LXMF peer announce policy exceeded admission limits".into(),
                        )
                    });
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(_)) => {
                if let Some(app_data) = cache
                    .lock()
                    .expect("native clean destination app-data cache lock")
                    .get(destination)
                    .cloned()
                {
                    return Ok(Some(app_data));
                }
            }
            Err(broadcast::error::RecvError::Closed) => return Ok(None),
        }
    }
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
fn clean_lxmf_direct_payload(delivery: &NativeLxmfSdkWireDelivery) -> Cow<'_, [u8]> {
    Cow::Borrowed(delivery.wire_bytes.as_slice())
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
impl CleanReticulumLxmfWireSubmitter {
    fn new(
        transport: Arc<reticulum_rs::runtime::Transport>,
        storage_path: std::path::PathBuf,
        state: CleanLxmfSubmitterState,
        timeout: Duration,
    ) -> AppResult<Self> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|error| {
            AppError::Runtime(format!(
                "native LXMF clean submitter needs an active Tokio runtime: {error}"
            ))
        })?;
        Ok(Self {
            transport,
            storage_path,
            event_tx: state.event_tx,
            outbound_propagation_node: state.outbound_propagation_node,
            destination_identities: state.destination_identities,
            destination_app_data: state.destination_app_data,
            pending_lxmf_proofs: state.pending_lxmf_proofs,
            propagation_link: Arc::new(AsyncMutex::new(None)),
            propagation_send_gate: Arc::new(AsyncMutex::new(())),
            timeout,
            runtime,
        })
    }

    async fn submit_wire_async(
        &self,
        delivery: &NativeLxmfSdkWireDelivery,
    ) -> AppResult<CleanLxmfSubmitOutcome> {
        let method = delivery.method.as_deref().unwrap_or("direct");
        if !matches!(method, "direct" | "propagated") {
            return Err(AppError::Unsupported(format!(
                "clean LXMF {method} submit is not supported by the embedded Reticulum sender"
            )));
        }
        let _propagation_send_guard = if method == "propagated" {
            Some(self.propagation_send_gate.lock().await)
        } else {
            None
        };
        let destination_hash = parse_transport_destination_hash(&delivery.destination_hash)?;
        let cancel = CancellationToken::new();
        let identity = clean_wait_for_destination_identity(
            &self.transport,
            &self.storage_path,
            destination_hash,
            self.timeout,
            cancel.clone(),
            Some(&self.event_tx),
            Some(&self.destination_identities),
        )
        .await?;
        let (
            send_hash,
            send_identity,
            send_payload,
            send_aspect,
            route_label,
            transient_id,
            propagation_stamp,
        ) = if method == "propagated" {
            let propagation_node = self
                .outbound_propagation_node
                .lock()
                .expect("native propagation node lock")
                .clone()
                .ok_or_else(|| {
                    AppError::Unsupported(
                        "clean LXMF propagated send needs a selected propagation node".into(),
                    )
                })?;
            let propagation_hash = parse_transport_destination_hash(&propagation_node)?;
            let propagation_cancel = CancellationToken::new();
            let propagation_identity = clean_wait_for_destination_identity(
                &self.transport,
                &self.storage_path,
                propagation_hash,
                self.timeout,
                propagation_cancel.clone(),
                Some(&self.event_tx),
                Some(&self.destination_identities),
            )
            .await?;
            clean_wait_for_destination_path(
                &self.transport,
                propagation_hash,
                self.timeout,
                propagation_cancel,
                Some(&self.event_tx),
                None,
            )
            .await?;
            let wire = lxmf::WireMessage::unpack(delivery.wire_bytes.as_slice()).map_err(
                    |error| {
                        AppError::Runtime(format!(
                            "clean LXMF propagated wire decode failed destination={} message_id={}: {error}",
                            delivery.destination_hash, delivery.message_id
                        ))
                    },
                )?;
            let lxmf_identity = reticulum_rs::core::identity::Identity::new_from_slices(
                identity.public_key_bytes(),
                identity.verifying_key_bytes(),
            );
            let propagation_app_data = clean_wait_for_destination_app_data(
                &self.destination_app_data,
                propagation_hash,
                self.timeout.min(Duration::from_secs(5)),
                Some(&self.event_tx),
            )
            .await;
            let advertised_target_stamp_cost = propagation_app_data.as_deref().and_then(
                crate::runtime::native_lxmf::codec::propagation_announce_target_stamp_cost,
            );
            let default_stamp_cost =
                crate::runtime::native_lxmf::codec::DEFAULT_PROPAGATION_STAMP_TARGET_COST;
            let (target_stamp_cost, target_stamp_source) = advertised_target_stamp_cost
                .map(|cost| (Some(cost), "advertised"))
                .unwrap_or((Some(default_stamp_cost), "default_missing_app_data"));
            let (mut lxm_data, transient_id) = wire
                    .pack_propagation_transient_with_rng(&lxmf_identity, OsRng)
                    .map_err(|error| {
                        AppError::Runtime(format!(
                            "clean LXMF propagation transient encode failed destination={} message_id={}: {error}",
                            delivery.destination_hash, delivery.message_id
                        ))
                    })?;
            let stamp = if let Some(target_cost) = target_stamp_cost {
                let transient_id_array: [u8; 32] = transient_id;
                let permit = PROPAGATION_STAMP_BLOCKING_GATE
                    .acquire()
                    .await
                    .map_err(|_| {
                        AppError::Runtime("propagation stamp blocking gate closed".into())
                    })?;
                let (stamp, returned_lxm_data) = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    generate_propagation_stamp_owned(lxm_data, transient_id_array, target_cost)
                })
                .await
                .map_err(|error| {
                    AppError::Runtime(format!("clean LXMF propagation stamp task failed: {error}"))
                })?;
                lxm_data = returned_lxm_data;
                let stamp = stamp?;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "clean LXMF propagation stamp generated destination={} propagation_node={} transient_id={} target_cost={} source={} stamp_value={} attempts={}",
                        delivery.destination_hash,
                        propagation_hash.to_hex_string(),
                        hex_encode(&stamp.transient_id),
                        stamp.target_cost,
                        target_stamp_source,
                        stamp.stamp_value,
                        stamp.attempts
                    )));
                Some(stamp)
            } else {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "clean LXMF propagation stamp skipped destination={} propagation_node={} reason=no_advertised_target_cost",
                        delivery.destination_hash,
                        propagation_hash.to_hex_string()
                    )));
                None
            };
            let envelope = lxmf::WireMessage::pack_propagation_envelope(
                    wire.payload.timestamp,
                    &lxm_data,
                    stamp.as_ref().map(|stamp| stamp.stamp.as_slice()),
                )
                .map_err(|error| {
                    AppError::Runtime(format!(
                        "clean LXMF propagation envelope encode failed destination={} message_id={}: {error}",
                        delivery.destination_hash, delivery.message_id
                    ))
                })?;
            (
                propagation_hash,
                propagation_identity,
                Cow::Owned(envelope),
                "propagation",
                "propagation-envelope",
                Some(hex_encode(&transient_id)),
                stamp,
            )
        } else {
            (
                destination_hash,
                identity,
                clean_lxmf_direct_payload(delivery),
                "delivery",
                "direct-wire",
                None,
                None,
            )
        };
        let destination =
            single_output_destination_desc(send_hash, send_identity, "lxmf", send_aspect)?;
        clean_wait_for_destination_path(
            &self.transport,
            send_hash,
            self.timeout,
            cancel,
            Some(&self.event_tx),
            None,
        )
        .await?;
        let link = if method == "propagated" {
            let mut cached = self.propagation_link.lock().await;
            let reusable = if let Some(cached_link) = cached.as_ref() {
                let active = cached_link.link.lock().await.status()
                    == rns_transport::destination::link::LinkStatus::Active;
                (cached_link.destination_hash == send_hash && active)
                    .then(|| cached_link.link.clone())
            } else {
                None
            };
            if let Some(link) = reusable {
                link
            } else {
                if let Some(old_link) = cached.take() {
                    clean_close_link(&self.transport, &old_link.link).await;
                }
                let link = self.transport.link(destination).await;
                rns_transport::delivery::await_link_activation(
                    &self.transport,
                    &link,
                    self.timeout,
                )
                .await
                .map_err(|error| {
                    AppError::Runtime(format!(
                        "clean LXMF {method} link activation failed destination={} send_destination={} message_id={}: {error}",
                        delivery.destination_hash,
                        send_hash.to_hex_string(),
                        delivery.message_id
                    ))
                })?;
                *cached = Some(CleanLxmfPropagationLink {
                    destination_hash: send_hash,
                    link: link.clone(),
                });
                link
            }
        } else {
            let link = self.transport.link(destination).await;
            rns_transport::delivery::await_link_activation(&self.transport, &link, self.timeout)
                .await
                .map_err(|error| {
                    AppError::Runtime(format!(
                        "clean LXMF {method} link activation failed destination={} send_destination={} message_id={}: {error}",
                        delivery.destination_hash,
                        send_hash.to_hex_string(),
                        delivery.message_id
                    ))
                })?;
            link
        };
        let mut receipt_hash = None;
        let mut resource_hash = None;
        let result = rns_transport::delivery::send_on_link_observed(
            &self.transport,
            &link,
            send_payload.as_ref(),
            |packet| {
                let packet_hash = hex_encode(packet.hash().as_slice());
                self.pending_lxmf_proofs
                    .lock()
                    .expect("native LXMF proof map lock")
                    .insert_correlated_submission(
                        packet_hash.clone(),
                        delivery.message_id.clone(),
                        delivery.destination_hash.clone(),
                        native_unix_timestamp(),
                        None,
                    );
                receipt_hash = Some(packet_hash);
            },
            |hash| {
                let hash = hex_encode(hash.as_slice());
                let submitted_at = native_unix_timestamp();
                self.pending_lxmf_proofs
                    .lock()
                    .expect("native LXMF resource map lock")
                    .insert_correlated_submission(
                        hash.clone(),
                        delivery.message_id.clone(),
                        delivery.destination_hash.clone(),
                        submitted_at,
                        None,
                    );
                emit_clean_lxmf_resource_offered(
                    &self.event_tx,
                    &hash,
                    &delivery.message_id,
                    &delivery.destination_hash,
                    send_payload.len() as u64,
                );
                resource_hash = Some(hash);
            },
        )
        .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if method == "propagated" {
                    let mut cached = self.propagation_link.lock().await;
                    if cached
                        .as_ref()
                        .is_some_and(|cached_link| Arc::ptr_eq(&cached_link.link, &link))
                    {
                        cached.take();
                    }
                    clean_close_link(&self.transport, &link).await;
                }
                if let Some(receipt_hash) = receipt_hash.as_deref() {
                    self.pending_lxmf_proofs
                        .lock()
                        .expect("native LXMF proof map lock")
                        .remove_correlation(receipt_hash);
                }
                if let Some(resource_hash) = resource_hash.as_deref() {
                    self.pending_lxmf_proofs
                        .lock()
                        .expect("native LXMF resource map lock")
                        .remove_correlation(resource_hash);
                }
                return Err(AppError::Runtime(format!(
                "clean LXMF {method} submit failed destination={} send_destination={} message_id={}: {error}",
                delivery.destination_hash,
                send_hash.to_hex_string(),
                delivery.message_id
                )));
            }
        };
        let route = match result {
            rns_transport::delivery::LinkSendResult::Packet(_) => "link-packet",
            rns_transport::delivery::LinkSendResult::Resource(_) => "link-resource",
        };
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 LXMF clean wire submitted method={} destination={} send_destination={} message_id={} route={} route_label={} bytes={} ticket={} reply_ticket={} transient_id={}",
            method,
            delivery.destination_hash,
            send_hash.to_hex_string(),
            delivery.message_id,
            route,
            route_label,
            send_payload.len(),
            delivery.include_ticket,
            delivery.reply_ticket_used,
            transient_id.unwrap_or_else(|| "-".into())
        )));
        Ok(CleanLxmfSubmitOutcome {
            route,
            receipt_hash,
            resource_hash,
            propagation_stamp,
        })
    }
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
impl NativeLxmfSdkWireSubmitter for CleanReticulumLxmfWireSubmitter {
    fn submit_wire(&self, delivery: &NativeLxmfSdkWireDelivery) -> std::io::Result<()> {
        self.runtime
            .block_on(self.submit_wire_async(delivery))
            .map(|_| ())
            .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRuntimeLifecycle {
    Stopped,
    Running,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRuntimeState {
    pub lifecycle: NativeRuntimeLifecycle,
    pub active_identity: Option<NativeIdentitySummary>,
    pub active_identity_profile: Option<IdentityProfile>,
    pub interfaces: Vec<NativeInterfacePlan>,
    pub transport_started: bool,
    #[cfg(all(feature = "native-rns-net", any()))]
    pub rns_net_started: bool,
}

#[derive(Clone)]
struct NativeTransportHandle {
    transport: Arc<reticulum_rs::runtime::Transport>,
    shutdown: tokio_util::sync::CancellationToken,
    interface_count: usize,
    attached_interfaces: Vec<String>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_attached_interfaces: Vec<CleanAttachedInterface>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    storage_path: std::path::PathBuf,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    path_restore_ready: watch::Receiver<bool>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_lxmf_delivery_destination:
        Arc<AsyncMutex<rns_transport::destination::SingleInputDestination>>,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
#[derive(Clone, Debug)]
struct CleanAttachedInterface {
    name: String,
    address: rns_transport::hash::AddressHash,
    ifac_configured: bool,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
impl CleanAttachedInterface {
    fn label(&self, endpoint: &str, ifac_status: &str) -> String {
        format!(
            "{} tcp_client {endpoint} ifac={} iface={}",
            self.name,
            ifac_status,
            self.address.to_hex_string()
        )
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
#[derive(Clone, Debug)]
struct CleanAttachedInterfaceRecord {
    label: String,
    clean: CleanAttachedInterface,
}

#[cfg(all(feature = "native-rns-net", any()))]
#[derive(Clone)]
struct NativeRnsNetHandle {
    client: RnsNetPageRequestClient,
    destination_keys: Arc<Mutex<RnsNetDestinationKeyStore>>,
    local_identity_private_key: Option<[u8; 64]>,
    local_lxmf_delivery_registered: bool,
    local_lxmf_delivery_link_registered: bool,
    local_lxmf_delivery_proof_capable: bool,
    local_lxmf_delivery_announced: bool,
    local_lxmf_delivery_destination_hash: Option<String>,
}

#[cfg(feature = "native-lxmf")]
type PendingLxmfProofs = Arc<Mutex<NativeDirectLxmfRouter>>;

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
type PendingDirectLxmfResources = Arc<Mutex<BTreeMap<String, PendingNativeDirectLxmfResource>>>;

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
#[derive(Clone, Debug, PartialEq)]
struct PendingNativeDirectLxmfResource {
    peer_hash: String,
    message_id: String,
    submitted_at: f64,
    transfer_state: String,
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
type PendingPropagatedLxmf = Arc<Mutex<BTreeMap<String, PendingNativePropagatedLxmf>>>;

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
#[derive(Clone, Debug, PartialEq)]
struct PendingNativePropagatedLxmf {
    peer_hash: String,
    propagation_node: String,
    submitted_at: f64,
    has_path: bool,
    known_app_data: bool,
    link_id: Option<String>,
    transfer_state: String,
    peer_activity_observed_at: Option<f64>,
    terminal_at: Option<f64>,
}

impl NativeNetworkRuntime {
    pub fn new(config: NativeRuntimeConfig) -> Self {
        #[cfg(feature = "native-lxmf-sdk")]
        let ticket_issuer = NativeLxmfTicketIssuer::new(&config.reticulum_storage_dir);
        Self {
            config,
            state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            lifecycle: Arc::new(Mutex::new(RuntimeLifecycleSnapshot::new(
                RuntimeLifecycleState::New,
                RuntimeBackendName::Reticulum,
            ))),
            transport: Arc::new(Mutex::new(None)),
            announces: Arc::new(Mutex::new(NativeAnnounceState::default())),
            inbound_messages: Arc::new(Mutex::new(Vec::new())),
            outbound_propagation_node: Arc::new(Mutex::new(None)),
            identify_on_connect_destinations: Arc::new(Mutex::new(BTreeSet::new())),
            event_tx: broadcast::channel(256).0,
            #[cfg(feature = "native-lxmf-sdk")]
            sdk_rpc_event_worker: Arc::new(NativeLxmfSdkEventWorker::default()),
            #[cfg(feature = "native-lxmf-sdk")]
            ticket_issuer,
            page_transport: Arc::new(ReticulumPageTransportClient::default()),
            #[cfg(all(feature = "native-rns-net", any()))]
            rns_net: Arc::new(Mutex::new(None)),
            #[cfg(feature = "native-lxmf")]
            pending_lxmf_proofs: Arc::new(Mutex::new(NativeDirectLxmfRouter::default())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_direct_lxmf_resources: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_propagated_lxmf: Arc::new(Mutex::new(BTreeMap::new())),
            active_omenchat_links: Arc::new(Mutex::new(BTreeSet::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_omenchat_links: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_omenchat_link_coordinator: CleanOmenChatLinkCoordinator::default(),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_recent_omenchat_announces: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_destination_identities: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_destination_app_data: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_local_lxmf: Arc::new(Mutex::new(CleanLocalLxmfState::default())),
        }
    }

    #[cfg(test)]
    fn with_page_transport(
        config: NativeRuntimeConfig,
        page_transport: Arc<dyn NativePageTransportClient>,
    ) -> Self {
        #[cfg(feature = "native-lxmf-sdk")]
        let ticket_issuer = NativeLxmfTicketIssuer::new(&config.reticulum_storage_dir);
        Self {
            config,
            state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            lifecycle: Arc::new(Mutex::new(RuntimeLifecycleSnapshot::new(
                RuntimeLifecycleState::New,
                RuntimeBackendName::Reticulum,
            ))),
            transport: Arc::new(Mutex::new(None)),
            announces: Arc::new(Mutex::new(NativeAnnounceState::default())),
            inbound_messages: Arc::new(Mutex::new(Vec::new())),
            outbound_propagation_node: Arc::new(Mutex::new(None)),
            identify_on_connect_destinations: Arc::new(Mutex::new(BTreeSet::new())),
            event_tx: broadcast::channel(256).0,
            #[cfg(feature = "native-lxmf-sdk")]
            sdk_rpc_event_worker: Arc::new(NativeLxmfSdkEventWorker::default()),
            #[cfg(feature = "native-lxmf-sdk")]
            ticket_issuer,
            page_transport,
            #[cfg(all(feature = "native-rns-net", any()))]
            rns_net: Arc::new(Mutex::new(None)),
            #[cfg(feature = "native-lxmf")]
            pending_lxmf_proofs: Arc::new(Mutex::new(NativeDirectLxmfRouter::default())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_direct_lxmf_resources: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_propagated_lxmf: Arc::new(Mutex::new(BTreeMap::new())),
            active_omenchat_links: Arc::new(Mutex::new(BTreeSet::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_omenchat_links: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_omenchat_link_coordinator: CleanOmenChatLinkCoordinator::default(),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_recent_omenchat_announces: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_destination_identities: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_destination_app_data: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_local_lxmf: Arc::new(Mutex::new(CleanLocalLxmfState::default())),
        }
    }

    pub fn config(&self) -> &NativeRuntimeConfig {
        &self.config
    }

    pub fn start(
        &self,
        identity: Option<IdentityProfile>,
        interfaces: Vec<NativeInterfacePlan>,
    ) -> AppResult<()> {
        if matches!(
            self.config.instance_mode,
            crate::runtime::native::config::NativeRuntimeMode::External
        ) {
            return Err(AppError::Unsupported(
                "external/shared Reticulum mode is configured but no live shared-instance backend has been negotiated; integrated interface startup is disabled"
                    .into(),
            ));
        }
        let ifac_summary = interfaces
            .iter()
            .filter(|interface| interface.enabled && interface.ifac_configured)
            .map(|interface| {
                format!(
                    "{}:{}",
                    interface.name,
                    interface.ifac_network_name.as_deref().unwrap_or("unnamed")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum startup requested identity={} configured_interfaces={} ifac_interfaces=[{}] announce_on_start={}",
            identity
                .as_ref()
                .map(|profile| profile.label.as_str())
                .unwrap_or("none"),
            interfaces.len(),
            if ifac_summary.is_empty() {
                "none"
            } else {
                ifac_summary.as_str()
            },
            self.config.announce_on_start
        )));
        if !ifac_summary.is_empty() {
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(
                "native Reticulum 0.9 IFAC TCP adapter active for configured private gateways"
                    .into(),
            ));
        }
        validate_startup_plans(&interfaces).map_err(AppError::from)?;
        let identity_path = identity
            .as_ref()
            .map(|profile| profile.path.clone())
            .or_else(|| self.config.identity_path.clone());
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum identity material path={}",
            identity_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".into())
        )));
        let active_identity = identity_path
            .as_ref()
            .map(|path| load_private_identity_file(path))
            .transpose()
            .map_err(AppError::from)?;
        #[cfg(all(feature = "native-rns-net", any()))]
        let transport = None;
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let transport = identity_path
            .as_ref()
            .map(|path| self.build_transport(path, &interfaces))
            .transpose()?;
        #[cfg(all(feature = "native-rns-net", any()))]
        let rns_net = self.build_rns_net_request_handle(
            active_identity.as_ref(),
            identity.as_ref(),
            identity_path.as_deref(),
        )?;

        let mut state = self.state.lock().expect("native runtime state lock");
        state.lifecycle = NativeRuntimeLifecycle::Running;
        state.active_identity = active_identity;
        state.active_identity_profile = identity;
        state.interfaces = interfaces;
        state.transport_started = transport.is_some();
        self.replace_transport(transport);
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let destination_hash = identity_path.as_deref().and_then(|path| {
                clean_lxmf_delivery_destination_hash_from_identity_path(path).ok()
            });
            *self
                .clean_local_lxmf
                .lock()
                .expect("native clean local lxmf lock") = CleanLocalLxmfState {
                announced: false,
                destination_hash,
            };
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            state.rns_net_started = rns_net.is_some();
            *self.rns_net.lock().expect("native rns-net lock") = rns_net;
        }
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum runtime running transport_started={}{}",
            state.transport_started,
            {
                #[cfg(all(feature = "native-rns-net", any()))]
                {
                    format!(" rns_net_started={}", state.rns_net_started)
                }
                #[cfg(not(all(feature = "native-rns-net", any())))]
                {
                    String::new()
                }
            }
        )));
        Ok(())
    }

    pub fn stop(&self) {
        #[cfg(feature = "native-lxmf-sdk")]
        self.sdk_rpc_event_worker.cancel();
        let mut state = self.state.lock().expect("native runtime state lock");
        state.lifecycle = NativeRuntimeLifecycle::Stopped;
        state.transport_started = false;
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            state.rns_net_started = false;
        }
        self.replace_transport(None);
        self.active_omenchat_links
            .lock()
            .expect("native active OMENchat link lock")
            .clear();
        #[cfg(not(all(feature = "native-rns-net", any())))]
        self.clean_omenchat_links
            .lock()
            .expect("native clean OMENchat link lock")
            .clear();
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            *self
                .clean_local_lxmf
                .lock()
                .expect("native clean local lxmf lock") = CleanLocalLxmfState::default();
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            *self.rns_net.lock().expect("native rns-net lock") = None;
        }
    }

    pub fn state_snapshot(&self) -> NativeRuntimeState {
        self.state
            .lock()
            .expect("native runtime state lock")
            .clone()
    }

    fn set_lifecycle_snapshot(&self, snapshot: RuntimeLifecycleSnapshot) {
        *self.lifecycle.lock().expect("native lifecycle lock") = snapshot;
    }

    fn lifecycle_snapshot_sync(&self) -> RuntimeLifecycleSnapshot {
        self.lifecycle
            .lock()
            .expect("native lifecycle lock")
            .clone()
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    fn clean_stack_identify_identity(
        &self,
        identify_on_connect: bool,
    ) -> Option<Arc<rns_transport::identity::PrivateIdentity>> {
        if !identify_on_connect {
            return None;
        }
        let identity_path = self
            .state_snapshot()
            .active_identity_profile
            .as_ref()
            .map(|profile| profile.path.clone())
            .or_else(|| self.config.identity_path.clone())?;
        match load_transport_private_identity_file(&identity_path) {
            Ok(identity) => Some(Arc::new(identity)),
            Err(error) => {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum clean-stack identify-on-connect skipped; active identity could not be loaded from {}: {:?}",
                    identity_path.display(),
                    error
                )));
                None
            }
        }
    }

    fn native_lxmf_sdk_rpc_status_summary(&self) -> String {
        #[cfg(feature = "native-lxmf-sdk")]
        {
            let sender = RpcNativeLxmfSdkSender::new(
                self.config
                    .native_lxmf_sdk_rpc_endpoint
                    .clone()
                    .unwrap_or_default(),
            );
            let status = sender.status();
            let state = match status.state {
                NativeLxmfSdkSenderState::Ready => "ready",
                NativeLxmfSdkSenderState::Configured => "configured_unprobed",
                NativeLxmfSdkSenderState::MissingEndpoint => "missing_endpoint",
                NativeLxmfSdkSenderState::RejectedEndpoint => "rejected_endpoint",
                NativeLxmfSdkSenderState::NotWired => "not_wired",
            };
            format!("native_lxmf_sdk_rpc={state} note=\"{}\"", status.note)
        }
        #[cfg(not(feature = "native-lxmf-sdk"))]
        {
            "native_lxmf_sdk_rpc=disabled".into()
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    fn clean_local_lxmf_status_summary(&self, identity_path: Option<&Path>) -> String {
        let state = self
            .clean_local_lxmf
            .lock()
            .expect("native clean local lxmf lock")
            .clone();
        let destination_hash = state.destination_hash.or_else(|| {
            identity_path
                .and_then(|path| clean_lxmf_delivery_destination_hash_from_identity_path(path).ok())
        });
        let registered = destination_hash.is_some();
        format!(
            "local_lxmf_registered={registered} link_registered={registered} proof_capable={registered} announced={} local_lxmf_destination={}",
            state.announced && registered,
            destination_hash.as_deref().unwrap_or("none")
        )
    }

    fn set_failed(&self, message: impl Into<String>) {
        let message = message.into();
        self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::failed(
            RuntimeBackendName::Reticulum,
            RuntimeFailure {
                category: RuntimeFailureCategory::Identity,
                summary: message.clone(),
                technical_detail: None,
                retryable: false,
            },
        ));
        let mut state = self.state.lock().expect("native runtime state lock");
        state.lifecycle = NativeRuntimeLifecycle::Failed(message);
        state.transport_started = false;
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            state.rns_net_started = false;
        }
        self.replace_transport(None);
        self.active_omenchat_links
            .lock()
            .expect("native active OMENchat link lock")
            .clear();
        #[cfg(not(all(feature = "native-rns-net", any())))]
        self.clean_omenchat_links
            .lock()
            .expect("native clean OMENchat link lock")
            .clear();
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            *self
                .clean_local_lxmf
                .lock()
                .expect("native clean local lxmf lock") = CleanLocalLxmfState::default();
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            *self.rns_net.lock().expect("native rns-net lock") = None;
        }
    }

    fn active_transport(&self) -> AppResult<NativeTransportHandle> {
        self.transport
            .lock()
            .expect("native transport lock")
            .clone()
            .ok_or_else(|| AppError::Runtime("native Reticulum transport is not started".into()))
    }

    fn replace_transport(&self, replacement: Option<NativeTransportHandle>) {
        let previous = std::mem::replace(
            &mut *self.transport.lock().expect("native transport lock"),
            replacement,
        );
        if let Some(previous) = previous {
            previous.shutdown.cancel();
        }
    }

    fn build_transport(
        &self,
        identity_path: &Path,
        interfaces: &[NativeInterfacePlan],
    ) -> AppResult<NativeTransportHandle> {
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum building transport identity_path={} interface_plans={}",
            identity_path.display(),
            interfaces.len()
        )));
        let identity =
            load_transport_private_identity_file(identity_path).map_err(AppError::from)?;
        let mut config =
            reticulum_rs::runtime::TransportConfig::new("omenbrowser_rs", &identity, true);
        config.set_path_request_timeout_secs(self.config.request_timeout_secs.max(1));
        config.set_ratchet_store_path(self.config.reticulum_storage_dir.join("ratchets"));
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum ratchet store {}",
            self.config.reticulum_storage_dir.join("ratchets").display()
        )));
        let mut transport = reticulum_rs::runtime::Transport::new(config);
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let clean_lxmf_delivery_destination =
            block_on_native_transport_setup(transport.add_destination(
                identity.clone(),
                rns_transport::destination::DestinationName::new("lxmf", "delivery"),
            ))?;
        #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
        block_on_native_transport_setup(transport.set_receipt_handler(Box::new(
            CleanLxmfReceiptHandler {
                pending_lxmf_proofs: self.pending_lxmf_proofs.clone(),
                event_tx: self.event_tx.clone(),
            },
        )))?;
        let transport = Arc::new(transport);
        let attached_interface_records = attach_tcp_client_interfaces(&transport, interfaces)?;
        let shutdown = tokio_util::sync::CancellationToken::new();
        let attached_interfaces = attached_interface_records
            .iter()
            .map(|record| record.label.clone())
            .collect::<Vec<_>>();
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let (path_restore_ready_tx, path_restore_ready) = watch::channel(false);
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            spawn_reticulum_path_table_restore(
                transport.clone(),
                self.config.reticulum_storage_dir.clone(),
                self.clean_destination_identities.clone(),
                self.event_tx.clone(),
                path_restore_ready_tx,
            );
            spawn_reticulum_path_table_saver(
                transport.clone(),
                self.config.reticulum_storage_dir.clone(),
                self.event_tx.clone(),
                shutdown.clone(),
            );
        }
        spawn_announce_listener(
            transport.clone(),
            NativeAnnounceListenerState {
                #[cfg(not(all(feature = "native-rns-net", any())))]
                storage_path: self.config.reticulum_storage_dir.clone(),
                announces: self.announces.clone(),
                #[cfg(not(all(feature = "native-rns-net", any())))]
                clean_destination_identities: self.clean_destination_identities.clone(),
                #[cfg(not(all(feature = "native-rns-net", any())))]
                clean_destination_app_data: self.clean_destination_app_data.clone(),
                #[cfg(not(all(feature = "native-rns-net", any())))]
                clean_recent_omenchat_announces: self.clean_recent_omenchat_announces.clone(),
                event_tx: self.event_tx.clone(),
                shutdown: shutdown.clone(),
            },
        );
        #[cfg(not(all(feature = "native-rns-net", any())))]
        spawn_clean_omenchat_event_bridge(
            transport.clone(),
            self.active_omenchat_links.clone(),
            self.clean_omenchat_links.clone(),
            self.clean_destination_identities.clone(),
            self.config.attachments_dir.clone(),
            self.event_tx.clone(),
            #[cfg(feature = "native-lxmf")]
            self.pending_lxmf_proofs.clone(),
        );
        let interface_count = attached_interfaces.len();
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum transport ready attached_tcp_clients={interface_count}"
        )));
        if !attached_interfaces.is_empty() {
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum attached clean interface map: {}",
                attached_interfaces.join(" | ")
            )));
        }

        Ok(NativeTransportHandle {
            transport,
            shutdown,
            interface_count,
            attached_interfaces,
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_attached_interfaces: attached_interface_records
                .iter()
                .map(|record| record.clean.clone())
                .collect(),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            storage_path: self.config.reticulum_storage_dir.clone(),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            path_restore_ready,
            #[cfg(not(all(feature = "native-rns-net", any())))]
            clean_lxmf_delivery_destination,
        })
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    fn build_rns_net_request_handle(
        &self,
        active_identity: Option<&NativeIdentitySummary>,
        active_identity_profile: Option<&IdentityProfile>,
        active_identity_path: Option<&Path>,
    ) -> AppResult<Option<NativeRnsNetHandle>> {
        if matches!(
            self.config.instance_mode,
            crate::runtime::native::config::NativeRuntimeMode::Managed
        ) {
            if let Some(path) = active_identity_path {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native rns-net syncing managed identity config identity_path={} config_dir={}",
                    path.display(),
                    self.config.reticulum_config_dir.display()
                )));
                sync_managed_rns_net_identity_config(&self.config.reticulum_config_dir, path)?;
            }
        }
        let mut key_store =
            RnsNetDestinationKeyStore::load_recent_known_destinations_from_config_dir(
                self.config.reticulum_config_dir.as_path(),
                NATIVE_KNOWN_DESTINATIONS_MAX_AGE_SECS,
            )
            .map_err(AppError::from)?;
        let managed_key_count = key_store.len();
        let system_key_count = if matches!(
            self.config.instance_mode,
            crate::runtime::native::config::NativeRuntimeMode::External
        ) {
            extend_with_system_known_destinations(&mut key_store)
        } else {
            0
        };
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native rns-net destination key preload managed={} system={} total={} mode={:?}",
            managed_key_count,
            system_key_count,
            key_store.len(),
            self.config.instance_mode
        )));
        let destination_keys = Arc::new(Mutex::new(key_store));
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native rns-net starting request node config_dir={}",
            self.config.reticulum_config_dir.display()
        )));
        let startup_timer = Instant::now();
        let (announce_tx, mut announce_rx) = tokio::sync::mpsc::unbounded_channel();
        let (path_tx, mut path_rx) = tokio::sync::mpsc::unbounded_channel();
        let (local_tx, mut local_rx) = tokio::sync::mpsc::unbounded_channel();
        let (proof_tx, mut proof_rx) = tokio::sync::mpsc::unbounded_channel();
        let (resource_tx, mut resource_rx) = tokio::sync::mpsc::unbounded_channel();
        let (link_data_tx, mut link_data_rx) = tokio::sync::mpsc::unbounded_channel();
        let (link_closed_tx, mut link_closed_rx) = tokio::sync::mpsc::unbounded_channel();
        let (mut callbacks, response_rx, link_rx) = RnsNetPageCallbacks::with_all_event_sinks(
            announce_tx,
            path_tx,
            local_tx,
            proof_tx,
            resource_tx,
        );
        callbacks.set_link_data_sink(link_data_tx);
        callbacks.set_link_closed_sink(link_closed_tx);
        let node = rns_net::RnsNode::from_config(
            Some(self.config.reticulum_config_dir.as_path()),
            Box::new(callbacks),
        )
        .map_err(|error| {
            AppError::Runtime(format!(
                "native rns-net request node failed to start: {error}"
            ))
        })?;
        let client = RnsNetPageRequestClient::from_started_node(node, response_rx, link_rx);
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native rns-net request node ready after {:.3}s",
            startup_timer.elapsed().as_secs_f64()
        )));
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native rns-net startup skipped bulk identity injection count={}; destination keys are recalled on demand",
            destination_keys
                .lock()
                .expect("native rns-net key store lock")
                .len()
        )));
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(
            "native ratchet store active: small direct LXMF can use cached announce ratchets for opportunistic single-packet submission when no direct path is known".into(),
        ));
        let mut local_lxmf_delivery_registered = false;
        let mut local_lxmf_delivery_link_registered = false;
        let mut local_lxmf_delivery_proof_capable = false;
        let mut local_lxmf_delivery_announced = false;
        let mut local_lxmf_delivery_destination_hash = None;
        let mut local_identity_private_key = None;
        if let Some(identity) = active_identity {
            let identity_hash = parse_rns_net_destination_hash(&identity.address_hash_hex)?;
            if let Some(path) = active_identity_path {
                let signing_key =
                    load_rns_net_proof_signing_key_file(path).map_err(AppError::from)?;
                local_identity_private_key = Some(signing_key);
                let registration = client.register_single_destination_with_proof(
                    identity_hash,
                    "lxmf",
                    "delivery",
                    signing_key,
                )?;
                local_lxmf_delivery_registered = true;
                local_lxmf_delivery_proof_capable = true;
                local_lxmf_delivery_destination_hash =
                    Some(hex_encode(&registration.destination_hash));
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native rns-net registered local {}.{} destination={} proof_strategy={}",
                    registration.app_name,
                    registration.aspect,
                    hex_encode(&registration.destination_hash),
                    registration.proof_strategy
                )));
                client.register_link_destination(registration.destination_hash, signing_key)?;
                local_lxmf_delivery_link_registered = true;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native rns-net registered local LXMF link destination={} resource_strategy=accept_all",
                    hex_encode(&registration.destination_hash)
                )));
                if self.config.announce_on_start {
                    let app_data = local_lxmf_delivery_announce_app_data(
                        local_lxmf_display_name(identity, active_identity_profile).as_str(),
                    )?;
                    let announcement = client.announce_single_destination(
                        identity_hash,
                        "lxmf",
                        "delivery",
                        signing_key,
                        Some(app_data.as_slice()),
                    )?;
                    local_lxmf_delivery_announced = true;
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native rns-net announced local {}.{} destination={}",
                        announcement.app_name,
                        announcement.aspect,
                        hex_encode(&announcement.destination_hash)
                    )));
                }
            } else {
                client.register_single_destination(rns_net_destination_hash(
                    &identity_hash,
                    "lxmf",
                    "delivery",
                ))?;
                local_lxmf_delivery_registered = true;
                local_lxmf_delivery_destination_hash = Some(hex_encode(&rns_net_destination_hash(
                    &identity_hash,
                    "lxmf",
                    "delivery",
                )));
            }
        }
        let keys_for_task = destination_keys.clone();
        let managed_known_destinations_path = self
            .config
            .reticulum_config_dir
            .join("storage")
            .join("known_destinations");
        let announce_events = self.event_tx.clone();
        tokio::spawn(async move {
            let mut save_interval = tokio::time::interval(Duration::from_secs(
                NATIVE_KNOWN_DESTINATIONS_SAVE_INTERVAL_SECS,
            ));
            save_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut dirty_known_destinations = false;
            loop {
                tokio::select! {
                    maybe_key = announce_rx.recv() => {
                        let Some(key) = maybe_key else {
                            if dirty_known_destinations {
                                save_rns_net_known_destinations_snapshot(
                                    &keys_for_task,
                                    &managed_known_destinations_path,
                                    &announce_events,
                                );
                            }
                            break;
                        };
                        let payload = rns_net_announce_payload(&key);
                        if !should_emit_directory_announce(&payload) {
                            continue;
                        }
                        let hops = key.hops.map(u32::from);
                        {
                            let mut guard = keys_for_task
                                .lock()
                                .expect("native rns-net key store lock");
                            guard.ingest_with_nomadnet_lxmf_siblings(key);
                        }
                        dirty_known_destinations = true;
                        let _ = announce_events.send(RuntimeBusEvent::Announce(payload.clone()));
                        let _ = announce_events.send(RuntimeBusEvent::PathUpdated(PathEvent {
                            destination_hash: payload.destination_hash.clone(),
                            known: true,
                            hops,
                        }));
                        if let Some(associated_hash) = payload.associated_hash.as_ref() {
                            let _ = announce_events.send(RuntimeBusEvent::PathUpdated(PathEvent {
                                destination_hash: associated_hash.clone(),
                                known: true,
                                hops,
                            }));
                        }
                        if let Some(node_associated_hash) = payload.node_associated_hash.as_ref() {
                            let _ = announce_events.send(RuntimeBusEvent::PathUpdated(PathEvent {
                                destination_hash: node_associated_hash.clone(),
                                known: true,
                                hops,
                            }));
                        }
                    }
                    _ = save_interval.tick() => {
                        if dirty_known_destinations {
                            save_rns_net_known_destinations_snapshot(
                                &keys_for_task,
                                &managed_known_destinations_path,
                                &announce_events,
                            );
                            dirty_known_destinations = false;
                        }
                    }
                }
            }
        });
        let path_events = self.event_tx.clone();
        let keys_for_path_task = destination_keys.clone();
        tokio::spawn(async move {
            while let Some(update) = path_rx.recv().await {
                emit_rns_net_path_update_with_siblings(&path_events, &keys_for_path_task, update);
            }
        });
        let proof_events = self.event_tx.clone();
        let pending_lxmf_proofs = self.pending_lxmf_proofs.clone();
        tokio::spawn(async move {
            while let Some(proof) = proof_rx.recv().await {
                for event in native_lxmf_events_for_packet_proof(&proof, &pending_lxmf_proofs) {
                    if proof_events.send(event).is_err() {
                        break;
                    }
                }
            }
        });
        let direct_router_events = self.event_tx.clone();
        let pending_lxmf_proofs = self.pending_lxmf_proofs.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(NATIVE_LXMF_DIRECT_ROUTER_TICK_SECS));
            loop {
                interval.tick().await;
                let events = native_lxmf_reconcile_direct_router_timeouts(
                    &pending_lxmf_proofs,
                    native_unix_timestamp(),
                    NATIVE_LXMF_DIRECT_PROOF_TIMEOUT_SECS,
                );
                for event in events {
                    emit_native_lxmf_direct_timeout_event(&direct_router_events, event);
                }
            }
        });
        #[cfg(feature = "native-lxmf")]
        {
            let link_data_events = self.event_tx.clone();
            let inbound_messages = self.inbound_messages.clone();
            let pending_lxmf_proofs = self.pending_lxmf_proofs.clone();
            let destination_keys = destination_keys.clone();
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            let pending_propagated_lxmf = self.pending_propagated_lxmf.clone();
            let attachments_dir = self.config.attachments_dir.clone();
            tokio::spawn(async move {
                while let Some(link_data) = link_data_rx.recv().await {
                    if link_data.context == OMENCHAT_LINK_CONTEXT {
                        let _ = link_data_events.send(RuntimeBusEvent::OmenChatLinkData(
                            OmenChatLinkData {
                                link_id: link_data.link_id,
                                frame_bytes: link_data.data,
                            },
                        ));
                        continue;
                    }
                    if link_data.context != 0 {
                        continue;
                    }
                    match native_lxmf_message_from_link_data(&link_data, &attachments_dir) {
                        Ok(message) => {
                            let direct_evidence = native_lxmf_inbound_peer_evidence(
                                &message,
                                &pending_lxmf_proofs,
                                &destination_keys,
                                "native link-data LXMF delivery from peer with pending direct outbound",
                            );
                            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                            let propagated_evidence = native_lxmf_inbound_peer_propagated_evidence(
                                &message,
                                &pending_propagated_lxmf,
                                &destination_keys,
                                "native link-data LXMF delivery from peer with pending propagated outbound",
                            );
                            inbound_messages
                                .lock()
                                .expect("native inbound message lock")
                                .push(message.clone());
                            let _ =
                                link_data_events.send(RuntimeBusEvent::MessageReceived(message));
                            for evidence in direct_evidence {
                                let _ = link_data_events
                                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                            }
                            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                            for evidence in propagated_evidence {
                                let _ = link_data_events
                                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                            }
                        }
                        Err(error) => {
                            if native_lxmf_decode_error_is_truncated(&error) {
                                continue;
                            }
                            let _ = link_data_events.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF link data decode failed link_id={} context={} bytes={}: {error}",
                                hex_encode(&link_data.link_id),
                                link_data.context,
                                link_data.data.len()
                            )));
                        }
                    }
                }
            });
        }
        {
            let link_closed_events = self.event_tx.clone();
            #[cfg(all(feature = "native-rns-net", any()))]
            let active_omenchat_links = self.active_omenchat_links.clone();
            tokio::spawn(async move {
                while let Some(event) = link_closed_rx.recv().await {
                    #[cfg(all(feature = "native-rns-net", any()))]
                    if let Some(closed) = take_active_omenchat_link_close(
                        &active_omenchat_links,
                        event.link_id,
                        event.reason,
                    ) {
                        let _ =
                            link_closed_events.send(RuntimeBusEvent::OmenChatLinkClosed(closed));
                    }
                }
            });
        }
        #[cfg(not(feature = "native-lxmf"))]
        {
            let link_data_events = self.event_tx.clone();
            tokio::spawn(async move {
                while let Some(link_data) = link_data_rx.recv().await {
                    if link_data.context == OMENCHAT_LINK_CONTEXT {
                        let _ = link_data_events.send(RuntimeBusEvent::OmenChatLinkData(
                            OmenChatLinkData {
                                link_id: link_data.link_id,
                                frame_bytes: link_data.data,
                            },
                        ));
                    }
                }
            });
        }
        #[cfg(feature = "native-lxmf")]
        {
            let resource_events = self.event_tx.clone();
            let active_omenchat_links = self.active_omenchat_links.clone();
            let pending_direct = self.pending_direct_lxmf_resources.clone();
            let pending_lxmf_proofs = self.pending_lxmf_proofs.clone();
            let pending_propagated = self.pending_propagated_lxmf.clone();
            let inbound_messages = self.inbound_messages.clone();
            let destination_keys = destination_keys.clone();
            let attachments_dir = self.config.attachments_dir.clone();
            tokio::spawn(async move {
                while let Some(event) = resource_rx.recv().await {
                    if let RnsNetResourceEvent::Received {
                        link_id,
                        data,
                        metadata,
                    } = &event
                    {
                        let is_omenchat_resource = metadata.as_deref().is_some_and(|value| {
                            value.starts_with(OMENCHAT_RESOURCE_METADATA_PREFIX)
                        }) || active_omenchat_links
                            .lock()
                            .expect("native active OMENchat link lock")
                            .contains(link_id);
                        if is_omenchat_resource {
                            let _ = resource_events.send(RuntimeBusEvent::OmenChatResourceData(
                                OmenChatResourceData {
                                    link_id: *link_id,
                                    data: data.clone(),
                                    metadata: metadata.clone(),
                                },
                            ));
                            continue;
                        }
                    }
                    match native_lxmf_message_from_resource_event(&event, &attachments_dir) {
                        Ok(Some(message)) => {
                            let direct_evidence = native_lxmf_inbound_peer_evidence(
                                &message,
                                &pending_lxmf_proofs,
                                &destination_keys,
                                "native resource LXMF delivery from peer with pending direct outbound",
                            );
                            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                            let propagated_evidence = native_lxmf_inbound_peer_propagated_evidence(
                                &message,
                                &pending_propagated,
                                &destination_keys,
                                "native resource LXMF delivery from peer with pending propagated outbound",
                            );
                            inbound_messages
                                .lock()
                                .expect("native inbound message lock")
                                .push(message.clone());
                            let _ = resource_events.send(RuntimeBusEvent::MessageReceived(message));
                            for evidence in direct_evidence {
                                let _ = resource_events
                                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                            }
                            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                            for evidence in propagated_evidence {
                                let _ = resource_events
                                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            if native_lxmf_decode_error_is_truncated(&error) {
                                continue;
                            }
                            let _ = resource_events.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF resource decode failed: {error}"
                            )));
                        }
                    }
                    if let Some((status, evidence)) =
                        native_lxmf_direct_resource_status_for_event(event.clone(), &pending_direct)
                    {
                        let peer_hash = status.peer_hash.clone();
                        let message_id = status.message_id.clone().unwrap_or_default();
                        let status_evidence = status.evidence.clone().unwrap_or_default();
                        let _ =
                            resource_events.send(RuntimeBusEvent::MessageDeliveryUpdated(status));
                        if let Some(evidence) = evidence {
                            let _ = resource_events
                                .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                        }
                        let _ = resource_events.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF direct resource event peer={} message_id={} evidence={}",
                            peer_hash, message_id, status_evidence
                        )));
                    } else if let Some((status, evidence)) =
                        native_lxmf_resource_status_for_event(event.clone(), &pending_propagated)
                    {
                        let peer_hash = status.peer_hash.clone();
                        let message_id = status.message_id.clone().unwrap_or_default();
                        let status_evidence = status.evidence.clone().unwrap_or_default();
                        let _ =
                            resource_events.send(RuntimeBusEvent::MessageDeliveryUpdated(status));
                        if let Some(evidence) = evidence {
                            if evidence.kind == LxmfDeliveryEvidenceKind::PropagationNodeAccepted {
                                let _ = resource_events.send(RuntimeBusEvent::PropagationSync(
                                    PropagationSyncEvent {
                                        stage: PropagationSyncStage::Complete,
                                        status: PropagationSyncEventStatus::Progress,
                                        destination_hash: evidence.detail.as_deref().and_then(
                                            |detail| {
                                                extract_native_evidence_value(
                                                    detail,
                                                    "propagation_node",
                                                )
                                                .map(ToOwned::to_owned)
                                            },
                                        ),
                                        detail: "propagation node accepted outbound LXMF; peer delivery remains unconfirmed, run propagation sync or wait for peer activity"
                                            .into(),
                                        counts: BTreeMap::from([(
                                            "accepted_outbound".into(),
                                            1,
                                        )]),
                                    },
                                ));
                            }
                            let _ = resource_events
                                .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                        }
                        let _ = resource_events.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF propagation resource event peer={} message_id={} evidence={}",
                            peer_hash, message_id, status_evidence
                        )));
                    } else {
                        let _ = resource_events.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF propagation resource event had no matching pending message: {:?}",
                            event
                        )));
                    }
                }
            });
        }
        #[cfg(not(feature = "native-lxmf"))]
        {
            let resource_events = self.event_tx.clone();
            let active_omenchat_links = self.active_omenchat_links.clone();
            tokio::spawn(async move {
                while let Some(event) = resource_rx.recv().await {
                    if let RnsNetResourceEvent::Received {
                        link_id,
                        data,
                        metadata,
                    } = &event
                    {
                        let is_omenchat_resource = metadata.as_deref().is_some_and(|value| {
                            value.starts_with(OMENCHAT_RESOURCE_METADATA_PREFIX)
                        }) || active_omenchat_links
                            .lock()
                            .expect("native active OMENchat link lock")
                            .contains(link_id);
                        if is_omenchat_resource {
                            let _ = resource_events.send(RuntimeBusEvent::OmenChatResourceData(
                                OmenChatResourceData {
                                    link_id: *link_id,
                                    data: data.clone(),
                                    metadata: metadata.clone(),
                                },
                            ));
                            continue;
                        }
                    }
                    let _ = resource_events.send(RuntimeBusEvent::Debug(format!(
                        "native RNS resource event ignored without native-lxmf feature: {:?}",
                        event
                    )));
                }
            });
        }
        #[cfg(feature = "native-lxmf")]
        {
            let local_events = self.event_tx.clone();
            let inbound_messages = self.inbound_messages.clone();
            let pending_lxmf_proofs = self.pending_lxmf_proofs.clone();
            let destination_keys = destination_keys.clone();
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            let pending_propagated_lxmf = self.pending_propagated_lxmf.clone();
            let attachments_dir = self.config.attachments_dir.clone();
            tokio::spawn(async move {
                while let Some(delivery) = local_rx.recv().await {
                    match decode_rns_net_lxmf_delivery(&delivery, &attachments_dir) {
                        Ok(message) => {
                            let direct_evidence = native_lxmf_inbound_peer_evidence(
                                &message,
                                &pending_lxmf_proofs,
                                &destination_keys,
                                "native local LXMF delivery from peer with pending direct outbound",
                            );
                            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                            let propagated_evidence = native_lxmf_inbound_peer_propagated_evidence(
                                &message,
                                &pending_propagated_lxmf,
                                &destination_keys,
                                "native local LXMF delivery from peer with pending propagated outbound",
                            );
                            inbound_messages
                                .lock()
                                .expect("native inbound message lock")
                                .push(message.clone());
                            let _ = local_events.send(RuntimeBusEvent::MessageReceived(message));
                            for evidence in direct_evidence {
                                let _ = local_events
                                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                            }
                            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                            for evidence in propagated_evidence {
                                let _ = local_events
                                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                            }
                        }
                        Err(error) => {
                            if native_lxmf_decode_error_is_truncated(&error) {
                                continue;
                            }
                            let _ = local_events.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF local delivery decode failed for {}: {error}",
                                hex_encode(&delivery.destination_hash)
                            )));
                        }
                    }
                }
            });
        }
        #[cfg(not(feature = "native-lxmf"))]
        {
            tokio::spawn(async move { while local_rx.recv().await.is_some() {} });
        }

        Ok(Some(NativeRnsNetHandle {
            client,
            destination_keys,
            local_identity_private_key,
            local_lxmf_delivery_registered,
            local_lxmf_delivery_link_registered,
            local_lxmf_delivery_proof_capable,
            local_lxmf_delivery_announced,
            local_lxmf_delivery_destination_hash,
        }))
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    fn selected_propagation_node(&self) -> Option<String> {
        self.outbound_propagation_node
            .lock()
            .expect("native propagation node lock")
            .clone()
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    async fn send_propagated_lxmf_message(
        &self,
        mut envelope: MessageEnvelope,
        identity_bytes: &[u8],
        fallback_reason: Option<String>,
    ) -> AppResult<MessageSummary> {
        envelope.delivery_mode = DeliveryMode::Propagated;
        let propagation_node = self
            .selected_propagation_node()
            .ok_or_else(|| AppError::Runtime("No propagation node is selected".into()))?;
        let propagation_destination = parse_rns_net_destination_hash(&propagation_node)?;
        let peer_destination = parse_rns_net_destination_hash(&envelope.peer_hash)?;
        let handle = self
            .rns_net
            .lock()
            .expect("native rns-net lock")
            .clone()
            .ok_or_else(|| AppError::Runtime("native rns-net runtime is not started".into()))?;
        let source_hash = handle.local_lxmf_delivery_destination_hash.clone().ok_or_else(|| {
            AppError::Runtime(
                "local LXMF delivery destination is not registered; attach/announce identity before sending"
                    .into(),
            )
        })?;
        let outbound = crate::runtime::native_lxmf::codec::build_outbound_message(
            &envelope,
            source_hash.as_str(),
        )?;
        let transport_method =
            crate::runtime::native_lxmf::codec::app_transport_method(outbound.delivery.method);
        let mut has_path = handle.client.has_path(propagation_destination).await?;
        if !has_path {
            handle.client.request_path(propagation_destination).await?;
            for _ in 0..NATIVE_LXMF_PROPAGATION_PATH_WAIT_ATTEMPTS {
                tokio::time::sleep(NATIVE_LXMF_PROPAGATION_PATH_WAIT_STEP).await;
                if handle.client.has_path(propagation_destination).await? {
                    has_path = true;
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation path acquired after request propagation_node={propagation_node}"
                    )));
                    break;
                }
            }
        }
        let propagation_key = handle
            .destination_keys
            .lock()
            .expect("native rns-net key store lock")
            .destination_key(&propagation_destination);
        let known_app_data = propagation_key
            .as_ref()
            .is_some_and(rns_net_propagation_app_data_valid);
        let submitted_at = native_unix_timestamp();
        let mut message_id = None;
        let mut propagation_envelope_bytes = None;
        let mut propagation_transient_id = None;
        let mut propagation_stamp_fields = BTreeMap::new();
        if has_path {
            if let Some(propagation_key) = propagation_key.as_ref() {
                let peer_key = {
                    handle
                        .destination_keys
                        .lock()
                        .expect("native rns-net key store lock")
                        .destination_key(&peer_destination)
                };
                let peer_key = match peer_key {
                    Some(key) => key,
                    None => {
                        let Some(key) = handle
                            .client
                            .recall_destination_key(peer_destination)
                            .await?
                        else {
                            return Err(AppError::Runtime(
                                "LXMF peer identity is not known; propagated sends need the peer announce/public key before encryption".into(),
                            ));
                        };
                        handle
                            .destination_keys
                            .lock()
                            .expect("native rns-net key store lock")
                            .ingest_with_nomadnet_lxmf_siblings(key.clone());
                        key
                    }
                };
                let target_stamp_cost = propagation_key.app_data.as_deref().and_then(
                    crate::runtime::native_lxmf::codec::propagation_announce_target_stamp_cost,
                );
                let outbound_for_task = outbound.clone();
                let identity_bytes_for_task = identity_bytes.to_vec();
                let peer_public_key = peer_key.full_public_key;
                let package = tokio::task::spawn_blocking(move || {
                    crate::runtime::native_lxmf::codec::encode_signed_propagation_envelope(
                        &outbound_for_task,
                        identity_bytes_for_task.as_slice(),
                        peer_public_key,
                        target_stamp_cost,
                        crate::runtime::native_lxmf::codec::DEFAULT_PROPAGATION_STAMP_MAX_ATTEMPTS,
                    )
                })
                .await
                .map_err(|error| {
                    AppError::Runtime(format!("LXMF propagation envelope task failed: {error}"))
                })??;
                let transient_hex = hex_encode(&package.transient_id);
                message_id = Some(transient_hex.clone());
                propagation_transient_id = Some(transient_hex);
                if let Some(stamp) = package.stamp.as_ref() {
                    propagation_stamp_fields.insert(
                        "native_lxmf_propagation_stamp_cost".to_string(),
                        stamp.target_cost.to_string(),
                    );
                    propagation_stamp_fields.insert(
                        "native_lxmf_propagation_stamp_value".to_string(),
                        stamp.stamp_value.to_string(),
                    );
                    propagation_stamp_fields.insert(
                        "native_lxmf_propagation_stamp_attempts".to_string(),
                        stamp.attempts.to_string(),
                    );
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation stamp generated peer={} propagation_node={} transient_id={} target_cost={} stamp_value={} attempts={}",
                        envelope.peer_hash,
                        propagation_node,
                        hex_encode(&stamp.transient_id),
                        stamp.target_cost,
                        stamp.stamp_value,
                        stamp.attempts
                    )));
                } else if target_stamp_cost.is_none() {
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation stamp skipped peer={} propagation_node={} reason=no_advertised_target_cost",
                        envelope.peer_hash, propagation_node
                    )));
                }
                propagation_envelope_bytes = Some(package.envelope);
            }
        }
        let message_id = match message_id {
            Some(message_id) => message_id,
            None => {
                let wire_bytes = crate::runtime::native_lxmf::codec::encode_signed_wire_message(
                    &outbound,
                    identity_bytes,
                )?;
                native_lxmf_wire_message_id(&wire_bytes)
            }
        };
        let mut link_id = None;
        let transfer_state: String;
        let retry_guidance: String;
        let mut failure_reason = fallback_reason.clone();

        if has_path {
            if let Some(propagation_key) = propagation_key {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF propagation link establishing peer={} propagation_node={} message_id={}",
                    envelope.peer_hash, propagation_node, message_id
                )));
                match handle
                    .client
                    .establish_link(
                        propagation_destination,
                        propagation_key.signing_public_key,
                        Duration::from_secs(8),
                        CancellationToken::new(),
                    )
                    .await
                {
                    Ok(link) => {
                        let link_hex = hex_encode(&link.link_id);
                        let propagation_envelope_bytes =
                            propagation_envelope_bytes.clone().ok_or_else(|| {
                                AppError::Runtime(
                                    "LXMF propagation envelope was not prepared".into(),
                                )
                            })?;
                        if propagation_envelope_bytes.len() <= NATIVE_LXMF_LINK_PACKET_MDU {
                            match handle
                                .client
                                .send_on_link(link.link_id, propagation_envelope_bytes, 0)
                                .await
                            {
                                Ok(()) => {
                                    link_id = Some(link_hex.clone());
                                    transfer_state = "link_packet_sent".into();
                                    retry_guidance =
                                        "LXMF propagation envelope was sent as a link packet to the selected propagation node; peer delivery remains unconfirmed until sync or peer activity"
                                            .into();
                                    let router_event =
                                        NativePropagatedLxmfRouter::propagation_node_accepted(
                                            PropagatedNodeAccepted {
                                                peer_hash: &envelope.peer_hash,
                                                message_id: &message_id,
                                                propagation_node: &propagation_node,
                                                submitted_at,
                                                transfer_state: "link_packet_sent",
                                                link_id: Some(&link_hex),
                                                representation: Some("link_packet"),
                                                observed_at: native_unix_timestamp(),
                                            },
                                        );
                                    let _ = self.event_tx.send(
                                        RuntimeBusEvent::MessageDeliveryUpdated(
                                            router_event.status,
                                        ),
                                    );
                                    let _ =
                                        self.event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
                                            router_event.evidence,
                                        ));
                                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native LXMF propagation envelope link packet sent peer={} propagation_node={} message_id={} link_id={} state=propagation_node_accepted",
                                        envelope.peer_hash, propagation_node, message_id, link_hex
                                    )));
                                }
                                Err(error) => {
                                    link_id = Some(link_hex.clone());
                                    transfer_state = "link_packet_failed".into();
                                    retry_guidance =
                                        "propagation link established but link-packet send failed; run Prop Diag and retry"
                                            .into();
                                    failure_reason = Some(match failure_reason {
                                        Some(reason) => format!("{reason}; {error}"),
                                        None => error.to_string(),
                                    });
                                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native LXMF propagation link-packet send failed peer={} propagation_node={} message_id={} link_id={} error={}",
                                        envelope.peer_hash, propagation_node, message_id, link_hex, error
                                    )));
                                }
                            }
                        } else {
                            link_id = Some(link_hex.clone());
                            transfer_state = "resource_advertised".into();
                            retry_guidance =
                                "LXMF propagation envelope resource was queued for the selected propagation node; peer delivery remains unconfirmed until sync or peer activity"
                                    .into();
                            let client = handle.client.clone();
                            let event_tx = self.event_tx.clone();
                            let peer_hash = envelope.peer_hash.clone();
                            let message_id_for_task = message_id.clone();
                            let propagation_node_for_task = propagation_node.clone();
                            let link_hex_for_task = link_hex.clone();
                            tokio::spawn(async move {
                                match client
                                    .send_resource(link.link_id, propagation_envelope_bytes, None)
                                    .await
                                {
                                    Ok(()) => {
                                        let router_event =
                                            NativePropagatedLxmfRouter::propagation_node_accepted(
                                                PropagatedNodeAccepted {
                                                    peer_hash: &peer_hash,
                                                    message_id: &message_id_for_task,
                                                    propagation_node: &propagation_node_for_task,
                                                    submitted_at,
                                                    transfer_state: "resource_advertised",
                                                    link_id: Some(&link_hex_for_task),
                                                    representation: Some("resource"),
                                                    observed_at: native_unix_timestamp(),
                                                },
                                            );
                                        let _ =
                                            event_tx.send(RuntimeBusEvent::MessageDeliveryUpdated(
                                                router_event.status,
                                            ));
                                        let _ =
                                            event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
                                                router_event.evidence,
                                            ));
                                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                            "native LXMF propagation envelope resource advertised peer={} propagation_node={} message_id={} link_id={} state=propagation_node_accepted",
                                            peer_hash,
                                            propagation_node_for_task,
                                            message_id_for_task,
                                            link_hex_for_task
                                        )));
                                    }
                                    Err(error) => {
                                        let error = error.to_string();
                                        let router_event =
                                            NativePropagatedLxmfRouter::propagation_node_failed(
                                                PropagatedNodeFailed {
                                                    peer_hash: &peer_hash,
                                                    message_id: &message_id_for_task,
                                                    propagation_node: &propagation_node_for_task,
                                                    submitted_at,
                                                    transfer_state: "resource_advertise_failed",
                                                    link_id: Some(&link_hex_for_task),
                                                    failure_reason: &error,
                                                    observed_at: native_unix_timestamp(),
                                                },
                                            );
                                        let _ =
                                            event_tx.send(RuntimeBusEvent::MessageDeliveryUpdated(
                                                router_event.status,
                                            ));
                                        let _ =
                                            event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
                                                router_event.evidence,
                                            ));
                                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                            "native LXMF propagation resource advertisement failed peer={} propagation_node={} message_id={} link_id={} error={}",
                                            peer_hash,
                                            propagation_node_for_task,
                                            message_id_for_task,
                                            link_hex_for_task,
                                            error
                                        )));
                                    }
                                }
                            });
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF propagation envelope resource queued peer={} propagation_node={} message_id={} link_id={} state=resource_advertised",
                                envelope.peer_hash, propagation_node, message_id, link_hex
                            )));
                        }
                    }
                    Err(error) => {
                        transfer_state = "link_timeout".into();
                        retry_guidance =
                            "propagation link did not establish; verify selected propagation node path/IFAC and retry"
                                .into();
                        failure_reason = Some(match failure_reason {
                            Some(reason) => format!("{reason}; {error}"),
                            None => error.to_string(),
                        });
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF propagation link failed peer={} propagation_node={} message_id={} error={}",
                            envelope.peer_hash, propagation_node, message_id, error
                        )));
                    }
                }
            } else {
                transfer_state = "router_deferred".into();
                retry_guidance =
                    "selected propagation node path is known but destination app data/key is missing; wait for announce or run Prop Diag"
                        .into();
            }
        } else {
            transfer_state = "router_deferred".into();
            retry_guidance =
                "selected propagation node path is not known yet; path was requested and the queued message can be retried after the path appears"
                    .into();
        }
        self.pending_propagated_lxmf
            .lock()
            .expect("pending propagated LXMF lock")
            .insert(
                message_id.clone(),
                PendingNativePropagatedLxmf {
                    peer_hash: envelope.peer_hash.clone(),
                    propagation_node: propagation_node.clone(),
                    submitted_at,
                    has_path,
                    known_app_data,
                    link_id: link_id.clone(),
                    transfer_state: transfer_state.clone(),
                    peer_activity_observed_at: None,
                    terminal_at: lxmf_propagation_transfer_terminal(&transfer_state)
                        .then_some(native_unix_timestamp()),
                },
            );
        let _ = self
            .event_tx
            .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                selected: true,
                destination_hash: Some(propagation_node.clone()),
                has_path,
                known_app_data,
                link_state: if has_path {
                    "path_known".into()
                } else {
                    "path_requested".into()
                },
                transfer_state: transfer_state.clone(),
            }));
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native LXMF propagated message queued locally peer={} propagation_node={} message_id={} state=queued_for_propagation fallback={} transfer_state={}",
            envelope.peer_hash,
            propagation_node,
            message_id,
            fallback_reason.is_some(),
            transfer_state
        )));
        let mut fields = BTreeMap::from([
            (
                "native_lxmf_state".to_string(),
                match transfer_state.as_str() {
                    "link_packet_sent" | "resource_advertised" | "resource_completed" => {
                        "propagation_transfer_completed"
                    }
                    "link_packet_failed"
                    | "resource_failed"
                    | "resource_advertise_failed"
                    | "link_timeout" => "failed",
                    _ => "queued_for_propagation",
                }
                .to_string(),
            ),
            (
                "native_lxmf_evidence".to_string(),
                if propagation_envelope_bytes.is_some() {
                    "lxmf_propagation_envelope_encoded".to_string()
                } else {
                    "lxmf_wire_encoded".to_string()
                },
            ),
            ("native_lxmf_message_id".to_string(), message_id.clone()),
            ("native_lxmf_source_hash".to_string(), source_hash),
            (
                "native_lxmf_submitted_at".to_string(),
                format!("{submitted_at:.3}"),
            ),
            ("native_lxmf_propagation_node".to_string(), propagation_node),
            (
                "native_lxmf_propagation_has_path".to_string(),
                has_path.to_string(),
            ),
            (
                "native_lxmf_propagation_link_id".to_string(),
                link_id.clone().unwrap_or_default(),
            ),
            (
                "native_lxmf_propagation_known_app_data".to_string(),
                known_app_data.to_string(),
            ),
            (
                "native_lxmf_propagation_transfer_state".to_string(),
                transfer_state.clone(),
            ),
            (
                "native_lxmf_propagation_state".to_string(),
                lxmf_propagation_state_for_transfer(transfer_state.as_str()).to_string(),
            ),
            ("native_lxmf_retry_guidance".to_string(), retry_guidance),
        ]);
        fields.extend(propagation_stamp_fields);
        annotate_native_lxmf_stamp_fields(&mut fields, &outbound, None);
        if let Some(transient_id) = propagation_transient_id {
            fields.insert("native_lxmf_propagation_transient_id".into(), transient_id);
        }
        fields.insert(
            "native_lxmf_propagation_payload".into(),
            if propagation_envelope_bytes.is_some() {
                "propagation_envelope"
            } else {
                "wire_fallback"
            }
            .into(),
        );
        if let Some(reason) = failure_reason {
            fields.insert("native_lxmf_failure_reason".into(), reason);
            fields.insert("native_lxmf_fallback".into(), "direct_to_propagated".into());
        }
        Ok(MessageSummary {
            peer_hash: envelope.peer_hash.clone(),
            peer_label: envelope.peer_hash.chars().take(8).collect(),
            title: envelope.title,
            content: envelope.body,
            timestamp: submitted_at,
            transport_method,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some(message_id.clone()),
            fields,
            attachments: outbound.attachments,
        })
    }

    fn record_announce(&self, payload: AnnouncePayload) {
        self.announces
            .lock()
            .expect("native announce state lock")
            .ingest(payload.clone());
        let _ = self.event_tx.send(RuntimeBusEvent::Announce(payload));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    async fn fetch_page_with_rns_net(
        &self,
        plan: &NativeFetchPlan,
        cancel: CancellationToken,
    ) -> AppResult<crate::runtime::native::request::NativePageResponse> {
        let handle = self.rns_net.lock().expect("native rns-net lock").clone();
        let Some(handle) = handle else {
            self.emit_rns_net_browser_fetch_trace(
                plan,
                false,
                vec![PageFetchProbeStep::failed(
                    PageFetchProbeStage::RuntimeSetup,
                    "rns-net runtime is not started",
                )
                .with_trace("origin", "browser_load")],
            );
            return Err(rns_net_page_fetch_error(
                plan,
                NativePageFetchFailureStage::Runtime,
                "rns-net runtime is not started",
            ));
        };
        let mut destination_hash = [0u8; 16];
        destination_hash.copy_from_slice(plan.request.destination_hash.as_slice());
        let signing_public_key = handle
            .destination_keys
            .lock()
            .expect("native rns-net key store lock")
            .signing_public_key(&destination_hash);
        let Some(signing_public_key) = signing_public_key else {
            if let Some(key) = handle
                .client
                .recall_destination_key(destination_hash)
                .await?
            {
                let signing_public_key = key.signing_public_key;
                handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .ingest_with_nomadnet_lxmf_siblings(key);
                return self
                    .fetch_page_with_rns_net_key(
                        &handle,
                        plan,
                        destination_hash,
                        signing_public_key,
                        cancel,
                    )
                    .await;
            }
            self.emit_rns_net_browser_fetch_trace(
                plan,
                false,
                vec![PageFetchProbeStep::failed(
                    PageFetchProbeStage::DestinationIdentity,
                    "destination signing public key is not known; request a path, wait for an announce, or preload the Reticulum known_destinations cache",
                )
                .with_trace("origin", "browser_load")
                .with_trace("destination", plan.request.destination_hash.to_hex_string())],
            );
            return Err(rns_net_page_fetch_error(
                plan,
                NativePageFetchFailureStage::DestinationIdentity,
                "destination signing public key is not known; request a path, wait for an announce, or preload the Reticulum known_destinations cache",
            ));
        };
        self.fetch_page_with_rns_net_key(
            &handle,
            plan,
            destination_hash,
            signing_public_key,
            cancel,
        )
        .await
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    async fn fetch_page_with_rns_net_key(
        &self,
        handle: &NativeRnsNetHandle,
        plan: &NativeFetchPlan,
        destination_hash: [u8; 16],
        signing_public_key: [u8; 32],
        cancel: CancellationToken,
    ) -> AppResult<crate::runtime::native::request::NativePageResponse> {
        let has_path = handle
            .client
            .has_path(destination_hash)
            .await
            .map_err(|error| {
                rns_net_page_fetch_error(
                    plan,
                    NativePageFetchFailureStage::PathDiscovery,
                    error.to_string(),
                )
            })?;
        if !has_path {
            if let Some(path_steps) = self
                .wait_for_rns_net_path(handle, plan, destination_hash, cancel.clone())
                .await?
            {
                self.emit_rns_net_browser_fetch_trace(plan, false, path_steps);
                return Err(rns_net_page_fetch_error(
                    plan,
                    NativePageFetchFailureStage::PathDiscovery,
                    "destination path is not known after active wait; retry after path discovery",
                ));
            }
        }
        let keys = RnsNetDestinationKeys::from_fetch_plan(plan, signing_public_key)
            .map_err(AppError::from)?;
        let identify_key = self.rns_net_browser_identify_key(handle, destination_hash);
        let (mut steps, mut response) = handle
            .client
            .fetch_page_observed(plan, keys.clone(), identify_key, cancel.clone())
            .await;
        if response.is_none()
            && rns_net_observed_reused_page_link(&steps)
            && rns_net_observed_stale_reused_page_link_failure(&steps)
            && !cancel.is_cancelled()
        {
            let cleanup = handle
                .client
                .reset_cached_page_link_for_destination(destination_hash)
                .await;
            steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::ResponseWait,
                    "cached page link timed out; retrying once with a fresh page link",
                )
                .with_trace("origin", "browser_load")
                .with_trace("destination", plan.request.destination_hash.to_hex_string())
                .with_trace("link_torn_down", cleanup.link_torn_down.to_string())
                .with_trace("path_dropped", cleanup.path_dropped.to_string()),
            );
            let (mut retry_steps, retry_response) = handle
                .client
                .fetch_page_observed(plan, keys, identify_key, cancel)
                .await;
            for step in &mut retry_steps {
                step.trace
                    .entry("fresh_link_retry".into())
                    .or_insert_with(|| "true".into());
            }
            steps.extend(retry_steps);
            response = retry_response;
        }
        for step in &mut steps {
            step.trace
                .entry("origin".into())
                .or_insert_with(|| "browser_load".into());
        }
        let ready = response.is_some();
        self.emit_rns_net_browser_fetch_trace(plan, ready, steps.clone());
        if let Some(response) = response {
            Ok(response)
        } else {
            let (stage, detail) = steps
                .iter()
                .find(|step| !step.ok)
                .map(|step| {
                    (
                        native_failure_stage_from_probe_stage(&step.stage),
                        step.detail.clone(),
                    )
                })
                .unwrap_or((
                    NativePageFetchFailureStage::ResponseWait,
                    "rns-net page request failed without a detailed observed step".into(),
                ));
            Err(rns_net_page_fetch_error(plan, stage, detail))
        }
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    fn rns_net_browser_identify_key(
        &self,
        handle: &NativeRnsNetHandle,
        destination_hash: [u8; 16],
    ) -> Option<[u8; 64]> {
        let destination_hex = hex_encode(&destination_hash);
        if self
            .identify_on_connect_destinations
            .lock()
            .expect("native identify policy lock")
            .contains(&destination_hex)
        {
            handle.local_identity_private_key
        } else {
            None
        }
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    async fn wait_for_rns_net_path(
        &self,
        handle: &NativeRnsNetHandle,
        plan: &NativeFetchPlan,
        destination_hash: [u8; 16],
        cancel: CancellationToken,
    ) -> AppResult<Option<Vec<PageFetchProbeStep>>> {
        let destination_hex = plan.request.destination_hash.to_hex_string();
        let cached_announce_packet_hash = handle
            .destination_keys
            .lock()
            .expect("native rns-net key store lock")
            .destination_key(&destination_hash)
            .and_then(|key| key.packet_hash)
            .map(|hash| hex_encode(&hash));
        let min_prime_window = Duration::from_secs(18);
        let retry_interval = Duration::from_secs(6);
        let mut attempts = 0usize;
        let mut steps = vec![PageFetchProbeStep::ok(
            PageFetchProbeStage::PathDiscovery,
            "path missing; requesting destination path and waiting like Python OMENbrowser",
        )
        .with_trace("origin", "browser_load")
        .with_trace("destination", destination_hex.clone())
        .with_trace(
            "prime_window_secs",
            plan.timeout.max(min_prime_window).as_secs().to_string(),
        )
        .with_trace(
            "cached_announce_packet_hash",
            cached_announce_packet_hash
                .clone()
                .unwrap_or_else(|| "none".into()),
        )
        .with_trace("request_path", "queued")];
        if let Some(packet_hash) = cached_announce_packet_hash {
            steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::PathDiscovery,
                    "cached announce packet hash is available; native rns-net identity cache is preloaded, raw announce replay is not exposed yet",
                )
                .with_trace("origin", "browser_load")
                .with_trace("destination", destination_hex.clone())
                .with_trace("packet_hash", packet_hash),
            );
        }
        let sibling_requests = self
            .request_rns_net_path_with_siblings(handle, destination_hash, "browser active wait")
            .await?;
        attempts += 1;
        steps.push(
            PageFetchProbeStep::ok(
                PageFetchProbeStage::PathDiscovery,
                "queued Reticulum path request for exact destination",
            )
            .with_trace("origin", "browser_load")
            .with_trace("destination", destination_hex.clone())
            .with_trace("attempt", attempts.to_string())
            .with_trace("sibling_requests", sibling_requests.to_string()),
        );

        let deadline = tokio::time::Instant::now() + plan.timeout.max(min_prime_window);
        let mut next_retry = tokio::time::Instant::now() + retry_interval;
        loop {
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            if handle
                .client
                .has_path(destination_hash)
                .await
                .map_err(|error| {
                    rns_net_page_fetch_error(
                        plan,
                        NativePageFetchFailureStage::PathDiscovery,
                        error.to_string(),
                    )
                })?
            {
                let hops = handle.client.hops_to(destination_hash).await.ok().flatten();
                steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::PathDiscovery,
                        match hops {
                            Some(hops) => {
                                format!("path discovered during active browser wait ({hops} hops)")
                            }
                            None => "path discovered during active browser wait".into(),
                        },
                    )
                    .with_trace("origin", "browser_load")
                    .with_trace("destination", destination_hex)
                    .with_trace(
                        "hops",
                        hops.map(|hops| hops.to_string())
                            .unwrap_or_else(|| "unknown".into()),
                    ),
                );
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native rns-net browser path discovered destination={} hops={}",
                    hex_encode(&destination_hash),
                    hops.map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".into())
                )));
                return Ok(None);
            }

            let now = tokio::time::Instant::now();
            if now >= deadline {
                steps.push(
                    PageFetchProbeStep::failed(
                        PageFetchProbeStage::PathDiscovery,
                        "destination path is not known after active wait; path requests were queued",
                    )
                    .with_trace("origin", "browser_load")
                    .with_trace("destination", plan.request.destination_hash.to_hex_string())
                    .with_trace("attempts", attempts.to_string())
                    .with_trace("request_path", "queued"),
                );
                return Ok(Some(steps));
            }
            if now >= next_retry {
                let sibling_requests = self
                    .request_rns_net_path_with_siblings(
                        handle,
                        destination_hash,
                        "browser active retry",
                    )
                    .await?;
                attempts += 1;
                steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::PathDiscovery,
                        "re-queued Reticulum path request for exact destination",
                    )
                    .with_trace("origin", "browser_load")
                    .with_trace("destination", plan.request.destination_hash.to_hex_string())
                    .with_trace("attempt", attempts.to_string())
                    .with_trace("sibling_requests", sibling_requests.to_string()),
                );
                next_retry = now + retry_interval;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    async fn request_rns_net_path_with_siblings(
        &self,
        handle: &NativeRnsNetHandle,
        destination_hash: [u8; 16],
        reason: &str,
    ) -> AppResult<usize> {
        handle.client.request_path(destination_hash).await?;
        let requested_siblings = self
            .request_rns_net_sibling_paths(handle, destination_hash)
            .await;
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native rns-net requested path destination={} reason={} sibling_requests={}",
            hex_encode(&destination_hash),
            reason,
            requested_siblings
        )));
        Ok(requested_siblings)
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    async fn request_rns_net_sibling_paths(
        &self,
        handle: &NativeRnsNetHandle,
        destination_hash: [u8; 16],
    ) -> usize {
        let key = {
            handle
                .destination_keys
                .lock()
                .expect("native rns-net key store lock")
                .destination_key(&destination_hash)
        };
        let Some(key) = key else {
            return 0;
        };
        let mut requested = 0;
        for sibling in rns_net_sibling_destination_hashes(&key) {
            if sibling == destination_hash {
                continue;
            }
            match handle.client.has_path(sibling).await {
                Ok(true) => {}
                Ok(false) => {
                    if handle.client.request_path(sibling).await.is_ok() {
                        requested += 1;
                    }
                }
                Err(_) => {}
            }
        }
        requested
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    async fn canonical_rns_net_propagation_hash(
        &self,
        handle: &NativeRnsNetHandle,
        destination_hash: [u8; 16],
    ) -> AppResult<Option<String>> {
        let key = {
            handle
                .destination_keys
                .lock()
                .expect("native rns-net key store lock")
                .destination_key(&destination_hash)
        };
        let key = match key {
            Some(key) => key,
            None => {
                let Some(key) = handle
                    .client
                    .recall_destination_key(destination_hash)
                    .await?
                else {
                    return Ok(None);
                };
                handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .ingest_with_nomadnet_lxmf_siblings(key.clone());
                key
            }
        };
        let propagation_hash = rns_net_destination_hash(&key.identity_hash, "lxmf", "propagation");
        if propagation_hash != destination_hash {
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native LXMF propagation selection canonicalized from {} to lxmf.propagation {}",
                hex_encode(&destination_hash),
                hex_encode(&propagation_hash)
            )));
        }
        Ok(Some(hex_encode(&propagation_hash)))
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    fn emit_rns_net_browser_fetch_trace(
        &self,
        plan: &NativeFetchPlan,
        ready_to_request: bool,
        steps: Vec<PageFetchProbeStep>,
    ) {
        let _ = self
            .event_tx
            .send(RuntimeBusEvent::PageFetchProbe(PageFetchProbeReport {
                backend: RuntimeBackendName::Reticulum,
                url: plan.request.url.clone(),
                destination_hash: Some(plan.request.destination_hash.to_hex_string()),
                path: Some(plan.request.path.clone()),
                execute_request: true,
                ready_to_request,
                steps,
            }));
    }
}

struct NativeAnnounceListenerState {
    #[cfg(not(all(feature = "native-rns-net", any())))]
    storage_path: std::path::PathBuf,
    announces: Arc<Mutex<NativeAnnounceState>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_destination_app_data: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    #[cfg(not(all(feature = "native-rns-net", any())))]
    clean_recent_omenchat_announces: Arc<Mutex<BTreeMap<String, CleanOmenChatAnnounce>>>,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    shutdown: tokio_util::sync::CancellationToken,
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn insert_bounded_destination_cache<V>(cache: &mut BTreeMap<String, V>, key: String, value: V) {
    cache.remove(&key);
    while cache.len() >= CLEAN_DESTINATION_CACHE_MAX_ITEMS {
        cache.pop_first();
    }
    cache.insert(key, value);
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn insert_bounded_destination_app_data(
    cache: &mut BTreeMap<String, Vec<u8>>,
    key: String,
    value: Vec<u8>,
) -> bool {
    if value.len() > CLEAN_DESTINATION_APP_DATA_MAX_ITEM_BYTES {
        return false;
    }
    cache.remove(&key);
    let mut current_bytes = cache.values().map(Vec::len).sum::<usize>();
    while cache.len() >= CLEAN_DESTINATION_CACHE_MAX_ITEMS
        || current_bytes.saturating_add(value.len()) > CLEAN_DESTINATION_APP_DATA_MAX_TOTAL_BYTES
    {
        let Some((_removed_key, removed_value)) = cache.pop_first() else {
            break;
        };
        current_bytes = current_bytes.saturating_sub(removed_value.len());
    }
    cache.insert(key, value);
    true
}

fn announce_lag_diagnostic(skipped: u64) -> RuntimeBusEvent {
    RuntimeBusEvent::Debug(format!(
        "native Reticulum announce stream lagged skipped={skipped}; destination state may be incomplete until subsequent announces"
    ))
}

fn spawn_announce_listener(
    transport: Arc<reticulum_rs::runtime::Transport>,
    state: NativeAnnounceListenerState,
) {
    tokio::spawn(async move {
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let storage_path = state.storage_path;
        let announces = state.announces;
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let clean_destination_identities = state.clean_destination_identities;
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let clean_destination_app_data = state.clean_destination_app_data;
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let clean_recent_omenchat_announces = state.clean_recent_omenchat_announces;
        let event_tx = state.event_tx;
        let shutdown = state.shutdown;
        let mut receiver = transport.recv_announces().await;
        loop {
            let received = tokio::select! {
                _ = shutdown.cancelled() => break,
                received = receiver.recv() => received,
            };
            match received {
                Ok(event) => {
                    let hops = event.hops;
                    let iface = hex_encode(&event.interface);
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    let iface_hash = rns_transport::hash::AddressHash::new_from_slice(
                        event.interface.as_slice(),
                    );
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    let clean_destination_identity = {
                        let destination = event.destination.lock().await;
                        (
                            destination.desc.address_hash.to_hex_string(),
                            destination.identity,
                        )
                    };
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    let clean_destination_app_data_entry = (
                        clean_destination_identity.0.clone(),
                        event.app_data.as_slice().to_vec(),
                    );
                    let payload = payload_from_announce_event(event).await;
                    if !should_emit_directory_announce(&payload) {
                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum ignored unclassified announce destination={} hops={} iface={}",
                            payload.destination_hash, hops, iface
                        )));
                        continue;
                    }
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum classified announce kind={:?} destination={} display={} hops={} iface={}",
                        payload.kind, payload.destination_hash, payload.display_name, hops, iface
                    )));
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    insert_bounded_destination_cache(
                        &mut clean_destination_identities
                            .lock()
                            .expect("native clean destination identity cache lock"),
                        clean_destination_identity.0,
                        clean_destination_identity.1,
                    );
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    {
                        let (destination_hash, app_data) = clean_destination_app_data_entry;
                        let app_data_len = app_data.len();
                        if !insert_bounded_destination_app_data(
                            &mut clean_destination_app_data
                                .lock()
                                .expect("native clean destination app-data cache lock"),
                            destination_hash.clone(),
                            app_data,
                        ) {
                            let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native Reticulum announce app-data rejected destination={} bytes={} limit={}",
                                destination_hash,
                                app_data_len,
                                CLEAN_DESTINATION_APP_DATA_MAX_ITEM_BYTES
                            )));
                        }
                    }
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    if payload.kind == DirectoryKind::OmenChat {
                        insert_bounded_destination_cache(
                            &mut clean_recent_omenchat_announces
                                .lock()
                                .expect("native recent OMENchat announce lock"),
                            payload.destination_hash.clone(),
                            CleanOmenChatAnnounce {
                                observed_at: tokio::time::Instant::now(),
                                hops,
                                iface: iface_hash,
                            },
                        );
                    }
                    announces
                        .lock()
                        .expect("native announce state lock")
                        .ingest(payload.clone());
                    let _ = event_tx.send(RuntimeBusEvent::Announce(payload));
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    {
                        let save_transport = transport.clone();
                        let save_storage_path = storage_path.clone();
                        let save_event_tx = event_tx.clone();
                        tokio::spawn(async move {
                            match save_transport
                                .save_reticulum_path_table(&save_storage_path)
                                .await
                            {
                                Ok(count) => {
                                    let _ = save_event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 saved path table after announce storage={} active_paths={}",
                                        save_storage_path.display(),
                                        count
                                    )));
                                }
                                Err(error) => {
                                    let _ = save_event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 path table save after announce failed storage={} error={}",
                                        save_storage_path.display(),
                                        error
                                    )));
                                }
                            }
                        });
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    let _ = event_tx.send(announce_lag_diagnostic(skipped));
                    continue;
                }
            }
        }
    });
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn parse_restored_destination_identity(
    public_key: &[u8],
    verifying_key: &[u8],
) -> Result<rns_transport::identity::Identity, String> {
    rns_transport::identity::Identity::try_new_from_slices(public_key, verifying_key)
        .map_err(|error| error.to_string())
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn spawn_reticulum_path_table_restore(
    transport: Arc<reticulum_rs::runtime::Transport>,
    storage_path: std::path::PathBuf,
    clean_destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    path_restore_ready: watch::Sender<bool>,
) {
    tokio::spawn(async move {
        match transport
            .restore_reticulum_path_table_report(&storage_path)
            .await
        {
            Ok(report) => {
                if !report.restored_identities.is_empty() {
                    let mut guard = clean_destination_identities
                        .lock()
                        .expect("native clean destination identity cache lock");
                    for restored in &report.restored_identities {
                        match parse_restored_destination_identity(
                            &restored.public_key,
                            &restored.verifying_key,
                        ) {
                            Ok(identity) => insert_bounded_destination_cache(
                                &mut guard,
                                restored.destination.to_hex_string(),
                                identity,
                            ),
                            Err(error) => {
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 ignored invalid restored identity destination={} error={error}",
                                    restored.destination.to_hex_string()
                                )));
                            }
                        }
                    }
                }
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 restored path table storage={} active_paths={} identities={}",
                    storage_path.display(),
                    report.restored_active_paths,
                    report.restored_identities.len()
                )));
            }
            Err(error) => {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 path table restore failed storage={} error={}",
                    storage_path.display(),
                    error
                )));
            }
        }
        let _ = path_restore_ready.send(true);
    });
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn wait_for_reticulum_path_table_restore(ready: &watch::Receiver<bool>) -> AppResult<()> {
    if *ready.borrow() {
        return Ok(());
    }
    let mut ready = ready.clone();
    ready.wait_for(|complete| *complete).await.map_err(|_| {
        AppError::Runtime("Reticulum path-table restore worker stopped early".into())
    })?;
    Ok(())
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn spawn_reticulum_path_table_saver(
    transport: Arc<reticulum_rs::runtime::Transport>,
    storage_path: std::path::PathBuf,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let save_interval = Duration::from_secs(RETICULUM_PATH_TABLE_SAVE_INTERVAL_SECS);
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(save_interval) => {}
        }
        let mut interval = tokio::time::interval(save_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            match transport.save_reticulum_path_table(&storage_path).await {
                Ok(count) => {
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum 0.9 saved path table storage={} active_paths={}",
                        storage_path.display(),
                        count
                    )));
                }
                Err(error) => {
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum 0.9 path table save failed storage={} error={}",
                        storage_path.display(),
                        error
                    )));
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {}
            }
        }
    });
}

#[cfg(all(feature = "native-rns-net", any()))]
fn save_rns_net_known_destinations_snapshot(
    destination_keys: &Arc<Mutex<RnsNetDestinationKeyStore>>,
    path: &Path,
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
) {
    let store = destination_keys
        .lock()
        .expect("native rns-net key store lock")
        .clone();
    if let Err(error) = store.save_recent_known_destinations_file(
        path,
        NATIVE_KNOWN_DESTINATIONS_MAX_AGE_SECS,
        NATIVE_KNOWN_DESTINATIONS_MAX_SAVED,
    ) {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "managed known_destinations save failed: {error:?}"
        )));
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn emit_rns_net_path_update_with_siblings(
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
    destination_keys: &Arc<Mutex<RnsNetDestinationKeyStore>>,
    update: RnsNetPathUpdate,
) {
    let hops = Some(u32::from(update.hops));
    let _ = event_tx.send(RuntimeBusEvent::PathUpdated(PathEvent {
        destination_hash: hex_encode(&update.destination_hash),
        known: true,
        hops,
    }));
    let siblings = destination_keys
        .lock()
        .expect("native rns-net key store lock")
        .sibling_destination_hashes(&update.destination_hash);
    for sibling in siblings {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native rns-net related sibling path evidence exact={} sibling={} hops={}",
            hex_encode(&update.destination_hash),
            hex_encode(&sibling),
            update.hops
        )));
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn extend_with_system_known_destinations(key_store: &mut RnsNetDestinationKeyStore) -> usize {
    let Some(home) = std::env::var_os("HOME") else {
        return 0;
    };
    let path = Path::new(&home)
        .join(".reticulum")
        .join("storage")
        .join("known_destinations");
    let Ok(system_store) = RnsNetDestinationKeyStore::load_recent_known_destinations_file(
        &path,
        NATIVE_KNOWN_DESTINATIONS_MAX_AGE_SECS,
    ) else {
        return 0;
    };
    let count = system_store.len();
    key_store.extend(system_store);
    count
}

fn attach_tcp_client_interfaces(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    interfaces: &[NativeInterfacePlan],
) -> AppResult<Vec<CleanAttachedInterfaceRecord>> {
    let mut attached = Vec::new();
    for interface in interfaces {
        if !interface.enabled || !interface.supported || interface.kind != "tcp_client" {
            continue;
        }
        let Some(endpoint) = interface.endpoint.as_ref() else {
            continue;
        };
        let address = format!("{}:{}", endpoint.host, endpoint.port);
        let manager = transport.iface_manager();
        let mut manager = manager.try_lock().map_err(|_| {
            AppError::Runtime("native Reticulum interface manager lock failed".into())
        })?;
        let (iface_address, ifac_status) = if interface.ifac_configured {
            let client = omen_ifac_tcp::IfacTcpClient::new(
                address.clone(),
                interface.ifac_network_name.clone(),
                interface.ifac_passphrase.clone(),
                16,
            )
            .map_err(|error| AppError::Runtime(format!("IFAC TCP client setup failed: {error}")))?;
            let context = manager.new_context(client);
            let iface_address = *context.channel.address();
            tokio::spawn(omen_ifac_tcp::IfacTcpClient::spawn(context));
            (iface_address, "configured")
        } else {
            let context = manager.new_context(rns_transport::iface::tcp_client::TcpClient::new(
                address.clone(),
            ));
            let iface_address = *context.channel.address();
            tokio::spawn(rns_transport::iface::tcp_client::TcpClient::spawn(context));
            (iface_address, "none")
        };
        let clean = CleanAttachedInterface {
            name: interface.name.clone(),
            address: iface_address,
            ifac_configured: ifac_status == "configured",
        };
        let label = clean.label(&address, ifac_status);
        attached.push(CleanAttachedInterfaceRecord { label, clean });
    }
    Ok(attached)
}

fn native_startup_failure(error: &AppError) -> RuntimeFailure {
    let (category, summary, retryable) = match error {
        AppError::Io(_) => (
            RuntimeFailureCategory::Storage,
            "native runtime could not access required local state",
            true,
        ),
        AppError::Settings(_) => (
            RuntimeFailureCategory::Configuration,
            "native runtime configuration is invalid",
            false,
        ),
        AppError::Unsupported(_) => (
            RuntimeFailureCategory::Interface,
            "a configured native interface is not supported",
            false,
        ),
        AppError::Runtime(_) => (
            RuntimeFailureCategory::Transport,
            "native Reticulum runtime could not start",
            true,
        ),
        AppError::Browser(_) | AppError::Micron(_) => (
            RuntimeFailureCategory::Internal,
            "native runtime startup failed",
            false,
        ),
    };
    RuntimeFailure {
        category,
        summary: summary.into(),
        technical_detail: None,
        retryable,
    }
}

fn capability_record(
    capability: RuntimeCapability,
    availability: RuntimeCapabilityAvailability,
    source: RuntimeCapabilitySource,
    detail: impl Into<String>,
) -> RuntimeCapabilityRecord {
    RuntimeCapabilityRecord {
        capability,
        availability,
        source,
        detail: Some(detail.into()),
    }
}

fn rpc_capability_record(probe: &LxmfSdkRpcProbeSnapshot) -> RuntimeCapabilityRecord {
    if probe.runtime_id.is_some() && probe.detail.is_none() {
        return capability_record(
            RuntimeCapability::RpcBackend,
            RuntimeCapabilityAvailability::Supported,
            RuntimeCapabilitySource::Negotiated,
            "compatible local SDK snapshot received",
        );
    }

    match probe.state.as_str() {
        "missing_endpoint" | "disabled" => capability_record(
            RuntimeCapability::RpcBackend,
            RuntimeCapabilityAvailability::Unsupported,
            RuntimeCapabilitySource::Configured,
            "external RPC mode is not configured",
        ),
        "rejected_endpoint" => capability_record(
            RuntimeCapability::RpcBackend,
            RuntimeCapabilityAvailability::Unsupported,
            RuntimeCapabilitySource::Configured,
            "configured RPC endpoint was rejected by the local-only policy",
        ),
        _ => capability_record(
            RuntimeCapability::RpcBackend,
            RuntimeCapabilityAvailability::Unknown,
            RuntimeCapabilitySource::Configured,
            "configured RPC endpoint has not produced a compatible snapshot",
        ),
    }
}

fn native_capability_records(
    transport_active: bool,
    rpc: RuntimeCapabilityRecord,
    event_stream: RuntimeCapabilityRecord,
) -> Vec<RuntimeCapabilityRecord> {
    let active_availability = if transport_active {
        RuntimeCapabilityAvailability::Supported
    } else {
        RuntimeCapabilityAvailability::Unknown
    };
    let active_detail = if transport_active {
        "active integrated Reticulum adapter"
    } else {
        "integrated adapter is not currently active"
    };
    let mut records = [
        RuntimeCapability::DirectDelivery,
        RuntimeCapability::PropagatedDelivery,
        RuntimeCapability::PropagationStatus,
        RuntimeCapability::Attachments,
        RuntimeCapability::PathMetadata,
        RuntimeCapability::IntegratedBackend,
    ]
    .into_iter()
    .map(|capability| {
        capability_record(
            capability,
            active_availability,
            RuntimeCapabilitySource::Configured,
            active_detail,
        )
    })
    .collect::<Vec<_>>();

    records.push(capability_record(
        RuntimeCapability::OpportunisticDelivery,
        if transport_active && cfg!(feature = "native-lxmf") {
            RuntimeCapabilityAvailability::Supported
        } else {
            RuntimeCapabilityAvailability::Unknown
        },
        RuntimeCapabilitySource::Configured,
        "requires an active LXMF adapter and a cached peer ratchet",
    ));

    for capability in [
        RuntimeCapability::PaperUriDelivery,
        RuntimeCapability::DeliveryCancellation,
        RuntimeCapability::ConversationListing,
        RuntimeCapability::Tickets,
        RuntimeCapability::Stamps,
        RuntimeCapability::SharedInstance,
        RuntimeCapability::InterfaceMutation,
    ] {
        records.push(capability_record(
            capability,
            RuntimeCapabilityAvailability::Unknown,
            RuntimeCapabilitySource::Compiled,
            "runtime negotiation or migration evidence is pending",
        ));
    }

    records.push(capability_record(
        RuntimeCapability::History,
        RuntimeCapabilityAvailability::Supported,
        RuntimeCapabilitySource::Configured,
        "OMENbrowser local history remains authoritative",
    ));
    records.push(event_stream);
    records.push(rpc);
    records
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_rpc_event_capability_record(
    snapshot: &NativeLxmfSdkEventStreamSnapshot,
    endpoint_configured: bool,
) -> RuntimeCapabilityRecord {
    match snapshot.state {
        NativeLxmfSdkEventStreamState::Connected if snapshot.negotiated => capability_record(
            RuntimeCapability::EventStream,
            RuntimeCapabilityAvailability::Supported,
            RuntimeCapabilitySource::Negotiated,
            "bounded SDK/RPC event stream negotiated",
        ),
        NativeLxmfSdkEventStreamState::Unsupported => capability_record(
            RuntimeCapability::EventStream,
            RuntimeCapabilityAvailability::Unsupported,
            RuntimeCapabilitySource::Negotiated,
            "configured SDK/RPC backend did not negotiate async events",
        ),
        NativeLxmfSdkEventStreamState::Disabled if !endpoint_configured => capability_record(
            RuntimeCapability::EventStream,
            RuntimeCapabilityAvailability::Unsupported,
            RuntimeCapabilitySource::Configured,
            "SDK/RPC event endpoint is not configured",
        ),
        _ => capability_record(
            RuntimeCapability::EventStream,
            RuntimeCapabilityAvailability::Unknown,
            RuntimeCapabilitySource::Configured,
            "SDK/RPC event stream is configured but not currently connected",
        ),
    }
}

#[async_trait]
impl NetworkRuntime for NativeNetworkRuntime {
    fn subscribe_events(&self) -> Option<broadcast::Receiver<RuntimeBusEvent>> {
        Some(self.event_tx.subscribe())
    }

    async fn start_runtime(
        &self,
        identity: Option<IdentityProfile>,
        interfaces: Vec<crate::interfaces::ReticulumInterfaceProfile>,
    ) -> AppResult<()> {
        if matches!(
            self.config.instance_mode,
            crate::runtime::native::config::NativeRuntimeMode::External
        ) {
            return Err(AppError::Unsupported(
                "external/shared Reticulum mode is configured but no live shared-instance backend has been negotiated; integrated interface startup is disabled"
                    .into(),
            ));
        }
        let planned_interfaces = plan_interfaces(&interfaces);
        let state = self.state_snapshot();
        if matches!(state.lifecycle, NativeRuntimeLifecycle::Running) {
            if state.active_identity_profile == identity && state.interfaces == planned_interfaces {
                self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::new(
                    RuntimeLifecycleState::Running,
                    RuntimeBackendName::Reticulum,
                ));
                return Ok(());
            }
            return Err(AppError::Runtime(
                "native Reticulum runtime is already running with a different identity or interface plan; stop it before reconfiguration"
                    .into(),
            ));
        }
        let lifecycle = self.lifecycle_snapshot_sync().state;
        if matches!(
            lifecycle,
            RuntimeLifecycleState::Starting | RuntimeLifecycleState::Draining
        ) {
            return Err(AppError::Runtime(format!(
                "native Reticulum runtime cannot start while lifecycle is {lifecycle:?}"
            )));
        }
        self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::new(
            RuntimeLifecycleState::Starting,
            RuntimeBackendName::Reticulum,
        ));
        match self.start(identity, planned_interfaces) {
            Ok(()) => {
                #[cfg(feature = "native-lxmf-sdk")]
                if let Some(endpoint) = self.config.native_lxmf_sdk_rpc_endpoint.clone() {
                    let sender = RpcNativeLxmfSdkSender::new(endpoint.clone());
                    if !matches!(
                        sender.status().state,
                        NativeLxmfSdkSenderState::RejectedEndpoint
                    ) {
                        self.sdk_rpc_event_worker
                            .start(endpoint, self.event_tx.clone());
                    }
                }
                self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::new(
                    RuntimeLifecycleState::Running,
                    RuntimeBackendName::Reticulum,
                ));
                Ok(())
            }
            Err(error) => {
                self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::failed(
                    RuntimeBackendName::Reticulum,
                    native_startup_failure(&error),
                ));
                Err(error)
            }
        }
    }

    async fn stop_runtime(&self) -> AppResult<()> {
        if matches!(
            self.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Stopped
        ) {
            #[cfg(feature = "native-lxmf-sdk")]
            self.sdk_rpc_event_worker.stop().await;
            self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::new(
                RuntimeLifecycleState::Stopped,
                RuntimeBackendName::Reticulum,
            ));
            return Ok(());
        }
        self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::new(
            RuntimeLifecycleState::Draining,
            RuntimeBackendName::Reticulum,
        ));
        self.stop();
        #[cfg(feature = "native-lxmf-sdk")]
        self.sdk_rpc_event_worker.stop().await;
        self.set_lifecycle_snapshot(RuntimeLifecycleSnapshot::new(
            RuntimeLifecycleState::Stopped,
            RuntimeBackendName::Reticulum,
        ));
        Ok(())
    }

    async fn lifecycle_snapshot(&self) -> RuntimeLifecycleSnapshot {
        self.lifecycle_snapshot_sync()
    }

    async fn capability_snapshot(&self) -> RuntimeCapabilitySnapshot {
        let state = self.state_snapshot();
        let transport_active =
            matches!(state.lifecycle, NativeRuntimeLifecycle::Running) && state.transport_started;
        let rpc_probe =
            self.native_lxmf_sdk_rpc_probe()
                .await
                .unwrap_or_else(|_| LxmfSdkRpcProbeSnapshot {
                    endpoint: None,
                    state: "probe_failed".into(),
                    runtime_id: None,
                    active_contract_version: None,
                    event_stream_position: None,
                    config_revision: None,
                    queued_messages: None,
                    in_flight_messages: None,
                    detail: Some("RPC capability probe failed".into()),
                });
        #[cfg(feature = "native-lxmf-sdk")]
        let event_stream = sdk_rpc_event_capability_record(
            &self.sdk_rpc_event_worker.snapshot(),
            self.config.native_lxmf_sdk_rpc_endpoint.is_some(),
        );
        #[cfg(not(feature = "native-lxmf-sdk"))]
        let event_stream = capability_record(
            RuntimeCapability::EventStream,
            RuntimeCapabilityAvailability::Unsupported,
            RuntimeCapabilitySource::Compiled,
            "native LXMF SDK support is not compiled",
        );
        RuntimeCapabilitySnapshot {
            backend: RuntimeBackendName::Reticulum,
            capabilities: native_capability_records(
                transport_active,
                rpc_capability_record(&rpc_probe),
                event_stream,
            ),
        }
    }

    async fn status(&self) -> NetworkStatus {
        let state = self.state_snapshot();
        let connected = matches!(state.lifecycle, NativeRuntimeLifecycle::Running);
        let message = match state.lifecycle {
            NativeRuntimeLifecycle::Stopped => {
                "native Reticulum adapter is configured but stopped".into()
            }
            #[cfg(all(feature = "native-rns-net", any()))]
            NativeRuntimeLifecycle::Running if state.rns_net_started => {
                let local = self
                        .rns_net
                        .lock()
                        .expect("native rns-net lock")
                        .as_ref()
                        .map(|handle| {
                            format!(
                                "local_lxmf_registered={} link_registered={} proof_capable={} announced={} local_lxmf_destination={}",
                                handle.local_lxmf_delivery_registered,
                                handle.local_lxmf_delivery_link_registered,
                                handle.local_lxmf_delivery_proof_capable,
                                handle.local_lxmf_delivery_announced,
                                handle.local_lxmf_delivery_destination_hash.as_deref().unwrap_or("none")
                            )
                        })
                        .unwrap_or_else(|| {
                            "local_lxmf_registered=false link_registered=false proof_capable=false announced=false local_lxmf_destination=none".into()
                        });
                let pending_direct = native_lxmf_pending_direct_summary(&self.pending_lxmf_proofs);
                let ratchet_announces = self
                    .announces
                    .lock()
                    .expect("native announce state lock")
                    .snapshot()
                    .ratchet_announces;
                format!("native rns-net runtime is primary for browser page/path requests; page requests require known destination identity keys; {local}; {pending_direct}; ratchet_announces_observed={ratchet_announces}; opportunistic_lxmf=small_direct_when_cached_ratchet")
            }
            NativeRuntimeLifecycle::Running if state.transport_started => {
                #[cfg(not(all(feature = "native-rns-net", any())))]
                {
                    let identity_path = state
                        .active_identity_profile
                        .as_ref()
                        .map(|profile| profile.path.as_path())
                        .or(self.config.identity_path.as_deref());
                    let local = self.clean_local_lxmf_status_summary(identity_path);
                    format!(
                        "native Reticulum 0.9 transport is running; NomadNet uses current-Python-verified direct requests within packet MDU and request-resource above it; {local}; {}",
                        self.native_lxmf_sdk_rpc_status_summary()
                    )
                }
                #[cfg(all(feature = "native-rns-net", any()))]
                {
                    format!(
                        "native Reticulum transport scaffold is constructed; {}",
                        self.native_lxmf_sdk_rpc_status_summary()
                    )
                }
            }
            NativeRuntimeLifecycle::Running => "native Reticulum identity/config layer is running without an active transport identity; transport requests are unavailable".to_string(),
            NativeRuntimeLifecycle::Failed(reason) => {
                format!("native Reticulum adapter failed: {reason}")
            }
        };

        NetworkStatus {
            connected,
            backend: RuntimeBackendName::Reticulum,
            active_identity: state.active_identity_profile,
            message,
        }
    }

    async fn attach_identity(&self, identity: IdentityProfile) -> AppResult<()> {
        match load_private_identity_file(&identity.path) {
            Ok(summary) => {
                #[cfg(all(feature = "native-rns-net", any()))]
                let transport = None;
                #[cfg(not(all(feature = "native-rns-net", any())))]
                let transport = if matches!(
                    self.state_snapshot().lifecycle,
                    NativeRuntimeLifecycle::Running
                ) {
                    let interfaces = self.state_snapshot().interfaces;
                    Some(self.build_transport(&identity.path, &interfaces)?)
                } else {
                    None
                };
                let mut state = self.state.lock().expect("native runtime state lock");
                state.active_identity = Some(summary);
                state.active_identity_profile = Some(identity);
                if transport.is_some() {
                    state.transport_started = true;
                    self.replace_transport(transport);
                }
                #[cfg(all(feature = "native-rns-net", any()))]
                if matches!(state.lifecycle, NativeRuntimeLifecycle::Running) {
                    let rns_net = self.build_rns_net_request_handle(
                        state.active_identity.as_ref(),
                        state.active_identity_profile.as_ref(),
                        state
                            .active_identity_profile
                            .as_ref()
                            .map(|profile| profile.path.as_path())
                            .or(self.config.identity_path.as_deref()),
                    )?;
                    state.rns_net_started = rns_net.is_some();
                    *self.rns_net.lock().expect("native rns-net lock") = rns_net;
                }
                Ok(())
            }
            Err(error) => {
                self.set_failed("identity load failed");
                Err(AppError::from(error))
            }
        }
    }

    async fn announce_identity(&self) -> AppResult<bool> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let state = self.state_snapshot();
            let Some(identity) = state.active_identity.as_ref() else {
                return Err(AppError::from(NativeRuntimeError::IdentityMissing));
            };
            let identity_path = state
                .active_identity_profile
                .as_ref()
                .map(|profile| profile.path.as_path())
                .or(self.config.identity_path.as_deref())
                .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
            let signing_key =
                load_rns_net_proof_signing_key_file(identity_path).map_err(AppError::from)?;
            let identity_hash = parse_rns_net_destination_hash(&identity.address_hash_hex)?;
            let app_data = local_lxmf_delivery_announce_app_data(
                local_lxmf_display_name(identity, state.active_identity_profile.as_ref()).as_str(),
            )?;
            let mut handle_guard = self.rns_net.lock().expect("native rns-net lock");
            let handle = handle_guard
                .as_mut()
                .ok_or_else(|| AppError::Runtime("native rns-net runtime is not started".into()))?;
            let announcement = handle.client.announce_single_destination(
                identity_hash,
                "lxmf",
                "delivery",
                signing_key,
                Some(app_data.as_slice()),
            )?;
            handle.local_lxmf_delivery_announced = true;
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native rns-net announced local {}.{} destination={}",
                announcement.app_name,
                announcement.aspect,
                hex_encode(&announcement.destination_hash)
            )));
            handle.local_lxmf_delivery_destination_hash =
                Some(hex_encode(&announcement.destination_hash));
            Ok(true)
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let state = self.state_snapshot();
            let Some(identity_summary) = state.active_identity.as_ref() else {
                return Err(AppError::from(NativeRuntimeError::IdentityMissing));
            };
            let handle = self.active_transport()?;
            let app_data = local_lxmf_delivery_announce_app_data(
                local_lxmf_display_name(identity_summary, state.active_identity_profile.as_ref())
                    .as_str(),
            )?;
            let destination = handle.clean_lxmf_delivery_destination.clone();
            handle
                .transport
                .set_destination_announce_app_data(&destination, Some(app_data.clone()))
                .await;
            let (destination_hash, packet) = {
                let mut destination = destination.lock().await;
                let destination_hash = destination.desc.address_hash.to_hex_string();
                let packet = destination
                    .announce(rand_core::OsRng, Some(app_data.as_slice()))
                    .map_err(|error| {
                        AppError::Runtime(format!(
                            "native Reticulum 0.9 LXMF announce failed: {error:?}"
                        ))
                    })?;
                (destination_hash, packet)
            };
            let trace = handle
                .transport
                .send_packet_broadcast_with_trace(packet)
                .await
                .dispatch;
            {
                let mut local = self
                    .clean_local_lxmf
                    .lock()
                    .expect("native clean local lxmf lock");
                local.announced = true;
                local.destination_hash = Some(destination_hash.clone());
            }
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 announced local lxmf.delivery destination={} dispatch=matched:{} sent:{} queued:{} failed:{}",
                destination_hash,
                trace.matched_ifaces,
                trace.sent_ifaces,
                trace.queued_ifaces,
                trace.failed_ifaces
            )));
            Ok(true)
        }
    }

    async fn set_identify_on_connect_destinations(
        &self,
        destination_hashes: BTreeSet<String>,
    ) -> AppResult<()> {
        let count = destination_hashes.len();
        *self
            .identify_on_connect_destinations
            .lock()
            .expect("native identify policy lock") = destination_hashes;
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native browser identify-on-connect policy updated destinations={count}"
        )));
        Ok(())
    }

    async fn fetch_page(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
    ) -> AppResult<BrowserPage> {
        self.fetch_page_with_operation(url, request_data, cancel, None)
            .await
    }

    async fn fetch_page_with_operation(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
        operation_id: Option<String>,
    ) -> AppResult<BrowserPage> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let plan = NativeFetchPlan::new(url, request_data, self.config.request_timeout_secs)
            .map_err(AppError::from)?;
        if !matches!(
            self.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Running
        ) {
            return Err(AppError::Runtime(
                "native Reticulum runtime must be started before fetching pages".into(),
            ));
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let response = self.fetch_page_with_rns_net(&plan, cancel.clone()).await?;
            let mut page = response.into_browser_page(&plan).map_err(AppError::from)?;
            page.metadata.insert(
                "native_request_backend".into(),
                serde_json::Value::String("rns-net".into()),
            );
            return Ok(page);
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let empty_request_data = BTreeMap::new();
            let request_primitive = NativeLinkRequestFrame::build(
                &plan.request.path,
                plan.request
                    .request_data
                    .as_ref()
                    .unwrap_or(&empty_request_data),
                0.0,
            )
            .map_err(AppError::from)
            .map(|frame| {
                if frame.requires_request_resource() {
                    "request-resource"
                } else {
                    "direct-request"
                }
            })?;
            let identify_on_connect = self
                .identify_on_connect_destinations
                .lock()
                .expect("native identify policy lock")
                .contains(&plan.request.destination_hash.to_hex_string());
            let identify_identity = self.clean_stack_identify_identity(identify_on_connect);
            let context = self
                .transport
                .lock()
                .expect("native transport lock")
                .as_ref()
                .map(|handle| {
                    NativePageFetchContext::with_identify_on_connect(
                        handle.transport.clone(),
                        identify_on_connect,
                        identify_identity.clone(),
                        Some(self.event_tx.clone()),
                    )
                    .with_operation_id(operation_id.clone())
                });
            let mut page = self
                .page_transport
                .fetch_page(&plan, context.as_ref(), cancel)
                .await?
                .into_browser_page(&plan)
                .map_err(AppError::from)?;
            page.metadata.insert(
                "native_request_backend".into(),
                serde_json::Value::String("reticulum-transport".into()),
            );
            page.metadata.insert(
                "native_request_primitive".into(),
                serde_json::Value::String(request_primitive.into()),
            );
            Ok(page)
        }
    }

    async fn download_file(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
    ) -> AppResult<DownloadedFile> {
        self.download_file_with_operation(url, downloads_dir, cancel, None)
            .await
    }

    async fn download_file_with_operation(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
        operation_id: Option<String>,
    ) -> AppResult<DownloadedFile> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let plan = NativeFetchPlan::new(url, None, self.config.request_timeout_secs)
            .map_err(AppError::from)?;
        if !matches!(
            self.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Running
        ) {
            return Err(AppError::Runtime(
                "native Reticulum runtime must be started before downloading files".into(),
            ));
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        let response = self.fetch_page_with_rns_net(&plan, cancel.clone()).await?;
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let response = {
            let identify_on_connect = self
                .identify_on_connect_destinations
                .lock()
                .expect("native identify policy lock")
                .contains(&plan.request.destination_hash.to_hex_string());
            let identify_identity = self.clean_stack_identify_identity(identify_on_connect);
            let context = self
                .transport
                .lock()
                .expect("native transport lock")
                .as_ref()
                .map(|handle| {
                    NativePageFetchContext::with_identify_on_connect(
                        handle.transport.clone(),
                        identify_on_connect,
                        identify_identity.clone(),
                        Some(self.event_tx.clone()),
                    )
                    .with_operation_id(operation_id.clone())
                });
            self.page_transport
                .fetch_page(&plan, context.as_ref(), cancel)
                .await?
        };
        let filename = filename_from_native_download_path(&plan.request.path);
        let path = next_available_download_path(downloads_dir, &filename)?;
        let content_type = response
            .content_type
            .unwrap_or_else(|| "application/octet-stream".into());
        atomic_write_new_bounded(path.clone(), response.body).await?;
        Ok(DownloadedFile {
            url: plan.request.url,
            path,
            content_type,
        })
    }

    async fn list_messages(&self) -> AppResult<Vec<MessageSummary>> {
        let mut messages = self
            .inbound_messages
            .lock()
            .expect("native inbound message lock");
        let drained = messages.clone();
        messages.clear();
        Ok(drained)
    }

    async fn lxmf_history_page(&self, request: LxmfHistoryRequest) -> AppResult<LxmfHistoryPage> {
        #[cfg(feature = "native-lxmf-sdk")]
        {
            let endpoint = self
                .config
                .native_lxmf_sdk_rpc_endpoint
                .clone()
                .ok_or_else(|| {
                    AppError::Unsupported(
                        "typed LXMF history requires a configured local SDK/RPC endpoint".into(),
                    )
                })?;
            return RpcNativeLxmfSdkSender::new(endpoint)
                .history_page(request)
                .await;
        }
        #[cfg(not(feature = "native-lxmf-sdk"))]
        {
            let _ = request;
            Err(AppError::Unsupported(
                "typed LXMF history is not compiled in this product profile".into(),
            ))
        }
    }

    async fn send_message(&self, envelope: MessageEnvelope) -> AppResult<MessageSummary> {
        if !matches!(
            self.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Running
        ) {
            return Err(AppError::Runtime(
                "native Reticulum runtime must be started before sending LXMF".into(),
            ));
        }
        if envelope
            .operation
            .as_ref()
            .is_some_and(|operation| operation.remaining_ttl_ms().is_none())
        {
            return Err(AppError::Runtime(
                "LXMF send deadline expired before native runtime admission".into(),
            ));
        }
        #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
        {
            let state = self.state_snapshot();
            state
                .active_identity
                .as_ref()
                .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
            let identity_path = state
                .active_identity_profile
                .as_ref()
                .map(|profile| profile.path.clone())
                .or_else(|| self.config.identity_path.clone())
                .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
            let identity_bytes = crate::identity::read_identity_material(&identity_path)
                .map_err(|_| AppError::from(NativeRuntimeError::IdentityMissing))?;
            if matches!(envelope.delivery_mode, DeliveryMode::Propagated) {
                return self
                    .send_propagated_lxmf_message(envelope, &identity_bytes, None)
                    .await;
            }
            let peer_destination = parse_rns_net_destination_hash(&envelope.peer_hash)?;
            let handle = self
                .rns_net
                .lock()
                .expect("native rns-net lock")
                .clone()
                .ok_or_else(|| AppError::Runtime("native rns-net runtime is not started".into()))?;
            let source_hash = handle.local_lxmf_delivery_destination_hash.clone().ok_or_else(|| {
                AppError::Runtime(
                    "local LXMF delivery destination is not registered; attach/announce identity before sending"
                        .into(),
                )
            })?;
            let mut outbound = crate::runtime::native_lxmf::codec::build_outbound_message(
                &envelope,
                source_hash.as_str(),
            )?;
            let transport_method =
                crate::runtime::native_lxmf::codec::app_transport_method(outbound.delivery.method);
            if !matches!(outbound.delivery.method, lxmf::TransportMethod::Direct) {
                return Err(unsupported("send_message"));
            }
            let destination_key = {
                handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .destination_key(&peer_destination)
            };
            let destination_key = match destination_key {
                Some(key) => key,
                None => {
                    let Some(key) = handle
                        .client
                        .recall_destination_key(peer_destination)
                        .await?
                    else {
                        return Err(AppError::Runtime(
                            "LXMF peer identity is not known; wait for an lxmf.delivery announce or preload known_destinations".into(),
                        ));
                    };
                    handle
                        .destination_keys
                        .lock()
                        .expect("native rns-net key store lock")
                        .ingest_with_nomadnet_lxmf_siblings(key.clone());
                    key
                }
            };
            let direct_stamp_cost = destination_key
                .app_data
                .as_deref()
                .and_then(crate::runtime::native_lxmf::codec::delivery_announce_stamp_cost);
            let direct_stamp = crate::runtime::native_lxmf::codec::apply_direct_stamp_if_needed(
                &mut outbound,
                direct_stamp_cost,
                crate::runtime::native_lxmf::codec::DEFAULT_DIRECT_STAMP_MAX_ATTEMPTS,
            )?;
            let wire_bytes = crate::runtime::native_lxmf::codec::encode_signed_wire_message(
                &outbound,
                &identity_bytes,
            )?;
            let wire_len = wire_bytes.len();
            let has_path = handle.client.has_path(peer_destination).await?;
            let has_cached_ratchet = native_lxmf_cached_ratchet_available(
                self.config.reticulum_config_dir.as_path(),
                &peer_destination,
            );
            if !has_path && has_cached_ratchet && wire_len <= NATIVE_LXMF_OPPORTUNISTIC_PACKET_MDU {
                let packet_hash = handle
                    .client
                    .send_single_packet(
                        destination_key.clone(),
                        "lxmf",
                        "delivery",
                        wire_bytes.clone(),
                    )
                    .await?;
                let packet_hash_hex = hex_encode(&packet_hash);
                let message_id = packet_hash_hex.clone();
                let submitted_at = native_unix_timestamp();
                let propagation_fallback_node = self.selected_propagation_node();
                self.pending_lxmf_proofs
                    .lock()
                    .expect("native LXMF proof map lock")
                    .insert_submission(
                        packet_hash_hex.clone(),
                        envelope.peer_hash.clone(),
                        submitted_at,
                        propagation_fallback_node.clone(),
                    );
                let evidence_detail = format!(
                    "packet_hash:{packet_hash_hex};submitted_at:{submitted_at:.3};proof_state:waiting_for_packet_proof;opportunistic_ratchet:true;direct_representation:single_packet"
                );
                let _ =
                    self.event_tx
                        .send(RuntimeBusEvent::MessageDeliveryUpdated(OutboundStatus {
                            peer_hash: envelope.peer_hash.clone(),
                            message_id: Some(message_id.clone()),
                            delivered: false,
                            failed: false,
                            state: OutboundDeliveryState::SubmittedToRnsNet,
                            evidence: Some(evidence_detail.clone()),
                            rtt: None,
                        }));
                let _ = self.event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
                    LxmfDeliveryEvidence {
                        peer_hash: envelope.peer_hash.clone(),
                        message_id: Some(message_id.clone()),
                        kind: LxmfDeliveryEvidenceKind::PacketSubmitted,
                        detail: Some(evidence_detail.clone()),
                        rtt: None,
                        observed_at: Some(submitted_at),
                    },
                ));
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF opportunistic ratchet packet submitted peer={} message_id={} bytes={} state=submitted_to_rns_net reply_ticket_stamp={} direct_stamp={}",
                    envelope.peer_hash, message_id, wire_len, outbound.reply_ticket_used, direct_stamp.is_some()
                )));
                let mut fields = BTreeMap::from([
                    ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
                    (
                        "native_lxmf_evidence".into(),
                        "opportunistic_ratchet_packet".into(),
                    ),
                    ("native_lxmf_message_id".into(), message_id.clone()),
                    ("native_lxmf_packet_hash".into(), packet_hash_hex),
                    ("native_lxmf_source_hash".into(), source_hash),
                    (
                        "native_lxmf_submitted_at".into(),
                        format!("{submitted_at:.3}"),
                    ),
                    (
                        "native_lxmf_proof_state".into(),
                        "waiting_for_packet_proof".into(),
                    ),
                    (
                        "native_lxmf_receipt_state".into(),
                        "waiting_for_lxmf_delivery_receipt".into(),
                    ),
                    (
                        "native_lxmf_retry_guidance".into(),
                        "LXMF opportunistic packet was submitted using a cached announce ratchet; wait for RNS proof, peer activity, or retry with path/propagation if no evidence appears"
                            .into(),
                    ),
                    (
                        "native_lxmf_opportunistic_state".into(),
                        "submitted".into(),
                    ),
                    (
                        "native_lxmf_opportunistic_reason".into(),
                        "cached announce ratchet available; submitted LXMF wire message as an encrypted single packet"
                            .into(),
                    ),
                ]);
                if let Some(propagation_node) = propagation_fallback_node {
                    fields.insert(
                        "native_lxmf_propagation_fallback_available".into(),
                        "true".into(),
                    );
                    fields.insert("native_lxmf_propagation_node".into(), propagation_node);
                }
                annotate_native_lxmf_stamp_fields(&mut fields, &outbound, direct_stamp.as_ref());
                if outbound.reply_ticket_used {
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF direct ticket stamp applied peer={} message_id={} path=opportunistic_ratchet",
                        envelope.peer_hash, message_id
                    )));
                } else if let Some(stamp) = direct_stamp.as_ref() {
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF direct stamp generated peer={} message_id={} path=opportunistic_ratchet target_cost={} stamp_value={} attempts={}",
                        envelope.peer_hash,
                        message_id,
                        stamp.target_cost,
                        stamp.stamp_value,
                        stamp.attempts
                    )));
                }
                return Ok(MessageSummary {
                    peer_hash: envelope.peer_hash.clone(),
                    peer_label: envelope.peer_hash.chars().take(8).collect(),
                    title: envelope.title,
                    content: envelope.body,
                    timestamp: submitted_at,
                    transport_method,
                    delivered: false,
                    failed: false,
                    incoming: false,
                    unread: false,
                    message_id: Some(message_id),
                    fields,
                    attachments: outbound.attachments,
                });
            }
            if !has_path {
                handle.client.request_path(peer_destination).await?;
                if self.selected_propagation_node().is_some() {
                    return self
                        .send_propagated_lxmf_message(
                            envelope,
                            &identity_bytes,
                            Some(
                                "direct path missing; Python LXMF would keep/retry through router, Rust queued propagated fallback because a propagation node is selected".into(),
                            ),
                        )
                        .await;
                }
                return Err(AppError::Runtime(
                    "LXMF peer path is not known; request_path was queued, retry after discovery"
                        .into(),
                ));
            }
            let message_id = native_lxmf_wire_message_id(&wire_bytes);
            let submitted_at = native_unix_timestamp();
            let link = match handle
                .client
                .establish_link(
                    peer_destination,
                    destination_key.signing_public_key,
                    Duration::from_secs(8),
                    CancellationToken::new(),
                )
                .await
            {
                Ok(link) => link,
                Err(error) if self.selected_propagation_node().is_some() => {
                    return self
                        .send_propagated_lxmf_message(
                            envelope,
                            &identity_bytes,
                            Some(format!(
                                "direct LXMF link setup failed; queued propagated fallback: {error}"
                            )),
                        )
                        .await;
                }
                Err(error) => return Err(error),
            };
            let link_hex = hex_encode(&link.link_id);
            if wire_len <= NATIVE_LXMF_LINK_PACKET_MDU {
                match handle
                    .client
                    .send_on_link(link.link_id, wire_bytes, 0)
                    .await
                {
                    Ok(()) => {}
                    Err(error) if self.selected_propagation_node().is_some() => {
                        let _ = handle.client.teardown_link(link.link_id).await;
                        return self
                            .send_propagated_lxmf_message(
                                envelope,
                                &identity_bytes,
                                Some(format!(
                                    "direct LXMF link-packet send failed; queued propagated fallback: {error}"
                                )),
                            )
                            .await;
                    }
                    Err(error) => return Err(error),
                }
                let evidence_detail = format!(
                    "direct_transfer_state:link_packet_sent;direct_link_id:{link_hex};submitted_at:{submitted_at:.3};receipt_state:direct_link_packet_sent_peer_unconfirmed;delivery_state:peer_delivery_unconfirmed;direct_representation:link_packet"
                );
                let propagation_fallback_node = self.selected_propagation_node();
                self.pending_lxmf_proofs
                    .lock()
                    .expect("native LXMF proof map lock")
                    .insert_submission(
                        message_id.clone(),
                        envelope.peer_hash.clone(),
                        submitted_at,
                        propagation_fallback_node.clone(),
                    );
                let _ =
                    self.event_tx
                        .send(RuntimeBusEvent::MessageDeliveryUpdated(OutboundStatus {
                            peer_hash: envelope.peer_hash.clone(),
                            message_id: Some(message_id.clone()),
                            delivered: false,
                            failed: false,
                            state: OutboundDeliveryState::SubmittedToRnsNet,
                            evidence: Some(evidence_detail.clone()),
                            rtt: None,
                        }));
                let _ = self.event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
                    LxmfDeliveryEvidence {
                        peer_hash: envelope.peer_hash.clone(),
                        message_id: Some(message_id.clone()),
                        kind: LxmfDeliveryEvidenceKind::PacketSubmitted,
                        detail: Some(evidence_detail.clone()),
                        rtt: None,
                        observed_at: Some(native_unix_timestamp()),
                    },
                ));
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF direct link packet sent peer={} message_id={} link_id={} bytes={} state=peer_unconfirmed reply_ticket_stamp={} direct_stamp={}",
                    envelope.peer_hash,
                    message_id,
                    link_hex,
                    wire_len,
                    outbound.reply_ticket_used,
                    direct_stamp.is_some()
                )));
                let mut fields = BTreeMap::from([
                    ("native_lxmf_state".into(), "submitted_unconfirmed".into()),
                    ("native_lxmf_evidence".into(), "direct_link_packet".into()),
                    ("native_lxmf_message_id".into(), message_id.clone()),
                    ("native_lxmf_direct_link_id".into(), link_hex),
                    ("native_lxmf_source_hash".into(), source_hash),
                    (
                        "native_lxmf_submitted_at".into(),
                        format!("{submitted_at:.3}"),
                    ),
                    (
                        "native_lxmf_proof_state".into(),
                        "link_packet_sent".into(),
                    ),
                    (
                        "native_lxmf_receipt_state".into(),
                        "direct_link_packet_sent_peer_unconfirmed".into(),
                    ),
                    (
                        "native_lxmf_retry_guidance".into(),
                        "LXMF direct link packet was sent on an established encrypted link; wait for LXMF router evidence or peer activity before treating it as final"
                            .into(),
                    ),
                    (
                        "native_lxmf_direct_transfer_state".into(),
                        "link_packet_sent".into(),
                    ),
                    (
                        "native_lxmf_opportunistic_state".into(),
                        if has_cached_ratchet {
                            "available_not_used"
                        } else {
                            "unavailable"
                        }
                        .into(),
                    ),
                    (
                        "native_lxmf_opportunistic_reason".into(),
                        if has_cached_ratchet {
                            "cached announce ratchet exists but a direct path was available, so the encrypted link-packet path was used"
                        } else {
                            "no cached announce ratchet is available for this LXMF destination"
                        }
                            .into(),
                    ),
                ]);
                if let Some(propagation_node) = propagation_fallback_node {
                    fields.insert(
                        "native_lxmf_propagation_fallback_available".into(),
                        "true".into(),
                    );
                    fields.insert("native_lxmf_propagation_node".into(), propagation_node);
                }
                annotate_native_lxmf_stamp_fields(&mut fields, &outbound, direct_stamp.as_ref());
                if outbound.reply_ticket_used {
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF direct ticket stamp applied peer={} message_id={} path=link_packet",
                        envelope.peer_hash, message_id
                    )));
                } else if let Some(stamp) = direct_stamp.as_ref() {
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF direct stamp generated peer={} message_id={} path=link_packet target_cost={} stamp_value={} attempts={}",
                        envelope.peer_hash,
                        message_id,
                        stamp.target_cost,
                        stamp.stamp_value,
                        stamp.attempts
                    )));
                }
                return Ok(MessageSummary {
                    peer_hash: envelope.peer_hash.clone(),
                    peer_label: envelope.peer_hash.chars().take(8).collect(),
                    title: envelope.title,
                    content: envelope.body,
                    timestamp: submitted_at,
                    transport_method,
                    delivered: false,
                    failed: false,
                    incoming: false,
                    unread: false,
                    message_id: Some(message_id.clone()),
                    fields,
                    attachments: outbound.attachments,
                });
            }
            match handle
                .client
                .send_resource(link.link_id, wire_bytes, None)
                .await
            {
                Ok(()) => {}
                Err(error) if self.selected_propagation_node().is_some() => {
                    let _ = handle.client.teardown_link(link.link_id).await;
                    return self
                        .send_propagated_lxmf_message(
                            envelope,
                            &identity_bytes,
                            Some(format!(
                                "direct LXMF resource advertisement failed; queued propagated fallback: {error}"
                            )),
                        )
                        .await;
                }
                Err(error) => {
                    let _ = handle.client.teardown_link(link.link_id).await;
                    return Err(error);
                }
            }
            self.pending_direct_lxmf_resources
                .lock()
                .expect("pending direct LXMF resource lock")
                .insert(
                    link_hex.clone(),
                    PendingNativeDirectLxmfResource {
                        peer_hash: envelope.peer_hash.clone(),
                        message_id: message_id.clone(),
                        submitted_at,
                        transfer_state: "resource_advertised".into(),
                    },
                );
            let evidence_detail = format!(
                "direct_transfer_state:resource_advertised;direct_link_id:{link_hex};submitted_at:{submitted_at:.3};proof_state:resource_advertised"
            );
            let _ = self
                .event_tx
                .send(RuntimeBusEvent::MessageDeliveryUpdated(OutboundStatus {
                    peer_hash: envelope.peer_hash.clone(),
                    message_id: Some(message_id.clone()),
                    delivered: false,
                    failed: false,
                    state: OutboundDeliveryState::SubmittedToRnsNet,
                    evidence: Some(evidence_detail.clone()),
                    rtt: None,
                }));
            let _ = self.event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
                LxmfDeliveryEvidence {
                    peer_hash: envelope.peer_hash.clone(),
                    message_id: Some(message_id.clone()),
                    kind: LxmfDeliveryEvidenceKind::PacketSubmitted,
                    detail: Some(evidence_detail.clone()),
                    rtt: None,
                    observed_at: Some(submitted_at),
                },
            ));
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native LXMF direct resource advertised peer={} message_id={} link_id={} state=submitted_to_rns_net reply_ticket_stamp={} direct_stamp={}",
                envelope.peer_hash, message_id, link_hex, outbound.reply_ticket_used, direct_stamp.is_some()
            )));
            let mut fields = BTreeMap::from([
                ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
                ("native_lxmf_evidence".into(), "direct_resource".into()),
                ("native_lxmf_message_id".into(), message_id.clone()),
                ("native_lxmf_direct_link_id".into(), link_hex),
                ("native_lxmf_source_hash".into(), source_hash),
                (
                    "native_lxmf_submitted_at".into(),
                    format!("{submitted_at:.3}"),
                ),
                (
                    "native_lxmf_proof_state".into(),
                    "resource_advertised".into(),
                ),
                (
                    "native_lxmf_receipt_state".into(),
                    "direct_resource_in_progress".into(),
                ),
                (
                    "native_lxmf_retry_guidance".into(),
                    "LXMF direct resource was advertised over an RNS link; wait for resource completion/failure callback"
                        .into(),
                ),
                (
                        "native_lxmf_opportunistic_state".into(),
                        if has_cached_ratchet {
                            "available_not_used"
                        } else {
                            "unavailable"
                        }
                        .into(),
                    ),
                    (
                        "native_lxmf_opportunistic_reason".into(),
                        if has_cached_ratchet {
                            "cached announce ratchet exists but a direct path was available, so the encrypted link-resource path was used"
                        } else {
                            "no cached announce ratchet is available for this LXMF destination"
                        }
                            .into(),
                    ),
                ]);
            if let Some(propagation_node) = self.selected_propagation_node() {
                fields.insert(
                    "native_lxmf_propagation_fallback_available".into(),
                    "true".into(),
                );
                fields.insert("native_lxmf_propagation_node".into(), propagation_node);
            }
            annotate_native_lxmf_stamp_fields(&mut fields, &outbound, direct_stamp.as_ref());
            if outbound.reply_ticket_used {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF direct ticket stamp applied peer={} message_id={} path=resource",
                    envelope.peer_hash, message_id
                )));
            } else if let Some(stamp) = direct_stamp.as_ref() {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF direct stamp generated peer={} message_id={} path=resource target_cost={} stamp_value={} attempts={}",
                    envelope.peer_hash,
                    message_id,
                    stamp.target_cost,
                    stamp.stamp_value,
                    stamp.attempts
                )));
            }
            Ok(MessageSummary {
                peer_hash: envelope.peer_hash.clone(),
                peer_label: envelope.peer_hash.chars().take(8).collect(),
                title: envelope.title,
                content: envelope.body,
                timestamp: submitted_at,
                transport_method,
                delivered: false,
                failed: false,
                incoming: false,
                unread: false,
                message_id: Some(message_id.clone()),
                fields,
                attachments: outbound.attachments,
            })
        }
        #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
        {
            let state = self.state_snapshot();
            if !matches!(state.lifecycle, NativeRuntimeLifecycle::Running) {
                return Err(AppError::Runtime(
                    "native Reticulum runtime must be started before sending LXMF".into(),
                ));
            }
            let _active_identity = state
                .active_identity
                .as_ref()
                .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
            let identity_path = state
                .active_identity_profile
                .as_ref()
                .map(|profile| profile.path.as_path())
                .or(self.config.identity_path.as_deref())
                .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
            let source_hash =
                clean_lxmf_delivery_destination_hash_from_identity_path(identity_path)?;
            #[cfg(feature = "native-lxmf-sdk")]
            {
                let delivery_mode = envelope.delivery_mode.clone();
                let summary_peer_hash = envelope.peer_hash.clone();
                let summary_peer_label = envelope.peer_hash.chars().take(8).collect();
                let summary_title = envelope.title.clone();
                let summary_content = envelope.body.clone();
                let summary_attachments = attachment_summaries_from_paths(&envelope.attachments);
                let (
                    receipt,
                    sender_name,
                    receipt_hash,
                    resource_hash,
                    ticket_state,
                    ticket_included,
                    direct_stamp,
                    propagation_stamp,
                    direct_policy_source,
                ) = if let Some(endpoint) = self
                    .config
                    .native_lxmf_sdk_rpc_endpoint
                    .clone()
                    .filter(|endpoint| !endpoint.trim().is_empty())
                {
                    let sender = RpcNativeLxmfSdkSender::new(endpoint);
                    let status = sender.status();
                    if !matches!(
                        status.state,
                        NativeLxmfSdkSenderState::Ready | NativeLxmfSdkSenderState::Configured
                    ) {
                        return Err(AppError::Unsupported(format!(
                            "clean LXMF SDK/RPC sender is not ready: {}",
                            status.note
                        )));
                    }
                    let plan = build_sdk_send_plan(&envelope, source_hash.as_str(), None);
                    (
                        sender.send_plan(plan).await?,
                        status.name.to_string(),
                        None,
                        None,
                        if envelope.include_ticket {
                            "delegated_external_runtime"
                        } else {
                            NativeLxmfTicketIssueState::NotRequested.as_str()
                        },
                        false,
                        None,
                        None,
                        "delegated_external_runtime",
                    )
                } else {
                    let handle = self.active_transport()?;
                    let identity_bytes = crate::identity::read_identity_material(identity_path)
                        .map_err(|_| AppError::from(NativeRuntimeError::IdentityMissing))?;
                    let submitter = Arc::new(CleanReticulumLxmfWireSubmitter::new(
                        handle.transport.clone(),
                        handle.storage_path.clone(),
                        CleanLxmfSubmitterState {
                            event_tx: self.event_tx.clone(),
                            outbound_propagation_node: self.outbound_propagation_node.clone(),
                            destination_identities: self.clean_destination_identities.clone(),
                            destination_app_data: self.clean_destination_app_data.clone(),
                            pending_lxmf_proofs: self.pending_lxmf_proofs.clone(),
                        },
                        Duration::from_secs(self.config.request_timeout_secs.max(1)),
                    )?);
                    let (direct_stamp_cost, direct_policy_source) = if matches!(
                        envelope.delivery_mode,
                        crate::messaging::DeliveryMode::Direct
                    ) {
                        let destination = parse_transport_destination_hash(&envelope.peer_hash)?;
                        let destination_key = destination.to_hex_string();
                        let mut policy_events = self.event_tx.subscribe();
                        let cached_app_data = self
                            .clean_destination_app_data
                            .lock()
                            .expect("native clean destination app-data cache lock")
                            .get(&destination_key)
                            .cloned();
                        let (app_data, policy_source) = if let Some(app_data) = cached_app_data {
                            (Some(app_data), "cached_authenticated_announce")
                        } else {
                            let identity_wait = clean_wait_for_destination_identity(
                                &handle.transport,
                                &handle.storage_path,
                                destination,
                                Duration::from_secs(self.config.request_timeout_secs.max(1)),
                                CancellationToken::new(),
                                Some(&self.event_tx),
                                Some(&self.clean_destination_identities),
                            );
                            tokio::select! {
                                result = identity_wait => { result?; }
                                _ = handle.shutdown.cancelled() => {
                                    return Err(AppError::Runtime(
                                        "LXMF direct stamp policy discovery cancelled during shutdown".into(),
                                    ));
                                }
                            }
                            let discovered_app_data = self
                                .clean_destination_app_data
                                .lock()
                                .expect("native clean destination app-data cache lock")
                                .get(&destination_key)
                                .cloned();
                            if let Some(app_data) = discovered_app_data {
                                (Some(app_data), "discovered_authenticated_announce")
                            } else {
                                handle
                                    .transport
                                    .request_path(&destination, None, None)
                                    .await;
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF direct stamp policy requested fresh announce destination={destination_key}"
                                )));
                                let refreshed = clean_wait_for_direct_policy_announce(
                                    &self.clean_destination_app_data,
                                    &destination_key,
                                    &mut policy_events,
                                    Duration::from_secs(self.config.request_timeout_secs.max(1))
                                        .min(CLEAN_DIRECT_POLICY_DISCOVERY_MAX_WAIT),
                                    &handle.shutdown,
                                )
                                .await?;
                                let source = if refreshed.is_some() {
                                    "refreshed_authenticated_announce"
                                } else {
                                    "refresh_timeout_unknown"
                                };
                                (refreshed, source)
                            }
                        };
                        let cost = clean_direct_stamp_cost(
                            app_data.as_deref(),
                            envelope.native_reply_ticket.as_ref(),
                            native_unix_timestamp(),
                        )?;
                        (cost, policy_source)
                    } else {
                        (None, "not_applicable_propagated")
                    };
                    let ticket_decision = self
                        .ticket_issuer
                        .prepare(
                            envelope.peer_hash.as_str(),
                            envelope.include_ticket,
                            native_unix_timestamp(),
                        )
                        .await?;
                    let mut effective_envelope = envelope.clone();
                    effective_envelope.include_ticket = ticket_decision.ticket.is_some();
                    let delivery = if let Some(target_cost) = direct_stamp_cost {
                        let shutdown = handle.shutdown.clone();
                        let permit = tokio::select! {
                            permit = DIRECT_STAMP_BLOCKING_GATE.acquire() => permit.map_err(|_| {
                                AppError::Runtime("direct stamp blocking gate closed".into())
                            })?,
                            _ = shutdown.cancelled() => {
                                return Err(AppError::Runtime(
                                    "LXMF direct stamp generation cancelled during shutdown".into(),
                                ));
                            }
                        };
                        let worker_envelope = effective_envelope.clone();
                        let worker_source_hash = source_hash.clone();
                        let worker_identity_bytes = identity_bytes.clone();
                        let worker_issued_ticket = ticket_decision.ticket.clone();
                        let worker_shutdown = shutdown.clone();
                        tokio::task::spawn_blocking(move || {
                            let _permit = permit;
                            build_sdk_wire_delivery_from_envelope_with_policy(
                                &worker_envelope,
                                worker_source_hash.as_str(),
                                worker_identity_bytes.as_slice(),
                                Some(u32::from(target_cost)),
                                worker_issued_ticket.as_ref(),
                                Some(target_cost),
                                || worker_shutdown.is_cancelled(),
                            )
                        })
                        .await
                        .map_err(|error| {
                            AppError::Runtime(format!(
                                "clean LXMF direct stamp task failed: {error}"
                            ))
                        })??
                    } else {
                        build_sdk_wire_delivery_from_envelope_with_issued_ticket(
                            &effective_envelope,
                            source_hash.as_str(),
                            identity_bytes.as_slice(),
                            None,
                            ticket_decision.ticket.as_ref(),
                        )?
                    };
                    let ticket_included = delivery.include_ticket;
                    let direct_stamp = delivery.direct_stamp.clone();
                    let outcome = submitter.submit_wire_async(&delivery).await?;
                    (
                        crate::runtime::native_lxmf::client::NativeLxmfSdkSendReceipt {
                            message_id: Some(delivery.message_id),
                            accepted: true,
                            state: "submitted_to_clean_reticulum".into(),
                        },
                        format!("embedded-clean-reticulum:{}", outcome.route),
                        outcome.receipt_hash,
                        outcome.resource_hash,
                        ticket_decision.state.as_str(),
                        ticket_included,
                        direct_stamp,
                        outcome.propagation_stamp,
                        direct_policy_source,
                    )
                };
                let submitted_at = native_unix_timestamp();
                let (delivered, failed) = clean_lxmf_submission_terminal_flags(receipt.accepted);
                let message_id = receipt
                    .message_id
                    .clone()
                    .unwrap_or_else(|| format!("sdk-rpc-{submitted_at:.3}"));
                let transport_method = match delivery_mode {
                    crate::messaging::DeliveryMode::Direct => TransportMethod::Direct,
                    crate::messaging::DeliveryMode::Propagated => TransportMethod::Propagated,
                };
                let mut fields = BTreeMap::from([
                    ("native_lxmf_state".into(), receipt.state.clone()),
                    ("native_lxmf_evidence".into(), "lxmf_sdk_rpc_sender".into()),
                    ("native_lxmf_message_id".into(), message_id.clone()),
                    ("native_lxmf_source_hash".into(), source_hash.clone()),
                    (
                        "native_lxmf_submitted_at".into(),
                        format!("{submitted_at:.3}"),
                    ),
                    ("native_lxmf_sdk_rpc".into(), sender_name),
                    (
                        "native_lxmf_proof_state".into(),
                        "awaiting_runtime_delivery_evidence".into(),
                    ),
                    (
                        "native_lxmf_receipt_state".into(),
                        "submitted_peer_delivery_unconfirmed".into(),
                    ),
                    (
                        "native_lxmf_retry_guidance".into(),
                        "LXMF message was accepted by the clean Reticulum/LXMF sender; delivery evidence comes from runtime link/resource activity"
                            .into(),
                    ),
                ]);
                if let Some(receipt_hash) = receipt_hash {
                    fields.insert("native_lxmf_packet_hash".into(), receipt_hash);
                    fields.insert(
                        "native_lxmf_proof_state".into(),
                        "waiting_for_transport_receipt".into(),
                    );
                }
                if let Some(resource_hash) = resource_hash {
                    fields.insert("native_lxmf_resource_hash".into(), resource_hash);
                    fields.insert(
                        "native_lxmf_proof_state".into(),
                        "waiting_for_resource_completion".into(),
                    );
                }
                if let Some(propagation_node) = self
                    .outbound_propagation_node
                    .lock()
                    .expect("native propagation node lock")
                    .clone()
                {
                    fields.insert("native_lxmf_propagation_node".into(), propagation_node);
                }
                if envelope.include_ticket {
                    fields.insert("native_lxmf_include_ticket_requested".into(), "true".into());
                }
                if ticket_included {
                    fields.insert("native_lxmf_include_ticket".into(), "true".into());
                    fields.insert("native_lxmf_reply_ticket_offered".into(), "true".into());
                }
                fields.insert("native_lxmf_ticket_issue_state".into(), ticket_state.into());
                fields.insert(
                    "native_lxmf_direct_stamp_policy_source".into(),
                    direct_policy_source.into(),
                );
                if let Some(stamp) = direct_stamp.as_ref() {
                    fields.insert("native_lxmf_stamp_state".into(), "direct_stamp".into());
                    fields.insert(
                        "native_lxmf_direct_stamp_cost".into(),
                        stamp.target_cost.to_string(),
                    );
                    fields.insert(
                        "native_lxmf_direct_stamp_value".into(),
                        stamp.stamp_value.to_string(),
                    );
                    fields.insert(
                        "native_lxmf_direct_stamp_attempts".into(),
                        stamp.attempts.to_string(),
                    );
                } else if envelope.native_reply_ticket.is_some() {
                    fields.insert("native_lxmf_stamp_state".into(), "ticket_stamp".into());
                }
                if let Some(stamp) = propagation_stamp.as_ref() {
                    fields.insert(
                        "native_lxmf_propagation_stamp_cost".into(),
                        stamp.target_cost.to_string(),
                    );
                    fields.insert(
                        "native_lxmf_propagation_stamp_value".into(),
                        stamp.stamp_value.to_string(),
                    );
                    fields.insert(
                        "native_lxmf_propagation_stamp_attempts".into(),
                        stamp.attempts.to_string(),
                    );
                }
                if envelope.native_reply_ticket.is_some() {
                    fields.insert("native_lxmf_reply_ticket".into(), "present".into());
                }
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF clean send accepted peer={} message_id={} state={} delivery={:?} ticket_state={}",
                    summary_peer_hash,
                    message_id,
                    receipt.state,
                    transport_method,
                    ticket_state
                )));
                return Ok(MessageSummary {
                    peer_hash: summary_peer_hash,
                    peer_label: summary_peer_label,
                    title: summary_title,
                    content: summary_content,
                    timestamp: submitted_at,
                    transport_method,
                    delivered,
                    failed,
                    incoming: false,
                    unread: false,
                    message_id: Some(message_id),
                    fields,
                    attachments: summary_attachments,
                });
            }
            #[cfg(not(feature = "native-lxmf-sdk"))]
            {
                let _ = (source_hash, envelope);
                Err(unsupported("send_message"))
            }
        }
        #[cfg(not(feature = "native-lxmf"))]
        {
            let _ = &envelope;
            Err(unsupported("send_message"))
        }
    }

    async fn cancel_lxmf_delivery(&self, message_id: &str) -> AppResult<LxmfCancelOutcome> {
        let message_id = message_id.trim();
        if message_id.is_empty() || message_id.len() > 512 || !message_id.is_ascii() {
            return Err(AppError::Runtime(
                "LXMF cancellation requires a bounded ASCII message identifier".into(),
            ));
        }
        #[cfg(feature = "native-lxmf-sdk")]
        {
            let Some(endpoint) = self
                .config
                .native_lxmf_sdk_rpc_endpoint
                .clone()
                .filter(|endpoint| !endpoint.trim().is_empty())
            else {
                return Ok(LxmfCancelOutcome::Unsupported);
            };
            let sender = RpcNativeLxmfSdkSender::new(endpoint);
            return sender.cancel_delivery(message_id).await;
        }
        #[cfg(not(feature = "native-lxmf-sdk"))]
        {
            Ok(LxmfCancelOutcome::Unsupported)
        }
    }

    async fn create_contact(&self, _peer_hash: &str, _label: &str) -> AppResult<()> {
        Err(unsupported("create_contact"))
    }

    async fn recover_lxmf_correlation(
        &self,
        messages: Vec<MessageSummary>,
    ) -> AppResult<LxmfCorrelationRecovery> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let recovered_direct =
                native_lxmf_recover_direct_correlation(&self.pending_lxmf_proofs, &messages);
            #[cfg(feature = "native-lxmf")]
            let recovered_propagated = native_lxmf_recover_propagated_correlation(
                &self.pending_propagated_lxmf,
                &messages,
            );
            #[cfg(not(feature = "native-lxmf"))]
            let recovered_propagated = 0;
            if recovered_direct > 0 || recovered_propagated > 0 {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF correlation recovered direct={} propagated={}",
                    recovered_direct, recovered_propagated
                )));
            }
            Ok(LxmfCorrelationRecovery {
                direct_recovered: recovered_direct,
                propagated_recovered: recovered_propagated,
            })
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            #[cfg(not(feature = "native-lxmf"))]
            let _ = &messages;
            #[cfg(feature = "native-lxmf")]
            let recovered_direct =
                native_lxmf_recover_direct_correlation(&self.pending_lxmf_proofs, &messages);
            #[cfg(not(feature = "native-lxmf"))]
            let recovered_direct = 0;
            if recovered_direct > 0 {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF clean receipt correlation recovered direct={recovered_direct}"
                )));
            }
            Ok(LxmfCorrelationRecovery {
                direct_recovered: recovered_direct,
                propagated_recovered: 0,
            })
        }
    }

    async fn set_outbound_propagation_node(&self, hash: Option<String>) -> AppResult<()> {
        let hash = match hash {
            Some(hash) => {
                #[cfg(all(feature = "native-rns-net", any()))]
                let parsed = parse_transport_destination_hash(&hash)?;
                #[cfg(all(feature = "native-rns-net", any()))]
                {
                    let mut destination = [0u8; 16];
                    destination.copy_from_slice(parsed.as_slice());
                    let handle = self.rns_net.lock().expect("native rns-net lock").clone();
                    if let Some(handle) = handle {
                        if let Some(canonical) = self
                            .canonical_rns_net_propagation_hash(&handle, destination)
                            .await?
                        {
                            Some(canonical)
                        } else {
                            Some(hash)
                        }
                    } else {
                        Some(hash)
                    }
                }
                #[cfg(not(all(feature = "native-rns-net", any())))]
                {
                    let _ = parse_transport_destination_hash(&hash)?;
                    Some(hash)
                }
            }
            None => None,
        };
        *self
            .outbound_propagation_node
            .lock()
            .expect("native propagation node lock") = hash.clone();
        let _ = self
            .event_tx
            .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                selected: hash.is_some(),
                destination_hash: hash,
                has_path: false,
                known_app_data: false,
                link_state: "not_connected".into(),
                transfer_state: "idle".into(),
            }));
        Ok(())
    }

    async fn get_outbound_propagation_node(&self) -> AppResult<Option<String>> {
        Ok(self
            .outbound_propagation_node
            .lock()
            .expect("native propagation node lock")
            .clone())
    }

    async fn sync_propagation_messages(&self, limit: Option<u32>) -> AppResult<()> {
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let _ = limit;
        let Some(hash) = self
            .outbound_propagation_node
            .lock()
            .expect("native propagation node lock")
            .clone()
        else {
            let _ = self
                .event_tx
                .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                    selected: false,
                    destination_hash: None,
                    has_path: false,
                    known_app_data: false,
                    link_state: "none".into(),
                    transfer_state: "no_propagation_node".into(),
                }));
            return Err(AppError::Runtime(
                "native LXMF propagation sync needs a selected propagation node".into(),
            ));
        };
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let destination = parse_rns_net_destination_hash(&hash)?;
            let Some(handle) = self.rns_net.lock().expect("native rns-net lock").clone() else {
                return Err(AppError::Runtime(
                    "native rns-net runtime is not started; propagation sync cannot run".into(),
                ));
            };
            let has_path = handle.client.has_path(destination).await?;
            if !has_path {
                handle.client.request_path(destination).await?;
            }
            let _ = self
                .event_tx
                .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                    selected: true,
                    destination_hash: Some(hash.clone()),
                    has_path,
                    known_app_data: handle
                        .destination_keys
                        .lock()
                        .expect("native rns-net key store lock")
                        .destination_key(&destination)
                        .as_ref()
                        .is_some_and(rns_net_propagation_app_data_valid),
                    link_state: if has_path {
                        "path_known".into()
                    } else {
                        "path_requested".into()
                    },
                    transfer_state: "router_deferred".into(),
                }));
            if !has_path {
                #[cfg(feature = "native-lxmf")]
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::PathCheck,
                    PropagationSyncEventStatus::Blocked,
                    Some(&hash),
                    "propagation node path is not known; path request queued",
                    [],
                );
                return Ok(());
            }
            #[cfg(feature = "native-lxmf")]
            {
                let Some(propagation_key) = handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .destination_key(&destination)
                else {
                    emit_propagation_sync_event(
                        &self.event_tx,
                        PropagationSyncStage::AppDataCheck,
                        PropagationSyncEventStatus::Blocked,
                        Some(&hash),
                        "propagation node app-data/signing key is missing",
                        [],
                    );
                    return Err(AppError::Runtime(
                        "native LXMF propagation sync needs propagation node app-data/signing key from an announce"
                            .into(),
                    ));
                };
                let state = self.state_snapshot();
                let Some(identity_path) = state
                    .active_identity_profile
                    .as_ref()
                    .map(|profile| profile.path.clone())
                    .or_else(|| self.config.identity_path.clone())
                else {
                    return Err(AppError::from(NativeRuntimeError::IdentityMissing));
                };
                for (status, evidence) in native_lxmf_timeout_stale_propagated(
                    &self.pending_propagated_lxmf,
                    native_unix_timestamp(),
                    NATIVE_LXMF_PROPAGATED_TRANSFER_TIMEOUT_SECS,
                ) {
                    let _ = self
                        .event_tx
                        .send(RuntimeBusEvent::MessageDeliveryUpdated(status));
                    let _ = self
                        .event_tx
                        .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                }
                if let Some(summary) =
                    native_lxmf_active_propagated_outbound_summary(&self.pending_propagated_lxmf)
                {
                    emit_propagation_sync_event(
                        &self.event_tx,
                        PropagationSyncStage::SelectNode,
                        PropagationSyncEventStatus::Blocked,
                        Some(&hash),
                        format!(
                            "skipping LXMF propagation sync while propagated outbound is still active: {summary}"
                        ),
                        [("active_outbound", 1)],
                    );
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation sync skipped while propagated outbound is active: {summary}"
                    )));
                    return Ok(());
                }
                let identity_key =
                    load_rns_net_proof_signing_key_file(&identity_path).map_err(AppError::from)?;
                let identity_bytes = crate::identity::read_identity_material(&identity_path)
                    .map_err(|_| AppError::from(NativeRuntimeError::IdentityMissing))?;
                let local_lxmf_destination_hash =
                    crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(
                        identity_bytes.as_slice(),
                    )?;
                let cancel = CancellationToken::new();
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::LinkEstablish,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "establishing propagation node link",
                    [],
                );
                let link = handle
                    .client
                    .establish_link(
                        destination,
                        propagation_key.signing_public_key,
                        Duration::from_secs(8),
                        cancel.clone(),
                    )
                    .await?;
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::LinkIdentify,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "identifying local identity on propagation link",
                    [],
                );
                if let Err(error) = handle
                    .client
                    .identify_link(link.link_id, identity_key)
                    .await
                {
                    cleanup_native_lxmf_propagation_sync_link(
                        &handle.client,
                        &self.event_tx,
                        &hash,
                        link.link_id,
                        "link identify failed",
                    )
                    .await;
                    return Err(error);
                }
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                        selected: true,
                        destination_hash: Some(hash.clone()),
                        has_path: true,
                        known_app_data: true,
                        link_state: "link_established".into(),
                        transfer_state: "list_request_sent".into(),
                    }));
                let delivered_store = DeliveredTransientIdStore::for_reticulum_storage(
                    &self.config.reticulum_storage_dir,
                );
                let mut delivered_ids = match delivered_store.load_or_default() {
                    Ok(delivered_ids) => delivered_ids,
                    Err(error) => {
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "local delivery cache load failed",
                        )
                        .await;
                        return Err(error);
                    }
                };
                let propagation_stamp_costs = propagation_key
                    .app_data
                    .as_deref()
                    .map(crate::runtime::native_lxmf::codec::propagation_announce_stamp_costs)
                    .filter(|costs| !costs.is_empty())
                    .unwrap_or_else(|| {
                        vec![
                            crate::runtime::native_lxmf::codec::DEFAULT_PROPAGATION_STAMP_TARGET_COST,
                        ]
                    });
                let now = crate::storage::transient_ids::unix_timestamp_secs();
                let pruned = DeliveredTransientIdStore::prune_expired(
                    &mut delivered_ids,
                    now,
                    LXMF_LOCAL_DELIVERY_CACHE_MAX_AGE_SECS,
                );
                if pruned > 0 {
                    if let Err(error) = delivered_store.save(&delivered_ids) {
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "local delivery cache prune save failed",
                        )
                        .await;
                        return Err(error);
                    }
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation sync pruned {pruned} expired local delivery cache entries"
                    )));
                }
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF propagation sync list request propagation_node={} cache_entries={}",
                    hash,
                    delivered_ids.len()
                )));
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::ListRequest,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "requesting available transient ids from propagation node",
                    [("cache_entries", delivered_ids.len())],
                );
                let list_response = match handle
                    .client
                    .send_request_value_and_wait(
                        link.link_id,
                        "/get",
                        &rmpv::Value::Array(vec![rmpv::Value::Nil, rmpv::Value::Nil]),
                        Duration::from_secs(20),
                        cancel.clone(),
                    )
                    .await
                {
                    Ok(response) => response,
                    Err(error) => {
                        emit_propagation_sync_event(
                            &self.event_tx,
                            PropagationSyncStage::ListRequest,
                            PropagationSyncEventStatus::Failed,
                            Some(&hash),
                            format!("list request failed: {error}"),
                            [("cache_entries", delivered_ids.len())],
                        );
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "list request failed",
                        )
                        .await;
                        return Err(error);
                    }
                };
                let available = match native_lxmf_parse_transient_id_list(&list_response.body) {
                    Ok(available) => available,
                    Err(error) => {
                        emit_propagation_sync_event(
                            &self.event_tx,
                            PropagationSyncStage::ListResponse,
                            PropagationSyncEventStatus::Failed,
                            Some(&hash),
                            format!("list response decode failed: {error}"),
                            [],
                        );
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "list response decode failed",
                        )
                        .await;
                        return Err(error);
                    }
                };
                let available_count = available.len();
                let (wants, mut haves) =
                    native_lxmf_select_sync_ids(available, &delivered_ids, limit);
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::ListResponse,
                    PropagationSyncEventStatus::Complete,
                    Some(&hash),
                    "received propagation transient id list",
                    [
                        ("available", available_count),
                        ("cached_haves", haves.len()),
                        ("wants", wants.len()),
                    ],
                );
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF propagation sync list response propagation_node={} available={} cached_haves={} wants={} limit={}",
                    hash,
                    available_count,
                    haves.len(),
                    wants.len(),
                    limit.map_or_else(|| "none".into(), |limit| limit.to_string())
                )));
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                        selected: true,
                        destination_hash: Some(hash.clone()),
                        has_path: true,
                        known_app_data: true,
                        link_state: "link_established".into(),
                        transfer_state: "list_received".into(),
                    }));
                if wants.is_empty() {
                    if !haves.is_empty() {
                        let haves_value = rmpv::Value::Array(
                            haves
                                .iter()
                                .map(|id| rmpv::Value::Binary(id.to_vec()))
                                .collect(),
                        );
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF propagation sync ack-only request propagation_node={} haves={}",
                            hash,
                            haves.len()
                        )));
                        emit_propagation_sync_event(
                            &self.event_tx,
                            PropagationSyncStage::AckRequest,
                            PropagationSyncEventStatus::Started,
                            Some(&hash),
                            "acknowledging cached propagation transient ids",
                            [("haves", haves.len())],
                        );
                        match handle
                            .client
                            .send_request_value_and_wait(
                                link.link_id,
                                "/get",
                                &rmpv::Value::Array(vec![rmpv::Value::Nil, haves_value]),
                                Duration::from_secs(10),
                                CancellationToken::new(),
                            )
                            .await
                        {
                            Ok(_) => {
                                emit_propagation_sync_event(
                                    &self.event_tx,
                                    PropagationSyncStage::AckRequest,
                                    PropagationSyncEventStatus::Complete,
                                    Some(&hash),
                                    "acknowledged cached propagation transient ids",
                                    [("haves", haves.len())],
                                );
                            }
                            Err(error) => {
                                emit_propagation_sync_event(
                                    &self.event_tx,
                                    PropagationSyncStage::AckRequest,
                                    PropagationSyncEventStatus::Failed,
                                    Some(&hash),
                                    format!("ack-only request failed: {error}"),
                                    [("haves", haves.len())],
                                );
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF propagation sync ack-only request failed: {error}"
                                )));
                            }
                        }
                    }
                    let no_payload_evidence = native_lxmf_propagation_no_payload_evidence(
                        &self.pending_propagated_lxmf,
                        &hash,
                        0,
                        0,
                        haves.len(),
                    );
                    for evidence in no_payload_evidence {
                        let _ = self
                            .event_tx
                            .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                    }
                    cleanup_native_lxmf_propagation_sync_link(
                        &handle.client,
                        &self.event_tx,
                        &hash,
                        link.link_id,
                        "no wanted propagation payloads",
                    )
                    .await;
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation sync complete propagation_node={} requested=0 cached_haves={}",
                        hash,
                        haves.len()
                    )));
                    emit_propagation_sync_event(
                        &self.event_tx,
                        PropagationSyncStage::Complete,
                        PropagationSyncEventStatus::Complete,
                        Some(&hash),
                        "propagation sync complete with no new wanted messages",
                        [("requested", 0), ("cached_haves", haves.len())],
                    );
                    let _ =
                        self.event_tx
                            .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                                selected: true,
                                destination_hash: Some(hash),
                                has_path: true,
                                known_app_data: true,
                                link_state: "link_closed".into(),
                                transfer_state: "complete".into(),
                            }));
                    return Ok(());
                }
                let wants_value = rmpv::Value::Array(
                    wants
                        .iter()
                        .map(|id| rmpv::Value::Binary(id.to_vec()))
                        .collect(),
                );
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF propagation sync get request propagation_node={} wants={}",
                    hash,
                    wants.len()
                )));
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::GetRequest,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "requesting wanted propagated LXMF payloads",
                    [("wants", wants.len())],
                );
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                        selected: true,
                        destination_hash: Some(hash.clone()),
                        has_path: true,
                        known_app_data: true,
                        link_state: "link_established".into(),
                        transfer_state: "receiving".into(),
                    }));
                let get_response = match handle
                    .client
                    .send_request_value_and_wait(
                        link.link_id,
                        "/get",
                        &rmpv::Value::Array(vec![
                            wants_value,
                            rmpv::Value::Array(Vec::new()),
                            rmpv::Value::F64(10240.0),
                        ]),
                        Duration::from_secs(45),
                        cancel,
                    )
                    .await
                {
                    Ok(response) => response,
                    Err(error) => {
                        emit_propagation_sync_event(
                            &self.event_tx,
                            PropagationSyncStage::GetRequest,
                            PropagationSyncEventStatus::Failed,
                            Some(&hash),
                            format!("get request failed: {error}"),
                            [("wants", wants.len())],
                        );
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "get request failed",
                        )
                        .await;
                        return Err(error);
                    }
                };
                let payloads = match native_lxmf_parse_propagation_payloads(&get_response.body) {
                    Ok(payloads) => payloads,
                    Err(error) => {
                        emit_propagation_sync_event(
                            &self.event_tx,
                            PropagationSyncStage::GetResponse,
                            PropagationSyncEventStatus::Failed,
                            Some(&hash),
                            format!("get response decode failed: {error}"),
                            [],
                        );
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "get response decode failed",
                        )
                        .await;
                        return Err(error);
                    }
                };
                let payload_count = payloads.len();
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::GetResponse,
                    PropagationSyncEventStatus::Complete,
                    Some(&hash),
                    "received propagation payload response",
                    [("payloads", payload_count)],
                );
                let mut decoded_count = 0usize;
                let mut decode_failed_count = 0usize;
                let mut skipped_count = 0usize;
                let mut deferred_count = 0usize;
                let mut cache_changed = false;
                for payload in payloads {
                    let payload_candidates = native_lxmf_payload_candidates(payload);
                    for lxmf_data in payload_candidates {
                        let mut transient_id = native_lxmf_transient_id(lxmf_data.as_slice());
                        let mut decode_data = lxmf_data;
                        if let Some(stamp) =
                            crate::runtime::native_lxmf::codec::validate_propagation_stamp_any_cost(
                                decode_data.as_slice(),
                                propagation_stamp_costs.as_slice(),
                            )
                        {
                            transient_id = stamp.transient_id;
                            decode_data = stamp.lxm_data;
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF propagation stamp accepted propagation_node={} transient_id={} target_cost={} stamp_value={}",
                                hash,
                                hex_encode(&transient_id),
                                stamp.target_cost,
                                stamp.stamp_value
                            )));
                        }
                        if let Some(payload_destination) =
                            crate::runtime::native_lxmf::codec::propagated_lxmf_destination_hash(
                                decode_data.as_slice(),
                            )
                        {
                            if payload_destination != local_lxmf_destination_hash {
                                skipped_count += 1;
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF propagation sync skipped payload not addressed to local identity transient_id={} destination={} local_destination={}",
                                    hex_encode(&transient_id),
                                    hex_encode(&payload_destination),
                                    hex_encode(&local_lxmf_destination_hash)
                                )));
                                continue;
                            }
                        }
                        let decoded = decode_propagated_lxmf_payload_bounded(
                            decode_data,
                            identity_bytes.clone(),
                            self.config.attachments_dir.clone(),
                            None,
                        )
                        .await?;
                        let decode_data = decoded.bytes;
                        match decoded.message {
                            Ok(message) => {
                                let direct_evidence = native_lxmf_inbound_peer_evidence(
                                    &message,
                                    &self.pending_lxmf_proofs,
                                    &handle.destination_keys,
                                    "propagation sync received LXMF from peer with pending direct outbound",
                                );
                                let propagated_evidence =
                                    native_lxmf_inbound_peer_propagated_evidence(
                                        &message,
                                        &self.pending_propagated_lxmf,
                                        &handle.destination_keys,
                                        "propagation sync received LXMF from peer with pending propagated outbound",
                                    );
                                if !DeliveredTransientIdStore::has_delivered(
                                    &delivered_ids,
                                    &transient_id,
                                ) {
                                    DeliveredTransientIdStore::mark_delivered(
                                        &mut delivered_ids,
                                        &transient_id,
                                        crate::storage::transient_ids::unix_timestamp_secs(),
                                    );
                                    cache_changed = true;
                                }
                                haves.push(transient_id);
                                decoded_count += 1;
                                self.inbound_messages
                                    .lock()
                                    .expect("native inbound message lock")
                                    .push(message.clone());
                                let _ = self
                                    .event_tx
                                    .send(RuntimeBusEvent::MessageReceived(message));
                                for evidence in direct_evidence {
                                    let _ = self
                                        .event_tx
                                        .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                                }
                                for evidence in propagated_evidence {
                                    let _ = self
                                        .event_tx
                                        .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                                }
                            }
                            Err(error) => {
                                decode_failed_count += 1;
                                if crate::runtime::native_lxmf::codec::propagated_lxmf_destination_hash(
                                    decode_data.as_slice(),
                                )
                                .is_some_and(|destination| destination == local_lxmf_destination_hash)
                                {
                                    deferred_count += 1;
                                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native LXMF propagation sync deferred undecryptable local payload transient_id={} destination={} error={error}; leaving on propagation node for retry",
                                        hex_encode(&transient_id),
                                        hex_encode(&local_lxmf_destination_hash)
                                    )));
                                    continue;
                                }
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF propagation sync payload decode failed transient_id={}: {error}",
                                    hex_encode(&transient_id)
                                )));
                            }
                        }
                    }
                }
                if cache_changed {
                    if let Err(error) = delivered_store.save(&delivered_ids) {
                        cleanup_native_lxmf_propagation_sync_link(
                            &handle.client,
                            &self.event_tx,
                            &hash,
                            link.link_id,
                            "local delivery cache save failed",
                        )
                        .await;
                        return Err(error);
                    }
                }
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF propagation sync get response propagation_node={} payloads={} decoded={} failed={} skipped={} deferred={}",
                    hash,
                    payload_count,
                    decoded_count,
                    decode_failed_count,
                    skipped_count,
                    deferred_count
                )));
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::Decode,
                    if decode_failed_count > deferred_count {
                        PropagationSyncEventStatus::Failed
                    } else {
                        PropagationSyncEventStatus::Complete
                    },
                    Some(&hash),
                    "decoded propagation payload candidates",
                    [
                        ("payloads", payload_count),
                        ("decoded", decoded_count),
                        ("failed", decode_failed_count),
                        ("skipped", skipped_count),
                        ("deferred", deferred_count),
                    ],
                );
                if deferred_count > 0 {
                    match self.announce_identity().await {
                        Ok(true) => {
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF propagation sync repair announce sent after deferred local decrypts count={deferred_count}"
                            )));
                        }
                        Ok(false) => {
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF propagation sync repair announce skipped after deferred local decrypts count={deferred_count}"
                            )));
                        }
                        Err(error) => {
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF propagation sync repair announce failed after deferred local decrypts count={deferred_count}: {error}"
                            )));
                        }
                    }
                }
                if !haves.is_empty() {
                    let haves_value = rmpv::Value::Array(
                        haves
                            .iter()
                            .map(|id| rmpv::Value::Binary(id.to_vec()))
                            .collect(),
                    );
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF propagation sync ack request propagation_node={} haves={}",
                        hash,
                        haves.len()
                    )));
                    emit_propagation_sync_event(
                        &self.event_tx,
                        PropagationSyncStage::AckRequest,
                        PropagationSyncEventStatus::Started,
                        Some(&hash),
                        "acknowledging delivered propagation transient ids",
                        [("haves", haves.len())],
                    );
                    let _ =
                        self.event_tx
                            .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                                selected: true,
                                destination_hash: Some(hash.clone()),
                                has_path: true,
                                known_app_data: true,
                                link_state: "link_established".into(),
                                transfer_state: "ack_sent".into(),
                            }));
                    match handle
                        .client
                        .send_request_value_and_wait(
                            link.link_id,
                            "/get",
                            &rmpv::Value::Array(vec![rmpv::Value::Nil, haves_value]),
                            Duration::from_secs(10),
                            CancellationToken::new(),
                        )
                        .await
                    {
                        Ok(_) => {
                            emit_propagation_sync_event(
                                &self.event_tx,
                                PropagationSyncStage::AckRequest,
                                PropagationSyncEventStatus::Complete,
                                Some(&hash),
                                "acknowledged delivered propagation transient ids",
                                [("haves", haves.len())],
                            );
                        }
                        Err(error) => {
                            emit_propagation_sync_event(
                                &self.event_tx,
                                PropagationSyncStage::AckRequest,
                                PropagationSyncEventStatus::Failed,
                                Some(&hash),
                                format!("ack request failed: {error}"),
                                [("haves", haves.len())],
                            );
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF propagation sync ack request failed: {error}"
                            )));
                        }
                    }
                }
                if decoded_count == 0 {
                    let no_payload_evidence = native_lxmf_propagation_no_payload_evidence(
                        &self.pending_propagated_lxmf,
                        &hash,
                        wants.len(),
                        decoded_count,
                        haves.len(),
                    );
                    for evidence in no_payload_evidence {
                        let _ = self
                            .event_tx
                            .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
                    }
                }
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native LXMF propagation sync complete propagation_node={} requested={} decoded={} cached_haves={} failed={} skipped={} deferred={}",
                    hash,
                    wants.len(),
                    decoded_count,
                    haves.len(),
                    decode_failed_count,
                    skipped_count,
                    deferred_count
                )));
                cleanup_native_lxmf_propagation_sync_link(
                    &handle.client,
                    &self.event_tx,
                    &hash,
                    link.link_id,
                    "propagation sync complete",
                )
                .await;
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::Complete,
                    PropagationSyncEventStatus::Complete,
                    Some(&hash),
                    "propagation sync complete",
                    [
                        ("requested", wants.len()),
                        ("decoded", decoded_count),
                        ("haves", haves.len()),
                        ("failed", decode_failed_count),
                        ("skipped", skipped_count),
                        ("deferred", deferred_count),
                    ],
                );
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                        selected: true,
                        destination_hash: Some(hash),
                        has_path: true,
                        known_app_data: true,
                        link_state: "link_closed".into(),
                        transfer_state: "complete".into(),
                    }));
                Ok(())
            }
            #[cfg(not(feature = "native-lxmf"))]
            {
                Err(unsupported("sync_propagation_messages"))
            }
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            #[cfg(not(feature = "native-lxmf"))]
            {
                let _ = parse_transport_destination_hash(&hash)?;
                Err(unsupported("sync_propagation_messages"))
            }
            #[cfg(feature = "native-lxmf")]
            {
                let destination_hash = parse_transport_destination_hash(&hash)?;
                let Some(handle) = self
                    .transport
                    .lock()
                    .expect("native transport lock")
                    .clone()
                else {
                    return Err(AppError::Runtime(
                        "native Reticulum runtime is not started; propagation sync cannot run"
                            .into(),
                    ));
                };
                wait_for_reticulum_path_table_restore(&handle.path_restore_ready).await?;
                let cancel = CancellationToken::new();
                let identity_path = self
                    .state_snapshot()
                    .active_identity_profile
                    .as_ref()
                    .map(|profile| profile.path.clone())
                    .or_else(|| self.config.identity_path.clone())
                    .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
                let identity_bytes = crate::identity::read_identity_material(&identity_path)
                    .map_err(|_| AppError::from(NativeRuntimeError::IdentityMissing))?;
                let identify_identity =
                    load_transport_private_identity_file(&identity_path).map_err(AppError::from)?;
                let local_lxmf_destination_hash_hex =
                    clean_lxmf_delivery_destination_hash_from_identity_path(&identity_path)?;
                let local_lxmf_destination_hash = address_hash_to_link_id(
                    parse_transport_destination_hash(&local_lxmf_destination_hash_hex)?,
                );

                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::PathCheck,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "resolving clean Reticulum propagation node identity",
                    [],
                );
                let propagation_identity = clean_wait_for_destination_identity(
                    &handle.transport,
                    &handle.storage_path,
                    destination_hash,
                    Duration::from_secs(12),
                    cancel.clone(),
                    Some(&self.event_tx),
                    Some(&self.clean_destination_identities),
                )
                .await?;
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                        selected: true,
                        destination_hash: Some(hash.clone()),
                        has_path: true,
                        known_app_data: true,
                        link_state: "path_known".into(),
                        transfer_state: "router_deferred".into(),
                    }));

                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::LinkEstablish,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "establishing clean propagation node link",
                    [],
                );
                let destination = single_output_destination_desc(
                    destination_hash,
                    propagation_identity,
                    "lxmf",
                    "propagation",
                )
                .map_err(AppError::from)?;
                let link = handle.transport.link(destination).await;
                rns_transport::delivery::await_link_activation(
                    &handle.transport,
                    &link,
                    Duration::from_secs(12),
                )
                .await
                .map_err(|error| {
                    AppError::from(NativeRuntimeError::Timeout(format!(
                        "clean LXMF propagation link establishment: {error}"
                    )))
                })?;
                let link_id = *link.lock().await.id();

                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::LinkIdentify,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "identifying local identity on clean propagation link",
                    [],
                );
                send_reticulum_link_identify(
                    &handle.transport,
                    &link,
                    &identify_identity,
                    destination_hash,
                )
                .await?;

                let delivered_store = DeliveredTransientIdStore::for_reticulum_storage(
                    &self.config.reticulum_storage_dir,
                );
                let mut delivered_ids = delivered_store.load_or_default()?;
                let now = crate::storage::transient_ids::unix_timestamp_secs();
                let pruned = DeliveredTransientIdStore::prune_expired(
                    &mut delivered_ids,
                    now,
                    LXMF_LOCAL_DELIVERY_CACHE_MAX_AGE_SECS,
                );
                if pruned > 0 {
                    delivered_store.save(&delivered_ids)?;
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native LXMF clean propagation sync pruned {pruned} expired local delivery cache entries"
                    )));
                }

                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::ListRequest,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "requesting available transient ids from clean propagation node",
                    [("cache_entries", delivered_ids.len())],
                );
                let list_response = clean_send_request_value_and_wait(
                    &handle.transport,
                    &link,
                    link_id,
                    &self.event_tx,
                    CleanRequestWait {
                        path: "/get",
                        value: rmpv::Value::Array(vec![rmpv::Value::Nil, rmpv::Value::Nil]),
                        timeout: Duration::from_secs(20),
                        cancel: cancel.clone(),
                        propagation_node: &hash,
                    },
                )
                .await?;
                let available = native_lxmf_parse_transient_id_list(&list_response.body)?;
                let available_count = available.len();
                let (wants, mut haves) =
                    native_lxmf_select_sync_ids(available, &delivered_ids, limit);
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::ListResponse,
                    PropagationSyncEventStatus::Complete,
                    Some(&hash),
                    "received clean propagation transient id list",
                    [
                        ("available", available_count),
                        ("cached_haves", haves.len()),
                        ("wants", wants.len()),
                    ],
                );

                if wants.is_empty() {
                    if !haves.is_empty() {
                        let haves_value = rmpv::Value::Array(
                            haves
                                .iter()
                                .map(|id| rmpv::Value::Binary(id.to_vec()))
                                .collect(),
                        );
                        emit_propagation_sync_event(
                            &self.event_tx,
                            PropagationSyncStage::AckRequest,
                            PropagationSyncEventStatus::Started,
                            Some(&hash),
                            "acknowledging cached propagation transient ids",
                            [("haves", haves.len())],
                        );
                        let _ = clean_send_request_value_and_wait(
                            &handle.transport,
                            &link,
                            link_id,
                            &self.event_tx,
                            CleanRequestWait {
                                path: "/get",
                                value: rmpv::Value::Array(vec![rmpv::Value::Nil, haves_value]),
                                timeout: Duration::from_secs(10),
                                cancel: CancellationToken::new(),
                                propagation_node: &hash,
                            },
                        )
                        .await;
                    }
                    clean_close_link(&handle.transport, &link).await;
                    emit_propagation_sync_event(
                        &self.event_tx,
                        PropagationSyncStage::Complete,
                        PropagationSyncEventStatus::Complete,
                        Some(&hash),
                        "clean propagation sync complete with no new wanted messages",
                        [("requested", 0), ("cached_haves", haves.len())],
                    );
                    let _ =
                        self.event_tx
                            .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                                selected: true,
                                destination_hash: Some(hash),
                                has_path: true,
                                known_app_data: true,
                                link_state: "link_closed".into(),
                                transfer_state: "complete".into(),
                            }));
                    return Ok(());
                }

                let wants_value = rmpv::Value::Array(
                    wants
                        .iter()
                        .map(|id| rmpv::Value::Binary(id.to_vec()))
                        .collect(),
                );
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::GetRequest,
                    PropagationSyncEventStatus::Started,
                    Some(&hash),
                    "requesting wanted propagated LXMF payloads",
                    [("wants", wants.len())],
                );
                let get_response = clean_send_request_value_and_wait(
                    &handle.transport,
                    &link,
                    link_id,
                    &self.event_tx,
                    CleanRequestWait {
                        path: "/get",
                        value: rmpv::Value::Array(vec![
                            wants_value,
                            rmpv::Value::Array(Vec::new()),
                            rmpv::Value::F64(10240.0),
                        ]),
                        timeout: Duration::from_secs(45),
                        cancel,
                        propagation_node: &hash,
                    },
                )
                .await?;
                let payloads = native_lxmf_parse_propagation_payloads(&get_response.body)?;
                let payload_count = payloads.len();
                let mut decoded_count = 0usize;
                let mut decode_failed_count = 0usize;
                let mut skipped_count = 0usize;
                let mut deferred_count = 0usize;
                let mut duplicate_count = 0usize;
                let mut sender_path_request_count = 0usize;
                let mut cache_changed = false;
                let mut response_transients = BTreeSet::new();
                let mut requested_sender_paths = BTreeSet::new();
                for payload in payloads {
                    let payload_candidates = native_lxmf_payload_candidates(payload);
                    for lxmf_data in payload_candidates {
                        let transient_id = native_lxmf_transient_id(lxmf_data.as_slice());
                        if !clean_propagation_admit_response_transient(
                            &mut response_transients,
                            transient_id,
                        ) {
                            duplicate_count += 1;
                            skipped_count += 1;
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF clean propagation sync suppressed duplicate response transient_id={}",
                                hex_encode(&transient_id)
                            )));
                            continue;
                        }
                        if DeliveredTransientIdStore::has_delivered(&delivered_ids, &transient_id) {
                            if !haves.contains(&transient_id) {
                                haves.push(transient_id);
                            }
                            skipped_count += 1;
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native LXMF clean propagation sync suppressed already delivered transient_id={}",
                                hex_encode(&transient_id)
                            )));
                            continue;
                        }
                        let decode_data = lxmf_data;
                        if let Some(payload_destination) =
                            crate::runtime::native_lxmf::codec::propagated_lxmf_destination_hash(
                                decode_data.as_slice(),
                            )
                        {
                            if payload_destination != local_lxmf_destination_hash {
                                skipped_count += 1;
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF clean propagation sync skipped payload not addressed to local identity transient_id={} destination={} local_destination={}",
                                    hex_encode(&transient_id),
                                    hex_encode(&payload_destination),
                                    hex_encode(&local_lxmf_destination_hash)
                                )));
                                continue;
                            }
                        }
                        let decoded = decode_propagated_lxmf_payload_bounded(
                            decode_data,
                            identity_bytes.clone(),
                            self.config.attachments_dir.clone(),
                            Some(self.clean_destination_identities.clone()),
                        )
                        .await?;
                        let NativeLxmfBlockingDecode {
                            message,
                            bytes: decode_data,
                            unresolved_source_hash,
                        } = decoded;
                        match message {
                            Ok(message) => {
                                if !DeliveredTransientIdStore::has_delivered(
                                    &delivered_ids,
                                    &transient_id,
                                ) {
                                    DeliveredTransientIdStore::mark_delivered(
                                        &mut delivered_ids,
                                        &transient_id,
                                        crate::storage::transient_ids::unix_timestamp_secs(),
                                    );
                                    cache_changed = true;
                                }
                                haves.push(transient_id);
                                decoded_count += 1;
                                self.inbound_messages
                                    .lock()
                                    .expect("native inbound message lock")
                                    .push(message.clone());
                                let _ = self
                                    .event_tx
                                    .send(RuntimeBusEvent::MessageReceived(message));
                            }
                            Err(error) => {
                                decode_failed_count += 1;
                                if crate::runtime::native_lxmf::codec::propagated_lxmf_destination_hash(
                                    decode_data.as_slice(),
                                )
                                .is_some_and(|destination| destination == local_lxmf_destination_hash)
                                {
                                    deferred_count += 1;
                                    if let Some(source_hash) = unresolved_source_hash {
                                        if clean_propagation_admit_sender_path_request(
                                            &mut requested_sender_paths,
                                            source_hash,
                                        ) {
                                            let source_destination =
                                                rns_transport::hash::AddressHash::new(source_hash);
                                            let trace = handle
                                                .transport
                                                .request_path(
                                                    &source_destination,
                                                    None,
                                                    None,
                                                )
                                                .await;
                                            sender_path_request_count += 1;
                                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(
                                                format!(
                                                    "native LXMF clean propagation sync requested missing sender path source={} dispatch=matched:{} sent:{} queued:{} failed:{}",
                                                    hex_encode(&source_hash),
                                                    trace.matched_ifaces,
                                                    trace.sent_ifaces,
                                                    trace.queued_ifaces,
                                                    trace.failed_ifaces
                                                ),
                                            ));
                                        }
                                    }
                                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native LXMF clean propagation sync deferred unauthenticated or undecryptable local payload transient_id={} destination={} error={error}",
                                        hex_encode(&transient_id),
                                        hex_encode(&local_lxmf_destination_hash)
                                    )));
                                    continue;
                                }
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF clean propagation sync payload decode failed transient_id={}: {error}",
                                    hex_encode(&transient_id)
                                )));
                            }
                        }
                    }
                }
                if cache_changed {
                    delivered_store.save(&delivered_ids)?;
                }
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::Decode,
                    if decode_failed_count > deferred_count {
                        PropagationSyncEventStatus::Failed
                    } else {
                        PropagationSyncEventStatus::Complete
                    },
                    Some(&hash),
                    "decoded clean propagation payload candidates",
                    [
                        ("payloads", payload_count),
                        ("decoded", decoded_count),
                        ("failed", decode_failed_count),
                        ("skipped", skipped_count),
                        ("deferred", deferred_count),
                        ("duplicates", duplicate_count),
                        ("sender_path_requests", sender_path_request_count),
                    ],
                );
                if !haves.is_empty() {
                    let haves_value = rmpv::Value::Array(
                        haves
                            .iter()
                            .map(|id| rmpv::Value::Binary(id.to_vec()))
                            .collect(),
                    );
                    emit_propagation_sync_event(
                        &self.event_tx,
                        PropagationSyncStage::AckRequest,
                        PropagationSyncEventStatus::Started,
                        Some(&hash),
                        "acknowledging delivered propagation transient ids",
                        [("haves", haves.len())],
                    );
                    let _ = clean_send_request_value_and_wait(
                        &handle.transport,
                        &link,
                        link_id,
                        &self.event_tx,
                        CleanRequestWait {
                            path: "/get",
                            value: rmpv::Value::Array(vec![rmpv::Value::Nil, haves_value]),
                            timeout: Duration::from_secs(10),
                            cancel: CancellationToken::new(),
                            propagation_node: &hash,
                        },
                    )
                    .await;
                }
                clean_close_link(&handle.transport, &link).await;
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::Complete,
                    PropagationSyncEventStatus::Complete,
                    Some(&hash),
                    "clean propagation sync complete",
                    [
                        ("requested", wants.len()),
                        ("decoded", decoded_count),
                        ("haves", haves.len()),
                        ("failed", decode_failed_count),
                        ("skipped", skipped_count),
                        ("deferred", deferred_count),
                    ],
                );
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::PropagationStatus(PropagationStatus {
                        selected: true,
                        destination_hash: Some(hash),
                        has_path: true,
                        known_app_data: true,
                        link_state: "link_closed".into(),
                        transfer_state: "complete".into(),
                    }));
                Ok(())
            }
        }
    }

    async fn request_path(
        &self,
        destination_hash: &str,
        reason: &str,
        sibling_aspects: bool,
    ) -> AppResult<bool> {
        #[cfg(all(feature = "native-rns-net", any()))]
        let rns_net_handle = {
            let guard = self.rns_net.lock().expect("native rns-net lock");
            guard.clone()
        };
        #[cfg(all(feature = "native-rns-net", any()))]
        if let Some(handle) = rns_net_handle {
            let destination = parse_rns_net_destination_hash(destination_hash)?;
            if handle.client.has_path(destination).await? {
                return Ok(true);
            }
            if sibling_aspects {
                self.request_rns_net_path_with_siblings(&handle, destination, reason)
                    .await?;
            } else {
                handle.client.request_path(destination).await?;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native rns-net requested path destination={} reason={} sibling_requests=0",
                    hex_encode(&destination),
                    reason
                )));
            }
            return Ok(true);
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let _ = (reason, sibling_aspects);
        let destination = parse_transport_destination_hash(destination_hash)?;
        let transport = self.active_transport()?;
        if transport.transport.knows_destination(&destination).await {
            return Ok(true);
        }
        let trace = transport
            .transport
            .request_path(&destination, None, None)
            .await;
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 requested path destination={} reason={} dispatch=matched:{} sent:{} queued:{} failed:{}",
            destination_hash,
            reason,
            trace.matched_ifaces,
            trace.sent_ifaces,
            trace.queued_ifaces,
            trace.failed_ifaces
        )));
        Ok(true)
    }

    async fn warm_paths(
        &self,
        hashes: &[String],
        max_requests: u32,
        _cooldown_secs: u64,
    ) -> AppResult<u32> {
        #[cfg(all(feature = "native-rns-net", any()))]
        let rns_net_handle = {
            let guard = self.rns_net.lock().expect("native rns-net lock");
            guard.clone()
        };
        #[cfg(all(feature = "native-rns-net", any()))]
        if let Some(handle) = rns_net_handle {
            let mut requested = 0;
            for hash in hashes.iter().take(max_requests as usize) {
                let destination = parse_rns_net_destination_hash(hash)?;
                if !handle.client.has_path(destination).await? {
                    handle.client.request_path(destination).await?;
                }
                requested += 1;
            }
            return Ok(requested);
        }
        let transport = self.active_transport()?;
        let mut requested = 0;
        for hash in hashes.iter().take(max_requests as usize) {
            let destination = parse_transport_destination_hash(hash)?;
            if !transport.transport.knows_destination(&destination).await {
                transport
                    .transport
                    .request_path(&destination, None, None)
                    .await;
            }
            requested += 1;
        }
        Ok(requested)
    }

    async fn preload_known_destinations(&self, path: &Path) -> AppResult<usize> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let rns_net_handle = {
                let guard = self.rns_net.lock().expect("native rns-net lock");
                guard.clone()
            };
            let Some(handle) = rns_net_handle else {
                return Err(AppError::Runtime(
                    "native rns-net runtime is not started; known destinations cannot be preloaded"
                        .into(),
                ));
            };
            let loaded = RnsNetDestinationKeyStore::load_known_destinations_file(path)
                .map_err(AppError::from)?;
            let count = loaded.len();
            handle
                .destination_keys
                .lock()
                .expect("native rns-net key store lock")
                .extend(loaded);
            return Ok(count);
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let _ = path;
            Err(unsupported("preload_known_destinations"))
        }
    }

    async fn interface_stats(&self) -> AppResult<InterfaceStats> {
        let state = self.state_snapshot();
        #[cfg(all(feature = "native-rns-net", any()))]
        let rns_net_started = state.rns_net_started;
        #[cfg(all(feature = "native-rns-net", any()))]
        let live_rns_net_stats = self
            .rns_net
            .lock()
            .expect("native rns-net lock")
            .as_ref()
            .map(|handle| handle.client.clone());
        #[cfg(all(feature = "native-rns-net", any()))]
        let live_rns_net_stats = if let Some(client) = live_rns_net_stats {
            client.interface_stats().await.ok()
        } else {
            None
        };
        let (transport_detail, attached_interfaces) = {
            let guard = self.transport.lock().expect("native transport lock");
            guard
                .as_ref()
                .map(|transport| {
                    let _ = Arc::strong_count(&transport.transport);
                    (
                        format!(
                            "transport constructed [{} TCP client interfaces attached]",
                            transport.interface_count
                        ),
                        transport.attached_interfaces.clone(),
                    )
                })
                .unwrap_or_else(|| (String::new(), Vec::new()))
        };
        let attached_samples = attached_interfaces.clone();
        #[cfg(all(feature = "native-rns-net", any()))]
        let live_tcp_interfaces = live_rns_net_stats
            .as_ref()
            .map(|stats| {
                stats
                    .interfaces
                    .iter()
                    .filter(|interface| {
                        interface
                            .interface_type
                            .to_ascii_lowercase()
                            .contains("tcpclient")
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        #[cfg(all(feature = "native-rns-net", any()))]
        let mut live_tcp_index = 0usize;
        let samples = state
            .interfaces
            .iter()
            .map(|plan| {
                let endpoint = plan
                    .endpoint
                    .as_ref()
                    .map(|endpoint| format!("{}:{}", endpoint.host, endpoint.port));
                #[cfg(all(feature = "native-rns-net", any()))]
                let matched_live_interface = live_rns_net_stats.as_ref().and_then(|stats| {
                    find_live_rns_net_interface(stats, plan, endpoint.as_deref())
                });
                #[cfg(all(feature = "native-rns-net", any()))]
                let ordered_live_interface =
                    if plan.enabled && plan.supported && plan.kind == "tcp_client" {
                        let interface = live_tcp_interfaces.get(live_tcp_index).copied();
                        live_tcp_index = live_tcp_index.saturating_add(1);
                        interface
                    } else {
                        None
                    };
                #[cfg(all(feature = "native-rns-net", any()))]
                let live_interface =
                    select_live_rns_net_interface(matched_live_interface, ordered_live_interface);
                let attached = attached_samples.iter().any(|line| {
                    line.contains(&plan.name)
                        || endpoint
                            .as_ref()
                            .is_some_and(|endpoint| line.contains(endpoint))
                }) || {
                    #[cfg(all(feature = "native-rns-net", any()))]
                    {
                        live_interface.is_some_and(|interface| interface.status)
                    }
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    {
                        false
                    }
                };
                let detail = if attached {
                    #[cfg(all(feature = "native-rns-net", any()))]
                    if let Some(interface) = live_interface {
                        Some(format_live_rns_net_interface_detail(interface))
                    } else {
                        attached_samples
                            .iter()
                            .find(|line| {
                                line.contains(&plan.name)
                                    || endpoint
                                        .as_ref()
                                        .is_some_and(|endpoint| line.contains(endpoint))
                            })
                            .cloned()
                    }
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    attached_samples
                        .iter()
                        .find(|line| {
                            line.contains(&plan.name)
                                || endpoint
                                    .as_ref()
                                    .is_some_and(|endpoint| line.contains(endpoint))
                        })
                        .cloned()
                } else {
                    #[cfg(all(feature = "native-rns-net", any()))]
                    if let Some(interface) = live_interface {
                        Some(format_live_rns_net_interface_detail(interface))
                    } else {
                        plan.reason.clone()
                    }
                    #[cfg(not(all(feature = "native-rns-net", any())))]
                    plan.reason.clone()
                };
                InterfaceSample {
                    profile_id: plan.profile_id.clone(),
                    name: plan.name.clone(),
                    kind: plan.kind.clone(),
                    state: if !plan.enabled {
                        InterfaceSampleState::Disabled
                    } else if !plan.supported {
                        InterfaceSampleState::Unsupported
                    } else if attached {
                        InterfaceSampleState::Attached
                    } else {
                        InterfaceSampleState::Configured
                    },
                    enabled: plan.enabled,
                    supported: plan.supported,
                    attached,
                    endpoint,
                    detail,
                }
            })
            .collect::<Vec<_>>();
        let mut interfaces = state
            .interfaces
            .iter()
            .map(|plan| {
                let support = if plan.supported {
                    "supported"
                } else {
                    "unsupported"
                };
                let enabled = if plan.enabled { "enabled" } else { "disabled" };
                format!("{} [{} {support} {enabled}]", plan.name, plan.kind)
            })
            .collect::<Vec<_>>();
        interfaces.extend(
            attached_interfaces
                .into_iter()
                .map(|interface| format!("attached {interface}")),
        );
        #[cfg(all(feature = "native-rns-net", any()))]
        if let Some(stats) = &live_rns_net_stats {
            interfaces.extend(
                stats
                    .interfaces
                    .iter()
                    .map(format_live_rns_net_interface_detail),
            );
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        if rns_net_started {
            interfaces.push("attached rns-net primary runtime".into());
            if let Some(identity_path) = state
                .active_identity_profile
                .as_ref()
                .map(|profile| profile.path.as_path())
                .or(self.config.identity_path.as_deref())
            {
                let aligned = managed_rns_net_identity_config_matches(
                    &self.config.reticulum_config_dir,
                    identity_path,
                )
                .unwrap_or(false);
                interfaces.push(format!(
                    "rns-net identity config {}",
                    if aligned {
                        "aligned with active identity"
                    } else {
                        "not aligned with active identity"
                    }
                ));
            }
        }

        Ok(InterfaceStats {
            available: matches!(state.lifecycle, NativeRuntimeLifecycle::Running),
            #[cfg(all(feature = "native-rns-net", any()))]
            reason: Some(if rns_net_started {
                "native rns-net runtime is primary for browser page/path requests".into()
            } else if transport_detail.is_empty() {
                "native Reticulum transport/interface runtime is not fully wired yet".into()
            } else {
                transport_detail
            }),
            #[cfg(not(all(feature = "native-rns-net", any())))]
            reason: Some(if transport_detail.is_empty() {
                "native Reticulum transport/interface runtime is not fully wired yet".into()
            } else {
                transport_detail
            }),
            interfaces,
            samples,
        })
    }

    async fn network_snapshot(&self) -> AppResult<NetworkSnapshot> {
        let state = self.announces.lock().expect("native announce state lock");
        let snapshot = state.snapshot();
        Ok(NetworkSnapshot {
            announce_counts: snapshot.announce_counts,
            pending_announces: snapshot.pending_announces,
            known_destinations: snapshot.known_destinations,
            ratchet_announces: snapshot.ratchet_announces,
            path_table_available: false,
            path_table_count: 0,
            request_failure_metrics_available: false,
            request_failures: 0,
            active_propagation_node: None,
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
        Ok(self
            .announces
            .lock()
            .expect("native announce state lock")
            .candidates(limit))
    }

    async fn inspect_destination(
        &self,
        destination_hash: &str,
        include_propagation_usable: bool,
    ) -> AppResult<DestinationInspection> {
        let Ok(destination) = parse_transport_destination_hash(destination_hash) else {
            return Ok(DestinationInspection {
                destination_hash: destination_hash.into(),
                valid_length: false,
                has_path: false,
                hops: None,
                first_hop_timeout: None,
                known_identity: false,
                known_app_data: false,
                propagation_usable: None,
            });
        };
        #[cfg(all(feature = "native-rns-net", any()))]
        let rns_net_handle = {
            let guard = self.rns_net.lock().expect("native rns-net lock");
            guard.clone()
        };
        #[cfg(all(feature = "native-rns-net", any()))]
        if let Some(handle) = rns_net_handle {
            let destination_bytes = parse_rns_net_destination_hash(destination_hash)?;
            let has_path = handle.client.has_path(destination_bytes).await?;
            let hops = handle.client.hops_to(destination_bytes).await?;
            let destination_key = handle
                .destination_keys
                .lock()
                .expect("native rns-net key store lock")
                .destination_key(&destination_bytes);
            let known_in_preload_store = destination_key.is_some();
            let known_identity = known_in_preload_store
                || handle
                    .client
                    .recall_destination_key(destination_bytes)
                    .await?
                    .is_some();
            let known_app_data = destination_key
                .as_ref()
                .is_some_and(|key| key.app_data.as_ref().is_some_and(|data| !data.is_empty()));
            let propagation_usable = destination_key
                .as_ref()
                .filter(|key| rns_net_announce_kind(key) == DirectoryKind::Propagation)
                .map(rns_net_propagation_app_data_valid);
            return Ok(DestinationInspection {
                destination_hash: destination_hash.into(),
                valid_length: true,
                has_path,
                hops: hops.map(u32::from),
                first_hop_timeout: None,
                known_identity,
                known_app_data,
                propagation_usable,
            });
        }
        let transport = self.active_transport()?;
        let path_status = transport.transport.path_status(&destination).await;
        let known_identity = transport
            .transport
            .destination_identity(&destination)
            .await
            .is_some();
        let app_data = self
            .clean_destination_app_data
            .lock()
            .expect("native clean destination app-data cache lock")
            .get(&destination.to_hex_string())
            .cloned();
        let known_app_data = app_data.as_ref().is_some_and(|data| !data.is_empty());
        let propagation_usable = include_propagation_usable
            .then(|| app_data.as_deref().map(clean_propagation_app_data_valid))
            .flatten();
        Ok(DestinationInspection {
            destination_hash: destination_hash.into(),
            valid_length: true,
            has_path: path_status.path_found,
            hops: path_status.hops.map(u32::from),
            first_hop_timeout: None,
            known_identity,
            known_app_data,
            propagation_usable,
        })
    }

    async fn probe_page_fetch(
        &self,
        url: &str,
        execute_request: bool,
    ) -> AppResult<PageFetchProbeReport> {
        let backend = RuntimeBackendName::Reticulum;
        let mut report = PageFetchProbeReport {
            backend,
            url: url.into(),
            destination_hash: None,
            path: None,
            execute_request,
            ready_to_request: false,
            steps: Vec::new(),
        };
        let plan = match NativeFetchPlan::new(url, None, self.config.request_timeout_secs) {
            Ok(plan) => {
                report.destination_hash = Some(plan.request.destination_hash.to_hex_string());
                report.path = Some(plan.request.path.clone());
                report.steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::AddressParse,
                        "parsed native NomadNet destination and path",
                    )
                    .with_trace("destination", plan.request.destination_hash.to_hex_string())
                    .with_trace("path", plan.request.path.clone()),
                );
                plan
            }
            Err(error) => {
                report.steps.push(PageFetchProbeStep::failed(
                    PageFetchProbeStage::AddressParse,
                    AppError::from(error).to_string(),
                ));
                return Ok(report);
            }
        };
        #[cfg(not(all(feature = "native-rns-net", any())))]
        let _ = &plan;
        if !matches!(
            self.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Running
        ) {
            report.steps.push(PageFetchProbeStep::failed(
                PageFetchProbeStage::RuntimeSetup,
                "native Reticulum runtime is not running",
            ));
            return Ok(report);
        }
        report.steps.push(
            PageFetchProbeStep::ok(
                PageFetchProbeStage::RuntimeSetup,
                "native Reticulum runtime is running",
            )
            .with_trace("backend", "reticulum"),
        );

        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let Some(handle) = self.rns_net.lock().expect("native rns-net lock").clone() else {
                report.steps.push(
                    PageFetchProbeStep::failed(
                        PageFetchProbeStage::RuntimeSetup,
                        "rns-net runtime is not started",
                    )
                    .with_trace("feature", "native-rns-net"),
                );
                return Ok(report);
            };
            let mut destination_hash = [0u8; 16];
            destination_hash.copy_from_slice(plan.request.destination_hash.as_slice());
            let mut signing_public_key = handle
                .destination_keys
                .lock()
                .expect("native rns-net key store lock")
                .signing_public_key(&destination_hash);
            if signing_public_key.is_none() {
                if let Some(key) = handle
                    .client
                    .recall_destination_key(destination_hash)
                    .await?
                {
                    signing_public_key = Some(key.signing_public_key);
                    handle
                        .destination_keys
                        .lock()
                        .expect("native rns-net key store lock")
                        .ingest_with_nomadnet_lxmf_siblings(key);
                    report.steps.push(
                        PageFetchProbeStep::ok(
                            PageFetchProbeStage::DestinationIdentity,
                            "recalled destination signing key from rns-net known destinations",
                        )
                        .with_trace("destination", plan.request.destination_hash.to_hex_string())
                        .with_trace("source", "rns-net recall_identity"),
                    );
                }
            } else {
                report.steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::DestinationIdentity,
                        "destination signing key is already known",
                    )
                    .with_trace("destination", plan.request.destination_hash.to_hex_string())
                    .with_trace("source", "runtime key store"),
                );
            }
            let Some(signing_public_key) = signing_public_key else {
                report.steps.push(PageFetchProbeStep::failed(
                    PageFetchProbeStage::DestinationIdentity,
                    "destination signing key is not known; wait for announce or preload known_destinations",
                )
                .with_trace("destination", plan.request.destination_hash.to_hex_string()));
                return Ok(report);
            };

            let has_path = handle.client.has_path(destination_hash).await?;
            if has_path {
                let hops = handle.client.hops_to(destination_hash).await?;
                report.steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::PathDiscovery,
                        match hops {
                            Some(hops) => format!("path is known ({hops} hops)"),
                            None => "path is known".into(),
                        },
                    )
                    .with_trace("destination", plan.request.destination_hash.to_hex_string())
                    .with_trace(
                        "hops",
                        hops.map(|hops| hops.to_string())
                            .unwrap_or_else(|| "unknown".into()),
                    ),
                );
            } else {
                handle.client.request_path(destination_hash).await?;
                report.steps.push(PageFetchProbeStep::failed(
                    PageFetchProbeStage::PathDiscovery,
                    "path is not known; queued request_path and retry is required after discovery",
                )
                .with_trace("destination", plan.request.destination_hash.to_hex_string())
                .with_trace("request_path", "queued"));
                return Ok(report);
            }

            report.ready_to_request = true;
            if !execute_request {
                report.steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::LinkSetup,
                        "not executed; probe stopped before live Link.request",
                    )
                    .with_trace("execute_request", "false"),
                );
                return Ok(report);
            }

            let keys = RnsNetDestinationKeys::from_fetch_plan(&plan, signing_public_key)
                .map_err(AppError::from)?;
            let (steps, _response) = handle
                .client
                .fetch_page_observed(&plan, keys, None, CancellationToken::new())
                .await;
            report.steps.extend(steps);
            return Ok(report);
        }

        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let _ = execute_request;
            let capabilities = native_reticulum09_capability_report();
            report.steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::DestinationIdentity,
                    "clean reticulum-rs 0.9 stack can inspect destination identity through transport",
                )
                .with_trace("stack", capabilities.stack)
                .with_trace("transport", capabilities.transport_crate)
                .with_trace("lxmf", capabilities.lxmf_crate),
            );
            report.steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::LinkSetup,
                    "reticulum-rs-transport exposes destination link lifecycle primitives",
                )
                .with_trace("capability", "destination-links"),
            );
            report.steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::RequestSend,
                    "clean reticulum-rs 0.9 page probe uses direct request-context packets within packet MDU and bounded request-resource above it",
                )
                .with_trace("capability", "link-request-receipt")
                .with_trace("state", "available")
                .with_trace("remaining_parity_gaps", capabilities.blockers.join("; "))
                .with_trace("next", capabilities.recommended_next_step),
            );
            Ok(report)
        }
    }

    async fn probe_lxmf_delivery(
        &self,
        peer_hash: &str,
        execute_send: bool,
    ) -> AppResult<LxmfDeliveryProbeReport> {
        let mut report = LxmfDeliveryProbeReport {
            backend: RuntimeBackendName::Reticulum,
            peer_hash: peer_hash.into(),
            execute_send,
            ready_to_send: false,
            steps: Vec::new(),
        };

        if !matches!(
            self.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Running
        ) {
            report.steps.push(LxmfDeliveryProbeStep::failed(
                LxmfDeliveryProbeStage::RuntimeSetup,
                "native Reticulum runtime is not running",
            ));
            return Ok(report);
        }
        report.steps.push(
            LxmfDeliveryProbeStep::ok(
                LxmfDeliveryProbeStage::RuntimeSetup,
                "native Reticulum runtime is running",
            )
            .with_trace("backend", "reticulum"),
        );

        #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
        {
            let state = self.state_snapshot();
            if state.active_identity.is_none() {
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "active OMEN identity is not attached",
                ));
                return Ok(report);
            }
            let Some(identity_path) = state
                .active_identity_profile
                .as_ref()
                .map(|profile| profile.path.clone())
                .or_else(|| self.config.identity_path.clone())
            else {
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "active identity path is not configured",
                ));
                return Ok(report);
            };
            let Ok(identity_bytes) = crate::identity::read_identity_material(&identity_path) else {
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "active identity file could not be read",
                ));
                return Ok(report);
            };
            let Some(handle) = self.rns_net.lock().expect("native rns-net lock").clone() else {
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::RuntimeSetup,
                    "rns-net runtime is not started",
                ));
                return Ok(report);
            };
            let Some(source_hash) = handle.local_lxmf_delivery_destination_hash.clone() else {
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "local LXMF delivery destination is not registered; attach/announce identity before sending",
                ));
                return Ok(report);
            };
            report.steps.push(
                LxmfDeliveryProbeStep::ok(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "active source identity and local LXMF delivery destination are available",
                )
                .with_trace("source_hash", source_hash.clone()),
            );

            let peer_destination = match parse_rns_net_destination_hash(peer_hash) {
                Ok(destination) => destination,
                Err(error) => {
                    report.steps.push(LxmfDeliveryProbeStep::failed(
                        LxmfDeliveryProbeStage::PeerIdentity,
                        error.to_string(),
                    ));
                    return Ok(report);
                }
            };
            let destination_key = {
                handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .destination_key(&peer_destination)
            };
            let destination_key = match destination_key {
                Some(key) => {
                    report.steps.push(
                        LxmfDeliveryProbeStep::ok(
                            LxmfDeliveryProbeStage::PeerIdentity,
                            "LXMF peer destination key is already known",
                        )
                        .with_trace("peer_hash", peer_hash),
                    );
                    key
                }
                None => {
                    if let Some(key) = handle
                        .client
                        .recall_destination_key(peer_destination)
                        .await?
                    {
                        handle
                            .destination_keys
                            .lock()
                            .expect("native rns-net key store lock")
                            .ingest_with_nomadnet_lxmf_siblings(key.clone());
                        report.steps.push(
                            LxmfDeliveryProbeStep::ok(
                                LxmfDeliveryProbeStage::PeerIdentity,
                                "recalled LXMF peer destination key from rns-net known destinations",
                            )
                            .with_trace("peer_hash", peer_hash)
                            .with_trace("source", "rns-net recall_identity"),
                        );
                        key
                    } else {
                        report.steps.push(LxmfDeliveryProbeStep::failed(
                            LxmfDeliveryProbeStage::PeerIdentity,
                            "LXMF peer identity is not known; wait for lxmf.delivery announce or preload known_destinations",
                        )
                        .with_trace("peer_hash", peer_hash));
                        return Ok(report);
                    }
                }
            };

            let has_path = handle.client.has_path(peer_destination).await?;
            if has_path {
                let hops = handle.client.hops_to(peer_destination).await?;
                report.steps.push(
                    LxmfDeliveryProbeStep::ok(
                        LxmfDeliveryProbeStage::PathDiscovery,
                        match hops {
                            Some(hops) => format!("LXMF peer path is known ({hops} hops)"),
                            None => "LXMF peer path is known".into(),
                        },
                    )
                    .with_trace(
                        "hops",
                        hops.map(|hops| hops.to_string())
                            .unwrap_or_else(|| "unknown".into()),
                    ),
                );
            } else {
                handle.client.request_path(peer_destination).await?;
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::PathDiscovery,
                    "LXMF peer path is not known; queued request_path and retry is required after discovery",
                )
                .with_trace("peer_hash", peer_hash)
                .with_trace("request_path", "queued"));
                return Ok(report);
            }

            report.steps.push(
                LxmfDeliveryProbeStep::ok(
                    LxmfDeliveryProbeStage::PeerIdentity,
                    "native opportunistic LXMF is available for small direct messages when a cached announce ratchet exists and no direct path is known",
                )
                .with_trace("opportunistic_lxmf", "small_direct_when_cached_ratchet")
                .with_trace("ratchet_source", "rns-net storage/ratchets"),
            );

            let envelope = MessageEnvelope {
                peer_hash: peer_hash.into(),
                title: "OMENbrowser_rs LXMF smoke test".into(),
                body: "OMENbrowser_rs native LXMF delivery smoke test".into(),
                delivery_mode: DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                attachments: Vec::new(),
            };
            let outbound = crate::runtime::native_lxmf::codec::build_outbound_message(
                &envelope,
                &source_hash,
            )?;
            let wire_bytes = crate::runtime::native_lxmf::codec::encode_signed_wire_message(
                &outbound,
                &identity_bytes,
            )?;
            report.steps.push(
                LxmfDeliveryProbeStep::ok(
                    LxmfDeliveryProbeStage::PacketBuild,
                    "LXMF wire message can be built and signed",
                )
                .with_trace("wire_bytes", wire_bytes.len().to_string()),
            );
            report.ready_to_send = true;

            if execute_send {
                let message_id = native_lxmf_wire_message_id(&wire_bytes);
                let link = handle
                    .client
                    .establish_link(
                        peer_destination,
                        destination_key.signing_public_key,
                        Duration::from_secs(8),
                        CancellationToken::new(),
                    )
                    .await?;
                let link_hex = hex_encode(&link.link_id);
                match handle
                    .client
                    .send_resource(link.link_id, wire_bytes, None)
                    .await
                {
                    Ok(()) => {}
                    Err(error) => {
                        let _ = handle.client.teardown_link(link.link_id).await;
                        return Err(error);
                    }
                }
                report.steps.push(
                    LxmfDeliveryProbeStep::ok(
                        LxmfDeliveryProbeStage::SendPacket,
                        "LXMF smoke message was advertised as a direct RNS link resource",
                    )
                    .with_trace("message_id", message_id)
                    .with_trace("direct_link_id", link_hex),
                );
            } else {
                report.steps.push(
                    LxmfDeliveryProbeStep::ok(
                        LxmfDeliveryProbeStage::SendPacket,
                        "not executed; probe stopped before advertising an LXMF direct resource",
                    )
                    .with_trace("execute_send", "false"),
                );
            }
            Ok(report)
        }

        #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
        {
            let state = self.state_snapshot();
            let Some(identity_path) = state
                .active_identity_profile
                .as_ref()
                .map(|profile| profile.path.as_path())
                .or(self.config.identity_path.as_deref())
            else {
                report.steps.push(LxmfDeliveryProbeStep::failed(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "active identity path is not configured",
                ));
                return Ok(report);
            };
            let source_hash =
                match clean_lxmf_delivery_destination_hash_from_identity_path(identity_path) {
                    Ok(hash) => hash,
                    Err(error) => {
                        report.steps.push(LxmfDeliveryProbeStep::failed(
                            LxmfDeliveryProbeStage::SourceIdentity,
                            error.to_string(),
                        ));
                        return Ok(report);
                    }
                };
            report.steps.push(
                LxmfDeliveryProbeStep::ok(
                    LxmfDeliveryProbeStage::SourceIdentity,
                    "active source identity and clean local LXMF delivery destination are available",
                )
                .with_trace("source_hash", source_hash),
            );

            let peer_destination = match parse_transport_destination_hash(peer_hash) {
                Ok(destination) => destination,
                Err(error) => {
                    report.steps.push(LxmfDeliveryProbeStep::failed(
                        LxmfDeliveryProbeStage::PeerIdentity,
                        error.to_string(),
                    ));
                    return Ok(report);
                }
            };
            let handle = match self.active_transport() {
                Ok(handle) => handle,
                Err(error) => {
                    report.steps.push(LxmfDeliveryProbeStep::failed(
                        LxmfDeliveryProbeStage::RuntimeSetup,
                        error.to_string(),
                    ));
                    return Ok(report);
                }
            };
            let cancel = CancellationToken::new();
            match clean_wait_for_destination_identity(
                &handle.transport,
                &handle.storage_path,
                peer_destination,
                Duration::from_secs(self.config.request_timeout_secs.max(1)),
                cancel.clone(),
                Some(&self.event_tx),
                Some(&self.clean_destination_identities),
            )
            .await
            {
                Ok(_) => {
                    report.steps.push(
                        LxmfDeliveryProbeStep::ok(
                            LxmfDeliveryProbeStage::PeerIdentity,
                            "clean LXMF peer destination identity is known",
                        )
                        .with_trace("peer_hash", peer_hash),
                    );
                }
                Err(error) => {
                    report.steps.push(LxmfDeliveryProbeStep::failed(
                        LxmfDeliveryProbeStage::PeerIdentity,
                        error.to_string(),
                    ));
                    return Ok(report);
                }
            }
            match clean_wait_for_destination_path(
                &handle.transport,
                peer_destination,
                Duration::from_secs(self.config.request_timeout_secs.max(1)),
                cancel,
                Some(&self.event_tx),
                None,
            )
            .await
            {
                Ok(_) => {
                    report.steps.push(
                        LxmfDeliveryProbeStep::ok(
                            LxmfDeliveryProbeStage::PathDiscovery,
                            "clean LXMF peer path is available",
                        )
                        .with_trace("peer_hash", peer_hash),
                    );
                }
                Err(error) => {
                    report.steps.push(LxmfDeliveryProbeStep::failed(
                        LxmfDeliveryProbeStage::PathDiscovery,
                        error.to_string(),
                    ));
                    return Ok(report);
                }
            }
            report.steps.push(LxmfDeliveryProbeStep::ok(
                LxmfDeliveryProbeStage::PacketBuild,
                "clean LXMF SDK bridge can build signed wire messages for direct send",
            ));
            report.ready_to_send = true;

            if execute_send {
                let summary = self
                    .send_message(MessageEnvelope {
                        peer_hash: peer_hash.into(),
                        title: "OMENbrowser_rs LXMF smoke test".into(),
                        body: "OMENbrowser_rs native LXMF delivery smoke test".into(),
                        delivery_mode: crate::messaging::DeliveryMode::Direct,
                        include_ticket: false,
                        native_reply_ticket: None,
                        operation: Some(crate::messaging::OutboundOperationIdentity::generate()),
                        attachments: Vec::new(),
                    })
                    .await?;
                report.steps.push(
                    LxmfDeliveryProbeStep::ok(
                        LxmfDeliveryProbeStage::SendPacket,
                        "clean LXMF smoke message was submitted through reticulum-rs-transport",
                    )
                    .with_trace(
                        "message_id",
                        summary.message_id.unwrap_or_else(|| "unknown".into()),
                    ),
                );
            } else {
                report.steps.push(
                    LxmfDeliveryProbeStep::ok(
                        LxmfDeliveryProbeStage::SendPacket,
                        "not executed; probe stopped before clean LXMF direct submit",
                    )
                    .with_trace("execute_send", "false"),
                );
            }
            Ok(report)
        }

        #[cfg(not(feature = "native-lxmf"))]
        {
            let _ = peer_hash;
            let _ = execute_send;
            report.steps.push(LxmfDeliveryProbeStep::failed(
                LxmfDeliveryProbeStage::RuntimeSetup,
                "native-lxmf feature is required for LXMF delivery probes",
            ));
            Ok(report)
        }
    }

    async fn propagation_status(&self) -> AppResult<PropagationStatus> {
        #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
        {
            for (status, evidence) in native_lxmf_timeout_stale_propagated(
                &self.pending_propagated_lxmf,
                native_unix_timestamp(),
                NATIVE_LXMF_PROPAGATED_TRANSFER_TIMEOUT_SECS,
            ) {
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::MessageDeliveryUpdated(status));
                let _ = self
                    .event_tx
                    .send(RuntimeBusEvent::LxmfDeliveryEvidence(evidence));
            }
            native_lxmf_prune_terminal_propagated(
                &self.pending_propagated_lxmf,
                native_unix_timestamp(),
                NATIVE_LXMF_PROPAGATED_TERMINAL_RETENTION_SECS,
            );
        }
        let destination_hash = self
            .outbound_propagation_node
            .lock()
            .expect("native propagation node lock")
            .clone();
        #[cfg(all(feature = "native-rns-net", any()))]
        if let Some(hash) = destination_hash {
            let destination = parse_rns_net_destination_hash(&hash)?;
            let handle = self.rns_net.lock().expect("native rns-net lock").clone();
            if let Some(handle) = handle {
                let has_path = handle.client.has_path(destination).await?;
                let known_app_data = handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .destination_key(&destination)
                    .as_ref()
                    .is_some_and(rns_net_propagation_app_data_valid);
                #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
                let transfer_state = if native_lxmf_active_propagated_outbound_summary(
                    &self.pending_propagated_lxmf,
                )
                .is_some()
                {
                    "active_outbound"
                } else if has_path && known_app_data {
                    "ready"
                } else {
                    "router_deferred"
                };
                #[cfg(not(all(feature = "native-lxmf", feature = "native-rns-net")))]
                let transfer_state = if has_path && known_app_data {
                    "ready"
                } else {
                    "router_deferred"
                };
                return Ok(PropagationStatus {
                    selected: true,
                    destination_hash: Some(hash),
                    has_path,
                    known_app_data,
                    link_state: if has_path { "path_known" } else { "no_path" }.into(),
                    transfer_state: transfer_state.into(),
                });
            }
            return Ok(PropagationStatus {
                selected: true,
                destination_hash: Some(hash),
                has_path: false,
                known_app_data: false,
                link_state: "runtime_not_started".into(),
                transfer_state: "router_deferred".into(),
            });
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        if let Some(hash) = destination_hash {
            let destination = parse_transport_destination_hash(&hash)?;
            let Some(handle) = self
                .transport
                .lock()
                .expect("native transport lock")
                .clone()
            else {
                return Ok(PropagationStatus {
                    selected: true,
                    destination_hash: Some(hash),
                    has_path: false,
                    known_app_data: false,
                    link_state: "runtime_not_started".into(),
                    transfer_state: "router_deferred".into(),
                });
            };
            let status = handle.transport.path_status(&destination).await;
            let known_app_data = self
                .clean_destination_app_data
                .lock()
                .expect("native clean destination app-data cache lock")
                .get(&destination.to_hex_string())
                .is_some_and(|data| clean_propagation_app_data_valid(data));
            let transfer_state = if status.path_found && known_app_data {
                "ready"
            } else {
                "router_deferred"
            };
            return Ok(PropagationStatus {
                selected: true,
                destination_hash: Some(hash),
                has_path: status.path_found,
                known_app_data,
                link_state: if status.path_found {
                    "path_known".into()
                } else {
                    "no_path".into()
                },
                transfer_state: transfer_state.into(),
            });
        }
        Ok(PropagationStatus {
            selected: false,
            destination_hash: None,
            has_path: false,
            known_app_data: false,
            link_state: "unsupported".into(),
            transfer_state: "idle".into(),
        })
    }

    async fn propagation_debug_snapshot(
        &self,
        message_id: Option<String>,
    ) -> AppResult<PropagationDebugSnapshot> {
        let status = self.propagation_status().await?;
        let mut snapshot = PropagationDebugSnapshot {
            selected_node: status.destination_hash,
            router_state: status.transfer_state,
            pending_outbound_ids: Vec::new(),
            pending_deferred_ids: Vec::new(),
            failed_outbound_ids: Vec::new(),
            link_state: status.link_state,
            message: None,
        };

        #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
        {
            native_lxmf_prune_terminal_propagated(
                &self.pending_propagated_lxmf,
                native_unix_timestamp(),
                NATIVE_LXMF_PROPAGATED_TERMINAL_RETENTION_SECS,
            );
            let pending = self
                .pending_propagated_lxmf
                .lock()
                .expect("pending propagated LXMF lock");
            for (message_id, pending) in pending.iter() {
                match pending.transfer_state.as_str() {
                    "resource_failed" | "resource_advertise_failed" | "link_timeout" => {
                        snapshot.failed_outbound_ids.push(message_id.clone());
                    }
                    "resource_completed" => snapshot.pending_outbound_ids.push(message_id.clone()),
                    _ => snapshot.pending_deferred_ids.push(message_id.clone()),
                }
            }
            if let Some(message_id) = message_id {
                snapshot.message =
                    pending
                        .get(&message_id)
                        .map(|pending| PropagationMessageSnapshot {
                            origin: "pending_deferred".into(),
                            message_id,
                            state: Some(pending.transfer_state.clone()),
                            desired_method: Some("propagated".into()),
                            method: Some("propagated".into()),
                        representation: Some(format!(
                            "peer={} node={} submitted_at={:.3} has_path={} known_app_data={} link_id={} peer_activity_observed={}",
                            pending.peer_hash,
                            pending.propagation_node,
                            pending.submitted_at,
                            pending.has_path,
                            pending.known_app_data,
                            pending.link_id.as_deref().unwrap_or("-"),
                            pending.peer_activity_observed_at.is_some()
                        )),
                            progress: None,
                        });
            }
        }

        #[cfg(any(not(feature = "native-lxmf"), not(feature = "native-rns-net")))]
        if let Some(message_id) = message_id {
            snapshot.message = Some(PropagationMessageSnapshot {
                origin: "-".into(),
                message_id,
                state: None,
                desired_method: None,
                method: None,
                representation: None,
                progress: None,
            });
        }

        Ok(snapshot)
    }

    async fn native_lxmf_sdk_rpc_probe(&self) -> AppResult<LxmfSdkRpcProbeSnapshot> {
        #[cfg(feature = "native-lxmf-sdk")]
        {
            let Some(endpoint) = self.config.native_lxmf_sdk_rpc_endpoint.clone() else {
                return Ok(LxmfSdkRpcProbeSnapshot {
                    endpoint: None,
                    state: "missing_endpoint".into(),
                    runtime_id: None,
                    active_contract_version: None,
                    event_stream_position: None,
                    config_revision: None,
                    queued_messages: None,
                    in_flight_messages: None,
                    detail: Some(
                        "native LXMF SDK/RPC endpoint is not configured in settings".into(),
                    ),
                });
            };

            let sender = RpcNativeLxmfSdkSender::new(endpoint);
            match sender.probe().await {
                Ok(probe) => Ok(LxmfSdkRpcProbeSnapshot {
                    endpoint: Some(probe.endpoint),
                    state: probe.state,
                    runtime_id: Some(probe.runtime_id),
                    active_contract_version: Some(probe.active_contract_version),
                    event_stream_position: Some(probe.event_stream_position),
                    config_revision: Some(probe.config_revision),
                    queued_messages: Some(probe.queued_messages),
                    in_flight_messages: Some(probe.in_flight_messages),
                    detail: None,
                }),
                Err(_error) => {
                    let rejected = matches!(
                        sender.status().state,
                        NativeLxmfSdkSenderState::RejectedEndpoint
                    );
                    Ok(LxmfSdkRpcProbeSnapshot {
                        endpoint: sender.diagnostic_endpoint(),
                        state: if rejected {
                            "rejected_endpoint"
                        } else {
                            "unreachable"
                        }
                        .into(),
                        runtime_id: None,
                        active_contract_version: None,
                        event_stream_position: None,
                        config_revision: None,
                        queued_messages: None,
                        in_flight_messages: None,
                        detail: Some(if rejected {
                            "RPC endpoint was rejected before connection because only local-trusted transports are currently configured"
                            .into()
                        } else {
                            "validated local RPC endpoint did not return a compatible SDK snapshot"
                                .into()
                        }),
                    })
                }
            }
        }
        #[cfg(not(feature = "native-lxmf-sdk"))]
        {
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
    }

    async fn open_omenchat_link(
        &self,
        destination_hash: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<OmenChatLinkOpened> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            if !matches!(
                self.state_snapshot().lifecycle,
                NativeRuntimeLifecycle::Running
            ) {
                return Err(AppError::Runtime(
                    "native Reticulum runtime is not running".into(),
                ));
            }
            let destination = parse_rns_net_destination_hash(destination_hash)?;
            let (client, signing_public_key, identify_key) = {
                let handle_guard = self.rns_net.lock().expect("native rns-net lock");
                let handle = handle_guard.as_ref().ok_or_else(|| {
                    AppError::Runtime("native rns-net runtime is not started".into())
                })?;
                let signing_public_key = handle
                    .destination_keys
                    .lock()
                    .expect("native rns-net key store lock")
                    .signing_public_key(&destination);
                (
                    handle.client.clone(),
                    signing_public_key,
                    handle.local_identity_private_key,
                )
            };
            let mut signing_public_key = signing_public_key;
            if signing_public_key.is_none() {
                if let Some(key) = client.recall_destination_key(destination).await? {
                    signing_public_key = Some(key.signing_public_key);
                    if let Some(handle) = self.rns_net.lock().expect("native rns-net lock").as_ref()
                    {
                        handle
                            .destination_keys
                            .lock()
                            .expect("native rns-net key store lock")
                            .ingest_with_nomadnet_lxmf_siblings(key);
                    }
                }
            }
            if signing_public_key.is_none() {
                client.request_path(destination).await?;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native OMENchat requested path for missing destination identity key destination={destination_hash}"
                )));
                for _ in 0..OMENCHAT_LINK_PATH_WAIT_ATTEMPTS {
                    if cancel.is_cancelled() {
                        return Err(AppError::from(NativeRuntimeError::Cancelled));
                    }
                    tokio::time::sleep(OMENCHAT_LINK_PATH_WAIT_STEP).await;
                    if let Some(key) = client.recall_destination_key(destination).await? {
                        signing_public_key = Some(key.signing_public_key);
                        if let Some(handle) =
                            self.rns_net.lock().expect("native rns-net lock").as_ref()
                        {
                            handle
                                .destination_keys
                                .lock()
                                .expect("native rns-net key store lock")
                                .ingest_with_nomadnet_lxmf_siblings(key);
                        }
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native OMENchat destination identity key acquired after path request destination={destination_hash}"
                        )));
                        break;
                    }
                }
            }
            let signing_public_key = signing_public_key.ok_or_else(|| {
                AppError::Runtime(format!(
                    "OMENchat destination {destination_hash} has no known identity key; request_path was queued, wait for server announce/path and reconnect"
                ))
            })?;
            let mut has_path = client.has_path(destination).await?;
            if !has_path {
                client.request_path(destination).await?;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native OMENchat requested path before link destination={destination_hash}"
                )));
                for _ in 0..OMENCHAT_LINK_PATH_WAIT_ATTEMPTS {
                    if cancel.is_cancelled() {
                        return Err(AppError::from(NativeRuntimeError::Cancelled));
                    }
                    tokio::time::sleep(OMENCHAT_LINK_PATH_WAIT_STEP).await;
                    if client.has_path(destination).await? {
                        has_path = true;
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native OMENchat path acquired before link destination={destination_hash}"
                        )));
                        break;
                    }
                }
            }
            if !has_path {
                return Err(AppError::Runtime(format!(
                    "OMENchat path to {destination_hash} is not known; request_path was queued, wait for server announce/path and reconnect"
                )));
            }
            let link = match client
                .establish_link(destination, signing_public_key, timeout, cancel.clone())
                .await
            {
                Ok(link) => link,
                Err(error) if omenchat_link_error_is_handshake_timeout(&error) => {
                    client.request_path(destination).await?;
                    let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native OMENchat link handshake timed out; refreshed path and retrying once destination={destination_hash}"
                    )));
                    for _ in 0..OMENCHAT_LINK_PATH_WAIT_ATTEMPTS.min(12) {
                        if cancel.is_cancelled() {
                            return Err(AppError::from(NativeRuntimeError::Cancelled));
                        }
                        tokio::time::sleep(OMENCHAT_LINK_PATH_WAIT_STEP).await;
                        if client.has_path(destination).await? {
                            break;
                        }
                    }
                    client
                        .establish_link(destination, signing_public_key, timeout, cancel)
                        .await
                        .map_err(|retry_error| {
                            AppError::Runtime(format!(
                                "OMENchat Link handshake failed after path refresh: {retry_error}"
                            ))
                        })?
                }
                Err(error) => return Err(error),
            };
            if let Some(identity_key) = identify_key {
                client.identify_link(link.link_id, identity_key).await?;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native OMENchat identified local identity on link destination={} link_id={}",
                    destination_hash,
                    hex_encode(&link.link_id)
                )));
            } else {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native OMENchat opened anonymous link destination={} link_id={} because no active identity key is loaded",
                    destination_hash,
                    hex_encode(&link.link_id)
                )));
            }
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native OMENchat link established destination={} link_id={} rtt_ms={:.1}",
                destination_hash,
                hex_encode(&link.link_id),
                link.rtt * 1000.0
            )));
            self.active_omenchat_links
                .lock()
                .expect("native active OMENchat link lock")
                .insert(link.link_id);
            Ok(OmenChatLinkOpened {
                destination_hash: destination_hash.into(),
                link_id: link.link_id,
                rtt_millis: Some((link.rtt * 1000.0).max(0.0) as u64),
            })
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            if !matches!(
                self.state_snapshot().lifecycle,
                NativeRuntimeLifecycle::Running
            ) {
                return Err(AppError::Runtime(
                    "native Reticulum runtime is not running".into(),
                ));
            }
            let destination_hash = parse_transport_destination_hash(destination_hash)?;
            let handle = self.active_transport()?;
            let mut omenchat_announce_rx = self.event_tx.subscribe();
            let identity = clean_wait_for_destination_identity(
                &handle.transport,
                &handle.storage_path,
                destination_hash,
                timeout,
                cancel.clone(),
                Some(&self.event_tx),
                Some(&self.clean_destination_identities),
            )
            .await?;
            let destination = single_output_destination_desc(
                destination_hash,
                identity,
                OMENCHAT_RNS_APP_NAME,
                OMENCHAT_NODE_ASPECT,
            )
            .map_err(AppError::from)?;
            let pre_wait_path_status = handle.transport.path_status(&destination_hash).await;
            let needs_fresh_announce = !pre_wait_path_status.path_found
                || pre_wait_path_status.hops.is_none_or(|hops| hops > 1);
            let destination_hex = destination_hash.to_hex_string();
            if needs_fresh_announce && pre_wait_path_status.path_found {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat found stale/high-hop cached path before discovery destination={} hops={:?} iface={}; explicit 0.9 path expiry remains behind the Phase 4 interoperability gate",
                    destination_hex,
                    pre_wait_path_status.hops,
                    pre_wait_path_status
                        .interface
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into())
                )));
            }
            let path_dispatch = clean_request_omenchat_paths_on_attached_interfaces(
                &handle.transport,
                destination_hash,
                &handle.clean_attached_interfaces,
                &self.event_tx,
                "before-clean-link",
            )
            .await;
            if path_dispatch == 0 {
                let path_dispatch = handle
                    .transport
                    .request_path(&destination_hash, None, None)
                    .await;
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat requested fresh path before clean link destination={} matched_ifaces={} sent_ifaces={} failed_ifaces={} scope=all",
                    destination_hex,
                    path_dispatch.matched_ifaces,
                    path_dispatch.sent_ifaces,
                    path_dispatch.failed_ifaces
                )));
                tracing::debug!(
                    destination = %destination_hex,
                    matched_ifaces = path_dispatch.matched_ifaces,
                    sent_ifaces = path_dispatch.sent_ifaces,
                    failed_ifaces = path_dispatch.failed_ifaces,
                    "native Reticulum 0.9 OMENchat requested fresh path before clean link"
                );
            }
            let recent_announce = if needs_fresh_announce {
                self.clean_recent_omenchat_announces
                    .lock()
                    .expect("native recent OMENchat announce lock")
                    .get(&destination_hex)
                    .copied()
                    .filter(|announce| announce.observed_at.elapsed() <= Duration::from_secs(45))
            } else {
                None
            };
            if let Some(announce) = recent_announce {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat recent announce evidence destination={} hops={} iface={}",
                    destination_hex,
                    announce.hops,
                    announce.iface.to_hex_string()
                )));
            }
            let fresh_announce_seen = if needs_fresh_announce {
                recent_announce.is_some()
                    || clean_wait_for_omenchat_fresh_announce(
                        &mut omenchat_announce_rx,
                        &destination_hex,
                        timeout.min(Duration::from_secs(12)),
                        cancel.clone(),
                        Some(&self.event_tx),
                    )
                    .await?
            } else {
                false
            };
            let has_path = clean_wait_for_destination_path(
                &handle.transport,
                destination_hash,
                timeout.min(Duration::from_secs(10)),
                cancel.clone(),
                Some(&self.event_tx),
                if needs_fresh_announce { Some(1) } else { None },
            )
            .await?;
            if !has_path {
                return Err(AppError::Runtime(format!(
                    "OMENchat destination {} has no known Reticulum path; request path and reconnect after announce/path discovery",
                    destination_hex
                )));
            }
            let status_after_wait = handle.transport.path_status(&destination_hash).await;
            if needs_fresh_announce && status_after_wait.hops.is_none_or(|hops| hops > 1) {
                let announce_detail = self
                    .clean_recent_omenchat_announces
                    .lock()
                    .expect("native recent OMENchat announce lock")
                    .get(&destination_hex)
                    .copied()
                    .filter(|announce| announce.observed_at.elapsed() <= Duration::from_secs(45))
                    .map(|announce| {
                        format!(
                            "recent_announce_hops={} recent_announce_iface={}",
                            announce.hops,
                            announce.iface.to_hex_string()
                        )
                    })
                    .unwrap_or_else(|| "recent_announce=none".into());
                let message = format!(
                    "OMENchat clean link blocked: only high-hop/stale path is known destination={} path_found={} hops={:?} next_hop={} iface={} fresh_announce_seen={} {}; wait for a low-hop server announce or verify gateway/IFAC alignment",
                    destination_hex,
                    status_after_wait.path_found,
                    status_after_wait.hops,
                    status_after_wait
                        .next_hop
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into()),
                    status_after_wait
                        .interface
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into()),
                    fresh_announce_seen,
                    announce_detail
                );
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 {message}"
                )));
                return Err(AppError::Runtime(message));
            }
            let _open_owner = self
                .clean_omenchat_link_coordinator
                .lock(&destination_hash, &cancel)
                .await?;
            let retired = retire_clean_omenchat_destination_links(
                &self.clean_omenchat_links,
                &self.active_omenchat_links,
                destination_hash,
            )
            .await;
            if retired > 0 {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat retired {retired} superseded clean link(s) before explicit reconnect destination={}",
                    destination_hash.to_hex_string()
                )));
            }
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let mut link_events = handle.transport.out_link_events();
            let link = handle.transport.link(destination).await;
            let link_id_hash = *link.lock().await.id();
            let link_id = address_hash_to_link_id(link_id_hash);
            let initial_status = link.lock().await.status();
            let post_request_path_status = handle.transport.path_status(&destination_hash).await;
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 OMENchat clean link requested destination={} link_id={} status={:?} route_iface={} hops={:?}",
                destination_hash.to_hex_string(),
                link_id_hash.to_hex_string(),
                initial_status,
                post_request_path_status
                    .interface
                    .map(|hash| hash.to_hex_string())
                    .unwrap_or_else(|| "-".into()),
                post_request_path_status.hops
            )));
            tracing::debug!(
                destination = %destination_hash.to_hex_string(),
                link_id = %link_id_hash.to_hex_string(),
                status = ?initial_status,
                route_iface = ?post_request_path_status.interface,
                hops = ?post_request_path_status.hops,
                "native Reticulum 0.9 OMENchat clean link requested"
            );

            if link.lock().await.status() != rns_transport::destination::link::LinkStatus::Active {
                let deadline = tokio::time::Instant::now() + timeout;
                loop {
                    if cancel.is_cancelled() {
                        clean_close_link(&handle.transport, &link).await;
                        handle.transport.reset_out_link(&destination_hash).await;
                        return Err(AppError::from(NativeRuntimeError::Cancelled));
                    }
                    if link.lock().await.status()
                        == rns_transport::destination::link::LinkStatus::Active
                    {
                        break;
                    }
                    let now = tokio::time::Instant::now();
                    if now >= deadline {
                        let status = handle.transport.path_status(&destination_hash).await;
                        {
                            let mut link = link.lock().await;
                            if matches!(
                                link.status(),
                                rns_transport::destination::link::LinkStatus::Pending
                                    | rns_transport::destination::link::LinkStatus::Handshake
                            ) {
                                link.close();
                            }
                        }
                        handle.transport.reset_out_link(&destination_hash).await;
                        let mut rediscovery_summary = "not-requested".to_string();
                        if let Some(blocked_iface) = status.interface {
                            let rediscovery = clean_request_omenchat_paths_on_attached_interfaces(
                                &handle.transport,
                                destination_hash,
                                &handle.clean_attached_interfaces,
                                &self.event_tx,
                                "after-link-timeout",
                            )
                            .await;
                            if rediscovery == 0 {
                                let rediscovery = handle
                                    .transport
                                    .request_path(&destination_hash, None, None)
                                    .await;
                                rediscovery_summary = format!(
                                    "previous_iface={} matched_ifaces={} sent_ifaces={} failed_ifaces={}",
                                    blocked_iface.to_hex_string(),
                                    rediscovery.matched_ifaces,
                                    rediscovery.sent_ifaces,
                                    rediscovery.failed_ifaces
                                );
                            } else {
                                rediscovery_summary =
                                    format!("attached_iface_requests={rediscovery}");
                            }
                        }
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum 0.9 OMENchat clean link establishment timed out destination={} link_id={} path_found={} hops={:?} next_hop={} iface={}; pending link reset invokes 0.9 cleanup before retained rediscovery fallback={}",
                            destination_hash.to_hex_string(),
                            link_id_hash.to_hex_string(),
                            status.path_found,
                            status.hops,
                            status
                                .next_hop
                                .map(|hash| hash.to_hex_string())
                                .unwrap_or_else(|| "-".into()),
                            status
                                .interface
                                .map(|hash| hash.to_hex_string())
                                .unwrap_or_else(|| "-".into()),
                            rediscovery_summary
                        )));
                        return Err(AppError::from(NativeRuntimeError::Timeout(
                            "OMENchat clean link establishment".into(),
                        )));
                    }
                    let wait = (deadline - now).min(Duration::from_millis(100));
                    match tokio::time::timeout(wait, link_events.recv()).await {
                        Ok(Ok(event))
                            if event.id == link_id_hash
                                && matches!(
                                    event.event,
                                    rns_transport::destination::link::LinkEvent::Activated
                                ) =>
                        {
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native Reticulum 0.9 OMENchat clean link activated destination={} link_id={}",
                                destination_hash.to_hex_string(),
                                link_id_hash.to_hex_string()
                            )));
                            break;
                        }
                        Ok(Ok(event))
                            if event.id == link_id_hash
                                && matches!(
                                    event.event,
                                    rns_transport::destination::link::LinkEvent::Closed
                                ) =>
                        {
                            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native Reticulum 0.9 OMENchat clean link closed during establishment destination={} link_id={}",
                                destination_hash.to_hex_string(),
                                link_id_hash.to_hex_string()
                            )));
                            return Err(AppError::Runtime(
                                "OMENchat clean link closed during establishment".into(),
                            ));
                        }
                        Ok(Ok(_))
                        | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
                        Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                            return Err(AppError::Runtime(
                                "OMENchat clean link event stream closed".into(),
                            ));
                        }
                        Err(_) => {}
                    }
                }
            }

            if let Some(identity) = self.clean_stack_identify_identity(true) {
                match send_reticulum_link_identify(
                    &handle.transport,
                    &link,
                    &identity,
                    destination_hash,
                )
                .await
                {
                    Ok(()) => {
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum 0.9 OMENchat LinkIdentify sent destination={} link_id={} identity={}",
                            destination_hash.to_hex_string(),
                            link_id_hash.to_hex_string(),
                            identity.address_hash()
                        )));
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum 0.9 OMENchat LinkIdentify failed destination={} link_id={} error={}",
                            destination_hash.to_hex_string(),
                            link_id_hash.to_hex_string(),
                            error
                        )));
                    }
                }
            } else {
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat opened anonymous clean link destination={} link_id={}",
                    destination_hash.to_hex_string(),
                    link_id_hash.to_hex_string()
                )));
            }

            let rtt_millis = link
                .lock()
                .await
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX));
            self.active_omenchat_links
                .lock()
                .expect("native active OMENchat link lock")
                .insert(link_id);
            self.clean_omenchat_links
                .lock()
                .expect("native clean OMENchat link lock")
                .insert(
                    link_id,
                    CleanOmenChatLink {
                        destination_hash,
                        link_id: link_id_hash,
                        transport: handle.transport,
                        link,
                    },
                );
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 OMENchat clean link established destination={} link_id={} rtt_ms={}",
                destination_hash.to_hex_string(),
                link_id_hash.to_hex_string(),
                rtt_millis
            )));
            Ok(OmenChatLinkOpened {
                destination_hash: destination_hash.to_hex_string(),
                link_id,
                rtt_millis: Some(rtt_millis as u64),
            })
        }
    }

    async fn send_omenchat_frame(&self, link_id: [u8; 16], frame_bytes: Vec<u8>) -> AppResult<()> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let client = {
                let handle_guard = self.rns_net.lock().expect("native rns-net lock");
                handle_guard
                    .as_ref()
                    .ok_or_else(|| {
                        AppError::Runtime("native rns-net runtime is not started".into())
                    })?
                    .client
                    .clone()
            };
            let result = client
                .send_on_link(link_id, frame_bytes, OMENCHAT_LINK_CONTEXT)
                .await;
            if let Err(error) = &result {
                let _ = self.event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                    ResourceLifecycleEvent {
                        transfer_id: resource_id.clone(),
                        state: ResourceLifecycleState::Failed,
                        bytes: Some(payload.len() as u64),
                        reason: Some(error.to_string()),
                        source: Some("omenchat".into()),
                        purpose: Some("omenchat-resource".into()),
                        direction: Some("outbound".into()),
                        peer: Some(hex_encode(&link_id)),
                    },
                ));
                let _ =
                    self.event_tx
                        .send(RuntimeBusEvent::OmenChatLinkClosed(OmenChatLinkClosed {
                            link_id,
                            reason: Some(format!("send failed: {error}")),
                        }));
            }
            result
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let entry = self
                .clean_omenchat_links
                .lock()
                .expect("native clean OMENchat link lock")
                .get(&link_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Runtime(format!(
                        "OMENchat clean link {} is not active",
                        hex_encode(&link_id)
                    ))
                })?;
            {
                let link = entry.link.lock().await;
                if link.status() != rns_transport::destination::link::LinkStatus::Active {
                    return Err(AppError::Runtime(format!(
                        "OMENchat clean link {} is not active",
                        hex_encode(&link_id)
                    )));
                }
                link.ingress_iface().ok_or_else(|| {
                    AppError::Runtime(format!(
                        "OMENchat clean link {} has no bound ingress interface",
                        hex_encode(&link_id)
                    ))
                })?;
            }
            rns_transport::delivery::send_on_link(&entry.transport, &entry.link, &frame_bytes)
                .await
                .map_err(|error| {
                    AppError::Runtime(format!(
                        "OMENchat clean frame send failed link_id={} destination={}: {error}",
                        hex_encode(&link_id),
                        entry.destination_hash.to_hex_string()
                    ))
                })?;
            let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 OMENchat clean frame sent via delivery link_id={} destination={} bytes={} context=0x00",
                hex_encode(&link_id),
                entry.destination_hash.to_hex_string(),
                frame_bytes.len()
            )));
            Ok(())
        }
    }

    async fn send_omenchat_resource(
        &self,
        link_id: [u8; 16],
        resource_id: String,
        payload: Vec<u8>,
    ) -> AppResult<()> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let client = {
                let handle_guard = self.rns_net.lock().expect("native rns-net lock");
                handle_guard
                    .as_ref()
                    .ok_or_else(|| {
                        AppError::Runtime("native rns-net runtime is not started".into())
                    })?
                    .client
                    .clone()
            };
            let mut metadata = OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec();
            metadata.extend(resource_id.as_bytes());
            let result = client.send_resource(link_id, payload, Some(metadata)).await;
            if let Err(error) = &result {
                let _ =
                    self.event_tx
                        .send(RuntimeBusEvent::OmenChatLinkClosed(OmenChatLinkClosed {
                            link_id,
                            reason: Some(format!("resource send failed: {error}")),
                        }));
            }
            result
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let entry = self
                .clean_omenchat_links
                .lock()
                .expect("native clean OMENchat link lock")
                .get(&link_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Runtime(format!(
                        "OMENchat clean link {} is not active",
                        hex_encode(&link_id)
                    ))
                })?;
            let mut metadata = OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec();
            metadata.extend(resource_id.as_bytes());
            let payload_len = payload.len() as u64;
            let result = entry
                .transport
                .send_resource(&entry.link_id, payload, Some(metadata))
                .await
                .map_err(|error| {
                    AppError::Runtime(format!(
                        "OMENchat clean resource send failed link_id={} resource_id={}: {error:?}",
                        hex_encode(&link_id),
                        resource_id
                    ))
                });
            if let Err(error) = &result {
                let _ = self.event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                    ResourceLifecycleEvent {
                        transfer_id: resource_id.clone(),
                        state: ResourceLifecycleState::Failed,
                        bytes: Some(payload_len),
                        reason: Some(error.to_string()),
                        operation_id: None,
                        source: Some("omenchat".into()),
                        purpose: Some("omenchat-resource".into()),
                        direction: Some("outbound".into()),
                        peer: Some(hex_encode(&link_id)),
                    },
                ));
                let _ =
                    self.event_tx
                        .send(RuntimeBusEvent::OmenChatLinkClosed(OmenChatLinkClosed {
                            link_id,
                            reason: Some(format!("resource send failed: {error}")),
                        }));
            } else {
                let resource_hash = result.as_ref().expect("checked success");
                let _ = self.event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                    ResourceLifecycleEvent {
                        transfer_id: resource_hash.to_string(),
                        state: ResourceLifecycleState::Offered,
                        bytes: Some(payload_len),
                        reason: None,
                        operation_id: None,
                        source: Some("omenchat".into()),
                        purpose: Some("omenchat-resource".into()),
                        direction: Some("outbound".into()),
                        peer: Some(hex_encode(&link_id)),
                    },
                ));
                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat clean resource advertised link_id={} resource_id={} resource_hash={}",
                    hex_encode(&link_id),
                    resource_id,
                    resource_hash
                )));
            }
            result.map(|_| ())
        }
    }

    async fn close_omenchat_link(&self, link_id: [u8; 16]) -> AppResult<bool> {
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let client = {
                let handle_guard = self.rns_net.lock().expect("native rns-net lock");
                handle_guard
                    .as_ref()
                    .ok_or_else(|| {
                        AppError::Runtime("native rns-net runtime is not started".into())
                    })?
                    .client
                    .clone()
            };
            let torn_down = client.teardown_link(link_id).await;
            self.active_omenchat_links
                .lock()
                .expect("native active OMENchat link lock")
                .remove(&link_id);
            Ok(torn_down)
        }
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            let entry = self
                .clean_omenchat_links
                .lock()
                .expect("native clean OMENchat link lock")
                .remove(&link_id);
            self.active_omenchat_links
                .lock()
                .expect("native active OMENchat link lock")
                .remove(&link_id);
            let Some(entry) = entry else {
                return Ok(false);
            };
            close_clean_omenchat_link_entry(&entry).await;
            Ok(true)
        }
    }
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
struct CleanRequestWait<'a> {
    path: &'a str,
    value: rmpv::Value,
    timeout: Duration,
    cancel: CancellationToken,
    propagation_node: &'a str,
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
async fn clean_send_request_value_and_wait(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    link: &Arc<AsyncMutex<rns_transport::destination::link::Link>>,
    link_id: rns_transport::hash::AddressHash,
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
    request: CleanRequestWait<'_>,
) -> AppResult<NativeLinkResponseFrame> {
    let CleanRequestWait {
        path,
        value,
        timeout,
        cancel,
        propagation_node,
    } = request;
    if cancel.is_cancelled() {
        return Err(AppError::from(NativeRuntimeError::Cancelled));
    }
    let frame = NativeLinkRequestFrame::build_with_value(path, value, native_unix_timestamp())
        .map_err(AppError::from)?;
    let mut resource_events = transport.resource_events();
    let mut received_data_events = transport.received_data_events();
    let (request_id, request_resource_hash) = if frame.requires_request_resource() {
        let request_resource_hash = transport
            .send_request_resource(
                &link_id,
                frame.request_id.to_vec(),
                frame.packed.clone(),
                None,
            )
            .await
            .map_err(|error| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "native Reticulum 0.9 LXMF propagation request-resource send failed: {error:?}"
                )))
            })?;
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native LXMF clean propagation request resource sent propagation_node={} link_id={} path={} request_id={} request_resource={} bytes={}",
            propagation_node,
            link_id,
            path,
            hex_encode(&frame.request_id),
            request_resource_hash,
            frame.packed.len()
        )));
        (frame.request_id, Some(request_resource_hash))
    } else {
        let (ingress_iface, packet) = {
            let link = link.lock().await;
            let ingress_iface = link.ingress_iface().ok_or_else(|| {
                AppError::from(NativeRuntimeError::Native(
                    "native Reticulum 0.9 LXMF propagation link has no bound ingress interface"
                        .into(),
                ))
            })?;
            let mut packet = link.data_packet(&frame.packed).map_err(|error| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "native Reticulum 0.9 LXMF propagation request packet build failed: {error:?}"
                )))
            })?;
            packet.context = rns_transport::PacketContext::Request;
            (ingress_iface, packet)
        };
        let packet_hash = packet.hash().to_bytes();
        let mut request_id = [0u8; rns_transport::hash::ADDRESS_HASH_SIZE];
        request_id.copy_from_slice(&packet_hash[..rns_transport::hash::ADDRESS_HASH_SIZE]);
        transport.send_direct(ingress_iface, packet).await;
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native LXMF clean propagation request packet sent propagation_node={} link_id={} path={} request_id={} bytes={}",
            propagation_node,
            link_id,
            path,
            hex_encode(&request_id),
            frame.packed.len()
        )));
        (request_id, None)
    };

    let deadline = tokio::time::Instant::now() + timeout;
    let mut target_resource_events = 0usize;
    let mut target_response_events = 0usize;
    let mut progress_events = 0usize;
    let mut unrelated_events = 0usize;
    let mut outbound_complete = false;
    let mut last_error = String::from("none");
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(AppError::from(NativeRuntimeError::Timeout(format!(
                "LXMF propagation request response path={path}; request_resource={}; target_resource_events={}; target_response_events={}; progress_events={}; unrelated_events={}; outbound_complete={}; last_error={}",
                request_resource_hash
                    .map(|hash| hash.to_string())
                    .unwrap_or_else(|| "none".into()),
                target_resource_events,
                target_response_events,
                progress_events,
                unrelated_events,
                outbound_complete,
                last_error
            ))));
        }
        let wait = (deadline - now).min(Duration::from_millis(100));
        tokio::select! {
            received = received_data_events.recv() => {
                match received {
                    Ok(data) if data.destination == link_id => {
                        target_response_events += 1;
                        if data.context == Some(rns_transport::PacketContext::Response) {
                            match NativeLinkResponseFrame::parse_matching(
                                data.data.as_slice(),
                                &request_id,
                            ) {
                                Ok(Some(response)) => {
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native LXMF clean propagation response packet received propagation_node={} link_id={} path={} request_id={} bytes={}",
                                        propagation_node,
                                        link_id,
                                        path,
                                        hex_encode(&request_id),
                                        data.data.len()
                                    )));
                                    return Ok(response);
                                }
                                Ok(None) => unrelated_events += 1,
                                Err(error) => {
                                    last_error = format!("response packet parse error: {error:?}");
                                    unrelated_events += 1;
                                }
                            }
                        } else {
                            unrelated_events += 1;
                        }
                    }
                    Ok(_) => unrelated_events += 1,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF clean propagation received-data event stream lagged skipped={skipped}"
                        )));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err(AppError::from(NativeRuntimeError::Native(
                            "native LXMF clean propagation received-data event stream closed".into(),
                        )));
                    }
                }
            }
            resource = resource_events.recv() => {
                match resource {
                    Ok(event) if event.link_id == link_id => {
                        target_resource_events += 1;
                        match event.kind {
                            ResourceEventKind::Complete(complete) => {
                                match NativeLinkResponseFrame::parse_matching_response_resource(
                                    &complete,
                                    &request_id,
                                ) {
                                    Ok(Some(response)) => {
                                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                            "native LXMF clean propagation response resource received propagation_node={} link_id={} path={} request_id={} response_resource={} bytes={}",
                                            propagation_node,
                                            link_id,
                                            path,
                                            hex_encode(&request_id),
                                            event.hash,
                                            complete.data.len()
                                        )));
                                        return Ok(response);
                                    }
                                    Ok(None) => unrelated_events += 1,
                                    Err(error) => {
                                        last_error = format!("response resource parse error: {error:?}");
                                        unrelated_events += 1;
                                    }
                                }
                            }
                            ResourceEventKind::Progress(progress) => {
                                progress_events += 1;
                                let _ = event_tx.send(RuntimeBusEvent::ResourceProgress(
                                        ResourceProgressEvent {
                                            transfer_id: event.hash.to_string(),
                                            received: progress.received_bytes,
                                            total: Some(progress.total_bytes),
                                            operation_id: None,
                                            source: Some("lxmf-propagation".into()),
                                            purpose: Some("lxmf-propagation".into()),
                                            direction: Some("inbound".into()),
                                            peer: Some(propagation_node.to_string()),
                                        },
                                    ));
                            }
                            ResourceEventKind::OutboundComplete
                                if request_resource_hash.is_some_and(|hash| hash == event.hash) =>
                            {
                                let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                    ResourceLifecycleEvent {
                                        transfer_id: event.hash.to_string(),
                                        state: ResourceLifecycleState::Complete,
                                        bytes: None,
                                        reason: None,
                                        operation_id: None,
                                        source: Some("lxmf-propagation".into()),
                                        purpose: Some("lxmf-propagation".into()),
                                        direction: Some("inbound".into()),
                                        peer: Some(propagation_node.to_string()),
                                    },
                                ));
                                outbound_complete = true;
                            }
                            ResourceEventKind::OutboundFailed
                                if request_resource_hash.is_some_and(|hash| hash == event.hash) =>
                            {
                                let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                    ResourceLifecycleEvent {
                                        transfer_id: event.hash.to_string(),
                                        state: ResourceLifecycleState::Failed,
                                        bytes: None,
                                        reason: Some(
                                            "outbound LXMF propagation request-resource transfer failed"
                                                .into(),
                                        ),
                                        operation_id: None,
                                        source: Some("lxmf-propagation".into()),
                                        purpose: Some("lxmf-propagation".into()),
                                        direction: Some("inbound".into()),
                                        peer: Some(propagation_node.to_string()),
                                    },
                                ));
                                return Err(AppError::from(NativeRuntimeError::Native(
                                    "native Reticulum 0.9 LXMF propagation request-resource transfer failed"
                                        .into(),
                                )));
                            }
                            ResourceEventKind::InboundFailed(failure) => {
                                let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                    ResourceLifecycleEvent {
                                        transfer_id: event.hash.to_string(),
                                        state: ResourceLifecycleState::Failed,
                                        bytes: None,
                                        reason: Some(failure.reason.clone()),
                                        operation_id: None,
                                        source: Some("lxmf-propagation".into()),
                                        purpose: Some("lxmf-propagation".into()),
                                        direction: Some("inbound".into()),
                                        peer: Some(propagation_node.to_string()),
                                    },
                                ));
                                last_error = failure.reason;
                            }
                            ResourceEventKind::OutboundCancelled
                                if request_resource_hash.is_some_and(|hash| hash == event.hash) =>
                            {
                                let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                    ResourceLifecycleEvent {
                                        transfer_id: event.hash.to_string(),
                                        state: ResourceLifecycleState::Cancelled,
                                        bytes: None,
                                        reason: Some("cancelled".into()),
                                        operation_id: None,
                                        source: Some("lxmf-propagation".into()),
                                        purpose: Some("lxmf-propagation".into()),
                                        direction: Some("inbound".into()),
                                        peer: Some(propagation_node.to_string()),
                                    },
                                ));
                                return Err(AppError::from(NativeRuntimeError::Cancelled));
                            }
                            _ => {}
                        }
                    }
                    Ok(_) => unrelated_events += 1,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native LXMF clean propagation resource event stream lagged skipped={skipped}"
                        )));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err(AppError::from(NativeRuntimeError::Native(
                            "native LXMF clean propagation resource event stream closed".into(),
                        )));
                    }
                }
            }
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn close_clean_omenchat_link_entry(entry: &CleanOmenChatLink) -> bool {
    let teardown = {
        let mut link = entry.link.lock().await;
        let ingress_iface = link.ingress_iface();
        link.teardown().map(|packet| (ingress_iface, packet))
    };
    if let Some((Some(ingress_iface), packet)) = teardown {
        entry.transport.send_direct(ingress_iface, packet).await;
        true
    } else {
        false
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn retire_clean_omenchat_destination_links(
    clean_links: &Arc<Mutex<BTreeMap<[u8; 16], CleanOmenChatLink>>>,
    active_links: &Arc<Mutex<BTreeSet<[u8; 16]>>>,
    destination_hash: rns_transport::hash::AddressHash,
) -> usize {
    let mut retired = 0usize;
    loop {
        let entry = {
            let mut links = clean_links.lock().expect("native clean OMENchat link lock");
            let link_id = links.iter().find_map(|(link_id, entry)| {
                (entry.destination_hash == destination_hash).then_some(*link_id)
            });
            link_id.and_then(|link_id| links.remove(&link_id))
        };
        let Some(entry) = entry else {
            return retired;
        };
        active_links
            .lock()
            .expect("native active OMENchat link lock")
            .remove(&address_hash_to_link_id(entry.link_id));
        close_clean_omenchat_link_entry(&entry).await;
        retired = retired.saturating_add(1);
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn clean_close_link(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    link: &Arc<AsyncMutex<rns_transport::destination::link::Link>>,
) -> bool {
    let teardown = {
        let mut link = link.lock().await;
        let ingress_iface = link.ingress_iface();
        link.teardown().map(|packet| (ingress_iface, packet))
    };
    if let Some((Some(ingress_iface), packet)) = teardown {
        transport.send_direct(ingress_iface, packet).await;
        true
    } else {
        false
    }
}

fn parse_transport_destination_hash(
    destination_hash: &str,
) -> AppResult<rns_transport::hash::AddressHash> {
    if destination_hash.len() < rns_transport::hash::ADDRESS_HASH_SIZE * 2
        || !destination_hash.chars().all(|ch| ch.is_ascii_hexdigit())
    {
        return Err(AppError::from(NativeRuntimeError::InvalidAddress(
            destination_hash.into(),
        )));
    }
    rns_transport::hash::AddressHash::new_from_hex_string(destination_hash)
        .map_err(|_| AppError::from(NativeRuntimeError::InvalidAddress(destination_hash.into())))
}

#[cfg(all(feature = "native-rns-net", any()))]
fn find_live_rns_net_interface<'a>(
    stats: &'a rns_net::InterfaceStatsResponse,
    plan: &NativeInterfacePlan,
    endpoint: Option<&str>,
) -> Option<&'a rns_net::SingleInterfaceStat> {
    let plan_name = plan.name.to_ascii_lowercase();
    let endpoint = endpoint.map(str::to_ascii_lowercase);
    stats.interfaces.iter().find(|interface| {
        let interface_name = interface.name.to_ascii_lowercase();
        interface_name == plan_name
            || interface_name.contains(&plan_name)
            || endpoint
                .as_ref()
                .is_some_and(|endpoint| interface_name.contains(endpoint))
    })
}

#[cfg(all(feature = "native-rns-net", any()))]
fn format_live_rns_net_interface_detail(interface: &rns_net::SingleInterfaceStat) -> String {
    format!(
        "{} [{}] {} | connected={} | rx={} in {} pkt | tx={} in {} pkt | ifac={}",
        interface.name,
        interface.id,
        interface.interface_type,
        interface.status,
        human_bytes(interface.rxb),
        interface.rx_packets,
        human_bytes(interface.txb),
        interface.tx_packets,
        interface
            .ifac_size
            .map(|size| human_bytes(size as u64))
            .unwrap_or_else(|| "none".into())
    )
}

#[cfg(all(feature = "native-rns-net", any()))]
fn select_live_rns_net_interface<'a>(
    matched: Option<&'a rns_net::SingleInterfaceStat>,
    ordered: Option<&'a rns_net::SingleInterfaceStat>,
) -> Option<&'a rns_net::SingleInterfaceStat> {
    matched.or(ordered)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn take_active_omenchat_link_close(
    active_omenchat_links: &Arc<Mutex<BTreeSet<[u8; 16]>>>,
    link_id: [u8; 16],
    reason: Option<String>,
) -> Option<OmenChatLinkClosed> {
    let was_active = active_omenchat_links
        .lock()
        .expect("native active OMENchat link lock")
        .remove(&link_id);
    was_active.then_some(OmenChatLinkClosed { link_id, reason })
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn block_on_native_transport_setup<F, T>(future: F) -> AppResult<T>
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return Ok(tokio::task::block_in_place(|| handle.block_on(future)));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            AppError::Runtime(format!(
                "native Reticulum setup runtime could not be created: {error}"
            ))
        })?;
    Ok(runtime.block_on(future))
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn clean_lxmf_delivery_destination_hash_from_identity_path(
    identity_path: &Path,
) -> AppResult<String> {
    let identity = load_transport_private_identity_file(identity_path).map_err(AppError::from)?;
    let destination = rns_transport::destination::SingleOutputDestination::new(
        *identity.as_identity(),
        rns_transport::destination::DestinationName::new("lxmf", "delivery"),
    );
    Ok(destination.desc.address_hash.to_hex_string())
}

fn address_hash_to_link_id(hash: rns_transport::hash::AddressHash) -> [u8; 16] {
    let mut link_id = [0u8; 16];
    link_id.copy_from_slice(hash.as_slice());
    link_id
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn clean_omenchat_frame_context(context: rns_transport::PacketContext) -> bool {
    context == rns_transport::PacketContext::None || context as u8 == OMENCHAT_LINK_CONTEXT
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn clean_omenchat_optional_frame_context(context: Option<rns_transport::PacketContext>) -> bool {
    context.map(clean_omenchat_frame_context).unwrap_or(true)
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn clean_wait_for_destination_identity(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    storage_path: &Path,
    destination_hash: rns_transport::hash::AddressHash,
    timeout: Duration,
    cancel: CancellationToken,
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
    cached_identities: Option<&Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>>,
) -> AppResult<rns_transport::identity::Identity> {
    if let Some(identity) = transport.destination_identity(&destination_hash).await {
        if let Some(event_tx) = event_tx {
            let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 OMENchat clean destination identity already known destination={}",
                destination_hash.to_hex_string()
            )));
        }
        return Ok(identity);
    }
    if let Some(identity) = cached_identities.and_then(|cache| {
        cache
            .lock()
            .expect("native clean destination identity cache lock")
            .get(&destination_hash.to_hex_string())
            .cloned()
    }) {
        if let Some(event_tx) = event_tx {
            let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                "native Reticulum 0.9 clean destination identity restored from announce cache destination={}",
                destination_hash.to_hex_string()
            )));
        }
        return Ok(identity);
    }
    match transport
        .restore_reticulum_path_table_report(storage_path)
        .await
    {
        Ok(report) => {
            if !report.restored_identities.is_empty() {
                if let Some(cache) = cached_identities {
                    let mut guard = cache
                        .lock()
                        .expect("native clean destination identity cache lock");
                    for restored in &report.restored_identities {
                        match parse_restored_destination_identity(
                            &restored.public_key,
                            &restored.verifying_key,
                        ) {
                            Ok(identity) => insert_bounded_destination_cache(
                                &mut guard,
                                restored.destination.to_hex_string(),
                                identity,
                            ),
                            Err(error) => {
                                if let Some(event_tx) = event_tx {
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 ignored invalid restored identity destination={} error={error}",
                                        restored.destination.to_hex_string()
                                    )));
                                }
                            }
                        }
                    }
                }
            }
            if let Some(identity) = transport.destination_identity(&destination_hash).await {
                if let Some(event_tx) = event_tx {
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum 0.9 OMENchat clean destination identity restored destination={} active_paths={} identities={}",
                        destination_hash.to_hex_string(),
                        report.restored_active_paths,
                        report.restored_identities.len()
                    )));
                }
                return Ok(identity);
            }
            if let Some(identity) = cached_identities.and_then(|cache| {
                cache
                    .lock()
                    .expect("native clean destination identity cache lock")
                    .get(&destination_hash.to_hex_string())
                    .cloned()
            }) {
                if let Some(event_tx) = event_tx {
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum 0.9 clean destination identity found in announce cache after path restore destination={} active_paths={} identities={}",
                        destination_hash.to_hex_string(),
                        report.restored_active_paths,
                        report.restored_identities.len()
                    )));
                }
                return Ok(identity);
            }
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat clean path restore had no identity destination={} active_paths={} identities={}",
                    destination_hash.to_hex_string(),
                    report.restored_active_paths,
                    report.restored_identities.len()
                )));
            }
        }
        Err(error) => {
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat clean path restore failed destination={} storage={} error={}",
                    destination_hash.to_hex_string(),
                    storage_path.display(),
                    error
                )));
            }
        }
    }
    transport.request_path(&destination_hash, None, None).await;
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 OMENchat clean path requested destination={}",
            destination_hash.to_hex_string()
        )));
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        if let Some(identity) = transport.destination_identity(&destination_hash).await {
            return Ok(identity);
        }
        if let Some(identity) = cached_identities.and_then(|cache| {
            cache
                .lock()
                .expect("native clean destination identity cache lock")
                .get(&destination_hash.to_hex_string())
                .cloned()
        }) {
            return Ok(identity);
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat clean destination identity timed out destination={}",
                    destination_hash.to_hex_string()
                )));
            }
            return Err(AppError::from(NativeRuntimeError::PathUnavailable(
                destination_hash.to_hex_string(),
            )));
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(100))).await;
    }
}

#[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
async fn clean_wait_for_destination_app_data(
    cache: &Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    destination_hash: rns_transport::hash::AddressHash,
    timeout: Duration,
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
) -> Option<Vec<u8>> {
    let destination = destination_hash.to_hex_string();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(app_data) = cache
            .lock()
            .expect("native clean destination app-data cache lock")
            .get(&destination)
            .cloned()
            .filter(|data| !data.is_empty())
        {
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 clean app-data cache hit destination={} bytes={}",
                    destination,
                    app_data.len()
                )));
            }
            return Some(app_data);
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 clean app-data cache timed out destination={}",
                    destination
                )));
            }
            return None;
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(100))).await;
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn clean_wait_for_destination_path(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    destination_hash: rns_transport::hash::AddressHash,
    timeout: Duration,
    cancel: CancellationToken,
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
    max_hops: Option<u8>,
) -> AppResult<bool> {
    let initial_status = transport.path_status(&destination_hash).await;
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 OMENchat clean path status destination={} found={} hops={:?} next_hop={} iface={}",
            destination_hash.to_hex_string(),
            initial_status.path_found,
            initial_status.hops,
            initial_status
                .next_hop
                .map(|hash| hash.to_hex_string())
                .unwrap_or_else(|| "-".into()),
            initial_status
                .interface
                .map(|hash| hash.to_hex_string())
                .unwrap_or_else(|| "-".into())
        )));
    }
    if initial_status.path_found && max_hops.is_none_or(|hops| initial_status.hops <= Some(hops)) {
        return Ok(true);
    }

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            let final_status = transport.path_status(&destination_hash).await;
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat clean path wait timed out destination={} found={} hops={:?} next_hop={} iface={}",
                    destination_hash.to_hex_string(),
                    final_status.path_found,
                    final_status.hops,
                    final_status
                        .next_hop
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into()),
                    final_status
                        .interface
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into())
                )));
            }
            return Ok(final_status.path_found
                && max_hops.is_none_or(|hops| final_status.hops <= Some(hops)));
        }
        tokio::time::sleep((deadline - now).min(OMENCHAT_CLEAN_LINK_PATH_WAIT_STEP)).await;
        let status = transport.path_status(&destination_hash).await;
        if status.path_found && max_hops.is_none_or(|hops| status.hops <= Some(hops)) {
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat clean path acquired destination={} hops={:?} next_hop={} iface={}",
                    destination_hash.to_hex_string(),
                    status.hops,
                    status
                        .next_hop
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into()),
                    status
                        .interface
                        .map(|hash| hash.to_hex_string())
                        .unwrap_or_else(|| "-".into())
                )));
            }
            return Ok(true);
        }
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn clean_request_omenchat_paths_on_attached_interfaces(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    destination_hash: rns_transport::hash::AddressHash,
    interfaces: &[CleanAttachedInterface],
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
    reason: &str,
) -> usize {
    let mut ordered = interfaces.to_vec();
    ordered.sort_by_key(|iface| (!iface.ifac_configured, iface.name.clone()));
    let mut sent_or_queued = 0usize;
    let mut details = Vec::new();
    for iface in ordered {
        let trace = transport
            .request_path(&destination_hash, Some(iface.address), None)
            .await;
        if trace.sent_ifaces > 0 || trace.queued_ifaces > 0 {
            sent_or_queued += trace.sent_ifaces + trace.queued_ifaces;
        }
        details.push(format!(
            "{}:{} ifac={} matched={} sent={} queued={} failed={}",
            iface.name,
            iface.address.to_hex_string(),
            iface.ifac_configured,
            trace.matched_ifaces,
            trace.sent_ifaces,
            trace.queued_ifaces,
            trace.failed_ifaces
        ));
    }
    if details.is_empty() {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 OMENchat attached-interface path request skipped destination={} reason={} interfaces=0",
            destination_hash.to_hex_string(),
            reason
        )));
    } else {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 OMENchat requested path on attached interfaces destination={} reason={} total_sent_or_queued={} {}",
            destination_hash.to_hex_string(),
            reason,
            sent_or_queued,
            details.join(" | ")
        )));
    }
    sent_or_queued
}

#[cfg(not(all(feature = "native-rns-net", any())))]
async fn clean_wait_for_omenchat_fresh_announce(
    receiver: &mut broadcast::Receiver<RuntimeBusEvent>,
    destination_hex: &str,
    timeout: Duration,
    cancel: CancellationToken,
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
) -> AppResult<bool> {
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum 0.9 OMENchat waiting for fresh announce before high-hop clean link destination={} timeout_ms={}",
            destination_hex,
            timeout.as_millis()
        )));
    }

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        let now = tokio::time::Instant::now();
        if now >= deadline {
            if let Some(event_tx) = event_tx {
                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                    "native Reticulum 0.9 OMENchat fresh announce wait timed out destination={}",
                    destination_hex
                )));
            }
            return Ok(false);
        }

        let wait = (deadline - now).min(Duration::from_millis(250));
        match tokio::time::timeout(wait, receiver.recv()).await {
            Ok(Ok(RuntimeBusEvent::Announce(payload)))
                if payload.kind == DirectoryKind::OmenChat
                    && payload
                        .destination_hash
                        .eq_ignore_ascii_case(destination_hex) =>
            {
                if let Some(event_tx) = event_tx {
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum 0.9 OMENchat fresh announce observed destination={} display={}",
                        destination_hex, payload.display_name
                    )));
                }
                return Ok(true);
            }
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(broadcast::error::RecvError::Lagged(skipped))) => {
                if let Some(event_tx) = event_tx {
                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                        "native Reticulum 0.9 OMENchat fresh announce wait lagged destination={} skipped={}",
                        destination_hex, skipped
                    )));
                }
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => return Ok(false),
        }
    }
}

#[cfg(not(all(feature = "native-rns-net", any())))]
fn spawn_clean_omenchat_event_bridge(
    transport: Arc<reticulum_rs::runtime::Transport>,
    active_omenchat_links: Arc<Mutex<BTreeSet<[u8; 16]>>>,
    clean_omenchat_links: Arc<Mutex<BTreeMap<[u8; 16], CleanOmenChatLink>>>,
    clean_destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
    attachments_dir: std::path::PathBuf,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    #[cfg(feature = "native-lxmf")] pending_lxmf_proofs: PendingLxmfProofs,
) {
    #[cfg(feature = "native-lxmf")]
    let clean_lxmf_seen_messages: Arc<Mutex<BTreeMap<String, tokio::time::Instant>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    {
        let mut iface_events = transport.iface_rx();
        let active_omenchat_links = active_omenchat_links.clone();
        let clean_omenchat_links = clean_omenchat_links.clone();
        let event_tx = event_tx.clone();
        tokio::spawn(async move {
            loop {
                match iface_events.recv().await {
                    Ok(event) => {
                        if event.packet.header.destination_type
                            != rns_transport::DestinationType::Link
                        {
                            continue;
                        }
                        let packet_link_id = address_hash_to_link_id(event.packet.destination);
                        let (active_count, is_active, is_clean) = {
                            let active_links = active_omenchat_links
                                .lock()
                                .expect("native active OMENchat link lock");
                            let active_count = active_links.len();
                            let is_active = active_links.contains(&packet_link_id);
                            drop(active_links);
                            let clean_links = clean_omenchat_links
                                .lock()
                                .expect("native clean OMENchat link lock");
                            let is_clean = clean_links.contains_key(&packet_link_id);
                            (active_count, is_active, is_clean)
                        };
                        let is_link_data_or_proof = matches!(
                            event.packet.header.packet_type,
                            rns_transport::PacketType::Data | rns_transport::PacketType::Proof
                        );
                        if active_count == 0
                            || ((!is_active && !is_clean) && !is_link_data_or_proof)
                        {
                            continue;
                        }
                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum 0.9 raw link packet observed destination={} packet_type={:?} header_type={:?} context=0x{:02x} hops={} bytes={} active_omenchat={} tracked_clean={} tracked_active_links={}",
                            event.packet.destination.to_hex_string(),
                            event.packet.header.packet_type,
                            event.packet.header.header_type,
                            event.packet.context as u8,
                            event.packet.header.hops,
                            event.packet.data.len(),
                            is_active,
                            is_clean,
                            active_count
                        )));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    {
        let mut link_events = transport.out_link_events();
        let active_omenchat_links = active_omenchat_links.clone();
        let clean_omenchat_links = clean_omenchat_links.clone();
        let clean_destination_identities = clean_destination_identities.clone();
        let attachments_dir = attachments_dir.clone();
        let event_tx = event_tx.clone();
        #[cfg(feature = "native-lxmf")]
        let clean_lxmf_seen_messages = clean_lxmf_seen_messages.clone();
        tokio::spawn(async move {
            loop {
                match link_events.recv().await {
                    Ok(event) => {
                        let link_id = address_hash_to_link_id(event.id);
                        match event.event {
                            rns_transport::destination::link::LinkEvent::Closed => {
                                clean_omenchat_links
                                    .lock()
                                    .expect("native clean OMENchat link lock")
                                    .remove(&link_id);
                                if let Some(closed) = take_active_omenchat_link_close(
                                    &active_omenchat_links,
                                    link_id,
                                    Some("clean Reticulum link closed".into()),
                                ) {
                                    let _ =
                                        event_tx.send(RuntimeBusEvent::OmenChatLinkClosed(closed));
                                }
                            }
                            rns_transport::destination::link::LinkEvent::Data(payload) => {
                                let (is_active, is_clean) = {
                                    let is_active = active_omenchat_links
                                        .lock()
                                        .expect("native active OMENchat link lock")
                                        .contains(&link_id);
                                    let is_clean = clean_omenchat_links
                                        .lock()
                                        .expect("native clean OMENchat link lock")
                                        .contains_key(&link_id);
                                    (is_active, is_clean)
                                };
                                let is_omenchat = is_active || is_clean;
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 clean link-event data observed link_id={} destination={} context=0x{:02x} bytes={} active={} tracked_clean={}",
                                    hex_encode(&link_id),
                                    event.address_hash.to_hex_string(),
                                    payload.context() as u8,
                                    payload.len(),
                                    is_active,
                                    is_clean
                                )));
                                if !is_omenchat {
                                    #[cfg(feature = "native-lxmf")]
                                    if clean_omenchat_frame_context(payload.context()) {
                                        match decode_native_lxmf_payload_bounded(
                                            payload.as_slice().to_vec(),
                                            attachments_dir.clone(),
                                            clean_destination_identities.clone(),
                                        )
                                        .await
                                        .and_then(|decoded| decoded.message)
                                        {
                                            Ok(message) => {
                                                let should_emit = clean_lxmf_should_emit_message(
                                                    &clean_lxmf_seen_messages,
                                                    &message,
                                                );
                                                let _ = event_tx.send(RuntimeBusEvent::Debug(
                                                    format!(
                                                        "native Reticulum 0.9 clean LXMF link-event decoded peer={} message_id={} duplicate={}",
                                                        message.peer_hash,
                                                        message
                                                            .message_id
                                                            .as_deref()
                                                            .unwrap_or("none"),
                                                        !should_emit
                                                    ),
                                                ));
                                                if should_emit {
                                                    let _ = event_tx.send(
                                                        RuntimeBusEvent::MessageReceived(message),
                                                    );
                                                }
                                            }
                                            Err(error)
                                                if !native_lxmf_decode_error_is_truncated(
                                                    &error,
                                                ) =>
                                            {
                                                emit_native_lxmf_rejection(
                                                    &event_tx,
                                                    "outbound-link-data",
                                                    &error,
                                                );
                                            }
                                            Err(_) => {}
                                        }
                                    }
                                    continue;
                                }
                                if !clean_omenchat_frame_context(payload.context()) {
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 OMENchat clean link-event ignored non-data context link_id={} context=0x{:02x}",
                                        hex_encode(&link_id),
                                        payload.context() as u8
                                    )));
                                    continue;
                                }
                                let frame_bytes = payload.as_slice().to_vec();
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 OMENchat clean link-event frame received link_id={} bytes={}",
                                    hex_encode(&link_id),
                                    frame_bytes.len()
                                )));
                                let _ = event_tx.send(RuntimeBusEvent::OmenChatLinkData(
                                    OmenChatLinkData {
                                        link_id,
                                        frame_bytes,
                                    },
                                ));
                            }
                            rns_transport::destination::link::LinkEvent::Activated
                            | rns_transport::destination::link::LinkEvent::PeerIdentified(_) => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    {
        let mut link_events = transport.in_link_events();
        let active_omenchat_links = active_omenchat_links.clone();
        let clean_omenchat_links = clean_omenchat_links.clone();
        let clean_destination_identities = clean_destination_identities.clone();
        let attachments_dir = attachments_dir.clone();
        let event_tx = event_tx.clone();
        #[cfg(feature = "native-lxmf")]
        let clean_lxmf_seen_messages = clean_lxmf_seen_messages.clone();
        tokio::spawn(async move {
            loop {
                match link_events.recv().await {
                    Ok(event) => {
                        let link_id = address_hash_to_link_id(event.id);
                        if let rns_transport::destination::link::LinkEvent::Data(payload) =
                            event.event
                        {
                            let (is_active, is_clean) = {
                                let is_active = active_omenchat_links
                                    .lock()
                                    .expect("native active OMENchat link lock")
                                    .contains(&link_id);
                                let is_clean = clean_omenchat_links
                                    .lock()
                                    .expect("native clean OMENchat link lock")
                                    .contains_key(&link_id);
                                (is_active, is_clean)
                            };
                            let is_omenchat = is_active || is_clean;
                            let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native Reticulum 0.9 clean inbound link-event data observed link_id={} destination={} context=0x{:02x} bytes={} active={} tracked_clean={}",
                                hex_encode(&link_id),
                                event.address_hash.to_hex_string(),
                                payload.context() as u8,
                                payload.len(),
                                is_active,
                                is_clean
                            )));
                            if !is_omenchat {
                                #[cfg(feature = "native-lxmf")]
                                if clean_omenchat_frame_context(payload.context()) {
                                    match decode_native_lxmf_payload_bounded(
                                        payload.as_slice().to_vec(),
                                        attachments_dir.clone(),
                                        clean_destination_identities.clone(),
                                    )
                                    .await
                                    .and_then(|decoded| decoded.message)
                                    {
                                        Ok(message) => {
                                            let should_emit = clean_lxmf_should_emit_message(
                                                &clean_lxmf_seen_messages,
                                                &message,
                                            );
                                            let _ = event_tx.send(RuntimeBusEvent::Debug(
                                                format!(
                                                    "native Reticulum 0.9 clean LXMF inbound link-event decoded peer={} message_id={} duplicate={}",
                                                    message.peer_hash,
                                                    message.message_id.as_deref().unwrap_or("none"),
                                                    !should_emit
                                                ),
                                            ));
                                            if should_emit {
                                                let _ = event_tx.send(
                                                    RuntimeBusEvent::MessageReceived(message),
                                                );
                                            }
                                        }
                                        Err(error)
                                            if !native_lxmf_decode_error_is_truncated(&error) =>
                                        {
                                            emit_native_lxmf_rejection(
                                                &event_tx,
                                                "inbound-link-data",
                                                &error,
                                            );
                                        }
                                        Err(_) => {}
                                    }
                                }
                                continue;
                            }
                            if !clean_omenchat_frame_context(payload.context()) {
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 OMENchat clean inbound link-event ignored non-data context link_id={} context=0x{:02x}",
                                    hex_encode(&link_id),
                                    payload.context() as u8
                                )));
                                continue;
                            }
                            let frame_bytes = payload.as_slice().to_vec();
                            let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native Reticulum 0.9 OMENchat clean inbound link-event frame received link_id={} bytes={}",
                                hex_encode(&link_id),
                                frame_bytes.len()
                            )));
                            let _ = event_tx.send(RuntimeBusEvent::OmenChatLinkData(
                                OmenChatLinkData {
                                    link_id,
                                    frame_bytes,
                                },
                            ));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    {
        let mut received_data_events = transport.received_data_events();
        let active_omenchat_links = active_omenchat_links.clone();
        let clean_omenchat_links = clean_omenchat_links.clone();
        let clean_destination_identities = clean_destination_identities.clone();
        let attachments_dir = attachments_dir.clone();
        let event_tx = event_tx.clone();
        #[cfg(feature = "native-lxmf")]
        let clean_lxmf_seen_messages = clean_lxmf_seen_messages.clone();
        tokio::spawn(async move {
            loop {
                match received_data_events.recv().await {
                    Ok(event) => {
                        if event.payload_mode
                            == rns_transport::transport::ReceivedPayloadMode::FullWire
                        {
                            #[cfg(feature = "native-lxmf")]
                            match decode_native_lxmf_payload_bounded(
                                event.data.as_slice().to_vec(),
                                attachments_dir.clone(),
                                clean_destination_identities.clone(),
                            )
                            .await
                            .and_then(|decoded| decoded.message)
                            {
                                Ok(message) => {
                                    let should_emit = clean_lxmf_should_emit_message(
                                        &clean_lxmf_seen_messages,
                                        &message,
                                    );
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 clean LXMF full-wire decoded destination={} peer={} message_id={} duplicate={}",
                                        event.destination.to_hex_string(),
                                        message.peer_hash,
                                        message.message_id.as_deref().unwrap_or("none"),
                                        !should_emit
                                    )));
                                    if should_emit {
                                        let _ = event_tx
                                            .send(RuntimeBusEvent::MessageReceived(message));
                                    }
                                    continue;
                                }
                                Err(error) if !native_lxmf_decode_error_is_truncated(&error) => {
                                    emit_native_lxmf_rejection(
                                        &event_tx,
                                        "received-full-wire",
                                        &error,
                                    );
                                    continue;
                                }
                                Err(_) => {}
                            }
                            let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                "native Reticulum 0.9 clean received-data full-wire observed destination={} context={} bytes={}; waiting for link-event decode",
                                event.destination.to_hex_string(),
                                event.context
                                    .map(|context| format!("0x{:02x}", context as u8))
                                    .unwrap_or_else(|| "none".into()),
                                event.data.as_slice().len()
                            )));
                            continue;
                        }
                        if !clean_omenchat_optional_frame_context(event.context) {
                            continue;
                        }
                        let event_link_id = address_hash_to_link_id(event.destination);
                        let link_id = {
                            let active_links = active_omenchat_links
                                .lock()
                                .expect("native active OMENchat link lock");
                            if active_links.contains(&event_link_id) {
                                Some(event_link_id)
                            } else {
                                drop(active_links);
                                clean_omenchat_links
                                    .lock()
                                    .expect("native clean OMENchat link lock")
                                    .iter()
                                    .find_map(|(link_id, clean_link)| {
                                        (clean_link.link_id == event.destination
                                            || clean_link.destination_hash == event.destination)
                                            .then_some(*link_id)
                                    })
                            }
                        };
                        let Some(link_id) = link_id else {
                            continue;
                        };
                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum 0.9 OMENchat clean frame received link_id={} source_hash={} bytes={}",
                            hex_encode(&link_id),
                            event.destination.to_hex_string(),
                            event.data.as_slice().len()
                        )));
                        let _ =
                            event_tx.send(RuntimeBusEvent::OmenChatLinkData(OmenChatLinkData {
                                link_id,
                                frame_bytes: event.data.as_slice().to_vec(),
                            }));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    {
        let mut resource_events = transport.resource_events();
        #[cfg(feature = "native-lxmf")]
        let pending_lxmf_proofs = pending_lxmf_proofs.clone();
        let active_omenchat_links = active_omenchat_links.clone();
        let clean_omenchat_links = clean_omenchat_links.clone();
        let clean_destination_identities = clean_destination_identities.clone();
        let attachments_dir = attachments_dir.clone();
        let event_tx = event_tx.clone();
        #[cfg(feature = "native-lxmf")]
        let clean_lxmf_seen_messages = clean_lxmf_seen_messages.clone();
        tokio::spawn(async move {
            loop {
                match resource_events.recv().await {
                    Ok(event) => {
                        let event_link_id = address_hash_to_link_id(event.link_id);
                        let link_id = {
                            let active_links = active_omenchat_links
                                .lock()
                                .expect("native active OMENchat link lock");
                            if active_links.contains(&event_link_id) {
                                Some(event_link_id)
                            } else {
                                drop(active_links);
                                clean_omenchat_links
                                    .lock()
                                    .expect("native clean OMENchat link lock")
                                    .iter()
                                    .find_map(|(link_id, clean_link)| {
                                        (clean_link.link_id == event.link_id
                                            || clean_link.destination_hash == event.link_id)
                                            .then_some(*link_id)
                                    })
                            }
                        };
                        let mut complete = match event.kind {
                            rns_transport::resource::ResourceEventKind::Progress(progress) => {
                                if let Some(link_id) = link_id {
                                    let transfer_id = hex_encode(event.hash.as_slice());
                                    let _ = event_tx.send(RuntimeBusEvent::ResourceProgress(
                                        ResourceProgressEvent {
                                            transfer_id: transfer_id.clone(),
                                            received: progress.received_bytes,
                                            total: Some(progress.total_bytes),
                                            operation_id: None,
                                            source: Some("omenchat".into()),
                                            purpose: Some("omenchat-resource".into()),
                                            direction: Some("inbound".into()),
                                            peer: Some(hex_encode(&link_id)),
                                        },
                                    ));
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 OMENchat resource progress link_id={} resource={} bytes={}/{} parts={}/{}",
                                        hex_encode(&link_id),
                                        transfer_id,
                                        progress.received_bytes,
                                        progress.total_bytes,
                                        progress.received_parts,
                                        progress.total_parts
                                    )));
                                }
                                continue;
                            }
                            rns_transport::resource::ResourceEventKind::SegmentComplete(
                                segment,
                            ) => {
                                if let Some(link_id) = link_id {
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 OMENchat resource segment complete link_id={} resource={} segment={}/{} total_bytes={}",
                                        hex_encode(&link_id),
                                        hex_encode(event.hash.as_slice()),
                                        segment.segment_index,
                                        segment.total_segments,
                                        segment.total_data_size
                                    )));
                                }
                                // The transport emits the final assembled payload through
                                // `Complete`; retaining a segment here would duplicate data.
                                continue;
                            }
                            rns_transport::resource::ResourceEventKind::Complete(complete) => {
                                complete
                            }
                            rns_transport::resource::ResourceEventKind::OutboundComplete => {
                                #[cfg(feature = "native-lxmf")]
                                if emit_clean_lxmf_resource_terminal(
                                    &event_tx,
                                    &pending_lxmf_proofs,
                                    event.hash.as_slice(),
                                    CleanLxmfResourceTerminal::Complete,
                                ) {
                                    continue;
                                }
                                if let Some(link_id) = link_id {
                                    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                        ResourceLifecycleEvent {
                                            transfer_id: hex_encode(event.hash.as_slice()),
                                            state: ResourceLifecycleState::Complete,
                                            bytes: None,
                                            reason: None,
                                            operation_id: None,
                                            source: Some("omenchat".into()),
                                            purpose: Some("omenchat-resource".into()),
                                            direction: Some("outbound".into()),
                                            peer: Some(hex_encode(&link_id)),
                                        },
                                    ));
                                }
                                continue;
                            }
                            rns_transport::resource::ResourceEventKind::OutboundFailed => {
                                #[cfg(feature = "native-lxmf")]
                                if emit_clean_lxmf_resource_terminal(
                                    &event_tx,
                                    &pending_lxmf_proofs,
                                    event.hash.as_slice(),
                                    CleanLxmfResourceTerminal::Failed,
                                ) {
                                    continue;
                                }
                                if let Some(link_id) = link_id {
                                    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                        ResourceLifecycleEvent {
                                            transfer_id: hex_encode(event.hash.as_slice()),
                                            state: ResourceLifecycleState::Failed,
                                            bytes: None,
                                            reason: Some(
                                                "outbound OMENchat resource failed".into(),
                                            ),
                                            operation_id: None,
                                            source: Some("omenchat".into()),
                                            purpose: Some("omenchat-resource".into()),
                                            direction: Some("outbound".into()),
                                            peer: Some(hex_encode(&link_id)),
                                        },
                                    ));
                                }
                                continue;
                            }
                            rns_transport::resource::ResourceEventKind::InboundFailed(failure) => {
                                if let Some(link_id) = link_id {
                                    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                        ResourceLifecycleEvent {
                                            transfer_id: hex_encode(event.hash.as_slice()),
                                            state: ResourceLifecycleState::Failed,
                                            bytes: None,
                                            reason: Some(failure.reason),
                                            operation_id: None,
                                            source: Some("omenchat".into()),
                                            purpose: Some("omenchat-resource".into()),
                                            direction: Some("inbound".into()),
                                            peer: Some(hex_encode(&link_id)),
                                        },
                                    ));
                                }
                                continue;
                            }
                            rns_transport::resource::ResourceEventKind::OutboundCancelled => {
                                #[cfg(feature = "native-lxmf")]
                                if emit_clean_lxmf_resource_terminal(
                                    &event_tx,
                                    &pending_lxmf_proofs,
                                    event.hash.as_slice(),
                                    CleanLxmfResourceTerminal::Cancelled,
                                ) {
                                    continue;
                                }
                                if let Some(link_id) = link_id {
                                    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                        ResourceLifecycleEvent {
                                            transfer_id: hex_encode(event.hash.as_slice()),
                                            state: ResourceLifecycleState::Cancelled,
                                            bytes: None,
                                            reason: Some("cancelled".into()),
                                            operation_id: None,
                                            source: Some("omenchat".into()),
                                            purpose: Some("omenchat-resource".into()),
                                            direction: Some("outbound".into()),
                                            peer: Some(hex_encode(&link_id)),
                                        },
                                    ));
                                }
                                continue;
                            }
                        };
                        if let Some(limit) =
                            clean_omenchat_resource_limit(complete.metadata.as_deref())
                        {
                            if complete.data.len() > limit {
                                let transfer_id = hex_encode(event.hash.as_slice());
                                if let Some(link_id) = link_id {
                                    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                                        ResourceLifecycleEvent {
                                            transfer_id: transfer_id.clone(),
                                            state: ResourceLifecycleState::Failed,
                                            bytes: Some(complete.data.len() as u64),
                                            reason: Some(format!(
                                                "inbound OMENchat resource exceeds {limit} byte limit"
                                            )),
                                            operation_id: None,
                                            source: Some("omenchat".into()),
                                            purpose: Some("omenchat-resource".into()),
                                            direction: Some("inbound".into()),
                                            peer: Some(hex_encode(&link_id)),
                                        },
                                    ));
                                }
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 OMENchat resource rejected before application event forwarding resource={transfer_id} bytes={} limit={limit}",
                                    complete.data.len()
                                )));
                                continue;
                            }
                        }
                        let is_metadata_frame =
                            complete.metadata.as_deref().is_some_and(|metadata| {
                                metadata.starts_with(OMENCHAT_FRAME_RESOURCE_METADATA)
                            });
                        if is_metadata_frame {
                            if let Some(link_id) = link_id {
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 OMENchat frame resource received link_id={} source_hash={} bytes={}",
                                    hex_encode(&link_id),
                                    event.link_id.to_hex_string(),
                                    complete.data.len()
                                )));
                                let _ = event_tx.send(RuntimeBusEvent::OmenChatLinkData(
                                    OmenChatLinkData {
                                        link_id,
                                        frame_bytes: complete.data,
                                    },
                                ));
                            }
                            continue;
                        }
                        let is_metadata_omenchat =
                            complete.metadata.as_deref().is_some_and(|metadata| {
                                metadata.starts_with(OMENCHAT_RESOURCE_METADATA_PREFIX)
                            });
                        #[cfg(feature = "native-lxmf")]
                        if !is_metadata_omenchat {
                            let decoded = match decode_native_lxmf_payload_bounded(
                                std::mem::take(&mut complete.data),
                                attachments_dir.clone(),
                                clean_destination_identities.clone(),
                            )
                            .await
                            {
                                Ok(decoded) => decoded,
                                Err(error) => {
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 clean LXMF blocking decode failed: {error}"
                                    )));
                                    continue;
                                }
                            };
                            complete.data = decoded.bytes;
                            match decoded.message {
                                Ok(message) => {
                                    let should_emit = clean_lxmf_should_emit_message(
                                        &clean_lxmf_seen_messages,
                                        &message,
                                    );
                                    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                        "native Reticulum 0.9 clean LXMF resource decoded source_hash={} peer={} message_id={} duplicate={}",
                                        event.link_id.to_hex_string(),
                                        message.peer_hash,
                                        message.message_id.as_deref().unwrap_or("none"),
                                        !should_emit
                                    )));
                                    if should_emit {
                                        let _ = event_tx
                                            .send(RuntimeBusEvent::MessageReceived(message));
                                    }
                                    continue;
                                }
                                Err(error) if !native_lxmf_decode_error_is_truncated(&error) => {
                                    emit_native_lxmf_rejection(&event_tx, "resource", &error);
                                }
                                Err(_) => {}
                            }
                        }
                        let Some(link_id) = link_id else {
                            if is_metadata_omenchat {
                                let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native Reticulum 0.9 OMENchat resource ignored with no active link source_hash={} bytes={}",
                                    event.link_id.to_hex_string(),
                                    complete.data.len()
                                )));
                            }
                            continue;
                        };
                        if !is_metadata_omenchat {
                            continue;
                        }
                        let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(
                            ResourceLifecycleEvent {
                                transfer_id: hex_encode(event.hash.as_slice()),
                                state: ResourceLifecycleState::Complete,
                                bytes: Some(complete.data.len() as u64),
                                reason: None,
                                operation_id: None,
                                source: Some("omenchat".into()),
                                purpose: Some("omenchat-resource".into()),
                                direction: Some("inbound".into()),
                                peer: Some(hex_encode(&link_id)),
                            },
                        ));
                        let _ = event_tx.send(RuntimeBusEvent::OmenChatResourceData(
                            OmenChatResourceData {
                                link_id,
                                data: complete.data,
                                metadata: complete.metadata,
                            },
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
fn clean_lxmf_should_emit_message(
    seen_messages: &Arc<Mutex<BTreeMap<String, tokio::time::Instant>>>,
    message: &MessageSummary,
) -> bool {
    let Some(message_id) = message.message_id.as_deref() else {
        return true;
    };
    let now = tokio::time::Instant::now();
    let mut seen = seen_messages
        .lock()
        .expect("native clean lxmf seen message lock");
    seen.retain(|_, observed| now.duration_since(*observed) < Duration::from_secs(300));
    seen.insert(message_id.to_string(), now).is_none()
}

#[cfg(all(feature = "native-rns-net", any()))]
fn omenchat_link_error_is_handshake_timeout(error: &AppError) -> bool {
    let lower = error.to_string().to_ascii_lowercase();
    lower.contains("timed out") && lower.contains("link establishment")
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_transient_id(lxmf_data: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(lxmf_data);
    let mut id = [0u8; 32];
    id.copy_from_slice(&digest);
    id
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_select_sync_ids(
    available: Vec<[u8; 32]>,
    delivered_ids: &BTreeMap<String, f64>,
    limit: Option<u32>,
) -> (Vec<[u8; 32]>, Vec<[u8; 32]>) {
    let max_wants = limit.unwrap_or(u32::MAX) as usize;
    let mut wants = Vec::new();
    let mut haves = Vec::new();
    for id in available {
        if DeliveredTransientIdStore::has_delivered(delivered_ids, &id) {
            haves.push(id);
        } else if wants.len() < max_wants {
            wants.push(id);
        }
    }
    (wants, haves)
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
fn clean_propagation_admit_response_transient(
    seen: &mut BTreeSet<[u8; 32]>,
    transient_id: [u8; 32],
) -> bool {
    seen.insert(transient_id)
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
fn clean_propagation_admit_sender_path_request(
    requested: &mut BTreeSet<[u8; 16]>,
    source_hash: [u8; 16],
) -> bool {
    requested.len() < CLEAN_PROPAGATION_SENDER_PATH_REQUEST_MAX && requested.insert(source_hash)
}

#[cfg(feature = "native-lxmf")]
fn emit_propagation_sync_event(
    tx: &broadcast::Sender<RuntimeBusEvent>,
    stage: PropagationSyncStage,
    status: PropagationSyncEventStatus,
    destination_hash: Option<&str>,
    detail: impl Into<String>,
    counts: impl IntoIterator<Item = (&'static str, usize)>,
) {
    let _ = tx.send(RuntimeBusEvent::PropagationSync(PropagationSyncEvent {
        stage,
        status,
        destination_hash: destination_hash.map(ToOwned::to_owned),
        detail: detail.into(),
        counts: counts
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    }));
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_propagation_sync_cleanup_message(
    reason: &str,
    link_id: [u8; 16],
    torn_down: bool,
) -> String {
    format!(
        "native LXMF propagation sync link cleanup reason={reason} link_id={} torn_down={torn_down}",
        hex_encode(&link_id)
    )
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
async fn cleanup_native_lxmf_propagation_sync_link(
    client: &RnsNetPageRequestClient,
    tx: &broadcast::Sender<RuntimeBusEvent>,
    destination_hash: &str,
    link_id: [u8; 16],
    reason: &str,
) -> bool {
    let torn_down = client.teardown_link(link_id).await;
    let _ = tx.send(RuntimeBusEvent::Debug(
        native_lxmf_propagation_sync_cleanup_message(reason, link_id, torn_down),
    ));
    emit_propagation_sync_event(
        tx,
        PropagationSyncStage::Complete,
        PropagationSyncEventStatus::Progress,
        Some(destination_hash),
        format!("cleaned up propagation sync link after {reason}"),
        [("link_torn_down", if torn_down { 1 } else { 0 })],
    );
    torn_down
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_parse_transient_id_list(bytes: &[u8]) -> AppResult<Vec<[u8; 32]>> {
    let value = native_lxmf_unpack_value(bytes, 256 * 1024, 32, 4096, 4097, 2)?;
    let rmpv::Value::Array(items) = value else {
        return Err(AppError::Runtime(
            "LXMF propagation list response was not an array".into(),
        ));
    };
    let mut ids = Vec::new();
    for item in items {
        let rmpv::Value::Binary(bytes) = item else {
            return Err(AppError::Runtime(
                "LXMF propagation list entry was not binary".into(),
            ));
        };
        if bytes.len() != 32 {
            return Err(AppError::Runtime(
                "LXMF propagation list entry was not a 32 byte transient id".into(),
            ));
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(&bytes);
        ids.push(id);
    }
    Ok(ids)
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_parse_propagation_payloads(bytes: &[u8]) -> AppResult<Vec<Vec<u8>>> {
    let value = native_lxmf_unpack_value(
        bytes,
        32 * 1024 * 1024,
        crate::runtime::native_lxmf::codec::MAX_LXMF_PROPAGATION_ENTRY_BYTES,
        4096,
        8192,
        4,
    )?;
    let rmpv::Value::Array(items) = value else {
        return Err(AppError::Runtime(
            "LXMF propagation get response was not an array".into(),
        ));
    };
    items
        .into_iter()
        .map(|item| match item {
            rmpv::Value::Binary(bytes) => Ok(bytes),
            _ => Err(AppError::Runtime(
                "LXMF propagation get response entry was not binary".into(),
            )),
        })
        .collect()
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_payload_candidates(bytes: Vec<u8>) -> Vec<Vec<u8>> {
    match crate::runtime::native_lxmf::codec::propagation_envelope_entries(&bytes) {
        Ok(entries) => entries,
        Err(_) => vec![bytes],
    }
}

#[cfg(feature = "native-lxmf")]
fn generate_propagation_stamp_owned(
    lxm_data: Vec<u8>,
    transient_id: [u8; 32],
    target_cost: u8,
) -> (
    AppResult<crate::runtime::native_lxmf::codec::GeneratedPropagationStamp>,
    Vec<u8>,
) {
    let stamp = crate::runtime::native_lxmf::codec::generate_propagation_stamp_for_transient(
        &lxm_data,
        transient_id,
        target_cost,
        crate::runtime::native_lxmf::codec::DEFAULT_PROPAGATION_STAMP_MAX_ATTEMPTS,
    );
    (stamp, lxm_data)
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_unpack_value(
    bytes: &[u8],
    max_bytes: usize,
    max_scalar_bytes: usize,
    max_container_items: usize,
    max_total_values: usize,
    max_depth: usize,
) -> AppResult<rmpv::Value> {
    crate::msgpack::validate_msgpack_with_limits(
        bytes,
        max_bytes,
        max_scalar_bytes,
        max_container_items,
        max_total_values,
        max_depth,
    )
    .map_err(|error| {
        AppError::Runtime(format!(
            "LXMF propagation msgpack response rejected: {error}"
        ))
    })?;
    let mut cursor = std::io::Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor)
        .map_err(|_| AppError::Runtime("LXMF propagation msgpack decode failed".into()))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(AppError::Runtime(
            "LXMF propagation msgpack response had trailing data".into(),
        ));
    }
    Ok(value)
}

fn should_emit_directory_announce(payload: &AnnouncePayload) -> bool {
    !matches!(payload.kind, DirectoryKind::Unknown)
}

fn clean_propagation_app_data_valid(app_data: &[u8]) -> bool {
    #[cfg(feature = "native-lxmf")]
    {
        crate::runtime::native_lxmf::codec::propagation_announce_data_is_valid(app_data)
    }
    #[cfg(not(feature = "native-lxmf"))]
    {
        !app_data.is_empty()
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_propagation_app_data_valid(key: &RnsNetAnnounceKey) -> bool {
    let Some(app_data) = key.app_data.as_deref() else {
        return false;
    };
    if rns_net_announce_kind(key) != DirectoryKind::Propagation {
        return !app_data.is_empty();
    }
    #[cfg(feature = "native-lxmf")]
    {
        crate::runtime::native_lxmf::codec::propagation_announce_data_is_valid(app_data)
    }
    #[cfg(not(feature = "native-lxmf"))]
    {
        !app_data.is_empty()
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_announce_payload(key: &RnsNetAnnounceKey) -> AnnouncePayload {
    let destination_hash = hex_encode(&key.destination_hash);
    let kind = rns_net_announce_kind(key);
    let app_data = key.app_data.as_deref().unwrap_or_default();
    let display_name = display_name_for_kind(&kind, app_data)
        .unwrap_or_else(|| destination_hash.chars().take(12).collect());
    let lxmf_stamp_cost = rns_net_lxmf_delivery_stamp_cost(&kind, app_data);
    let associated_hash = match kind {
        DirectoryKind::Node => Some(rns_net_associated_hash(
            &key.identity_hash,
            "lxmf",
            "delivery",
        )),
        DirectoryKind::Peer => Some(rns_net_associated_hash(
            &key.identity_hash,
            "nomadnetwork",
            "node",
        )),
        DirectoryKind::Propagation => Some(rns_net_associated_hash(
            &key.identity_hash,
            "lxmf",
            "delivery",
        )),
        DirectoryKind::OmenChat => None,
        DirectoryKind::Unknown => None,
    };
    let node_associated_hash = (kind == DirectoryKind::Propagation)
        .then(|| rns_net_associated_hash(&key.identity_hash, "nomadnetwork", "node"));

    AnnouncePayload {
        destination_hash,
        identity_hash: Some(hex_encode(&key.identity_hash)),
        display_name,
        kind,
        associated_hash,
        node_associated_hash,
        has_ratchet: false,
        lxmf_stamp_cost,
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_announce_kind(key: &RnsNetAnnounceKey) -> DirectoryKind {
    if key.destination_hash == rns_net_destination_hash(&key.identity_hash, "nomadnetwork", "node")
    {
        DirectoryKind::Node
    } else if key.destination_hash
        == rns_net_destination_hash(&key.identity_hash, "lxmf", "delivery")
    {
        DirectoryKind::Peer
    } else if key.destination_hash
        == rns_net_destination_hash(&key.identity_hash, "lxmf", "propagation")
    {
        DirectoryKind::Propagation
    } else if key.destination_hash
        == rns_net_destination_hash(&key.identity_hash, "omenchat", "node")
    {
        DirectoryKind::OmenChat
    } else {
        DirectoryKind::Unknown
    }
}

#[cfg(all(feature = "native-rns-net", feature = "native-lxmf"))]
fn rns_net_lxmf_delivery_stamp_cost(kind: &DirectoryKind, app_data: &[u8]) -> Option<u8> {
    if *kind == DirectoryKind::Peer {
        crate::runtime::native_lxmf::codec::delivery_announce_stamp_cost(app_data)
    } else {
        None
    }
}

#[cfg(all(feature = "native-rns-net", not(feature = "native-lxmf")))]
fn rns_net_lxmf_delivery_stamp_cost(_kind: &DirectoryKind, _app_data: &[u8]) -> Option<u8> {
    None
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_sibling_destination_hashes(key: &RnsNetAnnounceKey) -> [[u8; 16]; 3] {
    [
        rns_net_destination_hash(&key.identity_hash, "nomadnetwork", "node"),
        rns_net_destination_hash(&key.identity_hash, "lxmf", "delivery"),
        rns_net_destination_hash(&key.identity_hash, "lxmf", "propagation"),
    ]
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_associated_hash(identity_hash: &[u8; 16], app_name: &str, aspect: &str) -> String {
    hex_encode(&rns_net_destination_hash(identity_hash, app_name, aspect))
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_destination_hash(identity_hash: &[u8; 16], app_name: &str, aspect: &str) -> [u8; 16] {
    rns_core::destination::destination_hash(app_name, &[aspect], Some(identity_hash))
}

#[cfg(all(feature = "native-rns-net", any()))]
fn parse_rns_net_destination_hash(destination_hash: &str) -> AppResult<[u8; 16]> {
    let destination = parse_transport_destination_hash(destination_hash)?;
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(destination.as_slice());
    Ok(bytes)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_page_fetch_error(
    plan: &NativeFetchPlan,
    stage: NativePageFetchFailureStage,
    detail: impl Into<String>,
) -> AppError {
    AppError::from(NativeRuntimeError::PageFetchFailed {
        destination: plan.request.destination_hash.to_hex_string(),
        stage,
        detail: detail.into(),
    })
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_failure_stage_from_probe_stage(
    stage: &PageFetchProbeStage,
) -> NativePageFetchFailureStage {
    match stage {
        PageFetchProbeStage::AddressParse | PageFetchProbeStage::RuntimeSetup => {
            NativePageFetchFailureStage::Runtime
        }
        PageFetchProbeStage::DestinationIdentity => {
            NativePageFetchFailureStage::DestinationIdentity
        }
        PageFetchProbeStage::PathDiscovery => NativePageFetchFailureStage::PathDiscovery,
        PageFetchProbeStage::LinkSetup => NativePageFetchFailureStage::LinkSetup,
        PageFetchProbeStage::RequestSend => NativePageFetchFailureStage::RequestSend,
        PageFetchProbeStage::ResponseWait => NativePageFetchFailureStage::ResponseWait,
        PageFetchProbeStage::ResponseDecode => NativePageFetchFailureStage::ResponseDecode,
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_observed_reused_page_link(steps: &[PageFetchProbeStep]) -> bool {
    steps.iter().any(|step| {
        step.stage == PageFetchProbeStage::LinkSetup
            && step.ok
            && step.detail.contains("reused active page link")
    })
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_observed_response_wait_failed(steps: &[PageFetchProbeStep]) -> bool {
    steps
        .iter()
        .any(|step| step.stage == PageFetchProbeStage::ResponseWait && !step.ok)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_observed_request_send_failed(steps: &[PageFetchProbeStep]) -> bool {
    steps
        .iter()
        .any(|step| step.stage == PageFetchProbeStage::RequestSend && !step.ok)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn rns_net_observed_stale_reused_page_link_failure(steps: &[PageFetchProbeStep]) -> bool {
    rns_net_observed_response_wait_failed(steps) || rns_net_observed_request_send_failed(steps)
}

fn filename_from_native_download_path(path: &str) -> String {
    path.rsplit('/')
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or("download.bin")
        .to_string()
}

fn unsupported(operation: &str) -> AppError {
    AppError::from(NativeRuntimeError::Unsupported(match operation {
        "announce_identity" => "native Reticulum announce is not implemented yet",
        "fetch_page" => "native Reticulum page fetch is not implemented yet",
        "download_file" => "native Reticulum download is not implemented yet",
        "list_messages" => "native LXMF list is not implemented yet",
        "send_message" => "native LXMF send is not implemented yet",
        "send_message_propagated" => {
            "native LXMF propagated send is not implemented yet; direct delivery is the first native path"
        }
        "create_contact" => "native LXMF contact creation is not implemented yet",
        "set_outbound_propagation_node" => {
            "native LXMF propagation node selection is not implemented yet"
        }
        "get_outbound_propagation_node" => {
            "native LXMF propagation node lookup is not implemented yet"
        }
        "sync_propagation_messages" => "native LXMF propagation sync is not implemented yet",
        "request_path" => "native Reticulum path request is not implemented yet",
        "warm_paths" => "native Reticulum path warming is not implemented yet",
        _ => "native Reticulum operation is not implemented yet",
    }))
}

#[cfg(all(feature = "native-rns-net", any()))]
fn sync_managed_rns_net_identity_config(
    reticulum_config_dir: &Path,
    identity_path: &Path,
) -> AppResult<()> {
    std::fs::create_dir_all(reticulum_config_dir)?;
    let config_path = reticulum_config_dir.join("config");
    let existing = std::fs::read_to_string(&config_path).unwrap_or_default();
    let rendered = render_config_with_network_identity(&existing, identity_path);
    if rendered != existing {
        std::fs::write(&config_path, rendered)?;
    }
    Ok(())
}

#[cfg(all(feature = "native-rns-net", any()))]
fn managed_rns_net_identity_config_matches(
    reticulum_config_dir: &Path,
    identity_path: &Path,
) -> AppResult<bool> {
    let config_path = reticulum_config_dir.join("config");
    let existing = std::fs::read_to_string(config_path)?;
    Ok(read_network_identity_value(&existing).as_deref()
        == Some(&identity_path.display().to_string()))
}

#[cfg(all(feature = "native-rns-net", any()))]
fn render_config_with_network_identity(existing: &str, identity_path: &Path) -> String {
    let identity_line = format!("network_identity = {}", identity_path.display());
    let mut lines = if existing.trim().is_empty() {
        vec![
            "[reticulum]".to_string(),
            "share_instance = Yes".to_string(),
            "instance_name = omenbrowser_rs".to_string(),
            "enable_transport = No".to_string(),
        ]
    } else {
        existing.lines().map(str::to_string).collect()
    };

    let mut in_reticulum = false;
    let mut saw_reticulum = false;
    let mut inserted = false;
    for index in 0..lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_reticulum && !inserted {
                lines.insert(index, identity_line.clone());
                inserted = true;
                break;
            }
            in_reticulum = trimmed.eq_ignore_ascii_case("[reticulum]");
            saw_reticulum |= in_reticulum;
            continue;
        }
        if in_reticulum && trimmed.starts_with("network_identity") {
            lines[index] = identity_line.clone();
            inserted = true;
            break;
        }
    }

    if !inserted {
        if saw_reticulum {
            lines.push(identity_line);
        } else {
            if !lines.last().is_none_or(|line| line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[reticulum]".into());
            lines.push(identity_line);
        }
    }

    format!("{}\n", lines.join("\n").trim_end())
}

#[cfg(all(feature = "native-rns-net", any()))]
fn read_network_identity_value(config: &str) -> Option<String> {
    let mut in_reticulum = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_reticulum = trimmed.eq_ignore_ascii_case("[reticulum]");
            continue;
        }
        if in_reticulum {
            let Some((key, value)) = trimmed.split_once('=') else {
                continue;
            };
            if key.trim() == "network_identity" {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

#[cfg(feature = "native-lxmf")]
fn decode_native_lxmf_payload(bytes: &[u8], attachments_dir: &Path) -> AppResult<MessageSummary> {
    let direct = crate::runtime::native_lxmf::codec::decode_wire_message_storing_attachments(
        bytes,
        attachments_dir,
    );
    #[cfg(all(feature = "native-rns-net", any()))]
    {
        direct.or_else(|direct_error| {
            let packet = rns_core::packet::RawPacket::unpack(bytes).map_err(|_| direct_error)?;
            crate::runtime::native_lxmf::codec::decode_wire_message_storing_attachments(
                &packet.data,
                attachments_dir,
            )
        })
    }
    #[cfg(not(all(feature = "native-rns-net", any())))]
    {
        direct
    }
}

#[cfg(feature = "native-lxmf")]
struct NativeLxmfBlockingDecode {
    message: AppResult<MessageSummary>,
    bytes: Vec<u8>,
    unresolved_source_hash: Option<[u8; 16]>,
}

#[cfg(feature = "native-lxmf")]
async fn run_native_lxmf_blocking<T: Send + 'static>(
    gate: Arc<Semaphore>,
    job: impl FnOnce() -> T + Send + 'static,
) -> AppResult<T> {
    let permit = gate
        .acquire_owned()
        .await
        .map_err(|_| AppError::Runtime("native LXMF blocking gate closed".into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        job()
    })
    .await
    .map_err(|error| AppError::Runtime(format!("native LXMF blocking task failed: {error}")))
}

#[cfg(feature = "native-lxmf")]
async fn decode_native_lxmf_payload_bounded(
    bytes: Vec<u8>,
    attachments_dir: PathBuf,
    destination_identities: Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>,
) -> AppResult<NativeLxmfBlockingDecode> {
    run_native_lxmf_blocking(NATIVE_LXMF_DECODE_BLOCKING_GATE.clone(), move || {
        let mut unresolved_source_hash = None;
        let message = (|| {
            let source_hash = crate::runtime::native_lxmf::codec::wire_source_hash(&bytes)?;
            let source_hash_hex = hex_encode(&source_hash);
            let source_identity = destination_identities
                .lock()
                .expect("native clean destination identity cache lock")
                .get(&source_hash_hex)
                .copied()
                .ok_or_else(|| {
                    unresolved_source_hash = Some(source_hash);
                    AppError::Runtime(format!(
                        "LXMF source identity is not known from an authenticated lxmf.delivery announce: {source_hash_hex}"
                    ))
                })?;
            let source_identity = reticulum_rs::core::identity::Identity::new_from_slices(
                source_identity.public_key_bytes(),
                source_identity.verifying_key_bytes(),
            );
            crate::runtime::native_lxmf::codec::decode_verified_wire_message_storing_attachments(
                &bytes,
                &source_identity,
                &attachments_dir,
            )
        })();
        NativeLxmfBlockingDecode {
            message,
            bytes,
            unresolved_source_hash,
        }
    })
    .await
}

#[cfg(feature = "native-lxmf")]
async fn decode_propagated_lxmf_payload_bounded(
    bytes: Vec<u8>,
    identity_bytes: Vec<u8>,
    attachments_dir: PathBuf,
    destination_identities: Option<Arc<Mutex<BTreeMap<String, rns_transport::identity::Identity>>>>,
) -> AppResult<NativeLxmfBlockingDecode> {
    run_native_lxmf_blocking(NATIVE_LXMF_DECODE_BLOCKING_GATE.clone(), move || {
        let mut unresolved_source_hash = None;
        let message = if let Some(destination_identities) = destination_identities {
            (|| {
                let wire = crate::runtime::native_lxmf::codec::unpack_propagated_lxmf_wire(
                    &bytes,
                    &identity_bytes,
                )?;
                let source_hash = crate::runtime::native_lxmf::codec::wire_source_hash(&wire)?;
                let source_hash_hex = hex_encode(&source_hash);
                let source_identity = destination_identities
                    .lock()
                    .expect("native clean destination identity cache lock")
                    .get(&source_hash_hex)
                    .copied()
                    .ok_or_else(|| {
                        unresolved_source_hash = Some(source_hash);
                        AppError::Runtime(format!(
                            "propagated LXMF source identity is not known from an authenticated lxmf.delivery announce: {source_hash_hex}"
                        ))
                    })?;
                let source_identity = reticulum_rs::core::identity::Identity::new_from_slices(
                    source_identity.public_key_bytes(),
                    source_identity.verifying_key_bytes(),
                );
                crate::runtime::native_lxmf::codec::decode_verified_propagated_wire_message_storing_attachments(
                    &wire,
                    &source_identity,
                    &attachments_dir,
                )
            })()
        } else {
            crate::runtime::native_lxmf::codec::decode_propagated_lxmf_data_storing_attachments(
                &bytes,
                &identity_bytes,
                &attachments_dir,
            )
        };
        NativeLxmfBlockingDecode {
            message,
            bytes,
            unresolved_source_hash,
        }
    })
    .await
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_decode_error_is_truncated(error: &AppError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("wire message too short")
        || message.contains("failed to fill whole buffer")
        || message.contains("io error while reading marker")
}

#[cfg(feature = "native-lxmf")]
fn emit_native_lxmf_rejection(
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
    ingress: &str,
    error: &AppError,
) {
    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
        "native Reticulum 0.9 clean LXMF rejected ingress={ingress}: {error}"
    )));
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn decode_rns_net_lxmf_delivery(
    delivery: &RnsNetLocalDelivery,
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_native_lxmf_payload(&delivery.raw, attachments_dir)
}

fn native_unix_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanLxmfResourceTerminal {
    Complete,
    Failed,
    Cancelled,
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
fn emit_clean_lxmf_resource_offered(
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
    resource_hash: &str,
    message_id: &str,
    peer_hash: &str,
    total: u64,
) {
    let _ = event_tx.send(RuntimeBusEvent::ResourceProgress(ResourceProgressEvent {
        transfer_id: resource_hash.into(),
        received: 0,
        total: Some(total),
        operation_id: Some(message_id.into()),
        source: Some("lxmf".into()),
        purpose: Some("direct-message".into()),
        direction: Some("outbound".into()),
        peer: Some(peer_hash.into()),
    }));
    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(ResourceLifecycleEvent {
        transfer_id: resource_hash.into(),
        state: ResourceLifecycleState::Offered,
        bytes: Some(total),
        reason: None,
        operation_id: Some(message_id.into()),
        source: Some("lxmf".into()),
        purpose: Some("direct-message".into()),
        direction: Some("outbound".into()),
        peer: Some(peer_hash.into()),
    }));
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
fn emit_clean_lxmf_resource_terminal(
    event_tx: &broadcast::Sender<RuntimeBusEvent>,
    pending_lxmf_proofs: &PendingLxmfProofs,
    resource_hash: &[u8],
    terminal: CleanLxmfResourceTerminal,
) -> bool {
    let resource_hash = hex_encode(resource_hash);
    let pending = {
        let mut pending_lxmf_proofs = pending_lxmf_proofs
            .lock()
            .expect("native LXMF resource map lock");
        let Some(pending) = pending_lxmf_proofs.take_correlation(&resource_hash) else {
            return false;
        };
        pending
    };
    let (transfer_state, lifecycle_state, status_state, evidence_kind, failed, reason) =
        match terminal {
            CleanLxmfResourceTerminal::Complete => (
                "resource_completed",
                ResourceLifecycleState::Complete,
                OutboundDeliveryState::SubmittedToRnsNet,
                LxmfDeliveryEvidenceKind::PacketSubmitted,
                false,
                None,
            ),
            CleanLxmfResourceTerminal::Failed => (
                "resource_failed",
                ResourceLifecycleState::Failed,
                OutboundDeliveryState::Failed,
                LxmfDeliveryEvidenceKind::LxmfRouterFailed,
                true,
                Some("outbound clean LXMF resource failed".to_string()),
            ),
            CleanLxmfResourceTerminal::Cancelled => (
                "resource_cancelled",
                ResourceLifecycleState::Cancelled,
                OutboundDeliveryState::Failed,
                LxmfDeliveryEvidenceKind::LxmfRouterFailed,
                true,
                Some("outbound clean LXMF resource cancelled".to_string()),
            ),
        };
    let observed_at = native_unix_timestamp();
    let detail = format!(
        "direct_transfer_state:{transfer_state};resource_hash:{resource_hash};submitted_at:{:.3};receipt_state:{transfer_state}_peer_unconfirmed;delivery_state:{}",
        pending.submitted_at,
        if failed { "failed" } else { "peer_delivery_unconfirmed" }
    );
    let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(ResourceLifecycleEvent {
        transfer_id: resource_hash.clone(),
        state: lifecycle_state,
        bytes: None,
        reason,
        operation_id: None,
        source: Some("lxmf".into()),
        purpose: Some("direct-message".into()),
        direction: Some("outbound".into()),
        peer: Some(pending.peer_hash.clone()),
    }));
    let _ = event_tx.send(RuntimeBusEvent::MessageDeliveryUpdated(OutboundStatus {
        peer_hash: pending.peer_hash.clone(),
        message_id: Some(pending.message_id.clone()),
        delivered: false,
        failed,
        state: status_state,
        evidence: Some(detail.clone()),
        rtt: None,
    }));
    let _ = event_tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
        LxmfDeliveryEvidence {
            peer_hash: pending.peer_hash.clone(),
            message_id: Some(pending.message_id.clone()),
            kind: evidence_kind,
            detail: Some(detail),
            rtt: None,
            observed_at: Some(observed_at),
        },
    ));
    let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
        "native Reticulum 0.9 clean LXMF resource terminal correlated peer={} message_id={} resource_hash={} state={transfer_state}",
        pending.peer_hash, pending.message_id, resource_hash
    )));
    true
}

#[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
fn attachment_summaries_from_paths(paths: &[std::path::PathBuf]) -> Vec<AttachmentSummary> {
    paths
        .iter()
        .map(|path| AttachmentSummary {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            size: std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            path: Some(path.clone()),
        })
        .collect()
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn annotate_native_lxmf_stamp_fields(
    fields: &mut BTreeMap<String, String>,
    outbound: &crate::runtime::native_lxmf::codec::NativeLxmfOutbound,
    direct_stamp: Option<&crate::runtime::native_lxmf::codec::GeneratedDirectStamp>,
) {
    if outbound.include_ticket {
        fields.insert("native_lxmf_reply_ticket_offered".into(), "true".into());
    }
    if outbound.reply_ticket_used {
        fields.insert("native_lxmf_reply_ticket_used".into(), "true".into());
        fields.insert("native_lxmf_stamp_state".into(), "ticket_stamp".into());
    } else if let Some(stamp) = direct_stamp {
        fields.insert("native_lxmf_stamp_state".into(), "direct_stamp".into());
        fields.insert(
            "native_lxmf_direct_stamp_cost".into(),
            stamp.target_cost.to_string(),
        );
        fields.insert(
            "native_lxmf_direct_stamp_value".into(),
            stamp.stamp_value.to_string(),
        );
        fields.insert(
            "native_lxmf_direct_stamp_attempts".into(),
            stamp.attempts.to_string(),
        );
    }
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_pending_direct_summary(pending_lxmf_proofs: &PendingLxmfProofs) -> String {
    let pending = pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock");
    pending.summary(native_unix_timestamp())
}

#[cfg(feature = "native-lxmf")]
fn native_lxmf_recover_direct_correlation(
    pending_lxmf_proofs: &PendingLxmfProofs,
    messages: &[MessageSummary],
) -> usize {
    let mut pending = pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock");
    pending.recover_direct_correlations(messages)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_reconcile_direct_router_timeouts(
    pending_lxmf_proofs: &PendingLxmfProofs,
    now: f64,
    timeout_seconds: f64,
) -> Vec<DirectLxmfTimeoutEvent> {
    pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock")
        .reconcile_timeouts(now, timeout_seconds)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn emit_native_lxmf_direct_timeout_event(
    tx: &broadcast::Sender<RuntimeBusEvent>,
    event: DirectLxmfTimeoutEvent,
) {
    let age_secs = event.observed_at - event.submitted_at;
    let detail = if let Some(node) = event.propagation_fallback_node.as_deref() {
        format!(
            "packet_hash:{};direct_timeout_age_secs:{age_secs:.1};fallback_ready:true;propagation_node:{node};peer_activity_observed:false;proof_state:proof_not_observed",
            event.message_id
        )
    } else {
        format!(
            "packet_hash:{};direct_timeout_age_secs:{age_secs:.1};fallback_ready:false;peer_activity_observed:false;proof_state:proof_not_observed",
            event.message_id
        )
    };
    let fallback = event
        .propagation_fallback_node
        .as_deref()
        .map(|node| format!(" propagation_fallback_node={node}"))
        .unwrap_or_default();
    let _ = tx.send(RuntimeBusEvent::LxmfDeliveryEvidence(
        LxmfDeliveryEvidence {
            peer_hash: event.peer_hash.clone(),
            message_id: Some(event.message_id.clone()),
            kind: LxmfDeliveryEvidenceKind::NoReceiptObserved,
            detail: Some(detail),
            rtt: None,
            observed_at: Some(event.observed_at),
        },
    ));
    let _ = tx.send(RuntimeBusEvent::Debug(format!(
        "native LXMF direct proof timeout peer={} message_id={} age_secs={:.1}{}",
        event.peer_hash, event.message_id, age_secs, fallback
    )));
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_cached_ratchet_available(config_dir: &Path, destination_hash: &[u8; 16]) -> bool {
    use rns_net::storage::RatchetStore;

    let store = rns_net::storage::FsRatchetStore::new(config_dir.join("storage").join("ratchets"));
    store
        .current(
            destination_hash,
            rns_net::time::now(),
            rns_core::constants::RATCHET_EXPIRY as f64,
        )
        .ok()
        .flatten()
        .is_some()
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_recover_propagated_correlation(
    pending_propagated_lxmf: &PendingPropagatedLxmf,
    messages: &[MessageSummary],
) -> usize {
    let mut pending = pending_propagated_lxmf
        .lock()
        .expect("pending propagated LXMF lock");
    let mut recovered = 0usize;
    for message in messages {
        if !native_lxmf_message_can_recover_propagated(message) {
            continue;
        }
        let Some(message_id) = lxmf_message_runtime_id(message) else {
            continue;
        };
        if pending.contains_key(&message_id) {
            continue;
        }
        let transfer_state = message
            .fields
            .get("native_lxmf_propagation_transfer_state")
            .cloned()
            .unwrap_or_else(|| "router_deferred".into());
        let submitted_at = lxmf_message_submitted_at(message).unwrap_or(message.timestamp);
        pending.insert(
            message_id,
            PendingNativePropagatedLxmf {
                peer_hash: message.peer_hash.clone(),
                propagation_node: message
                    .fields
                    .get("native_lxmf_propagation_node")
                    .cloned()
                    .unwrap_or_default(),
                submitted_at,
                has_path: message
                    .fields
                    .get("native_lxmf_propagation_has_path")
                    .is_some_and(|value| value == "true"),
                known_app_data: message
                    .fields
                    .get("native_lxmf_propagation_known_app_data")
                    .is_some_and(|value| value == "true"),
                link_id: message
                    .fields
                    .get("native_lxmf_propagation_link_id")
                    .filter(|value| !value.is_empty())
                    .cloned(),
                transfer_state: transfer_state.clone(),
                peer_activity_observed_at: lxmf_message_peer_activity_observed(message)
                    .then_some(message.timestamp),
                terminal_at: lxmf_propagation_transfer_terminal(&transfer_state)
                    .then_some(message.timestamp),
            },
        );
        recovered += 1;
    }
    recovered
}

#[cfg(all(feature = "native-rns-net", any()))]
fn lxmf_message_runtime_id(message: &MessageSummary) -> Option<String> {
    message
        .fields
        .get("native_lxmf_packet_hash")
        .or_else(|| message.fields.get("native_lxmf_message_id"))
        .cloned()
        .or_else(|| message.message_id.clone())
        .filter(|value| !value.is_empty())
}

#[cfg(all(feature = "native-rns-net", any()))]
fn lxmf_message_submitted_at(message: &MessageSummary) -> Option<f64> {
    message
        .fields
        .get("native_lxmf_submitted_at")
        .and_then(|value| value.parse::<f64>().ok())
}

#[cfg(all(feature = "native-rns-net", any()))]
fn lxmf_message_peer_activity_observed(message: &MessageSummary) -> bool {
    message
        .fields
        .get("native_lxmf_peer_activity_observed")
        .is_some_and(|value| value == "true")
        || message
            .fields
            .get("native_lxmf_receipt_state")
            .is_some_and(|value| value == "peer_activity_after_send")
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_message_can_recover_direct(message: &MessageSummary) -> bool {
    if message.incoming || message.delivered || message.failed {
        return false;
    }
    matches!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("submitted_to_rns_net")
    ) && matches!(
        message
            .fields
            .get("native_lxmf_proof_state")
            .map(String::as_str),
        Some("waiting_for_packet_proof") | None
    )
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_message_can_recover_propagated(message: &MessageSummary) -> bool {
    if message.incoming || message.failed || lxmf_message_peer_activity_observed(message) {
        return false;
    }
    matches!(
        message.fields.get("native_lxmf_state").map(String::as_str),
        Some("queued_for_propagation") | Some("propagation_transfer_completed")
    ) && message
        .fields
        .get("native_lxmf_propagation_transfer_state")
        .is_some_and(|state| !lxmf_propagation_transfer_failed(state))
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn lxmf_propagation_transfer_terminal(transfer_state: &str) -> bool {
    matches!(
        transfer_state,
        "link_packet_sent"
            | "resource_advertised"
            | "resource_completed"
            | "resource_failed"
            | "resource_timeout"
            | "router_timeout"
    )
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn lxmf_propagation_transfer_failed(transfer_state: &str) -> bool {
    matches!(
        transfer_state,
        "link_packet_failed"
            | "resource_failed"
            | "resource_advertise_failed"
            | "resource_timeout"
            | "router_timeout"
            | "link_timeout"
    )
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_submitted_status(
    peer_hash: &str,
    message_id: &str,
    submitted_at: f64,
) -> OutboundStatus {
    OutboundStatus {
        peer_hash: peer_hash.into(),
        message_id: Some(message_id.into()),
        delivered: false,
        failed: false,
        state: OutboundDeliveryState::SubmittedToRnsNet,
        evidence: Some(format!(
            "packet_hash:{message_id};submitted_at:{submitted_at:.3}"
        )),
        rtt: None,
    }
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_wire_message_id(wire_bytes: &[u8]) -> String {
    let digest = Sha256::digest(wire_bytes);
    hex_encode(&digest)
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn extract_native_evidence_value<'a>(evidence: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}:");
    evidence
        .split(';')
        .find_map(|part| part.strip_prefix(prefix.as_str()))
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn lxmf_propagation_state_for_transfer(transfer_state: &str) -> &'static str {
    match transfer_state {
        "link_packet_sent" | "resource_advertised" | "resource_completed" => "completed",
        "link_packet_failed"
        | "resource_failed"
        | "resource_advertise_failed"
        | "resource_timeout"
        | "router_timeout"
        | "link_timeout" => "failed",
        "resource_progress" => "in_progress",
        "router_deferred" => "deferred",
        _ => "queued",
    }
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_timeout_stale_propagated(
    pending_propagated: &PendingPropagatedLxmf,
    now: f64,
    timeout_secs: f64,
) -> Vec<(OutboundStatus, LxmfDeliveryEvidence)> {
    let mut pending = pending_propagated
        .lock()
        .expect("pending propagated LXMF lock");
    pending
        .iter_mut()
        .filter_map(|(message_id, entry)| {
            if entry.peer_activity_observed_at.is_some()
                || lxmf_propagation_transfer_terminal(&entry.transfer_state)
                || now - entry.submitted_at < timeout_secs
            {
                return None;
            }
            let previous_state = entry.transfer_state.clone();
            let timed_out_state = match previous_state.as_str() {
                "resource_progress" => "resource_timeout",
                "link_establishing" | "link_timeout" => "link_timeout",
                "resource_advertise_failed" => "resource_advertise_failed",
                _ => "router_timeout",
            };
            entry.transfer_state = timed_out_state.into();
            entry.terminal_at = Some(now);
            let reason = match previous_state.as_str() {
                "resource_progress" => {
                    "native propagation resource did not report completion before timeout"
                }
                "link_establishing" | "link_timeout" => {
                    "native propagation link did not establish before timeout"
                }
                "resource_advertise_failed" => "native propagation resource advertisement failed",
                _ => "native propagation router transfer did not complete before timeout",
            };
            let detail = format!(
                "propagation_transfer_state:{timed_out_state};previous_transfer_state:{previous_state};propagation_node:{};submitted_at:{:.3};failure_reason:{reason};receipt_state:propagation_node_failed",
                entry.propagation_node, entry.submitted_at
            );
            let status = OutboundStatus {
                peer_hash: entry.peer_hash.clone(),
                message_id: Some(message_id.clone()),
                delivered: false,
                failed: true,
                state: OutboundDeliveryState::Failed,
                evidence: Some(detail.clone()),
                rtt: None,
            };
            let evidence = LxmfDeliveryEvidence {
                peer_hash: entry.peer_hash.clone(),
                message_id: Some(message_id.clone()),
                kind: LxmfDeliveryEvidenceKind::PropagationNodeFailed,
                detail: Some(detail),
                rtt: None,
                observed_at: Some(now),
            };
            Some((status, evidence))
        })
        .collect()
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_prune_terminal_propagated(
    pending_propagated: &PendingPropagatedLxmf,
    now: f64,
    retention_secs: f64,
) -> usize {
    let mut pending = pending_propagated
        .lock()
        .expect("pending propagated LXMF lock");
    let before = pending.len();
    pending.retain(|_, entry| {
        let Some(terminal_at) = entry.terminal_at else {
            return true;
        };
        now - terminal_at <= retention_secs
    });
    before.saturating_sub(pending.len())
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_active_propagated_outbound_summary(
    pending_propagated: &PendingPropagatedLxmf,
) -> Option<String> {
    let pending = pending_propagated
        .lock()
        .expect("pending propagated LXMF lock");
    pending.iter().find_map(|(message_id, entry)| {
        (!lxmf_propagation_transfer_terminal(&entry.transfer_state)
            && entry.peer_activity_observed_at.is_none())
        .then(|| {
            format!(
                "message_id={message_id} peer={} propagation_node={} transfer_state={}",
                entry.peer_hash, entry.propagation_node, entry.transfer_state
            )
        })
    })
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_propagation_no_payload_evidence(
    pending_propagated: &PendingPropagatedLxmf,
    propagation_node: &str,
    requested: usize,
    decoded: usize,
    haves: usize,
) -> Vec<LxmfDeliveryEvidence> {
    let observed_at = native_unix_timestamp();
    let pending = pending_propagated
        .lock()
        .expect("pending propagated LXMF lock");
    pending
        .iter()
        .filter(|(_, entry)| {
            entry.propagation_node.eq_ignore_ascii_case(propagation_node)
                && matches!(
                    entry.transfer_state.as_str(),
                    "link_packet_sent" | "resource_completed"
                    | "resource_advertised"
                )
                && entry.peer_activity_observed_at.is_none()
        })
        .map(|(message_id, entry)| LxmfDeliveryEvidence {
            peer_hash: entry.peer_hash.clone(),
            message_id: Some(message_id.clone()),
            kind: LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads,
            detail: Some(format!(
                "propagation_transfer_state:{};propagation_node:{};requested:{requested};decoded:{decoded};haves:{haves};receipt_state:propagation_sync_no_peer_payload;delivery_state:peer_delivery_unconfirmed",
                entry.transfer_state, entry.propagation_node
            )),
            rtt: None,
            observed_at: Some(observed_at),
        })
        .collect()
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_resource_status_for_event(
    event: RnsNetResourceEvent,
    pending_propagated: &PendingPropagatedLxmf,
) -> Option<(OutboundStatus, Option<LxmfDeliveryEvidence>)> {
    let link_id = match &event {
        RnsNetResourceEvent::Received { link_id, .. }
        | RnsNetResourceEvent::Completed { link_id }
        | RnsNetResourceEvent::Failed { link_id, .. }
        | RnsNetResourceEvent::Progress { link_id, .. } => hex_encode(link_id),
    };
    let mut pending = pending_propagated
        .lock()
        .expect("pending propagated LXMF lock");
    let message_id = pending
        .iter()
        .find(|(_, pending)| pending.link_id.as_deref() == Some(link_id.as_str()))
        .map(|(message_id, _)| message_id.clone())?;
    match event {
        RnsNetResourceEvent::Completed { .. } => {
            let entry = pending.get_mut(&message_id)?;
            entry.transfer_state = "resource_completed".into();
            let now = native_unix_timestamp();
            entry.terminal_at = Some(entry.terminal_at.unwrap_or(now));
            let peer_hash = entry.peer_hash.clone();
            let submitted_at = entry.submitted_at;
            let propagation_node = entry.propagation_node.clone();
            let router_event =
                NativePropagatedLxmfRouter::propagation_node_accepted(PropagatedNodeAccepted {
                    peer_hash: &peer_hash,
                    message_id: &message_id,
                    propagation_node: &propagation_node,
                    submitted_at,
                    transfer_state: "resource_completed",
                    link_id: Some(&link_id),
                    representation: Some("resource"),
                    observed_at: now,
                });
            Some((router_event.status, Some(router_event.evidence)))
        }
        RnsNetResourceEvent::Failed { error, .. } => {
            let entry = pending.get_mut(&message_id)?;
            entry.transfer_state = "resource_failed".into();
            let now = native_unix_timestamp();
            entry.terminal_at = Some(entry.terminal_at.unwrap_or(now));
            let peer_hash = entry.peer_hash.clone();
            let submitted_at = entry.submitted_at;
            let propagation_node = entry.propagation_node.clone();
            let router_event =
                NativePropagatedLxmfRouter::propagation_node_failed(PropagatedNodeFailed {
                    peer_hash: &peer_hash,
                    message_id: &message_id,
                    propagation_node: &propagation_node,
                    submitted_at,
                    transfer_state: "resource_failed",
                    link_id: Some(&link_id),
                    failure_reason: &error,
                    observed_at: now,
                });
            Some((router_event.status, Some(router_event.evidence)))
        }
        RnsNetResourceEvent::Progress {
            received, total, ..
        } => {
            let entry = pending.get_mut(&message_id)?;
            entry.transfer_state = "resource_progress".into();
            let peer_hash = entry.peer_hash.clone();
            let submitted_at = entry.submitted_at;
            let propagation_node = entry.propagation_node.clone();
            Some((OutboundStatus {
                peer_hash,
                message_id: Some(message_id),
                delivered: false,
                failed: false,
                state: OutboundDeliveryState::SubmittedToRnsNet,
                evidence: Some(format!(
                    "propagation_transfer_state:resource_progress;propagation_link_id:{link_id};propagation_node:{propagation_node};resource_received:{received};resource_total:{total};submitted_at:{submitted_at:.3}"
                )),
                rtt: None,
            }, None))
        }
        RnsNetResourceEvent::Received { .. } => None,
    }
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_direct_resource_status_for_event(
    event: RnsNetResourceEvent,
    pending_direct: &PendingDirectLxmfResources,
) -> Option<(OutboundStatus, Option<LxmfDeliveryEvidence>)> {
    let link_id = match &event {
        RnsNetResourceEvent::Received { link_id, .. }
        | RnsNetResourceEvent::Completed { link_id }
        | RnsNetResourceEvent::Failed { link_id, .. }
        | RnsNetResourceEvent::Progress { link_id, .. } => hex_encode(link_id),
    };
    let mut pending = pending_direct
        .lock()
        .expect("pending direct LXMF resource lock");
    let entry = pending.get_mut(&link_id)?;
    match event {
        RnsNetResourceEvent::Completed { .. } => {
            entry.transfer_state = "resource_completed".into();
            let peer_hash = entry.peer_hash.clone();
            let message_id = entry.message_id.clone();
            let submitted_at = entry.submitted_at;
            let evidence_detail = format!(
                "direct_transfer_state:resource_completed;direct_link_id:{link_id};submitted_at:{submitted_at:.3};receipt_state:direct_resource_completed_peer_unconfirmed;delivery_state:peer_delivery_unconfirmed"
            );
            pending.remove(&link_id);
            Some((
                OutboundStatus {
                    peer_hash: peer_hash.clone(),
                    message_id: Some(message_id.clone()),
                    delivered: false,
                    failed: false,
                    state: OutboundDeliveryState::SubmittedToRnsNet,
                    evidence: Some(evidence_detail.clone()),
                    rtt: None,
                },
                Some(LxmfDeliveryEvidence {
                    peer_hash,
                    message_id: Some(message_id),
                    kind: LxmfDeliveryEvidenceKind::PacketSubmitted,
                    detail: Some(evidence_detail),
                    rtt: None,
                    observed_at: Some(native_unix_timestamp()),
                }),
            ))
        }
        RnsNetResourceEvent::Failed { error, .. } => {
            entry.transfer_state = "resource_failed".into();
            let peer_hash = entry.peer_hash.clone();
            let message_id = entry.message_id.clone();
            let submitted_at = entry.submitted_at;
            if native_lxmf_resource_failure_is_unconfirmed(&error) {
                entry.transfer_state = "resource_timeout".into();
                let evidence_detail = format!(
                    "direct_transfer_state:resource_timeout;direct_link_id:{link_id};failure_reason:{error};submitted_at:{submitted_at:.3};receipt_state:direct_resource_timeout;delivery_state:peer_delivery_unconfirmed"
                );
                return Some((
                    OutboundStatus {
                        peer_hash: peer_hash.clone(),
                        message_id: Some(message_id.clone()),
                        delivered: false,
                        failed: false,
                        state: OutboundDeliveryState::SubmittedToRnsNet,
                        evidence: Some(evidence_detail.clone()),
                        rtt: None,
                    },
                    Some(LxmfDeliveryEvidence {
                        peer_hash,
                        message_id: Some(message_id),
                        kind: LxmfDeliveryEvidenceKind::NoReceiptObserved,
                        detail: Some(evidence_detail),
                        rtt: None,
                        observed_at: Some(native_unix_timestamp()),
                    }),
                ));
            }
            let evidence_detail = format!(
                "direct_transfer_state:resource_failed;direct_link_id:{link_id};failure_reason:{error};submitted_at:{submitted_at:.3};receipt_state:lxmf_failed;delivery_state:failed"
            );
            pending.remove(&link_id);
            Some((
                OutboundStatus {
                    peer_hash: peer_hash.clone(),
                    message_id: Some(message_id.clone()),
                    delivered: false,
                    failed: true,
                    state: OutboundDeliveryState::Failed,
                    evidence: Some(evidence_detail.clone()),
                    rtt: None,
                },
                Some(LxmfDeliveryEvidence {
                    peer_hash,
                    message_id: Some(message_id),
                    kind: LxmfDeliveryEvidenceKind::LxmfRouterFailed,
                    detail: Some(evidence_detail),
                    rtt: None,
                    observed_at: Some(native_unix_timestamp()),
                }),
            ))
        }
        RnsNetResourceEvent::Progress {
            received, total, ..
        } => {
            entry.transfer_state = "resource_progress".into();
            Some((
                OutboundStatus {
                    peer_hash: entry.peer_hash.clone(),
                    message_id: Some(entry.message_id.clone()),
                    delivered: false,
                    failed: false,
                    state: OutboundDeliveryState::SubmittedToRnsNet,
                    evidence: Some(format!(
                        "direct_transfer_state:resource_progress;direct_link_id:{link_id};resource_received:{received};resource_total:{total};submitted_at:{:.3}",
                        entry.submitted_at
                    )),
                    rtt: None,
                },
                None,
            ))
        }
        RnsNetResourceEvent::Received { .. } => None,
    }
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_resource_failure_is_unconfirmed(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("timeout") || lower.contains("timed out")
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_message_from_resource_event(
    event: &RnsNetResourceEvent,
    attachments_dir: &Path,
) -> AppResult<Option<MessageSummary>> {
    let RnsNetResourceEvent::Received { data, .. } = event else {
        return Ok(None);
    };
    decode_native_lxmf_payload(data, attachments_dir).map(Some)
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_message_from_link_data(
    link_data: &RnsNetLinkData,
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_native_lxmf_payload(&link_data.data, attachments_dir)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_proof_status_for_packet(
    proof: &RnsNetProof,
    pending_lxmf_proofs: &PendingLxmfProofs,
) -> (OutboundStatus, bool) {
    let packet_hash = hex_encode(&proof.packet_hash);
    let destination_hash = hex_encode(&proof.destination_hash);
    pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock")
        .proof_status_for_packet(packet_hash, destination_hash, proof.rtt)
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_events_for_packet_proof(
    proof: &RnsNetProof,
    pending_lxmf_proofs: &PendingLxmfProofs,
) -> Vec<RuntimeBusEvent> {
    let (status, matched_pending) = native_lxmf_proof_status_for_packet(proof, pending_lxmf_proofs);
    let packet_hash = status.message_id.clone().unwrap_or_default();
    let peer_hash = status.peer_hash.clone();
    let proof_destination = hex_encode(&proof.destination_hash);
    let mut events = Vec::new();
    if matched_pending {
        events.push(RuntimeBusEvent::MessageDeliveryUpdated(status.clone()));
        events.push(RuntimeBusEvent::LxmfDeliveryEvidence(
            LxmfDeliveryEvidence {
                peer_hash: peer_hash.clone(),
                message_id: status.message_id,
                kind: LxmfDeliveryEvidenceKind::RnsPacketProof,
                detail: Some(format!(
                    "packet_hash:{packet_hash};proof_destination:{proof_destination};matched_pending:{matched_pending};rtt:{:.3}",
                    proof.rtt
                )),
                rtt: Some(proof.rtt),
                observed_at: Some(native_unix_timestamp()),
            },
        ));
    }
    if matched_pending {
        events.push(RuntimeBusEvent::Debug(format!(
            "native RNS packet proof received peer={} packet_hash={} proof_destination={} matched_pending=true rtt={:.3}",
            peer_hash, packet_hash, proof_destination, proof.rtt
        )));
    }
    events
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_inbound_peer_evidence(
    message: &MessageSummary,
    pending_lxmf_proofs: &PendingLxmfProofs,
    destination_keys: &Arc<Mutex<RnsNetDestinationKeyStore>>,
    detail: &str,
) -> Vec<LxmfDeliveryEvidence> {
    if message.peer_hash.is_empty() {
        return Vec::new();
    }
    let observed_at = native_unix_timestamp();
    let peer_hashes = native_lxmf_peer_activity_hash_candidates(message, destination_keys);
    pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock")
        .inbound_peer_evidence_for_hashes(&peer_hashes, &message.peer_hash, detail, observed_at)
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_inbound_peer_propagated_evidence(
    message: &MessageSummary,
    pending_propagated_lxmf: &PendingPropagatedLxmf,
    destination_keys: &Arc<Mutex<RnsNetDestinationKeyStore>>,
    detail: &str,
) -> Vec<LxmfDeliveryEvidence> {
    if message.peer_hash.is_empty() {
        return Vec::new();
    }
    let observed_at = native_unix_timestamp();
    let peer_hashes = native_lxmf_peer_activity_hash_candidates(message, destination_keys);
    let mut pending = pending_propagated_lxmf
        .lock()
        .expect("pending propagated LXMF lock");
    pending
        .iter_mut()
        .filter(|(_, pending)| {
            peer_hashes
                .iter()
                .any(|peer_hash| pending.peer_hash.eq_ignore_ascii_case(peer_hash))
        })
        .map(|(message_id, pending)| {
            pending.peer_activity_observed_at =
                Some(pending.peer_activity_observed_at.unwrap_or(observed_at));
            LxmfDeliveryEvidence {
                peer_hash: pending.peer_hash.clone(),
                message_id: Some(message_id.clone()),
                kind: LxmfDeliveryEvidenceKind::InboundPeerMessage,
                detail: Some(format!(
                    "{detail};observed_peer_hash:{};peer_activity_observed:true;observed_at:{observed_at:.3}",
                    message.peer_hash
                )),
                rtt: None,
                observed_at: Some(observed_at),
            }
        })
        .collect()
}

#[cfg(all(feature = "native-rns-net", any()))]
fn native_lxmf_peer_activity_hash_candidates(
    message: &MessageSummary,
    destination_keys: &Arc<Mutex<RnsNetDestinationKeyStore>>,
) -> Vec<String> {
    let mut candidates = Vec::from([message.peer_hash.clone()]);
    if let Ok(destination_hash) = parse_rns_net_destination_hash(&message.peer_hash) {
        let siblings = destination_keys
            .lock()
            .expect("native rns-net key store lock")
            .sibling_destination_hashes(&destination_hash);
        for sibling in siblings {
            let sibling = hex_encode(&sibling);
            if !candidates
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&sibling))
            {
                candidates.push(sibling);
            }
        }
    }
    candidates
}

#[cfg(feature = "native-lxmf")]
fn local_lxmf_delivery_announce_app_data(display_name: &str) -> AppResult<Vec<u8>> {
    crate::runtime::native_lxmf::codec::encode_delivery_display_name_app_data(display_name)
}

#[cfg(not(feature = "native-lxmf"))]
fn local_lxmf_delivery_announce_app_data(display_name: &str) -> AppResult<Vec<u8>> {
    Ok(display_name.as_bytes().to_vec())
}

fn local_lxmf_display_name(
    identity: &NativeIdentitySummary,
    profile: Option<&IdentityProfile>,
) -> String {
    profile
        .and_then(|profile| {
            let label = profile.label.trim();
            (!label.is_empty()).then(|| label.to_string())
        })
        .unwrap_or_else(|| {
            format!(
                "OMENbrowser_rs {}",
                identity
                    .address_hash_hex
                    .chars()
                    .take(8)
                    .collect::<String>()
            )
        })
}

#[allow(dead_code)]
fn _announce_payload_shape() -> AnnouncePayload {
    AnnouncePayload {
        destination_hash: String::new(),
        identity_hash: None,
        display_name: String::new(),
        kind: DirectoryKind::Unknown,
        associated_hash: None,
        node_associated_hash: None,
        has_ratchet: false,
        lxmf_stamp_cost: None,
    }
}

impl Default for NativeRuntimeState {
    fn default() -> Self {
        Self {
            lifecycle: NativeRuntimeLifecycle::Stopped,
            active_identity: None,
            active_identity_profile: None,
            interfaces: Vec::new(),
            transport_started: false,
            #[cfg(all(feature = "native-rns-net", any()))]
            rns_net_started: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(all(feature = "native-rns-net", any())))]
    use crate::browser::PageSource;
    use crate::config::AppPaths;
    use crate::identity::{IdentityManager, IdentityMaterialProvider};
    use crate::runtime::native::identity::NativeReticulumIdentityProvider;
    use crate::runtime::native::interface::plan_interfaces;
    use crate::runtime::native::request::{NativePageFetchContext, NativePageResponse};
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::Instant;

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    fn restored_destination_identity_rejects_malformed_keys_without_panicking() {
        let invalid_verifying_key = (0_u8..=u8::MAX)
            .map(|byte| [byte; 32])
            .find(|bytes| parse_restored_destination_identity(&[0x11; 32], bytes).is_err())
            .expect("at least one compressed point encoding must be invalid");

        assert!(parse_restored_destination_identity(&[0x11; 32], &invalid_verifying_key).is_err());
        assert!(parse_restored_destination_identity(&[0x11; 31], &[0x22; 32]).is_err());
    }

    fn temp_paths(name: &str) -> AppPaths {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-native-runtime-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        AppPaths::from_root(root)
    }

    struct TestTreeCleanup(PathBuf);

    impl Drop for TestTreeCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn propagation_sync_waits_for_path_restore_completion() {
        let (ready_tx, ready_rx) = watch::channel(false);
        let waiter =
            tokio::spawn(async move { wait_for_reticulum_path_table_restore(&ready_rx).await });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        ready_tx.send(true).expect("signal path restore completion");
        waiter
            .await
            .expect("join path restore waiter")
            .expect("path restore readiness");
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn propagation_sync_rejects_stopped_path_restore_worker() {
        let (ready_tx, ready_rx) = watch::channel(false);
        drop(ready_tx);
        let error = wait_for_reticulum_path_table_restore(&ready_rx)
            .await
            .expect_err("closed restore worker must fail");
        assert!(error.to_string().contains("restore worker stopped early"));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    struct PythonPropagationPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    impl PythonPropagationPeer {
        fn spawn(
            root: &Path,
            port: u16,
            destination: &str,
            rns_source_env: &str,
            lxmf_source_env: Option<&str>,
            expected_rns: &str,
            expected_lxmf: &str,
        ) -> Self {
            let rns_source = std::env::var_os(rns_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{rns_source_env} must name a Python RNS source"));
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "src/server/crates/omen-ifac-tcp/tests/fixtures/current_python_lxmf_propagation.py",
            );
            let mut command = Command::new("python3");
            command.arg(script).arg("--rns-source").arg(rns_source);
            if let Some(lxmf_source_env) = lxmf_source_env {
                let lxmf_source = std::env::var_os(lxmf_source_env)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
                command.arg("--lxmf-source").arg(lxmf_source);
            }
            let mut child = command
                .arg("--expected-rns")
                .arg(expected_rns)
                .arg("--expected-lxmf")
                .arg(expected_lxmf)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--destination")
                .arg(destination)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn current Python LXMF propagation peer");
            let (sender, json_lines) = mpsc::sync_channel(4);
            let stdout = child
                .stdout
                .take()
                .expect("current Python propagation stdout");
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if sender.send(value).is_err() {
                            break;
                        }
                    }
                }
            });
            let ready = json_lines
                .recv_timeout(Duration::from_secs(8))
                .expect("current Python propagation readiness");
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], expected_rns);
            assert_eq!(ready["lxmf"], expected_lxmf);
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn wait_for_queued(&self) -> serde_json::Value {
            let queued = self
                .json_lines
                .recv_timeout(Duration::from_secs(14))
                .expect("current Python propagated transient queued");
            assert_eq!(queued["queued"], true);
            queued
        }

        fn finish(mut self) -> serde_json::Value {
            let result = self
                .json_lines
                .recv_timeout(Duration::from_secs(22))
                .expect("current Python propagation acknowledgement");
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self
                    .child
                    .try_wait()
                    .expect("poll current Python propagation peer")
                {
                    assert!(
                        status.success(),
                        "current Python propagation peer exited {status}"
                    );
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("current Python propagation peer did not exit within bounded shutdown");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader
                    .join()
                    .expect("current Python propagation stdout reader join");
            }
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    impl Drop for PythonPropagationPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    struct PythonNomadNetPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    impl PythonNomadNetPeer {
        fn spawn(root: &Path, port: u16) -> Self {
            Self::spawn_scenario(root, port, "matrix")
        }

        fn spawn_scenario(root: &Path, port: u16, scenario: &str) -> Self {
            assert!(matches!(
                scenario,
                "matrix" | "faults" | "reuse" | "performance" | "soak"
            ));
            let python_source = std::env::var_os("OMEN_PYTHON_NOMADNET_SOURCE")
                .map(PathBuf::from)
                .expect("OMEN_PYTHON_NOMADNET_SOURCE must name current Python site-packages");
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "src/server/crates/omen-ifac-tcp/tests/fixtures/current_python_nomadnet_node.py",
            );
            let mut child = Command::new("python3")
                .arg(script)
                .arg("--python-source")
                .arg(python_source)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--scenario")
                .arg(scenario)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn current Python NomadNet node");
            let (sender, json_lines) = mpsc::sync_channel(4);
            let stdout = child.stdout.take().expect("current Python NomadNet stdout");
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if sender.send(value).is_err() {
                            break;
                        }
                    }
                }
            });
            let ready = json_lines
                .recv_timeout(Duration::from_secs(8))
                .expect("current Python NomadNet readiness");
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], "1.4.0");
            assert_eq!(ready["nomadnet"], "1.2.7");
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn finish(mut self) -> serde_json::Value {
            let result = self
                .json_lines
                .recv_timeout(Duration::from_secs(8))
                .expect("current Python NomadNet request result");
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self
                    .child
                    .try_wait()
                    .expect("poll current Python NomadNet node")
                {
                    assert!(status.success(), "current Python NomadNet exited {status}");
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("current Python NomadNet node did not exit within bounded shutdown");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn wait_for_marker(&self, marker: &str) -> serde_json::Value {
            let value = self
                .json_lines
                .recv_timeout(Duration::from_secs(8))
                .expect("current Python NomadNet marker");
            assert_eq!(value[marker], true, "unexpected marker: {value}");
            value
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader.join().expect("current Python NomadNet reader join");
            }
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    impl Drop for PythonNomadNetPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    struct PythonStampedPropagationPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    impl PythonStampedPropagationPeer {
        fn spawn(
            root: &Path,
            port: u16,
            source: &str,
            rns_source_env: &str,
            lxmf_source_env: Option<&str>,
            expected_rns: &str,
            expected_lxmf: &str,
        ) -> Self {
            let rns_source = std::env::var_os(rns_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{rns_source_env} must name a Python RNS source"));
            let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "src/server/crates/omen-ifac-tcp/tests/fixtures/python_lxmf_stamped_propagation_peer.py",
            );
            let mut command = Command::new("python3");
            command.arg(script).arg("--rns-source").arg(rns_source);
            if let Some(lxmf_source_env) = lxmf_source_env {
                let lxmf_source = std::env::var_os(lxmf_source_env)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
                command.arg("--lxmf-source").arg(lxmf_source);
            }
            let mut child = command
                .arg("--expected-rns")
                .arg(expected_rns)
                .arg("--expected-lxmf")
                .arg(expected_lxmf)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--source")
                .arg(source)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn Python stamped propagation peer");
            let (sender, json_lines) = mpsc::sync_channel(4);
            let stdout = child.stdout.take().expect("stamped propagation stdout");
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if sender.send(value).is_err() {
                            break;
                        }
                    }
                }
            });
            let ready = json_lines
                .recv_timeout(Duration::from_secs(8))
                .expect("Python stamped propagation readiness");
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], expected_rns);
            assert_eq!(ready["lxmf"], expected_lxmf);
            assert_eq!(ready["advertised_cost"], 13);
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn next(&self, label: &str) -> serde_json::Value {
            self.json_lines
                .recv_timeout(Duration::from_secs(24))
                .unwrap_or_else(|_| panic!("Python stamped propagation {label}"))
        }

        fn finish(mut self) {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().expect("poll stamped peer") {
                    assert!(status.success(), "Python stamped peer exited {status}");
                    self.join_reader();
                    return;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("Python stamped propagation peer did not exit");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader.join().expect("stamped propagation reader join");
            }
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    impl Drop for PythonStampedPropagationPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    struct PythonDirectStampPeer {
        child: Child,
        json_lines: mpsc::Receiver<serde_json::Value>,
        reader: Option<JoinHandle<()>>,
        ready: serde_json::Value,
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    struct PythonDirectStampPeerConfig<'a> {
        fixture: &'a str,
        rns_source_env: &'a str,
        lxmf_source_env: Option<&'a str>,
        expected_rns: &'a str,
        expected_lxmf: &'a str,
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    impl PythonDirectStampPeer {
        fn spawn(
            root: &Path,
            port: u16,
            rust_source: &str,
            config: PythonDirectStampPeerConfig<'_>,
        ) -> Self {
            let rns_source = std::env::var_os(config.rns_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    panic!("{} must name a Python RNS source", config.rns_source_env)
                });
            let script = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/server/crates/omen-ifac-tcp/tests/fixtures")
                .join(config.fixture);
            let mut command = Command::new("python3");
            command.arg(script).arg("--rns-source").arg(rns_source);
            if let Some(lxmf_source_env) = config.lxmf_source_env {
                let lxmf_source = std::env::var_os(lxmf_source_env)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
                command.arg("--lxmf-source").arg(lxmf_source);
            }
            let mut child = command
                .arg("--expected-rns")
                .arg(config.expected_rns)
                .arg("--expected-lxmf")
                .arg(config.expected_lxmf)
                .arg("--root")
                .arg(root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--rust-source")
                .arg(rust_source)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("spawn Python direct-stamp peer");
            let (sender, json_lines) = mpsc::sync_channel(4);
            let stdout = child.stdout.take().expect("Python direct-stamp stdout");
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if sender.send(value).is_err() {
                            break;
                        }
                    }
                }
            });
            let ready = json_lines
                .recv_timeout(Duration::from_secs(8))
                .expect("Python direct-stamp readiness");
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["port"], port);
            assert_eq!(ready["rns"], config.expected_rns);
            assert_eq!(ready["lxmf"], config.expected_lxmf);
            assert_eq!(ready["stamp_cost"], 1);
            Self {
                child,
                json_lines,
                reader: Some(reader),
                ready,
            }
        }

        fn wait_for_source_announce(&self) {
            let announced = self
                .json_lines
                .recv_timeout(Duration::from_secs(12))
                .expect("Python direct-stamp source announce");
            assert_eq!(announced["source_announced"], true);
        }

        fn finish(mut self) -> serde_json::Value {
            let result = self
                .json_lines
                .recv_timeout(Duration::from_secs(28))
                .expect("Python direct-stamp result");
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(status) = self.child.try_wait().expect("poll direct-stamp peer") {
                    assert!(status.success(), "Python direct-stamp peer exited {status}");
                    self.join_reader();
                    return result;
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    panic!("Python direct-stamp peer did not exit");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn join_reader(&mut self) {
            if let Some(reader) = self.reader.take() {
                reader.join().expect("direct-stamp reader join");
            }
        }
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    impl Drop for PythonDirectStampPeer {
        fn drop(&mut self) {
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            self.join_reader();
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    fn current_python_propagation_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("reserve current Python propagation loopback port");
        listener
            .local_addr()
            .expect("current Python propagation reserved address")
            .port()
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    async fn wait_for_transport_tasks_to_release(
        transport: &Arc<reticulum_rs::runtime::Transport>,
    ) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while Arc::strong_count(transport) != 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("transport tasks release their owner after shutdown");
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn clean_omenchat_link_coordinator_cancels_superseded_waiter() {
        let coordinator = CleanOmenChatLinkCoordinator::default();
        let destination = rns_transport::hash::AddressHash::new_from_slice(&[0x31; 16]);
        let owner_cancel = CancellationToken::new();
        let _owner = coordinator
            .lock(&destination, &owner_cancel)
            .await
            .expect("first open owns destination");
        let waiter_cancel = CancellationToken::new();
        let waiter = {
            let coordinator = coordinator.clone();
            let waiter_cancel = waiter_cancel.clone();
            tokio::spawn(async move {
                coordinator
                    .lock(&destination, &waiter_cancel)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
        };

        waiter_cancel.cancel();

        let error = waiter
            .await
            .expect("waiter task")
            .expect_err("superseded waiter is cancelled");
        assert!(error.to_string().to_ascii_lowercase().contains("cancel"));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn clean_omenchat_reconnect_retires_only_matching_destination_link() {
        use rns_transport::destination::link::{Link, LinkStatus};
        use rns_transport::destination::{DestinationName, SingleOutputDestination};

        let local_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-omenchat-reconnect-retirement-local",
        );
        let config = reticulum_rs::runtime::TransportConfig::new(
            "omenbrowser-omenchat-reconnect-retirement",
            &local_identity,
            false,
        );
        let transport = Arc::new(reticulum_rs::runtime::Transport::new(config));
        let active_links = Arc::new(Mutex::new(BTreeSet::new()));
        let clean_links = Arc::new(Mutex::new(BTreeMap::new()));

        let first_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-omenchat-reconnect-retirement-first",
        );
        let first_destination = SingleOutputDestination::new(
            *first_identity.as_identity(),
            DestinationName::new(OMENCHAT_RNS_APP_NAME, OMENCHAT_NODE_ASPECT),
        );
        let (first_events, _) = broadcast::channel(4);
        let first_link = Arc::new(AsyncMutex::new(Link::new(
            first_destination.desc,
            first_events,
        )));
        let first_link_hash = rns_transport::hash::AddressHash::new([0x11; 16]);
        let first_link_id = address_hash_to_link_id(first_link_hash);

        let second_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-omenchat-reconnect-retirement-second",
        );
        let second_destination = SingleOutputDestination::new(
            *second_identity.as_identity(),
            DestinationName::new(OMENCHAT_RNS_APP_NAME, OMENCHAT_NODE_ASPECT),
        );
        let (second_events, _) = broadcast::channel(4);
        let second_link = Arc::new(AsyncMutex::new(Link::new(
            second_destination.desc,
            second_events,
        )));
        let second_link_hash = rns_transport::hash::AddressHash::new([0x22; 16]);
        let second_link_id = address_hash_to_link_id(second_link_hash);

        active_links
            .lock()
            .expect("active links")
            .extend([first_link_id, second_link_id]);
        clean_links.lock().expect("clean links").extend([
            (
                first_link_id,
                CleanOmenChatLink {
                    destination_hash: first_destination.desc.address_hash,
                    link_id: first_link_hash,
                    transport: transport.clone(),
                    link: first_link.clone(),
                },
            ),
            (
                second_link_id,
                CleanOmenChatLink {
                    destination_hash: second_destination.desc.address_hash,
                    link_id: second_link_hash,
                    transport: transport.clone(),
                    link: second_link.clone(),
                },
            ),
        ]);

        assert_eq!(
            retire_clean_omenchat_destination_links(
                &clean_links,
                &active_links,
                first_destination.desc.address_hash,
            )
            .await,
            1
        );
        assert_eq!(first_link.lock().await.status(), LinkStatus::Closed);
        assert_eq!(second_link.lock().await.status(), LinkStatus::Pending);
        assert!(!active_links
            .lock()
            .expect("active links")
            .contains(&first_link_id));
        assert!(active_links
            .lock()
            .expect("active links")
            .contains(&second_link_id));
        assert!(!clean_links
            .lock()
            .expect("clean links")
            .contains_key(&first_link_id));
        assert!(clean_links
            .lock()
            .expect("clean links")
            .contains_key(&second_link_id));
        assert_eq!(
            retire_clean_omenchat_destination_links(
                &clean_links,
                &active_links,
                first_destination.desc.address_hash,
            )
            .await,
            0
        );
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn clean_omenchat_cancelled_pending_open_is_not_reused() {
        use rns_transport::destination::link::LinkStatus;
        use rns_transport::destination::{DestinationName, SingleOutputDestination};

        let local_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-omenchat-cancelled-open-local",
        );
        let config = reticulum_rs::runtime::TransportConfig::new(
            "omenbrowser-omenchat-cancelled-open",
            &local_identity,
            false,
        );
        let transport = Arc::new(reticulum_rs::runtime::Transport::new(config));
        let remote_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-omenchat-cancelled-open-remote",
        );
        let destination = SingleOutputDestination::new(
            *remote_identity.as_identity(),
            DestinationName::new(OMENCHAT_RNS_APP_NAME, OMENCHAT_NODE_ASPECT),
        );

        let cancelled = transport.link(destination.desc).await;
        assert_eq!(cancelled.lock().await.status(), LinkStatus::Pending);
        clean_close_link(&transport, &cancelled).await;
        transport
            .reset_out_link(&destination.desc.address_hash)
            .await;
        let replacement = transport.link(destination.desc).await;

        assert_eq!(cancelled.lock().await.status(), LinkStatus::Closed);
        assert_eq!(replacement.lock().await.status(), LinkStatus::Pending);
        assert!(!Arc::ptr_eq(&cancelled, &replacement));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    fn clean_destination_caches_are_item_and_byte_bounded() {
        let mut identities = BTreeMap::new();
        for index in 0..=CLEAN_DESTINATION_CACHE_MAX_ITEMS {
            insert_bounded_destination_cache(&mut identities, format!("{index:04}"), index);
        }
        assert_eq!(identities.len(), CLEAN_DESTINATION_CACHE_MAX_ITEMS);
        assert!(!identities.contains_key("0000"));
        assert!(identities.contains_key(&format!("{:04}", CLEAN_DESTINATION_CACHE_MAX_ITEMS)));

        let mut app_data = BTreeMap::new();
        assert!(!insert_bounded_destination_app_data(
            &mut app_data,
            "oversize".into(),
            vec![0; CLEAN_DESTINATION_APP_DATA_MAX_ITEM_BYTES + 1],
        ));
        assert!(app_data.is_empty());
        for index in 0..=CLEAN_DESTINATION_CACHE_MAX_ITEMS {
            assert!(insert_bounded_destination_app_data(
                &mut app_data,
                format!("{index:04}"),
                vec![index as u8; 2 * 1024],
            ));
        }
        assert!(app_data.len() <= CLEAN_DESTINATION_CACHE_MAX_ITEMS);
        assert!(
            app_data.values().map(Vec::len).sum::<usize>()
                <= CLEAN_DESTINATION_APP_DATA_MAX_TOTAL_BYTES
        );
        assert!(app_data.contains_key(&format!("{:04}", CLEAN_DESTINATION_CACHE_MAX_ITEMS)));
        assert!(insert_bounded_destination_app_data(
            &mut app_data,
            "legacy-empty-policy".into(),
            Vec::new(),
        ));
        assert_eq!(
            app_data.get("legacy-empty-policy").map(Vec::as_slice),
            Some([].as_slice()),
            "an authenticated empty announce must be distinguishable from no announce"
        );
    }

    #[test]
    fn announce_lag_is_visible_without_exposing_payloads() {
        let RuntimeBusEvent::Debug(message) = announce_lag_diagnostic(17) else {
            panic!("announce lag must produce a diagnostic event");
        };

        assert!(message.contains("skipped=17"));
        assert!(message.contains("destination state may be incomplete"));
    }

    #[tokio::test]
    async fn external_mode_refuses_to_start_an_integrated_transport() {
        let paths = temp_paths("external-mode-no-integrated-start");
        paths.ensure().expect("isolated paths");
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.instance_mode = crate::runtime::native::config::NativeRuntimeMode::External;
        let runtime = NativeNetworkRuntime::new(config);

        let direct_error = runtime
            .start(None, Vec::new())
            .expect_err("direct start must also refuse external mode");
        assert!(direct_error
            .to_string()
            .contains("integrated interface startup is disabled"));

        let error = runtime
            .start_runtime(None, Vec::new())
            .await
            .expect_err("external mode must not start integrated interfaces");

        assert!(error.to_string().contains("external/shared Reticulum mode"));
        assert!(error
            .to_string()
            .contains("integrated interface startup is disabled"));
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::New
        );
        assert!(!runtime.interface_stats().await.expect("stats").available);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test(flavor = "multi_thread")]
    async fn path_table_save_and_corrupt_restore_stay_in_isolated_storage_root() {
        let paths = temp_paths("path-table-isolated");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime
            .start(Some(profile), Vec::new())
            .expect("start isolated runtime");
        let transport = runtime
            .active_transport()
            .expect("active transport")
            .transport;

        assert_eq!(
            transport
                .save_reticulum_path_table(&paths.reticulum_storage_dir)
                .await
                .expect("save empty path table"),
            0
        );
        let destination_table = paths.reticulum_storage_dir.join("destination_table");
        assert!(destination_table.is_file());
        assert!(paths.reticulum_storage_dir.join("tunnels").is_file());
        std::fs::write(&destination_table, b"invalid-path-table")
            .expect("write isolated corrupt fixture");

        let error = transport
            .restore_reticulum_path_table_report(&paths.reticulum_storage_dir)
            .await
            .expect_err("reject corrupt isolated path table");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(destination_table.starts_with(&paths.root));
        runtime.stop();
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    fn clean_omenchat_accepts_generic_data_and_collapsed_legacy_context_only() {
        use rns_transport::PacketContext;

        // reticulum-rs 0.9 maps unknown application contexts such as OMENchat's
        // historical 0x4f to generic data. Frame decoding remains the protocol
        // discriminator; standard non-data contexts must stay excluded.
        assert_eq!(
            PacketContext::from(OMENCHAT_LINK_CONTEXT),
            PacketContext::None
        );
        assert!(clean_omenchat_frame_context(PacketContext::None));
        assert!(clean_omenchat_optional_frame_context(None));
        assert!(clean_omenchat_optional_frame_context(Some(
            PacketContext::None
        )));
        assert!(!clean_omenchat_frame_context(PacketContext::Channel));
        assert!(!clean_omenchat_optional_frame_context(Some(
            PacketContext::Request
        )));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    fn clean_omenchat_resource_admission_distinguishes_frame_payload_and_other_metadata() {
        assert_eq!(
            clean_omenchat_resource_limit(Some(b"omenchat-frame:room")),
            Some(crate::protocol_limits::OMENCHAT_FRAME_MAX_BYTES)
        );
        assert_eq!(
            clean_omenchat_resource_limit(Some(b"omenchat-resource:upload:1")),
            Some(OMENCHAT_CLEAN_RESOURCE_MAX_BYTES)
        );
        assert_eq!(clean_omenchat_resource_limit(Some(b"lxmf-resource")), None);
        assert_eq!(clean_omenchat_resource_limit(None), None);
        const {
            assert!(
                crate::protocol_limits::OMENCHAT_FRAME_MAX_BYTES
                    < OMENCHAT_CLEAN_RESOURCE_MAX_BYTES
            );
        }
    }

    #[cfg(feature = "native-lxmf")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn native_lxmf_blocking_gate_caps_concurrency_and_releases_permits() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let gate = Arc::new(Semaphore::new(NATIVE_LXMF_DECODE_BLOCKING_JOBS));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let gate = gate.clone();
            let active = active.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                run_native_lxmf_blocking(gate, move || {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    active.fetch_sub(1, Ordering::SeqCst);
                })
                .await
            }));
        }
        for task in tasks {
            task.await
                .expect("blocking-gate task join")
                .expect("blocking-gate job");
        }

        assert_eq!(
            peak.load(Ordering::SeqCst),
            NATIVE_LXMF_DECODE_BLOCKING_JOBS
        );
        assert_eq!(gate.available_permits(), NATIVE_LXMF_DECODE_BLOCKING_JOBS);
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_lxmf_decode_rejects_unknown_announce_identity_before_attachment_write() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("clean-lxmf-unknown-source")
            .expect("sender identity");
        let source = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(&private)
            .expect("LXMF delivery hash");
        let envelope = MessageEnvelope {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            title: "Unknown source".into(),
            body: "Body".into(),
            delivery_mode: crate::messaging::DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound = crate::runtime::native_lxmf::codec::build_outbound_message(
            &envelope,
            &hex_encode(&source),
        )
        .expect("outbound");
        let wire =
            crate::runtime::native_lxmf::codec::encode_signed_wire_message(&outbound, &private)
                .expect("signed wire");
        let attachments_dir = temp_paths("clean-lxmf-unknown-source").attachments_dir;

        let decoded = decode_native_lxmf_payload_bounded(
            wire,
            attachments_dir.clone(),
            Arc::new(Mutex::new(BTreeMap::new())),
        )
        .await
        .expect("bounded decode owns verification result");
        let error = decoded
            .message
            .expect_err("unannounced source must be rejected");

        assert!(error
            .to_string()
            .contains("authenticated lxmf.delivery announce"));
        assert!(!attachments_dir.exists());
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_lxmf_decode_accepts_matching_announce_identity_and_suppresses_replay() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("clean-lxmf-known-source")
            .expect("sender identity");
        let signer =
            reticulum_rs::core::identity::PrivateIdentity::from_private_key_bytes(&private)
                .expect("core sender identity");
        let source = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(&private)
            .expect("LXMF delivery hash");
        let source_hex = hex_encode(&source);
        let envelope = MessageEnvelope {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            title: "Known source".into(),
            body: "Body".into(),
            delivery_mode: crate::messaging::DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound =
            crate::runtime::native_lxmf::codec::build_outbound_message(&envelope, &source_hex)
                .expect("outbound");
        let wire =
            crate::runtime::native_lxmf::codec::encode_signed_wire_message(&outbound, &private)
                .expect("signed wire");
        let mut identities = BTreeMap::new();
        identities.insert(
            source_hex.clone(),
            rns_transport::identity::Identity::new_from_slices(
                signer.as_identity().public_key_bytes(),
                signer.as_identity().verifying_key_bytes(),
            ),
        );
        let identities = Arc::new(Mutex::new(identities));
        let attachments_dir = temp_paths("clean-lxmf-known-source").attachments_dir;

        let first = decode_native_lxmf_payload_bounded(
            wire.clone(),
            attachments_dir.clone(),
            identities.clone(),
        )
        .await
        .expect("bounded decode")
        .message
        .expect("verified message");
        let second = decode_native_lxmf_payload_bounded(wire, attachments_dir, identities)
            .await
            .expect("repeat bounded decode")
            .message
            .expect("repeat verified message");
        let seen = Arc::new(Mutex::new(BTreeMap::new()));

        assert_eq!(first.peer_hash, source_hex);
        assert!(clean_lxmf_should_emit_message(&seen, &first));
        assert!(!clean_lxmf_should_emit_message(&seen, &second));
        assert_eq!(seen.lock().expect("seen lock").len(), 1);
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_lxmf_decode_rejects_cache_identity_that_does_not_match_source() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("clean-lxmf-cache-source")
            .expect("sender identity");
        let other_private = provider
            .create_identity_material("clean-lxmf-cache-other")
            .expect("other identity");
        let other =
            reticulum_rs::core::identity::PrivateIdentity::from_private_key_bytes(&other_private)
                .expect("other core identity");
        let source = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(&private)
            .expect("LXMF delivery hash");
        let source_hex = hex_encode(&source);
        let envelope = MessageEnvelope {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            title: "Mismatched source".into(),
            body: "Body".into(),
            delivery_mode: crate::messaging::DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let outbound =
            crate::runtime::native_lxmf::codec::build_outbound_message(&envelope, &source_hex)
                .expect("outbound");
        let wire =
            crate::runtime::native_lxmf::codec::encode_signed_wire_message(&outbound, &private)
                .expect("signed wire");
        let mut identities = BTreeMap::new();
        identities.insert(
            source_hex,
            rns_transport::identity::Identity::new_from_slices(
                other.as_identity().public_key_bytes(),
                other.as_identity().verifying_key_bytes(),
            ),
        );

        let decoded = decode_native_lxmf_payload_bounded(
            wire,
            temp_paths("clean-lxmf-cache-mismatch").attachments_dir,
            Arc::new(Mutex::new(identities)),
        )
        .await
        .expect("bounded decode owns verification result");
        let error = decoded
            .message
            .expect_err("mismatched cached identity must be rejected");

        assert!(error.to_string().contains("does not match"));
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_propagated_lxmf_rejects_unknown_announced_sender() {
        let sender = reticulum_rs::core::identity::PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver =
            reticulum_rs::core::identity::PrivateIdentity::new_from_rand(rand_core::OsRng);
        let sender_hash = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(
            sender.to_private_key_bytes().as_slice(),
        )
        .expect("sender delivery hash");
        let receiver_hash = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("receiver delivery hash");
        let payload = lxmf::Payload::new(
            42.0,
            Some(b"Body".to_vec()),
            Some(b"Unknown propagated sender".to_vec()),
            None,
            None,
        );
        let mut wire = lxmf::WireMessage::new(receiver_hash, sender_hash, payload);
        wire.sign(&sender).expect("sign");
        let (encrypted, _) = wire
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("pack propagated");
        let attachments_dir = temp_paths("clean-propagated-unknown").attachments_dir;

        let decoded = decode_propagated_lxmf_payload_bounded(
            encrypted,
            receiver.to_private_key_bytes().to_vec(),
            attachments_dir.clone(),
            Some(Arc::new(Mutex::new(BTreeMap::new()))),
        )
        .await
        .expect("bounded decode owns verification result");
        assert_eq!(decoded.unresolved_source_hash, Some(sender_hash));
        let error = decoded
            .message
            .expect_err("unknown propagated sender must be deferred");

        assert!(error
            .to_string()
            .contains("authenticated lxmf.delivery announce"));
        assert!(!attachments_dir.exists());
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_propagated_lxmf_accepts_matching_announced_sender() {
        let sender = reticulum_rs::core::identity::PrivateIdentity::new_from_rand(rand_core::OsRng);
        let receiver =
            reticulum_rs::core::identity::PrivateIdentity::new_from_rand(rand_core::OsRng);
        let sender_hash = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(
            sender.to_private_key_bytes().as_slice(),
        )
        .expect("sender delivery hash");
        let receiver_hash = crate::runtime::native_lxmf::codec::lxmf_delivery_destination_hash_from_private_identity_bytes(
            receiver.to_private_key_bytes().as_slice(),
        )
        .expect("receiver delivery hash");
        let payload = lxmf::Payload::new(
            42.0,
            Some(b"Body".to_vec()),
            Some(b"Known propagated sender".to_vec()),
            None,
            None,
        );
        let mut wire = lxmf::WireMessage::new(receiver_hash, sender_hash, payload);
        wire.sign(&sender).expect("sign");
        let (encrypted, _) = wire
            .pack_propagation_transient_with_rng(receiver.as_identity(), rand_core::OsRng)
            .expect("pack propagated");
        let mut identities = BTreeMap::new();
        identities.insert(
            hex_encode(&sender_hash),
            rns_transport::identity::Identity::new_from_slices(
                sender.as_identity().public_key_bytes(),
                sender.as_identity().verifying_key_bytes(),
            ),
        );

        let message = decode_propagated_lxmf_payload_bounded(
            encrypted,
            receiver.to_private_key_bytes().to_vec(),
            temp_paths("clean-propagated-known").attachments_dir,
            Some(Arc::new(Mutex::new(identities))),
        )
        .await
        .expect("bounded decode");
        assert_eq!(message.unresolved_source_hash, None);
        let message = message.message.expect("verified propagated message");

        assert_eq!(message.peer_hash, hex_encode(&sender_hash));
        assert_eq!(message.title, "Known propagated sender");
        assert_eq!(
            message.transport_method,
            crate::messaging::TransportMethod::Propagated
        );
    }

    #[cfg(feature = "native-lxmf")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_lxmf_waiter_does_not_leak_blocking_permit() {
        let gate = Arc::new(Semaphore::new(1));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task_gate = gate.clone();
        let task = tokio::spawn(async move {
            run_native_lxmf_blocking(task_gate, move || {
                let _ = started_tx.send(());
                std::thread::sleep(Duration::from_millis(50));
            })
            .await
        });
        started_rx.await.expect("blocking job started");
        task.abort();
        let _ = task.await;
        assert_eq!(gate.available_permits(), 0);

        tokio::time::timeout(Duration::from_secs(1), async {
            while gate.available_permits() != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled blocking job released permit");
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn omenchat_link_close_events_only_emit_for_active_omenchat_links() {
        let active = Arc::new(Mutex::new(BTreeSet::from([[0x4f; 16]])));

        assert!(
            take_active_omenchat_link_close(&active, [0x11; 16], Some("Timeout".into())).is_none()
        );
        assert!(active.lock().expect("active").contains(&[0x4f; 16]));

        let closed = take_active_omenchat_link_close(&active, [0x4f; 16], Some("Timeout".into()))
            .expect("active OMENchat close");
        assert_eq!(closed.link_id, [0x4f; 16]);
        assert_eq!(closed.reason.as_deref(), Some("Timeout"));
        assert!(!active.lock().expect("active").contains(&[0x4f; 16]));

        assert!(
            take_active_omenchat_link_close(&active, [0x4f; 16], Some("Timeout".into())).is_none()
        );
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn live_rns_net_interface_stats_match_profiles_and_format_connection_state() {
        let mut interface =
            crate::interfaces::ReticulumInterfaceProfile::tcp_client("tcp", "GatewayOne");
        interface.target_host = "10.0.0.7".into();
        interface.target_port = 4242;
        let plan = plan_interfaces(&[interface])
            .into_iter()
            .next()
            .expect("interface plan");
        let stats = rns_net::InterfaceStatsResponse {
            interfaces: vec![rns_net::SingleInterfaceStat {
                id: 7,
                name: "GatewayOne".into(),
                status: true,
                mode: 0,
                rxb: 2048,
                txb: 4096,
                rx_packets: 2,
                tx_packets: 4,
                bitrate: None,
                ifac_size: None,
                started: 0.0,
                ia_freq: 0.0,
                oa_freq: 0.0,
                ip_freq: 0.0,
                op_freq: 0.0,
                op_samples: 0,
                burst_active: false,
                burst_activated: 0.0,
                pr_burst_active: false,
                pr_burst_activated: 0.0,
                clients: None,
                announce_rate_target: None,
                announce_rate_grace: 0,
                announce_rate_penalty: 0.0,
                interface_type: "TCPClientInterface".into(),
            }],
            transport_id: None,
            transport_enabled: true,
            transport_uptime: 1.0,
            total_rxb: 2048,
            total_txb: 4096,
            probe_responder: None,
            backbone_peer_pool: None,
        };

        let live = find_live_rns_net_interface(&stats, &plan, Some("10.0.0.7:4242"))
            .expect("matched live interface");

        assert!(live.status);
        assert_eq!(
            format_live_rns_net_interface_detail(live),
            "GatewayOne [7] TCPClientInterface | connected=true | rx=2.00 KiB in 2 pkt | tx=4.00 KiB in 4 pkt | ifac=none"
        );
        let ordered_fallback = rns_net::SingleInterfaceStat {
            id: 8,
            name: "tcp-client-0".into(),
            status: true,
            mode: 0,
            rxb: 1,
            txb: 2,
            rx_packets: 1,
            tx_packets: 1,
            bitrate: None,
            ifac_size: None,
            started: 0.0,
            ia_freq: 0.0,
            oa_freq: 0.0,
            ip_freq: 0.0,
            op_freq: 0.0,
            op_samples: 0,
            burst_active: false,
            burst_activated: 0.0,
            pr_burst_active: false,
            pr_burst_activated: 0.0,
            clients: None,
            announce_rate_target: None,
            announce_rate_grace: 0,
            announce_rate_penalty: 0.0,
            interface_type: "TCPClientInterface".into(),
        };

        assert_eq!(
            select_live_rns_net_interface(None, Some(&ordered_fallback))
                .expect("ordered fallback")
                .id,
            8
        );
        assert_eq!(
            select_live_rns_net_interface(Some(live), Some(&ordered_fallback))
                .expect("matched live interface")
                .id,
            7
        );
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn reused_page_link_failure_detection_covers_send_and_response_failures() {
        let reused = PageFetchProbeStep::ok(
            PageFetchProbeStage::LinkSetup,
            "rns-net reused active page link",
        );
        let request_send_failed = PageFetchProbeStep::failed(
            PageFetchProbeStage::RequestSend,
            "rns-net failed to send page request",
        );
        let response_wait_failed = PageFetchProbeStep::failed(
            PageFetchProbeStage::ResponseWait,
            "timed out waiting for page response",
        );
        let fresh_link =
            PageFetchProbeStep::ok(PageFetchProbeStage::LinkSetup, "rns-net link established");

        assert!(rns_net_observed_reused_page_link(&[reused.clone()]));
        assert!(rns_net_observed_stale_reused_page_link_failure(&[
            reused.clone(),
            request_send_failed,
        ]));
        assert!(rns_net_observed_stale_reused_page_link_failure(&[
            reused,
            response_wait_failed,
        ]));
        assert!(!rns_net_observed_reused_page_link(&[fresh_link.clone()]));
        assert!(!rns_net_observed_stale_reused_page_link_failure(&[
            fresh_link
        ]));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_cached_ratchet_available_reads_rns_net_store() {
        use rns_net::storage::RatchetStore;

        let paths = temp_paths("cached-ratchet");
        std::fs::create_dir_all(paths.reticulum_config_dir.join("storage").join("ratchets"))
            .unwrap();
        let destination_hash = [0x42; 16];

        assert!(!native_lxmf_cached_ratchet_available(
            paths.reticulum_config_dir.as_path(),
            &destination_hash,
        ));

        let store = rns_net::storage::FsRatchetStore::new(
            paths.reticulum_config_dir.join("storage").join("ratchets"),
        );
        store
            .remember(
                destination_hash,
                rns_net::storage::RatchetEntry {
                    ratchet: [0x77; 32],
                    received_at: rns_net::time::now(),
                },
            )
            .unwrap();

        assert!(native_lxmf_cached_ratchet_available(
            paths.reticulum_config_dir.as_path(),
            &destination_hash,
        ));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn local_lxmf_display_name_prefers_active_profile_label() {
        let identity = NativeIdentitySummary {
            address_hash_hex: "0123456789abcdef0123456789abcdef".into(),
            byte_len: 64,
        };
        let profile = IdentityProfile {
            label: "Renamed Identity".into(),
            path: std::path::PathBuf::from("identity"),
            hash_hex: "profile_hash".into(),
            managed: true,
        };

        assert_eq!(
            local_lxmf_display_name(&identity, Some(&profile)),
            "Renamed Identity"
        );
    }

    #[derive(Clone, Debug)]
    struct StaticPageTransport {
        response: NativePageResponse,
    }

    #[async_trait]
    impl NativePageTransportClient for StaticPageTransport {
        async fn fetch_page(
            &self,
            _plan: &NativeFetchPlan,
            _context: Option<&NativePageFetchContext>,
            cancel: CancellationToken,
        ) -> AppResult<NativePageResponse> {
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            Ok(self.response.clone())
        }
    }

    #[derive(Clone, Debug)]
    struct OperationCapturingPageTransport {
        observed: Arc<std::sync::Mutex<Option<String>>>,
    }

    #[async_trait]
    impl NativePageTransportClient for OperationCapturingPageTransport {
        async fn fetch_page(
            &self,
            _plan: &NativeFetchPlan,
            context: Option<&NativePageFetchContext>,
            cancel: CancellationToken,
        ) -> AppResult<NativePageResponse> {
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            *self.observed.lock().expect("operation capture lock") =
                context.and_then(|context| context.operation_id.clone());
            Ok(NativePageResponse {
                body: b">Correlated Page".to_vec(),
                content_type: Some("text/x-micron".into()),
            })
        }
    }

    #[tokio::test]
    async fn native_runtime_stores_browser_identify_policy() {
        let paths = temp_paths("identify-policy");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destinations = BTreeSet::from(["00112233445566778899aabbccddeeff".to_string()]);

        runtime
            .set_identify_on_connect_destinations(destinations.clone())
            .await
            .expect("set policy");

        assert_eq!(
            *runtime
                .identify_on_connect_destinations
                .lock()
                .expect("identify policy lock"),
            destinations
        );
    }

    #[cfg(feature = "native-lxmf-sdk")]
    #[tokio::test]
    async fn native_runtime_reports_missing_lxmf_sdk_rpc_endpoint_for_probe() {
        let paths = temp_paths("sdk-rpc-probe-missing-endpoint");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        let probe = runtime
            .native_lxmf_sdk_rpc_probe()
            .await
            .expect("probe snapshot");

        assert_eq!(probe.state, "missing_endpoint");
        assert_eq!(probe.endpoint, None);
        assert!(probe
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("not configured")));
    }

    #[cfg(feature = "native-lxmf-sdk")]
    #[tokio::test]
    async fn native_runtime_rejects_remote_rpc_probe_without_exposing_endpoint() {
        let paths = temp_paths("sdk-rpc-probe-rejected-remote");
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.native_lxmf_sdk_rpc_endpoint = Some("tcp://203.0.113.8:37428/private-rpc".into());
        let runtime = NativeNetworkRuntime::new(config);

        let probe = runtime
            .native_lxmf_sdk_rpc_probe()
            .await
            .expect("rejected probe snapshot");

        assert_eq!(probe.state, "rejected_endpoint");
        assert_eq!(probe.endpoint, None);
        assert!(probe
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("local-trusted")));
        assert!(!format!("{probe:?}").contains("203.0.113.8"));
        assert!(runtime
            .native_lxmf_sdk_rpc_status_summary()
            .contains("rejected_endpoint"));
    }

    #[test]
    fn rpc_capability_requires_a_compatible_snapshot() {
        let missing = LxmfSdkRpcProbeSnapshot {
            endpoint: None,
            state: "missing_endpoint".into(),
            runtime_id: None,
            active_contract_version: None,
            event_stream_position: None,
            config_revision: None,
            queued_messages: None,
            in_flight_messages: None,
            detail: Some("not configured".into()),
        };
        assert_eq!(
            rpc_capability_record(&missing).availability,
            RuntimeCapabilityAvailability::Unsupported
        );

        let negotiated = LxmfSdkRpcProbeSnapshot {
            endpoint: Some("unix:///run/user/1000/reticulumd.sock".into()),
            state: "running".into(),
            runtime_id: Some("runtime-1".into()),
            active_contract_version: Some(1),
            event_stream_position: Some(4),
            config_revision: Some(2),
            queued_messages: Some(0),
            in_flight_messages: Some(0),
            detail: None,
        };
        let capability = rpc_capability_record(&negotiated);
        assert_eq!(
            capability.availability,
            RuntimeCapabilityAvailability::Supported
        );
        assert_eq!(capability.source, RuntimeCapabilitySource::Negotiated);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_trait_lifecycle_and_capabilities_follow_active_transport() {
        let paths = temp_paths("facade-lifecycle-capabilities");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::New
        );
        runtime
            .start_runtime(Some(profile.clone()), Vec::new())
            .await
            .expect("start native runtime through trait");
        runtime
            .start_runtime(Some(profile.clone()), Vec::new())
            .await
            .expect("repeat identical native start");
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Running
        );

        let capabilities = runtime.capability_snapshot().await;
        assert_eq!(capabilities.backend, RuntimeBackendName::Reticulum);
        assert!(capabilities.supports(RuntimeCapability::IntegratedBackend));
        assert!(capabilities.supports(RuntimeCapability::DirectDelivery));
        assert_eq!(
            capabilities.availability(RuntimeCapability::EventStream),
            RuntimeCapabilityAvailability::Unsupported
        );
        assert_eq!(
            capabilities.availability(RuntimeCapability::InterfaceMutation),
            RuntimeCapabilityAvailability::Unknown
        );

        runtime.stop_runtime().await.expect("stop native runtime");
        runtime
            .stop_runtime()
            .await
            .expect("repeat native stop is idempotent");
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Stopped
        );
        let error = runtime
            .fetch_page(
                "00112233445566778899aabbccddeeff:/",
                None,
                CancellationToken::new(),
            )
            .await
            .expect_err("stopped runtime must reject new page work");
        assert!(error.to_string().contains("must be started"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_duplicate_start_rejects_conflicting_configuration() {
        let paths = temp_paths("facade-conflicting-start");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime
            .start_runtime(Some(profile.clone()), Vec::new())
            .await
            .expect("start native runtime");

        let mut conflicting = profile;
        conflicting.label = "Different profile".into();
        let error = runtime
            .start_runtime(Some(conflicting), Vec::new())
            .await
            .expect_err("conflicting start must fail");
        assert!(error
            .to_string()
            .contains("different identity or interface"));
        assert_eq!(
            runtime.lifecycle_snapshot().await.state,
            RuntimeLifecycleState::Running
        );

        runtime.stop_runtime().await.expect("stop native runtime");
    }

    #[tokio::test]
    async fn native_trait_start_failure_is_structured_and_redacted() {
        let paths = temp_paths("facade-start-failure");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let profile = IdentityProfile {
            label: "Missing".into(),
            path: paths.identities_dir.join("does-not-exist"),
            hash_hex: String::new(),
            managed: true,
        };

        assert!(runtime
            .start_runtime(Some(profile), Vec::new())
            .await
            .is_err());
        let snapshot = runtime.lifecycle_snapshot().await;
        assert_eq!(snapshot.state, RuntimeLifecycleState::Failed);
        let failure = snapshot.failure.expect("structured failure");
        assert!(failure.technical_detail.is_none());
        assert!(!failure.summary.contains("does-not-exist"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_runtime_starts_with_identity_and_reports_status() {
        let paths = temp_paths("start");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        runtime
            .start(Some(profile.clone()), Vec::new())
            .expect("start runtime");
        let status = runtime.status().await;

        assert!(status.connected);
        assert_eq!(
            status.active_identity.as_ref().map(|item| &item.path),
            Some(&profile.path)
        );
        let state = runtime.state_snapshot();
        assert!(state.active_identity.is_some());
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            assert!(state.transport_started);
            assert!(status
                .message
                .contains("Reticulum 0.9 transport is running"));
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            assert!(!state.transport_started);
            assert!(state.rns_net_started);
            assert!(status.message.contains("rns-net runtime is primary"));
            assert!(status.message.contains("local_lxmf_registered=true"));
            assert!(status.message.contains("proof_capable=true"));
            assert!(status.message.contains("announced=true"));
        }
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test(flavor = "multi_thread")]
    async fn native_runtime_restart_and_stop_cancel_owned_transport_tasks() {
        let paths = temp_paths("restart-stop-task-ownership");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        runtime
            .start(Some(profile.clone()), Vec::new())
            .expect("start first transport");
        let (first_transport, first_shutdown) = {
            let guard = runtime.transport.lock().expect("native transport lock");
            let handle = guard.as_ref().expect("first transport handle");
            (handle.transport.clone(), handle.shutdown.clone())
        };

        runtime
            .start(Some(profile), Vec::new())
            .expect("replace running transport");
        assert!(first_shutdown.is_cancelled());
        wait_for_transport_tasks_to_release(&first_transport).await;

        let (second_transport, second_shutdown) = {
            let guard = runtime.transport.lock().expect("native transport lock");
            let handle = guard.as_ref().expect("second transport handle");
            (handle.transport.clone(), handle.shutdown.clone())
        };
        assert!(!second_shutdown.is_cancelled());

        runtime.stop();

        assert!(second_shutdown.is_cancelled());
        wait_for_transport_tasks_to_release(&second_transport).await;
        assert_eq!(
            runtime.state_snapshot().lifecycle,
            NativeRuntimeLifecycle::Stopped
        );
        assert!(runtime
            .transport
            .lock()
            .expect("native transport lock")
            .is_none());
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[tokio::test]
    async fn native_rns_net_start_respects_announce_on_start_setting() {
        let paths = temp_paths("announce-disabled");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.announce_on_start = false;
        let runtime = NativeNetworkRuntime::new(config);

        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");
        let status = runtime.status().await;

        assert!(status.message.contains("local_lxmf_registered=true"));
        assert!(status.message.contains("proof_capable=true"));
        assert!(status.message.contains("announced=false"));
    }

    #[tokio::test]
    async fn native_runtime_can_run_without_transport_until_identity_is_attached() {
        let paths = temp_paths("start-without-identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        runtime.start(None, Vec::new()).expect("start runtime");
        let status = runtime.status().await;

        assert!(status.connected);
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            assert!(!runtime.state_snapshot().transport_started);
            assert!(status
                .message
                .contains("without an active transport identity"));
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            assert!(runtime.state_snapshot().rns_net_started);
            assert!(status.message.contains("rns-net runtime is primary"));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_attach_identity_constructs_transport_when_runtime_is_running() {
        let paths = temp_paths("attach-transport");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        runtime.start(None, Vec::new()).expect("start runtime");
        runtime
            .attach_identity(profile.clone())
            .await
            .expect("attach identity");
        let stats = runtime.interface_stats().await.expect("interface stats");

        #[cfg(not(all(feature = "native-rns-net", any())))]
        assert!(runtime.state_snapshot().transport_started);
        #[cfg(all(feature = "native-rns-net", any()))]
        assert!(runtime.state_snapshot().rns_net_started);
        #[cfg(not(all(feature = "native-rns-net", any())))]
        assert!(stats
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("transport constructed")));
        #[cfg(all(feature = "native-rns-net", any()))]
        assert!(stats
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("rns-net runtime is primary")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_runtime_attaches_enabled_tcp_client_interface_plan() {
        let paths = temp_paths("tcp-client-attach");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let mut interface =
            crate::interfaces::ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
        interface.target_host = "127.0.0.1".into();
        interface.target_port = 4242;
        let plans = plan_interfaces(&[interface]);
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        runtime
            .start(Some(profile), plans)
            .expect("start runtime with tcp client");
        let stats = runtime.interface_stats().await.expect("interface stats");

        #[cfg(not(all(feature = "native-rns-net", any())))]
        assert!(runtime.state_snapshot().transport_started);
        #[cfg(all(feature = "native-rns-net", any()))]
        assert!(runtime.state_snapshot().rns_net_started);
        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            assert!(stats
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("1 TCP client interfaces attached")));
            assert!(stats
                .interfaces
                .iter()
                .any(|line| line.contains("attached Gateway tcp_client 127.0.0.1:4242")));
            let sample = stats
                .samples
                .iter()
                .find(|sample| sample.profile_id == "tcp")
                .expect("structured interface sample");
            assert_eq!(sample.endpoint.as_deref(), Some("127.0.0.1:4242"));
            assert!(sample.attached);
            assert_eq!(sample.state, InterfaceSampleState::Attached);
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            assert!(stats
                .interfaces
                .iter()
                .any(|line| line.contains("attached rns-net primary runtime")));
            let sample = stats
                .samples
                .iter()
                .find(|sample| sample.profile_id == "tcp")
                .expect("structured interface sample");
            assert_eq!(sample.endpoint.as_deref(), Some("127.0.0.1:4242"));
            assert!(!sample.attached);
            assert_eq!(sample.state, InterfaceSampleState::Configured);
        }
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn managed_rns_net_identity_config_inserts_and_updates_network_identity() {
        let existing = "# header
[reticulum]
share_instance = Yes
enable_transport = No

[interfaces]
";
        let identity_path = Path::new("/tmp/omen/identity-one");

        let rendered = render_config_with_network_identity(existing, identity_path);
        let updated =
            render_config_with_network_identity(&rendered, Path::new("/tmp/omen/identity-two"));

        assert!(rendered.contains("network_identity = /tmp/omen/identity-one"));
        assert_eq!(
            read_network_identity_value(&rendered).as_deref(),
            Some("/tmp/omen/identity-one")
        );
        assert_eq!(
            read_network_identity_value(&updated).as_deref(),
            Some("/tmp/omen/identity-two")
        );
        assert!(!updated.contains("identity-one"));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn sync_managed_rns_net_identity_config_preserves_identity_file_and_reports_alignment() {
        let paths = temp_paths("rns-net-identity-sync");
        let identity_path = paths.identities_dir.join("default_identity");
        std::fs::create_dir_all(&paths.identities_dir).expect("identity dir");
        std::fs::write(&identity_path, b"identity-material").expect("identity file");
        std::fs::create_dir_all(&paths.reticulum_config_dir).expect("config dir");
        std::fs::write(
            paths.reticulum_config_dir.join("config"),
            "[reticulum]\nshare_instance = Yes\n\n[interfaces]\n",
        )
        .expect("config");

        sync_managed_rns_net_identity_config(&paths.reticulum_config_dir, &identity_path)
            .expect("sync identity config");

        let config =
            std::fs::read_to_string(paths.reticulum_config_dir.join("config")).expect("config");
        let identity = std::fs::read(&identity_path).expect("identity");
        assert!(config.contains(&format!("network_identity = {}", identity_path.display())));
        assert_eq!(identity, b"identity-material");
        assert!(managed_rns_net_identity_config_matches(
            &paths.reticulum_config_dir,
            &identity_path
        )
        .expect("alignment"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_runtime_does_not_attach_disabled_tcp_client_interface_plan() {
        let paths = temp_paths("tcp-client-disabled");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let mut interface =
            crate::interfaces::ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
        interface.enabled = false;
        let plans = plan_interfaces(&[interface]);
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        runtime
            .start(Some(profile), plans)
            .expect("start runtime with disabled tcp client");
        let stats = runtime.interface_stats().await.expect("interface stats");

        #[cfg(not(all(feature = "native-rns-net", any())))]
        {
            assert!(stats
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("0 TCP client interfaces attached")));
            assert!(!stats
                .interfaces
                .iter()
                .any(|line| line.contains("attached Gateway")));
        }
        #[cfg(all(feature = "native-rns-net", any()))]
        assert!(stats
            .interfaces
            .iter()
            .any(|line| line.contains("attached rns-net primary runtime")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_request_path_uses_constructed_transport_boundary() {
        let paths = temp_paths("request-path");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";

        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");

        assert!(runtime
            .request_path(destination, "test", false)
            .await
            .expect("request path"));
        let inspection = runtime
            .inspect_destination(destination, false)
            .await
            .expect("inspect destination");
        assert!(inspection.valid_length);
        assert!(!inspection.has_path);
        assert_eq!(inspection.hops, None);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test(flavor = "multi_thread")]
    async fn native_destination_inspection_uses_bounded_announce_app_data_cache() {
        let paths = temp_paths("inspect-app-data-cache");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";
        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");
        insert_bounded_destination_app_data(
            &mut runtime
                .clean_destination_app_data
                .lock()
                .expect("destination app-data cache lock"),
            destination.into(),
            b"invalid-propagation-app-data".to_vec(),
        );

        let inspection = runtime
            .inspect_destination(destination, true)
            .await
            .expect("inspect cached destination");

        assert!(!inspection.has_path);
        assert_eq!(inspection.hops, None);
        assert!(inspection.known_app_data);
        assert_eq!(inspection.propagation_usable, Some(false));
        runtime.stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn native_warm_paths_validates_and_bounds_requests() {
        let paths = temp_paths("warm-paths");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");

        let warmed = runtime
            .warm_paths(
                &[
                    "00112233445566778899aabbccddeeff".into(),
                    "fedcba98765432100123456789abcdef".into(),
                ],
                1,
                0,
            )
            .await
            .expect("warm paths");

        assert_eq!(warmed, 1);
    }

    #[tokio::test]
    async fn native_path_requests_reject_invalid_hash_without_panic() {
        let paths = temp_paths("invalid-path-hash");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        let error = runtime
            .request_path("not-a-reticulum-hash", "test", false)
            .await
            .expect_err("invalid path hash");
        let inspection = runtime
            .inspect_destination("not-a-reticulum-hash", false)
            .await
            .expect("invalid inspection is safe");

        assert!(error
            .to_string()
            .contains("invalid native Reticulum address"));
        assert!(!inspection.valid_length);
    }

    #[tokio::test]
    async fn native_request_path_requires_started_transport() {
        let paths = temp_paths("request-path-no-transport");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";

        let error = runtime
            .request_path(destination, "test", false)
            .await
            .expect_err("missing transport");

        assert!(error.to_string().contains("transport is not started"));
    }

    #[tokio::test]
    async fn native_directory_candidates_are_backed_by_announce_state() {
        let paths = temp_paths("announce-candidates");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime.record_announce(AnnouncePayload {
            destination_hash: "00112233445566778899aabbccddeeff".into(),
            identity_hash: None,
            display_name: "Node".into(),
            kind: DirectoryKind::Node,
            associated_hash: Some("ffeeddccbbaa99887766554433221100".into()),
            node_associated_hash: None,
            has_ratchet: false,
            lxmf_stamp_cost: None,
        });

        let candidates = runtime
            .directory_candidates(Some(1), true)
            .await
            .expect("candidates");
        let snapshot = runtime.network_snapshot().await.expect("snapshot");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display_name, "Node");
        assert_eq!(snapshot.announce_counts.get("node"), Some(&1));
        assert_eq!(snapshot.pending_announces, 1);
    }

    #[tokio::test]
    async fn native_runtime_event_subscription_receives_announces() {
        let paths = temp_paths("announce-events");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let mut events = runtime
            .subscribe_events()
            .expect("native runtime event subscription");

        runtime.record_announce(AnnouncePayload {
            destination_hash: "00112233445566778899aabbccddeeff".into(),
            identity_hash: None,
            display_name: "Node".into(),
            kind: DirectoryKind::Node,
            associated_hash: None,
            node_associated_hash: None,
            has_ratchet: false,
            lxmf_stamp_cost: None,
        });

        let event = events.recv().await.expect("runtime event");
        assert!(matches!(
            event,
            RuntimeBusEvent::Announce(AnnouncePayload {
                kind: DirectoryKind::Node,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn native_fetch_validates_cancellation_before_request_mapping() {
        let paths = temp_paths("fetch-cancelled");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let cancel = CancellationToken::new();
        cancel.cancel();

        let error = runtime
            .fetch_page("not even a native address", None, cancel)
            .await
            .expect_err("cancelled");

        assert!(error.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn native_fetch_rejects_non_hash_destination_without_leaking_name() {
        let paths = temp_paths("fetch-invalid");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        let error = runtime
            .fetch_page("mock.node:/page/index.mu", None, CancellationToken::new())
            .await
            .expect_err("invalid destination");

        assert!(error
            .to_string()
            .contains("invalid native Reticulum address"));
        assert!(!error.to_string().contains("mock.node"));
    }

    #[tokio::test]
    async fn native_page_fetch_probe_reports_address_and_runtime_steps() {
        let paths = temp_paths("fetch-probe-not-running");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";

        let report = runtime
            .probe_page_fetch(&format!("{destination}:/"), false)
            .await
            .expect("probe report");

        assert_eq!(report.destination_hash.as_deref(), Some(destination));
        assert_eq!(report.path.as_deref(), Some("/page/index.mu"));
        assert!(!report.ready_to_request);
        assert_eq!(report.steps[0].stage, PageFetchProbeStage::AddressParse);
        assert!(report
            .steps
            .iter()
            .any(|step| step.stage == PageFetchProbeStage::RuntimeSetup && !step.ok));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn native_reticulum09_page_fetch_probe_reports_direct_and_resource_selection() {
        let paths = temp_paths("fetch-probe-reticulum09");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";
        runtime.start(None, Vec::new()).expect("start runtime");

        let report = runtime
            .probe_page_fetch(&format!("{destination}:/"), true)
            .await
            .expect("probe report");

        assert_eq!(report.destination_hash.as_deref(), Some(destination));
        assert_eq!(report.path.as_deref(), Some("/page/index.mu"));
        assert!(!report.ready_to_request);
        assert!(report.steps.iter().any(|step| {
            step.stage == PageFetchProbeStage::RuntimeSetup
                && step.ok
                && step.detail.contains("runtime is running")
        }));
        assert!(report.steps.iter().any(|step| {
            step.stage == PageFetchProbeStage::LinkSetup
                && step.ok
                && step
                    .trace
                    .get("capability")
                    .is_some_and(|value| value == "destination-links")
        }));
        assert!(report.steps.iter().any(|step| {
            step.stage == PageFetchProbeStage::RequestSend
                && step.ok
                && step.detail.contains("direct request-context")
                && step.detail.contains("request-resource")
                && step
                    .trace
                    .get("state")
                    .is_some_and(|value| value == "available")
        }));
        assert!(!report
            .steps
            .iter()
            .any(|step| step.stage == PageFetchProbeStage::ResponseWait && !step.ok));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn native_fetch_valid_hash_uses_page_transport_boundary() {
        let paths = temp_paths("fetch-unsupported");
        let runtime = NativeNetworkRuntime::with_page_transport(
            NativeRuntimeConfig::from_paths(&paths),
            Arc::new(StaticPageTransport {
                response: NativePageResponse {
                    body: b">Native Fetch\nLoaded over native boundary".to_vec(),
                    content_type: Some("text/x-micron".into()),
                },
            }),
        );
        let destination = "00112233445566778899aabbccddeeff";
        runtime.start(None, Vec::new()).expect("start runtime");

        let page = runtime
            .fetch_page(
                &format!("{destination}:/page/index.mu"),
                None,
                CancellationToken::new(),
            )
            .await
            .expect("native page");

        assert_eq!(page.url, format!("{destination}:/page/index.mu"));
        assert_eq!(page.title, "Native Fetch");
        assert_eq!(page.source, PageSource::Network);
        assert_eq!(
            page.metadata
                .get("content_type")
                .and_then(serde_json::Value::as_str),
            Some("text/x-micron")
        );
        assert_eq!(
            page.metadata
                .get("native_request_backend")
                .and_then(serde_json::Value::as_str),
            Some("reticulum-transport")
        );
        assert_eq!(
            page.metadata
                .get("native_request_primitive")
                .and_then(serde_json::Value::as_str),
            Some("direct-request")
        );
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test(flavor = "multi_thread")]
    async fn native_fetch_passes_browser_operation_id_to_page_transport_context() {
        let paths = temp_paths("fetch-operation-correlation");
        let observed = Arc::new(std::sync::Mutex::new(None));
        let runtime = NativeNetworkRuntime::with_page_transport(
            NativeRuntimeConfig::from_paths(&paths),
            Arc::new(OperationCapturingPageTransport {
                observed: observed.clone(),
            }),
        );
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let profile = manager
            .create_managed_identity_with_provider("Native", &NativeReticulumIdentityProvider)
            .expect("create identity");
        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");
        let destination = "00112233445566778899aabbccddeeff";

        runtime
            .fetch_page_with_operation(
                &format!("{destination}:/page/index.mu"),
                None,
                CancellationToken::new(),
                Some("browser-operation-42".into()),
            )
            .await
            .expect("native page");

        assert_eq!(
            observed.lock().expect("operation capture lock").as_deref(),
            Some("browser-operation-42")
        );
        runtime.stop();
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[tokio::test]
    async fn native_download_atomically_publishes_transport_bytes() {
        let paths = temp_paths("atomic-download");
        let runtime = NativeNetworkRuntime::with_page_transport(
            NativeRuntimeConfig::from_paths(&paths),
            Arc::new(StaticPageTransport {
                response: NativePageResponse {
                    body: b"complete transport bytes".to_vec(),
                    content_type: Some("application/octet-stream".into()),
                },
            }),
        );
        runtime.start(None, Vec::new()).expect("start runtime");
        let destination = "00112233445566778899aabbccddeeff";
        let downloads = paths.downloads_dir.join("atomic-downloads");
        std::fs::create_dir_all(&downloads).expect("download directory");
        std::fs::write(downloads.join("archive.bin"), b"previous").expect("existing download");

        let downloaded = runtime
            .download_file(
                &format!("{destination}:/files/archive.bin"),
                &downloads,
                CancellationToken::new(),
            )
            .await
            .expect("native download");

        assert_eq!(
            downloaded.path.file_name().and_then(|name| name.to_str()),
            Some("archive-1.bin")
        );
        assert_eq!(
            std::fs::read(&downloaded.path).expect("download bytes"),
            b"complete transport bytes"
        );
        assert_eq!(
            std::fs::read(downloads.join("archive.bin")).expect("existing bytes"),
            b"previous"
        );
        assert_eq!(
            std::fs::read_dir(downloads)
                .expect("temporary listing")
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn native_fetch_default_transport_remains_explicitly_unsupported() {
        let paths = temp_paths("fetch-default-unsupported");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";
        runtime.start(None, Vec::new()).expect("start runtime");

        let error = runtime
            .fetch_page(&format!("{destination}:/"), None, CancellationToken::new())
            .await
            .expect_err("default transport unsupported");

        #[cfg(not(all(feature = "native-rns-net", any())))]
        assert!(error.to_string().contains(
            "native Reticulum page transport needs a verified Link.request response API"
        ));
        #[cfg(all(feature = "native-rns-net", any()))]
        {
            let message = error.to_string();
            assert!(message.contains("destination identity"));
            assert!(message.contains(destination));
            assert!(!message.contains(":/"));
            assert!(!message.contains("not implemented"));
            assert!(!message.contains("Unsupported"));
        }
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[tokio::test]
    async fn native_rns_net_page_fetch_probe_reports_missing_destination_identity() {
        let paths = temp_paths("fetch-probe-missing-key");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let destination = "00112233445566778899aabbccddeeff";
        runtime.start(None, Vec::new()).expect("start runtime");

        let report = runtime
            .probe_page_fetch(&format!("{destination}:/"), false)
            .await
            .expect("probe report");

        assert!(!report.ready_to_request);
        assert!(report.steps.iter().any(|step| {
            step.stage == PageFetchProbeStage::DestinationIdentity
                && !step.ok
                && step.detail.contains("signing key")
        }));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn rns_net_announce_payload_classifies_node_peer_and_propagation() {
        fn key_for(
            identity_hash: [u8; 16],
            app: &str,
            aspect: &str,
            name: &[u8],
        ) -> RnsNetAnnounceKey {
            RnsNetAnnounceKey {
                destination_hash: super::rns_net_destination_hash(&identity_hash, app, aspect),
                identity_hash,
                signing_public_key: [2u8; 32],
                full_public_key: [3u8; 64],
                app_data: Some(name.to_vec()),
                hops: Some(1),
                packet_hash: None,
                observed_at: 1.0,
            }
        }

        let identity_hash = [9u8; 16];
        let node =
            rns_net_announce_payload(&key_for(identity_hash, "nomadnetwork", "node", b"Node One"));
        let peer =
            rns_net_announce_payload(&key_for(identity_hash, "lxmf", "delivery", b"Peer One"));
        let propagation =
            rns_net_announce_payload(&key_for(identity_hash, "lxmf", "propagation", b"Relay One"));

        assert_eq!(node.kind, DirectoryKind::Node);
        assert_eq!(node.display_name, "Node One");
        assert_eq!(peer.kind, DirectoryKind::Peer);
        assert_eq!(
            peer.associated_hash.as_deref(),
            Some(node.destination_hash.as_str())
        );
        assert_eq!(propagation.kind, DirectoryKind::Propagation);
        assert_eq!(
            propagation.node_associated_hash.as_deref(),
            Some(node.destination_hash.as_str())
        );
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn rns_net_sibling_destination_hashes_cover_python_omen_aspects() {
        let identity_hash = [7u8; 16];
        let key = RnsNetAnnounceKey {
            destination_hash: super::rns_net_destination_hash(&identity_hash, "lxmf", "delivery"),
            identity_hash,
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: Some(b"Peer".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        };

        let siblings = super::rns_net_sibling_destination_hashes(&key);

        assert_eq!(
            siblings[0],
            super::rns_net_destination_hash(&identity_hash, "nomadnetwork", "node")
        );
        assert_eq!(
            siblings[1],
            super::rns_net_destination_hash(&identity_hash, "lxmf", "delivery")
        );
        assert_eq!(
            siblings[2],
            super::rns_net_destination_hash(&identity_hash, "lxmf", "propagation")
        );
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn rns_net_path_update_emits_only_exact_known_path_event() {
        let identity_hash = [7u8; 16];
        let node_hash = super::rns_net_destination_hash(&identity_hash, "nomadnetwork", "node");
        let delivery_hash = super::rns_net_destination_hash(&identity_hash, "lxmf", "delivery");
        let propagation_hash =
            super::rns_net_destination_hash(&identity_hash, "lxmf", "propagation");
        let mut store = RnsNetDestinationKeyStore::default();
        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: delivery_hash,
            identity_hash,
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: Some(b"Peer".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        });
        let store = Arc::new(Mutex::new(store));
        let (tx, mut rx) = broadcast::channel(8);

        emit_rns_net_path_update_with_siblings(
            &tx,
            &store,
            RnsNetPathUpdate {
                destination_hash: delivery_hash,
                hops: 2,
            },
        );

        let mut destinations = Vec::new();
        let mut related_logs = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                RuntimeBusEvent::PathUpdated(path) => destinations.push(path.destination_hash),
                RuntimeBusEvent::Debug(line) => related_logs.push(line),
                _ => {}
            }
        }
        assert_eq!(destinations, vec![hex_encode(&delivery_hash)]);
        assert!(related_logs.iter().any(|line| {
            line.contains(&hex_encode(&node_hash)) && line.contains("related sibling path evidence")
        }));
        assert!(related_logs.iter().any(|line| {
            line.contains(&hex_encode(&propagation_hash))
                && line.contains("related sibling path evidence")
        }));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn native_directory_announces_suppress_unclassified_raw_announces() {
        let identity_hash = [7u8; 16];
        let unknown = RnsNetAnnounceKey {
            destination_hash: super::rns_net_destination_hash(&identity_hash, "other", "aspect"),
            identity_hash,
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: Some(b"Raw Other".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        };
        let payload = rns_net_announce_payload(&unknown);

        assert_eq!(payload.kind, DirectoryKind::Unknown);
        assert!(!should_emit_directory_announce(&payload));
    }

    #[cfg(all(feature = "native-rns-net", feature = "native-lxmf"))]
    #[test]
    fn propagation_readiness_requires_valid_python_style_app_data() {
        let identity_hash = [8u8; 16];
        let mut invalid = RnsNetAnnounceKey {
            destination_hash: super::rns_net_destination_hash(
                &identity_hash,
                "lxmf",
                "propagation",
            ),
            identity_hash,
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: Some(b"Relay".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        };

        assert!(!rns_net_propagation_app_data_valid(&invalid));

        let value = rmpv::Value::Array(vec![
            rmpv::Value::Nil,
            rmpv::Value::from(1_700_000_000_u64),
            rmpv::Value::Boolean(true),
            rmpv::Value::from(256_u64),
            rmpv::Value::from(10_240_u64),
            rmpv::Value::Array(vec![
                rmpv::Value::from(16_u64),
                rmpv::Value::from(3_u64),
                rmpv::Value::from(18_u64),
            ]),
            rmpv::Value::Map(vec![(
                rmpv::Value::from(0x01_u64),
                rmpv::Value::Binary(b"Relay".to_vec()),
            )]),
        ]);
        let mut encoded = Vec::new();
        rmpv::encode::write_value(&mut encoded, &value).expect("encode propagation app data");
        invalid.app_data = Some(encoded);

        assert!(rns_net_propagation_app_data_valid(&invalid));
    }

    #[test]
    fn native_runtime_rejects_invalid_identity_path() {
        let paths = temp_paths("invalid-identity");
        let bad_path = paths.identities_dir.join("bad_identity");
        std::fs::create_dir_all(&paths.identities_dir).expect("create identities dir");
        std::fs::write(&bad_path, b"not-valid").expect("write bad identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let profile = IdentityProfile {
            label: "bad".into(),
            path: bad_path,
            hash_hex: "bad".into(),
            managed: true,
        };

        let error = runtime
            .start(Some(profile), Vec::new())
            .expect_err("invalid identity");

        assert!(error.to_string().contains("identity is invalid"));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[tokio::test]
    async fn native_lxmf_direct_send_requires_active_identity_before_dispatch() {
        let paths = temp_paths("lxmf-send-no-identity");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime.start(None, Vec::new()).expect("start runtime");

        let error = runtime
            .send_message(MessageEnvelope {
                peer_hash: "00112233445566778899aabbccddeeff".into(),
                title: "Subject".into(),
                body: "Body".into(),
                delivery_mode: crate::messaging::DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                attachments: Vec::new(),
            })
            .await
            .expect_err("identity required");

        assert!(error.to_string().contains("identity is missing"));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[tokio::test]
    async fn native_lxmf_propagated_send_queues_with_router_deferred_metadata() {
        let paths = temp_paths("lxmf-send-propagated");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let native_identity =
            load_private_identity_file(&profile.path).expect("native identity summary");
        let identity_hash = parse_rns_net_destination_hash(&native_identity.address_hash_hex)
            .expect("identity hash");
        let expected_source_hash = hex_encode(&super::rns_net_destination_hash(
            &identity_hash,
            "lxmf",
            "delivery",
        ));
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");
        runtime
            .set_outbound_propagation_node(Some("fedcba98765432100123456789abcdef".into()))
            .await
            .expect("set propagation node");

        let message = runtime
            .send_message(MessageEnvelope {
                peer_hash: "00112233445566778899aabbccddeeff".into(),
                title: "Subject".into(),
                body: "Body".into(),
                delivery_mode: crate::messaging::DeliveryMode::Propagated,
                include_ticket: false,
                native_reply_ticket: None,
                attachments: Vec::new(),
            })
            .await
            .expect("queued propagated");

        assert_eq!(
            message.transport_method,
            crate::messaging::TransportMethod::Propagated
        );
        assert!(!message.delivered);
        assert!(!message.failed);
        assert_eq!(
            message.fields.get("native_lxmf_state").map(String::as_str),
            Some("queued_for_propagation")
        );
        assert_eq!(
            message
                .fields
                .get("native_lxmf_propagation_transfer_state")
                .map(String::as_str),
            Some("router_deferred")
        );
        assert_eq!(
            message
                .fields
                .get("native_lxmf_propagation_node")
                .map(String::as_str),
            Some("fedcba98765432100123456789abcdef")
        );
        assert_eq!(
            message
                .fields
                .get("native_lxmf_source_hash")
                .map(String::as_str),
            Some(expected_source_hash.as_str())
        );

        let snapshot = runtime
            .propagation_debug_snapshot(message.message_id.clone())
            .await
            .expect("propagation snapshot");
        assert_eq!(
            snapshot.selected_node.as_deref(),
            Some("fedcba98765432100123456789abcdef")
        );
        assert!(snapshot
            .pending_deferred_ids
            .iter()
            .any(|id| Some(id.as_str()) == message.message_id.as_deref()));
        assert_eq!(
            snapshot
                .message
                .as_ref()
                .map(|message| message.origin.as_str()),
            Some("pending_deferred")
        );
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[tokio::test]
    async fn native_lxmf_direct_send_falls_back_to_propagated_when_path_missing_and_node_selected()
    {
        let paths = temp_paths("lxmf-direct-fallback-propagated");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create identity");
        let peer_hash = "00112233445566778899aabbccddeeff";
        crate::runtime::native::rns_net::write_known_destinations_fixture(
            &paths
                .reticulum_config_dir
                .join("storage/known_destinations"),
            parse_rns_net_destination_hash(peer_hash).expect("peer hash"),
        )
        .expect("known destination fixture");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime
            .start(Some(profile), Vec::new())
            .expect("start runtime");
        runtime
            .set_outbound_propagation_node(Some("fedcba98765432100123456789abcdef".into()))
            .await
            .expect("set propagation node");

        let message = runtime
            .send_message(MessageEnvelope {
                peer_hash: peer_hash.into(),
                title: "Subject".into(),
                body: "Body".into(),
                delivery_mode: crate::messaging::DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                attachments: Vec::new(),
            })
            .await
            .expect("direct fallback queued propagated");

        assert_eq!(
            message.transport_method,
            crate::messaging::TransportMethod::Propagated
        );
        assert!(!message.delivered);
        assert!(!message.failed);
        assert_eq!(
            message.fields.get("native_lxmf_state").map(String::as_str),
            Some("queued_for_propagation")
        );
        assert_eq!(
            message
                .fields
                .get("native_lxmf_fallback")
                .map(String::as_str),
            Some("direct_to_propagated")
        );
        assert!(message
            .fields
            .get("native_lxmf_failure_reason")
            .is_some_and(|reason| reason.contains("direct path missing")));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_resource_events_update_pending_propagated_status() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([(
            "message-a".into(),
            PendingNativePropagatedLxmf {
                peer_hash: "peer-a".into(),
                propagation_node: "node-a".into(),
                submitted_at: 12.5,
                has_path: true,
                known_app_data: true,
                link_id: Some("01010101010101010101010101010101".into()),
                transfer_state: "resource_advertised".into(),
                peer_activity_observed_at: None,
                terminal_at: None,
            },
        )])));

        let progress = native_lxmf_resource_status_for_event(
            RnsNetResourceEvent::Progress {
                link_id: [1u8; 16],
                received: 2,
                total: 5,
            },
            &pending,
        )
        .expect("progress status");

        assert_eq!(progress.0.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert_eq!(progress.0.message_id.as_deref(), Some("message-a"));
        assert!(progress
            .0
            .evidence
            .as_deref()
            .unwrap_or_default()
            .contains("propagation_transfer_state:resource_progress"));
        assert!(progress.1.is_none());
        assert_eq!(
            pending
                .lock()
                .expect("pending")
                .get("message-a")
                .map(|entry| entry.transfer_state.as_str()),
            Some("resource_progress")
        );

        let completed = native_lxmf_resource_status_for_event(
            RnsNetResourceEvent::Completed { link_id: [1u8; 16] },
            &pending,
        )
        .expect("completed status");

        assert_eq!(completed.0.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert!(!completed.0.delivered);
        assert_eq!(completed.0.message_id.as_deref(), Some("message-a"));
        assert!(completed
            .0
            .evidence
            .as_deref()
            .unwrap_or_default()
            .contains("propagation_transfer_state:resource_completed"));
        assert_eq!(
            completed.1.as_ref().map(|evidence| evidence.kind),
            Some(LxmfDeliveryEvidenceKind::PropagationNodeAccepted)
        );
        assert_eq!(
            pending
                .lock()
                .expect("pending")
                .get("message-a")
                .map(|entry| entry.transfer_state.as_str()),
            Some("resource_completed")
        );
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_direct_resource_completion_remains_peer_unconfirmed() {
        let pending: PendingDirectLxmfResources = Arc::new(Mutex::new(BTreeMap::from([(
            "02020202020202020202020202020202".into(),
            PendingNativeDirectLxmfResource {
                peer_hash: "peer-a".into(),
                message_id: "message-a".into(),
                submitted_at: 42.0,
                transfer_state: "resource_advertised".into(),
            },
        )])));

        let progress = native_lxmf_direct_resource_status_for_event(
            RnsNetResourceEvent::Progress {
                link_id: [2u8; 16],
                received: 2,
                total: 5,
            },
            &pending,
        )
        .expect("progress status");

        assert_eq!(progress.0.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert!(!progress.0.delivered);
        assert_eq!(progress.0.message_id.as_deref(), Some("message-a"));
        assert!(progress
            .0
            .evidence
            .as_deref()
            .unwrap_or_default()
            .contains("direct_transfer_state:resource_progress"));
        assert!(progress.1.is_none());

        let completed = native_lxmf_direct_resource_status_for_event(
            RnsNetResourceEvent::Completed { link_id: [2u8; 16] },
            &pending,
        )
        .expect("completed status");

        assert_eq!(completed.0.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert!(!completed.0.delivered);
        assert!(!completed.0.failed);
        assert_eq!(
            completed.1.as_ref().map(|evidence| evidence.kind),
            Some(LxmfDeliveryEvidenceKind::PacketSubmitted)
        );
        assert!(completed
            .0
            .evidence
            .as_deref()
            .is_some_and(|detail| detail.contains("peer_delivery_unconfirmed")));
        assert!(!pending
            .lock()
            .expect("pending")
            .contains_key("02020202020202020202020202020202"));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_direct_resource_timeout_remains_unconfirmed_not_failed() {
        let pending: PendingDirectLxmfResources = Arc::new(Mutex::new(BTreeMap::from([(
            "03030303030303030303030303030303".into(),
            PendingNativeDirectLxmfResource {
                peer_hash: "peer-a".into(),
                message_id: "message-a".into(),
                submitted_at: 42.0,
                transfer_state: "resource_advertised".into(),
            },
        )])));

        let failed = native_lxmf_direct_resource_status_for_event(
            RnsNetResourceEvent::Failed {
                link_id: [3u8; 16],
                error: "Resource transfer timeout".into(),
            },
            &pending,
        )
        .expect("timeout status");

        assert_eq!(failed.0.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert!(!failed.0.delivered);
        assert!(!failed.0.failed);
        assert_eq!(
            failed.1.as_ref().map(|evidence| evidence.kind),
            Some(LxmfDeliveryEvidenceKind::NoReceiptObserved)
        );
        assert_eq!(
            pending
                .lock()
                .expect("pending")
                .get("03030303030303030303030303030303")
                .map(|entry| entry.transfer_state.as_str()),
            Some("resource_timeout")
        );
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn active_propagated_outbound_summary_ignores_completed_peer_unconfirmed_rows() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([
            (
                "completed-a".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-a".into(),
                    propagation_node: "node-a".into(),
                    submitted_at: 1.0,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("01010101010101010101010101010101".into()),
                    transfer_state: "resource_completed".into(),
                    peer_activity_observed_at: None,
                    terminal_at: Some(2.0),
                },
            ),
            (
                "active-b".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-b".into(),
                    propagation_node: "node-b".into(),
                    submitted_at: 3.0,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("02020202020202020202020202020202".into()),
                    transfer_state: "resource_progress".into(),
                    peer_activity_observed_at: None,
                    terminal_at: None,
                },
            ),
        ])));

        let summary = native_lxmf_active_propagated_outbound_summary(&pending)
            .expect("active propagated outbound");
        assert!(summary.contains("message_id=active-b"));
        assert!(summary.contains("transfer_state=resource_progress"));

        pending
            .lock()
            .expect("pending")
            .get_mut("active-b")
            .expect("active")
            .transfer_state = "resource_completed".into();

        assert!(native_lxmf_active_propagated_outbound_summary(&pending).is_none());
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn stale_propagated_resource_progress_times_out_and_unblocks_sync() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([(
            "message-a".into(),
            PendingNativePropagatedLxmf {
                peer_hash: "peer-a".into(),
                propagation_node: "node-a".into(),
                submitted_at: 10.0,
                has_path: true,
                known_app_data: true,
                link_id: Some("01010101010101010101010101010101".into()),
                transfer_state: "resource_progress".into(),
                peer_activity_observed_at: None,
                terminal_at: None,
            },
        )])));

        assert!(native_lxmf_active_propagated_outbound_summary(&pending).is_some());
        let timed_out = native_lxmf_timeout_stale_propagated(&pending, 60.0, 45.0);
        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0].0.state, OutboundDeliveryState::Failed);
        assert_eq!(
            timed_out[0].0.evidence.as_deref().and_then(|detail| {
                extract_native_evidence_value(detail, "propagation_transfer_state")
            }),
            Some("resource_timeout")
        );
        assert!(native_lxmf_active_propagated_outbound_summary(&pending).is_none());
        assert_eq!(
            pending
                .lock()
                .expect("pending")
                .get("message-a")
                .map(|entry| entry.transfer_state.as_str()),
            Some("resource_timeout")
        );
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn advertised_propagated_resource_is_terminal_node_handoff() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([(
            "message-a".into(),
            PendingNativePropagatedLxmf {
                peer_hash: "peer-a".into(),
                propagation_node: "node-a".into(),
                submitted_at: 10.0,
                has_path: true,
                known_app_data: true,
                link_id: Some("01010101010101010101010101010101".into()),
                transfer_state: "resource_advertised".into(),
                peer_activity_observed_at: None,
                terminal_at: Some(11.0),
            },
        )])));

        assert!(native_lxmf_active_propagated_outbound_summary(&pending).is_none());
        assert!(native_lxmf_timeout_stale_propagated(&pending, 60.0, 45.0).is_empty());
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn propagation_no_payload_evidence_marks_completed_rows_peer_unconfirmed() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([
            (
                "message-a".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-a".into(),
                    propagation_node: "node-a".into(),
                    submitted_at: 12.5,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("01010101010101010101010101010101".into()),
                    transfer_state: "resource_completed".into(),
                    peer_activity_observed_at: None,
                    terminal_at: Some(13.0),
                },
            ),
            (
                "message-b".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-b".into(),
                    propagation_node: "node-b".into(),
                    submitted_at: 12.5,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("02020202020202020202020202020202".into()),
                    transfer_state: "resource_completed".into(),
                    peer_activity_observed_at: None,
                    terminal_at: Some(13.0),
                },
            ),
        ])));

        let evidence = native_lxmf_propagation_no_payload_evidence(&pending, "node-a", 0, 0, 2);

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].peer_hash, "peer-a");
        assert_eq!(evidence[0].message_id.as_deref(), Some("message-a"));
        assert_eq!(
            evidence[0].kind,
            LxmfDeliveryEvidenceKind::PropagationSyncNoPayloads
        );
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("requested:0")
                && detail.contains("decoded:0")
                && detail.contains("delivery_state:peer_delivery_unconfirmed")));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn propagation_sync_cleanup_message_names_reason_link_and_teardown_result() {
        let message =
            native_lxmf_propagation_sync_cleanup_message("list request failed", [0xabu8; 16], true);

        assert!(message.contains("reason=list request failed"));
        assert!(message.contains("link_id=abababababababababababababababab"));
        assert!(message.contains("torn_down=true"));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn propagation_sync_deferred_local_decrypts_are_not_acked_as_delivered() {
        let source = include_str!("adapter.rs");
        let marker = "leaving on propagation node for retry";
        let marker_index = source
            .find(marker)
            .expect("deferred local decrypt branch should be present");
        let start = marker_index.saturating_sub(800);
        let end = source.len().min(marker_index + marker.len() + 250);
        let branch = &source[start..end];

        assert!(branch.contains("deferred_count += 1"));
        assert!(!branch.contains("DeliveredTransientIdStore::mark_delivered"));
        assert!(!branch.contains("haves.push(transient_id)"));
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_propagation_sync_rejected_sender_is_not_acked_as_delivered() {
        let source = include_str!("adapter.rs");
        let marker = "native LXMF clean propagation sync deferred unauthenticated or undecryptable local payload";
        let marker_index = source
            .find(marker)
            .expect("clean deferred local payload branch should be present");
        let start = source[..marker_index]
            .rfind("Err(error) =>")
            .expect("clean decode error branch should be present");
        let end = marker_index
            + source[marker_index..]
                .find("continue;")
                .expect("clean deferred branch should continue without acknowledgement")
            + "continue;".len();
        let branch = &source[start..end];

        assert!(branch.contains("deferred_count += 1"));
        assert!(branch.contains("clean_propagation_admit_sender_path_request"));
        assert!(!branch.contains("DeliveredTransientIdStore::mark_delivered"));
        assert!(!branch.contains("haves.push(transient_id)"));
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_propagation_response_transient_admission_suppresses_duplicates() {
        let mut seen = BTreeSet::new();
        let first = [0x11; 32];
        let second = [0x22; 32];

        assert!(clean_propagation_admit_response_transient(&mut seen, first));
        assert!(!clean_propagation_admit_response_transient(
            &mut seen, first
        ));
        assert!(clean_propagation_admit_response_transient(
            &mut seen, second
        ));
        assert_eq!(seen, BTreeSet::from([first, second]));
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_propagation_sender_path_requests_are_unique_and_bounded() {
        let mut requested = BTreeSet::new();
        for value in 0..CLEAN_PROPAGATION_SENDER_PATH_REQUEST_MAX {
            let mut source = [0u8; 16];
            source[..8].copy_from_slice(&(value as u64).to_be_bytes());
            assert!(clean_propagation_admit_sender_path_request(
                &mut requested,
                source
            ));
            assert!(!clean_propagation_admit_sender_path_request(
                &mut requested,
                source
            ));
        }

        assert_eq!(requested.len(), CLEAN_PROPAGATION_SENDER_PATH_REQUEST_MAX);
        assert!(!clean_propagation_admit_sender_path_request(
            &mut requested,
            [0xff; 16]
        ));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn inbound_peer_message_evidence_marks_pending_propagated_outbound_activity() {
        let peer_hash = "00112233445566778899aabbccddeeff";
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([(
            "prop-a".into(),
            PendingNativePropagatedLxmf {
                peer_hash: peer_hash.into(),
                propagation_node: "fedcba98765432100123456789abcdef".into(),
                submitted_at: 123.456,
                has_path: true,
                known_app_data: true,
                link_id: Some("01010101010101010101010101010101".into()),
                transfer_state: "resource_completed".into(),
                peer_activity_observed_at: None,
                terminal_at: Some(123.0),
            },
        )])));
        let message = MessageSummary {
            peer_hash: peer_hash.into(),
            peer_label: "Peer".into(),
            title: "Reply".into(),
            content: "Body".into(),
            timestamp: 124.0,
            transport_method: crate::messaging::TransportMethod::Propagated,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("inbound-a".into()),
            fields: BTreeMap::new(),
            attachments: Vec::new(),
        };

        let destination_keys = Arc::new(Mutex::new(RnsNetDestinationKeyStore::default()));
        let evidence = native_lxmf_inbound_peer_propagated_evidence(
            &message,
            &pending,
            &destination_keys,
            "prop activity",
        );

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].peer_hash, peer_hash);
        assert_eq!(evidence[0].message_id.as_deref(), Some("prop-a"));
        assert_eq!(
            evidence[0].kind,
            LxmfDeliveryEvidenceKind::InboundPeerMessage
        );
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("prop activity")
                && detail.contains("peer_activity_observed:true")));
        assert!(pending
            .lock()
            .expect("pending")
            .get("prop-a")
            .and_then(|pending| pending.peer_activity_observed_at)
            .is_some());
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn terminal_propagated_pending_entries_prune_after_retention_window() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::from([
            (
                "old-completed".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-a".into(),
                    propagation_node: "node-a".into(),
                    submitted_at: 1.0,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("01010101010101010101010101010101".into()),
                    transfer_state: "resource_completed".into(),
                    peer_activity_observed_at: None,
                    terminal_at: Some(10.0),
                },
            ),
            (
                "recent-failed".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-b".into(),
                    propagation_node: "node-b".into(),
                    submitted_at: 20.0,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("02020202020202020202020202020202".into()),
                    transfer_state: "resource_failed".into(),
                    peer_activity_observed_at: None,
                    terminal_at: Some(95.0),
                },
            ),
            (
                "active-progress".into(),
                PendingNativePropagatedLxmf {
                    peer_hash: "peer-c".into(),
                    propagation_node: "node-c".into(),
                    submitted_at: 30.0,
                    has_path: true,
                    known_app_data: true,
                    link_id: Some("03030303030303030303030303030303".into()),
                    transfer_state: "resource_progress".into(),
                    peer_activity_observed_at: None,
                    terminal_at: None,
                },
            ),
        ])));

        let pruned = native_lxmf_prune_terminal_propagated(&pending, 100.0, 60.0);
        let pending = pending.lock().expect("pending");

        assert_eq!(pruned, 1);
        assert!(!pending.contains_key("old-completed"));
        assert!(pending.contains_key("recent-failed"));
        assert!(pending.contains_key("active-progress"));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn native_lxmf_recover_direct_correlation_restores_waiting_packet() {
        let pending: PendingLxmfProofs = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let message = MessageSummary {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            peer_label: "Peer".into(),
            title: "Subject".into(),
            content: "Body".into(),
            timestamp: 100.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("packet-a".into()),
            fields: BTreeMap::from([
                ("native_lxmf_state".into(), "submitted_to_rns_net".into()),
                (
                    "native_lxmf_proof_state".into(),
                    "waiting_for_packet_proof".into(),
                ),
                ("native_lxmf_packet_hash".into(), "packet-a".into()),
                ("native_lxmf_submitted_at".into(), "99.500".into()),
            ]),
            attachments: Vec::new(),
        };

        let recovered = native_lxmf_recover_direct_correlation(&pending, &[message]);
        let pending = pending.lock().expect("pending");

        assert_eq!(recovered, 1);
        assert_eq!(
            pending
                .pending("packet-a")
                .map(|entry| entry.peer_hash.as_str()),
            Some("00112233445566778899aabbccddeeff")
        );
        assert_eq!(
            pending.pending("packet-a").map(|entry| entry.submitted_at),
            Some(99.5)
        );
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_recover_propagated_correlation_restores_completed_transfer() {
        let pending: PendingPropagatedLxmf = Arc::new(Mutex::new(BTreeMap::new()));
        let message = MessageSummary {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            peer_label: "Peer".into(),
            title: "Subject".into(),
            content: "Body".into(),
            timestamp: 100.0,
            transport_method: crate::messaging::TransportMethod::Propagated,
            delivered: true,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some("prop-a".into()),
            fields: BTreeMap::from([
                (
                    "native_lxmf_state".into(),
                    "propagation_transfer_completed".into(),
                ),
                ("native_lxmf_message_id".into(), "prop-a".into()),
                (
                    "native_lxmf_propagation_transfer_state".into(),
                    "resource_completed".into(),
                ),
                (
                    "native_lxmf_propagation_node".into(),
                    "fedcba98765432100123456789abcdef".into(),
                ),
                (
                    "native_lxmf_propagation_link_id".into(),
                    "01010101010101010101010101010101".into(),
                ),
                ("native_lxmf_submitted_at".into(), "99.500".into()),
            ]),
            attachments: Vec::new(),
        };

        let recovered = native_lxmf_recover_propagated_correlation(&pending, &[message]);
        let pending = pending.lock().expect("pending");

        assert_eq!(recovered, 1);
        let entry = pending.get("prop-a").expect("recovered propagated");
        assert_eq!(entry.peer_hash, "00112233445566778899aabbccddeeff");
        assert_eq!(entry.propagation_node, "fedcba98765432100123456789abcdef");
        assert_eq!(entry.transfer_state, "resource_completed");
        assert_eq!(entry.terminal_at, Some(100.0));
    }

    #[tokio::test]
    async fn native_list_messages_drains_inbound_lxmf_queue() {
        let paths = temp_paths("lxmf-list-messages");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        runtime
            .inbound_messages
            .lock()
            .expect("inbound messages")
            .push(MessageSummary {
                peer_hash: "00112233445566778899aabbccddeeff".into(),
                peer_label: "Peer".into(),
                title: "Inbound".into(),
                content: "Hello".into(),
                timestamp: 1.0,
                transport_method: crate::messaging::TransportMethod::Direct,
                delivered: true,
                failed: false,
                incoming: true,
                unread: true,
                message_id: Some("inbound-1".into()),
                fields: BTreeMap::new(),
                attachments: Vec::new(),
            });

        let first = runtime.list_messages().await.expect("first list");
        let second = runtime.list_messages().await.expect("second list");

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[tokio::test]
    async fn native_propagation_node_selection_is_stored_and_reported() {
        let paths = temp_paths("lxmf-propagation-selection");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let hash = "00112233445566778899aabbccddeeff".to_string();

        runtime
            .set_outbound_propagation_node(Some(hash.clone()))
            .await
            .expect("set propagation");

        assert_eq!(
            runtime
                .get_outbound_propagation_node()
                .await
                .expect("get propagation"),
            Some(hash)
        );
        assert!(runtime
            .set_outbound_propagation_node(Some("not-a-hash".into()))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn native_propagation_status_reports_selected_node_before_router_support() {
        let paths = temp_paths("lxmf-propagation-status-selected");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let hash = "00112233445566778899aabbccddeeff".to_string();
        runtime
            .set_outbound_propagation_node(Some(hash.clone()))
            .await
            .expect("set propagation");

        let status = runtime.propagation_status().await.expect("status");

        assert!(status.selected);
        assert_eq!(status.destination_hash.as_deref(), Some(hash.as_str()));
        assert!(!status.has_path);
        assert_eq!(status.transfer_state, "router_deferred");
    }

    #[tokio::test]
    async fn native_propagation_sync_requires_selected_node() {
        let paths = temp_paths("lxmf-propagation-sync-no-node");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));

        let error = runtime
            .sync_propagation_messages(Some(10))
            .await
            .expect_err("sync should require selected propagation node");

        assert!(error.to_string().contains("selected propagation node"));
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python LXMF propagation enqueue/sync/ack interoperability test"]
    fn current_python_lxmf_propagation_sync_is_received_and_acknowledged() {
        run_python_lxmf_propagation_interop(
            "current-python-lxmf-propagation",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.0",
            "1.1.0",
        );
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python NomadNet request/response primitive interoperability test"]
    fn current_python_nomadnet_request_response_primitive_matrix_preserves_exact_bytes() {
        const INDEX_PAGE: &str = ">Current Python NomadNet\nempty request passed\n";
        const FORM_PAGE: &str = ">Current Python Form\nfield=omen\nnext=/page/index.mu\n";
        const OVERSIZED_FORM_PAGE: &str =
            ">Current Python Form\nfield_size=2048\nnext=/page/index.mu\n";
        let large_page = format!(
            ">Current Python Large Response\n{}",
            "resource-response-line\n".repeat(256)
        );

        let paths = temp_paths("current-python-nomadnet");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let identity = manager
            .create_managed_identity_with_provider("Native", &NativeReticulumIdentityProvider)
            .expect("create current Python NomadNet identity");
        let port = current_python_propagation_port();
        let peer = PythonNomadNetPeer::spawn(&paths.root.join("python-nomadnet"), port);
        let destination = peer.ready["destination"]
            .as_str()
            .expect("current Python NomadNet destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-nomadnet",
            "Python NomadNet IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 15;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python NomadNet Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity), plans)
                .expect("start current Python NomadNet runtime");
        });
        let started = Instant::now();
        let index = tokio
            .block_on(runtime.fetch_page(
                &format!("{destination}:/page/index.mu"),
                None,
                CancellationToken::new(),
            ))
            .expect("fetch current Python NomadNet index");
        assert_eq!(index.markup.as_bytes(), INDEX_PAGE.as_bytes());
        assert_eq!(index.source, PageSource::Network);
        assert_eq!(
            index
                .metadata
                .get("native_request_primitive")
                .and_then(serde_json::Value::as_str),
            Some("direct-request")
        );

        let request_data = BTreeMap::from([
            ("field_name".to_string(), "omen".to_string()),
            ("ignored_name".to_string(), "must-not-pass".to_string()),
            ("var_next".to_string(), "/page/index.mu".to_string()),
        ]);
        let form = tokio
            .block_on(runtime.fetch_page(
                &format!("{destination}:/page/form.mu"),
                Some(request_data),
                CancellationToken::new(),
            ))
            .expect("fetch current Python NomadNet form");
        assert_eq!(form.markup.as_bytes(), FORM_PAGE.as_bytes());
        assert_eq!(form.source, PageSource::Network);

        let mut page_events = runtime
            .subscribe_events()
            .expect("current Python NomadNet runtime events");
        let oversized_request_data = BTreeMap::from([
            ("field_name".to_string(), "x".repeat(2048)),
            ("var_next".to_string(), "/page/index.mu".to_string()),
        ]);
        let oversized_form = tokio
            .block_on(runtime.fetch_page(
                &format!("{destination}:/page/form.mu"),
                Some(oversized_request_data),
                CancellationToken::new(),
            ))
            .expect("fetch current Python NomadNet oversized form request");
        assert_eq!(
            oversized_form.markup.as_bytes(),
            OVERSIZED_FORM_PAGE.as_bytes()
        );
        assert_eq!(oversized_form.source, PageSource::Network);
        assert_eq!(
            oversized_form
                .metadata
                .get("native_request_primitive")
                .and_then(serde_json::Value::as_str),
            Some("request-resource")
        );
        let mut saw_outbound_request_resource_complete = false;
        while let Ok(event) = page_events.try_recv() {
            if let RuntimeBusEvent::ResourceLifecycle(lifecycle) = event {
                saw_outbound_request_resource_complete |= lifecycle.source.as_deref()
                    == Some("nomadnet-page")
                    && lifecycle.direction.as_deref() == Some("outbound")
                    && lifecycle.state == ResourceLifecycleState::Complete;
            }
        }
        assert!(saw_outbound_request_resource_complete);

        let large = tokio
            .block_on(runtime.fetch_page(
                &format!("{destination}:/page/large.mu"),
                None,
                CancellationToken::new(),
            ))
            .expect("fetch current Python NomadNet large response resource");
        assert_eq!(large.markup.as_bytes(), large_page.as_bytes());
        assert_eq!(large.source, PageSource::Network);
        assert_eq!(
            large
                .metadata
                .get("native_request_primitive")
                .and_then(serde_json::Value::as_str),
            Some("direct-request")
        );
        let mut saw_inbound_response_resource_complete = false;
        while let Ok(event) = page_events.try_recv() {
            if let RuntimeBusEvent::ResourceLifecycle(lifecycle) = event {
                saw_inbound_response_resource_complete |= lifecycle.source.as_deref()
                    == Some("nomadnet-page")
                    && lifecycle.direction.as_deref() == Some("inbound")
                    && lifecycle.state == ResourceLifecycleState::Complete
                    && lifecycle
                        .bytes
                        .is_some_and(|bytes| bytes >= large_page.len() as u64);
            }
        }
        assert!(saw_inbound_response_resource_complete);

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["served_page_requests"], 4);
        assert_eq!(result["index_bytes"], INDEX_PAGE.len());
        assert_eq!(result["form_bytes"], FORM_PAGE.len());
        assert_eq!(result["oversized_form_bytes"], OVERSIZED_FORM_PAGE.len());
        assert_eq!(result["large_page_bytes"], large_page.len());
        eprintln!(
            "current Python NomadNet request/response primitive matrix interoperated: elapsed_ms={}",
            started.elapsed().as_millis()
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python NomadNet timeout/cancellation interoperability test"]
    fn current_python_nomadnet_timeout_and_cancellation_are_bounded_without_replay() {
        let paths = temp_paths("current-python-nomadnet-faults");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let identity = manager
            .create_managed_identity_with_provider("Native", &NativeReticulumIdentityProvider)
            .expect("create current Python NomadNet fault identity");
        let port = current_python_propagation_port();
        let peer = PythonNomadNetPeer::spawn_scenario(
            &paths.root.join("python-nomadnet-faults"),
            port,
            "faults",
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("current Python NomadNet fault destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-nomadnet-faults",
            "Python NomadNet Fault IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 2;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python NomadNet fault Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity), plans)
                .expect("start current Python NomadNet fault runtime");
        });
        let started = Instant::now();
        let mut timeout_events = runtime
            .subscribe_events()
            .expect("current Python NomadNet timeout events");
        let timeout_error = tokio
            .block_on(runtime.fetch_page(
                &format!("{destination}:/page/timeout.mu"),
                None,
                CancellationToken::new(),
            ))
            .expect_err("current Python delayed response must time out");
        assert!(
            timeout_error
                .to_string()
                .contains("timeout during NomadNet direct request response"),
            "unexpected timeout error: {timeout_error}"
        );
        let mut timeout_request_observed = false;
        while let Ok(event) = timeout_events.try_recv() {
            timeout_request_observed |= matches!(
                event,
                RuntimeBusEvent::Debug(line)
                    if line.contains("direct page request sent")
                        && line.contains("path=/page/timeout.mu")
            );
        }
        assert!(timeout_request_observed);

        // The Python handler deliberately finishes after the Rust response
        // deadline. Let that one handler drain before opening the cancellation
        // case so the test measures cancellation rather than server scheduling.
        tokio.block_on(async {
            tokio::time::sleep(Duration::from_millis(1_250)).await;
        });

        let cancel = CancellationToken::new();
        let cancel_trigger = cancel.clone();
        let mut cancel_events = runtime
            .subscribe_events()
            .expect("current Python NomadNet cancellation events");
        let cancel_url = format!("{destination}:/page/cancel.mu");
        let (cancel_result, request_observed) = tokio.block_on(async {
            tokio::join!(runtime.fetch_page(&cancel_url, None, cancel), async move {
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        match cancel_events.recv().await {
                            Ok(RuntimeBusEvent::Debug(line))
                                if line.contains("direct page request sent")
                                    && line.contains("path=/page/cancel.mu") =>
                            {
                                break;
                            }
                            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                panic!("current Python NomadNet event stream closed")
                            }
                        }
                    }
                })
                .await
                .expect("current Python cancellation request dispatch timeout");
                cancel_trigger.cancel();
                true
            })
        });
        assert!(request_observed);
        let cancel_error = cancel_result.expect_err("current Python request must be cancelled");
        assert!(
            cancel_error.to_string().contains("operation cancelled"),
            "unexpected cancellation error: {cancel_error}"
        );

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["scenario"], "faults");
        assert_eq!(result["served_page_requests"], 2);
        eprintln!(
            "current Python NomadNet timeout/cancellation remained bounded without replay: elapsed_ms={}",
            started.elapsed().as_millis()
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python NomadNet repeated-request link-reuse interoperability test"]
    fn current_python_nomadnet_repeated_requests_reuse_one_active_link() {
        const FIRST_PAGE: &str = ">Current Python Repeated Request\nvisit=1\nsame_link=initial\n";
        const SECOND_PAGE: &str = ">Current Python Repeated Request\nvisit=2\nsame_link=true\n";

        let paths = temp_paths("current-python-nomadnet-reuse");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let identity = manager
            .create_managed_identity_with_provider("Native", &NativeReticulumIdentityProvider)
            .expect("create current Python NomadNet reuse identity");
        let port = current_python_propagation_port();
        let peer = PythonNomadNetPeer::spawn_scenario(
            &paths.root.join("python-nomadnet-reuse"),
            port,
            "reuse",
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("current Python NomadNet reuse destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-nomadnet-reuse",
            "Python NomadNet Reuse IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 5;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python NomadNet reuse Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity), plans)
                .expect("start current Python NomadNet reuse runtime");
        });
        let mut events = runtime
            .subscribe_events()
            .expect("current Python NomadNet reuse events");
        let url = format!("{destination}:/page/reuse.mu");
        let first_started = Instant::now();
        let first = tokio
            .block_on(runtime.fetch_page(&url, None, CancellationToken::new()))
            .expect("first current Python repeated request");
        let first_elapsed = first_started.elapsed();
        assert_eq!(first.markup.as_bytes(), FIRST_PAGE.as_bytes());

        let second_started = Instant::now();
        let second = tokio
            .block_on(runtime.fetch_page(&url, None, CancellationToken::new()))
            .expect("second current Python repeated request");
        let second_elapsed = second_started.elapsed();
        assert_eq!(second.markup.as_bytes(), SECOND_PAGE.as_bytes());

        let mut retained_events = 0usize;
        while let Ok(event) = events.try_recv() {
            if matches!(
                event,
                RuntimeBusEvent::Debug(line)
                    if line.contains("retained successful NomadNet page link")
                        && line.contains("path=/page/reuse.mu")
            ) {
                retained_events += 1;
            }
        }
        assert_eq!(retained_events, 2);

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["scenario"], "reuse");
        assert_eq!(result["served_page_requests"], 2);
        assert_eq!(result["reuse_request_count"], 2);
        assert_eq!(result["reuse_same_link"], true);
        eprintln!(
            "current Python NomadNet repeated requests reused one link: first_ms={} second_ms={}",
            first_elapsed.as_millis(),
            second_elapsed.as_millis()
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python NomadNet direct/request-Resource comparative measurement"]
    fn current_python_nomadnet_measures_direct_and_request_resource_on_one_link() {
        fn percentile_micros(samples: &mut [u128], percentile: usize) -> u128 {
            assert!(!samples.is_empty());
            samples.sort_unstable();
            let index = (samples.len() * percentile).div_ceil(100) - 1;
            samples[index]
        }

        fn debug_assertions_enabled() -> bool {
            cfg!(debug_assertions)
        }

        let optimized_profile_required =
            std::env::var_os("OMEN_REQUIRE_OPTIMIZED_NOMADNET_MEASUREMENT").is_some();
        if optimized_profile_required {
            assert!(
                !debug_assertions_enabled(),
                "optimized NomadNet measurement must run under cargo test --release"
            );
        }
        let build_profile = if debug_assertions_enabled() {
            "debug"
        } else {
            "release"
        };

        let paths = temp_paths("current-python-nomadnet-performance");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let identity = manager
            .create_managed_identity_with_provider("Native", &NativeReticulumIdentityProvider)
            .expect("create current Python NomadNet performance identity");
        let port = current_python_propagation_port();
        let peer = PythonNomadNetPeer::spawn_scenario(
            &paths.root.join("python-nomadnet-performance"),
            port,
            "performance",
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("current Python NomadNet performance destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-nomadnet-performance",
            "Python NomadNet Performance IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 5;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python NomadNet performance Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity), plans)
                .expect("start current Python NomadNet performance runtime");
        });
        let url = format!("{destination}:/page/measure.mu");
        let mut request_resource_order = vec![false, true];
        for pair in 0..8 {
            if pair % 2 == 0 {
                request_resource_order.extend([false, true]);
            } else {
                request_resource_order.extend([true, false]);
            }
        }
        let mut direct_micros = Vec::with_capacity(8);
        let mut resource_micros = Vec::with_capacity(8);
        for (index, request_resource) in request_resource_order.into_iter().enumerate() {
            let payload = if request_resource {
                "x".repeat(2_048)
            } else {
                "d".into()
            };
            let request_data = BTreeMap::from([("field_payload".to_string(), payload.clone())]);
            let started = Instant::now();
            let page = tokio
                .block_on(runtime.fetch_page(&url, Some(request_data), CancellationToken::new()))
                .expect("current Python primitive measurement fetch");
            let elapsed_micros = started.elapsed().as_micros();
            let expected = format!(
                ">Current Python Primitive Measurement\nrequest={}\nfield_size={}\n",
                index + 1,
                payload.len()
            );
            assert_eq!(page.markup.as_bytes(), expected.as_bytes());
            assert_eq!(page.source, PageSource::Network);
            assert_eq!(
                page.metadata
                    .get("native_request_primitive")
                    .and_then(serde_json::Value::as_str),
                Some(if request_resource {
                    "request-resource"
                } else {
                    "direct-request"
                })
            );
            if index >= 2 {
                if request_resource {
                    resource_micros.push(elapsed_micros);
                } else {
                    direct_micros.push(elapsed_micros);
                }
            }
        }
        assert_eq!(direct_micros.len(), 8);
        assert_eq!(resource_micros.len(), 8);
        let direct_median = percentile_micros(&mut direct_micros, 50);
        let direct_p95 = percentile_micros(&mut direct_micros, 95);
        let resource_median = percentile_micros(&mut resource_micros, 50);
        let resource_p95 = percentile_micros(&mut resource_micros, 95);

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["scenario"], "performance");
        assert_eq!(result["served_page_requests"], 18);
        assert_eq!(result["measure_request_count"], 18);
        assert_eq!(result["measure_same_link"], true);
        if let Some(report_path) = std::env::var_os("OMEN_NOMADNET_MEASUREMENT_REPORT") {
            let report = serde_json::json!({
                "direct_median_us": direct_median,
                "direct_p95_us": direct_p95,
                "profile": build_profile,
                "request_resource_median_us": resource_median,
                "request_resource_p95_us": resource_p95,
                "samples_per_primitive": 8,
                "same_link": true,
            });
            std::fs::write(
                report_path,
                serde_json::to_vec_pretty(&report).expect("serialize NomadNet measurement report"),
            )
            .expect("write NomadNet measurement report");
        }
        eprintln!(
            "current Python NomadNet primitive measurement: profile={build_profile} samples_per_primitive=8 direct_median_us={direct_median} direct_p95_us={direct_p95} request_resource_median_us={resource_median} request_resource_p95_us={resource_p95}"
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python NomadNet bounded keepalive/recovery soak"]
    fn current_python_nomadnet_retained_link_keepalive_and_recovery_are_bounded() {
        const REQUESTS_PER_LINK: usize = 16;
        const TOTAL_REQUESTS: usize = REQUESTS_PER_LINK * 2;

        let paths = temp_paths("current-python-nomadnet-soak");
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let identity = manager
            .create_managed_identity_with_provider("Native", &NativeReticulumIdentityProvider)
            .expect("create current Python NomadNet soak identity");
        let port = current_python_propagation_port();
        let peer = PythonNomadNetPeer::spawn_scenario(
            &paths.root.join("python-nomadnet-soak"),
            port,
            "soak",
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("current Python NomadNet soak destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-nomadnet-soak",
            "Python NomadNet Soak IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 5;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python NomadNet soak Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity), plans)
                .expect("start current Python NomadNet soak runtime");
        });
        let mut events = runtime
            .subscribe_events()
            .expect("current Python NomadNet soak events");
        let url = format!("{destination}:/page/soak.mu");
        let started = Instant::now();

        for request_index in 0..REQUESTS_PER_LINK {
            if request_index == REQUESTS_PER_LINK / 2 {
                tokio.block_on(async {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                });
            }
            let payload = if request_index % 2 == 0 {
                "x"
            } else {
                &"x".repeat(2048)
            };
            let page = tokio
                .block_on(runtime.fetch_page(
                    &url,
                    Some(BTreeMap::from([(
                        "field_payload".to_string(),
                        payload.to_string(),
                    )])),
                    CancellationToken::new(),
                ))
                .expect("current Python NomadNet pre-recovery soak request");
            let expected = format!(
                ">Current Python Keepalive Recovery Soak\nrequest={}\ngeneration=1\nfield_size={}\n",
                request_index + 1,
                payload.len()
            );
            assert_eq!(page.markup.as_bytes(), expected.as_bytes());
        }

        let recovery_marker = peer.wait_for_marker("recovery_ready");
        assert_eq!(recovery_marker["requests_before_close"], REQUESTS_PER_LINK);
        let recovery_started = Instant::now();
        for request_index in REQUESTS_PER_LINK..TOTAL_REQUESTS {
            let payload = if request_index % 2 == 0 {
                "x"
            } else {
                &"x".repeat(2048)
            };
            let page = tokio
                .block_on(async {
                    tokio::time::timeout(
                        Duration::from_secs(8),
                        runtime.fetch_page(
                            &url,
                            Some(BTreeMap::from([(
                                "field_payload".to_string(),
                                payload.to_string(),
                            )])),
                            CancellationToken::new(),
                        ),
                    )
                    .await
                })
                .expect("current Python NomadNet recovery exceeded bounded deadline")
                .expect("current Python NomadNet post-recovery soak request");
            let expected = format!(
                ">Current Python Keepalive Recovery Soak\nrequest={}\ngeneration=2\nfield_size={}\n",
                request_index + 1,
                payload.len()
            );
            assert_eq!(page.markup.as_bytes(), expected.as_bytes());
        }
        let recovery_elapsed = recovery_started.elapsed();
        let soak_elapsed = started.elapsed();

        let mut retained_events = 0usize;
        while let Ok(event) = events.try_recv() {
            if matches!(
                event,
                RuntimeBusEvent::Debug(line)
                    if line.contains("retained successful NomadNet page link")
                        && line.contains("path=/page/soak.mu")
            ) {
                retained_events += 1;
            }
        }
        assert_eq!(retained_events, TOTAL_REQUESTS);

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["scenario"], "soak");
        assert_eq!(result["served_page_requests"], TOTAL_REQUESTS);
        assert_eq!(result["soak_first_generation_requests"], REQUESTS_PER_LINK);
        assert_eq!(result["soak_second_generation_requests"], REQUESTS_PER_LINK);
        assert_eq!(result["soak_max_active_links"], 1);
        assert_eq!(result["soak_recovery_performed"], true);
        eprintln!(
            "current Python NomadNet retained-link soak passed: requests={TOTAL_REQUESTS} generations=2 max_active_links=1 recovery_ms={} elapsed_ms={}",
            recovery_elapsed.as_millis(),
            soak_elapsed.as_millis()
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit current-Python LXMF propagation stamp acceptance/rejection test"]
    fn current_python_lxmf_propagation_stamp_boundaries_match_rust() {
        run_python_lxmf_propagation_stamp_matrix(
            "current-python-lxmf-propagation-stamp",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.0",
            "1.1.0",
        );
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit pinned-Python LXMF propagation enqueue/sync/ack interoperability test"]
    fn pinned_python_lxmf_propagation_sync_is_received_and_acknowledged() {
        run_python_lxmf_propagation_interop(
            "pinned-python-lxmf-propagation",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.2.2",
            "0.9.6",
        );
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit pinned-Python LXMF propagation stamp acceptance/rejection test"]
    fn pinned_python_lxmf_propagation_stamp_boundaries_match_rust() {
        run_python_lxmf_propagation_stamp_matrix(
            "pinned-python-lxmf-propagation-stamp",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.2.2",
            "0.9.6",
        );
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit current-Python network-facing stamped propagation test"]
    fn current_python_lxmf_network_propagation_accepts_and_rejects_rust_stamps() {
        run_python_lxmf_stamped_propagation_interop(
            "current-python-network-propagation-stamp",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.0",
            "1.1.0",
        );
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    #[test]
    #[ignore = "explicit pinned-Python network-facing stamped propagation test"]
    fn pinned_python_lxmf_network_propagation_accepts_and_rejects_rust_stamps() {
        run_python_lxmf_stamped_propagation_interop(
            "pinned-python-network-propagation-stamp",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.2.2",
            "0.9.6",
        );
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit current-Python LXMF ticket issue/use/expiry/reuse matrix"]
    fn current_python_lxmf_ticket_issue_use_expiry_and_reuse_match_rust() {
        run_python_lxmf_ticket_matrix(
            "current-python-lxmf-ticket",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.0",
            "1.1.0",
        );
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit pinned-Python LXMF ticket issue/use/expiry/reuse matrix"]
    fn pinned_python_lxmf_ticket_issue_use_expiry_and_reuse_match_rust() {
        run_python_lxmf_ticket_matrix(
            "pinned-python-lxmf-ticket",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.2.2",
            "0.9.6",
        );
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    fn run_python_lxmf_ticket_matrix(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        use rand_core::RngCore as _;

        let paths = temp_paths(case);
        paths.ensure().expect("isolated ticket paths");
        let fixture_root = paths.root.join("python-ticket");
        std::fs::create_dir_all(&fixture_root).expect("create ticket fixture root");

        let mut ticket = [0_u8; 16];
        let mut message_id = [0_u8; 32];
        rand_core::OsRng.fill_bytes(&mut ticket);
        rand_core::OsRng.fill_bytes(&mut message_id);
        let stamp =
            crate::runtime::native_lxmf::codec::ticket_stamp_for_message(&ticket, &message_id)
                .expect("bounded Rust ticket stamp");
        std::fs::write(fixture_root.join("ticket.bin"), ticket)
            .expect("write isolated ticket material");
        std::fs::write(fixture_root.join("message-id.bin"), message_id)
            .expect("write isolated ticket message identifier");
        std::fs::write(fixture_root.join("stamp.bin"), stamp).expect("write isolated ticket stamp");

        let rns_source = std::env::var_os(rns_source_env)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("{rns_source_env} must name a Python RNS source"));
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/server/crates/omen-ifac-tcp/tests/fixtures/python_lxmf_ticket_matrix.py");
        let mut command = Command::new("python3");
        command.arg(script).arg("--rns-source").arg(rns_source);
        if let Some(lxmf_source_env) = lxmf_source_env {
            let lxmf_source = std::env::var_os(lxmf_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
            command.arg("--lxmf-source").arg(lxmf_source);
        }
        let output = command
            .arg("--expected-rns")
            .arg(expected_rns)
            .arg("--expected-lxmf")
            .arg(expected_lxmf)
            .arg("--root")
            .arg(&fixture_root)
            .output()
            .expect("run Python LXMF ticket matrix");
        assert!(
            output.status.success(),
            "Python LXMF ticket matrix failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Python ticket matrix JSON");
        assert_eq!(result["ticket_bytes"], 16);
        assert_eq!(result["rns"], expected_rns);
        assert_eq!(result["lxmf"], expected_lxmf);
        for check in [
            "active_only",
            "default_expiry_window",
            "expired_cleaned",
            "expired_outbound_rejected",
            "expiry_preserved",
            "remembered_for_use",
            "renewed_near_expiry",
            "reused_before_renewal",
            "rust_stamp_accepted",
            "throttled_after_delivery",
            "wrong_ticket_rejected",
        ] {
            assert_eq!(result["checks"][check], true, "ticket check {check}");
        }
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    fn run_python_lxmf_stamped_propagation_interop(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        const ACCEPTED_TITLE: &str = "OMEN Rust stamped propagation accepted";
        const REJECTED_TITLE: &str = "OMEN Rust under-cost propagation rejected";

        let paths = temp_paths(case);
        paths.ensure().expect("isolated stamped propagation paths");
        let _test_tree_cleanup = TestTreeCleanup(paths.root.clone());
        let identity = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        )
        .create_managed_identity_with_provider(
            "Rust stamped propagation sender",
            &NativeReticulumIdentityProvider,
        )
        .expect("isolated Rust stamped sender identity");
        let source = clean_lxmf_delivery_destination_hash_from_identity_path(&identity.path)
            .expect("Rust stamped source destination");
        let port = current_python_propagation_port();
        let peer = PythonStampedPropagationPeer::spawn(
            &paths.root.join("python-stamped-propagation"),
            port,
            &source,
            rns_source_env,
            lxmf_source_env,
            expected_rns,
            expected_lxmf,
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("Python stamped receiver")
            .to_string();
        let propagation = peer.ready["propagation"]
            .as_str()
            .expect("Python stamped propagation destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-stamped-propagation",
            "Python Stamped Propagation IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 12;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("stamped propagation Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity.clone()), plans)
                .expect("start stamped propagation runtime");
            tokio::time::sleep(Duration::from_millis(300)).await;
            for _ in 0..3 {
                assert!(runtime
                    .announce_identity()
                    .await
                    .expect("announce stamped propagation sender"));
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            runtime
                .set_outbound_propagation_node(Some(propagation.clone()))
                .await
                .expect("select stamped Python propagation node");
        });

        let accepted_send = tokio
            .block_on(runtime.send_message(MessageEnvelope {
                peer_hash: destination.clone(),
                title: ACCEPTED_TITLE.into(),
                body: "Python accepted this cost-13 Rust propagation envelope".into(),
                delivery_mode: crate::messaging::DeliveryMode::Propagated,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            }))
            .expect("send accepted stamped propagation message");
        assert_eq!(accepted_send.transport_method, TransportMethod::Propagated);
        let accepted = peer.next("acceptance result");
        assert_eq!(accepted["accepted"], true);
        assert_eq!(accepted["validation"]["messages"], 1);
        assert_eq!(accepted["validation"]["accepted"], 1);
        assert_eq!(accepted["validation"]["target_cost"], 13);
        assert_eq!(accepted["validation"]["active_propagation_links"], 1);
        assert!(accepted["validation"]["stamp_value"]
            .as_u64()
            .is_some_and(|value| value >= 13));
        assert_eq!(accepted["delivery"]["title"], ACCEPTED_TITLE);
        assert_eq!(accepted["delivery"]["source_hash"], source);
        assert_eq!(accepted["delivery"]["destination_hash"], destination);
        assert_eq!(accepted["delivery"]["signature_validated"], true);
        assert_eq!(accepted["delivery"]["method"], 3);
        assert_eq!(accepted["client_messages"], 1);

        let rejected_send = tokio
            .block_on(runtime.send_message(MessageEnvelope {
                peer_hash: destination,
                title: REJECTED_TITLE.into(),
                body: "Python must reject this stale-advertisement under-cost envelope".into(),
                delivery_mode: crate::messaging::DeliveryMode::Propagated,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            }))
            .expect("transport accepts under-cost envelope before remote validation");
        assert_eq!(rejected_send.transport_method, TransportMethod::Propagated);
        let rejected = peer.next("rejection result");
        assert_eq!(rejected["rejected"], true);
        assert_eq!(rejected["validation"]["messages"], 1);
        assert_eq!(rejected["validation"]["accepted"], 0);
        assert_eq!(rejected["validation"]["target_cost"], 255);
        assert_eq!(rejected["validation"]["active_propagation_links"], 1);
        assert_eq!(rejected["delivery_count"], 1);
        assert_eq!(rejected["client_messages"], 1);
        assert_eq!(rejected["rejection_cost"], 255);
        peer.finish();
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    fn run_python_lxmf_propagation_stamp_matrix(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        use sha2::Digest as _;

        let paths = temp_paths(case);
        paths.ensure().expect("isolated propagation stamp paths");
        let fixture_root = paths.root.join("python-stamp");
        std::fs::create_dir_all(&fixture_root).expect("create propagation stamp fixture root");

        let mut lxm_data = vec![0x33; 180];
        lxm_data[0] = 0x92;
        let digest = sha2::Sha256::digest(&lxm_data);
        let mut transient_id = [0u8; 32];
        transient_id.copy_from_slice(&digest);
        let stamp = crate::runtime::native_lxmf::codec::generate_propagation_stamp_for_transient(
            &lxm_data,
            transient_id,
            2,
            4_096,
        )
        .expect("bounded Rust propagation stamp");
        assert!(stamp.stamp_value < 255);
        std::fs::write(fixture_root.join("lxm-data.bin"), &lxm_data)
            .expect("write isolated propagation transient");
        std::fs::write(fixture_root.join("stamp.bin"), &stamp.stamp)
            .expect("write isolated propagation stamp");

        let rns_source = std::env::var_os(rns_source_env)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("{rns_source_env} must name a Python RNS source"));
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "src/server/crates/omen-ifac-tcp/tests/fixtures/python_lxmf_propagation_stamp_matrix.py",
        );
        let mut command = Command::new("python3");
        command.arg(script).arg("--rns-source").arg(rns_source);
        if let Some(lxmf_source_env) = lxmf_source_env {
            let lxmf_source = std::env::var_os(lxmf_source_env)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("{lxmf_source_env} must name a Python LXMF source"));
            command.arg("--lxmf-source").arg(lxmf_source);
        }
        let output = command
            .arg("--expected-rns")
            .arg(expected_rns)
            .arg("--expected-lxmf")
            .arg(expected_lxmf)
            .arg("--root")
            .arg(&fixture_root)
            .arg("--stamp-value")
            .arg(stamp.stamp_value.to_string())
            .output()
            .expect("run Python LXMF propagation stamp matrix");
        assert!(
            output.status.success(),
            "Python LXMF propagation stamp matrix failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("Python propagation stamp JSON");
        assert_eq!(result["accepted_at_value"], stamp.stamp_value);
        assert_eq!(result["rejected_at_value"], stamp.stamp_value + 1);
        assert_eq!(result["transient_bytes"], lxm_data.len());
        assert_eq!(result["rns"], expected_rns);
        assert_eq!(result["lxmf"], expected_lxmf);
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(not(all(feature = "native-rns-net", any())))]
    fn run_python_lxmf_propagation_interop(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        let paths = temp_paths(case);
        paths.ensure().expect("isolated Python propagation paths");
        let identity = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        )
        .create_managed_identity_with_provider(
            "Python propagated receiver",
            &NativeReticulumIdentityProvider,
        )
        .expect("isolated Rust propagation identity");
        let destination = clean_lxmf_delivery_destination_hash_from_identity_path(&identity.path)
            .expect("Rust propagation delivery destination");
        let port = current_python_propagation_port();
        let peer = PythonPropagationPeer::spawn(
            &paths.root.join("python-propagation"),
            port,
            &destination,
            rns_source_env,
            lxmf_source_env,
            expected_rns,
            expected_lxmf,
        );
        let propagation = peer.ready["propagation"]
            .as_str()
            .expect("current Python propagation destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "current-python-propagation",
            "Current Python Propagation IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 12;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("current Python propagation Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity.clone()), plans)
                .expect("start isolated Rust propagation runtime");
            tokio::time::sleep(Duration::from_millis(300)).await;
            for _ in 0..3 {
                assert!(runtime
                    .announce_identity()
                    .await
                    .expect("announce Rust propagation receiver"));
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        });
        let queued = peer.wait_for_queued();

        let messages = tokio.block_on(async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            runtime
                .set_outbound_propagation_node(Some(propagation.clone()))
                .await
                .expect("select current Python propagation node");
            runtime
                .sync_propagation_messages(Some(1))
                .await
                .expect("sync current Python propagated transient");
            runtime
                .list_messages()
                .await
                .expect("drain propagated message")
        });

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].title, queued["title"]);
        assert_eq!(messages[0].content, queued["content"]);
        assert_eq!(messages[0].peer_hash, queued["source_hash"]);
        assert_eq!(
            messages[0].transport_method,
            crate::messaging::TransportMethod::Propagated
        );
        assert!(messages[0].incoming);
        assert_eq!(
            messages[0].fields.get("native_lxmf_delivery_source"),
            Some(&"propagation_sync".to_string())
        );

        let result = peer.finish();
        assert_eq!(result["acknowledged"], true);
        assert_eq!(result["remaining"], 0);
        assert_eq!(result["transient_id"], queued["transient_id"]);
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_propagation_sync_parses_python_get_responses() {
        let id_a = [1u8; 32];
        let id_b = [2u8; 32];
        let list = rmpv::Value::Array(vec![
            rmpv::Value::Binary(id_a.to_vec()),
            rmpv::Value::Binary(id_b.to_vec()),
        ]);
        let mut list_bytes = Vec::new();
        rmpv::encode::write_value(&mut list_bytes, &list).expect("pack list");
        let payloads = rmpv::Value::Array(vec![
            rmpv::Value::Binary(b"lxmf-a".to_vec()),
            rmpv::Value::Binary(b"lxmf-b".to_vec()),
        ]);
        let mut payload_bytes = Vec::new();
        rmpv::encode::write_value(&mut payload_bytes, &payloads).expect("pack payloads");

        assert_eq!(
            native_lxmf_parse_transient_id_list(list_bytes.as_slice()).expect("ids"),
            vec![id_a, id_b]
        );
        assert_eq!(
            native_lxmf_parse_propagation_payloads(payload_bytes.as_slice()).expect("payloads"),
            vec![b"lxmf-a".to_vec(), b"lxmf-b".to_vec()]
        );
    }

    #[cfg(feature = "native-lxmf")]
    #[test]
    fn native_lxmf_adapter_propagation_decoders_enforce_shape_budgets() {
        let id = [0x42u8; 32];
        let mut valid_ids = Vec::new();
        rmpv::encode::write_value(
            &mut valid_ids,
            &rmpv::Value::Array(vec![rmpv::Value::Binary(id.to_vec())]),
        )
        .expect("pack ids");
        assert_eq!(
            native_lxmf_parse_transient_id_list(&valid_ids).expect("ids"),
            vec![id]
        );
        let mut trailing = valid_ids;
        trailing.push(0xc0);
        assert!(native_lxmf_parse_transient_id_list(&trailing).is_err());

        let id_scalar_too_wide = [0xc4, 33];
        assert!(native_lxmf_parse_transient_id_list(&id_scalar_too_wide).is_err());
        let list_too_wide = [0xdd, 0x00, 0x00, 0x10, 0x01];
        assert!(native_lxmf_parse_transient_id_list(&list_too_wide).is_err());

        let payload_scalar_too_wide = [0xc6, 0x00, 0x80, 0x00, 0x01];
        assert!(native_lxmf_parse_propagation_payloads(&payload_scalar_too_wide).is_err());
        let mut deep = vec![0x91; 6];
        deep.push(0xc0);
        assert!(native_lxmf_parse_propagation_payloads(&deep).is_err());
    }

    #[cfg(feature = "native-lxmf")]
    #[test]
    fn native_lxmf_payload_candidates_move_raw_fallback_and_expand_envelopes() {
        let raw = vec![0x55; 1024 * 1024];
        let raw_ptr = raw.as_ptr();
        let candidates = native_lxmf_payload_candidates(raw);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].as_ptr(), raw_ptr);

        let envelope =
            lxmf::WireMessage::pack_propagation_envelope(42.0, b"lxmf-data", Some(&[0xAB; 32]))
                .expect("propagation envelope");
        let mut stamped = b"lxmf-data".to_vec();
        stamped.extend_from_slice(&[0xAB; 32]);
        assert_eq!(native_lxmf_payload_candidates(envelope), vec![stamped]);
    }

    #[cfg(feature = "native-lxmf")]
    #[test]
    fn propagation_stamp_worker_returns_the_original_buffer() {
        let lxm_data = vec![0x42; 1024 * 1024];
        let data_ptr = lxm_data.as_ptr();

        let (stamp, returned) = generate_propagation_stamp_owned(lxm_data, [0x11; 32], 0);

        assert!(stamp.is_ok());
        assert_eq!(returned.as_ptr(), data_ptr);
        assert_eq!(PROPAGATION_STAMP_BLOCKING_JOBS, 2);
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    fn clean_direct_submit_payload_borrows_signed_wire_allocation() {
        let delivery = NativeLxmfSdkWireDelivery {
            wire_bytes: vec![0x77; 1024 * 1024],
            message_id: "message-id".into(),
            destination_hash: "00112233445566778899aabbccddeeff".into(),
            method: Some("direct".into()),
            include_ticket: false,
            reply_ticket_used: false,
            direct_stamp: None,
        };
        let wire_ptr = delivery.wire_bytes.as_ptr();

        let payload = clean_lxmf_direct_payload(&delivery);

        assert!(matches!(payload, Cow::Borrowed(_)));
        assert_eq!(payload.as_ptr(), wire_ptr);
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    fn clean_direct_stamp_policy_enforces_ceiling_and_ticket_precedence() {
        fn app_data(cost: rmpv::Value) -> Vec<u8> {
            let mut bytes = Vec::new();
            rmpv::encode::write_value(
                &mut bytes,
                &rmpv::Value::Array(vec![rmpv::Value::Binary(b"Peer".to_vec()), cost]),
            )
            .expect("announce app data");
            bytes
        }

        let admitted = app_data(rmpv::Value::from(
            crate::runtime::native_lxmf::codec::CLEAN_DIRECT_STAMP_MAX_COST,
        ));
        assert_eq!(
            clean_direct_stamp_cost(Some(&admitted), None, 1_000.0).expect("admitted cost"),
            Some(crate::runtime::native_lxmf::codec::CLEAN_DIRECT_STAMP_MAX_COST)
        );
        let over = app_data(rmpv::Value::from(
            crate::runtime::native_lxmf::codec::CLEAN_DIRECT_STAMP_MAX_COST + 1,
        ));
        assert!(clean_direct_stamp_cost(Some(&over), None, 1_000.0)
            .expect_err("cost over ceiling")
            .to_string()
            .contains("safety ceiling"));
        let unsupported = app_data(rmpv::Value::from(0_u8));
        assert!(clean_direct_stamp_cost(Some(&unsupported), None, 1_000.0).is_err());

        let ticket = crate::messaging::NativeLxmfReplyTicket {
            ticket: vec![0x44; 16],
            expires: 2_000.0,
        };
        assert_eq!(
            clean_direct_stamp_cost(Some(&over), Some(&ticket), 1_000.0)
                .expect("ticket precedence"),
            None
        );
        let expired = crate::messaging::NativeLxmfReplyTicket {
            expires: 999.0,
            ..ticket
        };
        assert!(clean_direct_stamp_cost(Some(&admitted), Some(&expired), 1_000.0).is_err());
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_direct_policy_wait_is_event_driven_bounded_and_cancellable() {
        fn announce(destination_hash: &str) -> RuntimeBusEvent {
            RuntimeBusEvent::Announce(AnnouncePayload {
                destination_hash: destination_hash.into(),
                identity_hash: None,
                display_name: "Peer".into(),
                kind: DirectoryKind::Peer,
                associated_hash: None,
                node_associated_hash: None,
                has_ratchet: false,
                lxmf_stamp_cost: Some(1),
            })
        }

        const DESTINATION: &str = "00112233445566778899aabbccddeeff";
        let cache = Arc::new(Mutex::new(BTreeMap::from([(
            DESTINATION.into(),
            Vec::new(),
        )])));
        let (_events, mut receiver) = broadcast::channel(4);
        let shutdown = tokio_util::sync::CancellationToken::new();
        assert_eq!(
            clean_wait_for_direct_policy_announce(
                &cache,
                DESTINATION,
                &mut receiver,
                Duration::from_secs(1),
                &shutdown,
            )
            .await
            .expect("cached empty authenticated policy"),
            Some(Vec::new())
        );

        let cache = Arc::new(Mutex::new(BTreeMap::new()));
        let (events, mut receiver) = broadcast::channel(4);
        let wait_cache = cache.clone();
        let wait_shutdown = tokio_util::sync::CancellationToken::new();
        let waiter_shutdown = wait_shutdown.clone();
        let waiter = tokio::spawn(async move {
            clean_wait_for_direct_policy_announce(
                &wait_cache,
                DESTINATION,
                &mut receiver,
                Duration::from_secs(1),
                &waiter_shutdown,
            )
            .await
        });
        events
            .send(announce("ffeeddccbbaa99887766554433221100"))
            .expect("unrelated announce");
        let policy = vec![0x92, 0xc4, 0x04, b'P', b'e', b'e', b'r', 0x01];
        assert!(insert_bounded_destination_app_data(
            &mut cache.lock().expect("direct policy test cache lock"),
            DESTINATION.into(),
            policy.clone(),
        ));
        events
            .send(announce(DESTINATION))
            .expect("matching announce");
        assert_eq!(
            waiter
                .await
                .expect("policy waiter join")
                .expect("policy wait"),
            Some(policy)
        );

        let cache = Arc::new(Mutex::new(BTreeMap::new()));
        let (events, mut receiver) = broadcast::channel(2);
        let shutdown = tokio_util::sync::CancellationToken::new();
        events
            .send(announce(DESTINATION))
            .expect("over-limit announce evidence");
        assert!(clean_wait_for_direct_policy_announce(
            &cache,
            DESTINATION,
            &mut receiver,
            Duration::from_secs(1),
            &shutdown,
        )
        .await
        .expect_err("matching announce without admitted policy must fail")
        .to_string()
        .contains("admission limits"));

        let (_events, mut receiver) = broadcast::channel(2);
        let shutdown = tokio_util::sync::CancellationToken::new();
        shutdown.cancel();
        assert!(clean_wait_for_direct_policy_announce(
            &cache,
            DESTINATION,
            &mut receiver,
            Duration::from_secs(1),
            &shutdown,
        )
        .await
        .expect_err("shutdown must cancel policy wait")
        .to_string()
        .contains("cancelled"));
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit current-Python live first-send direct-stamp policy discovery test"]
    fn current_python_lxmf_first_direct_send_discovers_stamp_policy_before_encoding() {
        run_python_lxmf_first_direct_send_policy_discovery(
            "current-python-first-direct-policy",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.0",
            "1.1.0",
        );
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit pinned-Python live first-send direct-stamp policy discovery test"]
    fn pinned_python_lxmf_first_direct_send_discovers_stamp_policy_before_encoding() {
        run_python_lxmf_first_direct_send_policy_discovery(
            "pinned-python-first-direct-policy",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.2.2",
            "0.9.6",
        );
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    fn run_python_lxmf_first_direct_send_policy_discovery(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        const STAMPED_TITLE: &str = "OMEN Rust stamped direct LXMF";
        const UNSTAMPED_TITLE: &str = "OMEN Rust unstamped direct LXMF";

        let paths = temp_paths(case);
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let identity = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create direct-policy identity");
        let source = clean_lxmf_delivery_destination_hash_from_identity_path(&identity.path)
            .expect("direct-policy source hash");
        let port = current_python_propagation_port();
        let peer = PythonDirectStampPeer::spawn(
            &paths.root.join("python-direct-stamp"),
            port,
            &source,
            PythonDirectStampPeerConfig {
                fixture: "python_lxmf_direct_stamp_peer.py",
                rns_source_env,
                lxmf_source_env,
                expected_rns,
                expected_lxmf,
            },
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("Python direct-stamp destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-direct-stamp",
            "Python Direct Stamp IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 12;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("direct-policy Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity.clone()), plans)
                .expect("start direct-policy runtime");
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(runtime
                .announce_identity()
                .await
                .expect("announce direct-policy sender"));
        });
        peer.wait_for_source_announce();

        runtime
            .clean_destination_app_data
            .lock()
            .expect("clear direct-policy cache lock")
            .remove(&destination);
        let started = Instant::now();
        let stamped = tokio
            .block_on(runtime.send_message(MessageEnvelope {
                peer_hash: destination.clone(),
                title: STAMPED_TITLE.into(),
                body: "First send must discover the authenticated policy before encoding".into(),
                delivery_mode: crate::messaging::DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            }))
            .expect("first direct send discovers and applies stamp policy");
        assert!(matches!(
            stamped
                .fields
                .get("native_lxmf_direct_stamp_policy_source")
                .map(String::as_str),
            Some("discovered_authenticated_announce" | "refreshed_authenticated_announce")
        ));
        assert_eq!(
            stamped
                .fields
                .get("native_lxmf_direct_stamp_cost")
                .map(String::as_str),
            Some("1")
        );

        let handle = runtime
            .active_transport()
            .expect("active direct-policy transport");
        let identity_bytes = crate::identity::read_identity_material(&identity.path)
            .expect("read direct-policy identity");
        let unstamped_envelope = MessageEnvelope {
            peer_hash: destination.clone(),
            title: UNSTAMPED_TITLE.into(),
            body: "Python must reject this unstamped control".into(),
            delivery_mode: crate::messaging::DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let unstamped = build_sdk_wire_delivery_from_envelope_with_issued_ticket(
            &unstamped_envelope,
            &source,
            identity_bytes.as_slice(),
            Some(1),
            None,
        )
        .expect("build unstamped control");
        assert!(unstamped.direct_stamp.is_none());
        let submitter = {
            let _runtime_context = tokio.enter();
            CleanReticulumLxmfWireSubmitter::new(
                handle.transport.clone(),
                handle.storage_path.clone(),
                CleanLxmfSubmitterState {
                    event_tx: runtime.event_tx.clone(),
                    outbound_propagation_node: runtime.outbound_propagation_node.clone(),
                    destination_identities: runtime.clean_destination_identities.clone(),
                    destination_app_data: runtime.clean_destination_app_data.clone(),
                    pending_lxmf_proofs: runtime.pending_lxmf_proofs.clone(),
                },
                Duration::from_secs(12),
            )
            .expect("direct-policy control submitter")
        };
        tokio
            .block_on(submitter.submit_wire_async(&unstamped))
            .expect("submit unstamped control");

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["received_count"], 1);
        assert_eq!(result["stamped_accepted"], true);
        assert_eq!(result["unstamped_rejected"], true);
        eprintln!(
            "first-send direct policy discovery interoperated: elapsed_ms={}",
            started.elapsed().as_millis()
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit current-Python live stamped direct Resource test"]
    fn current_python_lxmf_stamped_direct_resource_preserves_bytes_and_reports_progress() {
        run_python_lxmf_stamped_direct_resource(
            "current-python-direct-resource",
            "OMEN_PYTHON_RNS_SOURCE",
            None,
            "1.4.0",
            "1.1.0",
        );
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "explicit pinned-Python live stamped direct Resource test"]
    fn pinned_python_lxmf_stamped_direct_resource_preserves_bytes_and_reports_progress() {
        run_python_lxmf_stamped_direct_resource(
            "pinned-python-direct-resource",
            "OMEN_PINNED_RNS_SOURCE",
            Some("OMEN_PINNED_LXMF_SOURCE"),
            "1.2.2",
            "0.9.6",
        );
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    fn run_python_lxmf_stamped_direct_resource(
        case: &str,
        rns_source_env: &str,
        lxmf_source_env: Option<&str>,
        expected_rns: &str,
        expected_lxmf: &str,
    ) {
        const RESOURCE_TITLE: &str = "OMEN Rust stamped Resource LXMF";
        const RESOURCE_BODY_BYTES: usize = 64 * 1024;

        let paths = temp_paths(case);
        let manager = IdentityManager::new(
            paths.identities_dir.clone(),
            paths.identity_backups_dir.clone(),
        );
        let provider = NativeReticulumIdentityProvider;
        let identity = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create direct-Resource identity");
        let source = clean_lxmf_delivery_destination_hash_from_identity_path(&identity.path)
            .expect("direct-Resource source hash");
        let port = current_python_propagation_port();
        let peer = PythonDirectStampPeer::spawn(
            &paths.root.join("python-direct-resource"),
            port,
            &source,
            PythonDirectStampPeerConfig {
                fixture: "python_lxmf_direct_stamp_resource_peer.py",
                rns_source_env,
                lxmf_source_env,
                expected_rns,
                expected_lxmf,
            },
        );
        let destination = peer.ready["destination"]
            .as_str()
            .expect("Python direct-Resource destination")
            .to_string();

        let mut profile = crate::interfaces::ReticulumInterfaceProfile::tcp_client(
            "python-direct-resource",
            "Python Direct Resource IFAC",
        );
        profile.target_host = "127.0.0.1".into();
        profile.target_port = port;
        profile.network_name = "omen-ifac-vector".into();
        profile.passphrase = "public-test-fixture".into();
        let plans = plan_interfaces(&[profile]);
        let mut config = NativeRuntimeConfig::from_paths(&paths);
        config.identity_path = Some(identity.path.clone());
        config.request_timeout_secs = 12;
        let runtime = NativeNetworkRuntime::new(config);
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("direct-Resource Tokio runtime");

        tokio.block_on(async {
            runtime
                .start(Some(identity.clone()), plans)
                .expect("start direct-Resource runtime");
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(runtime
                .announce_identity()
                .await
                .expect("announce direct-Resource sender"));
        });
        peer.wait_for_source_announce();

        let mut events = runtime
            .subscribe_events()
            .expect("direct-Resource runtime event stream");
        let started = Instant::now();
        let message = tokio
            .block_on(runtime.send_message(MessageEnvelope {
                peer_hash: destination,
                title: RESOURCE_TITLE.into(),
                body: "R".repeat(RESOURCE_BODY_BYTES),
                delivery_mode: crate::messaging::DeliveryMode::Direct,
                include_ticket: false,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            }))
            .expect("send stamped direct Resource");
        let message_id = message
            .message_id
            .as_deref()
            .expect("direct-Resource message id");
        let resource_hash = message
            .fields
            .get("native_lxmf_resource_hash")
            .expect("Resource-sized wire uses Resource");
        assert_eq!(
            message
                .fields
                .get("native_lxmf_proof_state")
                .map(String::as_str),
            Some("waiting_for_resource_completion")
        );
        assert_eq!(
            message
                .fields
                .get("native_lxmf_direct_stamp_cost")
                .map(String::as_str),
            Some("1")
        );

        let (saw_progress, saw_complete) = tokio.block_on(async {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            let mut saw_progress = false;
            let mut saw_complete = false;
            while !saw_complete {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                assert!(!remaining.is_zero(), "direct-Resource lifecycle timed out");
                match tokio::time::timeout(remaining, events.recv())
                    .await
                    .expect("direct-Resource event timeout")
                    .expect("direct-Resource event stream")
                {
                    RuntimeBusEvent::ResourceProgress(progress)
                        if progress.transfer_id == *resource_hash
                            && progress.operation_id.as_deref() == Some(message_id) =>
                    {
                        assert!(progress.received <= progress.total.unwrap_or(u64::MAX));
                        assert_eq!(progress.source.as_deref(), Some("lxmf"));
                        assert_eq!(progress.purpose.as_deref(), Some("direct-message"));
                        assert_eq!(progress.direction.as_deref(), Some("outbound"));
                        saw_progress = true;
                    }
                    RuntimeBusEvent::ResourceLifecycle(lifecycle)
                        if lifecycle.transfer_id == *resource_hash =>
                    {
                        assert_eq!(lifecycle.source.as_deref(), Some("lxmf"));
                        assert_eq!(lifecycle.purpose.as_deref(), Some("direct-message"));
                        match lifecycle.state {
                            ResourceLifecycleState::Offered => {
                                assert!(lifecycle.bytes.is_some_and(|bytes| bytes > 0));
                            }
                            ResourceLifecycleState::Complete => saw_complete = true,
                            state => panic!("unexpected direct-Resource lifecycle: {state:?}"),
                        }
                    }
                    _ => {}
                }
            }
            (saw_progress, saw_complete)
        });
        assert!(saw_progress, "direct Resource must expose bounded progress");
        assert!(saw_complete);

        let result = peer.finish();
        assert_eq!(result["passed"], true);
        assert_eq!(result["body_bytes"], RESOURCE_BODY_BYTES);
        assert_eq!(result["body_sha256_match"], true);
        assert_eq!(result["signature_validated"], true);
        assert_eq!(result["stamp_valid"], true);
        eprintln!(
            "stamped direct Resource interoperated: bytes={RESOURCE_BODY_BYTES} elapsed_ms={}",
            started.elapsed().as_millis()
        );
        runtime.stop();
        let _ = std::fs::remove_dir_all(paths.root);
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn clean_direct_stamp_gate_caps_concurrency_and_releases_permits() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let active = active.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = DIRECT_STAMP_BLOCKING_GATE
                    .acquire()
                    .await
                    .expect("direct stamp gate");
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.expect("direct stamp gate task");
        }

        assert_eq!(peak.load(Ordering::SeqCst), DIRECT_STAMP_BLOCKING_JOBS);
        assert_eq!(
            DIRECT_STAMP_BLOCKING_GATE.available_permits(),
            DIRECT_STAMP_BLOCKING_JOBS
        );
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_propagation_sync_selects_wants_and_cached_haves() {
        let id_a = [1u8; 32];
        let id_b = [2u8; 32];
        let id_c = [3u8; 32];
        let mut delivered = BTreeMap::new();
        DeliveredTransientIdStore::mark_delivered(&mut delivered, &id_b, 10.0);

        let (wants, haves) =
            native_lxmf_select_sync_ids(vec![id_a, id_b, id_c], &delivered, Some(1));

        assert_eq!(wants, vec![id_a]);
        assert_eq!(haves, vec![id_b]);
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_transient_id_hashes_wire_payload() {
        let payload = b"lxmf-propagated-wire-payload";
        let transient_id = native_lxmf_transient_id(payload);
        let expected = Sha256::digest(payload);

        assert_eq!(transient_id.as_slice(), expected.as_slice());
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_submitted_status_is_not_marked_delivered() {
        let status = native_lxmf_submitted_status(
            "00112233445566778899aabbccddeeff",
            "aabbccddeeff00112233445566778899",
            123.456,
        );

        assert_eq!(status.peer_hash, "00112233445566778899aabbccddeeff");
        assert_eq!(
            status.message_id.as_deref(),
            Some("aabbccddeeff00112233445566778899")
        );
        assert!(!status.delivered);
        assert!(!status.failed);
        assert_eq!(
            status.state,
            crate::runtime::network::OutboundDeliveryState::SubmittedToRnsNet
        );
        assert_eq!(
            status.evidence.as_deref(),
            Some("packet_hash:aabbccddeeff00112233445566778899;submitted_at:123.456")
        );
        assert_eq!(status.rtt, None);
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn native_lxmf_pending_direct_summary_reports_count_and_age() {
        let pending: PendingLxmfProofs = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        assert_eq!(
            native_lxmf_pending_direct_summary(&pending),
            "pending_lxmf_direct=0"
        );

        pending.lock().expect("pending lock").insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            native_unix_timestamp() - 5.0,
            None,
        );
        let message = MessageSummary {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            peer_label: "Peer".into(),
            title: "Reply".into(),
            content: "Body".into(),
            timestamp: 124.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("inbound-a".into()),
            fields: BTreeMap::new(),
            attachments: Vec::new(),
        };
        let destination_keys = Arc::new(Mutex::new(RnsNetDestinationKeyStore::default()));
        let _ = native_lxmf_inbound_peer_evidence(
            &message,
            &pending,
            &destination_keys,
            "peer activity",
        );

        let summary = native_lxmf_pending_direct_summary(&pending);
        assert!(summary.contains("pending_lxmf_direct=1"));
        assert!(summary.contains("peer_activity_observed_for_pending_direct=1"));
        assert!(summary.contains("oldest_pending_lxmf_direct_age_secs="));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn native_lxmf_direct_router_timeout_emits_no_receipt_evidence_once() {
        let pending: PendingLxmfProofs = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        pending.lock().expect("pending").insert_submission(
            "packet-a".into(),
            "00112233445566778899aabbccddeeff".into(),
            10.0,
            Some("fedcba98765432100123456789abcdef".into()),
        );
        let (tx, mut rx) = broadcast::channel(8);

        let events = native_lxmf_reconcile_direct_router_timeouts(&pending, 60.0, 45.0);
        assert_eq!(events.len(), 1);
        emit_native_lxmf_direct_timeout_event(&tx, events[0].clone());
        assert!(native_lxmf_reconcile_direct_router_timeouts(&pending, 61.0, 45.0).is_empty());

        let evidence = rx.try_recv().expect("evidence event");
        let debug = rx.try_recv().expect("debug event");
        match evidence {
            RuntimeBusEvent::LxmfDeliveryEvidence(evidence) => {
                assert_eq!(evidence.peer_hash, "00112233445566778899aabbccddeeff");
                assert_eq!(evidence.message_id.as_deref(), Some("packet-a"));
                assert_eq!(evidence.kind, LxmfDeliveryEvidenceKind::NoReceiptObserved);
                assert!(evidence
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("fallback_ready:true")));
                assert!(evidence
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("packet_hash:packet-a")));
                assert!(evidence
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("peer_activity_observed:false")));
            }
            other => panic!("unexpected event: {other:?}"),
        }
        match debug {
            RuntimeBusEvent::Debug(line) => {
                assert!(line.contains("native LXMF direct proof timeout"));
                assert!(line.contains("propagation_fallback_node=fedcba98765432100123456789abcdef"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn native_packet_proof_uses_pending_lxmf_peer_mapping() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        pending.lock().expect("pending").insert_submission(
            hex_encode(&[10u8; 32]),
            "00112233445566778899aabbccddeeff".into(),
            123.456,
            None,
        );
        let proof = RnsNetProof {
            destination_hash: [9u8; 16],
            packet_hash: [10u8; 32],
            rtt: 0.125,
        };

        let (status, matched) = native_lxmf_proof_status_for_packet(&proof, &pending);

        assert!(matched);
        assert_eq!(status.peer_hash, "00112233445566778899aabbccddeeff");
        assert_eq!(
            status.message_id.as_deref(),
            Some("0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a")
        );
        assert!(!status.delivered);
        assert!(!status.failed);
        assert_eq!(status.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert_eq!(status.evidence.as_deref(), Some("rns_packet_proof"));
        assert_eq!(status.rtt, Some(0.125));
        assert_eq!(pending.lock().expect("pending").pending_len(), 1);
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn unmatched_native_packet_proof_falls_back_to_destination_hash() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let proof = RnsNetProof {
            destination_hash: [9u8; 16],
            packet_hash: [10u8; 32],
            rtt: 0.125,
        };

        let (status, matched) = native_lxmf_proof_status_for_packet(&proof, &pending);

        assert!(!matched);
        assert_eq!(status.peer_hash, "09090909090909090909090909090909");
        assert_eq!(status.evidence.as_deref(), Some("rns_packet_proof"));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn unmatched_native_packet_proof_emits_no_runtime_events() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let proof = RnsNetProof {
            destination_hash: [9u8; 16],
            packet_hash: [10u8; 32],
            rtt: 0.125,
        };

        let events = native_lxmf_events_for_packet_proof(&proof, &pending);

        assert!(events.is_empty());
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn matched_native_packet_proof_emits_lxmf_delivery_events() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        pending.lock().expect("pending").insert_submission(
            hex_encode(&[10u8; 32]),
            "00112233445566778899aabbccddeeff".into(),
            123.456,
            None,
        );
        let proof = RnsNetProof {
            destination_hash: [9u8; 16],
            packet_hash: [10u8; 32],
            rtt: 0.125,
        };

        let events = native_lxmf_events_for_packet_proof(&proof, &pending);

        assert_eq!(events.len(), 3);
        assert!(matches!(
            events[0],
            RuntimeBusEvent::MessageDeliveryUpdated(_)
        ));
        assert!(matches!(
            events[1],
            RuntimeBusEvent::LxmfDeliveryEvidence(_)
        ));
        assert!(matches!(events[2], RuntimeBusEvent::Debug(_)));
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn inbound_peer_message_evidence_matches_pending_direct_outbound() {
        let peer_hash = "00112233445566778899aabbccddeeff";
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        pending.lock().expect("pending").insert_submission(
            "packet-a".into(),
            peer_hash.into(),
            123.456,
            None,
        );
        pending.lock().expect("pending").insert_submission(
            "packet-b".into(),
            "ffffffffffffffffffffffffffffffff".into(),
            123.456,
            None,
        );
        let message = MessageSummary {
            peer_hash: peer_hash.into(),
            peer_label: "Peer".into(),
            title: "Reply".into(),
            content: "Body".into(),
            timestamp: 124.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("inbound-a".into()),
            fields: BTreeMap::new(),
            attachments: Vec::new(),
        };

        let destination_keys = Arc::new(Mutex::new(RnsNetDestinationKeyStore::default()));
        let evidence = native_lxmf_inbound_peer_evidence(
            &message,
            &pending,
            &destination_keys,
            "peer activity",
        );

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].peer_hash, peer_hash);
        assert_eq!(evidence[0].message_id.as_deref(), Some("packet-a"));
        assert_eq!(
            evidence[0].kind,
            LxmfDeliveryEvidenceKind::InboundPeerMessage
        );
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("peer activity;packet_hash:packet-a")));
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("peer_activity_observed:true")));
        assert_eq!(pending.lock().expect("pending").pending_len(), 2);
        assert!(pending
            .lock()
            .expect("pending")
            .pending("packet-a")
            .and_then(|pending| pending.peer_activity_observed_at)
            .is_some());
        assert!(pending
            .lock()
            .expect("pending")
            .pending("packet-b")
            .and_then(|pending| pending.peer_activity_observed_at)
            .is_none());
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn inbound_peer_message_evidence_matches_known_sibling_destination() {
        let identity_hash = [7u8; 16];
        let observed_node_hash = rns_net_destination_hash(&identity_hash, "nomadnetwork", "node");
        let delivery_hash = rns_net_destination_hash(&identity_hash, "lxmf", "delivery");
        let delivery_hex = hex_encode(&delivery_hash);
        let observed_hex = hex_encode(&observed_node_hash);
        let destination_keys = Arc::new(Mutex::new(RnsNetDestinationKeyStore::default()));
        destination_keys
            .lock()
            .expect("keys")
            .ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
                destination_hash: observed_node_hash,
                identity_hash,
                signing_public_key: [2u8; 32],
                full_public_key: [3u8; 64],
                app_data: None,
                hops: None,
                packet_hash: None,
                observed_at: 1.0,
            });
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        pending.lock().expect("pending").insert_submission(
            "packet-a".into(),
            delivery_hex.clone(),
            123.456,
            None,
        );
        let message = MessageSummary {
            peer_hash: observed_hex.clone(),
            peer_label: "Peer".into(),
            title: "Reply".into(),
            content: "Body".into(),
            timestamp: 124.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: true,
            unread: true,
            message_id: Some("inbound-a".into()),
            fields: BTreeMap::new(),
            attachments: Vec::new(),
        };

        let evidence = native_lxmf_inbound_peer_evidence(
            &message,
            &pending,
            &destination_keys,
            "peer activity",
        );

        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].peer_hash, delivery_hex);
        assert_eq!(evidence[0].message_id.as_deref(), Some("packet-a"));
        assert!(evidence[0]
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains(&format!("observed_peer_hash:{observed_hex}"))));
    }

    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    #[test]
    fn native_lxmf_local_delivery_decodes_raw_wire_or_packet_payload() {
        let provider = NativeReticulumIdentityProvider;
        let private = provider
            .create_identity_material("sender")
            .expect("native identity");
        let signer =
            reticulum_rs::core::identity::PrivateIdentity::from_private_key_bytes(&private)
                .expect("signer");
        let source = signer.address_hash().to_hex_string();
        let envelope = MessageEnvelope {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            title: "Inbound".into(),
            body: "Hello".into(),
            delivery_mode: crate::messaging::DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            attachments: Vec::new(),
        };
        let outbound =
            crate::runtime::native_lxmf::codec::build_outbound_message(&envelope, &source)
                .expect("outbound");
        let wire =
            crate::runtime::native_lxmf::codec::encode_signed_wire_message(&outbound, &private)
                .expect("wire");
        let destination = [9u8; 16];
        let raw_delivery = RnsNetLocalDelivery {
            destination_hash: destination,
            raw: wire.clone(),
            packet_hash: [8u8; 32],
        };
        let packet = rns_core::packet::RawPacket::pack(
            rns_core::packet::PacketFlags {
                header_type: rns_core::constants::HEADER_1,
                context_flag: rns_core::constants::FLAG_UNSET,
                transport_type: rns_core::constants::TRANSPORT_BROADCAST,
                destination_type: rns_core::constants::DESTINATION_SINGLE,
                packet_type: rns_core::constants::PACKET_TYPE_DATA,
            },
            0,
            &destination,
            None,
            rns_core::constants::CONTEXT_NONE,
            &wire,
        )
        .expect("packet");
        let packet_delivery = RnsNetLocalDelivery {
            destination_hash: destination,
            raw: packet.raw,
            packet_hash: [7u8; 32],
        };

        let attachments_dir = temp_paths("decode-rns-lxmf-delivery").attachments_dir;
        let raw =
            decode_rns_net_lxmf_delivery(&raw_delivery, &attachments_dir).expect("decode raw");
        let packet = decode_rns_net_lxmf_delivery(&packet_delivery, &attachments_dir)
            .expect("decode packet");

        assert_eq!(raw.peer_hash, source);
        assert_eq!(packet.peer_hash, source);
        assert_eq!(raw.title, "Inbound");
        assert_eq!(packet.content, "Hello");
    }

    #[test]
    fn native_runtime_rejects_enabled_unsupported_interface() {
        let paths = temp_paths("unsupported-interface");
        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let profile = crate::interfaces::ReticulumInterfaceProfile::rnode("rnode", "LoRa");
        let plans = plan_interfaces(&[profile]);

        let error = runtime
            .start(None, plans)
            .expect_err("unsupported interface");

        assert!(error.to_string().contains("LoRa"));
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn native_identity_provider_name_is_non_placeholder() {
        let provider = NativeReticulumIdentityProvider;

        assert_eq!(provider.provider_name(), "native-reticulum");
    }

    #[test]
    fn native_download_filename_uses_last_path_segment() {
        assert_eq!(
            filename_from_native_download_path("/files/archive.tar.gz"),
            "archive.tar.gz"
        );
        assert_eq!(filename_from_native_download_path("/files/"), "files");
        assert_eq!(filename_from_native_download_path("/"), "download.bin");
    }

    #[cfg(all(feature = "native-lxmf-sdk", not(feature = "native-rns-net")))]
    #[test]
    fn clean_lxmf_submission_acceptance_is_not_peer_delivery() {
        assert_eq!(clean_lxmf_submission_terminal_flags(true), (false, false));
        assert_eq!(clean_lxmf_submission_terminal_flags(false), (false, true));
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_reticulum_receipt_handler_correlates_once_without_claiming_delivery() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let receipt_hash = hex_encode(&[7u8; 32]);
        pending
            .lock()
            .expect("pending receipt map")
            .insert_correlated_submission(
                receipt_hash,
                "lxmf-message-a".into(),
                "00112233445566778899aabbccddeeff".into(),
                10.0,
                None,
            );
        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: pending,
            event_tx,
        };
        let receipt = rns_transport::transport::DeliveryReceipt::new([7u8; 32]);

        rns_transport::transport::ReceiptHandler::on_receipt(&handler, &receipt);

        let RuntimeBusEvent::MessageDeliveryUpdated(status) =
            events.try_recv().expect("status event")
        else {
            panic!("receipt must emit a status event first");
        };
        assert_eq!(status.message_id.as_deref(), Some("lxmf-message-a"));
        assert!(!status.delivered);
        assert!(!status.failed);
        assert_eq!(
            status.state,
            crate::runtime::network::OutboundDeliveryState::SubmittedToRnsNet
        );
        let RuntimeBusEvent::LxmfDeliveryEvidence(evidence) =
            events.try_recv().expect("evidence event")
        else {
            panic!("receipt must emit evidence second");
        };
        assert_eq!(evidence.message_id.as_deref(), Some("lxmf-message-a"));
        assert_eq!(evidence.kind, LxmfDeliveryEvidenceKind::RnsPacketProof);
        assert!(evidence
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("reticulum_rs_0_9_receipt_handler")));
        assert!(matches!(
            events.try_recv().expect("diagnostic event"),
            RuntimeBusEvent::Debug(_)
        ));

        rns_transport::transport::ReceiptHandler::on_receipt(&handler, &receipt);

        let RuntimeBusEvent::Debug(duplicate) = events.try_recv().expect("duplicate diagnostic")
        else {
            panic!("duplicate receipt must only emit a diagnostic");
        };
        assert!(duplicate.contains("duplicate receipt ignored"));
        assert!(events.try_recv().is_err());
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_reticulum_stale_receipt_cannot_complete_a_newer_retry() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let stale_hash = hex_encode(&[3u8; 32]);
        let retry_hash = hex_encode(&[4u8; 32]);
        {
            let mut pending = pending.lock().expect("pending receipt map");
            pending.insert_correlated_submission(
                stale_hash.clone(),
                "lxmf-message-a".into(),
                "00112233445566778899aabbccddeeff".into(),
                10.0,
                None,
            );
            assert!(pending.remove_correlation(&stale_hash));
            pending.insert_correlated_submission(
                retry_hash.clone(),
                "lxmf-message-a".into(),
                "00112233445566778899aabbccddeeff".into(),
                20.0,
                None,
            );
        }
        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: pending.clone(),
            event_tx,
        };

        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([3u8; 32]),
        );

        let RuntimeBusEvent::Debug(stale) = events.try_recv().expect("stale diagnostic") else {
            panic!("stale receipt must only emit a diagnostic");
        };
        assert!(stale.contains("reason=no_pending_correlation"));
        assert!(events.try_recv().is_err());
        {
            let pending = pending.lock().expect("pending receipt map");
            assert!(pending.pending(&stale_hash).is_none());
            assert!(pending
                .pending(&retry_hash)
                .is_some_and(|retry| retry.packet_proof_observed_at.is_none()));
        }

        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([4u8; 32]),
        );

        let RuntimeBusEvent::MessageDeliveryUpdated(status) =
            events.try_recv().expect("retry status")
        else {
            panic!("matching retry receipt must emit status");
        };
        assert_eq!(status.message_id.as_deref(), Some("lxmf-message-a"));
        assert!(!status.delivered);
        assert!(!status.failed);
        assert!(matches!(
            events.try_recv(),
            Ok(RuntimeBusEvent::LxmfDeliveryEvidence(_))
        ));
        assert!(matches!(events.try_recv(), Ok(RuntimeBusEvent::Debug(_))));
        assert!(events.try_recv().is_err());
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_lxmf_resource_completion_correlates_once_without_claiming_peer_delivery() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let resource_hash = [9u8; 32];
        let resource_hash_hex = hex_encode(&resource_hash);
        pending
            .lock()
            .expect("pending resource map")
            .insert_correlated_submission(
                resource_hash_hex.clone(),
                "lxmf-resource-a".into(),
                "00112233445566778899aabbccddeeff".into(),
                12.0,
                None,
            );
        let (event_tx, mut events) = broadcast::channel(8);

        assert!(emit_clean_lxmf_resource_terminal(
            &event_tx,
            &pending,
            &resource_hash,
            CleanLxmfResourceTerminal::Complete,
        ));

        let RuntimeBusEvent::ResourceLifecycle(lifecycle) =
            events.try_recv().expect("resource lifecycle")
        else {
            panic!("resource completion must emit lifecycle first");
        };
        assert_eq!(lifecycle.transfer_id, resource_hash_hex);
        assert_eq!(lifecycle.state, ResourceLifecycleState::Complete);
        assert_eq!(lifecycle.source.as_deref(), Some("lxmf"));
        let RuntimeBusEvent::MessageDeliveryUpdated(status) =
            events.try_recv().expect("delivery status")
        else {
            panic!("resource completion must emit delivery status second");
        };
        assert_eq!(status.message_id.as_deref(), Some("lxmf-resource-a"));
        assert!(!status.delivered);
        assert!(!status.failed);
        assert_eq!(status.state, OutboundDeliveryState::SubmittedToRnsNet);
        let RuntimeBusEvent::LxmfDeliveryEvidence(evidence) =
            events.try_recv().expect("delivery evidence")
        else {
            panic!("resource completion must emit evidence third");
        };
        assert_eq!(evidence.kind, LxmfDeliveryEvidenceKind::PacketSubmitted);
        assert!(evidence
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("resource_completed")));
        assert!(matches!(
            events.try_recv().expect("diagnostic"),
            RuntimeBusEvent::Debug(_)
        ));
        assert_eq!(
            pending.lock().expect("pending resource map").pending_len(),
            0
        );
        assert!(!emit_clean_lxmf_resource_terminal(
            &event_tx,
            &pending,
            &resource_hash,
            CleanLxmfResourceTerminal::Complete,
        ));
        assert!(events.try_recv().is_err());
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_lxmf_resource_offer_reports_bounded_total_and_operation() {
        let resource_hash = hex_encode(&[8u8; 32]);
        let (events, mut receiver) = broadcast::channel(8);

        emit_clean_lxmf_resource_offered(
            &events,
            &resource_hash,
            "message-progress",
            "peer-progress",
            4096,
        );
        let RuntimeBusEvent::ResourceProgress(progress) =
            receiver.try_recv().expect("Resource progress event")
        else {
            panic!("expected ResourceProgress");
        };
        assert_eq!(progress.transfer_id, resource_hash);
        assert_eq!(progress.received, 0);
        assert_eq!(progress.total, Some(4096));
        assert_eq!(progress.operation_id.as_deref(), Some("message-progress"));
        assert_eq!(progress.source.as_deref(), Some("lxmf"));
        assert_eq!(progress.purpose.as_deref(), Some("direct-message"));
        assert_eq!(progress.direction.as_deref(), Some("outbound"));

        let RuntimeBusEvent::ResourceLifecycle(lifecycle) =
            receiver.try_recv().expect("Resource offered lifecycle")
        else {
            panic!("expected ResourceLifecycle");
        };
        assert_eq!(lifecycle.transfer_id, resource_hash);
        assert_eq!(lifecycle.state, ResourceLifecycleState::Offered);
        assert_eq!(lifecycle.bytes, Some(4096));
        assert_eq!(lifecycle.operation_id.as_deref(), Some("message-progress"));
        assert_eq!(lifecycle.source.as_deref(), Some("lxmf"));
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    fn clean_lxmf_resource_failure_and_cancellation_release_correlation() {
        for (resource_hash_byte, terminal, lifecycle_state, transfer_state) in [
            (
                1,
                CleanLxmfResourceTerminal::Failed,
                ResourceLifecycleState::Failed,
                "resource_failed",
            ),
            (
                2,
                CleanLxmfResourceTerminal::Cancelled,
                ResourceLifecycleState::Cancelled,
                "resource_cancelled",
            ),
        ] {
            let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
            let resource_hash = [resource_hash_byte; 32];
            pending
                .lock()
                .expect("pending resource map")
                .insert_correlated_submission(
                    hex_encode(&resource_hash),
                    format!("lxmf-{transfer_state}"),
                    "ffeeddccbbaa99887766554433221100".into(),
                    20.0,
                    None,
                );
            let (event_tx, mut events) = broadcast::channel(8);

            assert!(emit_clean_lxmf_resource_terminal(
                &event_tx,
                &pending,
                &resource_hash,
                terminal,
            ));

            let RuntimeBusEvent::ResourceLifecycle(lifecycle) =
                events.try_recv().expect("resource lifecycle")
            else {
                panic!("terminal resource must emit lifecycle");
            };
            assert_eq!(lifecycle.state, lifecycle_state);
            let RuntimeBusEvent::MessageDeliveryUpdated(status) =
                events.try_recv().expect("delivery status")
            else {
                panic!("terminal resource must emit status");
            };
            assert!(!status.delivered);
            assert!(status.failed);
            assert_eq!(status.state, OutboundDeliveryState::Failed);
            let RuntimeBusEvent::LxmfDeliveryEvidence(evidence) =
                events.try_recv().expect("delivery evidence")
            else {
                panic!("terminal resource must emit evidence");
            };
            assert_eq!(evidence.kind, LxmfDeliveryEvidenceKind::LxmfRouterFailed);
            assert!(evidence
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(transfer_state)));
            assert!(matches!(events.try_recv(), Ok(RuntimeBusEvent::Debug(_))));
            assert_eq!(
                pending.lock().expect("pending resource map").pending_len(),
                0
            );
        }
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_runtime_restart_recovers_only_current_persisted_receipt_correlation() {
        let paths = temp_paths("clean-receipt-recovery");
        let store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("isolated message store");
        let message = |peer_hash: &str, message_id: &str, packet_hash: String| MessageSummary {
            peer_hash: peer_hash.into(),
            peer_label: "Peer".into(),
            title: "Title".into(),
            content: "Body".into(),
            timestamp: 10.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some(message_id.into()),
            fields: BTreeMap::from([
                (
                    "native_lxmf_state".into(),
                    "submitted_to_clean_reticulum".into(),
                ),
                (
                    "native_lxmf_proof_state".into(),
                    "waiting_for_transport_receipt".into(),
                ),
                ("native_lxmf_packet_hash".into(), packet_hash),
            ]),
            attachments: Vec::new(),
        };
        let stale_peer = "00112233445566778899aabbccddeeff";
        let current_peer = "ffeeddccbbaa99887766554433221100";
        let stale_hash = hex_encode(&[3u8; 32]);
        let current_hash = hex_encode(&[4u8; 32]);
        store
            .append(message(stale_peer, "lxmf-stale", stale_hash.clone()))
            .expect("persist stale correlation");
        store
            .append(message(current_peer, "lxmf-current", current_hash.clone()))
            .expect("persist current correlation");

        let first_runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let first_rows = store
            .list_threads()
            .expect("load first runtime rows")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect();
        assert_eq!(
            first_runtime
                .recover_lxmf_correlation(first_rows)
                .await
                .expect("recover first runtime correlations")
                .direct_recovered,
            2
        );
        assert!(store
            .delete_thread(stale_peer)
            .expect("delete obsolete correlation thread"));
        drop(first_runtime);
        drop(store);

        let restarted_store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("reopen isolated message store");
        let restarted_rows = restarted_store
            .list_threads()
            .expect("load restarted runtime rows")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect::<Vec<_>>();
        assert_eq!(restarted_rows.len(), 1);
        assert_eq!(
            restarted_rows[0].message_id.as_deref(),
            Some("lxmf-current")
        );

        let restarted_runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let recovered = restarted_runtime
            .recover_lxmf_correlation(restarted_rows)
            .await
            .expect("recover restarted clean correlation");
        assert_eq!(recovered.direct_recovered, 1);
        assert_eq!(recovered.propagated_recovered, 0);
        {
            let pending = restarted_runtime
                .pending_lxmf_proofs
                .lock()
                .expect("restarted pending proof map");
            assert!(pending.pending(&stale_hash).is_none());
            assert_eq!(
                pending
                    .pending(&current_hash)
                    .map(|pending| pending.message_id.as_str()),
                Some("lxmf-current")
            );
        }

        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: restarted_runtime.pending_lxmf_proofs.clone(),
            event_tx,
        };
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([3u8; 32]),
        );
        let RuntimeBusEvent::Debug(stale) = events.try_recv().expect("stale diagnostic") else {
            panic!("deleted pre-restart correlation must not emit delivery state");
        };
        assert!(stale.contains("reason=no_pending_correlation"));
        assert!(events.try_recv().is_err());

        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([4u8; 32]),
        );
        let RuntimeBusEvent::MessageDeliveryUpdated(status) =
            events.try_recv().expect("current delivery status")
        else {
            panic!("current persisted correlation must emit delivery state");
        };
        assert_eq!(status.message_id.as_deref(), Some("lxmf-current"));
        assert_eq!(status.peer_hash, current_peer);
        assert!(matches!(
            events.try_recv(),
            Ok(RuntimeBusEvent::LxmfDeliveryEvidence(_))
        ));
        assert!(matches!(events.try_recv(), Ok(RuntimeBusEvent::Debug(_))));
        assert!(events.try_recv().is_err());

        drop(restarted_store);
        drop(restarted_runtime);
        std::fs::remove_dir_all(&paths.root).expect("remove isolated recovery root");
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_timeout_persistence_keeps_late_proof_scoped_to_old_attempt() {
        let paths = temp_paths("clean-timeout-replacement-ownership");
        let store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("isolated timeout store");
        let operation = crate::messaging::OutboundOperationIdentity::generate();
        let message =
            |message_id: &str, packet_hash: String, timestamp: f64, submitted_at: &str| {
                let mut fields = BTreeMap::from([
                    (
                        "native_lxmf_state".into(),
                        "submitted_to_clean_reticulum".into(),
                    ),
                    (
                        "native_lxmf_proof_state".into(),
                        "waiting_for_transport_receipt".into(),
                    ),
                    ("native_lxmf_packet_hash".into(), packet_hash),
                    ("native_lxmf_submitted_at".into(), submitted_at.into()),
                ]);
                operation.insert_fields(&mut fields);
                MessageSummary {
                    peer_hash: "00112233445566778899aabbccddeeff".into(),
                    peer_label: "Peer".into(),
                    title: "Title".into(),
                    content: "Body".into(),
                    timestamp,
                    transport_method: crate::messaging::TransportMethod::Direct,
                    delivered: false,
                    failed: false,
                    incoming: false,
                    unread: false,
                    message_id: Some(message_id.into()),
                    fields,
                    attachments: Vec::new(),
                }
            };
        let old_hash = hex_encode(&[3u8; 32]);
        let current_hash = hex_encode(&[4u8; 32]);
        store
            .append(message("lxmf-old", old_hash.clone(), 10.0, "10.0"))
            .expect("persist old clean attempt");

        let first_runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let old_rows = store
            .list_threads()
            .expect("load old attempt")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect();
        assert_eq!(
            first_runtime
                .recover_lxmf_correlation(old_rows)
                .await
                .expect("recover old clean attempt")
                .direct_recovered,
            1
        );
        assert_eq!(
            first_runtime
                .pending_lxmf_proofs
                .lock()
                .expect("old timeout map")
                .reconcile_timeouts(60.0, 45.0)
                .len(),
            1
        );
        let timed_out = store
            .reconcile_stale_native_lxmf_direct(60.0, 45.0)
            .expect("persist clean timeout transition");
        assert_eq!(timed_out.len(), 1);
        assert_eq!(
            timed_out[0]
                .fields
                .get("native_lxmf_state")
                .map(String::as_str),
            Some("submitted_unconfirmed")
        );
        assert_eq!(
            timed_out[0]
                .fields
                .get("native_lxmf_proof_state")
                .map(String::as_str),
            Some("proof_not_observed")
        );

        store
            .append(message("lxmf-current", current_hash.clone(), 60.0, "60.0"))
            .expect("persist replacement clean attempt");
        let replacement_rows = store
            .list_threads()
            .expect("load replacement rows")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect();
        assert_eq!(
            first_runtime
                .recover_lxmf_correlation(replacement_rows)
                .await
                .expect("recover replacement correlation")
                .direct_recovered,
            1
        );

        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: first_runtime.pending_lxmf_proofs.clone(),
            event_tx,
        };
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([3u8; 32]),
        );
        let RuntimeBusEvent::MessageDeliveryUpdated(old_status) =
            events.try_recv().expect("late old-attempt status")
        else {
            panic!("late proof must remain scoped to its retained old attempt");
        };
        assert_eq!(old_status.message_id.as_deref(), Some("lxmf-old"));
        assert!(matches!(
            events.try_recv(),
            Ok(RuntimeBusEvent::LxmfDeliveryEvidence(_))
        ));
        assert!(matches!(events.try_recv(), Ok(RuntimeBusEvent::Debug(_))));
        assert!(first_runtime
            .pending_lxmf_proofs
            .lock()
            .expect("replacement proof map")
            .pending(&current_hash)
            .is_some_and(|current| current.packet_proof_observed_at.is_none()));
        drop(first_runtime);
        drop(store);

        let restarted_store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("reopen timeout store");
        let restarted_rows = restarted_store
            .list_threads()
            .expect("load timeout rows after restart")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect::<Vec<_>>();
        let restarted_runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        assert_eq!(
            restarted_runtime
                .recover_lxmf_correlation(restarted_rows)
                .await
                .expect("recover only replacement after timeout restart")
                .direct_recovered,
            1
        );
        {
            let pending = restarted_runtime
                .pending_lxmf_proofs
                .lock()
                .expect("restarted timeout proof map");
            assert!(pending.pending(&old_hash).is_none());
            assert_eq!(
                pending
                    .pending(&current_hash)
                    .map(|current| current.message_id.as_str()),
                Some("lxmf-current")
            );
        }

        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: restarted_runtime.pending_lxmf_proofs.clone(),
            event_tx,
        };
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([3u8; 32]),
        );
        assert!(matches!(
            events.try_recv(),
            Ok(RuntimeBusEvent::Debug(detail)) if detail.contains("reason=no_pending_correlation")
        ));
        assert!(events.try_recv().is_err());
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([4u8; 32]),
        );
        let RuntimeBusEvent::MessageDeliveryUpdated(current_status) =
            events.try_recv().expect("replacement status after restart")
        else {
            panic!("replacement proof must retain current ownership after restart");
        };
        assert_eq!(current_status.message_id.as_deref(), Some("lxmf-current"));

        drop(restarted_store);
        drop(restarted_runtime);
        std::fs::remove_dir_all(&paths.root).expect("remove isolated timeout ownership root");
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_timeout_replacement_survives_abrupt_process_termination() {
        const CHILD_TEST: &str =
            "runtime::native::adapter::tests::clean_timeout_replacement_crash_boundary_child";
        const CHILD_ROOT_ENV: &str = "OMEN_TEST_CLEAN_TIMEOUT_CRASH_ROOT";
        const CHILD_READY_ENV: &str = "OMEN_TEST_CLEAN_TIMEOUT_CRASH_READY";

        let paths = temp_paths("clean-timeout-crash-boundary");
        let ready = paths.root.join("replacement-committed.ready");
        let mut child = std::process::Command::new(
            std::env::current_exe().expect("current unit-test executable"),
        )
        .args([
            "--exact",
            CHILD_TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ROOT_ENV, &paths.root)
        .env(CHILD_READY_ENV, &ready)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn isolated timeout crash-boundary child");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let (reached_boundary, timed_out) = loop {
            if ready.is_file() {
                break (true, false);
            }
            if child
                .try_wait()
                .expect("poll timeout crash-boundary child")
                .is_some()
            {
                break (false, false);
            }
            if tokio::time::Instant::now() >= deadline {
                child
                    .kill()
                    .expect("kill timed-out timeout crash-boundary child");
                break (false, true);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        if !reached_boundary {
            let output = child
                .wait_with_output()
                .expect("reap failed timeout crash-boundary child");
            panic!(
                "timeout crash-boundary child did not publish committed marker timed_out={timed_out}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        child
            .kill()
            .expect("terminate child after committed timeout/replacement boundary");
        let output = child
            .wait_with_output()
            .expect("reap terminated timeout crash-boundary child");
        assert!(
            !output.status.success(),
            "terminated timeout crash-boundary child unexpectedly exited successfully"
        );

        let store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("reopen abruptly terminated timeout store");
        let rows = store
            .list_threads()
            .expect("load abruptly terminated timeout rows")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        let old = rows
            .iter()
            .find(|message| message.message_id.as_deref() == Some("lxmf-old"))
            .expect("committed timed-out old row");
        assert_eq!(
            old.fields.get("native_lxmf_state").map(String::as_str),
            Some("submitted_unconfirmed")
        );
        assert_eq!(
            old.fields
                .get("native_lxmf_proof_state")
                .map(String::as_str),
            Some("proof_not_observed")
        );
        assert!(rows
            .iter()
            .any(|message| message.message_id.as_deref() == Some("lxmf-current")));

        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        assert_eq!(
            runtime
                .recover_lxmf_correlation(rows)
                .await
                .expect("recover after abrupt timeout termination")
                .direct_recovered,
            1
        );
        let old_hash = hex_encode(&[3u8; 32]);
        let current_hash = hex_encode(&[4u8; 32]);
        {
            let pending = runtime
                .pending_lxmf_proofs
                .lock()
                .expect("abrupt restart pending proof map");
            assert!(pending.pending(&old_hash).is_none());
            assert_eq!(
                pending
                    .pending(&current_hash)
                    .map(|current| current.message_id.as_str()),
                Some("lxmf-current")
            );
        }

        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: runtime.pending_lxmf_proofs.clone(),
            event_tx,
        };
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([3u8; 32]),
        );
        assert!(matches!(
            events.try_recv(),
            Ok(RuntimeBusEvent::Debug(detail)) if detail.contains("reason=no_pending_correlation")
        ));
        assert!(events.try_recv().is_err());
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([4u8; 32]),
        );
        let RuntimeBusEvent::MessageDeliveryUpdated(current_status) = events
            .try_recv()
            .expect("current status after abrupt restart")
        else {
            panic!("current replacement must recover after abrupt process termination");
        };
        assert_eq!(current_status.message_id.as_deref(), Some("lxmf-current"));

        drop(store);
        drop(runtime);
        std::fs::remove_dir_all(&paths.root).expect("remove isolated timeout crash root");
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[test]
    #[ignore = "helper is terminated by the isolated timeout crash-boundary regression"]
    fn clean_timeout_replacement_crash_boundary_child() {
        use std::io::Write as _;

        const CHILD_ROOT_ENV: &str = "OMEN_TEST_CLEAN_TIMEOUT_CRASH_ROOT";
        const CHILD_READY_ENV: &str = "OMEN_TEST_CLEAN_TIMEOUT_CRASH_READY";
        let Some(root) = std::env::var_os(CHILD_ROOT_ENV) else {
            return;
        };
        let ready = PathBuf::from(
            std::env::var_os(CHILD_READY_ENV).expect("timeout crash ready marker path"),
        );
        let paths = AppPaths::from_root(root.into());
        let store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("create timeout crash-boundary store");
        let operation = crate::messaging::OutboundOperationIdentity::generate();
        let message =
            |message_id: &str, packet_hash: String, timestamp: f64, submitted_at: &str| {
                let mut fields = BTreeMap::from([
                    (
                        "native_lxmf_state".into(),
                        "submitted_to_clean_reticulum".into(),
                    ),
                    (
                        "native_lxmf_proof_state".into(),
                        "waiting_for_transport_receipt".into(),
                    ),
                    ("native_lxmf_packet_hash".into(), packet_hash),
                    ("native_lxmf_submitted_at".into(), submitted_at.into()),
                ]);
                operation.insert_fields(&mut fields);
                MessageSummary {
                    peer_hash: "00112233445566778899aabbccddeeff".into(),
                    peer_label: "Peer".into(),
                    title: "Title".into(),
                    content: "Body".into(),
                    timestamp,
                    transport_method: crate::messaging::TransportMethod::Direct,
                    delivered: false,
                    failed: false,
                    incoming: false,
                    unread: false,
                    message_id: Some(message_id.into()),
                    fields,
                    attachments: Vec::new(),
                }
            };
        store
            .append(message("lxmf-old", hex_encode(&[3u8; 32]), 10.0, "10.0"))
            .expect("commit old attempt before crash boundary");
        assert_eq!(
            store
                .reconcile_stale_native_lxmf_direct(60.0, 45.0)
                .expect("commit timeout before crash boundary")
                .len(),
            1
        );
        store
            .append(message(
                "lxmf-current",
                hex_encode(&[4u8; 32]),
                60.0,
                "60.0",
            ))
            .expect("commit replacement before crash boundary");
        assert_eq!(
            store
                .list_threads()
                .expect("verify committed crash-boundary rows")[0]
                .messages
                .len(),
            2
        );

        let mut marker = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&ready)
            .expect("create timeout crash-boundary marker");
        marker
            .write_all(b"timeout-and-replacement-committed\n")
            .expect("write timeout crash-boundary marker");
        marker
            .sync_all()
            .expect("sync timeout crash-boundary marker");
        drop(marker);

        loop {
            std::thread::park();
        }
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    async fn clean_runtime_process_restart_recovers_only_current_persisted_correlation() {
        const CHILD_TEST: &str =
            "runtime::native::adapter::tests::clean_runtime_process_restart_recovery_child";
        const CHILD_ROOT_ENV: &str = "OMEN_TEST_CLEAN_RECEIPT_RESTART_ROOT";

        let paths = temp_paths("clean-receipt-process-recovery");
        let store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("isolated process-recovery store");
        let message = |peer_hash: &str, message_id: &str, packet_hash: String| MessageSummary {
            peer_hash: peer_hash.into(),
            peer_label: "Peer".into(),
            title: "Title".into(),
            content: "Body".into(),
            timestamp: 10.0,
            transport_method: crate::messaging::TransportMethod::Direct,
            delivered: false,
            failed: false,
            incoming: false,
            unread: false,
            message_id: Some(message_id.into()),
            fields: BTreeMap::from([
                (
                    "native_lxmf_state".into(),
                    "submitted_to_clean_reticulum".into(),
                ),
                (
                    "native_lxmf_proof_state".into(),
                    "waiting_for_transport_receipt".into(),
                ),
                ("native_lxmf_packet_hash".into(), packet_hash),
            ]),
            attachments: Vec::new(),
        };
        let stale_peer = "00112233445566778899aabbccddeeff";
        store
            .append(message(stale_peer, "lxmf-stale", hex_encode(&[3u8; 32])))
            .expect("persist stale process correlation");
        store
            .append(message(
                "ffeeddccbbaa99887766554433221100",
                "lxmf-current",
                hex_encode(&[4u8; 32]),
            ))
            .expect("persist current process correlation");

        let first_runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let first_rows = store
            .list_threads()
            .expect("load parent runtime rows")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect();
        assert_eq!(
            first_runtime
                .recover_lxmf_correlation(first_rows)
                .await
                .expect("recover parent runtime correlations")
                .direct_recovered,
            2
        );
        assert!(store
            .delete_thread(stale_peer)
            .expect("delete obsolete process correlation"));
        drop(first_runtime);
        drop(store);

        let mut child = std::process::Command::new(
            std::env::current_exe().expect("current unit-test executable"),
        )
        .args([
            "--exact",
            CHILD_TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ROOT_ENV, &paths.root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn isolated process-recovery child");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let timed_out = loop {
            if child
                .try_wait()
                .expect("poll process-recovery child")
                .is_some()
            {
                break false;
            }
            if tokio::time::Instant::now() >= deadline {
                child.kill().expect("kill timed-out process-recovery child");
                break true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let output = child
            .wait_with_output()
            .expect("reap process-recovery child");
        assert!(!timed_out, "process-recovery child exceeded ten seconds");
        assert!(
            output.status.success(),
            "process-recovery child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        std::fs::remove_dir_all(&paths.root).expect("remove isolated process-recovery root");
    }

    #[cfg(all(feature = "native-lxmf", not(feature = "native-rns-net")))]
    #[tokio::test]
    #[ignore = "helper executed only by the isolated process-restart regression"]
    async fn clean_runtime_process_restart_recovery_child() {
        const CHILD_ROOT_ENV: &str = "OMEN_TEST_CLEAN_RECEIPT_RESTART_ROOT";
        let Some(root) = std::env::var_os(CHILD_ROOT_ENV) else {
            return;
        };
        let paths = AppPaths::from_root(root.into());
        let store = crate::messaging::MessageStore::new(paths.messages_dir.clone())
            .expect("reopen child process message store");
        let rows = store
            .list_threads()
            .expect("load child process rows")
            .into_iter()
            .flat_map(|thread| thread.messages)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id.as_deref(), Some("lxmf-current"));

        let runtime = NativeNetworkRuntime::new(NativeRuntimeConfig::from_paths(&paths));
        let recovered = runtime
            .recover_lxmf_correlation(rows)
            .await
            .expect("recover child process correlation");
        assert_eq!(recovered.direct_recovered, 1);
        assert_eq!(recovered.propagated_recovered, 0);
        let stale_hash = hex_encode(&[3u8; 32]);
        let current_hash = hex_encode(&[4u8; 32]);
        {
            let pending = runtime
                .pending_lxmf_proofs
                .lock()
                .expect("child pending proof map");
            assert!(pending.pending(&stale_hash).is_none());
            assert_eq!(
                pending
                    .pending(&current_hash)
                    .map(|pending| pending.message_id.as_str()),
                Some("lxmf-current")
            );
        }

        let (event_tx, mut events) = broadcast::channel(8);
        let handler = CleanLxmfReceiptHandler {
            pending_lxmf_proofs: runtime.pending_lxmf_proofs.clone(),
            event_tx,
        };
        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([3u8; 32]),
        );
        let RuntimeBusEvent::Debug(stale) = events.try_recv().expect("stale child diagnostic")
        else {
            panic!("deleted pre-process correlation must not emit delivery state");
        };
        assert!(stale.contains("reason=no_pending_correlation"));
        assert!(events.try_recv().is_err());

        rns_transport::transport::ReceiptHandler::on_receipt(
            &handler,
            &rns_transport::transport::DeliveryReceipt::new([4u8; 32]),
        );
        let RuntimeBusEvent::MessageDeliveryUpdated(status) =
            events.try_recv().expect("current child delivery status")
        else {
            panic!("current post-process correlation must emit delivery state");
        };
        assert_eq!(status.message_id.as_deref(), Some("lxmf-current"));
        assert_eq!(status.peer_hash, "ffeeddccbbaa99887766554433221100");
        assert!(matches!(
            events.try_recv(),
            Ok(RuntimeBusEvent::LxmfDeliveryEvidence(_))
        ));
        assert!(matches!(events.try_recv(), Ok(RuntimeBusEvent::Debug(_))));
        assert!(events.try_recv().is_err());
    }
}
