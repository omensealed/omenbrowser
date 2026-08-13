#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeLxmfClientState {
    pub started: bool,
}

#[cfg(feature = "native-lxmf-sdk")]
use crate::error::{AppError, AppResult};
#[cfg(feature = "native-lxmf-sdk")]
use crate::runtime::lxmf_topics::{
    LxmfTopicCapabilityReport, LXMF_TOPIC_CAP_ASYNC_EVENTS, LXMF_TOPIC_CAP_CURSOR_REPLAY,
    LXMF_TOPIC_CAP_FANOUT, LXMF_TOPIC_CAP_SUBSCRIPTIONS, LXMF_TOPIC_CAP_TOPICS,
};
#[cfg(feature = "native-lxmf-sdk")]
use crate::runtime::LxmfCancelOutcome;
#[cfg(feature = "native-lxmf-sdk")]
use crate::runtime::{LxmfHistoryPage, LxmfHistoryRecord, LxmfHistoryRequest};

#[cfg(feature = "native-lxmf-sdk")]
use std::collections::{HashMap, VecDeque};

#[cfg(feature = "native-lxmf-sdk")]
use std::io;

#[cfg(feature = "native-lxmf-sdk")]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

#[cfg(feature = "native-lxmf-sdk")]
use rand_core::RngCore;

#[cfg(feature = "native-lxmf-sdk")]
const LXMF_TICKET_FIELD: i64 = 0x0C;
#[cfg(feature = "native-lxmf-sdk")]
const LXMF_TICKET_LENGTH: usize = 16;
#[cfg(feature = "native-lxmf-sdk")]
const LXMF_TICKET_EXPIRY_SECONDS: f64 = 21.0 * 24.0 * 60.0 * 60.0;

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq)]
pub struct NativeLxmfSdkSendPlan {
    pub send_request: lxmf_sdk::SendRequest,
    pub rpc_delivery: rns_rpc::OutboundDeliveryOptions,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeLxmfSdkSenderState {
    Ready,
    Configured,
    MissingEndpoint,
    RejectedEndpoint,
    NotWired,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkSenderStatus {
    pub name: &'static str,
    pub state: NativeLxmfSdkSenderState,
    pub note: &'static str,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkSendReceipt {
    pub message_id: Option<String>,
    pub accepted: bool,
    pub state: String,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkWireDelivery {
    pub wire_bytes: Vec<u8>,
    pub message_id: String,
    pub destination_hash: String,
    pub method: Option<String>,
    pub try_propagation_on_fail: bool,
    pub include_ticket: bool,
    pub reply_ticket_used: bool,
    pub direct_stamp: Option<NativeLxmfSdkDirectStamp>,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkDirectStamp {
    pub target_cost: u8,
    pub stamp_value: u32,
    pub attempts: u64,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkProbe {
    pub endpoint: String,
    pub runtime_id: String,
    pub state: String,
    pub active_contract_version: u16,
    pub event_stream_position: u64,
    pub config_revision: u64,
    pub queued_messages: u64,
    pub in_flight_messages: u64,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfTopicCapabilityProbe {
    pub endpoint: String,
    pub active_contract_version: u16,
    pub capabilities: LxmfTopicCapabilityReport,
}

#[cfg(feature = "native-lxmf-sdk")]
pub const NATIVE_LXMF_TOPIC_CAPABILITY_PROBE_DEADLINE_MS: u64 = 10_000;

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcNativeLxmfSdkSender {
    endpoint: String,
}

#[cfg(feature = "native-lxmf-sdk")]
pub struct EmbeddedNativeLxmfSdkSender {
    daemon: Arc<rns_rpc::RpcDaemon>,
    next_request_id: AtomicU64,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Debug, Default)]
pub struct NativeLxmfSdkTicketCache {
    state: Mutex<NativeLxmfSdkTicketCacheState>,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Debug, Default)]
struct NativeLxmfSdkTicketCacheState {
    pending: HashMap<String, Option<String>>,
    order: VecDeque<String>,
    ticket_bytes: usize,
}

#[cfg(feature = "native-lxmf-sdk")]
const NATIVE_LXMF_SDK_TICKET_CACHE_MAX_ITEMS: usize = 1_024;
#[cfg(feature = "native-lxmf-sdk")]
const NATIVE_LXMF_SDK_TICKET_CACHE_MAX_BYTES: usize = 256 * 1024;
#[cfg(feature = "native-lxmf-sdk")]
const NATIVE_LXMF_SDK_TICKET_MAX_BYTES: usize = 256;

#[cfg(feature = "native-lxmf-sdk")]
pub trait NativeLxmfSdkWireSubmitter: Send + Sync {
    fn submit_wire(&self, delivery: &NativeLxmfSdkWireDelivery) -> io::Result<()>;
}

#[cfg(feature = "native-lxmf-sdk")]
pub struct NativeLxmfSdkOutboundBridge {
    identity_bytes: Vec<u8>,
    submitter: Arc<dyn NativeLxmfSdkWireSubmitter>,
    ticket_cache: NativeLxmfSdkTicketCache,
}

#[cfg(feature = "native-lxmf-sdk")]
impl RpcNativeLxmfSdkSender {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    pub fn endpoint(&self) -> &str {
        self.endpoint.as_str()
    }

    pub fn diagnostic_endpoint(&self) -> Option<String> {
        validate_local_rpc_endpoint(self.endpoint.as_str())
            .ok()
            .map(|endpoint| endpoint.diagnostic_label)
    }

    /// Negotiates the topic-related SDK surface without subscribing, publishing,
    /// starting a product worker, or sending a shutdown request to the daemon.
    pub async fn probe_topic_capabilities(&self) -> AppResult<NativeLxmfTopicCapabilityProbe> {
        self.probe_topic_capabilities_with_deadline(std::time::Duration::from_millis(
            NATIVE_LXMF_TOPIC_CAPABILITY_PROBE_DEADLINE_MS,
        ))
        .await
    }

    async fn probe_topic_capabilities_with_deadline(
        &self,
        deadline: std::time::Duration,
    ) -> AppResult<NativeLxmfTopicCapabilityProbe> {
        let validated = validate_local_rpc_endpoint(self.endpoint.as_str())?;
        let backend = lxmf_sdk::RpcBackendClient::new(self.endpoint.clone());
        let client = lxmf_sdk::Client::new(backend);
        let request = lxmf_sdk::StartRequest::new(lxmf_sdk::SdkConfig::desktop_local_default())
            .with_requested_capabilities([
                LXMF_TOPIC_CAP_TOPICS,
                LXMF_TOPIC_CAP_SUBSCRIPTIONS,
                LXMF_TOPIC_CAP_FANOUT,
                LXMF_TOPIC_CAP_CURSOR_REPLAY,
                LXMF_TOPIC_CAP_ASYNC_EVENTS,
            ]);
        let handle = tokio::time::timeout(deadline, client.start_async(request))
            .await
            .map_err(|_| {
                AppError::Runtime("external LXMF topic capability negotiation timed out".into())
            })?
            .map_err(|error| {
                AppError::Runtime(format!(
                    "external LXMF topic capability negotiation failed ({:?})",
                    error.category
                ))
            })?;
        let capabilities = LxmfTopicCapabilityReport::external_negotiated(
            &handle.effective_capabilities,
            false,
            false,
            false,
        )
        .map_err(|error| {
            AppError::Runtime(format!(
                "external LXMF topic capability response was rejected: {error}"
            ))
        })?;

        Ok(NativeLxmfTopicCapabilityProbe {
            endpoint: validated.diagnostic_label,
            active_contract_version: handle.active_contract_version,
            capabilities,
        })
    }
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedLocalRpcEndpoint {
    diagnostic_label: String,
}

#[cfg(feature = "native-lxmf-sdk")]
fn validate_local_rpc_endpoint(endpoint: &str) -> AppResult<ValidatedLocalRpcEndpoint> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC endpoint is not configured".into(),
        ));
    }
    if endpoint.chars().any(char::is_control) {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC endpoint contains control characters".into(),
        ));
    }
    if let Some(path) = endpoint
        .strip_prefix("unix://")
        .or_else(|| endpoint.strip_prefix("unix:"))
        .map(str::trim)
    {
        if path.is_empty() || !std::path::Path::new(path).is_absolute() {
            return Err(AppError::Unsupported(
                "native LXMF SDK/RPC Unix endpoint must use an absolute socket path".into(),
            ));
        }
        #[cfg(not(unix))]
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC Unix endpoints are unavailable on this platform".into(),
        ));
        #[cfg(unix)]
        return Ok(ValidatedLocalRpcEndpoint {
            diagnostic_label: "unix:<local-socket>".into(),
        });
    }

    let authority_and_path = endpoint
        .strip_prefix("tcp://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint);
    if (endpoint.contains("://") && authority_and_path == endpoint)
        || endpoint.starts_with("https://")
        || endpoint.starts_with("tls://")
    {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC endpoint requires a supported local transport; remote or implied-TLS endpoints need an explicit authenticated configuration"
                .into(),
        ));
    }
    let (authority, path) = authority_and_path
        .split_once('/')
        .map_or((authority_and_path, ""), |(authority, path)| {
            (authority, path)
        });
    if !path.is_empty() && path != "rpc" {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC endpoint path must be /rpc".into(),
        ));
    }
    if authority.contains('@') || authority.contains('?') || authority.contains('#') {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC endpoint must not contain credentials or query data".into(),
        ));
    }
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some(end) = bracketed.find(']') else {
            return Err(AppError::Unsupported(
                "native LXMF SDK/RPC endpoint has an invalid bracketed host".into(),
            ));
        };
        let host = &bracketed[..end];
        let suffix = &bracketed[end + 1..];
        let Some(port) = suffix.strip_prefix(':') else {
            return Err(AppError::Unsupported(
                "native LXMF SDK/RPC endpoint must include a port".into(),
            ));
        };
        (host, port)
    } else {
        authority.rsplit_once(':').ok_or_else(|| {
            AppError::Unsupported(
                "native LXMF SDK/RPC endpoint must include a host and port".into(),
            )
        })?
    };
    let address = host.parse::<std::net::IpAddr>().map_err(|_| {
        AppError::Unsupported(
            "native LXMF SDK/RPC local-trusted endpoint must use a literal loopback address".into(),
        )
    })?;
    if !address.is_loopback() {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC remote endpoints require explicit authenticated configuration"
                .into(),
        ));
    }
    let port = port.parse::<u16>().map_err(|_| {
        AppError::Unsupported("native LXMF SDK/RPC endpoint port is invalid".into())
    })?;
    if port == 0 {
        return Err(AppError::Unsupported(
            "native LXMF SDK/RPC endpoint port must be nonzero".into(),
        ));
    }
    Ok(ValidatedLocalRpcEndpoint {
        diagnostic_label: format!("loopback:{port}"),
    })
}

#[cfg(feature = "native-lxmf-sdk")]
impl EmbeddedNativeLxmfSdkSender {
    pub fn new(daemon: rns_rpc::RpcDaemon) -> Self {
        Self::from_shared(Arc::new(daemon))
    }

    pub fn from_shared(daemon: Arc<rns_rpc::RpcDaemon>) -> Self {
        Self {
            daemon,
            next_request_id: AtomicU64::new(1),
        }
    }

    fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }
}

