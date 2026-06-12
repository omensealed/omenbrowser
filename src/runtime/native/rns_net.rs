use std::any::type_name;
use std::collections::HashMap;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rmpv::Value;
use tokio::sync::{mpsc, Mutex};

use crate::error::{AppError, AppResult};
use crate::runtime::native::request::{
    NativeFetchPlan, NativeLinkRequestFrame, NativeLinkResponseFrame, NativePageResponse,
};
use crate::runtime::native::{NativePageFetchFailureStage, NativeRuntimeError};
use crate::runtime::network::{CancellationToken, PageFetchProbeStage, PageFetchProbeStep};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRnsNetRequestApi {
    pub node_type: &'static str,
    pub callbacks_trait: &'static str,
    pub create_link_available: bool,
    pub send_request_available: bool,
    pub response_callback_available: bool,
}

type RnsNetCreateLinkFn =
    fn(&rns_net::RnsNode, [u8; 16], [u8; 32]) -> Result<[u8; 16], rns_net::SendError>;
type RnsNetSendRequestFn =
    fn(&rns_net::RnsNode, [u8; 16], &str, &[u8]) -> Result<(), rns_net::SendError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetDestinationKeys {
    pub destination_hash: [u8; 16],
    pub signing_public_key: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetPageResponse {
    pub link_id: [u8; 16],
    pub request_id: [u8; 16],
    pub body: Vec<u8>,
    pub summary: RnsNetResponseDecodeSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RnsNetWaitedPageResponse {
    response: RnsNetPageResponse,
    source: &'static str,
    pending_buffer_before: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetResponseDecodeSummary {
    pub raw_bytes: usize,
    pub decoded_body_bytes: usize,
    pub format: &'static str,
    pub framed_request_id: Option<[u8; 16]>,
    pub request_id_matches_frame: Option<bool>,
}

impl Default for RnsNetResponseDecodeSummary {
    fn default() -> Self {
        Self {
            raw_bytes: 0,
            decoded_body_bytes: 0,
            format: "test",
            framed_request_id: None,
            request_id_matches_frame: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RnsNetPageRequestCleanup {
    pub link_torn_down: bool,
    pub path_dropped: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RnsNetLinkEstablished {
    pub link_id: [u8; 16],
    pub destination_hash: [u8; 16],
    pub rtt: f64,
    pub is_initiator: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RnsNetAnnounceKey {
    pub destination_hash: [u8; 16],
    pub identity_hash: [u8; 16],
    pub signing_public_key: [u8; 32],
    pub full_public_key: [u8; 64],
    pub app_data: Option<Vec<u8>>,
    pub hops: Option<u8>,
    pub packet_hash: Option<[u8; 32]>,
    pub observed_at: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetPathUpdate {
    pub destination_hash: [u8; 16],
    pub hops: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetLocalDelivery {
    pub destination_hash: [u8; 16],
    pub raw: Vec<u8>,
    pub packet_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq)]
pub struct RnsNetProof {
    pub destination_hash: [u8; 16],
    pub packet_hash: [u8; 32],
    pub rtt: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RnsNetResourceEvent {
    Received {
        link_id: [u8; 16],
        data: Vec<u8>,
        metadata: Option<Vec<u8>>,
    },
    Completed {
        link_id: [u8; 16],
    },
    Failed {
        link_id: [u8; 16],
        error: String,
    },
    Progress {
        link_id: [u8; 16],
        received: usize,
        total: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetLinkData {
    pub link_id: [u8; 16],
    pub context: u8,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetLinkClosed {
    pub link_id: [u8; 16],
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetLocalDestinationRegistration {
    pub destination_hash: [u8; 16],
    pub app_name: &'static str,
    pub aspect: &'static str,
    pub proof_strategy: &'static str,
    pub signing_key_supplied: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RnsNetDestinationKeyStore {
    keys: BTreeMap<[u8; 16], RnsNetAnnounceKey>,
}

#[derive(Clone)]
pub struct RnsNetPageRequestClient {
    node: Arc<rns_net::RnsNode>,
    responses: Arc<Mutex<mpsc::UnboundedReceiver<RnsNetPageResponse>>>,
    pending_responses: Arc<Mutex<VecDeque<RnsNetPageResponse>>>,
    link_events: Arc<Mutex<mpsc::UnboundedReceiver<RnsNetLinkEstablished>>>,
    active_page_link: Arc<std::sync::Mutex<Option<RnsNetCachedPageLink>>>,
}

#[derive(Clone)]
pub struct RnsNetPageCallbacks {
    response_tx: mpsc::UnboundedSender<RnsNetPageResponse>,
    link_tx: mpsc::UnboundedSender<RnsNetLinkEstablished>,
    announce_tx: Option<mpsc::UnboundedSender<RnsNetAnnounceKey>>,
    path_tx: Option<mpsc::UnboundedSender<RnsNetPathUpdate>>,
    local_delivery_tx: Option<mpsc::UnboundedSender<RnsNetLocalDelivery>>,
    proof_tx: Option<mpsc::UnboundedSender<RnsNetProof>>,
    resource_tx: Option<mpsc::UnboundedSender<RnsNetResourceEvent>>,
    link_data_tx: Option<mpsc::UnboundedSender<RnsNetLinkData>>,
    link_closed_tx: Option<mpsc::UnboundedSender<RnsNetLinkClosed>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RnsNetCachedPageLink {
    destination_hash: [u8; 16],
    signing_public_key: [u8; 32],
    link_id: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRequestBackend {
    ReticulumTransport,
    RnsNet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRequestBackendDecision {
    pub backend: NativeRequestBackend,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RnsNetRequestPayloadContract {
    pub path_sent_separately: bool,
    pub payload_shape: &'static str,
    pub none_or_empty_data_hex: &'static str,
    pub source_note: &'static str,
}

/// `reticulum-rs-transport` exposes link setup, encryption, channels, and resources, but its
/// public helpers do not expose a `PacketContext::Request` sender. `rns-net` does.
pub fn select_native_request_backend() -> NativeRequestBackendDecision {
    NativeRequestBackendDecision {
        backend: NativeRequestBackend::RnsNet,
        reason: "rns-net exposes public create_link/send_request APIs and response callbacks",
    }
}

pub fn rns_net_request_payload_contract() -> RnsNetRequestPayloadContract {
    RnsNetRequestPayloadContract {
        path_sent_separately: true,
        payload_shape: "msgpack request data value",
        none_or_empty_data_hex: "c0",
        source_note: "Python OMENbrowser calls RNS Link.request(path, data=...); rns-net send_request accepts the path separately, so OMENbrowser_rs encodes only the data value as payload.",
    }
}

pub fn native_rns_net_request_api() -> NativeRnsNetRequestApi {
    let _create_link: RnsNetCreateLinkFn = rns_net::RnsNode::create_link;
    let _send_request: RnsNetSendRequestFn = rns_net::RnsNode::send_request;

    NativeRnsNetRequestApi {
        node_type: type_name::<rns_net::RnsNode>(),
        callbacks_trait: type_name::<dyn rns_net::Callbacks>(),
        create_link_available: true,
        send_request_available: true,
        response_callback_available: true,
    }
}

pub fn write_known_destinations_fixture(
    path: &Path,
    destination_hash: [u8; 16],
) -> Result<(), NativeRuntimeError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            NativeRuntimeError::Native(format!(
                "failed to create known destinations fixture directory: {error}"
            ))
        })?;
    }
    let mut public_key = [0u8; 64];
    public_key[0..32].copy_from_slice(&[0x51; 32]);
    public_key[32..64].copy_from_slice(&[0x42; 32]);
    let mut known = HashMap::new();
    known.insert(
        destination_hash,
        rns_net::storage::KnownDestination {
            identity_hash: rns_core::hash::truncated_hash(&public_key),
            public_key,
            app_data: Some(b"OMENbrowser_rs smoke fixture".to_vec()),
            hops: 0,
            received_at: 1.0,
            receiving_interface: 0,
            was_used: false,
            last_used_at: None,
            retained: true,
        },
    );
    rns_net::storage::save_known_destinations(&known, path).map_err(|error| {
        NativeRuntimeError::Native(format!(
            "failed to write known destinations fixture: {error}"
        ))
    })
}

impl RnsNetDestinationKeys {
    pub fn from_fetch_plan(
        plan: &NativeFetchPlan,
        signing_public_key: [u8; 32],
    ) -> Result<Self, NativeRuntimeError> {
        let mut destination_hash = [0u8; 16];
        let bytes = plan.request.destination_hash.as_slice();
        if bytes.len() != destination_hash.len() {
            return Err(NativeRuntimeError::InvalidAddress(
                plan.request.destination_hash.to_hex_string(),
            ));
        }
        destination_hash.copy_from_slice(bytes);
        Ok(Self {
            destination_hash,
            signing_public_key,
        })
    }
}

impl RnsNetAnnounceKey {
    pub fn from_announced_identity(announced: &rns_net::AnnouncedIdentity) -> Self {
        Self::from_destination_public_key(
            announced.dest_hash.0,
            announced.identity_hash.0,
            announced.public_key,
            announced.app_data.clone(),
            Some(announced.hops),
            None,
            now_epoch_secs(),
        )
    }

    pub fn from_known_destination(
        destination_hash: [u8; 16],
        known: &rns_net::storage::KnownDestination,
    ) -> Self {
        Self::from_destination_public_key(
            destination_hash,
            known.identity_hash,
            known.public_key,
            known.app_data.clone(),
            Some(known.hops),
            None,
            known.last_used_at.unwrap_or(known.received_at),
        )
    }

    pub fn from_destination_public_key(
        destination_hash: [u8; 16],
        identity_hash: [u8; 16],
        public_key: [u8; 64],
        app_data: Option<Vec<u8>>,
        hops: Option<u8>,
        packet_hash: Option<[u8; 32]>,
        observed_at: f64,
    ) -> Self {
        let mut signing_public_key = [0u8; 32];
        signing_public_key.copy_from_slice(&public_key[32..64]);
        Self {
            destination_hash,
            identity_hash,
            signing_public_key,
            full_public_key: public_key,
            app_data,
            hops,
            packet_hash,
            observed_at,
        }
    }
}

impl RnsNetDestinationKeyStore {
    pub fn load_known_destinations_from_config_dir(
        config_dir: &Path,
    ) -> Result<Self, NativeRuntimeError> {
        let path = config_dir.join("storage").join("known_destinations");
        Self::load_known_destinations_file(&path)
    }

    pub fn load_recent_known_destinations_from_config_dir(
        config_dir: &Path,
        max_age_secs: f64,
    ) -> Result<Self, NativeRuntimeError> {
        let path = config_dir.join("storage").join("known_destinations");
        Self::load_recent_known_destinations_file(&path, max_age_secs)
    }

    pub fn load_known_destinations_file(path: &Path) -> Result<Self, NativeRuntimeError> {
        Self::load_known_destinations_file_filtered(path, None)
    }

    pub fn load_recent_known_destinations_file(
        path: &Path,
        max_age_secs: f64,
    ) -> Result<Self, NativeRuntimeError> {
        Self::load_known_destinations_file_filtered(path, Some(max_age_secs))
    }

    fn load_known_destinations_file_filtered(
        path: &Path,
        max_age_secs: Option<f64>,
    ) -> Result<Self, NativeRuntimeError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let known = rns_net::storage::load_known_destinations(path).map_err(|error| {
            NativeRuntimeError::Native(format!(
                "failed to load rns-net known destinations: {error}"
            ))
        })?;
        let mut store = Self::default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or_default();
        for (destination_hash, known) in known {
            if let Some(max_age_secs) = max_age_secs {
                let last_observed = known.last_used_at.unwrap_or(known.received_at);
                if last_observed <= 0.0 || now - last_observed > max_age_secs {
                    continue;
                }
            }
            store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey::from_known_destination(
                destination_hash,
                &known,
            ));
        }
        Ok(store)
    }

    pub fn save_known_destinations_file(&self, path: &Path) -> Result<(), NativeRuntimeError> {
        self.save_known_destinations_file_filtered(path, None, None)
    }

    pub fn save_recent_known_destinations_file(
        &self,
        path: &Path,
        max_age_secs: f64,
        max_entries: usize,
    ) -> Result<(), NativeRuntimeError> {
        self.save_known_destinations_file_filtered(path, Some(max_age_secs), Some(max_entries))
    }

    fn save_known_destinations_file_filtered(
        &self,
        path: &Path,
        max_age_secs: Option<f64>,
        max_entries: Option<usize>,
    ) -> Result<(), NativeRuntimeError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                NativeRuntimeError::Native(format!(
                    "failed to create managed known_destinations directory: {error}"
                ))
            })?;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or_default();
        let mut entries = self
            .keys
            .iter()
            .filter(|(_, key)| {
                let Some(max_age_secs) = max_age_secs else {
                    return true;
                };
                key.observed_at > 0.0 && now - key.observed_at <= max_age_secs
            })
            .collect::<Vec<_>>();
        entries.sort_by(|(left_hash, left), (right_hash, right)| {
            right
                .observed_at
                .total_cmp(&left.observed_at)
                .then_with(|| left_hash.cmp(right_hash))
        });
        if let Some(max_entries) = max_entries {
            entries.truncate(max_entries);
        }
        let known = entries
            .into_iter()
            .map(|(destination_hash, key)| {
                (
                    *destination_hash,
                    rns_net::storage::KnownDestination {
                        identity_hash: key.identity_hash,
                        public_key: key.full_public_key,
                        app_data: key.app_data.clone(),
                        hops: key.hops.unwrap_or(0),
                        received_at: if key.observed_at > 0.0 {
                            key.observed_at
                        } else {
                            now
                        },
                        receiving_interface: 0,
                        was_used: false,
                        last_used_at: (key.observed_at > 0.0).then_some(key.observed_at),
                        retained: true,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        rns_net::storage::save_known_destinations(&known, path).map_err(|error| {
            NativeRuntimeError::Native(format!(
                "failed to save managed known_destinations: {error}"
            ))
        })
    }

    pub fn extend(&mut self, other: Self) {
        for key in other.keys.into_values() {
            self.ingest(key);
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn ingest(&mut self, key: RnsNetAnnounceKey) {
        if let Some(existing) = self.keys.get_mut(&key.destination_hash) {
            existing.identity_hash = key.identity_hash;
            existing.signing_public_key = key.signing_public_key;
            existing.full_public_key = key.full_public_key;
            if key.app_data.is_some() {
                existing.app_data = key.app_data;
            }
            if key.hops.is_some() {
                existing.hops = key.hops;
            }
            if key.packet_hash.is_some() {
                existing.packet_hash = key.packet_hash;
            }
            existing.observed_at = existing.observed_at.max(key.observed_at);
            return;
        }
        self.keys.insert(key.destination_hash, key);
    }

    pub fn ingest_with_nomadnet_lxmf_siblings(&mut self, key: RnsNetAnnounceKey) {
        let siblings = nomadnet_lxmf_sibling_destination_keys(&key);
        self.ingest(key);
        for sibling in siblings {
            self.ingest(sibling);
        }
    }

    pub fn signing_public_key(&self, destination_hash: &[u8; 16]) -> Option<[u8; 32]> {
        self.keys
            .get(destination_hash)
            .map(|key| key.signing_public_key)
    }

    pub fn destination_key(&self, destination_hash: &[u8; 16]) -> Option<RnsNetAnnounceKey> {
        self.keys.get(destination_hash).cloned()
    }

    pub fn sibling_destination_hashes(&self, destination_hash: &[u8; 16]) -> Vec<[u8; 16]> {
        let Some(key) = self.keys.get(destination_hash) else {
            return Vec::new();
        };
        [
            rns_destination_hash(&key.identity_hash, "nomadnetwork", "node"),
            rns_destination_hash(&key.identity_hash, "lxmf", "delivery"),
            rns_destination_hash(&key.identity_hash, "lxmf", "propagation"),
        ]
        .into_iter()
        .filter(|hash| hash != destination_hash)
        .collect()
    }

    pub fn values(&self) -> impl Iterator<Item = &RnsNetAnnounceKey> {
        self.keys.values()
    }
}

fn nomadnet_lxmf_sibling_destination_keys(key: &RnsNetAnnounceKey) -> Vec<RnsNetAnnounceKey> {
    let family = [
        rns_destination_hash(&key.identity_hash, "nomadnetwork", "node"),
        rns_destination_hash(&key.identity_hash, "lxmf", "delivery"),
        rns_destination_hash(&key.identity_hash, "lxmf", "propagation"),
    ];
    if !family.contains(&key.destination_hash) {
        return Vec::new();
    }
    family
        .into_iter()
        .filter(|hash| *hash != key.destination_hash)
        .map(|destination_hash| RnsNetAnnounceKey {
            destination_hash,
            identity_hash: key.identity_hash,
            signing_public_key: key.signing_public_key,
            full_public_key: key.full_public_key,
            // The sibling key is valid for signing, but its app-data was not actually announced.
            app_data: None,
            hops: key.hops,
            packet_hash: key.packet_hash,
            observed_at: key.observed_at,
        })
        .collect()
}

fn now_epoch_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

fn rns_destination_hash(identity_hash: &[u8; 16], app_name: &str, aspect: &str) -> [u8; 16] {
    rns_core::destination::destination_hash(app_name, &[aspect], Some(identity_hash))
}

impl RnsNetPageRequestClient {
    pub fn from_started_node(
        node: rns_net::RnsNode,
        response_rx: mpsc::UnboundedReceiver<RnsNetPageResponse>,
        link_rx: mpsc::UnboundedReceiver<RnsNetLinkEstablished>,
    ) -> Self {
        Self {
            node: Arc::new(node),
            responses: Arc::new(Mutex::new(response_rx)),
            pending_responses: Arc::new(Mutex::new(VecDeque::new())),
            link_events: Arc::new(Mutex::new(link_rx)),
            active_page_link: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn new(node: rns_net::RnsNode) -> (Self, RnsNetPageCallbacks) {
        Self::new_with_announce_sink(node, None)
    }

    pub fn new_with_announce_sink(
        node: rns_net::RnsNode,
        announce_tx: Option<mpsc::UnboundedSender<RnsNetAnnounceKey>>,
    ) -> (Self, RnsNetPageCallbacks) {
        let (response_tx, response_rx) = mpsc::unbounded_channel();
        let (link_tx, link_rx) = mpsc::unbounded_channel();
        (
            Self {
                node: Arc::new(node),
                responses: Arc::new(Mutex::new(response_rx)),
                pending_responses: Arc::new(Mutex::new(VecDeque::new())),
                link_events: Arc::new(Mutex::new(link_rx)),
                active_page_link: Arc::new(std::sync::Mutex::new(None)),
            },
            RnsNetPageCallbacks {
                response_tx,
                link_tx,
                announce_tx,
                path_tx: None,
                local_delivery_tx: None,
                proof_tx: None,
                resource_tx: None,
                link_data_tx: None,
                link_closed_tx: None,
            },
        )
    }

    pub async fn interface_stats(&self) -> AppResult<rns_net::InterfaceStatsResponse> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || node.query(rns_net::QueryRequest::InterfaceStats))
            .await
            .map_err(|error| AppError::Runtime(format!("rns-net interface query failed: {error}")))?
            .map_err(|error| {
                AppError::Runtime(format!("rns-net interface query failed: {error:?}"))
            })
            .and_then(|response| match response {
                rns_net::QueryResponse::InterfaceStats(stats) => Ok(stats),
                other => Err(AppError::Runtime(format!(
                    "rns-net interface query returned unexpected response: {other:?}"
                ))),
            })
    }

    pub async fn fetch_page(
        &self,
        plan: &NativeFetchPlan,
        keys: RnsNetDestinationKeys,
        identify_private_key: Option<[u8; 64]>,
        cancel: CancellationToken,
    ) -> AppResult<NativePageResponse> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        let reuse_page_link = request_allows_page_link_reuse(plan);
        let link_id = if reuse_page_link {
            self.cached_page_link(&keys)
        } else {
            None
        };
        let link_id = if let Some(link_id) = link_id {
            let _ = identify_private_key;
            link_id
        } else {
            let node = self.node.clone();
            let link_id = tokio::task::spawn_blocking(move || {
                node.create_link(keys.destination_hash, keys.signing_public_key)
            })
            .await
            .map_err(|error| {
                page_fetch_error(
                    plan,
                    NativePageFetchFailureStage::LinkSetup,
                    format!("rns-net create_link task failed: {error}"),
                )
            })?
            .map_err(|_| {
                page_fetch_error(
                    plan,
                    NativePageFetchFailureStage::LinkSetup,
                    "rns-net failed to create page request link",
                )
            })?;

            if let Err(error) = self
                .wait_for_link_established(plan, link_id, plan.timeout, cancel.clone())
                .await
            {
                self.cleanup_failed_page_request(link_id, keys.destination_hash)
                    .await;
                return Err(error);
            }
            if let Some(identity_key) = identify_private_key {
                if let Err(error) = self.identify_link(link_id, identity_key).await {
                    self.cleanup_failed_page_request(link_id, keys.destination_hash)
                        .await;
                    return Err(page_fetch_error(
                        plan,
                        NativePageFetchFailureStage::LinkSetup,
                        format!("rns-net failed to identify on page link: {error}"),
                    ));
                }
            }
            if reuse_page_link {
                self.remember_page_link(&keys, link_id);
            }
            link_id
        };

        let payload = encode_request_data(plan.request.request_data.as_ref())?;
        let request_frame_bytes = request_frame_bytes(plan);
        if self
            .node
            .send_request(link_id, &plan.request.path, &payload)
            .is_err()
        {
            self.cleanup_failed_page_request(link_id, keys.destination_hash)
                .await;
            return Err(page_fetch_error(
                plan,
                NativePageFetchFailureStage::RequestSend,
                request_send_failed_detail(request_frame_bytes),
            ));
        }

        let response = self
            .wait_for_response(plan, link_id, plan.timeout, cancel)
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) if request_data_is_micronplus_defaults_only(plan) => {
                let fallback_payload = encode_request_data(None)?;
                if self
                    .node
                    .send_request(link_id, &plan.request.path, &fallback_payload)
                    .is_err()
                {
                    self.cleanup_failed_page_request(link_id, keys.destination_hash)
                        .await;
                    return Err(error);
                }
                match self
                    .wait_for_response(plan, link_id, plan.timeout, CancellationToken::new())
                    .await
                {
                    Ok(response) => response,
                    Err(_) => {
                        self.cleanup_failed_page_request(link_id, keys.destination_hash)
                            .await;
                        return Err(error);
                    }
                }
            }
            Err(error) => {
                self.cleanup_failed_page_request(link_id, keys.destination_hash)
                    .await;
                return Err(error);
            }
        };
        let response = response.response;
        Ok(NativePageResponse {
            body: response.body,
            content_type: Some("text/micron".into()),
        })
    }

    pub async fn fetch_page_observed(
        &self,
        plan: &NativeFetchPlan,
        keys: RnsNetDestinationKeys,
        identify_private_key: Option<[u8; 64]>,
        cancel: CancellationToken,
    ) -> (Vec<PageFetchProbeStep>, Option<NativePageResponse>) {
        let mut steps = Vec::new();
        if cancel.is_cancelled() {
            steps.push(
                PageFetchProbeStep::failed(
                    PageFetchProbeStage::LinkSetup,
                    "request cancelled before rns-net link setup",
                )
                .with_trace("destination", plan.request.destination_hash.to_hex_string())
                .with_trace("path", plan.request.path.clone()),
            );
            return (steps, None);
        }

        let destination_hex = plan.request.destination_hash.to_hex_string();
        let reuse_page_link = request_allows_page_link_reuse(plan);
        let link_id = if reuse_page_link {
            self.cached_page_link(&keys)
        } else {
            None
        };
        let link_id = if let Some(link_id) = link_id {
            steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::LinkSetup,
                    "rns-net reused active page link",
                )
                .with_trace("destination", destination_hex.clone())
                .with_trace("link_id", hex_bytes(&link_id)),
            );
            if identify_private_key.is_some() {
                steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::LinkSetup,
                        "rns-net reused cached page link without re-identifying",
                    )
                    .with_trace("link_id", hex_bytes(&link_id))
                    .with_trace("identify_on_connect", "already-established"),
                );
            }
            link_id
        } else {
            if !reuse_page_link {
                steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::LinkSetup,
                        "rns-net form/request-data fetch uses a fresh page link",
                    )
                    .with_trace("destination", destination_hex.clone())
                    .with_trace("request_data_count", request_data_count(plan).to_string())
                    .with_trace("request_data_keys", request_data_keys(plan).join(",")),
                );
            }
            let node = self.node.clone();
            let link_id = match tokio::task::spawn_blocking(move || {
                node.create_link(keys.destination_hash, keys.signing_public_key)
            })
            .await
            {
                Ok(Ok(link_id)) => {
                    steps.push(
                        PageFetchProbeStep::ok(
                            PageFetchProbeStage::LinkSetup,
                            "rns-net link request queued",
                        )
                        .with_trace("destination", destination_hex.clone())
                        .with_trace("link_id", hex_bytes(&link_id)),
                    );
                    link_id
                }
                Ok(Err(_)) => {
                    steps.push(
                        PageFetchProbeStep::failed(
                            PageFetchProbeStage::LinkSetup,
                            "rns-net failed to create page request link",
                        )
                        .with_trace("destination", destination_hex.clone()),
                    );
                    return (steps, None);
                }
                Err(error) => {
                    steps.push(
                        PageFetchProbeStep::failed(
                            PageFetchProbeStage::LinkSetup,
                            format!("rns-net create_link task failed: {error}"),
                        )
                        .with_trace("destination", destination_hex.clone()),
                    );
                    return (steps, None);
                }
            };

            match self
                .wait_for_link_established(plan, link_id, plan.timeout, cancel.clone())
                .await
            {
                Ok(established) => {
                    let established_link_id = established.link_id;
                    if reuse_page_link {
                        self.remember_page_link(&keys, link_id);
                    }
                    steps.push(
                        PageFetchProbeStep::ok(
                            PageFetchProbeStage::LinkSetup,
                            "rns-net link established",
                        )
                        .with_trace("destination", hex_bytes(&established.destination_hash))
                        .with_trace("link_id", hex_bytes(&established_link_id))
                        .with_trace("rtt", format!("{:.3}", established.rtt))
                        .with_trace("initiator", established.is_initiator.to_string()),
                    );
                    if let Some(identity_key) = identify_private_key {
                        match self.identify_link(link_id, identity_key).await {
                            Ok(()) => steps.push(
                                PageFetchProbeStep::ok(
                                    PageFetchProbeStage::LinkSetup,
                                    "rns-net identified local identity on page link",
                                )
                                .with_trace("link_id", hex_bytes(&established_link_id))
                                .with_trace("identify_on_connect", "true"),
                            ),
                            Err(error) => {
                                steps.push(
                                    PageFetchProbeStep::failed(
                                        PageFetchProbeStage::LinkSetup,
                                        format!("rns-net failed to identify on page link: {error}"),
                                    )
                                    .with_trace("link_id", hex_bytes(&established_link_id))
                                    .with_trace("identify_on_connect", "true"),
                                );
                                return (steps, None);
                            }
                        }
                    }
                }
                Err(error) => {
                    let cleanup = self
                        .cleanup_failed_page_request(link_id, keys.destination_hash)
                        .await;
                    steps.push(
                        PageFetchProbeStep::failed(
                            PageFetchProbeStage::LinkSetup,
                            error.to_string(),
                        )
                        .with_trace("destination", destination_hex.clone())
                        .with_trace("link_id", hex_bytes(&link_id)),
                    );
                    steps.push(cleanup_probe_step(cleanup));
                    return (steps, None);
                }
            }
            link_id
        };

        let payload = match encode_request_data(plan.request.request_data.as_ref()) {
            Ok(payload) => payload,
            Err(error) => {
                steps.push(
                    PageFetchProbeStep::failed(
                        PageFetchProbeStage::RequestSend,
                        AppError::from(error).to_string(),
                    )
                    .with_trace("destination", destination_hex.clone())
                    .with_trace("path", plan.request.path.clone())
                    .with_trace("link_id", hex_bytes(&link_id)),
                );
                return (steps, None);
            }
        };
        let payload_len = payload.len();
        let request_frame_bytes = request_frame_bytes(plan);
        match self
            .node
            .send_request(link_id, &plan.request.path, &payload)
        {
            Ok(()) => steps.push(
                PageFetchProbeStep::ok(
                    PageFetchProbeStage::RequestSend,
                    "rns-net page request sent",
                )
                .with_trace("destination", destination_hex.clone())
                .with_trace("path", plan.request.path.clone())
                .with_trace("link_id", hex_bytes(&link_id))
                .with_trace("payload_bytes", payload_len.to_string())
                .with_trace(
                    "request_frame_bytes",
                    request_frame_bytes
                        .map(|bytes| bytes.to_string())
                        .unwrap_or_else(|| "unknown".into()),
                )
                .with_trace("request_data_count", request_data_count(plan).to_string())
                .with_trace("request_data_keys", request_data_keys(plan).join(",")),
            ),
            Err(_) => {
                let cleanup = self
                    .cleanup_failed_page_request(link_id, keys.destination_hash)
                    .await;
                steps.push(
                    PageFetchProbeStep::failed(
                        PageFetchProbeStage::RequestSend,
                        "rns-net failed to send page request",
                    )
                    .with_trace("destination", destination_hex.clone())
                    .with_trace("path", plan.request.path.clone())
                    .with_trace("link_id", hex_bytes(&link_id))
                    .with_trace("payload_bytes", payload_len.to_string())
                    .with_trace(
                        "request_frame_bytes",
                        request_frame_bytes
                            .map(|bytes| bytes.to_string())
                            .unwrap_or_else(|| "unknown".into()),
                    ),
                );
                steps.push(cleanup_probe_step(cleanup));
                return (steps, None);
            }
        }

        match self
            .wait_for_response(plan, link_id, plan.timeout, cancel)
            .await
        {
            Ok(waited) => {
                let response = waited.response;
                let request_id = hex_bytes(&response.request_id);
                let body_len = response.body.len();
                let mut response_step = PageFetchProbeStep::ok(
                    PageFetchProbeStage::ResponseWait,
                    "rns-net page response received",
                )
                .with_trace("link_id", hex_bytes(&response.link_id))
                .with_trace("request_id", request_id.clone())
                .with_trace("response_source", waited.source)
                .with_trace(
                    "pending_response_buffer_before",
                    waited.pending_buffer_before.to_string(),
                )
                .with_trace("response_bytes", body_len.to_string())
                .with_trace("raw_response_bytes", response.summary.raw_bytes.to_string())
                .with_trace("response_format", response.summary.format)
                .with_trace(
                    "decoded_body_bytes",
                    response.summary.decoded_body_bytes.to_string(),
                );
                if let Some(framed_request_id) = response.summary.framed_request_id {
                    response_step = response_step
                        .with_trace("framed_request_id", hex_bytes(&framed_request_id));
                }
                if let Some(matches_frame) = response.summary.request_id_matches_frame {
                    response_step = response_step
                        .with_trace("request_id_matches_frame", matches_frame.to_string());
                }
                steps.push(response_step);
                let page = NativePageResponse {
                    body: response.body,
                    content_type: Some("text/micron".into()),
                };
                if std::str::from_utf8(&page.body).is_err() {
                    steps.push(
                        PageFetchProbeStep::failed(
                            PageFetchProbeStage::ResponseDecode,
                            "response body was not valid UTF-8 Micron text",
                        )
                        .with_trace("link_id", hex_bytes(&link_id))
                        .with_trace("request_id", request_id)
                        .with_trace("body_bytes", page.body.len().to_string())
                        .with_trace("content_type", "text/micron"),
                    );
                    return (steps, None);
                }
                steps.push(
                    PageFetchProbeStep::ok(
                        PageFetchProbeStage::ResponseDecode,
                        format!("response body decoded ({} bytes)", page.body.len()),
                    )
                    .with_trace("link_id", hex_bytes(&link_id))
                    .with_trace("request_id", request_id)
                    .with_trace("body_bytes", page.body.len().to_string())
                    .with_trace("content_type", "text/micron"),
                );
                (steps, Some(page))
            }
            Err(error) => {
                if request_data_is_micronplus_defaults_only(plan) {
                    steps.push(
                        PageFetchProbeStep::ok(
                            PageFetchProbeStage::RequestSend,
                            "rns-net retrying page request without MicronPlus detection vars after response timeout",
                        )
                        .with_trace("destination", destination_hex.clone())
                        .with_trace("path", plan.request.path.clone())
                        .with_trace("link_id", hex_bytes(&link_id))
                        .with_trace("fallback_reason", error.to_string()),
                    );
                    let fallback_payload = match encode_request_data(None) {
                        Ok(payload) => payload,
                        Err(fallback_error) => {
                            let cleanup = self
                                .cleanup_failed_page_request(link_id, keys.destination_hash)
                                .await;
                            steps.push(
                                PageFetchProbeStep::failed(
                                    PageFetchProbeStage::RequestSend,
                                    AppError::from(fallback_error).to_string(),
                                )
                                .with_trace("destination", destination_hex.clone())
                                .with_trace("path", plan.request.path.clone())
                                .with_trace("link_id", hex_bytes(&link_id)),
                            );
                            steps.push(cleanup_probe_step(cleanup));
                            return (steps, None);
                        }
                    };
                    match self
                        .node
                        .send_request(link_id, &plan.request.path, &fallback_payload)
                    {
                        Ok(()) => steps.push(
                            PageFetchProbeStep::ok(
                                PageFetchProbeStage::RequestSend,
                                "rns-net fallback page request sent without MicronPlus detection vars",
                            )
                            .with_trace("destination", destination_hex.clone())
                            .with_trace("path", plan.request.path.clone())
                            .with_trace("link_id", hex_bytes(&link_id))
                            .with_trace("payload_bytes", fallback_payload.len().to_string())
                            .with_trace("request_data_count", "0")
                            .with_trace("request_data_keys", ""),
                        ),
                        Err(_) => {
                            let cleanup = self
                                .cleanup_failed_page_request(link_id, keys.destination_hash)
                                .await;
                            steps.push(
                                PageFetchProbeStep::failed(
                                    PageFetchProbeStage::RequestSend,
                                    "rns-net failed to send fallback page request",
                                )
                                .with_trace("destination", destination_hex.clone())
                                .with_trace("path", plan.request.path.clone())
                                .with_trace("link_id", hex_bytes(&link_id))
                                .with_trace("payload_bytes", fallback_payload.len().to_string()),
                            );
                            steps.push(cleanup_probe_step(cleanup));
                            return (steps, None);
                        }
                    }
                    match self
                        .wait_for_response(plan, link_id, plan.timeout, CancellationToken::new())
                        .await
                    {
                        Ok(waited) => {
                            let response = waited.response;
                            let request_id = hex_bytes(&response.request_id);
                            let body_len = response.body.len();
                            steps.push(
                                PageFetchProbeStep::ok(
                                    PageFetchProbeStage::ResponseWait,
                                    "rns-net fallback page response received",
                                )
                                .with_trace("link_id", hex_bytes(&response.link_id))
                                .with_trace("request_id", request_id.clone())
                                .with_trace("response_source", waited.source)
                                .with_trace(
                                    "pending_response_buffer_before",
                                    waited.pending_buffer_before.to_string(),
                                )
                                .with_trace("response_bytes", body_len.to_string())
                                .with_trace(
                                    "raw_response_bytes",
                                    response.summary.raw_bytes.to_string(),
                                )
                                .with_trace("response_format", response.summary.format)
                                .with_trace(
                                    "decoded_body_bytes",
                                    response.summary.decoded_body_bytes.to_string(),
                                )
                                .with_trace("micronplus_detection_fallback", "true"),
                            );
                            let page = NativePageResponse {
                                body: response.body,
                                content_type: Some("text/micron".into()),
                            };
                            if std::str::from_utf8(&page.body).is_err() {
                                steps.push(
                                    PageFetchProbeStep::failed(
                                        PageFetchProbeStage::ResponseDecode,
                                        "fallback response body was not valid UTF-8 Micron text",
                                    )
                                    .with_trace("link_id", hex_bytes(&link_id))
                                    .with_trace("request_id", request_id)
                                    .with_trace("body_bytes", page.body.len().to_string())
                                    .with_trace("content_type", "text/micron"),
                                );
                                return (steps, None);
                            }
                            steps.push(
                                PageFetchProbeStep::ok(
                                    PageFetchProbeStage::ResponseDecode,
                                    format!(
                                        "fallback response body decoded ({} bytes)",
                                        page.body.len()
                                    ),
                                )
                                .with_trace("link_id", hex_bytes(&link_id))
                                .with_trace("request_id", request_id)
                                .with_trace("body_bytes", page.body.len().to_string())
                                .with_trace("content_type", "text/micron")
                                .with_trace("micronplus_detection_fallback", "true"),
                            );
                            return (steps, Some(page));
                        }
                        Err(fallback_error) => {
                            steps.push(
                                PageFetchProbeStep::failed(
                                    PageFetchProbeStage::ResponseWait,
                                    fallback_error.to_string(),
                                )
                                .with_trace("link_id", hex_bytes(&link_id))
                                .with_trace("timeout_secs", plan.timeout.as_secs().to_string())
                                .with_trace("micronplus_detection_fallback", "true"),
                            );
                        }
                    }
                }
                let cleanup = self
                    .cleanup_failed_page_request(link_id, keys.destination_hash)
                    .await;
                steps.push(
                    PageFetchProbeStep::failed(
                        PageFetchProbeStage::ResponseWait,
                        error.to_string(),
                    )
                    .with_trace("link_id", hex_bytes(&link_id))
                    .with_trace("timeout_secs", plan.timeout.as_secs().to_string()),
                );
                steps.push(cleanup_probe_step(cleanup));
                (steps, None)
            }
        }
    }

    fn cached_page_link(&self, keys: &RnsNetDestinationKeys) -> Option<[u8; 16]> {
        let cached = self
            .active_page_link
            .lock()
            .expect("rns-net active page link lock");
        cached_page_link_for(keys, cached.as_ref())
    }

    fn remember_page_link(&self, keys: &RnsNetDestinationKeys, link_id: [u8; 16]) {
        *self
            .active_page_link
            .lock()
            .expect("rns-net active page link lock") = Some(RnsNetCachedPageLink {
            destination_hash: keys.destination_hash,
            signing_public_key: keys.signing_public_key,
            link_id,
        });
    }

    fn forget_page_link(&self, link_id: [u8; 16]) {
        let mut cached = self
            .active_page_link
            .lock()
            .expect("rns-net active page link lock");
        if cached
            .as_ref()
            .is_some_and(|cached| cached.link_id == link_id)
        {
            *cached = None;
        }
    }

    pub async fn reset_cached_page_link_for_destination(
        &self,
        destination_hash: [u8; 16],
    ) -> RnsNetPageRequestCleanup {
        let link_id = {
            let mut cached = self
                .active_page_link
                .lock()
                .expect("rns-net active page link lock");
            if cached
                .as_ref()
                .is_some_and(|cached| cached.destination_hash == destination_hash)
            {
                cached.take().map(|cached| cached.link_id)
            } else {
                None
            }
        };
        if let Some(link_id) = link_id {
            self.cleanup_failed_page_request(link_id, destination_hash)
                .await
        } else {
            RnsNetPageRequestCleanup::default()
        }
    }

    async fn cleanup_failed_page_request(
        &self,
        link_id: [u8; 16],
        destination_hash: [u8; 16],
    ) -> RnsNetPageRequestCleanup {
        self.forget_page_link(link_id);
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || {
            let link_torn_down = node.teardown_link(link_id).is_ok();
            let _ = destination_hash;
            // Python OMENbrowser does not discard a known destination path after one
            // request timeout. Keeping the path lets the next page request rebuild a
            // fresh link without forcing path discovery again.
            let path_dropped = false;
            RnsNetPageRequestCleanup {
                link_torn_down,
                path_dropped,
            }
        })
        .await
        .unwrap_or_default()
    }

    pub async fn recall_destination_key(
        &self,
        destination_hash: [u8; 16],
    ) -> AppResult<Option<RnsNetAnnounceKey>> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || {
            node.recall_identity(&rns_core::types::DestHash(destination_hash))
        })
        .await
        .map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "rns-net recall_identity task failed: {error}"
            )))
        })?
        .map(|announced| {
            announced.map(|announced| RnsNetAnnounceKey::from_announced_identity(&announced))
        })
        .map_err(|_| {
            AppError::from(NativeRuntimeError::Native(
                "rns-net failed to recall destination identity".into(),
            ))
        })
    }

    pub fn inject_destination_identity(&self, key: RnsNetAnnounceKey) -> AppResult<bool> {
        self.node
            .query(rns_net::QueryRequest::InjectIdentity {
                dest_hash: key.destination_hash,
                identity_hash: key.identity_hash,
                public_key: key.full_public_key,
                app_data: key.app_data.clone(),
                hops: key.hops.unwrap_or(0),
                received_at: rns_net::time::now(),
            })
            .map(|response| matches!(response, rns_net::QueryResponse::InjectIdentity(true)))
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to inject known destination identity".into(),
                ))
            })
    }

    pub async fn request_path(&self, destination_hash: [u8; 16]) -> AppResult<()> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || {
            node.request_path(&rns_core::types::DestHash(destination_hash))
        })
        .await
        .map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "rns-net request_path task failed: {error}"
            )))
        })?
        .map_err(|_| {
            AppError::from(NativeRuntimeError::Native(
                "rns-net failed to request path".into(),
            ))
        })
    }

    pub async fn has_path(&self, destination_hash: [u8; 16]) -> AppResult<bool> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || {
            node.has_path(&rns_core::types::DestHash(destination_hash))
        })
        .await
        .map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "rns-net has_path task failed: {error}"
            )))
        })?
        .map_err(|_| {
            AppError::from(NativeRuntimeError::Native(
                "rns-net failed to inspect path".into(),
            ))
        })
    }

    pub async fn hops_to(&self, destination_hash: [u8; 16]) -> AppResult<Option<u8>> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || {
            node.hops_to(&rns_core::types::DestHash(destination_hash))
        })
        .await
        .map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "rns-net hops_to task failed: {error}"
            )))
        })?
        .map_err(|_| {
            AppError::from(NativeRuntimeError::Native(
                "rns-net failed to inspect hops".into(),
            ))
        })
    }

    pub async fn send_single_packet(
        &self,
        destination: RnsNetAnnounceKey,
        app_name: &'static str,
        aspect: &'static str,
        payload: Vec<u8>,
    ) -> AppResult<[u8; 32]> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || {
            let announced = rns_net::AnnouncedIdentity {
                dest_hash: rns_core::types::DestHash(destination.destination_hash),
                identity_hash: rns_core::types::IdentityHash(destination.identity_hash),
                public_key: destination.full_public_key,
                app_data: destination.app_data,
                hops: 0,
                received_at: 0.0,
                receiving_interface: rns_core::transport::types::InterfaceId(0),
                rssi: None,
                snr: None,
            };
            let destination = rns_net::Destination::single_out(app_name, &[aspect], &announced);
            if destination.hash.0 != announced.dest_hash.0 {
                return Err(NativeRuntimeError::InvalidAddress(hex_bytes(
                    &announced.dest_hash.0,
                )));
            }
            node.send_packet(&destination, &payload)
                .map(|packet_hash| packet_hash.0)
                .map_err(|_| NativeRuntimeError::Native("rns-net failed to send packet".into()))
        })
        .await
        .map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "rns-net send packet task failed: {error}"
            )))
        })?
        .map_err(AppError::from)
    }

    pub async fn establish_link(
        &self,
        destination_hash: [u8; 16],
        signing_public_key: [u8; 32],
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<RnsNetLinkEstablished> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        let node = self.node.clone();
        let link_id = tokio::task::spawn_blocking(move || {
            node.create_link(destination_hash, signing_public_key)
        })
        .await
        .map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "rns-net create link task failed: {error}"
            )))
        })?
        .map_err(|_| {
            AppError::from(NativeRuntimeError::Native(
                "rns-net failed to create link".into(),
            ))
        })?;

        match self.wait_for_link_id(link_id, timeout, cancel).await {
            Ok(link) => Ok(link),
            Err(error) => {
                let _ = self.teardown_link(link_id).await;
                Err(error)
            }
        }
    }

    pub async fn teardown_link(&self, link_id: [u8; 16]) -> bool {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || node.teardown_link(link_id).is_ok())
            .await
            .unwrap_or(false)
    }

    pub async fn send_resource(
        &self,
        link_id: [u8; 16],
        data: Vec<u8>,
        metadata: Option<Vec<u8>>,
    ) -> AppResult<()> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || node.send_resource(link_id, data, metadata))
            .await
            .map_err(|error| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "rns-net send propagation resource task failed: {error}"
                )))
            })?
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to send propagation resource".into(),
                ))
            })
    }

    pub async fn send_on_link(
        &self,
        link_id: [u8; 16],
        data: Vec<u8>,
        context: u8,
    ) -> AppResult<()> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || node.send_on_link(link_id, data, context))
            .await
            .map_err(|error| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "rns-net send link packet task failed: {error}"
                )))
            })?
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to send link packet".into(),
                ))
            })
    }

    pub async fn identify_link(
        &self,
        link_id: [u8; 16],
        identity_private_key: [u8; 64],
    ) -> AppResult<()> {
        let node = self.node.clone();
        tokio::task::spawn_blocking(move || node.identify_on_link(link_id, identity_private_key))
            .await
            .map_err(|error| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "rns-net identify link task failed: {error}"
                )))
            })?
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to identify on link".into(),
                ))
            })
    }

    pub async fn send_request_value_and_wait(
        &self,
        link_id: [u8; 16],
        path: &str,
        value: &Value,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<RnsNetPageResponse> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let payload = pack_value(value).map_err(AppError::from)?;
        self.node
            .send_request(link_id, path, &payload)
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "rns-net failed to send link request path={path}"
                )))
            })?;
        self.wait_for_link_response(link_id, timeout, cancel).await
    }

    pub fn register_single_destination(&self, destination_hash: [u8; 16]) -> AppResult<()> {
        self.node
            .register_destination(destination_hash, rns_core::constants::DESTINATION_SINGLE)
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to register local single destination".into(),
                ))
            })
    }

    pub fn register_single_destination_with_proof(
        &self,
        identity_hash: [u8; 16],
        app_name: &'static str,
        aspect: &'static str,
        signing_key: [u8; 64],
    ) -> AppResult<RnsNetLocalDestinationRegistration> {
        let destination = rns_net::Destination::single_in(
            app_name,
            &[aspect],
            rns_core::types::IdentityHash(identity_hash),
        )
        .set_proof_strategy(rns_core::types::ProofStrategy::ProveAll);
        let destination_hash = destination.hash.0;
        self.node
            .register_destination_with_proof(&destination, Some(signing_key))
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to register local single destination proof strategy".into(),
                ))
            })?;
        Ok(RnsNetLocalDestinationRegistration {
            destination_hash,
            app_name,
            aspect,
            proof_strategy: "prove_all",
            signing_key_supplied: true,
        })
    }

    pub fn register_link_destination(
        &self,
        destination_hash: [u8; 16],
        identity_private_key: [u8; 64],
    ) -> AppResult<()> {
        let identity = crate::runtime::native::identity::rns_net_identity_from_signing_key(
            &identity_private_key,
        );
        let public_key = identity.get_public_key().ok_or_else(|| {
            AppError::from(NativeRuntimeError::Native(
                "rns-net failed to derive local link destination public key".into(),
            ))
        })?;
        let mut signing_private_key = [0u8; 32];
        let mut signing_public_key = [0u8; 32];
        signing_private_key.copy_from_slice(&identity_private_key[32..64]);
        signing_public_key.copy_from_slice(&public_key[32..64]);
        self.node
            .register_link_destination(destination_hash, signing_private_key, signing_public_key, 1)
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to register local link destination".into(),
                ))
            })
    }

    pub fn announce_single_destination(
        &self,
        identity_hash: [u8; 16],
        app_name: &'static str,
        aspect: &'static str,
        signing_key: [u8; 64],
        app_data: Option<&[u8]>,
    ) -> AppResult<RnsNetLocalDestinationRegistration> {
        let destination = rns_net::Destination::single_in(
            app_name,
            &[aspect],
            rns_core::types::IdentityHash(identity_hash),
        );
        let destination_hash = destination.hash.0;
        let identity =
            crate::runtime::native::identity::rns_net_identity_from_signing_key(&signing_key);
        self.node
            .announce(&destination, &identity, app_data)
            .map_err(|_| {
                AppError::from(NativeRuntimeError::Native(
                    "rns-net failed to announce local single destination".into(),
                ))
            })?;
        Ok(RnsNetLocalDestinationRegistration {
            destination_hash,
            app_name,
            aspect,
            proof_strategy: "announce_only",
            signing_key_supplied: true,
        })
    }

    async fn wait_for_response(
        &self,
        plan: &NativeFetchPlan,
        link_id: [u8; 16],
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<RnsNetWaitedPageResponse> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut responses = self.responses.lock().await;
        loop {
            if let Some((response, pending_buffer_before)) =
                self.take_pending_response(link_id).await
            {
                return Ok(RnsNetWaitedPageResponse {
                    response,
                    source: "pending_buffer",
                    pending_buffer_before,
                });
            }
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(page_fetch_error(
                    plan,
                    NativePageFetchFailureStage::ResponseWait,
                    response_wait_timeout_detail(plan),
                ));
            }
            match tokio::time::timeout(deadline - now, responses.recv()).await {
                Ok(Some(response)) if response.link_id == link_id => {
                    return Ok(RnsNetWaitedPageResponse {
                        response,
                        source: "live_receiver",
                        pending_buffer_before: self.pending_response_count().await,
                    });
                }
                Ok(Some(response)) => self.store_pending_response(response).await,
                Ok(None) => {
                    return Err(page_fetch_error(
                        plan,
                        NativePageFetchFailureStage::ResponseWait,
                        "rns-net response stream closed",
                    ));
                }
                Err(_) => {
                    return Err(page_fetch_error(
                        plan,
                        NativePageFetchFailureStage::ResponseWait,
                        response_wait_timeout_detail(plan),
                    ));
                }
            }
        }
    }

    async fn wait_for_link_response(
        &self,
        link_id: [u8; 16],
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<RnsNetPageResponse> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut responses = self.responses.lock().await;
        loop {
            if let Some((response, _pending_buffer_before)) =
                self.take_pending_response(link_id).await
            {
                return Ok(response);
            }
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(AppError::from(NativeRuntimeError::Native(
                    "timed out waiting for rns-net link response".into(),
                )));
            }
            match tokio::time::timeout(deadline - now, responses.recv()).await {
                Ok(Some(response)) if response.link_id == link_id => return Ok(response),
                Ok(Some(response)) => self.store_pending_response(response).await,
                Ok(None) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "rns-net response stream closed".into(),
                    )));
                }
                Err(_) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "timed out waiting for rns-net link response".into(),
                    )));
                }
            }
        }
    }

    async fn wait_for_link_established(
        &self,
        plan: &NativeFetchPlan,
        link_id: [u8; 16],
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<RnsNetLinkEstablished> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut events = self.link_events.lock().await;
        loop {
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(page_fetch_error(
                    plan,
                    NativePageFetchFailureStage::LinkSetup,
                    "timed out waiting for rns-net link establishment",
                ));
            }
            match tokio::time::timeout(deadline - now, events.recv()).await {
                Ok(Some(event)) if event.link_id == link_id && event.is_initiator => {
                    return Ok(event);
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(page_fetch_error(
                        plan,
                        NativePageFetchFailureStage::LinkSetup,
                        "rns-net link event stream closed",
                    ));
                }
                Err(_) => {
                    return Err(page_fetch_error(
                        plan,
                        NativePageFetchFailureStage::LinkSetup,
                        "timed out waiting for rns-net link establishment",
                    ));
                }
            }
        }
    }

    async fn wait_for_link_id(
        &self,
        link_id: [u8; 16],
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<RnsNetLinkEstablished> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut events = self.link_events.lock().await;
        loop {
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(AppError::from(NativeRuntimeError::Native(
                    "timed out waiting for rns-net link establishment".into(),
                )));
            }
            match tokio::time::timeout(deadline - now, events.recv()).await {
                Ok(Some(event)) if event.link_id == link_id && event.is_initiator => {
                    return Ok(event);
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "rns-net link event stream closed".into(),
                    )));
                }
                Err(_) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "timed out waiting for rns-net link establishment".into(),
                    )));
                }
            }
        }
    }

    async fn take_pending_response(
        &self,
        link_id: [u8; 16],
    ) -> Option<(RnsNetPageResponse, usize)> {
        let mut pending = self.pending_responses.lock().await;
        take_pending_response_from(&mut pending, link_id)
            .map(|response| (response, pending.len() + 1))
    }

    async fn store_pending_response(&self, response: RnsNetPageResponse) {
        let mut pending = self.pending_responses.lock().await;
        store_pending_response_in(&mut pending, response);
    }

    async fn pending_response_count(&self) -> usize {
        self.pending_responses.lock().await.len()
    }
}

