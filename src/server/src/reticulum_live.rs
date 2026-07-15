use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand_core::OsRng;
use rmpv::Value;
use rns_transport::destination::link::LinkEvent;
use rns_transport::destination::{DestinationName, SingleInputDestination};
use rns_transport::hash::AddressHash;
use rns_transport::identity::PrivateIdentity;
use rns_transport::resource::ResourceEventKind;
use rns_transport::transport::{ReceivedPayloadMode, Transport, TransportConfig};
use rns_transport::PacketContext;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::live::{LiveServerStats, OmenchatLinkEvent, OmenchatLiveServer};
use crate::protocol::codec::decode_frame;
use crate::session::{ServerPeer, SessionEngine};
use crate::store::OmenchatStore;
use crate::transport::{
    LinkId, OmenchatTransport, OMENCHAT_LINK_CONTEXT, OMENCHAT_RESOURCE_METADATA_PREFIX,
};

#[path = "../../runtime/native/ifac_tcp.rs"]
mod ifac_tcp;

pub const OMENCHAT_RNS_APP_NAME: &str = "omenchat";
pub const NOMADNET_RNS_APP_NAME: &str = "nomadnetwork";

const TRANSPORT_QUEUE_ITEMS: usize = 256;
const TRANSPORT_CONTROL_ITEMS: usize = 32;
const TRANSPORT_QUEUE_BYTES: usize = 16 * 1024 * 1024;
const TRANSPORT_PER_LINK_BYTES: usize = 4 * 1024 * 1024;
const EVENT_QUEUE_ITEMS: usize = 512;
const EVENT_CONTROL_ITEMS: usize = 64;
const EVENT_QUEUE_BYTES: usize = 32 * 1024 * 1024;
const EVENT_PER_LINK_BYTES: usize = 8 * 1024 * 1024;
const CONTROL_QUEUE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueMetricsSnapshot {
    pub queued_items: usize,
    pub queued_bytes: usize,
    pub rejected_items: u64,
    pub oldest_age_ms: u64,
}

impl QueueMetricsSnapshot {
    fn summary(&self, name: &str) -> String {
        format!(
            "{name}=items:{} bytes:{} oldest_ms:{} rejected:{}",
            self.queued_items, self.queued_bytes, self.oldest_age_ms, self.rejected_items
        )
    }
}

#[derive(Clone)]
struct EventQueueSender {
    payload_tx: mpsc::Sender<Queued<OmenchatLinkEvent>>,
    control_tx: mpsc::Sender<Queued<OmenchatLinkEvent>>,
    budget: Arc<QueueBudget>,
    log_path: std::path::PathBuf,
}

impl EventQueueSender {
    async fn send_control(&self, event: OmenchatLinkEvent) {
        let link_id = event_link_id(&event);
        let Some(permit) = self.budget.reserve(link_id, 0) else {
            return;
        };
        let queued = Queued {
            value: event,
            _permit: permit,
        };
        match tokio::time::timeout(CONTROL_QUEUE_TIMEOUT, self.control_tx.send(queued)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                self.budget.reject();
                append_server_log_warning_path(
                    &self.log_path,
                    "reticulum-rs event control queue stopped",
                );
            }
            Err(_) => {
                self.budget.reject();
                append_server_log_warning_path(
                    &self.log_path,
                    "reticulum-rs event control queue timed out",
                );
            }
        }
    }

    fn try_send_payload(&self, link_id: LinkId, bytes: usize, event: OmenchatLinkEvent) {
        let Some(permit) = self.budget.reserve(link_id, bytes) else {
            append_server_log_warning_path(
                &self.log_path,
                format!(
                    "reticulum-rs event queue overloaded link={} bytes={} action=drop",
                    hex_lower(&link_id),
                    bytes
                ),
            );
            return;
        };
        if let Err(error) = self.payload_tx.try_send(Queued {
            value: event,
            _permit: permit,
        }) {
            self.budget.reject();
            append_server_log_warning_path(
                &self.log_path,
                format!(
                    "reticulum-rs event queue overloaded link={} bytes={} action=drop error={error}",
                    hex_lower(&link_id),
                    bytes
                ),
            );
        }
    }
}

#[derive(Debug)]
struct QueueBudget {
    max_bytes: usize,
    max_link_bytes: usize,
    queued_items: AtomicUsize,
    queued_bytes: AtomicUsize,
    rejected_items: AtomicU64,
    oldest_epoch_ms: AtomicU64,
    next_reservation_id: AtomicU64,
    state: Mutex<QueueBudgetState>,
}

#[derive(Debug, Default)]
struct QueueBudgetState {
    link_bytes: BTreeMap<LinkId, usize>,
    pending_epochs: BTreeMap<u64, u64>,
}

impl QueueBudget {
    fn new(max_bytes: usize, max_link_bytes: usize) -> Arc<Self> {
        Arc::new(Self {
            max_bytes,
            max_link_bytes,
            queued_items: AtomicUsize::new(0),
            queued_bytes: AtomicUsize::new(0),
            rejected_items: AtomicU64::new(0),
            oldest_epoch_ms: AtomicU64::new(0),
            next_reservation_id: AtomicU64::new(1),
            state: Mutex::new(QueueBudgetState::default()),
        })
    }

    fn reserve(self: &Arc<Self>, link_id: LinkId, bytes: usize) -> Option<QueuePermit> {
        if bytes > self.max_bytes || bytes > self.max_link_bytes {
            self.rejected_items.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let link_total = state.link_bytes.get(&link_id).copied().unwrap_or(0);
        let total = self.queued_bytes.load(Ordering::Acquire);
        if link_total.saturating_add(bytes) > self.max_link_bytes
            || total.saturating_add(bytes) > self.max_bytes
        {
            self.rejected_items.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let reservation_id = self.next_reservation_id.fetch_add(1, Ordering::Relaxed);
        let reserved_epoch_ms = current_epoch_ms();
        state.link_bytes.insert(link_id, link_total + bytes);
        state
            .pending_epochs
            .insert(reservation_id, reserved_epoch_ms);
        self.queued_bytes.fetch_add(bytes, Ordering::AcqRel);
        if self.queued_items.fetch_add(1, Ordering::AcqRel) == 0 {
            self.oldest_epoch_ms
                .store(reserved_epoch_ms, Ordering::Release);
        }
        Some(QueuePermit {
            budget: self.clone(),
            link_id,
            bytes,
            reservation_id,
        })
    }

    fn reject(&self) {
        self.rejected_items.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> QueueMetricsSnapshot {
        let oldest = self.oldest_epoch_ms.load(Ordering::Acquire);
        QueueMetricsSnapshot {
            queued_items: self.queued_items.load(Ordering::Acquire),
            queued_bytes: self.queued_bytes.load(Ordering::Acquire),
            rejected_items: self.rejected_items.load(Ordering::Relaxed),
            oldest_age_ms: if oldest == 0 {
                0
            } else {
                current_epoch_ms().saturating_sub(oldest)
            },
        }
    }
}

#[derive(Debug)]
struct QueuePermit {
    budget: Arc<QueueBudget>,
    link_id: LinkId,
    bytes: usize,
    reservation_id: u64,
}

impl Drop for QueuePermit {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(link_total) = state.link_bytes.get_mut(&self.link_id) {
            *link_total = link_total.saturating_sub(self.bytes);
            if *link_total == 0 {
                state.link_bytes.remove(&self.link_id);
            }
        }
        state.pending_epochs.remove(&self.reservation_id);
        let next_oldest = state
            .pending_epochs
            .first_key_value()
            .map(|(_, epoch)| *epoch);
        self.budget
            .queued_bytes
            .fetch_sub(self.bytes, Ordering::AcqRel);
        if self.budget.queued_items.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.budget.oldest_epoch_ms.store(0, Ordering::Release);
        } else if let Some(next_oldest) = next_oldest {
            self.budget
                .oldest_epoch_ms
                .store(next_oldest, Ordering::Release);
        }
    }
}

#[derive(Debug)]
struct Queued<T> {
    value: T,
    _permit: QueuePermit,
}

fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub struct ReticulumLiveRuntime {
    transport: Arc<Transport>,
    destination: Arc<tokio::sync::Mutex<SingleInputDestination>>,
    nomadnet_destination: Arc<tokio::sync::Mutex<SingleInputDestination>>,
    pub identity_hash: [u8; 16],
    pub destination_hash: [u8; 16],
    pub nomadnet_destination_hash: [u8; 16],
    pub destination_name: String,
    pub nomadnet_destination_name: String,
    event_control_rx: mpsc::Receiver<Queued<OmenchatLinkEvent>>,
    event_rx: mpsc::Receiver<Queued<OmenchatLinkEvent>>,
    event_queue_budget: Arc<QueueBudget>,
    transport_queue_budget: Arc<QueueBudget>,
    pub live_server: LiveServerWorker<ReticulumOmenchatTransport>,
    interface_statuses: Vec<ReticulumInterfaceStatus>,
}

pub struct LiveServerWorker<T> {
    server: Arc<std::sync::Mutex<OmenchatLiveServer<T>>>,
    permit: Arc<tokio::sync::Semaphore>,
    metrics: Arc<LiveServerWorkerMetrics>,
}

#[derive(Default)]
struct LiveServerWorkerMetrics {
    in_flight: AtomicUsize,
    completed: AtomicU64,
    rejected: AtomicU64,
    total_micros: AtomicU64,
    max_micros: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveServerWorkerMetricsSnapshot {
    pub in_flight: usize,
    pub completed: u64,
    pub rejected: u64,
    pub average_micros: u64,
    pub max_micros: u64,
}

impl LiveServerWorkerMetricsSnapshot {
    fn summary(self) -> String {
        format!(
            "db-worker: in_flight={} completed={} rejected={} latency_avg_us={} latency_max_us={}",
            self.in_flight, self.completed, self.rejected, self.average_micros, self.max_micros
        )
    }
}

impl<T> LiveServerWorker<T>
where
    T: OmenchatTransport + Send + 'static,
{
    fn new(server: OmenchatLiveServer<T>) -> Self {
        Self {
            server: Arc::new(std::sync::Mutex::new(server)),
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
            metrics: Arc::new(LiveServerWorkerMetrics::default()),
        }
    }

    pub async fn handle_event(&self, event: OmenchatLinkEvent) -> ServerResult<()> {
        let permit = self.permit.clone().try_acquire_owned().map_err(|_| {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            ServerError::Message("live-server worker is busy".into())
        })?;
        let server = self.server.clone();
        let metrics = self.metrics.clone();
        metrics.in_flight.store(1, Ordering::Release);
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let started = Instant::now();
            let result = match server.lock() {
                Ok(mut server) => server.handle_event(event),
                Err(_) => Err(ServerError::Message(
                    "live-server worker lock poisoned".into(),
                )),
            };
            let elapsed = started.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
            (result, elapsed)
        })
        .await;
        metrics.in_flight.store(0, Ordering::Release);
        let (result, elapsed) = task
            .map_err(|error| ServerError::Message(format!("live-server worker failed: {error}")))?;
        metrics.total_micros.fetch_add(elapsed, Ordering::Relaxed);
        metrics.max_micros.fetch_max(elapsed, Ordering::Relaxed);
        metrics.completed.fetch_add(1, Ordering::Relaxed);
        result
    }

