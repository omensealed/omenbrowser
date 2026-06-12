use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATED_TERMINAL_RETENTION_SECS: f64 = 3600.0;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATED_TRANSFER_TIMEOUT_SECS: f64 = 45.0;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATION_PATH_WAIT_ATTEMPTS: usize = 30;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_PROPAGATION_PATH_WAIT_STEP: Duration = Duration::from_millis(100);
#[cfg(feature = "native-rns-net")]
const NATIVE_LXMF_DIRECT_PROOF_TIMEOUT_SECS: f64 = 45.0;
#[cfg(feature = "native-rns-net")]
const NATIVE_LXMF_DIRECT_ROUTER_TICK_SECS: u64 = 5;
#[cfg(feature = "native-rns-net")]
const NATIVE_KNOWN_DESTINATIONS_MAX_AGE_SECS: f64 = 6.0 * 60.0 * 60.0;
#[cfg(feature = "native-rns-net")]
const NATIVE_KNOWN_DESTINATIONS_MAX_SAVED: usize = 4096;
#[cfg(feature = "native-rns-net")]
const NATIVE_KNOWN_DESTINATIONS_SAVE_INTERVAL_SECS: u64 = 30;
#[cfg(feature = "native-rns-net")]
const OMENCHAT_LINK_CONTEXT: u8 = 0x4f;
#[cfg(feature = "native-rns-net")]
const OMENCHAT_LINK_PATH_WAIT_ATTEMPTS: usize = 40;
#[cfg(feature = "native-rns-net")]
const OMENCHAT_LINK_PATH_WAIT_STEP: Duration = Duration::from_millis(250);
#[cfg(feature = "native-rns-net")]
const OMENCHAT_RESOURCE_METADATA_PREFIX: &[u8] = b"omenchat-resource:";
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_LINK_PACKET_MDU: usize = 431;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
const NATIVE_LXMF_OPPORTUNISTIC_PACKET_MDU: usize = rns_core::constants::ENCRYPTED_MDU;

use crate::browser::{BrowserPage, DownloadedFile};
use crate::directory::DirectoryKind;
use crate::error::{AppError, AppResult};
use crate::identity::IdentityProfile;
#[cfg(feature = "native-lxmf")]
use crate::messaging::DeliveryMode;
use crate::messaging::{MessageEnvelope, MessageSummary};
#[cfg(feature = "native-rns-net")]
use crate::runtime::native::announce::display_name_for_kind;
use crate::runtime::native::announce::{payload_from_announce_event, NativeAnnounceState};
#[cfg(feature = "native-rns-net")]
use crate::runtime::native::identity::load_rns_net_proof_signing_key_file;
use crate::runtime::native::identity::{
    load_private_identity_file, load_transport_private_identity_file, NativeIdentitySummary,
};
use crate::runtime::native::interface::{
    plan_interfaces, validate_startup_plans, NativeInterfacePlan,
};
#[cfg(feature = "native-rns-net")]
use crate::runtime::native::lxmf_router::{
    DirectLxmfTimeoutEvent, NativeDirectLxmfRouter, NativePropagatedLxmfRouter,
    PropagatedNodeAccepted, PropagatedNodeFailed,
};
#[cfg(not(feature = "native-rns-net"))]
use crate::runtime::native::request::NativePageFetchContext;
use crate::runtime::native::request::{
    NativeFetchPlan, NativePageTransportClient, ReticulumPageTransportClient,
};
#[cfg(feature = "native-rns-net")]
use crate::runtime::native::rns_net::{
    RnsNetAnnounceKey, RnsNetDestinationKeyStore, RnsNetDestinationKeys, RnsNetLinkData,
    RnsNetLocalDelivery, RnsNetPageCallbacks, RnsNetPageRequestClient, RnsNetPathUpdate,
    RnsNetProof, RnsNetResourceEvent,
};
#[cfg(feature = "native-rns-net")]
use crate::runtime::native::NativePageFetchFailureStage;
use crate::runtime::native::{NativeRuntimeConfig, NativeRuntimeError};
use crate::runtime::network::{
    AnnouncePayload, CancellationToken, DestinationInspection, DirectoryCandidate, InterfaceSample,
    InterfaceSampleState, InterfaceStats, LxmfCorrelationRecovery, LxmfDeliveryProbeReport,
    LxmfDeliveryProbeStage, LxmfDeliveryProbeStep, NetworkRuntime, NetworkSnapshot, NetworkStatus,
    PageFetchProbeReport, PageFetchProbeStage, PageFetchProbeStep, PropagationDebugSnapshot,
    PropagationMessageSnapshot, PropagationStatus, RuntimeBackendName,
};
#[cfg(feature = "native-rns-net")]
use crate::runtime::network::{
    LxmfDeliveryEvidence, LxmfDeliveryEvidenceKind, OmenChatLinkClosed, OmenChatLinkData,
    OmenChatLinkOpened, OmenChatResourceData, OutboundDeliveryState, OutboundStatus,
};
#[cfg(feature = "native-rns-net")]
use crate::runtime::PathEvent;
use crate::runtime::RuntimeBusEvent;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
use crate::runtime::{PropagationSyncEvent, PropagationSyncEventStatus, PropagationSyncStage};
use crate::storage::files::next_available_download_path;
#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
use crate::storage::transient_ids::{
    DeliveredTransientIdStore, LXMF_LOCAL_DELIVERY_CACHE_MAX_AGE_SECS,
};