fn take_pending_response_from(
    pending: &mut VecDeque<RnsNetPageResponse>,
    link_id: [u8; 16],
) -> Option<RnsNetPageResponse> {
    let index = pending
        .iter()
        .position(|response| response.link_id == link_id)?;
    pending.remove(index)
}

fn store_pending_response_in(
    pending: &mut VecDeque<RnsNetPageResponse>,
    response: RnsNetPageResponse,
) {
    const MAX_PENDING_RESPONSES: usize = 32;
    if pending.len() >= MAX_PENDING_RESPONSES {
        pending.pop_front();
    }
    pending.push_back(response);
}

fn cached_page_link_for(
    keys: &RnsNetDestinationKeys,
    cached: Option<&RnsNetCachedPageLink>,
) -> Option<[u8; 16]> {
    cached
        .filter(|cached| {
            cached.destination_hash == keys.destination_hash
                && cached.signing_public_key == keys.signing_public_key
        })
        .map(|cached| cached.link_id)
}

fn request_allows_page_link_reuse(plan: &NativeFetchPlan) -> bool {
    plan.request
        .request_data
        .as_ref()
        .is_none_or(|data| data.is_empty() || request_data_is_link_reuse_safe(data))
}

fn request_data_is_micronplus_defaults_only(plan: &NativeFetchPlan) -> bool {
    plan.request
        .request_data
        .as_ref()
        .is_some_and(|data| !data.is_empty() && request_data_is_link_reuse_safe(data))
}