    pub fn worker_metrics(&self) -> LiveServerWorkerMetricsSnapshot {
        let completed = self.metrics.completed.load(Ordering::Relaxed);
        let total_micros = self.metrics.total_micros.load(Ordering::Relaxed);
        LiveServerWorkerMetricsSnapshot {
            in_flight: self.metrics.in_flight.load(Ordering::Acquire),
            completed,
            rejected: self.metrics.rejected.load(Ordering::Relaxed),
            average_micros: total_micros.checked_div(completed).unwrap_or(0),
            max_micros: self.metrics.max_micros.load(Ordering::Relaxed),
        }
    }

    pub fn stats(&self) -> LiveServerStats {
        self.server
            .lock()
            .expect("live-server worker lock")
            .stats()
            .clone()
    }

    pub fn recent_closed_link_summaries(&self) -> Vec<crate::live::ClosedLinkSummary> {
        self.server
            .lock()
            .expect("live-server worker lock")
            .recent_closed_link_summaries()
    }

    pub fn active_room_counts(&self) -> Vec<(crate::protocol::RoomId, usize)> {
        self.server
            .lock()
            .expect("live-server worker lock")
            .active_room_counts()
    }

    pub fn active_link_summaries(&self) -> Vec<crate::live::ActiveLinkSummary> {
        self.server
            .lock()
            .expect("live-server worker lock")
            .active_link_summaries()
    }

    pub fn active_identity_counts(&self) -> Vec<(Vec<u8>, usize)> {
        self.server
            .lock()
            .expect("live-server worker lock")
            .active_identity_counts()
    }

    pub fn disconnect_identity(&self, identity_hash: &[u8]) -> usize {
        self.server
            .lock()
            .expect("live-server worker lock")
            .disconnect_identity(identity_hash)
    }
}

#[derive(Clone)]
struct ReticulumInterfaceStatus {
    label: String,
    kind: ReticulumInterfaceStatusKind,
}

#[derive(Clone)]
enum ReticulumInterfaceStatusKind {
    TcpClient(rns_transport::iface::tcp_client::TcpRuntimeStatusHandle),
    IfacTcpClient(ifac_tcp::IfacTcpRuntimeStatusHandle),
    TcpServer(rns_transport::iface::tcp_server::TcpListenerRuntimeStatusHandle),
}

