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
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::live::{
    LiveResourceDirection, LiveResourceOutcome, LiveServerStats, OmenchatLinkEvent,
    OmenchatLiveServer,
};
use crate::protocol::codec::decode_frame;
use crate::session::{ServerPeer, SessionEngine};
use crate::store::OmenchatStore;
use crate::transport::{
    LinkId, OmenchatTransport, OMENCHAT_LINK_CONTEXT, OMENCHAT_RESOURCE_METADATA_PREFIX,
};

use omen_ifac_tcp as ifac_tcp;

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
const OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS: usize = 256;
const OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS_PER_LINK: usize = 16;
const OUTBOUND_RESOURCE_CORRELATION_MAX_BYTES: usize = 1024 * 1024;
const OUTBOUND_RESOURCE_CORRELATION_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const REJECTED_SPLIT_RESOURCE_MAX_ITEMS: usize = 256;
const REJECTED_SPLIT_RESOURCE_TTL: Duration = Duration::from_secs(2 * 60);

fn remember_rejected_split_resource(
    rejected: &mut BTreeMap<[u8; 32], Instant>,
    resource_hash: [u8; 32],
    now: Instant,
    metrics: &SplitResourceSafeguardMetrics,
) -> bool {
    let before_purge = rejected.len();
    rejected.retain(|_, inserted| now.duration_since(*inserted) <= REJECTED_SPLIT_RESOURCE_TTL);
    metrics.add_expired(before_purge.saturating_sub(rejected.len()));
    if rejected.contains_key(&resource_hash) {
        return false;
    }
    if rejected.len() >= REJECTED_SPLIT_RESOURCE_MAX_ITEMS {
        if let Some(oldest) = rejected
            .iter()
            .min_by_key(|(_, inserted)| **inserted)
            .map(|(hash, _)| *hash)
        {
            rejected.remove(&oldest);
        }
    }
    rejected.insert(resource_hash, now);
    metrics.increment_rejected();
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplitResourceSafeguardMetricsSnapshot {
    pub split_resources_rejected: u64,
    pub late_split_completions_suppressed: u64,
    pub split_rejection_markers_expired: u64,
}

#[derive(Debug, Default)]
struct SplitResourceSafeguardMetrics {
    split_resources_rejected: AtomicU64,
    late_split_completions_suppressed: AtomicU64,
    split_rejection_markers_expired: AtomicU64,
}

impl SplitResourceSafeguardMetrics {
    fn saturating_increment(counter: &AtomicU64, amount: u64) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(amount))
        });
    }

    fn increment_rejected(&self) {
        Self::saturating_increment(&self.split_resources_rejected, 1);
    }

    fn increment_late_suppressed(&self) {
        Self::saturating_increment(&self.late_split_completions_suppressed, 1);
    }

    fn add_expired(&self, count: usize) {
        Self::saturating_increment(
            &self.split_rejection_markers_expired,
            u64::try_from(count).unwrap_or(u64::MAX),
        );
    }

    fn snapshot(&self) -> SplitResourceSafeguardMetricsSnapshot {
        SplitResourceSafeguardMetricsSnapshot {
            split_resources_rejected: self.split_resources_rejected.load(Ordering::Relaxed),
            late_split_completions_suppressed: self
                .late_split_completions_suppressed
                .load(Ordering::Relaxed),
            split_rejection_markers_expired: self
                .split_rejection_markers_expired
                .load(Ordering::Relaxed),
        }
    }
}

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
    split_resource_safeguard_metrics: Arc<SplitResourceSafeguardMetrics>,
    pub live_server: LiveServerWorker<ReticulumOmenchatTransport>,
    interface_statuses: Vec<ReticulumInterfaceStatus>,
    shutdown: CancellationToken,
    owned_tasks: Vec<OwnedTask>,
    shutdown_complete: bool,
}

struct OwnedTask {
    handle: JoinHandle<()>,
    abort_on_shutdown: bool,
}

#[derive(Default)]
struct StartupTaskGuard(Vec<JoinHandle<()>>);

impl StartupTaskGuard {
    fn push(&mut self, task: JoinHandle<()>) {
        self.0.push(task);
    }

    fn finish(mut self) -> Vec<JoinHandle<()>> {
        std::mem::take(&mut self.0)
    }
}

impl Drop for StartupTaskGuard {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

impl OwnedTask {
    fn cancellable(handle: JoinHandle<()>) -> Self {
        Self {
            handle,
            abort_on_shutdown: false,
        }
    }

    fn interface(handle: JoinHandle<()>) -> Self {
        Self {
            handle,
            abort_on_shutdown: true,
        }
    }
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
    fn lock_server(&self) -> ServerResult<std::sync::MutexGuard<'_, OmenchatLiveServer<T>>> {
        self.server
            .lock()
            .map_err(|_| ServerError::Message("live-server worker lock poisoned".into()))
    }

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

    #[cfg(feature = "omenchat-slow-mode-qualification")]
    pub async fn transition_slow_mode_for_qualification(
        &self,
        room_id: u32,
        slow_mode_seconds: u32,
    ) -> ServerResult<bool> {
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
                Ok(mut server) => {
                    server.transition_slow_mode_for_qualification(room_id, slow_mode_seconds)
                }
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

    pub fn stats(&self) -> ServerResult<LiveServerStats> {
        Ok(self.lock_server()?.stats())
    }

    pub async fn expire_pending_handshakes(&self, now_unix: i64) -> ServerResult<usize> {
        let permit = self.permit.clone().try_acquire_owned().map_err(|_| {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            ServerError::Message("live-server worker is busy".into())
        })?;
        let server = self.server.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            server
                .lock()
                .map_err(|_| ServerError::Message("live-server worker lock poisoned".into()))
                .map(|mut server| server.expire_pending_handshakes(now_unix))
        })
        .await
        .map_err(|error| ServerError::Message(format!("live-server worker failed: {error}")))?
    }

    pub fn recent_closed_link_summaries(
        &self,
    ) -> ServerResult<Vec<crate::live::ClosedLinkSummary>> {
        Ok(self.lock_server()?.recent_closed_link_summaries())
    }

    pub fn active_room_counts(&self) -> ServerResult<Vec<(crate::protocol::RoomId, usize)>> {
        Ok(self.lock_server()?.active_room_counts())
    }

    pub fn active_link_summaries(&self) -> ServerResult<Vec<crate::live::ActiveLinkSummary>> {
        Ok(self.lock_server()?.active_link_summaries())
    }

    pub fn active_identity_counts(&self) -> ServerResult<Vec<(Vec<u8>, usize)>> {
        Ok(self.lock_server()?.active_identity_counts())
    }

    pub fn disconnect_identity(&self, identity_hash: &[u8]) -> ServerResult<usize> {
        Ok(self.lock_server()?.disconnect_identity(identity_hash))
    }

    #[cfg(test)]
    pub(crate) fn poison_lock_for_test(&self) {
        let server = self.server.clone();
        std::thread::spawn(move || {
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _guard = server.lock().expect("test acquires live-server lock");
                panic!("intentional test-only live-server poison");
            }));
            assert!(caught.is_err());
        })
        .join()
        .expect("poison helper thread");
    }
}

#[derive(Clone)]
struct ReticulumInterfaceStatus {
    label: String,
    kind: ReticulumInterfaceStatusKind,
    worker: tokio::task::AbortHandle,
}

#[derive(Clone)]
enum ReticulumInterfaceStatusKind {
    TcpClient(rns_transport::iface::tcp_client::TcpRuntimeStatusHandle),
    IfacTcpClient(ifac_tcp::IfacTcpRuntimeStatusHandle),
    TcpServer(rns_transport::iface::tcp_server::TcpListenerRuntimeStatusHandle),
}

impl ReticulumInterfaceStatus {
    fn observation(&self) -> InterfaceObservation {
        let status = match &self.kind {
            ReticulumInterfaceStatusKind::TcpClient(handle) => handle.to_json(),
            ReticulumInterfaceStatusKind::IfacTcpClient(handle) => handle.to_json(),
            ReticulumInterfaceStatusKind::TcpServer(handle) => handle.to_json(),
        };
        let state = match &self.kind {
            ReticulumInterfaceStatusKind::TcpServer(_) => json_str(&status, "listener_state"),
            ReticulumInterfaceStatusKind::TcpClient(_)
            | ReticulumInterfaceStatusKind::IfacTcpClient(_) => json_str(&status, "stream_state"),
        };
        InterfaceObservation::new(state, !self.worker.is_finished())
    }

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
    NoInterfaces,
    Starting,
    Connecting,
    Healthy,
    Reconnecting,
    Degraded,
    Terminal,
}