fn request_data_is_link_reuse_safe(data: &BTreeMap<String, String>) -> bool {
    data.keys().all(|key| {
        matches!(
            key.as_str(),
            "var_client" | "var_micronplus_plugin_enabled" | "var_micronplus_version"
        )
    })
}

fn request_data_count(plan: &NativeFetchPlan) -> usize {
    plan.request.request_data.as_ref().map_or(0, BTreeMap::len)
}

fn request_frame_bytes(plan: &NativeFetchPlan) -> Option<usize> {
    let request_data = plan.request.request_data.as_ref()?;
    NativeLinkRequestFrame::build(
        &plan.request.path,
        request_data,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or_default(),
    )
    .ok()
    .map(|frame| frame.packed.len())
}

fn request_send_failed_detail(request_frame_bytes: Option<usize>) -> String {
    match request_frame_bytes {
        Some(bytes) => {
            format!("rns-net failed to send page request; encoded request frame was {bytes} bytes")
        }
        None => "rns-net failed to send page request".into(),
    }
}

fn response_wait_timeout_detail(plan: &NativeFetchPlan) -> String {
    let Some(bytes) = request_frame_bytes(plan) else {
        return "timed out waiting for rns-net page response".into();
    };
    let mut detail = format!(
        "timed out waiting for rns-net page response; encoded request frame was {bytes} bytes"
    );
    if request_data_count(plan) > 0 && bytes > 900 {
        detail.push_str(
            "; native rns-net currently sends form requests as packets and may need Python Reticulum-style request-resource fallback for large submits",
        );
    }
    detail
}

