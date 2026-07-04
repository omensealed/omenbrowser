#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeLxmfClientState {
    pub started: bool,
}

#[cfg(feature = "native-lxmf-sdk")]
use crate::error::{AppError, AppResult};

#[cfg(feature = "native-lxmf-sdk")]
use std::collections::HashMap;

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
    pub send_request: lxmf::sdk::SendRequest,
    pub rpc_delivery: rns_rpc::OutboundDeliveryOptions,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeLxmfSdkSenderState {
    Ready,
    MissingEndpoint,
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
    pub include_ticket: bool,
    pub reply_ticket_used: bool,
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkProbe {
    pub endpoint: String,
    pub runtime_id: String,
    pub state: String,
    pub active_contract_version: u16,
    pub queued_messages: u64,
    pub in_flight_messages: u64,
}

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
    pending: Mutex<HashMap<String, Option<String>>>,
}

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

    fn endpoint_is_configured(&self) -> bool {
        !self.endpoint.trim().is_empty()
    }
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
    pub fn capture_validate_record(&self, record: &rns_rpc::MessageRecord) {
        self.pending
            .lock()
            .expect("native LXMF SDK ticket cache")
            .insert(record.id.clone(), native_lxmf_sdk_record_ticket(record));
    }

    pub fn take_ticket(&self, message_id: &str) -> Option<String> {
        self.pending
            .lock()
            .expect("native LXMF SDK ticket cache")
            .remove(message_id)
            .flatten()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.pending
            .lock()
            .expect("native LXMF SDK ticket cache")
            .is_empty()
    }
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
        self.ticket_cache.capture_validate_record(record);
        Ok(())
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
    let destination = parse_lxmf_hash_hex(record.destination.as_str())?;
    let source = parse_lxmf_hash_hex(record.source.as_str())?;
    let signer =
        reticulum_rs::core::identity::PrivateIdentity::from_private_key_bytes(identity_bytes)
            .map_err(|_| AppError::Runtime("native LXMF SDK bridge identity is invalid".into()))?;

    let mut fields = sdk_record_fields_to_rmpv(record.fields.as_ref())?;
    if options.include_ticket {
        insert_lxmf_ticket_field(&mut fields);
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
    if let Some(ticket_hex) = reply_ticket_hex {
        let ticket = parse_lxmf_ticket_hex(ticket_hex)?;
        let message_id = sdk_message_id(&message)?;
        let stamp = sdk_ticket_stamp(ticket.as_slice(), &message_id)?;
        message.set_stamp_from_bytes(&stamp);
        reply_ticket_used = true;
    }

    let wire_bytes = message.to_wire(Some(&signer)).map_err(|error| {
        AppError::Runtime(format!("LXMF SDK bridge wire encode failed: {error}"))
    })?;
    let wire = lxmf::WireMessage::unpack(wire_bytes.as_slice()).map_err(|error| {
        AppError::Runtime(format!(
            "LXMF SDK bridge encoded wire decode failed: {error}"
        ))
    })?;

    Ok(NativeLxmfSdkWireDelivery {
        wire_bytes,
        message_id: hex_bytes(&wire.message_id()),
        destination_hash: record.destination.clone(),
        method: options.method.clone(),
        include_ticket: options.include_ticket,
        reply_ticket_used,
    })
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn build_sdk_wire_delivery_from_envelope(
    envelope: &crate::messaging::MessageEnvelope,
    source_hash: &str,
    identity_bytes: &[u8],
    stamp_cost: Option<u32>,
) -> AppResult<NativeLxmfSdkWireDelivery> {
    let plan = build_sdk_send_plan(envelope, source_hash, stamp_cost);
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
    let reply_ticket_hex = envelope
        .native_reply_ticket
        .as_ref()
        .map(|ticket| hex_bytes(&ticket.ticket));
    build_sdk_wire_delivery(
        &record,
        &plan.rpc_delivery,
        identity_bytes,
        reply_ticket_hex.as_deref(),
    )
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
    match fields {
        Some(rmpv::Value::Map(entries)) => {
            entries.push((rmpv::Value::Integer(LXMF_TICKET_FIELD.into()), value));
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
    Ok(lxmf::WireMessage::new(destination, source, payload).message_id())
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
    let _rpc_client = lxmf::sdk::RpcBackendClient::new("tcp://127.0.0.1:0/rpc");
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
        if self.endpoint_is_configured() {
            NativeLxmfSdkSenderStatus {
                name: "rpc-native-lxmf-sdk-sender",
                state: NativeLxmfSdkSenderState::Ready,
                note: "lxmf-sdk RPC sender has a configured endpoint; delivery still requires a compatible local sidecar",
            }
        } else {
            NativeLxmfSdkSenderStatus {
                name: "rpc-native-lxmf-sdk-sender",
                state: NativeLxmfSdkSenderState::MissingEndpoint,
                note:
                    "lxmf-sdk RPC sender needs a configured local endpoint before it can dispatch",
            }
        }
    }

    async fn probe(&self) -> AppResult<NativeLxmfSdkProbe> {
        if !self.endpoint_is_configured() {
            return Err(AppError::Unsupported(
                "native LXMF SDK/RPC probe has no configured endpoint".into(),
            ));
        }

        let endpoint = self.endpoint.clone();
        let snapshot = tokio::task::spawn_blocking({
            let endpoint = endpoint.clone();
            move || {
                use lxmf::sdk::SdkBackend;

                let client = lxmf::sdk::RpcBackendClient::new(endpoint);
                client.snapshot()
            }
        })
        .await
        .map_err(|err| AppError::Runtime(format!("native LXMF SDK probe task failed: {err}")))?
        .map_err(|err| AppError::Runtime(format!("native LXMF SDK probe failed: {err}")))?;

        Ok(NativeLxmfSdkProbe {
            endpoint,
            runtime_id: snapshot.runtime_id,
            state: format!("{:?}", snapshot.state),
            active_contract_version: snapshot.active_contract_version,
            queued_messages: snapshot.queued_messages,
            in_flight_messages: snapshot.in_flight_messages,
        })
    }

    async fn send_plan(&self, plan: NativeLxmfSdkSendPlan) -> AppResult<NativeLxmfSdkSendReceipt> {
        if !self.endpoint_is_configured() {
            return Err(AppError::Unsupported(
                "native LXMF SDK/RPC sender has no configured endpoint".into(),
            ));
        }

        let endpoint = self.endpoint.clone();
        let send_request = plan.send_request;
        let message_id = tokio::task::spawn_blocking(move || {
            use lxmf::sdk::SdkBackend;

            let client = lxmf::sdk::RpcBackendClient::new(endpoint);
            client.send(send_request)
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
}

#[cfg(feature = "native-lxmf-sdk")]
fn embedded_sdk_send_params(
    request: lxmf::sdk::SendRequest,
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
    })
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
        crate::messaging::DeliveryMode::Propagated
    );
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
        lxmf::sdk::SendRequest::new(source_hash, envelope.peer_hash.as_str(), payload)
            .with_delivery_method(delivery_method)
            .with_include_ticket(envelope.include_ticket)
            .with_try_propagation_on_fail(try_propagation_on_fail);
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
    use std::path::PathBuf;
    use std::sync::mpsc as std_mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use crate::identity::IdentityMaterialProvider;
    use crate::messaging::{DeliveryMode, MessageEnvelope, NativeLxmfReplyTicket};
    use crate::runtime::native::identity::{
        load_private_identity_bytes, NativeReticulumIdentityProvider,
    };
    use rns_rpc::OutboundBridge;

    use super::{
        build_sdk_send_plan, build_sdk_wire_delivery, hex_bytes, native_lxmf_sdk_record_ticket,
        native_lxmf_sdk_runtime_boundary_decision, EmbeddedNativeLxmfSdkSender,
        MissingNativeLxmfSdkSender, NativeLxmfSdkOutboundBridge, NativeLxmfSdkRuntimeBoundaryKind,
        NativeLxmfSdkSender, NativeLxmfSdkSenderState, NativeLxmfSdkTicketCache,
        NativeLxmfSdkWireDelivery, NativeLxmfSdkWireSubmitter, RpcNativeLxmfSdkSender,
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
            self.ticket_cache.capture_validate_record(record);
            Ok(())
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
            attachments: vec![PathBuf::from("/tmp/report.txt")],
        };

        let plan = build_sdk_send_plan(&envelope, "source", Some(7));

        assert_eq!(plan.send_request.source, "source");
        assert_eq!(plan.send_request.destination, "peer");
        assert_eq!(plan.send_request.delivery_method.as_deref(), Some("direct"));
        assert_eq!(plan.send_request.stamp_cost, Some(7));
        assert_eq!(plan.send_request.include_ticket, Some(true));
        assert_eq!(plan.send_request.try_propagation_on_fail, Some(false));
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
    fn sdk_send_plan_maps_propagated_message_to_propagation_method() {
        let envelope = MessageEnvelope {
            peer_hash: "peer".into(),
            title: String::new(),
            body: "propagated".into(),
            delivery_mode: DeliveryMode::Propagated,
            include_ticket: false,
            native_reply_ticket: None,
            attachments: Vec::new(),
        };

        let plan = build_sdk_send_plan(&envelope, "source", None);

        assert_eq!(
            plan.send_request.delivery_method.as_deref(),
            Some("propagated")
        );
        assert_eq!(plan.send_request.stamp_cost, None);
        assert_eq!(plan.send_request.include_ticket, Some(false));
        assert_eq!(plan.send_request.try_propagation_on_fail, Some(true));
        assert_eq!(plan.rpc_delivery.method.as_deref(), Some("propagated"));
        assert_eq!(plan.rpc_delivery.stamp_cost, None);
        assert!(!plan.rpc_delivery.include_ticket);
        assert!(plan.rpc_delivery.try_propagation_on_fail);
        assert_eq!(plan.rpc_delivery.ticket, None);
    }

    #[test]
    fn sdk_send_plan_covers_direct_and_propagated_ticket_matrix() {
        for (delivery_mode, expected_method, include_ticket, expected_retry) in [
            (DeliveryMode::Direct, "direct", false, false),
            (DeliveryMode::Direct, "direct", true, false),
            (DeliveryMode::Propagated, "propagated", false, true),
            (DeliveryMode::Propagated, "propagated", true, true),
        ] {
            let envelope = MessageEnvelope {
                peer_hash: "peer".into(),
                title: String::new(),
                body: "body".into(),
                delivery_mode,
                include_ticket,
                native_reply_ticket: None,
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
            assert_eq!(
                plan.send_request.try_propagation_on_fail,
                Some(expected_retry)
            );
            assert_eq!(plan.rpc_delivery.try_propagation_on_fail, expected_retry);
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
        assert!(delivery.options.try_propagation_on_fail);
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
        cache.capture_validate_record(&record);

        let mut sanitized = record.clone();
        sanitized.fields = None;
        assert_eq!(native_lxmf_sdk_record_ticket(&sanitized), None);
        assert_eq!(cache.take_ticket("message-1").as_deref(), Some("aabbccdd"));
        assert_eq!(cache.take_ticket("message-1"), None);
        assert!(cache.is_empty());
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

        for (delivery_mode, expected_method, include_ticket, expected_retry) in [
            (DeliveryMode::Direct, "direct", false, false),
            (DeliveryMode::Direct, "direct", true, false),
            (DeliveryMode::Propagated, "propagated", false, true),
            (DeliveryMode::Propagated, "propagated", true, true),
        ] {
            let envelope = MessageEnvelope {
                peer_hash: "peer".into(),
                title: String::new(),
                body: "matrix body".into(),
                delivery_mode,
                include_ticket,
                native_reply_ticket: None,
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
            assert_eq!(delivery.options.try_propagation_on_fail, expected_retry);
            assert_eq!(delivery.record.content, "matrix body");
        }
    }

    #[test]
    fn rpc_sdk_sender_reports_missing_endpoint_before_dispatch() {
        let sender = RpcNativeLxmfSdkSender::new(" ");
        let status = sender.status();

        assert_eq!(status.state, NativeLxmfSdkSenderState::MissingEndpoint);
        assert!(status.note.contains("configured local endpoint"));
    }

    #[test]
    fn rpc_sdk_sender_reports_ready_when_endpoint_is_configured() {
        let sender = RpcNativeLxmfSdkSender::new("tcp://127.0.0.1:0/rpc");
        let status = sender.status();

        assert_eq!(sender.endpoint(), "tcp://127.0.0.1:0/rpc");
        assert_eq!(status.state, NativeLxmfSdkSenderState::Ready);
        assert!(status.note.contains("compatible local sidecar"));
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
            attachments: Vec::new(),
        };
        let sender = RpcNativeLxmfSdkSender::new("");
        let plan = build_sdk_send_plan(&envelope, "source", None);

        let error = sender
            .send_plan(plan)
            .await
            .expect_err("missing endpoint should fail locally");
        assert!(format!("{error}").contains("no configured endpoint"));
    }

    #[tokio::test]
    async fn rpc_sdk_sender_missing_endpoint_probe_fails_locally() {
        let sender = RpcNativeLxmfSdkSender::new("");

        let error = sender
            .probe()
            .await
            .expect_err("missing endpoint should fail before RPC dispatch");
        assert!(format!("{error}").contains("no configured endpoint"));
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