#[derive(Clone)]
pub struct NativeNetworkRuntime {
    config: NativeRuntimeConfig,
    state: Arc<Mutex<NativeRuntimeState>>,
    transport: Arc<Mutex<Option<NativeTransportHandle>>>,
    announces: Arc<Mutex<NativeAnnounceState>>,
    inbound_messages: Arc<Mutex<Vec<MessageSummary>>>,
    outbound_propagation_node: Arc<Mutex<Option<String>>>,
    identify_on_connect_destinations: Arc<Mutex<BTreeSet<String>>>,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
    page_transport: Arc<dyn NativePageTransportClient>,
    #[cfg(feature = "native-rns-net")]
    rns_net: Arc<Mutex<Option<NativeRnsNetHandle>>>,
    #[cfg(feature = "native-rns-net")]
    pending_lxmf_proofs: PendingLxmfProofs,
    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    pending_direct_lxmf_resources: PendingDirectLxmfResources,
    #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
    pending_propagated_lxmf: PendingPropagatedLxmf,
    #[cfg(feature = "native-rns-net")]
    active_omenchat_links: Arc<Mutex<BTreeSet<[u8; 16]>>>,
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
    #[cfg(feature = "native-rns-net")]
    pub rns_net_started: bool,
}

#[derive(Clone)]
struct NativeTransportHandle {
    transport: Arc<reticulum_rs::runtime::Transport>,
    interface_count: usize,
    attached_interfaces: Vec<String>,
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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
        Self {
            config,
            state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            transport: Arc::new(Mutex::new(None)),
            announces: Arc::new(Mutex::new(NativeAnnounceState::default())),
            inbound_messages: Arc::new(Mutex::new(Vec::new())),
            outbound_propagation_node: Arc::new(Mutex::new(None)),
            identify_on_connect_destinations: Arc::new(Mutex::new(BTreeSet::new())),
            event_tx: broadcast::channel(256).0,
            page_transport: Arc::new(ReticulumPageTransportClient),
            #[cfg(feature = "native-rns-net")]
            rns_net: Arc::new(Mutex::new(None)),
            #[cfg(feature = "native-rns-net")]
            pending_lxmf_proofs: Arc::new(Mutex::new(NativeDirectLxmfRouter::default())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_direct_lxmf_resources: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_propagated_lxmf: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(feature = "native-rns-net")]
            active_omenchat_links: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    #[cfg(test)]
    fn with_page_transport(
        config: NativeRuntimeConfig,
        page_transport: Arc<dyn NativePageTransportClient>,
    ) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            transport: Arc::new(Mutex::new(None)),
            announces: Arc::new(Mutex::new(NativeAnnounceState::default())),
            inbound_messages: Arc::new(Mutex::new(Vec::new())),
            outbound_propagation_node: Arc::new(Mutex::new(None)),
            identify_on_connect_destinations: Arc::new(Mutex::new(BTreeSet::new())),
            event_tx: broadcast::channel(256).0,
            page_transport,
            #[cfg(feature = "native-rns-net")]
            rns_net: Arc::new(Mutex::new(None)),
            #[cfg(feature = "native-rns-net")]
            pending_lxmf_proofs: Arc::new(Mutex::new(NativeDirectLxmfRouter::default())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_direct_lxmf_resources: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
            pending_propagated_lxmf: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(feature = "native-rns-net")]
            active_omenchat_links: Arc::new(Mutex::new(BTreeSet::new())),
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
                "none".to_string()
            } else {
                ifac_summary
            },
            self.config.announce_on_start
        )));
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
        #[cfg(feature = "native-rns-net")]
        let transport = None;
        #[cfg(not(feature = "native-rns-net"))]
        let transport = identity_path
            .as_ref()
            .map(|path| self.build_transport(path, &interfaces))
            .transpose()?;
        #[cfg(feature = "native-rns-net")]
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
        *self.transport.lock().expect("native transport lock") = transport;
        #[cfg(feature = "native-rns-net")]
        {
            state.rns_net_started = rns_net.is_some();
            *self.rns_net.lock().expect("native rns-net lock") = rns_net;
        }
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum runtime running transport_started={}{}",
            state.transport_started,
            {
                #[cfg(feature = "native-rns-net")]
                {
                    format!(" rns_net_started={}", state.rns_net_started)
                }
                #[cfg(not(feature = "native-rns-net"))]
                {
                    String::new()
                }
            }
        )));
        Ok(())
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().expect("native runtime state lock");
        state.lifecycle = NativeRuntimeLifecycle::Stopped;
        state.transport_started = false;
        #[cfg(feature = "native-rns-net")]
        {
            state.rns_net_started = false;
        }
        *self.transport.lock().expect("native transport lock") = None;
        #[cfg(feature = "native-rns-net")]
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

    fn set_failed(&self, message: impl Into<String>) {
        let mut state = self.state.lock().expect("native runtime state lock");
        state.lifecycle = NativeRuntimeLifecycle::Failed(message.into());
        state.transport_started = false;
        #[cfg(feature = "native-rns-net")]
        {
            state.rns_net_started = false;
        }
        *self.transport.lock().expect("native transport lock") = None;
        #[cfg(feature = "native-rns-net")]
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
        let transport = reticulum_rs::runtime::Transport::new(config);
        let transport = Arc::new(transport);
        let attached_interfaces = attach_tcp_client_interfaces(&transport, interfaces)?;
        spawn_announce_listener(
            transport.clone(),
            self.announces.clone(),
            self.event_tx.clone(),
        );
        let interface_count = attached_interfaces.len();
        let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
            "native Reticulum transport ready attached_tcp_clients={interface_count}"
        )));

        Ok(NativeTransportHandle {
            transport,
            interface_count,
            attached_interfaces,
        })
    }

    #[cfg(feature = "native-rns-net")]
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
            #[cfg(feature = "native-rns-net")]
            let active_omenchat_links = self.active_omenchat_links.clone();
            tokio::spawn(async move {
                while let Some(event) = link_closed_rx.recv().await {
                    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

fn spawn_announce_listener(
    transport: Arc<reticulum_rs::runtime::Transport>,
    announces: Arc<Mutex<NativeAnnounceState>>,
    event_tx: broadcast::Sender<RuntimeBusEvent>,
) {
    tokio::spawn(async move {
        let mut receiver = transport.recv_announces().await;
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let payload = payload_from_announce_event(event).await;
                    if !should_emit_directory_announce(&payload) {
                        let _ = event_tx.send(RuntimeBusEvent::Debug(format!(
                            "native Reticulum ignored unclassified announce destination={}",
                            payload.destination_hash
                        )));
                        continue;
                    }
                    announces
                        .lock()
                        .expect("native announce state lock")
                        .ingest(payload.clone());
                    let _ = event_tx.send(RuntimeBusEvent::Announce(payload));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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
) -> AppResult<Vec<String>> {
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
        manager.spawn(
            rns_transport::iface::tcp_client::TcpClient::new(address.clone()),
            rns_transport::iface::tcp_client::TcpClient::spawn,
        );
        attached.push(format!(
            "{} tcp_client {address} ifac={}",
            interface.name,
            if interface.ifac_configured {
                "configured-in-profile-not-supported-by-reticulum-rs-transport"
            } else {
                "none"
            }
        ));
    }
    Ok(attached)
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
        self.start(identity, plan_interfaces(&interfaces))
    }

    async fn stop_runtime(&self) -> AppResult<()> {
        self.stop();
        Ok(())
    }

    async fn status(&self) -> NetworkStatus {
        let state = self.state_snapshot();
        let connected = matches!(state.lifecycle, NativeRuntimeLifecycle::Running);
        let message = match state.lifecycle {
            NativeRuntimeLifecycle::Stopped => {
                "native Reticulum adapter is configured but stopped".into()
            }
            #[cfg(feature = "native-rns-net")]
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
                #[cfg(not(feature = "native-rns-net"))]
                {
                    "native Reticulum transport is constructed; page/link/resource requests are not wired yet".to_string()
                }
                #[cfg(feature = "native-rns-net")]
                {
                    "native Reticulum transport scaffold is constructed".to_string()
                }
            }
            NativeRuntimeLifecycle::Running => "native Reticulum identity/config layer is running without an active transport identity; transport requests are not wired yet".to_string(),
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
                #[cfg(feature = "native-rns-net")]
                let transport = None;
                #[cfg(not(feature = "native-rns-net"))]
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
                    *self.transport.lock().expect("native transport lock") = transport;
                }
                #[cfg(feature = "native-rns-net")]
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
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            Err(unsupported("announce_identity"))
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
        #[cfg(feature = "native-rns-net")]
        {
            let response = self.fetch_page_with_rns_net(&plan, cancel.clone()).await?;
            let mut page = response.into_browser_page(&plan).map_err(AppError::from)?;
            page.metadata.insert(
                "native_request_backend".into(),
                serde_json::Value::String("rns-net".into()),
            );
            return Ok(page);
        }
        #[cfg(not(feature = "native-rns-net"))]
        {
            let context = self
                .transport
                .lock()
                .expect("native transport lock")
                .as_ref()
                .map(|handle| NativePageFetchContext::new(handle.transport.clone()));
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
            Ok(page)
        }
    }

    async fn download_file(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
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
        #[cfg(feature = "native-rns-net")]
        let response = self.fetch_page_with_rns_net(&plan, cancel.clone()).await?;
        #[cfg(not(feature = "native-rns-net"))]
        let response = {
            let context = self
                .transport
                .lock()
                .expect("native transport lock")
                .as_ref()
                .map(|handle| NativePageFetchContext::new(handle.transport.clone()));
            self.page_transport
                .fetch_page(&plan, context.as_ref(), cancel)
                .await?
        };
        let filename = filename_from_native_download_path(&plan.request.path);
        let path = next_available_download_path(downloads_dir, &filename)?;
        std::fs::write(&path, &response.body)?;
        Ok(DownloadedFile {
            url: plan.request.url,
            path,
            content_type: response
                .content_type
                .unwrap_or_else(|| "application/octet-stream".into()),
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

    async fn send_message(&self, envelope: MessageEnvelope) -> AppResult<MessageSummary> {
        #[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
        {
            let state = self.state_snapshot();
            if !matches!(state.lifecycle, NativeRuntimeLifecycle::Running) {
                return Err(AppError::Runtime(
                    "native Reticulum runtime must be started before sending LXMF".into(),
                ));
            }
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
            let identity_bytes = std::fs::read(&identity_path)
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
            let outbound = crate::runtime::native_lxmf::codec::build_outbound_message(
                &envelope,
                source_hash.as_str(),
            )?;
            let wire_bytes = crate::runtime::native_lxmf::codec::encode_signed_wire_message(
                &outbound,
                &identity_bytes,
            )?;
            let wire_len = wire_bytes.len();
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
                    "native LXMF opportunistic ratchet packet submitted peer={} message_id={} bytes={} state=submitted_to_rns_net",
                    envelope.peer_hash, message_id, wire_len
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
                    "native LXMF direct link packet sent peer={} message_id={} link_id={} bytes={} state=peer_unconfirmed",
                    envelope.peer_hash,
                    message_id,
                    link_hex,
                    wire_len
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
                "native LXMF direct resource advertised peer={} message_id={} link_id={} state=submitted_to_rns_net",
                envelope.peer_hash, message_id, link_hex
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
            let source_hash = state
                .active_identity
                .as_ref()
                .map(|identity| identity.address_hash_hex.as_str())
                .ok_or_else(|| AppError::from(NativeRuntimeError::IdentityMissing))?;
            let outbound =
                crate::runtime::native_lxmf::codec::build_outbound_message(&envelope, source_hash)?;
            let _transport_method =
                crate::runtime::native_lxmf::codec::app_transport_method(outbound.delivery.method);
            Err(unsupported("send_message"))
        }
        #[cfg(not(feature = "native-lxmf"))]
        {
            let _ = &envelope;
            Err(unsupported("send_message"))
        }
    }

    async fn create_contact(&self, _peer_hash: &str, _label: &str) -> AppResult<()> {
        Err(unsupported("create_contact"))
    }

    async fn recover_lxmf_correlation(
        &self,
        messages: Vec<MessageSummary>,
    ) -> AppResult<LxmfCorrelationRecovery> {
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = messages;
            Ok(LxmfCorrelationRecovery::default())
        }
    }

    async fn set_outbound_propagation_node(&self, hash: Option<String>) -> AppResult<()> {
        let hash = match hash {
            Some(hash) => {
                let parsed = parse_transport_destination_hash(&hash)?;
                #[cfg(feature = "native-rns-net")]
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
                #[cfg(not(feature = "native-rns-net"))]
                {
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
        #[cfg(feature = "native-rns-net")]
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
                let identity_bytes = std::fs::read(&identity_path)
                    .map_err(|_| AppError::from(NativeRuntimeError::IdentityMissing))?;
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
                    .unwrap_or_default();
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
                let mut cache_changed = false;
                for payload in payloads {
                    let payload_candidates = native_lxmf_payload_candidates(payload.as_slice())
                        .unwrap_or_else(|_| vec![payload.clone()]);
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
                        match crate::runtime::native_lxmf::codec::decode_propagated_lxmf_data_storing_attachments(
                            decode_data.as_slice(),
                            identity_bytes.as_slice(),
                            &self.config.attachments_dir,
                        ) {
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
                                let _ = self.event_tx.send(RuntimeBusEvent::Debug(format!(
                                    "native LXMF propagation sync payload decode failed: {error}"
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
                    "native LXMF propagation sync get response propagation_node={} payloads={} decoded={} failed={}",
                    hash,
                    payload_count,
                    decoded_count,
                    decode_failed_count
                )));
                emit_propagation_sync_event(
                    &self.event_tx,
                    PropagationSyncStage::Decode,
                    if decode_failed_count > 0 {
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
                    ],
                );
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
                    "native LXMF propagation sync complete propagation_node={} requested={} decoded={} cached_haves={} failed={}",
                    hash,
                    wants.len(),
                    decoded_count,
                    haves.len(),
                    decode_failed_count
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = parse_transport_destination_hash(&hash)?;
            Err(unsupported("sync_propagation_messages"))
        }
    }

    async fn request_path(
        &self,
        destination_hash: &str,
        reason: &str,
        sibling_aspects: bool,
    ) -> AppResult<bool> {
        #[cfg(feature = "native-rns-net")]
        let rns_net_handle = {
            let guard = self.rns_net.lock().expect("native rns-net lock");
            guard.clone()
        };
        #[cfg(feature = "native-rns-net")]
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
        let destination = parse_transport_destination_hash(destination_hash)?;
        let transport = self.active_transport()?;
        if transport.transport.knows_destination(&destination).await {
            return Ok(true);
        }
        transport
            .transport
            .request_path(&destination, None, None)
            .await;
        Ok(true)
    }

    async fn warm_paths(
        &self,
        hashes: &[String],
        max_requests: u32,
        _cooldown_secs: u64,
    ) -> AppResult<u32> {
        #[cfg(feature = "native-rns-net")]
        let rns_net_handle = {
            let guard = self.rns_net.lock().expect("native rns-net lock");
            guard.clone()
        };
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = path;
            Err(unsupported("preload_known_destinations"))
        }
    }

    async fn interface_stats(&self) -> AppResult<InterfaceStats> {
        let state = self.state_snapshot();
        #[cfg(feature = "native-rns-net")]
        let rns_net_started = state.rns_net_started;
        #[cfg(feature = "native-rns-net")]
        let live_rns_net_stats = self
            .rns_net
            .lock()
            .expect("native rns-net lock")
            .as_ref()
            .map(|handle| handle.client.clone());
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(feature = "native-rns-net")]
        let mut live_tcp_index = 0usize;
        let samples = state
            .interfaces
            .iter()
            .map(|plan| {
                let endpoint = plan
                    .endpoint
                    .as_ref()
                    .map(|endpoint| format!("{}:{}", endpoint.host, endpoint.port));
                #[cfg(feature = "native-rns-net")]
                let matched_live_interface = live_rns_net_stats.as_ref().and_then(|stats| {
                    find_live_rns_net_interface(stats, plan, endpoint.as_deref())
                });
                #[cfg(feature = "native-rns-net")]
                let ordered_live_interface =
                    if plan.enabled && plan.supported && plan.kind == "tcp_client" {
                        let interface = live_tcp_interfaces.get(live_tcp_index).copied();
                        live_tcp_index = live_tcp_index.saturating_add(1);
                        interface
                    } else {
                        None
                    };
                #[cfg(feature = "native-rns-net")]
                let live_interface =
                    select_live_rns_net_interface(matched_live_interface, ordered_live_interface);
                let attached = attached_samples.iter().any(|line| {
                    line.contains(&plan.name)
                        || endpoint
                            .as_ref()
                            .is_some_and(|endpoint| line.contains(endpoint))
                }) || {
                    #[cfg(feature = "native-rns-net")]
                    {
                        live_interface.is_some_and(|interface| interface.status)
                    }
                    #[cfg(not(feature = "native-rns-net"))]
                    {
                        false
                    }
                };
                let detail = if attached {
                    #[cfg(feature = "native-rns-net")]
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
                    #[cfg(not(feature = "native-rns-net"))]
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
                    #[cfg(feature = "native-rns-net")]
                    if let Some(interface) = live_interface {
                        Some(format_live_rns_net_interface_detail(interface))
                    } else {
                        plan.reason.clone()
                    }
                    #[cfg(not(feature = "native-rns-net"))]
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
        #[cfg(feature = "native-rns-net")]
        if let Some(stats) = &live_rns_net_stats {
            interfaces.extend(
                stats
                    .interfaces
                    .iter()
                    .map(format_live_rns_net_interface_detail),
            );
        }
        #[cfg(feature = "native-rns-net")]
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
            #[cfg(feature = "native-rns-net")]
            reason: Some(if rns_net_started {
                "native rns-net runtime is primary for browser page/path requests".into()
            } else if transport_detail.is_empty() {
                "native Reticulum transport/interface runtime is not fully wired yet".into()
            } else {
                transport_detail
            }),
            #[cfg(not(feature = "native-rns-net"))]
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
        let mut snapshot = state.snapshot();
        snapshot.connected_to_shared_instance = matches!(
            self.config.instance_mode,
            crate::runtime::native::config::NativeRuntimeMode::External
        );
        snapshot.is_shared_instance = matches!(
            self.config.instance_mode,
            crate::runtime::native::config::NativeRuntimeMode::Managed
        );
        Ok(NetworkSnapshot {
            announce_counts: snapshot.announce_counts,
            pending_announces: snapshot.pending_announces,
            known_destinations: snapshot.known_destinations,
            ratchet_announces: snapshot.ratchet_announces,
            path_table_count: 0,
            request_failures: 0,
            active_propagation_node: None,
            connected_to_shared_instance: snapshot.connected_to_shared_instance,
            is_shared_instance: snapshot.is_shared_instance,
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
        _include_propagation_usable: bool,
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
        #[cfg(feature = "native-rns-net")]
        let rns_net_handle = {
            let guard = self.rns_net.lock().expect("native rns-net lock");
            guard.clone()
        };
        #[cfg(feature = "native-rns-net")]
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
        let has_path = transport.transport.knows_destination(&destination).await;
        let known_identity = transport
            .transport
            .destination_identity(&destination)
            .await
            .is_some();
        Ok(DestinationInspection {
            destination_hash: destination_hash.into(),
            valid_length: true,
            has_path,
            hops: None,
            first_hop_timeout: None,
            known_identity,
            known_app_data: false,
            propagation_usable: None,
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
        #[cfg(not(feature = "native-rns-net"))]
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

        #[cfg(feature = "native-rns-net")]
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

        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = execute_request;
            report.steps.push(PageFetchProbeStep::failed(
                PageFetchProbeStage::RuntimeSetup,
                "native-rns-net feature is not compiled; live NomadNet page probes are unavailable",
            ));
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
            let Ok(identity_bytes) = std::fs::read(&identity_path) else {
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

        #[cfg(any(not(feature = "native-lxmf"), not(feature = "native-rns-net")))]
        {
            let _ = peer_hash;
            let _ = execute_send;
            report.steps.push(LxmfDeliveryProbeStep::failed(
                LxmfDeliveryProbeStage::RuntimeSetup,
                "native-lxmf and native-rns-net features are required for LXMF delivery probes",
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
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        if let Some(hash) = destination_hash {
            let _ = parse_transport_destination_hash(&hash)?;
            return Ok(PropagationStatus {
                selected: true,
                destination_hash: Some(hash),
                has_path: false,
                known_app_data: false,
                link_state: "unsupported".into(),
                transfer_state: "router_deferred".into(),
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

    async fn open_omenchat_link(
        &self,
        destination_hash: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<OmenChatLinkOpened> {
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = (destination_hash, timeout, cancel);
            Err(unsupported("open_omenchat_link"))
        }
    }

    async fn send_omenchat_frame(&self, link_id: [u8; 16], frame_bytes: Vec<u8>) -> AppResult<()> {
        #[cfg(feature = "native-rns-net")]
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
                let _ =
                    self.event_tx
                        .send(RuntimeBusEvent::OmenChatLinkClosed(OmenChatLinkClosed {
                            link_id,
                            reason: Some(format!("send failed: {error}")),
                        }));
            }
            result
        }
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = (link_id, frame_bytes);
            Err(unsupported("send_omenchat_frame"))
        }
    }

    async fn send_omenchat_resource(
        &self,
        link_id: [u8; 16],
        resource_id: String,
        payload: Vec<u8>,
    ) -> AppResult<()> {
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = (link_id, resource_id, payload);
            Err(unsupported("send_omenchat_resource"))
        }
    }

    async fn close_omenchat_link(&self, link_id: [u8; 16]) -> AppResult<bool> {
        #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            let _ = link_id;
            Err(unsupported("close_omenchat_link"))
        }
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn select_live_rns_net_interface<'a>(
    matched: Option<&'a rns_net::SingleInterfaceStat>,
    ordered: Option<&'a rns_net::SingleInterfaceStat>,
) -> Option<&'a rns_net::SingleInterfaceStat> {
    matched.or(ordered)
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(feature = "native-rns-net")]
fn omenchat_link_error_is_handshake_timeout(error: &AppError) -> bool {
    let lower = error.to_string().to_ascii_lowercase();
    lower.contains("timed out") && lower.contains("link establishment")
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_transient_id(lxmf_data: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(lxmf_data);
    let mut id = [0u8; 32];
    id.copy_from_slice(&digest);
    id
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
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

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
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

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_parse_transient_id_list(bytes: &[u8]) -> AppResult<Vec<[u8; 32]>> {
    let value = native_lxmf_unpack_value(bytes)?;
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

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_parse_propagation_payloads(bytes: &[u8]) -> AppResult<Vec<Vec<u8>>> {
    let value = native_lxmf_unpack_value(bytes)?;
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

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_payload_candidates(bytes: &[u8]) -> AppResult<Vec<Vec<u8>>> {
    crate::runtime::native_lxmf::codec::propagation_envelope_entries(bytes)
        .or_else(|_| Ok(vec![bytes.to_vec()]))
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_unpack_value(bytes: &[u8]) -> AppResult<rmpv::Value> {
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn rns_net_announce_payload(key: &RnsNetAnnounceKey) -> AnnouncePayload {
    let destination_hash = hex_encode(&key.destination_hash);
    let kind = rns_net_announce_kind(key);
    let app_data = key.app_data.as_deref().unwrap_or_default();
    let display_name = display_name_for_kind(&kind, app_data)
        .unwrap_or_else(|| destination_hash.chars().take(12).collect());
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
        display_name,
        kind,
        associated_hash,
        node_associated_hash,
        has_ratchet: false,
    }
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn rns_net_sibling_destination_hashes(key: &RnsNetAnnounceKey) -> [[u8; 16]; 3] {
    [
        rns_net_destination_hash(&key.identity_hash, "nomadnetwork", "node"),
        rns_net_destination_hash(&key.identity_hash, "lxmf", "delivery"),
        rns_net_destination_hash(&key.identity_hash, "lxmf", "propagation"),
    ]
}

#[cfg(feature = "native-rns-net")]
fn rns_net_associated_hash(identity_hash: &[u8; 16], app_name: &str, aspect: &str) -> String {
    hex_encode(&rns_net_destination_hash(identity_hash, app_name, aspect))
}

#[cfg(feature = "native-rns-net")]
fn rns_net_destination_hash(identity_hash: &[u8; 16], app_name: &str, aspect: &str) -> [u8; 16] {
    rns_core::destination::destination_hash(app_name, &[aspect], Some(identity_hash))
}

#[cfg(feature = "native-rns-net")]
fn parse_rns_net_destination_hash(destination_hash: &str) -> AppResult<[u8; 16]> {
    let destination = parse_transport_destination_hash(destination_hash)?;
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(destination.as_slice());
    Ok(bytes)
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn rns_net_observed_reused_page_link(steps: &[PageFetchProbeStep]) -> bool {
    steps.iter().any(|step| {
        step.stage == PageFetchProbeStage::LinkSetup
            && step.ok
            && step.detail.contains("reused active page link")
    })
}

#[cfg(feature = "native-rns-net")]
fn rns_net_observed_response_wait_failed(steps: &[PageFetchProbeStep]) -> bool {
    steps
        .iter()
        .any(|step| step.stage == PageFetchProbeStage::ResponseWait && !step.ok)
}

#[cfg(feature = "native-rns-net")]
fn rns_net_observed_request_send_failed(steps: &[PageFetchProbeStep]) -> bool {
    steps
        .iter()
        .any(|step| step.stage == PageFetchProbeStage::RequestSend && !step.ok)
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn managed_rns_net_identity_config_matches(
    reticulum_config_dir: &Path,
    identity_path: &Path,
) -> AppResult<bool> {
    let config_path = reticulum_config_dir.join("config");
    let existing = std::fs::read_to_string(config_path)?;
    Ok(read_network_identity_value(&existing).as_deref()
        == Some(&identity_path.display().to_string()))
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn decode_native_lxmf_payload(bytes: &[u8], attachments_dir: &Path) -> AppResult<MessageSummary> {
    crate::runtime::native_lxmf::codec::decode_wire_message_storing_attachments(
        bytes,
        attachments_dir,
    )
    .or_else(|direct_error| {
        let packet = rns_core::packet::RawPacket::unpack(bytes).map_err(|_| direct_error)?;
        crate::runtime::native_lxmf::codec::decode_wire_message_storing_attachments(
            &packet.data,
            attachments_dir,
        )
    })
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn native_lxmf_decode_error_is_truncated(error: &AppError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("wire message too short")
        || message.contains("failed to fill whole buffer")
        || message.contains("io error while reading marker")
}

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn decode_rns_net_lxmf_delivery(
    delivery: &RnsNetLocalDelivery,
    attachments_dir: &Path,
) -> AppResult<MessageSummary> {
    decode_native_lxmf_payload(&delivery.raw, attachments_dir)
}

#[cfg(feature = "native-rns-net")]
fn native_unix_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(feature = "native-rns-net")]
fn native_lxmf_pending_direct_summary(pending_lxmf_proofs: &PendingLxmfProofs) -> String {
    let pending = pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock");
    pending.summary(native_unix_timestamp())
}

#[cfg(feature = "native-rns-net")]
fn native_lxmf_recover_direct_correlation(
    pending_lxmf_proofs: &PendingLxmfProofs,
    messages: &[MessageSummary],
) -> usize {
    let mut pending = pending_lxmf_proofs
        .lock()
        .expect("native LXMF proof map lock");
    pending.recover_direct_correlations(messages)
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
fn lxmf_message_runtime_id(message: &MessageSummary) -> Option<String> {
    message
        .fields
        .get("native_lxmf_packet_hash")
        .or_else(|| message.fields.get("native_lxmf_message_id"))
        .cloned()
        .or_else(|| message.message_id.clone())
        .filter(|value| !value.is_empty())
}

#[cfg(feature = "native-rns-net")]
fn lxmf_message_submitted_at(message: &MessageSummary) -> Option<f64> {
    message
        .fields
        .get("native_lxmf_submitted_at")
        .and_then(|value| value.parse::<f64>().ok())
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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
    events.push(RuntimeBusEvent::Debug(format!(
        "native RNS packet proof received peer={} packet_hash={} proof_destination={} matched_pending={} rtt={:.3}",
        peer_hash, packet_hash, proof_destination, matched_pending, proof.rtt
    )));
    events
}

#[cfg(feature = "native-rns-net")]
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

#[cfg(feature = "native-rns-net")]
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

#[cfg(all(feature = "native-lxmf", feature = "native-rns-net"))]
fn local_lxmf_delivery_announce_app_data(display_name: &str) -> AppResult<Vec<u8>> {
    crate::runtime::native_lxmf::codec::encode_delivery_display_name_app_data(display_name)
}

#[cfg(all(not(feature = "native-lxmf"), feature = "native-rns-net"))]
fn local_lxmf_delivery_announce_app_data(display_name: &str) -> AppResult<Vec<u8>> {
    Ok(display_name.as_bytes().to_vec())
}

#[cfg(feature = "native-rns-net")]
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
        display_name: String::new(),
        kind: DirectoryKind::Unknown,
        associated_hash: None,
        node_associated_hash: None,
        has_ratchet: false,
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
            #[cfg(feature = "native-rns-net")]
            rns_net_started: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "native-rns-net"))]
    use crate::browser::PageSource;
    use crate::config::AppPaths;
    use crate::identity::{IdentityManager, IdentityMaterialProvider};
    use crate::runtime::native::identity::NativeReticulumIdentityProvider;
    use crate::runtime::native::interface::plan_interfaces;
    use crate::runtime::native::request::{NativePageFetchContext, NativePageResponse};

    fn temp_paths(name: &str) -> AppPaths {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-rs-native-runtime-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        AppPaths::from_root(root)
    }

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[tokio::test]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            assert!(state.transport_started);
            assert!(status.message.contains("transport is constructed"));
        }
        #[cfg(feature = "native-rns-net")]
        {
            assert!(!state.transport_started);
            assert!(state.rns_net_started);
            assert!(status.message.contains("rns-net runtime is primary"));
            assert!(status.message.contains("local_lxmf_registered=true"));
            assert!(status.message.contains("proof_capable=true"));
            assert!(status.message.contains("announced=true"));
        }
    }

    #[cfg(feature = "native-rns-net")]
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
        #[cfg(not(feature = "native-rns-net"))]
        {
            assert!(!runtime.state_snapshot().transport_started);
            assert!(status
                .message
                .contains("without an active transport identity"));
        }
        #[cfg(feature = "native-rns-net")]
        {
            assert!(runtime.state_snapshot().rns_net_started);
            assert!(status.message.contains("rns-net runtime is primary"));
        }
    }

    #[tokio::test]
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

        #[cfg(not(feature = "native-rns-net"))]
        assert!(runtime.state_snapshot().transport_started);
        #[cfg(feature = "native-rns-net")]
        assert!(runtime.state_snapshot().rns_net_started);
        #[cfg(not(feature = "native-rns-net"))]
        assert!(stats
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("transport constructed")));
        #[cfg(feature = "native-rns-net")]
        assert!(stats
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("rns-net runtime is primary")));
    }

    #[tokio::test]
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

        #[cfg(not(feature = "native-rns-net"))]
        assert!(runtime.state_snapshot().transport_started);
        #[cfg(feature = "native-rns-net")]
        assert!(runtime.state_snapshot().rns_net_started);
        #[cfg(not(feature = "native-rns-net"))]
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
        #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[tokio::test]
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

        #[cfg(not(feature = "native-rns-net"))]
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
        #[cfg(feature = "native-rns-net")]
        assert!(stats
            .interfaces
            .iter()
            .any(|line| line.contains("attached rns-net primary runtime")));
    }

    #[tokio::test]
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

    #[tokio::test]
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
            display_name: "Node".into(),
            kind: DirectoryKind::Node,
            associated_hash: Some("ffeeddccbbaa99887766554433221100".into()),
            node_associated_hash: None,
            has_ratchet: false,
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
            display_name: "Node".into(),
            kind: DirectoryKind::Node,
            associated_hash: None,
            node_associated_hash: None,
            has_ratchet: false,
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

    #[cfg(not(feature = "native-rns-net"))]
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

        #[cfg(not(feature = "native-rns-net"))]
        assert!(error.to_string().contains(
            "native Reticulum page transport needs a verified Link.request response API"
        ));
        #[cfg(feature = "native-rns-net")]
        {
            let message = error.to_string();
            assert!(message.contains("destination identity"));
            assert!(message.contains(destination));
            assert!(!message.contains(":/"));
            assert!(!message.contains("not implemented"));
            assert!(!message.contains("Unsupported"));
        }
    }

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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
        assert_eq!(status.state, OutboundDeliveryState::SubmittedToRnsNet);
        assert_eq!(
            status.evidence.as_deref(),
            Some("packet_hash:aabbccddeeff00112233445566778899;submitted_at:123.456")
        );
        assert_eq!(status.rtt, None);
    }

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
    #[test]
    fn unmatched_native_packet_proof_only_emits_debug_event() {
        let pending = Arc::new(Mutex::new(NativeDirectLxmfRouter::default()));
        let proof = RnsNetProof {
            destination_hash: [9u8; 16],
            packet_hash: [10u8; 32],
            rtt: 0.125,
        };

        let events = native_lxmf_events_for_packet_proof(&proof, &pending);

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], RuntimeBusEvent::Debug(_)));
    }

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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

    #[cfg(feature = "native-rns-net")]
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
}