fn request_data_keys(plan: &NativeFetchPlan) -> Vec<String> {
    plan.request
        .request_data
        .as_ref()
        .map(|data| data.keys().cloned().collect())
        .unwrap_or_default()
}

fn cleanup_probe_step(cleanup: RnsNetPageRequestCleanup) -> PageFetchProbeStep {
    PageFetchProbeStep::ok(
        PageFetchProbeStage::ResponseWait,
        "cleaned up stale rns-net page link after request failure",
    )
    .with_trace("link_torn_down", cleanup.link_torn_down.to_string())
    .with_trace("path_dropped", cleanup.path_dropped.to_string())
}

fn page_fetch_error(
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

impl RnsNetPageCallbacks {
    pub fn new() -> (
        Self,
        mpsc::UnboundedReceiver<RnsNetPageResponse>,
        mpsc::UnboundedReceiver<RnsNetLinkEstablished>,
    ) {
        let (response_tx, response_rx) = mpsc::unbounded_channel();
        let (link_tx, link_rx) = mpsc::unbounded_channel();
        (
            Self {
                response_tx,
                link_tx,
                announce_tx: None,
                path_tx: None,
                local_delivery_tx: None,
                proof_tx: None,
                resource_tx: None,
                link_data_tx: None,
                link_closed_tx: None,
            },
            response_rx,
            link_rx,
        )
    }

    pub fn with_announce_sink(
        announce_tx: mpsc::UnboundedSender<RnsNetAnnounceKey>,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<RnsNetPageResponse>,
        mpsc::UnboundedReceiver<RnsNetLinkEstablished>,
    ) {
        let (response_tx, response_rx) = mpsc::unbounded_channel();
        let (link_tx, link_rx) = mpsc::unbounded_channel();
        (
            Self {
                response_tx,
                link_tx,
                announce_tx: Some(announce_tx),
                path_tx: None,
                local_delivery_tx: None,
                proof_tx: None,
                resource_tx: None,
                link_data_tx: None,
                link_closed_tx: None,
            },
            response_rx,
            link_rx,
        )
    }

    pub fn with_event_sinks(
        announce_tx: mpsc::UnboundedSender<RnsNetAnnounceKey>,
        path_tx: mpsc::UnboundedSender<RnsNetPathUpdate>,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<RnsNetPageResponse>,
        mpsc::UnboundedReceiver<RnsNetLinkEstablished>,
    ) {
        let (response_tx, response_rx) = mpsc::unbounded_channel();
        let (link_tx, link_rx) = mpsc::unbounded_channel();
        (
            Self {
                response_tx,
                link_tx,
                announce_tx: Some(announce_tx),
                path_tx: Some(path_tx),
                local_delivery_tx: None,
                proof_tx: None,
                resource_tx: None,
                link_data_tx: None,
                link_closed_tx: None,
            },
            response_rx,
            link_rx,
        )
    }

    pub fn with_all_event_sinks(
        announce_tx: mpsc::UnboundedSender<RnsNetAnnounceKey>,
        path_tx: mpsc::UnboundedSender<RnsNetPathUpdate>,
        local_delivery_tx: mpsc::UnboundedSender<RnsNetLocalDelivery>,
        proof_tx: mpsc::UnboundedSender<RnsNetProof>,
        resource_tx: mpsc::UnboundedSender<RnsNetResourceEvent>,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<RnsNetPageResponse>,
        mpsc::UnboundedReceiver<RnsNetLinkEstablished>,
    ) {
        let (response_tx, response_rx) = mpsc::unbounded_channel();
        let (link_tx, link_rx) = mpsc::unbounded_channel();
        (
            Self {
                response_tx,
                link_tx,
                announce_tx: Some(announce_tx),
                path_tx: Some(path_tx),
                local_delivery_tx: Some(local_delivery_tx),
                proof_tx: Some(proof_tx),
                resource_tx: Some(resource_tx),
                link_data_tx: None,
                link_closed_tx: None,
            },
            response_rx,
            link_rx,
        )
    }

    pub fn set_link_data_sink(&mut self, link_data_tx: mpsc::UnboundedSender<RnsNetLinkData>) {
        self.link_data_tx = Some(link_data_tx);
    }

    pub fn set_link_closed_sink(
        &mut self,
        link_closed_tx: mpsc::UnboundedSender<RnsNetLinkClosed>,
    ) {
        self.link_closed_tx = Some(link_closed_tx);
    }
}

impl rns_net::Callbacks for RnsNetPageCallbacks {
    fn on_announce(&mut self, announced: rns_net::AnnouncedIdentity) {
        if let Some(tx) = &self.announce_tx {
            let _ = tx.send(RnsNetAnnounceKey::from_announced_identity(&announced));
        }
    }

    fn on_path_updated(&mut self, dest_hash: rns_core::types::DestHash, hops: u8) {
        if let Some(tx) = &self.path_tx {
            let _ = tx.send(RnsNetPathUpdate {
                destination_hash: dest_hash.0,
                hops,
            });
        }
    }

    fn on_local_delivery(
        &mut self,
        dest_hash: rns_core::types::DestHash,
        raw: Vec<u8>,
        packet_hash: rns_core::types::PacketHash,
    ) {
        if let Some(tx) = &self.local_delivery_tx {
            let _ = tx.send(RnsNetLocalDelivery {
                destination_hash: dest_hash.0,
                raw,
                packet_hash: packet_hash.0,
            });
        }
    }

    fn on_link_established(
        &mut self,
        link_id: rns_core::types::LinkId,
        dest_hash: rns_core::types::DestHash,
        rtt: f64,
        is_initiator: bool,
    ) {
        let _ = self.link_tx.send(RnsNetLinkEstablished {
            link_id: link_id.0,
            destination_hash: dest_hash.0,
            rtt,
            is_initiator,
        });
    }

    fn on_link_closed(
        &mut self,
        link_id: rns_core::types::LinkId,
        reason: Option<rns_net::TeardownReason>,
    ) {
        if let Some(tx) = &self.link_closed_tx {
            let _ = tx.send(RnsNetLinkClosed {
                link_id: link_id.0,
                reason: reason.map(|value| format!("{value:?}")),
            });
        }
    }

    fn on_proof(
        &mut self,
        dest_hash: rns_core::types::DestHash,
        packet_hash: rns_core::types::PacketHash,
        rtt: f64,
    ) {
        if let Some(tx) = &self.proof_tx {
            let _ = tx.send(RnsNetProof {
                destination_hash: dest_hash.0,
                packet_hash: packet_hash.0,
                rtt,
            });
        }
    }

    fn on_resource_received(
        &mut self,
        link_id: rns_core::types::LinkId,
        data: Vec<u8>,
        metadata: Option<Vec<u8>>,
    ) {
        if let Some(tx) = &self.resource_tx {
            let _ = tx.send(RnsNetResourceEvent::Received {
                link_id: link_id.0,
                data,
                metadata,
            });
        }
    }

    fn on_resource_completed(&mut self, link_id: rns_core::types::LinkId) {
        if let Some(tx) = &self.resource_tx {
            let _ = tx.send(RnsNetResourceEvent::Completed { link_id: link_id.0 });
        }
    }

    fn on_resource_failed(&mut self, link_id: rns_core::types::LinkId, error: String) {
        if let Some(tx) = &self.resource_tx {
            let _ = tx.send(RnsNetResourceEvent::Failed {
                link_id: link_id.0,
                error,
            });
        }
    }

    fn on_resource_accept_query(
        &mut self,
        _link_id: rns_core::types::LinkId,
        _resource_hash: Vec<u8>,
        _transfer_size: u64,
        _has_metadata: bool,
    ) -> bool {
        true
    }

    fn on_resource_progress(
        &mut self,
        link_id: rns_core::types::LinkId,
        received: usize,
        total: usize,
    ) {
        if let Some(tx) = &self.resource_tx {
            let _ = tx.send(RnsNetResourceEvent::Progress {
                link_id: link_id.0,
                received,
                total,
            });
        }
    }

    fn on_link_data(&mut self, link_id: rns_core::types::LinkId, context: u8, data: Vec<u8>) {
        if let Some(tx) = &self.link_data_tx {
            let _ = tx.send(RnsNetLinkData {
                link_id: link_id.0,
                context,
                data,
            });
        }
    }

    fn on_response(
        &mut self,
        link_id: rns_core::types::LinkId,
        request_id: [u8; 16],
        data: Vec<u8>,
    ) {
        let (body, summary) = decode_response_value_with_summary(&data, request_id)
            .unwrap_or_else(|_| raw_response_body_with_summary(data));
        let _ = self.response_tx.send(RnsNetPageResponse {
            link_id: link_id.0,
            request_id,
            body,
            summary,
        });
    }
}

fn encode_request_data(
    request_data: Option<&BTreeMap<String, String>>,
) -> Result<Vec<u8>, NativeRuntimeError> {
    // Python OMENbrowser passes `path` and `data` separately to RNS Link.request. The rns-net
    // API mirrors that split, so this payload intentionally remains only the msgpack data value,
    // not the lower-level full link request frame modeled in request.rs.
    let value = match request_data {
        Some(data) if !data.is_empty() => Value::Map(
            data.iter()
                .map(|(key, value)| {
                    (
                        Value::String(key.as_str().into()),
                        Value::String(value.as_str().into()),
                    )
                })
                .collect(),
        ),
        _ => Value::Nil,
    };
    pack_value(&value)
}

fn decode_response_value(bytes: &[u8]) -> Result<Vec<u8>, NativeRuntimeError> {
    decode_response_value_with_summary(bytes, [0; 16]).map(|(body, _summary)| body)
}

fn decode_response_value_with_summary(
    bytes: &[u8],
    callback_request_id: [u8; 16],
) -> Result<(Vec<u8>, RnsNetResponseDecodeSummary), NativeRuntimeError> {
    if let Ok(frame) = NativeLinkResponseFrame::parse(bytes) {
        let decoded_body_bytes = frame.body.len();
        let framed_request_id = frame.request_id;
        let request_id_matches_frame = framed_request_id == callback_request_id;
        return Ok((
            frame.body,
            RnsNetResponseDecodeSummary {
                raw_bytes: bytes.len(),
                decoded_body_bytes,
                format: "link_request_frame",
                framed_request_id: Some(framed_request_id),
                request_id_matches_frame: Some(request_id_matches_frame),
            },
        ));
    }
    let value = unpack_value(bytes)?;
    let (body, format) = match value {
        Value::Binary(bytes) => (bytes, "msgpack_binary"),
        Value::String(text) => {
            let body = text
                .as_str()
                .map(|text| text.as_bytes().to_vec())
                .ok_or_else(|| {
                    NativeRuntimeError::InvalidResponse(
                        "rns-net response string was invalid".into(),
                    )
                })?;
            (body, "msgpack_string")
        }
        Value::Nil => (Vec::new(), "msgpack_nil"),
        other => (pack_value(&other)?, "msgpack_value_repacked"),
    };
    let decoded_body_bytes = body.len();
    Ok((
        body,
        RnsNetResponseDecodeSummary {
            raw_bytes: bytes.len(),
            decoded_body_bytes,
            format,
            framed_request_id: None,
            request_id_matches_frame: None,
        },
    ))
}

fn raw_response_body_with_summary(data: Vec<u8>) -> (Vec<u8>, RnsNetResponseDecodeSummary) {
    let len = data.len();
    (
        data,
        RnsNetResponseDecodeSummary {
            raw_bytes: len,
            decoded_body_bytes: len,
            format: "raw_non_msgpack",
            framed_request_id: None,
            request_id_matches_frame: None,
        },
    )
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn pack_value(value: &Value) -> Result<Vec<u8>, NativeRuntimeError> {
    let mut packed = Vec::new();
    rmpv::encode::write_value(&mut packed, value)
        .map_err(|_| NativeRuntimeError::InvalidResponse("rns-net msgpack encode failed".into()))?;
    Ok(packed)
}

fn unpack_value(bytes: &[u8]) -> Result<Value, NativeRuntimeError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor)
        .map_err(|_| NativeRuntimeError::InvalidResponse("rns-net msgpack decode failed".into()))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(NativeRuntimeError::InvalidResponse(
            "rns-net msgpack response had trailing bytes".into(),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::native::request::NativeFetchPlan;
    use rns_net::Callbacks;

    #[test]
    fn rns_net_exposes_python_compatible_request_surface() {
        let api = native_rns_net_request_api();

        assert!(api.node_type.contains("RnsNode"));
        assert!(api.callbacks_trait.contains("Callbacks"));
        assert!(api.create_link_available);
        assert!(api.send_request_available);
        assert!(api.response_callback_available);
    }

    #[test]
    fn request_backend_prefers_rns_net_for_link_request_dispatch() {
        let decision = select_native_request_backend();

        assert_eq!(decision.backend, NativeRequestBackend::RnsNet);
        assert!(decision.reason.contains("send_request"));
    }

    #[test]
    fn pending_response_buffer_preserves_other_link_responses() {
        let mut pending = VecDeque::new();
        let first = RnsNetPageResponse {
            link_id: [1u8; 16],
            request_id: [2u8; 16],
            body: b"first".to_vec(),
            summary: RnsNetResponseDecodeSummary::default(),
        };
        let second = RnsNetPageResponse {
            link_id: [3u8; 16],
            request_id: [4u8; 16],
            body: b"second".to_vec(),
            summary: RnsNetResponseDecodeSummary::default(),
        };

        store_pending_response_in(&mut pending, first.clone());
        store_pending_response_in(&mut pending, second.clone());

        assert_eq!(
            take_pending_response_from(&mut pending, [3u8; 16]),
            Some(second)
        );
        assert_eq!(pending.len(), 1);
        assert_eq!(
            take_pending_response_from(&mut pending, [1u8; 16]),
            Some(first)
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn pending_response_buffer_is_bounded() {
        let mut pending = VecDeque::new();
        for index in 0u8..40 {
            store_pending_response_in(
                &mut pending,
                RnsNetPageResponse {
                    link_id: [index; 16],
                    request_id: [index; 16],
                    body: Vec::new(),
                    summary: RnsNetResponseDecodeSummary::default(),
                },
            );
        }

        assert_eq!(pending.len(), 32);
        assert!(take_pending_response_from(&mut pending, [0u8; 16]).is_none());
        assert!(take_pending_response_from(&mut pending, [7u8; 16]).is_none());
        assert!(take_pending_response_from(&mut pending, [8u8; 16]).is_some());
        assert!(take_pending_response_from(&mut pending, [39u8; 16]).is_some());
    }

    #[test]
    fn destination_keys_convert_from_native_fetch_plan() {
        let plan = NativeFetchPlan::new("0123456789abcdef0123456789abcdef:/index.mu", None, 5)
            .expect("valid plan");
        let keys = RnsNetDestinationKeys::from_fetch_plan(&plan, [7; 32]).expect("keys");

        assert_eq!(
            keys.destination_hash,
            [
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
                0xcd, 0xef
            ]
        );
        assert_eq!(keys.signing_public_key, [7; 32]);
    }

    #[test]
    fn cached_page_link_reuses_only_matching_destination_and_key() {
        let keys = RnsNetDestinationKeys {
            destination_hash: [1u8; 16],
            signing_public_key: [2u8; 32],
        };
        let cached = RnsNetCachedPageLink {
            destination_hash: [1u8; 16],
            signing_public_key: [2u8; 32],
            link_id: [3u8; 16],
        };

        assert_eq!(cached_page_link_for(&keys, Some(&cached)), Some([3u8; 16]));

        let other_destination = RnsNetDestinationKeys {
            destination_hash: [9u8; 16],
            signing_public_key: [2u8; 32],
        };
        let other_key = RnsNetDestinationKeys {
            destination_hash: [1u8; 16],
            signing_public_key: [9u8; 32],
        };

        assert_eq!(
            cached_page_link_for(&other_destination, Some(&cached)),
            None
        );
        assert_eq!(cached_page_link_for(&other_key, Some(&cached)), None);
        assert_eq!(cached_page_link_for(&keys, None), None);
    }

    #[test]
    fn page_link_reuse_is_disabled_only_for_explicit_form_request_data() {
        let plain =
            NativeFetchPlan::new("0123456789abcdef0123456789abcdef:/page/index.mu", None, 5)
                .expect("plain plan");
        let empty = NativeFetchPlan::new(
            "0123456789abcdef0123456789abcdef:/page/index.mu",
            Some(BTreeMap::new()),
            5,
        )
        .expect("empty plan");
        let form = NativeFetchPlan::new(
            "0123456789abcdef0123456789abcdef:/page/login.mu",
            Some(BTreeMap::from([(
                "field_username".to_string(),
                "omen".to_string(),
            )])),
            5,
        )
        .expect("form plan");
        let micronplus_default = NativeFetchPlan::new(
            "0123456789abcdef0123456789abcdef:/page/index.mu",
            Some(BTreeMap::from([
                ("var_client".to_string(), "omenbrowser".to_string()),
                ("var_micronplus_plugin_enabled".to_string(), "1".to_string()),
                ("var_micronplus_version".to_string(), "1".to_string()),
            ])),
            5,
        )
        .expect("micronplus default plan");
        let micronplus_with_field = NativeFetchPlan::new(
            "0123456789abcdef0123456789abcdef:/page/login.mu",
            Some(BTreeMap::from([
                ("var_client".to_string(), "omenbrowser".to_string()),
                ("field_username".to_string(), "omen".to_string()),
            ])),
            5,
        )
        .expect("micronplus field plan");

        assert!(request_allows_page_link_reuse(&plain));
        assert!(request_allows_page_link_reuse(&empty));
        assert!(request_allows_page_link_reuse(&micronplus_default));
        assert!(!request_allows_page_link_reuse(&form));
        assert!(!request_allows_page_link_reuse(&micronplus_with_field));
        assert!(!request_data_is_micronplus_defaults_only(&plain));
        assert!(request_data_is_micronplus_defaults_only(
            &micronplus_default
        ));
        assert!(!request_data_is_micronplus_defaults_only(
            &micronplus_with_field
        ));
        assert_eq!(request_data_count(&form), 1);
        assert_eq!(request_data_keys(&form), vec!["field_username"]);
    }

    #[test]
    fn large_form_timeout_detail_calls_out_request_resource_gap() {
        let plan = NativeFetchPlan::new(
            "0123456789abcdef0123456789abcdef:/page/myblog.mu",
            Some(BTreeMap::from([
                ("field_post_title".to_string(), "Title".to_string()),
                ("field_post_body".to_string(), "x".repeat(1400)),
                ("var_action".to_string(), "publish_post".to_string()),
            ])),
            5,
        )
        .expect("form plan");

        let detail = response_wait_timeout_detail(&plan);

        assert!(detail.contains("encoded request frame was"));
        assert!(detail.contains("request-resource fallback"));
    }

    #[test]
    fn cleanup_probe_step_reports_link_teardown_without_path_drop() {
        let step = cleanup_probe_step(RnsNetPageRequestCleanup {
            link_torn_down: true,
            path_dropped: false,
        });

        assert_eq!(step.stage, PageFetchProbeStage::ResponseWait);
        assert!(step.ok);
        assert!(step.detail.contains("cleaned up stale rns-net page link"));
        assert!(!step.detail.contains("path after request failure"));
        assert_eq!(
            step.trace.get("link_torn_down").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            step.trace.get("path_dropped").map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn request_payload_contract_matches_python_link_request_usage() {
        let contract = rns_net_request_payload_contract();

        assert!(contract.path_sent_separately);
        assert_eq!(contract.payload_shape, "msgpack request data value");
        assert_eq!(contract.none_or_empty_data_hex, "c0");
        assert!(contract
            .source_note
            .contains("Link.request(path, data=...)"));
    }

    #[test]
    fn request_data_none_and_empty_encode_as_msgpack_nil() {
        let empty = BTreeMap::new();

        let none = encode_request_data(None).expect("none encoded");
        let empty = encode_request_data(Some(&empty)).expect("empty encoded");

        assert_eq!(hex_bytes(&none), "c0");
        assert_eq!(empty, none);
        assert_eq!(unpack_value(&none).expect("decode none"), Value::Nil);
    }

    #[test]
    fn request_data_is_exact_msgpack_map_value_not_full_link_frame() {
        let mut request_data = BTreeMap::new();
        request_data.insert("field".into(), "value".into());

        let encoded = encode_request_data(Some(&request_data)).expect("encoded");
        let decoded = unpack_value(&encoded).expect("decoded");

        assert_eq!(hex_bytes(&encoded), "81a56669656c64a576616c7565");
        assert!(matches!(decoded, Value::Map(_)));
    }

    #[test]
    fn request_data_form_fixture_uses_stable_python_data_ordering() {
        let mut request_data = BTreeMap::new();
        request_data.insert("field_name".into(), "omen".into());
        request_data.insert("var_next".into(), "/next.mu".into());

        let encoded = encode_request_data(Some(&request_data)).expect("encoded");
        let decoded = unpack_value(&encoded).expect("decoded");

        assert_eq!(
            hex_bytes(&encoded),
            "82aa6669656c645f6e616d65a46f6d656ea87661725f6e657874a82f6e6578742e6d75"
        );
        assert!(matches!(decoded, Value::Map(_)));
    }

    #[test]
    fn response_value_decodes_string_binary_and_nil_bodies() {
        let string = pack_value(&Value::String("hello".into())).expect("string");
        let binary = pack_value(&Value::Binary(b">Page\nBody".to_vec())).expect("binary");
        let nil = pack_value(&Value::Nil).expect("nil");

        assert_eq!(
            decode_response_value(&string).expect("decode string"),
            b"hello"
        );
        assert_eq!(
            decode_response_value(&binary).expect("decode binary"),
            b">Page\nBody"
        );
        assert_eq!(
            decode_response_value(&nil).expect("decode nil"),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn response_value_decodes_link_request_frame_body() {
        let frame = pack_value(&Value::Array(vec![
            Value::Binary(vec![0x11; 16]),
            Value::String(">Live page\nBody".into()),
        ]))
        .expect("link response frame");

        assert_eq!(
            decode_response_value(&frame).expect("decode link frame"),
            b">Live page\nBody"
        );
    }

    #[tokio::test]
    async fn callbacks_preserve_raw_non_msgpack_response_body() {
        let (mut callbacks, mut rx, _link_rx) = RnsNetPageCallbacks::new();
        let link_id = [3u8; 16];
        let request_id = [5u8; 16];

        callbacks.on_response(
            rns_core::types::LinkId(link_id),
            request_id,
            b"raw micron body".to_vec(),
        );
        let routed = rx.recv().await.expect("routed response");

        assert_eq!(routed.link_id, link_id);
        assert_eq!(routed.request_id, request_id);
        assert_eq!(routed.body, b"raw micron body");
        assert_eq!(routed.summary.format, "raw_non_msgpack");
        assert_eq!(routed.summary.raw_bytes, b"raw micron body".len());
        assert_eq!(routed.summary.decoded_body_bytes, b"raw micron body".len());
    }

    #[tokio::test]
    async fn callbacks_route_response_by_link_id() {
        let (mut callbacks, mut rx, _link_rx) = RnsNetPageCallbacks::new();
        let link_id = [3u8; 16];
        let request_id = [5u8; 16];
        let body = pack_value(&Value::String("hello".into())).expect("packed body");

        callbacks.on_response(rns_core::types::LinkId(link_id), request_id, body);
        let routed = rx.recv().await.expect("routed response");

        assert_eq!(routed.link_id, link_id);
        assert_eq!(routed.request_id, request_id);
        assert_eq!(routed.body, b"hello");
        assert_eq!(routed.summary.format, "msgpack_string");
        assert_eq!(routed.summary.decoded_body_bytes, 5);
    }

    #[tokio::test]
    async fn callbacks_route_link_established_before_request_send() {
        let (mut callbacks, _response_rx, mut link_rx) = RnsNetPageCallbacks::new();
        let link_id = [3u8; 16];
        let destination = [4u8; 16];

        callbacks.on_link_established(
            rns_core::types::LinkId(link_id),
            rns_core::types::DestHash(destination),
            0.25,
            true,
        );
        let event = link_rx.recv().await.expect("link event");

        assert_eq!(event.link_id, link_id);
        assert_eq!(event.destination_hash, destination);
        assert_eq!(event.rtt, 0.25);
        assert!(event.is_initiator);
    }

    #[tokio::test]
    async fn callbacks_route_link_request_frame_response_body() {
        let (mut callbacks, mut rx, _link_rx) = RnsNetPageCallbacks::new();
        let link_id = [3u8; 16];
        let request_id = [5u8; 16];
        let body = pack_value(&Value::Array(vec![
            Value::Binary(request_id.to_vec()),
            Value::Binary(b">Framed page".to_vec()),
        ]))
        .expect("packed frame body");

        callbacks.on_response(rns_core::types::LinkId(link_id), request_id, body);
        let routed = rx.recv().await.expect("routed response");

        assert_eq!(routed.link_id, link_id);
        assert_eq!(routed.request_id, request_id);
        assert_eq!(routed.body, b">Framed page");
        assert_eq!(routed.summary.format, "link_request_frame");
        assert_eq!(routed.summary.framed_request_id, Some(request_id));
        assert_eq!(routed.summary.request_id_matches_frame, Some(true));
    }

    #[tokio::test]
    async fn callbacks_report_mismatched_link_request_frame_ids() {
        let (mut callbacks, mut rx, _link_rx) = RnsNetPageCallbacks::new();
        let link_id = [3u8; 16];
        let callback_request_id = [5u8; 16];
        let framed_request_id = [9u8; 16];
        let body = pack_value(&Value::Array(vec![
            Value::Binary(framed_request_id.to_vec()),
            Value::Binary(b">Framed page".to_vec()),
        ]))
        .expect("packed frame body");

        callbacks.on_response(rns_core::types::LinkId(link_id), callback_request_id, body);
        let routed = rx.recv().await.expect("routed response");

        assert_eq!(routed.request_id, callback_request_id);
        assert_eq!(routed.summary.format, "link_request_frame");
        assert_eq!(routed.summary.framed_request_id, Some(framed_request_id));
        assert_eq!(routed.summary.request_id_matches_frame, Some(false));
    }

    #[tokio::test]
    async fn callbacks_capture_destination_signing_key_from_announces() {
        let (announce_tx, mut announce_rx) = mpsc::unbounded_channel();
        let (mut callbacks, _response_rx, _link_rx) =
            RnsNetPageCallbacks::with_announce_sink(announce_tx);
        let mut public_key = [0u8; 64];
        public_key[32..64].copy_from_slice(&[9u8; 32]);
        let announced = rns_net::AnnouncedIdentity {
            dest_hash: rns_core::types::DestHash([4u8; 16]),
            identity_hash: rns_core::types::IdentityHash([5u8; 16]),
            public_key,
            app_data: None,
            hops: 1,
            rssi: None,
            snr: None,
            received_at: 1.0,
            receiving_interface: rns_core::transport::types::InterfaceId(1),
        };

        callbacks.on_announce(announced);
        let key = announce_rx.recv().await.expect("announce key");

        assert_eq!(key.destination_hash, [4u8; 16]);
        assert_eq!(key.identity_hash, [5u8; 16]);
        assert_eq!(key.signing_public_key, [9u8; 32]);
        assert_eq!(key.full_public_key, public_key);
    }

    #[tokio::test]
    async fn callbacks_route_path_updates() {
        let (announce_tx, _announce_rx) = mpsc::unbounded_channel();
        let (path_tx, mut path_rx) = mpsc::unbounded_channel();
        let (mut callbacks, _response_rx, _link_rx) =
            RnsNetPageCallbacks::with_event_sinks(announce_tx, path_tx);

        callbacks.on_path_updated(rns_core::types::DestHash([6u8; 16]), 3);
        let update = path_rx.recv().await.expect("path update");

        assert_eq!(update.destination_hash, [6u8; 16]);
        assert_eq!(update.hops, 3);
    }

    #[tokio::test]
    async fn callbacks_route_local_deliveries() {
        let (announce_tx, _announce_rx) = mpsc::unbounded_channel();
        let (path_tx, _path_rx) = mpsc::unbounded_channel();
        let (local_tx, mut local_rx) = mpsc::unbounded_channel();
        let (proof_tx, _proof_rx) = mpsc::unbounded_channel();
        let (resource_tx, _resource_rx) = mpsc::unbounded_channel();
        let (mut callbacks, _response_rx, _link_rx) = RnsNetPageCallbacks::with_all_event_sinks(
            announce_tx,
            path_tx,
            local_tx,
            proof_tx,
            resource_tx,
        );

        callbacks.on_local_delivery(
            rns_core::types::DestHash([7u8; 16]),
            b"payload".to_vec(),
            rns_core::types::PacketHash([8u8; 32]),
        );
        let delivery = local_rx.recv().await.expect("local delivery");

        assert_eq!(delivery.destination_hash, [7u8; 16]);
        assert_eq!(delivery.raw, b"payload");
        assert_eq!(delivery.packet_hash, [8u8; 32]);
    }

    #[tokio::test]
    async fn callbacks_route_packet_proofs() {
        let (announce_tx, _announce_rx) = mpsc::unbounded_channel();
        let (path_tx, _path_rx) = mpsc::unbounded_channel();
        let (local_tx, _local_rx) = mpsc::unbounded_channel();
        let (proof_tx, mut proof_rx) = mpsc::unbounded_channel();
        let (resource_tx, _resource_rx) = mpsc::unbounded_channel();
        let (mut callbacks, _response_rx, _link_rx) = RnsNetPageCallbacks::with_all_event_sinks(
            announce_tx,
            path_tx,
            local_tx,
            proof_tx,
            resource_tx,
        );

        callbacks.on_proof(
            rns_core::types::DestHash([9u8; 16]),
            rns_core::types::PacketHash([10u8; 32]),
            0.125,
        );
        let proof = proof_rx.recv().await.expect("proof");

        assert_eq!(proof.destination_hash, [9u8; 16]);
        assert_eq!(proof.packet_hash, [10u8; 32]);
        assert_eq!(proof.rtt, 0.125);
    }

    #[tokio::test]
    async fn callbacks_route_resource_events() {
        let (announce_tx, _announce_rx) = mpsc::unbounded_channel();
        let (path_tx, _path_rx) = mpsc::unbounded_channel();
        let (local_tx, _local_rx) = mpsc::unbounded_channel();
        let (proof_tx, _proof_rx) = mpsc::unbounded_channel();
        let (resource_tx, mut resource_rx) = mpsc::unbounded_channel();
        let (mut callbacks, _response_rx, _link_rx) = RnsNetPageCallbacks::with_all_event_sinks(
            announce_tx,
            path_tx,
            local_tx,
            proof_tx,
            resource_tx,
        );

        callbacks.on_resource_received(
            rns_core::types::LinkId([3u8; 16]),
            b"resource".to_vec(),
            Some(b"metadata".to_vec()),
        );
        callbacks.on_resource_progress(rns_core::types::LinkId([1u8; 16]), 2, 4);
        callbacks.on_resource_completed(rns_core::types::LinkId([1u8; 16]));
        callbacks.on_resource_failed(rns_core::types::LinkId([2u8; 16]), "timeout".into());

        assert_eq!(
            resource_rx.recv().await,
            Some(RnsNetResourceEvent::Received {
                link_id: [3u8; 16],
                data: b"resource".to_vec(),
                metadata: Some(b"metadata".to_vec()),
            })
        );
        assert_eq!(
            resource_rx.recv().await,
            Some(RnsNetResourceEvent::Progress {
                link_id: [1u8; 16],
                received: 2,
                total: 4,
            })
        );
        assert_eq!(
            resource_rx.recv().await,
            Some(RnsNetResourceEvent::Completed { link_id: [1u8; 16] })
        );
        assert_eq!(
            resource_rx.recv().await,
            Some(RnsNetResourceEvent::Failed {
                link_id: [2u8; 16],
                error: "timeout".into(),
            })
        );
    }

    #[tokio::test]
    async fn callbacks_route_link_data_events() {
        let (mut callbacks, _response_rx, _link_rx) = RnsNetPageCallbacks::new();
        let (link_data_tx, mut link_data_rx) = mpsc::unbounded_channel();
        callbacks.set_link_data_sink(link_data_tx);

        callbacks.on_link_data(
            rns_core::types::LinkId([4u8; 16]),
            0x42,
            b"link payload".to_vec(),
        );

        assert_eq!(
            link_data_rx.recv().await,
            Some(RnsNetLinkData {
                link_id: [4u8; 16],
                context: 0x42,
                data: b"link payload".to_vec(),
            })
        );
    }

    #[tokio::test]
    async fn callbacks_route_link_closed_events() {
        let (mut callbacks, _response_rx, _link_rx) = RnsNetPageCallbacks::new();
        let (link_closed_tx, mut link_closed_rx) = mpsc::unbounded_channel();
        callbacks.set_link_closed_sink(link_closed_tx);

        callbacks.on_link_closed(rns_core::types::LinkId([9u8; 16]), None);

        assert_eq!(
            link_closed_rx.recv().await,
            Some(RnsNetLinkClosed {
                link_id: [9u8; 16],
                reason: None,
            })
        );
    }

    #[test]
    fn destination_key_store_returns_signing_keys_by_destination() {
        let mut store = RnsNetDestinationKeyStore::default();
        let key = RnsNetAnnounceKey {
            destination_hash: [1u8; 16],
            identity_hash: [4u8; 16],
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: None,
            hops: None,
            packet_hash: None,
            observed_at: 1.0,
        };
        store.ingest(key.clone());

        assert_eq!(store.len(), 1);
        assert_eq!(store.signing_public_key(&[1u8; 16]), Some([2u8; 32]));
        assert_eq!(store.destination_key(&[1u8; 16]), Some(key));
        assert_eq!(store.signing_public_key(&[9u8; 16]), None);
        assert_eq!(store.destination_key(&[9u8; 16]), None);
    }

    #[test]
    fn destination_key_store_derives_lxmf_delivery_key_from_node_announce() {
        let identity_hash = [4u8; 16];
        let node_hash = rns_destination_hash(&identity_hash, "nomadnetwork", "node");
        let delivery_hash = rns_destination_hash(&identity_hash, "lxmf", "delivery");
        let propagation_hash = rns_destination_hash(&identity_hash, "lxmf", "propagation");
        let mut store = RnsNetDestinationKeyStore::default();

        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: node_hash,
            identity_hash,
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: Some(b"Node".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        });

        assert_eq!(store.signing_public_key(&node_hash), Some([2u8; 32]));
        assert_eq!(store.signing_public_key(&delivery_hash), Some([2u8; 32]));
        assert_eq!(store.signing_public_key(&propagation_hash), Some([2u8; 32]));
        assert_eq!(
            store
                .destination_key(&delivery_hash)
                .and_then(|key| key.app_data),
            None
        );
        assert_eq!(
            store
                .destination_key(&delivery_hash)
                .and_then(|key| key.hops),
            Some(1)
        );
    }

    #[test]
    fn destination_key_store_returns_nomadnet_lxmf_sibling_hashes() {
        let identity_hash = [4u8; 16];
        let node_hash = rns_destination_hash(&identity_hash, "nomadnetwork", "node");
        let delivery_hash = rns_destination_hash(&identity_hash, "lxmf", "delivery");
        let propagation_hash = rns_destination_hash(&identity_hash, "lxmf", "propagation");
        let mut store = RnsNetDestinationKeyStore::default();

        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: node_hash,
            identity_hash,
            signing_public_key: [2u8; 32],
            full_public_key: [3u8; 64],
            app_data: Some(b"Node".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        });

        let siblings = store.sibling_destination_hashes(&node_hash);
        assert_eq!(siblings.len(), 2);
        assert!(siblings.contains(&delivery_hash));
        assert!(siblings.contains(&propagation_hash));
        assert!(store.sibling_destination_hashes(&[9u8; 16]).is_empty());
    }

    #[test]
    fn destination_key_store_derives_node_key_from_lxmf_delivery_announce() {
        let identity_hash = [5u8; 16];
        let node_hash = rns_destination_hash(&identity_hash, "nomadnetwork", "node");
        let delivery_hash = rns_destination_hash(&identity_hash, "lxmf", "delivery");
        let propagation_hash = rns_destination_hash(&identity_hash, "lxmf", "propagation");
        let mut store = RnsNetDestinationKeyStore::default();

        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: delivery_hash,
            identity_hash,
            signing_public_key: [6u8; 32],
            full_public_key: [7u8; 64],
            app_data: Some(b"Peer".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        });

        assert_eq!(store.signing_public_key(&delivery_hash), Some([6u8; 32]));
        assert_eq!(store.signing_public_key(&node_hash), Some([6u8; 32]));
        assert_eq!(store.signing_public_key(&propagation_hash), Some([6u8; 32]));
        assert_eq!(
            store
                .destination_key(&node_hash)
                .and_then(|key| key.app_data),
            None
        );
    }

    #[test]
    fn destination_key_store_preserves_real_app_data_when_sibling_placeholder_arrives_later() {
        let identity_hash = [9u8; 16];
        let node_hash = rns_destination_hash(&identity_hash, "nomadnetwork", "node");
        let propagation_hash = rns_destination_hash(&identity_hash, "lxmf", "propagation");
        let mut store = RnsNetDestinationKeyStore::default();

        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: propagation_hash,
            identity_hash,
            signing_public_key: [8u8; 32],
            full_public_key: [9u8; 64],
            app_data: Some(b"real propagation metadata".to_vec()),
            hops: Some(2),
            packet_hash: Some([0x77; 32]),
            observed_at: 1.0,
        });
        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: node_hash,
            identity_hash,
            signing_public_key: [8u8; 32],
            full_public_key: [9u8; 64],
            app_data: Some(b"Node".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 2.0,
        });

        let propagation = store
            .destination_key(&propagation_hash)
            .expect("propagation key");
        assert_eq!(
            propagation.app_data.as_deref(),
            Some(&b"real propagation metadata"[..])
        );
        assert_eq!(propagation.hops, Some(1));
        assert_eq!(propagation.packet_hash, Some([0x77; 32]));
    }

    #[test]
    fn destination_key_store_does_not_derive_omen_siblings_for_unknown_aspects() {
        let identity_hash = [8u8; 16];
        let unknown_hash = rns_destination_hash(&identity_hash, "other", "aspect");
        let node_hash = rns_destination_hash(&identity_hash, "nomadnetwork", "node");
        let delivery_hash = rns_destination_hash(&identity_hash, "lxmf", "delivery");
        let propagation_hash = rns_destination_hash(&identity_hash, "lxmf", "propagation");
        let mut store = RnsNetDestinationKeyStore::default();

        store.ingest_with_nomadnet_lxmf_siblings(RnsNetAnnounceKey {
            destination_hash: unknown_hash,
            identity_hash,
            signing_public_key: [1u8; 32],
            full_public_key: [2u8; 64],
            app_data: Some(b"Unknown".to_vec()),
            hops: Some(1),
            packet_hash: None,
            observed_at: 1.0,
        });

        assert_eq!(store.len(), 1);
        assert!(store.destination_key(&unknown_hash).is_some());
        assert!(store.destination_key(&node_hash).is_none());
        assert!(store.destination_key(&delivery_hash).is_none());
        assert!(store.destination_key(&propagation_hash).is_none());
    }

    #[test]
    fn destination_key_store_loads_python_known_destinations_file() {
        let dir = std::env::temp_dir().join(format!("omen-rns-net-known-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("storage")).expect("storage dir");
        let mut public_key = [0u8; 64];
        public_key[32..64].copy_from_slice(&[8u8; 32]);
        let mut known = std::collections::HashMap::new();
        known.insert(
            [6u8; 16],
            rns_net::storage::KnownDestination {
                identity_hash: rns_core::hash::truncated_hash(&public_key),
                public_key,
                app_data: None,
                hops: 2,
                received_at: 1.0,
                receiving_interface: 0,
                was_used: false,
                last_used_at: None,
                retained: true,
            },
        );
        rns_net::storage::save_known_destinations(&known, &dir.join("storage/known_destinations"))
            .expect("save known destinations");

        let store =
            RnsNetDestinationKeyStore::load_known_destinations_from_config_dir(&dir).expect("load");

        assert_eq!(store.signing_public_key(&[6u8; 16]), Some([8u8; 32]));
        assert_eq!(
            store.destination_key(&[6u8; 16]).and_then(|key| key.hops),
            Some(2)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn destination_key_store_recent_loader_prunes_stale_known_destinations() {
        let dir =
            std::env::temp_dir().join(format!("omen-rns-net-known-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("storage")).expect("storage dir");
        let path = dir.join("storage/known_destinations");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_secs_f64();
        let mut recent_public_key = [0u8; 64];
        recent_public_key[32..64].copy_from_slice(&[8u8; 32]);
        let mut stale_public_key = [0u8; 64];
        stale_public_key[32..64].copy_from_slice(&[9u8; 32]);
        let mut known = std::collections::HashMap::new();
        known.insert(
            [6u8; 16],
            rns_net::storage::KnownDestination {
                identity_hash: rns_core::hash::truncated_hash(&recent_public_key),
                public_key: recent_public_key,
                app_data: None,
                hops: 2,
                received_at: now,
                receiving_interface: 0,
                was_used: false,
                last_used_at: None,
                retained: true,
            },
        );
        known.insert(
            [7u8; 16],
            rns_net::storage::KnownDestination {
                identity_hash: rns_core::hash::truncated_hash(&stale_public_key),
                public_key: stale_public_key,
                app_data: None,
                hops: 2,
                received_at: now - 7.0 * 60.0 * 60.0,
                receiving_interface: 0,
                was_used: false,
                last_used_at: None,
                retained: true,
            },
        );
        rns_net::storage::save_known_destinations(&known, &path).expect("save known destinations");

        let store = RnsNetDestinationKeyStore::load_recent_known_destinations_from_config_dir(
            &dir,
            6.0 * 60.0 * 60.0,
        )
        .expect("load recent");

        assert!(store.destination_key(&[6u8; 16]).is_some());
        assert!(store.destination_key(&[7u8; 16]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn destination_key_store_loads_explicit_known_destinations_file() {
        let dir =
            std::env::temp_dir().join(format!("omen-rns-net-known-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("known_destinations");
        let mut public_key = [0u8; 64];
        public_key[32..64].copy_from_slice(&[4u8; 32]);
        let mut known = std::collections::HashMap::new();
        known.insert(
            [3u8; 16],
            rns_net::storage::KnownDestination {
                identity_hash: rns_core::hash::truncated_hash(&public_key),
                public_key,
                app_data: Some(b"node".to_vec()),
                hops: 1,
                received_at: 1.0,
                receiving_interface: 0,
                was_used: false,
                last_used_at: None,
                retained: true,
            },
        );
        rns_net::storage::save_known_destinations(&known, &path).expect("save known destinations");

        let store = RnsNetDestinationKeyStore::load_known_destinations_file(&path).expect("load");

        assert_eq!(store.signing_public_key(&[3u8; 16]), Some([4u8; 32]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn destination_key_store_saves_managed_known_destinations_file() {
        let dir =
            std::env::temp_dir().join(format!("omen-rns-net-known-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("reticulum/storage/known_destinations");
        let mut store = RnsNetDestinationKeyStore::default();
        let mut full_public_key = [0x66; 64];
        full_public_key[32..64].copy_from_slice(&[0x44; 32]);
        store.ingest(RnsNetAnnounceKey {
            destination_hash: [0x22; 16],
            identity_hash: [0x33; 16],
            signing_public_key: [0x44; 32],
            full_public_key,
            app_data: Some(b"Managed Node".to_vec()),
            hops: Some(1),
            packet_hash: Some([0x55; 32]),
            observed_at: 123.0,
        });

        store
            .save_known_destinations_file(&path)
            .expect("save managed known destinations");
        let loaded = RnsNetDestinationKeyStore::load_known_destinations_file(&path).expect("load");

        assert_eq!(loaded.signing_public_key(&[0x22; 16]), Some([0x44; 32]));
        assert_eq!(
            loaded
                .destination_key(&[0x22; 16])
                .and_then(|key| key.packet_hash),
            None
        );
        assert!(path.starts_with(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn known_destinations_fixture_round_trips_through_loader() {
        let dir =
            std::env::temp_dir().join(format!("omen-rns-net-known-fixture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("known_destinations");

        write_known_destinations_fixture(&path, [0x11; 16]).expect("write fixture");
        let store = RnsNetDestinationKeyStore::load_known_destinations_file(&path).expect("load");

        assert_eq!(store.signing_public_key(&[0x11; 16]), Some([0x42; 32]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