#[cfg(feature = "native-lxmf-sdk")]
impl NativeLxmfSdkTicketCache {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, NativeLxmfSdkTicketCacheState> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                *state = NativeLxmfSdkTicketCacheState::default();
                self.state.clear_poison();
                tracing::warn!(
                    "recovered poisoned auxiliary native LXMF SDK ticket cache; cached tickets were discarded"
                );
                state
            }
        }
    }

    pub fn capture_validate_record(&self, record: &rns_rpc::MessageRecord) -> io::Result<()> {
        let ticket = native_lxmf_sdk_record_ticket(record);
        if ticket
            .as_ref()
            .is_some_and(|ticket| ticket.len() > NATIVE_LXMF_SDK_TICKET_MAX_BYTES)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "native LXMF SDK ticket exceeds byte limit",
            ));
        }
        let key = native_lxmf_sdk_ticket_cache_key(record.id.as_str());
        let mut state = self.lock_state();
        if let Some(previous) = state.pending.remove(&key) {
            state.ticket_bytes = state
                .ticket_bytes
                .saturating_sub(previous.as_ref().map_or(0, String::len));
            state.order.retain(|stored| stored != &key);
        }
        state.ticket_bytes = state
            .ticket_bytes
            .saturating_add(ticket.as_ref().map_or(0, String::len));
        state.pending.insert(key.clone(), ticket);
        state.order.push_back(key);
        while state.pending.len() > NATIVE_LXMF_SDK_TICKET_CACHE_MAX_ITEMS
            || state.ticket_bytes > NATIVE_LXMF_SDK_TICKET_CACHE_MAX_BYTES
        {
            let Some(oldest) = state.order.pop_front() else {
                break;
            };
            if let Some(removed) = state.pending.remove(&oldest) {
                state.ticket_bytes = state
                    .ticket_bytes
                    .saturating_sub(removed.as_ref().map_or(0, String::len));
            }
        }
        Ok(())
    }

    pub fn take_ticket(&self, message_id: &str) -> Option<String> {
        let key = native_lxmf_sdk_ticket_cache_key(message_id);
        let mut state = self.lock_state();
        state.order.retain(|stored| stored != &key);
        let removed = state.pending.remove(&key);
        state.ticket_bytes = state.ticket_bytes.saturating_sub(
            removed
                .as_ref()
                .and_then(Option::as_ref)
                .map_or(0, String::len),
        );
        removed.flatten()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.lock_state().pending.is_empty()
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn native_lxmf_sdk_ticket_cache_key(message_id: &str) -> String {
    use sha2::{Digest, Sha256};
    hex_bytes(&Sha256::digest(message_id.as_bytes()))
}

#[cfg(feature = "native-lxmf-sdk")]
impl NativeLxmfSdkOutboundBridge {
    pub fn new(identity_bytes: Vec<u8>, submitter: Arc<dyn NativeLxmfSdkWireSubmitter>) -> Self {
        Self {
            identity_bytes,
            submitter,
            ticket_cache: NativeLxmfSdkTicketCache::default(),
        }
    }
}

#[cfg(feature = "native-lxmf-sdk")]
impl rns_rpc::OutboundBridge for NativeLxmfSdkOutboundBridge {
    fn validate_delivery(
        &self,
        record: &rns_rpc::MessageRecord,
        _options: &rns_rpc::OutboundDeliveryOptions,
    ) -> io::Result<()> {
        self.ticket_cache.capture_validate_record(record)
    }

    fn deliver(
        &self,
        record: &rns_rpc::MessageRecord,
        options: &rns_rpc::OutboundDeliveryOptions,
    ) -> io::Result<()> {
        let reply_ticket = self.ticket_cache.take_ticket(record.id.as_str());
        let delivery = build_sdk_wire_delivery(
            record,
            options,
            self.identity_bytes.as_slice(),
            reply_ticket.as_deref(),
        )
        .map_err(|error| io::Error::other(error.to_string()))?;
        self.submitter.submit_wire(&delivery)
    }
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn native_lxmf_sdk_record_ticket(record: &rns_rpc::MessageRecord) -> Option<String> {
    record
        .fields
        .as_ref()
        .and_then(|fields| fields.get("_lxmf"))
        .and_then(|lxmf| lxmf.get("ticket"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn build_sdk_wire_delivery(
    record: &rns_rpc::MessageRecord,
    options: &rns_rpc::OutboundDeliveryOptions,
    identity_bytes: &[u8],
    reply_ticket_hex: Option<&str>,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    build_sdk_wire_delivery_with_issued_ticket(
        record,
        options,
        identity_bytes,
        reply_ticket_hex,
        None,
    )
}

#[cfg(feature = "native-lxmf-sdk")]
fn build_sdk_wire_delivery_with_issued_ticket(
    record: &rns_rpc::MessageRecord,
    options: &rns_rpc::OutboundDeliveryOptions,
    identity_bytes: &[u8],
    reply_ticket_hex: Option<&str>,
    issued_ticket: Option<&crate::messaging::NativeLxmfReplyTicket>,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    build_sdk_wire_delivery_with_policy(
        record,
        options,
        identity_bytes,
        reply_ticket_hex,
        issued_ticket,
        None,
        || false,
    )
}

#[cfg(feature = "native-lxmf-sdk")]
fn build_sdk_wire_delivery_with_policy(
    record: &rns_rpc::MessageRecord,
    options: &rns_rpc::OutboundDeliveryOptions,
    identity_bytes: &[u8],
    reply_ticket_hex: Option<&str>,
    issued_ticket: Option<&crate::messaging::NativeLxmfReplyTicket>,
    direct_stamp_cost: Option<u8>,
    cancelled: impl FnMut() -> bool,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    let fields = sdk_record_fields_to_rmpv(record.fields.as_ref())?;
    build_sdk_wire_delivery_with_policy_fields(
        record,
        options,
        identity_bytes,
        reply_ticket_hex,
        NativeLxmfWireFieldPolicy {
            issued_ticket,
            direct_stamp_cost,
            fields,
        },
        cancelled,
    )
}

#[cfg(feature = "native-lxmf-sdk")]
struct NativeLxmfWireFieldPolicy<'a> {
    issued_ticket: Option<&'a crate::messaging::NativeLxmfReplyTicket>,
    direct_stamp_cost: Option<u8>,
    fields: Option<rmpv::Value>,
}

#[cfg(feature = "native-lxmf-sdk")]
fn build_sdk_wire_delivery_with_policy_fields(
    record: &rns_rpc::MessageRecord,
    options: &rns_rpc::OutboundDeliveryOptions,
    identity_bytes: &[u8],
    reply_ticket_hex: Option<&str>,
    policy: NativeLxmfWireFieldPolicy<'_>,
    cancelled: impl FnMut() -> bool,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    let NativeLxmfWireFieldPolicy {
        issued_ticket,
        direct_stamp_cost,
        mut fields,
    } = policy;
    let destination = parse_lxmf_hash_hex(record.destination.as_str())?;
    let source = parse_lxmf_hash_hex(record.source.as_str())?;
    let signer =
        reticulum_rs::core::identity::PrivateIdentity::from_private_key_bytes(identity_bytes)
            .map_err(|_| AppError::Runtime("native LXMF SDK bridge identity is invalid".into()))?;

    if options.include_ticket {
        match issued_ticket {
            Some(ticket) => insert_issued_lxmf_ticket_field(&mut fields, ticket)?,
            None => insert_lxmf_ticket_field(&mut fields),
        }
    }

    let mut message = lxmf::Message::new();
    message.destination_hash = Some(destination);
    message.source_hash = Some(source);
    message.set_title_from_string(record.title.as_str());
    message.set_content_from_string(record.content.as_str());
    message.set_state(lxmf::wire::message::State::Outbound);
    message.timestamp = Some(record.timestamp as f64);
    message.fields = fields;

    let mut reply_ticket_used = false;
    let mut direct_stamp = None;
    if let Some(ticket_hex) = reply_ticket_hex {
        let ticket = parse_lxmf_ticket_hex(ticket_hex)?;
        let message_id = sdk_message_id(&message)?;
        let stamp = sdk_ticket_stamp(ticket.as_slice(), &message_id)?;
        message.set_stamp_from_bytes(&stamp);
        reply_ticket_used = true;
    } else if let Some(target_cost) = direct_stamp_cost {
        let message_id = sdk_message_id(&message)?;
        let generated =
            crate::runtime::native_lxmf::codec::generate_direct_stamp_for_message_cancellable(
                message_id,
                target_cost,
                crate::runtime::native_lxmf::codec::CLEAN_DIRECT_STAMP_MAX_ATTEMPTS,
                cancelled,
            )?;
        message.set_stamp_from_bytes(&generated.stamp);
        direct_stamp = Some(NativeLxmfSdkDirectStamp {
            target_cost: generated.target_cost,
            stamp_value: generated.stamp_value,
            attempts: generated.attempts,
        });
    }

    let wire_bytes = message.to_wire(Some(&signer)).map_err(|error| {
        AppError::Runtime(format!("LXMF SDK bridge wire encode failed: {error}"))
    })?;
    let wire = lxmf::WireMessage::unpack(wire_bytes.as_slice()).map_err(|error| {
        AppError::Runtime(format!(
            "LXMF SDK bridge encoded wire decode failed: {error}"
        ))
    })?;

    let message_id = wire.try_message_id().map_err(|error| {
        AppError::Runtime(format!("LXMF SDK bridge message ID encode failed: {error}"))
    })?;

    Ok(NativeLxmfSdkWireDelivery {
        wire_bytes,
        message_id: hex_bytes(&message_id),
        destination_hash: record.destination.clone(),
        method: options.method.clone(),
        try_propagation_on_fail: options.try_propagation_on_fail,
        include_ticket: options.include_ticket,
        reply_ticket_used,
        direct_stamp,
    })
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn build_sdk_wire_delivery_from_envelope(
    envelope: &crate::messaging::MessageEnvelope,
    source_hash: &str,
    identity_bytes: &[u8],
    stamp_cost: Option<u32>,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    build_sdk_wire_delivery_from_envelope_with_issued_ticket(
        envelope,
        source_hash,
        identity_bytes,
        stamp_cost,
        None,
    )
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn build_sdk_wire_delivery_from_envelope_with_issued_ticket(
    envelope: &crate::messaging::MessageEnvelope,
    source_hash: &str,
    identity_bytes: &[u8],
    stamp_cost: Option<u32>,
    issued_ticket: Option<&crate::messaging::NativeLxmfReplyTicket>,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    build_sdk_wire_delivery_from_envelope_with_policy(
        envelope,
        source_hash,
        identity_bytes,
        stamp_cost,
        issued_ticket,
        None,
        || false,
    )
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn build_sdk_wire_delivery_from_envelope_with_policy(
    envelope: &crate::messaging::MessageEnvelope,
    source_hash: &str,
    identity_bytes: &[u8],
    stamp_cost: Option<u32>,
    issued_ticket: Option<&crate::messaging::NativeLxmfReplyTicket>,
    direct_stamp_cost: Option<u8>,
    cancelled: impl FnMut() -> bool,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    let plan = build_sdk_send_plan(envelope, source_hash, stamp_cost);
    validate_sdk_send_plan_ttl(&plan)?;
    let params = embedded_sdk_send_params(plan.send_request, plan.rpc_delivery.clone(), 1);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let record = rns_rpc::MessageRecord {
        id: params
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("embedded-sdk-1")
            .to_string(),
        source: params
            .get("source")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(source_hash)
            .to_string(),
        destination: params
            .get("destination")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(envelope.peer_hash.as_str())
            .to_string(),
        title: params
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        content: params
            .get("content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        timestamp: i64::try_from(now).unwrap_or(i64::MAX),
        direction: "out".into(),
        fields: params.get("fields").cloned(),
        receipt_status: None,
    };
    let reply_ticket_hex = match envelope.native_reply_ticket.as_ref() {
        Some(ticket) => {
            validate_outbound_reply_ticket(ticket)?;
            Some(hex_bytes(&ticket.ticket))
        }
        None => None,
    };
    let (fields, _) =
        crate::runtime::native_lxmf::codec::attachment_fields_from_paths(&envelope.attachments)?;
    build_sdk_wire_delivery_with_policy_fields(
        &record,
        &plan.rpc_delivery,
        identity_bytes,
        reply_ticket_hex.as_deref(),
        NativeLxmfWireFieldPolicy {
            issued_ticket,
            direct_stamp_cost,
            fields,
        },
        cancelled,
    )
}

#[cfg(feature = "native-lxmf-sdk")]
fn validate_outbound_reply_ticket(
    ticket: &crate::messaging::NativeLxmfReplyTicket,
) -> AppResult<()> {
    if ticket.ticket.len() != LXMF_TICKET_LENGTH {
        return Err(AppError::Runtime(format!(
            "LXMF reply ticket must be {LXMF_TICKET_LENGTH} bytes"
        )));
    }
    if !ticket.expires.is_finite() || ticket.expires <= current_unix_secs_f64() {
        return Err(AppError::Runtime(
            "LXMF reply ticket is expired or invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "native-lxmf-sdk")]
fn parse_lxmf_hash_hex(value: &str) -> AppResult<[u8; 16]> {
    let trimmed = value.trim();
    if trimmed.len() != 32 {
        return Err(AppError::Runtime(format!(
            "LXMF hash must be 32 hex characters, got {}",
            trimmed.len()
        )));
    }
    let mut out = [0u8; 16];
    for index in 0..16 {
        out[index] = u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16)
            .map_err(|_| AppError::Runtime("LXMF hash contains non-hex characters".into()))?;
    }
    Ok(out)
}

#[cfg(feature = "native-lxmf-sdk")]
fn parse_lxmf_ticket_hex(value: &str) -> AppResult<Vec<u8>> {
    let trimmed = value.trim();
    if trimmed.len() != LXMF_TICKET_LENGTH * 2 {
        return Err(AppError::Runtime(format!(
            "LXMF ticket must be {} hex characters",
            LXMF_TICKET_LENGTH * 2
        )));
    }
    let mut out = Vec::with_capacity(LXMF_TICKET_LENGTH);
    for index in 0..LXMF_TICKET_LENGTH {
        out.push(
            u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16)
                .map_err(|_| AppError::Runtime("LXMF ticket contains non-hex characters".into()))?,
        );
    }
    Ok(out)
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_record_fields_to_rmpv(fields: Option<&serde_json::Value>) -> AppResult<Option<rmpv::Value>> {
    let Some(fields) = fields else {
        return Ok(None);
    };
    let Some(map) = fields.as_object() else {
        return Ok(Some(sdk_json_to_rmpv(fields)?));
    };
    let entries = map
        .iter()
        .filter(|(key, _)| !key.starts_with('_'))
        .map(|(key, value)| Ok((sdk_json_key_to_rmpv(key.as_str()), sdk_json_to_rmpv(value)?)))
        .collect::<AppResult<Vec<_>>>()?;
    if entries.is_empty() {
        Ok(None)
    } else {
        Ok(Some(rmpv::Value::Map(entries)))
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_json_key_to_rmpv(key: &str) -> rmpv::Value {
    key.parse::<i64>()
        .map(|value| rmpv::Value::Integer(value.into()))
        .unwrap_or_else(|_| rmpv::Value::String(key.into()))
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_json_to_rmpv(value: &serde_json::Value) -> AppResult<rmpv::Value> {
    Ok(match value {
        serde_json::Value::Null => rmpv::Value::Nil,
        serde_json::Value::Bool(value) => rmpv::Value::Boolean(*value),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                rmpv::Value::Integer(value.into())
            } else if let Some(value) = value.as_u64() {
                rmpv::Value::Integer(value.into())
            } else if let Some(value) = value.as_f64() {
                rmpv::Value::F64(value)
            } else {
                return Err(AppError::Runtime(
                    "JSON number could not be converted to MessagePack".into(),
                ));
            }
        }
        serde_json::Value::String(value) => rmpv::Value::String(value.as_str().into()),
        serde_json::Value::Array(items) => rmpv::Value::Array(
            items
                .iter()
                .map(sdk_json_to_rmpv)
                .collect::<AppResult<Vec<_>>>()?,
        ),
        serde_json::Value::Object(map) => rmpv::Value::Map(
            map.iter()
                .filter(|(key, _)| !key.starts_with('_'))
                .map(|(key, value)| {
                    Ok((sdk_json_key_to_rmpv(key.as_str()), sdk_json_to_rmpv(value)?))
                })
                .collect::<AppResult<Vec<_>>>()?,
        ),
    })
}

#[cfg(feature = "native-lxmf-sdk")]
fn insert_lxmf_ticket_field(fields: &mut Option<rmpv::Value>) {
    let expires = current_unix_secs_f64() + LXMF_TICKET_EXPIRY_SECONDS;
    let mut ticket = vec![0u8; LXMF_TICKET_LENGTH];
    rand_core::OsRng.fill_bytes(&mut ticket);
    let value = rmpv::Value::Array(vec![rmpv::Value::F64(expires), rmpv::Value::Binary(ticket)]);
    insert_lxmf_ticket_value(fields, value);
}

#[cfg(feature = "native-lxmf-sdk")]
fn insert_issued_lxmf_ticket_field(
    fields: &mut Option<rmpv::Value>,
    ticket: &crate::messaging::NativeLxmfReplyTicket,
) -> AppResult<()> {
    if ticket.ticket.len() != LXMF_TICKET_LENGTH {
        return Err(AppError::Runtime(format!(
            "issued LXMF ticket must be {LXMF_TICKET_LENGTH} bytes"
        )));
    }
    if !ticket.expires.is_finite() || ticket.expires <= current_unix_secs_f64() {
        return Err(AppError::Runtime(
            "issued LXMF ticket expiry must be in the future".into(),
        ));
    }
    let value = rmpv::Value::Array(vec![
        rmpv::Value::F64(ticket.expires),
        rmpv::Value::Binary(ticket.ticket.clone()),
    ]);
    insert_lxmf_ticket_value(fields, value);
    Ok(())
}

#[cfg(feature = "native-lxmf-sdk")]
fn insert_lxmf_ticket_value(fields: &mut Option<rmpv::Value>, value: rmpv::Value) {
    match fields {
        Some(rmpv::Value::Map(entries)) => {
            if let Some((_, existing)) = entries.iter_mut().find(|(key, _)| {
                matches!(key, rmpv::Value::Integer(value) if value.as_i64() == Some(LXMF_TICKET_FIELD))
            }) {
                *existing = value;
            } else {
                entries.push((rmpv::Value::Integer(LXMF_TICKET_FIELD.into()), value));
            }
        }
        _ => {
            *fields = Some(rmpv::Value::Map(vec![(
                rmpv::Value::Integer(LXMF_TICKET_FIELD.into()),
                value,
            )]));
        }
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_message_id(message: &lxmf::Message) -> AppResult<[u8; 32]> {
    let destination = message
        .destination_hash
        .ok_or_else(|| AppError::Runtime("LXMF SDK bridge missing destination".into()))?;
    let source = message
        .source_hash
        .ok_or_else(|| AppError::Runtime("LXMF SDK bridge missing source".into()))?;
    let timestamp = message
        .timestamp
        .ok_or_else(|| AppError::Runtime("LXMF SDK bridge missing timestamp".into()))?;
    let payload = lxmf::Payload::new(
        timestamp,
        Some(message.content.clone()),
        Some(message.title.clone()),
        message.fields.clone(),
        None,
    );
    lxmf::WireMessage::new(destination, source, payload)
        .try_message_id()
        .map_err(|error| {
            AppError::Runtime(format!("LXMF SDK bridge message ID encode failed: {error}"))
        })
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_ticket_stamp(ticket: &[u8], message_id: &[u8; 32]) -> AppResult<[u8; 16]> {
    if ticket.len() != LXMF_TICKET_LENGTH {
        return Err(AppError::Runtime(format!(
            "LXMF ticket must be {LXMF_TICKET_LENGTH} bytes"
        )));
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(ticket);
    hasher.update(message_id);
    let digest = hasher.finalize();
    let mut stamp = [0u8; 16];
    stamp.copy_from_slice(&digest[..16]);
    Ok(stamp)
}

#[cfg(feature = "native-lxmf-sdk")]
fn current_unix_secs_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLxmfSdkRuntimeBoundaryKind {
    RpcSidecarClient,
    EmbeddedRpcDaemon,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkRuntimeBoundaryDecision {
    pub preferred: NativeLxmfSdkRuntimeBoundaryKind,
    pub sidecar_client_available: bool,
    pub embedded_daemon_available: bool,
    pub reason: &'static str,
    pub next_step: &'static str,
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn native_lxmf_sdk_runtime_boundary_decision() -> NativeLxmfSdkRuntimeBoundaryDecision {
    let _rpc_client = lxmf_sdk::RpcBackendClient::new("tcp://127.0.0.1:0/rpc");
    let _embedded_daemon_type = std::any::type_name::<rns_rpc::RpcDaemon>();
    let _outbound_bridge_type = std::any::type_name::<dyn rns_rpc::OutboundBridge>();

    NativeLxmfSdkRuntimeBoundaryDecision {
        preferred: NativeLxmfSdkRuntimeBoundaryKind::RpcSidecarClient,
        sidecar_client_available: true,
        embedded_daemon_available: true,
        reason: "the SDK RPC client keeps LXMF delivery outside the Iced UI process while the embedded daemon still needs a store and outbound Reticulum bridge",
        next_step: "wire a managed/local RPC endpoint behind NativeLxmfSdkSender, then compare direct, propagated, ticket, stamp, and attachment behavior against the legacy live path",
    }
}

#[cfg(feature = "native-lxmf-sdk")]
#[async_trait::async_trait]
pub trait NativeLxmfSdkSender: Send + Sync {
    fn status(&self) -> NativeLxmfSdkSenderStatus;

    async fn probe(&self) -> AppResult<NativeLxmfSdkProbe>;

    async fn send_plan(&self, plan: NativeLxmfSdkSendPlan) -> AppResult<NativeLxmfSdkSendReceipt>;

    async fn cancel_delivery(&self, message_id: &str) -> AppResult<LxmfCancelOutcome>;

    async fn history_page(&self, request: LxmfHistoryRequest) -> AppResult<LxmfHistoryPage>;
}

#[cfg(feature = "native-lxmf-sdk")]
fn map_sdk_history_page(page: lxmf_sdk::MessageHistoryPage) -> AppResult<LxmfHistoryPage> {
    LxmfHistoryPage {
        messages: page
            .messages
            .into_iter()
            .map(|record| LxmfHistoryRecord {
                message_id: record.id,
                source: record.source,
                destination: record.destination,
                title: record.title,
                content: record.content,
                timestamp: record.timestamp,
                direction: record.direction,
                receipt_status: record.receipt_status,
            })
            .collect(),
        next_cursor: page.next_cursor,
    }
    .validate()
}

#[cfg(feature = "native-lxmf-sdk")]
fn sdk_history_request(request: LxmfHistoryRequest) -> lxmf_sdk::MessageHistoryListRequest {
    lxmf_sdk::MessageHistoryListRequest {
        peer_id: request.peer_hash,
        conversation_id: None,
        include_receipts: Some(true),
        limit: Some(request.limit),
        before_ts: None,
        cursor: request.cursor,
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn map_sdk_cancel_result(result: lxmf_sdk::CancelResult) -> LxmfCancelOutcome {
    match result {
        lxmf_sdk::CancelResult::Accepted => LxmfCancelOutcome::Accepted,
        lxmf_sdk::CancelResult::AlreadyTerminal => LxmfCancelOutcome::AlreadyTerminal,
        lxmf_sdk::CancelResult::NotFound => LxmfCancelOutcome::NotFound,
        lxmf_sdk::CancelResult::TooLateToCancel => LxmfCancelOutcome::TooLateToCancel,
        lxmf_sdk::CancelResult::Unsupported => LxmfCancelOutcome::Unsupported,
        _ => LxmfCancelOutcome::Unsupported,
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn parse_sdk_cancel_outcome(value: &str) -> AppResult<LxmfCancelOutcome> {
    match value {
        "Accepted" => Ok(LxmfCancelOutcome::Accepted),
        "AlreadyTerminal" => Ok(LxmfCancelOutcome::AlreadyTerminal),
        "NotFound" => Ok(LxmfCancelOutcome::NotFound),
        "TooLateToCancel" => Ok(LxmfCancelOutcome::TooLateToCancel),
        "Unsupported" => Ok(LxmfCancelOutcome::Unsupported),
        _ => Err(AppError::Runtime(
            "native LXMF SDK cancellation returned an unknown outcome".into(),
        )),
    }
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, Default)]
pub struct MissingNativeLxmfSdkSender;

#[cfg(feature = "native-lxmf-sdk")]
#[async_trait::async_trait]
impl NativeLxmfSdkSender for MissingNativeLxmfSdkSender {
    fn status(&self) -> NativeLxmfSdkSenderStatus {
        NativeLxmfSdkSenderStatus {
            name: "missing-native-lxmf-sdk-sender",
            state: NativeLxmfSdkSenderState::NotWired,
            note: "lxmf-sdk/reticulum-rs-rpc send boundary is defined, but no live SDK/RPC sender is configured",
        }
    }

    async fn send_plan(&self, plan: NativeLxmfSdkSendPlan) -> AppResult<NativeLxmfSdkSendReceipt> {
        let _ = plan;
        Err(AppError::Unsupported(
            "native LXMF SDK/RPC sender is not wired; configure a local lxmf-sdk/reticulum-rs-rpc endpoint or use mock mode".into(),
        ))
    }

    async fn cancel_delivery(&self, _message_id: &str) -> AppResult<LxmfCancelOutcome> {
        Ok(LxmfCancelOutcome::Unsupported)
    }

    async fn history_page(&self, _request: LxmfHistoryRequest) -> AppResult<LxmfHistoryPage> {
        Err(AppError::Unsupported(
            "native LXMF SDK history is not wired".into(),
        ))
    }

    async fn probe(&self) -> AppResult<NativeLxmfSdkProbe> {
        Err(AppError::Unsupported(
            "native LXMF SDK/RPC sender is not wired; no endpoint can be probed".into(),
        ))
    }
}

#[cfg(feature = "native-lxmf-sdk")]
#[async_trait::async_trait]
impl NativeLxmfSdkSender for RpcNativeLxmfSdkSender {
    fn status(&self) -> NativeLxmfSdkSenderStatus {
        let endpoint = self.endpoint.trim();
        if endpoint.is_empty() {
            return NativeLxmfSdkSenderStatus {
                name: "rpc-native-lxmf-sdk-sender",
                state: NativeLxmfSdkSenderState::MissingEndpoint,
                note:
                    "lxmf-sdk RPC sender needs a configured local endpoint before it can dispatch",
            };
        }
        match validate_local_rpc_endpoint(endpoint) {
            Ok(_) => NativeLxmfSdkSenderStatus {
                name: "rpc-native-lxmf-sdk-sender",
                state: NativeLxmfSdkSenderState::Configured,
                note: "lxmf-sdk RPC sender has a validated local endpoint; readiness requires a successful capability probe",
            },
            Err(_) => NativeLxmfSdkSenderStatus {
                name: "rpc-native-lxmf-sdk-sender",
                state: NativeLxmfSdkSenderState::RejectedEndpoint,
                note: "lxmf-sdk RPC endpoint was rejected before connection because it is not an approved local-trusted transport",
            },
        }
    }

    async fn probe(&self) -> AppResult<NativeLxmfSdkProbe> {
        let validated = validate_local_rpc_endpoint(self.endpoint.as_str())?;

        let endpoint = self.endpoint.clone();
        let diagnostic_endpoint = validated.diagnostic_label;
        let snapshot = tokio::task::spawn_blocking({
            let endpoint = endpoint.clone();
            move || {
                use lxmf_sdk::SdkBackend;

                let client = lxmf_sdk::RpcBackendClient::new(endpoint);
                client.snapshot().map_err(|error| error.to_string())
            }
        })
        .await
        .map_err(|err| AppError::Runtime(format!("native LXMF SDK probe task failed: {err}")))?
        .map_err(|err| AppError::Runtime(format!("native LXMF SDK probe failed: {err}")))?;

        Ok(NativeLxmfSdkProbe {
            endpoint: diagnostic_endpoint,
            runtime_id: snapshot.runtime_id,
            state: format!("{:?}", snapshot.state),
            active_contract_version: snapshot.active_contract_version,
            event_stream_position: snapshot.event_stream_position,
            config_revision: snapshot.config_revision,
            queued_messages: snapshot.queued_messages,
            in_flight_messages: snapshot.in_flight_messages,
        })
    }

    async fn send_plan(&self, plan: NativeLxmfSdkSendPlan) -> AppResult<NativeLxmfSdkSendReceipt> {
        validate_local_rpc_endpoint(self.endpoint.as_str())?;
        validate_sdk_send_plan_ttl(&plan)?;
        validate_external_rpc_delivery_options(&plan)?;

        let endpoint = self.endpoint.clone();
        let send_request = plan.send_request;
        let message_id = tokio::task::spawn_blocking(move || {
            use lxmf_sdk::SdkBackend;

            let client = lxmf_sdk::RpcBackendClient::new(endpoint);
            client.send(send_request).map_err(|error| error.to_string())
        })
        .await
        .map_err(|err| AppError::Runtime(format!("native LXMF SDK sender task failed: {err}")))?
        .map_err(|err| AppError::Runtime(format!("native LXMF SDK sender failed: {err}")))?;

        Ok(NativeLxmfSdkSendReceipt {
            message_id: Some(message_id.0),
            accepted: true,
            state: "submitted_to_sdk_rpc".into(),
        })
    }

    async fn cancel_delivery(&self, message_id: &str) -> AppResult<LxmfCancelOutcome> {
        validate_local_rpc_endpoint(self.endpoint.as_str())?;
        let endpoint = self.endpoint.clone();
        let message_id = message_id.to_owned();
        tokio::task::spawn_blocking(move || {
            use lxmf_sdk::SdkBackend;

            let client = lxmf_sdk::RpcBackendClient::new(endpoint);
            client
                .cancel(lxmf_sdk::MessageId(message_id))
                .map(map_sdk_cancel_result)
                .map_err(|error| format!("{error:?}"))
        })
        .await
        .map_err(|error| {
            AppError::Runtime(format!("native LXMF SDK cancellation task failed: {error}"))
        })?
        .map_err(|error| AppError::Runtime(format!("native LXMF SDK cancellation failed: {error}")))
    }

    async fn history_page(&self, request: LxmfHistoryRequest) -> AppResult<LxmfHistoryPage> {
        validate_local_rpc_endpoint(self.endpoint.as_str())?;
        let request =
            LxmfHistoryRequest::bounded(request.peer_hash, request.cursor, request.limit)?;
        let endpoint = self.endpoint.clone();
        tokio::task::spawn_blocking(move || {
            let client = lxmf_sdk::app::Client::rpc(endpoint);
            client
                .messages()
                .history(sdk_history_request(request))
                .map_err(|error| format!("{error:?}"))
        })
        .await
        .map_err(|error| {
            AppError::Runtime(format!("native LXMF SDK history task failed: {error}"))
        })?
        .map_err(|error| AppError::Runtime(format!("native LXMF SDK history failed: {error}")))
        .and_then(map_sdk_history_page)
    }
}

#[cfg(feature = "native-lxmf-sdk")]
#[async_trait::async_trait]
impl NativeLxmfSdkSender for EmbeddedNativeLxmfSdkSender {
    fn status(&self) -> NativeLxmfSdkSenderStatus {
        NativeLxmfSdkSenderStatus {
            name: "embedded-native-lxmf-sdk-sender",
            state: NativeLxmfSdkSenderState::Ready,
            note: "embedded reticulum-rs-rpc daemon is available; live delivery depends on the configured outbound bridge",
        }
    }

    async fn probe(&self) -> AppResult<NativeLxmfSdkProbe> {
        let daemon = Arc::clone(&self.daemon);
        let request_id = self.next_request_id();
        let response = tokio::task::spawn_blocking(move || {
            daemon.handle_rpc(rns_rpc::RpcRequest {
                id: request_id,
                method: "sdk_snapshot_v2".into(),
                params: None,
            })
        })
        .await
        .map_err(|err| {
            AppError::Runtime(format!("embedded native LXMF SDK probe task failed: {err}"))
        })?
        .map_err(|err| {
            AppError::Runtime(format!("embedded native LXMF SDK probe failed: {err}"))
        })?;

        if let Some(error) = response.error {
            return Err(AppError::Runtime(format!(
                "embedded native LXMF SDK probe failed: {}: {}",
                error.code, error.message
            )));
        }

        let result = response.result.unwrap_or_else(|| serde_json::json!({}));
        Ok(NativeLxmfSdkProbe {
            endpoint: "embedded".into(),
            runtime_id: result
                .get("runtime_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("embedded")
                .to_string(),
            state: result
                .get("state")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Ready")
                .to_string(),
            active_contract_version: result
                .get("active_contract_version")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
                .unwrap_or(0),
            event_stream_position: result
                .get("event_stream_position")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            config_revision: result
                .get("config_revision")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            queued_messages: result
                .get("queued_messages")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            in_flight_messages: result
                .get("in_flight_messages")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        })
    }

    async fn send_plan(&self, plan: NativeLxmfSdkSendPlan) -> AppResult<NativeLxmfSdkSendReceipt> {
        validate_sdk_send_plan_ttl(&plan)?;
        let daemon = Arc::clone(&self.daemon);
        let request_id = self.next_request_id();
        let params = embedded_sdk_send_params(plan.send_request, plan.rpc_delivery, request_id);
        let response = tokio::task::spawn_blocking(move || {
            daemon.handle_rpc(rns_rpc::RpcRequest {
                id: request_id,
                method: "sdk_send_v2".into(),
                params: Some(params),
            })
        })
        .await
        .map_err(|err| {
            AppError::Runtime(format!(
                "embedded native LXMF SDK sender task failed: {err}"
            ))
        })?
        .map_err(|err| {
            AppError::Runtime(format!("embedded native LXMF SDK sender failed: {err}"))
        })?;

        if let Some(error) = response.error {
            return Err(AppError::Runtime(format!(
                "embedded native LXMF SDK sender failed: {}: {}",
                error.code, error.message
            )));
        }

        let message_id = response
            .result
            .as_ref()
            .and_then(|result| result.get("message_id"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);

        Ok(NativeLxmfSdkSendReceipt {
            message_id,
            accepted: true,
            state: "submitted_to_embedded_rpc".into(),
        })
    }

    async fn cancel_delivery(&self, message_id: &str) -> AppResult<LxmfCancelOutcome> {
        let daemon = Arc::clone(&self.daemon);
        let request_id = self.next_request_id();
        let message_id = message_id.to_owned();
        let response = tokio::task::spawn_blocking(move || {
            daemon.handle_rpc(rns_rpc::RpcRequest {
                id: request_id,
                method: "sdk_cancel_message_v2".into(),
                params: Some(serde_json::json!({ "message_id": message_id })),
            })
        })
        .await
        .map_err(|error| {
            AppError::Runtime(format!(
                "embedded native LXMF SDK cancellation task failed: {error}"
            ))
        })?
        .map_err(|error| {
            AppError::Runtime(format!(
                "embedded native LXMF SDK cancellation failed: {error}"
            ))
        })?;
        if let Some(error) = response.error {
            return Err(AppError::Runtime(format!(
                "embedded native LXMF SDK cancellation failed: {}: {}",
                error.code, error.message
            )));
        }
        let outcome = response
            .result
            .as_ref()
            .and_then(|result| result.get("result"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AppError::Runtime(
                    "embedded native LXMF SDK cancellation omitted its outcome".into(),
                )
            })?;
        parse_sdk_cancel_outcome(outcome)
    }

    async fn history_page(&self, request: LxmfHistoryRequest) -> AppResult<LxmfHistoryPage> {
        let request =
            LxmfHistoryRequest::bounded(request.peer_hash, request.cursor, request.limit)?;
        let daemon = Arc::clone(&self.daemon);
        let request_id = self.next_request_id();
        let params = serde_json::to_value(lxmf_sdk::app::Envelope::query(
            "app.message.history.list",
            serde_json::to_value(sdk_history_request(request))
                .map_err(|error| AppError::Runtime(error.to_string()))?,
        ))
        .map_err(|error| AppError::Runtime(error.to_string()))?;
        let response = tokio::task::spawn_blocking(move || {
            daemon.handle_rpc(rns_rpc::RpcRequest {
                id: request_id,
                method: "sdk_envelope_execute_v2".into(),
                params: Some(params),
            })
        })
        .await
        .map_err(|error| {
            AppError::Runtime(format!(
                "embedded native LXMF SDK history task failed: {error}"
            ))
        })?
        .map_err(|error| {
            AppError::Runtime(format!("embedded native LXMF SDK history failed: {error}"))
        })?;
        if let Some(error) = response.error {
            return Err(AppError::Runtime(format!(
                "embedded native LXMF SDK history failed: {}: {}",
                error.code, error.message
            )));
        }
        let payload = response
            .result
            .and_then(|result| result.get("response").cloned())
            .and_then(|response| response.get("payload").cloned())
            .ok_or_else(|| {
                AppError::Runtime(
                    "embedded native LXMF SDK history returned no response payload".into(),
                )
            })?;
        let page = serde_json::from_value::<lxmf_sdk::MessageHistoryPage>(payload)
            .map_err(|error| AppError::Runtime(format!("invalid SDK history page: {error}")))?;
        map_sdk_history_page(page)
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn embedded_sdk_send_params(
    request: lxmf_sdk::SendRequest,
    options: rns_rpc::OutboundDeliveryOptions,
    request_id: u64,
) -> serde_json::Value {
    let content = request
        .payload
        .get("content")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            request
                .payload
                .get("body")
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_owned)
        .unwrap_or_else(|| request.payload.to_string());
    let title = request
        .payload
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut fields = request.payload.get("fields").cloned();
    if let Some(ticket) = options.ticket.as_deref() {
        let mut root = fields
            .as_ref()
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut lxmf = root
            .get("_lxmf")
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        lxmf.insert(
            "ticket".into(),
            serde_json::Value::String(ticket.to_string()),
        );
        root.insert("_lxmf".into(), serde_json::Value::Object(lxmf));
        fields = Some(serde_json::Value::Object(root));
    }
    let method = options.method.or(request.delivery_method);
    let stamp_cost = options.stamp_cost.or(request.stamp_cost);
    let include_ticket = Some(options.include_ticket || request.include_ticket.unwrap_or(false));
    let try_propagation_on_fail =
        Some(options.try_propagation_on_fail || request.try_propagation_on_fail.unwrap_or(false));

    serde_json::json!({
        "id": format!("embedded-sdk-{request_id}"),
        "source": request.source,
        "destination": request.destination,
        "title": title,
        "content": content,
        "fields": fields,
        "method": method,
        "stamp_cost": stamp_cost,
        "include_ticket": include_ticket,
        "try_propagation_on_fail": try_propagation_on_fail,
        "ticket": options.ticket,
        "idempotency_key": request.idempotency_key,
        "ttl_ms": request.ttl_ms,
        "correlation_id": request.correlation_id,
    })
}

#[cfg(feature = "native-lxmf-sdk")]
fn validate_sdk_send_plan_ttl(plan: &NativeLxmfSdkSendPlan) -> AppResult<()> {
    if plan.send_request.ttl_ms == Some(0) {
        return Err(AppError::Runtime(
            "LXMF send deadline expired before SDK dispatch".into(),
        ));
    }
    Ok(())
}

#[cfg(feature = "native-lxmf-sdk")]
fn validate_external_rpc_delivery_options(plan: &NativeLxmfSdkSendPlan) -> AppResult<()> {
    let mut missing_guarantees = Vec::with_capacity(5);
    if plan.send_request.ttl_ms.is_some() {
        missing_guarantees.push("TTL");
    }
    if plan.send_request.idempotency_key.is_some() {
        missing_guarantees.push("idempotency key");
    }
    if plan.send_request.correlation_id.is_some() {
        missing_guarantees.push("correlation identifier");
    }
    if !plan.send_request.extensions.is_empty() {
        missing_guarantees.push("extensions");
    }
    if plan.rpc_delivery.ticket.is_some() {
        missing_guarantees.push("explicit reply ticket");
    }
    if !missing_guarantees.is_empty() {
        return Err(AppError::Unsupported(format!(
            "external LXMF SDK/RPC 0.9.9 cannot preserve required send guarantees: {}",
            missing_guarantees.join(", ")
        )));
    }
    Ok(())
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn build_sdk_send_plan(
    envelope: &crate::messaging::MessageEnvelope,
    source_hash: &str,
    stamp_cost: Option<u32>,
) -> NativeLxmfSdkSendPlan {
    let delivery_method = match envelope.delivery_mode {
        crate::messaging::DeliveryMode::Direct => "direct",
        crate::messaging::DeliveryMode::Propagated => "propagated",
    };
    let try_propagation_on_fail = matches!(
        envelope.delivery_mode,
        crate::messaging::DeliveryMode::Direct
    ) && envelope.operation.as_ref().is_some_and(|operation| {
        operation.allow_propagation_fallback && operation.automatic_propagation_fallback
    });
    let reply_ticket_hex = envelope
        .native_reply_ticket
        .as_ref()
        .map(|ticket| hex_bytes(&ticket.ticket));
    let payload = serde_json::json!({
        "title": envelope.title,
        "content": envelope.body,
        "body": envelope.body,
        "attachments": envelope.attachments.iter().map(|path| {
            serde_json::json!({
                "path": path.to_string_lossy(),
                "name": path.file_name().and_then(|name| name.to_str()).unwrap_or_default(),
            })
        }).collect::<Vec<_>>(),
    });
    let mut send_request =
        lxmf_sdk::SendRequest::new(source_hash, envelope.peer_hash.as_str(), payload)
            .with_delivery_method(delivery_method)
            .with_include_ticket(envelope.include_ticket)
            .with_try_propagation_on_fail(try_propagation_on_fail);
    if let Some(operation) = envelope.operation.as_ref() {
        send_request = send_request
            .with_idempotency_key(operation.idempotency_key.clone())
            .with_correlation_id(operation.correlation_id.clone())
            .with_ttl_ms(operation.remaining_ttl_ms().unwrap_or(0));
    }
    if let Some(stamp_cost) = stamp_cost {
        send_request = send_request.with_stamp_cost(stamp_cost);
    }

    NativeLxmfSdkSendPlan {
        send_request,
        rpc_delivery: rns_rpc::OutboundDeliveryOptions {
            method: Some(delivery_method.into()),
            stamp_cost,
            include_ticket: envelope.include_ticket,
            try_propagation_on_fail,
            ticket: reply_ticket_hex,
            source_private_key: None,
        },
    }
}

#[cfg(feature = "native-lxmf-sdk")]
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(all(test, feature = "native-lxmf-sdk"))]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc as std_mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use crate::identity::IdentityMaterialProvider;
    use crate::messaging::{
        DeliveryMode, MessageEnvelope, NativeLxmfReplyTicket, OutboundOperationIdentity,
    };
    use crate::runtime::native::identity::{
        load_private_identity_bytes, NativeReticulumIdentityProvider,
    };
    use crate::runtime::{LxmfCancelOutcome, LxmfHistoryRequest};
    use rns_rpc::OutboundBridge;

    use super::{
        build_sdk_send_plan, build_sdk_wire_delivery,
        build_sdk_wire_delivery_from_envelope_with_issued_ticket,
        build_sdk_wire_delivery_with_issued_ticket, build_sdk_wire_delivery_with_policy,
        current_unix_secs_f64, hex_bytes, map_sdk_cancel_result, native_lxmf_sdk_record_ticket,
        native_lxmf_sdk_runtime_boundary_decision, validate_external_rpc_delivery_options,
        validate_sdk_send_plan_ttl, EmbeddedNativeLxmfSdkSender, MissingNativeLxmfSdkSender,
        NativeLxmfSdkOutboundBridge, NativeLxmfSdkRuntimeBoundaryKind, NativeLxmfSdkSendPlan,
        NativeLxmfSdkSender, NativeLxmfSdkSenderState, NativeLxmfSdkTicketCache,
        NativeLxmfSdkWireDelivery, NativeLxmfSdkWireSubmitter, RpcNativeLxmfSdkSender,
        NATIVE_LXMF_SDK_TICKET_CACHE_MAX_ITEMS, NATIVE_LXMF_SDK_TICKET_MAX_BYTES,
    };

    struct RecordedDelivery {
        record: rns_rpc::MessageRecord,
        options: rns_rpc::OutboundDeliveryOptions,
        ticket: Option<String>,
    }

    struct RecordingOutboundBridge {
        tx: Mutex<std_mpsc::Sender<RecordedDelivery>>,
        ticket_cache: NativeLxmfSdkTicketCache,
    }

    fn capture_rpc_requests(
        expected_requests: usize,
    ) -> (
        String,
        std_mpsc::Receiver<Vec<rns_rpc::RpcRequest>>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind isolated RPC capture");
        listener
            .set_nonblocking(true)
            .expect("set isolated RPC capture nonblocking");
        let endpoint = format!(
            "tcp://127.0.0.1:{}/rpc",
            listener.local_addr().expect("RPC capture address").port()
        );
        let (request_tx, request_rx) = std_mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut requests = Vec::with_capacity(expected_requests);
            while requests.len() < expected_requests {
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(accepted) => break accepted,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "timed out waiting for isolated RPC request"
                            );
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("isolated RPC accept failed: {error}"),
                    }
                };
                stream
                    .set_nonblocking(false)
                    .expect("set isolated RPC capture blocking");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("set RPC capture read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .expect("set RPC capture write timeout");
                let mut request_bytes = Vec::new();
                stream
                    .read_to_end(&mut request_bytes)
                    .expect("read isolated RPC request");
                let body_start = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|index| index + 4)
                    .expect("isolated RPC request headers");
                let request: rns_rpc::RpcRequest =
                    rns_rpc::rpc::codec::decode_frame(&request_bytes[body_start..])
                        .expect("decode isolated RPC request");
                let result = match request.method.as_str() {
                    "sdk_send_v2" => serde_json::json!({
                        "message_id": "daemon-message-id"
                    }),
                    "sdk_cancel_message_v2" => serde_json::json!({
                        "result": "Accepted"
                    }),
                    method => panic!("unexpected isolated RPC method {method}"),
                };
                let response = rns_rpc::RpcResponse {
                    id: request.id,
                    result: Some(result),
                    error: None,
                };
                let response_body =
                    rns_rpc::rpc::codec::encode_frame(&response).expect("encode RPC response");
                let response_headers = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/msgpack\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
                stream
                    .write_all(response_headers.as_bytes())
                    .and_then(|()| stream.write_all(&response_body))
                    .expect("write isolated RPC response");
                requests.push(request);
            }
            request_tx
                .send(requests)
                .expect("publish captured RPC requests");
        });
        (endpoint, request_rx, worker)
    }

    fn capture_topic_negotiation() -> (
        String,
        std_mpsc::Receiver<rns_rpc::RpcRequest>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind topic negotiation capture");
        let endpoint = format!(
            "tcp://127.0.0.1:{}/rpc",
            listener.local_addr().expect("topic capture address").port()
        );
        let (request_tx, request_rx) = std_mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept topic negotiation");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set topic capture read timeout");
            let mut request_bytes = Vec::new();
            let mut buffer = [0_u8; 4_096];
            let body_end = loop {
                let read = stream
                    .read(&mut buffer)
                    .expect("read topic negotiation request");
                assert!(read > 0, "topic negotiation request closed before its body");
                request_bytes.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|index| index + 4)
                else {
                    continue;
                };
                let headers = std::str::from_utf8(&request_bytes[..header_end])
                    .expect("topic negotiation HTTP headers");
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().expect("content length"))
                        })
                    })
                    .expect("topic negotiation content length");
                let body_end = header_end + content_length;
                if request_bytes.len() >= body_end {
                    break (header_end, body_end);
                }
            };
            let request: rns_rpc::RpcRequest =
                rns_rpc::rpc::codec::decode_frame(&request_bytes[body_end.0..body_end.1])
                    .expect("decode topic negotiation request");

            let mut capabilities =
                lxmf_sdk::required_capabilities(lxmf_sdk::Profile::DesktopLocalRuntime)
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>();
            for capability in [
                crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_TOPICS,
                crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_SUBSCRIPTIONS,
                crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_FANOUT,
                crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_CURSOR_REPLAY,
                crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_ASYNC_EVENTS,
            ] {
                if !capabilities.iter().any(|current| current == capability) {
                    capabilities.push(capability.to_owned());
                }
            }
            let response = rns_rpc::RpcResponse {
                id: request.id,
                result: Some(serde_json::json!({
                    "runtime_id": "topic-probe-runtime",
                    "active_contract_version": 2,
                    "effective_capabilities": capabilities,
                    "effective_limits": {
                        "max_poll_events": 64,
                        "max_event_bytes": 65536,
                        "max_batch_bytes": 262144,
                        "max_extension_keys": 16,
                        "idempotency_ttl_ms": 43200000
                    },
                    "contract_release": "0.9.9",
                    "schema_namespace": "lxmf-sdk-v2",
                    "sdk_version": "0.9.9",
                    "python_reference": lxmf_sdk::ParityReference::default()
                })),
                error: None,
            };
            let response_body =
                rns_rpc::rpc::codec::encode_frame(&response).expect("encode topic response");
            let response_headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/msgpack\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            );
            stream
                .write_all(response_headers.as_bytes())
                .and_then(|()| stream.write_all(&response_body))
                .expect("write topic negotiation response");
            request_tx.send(request).expect("publish topic request");
        });
        (endpoint, request_rx, worker)
    }

    #[derive(Default)]
    struct RecordingWireSubmitter {
        deliveries: Mutex<Vec<NativeLxmfSdkWireDelivery>>,
    }

    impl NativeLxmfSdkWireSubmitter for RecordingWireSubmitter {
        fn submit_wire(&self, delivery: &NativeLxmfSdkWireDelivery) -> std::io::Result<()> {
            self.deliveries
                .lock()
                .expect("recording wire submitter mutex")
                .push(delivery.clone());
            Ok(())
        }
    }

    impl rns_rpc::OutboundBridge for RecordingOutboundBridge {
        fn validate_delivery(
            &self,
            record: &rns_rpc::MessageRecord,
            _options: &rns_rpc::OutboundDeliveryOptions,
        ) -> Result<(), std::io::Error> {
            self.ticket_cache.capture_validate_record(record)
        }

        fn deliver(
            &self,
            record: &rns_rpc::MessageRecord,
            options: &rns_rpc::OutboundDeliveryOptions,
        ) -> Result<(), std::io::Error> {
            let ticket = self.ticket_cache.take_ticket(record.id.as_str());
            self.tx
                .lock()
                .expect("recording bridge mutex")
                .send(RecordedDelivery {
                    record: record.clone(),
                    options: options.clone(),
                    ticket,
                })
                .map_err(std::io::Error::other)
        }
    }

    #[test]
    fn sdk_send_plan_maps_direct_ticketed_message_to_rpc_options() {
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: "subject".into(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: Some(NativeLxmfReplyTicket {
                ticket: vec![0x10, 0x20, 0x30],
                expires: 10.0,
            }),
            operation: Some(
                OutboundOperationIdentity::validated("idem-fixed".into(), "corr-fixed".into())
                    .expect("valid operation"),
            ),
            attachments: vec![PathBuf::from("/tmp/report.txt")],
        };

        let mut plan = build_sdk_send_plan(&envelope, "source", Some(7));
        plan.send_request.extensions.insert(
            "omen_test_extension".into(),
            serde_json::json!({ "bounded": true }),
        );

        assert_eq!(plan.send_request.source, "source");
        assert_eq!(plan.send_request.destination, "peer");
        assert_eq!(plan.send_request.delivery_method.as_deref(), Some("direct"));
        assert_eq!(plan.send_request.stamp_cost, Some(7));
        assert_eq!(plan.send_request.include_ticket, Some(true));
        assert_eq!(plan.send_request.try_propagation_on_fail, Some(false));
        assert_eq!(
            plan.send_request.idempotency_key.as_deref(),
            Some("idem-fixed")
        );
        assert_eq!(
            plan.send_request.correlation_id.as_deref(),
            Some("corr-fixed")
        );
        assert!(matches!(
            plan.send_request.ttl_ms,
            Some(1..=crate::messaging::OUTBOUND_DEFAULT_TTL_MS)
        ));
        assert_eq!(plan.rpc_delivery.method.as_deref(), Some("direct"));
        assert_eq!(plan.rpc_delivery.stamp_cost, Some(7));
        assert!(plan.rpc_delivery.include_ticket);
        assert!(!plan.rpc_delivery.try_propagation_on_fail);
        assert_eq!(plan.rpc_delivery.ticket.as_deref(), Some("102030"));
        assert_eq!(plan.send_request.payload["title"], "subject");
        assert_eq!(plan.send_request.payload["body"], "body");
        assert_eq!(
            plan.send_request.payload["attachments"][0]["name"],
            "report.txt"
        );
        assert_eq!(plan.send_request.payload["content"], "body");
    }

    #[test]
    fn sdk_send_plan_enables_only_snapshotted_safe_direct_fallback() {
        let mut operation =
            OutboundOperationIdentity::validated("idem-automatic".into(), "corr-automatic".into())
                .expect("valid operation");
        operation.automatic_propagation_fallback = true;
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "automatic fallback".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: Some(operation.clone()),
            attachments: Vec::new(),
        };

        let plan = build_sdk_send_plan(&envelope, "source", None);
        assert_eq!(plan.send_request.try_propagation_on_fail, Some(true));
        assert!(plan.rpc_delivery.try_propagation_on_fail);
        assert_eq!(
            plan.send_request.idempotency_key.as_deref(),
            Some(operation.idempotency_key.as_str())
        );

        operation.allow_propagation_fallback = false;
        let strict = build_sdk_send_plan(
            &MessageEnvelope {
                operation: Some(operation),
                ..envelope
            },
            "source",
            None,
        );
        assert_eq!(strict.send_request.try_propagation_on_fail, Some(false));
        assert!(!strict.rpc_delivery.try_propagation_on_fail);
    }

    #[test]
    fn sdk_send_plan_maps_propagated_message_to_propagation_method() {
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "propagated".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };

        let plan = build_sdk_send_plan(&envelope, "source", None);

        assert_eq!(
            plan.send_request.delivery_method.as_deref(),
            Some("propagated")
        );
        assert_eq!(plan.send_request.stamp_cost, None);
        assert_eq!(plan.send_request.include_ticket, Some(false));
        assert_eq!(plan.send_request.try_propagation_on_fail, Some(false));
        assert_eq!(plan.rpc_delivery.method.as_deref(), Some("propagated"));
        assert_eq!(plan.rpc_delivery.stamp_cost, None);
        assert!(!plan.rpc_delivery.include_ticket);
        assert!(!plan.rpc_delivery.try_propagation_on_fail);
        assert_eq!(plan.rpc_delivery.ticket, None);
    }

    #[test]
    fn clean_sdk_wire_envelope_preserves_file_attachment_bytes() {
        let private = NativeReticulumIdentityProvider
            .create_identity_material("sdk-attachment-smoke")
            .expect("isolated identity");
        let signer =
            reticulum_rs::core::identity::PrivateIdentity::from_private_key_bytes(&private)
                .expect("isolated signer");
        let source = signer.address_hash().to_hex_string();
        let attachment_path = std::env::temp_dir().join(format!(
            "omen-lxmf-sdk-attachment-{}-{}.bin",
            std::process::id(),
            current_unix_secs_f64().to_bits()
        ));
        std::fs::write(&attachment_path, b"clean sdk attachment bytes")
            .expect("isolated attachment");
        let envelope = MessageEnvelope {
            peer_hash: "00112233445566778899aabbccddeeff".into(),
            title: "attachment smoke".into(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: vec![attachment_path.clone()],
        };

        let delivery = build_sdk_wire_delivery_from_envelope_with_issued_ticket(
            &envelope, &source, &private, None, None,
        )
        .expect("clean SDK wire attachment");
        let decoded = crate::runtime::native_lxmf::codec::decode_wire_message(&delivery.wire_bytes)
            .expect("decode clean SDK wire");

        assert_eq!(decoded.attachments.len(), 1);
        assert_eq!(
            decoded.attachments[0].name,
            attachment_path
                .file_name()
                .expect("attachment name")
                .to_string_lossy()
        );
        assert_eq!(decoded.attachments[0].size, 26);
        assert_eq!(decoded.attachments[0].path, None);
        std::fs::remove_file(attachment_path).expect("remove isolated attachment");
    }

    #[test]
    fn sdk_send_plan_rejects_a_deadline_that_expires_before_dispatch() {
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: Some(
                OutboundOperationIdentity::generate_at(1, 1_000)
                    .expect("bounded expired operation"),
            ),
            attachments: Vec::new(),
        };

        let error = validate_sdk_send_plan_ttl(&build_sdk_send_plan(&envelope, "source", None))
            .expect_err("expired SDK plan");

        assert!(error.to_string().contains("deadline expired"));
    }

    #[test]
    fn sdk_send_plan_covers_direct_and_propagated_ticket_matrix() {
        for (delivery_mode, expected_method, include_ticket) in [
            (DeliveryMode::Direct, "direct", false),
            (DeliveryMode::Direct, "direct", true),
            (DeliveryMode::Propagated, "propagated", false),
            (DeliveryMode::Propagated, "propagated", true),
        ] {
            let envelope = MessageEnvelope {
                peer_hash: "peer".into(),
                title: String::new(),
                body: "body".into(),
                delivery_mode,
                include_ticket,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            };

            let plan = build_sdk_send_plan(&envelope, "source", None);

            assert_eq!(
                plan.send_request.delivery_method.as_deref(),
                Some(expected_method)
            );
            assert_eq!(plan.rpc_delivery.method.as_deref(), Some(expected_method));
            assert_eq!(plan.send_request.include_ticket, Some(include_ticket));
            assert_eq!(plan.rpc_delivery.include_ticket, include_ticket);
            assert_eq!(plan.send_request.try_propagation_on_fail, Some(false));
            assert!(!plan.rpc_delivery.try_propagation_on_fail);
        }
    }

    #[tokio::test]
    async fn missing_sdk_sender_reports_explicit_unsupported_boundary() {
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let plan = build_sdk_send_plan(&envelope, "source", None);
        let sender = MissingNativeLxmfSdkSender;

        let status = sender.status();
        assert_eq!(status.state, NativeLxmfSdkSenderState::NotWired);
        assert!(status.note.contains("SDK/RPC"));

        let error = sender
            .send_plan(plan)
            .await
            .expect_err("missing sender must fail before pretending to deliver");
        assert!(format!("{error}").contains("SDK/RPC sender is not wired"));
    }

    #[test]
    fn sdk_runtime_boundary_prefers_sidecar_before_embedded_daemon() {
        let decision = native_lxmf_sdk_runtime_boundary_decision();

        assert_eq!(
            decision.preferred,
            NativeLxmfSdkRuntimeBoundaryKind::RpcSidecarClient
        );
        assert!(decision.sidecar_client_available);
        assert!(decision.embedded_daemon_available);
        assert!(decision.reason.contains("Iced UI process"));
        assert!(decision.next_step.contains("NativeLxmfSdkSender"));
    }

    #[test]
    fn embedded_rpc_daemon_is_available_but_requires_bridge_before_live_use() {
        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory RPC store");
        let daemon = rns_rpc::RpcDaemon::with_store(store, "test-identity".into());

        drop(daemon);
    }

    #[tokio::test]
    async fn embedded_rpc_probe_maps_v09_snapshot_cursor_and_revision() {
        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory RPC store");
        let sender = EmbeddedNativeLxmfSdkSender::new(rns_rpc::RpcDaemon::with_store(
            store,
            "test-identity".into(),
        ));

        let probe = sender.probe().await.expect("embedded RPC probe");

        assert_eq!(probe.endpoint, "embedded");
        assert_eq!(probe.active_contract_version, 2);
        assert_eq!(probe.event_stream_position, 0);
        assert_eq!(probe.config_revision, 0);
        assert_eq!(probe.queued_messages, 0);
        assert_eq!(probe.in_flight_messages, 0);
    }

    #[tokio::test]
    async fn embedded_sdk_history_is_typed_filtered_and_bounded() {
        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory RPC store");
        store
            .insert_message(&rns_rpc::MessageRecord {
                id: "history-a".into(),
                source: "peer-a".into(),
                destination: "local".into(),
                title: "Recovered".into(),
                content: "bounded history".into(),
                timestamp: 1_700_000_000,
                direction: "in".into(),
                fields: None,
                receipt_status: Some("received".into()),
            })
            .expect("insert history");
        store
            .insert_message(&rns_rpc::MessageRecord {
                id: "history-a-older".into(),
                source: "peer-a".into(),
                destination: "local".into(),
                title: "Recovered older".into(),
                content: "cursor continuation".into(),
                timestamp: 1_699_999_999,
                direction: "in".into(),
                fields: None,
                receipt_status: Some("received".into()),
            })
            .expect("insert older history");
        store
            .insert_message(&rns_rpc::MessageRecord {
                id: "history-other".into(),
                source: "peer-b".into(),
                destination: "local".into(),
                title: "Other".into(),
                content: "not selected".into(),
                timestamp: 1_700_000_001,
                direction: "in".into(),
                fields: None,
                receipt_status: Some("received".into()),
            })
            .expect("insert other history");
        let sender = EmbeddedNativeLxmfSdkSender::new(rns_rpc::RpcDaemon::with_store(
            store,
            "test-identity".into(),
        ));

        let page = sender
            .history_page(
                LxmfHistoryRequest::bounded(Some("peer-a".into()), None, 1)
                    .expect("bounded request"),
            )
            .await
            .expect("typed history");

        assert_eq!(page.messages.len(), 1);
        assert_eq!(page.messages[0].message_id, "history-a");
        assert_eq!(page.messages[0].receipt_status.as_deref(), Some("received"));
        let next = sender
            .history_page(
                LxmfHistoryRequest::bounded(Some("peer-a".into()), page.next_cursor, 1)
                    .expect("bounded cursor request"),
            )
            .await
            .expect("typed cursor history");
        assert_eq!(next.messages.len(), 1);
        assert_eq!(next.messages[0].message_id, "history-a-older");
    }

    #[tokio::test]
    async fn embedded_sdk_cancellation_preserves_typed_not_found_outcome() {
        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory RPC store");
        let sender = EmbeddedNativeLxmfSdkSender::new(rns_rpc::RpcDaemon::with_store(
            store,
            "test-identity".into(),
        ));

        let outcome = sender
            .cancel_delivery("missing-message")
            .await
            .expect("typed cancellation outcome");

        assert_eq!(outcome, LxmfCancelOutcome::NotFound);
    }

    #[test]
    fn sdk_cancellation_preserves_every_v09_typed_outcome() {
        for (upstream, expected) in [
            (
                lxmf_sdk::CancelResult::Accepted,
                LxmfCancelOutcome::Accepted,
            ),
            (
                lxmf_sdk::CancelResult::AlreadyTerminal,
                LxmfCancelOutcome::AlreadyTerminal,
            ),
            (
                lxmf_sdk::CancelResult::NotFound,
                LxmfCancelOutcome::NotFound,
            ),
            (
                lxmf_sdk::CancelResult::TooLateToCancel,
                LxmfCancelOutcome::TooLateToCancel,
            ),
            (
                lxmf_sdk::CancelResult::Unsupported,
                LxmfCancelOutcome::Unsupported,
            ),
        ] {
            assert_eq!(map_sdk_cancel_result(upstream), expected);
        }
    }

    #[tokio::test]
    async fn embedded_sdk_sender_routes_delivery_options_to_rpc_bridge() {
        let (tx, rx) = std_mpsc::channel();
        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory RPC store");
        let daemon = rns_rpc::RpcDaemon::with_store_and_bridges(
            store,
            "source".into(),
            Some(Arc::new(RecordingOutboundBridge {
                tx: Mutex::new(tx),
                ticket_cache: NativeLxmfSdkTicketCache::default(),
            })),
            None,
        );
        let sender = EmbeddedNativeLxmfSdkSender::new(daemon);

        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: "subject".into(),
            body: "hello over lxmf".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: true,
            native_reply_ticket: Some(NativeLxmfReplyTicket {
                ticket: vec![0xaa, 0xbb],
                expires: 99.0,
            }),
            operation: None,
            attachments: Vec::new(),
        };
        let receipt = sender
            .send_plan(build_sdk_send_plan(&envelope, "source", Some(12)))
            .await
            .expect("embedded SDK send should be accepted by daemon");
        assert!(receipt.accepted);
        assert_eq!(receipt.state, "submitted_to_embedded_rpc");

        let delivery = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("daemon should hand off delivery to outbound bridge");
        assert_eq!(delivery.record.source, "source");
        assert_eq!(delivery.record.destination, "peer");
        assert_eq!(delivery.record.title, "subject");
        assert_eq!(delivery.record.content, "hello over lxmf");
        assert_eq!(delivery.options.method.as_deref(), Some("propagated"));
        assert_eq!(delivery.options.stamp_cost, Some(12));
        assert!(delivery.options.include_ticket);
        assert!(!delivery.options.try_propagation_on_fail);
        assert_eq!(delivery.options.ticket, None);
        assert_eq!(delivery.ticket.as_deref(), Some("aabb"));
        assert!(delivery.record.fields.is_none());
    }

    #[test]
    fn sdk_ticket_cache_captures_private_lxmf_ticket_before_sanitized_delivery() {
        let cache = NativeLxmfSdkTicketCache::default();
        let record = rns_rpc::MessageRecord {
            id: "message-1".into(),
            source: "source".into(),
            destination: "peer".into(),
            title: String::new(),
            content: "body".into(),
            timestamp: 0,
            direction: "outbound".into(),
            fields: Some(serde_json::json!({
                "_lxmf": {
                    "ticket": "aabbccdd",
                }
            })),
            receipt_status: None,
        };

        assert_eq!(
            native_lxmf_sdk_record_ticket(&record).as_deref(),
            Some("aabbccdd")
        );
        cache
            .capture_validate_record(&record)
            .expect("capture ticket");

        let mut sanitized = record.clone();
        sanitized.fields = None;
        assert_eq!(native_lxmf_sdk_record_ticket(&sanitized), None);
        assert_eq!(cache.take_ticket("message-1").as_deref(), Some("aabbccdd"));
        assert_eq!(cache.take_ticket("message-1"), None);
        assert!(cache.is_empty());
    }

    #[test]
    fn sdk_ticket_cache_is_item_and_byte_bounded_and_rejects_oversize() {
        let cache = NativeLxmfSdkTicketCache::default();
        let ticket = "ab".repeat(NATIVE_LXMF_SDK_TICKET_MAX_BYTES / 2);
        let record = |index: usize, ticket: &str| rns_rpc::MessageRecord {
            id: format!("message-{index}"),
            source: "source".into(),
            destination: "peer".into(),
            title: String::new(),
            content: "body".into(),
            timestamp: 0,
            direction: "outbound".into(),
            fields: Some(serde_json::json!({"_lxmf": {"ticket": ticket}})),
            receipt_status: None,
        };
        for index in 0..=NATIVE_LXMF_SDK_TICKET_CACHE_MAX_ITEMS {
            cache
                .capture_validate_record(&record(index, &ticket))
                .expect("bounded capture");
        }
        assert_eq!(cache.take_ticket("message-0"), None);
        assert_eq!(
            cache
                .take_ticket(&format!("message-{NATIVE_LXMF_SDK_TICKET_CACHE_MAX_ITEMS}"))
                .as_deref(),
            Some(ticket.as_str())
        );

        let oversized = "x".repeat(NATIVE_LXMF_SDK_TICKET_MAX_BYTES + 1);
        let error = cache
            .capture_validate_record(&record(9_999, &oversized))
            .expect_err("oversized ticket");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(cache.take_ticket("message-9999"), None);
    }

    #[test]
    fn sdk_ticket_cache_recovers_poison_without_exposing_or_retaining_ticket_material() {
        let cache = NativeLxmfSdkTicketCache::default();
        let private_ticket = "private-ticket-material";
        let record = rns_rpc::MessageRecord {
            id: "message-before-poison".into(),
            source: "source".into(),
            destination: "peer".into(),
            title: String::new(),
            content: "body".into(),
            timestamp: 0,
            direction: "outbound".into(),
            fields: Some(serde_json::json!({"_lxmf": {"ticket": private_ticket}})),
            receipt_status: None,
        };
        cache
            .capture_validate_record(&record)
            .expect("capture ticket before poison");

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = cache.state.lock().expect("lock cache to poison it");
            panic!("test-only ticket cache poison");
        }));
        assert!(panic.is_err());

        assert_eq!(cache.take_ticket("message-before-poison"), None);
        assert!(cache.is_empty());

        let replacement = rns_rpc::MessageRecord {
            id: "message-after-poison".into(),
            fields: Some(serde_json::json!({"_lxmf": {"ticket": "replacement"}})),
            ..record
        };
        cache
            .capture_validate_record(&replacement)
            .expect("cache remains usable after poison recovery");
        assert_eq!(
            cache.take_ticket("message-after-poison").as_deref(),
            Some("replacement")
        );

        let recovery_text =
            "recovered poisoned auxiliary native LXMF SDK ticket cache; cached tickets were discarded";
        assert!(!recovery_text.contains(private_ticket));
        assert!(!recovery_text.contains("replacement"));
    }

    #[test]
    fn sdk_wire_delivery_signs_record_and_applies_ticket_metadata() {
        let provider = NativeReticulumIdentityProvider;
        let source_identity = provider
            .create_identity_material("sdk-wire-source")
            .expect("source identity");
        let destination_identity = provider
            .create_identity_material("sdk-wire-destination")
            .expect("destination identity");
        let source_hash = load_private_identity_bytes(&source_identity)
            .expect("source summary")
            .address_hash_hex;
        let destination_hash = load_private_identity_bytes(&destination_identity)
            .expect("destination summary")
            .address_hash_hex;
        let record = rns_rpc::MessageRecord {
            id: "message-1".into(),
            source: source_hash.clone(),
            destination: destination_hash.clone(),
            title: "subject".into(),
            content: "body".into(),
            timestamp: 1_700_000_000,
            direction: "outbound".into(),
            fields: Some(serde_json::json!({
                "_lxmf": {
                    "ticket": "00112233445566778899aabbccddeeff",
                },
                "15": 1,
            })),
            receipt_status: None,
        };
        let options = rns_rpc::OutboundDeliveryOptions {
            method: Some("direct".into()),
            stamp_cost: None,
            include_ticket: true,
            try_propagation_on_fail: false,
            ticket: None,
            source_private_key: None,
        };

        let delivery = build_sdk_wire_delivery(
            &record,
            &options,
            source_identity.as_slice(),
            Some("00112233445566778899aabbccddeeff"),
        )
        .expect("wire delivery");

        assert_eq!(delivery.method.as_deref(), Some("direct"));
        assert!(delivery.include_ticket);
        assert!(delivery.reply_ticket_used);
        assert_eq!(delivery.message_id.len(), 64);

        let wire = lxmf::WireMessage::unpack(delivery.wire_bytes.as_slice()).expect("wire");
        let message = lxmf::Message::from_wire(delivery.wire_bytes.as_slice()).expect("message");
        assert_eq!(hex_bytes(&wire.source), source_hash);
        assert_eq!(hex_bytes(&wire.destination), destination_hash);
        assert_eq!(message.title_as_string().as_deref(), Some("subject"));
        assert_eq!(message.content_as_string().as_deref(), Some("body"));
        assert!(message.stamp_bytes().is_some());

        let rmpv::Value::Map(fields) = message.fields.expect("fields") else {
            panic!("fields should be a map");
        };
        assert!(fields.iter().any(|(key, _)| {
            matches!(key, rmpv::Value::Integer(value) if value.as_i64() == Some(0x0C))
        }));
        assert!(fields.iter().any(|(key, _)| {
            matches!(key, rmpv::Value::Integer(value) if value.as_i64() == Some(0x0F))
        }));
        assert!(!fields.iter().any(|(key, _)| {
            matches!(key, rmpv::Value::String(value) if value.as_str() == Some("_lxmf"))
        }));
    }

    #[test]
    fn sdk_wire_delivery_uses_validated_issuer_ticket_exactly() {
        let provider = NativeReticulumIdentityProvider;
        let source_identity = provider
            .create_identity_material("sdk-issued-ticket-source")
            .expect("source identity");
        let destination_identity = provider
            .create_identity_material("sdk-issued-ticket-destination")
            .expect("destination identity");
        let source_hash = load_private_identity_bytes(&source_identity)
            .expect("source summary")
            .address_hash_hex;
        let destination_hash = load_private_identity_bytes(&destination_identity)
            .expect("destination summary")
            .address_hash_hex;
        let record = rns_rpc::MessageRecord {
            id: "issued-ticket-message".into(),
            source: source_hash,
            destination: destination_hash,
            title: "subject".into(),
            content: "body".into(),
            timestamp: 1_700_000_000,
            direction: "outbound".into(),
            fields: None,
            receipt_status: None,
        };
        let options = rns_rpc::OutboundDeliveryOptions {
            method: Some("direct".into()),
            stamp_cost: None,
            include_ticket: true,
            try_propagation_on_fail: false,
            ticket: None,
            source_private_key: None,
        };
        let issued = NativeLxmfReplyTicket {
            ticket: (0_u8..16).collect(),
            expires: current_unix_secs_f64() + 3_600.0,
        };

        let delivery = build_sdk_wire_delivery_with_issued_ticket(
            &record,
            &options,
            source_identity.as_slice(),
            None,
            Some(&issued),
        )
        .expect("issued ticket wire delivery");
        let message = lxmf::Message::from_wire(delivery.wire_bytes.as_slice()).expect("message");
        let rmpv::Value::Map(fields) = message.fields.expect("fields") else {
            panic!("fields should be a map");
        };
        let (_, rmpv::Value::Array(ticket)) = fields
            .iter()
            .find(|(key, _)| {
                matches!(key, rmpv::Value::Integer(value) if value.as_i64() == Some(0x0C))
            })
            .expect("ticket field")
        else {
            panic!("ticket field should be an array");
        };
        assert_eq!(
            ticket.first().and_then(rmpv::Value::as_f64),
            Some(issued.expires)
        );
        assert_eq!(
            ticket.get(1),
            Some(&rmpv::Value::Binary(issued.ticket.clone()))
        );

        let expired = NativeLxmfReplyTicket {
            ticket: vec![0; 16],
            expires: current_unix_secs_f64() - 1.0,
        };
        assert!(build_sdk_wire_delivery_with_issued_ticket(
            &record,
            &options,
            source_identity.as_slice(),
            None,
            Some(&expired),
        )
        .is_err());
        let wrong_size = NativeLxmfReplyTicket {
            ticket: vec![0; 15],
            expires: current_unix_secs_f64() + 3_600.0,
        };
        assert!(build_sdk_wire_delivery_with_issued_ticket(
            &record,
            &options,
            source_identity.as_slice(),
            None,
            Some(&wrong_size),
        )
        .is_err());
    }

    #[test]
    fn sdk_wire_delivery_applies_bounded_direct_stamp_with_ticket_precedence() {
        let provider = NativeReticulumIdentityProvider;
        let source_identity = provider
            .create_identity_material("sdk-direct-stamp-source")
            .expect("source identity");
        let destination_identity = provider
            .create_identity_material("sdk-direct-stamp-destination")
            .expect("destination identity");
        let source_hash = load_private_identity_bytes(&source_identity)
            .expect("source summary")
            .address_hash_hex;
        let destination_hash = load_private_identity_bytes(&destination_identity)
            .expect("destination summary")
            .address_hash_hex;
        let record = rns_rpc::MessageRecord {
            id: "direct-stamp-message".into(),
            source: source_hash,
            destination: destination_hash,
            title: "subject".into(),
            content: "body".into(),
            timestamp: 1_700_000_000,
            direction: "outbound".into(),
            fields: None,
            receipt_status: None,
        };
        let options = rns_rpc::OutboundDeliveryOptions {
            method: Some("direct".into()),
            stamp_cost: Some(1),
            include_ticket: false,
            try_propagation_on_fail: false,
            ticket: None,
            source_private_key: None,
        };

        let stamped = build_sdk_wire_delivery_with_policy(
            &record,
            &options,
            source_identity.as_slice(),
            None,
            None,
            Some(1),
            || false,
        )
        .expect("direct stamp delivery");
        let metadata = stamped
            .direct_stamp
            .as_ref()
            .expect("direct stamp metadata");
        let wire = lxmf::WireMessage::unpack(stamped.wire_bytes.as_slice()).expect("wire");
        let message = lxmf::Message::from_wire(stamped.wire_bytes.as_slice()).expect("message");
        assert_eq!(metadata.target_cost, 1);
        assert_eq!(
            crate::runtime::native_lxmf::codec::validate_direct_stamp(
                &wire.message_id(),
                &message.stamp_bytes().expect("stamp"),
                metadata.target_cost,
            ),
            Some(metadata.stamp_value)
        );

        let ticketed = build_sdk_wire_delivery_with_policy(
            &record,
            &options,
            source_identity.as_slice(),
            Some("00112233445566778899aabbccddeeff"),
            None,
            Some(1),
            || false,
        )
        .expect("ticket precedence delivery");
        assert!(ticketed.reply_ticket_used);
        assert!(ticketed.direct_stamp.is_none());

        let cancelled = build_sdk_wire_delivery_with_policy(
            &record,
            &options,
            source_identity.as_slice(),
            None,
            None,
            Some(1),
            || true,
        )
        .expect_err("cancelled direct stamp");
        assert!(cancelled.to_string().contains("cancelled before work"));
    }

    #[test]
    fn sdk_outbound_bridge_encodes_and_submits_signed_wire_delivery() {
        let provider = NativeReticulumIdentityProvider;
        let source_identity = provider
            .create_identity_material("sdk-bridge-source")
            .expect("source identity");
        let destination_identity = provider
            .create_identity_material("sdk-bridge-destination")
            .expect("destination identity");
        let source_hash = load_private_identity_bytes(&source_identity)
            .expect("source summary")
            .address_hash_hex;
        let destination_hash = load_private_identity_bytes(&destination_identity)
            .expect("destination summary")
            .address_hash_hex;
        let submitter = Arc::new(RecordingWireSubmitter::default());
        let bridge = NativeLxmfSdkOutboundBridge::new(source_identity, submitter.clone());
        let record = rns_rpc::MessageRecord {
            id: "message-bridge".into(),
            source: source_hash,
            destination: destination_hash.clone(),
            title: String::new(),
            content: "bridge body".into(),
            timestamp: 1_700_000_001,
            direction: "outbound".into(),
            fields: Some(serde_json::json!({
                "_lxmf": {
                    "ticket": "00112233445566778899aabbccddeeff",
                }
            })),
            receipt_status: None,
        };
        let options = rns_rpc::OutboundDeliveryOptions {
            method: Some("propagated".into()),
            stamp_cost: Some(8),
            include_ticket: false,
            try_propagation_on_fail: true,
            ticket: None,
            source_private_key: None,
        };

        bridge
            .validate_delivery(&record, &options)
            .expect("validate delivery");
        bridge.deliver(&record, &options).expect("deliver");

        let deliveries = submitter
            .deliveries
            .lock()
            .expect("recording wire submitter mutex");
        assert_eq!(deliveries.len(), 1);
        let delivery = &deliveries[0];
        assert_eq!(delivery.destination_hash, destination_hash);
        assert_eq!(delivery.method.as_deref(), Some("propagated"));
        assert!(delivery.try_propagation_on_fail);
        assert!(delivery.reply_ticket_used);
        assert!(!delivery.include_ticket);
        let message = lxmf::Message::from_wire(delivery.wire_bytes.as_slice()).expect("message");
        assert_eq!(message.content_as_string().as_deref(), Some("bridge body"));
        assert!(message.stamp_bytes().is_some());
    }

    #[tokio::test]
    async fn embedded_sdk_sender_covers_direct_and_propagated_ticket_matrix() {
        let (tx, rx) = std_mpsc::channel();
        let store = rns_rpc::MessagesStore::in_memory().expect("in-memory RPC store");
        let daemon = rns_rpc::RpcDaemon::with_store_and_bridges(
            store,
            "source".into(),
            Some(Arc::new(RecordingOutboundBridge {
                tx: Mutex::new(tx),
                ticket_cache: NativeLxmfSdkTicketCache::default(),
            })),
            None,
        );
        let sender = EmbeddedNativeLxmfSdkSender::new(daemon);

        for (delivery_mode, expected_method, include_ticket) in [
            (DeliveryMode::Direct, "direct", false),
            (DeliveryMode::Direct, "direct", true),
            (DeliveryMode::Propagated, "propagated", false),
            (DeliveryMode::Propagated, "propagated", true),
        ] {
            let envelope = MessageEnvelope {
                peer_hash: "peer".into(),
                title: String::new(),
                body: "matrix body".into(),
                delivery_mode,
                include_ticket,
                native_reply_ticket: None,
                operation: None,
                attachments: Vec::new(),
            };
            sender
                .send_plan(build_sdk_send_plan(&envelope, "source", None))
                .await
                .expect("embedded SDK send should enqueue");

            let delivery = rx
                .recv_timeout(Duration::from_secs(2))
                .expect("daemon should hand off each delivery");
            assert_eq!(delivery.options.method.as_deref(), Some(expected_method));
            assert_eq!(delivery.options.include_ticket, include_ticket);
            assert!(!delivery.options.try_propagation_on_fail);
            assert_eq!(delivery.record.content, "matrix body");
        }
    }

    #[tokio::test]
    async fn upstream_rpc_099_send_capture_proves_preserved_and_dropped_fields() {
        let (endpoint, captured_rx, capture_worker) = capture_rpc_requests(2);
        let mut operation =
            OutboundOperationIdentity::validated("idem-external".into(), "corr-external".into())
                .expect("valid operation");
        operation.automatic_propagation_fallback = true;
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: "external title".into(),
            body: "external body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: true,
            native_reply_ticket: None,
            operation: Some(operation),
            attachments: Vec::new(),
        };
        let plan = build_sdk_send_plan(&envelope, "source", Some(7));
        assert!(matches!(plan.send_request.ttl_ms, Some(1..)));
        assert_eq!(plan.rpc_delivery.ticket, None);

        let endpoint_for_send = endpoint.clone();
        let send_request = plan.send_request;
        let message_id = tokio::task::spawn_blocking(move || {
            use lxmf_sdk::SdkBackend;

            lxmf_sdk::RpcBackendClient::new(endpoint_for_send)
                .send(send_request)
                .expect("upstream RPC capture send")
        })
        .await
        .expect("join upstream RPC capture send");
        assert_eq!(message_id.0, "daemon-message-id");
        tokio::task::spawn_blocking(move || {
            use lxmf_sdk::SdkBackend;

            lxmf_sdk::RpcBackendClient::new(endpoint)
                .cancel(lxmf_sdk::MessageId(message_id.0))
                .expect("upstream RPC capture cancellation")
        })
        .await
        .expect("join upstream RPC capture cancellation");

        let captured = captured_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("captured external RPC requests");
        capture_worker.join().expect("join RPC capture worker");
        assert_eq!(captured.len(), 2);

        let send = &captured[0];
        assert_eq!(send.method, "sdk_send_v2");
        let params = send.params.as_ref().expect("sdk_send_v2 params");
        assert_eq!(params["source"], "source");
        assert_eq!(params["destination"], "peer");
        assert_eq!(params["title"], "external title");
        assert_eq!(params["content"], "external body");
        assert_eq!(params["method"], "direct");
        assert_eq!(params["stamp_cost"], 7);
        assert_eq!(params["include_ticket"], true);
        assert_eq!(params["try_propagation_on_fail"], true);

        // The published lxmf-sdk 0.9.9 RpcBackendClient still drops
        // these SendRequest fields before constructing sdk_send_v2 params.
        for absent in [
            "idempotency_key",
            "ttl_ms",
            "correlation_id",
            "extensions",
            "ticket",
        ] {
            assert!(
                params.get(absent).is_none(),
                "upstream behavior changed: {absent} is now present"
            );
        }
        let cancel = &captured[1];
        assert_eq!(cancel.method, "sdk_cancel_message_v2");
        assert_eq!(
            cancel
                .params
                .as_ref()
                .and_then(|params| params.get("message_id"))
                .and_then(serde_json::Value::as_str),
            Some("daemon-message-id")
        );
    }

    #[test]
    fn external_rpc_099_rejects_each_lossy_send_guarantee() {
        let base = NativeLxmfSdkSendPlan {
            send_request: lxmf_sdk::SendRequest::new("source", "peer", serde_json::json!({})),
            rpc_delivery: rns_rpc::OutboundDeliveryOptions::default(),
        };
        let cases = [
            ("TTL", {
                let mut plan = base.clone();
                plan.send_request.ttl_ms = Some(1);
                plan
            }),
            ("idempotency key", {
                let mut plan = base.clone();
                plan.send_request.idempotency_key = Some("bounded-idempotency".into());
                plan
            }),
            ("correlation identifier", {
                let mut plan = base.clone();
                plan.send_request.correlation_id = Some("bounded-correlation".into());
                plan
            }),
            ("extensions", {
                let mut plan = base.clone();
                plan.send_request
                    .extensions
                    .insert("bounded".into(), serde_json::json!(true));
                plan
            }),
            ("explicit reply ticket", {
                let mut plan = base.clone();
                plan.rpc_delivery.ticket = Some("not-logged".into());
                plan
            }),
        ];

        for (guarantee, plan) in cases {
            let error = validate_external_rpc_delivery_options(&plan)
                .expect_err("lossy external guarantee must fail closed");
            let message = error.to_string();
            assert!(message.contains("external LXMF SDK/RPC 0.9.9"));
            assert!(message.contains(guarantee));
            assert!(!message.contains("not-logged"));
        }
    }

    #[tokio::test]
    async fn external_rpc_099_rejects_each_lossy_guarantee_before_connection() {
        let base = NativeLxmfSdkSendPlan {
            send_request: lxmf_sdk::SendRequest::new("source", "peer", serde_json::json!({})),
            rpc_delivery: rns_rpc::OutboundDeliveryOptions::default(),
        };
        let cases = [
            ("TTL", {
                let mut plan = base.clone();
                plan.send_request.ttl_ms = Some(1_000);
                plan
            }),
            ("idempotency key", {
                let mut plan = base.clone();
                plan.send_request.idempotency_key = Some("bounded-idempotency".into());
                plan
            }),
            ("correlation identifier", {
                let mut plan = base.clone();
                plan.send_request.correlation_id = Some("bounded-correlation".into());
                plan
            }),
            ("extensions", {
                let mut plan = base.clone();
                plan.send_request
                    .extensions
                    .insert("bounded".into(), serde_json::json!(true));
                plan
            }),
            ("explicit reply ticket", {
                let mut plan = base.clone();
                plan.rpc_delivery.ticket = Some("not-logged".into());
                plan
            }),
        ];

        for (guarantee, plan) in cases {
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind isolated per-guarantee RPC sentinel");
            listener
                .set_nonblocking(true)
                .expect("set per-guarantee RPC sentinel nonblocking");
            let endpoint = format!(
                "tcp://127.0.0.1:{}/rpc",
                listener.local_addr().expect("RPC sentinel address").port()
            );

            let error = RpcNativeLxmfSdkSender::new(endpoint)
                .send_plan(plan)
                .await
                .expect_err("lossy guarantee must be rejected before connection");
            assert!(error.to_string().contains(guarantee));
            assert!(matches!(
                listener.accept(),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
            ));
        }
    }

    #[tokio::test]
    async fn external_rpc_099_rejects_combined_guarantees_before_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind isolated RPC sentinel");
        listener
            .set_nonblocking(true)
            .expect("set isolated RPC sentinel nonblocking");
        let endpoint = format!(
            "tcp://127.0.0.1:{}/rpc",
            listener.local_addr().expect("RPC sentinel address").port()
        );
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: "combined".into(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: Some(
                OutboundOperationIdentity::validated("idem".into(), "corr".into())
                    .expect("valid operation"),
            ),
            attachments: Vec::new(),
        };
        let mut plan = build_sdk_send_plan(&envelope, "source", None);
        plan.send_request
            .extensions
            .insert("bounded".into(), serde_json::json!(true));

        let error = RpcNativeLxmfSdkSender::new(endpoint)
            .send_plan(plan)
            .await
            .expect_err("combined lossy guarantees must fail closed");
        let message = error.to_string();
        for guarantee in [
            "TTL",
            "idempotency key",
            "correlation identifier",
            "extensions",
        ] {
            assert!(message.contains(guarantee));
        }
        assert!(matches!(
            listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[tokio::test]
    async fn external_rpc_099_rejects_explicit_reply_ticket_before_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind isolated RPC sentinel");
        listener
            .set_nonblocking(true)
            .expect("set isolated RPC sentinel nonblocking");
        let endpoint = format!(
            "tcp://127.0.0.1:{}/rpc",
            listener.local_addr().expect("RPC sentinel address").port()
        );
        let mut operation =
            OutboundOperationIdentity::validated("idem-ticket".into(), "corr-ticket".into())
                .expect("valid operation");
        operation.automatic_propagation_fallback = false;
        let ticket_bytes = vec![0x10, 0x20, 0x30];
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: "ticket title".into(),
            body: "ticket body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: Some(NativeLxmfReplyTicket {
                ticket: ticket_bytes.clone(),
                expires: current_unix_secs_f64() + 60.0,
            }),
            operation: Some(operation),
            attachments: Vec::new(),
        };
        let plan = build_sdk_send_plan(&envelope, "source", None);
        assert_eq!(plan.rpc_delivery.ticket.as_deref(), Some("102030"));

        let error = RpcNativeLxmfSdkSender::new(endpoint)
            .send_plan(plan)
            .await
            .expect_err("unsupported explicit reply ticket must fail closed");
        let message = error.to_string();
        assert!(message.contains("cannot preserve required send guarantees"));
        assert!(message.contains("explicit reply ticket"));
        assert!(!message.contains("102030"));
        assert!(!message.contains(ticket_bytes.as_slice().escape_ascii().to_string().as_str()));
        assert!(matches!(
            listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[test]
    fn rpc_sdk_sender_reports_missing_endpoint_before_dispatch() {
        let sender = RpcNativeLxmfSdkSender::new(" ");
        let status = sender.status();

        assert_eq!(status.state, NativeLxmfSdkSenderState::MissingEndpoint);
        assert!(status.note.contains("configured local endpoint"));
    }

    #[test]
    fn rpc_sdk_sender_reports_configured_but_unprobed_for_local_endpoint() {
        let sender = RpcNativeLxmfSdkSender::new("tcp://127.0.0.1:37428/rpc");
        let status = sender.status();

        assert_eq!(sender.endpoint(), "tcp://127.0.0.1:37428/rpc");
        assert_eq!(status.state, NativeLxmfSdkSenderState::Configured);
        assert!(status.note.contains("readiness requires"));
        assert_eq!(
            sender.diagnostic_endpoint().as_deref(),
            Some("loopback:37428")
        );
    }

    #[tokio::test]
    async fn rpc_topic_capability_probe_negotiates_once_without_topic_or_shutdown_calls() {
        let (endpoint, captured_rx, capture_worker) = capture_topic_negotiation();
        let sender = RpcNativeLxmfSdkSender::new(endpoint);

        let probe = sender
            .probe_topic_capabilities()
            .await
            .expect("topic capability negotiation");
        let request = captured_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("captured topic negotiation");
        capture_worker.join().expect("join topic capture worker");

        assert_eq!(
            probe.endpoint.as_str(),
            sender.diagnostic_endpoint().unwrap()
        );
        assert_eq!(probe.active_contract_version, 2);
        assert!(probe.capabilities.topics);
        assert!(probe.capabilities.subscriptions);
        assert!(probe.capabilities.fanout);
        assert!(probe.capabilities.cursor_replay);
        assert!(probe.capabilities.async_events);
        assert!(!probe.capabilities.may_activate_receive_adapter());
        assert_eq!(request.method, "sdk_negotiate_v2");
        let requested = request
            .params
            .as_ref()
            .and_then(|params| params.get("requested_capabilities"))
            .and_then(serde_json::Value::as_array)
            .expect("requested topic capabilities");
        assert_eq!(requested.len(), 5);
        for capability in [
            crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_TOPICS,
            crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_SUBSCRIPTIONS,
            crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_FANOUT,
            crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_CURSOR_REPLAY,
            crate::runtime::lxmf_topics::LXMF_TOPIC_CAP_ASYNC_EVENTS,
        ] {
            assert!(requested
                .iter()
                .any(|value| value.as_str() == Some(capability)));
        }
    }

    #[tokio::test]
    async fn rpc_topic_capability_probe_has_one_total_deadline_and_redacts_endpoint_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stalled topic probe");
        let endpoint = format!(
            "tcp://127.0.0.1:{}/rpc",
            listener.local_addr().expect("stalled probe address").port()
        );
        let private_endpoint = endpoint.clone();
        let worker = thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept stalled topic probe");
            thread::sleep(Duration::from_millis(200));
        });
        let started = std::time::Instant::now();
        let error = RpcNativeLxmfSdkSender::new(endpoint)
            .probe_topic_capabilities_with_deadline(Duration::from_millis(50))
            .await
            .expect_err("stalled negotiation must time out");
        let elapsed = started.elapsed();
        worker.join().expect("join stalled topic probe worker");

        assert!(error.to_string().contains("timed out"));
        assert!(!error.to_string().contains(private_endpoint.as_str()));
        assert!(elapsed >= Duration::from_millis(50));
        assert!(elapsed < Duration::from_millis(500));
    }

    #[test]
    fn rpc_sdk_sender_rejects_remote_ambiguous_and_implied_tls_endpoints() {
        for endpoint in [
            "tcp://192.168.1.20:37428/rpc",
            "tcp://localhost:37428/rpc",
            "tcp://example.com:37428/rpc",
            "https://127.0.0.1:37428/rpc",
            "tls://127.0.0.1:37428/rpc",
            "tcp://127.0.0.1:0/rpc",
            "tcp://user@127.0.0.1:37428/rpc",
            "udp://127.0.0.1:37428/rpc",
        ] {
            let sender = RpcNativeLxmfSdkSender::new(endpoint);
            assert_eq!(
                sender.status().state,
                NativeLxmfSdkSenderState::RejectedEndpoint,
                "endpoint must be rejected: {endpoint}"
            );
            assert_eq!(sender.diagnostic_endpoint(), None);
        }
    }

    #[test]
    fn rpc_sdk_sender_accepts_literal_ipv4_and_ipv6_loopback() {
        for (endpoint, label) in [
            ("127.0.0.1:37428", "loopback:37428"),
            ("http://127.0.0.2:42/rpc", "loopback:42"),
            ("tcp://[::1]:37428/rpc", "loopback:37428"),
        ] {
            let sender = RpcNativeLxmfSdkSender::new(endpoint);
            assert_eq!(sender.status().state, NativeLxmfSdkSenderState::Configured);
            assert_eq!(sender.diagnostic_endpoint().as_deref(), Some(label));
        }
    }

    #[cfg(unix)]
    #[test]
    fn rpc_sdk_sender_accepts_absolute_unix_socket_without_exposing_path() {
        let sender = RpcNativeLxmfSdkSender::new("unix:/tmp/private/reticulum.sock");

        assert_eq!(sender.status().state, NativeLxmfSdkSenderState::Configured);
        assert_eq!(
            sender.diagnostic_endpoint().as_deref(),
            Some("unix:<local-socket>")
        );
    }

    #[tokio::test]
    async fn rpc_sdk_sender_missing_endpoint_fails_before_sending() {
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let sender = RpcNativeLxmfSdkSender::new("");
        let plan = build_sdk_send_plan(&envelope, "source", None);

        let error = sender
            .send_plan(plan)
            .await
            .expect_err("missing endpoint should fail locally");
        assert!(format!("{error}").contains("not configured"));
    }

    #[tokio::test]
    async fn rpc_sdk_sender_missing_endpoint_probe_fails_locally() {
        let sender = RpcNativeLxmfSdkSender::new("");

        let error = sender
            .probe()
            .await
            .expect_err("missing endpoint should fail before RPC dispatch");
        assert!(format!("{error}").contains("not configured"));
    }

    #[tokio::test]
    async fn rpc_sdk_sender_rejects_remote_endpoint_before_probe_or_send() {
        let sender = RpcNativeLxmfSdkSender::new("tcp://203.0.113.8:37428/rpc");
        let probe_error = sender
            .probe()
            .await
            .expect_err("remote endpoint must fail before probe I/O");
        assert!(format!("{probe_error}").contains("authenticated configuration"));

        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "body".into(),
            delivery_mode: DeliveryMode::Direct,
            include_ticket: false,
            native_reply_ticket: None,
            operation: None,
            attachments: Vec::new(),
        };
        let send_error = sender
            .send_plan(build_sdk_send_plan(&envelope, "source", None))
            .await
            .expect_err("remote endpoint must fail before send I/O");
        assert!(format!("{send_error}").contains("authenticated configuration"));
    }

    #[tokio::test]
    async fn missing_sdk_sender_probe_reports_not_wired() {
        let sender = MissingNativeLxmfSdkSender;

        let error = sender
            .probe()
            .await
            .expect_err("missing sender should not probe anything");
        assert!(format!("{error}").contains("not wired"));
    }
}