impl ReticulumInterfaceStatus {
    fn line(&self) -> String {
        match &self.kind {
            ReticulumInterfaceStatusKind::TcpClient(handle) => {
                let status = handle.to_json();
                let state = json_str(&status, "stream_state");
                let last_error = json_str(&status, "last_error");
                let received = json_u64(&status, "bytes_rx");
                let sent = json_u64(&status, "bytes_tx");
                if last_error == "none" {
                    format!(
                        "{} state={} traffic_in={} traffic_out={}",
                        self.label,
                        state,
                        human_bytes(received),
                        human_bytes(sent)
                    )
                } else {
                    format!(
                        "{} state={} error={} traffic_in={} traffic_out={}",
                        self.label,
                        state,
                        last_error,
                        human_bytes(received),
                        human_bytes(sent)
                    )
                }
            }
            ReticulumInterfaceStatusKind::IfacTcpClient(handle) => {
                let status = handle.to_json();
                let state = json_str(&status, "stream_state");
                let last_error = json_str(&status, "last_error");
                let received = json_u64(&status, "bytes_rx");
                let sent = json_u64(&status, "bytes_tx");
                if last_error == "none" {
                    format!(
                        "{} state={} traffic_in={} traffic_out={}",
                        self.label,
                        state,
                        human_bytes(received),
                        human_bytes(sent)
                    )
                } else {
                    format!(
                        "{} state={} error={} traffic_in={} traffic_out={}",
                        self.label,
                        state,
                        last_error,
                        human_bytes(received),
                        human_bytes(sent)
                    )
                }
            }
            ReticulumInterfaceStatusKind::TcpServer(handle) => {
                let status = handle.to_json();
                let state = json_str(&status, "listener_state");
                let accepted = json_u64(&status, "accepted_connections");
                let errors = json_u64(&status, "accept_errors");
                let last_error = json_str(&status, "last_error");
                if last_error == "none" {
                    format!(
                        "{} state={} accepted={} accept_errors={}",
                        self.label, state, accepted, errors
                    )
                } else {
                    format!(
                        "{} state={} accepted={} accept_errors={} error={}",
                        self.label, state, accepted, errors, last_error
                    )
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterfaceHealth {
    Connected,
    NoInterfaces,
}

impl InterfaceHealth {
    pub fn needs_runtime_restart(self) -> bool {
        false
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::NoInterfaces => "no interfaces configured",
        }
    }
}

#[derive(Clone)]
pub struct ReticulumOmenchatTransport {
    tx: mpsc::Sender<Queued<TransportCommand>>,
    control_tx: mpsc::Sender<Queued<TransportCommand>>,
    queue_budget: Arc<QueueBudget>,
    sent_frames: Arc<AtomicU64>,
    offered_resources: Arc<AtomicU64>,
    sent_frame_bytes: Arc<AtomicU64>,
    offered_resource_bytes: Arc<AtomicU64>,
}

enum TransportCommand {
    SendFrame {
        link_id: LinkId,
        frame_bytes: Vec<u8>,
    },
    OfferResource {
        link_id: LinkId,
        offer: ResourceOffer,
    },
    CloseLink {
        link_id: LinkId,
    },
}

struct ResourceOffer {
    payload: Vec<u8>,
    metadata: Vec<u8>,
}

impl ResourceOffer {
    fn new(payload: Vec<u8>, metadata: Vec<u8>) -> Self {
        Self { payload, metadata }
    }

    fn queued_bytes(&self) -> usize {
        self.payload.len().saturating_add(self.metadata.len())
    }
}

impl ReticulumOmenchatTransport {
    fn new(transport: Arc<Transport>, log_path: std::path::PathBuf) -> Self {
        let (tx, mut rx) = mpsc::channel::<Queued<TransportCommand>>(TRANSPORT_QUEUE_ITEMS);
        let (control_tx, mut control_rx) =
            mpsc::channel::<Queued<TransportCommand>>(TRANSPORT_CONTROL_ITEMS);
        let queue_budget = QueueBudget::new(TRANSPORT_QUEUE_BYTES, TRANSPORT_PER_LINK_BYTES);
        let sent_frames = Arc::new(AtomicU64::new(0));
        let offered_resources = Arc::new(AtomicU64::new(0));
        let sent_frame_bytes = Arc::new(AtomicU64::new(0));
        let offered_resource_bytes = Arc::new(AtomicU64::new(0));
        tokio::spawn(async move {
            let mut control_open = true;
            let mut payload_open = true;
            while control_open || payload_open {
                let selected = tokio::select! {
                    biased;
                    queued = control_rx.recv(), if control_open => match queued {
                        Some(queued) => {
                            let Queued { value, _permit } = queued;
                            Some((value, Some(_permit)))
                        }
                        None => {
                            control_open = false;
                            None
                        }
                    },
                    queued = rx.recv(), if payload_open => match queued {
                        Some(queued) => {
                            let Queued { value, _permit } = queued;
                            Some((value, Some(_permit)))
                        }
                        None => {
                            payload_open = false;
                            None
                        }
                    },
                };
                let Some((command, _permit)) = selected else {
                    continue;
                };
                match command {
                    TransportCommand::SendFrame {
                        link_id,
                        frame_bytes,
                    } => {
                        let link_hash = AddressHash::new(link_id);
                        let link = transport.find_in_link(&link_hash).await;
                        let Some(link) = link else {
                            append_server_log_error_path(
                                &log_path,
                                format!(
                                    "reticulum-rs OMENchat frame send failed link={} bytes={} error=inbound link not found",
                                    hex_lower(&link_id),
                                    frame_bytes.len()
                                ),
                            );
                            continue;
                        };
                        match rns_transport::delivery::send_on_link(
                            &transport,
                            &link,
                            &frame_bytes,
                        )
                        .await
                        {
                            Ok(result) => {
                                append_server_log_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs OMENchat frame sent link={} result={result:?} bytes={} context=0x00",
                                        hex_lower(&link_id),
                                        frame_bytes.len()
                                    ),
                                );
                            }
                            Err(error) => append_server_log_error_path(
                                &log_path,
                                format!(
                                    "reticulum-rs OMENchat frame send failed link={} bytes={} error={error:?}",
                                    hex_lower(&link_id),
                                    frame_bytes.len()
                                ),
                            ),
                        }
                    }
                    TransportCommand::OfferResource { link_id, offer } => {
                        let payload_bytes = offer.payload.len();
                        match transport
                            .send_resource(
                                &AddressHash::new(link_id),
                                offer.payload,
                                Some(offer.metadata),
                            )
                            .await
                        {
                            Ok(hash) => {
                                append_server_log_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs OMENchat resource offered link={} hash={} bytes={}",
                                        hex_lower(&link_id),
                                        hash,
                                        payload_bytes
                                    ),
                                );
                            }
                            Err(error) => append_server_log_error_path(
                                &log_path,
                                format!(
                                    "reticulum-rs OMENchat offer resource failed link={} bytes={} error={error:?}",
                                    hex_lower(&link_id),
                                    payload_bytes
                                ),
                            ),
                        }
                    }
                    TransportCommand::CloseLink { link_id } => {
                        let channel = transport.channel(AddressHash::new(link_id));
                        if let Err(error) = channel.close().await {
                            append_server_log_error_path(
                                &log_path,
                                format!(
                                    "reticulum-rs OMENchat close link failed link={} error={error:?}",
                                    hex_lower(&link_id)
                                ),
                            );
                        }
                    }
                }
            }
        });

        Self {
            tx,
            control_tx,
            queue_budget,
            sent_frames,
            offered_resources,
            sent_frame_bytes,
            offered_resource_bytes,
        }
    }
}

impl OmenchatTransport for ReticulumOmenchatTransport {
    fn send_frame(&mut self, link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
        let byte_count = frame_bytes.len() as u64;
        let permit = self
            .queue_budget
            .reserve(link_id, frame_bytes.len())
            .ok_or_else(|| transport_overload_error("frame", frame_bytes.len()))?;
        self.tx
            .try_send(Queued {
                value: TransportCommand::SendFrame {
                    link_id,
                    frame_bytes,
                },
                _permit: permit,
            })
            .map_err(|error| {
                self.queue_budget.reject();
                ServerError::Message(format!(
                    "reticulum-rs transport overloaded: frame queue unavailable ({error})"
                ))
            })?;
        self.sent_frames.fetch_add(1, Ordering::Relaxed);
        self.sent_frame_bytes
            .fetch_add(byte_count, Ordering::Relaxed);
        Ok(())
    }

    fn send_frame_with_context(
        &mut self,
        link_id: LinkId,
        frame_bytes: Vec<u8>,
        _context: u8,
    ) -> ServerResult<()> {
        self.send_frame(link_id, frame_bytes)
    }

    fn offer_resource(
        &mut self,
        link_id: LinkId,
        _resource_id: String,
        payload: Vec<u8>,
        metadata: Vec<u8>,
    ) -> ServerResult<()> {
        let byte_count = payload.len() as u64;
        let offer = ResourceOffer::new(payload, metadata);
        let queued_bytes = offer.queued_bytes();
        let permit = self
            .queue_budget
            .reserve(link_id, queued_bytes)
            .ok_or_else(|| transport_overload_error("resource", queued_bytes))?;
        self.tx
            .try_send(Queued {
                value: TransportCommand::OfferResource { link_id, offer },
                _permit: permit,
            })
            .map_err(|error| {
                self.queue_budget.reject();
                ServerError::Message(format!(
                    "reticulum-rs transport overloaded: resource queue unavailable ({error})"
                ))
            })?;
        self.offered_resources.fetch_add(1, Ordering::Relaxed);
        self.offered_resource_bytes
            .fetch_add(byte_count, Ordering::Relaxed);
        Ok(())
    }

    fn sent_frame_count(&self) -> u64 {
        self.sent_frames.load(Ordering::Relaxed)
    }

    fn offered_resource_count(&self) -> u64 {
        self.offered_resources.load(Ordering::Relaxed)
    }

    fn sent_frame_bytes(&self) -> u64 {
        self.sent_frame_bytes.load(Ordering::Relaxed)
    }

    fn offered_resource_bytes(&self) -> u64 {
        self.offered_resource_bytes.load(Ordering::Relaxed)
    }

    fn close_link(&mut self, link_id: LinkId) -> ServerResult<()> {
        let permit = self
            .queue_budget
            .reserve(link_id, 0)
            .ok_or_else(|| transport_overload_error("control", 0))?;
        self.control_tx
            .try_send(Queued {
                value: TransportCommand::CloseLink { link_id },
                _permit: permit,
            })
            .map_err(|error| {
                self.queue_budget.reject();
                ServerError::Message(format!(
                    "reticulum-rs transport control queue unavailable ({error})"
                ))
            })
    }
}

fn transport_overload_error(kind: &str, bytes: usize) -> ServerError {
    ServerError::Message(format!(
        "reticulum-rs transport overloaded: rejected {kind} bytes={bytes}"
    ))
}

fn event_link_id(event: &OmenchatLinkEvent) -> LinkId {
    match event {
        OmenchatLinkEvent::LinkOpened { link_id, .. }
        | OmenchatLinkEvent::LinkData { link_id, .. }
        | OmenchatLinkEvent::ResourceReceived { link_id, .. }
        | OmenchatLinkEvent::LinkClosed { link_id, .. } => *link_id,
    }
}

pub fn configured_destination_status(config: &ServerConfig) -> ServerResult<String> {
    crate::config::init_files(config)?;
    let identity = load_or_create_identity(config)?;
    let destination = destination_for_identity(&identity);
    let nomadnet_destination = nomadnet_destination_for_identity(&identity);
    let omenchat_destination_hash = destination.desc.address_hash.to_hex_string();
    crate::config::ensure_nomadnet_portal(config, &omenchat_destination_hash)?;
    let nomadnet_destination_hash = nomadnet_destination.desc.address_hash.to_hex_string();
    Ok(format!(
        "identity hash: {}\ndestination: omenchat.node ({})\nclient uri: omenchat://{}\nnomadnet portal: nomadnetwork.node ({}) path={}\nportal url: {}:{}\n",
        identity.address_hash().to_hex_string(),
        omenchat_destination_hash,
        omenchat_destination_hash,
        nomadnet_destination_hash,
        crate::config::NOMADNET_PORTAL_PATH,
        nomadnet_destination_hash,
        crate::config::NOMADNET_PORTAL_PATH
    ))
}