impl InterfaceHealth {
    pub fn needs_runtime_restart(self) -> bool {
        matches!(self, Self::Terminal)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NoInterfaces => "no interfaces configured",
            Self::Starting => "configured; workers starting",
            Self::Connecting => "interfaces connecting",
            Self::Healthy => "operational",
            Self::Reconnecting => "interfaces reconnecting",
            Self::Degraded => "degraded; at least one interface operational",
            Self::Terminal => "all interface workers terminal",
        }
    }

    pub fn machine_label(self) -> &'static str {
        match self {
            Self::NoInterfaces => "no_interface",
            Self::Starting => "configured",
            Self::Connecting => "connecting",
            Self::Healthy => "operational",
            Self::Reconnecting => "reconnecting",
            Self::Degraded => "degraded",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InterfaceObservation {
    state: String,
    worker_alive: bool,
}

impl InterfaceObservation {
    fn new(state: impl Into<String>, worker_alive: bool) -> Self {
        Self {
            state: state.into(),
            worker_alive,
        }
    }
}

fn aggregate_interface_health(observations: &[InterfaceObservation]) -> InterfaceHealth {
    if observations.is_empty() {
        return InterfaceHealth::NoInterfaces;
    }

    let mut healthy = 0usize;
    let mut reconnecting = 0usize;
    let mut connecting = 0usize;
    let mut starting = 0usize;
    let mut terminal = 0usize;
    for observation in observations {
        if !observation.worker_alive {
            terminal += 1;
            continue;
        }
        match observation.state.as_str() {
            "connected" | "listening" | "active" => healthy += 1,
            "reconnecting" | "stale" => reconnecting += 1,
            "connecting" | "binding" => connecting += 1,
            "configured" | "starting" => starting += 1,
            "closed" | "bind_error" | "failed" | "error" => terminal += 1,
            _ => starting += 1,
        }
    }

    if healthy == observations.len() {
        InterfaceHealth::Healthy
    } else if healthy > 0 {
        InterfaceHealth::Degraded
    } else if reconnecting > 0 {
        InterfaceHealth::Reconnecting
    } else if connecting > 0 {
        InterfaceHealth::Connecting
    } else if starting > 0 {
        InterfaceHealth::Starting
    } else if terminal == observations.len() {
        InterfaceHealth::Terminal
    } else {
        InterfaceHealth::Degraded
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
    resource_id: String,
    payload: Vec<u8>,
    metadata: Vec<u8>,
}

impl ResourceOffer {
    fn new(resource_id: String, payload: Vec<u8>, metadata: Vec<u8>) -> Self {
        Self {
            resource_id,
            payload,
            metadata,
        }
    }

    fn queued_bytes(&self) -> usize {
        self.resource_id
            .len()
            .saturating_add(self.payload.len())
            .saturating_add(self.metadata.len())
    }
}

#[derive(Debug)]
struct OutboundResourceCorrelation {
    resource_id: String,
    inserted_at: Instant,
}

#[derive(Debug, Default)]
struct OutboundResourceCorrelations {
    entries: BTreeMap<(LinkId, [u8; 32]), OutboundResourceCorrelation>,
    order: std::collections::VecDeque<(LinkId, [u8; 32])>,
    retained_bytes: usize,
    rejected: u64,
    expired: u64,
}

impl OutboundResourceCorrelations {
    fn insert(
        &mut self,
        link_id: LinkId,
        resource_hash: [u8; 32],
        resource_id: String,
        now: Instant,
    ) -> bool {
        self.purge_expired(now);
        let key = (link_id, resource_hash);
        if let Some(previous) = self.entries.remove(&key) {
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(previous.resource_id.len());
        }
        let per_link = self
            .entries
            .keys()
            .filter(|(candidate, _)| *candidate == link_id)
            .count();
        if self.entries.len() >= OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS
            || per_link >= OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS_PER_LINK
            || self.retained_bytes.saturating_add(resource_id.len())
                > OUTBOUND_RESOURCE_CORRELATION_MAX_BYTES
        {
            self.rejected = self.rejected.saturating_add(1);
            return false;
        }
        self.retained_bytes = self.retained_bytes.saturating_add(resource_id.len());
        self.entries.insert(
            key,
            OutboundResourceCorrelation {
                resource_id,
                inserted_at: now,
            },
        );
        self.order.push_back(key);
        true
    }

    fn take(&mut self, link_id: LinkId, resource_hash: [u8; 32], now: Instant) -> Option<String> {
        self.purge_expired(now);
        let entry = self.entries.remove(&(link_id, resource_hash))?;
        self.retained_bytes = self.retained_bytes.saturating_sub(entry.resource_id.len());
        Some(entry.resource_id)
    }

    fn remove_link(&mut self, link_id: LinkId) -> usize {
        let before = self.entries.len();
        self.entries.retain(|(candidate, _), entry| {
            if *candidate == link_id {
                self.retained_bytes = self.retained_bytes.saturating_sub(entry.resource_id.len());
                false
            } else {
                true
            }
        });
        before.saturating_sub(self.entries.len())
    }

    fn purge_expired(&mut self, now: Instant) {
        while let Some(key) = self.order.front().copied() {
            let expired = self.entries.get(&key).is_none_or(|entry| {
                now.saturating_duration_since(entry.inserted_at)
                    >= OUTBOUND_RESOURCE_CORRELATION_TTL
            });
            if !expired {
                break;
            }
            self.order.pop_front();
            if let Some(entry) = self.entries.remove(&key) {
                self.retained_bytes = self.retained_bytes.saturating_sub(entry.resource_id.len());
                self.expired = self.expired.saturating_add(1);
            }
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.retained_bytes = 0;
    }
}

impl ReticulumOmenchatTransport {
    fn new(
        transport: Arc<Transport>,
        log_path: std::path::PathBuf,
        shutdown: CancellationToken,
        outbound_correlations: Arc<Mutex<OutboundResourceCorrelations>>,
    ) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<Queued<TransportCommand>>(TRANSPORT_QUEUE_ITEMS);
        let (control_tx, mut control_rx) =
            mpsc::channel::<Queued<TransportCommand>>(TRANSPORT_CONTROL_ITEMS);
        let queue_budget = QueueBudget::new(TRANSPORT_QUEUE_BYTES, TRANSPORT_PER_LINK_BYTES);
        let sent_frames = Arc::new(AtomicU64::new(0));
        let offered_resources = Arc::new(AtomicU64::new(0));
        let sent_frame_bytes = Arc::new(AtomicU64::new(0));
        let offered_resource_bytes = Arc::new(AtomicU64::new(0));
        let worker = tokio::spawn(async move {
            let mut control_open = true;
            let mut payload_open = true;
            while control_open || payload_open {
                let selected = tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => break,
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
                        let resource_id = offer.resource_id;
                        match transport
                            .send_resource(
                                &AddressHash::new(link_id),
                                offer.payload,
                                Some(offer.metadata),
                            )
                            .await
                        {
                            Ok(hash) => {
                                let retained = outbound_correlations
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .insert(link_id, hash.to_bytes(), resource_id, Instant::now());
                                append_server_log_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs OMENchat resource offered link={} hash={} bytes={} correlation={}",
                                        hex_lower(&link_id),
                                        hash,
                                        payload_bytes,
                                        if retained { "retained" } else { "rejected" }
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
            outbound_correlations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clear();
        });

        (
            Self {
                tx,
                control_tx,
                queue_budget,
                sent_frames,
                offered_resources,
                sent_frame_bytes,
                offered_resource_bytes,
            },
            worker,
        )
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
        resource_id: String,
        payload: Vec<u8>,
        metadata: Vec<u8>,
    ) -> ServerResult<()> {
        let byte_count = payload.len() as u64;
        let offer = ResourceOffer::new(resource_id, payload, metadata);
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
        | OmenchatLinkEvent::PeerIdentified { link_id, .. }
        | OmenchatLinkEvent::LinkData { link_id, .. }
        | OmenchatLinkEvent::ResourceReceived { link_id, .. }
        | OmenchatLinkEvent::ResourceTerminal { link_id, .. }
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

pub fn run_live_server(
    config: ServerConfig,
    qualification_slow_mode_transition_seconds: Option<u32>,
) -> ServerResult<()> {
    let runtime =
        crate::runtime_policy::build_runtime(crate::runtime_policy::HEADLESS_THREAD_NAME)?;
    runtime.block_on(run_live_server_async(
        config,
        qualification_slow_mode_transition_seconds,
    ))
}

async fn run_live_server_async(
    config: ServerConfig,
    qualification_slow_mode_transition_seconds: Option<u32>,
) -> ServerResult<()> {
    let effective_upload_max = config
        .upload_max_file_bytes
        .min(crate::resource_compat::exact_train_upload_payload_max() as u64);
    if effective_upload_max != config.upload_max_file_bytes {
        append_server_log_warning_path(
            &config.log_path(),
            format!(
                "configured upload limit {} bytes is capped to {} bytes on Reticulum 0.9.7",
                config.upload_max_file_bytes, effective_upload_max
            ),
        );
    }
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
    let handshake_sweep_interval = Duration::from_secs(1);
    let mut next_announce = Instant::now() + announce_interval;
    let mut next_stats = Instant::now() + stats_interval;
    let mut next_handshake_sweep = Instant::now() + handshake_sweep_interval;
    #[cfg(feature = "omenchat-slow-mode-qualification")]
    let mut qualification_slow_mode_transition_seconds = qualification_slow_mode_transition_seconds;
    #[cfg(not(feature = "omenchat-slow-mode-qualification"))]
    let _ = qualification_slow_mode_transition_seconds;
    let shutdown_signal = wait_for_shutdown_signal();
    tokio::pin!(shutdown_signal);

    // Poll the signal future once before advertising readiness. This registers
    // the platform handlers and closes the startup window where an immediate
    // service-manager stop could otherwise bypass the orderly drain path.
    let mut pending_shutdown = None;
    tokio::select! {
        biased;
        reason = &mut shutdown_signal => pending_shutdown = Some(reason),
        _ = tokio::task::yield_now() => {}
    }

    if pending_shutdown.is_none() {
        println!("omenchatd reticulum-rs live server ready");
        println!("readiness: {}", runtime.interface_health().machine_label());
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
    }

    let run_result: ServerResult<&'static str> = loop {
        if let Some(reason) = pending_shutdown.take() {
            break reason;
        }
        for _ in 0..EVENT_QUEUE_ITEMS.saturating_add(EVENT_CONTROL_ITEMS) {
            let Some(event) = runtime.try_recv_event() else {
                break;
            };
            if let Err(error) = runtime.live_server.handle_event(event).await {
                append_server_log_error(
                    &config,
                    format!("reticulum-rs live event failed: {error}"),
                );
            }
        }

        #[cfg(feature = "omenchat-slow-mode-qualification")]
        if let Some(seconds) = qualification_slow_mode_transition_seconds {
            if !(1..=crate::protocol::ROOM_SLOW_MODE_MAX_SECONDS).contains(&seconds) {
                break Err(ServerError::Message(
                    "qualification slow-mode transition is outside protocol bounds".into(),
                ));
            }
            match runtime
                .live_server
                .transition_slow_mode_for_qualification(1, seconds)
                .await
            {
                Ok(true) => {
                    println!(
                        "omenchatd qualification slow-mode transition committed: room=1 seconds={seconds}"
                    );
                    qualification_slow_mode_transition_seconds = None;
                }
                Ok(false) => {}
                Err(error) => {
                    append_server_log_error(
                        &config,
                        format!("slow-mode transition qualification failed: {error}"),
                    );
                    break Err(error);
                }
            }
        }

        if Instant::now() >= next_announce {
            if let Err(error) = announce_destinations(
                &runtime.transport,
                &runtime.destination,
                &runtime.nomadnet_destination,
                &config,
            )
            .await
            {
                append_server_log_error(
                    &config,
                    format!("reticulum-rs periodic announce failed: {error}"),
                );
                break Err(error);
            }
            next_announce = Instant::now() + announce_interval;
        }
        if Instant::now() >= next_handshake_sweep {
            let now_unix = (current_epoch_ms() / 1_000).try_into().unwrap_or(i64::MAX);
            if let Err(error) = runtime
                .live_server
                .expire_pending_handshakes(now_unix)
                .await
            {
                append_server_log_warning_path(
                    &config.log_path(),
                    format!("reticulum-rs handshake expiry sweep failed: {error}"),
                );
            }
            next_handshake_sweep = Instant::now() + handshake_sweep_interval;
        }
        if Instant::now() >= next_stats {
            let stats = match headless_stats_sample(&runtime.live_server, &config) {
                Ok(stats) => stats,
                Err(error) => break Err(error),
            };
            println!("{}", stats.summary_line());
            println!("readiness: {}", runtime.interface_health().machine_label());
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

        let next_deadline = next_announce.min(next_handshake_sweep).min(next_stats);
        tokio::select! {
            biased;
            reason = &mut shutdown_signal => break reason,
            event = runtime.recv_next_event() => match event {
                Some(event) => {
                    if let Err(error) = runtime.live_server.handle_event(event).await {
                        append_server_log_error(
                            &config,
                            format!("reticulum-rs live event failed: {error}"),
                        );
                    }
                }
                None => break Err(ServerError::Message(
                    "reticulum-rs live event queues stopped unexpectedly".into(),
                )),
            },
            _ = tokio::time::sleep_until(next_deadline.into()) => {}
        }
    };

    match &run_result {
        Ok(reason) => append_server_log(
            &config,
            format!("reticulum-rs live server shutdown requested reason={reason}"),
        ),
        Err(error) => append_server_log_error(
            &config,
            format!("reticulum-rs live server draining after fatal runtime error: {error}"),
        ),
    }
    let shutdown_result = runtime.shutdown(&config).await;
    let logs_flushed = crate::server_log::flush(Duration::from_secs(2));
    if !logs_flushed {
        return Err(ServerError::Message(
            "reticulum-rs shutdown completed but server log flush timed out".into(),
        ));
    }
    run_result?;
    shutdown_result
}

fn headless_stats_sample<T>(
    worker: &LiveServerWorker<T>,
    config: &ServerConfig,
) -> ServerResult<LiveServerStats>
where
    T: OmenchatTransport + Send + 'static,
{
    worker.stats().map_err(|error| {
        append_server_log_error(
            config,
            format!("reticulum-rs live statistics failed: {error}"),
        );
        error
    })
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() -> ServerResult<&'static str> {
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|error| ServerError::Message(format!("SIGTERM handler failed: {error}")))?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result.map_err(|error| ServerError::Message(format!("SIGINT handler failed: {error}")))?;
            Ok("sigint")
        }
        _ = terminate.recv() => Ok("sigterm"),
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() -> ServerResult<&'static str> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|error| ServerError::Message(format!("interrupt handler failed: {error}")))?;
    Ok("interrupt")
}

pub async fn start_live_server(config: &ServerConfig) -> ServerResult<ReticulumLiveRuntime> {
    crate::config::init_files(config)?;
    let identity = load_or_create_identity(config)?;
    let mut transport_config = TransportConfig::new("omenchatd", &identity, true);
    transport_config.set_ratchet_store_path(config.reticulum_storage_path().join("ratchets"));
    let transport = Transport::new(transport_config);
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
    let (attached, interface_tasks) = attach_configured_interfaces(&transport, config).await?;
    let interface_tasks = StartupTaskGuard(interface_tasks);
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

    let store = OmenchatStore::open(&config.database_path)?
        .with_room_history_retention((&config.history_retention).into());
    let engine =
        SessionEngine::with_limits_and_motd(store, config.into(), Some(config.motd.clone()));

    let (event_payload_tx, event_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
    let (event_control_tx, event_control_rx) = mpsc::channel(EVENT_CONTROL_ITEMS);
    let event_queue_budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
    let event_tx = EventQueueSender {
        payload_tx: event_payload_tx,
        control_tx: event_control_tx,
        budget: event_queue_budget.clone(),
        log_path: config.log_path(),
    };
    let outbound_correlations = Arc::new(Mutex::new(OutboundResourceCorrelations::default()));
    let split_resource_safeguard_metrics = Arc::new(SplitResourceSafeguardMetrics::default());
    let shutdown = CancellationToken::new();
    let mut owned_tasks = vec![
        OwnedTask::cancellable(spawn_link_event_bridge(
            transport.clone(),
            event_tx.clone(),
            config.clone(),
            shutdown.clone(),
            outbound_correlations.clone(),
        )),
        OwnedTask::cancellable(spawn_received_data_bridge(
            transport.clone(),
            event_tx.clone(),
            shutdown.clone(),
        )),
        OwnedTask::cancellable(spawn_resource_event_bridge(
            transport.clone(),
            event_tx,
            config.clone(),
            shutdown.clone(),
            outbound_correlations.clone(),
            split_resource_safeguard_metrics.clone(),
        )),
    ];
    owned_tasks.extend(
        interface_tasks
            .finish()
            .into_iter()
            .map(OwnedTask::interface),
    );
    let (transport_impl, transport_worker) = ReticulumOmenchatTransport::new(
        transport.clone(),
        config.log_path(),
        shutdown.clone(),
        outbound_correlations,
    );
    owned_tasks.push(OwnedTask::cancellable(transport_worker));
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
        split_resource_safeguard_metrics,
        live_server: LiveServerWorker::new(OmenchatLiveServer::new(engine, transport_impl)),
        interface_statuses: attached,
        shutdown,
        owned_tasks,
        shutdown_complete: false,
    })
}

impl ReticulumLiveRuntime {
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_complete
    }

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

    pub fn split_resource_safeguard_metrics(&self) -> SplitResourceSafeguardMetricsSnapshot {
        self.split_resource_safeguard_metrics.snapshot()
    }

    pub fn interface_health(&self) -> InterfaceHealth {
        let observations = self
            .interface_statuses
            .iter()
            .map(ReticulumInterfaceStatus::observation)
            .collect::<Vec<_>>();
        aggregate_interface_health(&observations)
    }

    pub fn queue_metrics(&self) -> (QueueMetricsSnapshot, QueueMetricsSnapshot) {
        (
            self.transport_queue_budget.snapshot(),
            self.event_queue_budget.snapshot(),
        )
    }

    pub fn queue_summary_line(&self) -> String {
        let (transport_queue, event_queue) = self.queue_metrics();
        let split_metrics = self.split_resource_safeguard_metrics();
        format!(
            "queues: {} {} {} {} resource_safeguards=split_rejected:{} late_suppressed:{} markers_expired:{}",
            transport_queue.summary("transport"),
            event_queue.summary("events"),
            self.live_server.worker_metrics().summary(),
            crate::server_log::metrics().summary(),
            split_metrics.split_resources_rejected,
            split_metrics.late_split_completions_suppressed,
            split_metrics.split_rejection_markers_expired,
        )
    }

    pub async fn shutdown(&mut self, config: &ServerConfig) -> ServerResult<()> {
        if self.shutdown_complete {
            return Ok(());
        }

        let (active_links, enumeration_error) = match self.live_server.active_link_summaries() {
            Ok(active_links) => (active_links, None),
            Err(error) => {
                append_server_log_warning_path(
                    &config.log_path(),
                    format!("reticulum-rs shutdown skipped active-link close enumeration: {error}"),
                );
                (Vec::new(), Some(error))
            }
        };
        for active in &active_links {
            let channel = self.transport.channel(AddressHash::new(active.link_id));
            if let Err(error) = channel.close().await {
                append_server_log_warning_path(
                    &config.log_path(),
                    format!(
                        "reticulum-rs shutdown link close failed link={} error={error:?}",
                        hex_lower(&active.link_id)
                    ),
                );
            }
        }

        self.shutdown.cancel();
        let tasks = std::mem::take(&mut self.owned_tasks);
        let mut join_timeouts = 0usize;
        let mut join_failures = 0usize;
        for task in tasks {
            if task.abort_on_shutdown {
                task.handle.abort();
            }
            match tokio::time::timeout(Duration::from_secs(2), task.handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) if error.is_cancelled() && task.abort_on_shutdown => {}
                Ok(Err(_)) => join_failures += 1,
                Err(_) => join_timeouts += 1,
            }
        }
        self.shutdown_complete = true;
        append_server_log(
            config,
            format!(
                "reticulum-rs live server drained active_links={} worker_join_timeouts={} worker_join_failures={} {}",
                active_links.len(),
                join_timeouts,
                join_failures,
                self.queue_summary_line()
            ),
        );

        if join_timeouts > 0 || join_failures > 0 {
            return Err(ServerError::Message(format!(
                "reticulum-rs shutdown incomplete: worker_join_timeouts={join_timeouts} worker_join_failures={join_failures} active_link_enumeration_failed={}"
                , enumeration_error.is_some()
            )));
        }
        if let Some(error) = enumeration_error {
            return Err(ServerError::Message(format!(
                "reticulum-rs shutdown completed after active-link enumeration failed: {error}"
            )));
        }
        Ok(())
    }

    fn try_recv_event(&mut self) -> Option<OmenchatLinkEvent> {
        match self.event_control_rx.try_recv() {
            Ok(queued) => return Some(queued.value),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
            | Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }
        self.event_rx.try_recv().ok().map(|queued| queued.value)
    }

    async fn recv_next_event(&mut self) -> Option<OmenchatLinkEvent> {
        recv_prioritized_event(&mut self.event_control_rx, &mut self.event_rx).await
    }
}

async fn recv_prioritized_event(
    control_rx: &mut mpsc::Receiver<Queued<OmenchatLinkEvent>>,
    payload_rx: &mut mpsc::Receiver<Queued<OmenchatLinkEvent>>,
) -> Option<OmenchatLinkEvent> {
    loop {
        if let Ok(queued) = control_rx.try_recv() {
            return Some(queued.value);
        }
        if let Ok(queued) = payload_rx.try_recv() {
            return Some(queued.value);
        }
        if control_rx.is_closed() && payload_rx.is_closed() {
            return None;
        }
        tokio::select! {
            biased;
            queued = control_rx.recv(), if !control_rx.is_closed() => {
                if let Some(queued) = queued {
                    return Some(queued.value);
                }
            }
            queued = payload_rx.recv(), if !payload_rx.is_closed() => {
                if let Some(queued) = queued {
                    return Some(queued.value);
                }
            }
            else => continue,
        }
    }
}

impl Drop for ReticulumLiveRuntime {
    fn drop(&mut self) {
        self.shutdown.cancel();
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

fn spawn_link_event_bridge(
    transport: Arc<Transport>,
    event_tx: EventQueueSender,
    config: ServerConfig,
    shutdown: CancellationToken,
    outbound_correlations: Arc<Mutex<OutboundResourceCorrelations>>,
) -> JoinHandle<()> {
    let log_path = event_tx.log_path.clone();
    let mut events = transport.in_link_events();
    tokio::spawn(async move {
        loop {
            let next = tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                next = events.recv() => next,
            };
            match next {
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
                        if payload.context() == PacketContext::Request {
                            send_direct_nomadnet_response(
                                &transport,
                                &config,
                                &log_path,
                                event.id,
                                payload.request_id(),
                                payload.as_slice(),
                            )
                            .await;
                            continue;
                        }
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
                        outbound_correlations
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .remove_link(link_id);
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
                    LinkEvent::PeerIdentified(identity) => {
                        let link_id = address_hash_bytes(event.id);
                        let identity_hash = address_hash_bytes(identity.address_hash);
                        append_server_log_path(
                            &log_path,
                            format!(
                                "reticulum-rs in-link identified link={} identity={}",
                                hex_lower(&link_id),
                                hex_lower(&identity_hash)
                            ),
                        );
                        event_tx
                            .send_control(OmenchatLinkEvent::PeerIdentified {
                                link_id,
                                identity_hash,
                            })
                            .await;
                    }
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
    })
}

async fn send_direct_nomadnet_response(
    transport: &Transport,
    config: &ServerConfig,
    log_path: &Path,
    link_id: AddressHash,
    request_id: Option<[u8; 16]>,
    request_payload: &[u8],
) {
    let Some(request_id) = request_id else {
        append_server_log_warning_path(
            log_path,
            "reticulum-rs direct NomadNet request ignored: missing request id",
        );
        return;
    };
    let Some(request_path) = nomadnet_request_path_for_payload(config, request_payload) else {
        append_server_log_warning_path(
            log_path,
            format!(
                "reticulum-rs direct NomadNet request ignored link={} unknown path hash bytes={}",
                link_id,
                request_payload.len()
            ),
        );
        return;
    };

    // The link-event bridge processes one request at a time, bounding blocking
    // portal reads to one owned job instead of growing a task per request.
    let response_config = config.clone();
    let response = tokio::task::spawn_blocking(move || {
        nomadnet_response_resource_payload(&response_config, &request_id)
    })
    .await;
    let payload = match response {
        Ok(Ok(payload)) => payload,
        Ok(Err(error)) => {
            append_server_log_error_path(
                log_path,
                format!(
                    "reticulum-rs direct NomadNet response payload failed request_path={} error={error}",
                    request_path
                ),
            );
            return;
        }
        Err(error) => {
            append_server_log_error_path(
                log_path,
                format!(
                    "reticulum-rs direct NomadNet response worker failed request_path={} error={error}",
                    request_path
                ),
            );
            return;
        }
    };
    let Some(link) = transport.find_in_link(&link_id).await else {
        append_server_log_warning_path(
            log_path,
            format!(
                "reticulum-rs direct NomadNet response ignored link={} request_path={} missing inbound link",
                link_id, request_path
            ),
        );
        return;
    };
    let response_packet = {
        let link = link.lock().await;
        let Some(ingress_iface) = link.ingress_iface() else {
            append_server_log_warning_path(
                log_path,
                format!(
                    "reticulum-rs direct NomadNet response ignored link={} request_path={} missing ingress interface",
                    link_id, request_path
                ),
            );
            return;
        };
        match link.data_packet(&payload) {
            Ok(mut packet) => {
                packet.context = PacketContext::Response;
                Some((ingress_iface, packet))
            }
            Err(error) => {
                append_server_log_error_path(
                    log_path,
                    format!(
                        "reticulum-rs direct NomadNet response packet failed link={} request_path={} bytes={} error={error:?}",
                        link_id,
                        request_path,
                        payload.len()
                    ),
                );
                None
            }
        }
    };
    let Some((ingress_iface, packet)) = response_packet else {
        return;
    };
    transport.send_direct(ingress_iface, packet).await;
    append_server_log_path(
        log_path,
        format!(
            "reticulum-rs direct NomadNet response sent link={} request_path={} bytes={}",
            link_id,
            request_path,
            payload.len()
        ),
    );
}

fn spawn_received_data_bridge(
    transport: Arc<Transport>,
    event_tx: EventQueueSender,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    let log_path = event_tx.log_path.clone();
    let mut events = transport.received_data_events();
    tokio::spawn(async move {
        loop {
            let next = tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                next = events.recv() => next,
            };
            match next {
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
    })
}

fn spawn_resource_event_bridge(
    transport: Arc<Transport>,
    event_tx: EventQueueSender,
    config: ServerConfig,
    shutdown: CancellationToken,
    outbound_correlations: Arc<Mutex<OutboundResourceCorrelations>>,
    split_resource_safeguard_metrics: Arc<SplitResourceSafeguardMetrics>,
) -> JoinHandle<()> {
    spawn_resource_event_receiver(
        transport.clone(),
        transport.resource_events(),
        event_tx,
        config,
        shutdown,
        outbound_correlations,
        split_resource_safeguard_metrics,
    )
}

fn spawn_resource_event_receiver(
    transport: Arc<Transport>,
    mut events: tokio::sync::broadcast::Receiver<rns_transport::resource::ResourceEvent>,
    event_tx: EventQueueSender,
    config: ServerConfig,
    shutdown: CancellationToken,
    outbound_correlations: Arc<Mutex<OutboundResourceCorrelations>>,
    split_resource_safeguard_metrics: Arc<SplitResourceSafeguardMetrics>,
) -> JoinHandle<()> {
    let log_path = config.log_path();
    tokio::spawn(async move {
        let mut rejected_split_resources = BTreeMap::<[u8; 32], Instant>::new();
        loop {
            let next = tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                next = events.recv() => next,
            };
            match next {
                Ok(event) => {
                    let complete = match event.kind {
                        ResourceEventKind::Complete(complete) => {
                            if rejected_split_resources
                                .remove(&event.hash.to_bytes())
                                .is_some()
                            {
                                split_resource_safeguard_metrics.increment_late_suppressed();
                                append_server_log_warning_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs split resource completion suppressed link={} hash={}",
                                        event.link_id, event.hash
                                    ),
                                );
                                continue;
                            }
                            complete
                        }
                        ResourceEventKind::InboundFailed(failure) => {
                            let link_id = address_hash_bytes(event.link_id);
                            let reason = failure
                                .reason
                                .chars()
                                .filter(|character| !character.is_control())
                                .take(128)
                                .collect::<String>();
                            append_server_log_warning_path(
                                &log_path,
                                format!(
                                    "reticulum-rs inbound resource failed link={} hash={} received_bytes={} total_bytes={} reason={}",
                                    hex_lower(&link_id),
                                    event.hash,
                                    failure.progress.received_bytes,
                                    failure.progress.total_bytes,
                                    reason
                                ),
                            );
                            event_tx
                                .send_control(OmenchatLinkEvent::ResourceTerminal {
                                    link_id,
                                    resource_hash: event.hash.to_bytes(),
                                    resource_id: None,
                                    direction: LiveResourceDirection::Inbound,
                                    outcome: LiveResourceOutcome::Failed,
                                    expected_size: Some(failure.progress.total_bytes),
                                    reason: (!reason.is_empty()).then_some(reason),
                                })
                                .await;
                            continue;
                        }
                        ResourceEventKind::OutboundComplete => {
                            send_resource_terminal(
                                &event_tx,
                                &log_path,
                                &outbound_correlations,
                                event.link_id,
                                event.hash,
                                LiveResourceOutcome::Complete,
                            )
                            .await;
                            continue;
                        }
                        ResourceEventKind::OutboundFailed => {
                            send_resource_terminal(
                                &event_tx,
                                &log_path,
                                &outbound_correlations,
                                event.link_id,
                                event.hash,
                                LiveResourceOutcome::Failed,
                            )
                            .await;
                            continue;
                        }
                        ResourceEventKind::OutboundCancelled => {
                            send_resource_terminal(
                                &event_tx,
                                &log_path,
                                &outbound_correlations,
                                event.link_id,
                                event.hash,
                                LiveResourceOutcome::Cancelled,
                            )
                            .await;
                            continue;
                        }
                        ResourceEventKind::Progress(_) => continue,
                        ResourceEventKind::SegmentComplete(segment) => {
                            if segment.total_segments <= 1 {
                                continue;
                            }
                            let now = Instant::now();
                            let resource_hash = event.hash.to_bytes();
                            let first_rejection = remember_rejected_split_resource(
                                &mut rejected_split_resources,
                                resource_hash,
                                now,
                                &split_resource_safeguard_metrics,
                            );
                            if first_rejection {
                                let link_id = address_hash_bytes(event.link_id);
                                append_server_log_warning_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs split resource rejected link={} hash={} segments={} tracked={}",
                                        hex_lower(&link_id),
                                        event.hash,
                                        segment.total_segments,
                                        rejected_split_resources.len()
                                    ),
                                );
                                event_tx
                                    .send_control(OmenchatLinkEvent::ResourceTerminal {
                                        link_id,
                                        resource_hash,
                                        resource_id: None,
                                        direction: LiveResourceDirection::Inbound,
                                        outcome: LiveResourceOutcome::Failed,
                                        expected_size: Some(segment.total_data_size),
                                        reason: Some(
                                            "split Resource rejected on affected Reticulum 0.9.7 train"
                                                .into(),
                                        ),
                                    })
                                    .await;
                                if let Err(error) = transport.channel(event.link_id).close().await {
                                    append_server_log_warning_path(
                                        &log_path,
                                        format!(
                                            "reticulum-rs split resource link close failed link={} error={error:?}",
                                            hex_lower(&link_id)
                                        ),
                                    );
                                }
                            }
                            continue;
                        }
                    };
                    if complete.metadata.as_deref().is_some_and(|metadata| {
                        !crate::resource_compat::metadata_bearing_resource_is_unsplit_safe(
                            complete.data.len(),
                            metadata.len(),
                        )
                    }) {
                        let link_id = address_hash_bytes(event.link_id);
                        append_server_log_warning_path(
                            &log_path,
                            format!(
                                "reticulum-rs oversized metadata resource rejected link={} hash={} bytes={}",
                                hex_lower(&link_id),
                                event.hash,
                                complete.data.len()
                            ),
                        );
                        event_tx
                            .send_control(OmenchatLinkEvent::ResourceTerminal {
                                link_id,
                                resource_hash: event.hash.to_bytes(),
                                resource_id: None,
                                direction: LiveResourceDirection::Inbound,
                                outcome: LiveResourceOutcome::Failed,
                                expected_size: Some(complete.data.len() as u64),
                                reason: Some(
                                    "metadata Resource exceeds safe Reticulum 0.9.7 boundary"
                                        .into(),
                                ),
                            })
                            .await;
                        if let Err(error) = transport.channel(event.link_id).close().await {
                            append_server_log_warning_path(
                                &log_path,
                                format!(
                                    "reticulum-rs oversized metadata resource link close failed link={} error={error:?}",
                                    hex_lower(&link_id)
                                ),
                            );
                        }
                        continue;
                    }
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
                            resource_hash: event.hash.to_bytes(),
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
    })
}

async fn send_resource_terminal(
    event_tx: &EventQueueSender,
    log_path: &Path,
    outbound_correlations: &Arc<Mutex<OutboundResourceCorrelations>>,
    link_hash: AddressHash,
    resource_hash: rns_transport::hash::Hash,
    outcome: LiveResourceOutcome,
) {
    let link_id = address_hash_bytes(link_hash);
    let resource_id = outbound_correlations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take(link_id, resource_hash.to_bytes(), Instant::now());
    append_server_log_path(
        log_path,
        format!(
            "reticulum-rs outbound resource terminal link={} hash={} outcome={outcome:?}",
            hex_lower(&link_id),
            resource_hash
        ),
    );
    event_tx
        .send_control(OmenchatLinkEvent::ResourceTerminal {
            link_id,
            resource_hash: resource_hash.to_bytes(),
            resource_id,
            direction: LiveResourceDirection::Outbound,
            outcome,
            expected_size: None,
            reason: None,
        })
        .await;
}

async fn attach_configured_interfaces(
    transport: &Arc<Transport>,
    config: &ServerConfig,
) -> ServerResult<(Vec<ReticulumInterfaceStatus>, Vec<JoinHandle<()>>)> {
    let interfaces = parse_reticulum_interfaces(&config.reticulum_config_file())?;
    validate_reticulum_interfaces(&interfaces)?;
    let mut attached = Vec::new();
    let mut tasks = StartupTaskGuard::default();
    for interface in interfaces {
        if !interface.enabled {
            continue;
        }
        match interface.kind.as_deref() {
            Some("TCPClientInterface") | Some("tcp_client") => {
                let host = interface
                    .target_host
                    .as_deref()
                    .filter(|host| !host.trim().is_empty())
                    .ok_or_else(|| {
                        ServerError::Message(format!(
                            "enabled interface {} is missing target_host",
                            interface.name
                        ))
                    })?;
                let port = interface
                    .target_port
                    .filter(|port| *port != 0)
                    .ok_or_else(|| {
                        ServerError::Message(format!(
                            "enabled interface {} requires a nonzero target_port",
                            interface.name
                        ))
                    })?;
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
                    let task = tokio::spawn(ifac_tcp::IfacTcpClient::spawn(context));
                    let worker = task.abort_handle();
                    tasks.push(task);
                    attached.push(ReticulumInterfaceStatus {
                        label: format!(
                            "{} tcp_client {address} ifac=configured iface={}",
                            interface.name,
                            iface_address.to_hex_string()
                        ),
                        kind: ReticulumInterfaceStatusKind::IfacTcpClient(status),
                        worker,
                    });
                } else {
                    let client = rns_transport::iface::tcp_client::TcpClient::new(address.clone());
                    let status = client.runtime_status_handle();
                    let context = manager.new_context(client);
                    let iface_address = *context.channel.address();
                    let task =
                        tokio::spawn(rns_transport::iface::tcp_client::TcpClient::spawn(context));
                    let worker = task.abort_handle();
                    tasks.push(task);
                    attached.push(ReticulumInterfaceStatus {
                        label: format!(
                            "{} tcp_client {address} ifac=none iface={}",
                            interface.name,
                            iface_address.to_hex_string()
                        ),
                        kind: ReticulumInterfaceStatusKind::TcpClient(status),
                        worker,
                    });
                }
            }
            Some("TCPServerInterface") | Some("tcp_server") => {
                let listen_ip = interface.listen_ip.as_deref().unwrap_or("127.0.0.1");
                let port = interface
                    .listen_port
                    .filter(|port| *port != 0)
                    .ok_or_else(|| {
                        ServerError::Message(format!(
                            "enabled interface {} requires a nonzero listen_port",
                            interface.name
                        ))
                    })?;
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
                let task =
                    tokio::spawn(rns_transport::iface::tcp_server::TcpServer::spawn(context));
                let worker = task.abort_handle();
                tasks.push(task);
                attached.push(ReticulumInterfaceStatus {
                    label: format!(
                        "{} tcp_server {address} ifac=none iface={}",
                        interface.name,
                        iface_address.to_hex_string()
                    ),
                    kind: ReticulumInterfaceStatusKind::TcpServer(status),
                    worker,
                });
            }
            Some(kind) => {
                return Err(ServerError::Message(format!(
                    "enabled interface {} has unsupported type {kind}",
                    interface.name
                )));
            }
            None => {
                return Err(ServerError::Message(format!(
                    "enabled interface {} is missing type",
                    interface.name
                )));
            }
        }
    }
    Ok((attached, tasks.finish()))
}

fn validate_reticulum_interfaces(interfaces: &[ReticulumInterface]) -> ServerResult<()> {
    for interface in interfaces.iter().filter(|interface| interface.enabled) {
        match interface.kind.as_deref() {
            Some("TCPClientInterface") | Some("tcp_client") => {
                if interface
                    .target_host
                    .as_deref()
                    .is_none_or(|host| host.trim().is_empty())
                {
                    return Err(ServerError::Message(format!(
                        "enabled interface {} is missing target_host",
                        interface.name
                    )));
                }
                if interface.target_port.is_none_or(|port| port == 0) {
                    return Err(ServerError::Message(format!(
                        "enabled interface {} requires a nonzero target_port",
                        interface.name
                    )));
                }
            }
            Some("TCPServerInterface") | Some("tcp_server") => {
                if interface.network_name.is_some() || interface.passphrase.is_some() {
                    return Err(ServerError::Message(format!(
                        "enabled TCP server interface {} configures IFAC, but the published reticulum-rs 0.9.7 TCP server does not enforce the Python IFAC wire transform; use an IFAC TCP client or disable this interface",
                        interface.name
                    )));
                }
                if interface.listen_port.is_none_or(|port| port == 0) {
                    return Err(ServerError::Message(format!(
                        "enabled interface {} requires a nonzero listen_port",
                        interface.name
                    )));
                }
            }
            Some(kind) => {
                return Err(ServerError::Message(format!(
                    "enabled interface {} has unsupported type {kind}",
                    interface.name
                )));
            }
            None => {
                return Err(ServerError::Message(format!(
                    "enabled interface {} is missing type",
                    interface.name
                )));
            }
        }
    }
    Ok(())
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
    let contents = crate::config::read_reticulum_config_bounded(path)?;
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
    let existing = match std::fs::symlink_metadata(&config.identity_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(ServerError::Message(
                    "omenchatd identity must be a regular non-symlink file".into(),
                ));
            }
            Some(
                crate::private_fs::read_private_bounded(&config.identity_path, 4096).map_err(
                    |error| {
                        ServerError::Message(format!(
                            "omenchatd identity could not be read: {error}"
                        ))
                    },
                )?,
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(ServerError::Message(format!(
                "omenchatd identity metadata could not be read: {error}"
            )));
        }
    };

    if let Some(raw) = existing.as_deref() {
        if raw != crate::config::PLACEHOLDER_IDENTITY {
            let identity = PrivateIdentity::from_private_key_bytes(raw).map_err(|_| {
                ServerError::Message(
                    "existing omenchatd identity is invalid; file was preserved".into(),
                )
            })?;
            crate::config::enforce_private_file(&config.identity_path)?;
            return Ok(identity);
        }
    }

    let identity = PrivateIdentity::new_from_rand(OsRng);
    config.ensure_sensitive_file_parent(&config.identity_path)?;
    crate::config::replace_private_file(&config.identity_path, &identity.to_private_key_bytes())?;
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
        OmenchatLinkEvent::PeerIdentified {
            link_id,
            identity_hash,
        } => format!(
            "reticulum-rs link identified link={} identity={}",
            hex_lower(link_id),
            hex_lower(identity_hash)
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
            resource_hash,
            data,
            metadata,
        } => format!(
            "reticulum-rs resource received link={} hash={} bytes={} metadata_bytes={}",
            hex_lower(link_id),
            hex_lower_32(resource_hash),
            data.len(),
            metadata.as_ref().map(Vec::len).unwrap_or(0)
        ),
        OmenchatLinkEvent::ResourceTerminal {
            link_id,
            resource_hash,
            resource_id,
            direction,
            outcome,
            expected_size,
            reason,
        } => format!(
            "reticulum-rs resource terminal link={} hash={} resource_id={} direction={direction:?} outcome={outcome:?} expected_bytes={} reason={}",
            hex_lower(link_id),
            hex_lower_32(resource_hash),
            resource_id.as_deref().unwrap_or("unmapped"),
            expected_size.map(|size| size.to_string()).unwrap_or_else(|| "unknown".into()),
            reason.as_deref().unwrap_or("none")
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

fn hex_lower_32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_health_distinguishes_progress_from_terminal_workers() {
        let configured = InterfaceObservation::new("configured", true);
        let connecting = InterfaceObservation::new("connecting", true);
        let reconnecting = InterfaceObservation::new("reconnecting", true);
        let connected = InterfaceObservation::new("connected", true);
        let closed = InterfaceObservation::new("closed", false);

        assert_eq!(
            aggregate_interface_health(&[]),
            InterfaceHealth::NoInterfaces
        );
        assert_eq!(
            aggregate_interface_health(&[configured]),
            InterfaceHealth::Starting
        );
        assert_eq!(
            aggregate_interface_health(&[connecting]),
            InterfaceHealth::Connecting
        );
        assert_eq!(
            aggregate_interface_health(&[reconnecting]),
            InterfaceHealth::Reconnecting
        );
        assert_eq!(
            aggregate_interface_health(std::slice::from_ref(&connected)),
            InterfaceHealth::Healthy
        );
        assert_eq!(
            aggregate_interface_health(&[connected, closed.clone()]),
            InterfaceHealth::Degraded
        );
        assert_eq!(
            aggregate_interface_health(&[closed]),
            InterfaceHealth::Terminal
        );
    }

    #[test]
    fn reconnect_progress_never_requests_a_competing_runtime() {
        for health in [
            InterfaceHealth::Starting,
            InterfaceHealth::Connecting,
            InterfaceHealth::Healthy,
            InterfaceHealth::Reconnecting,
            InterfaceHealth::Degraded,
            InterfaceHealth::NoInterfaces,
        ] {
            assert!(!health.needs_runtime_restart(), "{health:?}");
        }
        assert!(InterfaceHealth::Terminal.needs_runtime_restart());
    }

    #[test]
    fn headless_loop_has_no_fixed_25ms_idle_poll() {
        let source = include_str!("reticulum_live.rs");
        let forbidden = ["tokio::time::sleep(Duration::from_", "millis(25))"].concat();
        assert!(!source.contains(&forbidden));
        assert!(source.contains("event = runtime.recv_next_event()"));
        assert!(source.contains("next_announce.min(next_handshake_sweep).min(next_stats)"));
    }

    #[test]
    fn production_live_server_lock_has_no_expect_guard() {
        let source = include_str!("reticulum_live.rs");
        assert!(!source.contains("expect(\"live-server worker lock\")"));
    }

    #[tokio::test]
    async fn event_wait_prioritizes_control_and_tolerates_one_closed_lane() {
        let budget = QueueBudget::new(1024, 1024);
        let (control_tx, mut control_rx) = mpsc::channel(2);
        let (payload_tx, mut payload_rx) = mpsc::channel(2);
        let payload = OmenchatLinkEvent::LinkClosed {
            link_id: [1; 16],
            reason: Some("payload".into()),
        };
        let control = OmenchatLinkEvent::LinkClosed {
            link_id: [2; 16],
            reason: Some("control".into()),
        };
        payload_tx
            .send(Queued {
                value: payload,
                _permit: budget.reserve([1; 16], 0).expect("payload permit"),
            })
            .await
            .expect("payload queue");
        control_tx
            .send(Queued {
                value: control,
                _permit: budget.reserve([2; 16], 0).expect("control permit"),
            })
            .await
            .expect("control queue");
        assert!(matches!(
            recv_prioritized_event(&mut control_rx, &mut payload_rx).await,
            Some(OmenchatLinkEvent::LinkClosed { link_id, .. }) if link_id == [2; 16]
        ));
        assert!(matches!(
            recv_prioritized_event(&mut control_rx, &mut payload_rx).await,
            Some(OmenchatLinkEvent::LinkClosed { link_id, .. }) if link_id == [1; 16]
        ));

        drop(control_tx);
        payload_tx
            .send(Queued {
                value: OmenchatLinkEvent::LinkClosed {
                    link_id: [3; 16],
                    reason: Some("payload after control close".into()),
                },
                _permit: budget.reserve([3; 16], 0).expect("payload permit"),
            })
            .await
            .expect("payload queue");
        assert!(matches!(
            recv_prioritized_event(&mut control_rx, &mut payload_rx).await,
            Some(OmenchatLinkEvent::LinkClosed { link_id, .. }) if link_id == [3; 16]
        ));
        drop(payload_tx);
        assert!(tokio::time::timeout(
            Duration::from_millis(100),
            recv_prioritized_event(&mut control_rx, &mut payload_rx)
        )
        .await
        .expect("closed lanes must not spin or wait")
        .is_none());
    }

    #[test]
    fn resource_offer_preserves_owned_allocations() {
        let payload = vec![0x41; 1024 * 1024];
        let metadata = vec![0x42; 4096];
        let payload_ptr = payload.as_ptr();
        let metadata_ptr = metadata.as_ptr();

        let offer = ResourceOffer::new("resource:test".into(), payload, metadata);

        assert_eq!(offer.payload.as_ptr(), payload_ptr);
        assert_eq!(offer.metadata.as_ptr(), metadata_ptr);
        assert_eq!(offer.queued_bytes(), 1024 * 1024 + 4096 + 13);
    }

    #[test]
    fn outbound_resource_correlation_is_bounded_and_released_exactly() {
        let now = Instant::now();
        let link = [7; 16];
        let mut correlations = OutboundResourceCorrelations::default();
        assert!(correlations.insert(link, [1; 32], "history:one".into(), now));
        assert_eq!(
            correlations.take(link, [1; 32], now),
            Some("history:one".into())
        );
        assert!(correlations.take(link, [1; 32], now).is_none());

        for index in 0..OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS_PER_LINK {
            assert!(correlations.insert(link, [index as u8; 32], format!("resource:{index}"), now,));
        }
        assert!(!correlations.insert(link, [0xff; 32], "overflow".into(), now));
        assert_eq!(
            correlations.entries.len(),
            OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS_PER_LINK
        );
        assert_eq!(
            correlations.remove_link(link),
            OUTBOUND_RESOURCE_CORRELATION_MAX_ITEMS_PER_LINK
        );
        assert!(correlations.entries.is_empty());
        assert_eq!(correlations.retained_bytes, 0);
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

    fn poison_live_worker<T>(worker: &LiveServerWorker<T>)
    where
        T: OmenchatTransport + Send + 'static,
    {
        worker.poison_lock_for_test();
    }

    fn assert_poison_error<T: std::fmt::Debug>(result: ServerResult<T>) {
        let error = result.expect_err("poisoned live-server lock must fail");
        assert_eq!(error.to_string(), "live-server worker lock poisoned");
    }

    #[tokio::test]
    async fn live_worker_poison_is_typed_for_async_and_all_status_accessors() {
        let worker = test_live_worker();
        poison_live_worker(&worker);

        assert_poison_error(
            worker
                .handle_event(OmenchatLinkEvent::LinkClosed {
                    link_id: [0x31; 16],
                    reason: Some("test".into()),
                })
                .await,
        );
        assert_poison_error(worker.stats());
        assert_poison_error(worker.recent_closed_link_summaries());
        assert_poison_error(worker.active_room_counts());
        assert_poison_error(worker.active_link_summaries());
        assert_poison_error(worker.active_identity_counts());
        assert_poison_error(worker.disconnect_identity(b"test identity"));
    }

    #[test]
    fn headless_statistics_poison_returns_fatal_error_and_logs_safely() {
        let config = test_config("headless-stats-poison");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("isolated config");
        let worker = test_live_worker();
        poison_live_worker(&worker);

        assert_poison_error(headless_stats_sample(&worker, &config));
        assert!(crate::server_log::flush(Duration::from_secs(1)));
        let log = std::fs::read_to_string(config.log_path()).expect("server log");
        assert!(log.contains("reticulum-rs live statistics failed"));
        assert!(log.contains("live-server worker lock poisoned"));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[tokio::test]
    async fn live_runtime_shutdown_is_idempotent_and_joins_owned_workers() {
        let config = test_config("owned-shutdown");
        let _ = std::fs::remove_dir_all(config.root_dir());
        let mut runtime = start_live_server(&config)
            .await
            .expect("start live runtime");

        assert_eq!(runtime.owned_tasks.len(), 4);
        tokio::time::timeout(Duration::from_secs(5), runtime.shutdown(&config))
            .await
            .expect("shutdown must be bounded")
            .expect("shutdown");
        assert!(runtime.shutdown_complete);
        assert!(runtime.owned_tasks.is_empty());
        assert_eq!(runtime.queue_metrics().0.queued_items, 0);
        assert_eq!(runtime.queue_metrics().1.queued_items, 0);

        runtime
            .shutdown(&config)
            .await
            .expect("idempotent shutdown");
        drop(runtime);
        assert!(crate::server_log::flush(Duration::from_secs(1)));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[tokio::test]
    async fn poisoned_live_runtime_shutdown_still_cancels_and_joins_owned_workers() {
        let config = test_config("poisoned-owned-shutdown");
        let _ = std::fs::remove_dir_all(config.root_dir());
        let mut runtime = start_live_server(&config)
            .await
            .expect("start live runtime");
        poison_live_worker(&runtime.live_server);

        let error = tokio::time::timeout(Duration::from_secs(5), runtime.shutdown(&config))
            .await
            .expect("shutdown must remain bounded")
            .expect_err("poisoned enumeration must remain visible");
        assert!(error.to_string().contains("active-link enumeration failed"));
        assert!(runtime.shutdown_complete);
        assert!(runtime.owned_tasks.is_empty());
        assert_eq!(runtime.queue_metrics().0.queued_items, 0);
        assert_eq!(runtime.queue_metrics().1.queued_items, 0);

        runtime
            .shutdown(&config)
            .await
            .expect("completed shutdown remains idempotent");
        assert!(crate::server_log::flush(Duration::from_secs(1)));
        let log = std::fs::read_to_string(config.log_path()).expect("server log");
        assert!(log.contains("skipped active-link close enumeration"));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[tokio::test]
    async fn live_startup_rejects_invalid_enabled_interface_before_spawning_workers() {
        let config = test_config("invalid-interface-startup");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");
        std::fs::write(
            config.reticulum_config_file(),
            "[reticulum]\n  panic_on_interface_error = No\n\n[[Broken Client]]\n  type = TCPClientInterface\n  interface_enabled = true\n  target_port = 4242\n",
        )
        .expect("write invalid isolated interface config");

        let error = match start_live_server(&config).await {
            Ok(_) => panic!("invalid enabled interface must fail startup"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("missing target_host"));
        assert!(!error
            .to_string()
            .contains(config.root_dir().to_string_lossy().as_ref()));

        assert!(crate::server_log::flush(Duration::from_secs(1)));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[test]
    fn configured_ifac_tcp_server_is_rejected_instead_of_claiming_enforcement() {
        let interfaces = vec![ReticulumInterface {
            name: "Private Gateway".into(),
            kind: Some("TCPServerInterface".into()),
            enabled: true,
            listen_port: Some(4242),
            network_name: Some("private_ret".into()),
            passphrase: Some("public-test-fixture".into()),
            ..ReticulumInterface::default()
        }];

        let error = validate_reticulum_interfaces(&interfaces)
            .expect_err("stock TCP server must not claim IFAC enforcement");
        assert!(error.to_string().contains("does not enforce"));
        assert!(!error.to_string().contains("public-test-fixture"));
    }

    #[test]
    fn generated_multi_client_config_is_accepted_by_live_runtime_parser() {
        let config = test_config("multiple-tcp-clients");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");
        for (host, port) in [("private.example", 42420), ("wns.example", 42421)] {
            crate::config::add_reticulum_tcp_client_config(
                &config,
                &crate::TcpClientOverride {
                    target_host: host.into(),
                    target_port: port,
                    network_name: None,
                    passphrase: None,
                },
            )
            .expect("add TCP client");
        }

        let interfaces =
            parse_reticulum_interfaces(&config.reticulum_config_file()).expect("parse interfaces");
        validate_reticulum_interfaces(&interfaces).expect("validate interfaces");
        let clients = interfaces
            .iter()
            .filter(|interface| interface.kind.as_deref() == Some("TCPClientInterface"))
            .collect::<Vec<_>>();
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].target_host.as_deref(), Some("private.example"));
        assert_eq!(clients[1].target_host.as_deref(), Some("wns.example"));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[tokio::test]
    async fn live_runtime_owns_two_configured_tcp_client_workers() {
        let config = test_config("multiple-live-tcp-clients");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");
        for port in [42431, 42432] {
            crate::config::add_reticulum_tcp_client_config(
                &config,
                &crate::TcpClientOverride {
                    target_host: "127.0.0.1".into(),
                    target_port: port,
                    network_name: None,
                    passphrase: None,
                },
            )
            .expect("add TCP client");
        }

        let mut runtime = start_live_server(&config)
            .await
            .expect("start live runtime");
        let interfaces = runtime.interface_stats_lines();
        assert_eq!(interfaces.len(), 2);
        assert!(interfaces
            .iter()
            .any(|line| line.contains("127.0.0.1:42431")));
        assert!(interfaces
            .iter()
            .any(|line| line.contains("127.0.0.1:42432")));
        assert_eq!(runtime.owned_tasks.len(), 6);

        tokio::time::timeout(Duration::from_secs(5), runtime.shutdown(&config))
            .await
            .expect("shutdown must be bounded")
            .expect("shutdown");
        assert!(runtime.owned_tasks.is_empty());
        assert!(crate::server_log::flush(Duration::from_secs(1)));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[test]
    fn invalid_existing_identity_is_preserved_and_never_regenerated() {
        let config = test_config("invalid-identity-preserved");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");
        let invalid = b"existing-invalid-private-identity";
        std::fs::write(&config.identity_path, invalid).expect("write invalid identity fixture");

        let error = match load_or_create_identity(&config) {
            Ok(_) => panic!("invalid identity must fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("file was preserved"));
        assert_eq!(
            std::fs::read(&config.identity_path).expect("read preserved identity"),
            invalid
        );
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[cfg(unix)]
    #[test]
    fn identity_symlink_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let config = test_config("identity-symlink-rejected");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");
        let target = config.root_dir().join("identity-target");
        let target_bytes = [0x42; 64];
        std::fs::write(&target, target_bytes).expect("write identity target");
        std::fs::remove_file(&config.identity_path).expect("remove placeholder");
        symlink(&target, &config.identity_path).expect("create identity symlink");

        let error = match load_or_create_identity(&config) {
            Ok(_) => panic!("symlink must fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("non-symlink"));
        assert_eq!(std::fs::read(&target).expect("read target"), target_bytes);
        let _ = std::fs::remove_dir_all(config.root_dir());
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
        assert!(
            !task.is_finished(),
            "worker must still be blocked on the deliberately held server lock"
        );

        release_tx.send(()).expect("release worker lock");
        lock_thread.join().expect("lock thread");
        task.await.expect("worker task").expect("handle event");
        let metrics = worker.worker_metrics();
        assert_eq!(metrics.in_flight, 0);
        assert_eq!(metrics.completed, 1);
        assert_eq!(metrics.rejected, 0);
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
        let stats = worker.stats().expect("worker stats");
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

    #[test]
    fn split_resource_rejection_markers_are_deduplicated_bounded_and_expire() {
        let start = Instant::now();
        let mut rejected = BTreeMap::new();
        let metrics = SplitResourceSafeguardMetrics::default();
        assert!(remember_rejected_split_resource(
            &mut rejected,
            [0x01; 32],
            start,
            &metrics,
        ));
        assert!(!remember_rejected_split_resource(
            &mut rejected,
            [0x01; 32],
            start + Duration::from_secs(1),
            &metrics,
        ));
        for index in 0..=REJECTED_SPLIT_RESOURCE_MAX_ITEMS {
            let mut hash = [0u8; 32];
            hash[..8].copy_from_slice(&(index as u64 + 2).to_be_bytes());
            assert!(remember_rejected_split_resource(
                &mut rejected,
                hash,
                start + Duration::from_secs(2 + index as u64),
                &metrics,
            ));
            assert!(rejected.len() <= REJECTED_SPLIT_RESOURCE_MAX_ITEMS);
        }
        assert!(!rejected.contains_key(&[0x01; 32]));

        let expired_at = start + REJECTED_SPLIT_RESOURCE_TTL + Duration::from_secs(600);
        assert!(remember_rejected_split_resource(
            &mut rejected,
            [0xff; 32],
            expired_at,
            &metrics,
        ));
        assert_eq!(rejected.len(), 1);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.split_resources_rejected, 259);
        assert_eq!(snapshot.late_split_completions_suppressed, 0);
        assert_eq!(snapshot.split_rejection_markers_expired, 258);
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

    #[tokio::test]
    async fn reticulum_resource_terminals_cross_production_bridge_and_shutdown_cleanly() {
        use rns_transport::hash::Hash;
        use rns_transport::resource::{ResourceEvent, ResourceFailure, ResourceProgress};
        use tokio::sync::broadcast;

        let config = test_config("resource-terminal-bridge");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");

        let budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
        let (payload_tx, _payload_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
        let (control_tx, mut control_rx) = mpsc::channel(EVENT_CONTROL_ITEMS);
        let event_tx = EventQueueSender {
            payload_tx,
            control_tx,
            budget: budget.clone(),
            log_path: config.log_path(),
        };
        let (resource_tx, resource_rx) = broadcast::channel(8);
        let shutdown = CancellationToken::new();
        let identity = PrivateIdentity::new_from_name("omenchatd-resource-terminal-bridge");
        let transport = Arc::new(Transport::new(TransportConfig::new(
            "omenchatd-resource-terminal-bridge",
            &identity,
            true,
        )));
        let metrics = Arc::new(SplitResourceSafeguardMetrics::default());
        let bridge = spawn_resource_event_receiver(
            transport,
            resource_rx,
            event_tx,
            config.clone(),
            shutdown.clone(),
            Arc::new(Mutex::new(OutboundResourceCorrelations::default())),
            metrics.clone(),
        );
        let link_id = AddressHash::new([0x42; 16]);

        let cases = [
            (
                ResourceEventKind::InboundFailed(ResourceFailure {
                    reason: "forced\nbridge\tfailure".into(),
                    progress: ResourceProgress {
                        received_bytes: 7,
                        total_bytes: 99,
                        received_parts: 1,
                        total_parts: 3,
                    },
                }),
                LiveResourceDirection::Inbound,
                LiveResourceOutcome::Failed,
            ),
            (
                ResourceEventKind::OutboundComplete,
                LiveResourceDirection::Outbound,
                LiveResourceOutcome::Complete,
            ),
            (
                ResourceEventKind::OutboundFailed,
                LiveResourceDirection::Outbound,
                LiveResourceOutcome::Failed,
            ),
            (
                ResourceEventKind::OutboundCancelled,
                LiveResourceDirection::Outbound,
                LiveResourceOutcome::Cancelled,
            ),
        ];

        for (sequence, (kind, _, _)) in cases.iter().enumerate() {
            resource_tx
                .send(ResourceEvent {
                    hash: Hash::new([sequence as u8 + 1; 32]),
                    link_id,
                    kind: kind.clone(),
                })
                .expect("production bridge owns resource receiver");
        }

        for (sequence, (_, expected_direction, expected_outcome)) in cases.into_iter().enumerate() {
            let expected_size = (sequence == 0).then_some(99);
            let expected_reason = (sequence == 0).then(|| "forcedbridgefailure".to_string());
            let queued = tokio::time::timeout(Duration::from_secs(1), control_rx.recv())
                .await
                .expect("terminal must cross bridge promptly")
                .expect("control lane remains open");
            assert_eq!(
                queued.value,
                OmenchatLinkEvent::ResourceTerminal {
                    link_id: [0x42; 16],
                    resource_hash: [sequence as u8 + 1; 32],
                    resource_id: None,
                    direction: expected_direction,
                    outcome: expected_outcome,
                    expected_size,
                    reason: expected_reason,
                }
            );
            drop(queued);
        }
        assert_eq!(budget.snapshot().queued_items, 0);
        assert_eq!(budget.snapshot().queued_bytes, 0);

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(1), bridge)
            .await
            .expect("resource bridge shutdown must be bounded")
            .expect("resource bridge task must join");
        assert!(crate::server_log::flush(Duration::from_secs(1)));
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[tokio::test]
    async fn split_resource_terminal_suppresses_later_completion_without_runtime_shutdown() {
        use rns_transport::hash::Hash;
        use rns_transport::resource::{ResourceComplete, ResourceEvent, ResourceSegmentProgress};
        use tokio::sync::broadcast;

        let config = test_config("split-resource-terminal-bridge");
        let _ = std::fs::remove_dir_all(config.root_dir());
        crate::config::init_files(&config).expect("init isolated config");
        let budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
        let (payload_tx, mut payload_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
        let (control_tx, mut control_rx) = mpsc::channel(EVENT_CONTROL_ITEMS);
        let event_tx = EventQueueSender {
            payload_tx,
            control_tx,
            budget,
            log_path: config.log_path(),
        };
        let (resource_tx, resource_rx) = broadcast::channel(8);
        let shutdown = CancellationToken::new();
        let identity = PrivateIdentity::new_from_name("omenchatd-split-terminal-bridge");
        let transport = Arc::new(Transport::new(TransportConfig::new(
            "omenchatd-split-terminal-bridge",
            &identity,
            true,
        )));
        let metrics = Arc::new(SplitResourceSafeguardMetrics::default());
        let bridge = spawn_resource_event_receiver(
            transport,
            resource_rx,
            event_tx,
            config.clone(),
            shutdown.clone(),
            Arc::new(Mutex::new(OutboundResourceCorrelations::default())),
            metrics.clone(),
        );
        let link_id = AddressHash::new([0x52; 16]);
        let hash = Hash::new([0x53; 32]);
        resource_tx
            .send(ResourceEvent {
                hash,
                link_id,
                kind: ResourceEventKind::SegmentComplete(ResourceSegmentProgress {
                    original_hash: hash,
                    segment_index: 1,
                    total_segments: 2,
                    total_data_size: 1_048_576,
                }),
            })
            .expect("bridge owns split event receiver");
        let terminal = tokio::time::timeout(Duration::from_secs(1), control_rx.recv())
            .await
            .expect("split terminal arrives")
            .expect("control queue open");
        let OmenchatLinkEvent::ResourceTerminal {
            resource_hash,
            outcome,
            expected_size,
            ..
        } = terminal.value
        else {
            panic!("expected split Resource terminal");
        };
        assert_eq!(resource_hash, [0x53; 32]);
        assert_eq!(outcome, LiveResourceOutcome::Failed);
        assert_eq!(expected_size, Some(1_048_576));
        drop(terminal);

        resource_tx
            .send(ResourceEvent {
                hash,
                link_id,
                kind: ResourceEventKind::Complete(ResourceComplete {
                    data: vec![0x99; 64],
                    metadata: Some(b"omenchat-resource:upload:1".to_vec()),
                    request_id: None,
                    is_request: false,
                    is_response: false,
                }),
            })
            .expect("bridge owns completion receiver");
        assert!(
            tokio::time::timeout(Duration::from_millis(100), payload_rx.recv())
                .await
                .is_err(),
            "completion queued after split rejection must not reach application state"
        );
        assert_eq!(
            metrics.snapshot(),
            SplitResourceSafeguardMetricsSnapshot {
                split_resources_rejected: 1,
                late_split_completions_suppressed: 1,
                split_rejection_markers_expired: 0,
            }
        );

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(1), bridge)
            .await
            .expect("resource bridge shutdown bounded")
            .expect("resource bridge joins");
        let _ = std::fs::remove_dir_all(config.root_dir());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "explicit loopback Reticulum Resource initiator-cancellation interoperability test"]
    async fn reticulum_loopback_resource_cancel_crosses_wire_and_production_bridge() {
        use rns_transport::delivery::await_link_activation;
        use rns_transport::destination::link::LinkStatus;
        use rns_transport::iface::udp::UdpInterface;

        fn reserve_udp_ports() -> (u16, u16) {
            let first = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve first UDP port");
            let second = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve second UDP port");
            let first_port = first.local_addr().expect("first UDP address").port();
            let second_port = second.local_addr().expect("second UDP address").port();
            assert_ne!(first_port, second_port);
            (first_port, second_port)
        }

        fn event_queue(
            config: &ServerConfig,
        ) -> (
            EventQueueSender,
            mpsc::Receiver<Queued<OmenchatLinkEvent>>,
            Arc<QueueBudget>,
        ) {
            let budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
            let (payload_tx, _payload_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
            let (control_tx, control_rx) = mpsc::channel(EVENT_CONTROL_ITEMS);
            (
                EventQueueSender {
                    payload_tx,
                    control_tx,
                    budget: budget.clone(),
                    log_path: config.log_path(),
                },
                control_rx,
                budget,
            )
        }

        let nonce = current_epoch_ms();
        let server_config = test_config(&format!("resource-loopback-server-{nonce}"));
        let client_config = test_config(&format!("resource-loopback-client-{nonce}"));
        for config in [&server_config, &client_config] {
            let _ = std::fs::remove_dir_all(config.root_dir());
            crate::config::init_files(config).expect("init isolated loopback config");
        }

        let server_identity =
            PrivateIdentity::new_from_name(&format!("omenchatd-loopback-server-{nonce}"));
        let client_identity =
            PrivateIdentity::new_from_name(&format!("omenchatd-loopback-client-{nonce}"));
        let mut server_transport_config =
            TransportConfig::new("omenchatd-resource-loopback-server", &server_identity, true);
        server_transport_config.set_resource_retry_interval_secs(1);
        server_transport_config.set_resource_retry_limit(10);
        let server_transport = Transport::new(server_transport_config);
        let server_destination = server_transport
            .add_destination(
                server_identity,
                DestinationName::new(OMENCHAT_RNS_APP_NAME, "resource-test"),
            )
            .await;
        let server_transport = Arc::new(server_transport);
        let mut client_transport_config =
            TransportConfig::new("omenchatd-resource-loopback-client", &client_identity, true);
        client_transport_config.set_resource_retry_interval_secs(1);
        client_transport_config.set_resource_retry_limit(10);
        let client_transport = Arc::new(Transport::new(client_transport_config));

        let (server_port, client_port) = reserve_udp_ports();
        server_transport.iface_manager().lock().await.spawn(
            UdpInterface::new(
                format!("127.0.0.1:{server_port}"),
                Some(format!("127.0.0.1:{client_port}")),
            ),
            UdpInterface::spawn,
        );
        client_transport.iface_manager().lock().await.spawn(
            UdpInterface::new(
                format!("127.0.0.1:{client_port}"),
                Some(format!("127.0.0.1:{server_port}")),
            ),
            UdpInterface::spawn,
        );
        tokio::time::sleep(Duration::from_millis(100)).await;

        let (client_events, mut client_control_rx, client_budget) = event_queue(&client_config);
        let shutdown = CancellationToken::new();
        let client_bridge = spawn_resource_event_receiver(
            client_transport.clone(),
            client_transport.resource_events(),
            client_events,
            client_config.clone(),
            shutdown.clone(),
            Arc::new(Mutex::new(OutboundResourceCorrelations::default())),
            Arc::new(SplitResourceSafeguardMetrics::default()),
        );

        let destination = server_destination.lock().await.desc;
        server_transport
            .send_announce(&server_destination, None)
            .await;
        assert!(
            client_transport
                .await_path(&destination.address_hash, Duration::from_secs(5), None)
                .await,
            "client must learn the server path over loopback UDP"
        );
        let link = client_transport.link(destination).await;
        await_link_activation(&client_transport, &link, Duration::from_secs(5))
            .await
            .expect("loopback link activation");
        let link_id = *link.lock().await.id();
        let mut server_wire_rx = server_transport.iface_rx();

        let cancelled_hash = client_transport
            .send_resource(
                &link_id,
                vec![0x51; 4 * 1024],
                Some(OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec()),
            )
            .await
            .expect("advertise cancellable Resource");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = server_wire_rx
                    .recv()
                    .await
                    .expect("server interface event stream remains open");
                if message.packet.context == PacketContext::ResourceAdvrtisement {
                    assert_eq!(
                        message.packet.header.destination_type,
                        rns_transport::packet::DestinationType::Link
                    );
                    break;
                }
            }
        })
        .await
        .expect("server must physically receive the Resource advertisement");
        assert!(
            client_transport
                .cancel_resource(&link_id, cancelled_hash)
                .await
                .expect("send initiator cancellation"),
            "active outbound Resource must be cancellable"
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = server_wire_rx
                    .recv()
                    .await
                    .expect("server interface event stream remains open");
                if message.packet.context == PacketContext::ResourceInitiatorCancel {
                    assert_eq!(
                        message.packet.header.destination_type,
                        rns_transport::packet::DestinationType::Link
                    );
                    break;
                }
            }
        })
        .await
        .expect("server must physically receive initiator cancellation");
        let cancelled = tokio::time::timeout(Duration::from_secs(2), client_control_rx.recv())
            .await
            .expect("cancel terminal timeout")
            .expect("client control bridge remains open");
        assert_eq!(
            cancelled.value,
            OmenchatLinkEvent::ResourceTerminal {
                link_id: address_hash_bytes(link_id),
                resource_hash: cancelled_hash.to_bytes(),
                resource_id: None,
                direction: LiveResourceDirection::Outbound,
                outcome: LiveResourceOutcome::Cancelled,
                expected_size: None,
                reason: None,
            }
        );
        drop(cancelled);
        assert_eq!(link.lock().await.status(), LinkStatus::Active);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let server_link = server_transport
            .find_in_link(&link_id)
            .await
            .expect("server retains inbound link after initiator cancellation");
        assert_eq!(server_link.lock().await.status(), LinkStatus::Active);

        assert_eq!(client_budget.snapshot().queued_items, 0);

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(1), client_bridge)
            .await
            .expect("loopback Resource bridge shutdown must be bounded")
            .expect("loopback Resource bridge task must join");
        assert!(server_transport.detach_interfaces().await >= 1);
        assert!(client_transport.detach_interfaces().await >= 1);
        assert!(crate::server_log::flush(Duration::from_secs(1)));
        for config in [&server_config, &client_config] {
            let _ = std::fs::remove_dir_all(config.root_dir());
        }
    }
}

#[cfg(test)]
#[path = "reticulum_live_soak_tests.rs"]
mod soak_tests;

#[cfg(test)]
#[path = "reticulum_live_db_soak_tests.rs"]
mod db_soak_tests;

#[cfg(test)]
#[path = "reticulum_live_multiprocess_tests.rs"]
mod multiprocess_tests;