pub fn run_live_server(config: ServerConfig) -> ServerResult<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| ServerError::Message(format!("tokio runtime failed: {error}")))?;
    runtime.block_on(run_live_server_async(config))
}

async fn run_live_server_async(config: ServerConfig) -> ServerResult<()> {
    append_server_log(
        &config,
        format!(
            "reticulum-rs live server starting config={} reticulum_config={} announce_interval_minutes={}",
            config.config_path.display(),
            config.reticulum_config_file().display(),
            config.announce_interval_minutes.max(1)
        ),
    );
    let mut runtime = start_live_server(&config).await?;
    let announce_interval = Duration::from_secs(config.announce_interval_minutes.max(1) * 60);
    let stats_interval = Duration::from_secs(30);
    let mut next_announce = Instant::now() + announce_interval;
    let mut next_stats = Instant::now() + stats_interval;

    println!("omenchatd reticulum-rs live server ready");
    println!(
        "destination: {} ({})",
        runtime.destination_name,
        hex_lower(&runtime.destination_hash)
    );
    println!(
        "client uri: omenchat://{}",
        hex_lower(&runtime.destination_hash)
    );
    println!(
        "nomadnet portal: {} ({}) {}",
        runtime.nomadnet_destination_name,
        hex_lower(&runtime.nomadnet_destination_hash),
        crate::config::NOMADNET_PORTAL_PATH
    );
    println!("database: {}", config.database_path.display());
    println!("reticulum: {}", config.reticulum_config_path.display());

    loop {
        while let Some(event) = runtime.try_recv_event() {
            if let Err(error) = runtime.live_server.handle_event(event).await {
                append_server_log_error(
                    &config,
                    format!("reticulum-rs live event failed: {error}"),
                );
            }
        }

        if Instant::now() >= next_announce {
            announce_destinations(
                &runtime.transport,
                &runtime.destination,
                &runtime.nomadnet_destination,
                &config,
            )
            .await?;
            next_announce = Instant::now() + announce_interval;
        }
        if Instant::now() >= next_stats {
            let stats = runtime.live_server.stats();
            println!("{}", stats.summary_line());
            append_server_log(&config, stats.summary_line());
            let (transport_queue, event_queue) = runtime.queue_metrics();
            let queue_line = format!(
                "queues: {} {} {} {}",
                transport_queue.summary("transport"),
                event_queue.summary("events"),
                runtime.live_server.worker_metrics().summary(),
                crate::server_log::metrics().summary()
            );
            println!("{queue_line}");
            append_server_log(&config, queue_line);
            next_stats = Instant::now() + stats_interval;
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub async fn start_live_server(config: &ServerConfig) -> ServerResult<ReticulumLiveRuntime> {
    crate::config::init_files(config)?;
    let identity = load_or_create_identity(config)?;
    let mut transport_config = TransportConfig::new("omenchatd", &identity, true);
    transport_config.set_ratchet_store_path(config.reticulum_storage_path().join("ratchets"));
    let mut transport = Transport::new(transport_config);
    let destination = transport
        .add_destination(
            identity.clone(),
            DestinationName::new(OMENCHAT_RNS_APP_NAME, "node"),
        )
        .await;
    let nomadnet_destination = transport
        .add_destination(
            identity.clone(),
            DestinationName::new(NOMADNET_RNS_APP_NAME, "node"),
        )
        .await;
    let transport = Arc::new(transport);
    let attached = attach_configured_interfaces(&transport, config).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    append_server_log(
        config,
        format!(
            "reticulum-rs attached interface(s): {}",
            format_interface_status_lines(&attached).join("; ")
        ),
    );
    if attached
        .iter()
        .any(|interface| interface.label.contains("ifac=configured"))
    {
        append_server_log(
            config,
            "reticulum-rs IFAC TCP adapter active for configured private gateways",
        );
    }

    let destination_hash = destination.lock().await.desc.address_hash;
    let nomadnet_destination_hash = nomadnet_destination.lock().await.desc.address_hash;
    crate::config::ensure_nomadnet_portal(config, &destination_hash.to_hex_string())?;
    announce_destinations(&transport, &destination, &nomadnet_destination, config).await?;

    let (event_payload_tx, event_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
    let (event_control_tx, event_control_rx) = mpsc::channel(EVENT_CONTROL_ITEMS);
    let event_queue_budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
    let event_tx = EventQueueSender {
        payload_tx: event_payload_tx,
        control_tx: event_control_tx,
        budget: event_queue_budget.clone(),
        log_path: config.log_path(),
    };
    spawn_link_event_bridge(transport.clone(), event_tx.clone());
    spawn_received_data_bridge(transport.clone(), event_tx.clone());
    spawn_resource_event_bridge(transport.clone(), event_tx, config.clone());

    let store = OmenchatStore::open(&config.database_path)?;
    let engine =
        SessionEngine::with_limits_and_motd(store, config.into(), Some(config.motd.clone()));
    let transport_impl = ReticulumOmenchatTransport::new(transport.clone(), config.log_path());
    let transport_queue_budget = transport_impl.queue_budget.clone();

    append_server_log(
        config,
        format!(
            "reticulum-rs live server ready destination=omenchat.node hash={} client_uri=omenchat://{} nomadnet_hash={}",
            destination_hash.to_hex_string(),
            destination_hash.to_hex_string(),
            nomadnet_destination_hash.to_hex_string()
        ),
    );

    Ok(ReticulumLiveRuntime {
        transport,
        destination,
        nomadnet_destination,
        identity_hash: address_hash_bytes(*identity.address_hash()),
        destination_hash: address_hash_bytes(destination_hash),
        nomadnet_destination_hash: address_hash_bytes(nomadnet_destination_hash),
        destination_name: "omenchat.node".into(),
        nomadnet_destination_name: "nomadnetwork.node".into(),
        event_control_rx,
        event_rx,
        event_queue_budget,
        transport_queue_budget,
        live_server: LiveServerWorker::new(OmenchatLiveServer::new(engine, transport_impl)),
        interface_statuses: attached,
    })
}

impl ReticulumLiveRuntime {
    pub async fn announce(&mut self, config: &ServerConfig) -> ServerResult<()> {
        announce_destinations(
            &self.transport,
            &self.destination,
            &self.nomadnet_destination,
            config,
        )
        .await
    }

    pub fn interface_stats_lines(&self) -> Vec<String> {
        if self.interface_statuses.is_empty() {
            vec!["interfaces: 0 configured".into()]
        } else {
            self.interface_statuses
                .iter()
                .map(ReticulumInterfaceStatus::line)
                .collect()
        }
    }

    pub fn interface_health(&self) -> InterfaceHealth {
        if self.interface_statuses.is_empty() {
            InterfaceHealth::NoInterfaces
        } else {
            InterfaceHealth::Connected
        }
    }

    pub fn queue_metrics(&self) -> (QueueMetricsSnapshot, QueueMetricsSnapshot) {
        (
            self.transport_queue_budget.snapshot(),
            self.event_queue_budget.snapshot(),
        )
    }

    pub fn queue_summary_line(&self) -> String {
        let (transport_queue, event_queue) = self.queue_metrics();
        format!(
            "queues: {} {} {} {}",
            transport_queue.summary("transport"),
            event_queue.summary("events"),
            self.live_server.worker_metrics().summary(),
            crate::server_log::metrics().summary()
        )
    }

    fn try_recv_event(&mut self) -> Option<OmenchatLinkEvent> {
        match self.event_control_rx.try_recv() {
            Ok(queued) => return Some(queued.value),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
            | Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }
        self.event_rx.try_recv().ok().map(|queued| queued.value)
    }
}

pub async fn drain_live_events_logged(
    runtime: &mut ReticulumLiveRuntime,
    max_events: usize,
    config: &ServerConfig,
) -> ServerResult<usize> {
    let mut drained = 0usize;
    while drained < max_events {
        match runtime.try_recv_event() {
            Some(event) => {
                append_server_log(config, describe_live_event(&event));
                runtime.live_server.handle_event(event).await?;
                drained += 1;
            }
            None => break,
        }
    }
    Ok(drained)
}

async fn announce_destinations(
    transport: &Transport,
    destination: &Arc<tokio::sync::Mutex<SingleInputDestination>>,
    nomadnet_destination: &Arc<tokio::sync::Mutex<SingleInputDestination>>,
    config: &ServerConfig,
) -> ServerResult<()> {
    let (destination_hash, destination_trace) =
        send_announce_broadcast(transport, destination, Some(config.name.as_bytes())).await?;
    let (nomadnet_destination_hash, nomadnet_trace) = send_announce_broadcast(
        transport,
        nomadnet_destination,
        Some(config.name.as_bytes()),
    )
    .await?;
    append_server_log(
        config,
        format!(
            "reticulum-rs announce sent destination=omenchat.node hash={} dispatch=matched:{} sent:{} queued:{} failed:{} nomadnet_hash={} nomadnet_dispatch=matched:{} sent:{} queued:{} failed:{} next_announce_minutes={}",
            destination_hash,
            destination_trace.matched_ifaces,
            destination_trace.sent_ifaces,
            destination_trace.queued_ifaces,
            destination_trace.failed_ifaces,
            nomadnet_destination_hash,
            nomadnet_trace.matched_ifaces,
            nomadnet_trace.sent_ifaces,
            nomadnet_trace.queued_ifaces,
            nomadnet_trace.failed_ifaces,
            config.announce_interval_minutes.max(1)
        ),
    );
    Ok(())
}

async fn send_announce_broadcast(
    transport: &Transport,
    destination: &Arc<tokio::sync::Mutex<SingleInputDestination>>,
    app_data: Option<&[u8]>,
) -> ServerResult<(String, rns_transport::iface::TxDispatchTrace)> {
    transport
        .set_destination_announce_app_data(destination, app_data.map(Vec::from))
        .await;
    let (destination_hash, packet) = {
        let mut destination = destination.lock().await;
        let destination_hash = destination.desc.address_hash.to_hex_string();
        let packet = destination
            .announce(rand_core::OsRng, app_data)
            .map_err(|err| ServerError::Message(format!("Reticulum announce failed: {err:?}")))?;
        (destination_hash, packet)
    };
    let trace = transport
        .send_packet_broadcast_with_trace(packet)
        .await
        .dispatch;
    Ok((destination_hash, trace))
}

fn spawn_link_event_bridge(transport: Arc<Transport>, event_tx: EventQueueSender) {
    let log_path = event_tx.log_path.clone();
    let mut events = transport.in_link_events();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => match event.event {
                    LinkEvent::Activated => {
                        let link_id = address_hash_bytes(event.id);
                        append_server_log_path(
                            &log_path,
                            format!(
                                "reticulum-rs in-link activated link={} address_hash={}",
                                hex_lower(&link_id),
                                event.address_hash
                            ),
                        );
                        let peer = ServerPeer {
                            identity_hash: link_id.to_vec(),
                            display_name: format!("link-{}", &hex_lower(&link_id)[..8]),
                            lxmf_destination: None,
                        };
                        event_tx
                            .send_control(OmenchatLinkEvent::LinkOpened { link_id, peer })
                            .await;
                    }
                    LinkEvent::Data(payload) => {
                        let link_id = address_hash_bytes(event.id);
                        let decodes_as_omenchat = decode_frame(payload.as_slice()).is_ok();
                        if payload.context() as u8 != OMENCHAT_LINK_CONTEXT && !decodes_as_omenchat
                        {
                            continue;
                        }
                        append_server_log_path(
                            &log_path,
                            format!(
                                "reticulum-rs OMENchat link data observed link={} address_hash={} context=0x{:02x} bytes={} {}",
                                hex_lower(&link_id),
                                event.address_hash,
                                payload.context() as u8,
                                payload.as_slice().len(),
                                decoded_frame_summary(payload.as_slice())
                            ),
                        );
                        event_tx.try_send_payload(
                            link_id,
                            payload.as_slice().len(),
                            OmenchatLinkEvent::LinkData {
                                link_id,
                                context: payload.context() as u8,
                                data: payload.as_slice().to_vec(),
                            },
                        );
                    }
                    LinkEvent::Closed => {
                        let link_id = address_hash_bytes(event.id);
                        append_server_log_path(
                            &log_path,
                            format!("reticulum-rs in-link closed link={}", hex_lower(&link_id)),
                        );
                        event_tx
                            .send_control(OmenchatLinkEvent::LinkClosed {
                                link_id,
                                reason: Some("closed".into()),
                            })
                            .await;
                    }
                    LinkEvent::PeerIdentified(_) => {}
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    append_server_log_warning_path(
                        &log_path,
                        format!("reticulum-rs in-link event receiver lagged skipped={skipped}"),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn spawn_received_data_bridge(transport: Arc<Transport>, event_tx: EventQueueSender) {
    let log_path = event_tx.log_path.clone();
    let mut events = transport.received_data_events();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if event.payload_mode == ReceivedPayloadMode::FullWire {
                        continue;
                    }
                    if !matches!(event.context, None | Some(PacketContext::None)) {
                        continue;
                    }
                    if decode_frame(event.data.as_slice()).is_err() {
                        continue;
                    }
                    let link_id = address_hash_bytes(event.destination);
                    append_server_log_path(
                        &log_path,
                        format!(
                            "reticulum-rs OMENchat received-data frame observed link={} bytes={} {}",
                            hex_lower(&link_id),
                            event.data.as_slice().len(),
                            decoded_frame_summary(event.data.as_slice())
                        ),
                    );
                    event_tx.try_send_payload(
                        link_id,
                        event.data.as_slice().len(),
                        OmenchatLinkEvent::LinkData {
                            link_id,
                            context: 0,
                            data: event.data.as_slice().to_vec(),
                        },
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    append_server_log_warning_path(
                        &log_path,
                        format!("reticulum-rs received-data receiver lagged skipped={skipped}"),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn spawn_resource_event_bridge(
    transport: Arc<Transport>,
    event_tx: EventQueueSender,
    config: ServerConfig,
) {
    let log_path = config.log_path();
    let mut events = transport.resource_events();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let ResourceEventKind::Complete(complete) = event.kind else {
                        continue;
                    };
                    if complete.is_request {
                        let Some(request_id) = complete.request_id.clone() else {
                            append_server_log_warning_path(
                                &log_path,
                                format!(
                                    "reticulum-rs NomadNet resource request ignored link={} hash={} missing request_id",
                                    event.link_id, event.hash
                                ),
                            );
                            continue;
                        };
                        let Some(request_path) =
                            nomadnet_request_path_for_payload(&config, complete.data.as_slice())
                        else {
                            append_server_log_warning_path(
                                &log_path,
                                format!(
                                    "reticulum-rs NomadNet resource request ignored link={} hash={} unknown path hash bytes={}",
                                    event.link_id,
                                    event.hash,
                                    complete.data.len()
                                ),
                            );
                            continue;
                        };
                        match nomadnet_response_resource_payload(&config, &request_id) {
                            Ok(payload) => {
                                let response_bytes = payload.len();
                                match transport
                                    .send_response_resource(
                                        &event.link_id,
                                        request_id,
                                        payload,
                                        None,
                                    )
                                    .await
                                {
                                Ok(response_hash) => append_server_log_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs NomadNet response resource sent link={} request_path={} response_hash={} bytes={}",
                                        event.link_id,
                                        request_path,
                                        response_hash,
                                        response_bytes
                                    ),
                                ),
                                Err(error) => append_server_log_error_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs NomadNet response resource failed link={} request_path={} error={error:?}",
                                        event.link_id, request_path
                                    ),
                                ),
                                }
                            }
                            Err(error) => append_server_log_error_path(
                                &log_path,
                                format!(
                                    "reticulum-rs NomadNet response payload failed link={} request_path={} error={error}",
                                    event.link_id, request_path
                                ),
                            ),
                        }
                        continue;
                    }
                    let Some(metadata) = complete.metadata.clone() else {
                        continue;
                    };
                    if !metadata.starts_with(OMENCHAT_RESOURCE_METADATA_PREFIX) {
                        continue;
                    }
                    let link_id = address_hash_bytes(event.link_id);
                    append_server_log_path(
                        &log_path,
                        format!(
                            "reticulum-rs OMENchat resource received link={} hash={} bytes={} metadata_bytes={}",
                            hex_lower(&link_id),
                            event.hash,
                            complete.data.len(),
                            metadata.len()
                        ),
                    );
                    let queued_bytes = complete.data.len().saturating_add(metadata.len());
                    event_tx.try_send_payload(
                        link_id,
                        queued_bytes,
                        OmenchatLinkEvent::ResourceReceived {
                            link_id,
                            data: complete.data,
                            metadata: Some(metadata),
                        },
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    append_server_log_warning_path(
                        &log_path,
                        format!("reticulum-rs resource event receiver lagged skipped={skipped}"),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

async fn attach_configured_interfaces(
    transport: &Arc<Transport>,
    config: &ServerConfig,
) -> ServerResult<Vec<ReticulumInterfaceStatus>> {
    let interfaces = parse_reticulum_interfaces(&config.reticulum_config_file())?;
    let mut attached = Vec::new();
    for interface in interfaces {
        if !interface.enabled {
            continue;
        }
        match interface.kind.as_deref() {
            Some("TCPClientInterface") | Some("tcp_client") => {
                let Some(host) = interface.target_host.as_deref() else {
                    continue;
                };
                let Some(port) = interface.target_port else {
                    continue;
                };
                let address = format!("{host}:{port}");
                let manager = transport.iface_manager();
                let mut manager = manager.lock().await;
                let has_ifac = interface.network_name.is_some() || interface.passphrase.is_some();
                if has_ifac {
                    let client = ifac_tcp::IfacTcpClient::new(
                        address.clone(),
                        interface.network_name.clone(),
                        interface.passphrase.clone(),
                        16,
                    )
                    .map_err(|error| {
                        ServerError::Message(format!("IFAC TCP client setup failed: {error}"))
                    })?;
                    let status = client.runtime_status_handle();
                    let context = manager.new_context(client);
                    let iface_address = *context.channel.address();
                    tokio::spawn(ifac_tcp::IfacTcpClient::spawn(context));
                    attached.push(ReticulumInterfaceStatus {
                        label: format!(
                            "{} tcp_client {address} ifac=configured iface={}",
                            interface.name,
                            iface_address.to_hex_string()
                        ),
                        kind: ReticulumInterfaceStatusKind::IfacTcpClient(status),
                    });
                } else {
                    let client = rns_transport::iface::tcp_client::TcpClient::new(address.clone());
                    let status = client.runtime_status_handle();
                    let context = manager.new_context(client);
                    let iface_address = *context.channel.address();
                    tokio::spawn(rns_transport::iface::tcp_client::TcpClient::spawn(context));
                    attached.push(ReticulumInterfaceStatus {
                        label: format!(
                            "{} tcp_client {address} ifac=none iface={}",
                            interface.name,
                            iface_address.to_hex_string()
                        ),
                        kind: ReticulumInterfaceStatusKind::TcpClient(status),
                    });
                }
            }
            Some("TCPServerInterface") | Some("tcp_server") => {
                let listen_ip = interface.listen_ip.as_deref().unwrap_or("127.0.0.1");
                let Some(port) = interface.listen_port else {
                    continue;
                };
                let address = format!("{listen_ip}:{port}");
                let manager = transport.iface_manager();
                let server = rns_transport::iface::tcp_server::TcpServer::new(
                    address.clone(),
                    manager.clone(),
                );
                let status = server.runtime_status_handle();
                let mut manager = manager.lock().await;
                let context = manager.new_context(server);
                let iface_address = *context.channel.address();
                let ifac_status = apply_ifac(&mut manager, iface_address, &interface);
                tokio::spawn(rns_transport::iface::tcp_server::TcpServer::spawn(context));
                attached.push(ReticulumInterfaceStatus {
                    label: format!(
                        "{} tcp_server {address} ifac={ifac_status} iface={}",
                        interface.name,
                        iface_address.to_hex_string()
                    ),
                    kind: ReticulumInterfaceStatusKind::TcpServer(status),
                });
            }
            _ => {}
        }
    }
    Ok(attached)
}

fn apply_ifac(
    manager: &mut rns_transport::iface::InterfaceManager,
    iface: AddressHash,
    interface: &ReticulumInterface,
) -> &'static str {
    let network_name = interface.network_name.clone();
    let passphrase = interface.passphrase.clone();
    if network_name.is_none() && passphrase.is_none() {
        return "none";
    }
    let shared = rns_transport::iface::InterfaceSharedConfig {
        ifac_size: Some(16),
        network_name,
        passphrase,
        ..rns_transport::iface::InterfaceSharedConfig::default()
    };
    if manager.set_shared_config(iface, shared) {
        "configured"
    } else {
        "configure-failed"
    }
}

#[derive(Default)]
struct ReticulumInterface {
    name: String,
    kind: Option<String>,
    enabled: bool,
    target_host: Option<String>,
    target_port: Option<u16>,
    listen_ip: Option<String>,
    listen_port: Option<u16>,
    network_name: Option<String>,
    passphrase: Option<String>,
}

fn parse_reticulum_interfaces(path: &Path) -> ServerResult<Vec<ReticulumInterface>> {
    let contents = std::fs::read_to_string(path)?;
    let mut interfaces = Vec::new();
    let mut current: Option<ReticulumInterface> = None;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            if let Some(interface) = current.take() {
                interfaces.push(interface);
            }
            current = Some(ReticulumInterface {
                name: trimmed
                    .trim_start_matches("[[")
                    .trim_end_matches("]]")
                    .trim()
                    .to_string(),
                ..ReticulumInterface::default()
            });
            continue;
        }
        let Some(interface) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = parse_assignment(trimmed) else {
            continue;
        };
        match key {
            "type" => interface.kind = Some(value),
            "enabled" | "interface_enabled" => {
                interface.enabled = matches!(
                    value.to_ascii_lowercase().as_str(),
                    "yes" | "true" | "1" | "on"
                )
            }
            "target_host" => interface.target_host = Some(value),
            "target_port" => interface.target_port = value.parse().ok(),
            "listen_ip" => interface.listen_ip = Some(value),
            "listen_port" => interface.listen_port = value.parse().ok(),
            "network_name" => interface.network_name = Some(value),
            "passphrase" => interface.passphrase = Some(value),
            _ => {}
        }
    }
    if let Some(interface) = current.take() {
        interfaces.push(interface);
    }
    Ok(interfaces)
}

fn parse_assignment(line: &str) -> Option<(&str, String)> {
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value
        .trim()
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or_else(|| value.trim())
        .to_string();
    Some((key, value))
}

fn load_or_create_identity(config: &ServerConfig) -> ServerResult<PrivateIdentity> {
    if let Ok(raw) = std::fs::read(&config.identity_path) {
        if !raw.is_empty()
            && !raw.starts_with(b"OMENCHATD_IDENTITY_PLACEHOLDER")
            && !raw.starts_with(b"OMENCHATD_IDENTITY_PLACEHOLDER\n")
        {
            if let Ok(identity) = PrivateIdentity::from_private_key_bytes(&raw) {
                return Ok(identity);
            }
        }
    }

    let identity = PrivateIdentity::new_from_rand(OsRng);
    if let Some(parent) = config.identity_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config.identity_path, identity.to_private_key_bytes())?;
    Ok(identity)
}

fn destination_for_identity(identity: &PrivateIdentity) -> SingleInputDestination {
    SingleInputDestination::new(
        identity.clone(),
        DestinationName::new(OMENCHAT_RNS_APP_NAME, "node"),
    )
}

fn nomadnet_destination_for_identity(identity: &PrivateIdentity) -> SingleInputDestination {
    SingleInputDestination::new(
        identity.clone(),
        DestinationName::new(NOMADNET_RNS_APP_NAME, "node"),
    )
}

fn address_hash_bytes(hash: AddressHash) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(hash.as_slice());
    bytes
}

fn format_interface_status_lines(statuses: &[ReticulumInterfaceStatus]) -> Vec<String> {
    statuses
        .iter()
        .map(ReticulumInterfaceStatus::line)
        .collect()
}

fn json_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or("none")
        .to_string()
}

fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn append_server_log(config: &ServerConfig, message: impl AsRef<str>) {
    append_server_log_path(&config.log_path(), message);
}

fn append_server_log_error(config: &ServerConfig, message: impl AsRef<str>) {
    append_server_log_error_path(&config.log_path(), message);
}

fn append_server_log_path(path: &Path, message: impl AsRef<str>) {
    crate::server_log::append_with_severity(
        path,
        crate::server_log::ServerLogSeverity::Info,
        message.as_ref(),
    );
}

fn append_server_log_warning_path(path: &Path, message: impl AsRef<str>) {
    crate::server_log::append_with_severity(
        path,
        crate::server_log::ServerLogSeverity::Warning,
        message.as_ref(),
    );
}

fn append_server_log_error_path(path: &Path, message: impl AsRef<str>) {
    crate::server_log::append_with_severity(
        path,
        crate::server_log::ServerLogSeverity::Error,
        message.as_ref(),
    );
}

fn describe_live_event(event: &OmenchatLinkEvent) -> String {
    match event {
        OmenchatLinkEvent::LinkOpened { link_id, peer } => format!(
            "reticulum-rs link opened link={} peer={}",
            hex_lower(link_id),
            peer.display_name
        ),
        OmenchatLinkEvent::LinkData {
            link_id,
            context,
            data,
        } => format!(
            "reticulum-rs link data link={} context={} bytes={}",
            hex_lower(link_id),
            context,
            data.len()
        ),
        OmenchatLinkEvent::ResourceReceived {
            link_id,
            data,
            metadata,
        } => format!(
            "reticulum-rs resource received link={} bytes={} metadata_bytes={}",
            hex_lower(link_id),
            data.len(),
            metadata.as_ref().map(Vec::len).unwrap_or(0)
        ),
        OmenchatLinkEvent::LinkClosed { link_id, reason } => format!(
            "reticulum-rs link closed link={} reason={}",
            hex_lower(link_id),
            reason.as_deref().unwrap_or("unknown")
        ),
    }
}

fn decoded_frame_summary(data: &[u8]) -> String {
    match decode_frame(data) {
        Ok(frame) => format!(
            "op={:?} seq={} room={:?}",
            frame.op, frame.seq, frame.room_id
        ),
        Err(error) => format!("decode_error={error}"),
    }
}

fn nomadnet_request_path_for_payload(config: &ServerConfig, payload: &[u8]) -> Option<String> {
    const MAX_REQUEST_BYTES: usize = 4 * 1024;
    const MAX_REQUEST_SCALAR_BYTES: usize = 1024;
    const MAX_REQUEST_CONTAINER_ITEMS: usize = 32;
    const MAX_REQUEST_TOTAL_VALUES: usize = 64;
    const MAX_REQUEST_DEPTH: usize = 4;
    let value = unpack_msgpack_value(
        payload,
        MAX_REQUEST_BYTES,
        MAX_REQUEST_SCALAR_BYTES,
        MAX_REQUEST_CONTAINER_ITEMS,
        MAX_REQUEST_TOTAL_VALUES,
        MAX_REQUEST_DEPTH,
    )
    .ok()?;
    let Value::Array(items) = value else {
        return None;
    };
    let path_hash = match items.get(1)? {
        Value::Binary(bytes) if bytes.len() == 16 => {
            let mut hash = [0u8; 16];
            hash.copy_from_slice(bytes);
            hash
        }
        _ => return None,
    };
    crate::config::nomadnet_portal_paths(config)
        .into_iter()
        .find(|path| truncated_sha256(path.as_bytes()) == path_hash)
}

fn nomadnet_response_resource_payload(
    config: &ServerConfig,
    request_id: &[u8],
) -> ServerResult<Vec<u8>> {
    if request_id.len() != 16 {
        return Err(ServerError::Message(
            "NomadNet request resource id must be 16 bytes".into(),
        ));
    }
    let body = std::fs::read(config.nomadnet_index_page_path())?;
    pack_msgpack_value(&Value::Array(vec![
        Value::Binary(request_id.to_vec()),
        Value::Binary(body),
    ]))
}

fn unpack_msgpack_value(
    bytes: &[u8],
    max_bytes: usize,
    max_scalar_bytes: usize,
    max_container_items: usize,
    max_total_values: usize,
    max_depth: usize,
) -> ServerResult<Value> {
    crate::protocol::codec::validate_msgpack_with_limits(
        bytes,
        max_bytes,
        max_scalar_bytes,
        max_container_items,
        max_total_values,
        max_depth,
    )
    .map_err(|error| ServerError::Message(error.to_string()))?;
    let mut cursor = Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor)
        .map_err(|_| ServerError::Message("failed to decode NomadNet request msgpack".into()))?;
    if cursor.position() != bytes.len() as u64 {
        return Err(ServerError::Message(
            "trailing NomadNet request msgpack data".into(),
        ));
    }
    Ok(value)
}

fn pack_msgpack_value(value: &Value) -> ServerResult<Vec<u8>> {
    let mut packed = Vec::new();
    rmpv::encode::write_value(&mut packed, value)
        .map_err(|_| ServerError::Message("failed to encode NomadNet response msgpack".into()))?;
    Ok(packed)
}

fn truncated_sha256(bytes: &[u8]) -> [u8; 16] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn hex_lower(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_offer_preserves_owned_allocations() {
        let payload = vec![0x41; 1024 * 1024];
        let metadata = vec![0x42; 4096];
        let payload_ptr = payload.as_ptr();
        let metadata_ptr = metadata.as_ptr();

        let offer = ResourceOffer::new(payload, metadata);

        assert_eq!(offer.payload.as_ptr(), payload_ptr);
        assert_eq!(offer.metadata.as_ptr(), metadata_ptr);
        assert_eq!(offer.queued_bytes(), 1024 * 1024 + 4096);
    }
    use crate::transport::CapturedTransport;

    fn test_config(name: &str) -> ServerConfig {
        ServerConfig::for_root(std::env::temp_dir().join(format!(
            "omenchatd-reticulum-live-{name}-{}",
            std::process::id()
        )))
    }

    fn pack_request_for_path(path: &str) -> Vec<u8> {
        pack_msgpack_value(&Value::Array(vec![
            Value::F64(1.0),
            Value::Binary(truncated_sha256(path.as_bytes()).to_vec()),
            Value::Nil,
        ]))
        .expect("pack request")
    }

    fn test_live_worker() -> LiveServerWorker<CapturedTransport> {
        let store = OmenchatStore::in_memory().expect("store");
        let engine = SessionEngine::new(store);
        LiveServerWorker::new(OmenchatLiveServer::new(
            engine,
            CapturedTransport::default(),
        ))
    }

    #[tokio::test]
    async fn live_worker_rejects_saturation_without_queuing_waiters() {
        let worker = test_live_worker();
        let held = worker.permit.clone().try_acquire_owned().expect("permit");

        let error = worker
            .handle_event(OmenchatLinkEvent::LinkClosed {
                link_id: [1; 16],
                reason: Some("test".into()),
            })
            .await
            .expect_err("busy worker must reject");
        assert!(error.to_string().contains("worker is busy"));
        assert_eq!(worker.worker_metrics().rejected, 1);
        assert_eq!(worker.worker_metrics().completed, 0);

        drop(held);
    }

    #[tokio::test]
    async fn blocked_live_worker_does_not_stall_tokio_timers() {
        let worker = Arc::new(test_live_worker());
        let server = worker.server.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let lock_thread = std::thread::spawn(move || {
            let _held_server = server.lock().expect("hold worker lock");
            ready_tx.send(()).expect("report held lock");
            release_rx.recv().expect("release held lock");
        });
        ready_rx.recv().expect("worker lock ready");
        let task_worker = worker.clone();
        let task = tokio::spawn(async move {
            task_worker
                .handle_event(OmenchatLinkEvent::LinkClosed {
                    link_id: [2; 16],
                    reason: Some("test".into()),
                })
                .await
        });

        tokio::time::timeout(Duration::from_millis(100), async {
            while worker.worker_metrics().in_flight == 0 {
                tokio::task::yield_now().await;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        })
        .await
        .expect("Tokio timer must progress while blocking worker waits");

        release_tx.send(()).expect("release worker lock");
        lock_thread.join().expect("lock thread");
        task.await.expect("worker task").expect("handle event");
        let metrics = worker.worker_metrics();
        assert_eq!(metrics.in_flight, 0);
        assert_eq!(metrics.completed, 1);
        assert!(metrics.max_micros >= 10_000);
    }

    #[tokio::test]
    async fn sqlite_write_lock_is_bounded_without_stalling_tokio() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "omenchatd-reticulum-live-sqlite-lock-{}-{nonce}",
            std::process::id()
        ));
        let path = root.join("omenchat.sqlite");
        let store = OmenchatStore::open_for_lock_test(&path, Duration::from_millis(100))
            .expect("test store");
        let locker = rusqlite::Connection::open(&path).expect("lock connection");
        locker
            .execute_batch("BEGIN IMMEDIATE")
            .expect("acquire SQLite write lock");

        let worker = Arc::new(LiveServerWorker::new(OmenchatLiveServer::new(
            SessionEngine::new(store),
            CapturedTransport::default(),
        )));
        let session_open = crate::protocol::codec::encode_frame(&crate::protocol::Frame::new(
            crate::protocol::ChatOp::SessionOpen,
            1,
            None,
            crate::protocol::FrameBody::Fields(vec![
                crate::protocol::FrameValue::String("omenchat/0.1".into()),
                crate::protocol::FrameValue::String("Locked Client".into()),
                crate::protocol::FrameValue::String("locked-client-destination".into()),
            ]),
        ))
        .expect("session frame");
        let task_worker = worker.clone();
        let started = Instant::now();
        let task = tokio::spawn(async move {
            task_worker
                .handle_event(OmenchatLinkEvent::LinkData {
                    link_id: [3; 16],
                    context: OMENCHAT_LINK_CONTEXT,
                    data: session_open,
                })
                .await
        });

        tokio::time::timeout(
            Duration::from_millis(75),
            tokio::time::sleep(Duration::from_millis(10)),
        )
        .await
        .expect("Tokio timer must progress during SQLite busy wait");
        task.await
            .expect("worker task")
            .expect("handled protocol error");
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(90));
        assert!(elapsed < Duration::from_secs(1));
        let stats = worker.stats();
        assert_eq!(stats.protocol_errors, 1);
        assert!(stats
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("database is locked"));
        assert!(worker.worker_metrics().max_micros >= 90_000);

        locker.execute_batch("ROLLBACK").expect("release lock");
        drop(locker);
        drop(worker);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        std::fs::remove_dir(root).expect("remove isolated database root");
    }

    #[test]
    fn nomadnet_resource_request_matches_configured_portal_paths() {
        let config = test_config("request-match");

        assert_eq!(
            nomadnet_request_path_for_payload(&config, &pack_request_for_path("/page/index.mu")),
            Some("/page/index.mu".into())
        );
        assert_eq!(
            nomadnet_request_path_for_payload(&config, &pack_request_for_path("/")),
            Some("/".into())
        );
        assert_eq!(
            nomadnet_request_path_for_payload(&config, &pack_request_for_path("/missing.mu")),
            None
        );
    }

    #[test]
    fn nomadnet_resource_request_rejects_unbounded_or_trailing_msgpack() {
        let config = test_config("request-bounds");
        let mut trailing = pack_request_for_path("/page/index.mu");
        trailing.push(0xc0);
        assert!(nomadnet_request_path_for_payload(&config, &trailing).is_none());

        let oversized_scalar = [0xdb, 0x00, 0x00, 0x04, 0x01];
        assert!(nomadnet_request_path_for_payload(&config, &oversized_scalar).is_none());

        let mut deep = vec![0x91; 6];
        deep.push(0xc0);
        assert!(nomadnet_request_path_for_payload(&config, &deep).is_none());

        assert!(nomadnet_request_path_for_payload(&config, &vec![0xc0; 4097]).is_none());
    }

    #[test]
    fn nomadnet_response_resource_payload_roundtrips_request_id_and_body() {
        let config = test_config("response-payload");
        crate::config::init_files(&config).expect("init");
        std::fs::write(config.nomadnet_index_page_path(), b">Smoke\nPage").expect("write page");

        let request_id = [0x42u8; 16];
        let payload = nomadnet_response_resource_payload(&config, &request_id).expect("payload");
        let value = unpack_msgpack_value(
            &payload,
            4 * 1024 * 1024,
            4 * 1024 * 1024,
            16 * 1024,
            64 * 1024,
            16,
        )
        .expect("decode");
        let Value::Array(items) = value else {
            panic!("response must be an array");
        };

        assert_eq!(items[0], Value::Binary(request_id.to_vec()));
        assert_eq!(items[1], Value::Binary(b">Smoke\nPage".to_vec()));

        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[test]
    fn queue_budget_enforces_global_and_per_link_byte_limits() {
        let budget = QueueBudget::new(10, 6);
        let link_a = [0x11; 16];
        let link_b = [0x22; 16];

        let first = budget.reserve(link_a, 6).expect("first reservation");
        assert!(budget.reserve(link_a, 1).is_none());
        let second = budget.reserve(link_b, 4).expect("second reservation");
        assert!(budget.reserve(link_b, 1).is_none());

        let snapshot = budget.snapshot();
        assert_eq!(snapshot.queued_items, 2);
        assert_eq!(snapshot.queued_bytes, 10);
        assert_eq!(snapshot.rejected_items, 2);

        drop(first);
        assert_eq!(budget.snapshot().queued_bytes, 4);
        drop(second);
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.queued_items, 0);
        assert_eq!(snapshot.queued_bytes, 0);
        assert_eq!(snapshot.oldest_age_ms, 0);
        assert_eq!(snapshot.rejected_items, 2);
    }

    #[test]
    fn bounded_queue_releases_permits_on_saturation_and_receiver_drop() {
        let budget = QueueBudget::new(1024, 1024);
        let (tx, rx) = mpsc::channel::<Queued<Vec<u8>>>(2);
        let link_id = [0x33; 16];
        let mut accepted = 0;

        for _ in 0..20 {
            let permit = budget.reserve(link_id, 4).expect("byte budget");
            if tx
                .try_send(Queued {
                    value: vec![0; 4],
                    _permit: permit,
                })
                .is_ok()
            {
                accepted += 1;
            } else {
                budget.reject();
            }
        }

        assert_eq!(accepted, 2);
        assert_eq!(budget.snapshot().queued_items, 2);
        assert_eq!(budget.snapshot().queued_bytes, 8);
        assert_eq!(budget.snapshot().rejected_items, 18);

        drop(rx);
        assert_eq!(budget.snapshot().queued_items, 0);
        assert_eq!(budget.snapshot().queued_bytes, 0);
    }

    #[test]
    fn queue_permit_release_is_cancellation_safe_and_tracks_oldest_age() {
        let budget = QueueBudget::new(128, 128);
        let first = budget.reserve([0x44; 16], 64).expect("first reservation");
        std::thread::sleep(Duration::from_millis(20));
        let second = budget.reserve([0x44; 16], 32).expect("second reservation");
        std::thread::sleep(Duration::from_millis(2));
        let first_age = budget.snapshot().oldest_age_ms;
        assert!(first_age >= 20);

        drop(first);
        let second_age = budget.snapshot().oldest_age_ms;
        assert!(second_age < first_age);
        assert_eq!(budget.snapshot().queued_items, 1);

        drop(second);
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.queued_items, 0);
        assert_eq!(snapshot.queued_bytes, 0);
        assert_eq!(snapshot.oldest_age_ms, 0);
    }

    #[test]
    fn queue_budget_rejects_single_payload_larger_than_any_budget() {
        let budget = QueueBudget::new(16, 8);
        assert!(budget.reserve([0x55; 16], 9).is_none());
        assert_eq!(budget.snapshot().rejected_items, 1);
    }

    #[tokio::test]
    async fn control_lane_remains_responsive_while_payload_lane_is_saturated() {
        let budget = QueueBudget::new(128, 128);
        let (payload_tx, mut payload_rx) = mpsc::channel::<Queued<Vec<u8>>>(1);
        let (control_tx, mut control_rx) = mpsc::channel::<Queued<&'static str>>(1);
        let link_id = [0x66; 16];

        payload_tx
            .try_send(Queued {
                value: vec![0; 64],
                _permit: budget.reserve(link_id, 64).expect("payload permit"),
            })
            .expect("fill payload lane");
        assert!(payload_tx
            .try_send(Queued {
                value: vec![0; 1],
                _permit: budget.reserve(link_id, 1).expect("rejected payload permit"),
            })
            .is_err());

        control_tx
            .send(Queued {
                value: "close",
                _permit: budget.reserve(link_id, 0).expect("control permit"),
            })
            .await
            .expect("control admission");
        assert_eq!(
            control_rx.recv().await.expect("control event").value,
            "close"
        );
        assert_eq!(
            payload_rx.recv().await.expect("payload event").value.len(),
            64
        );
        assert_eq!(budget.snapshot().queued_items, 0);
        assert_eq!(budget.snapshot().queued_bytes, 0);
    }

    #[tokio::test]
    async fn closed_lanes_drain_without_spinning_or_retaining_permits() {
        let budget = QueueBudget::new(128, 128);
        let (payload_tx, mut payload_rx) = mpsc::channel::<Queued<u8>>(2);
        let (control_tx, mut control_rx) = mpsc::channel::<Queued<u8>>(2);
        let link_id = [0x77; 16];
        payload_tx
            .send(Queued {
                value: 1,
                _permit: budget.reserve(link_id, 64).expect("payload permit"),
            })
            .await
            .expect("payload send");
        control_tx
            .send(Queued {
                value: 2,
                _permit: budget.reserve(link_id, 0).expect("control permit"),
            })
            .await
            .expect("control send");
        drop(payload_tx);
        drop(control_tx);

        let drained = tokio::time::timeout(Duration::from_secs(1), async move {
            let mut control_open = true;
            let mut payload_open = true;
            let mut values = Vec::new();
            while control_open || payload_open {
                let queued = tokio::select! {
                    biased;
                    queued = control_rx.recv(), if control_open => match queued {
                        Some(value) => Some(value),
                        None => { control_open = false; None }
                    },
                    queued = payload_rx.recv(), if payload_open => match queued {
                        Some(value) => Some(value),
                        None => { payload_open = false; None }
                    },
                };
                if let Some(queued) = queued {
                    values.push(queued.value);
                }
            }
            values
        })
        .await
        .expect("closed queues must terminate");

        assert_eq!(drained, vec![2, 1]);
        assert_eq!(budget.snapshot().queued_items, 0);
        assert_eq!(budget.snapshot().queued_bytes, 0);
    }
}

#[cfg(test)]
#[path = "reticulum_live_soak_tests.rs"]
mod soak_tests;

#[cfg(test)]
#[path = "reticulum_live_db_soak_tests.rs"]
mod db_soak_tests;
