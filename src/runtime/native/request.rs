use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rmpv::Value;
use rns_transport::destination::link::{Link, LinkEvent, LinkStatus};
use rns_transport::destination::{DestinationDesc, DestinationName, SingleOutputDestination};
use rns_transport::hash::AddressHash;
use rns_transport::resource::{ResourceComplete, ResourceEventKind};
use rns_transport::PacketContext;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
use tokio::sync::Mutex;

use crate::browser::page::DEFAULT_PATH;
use crate::browser::{BrowserAddress, BrowserPage, PageSource};
use crate::error::{AppError, AppResult};
use crate::runtime::native::NativeRuntimeError;
use crate::runtime::network::{
    CancellationToken, ResourceLifecycleEvent, ResourceLifecycleState, ResourceProgressEvent,
};
use crate::runtime::RuntimeBusEvent;

pub const NOMADNET_APP_NAME: &str = "nomadnetwork";
pub const NOMADNET_NODE_ASPECT: &str = "node";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePageRequest {
    pub url: String,
    pub destination_hash: AddressHash,
    pub path: String,
    pub request_data: Option<BTreeMap<String, String>>,
}

impl NativePageRequest {
    pub fn from_url(
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
    ) -> Result<Self, NativeRuntimeError> {
        if url.starts_with("http://") || url.starts_with("https://") {
            return Err(NativeRuntimeError::InvalidAddress(url.into()));
        }

        let address = BrowserAddress::parse(url)
            .ok_or_else(|| NativeRuntimeError::InvalidAddress(url.into()))?;
        let destination_hash = AddressHash::new_from_hex_string(&address.destination)
            .map_err(|_| NativeRuntimeError::InvalidAddress(redacted_destination_url(&address)))?;
        let path = normalize_native_nomadnet_path(&address.path);
        let url = format!("{}:{}", address.destination, path);

        Ok(Self {
            url,
            destination_hash,
            path,
            request_data,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativePageExchangePrimitive {
    /// Python OMENbrowser uses RNS Link.request(path, data=...) and waits on the receipt response.
    /// The verified Rust transport APIs do not currently expose this exact high-level request API.
    LinkRequestReceipt,
    /// Available in reticulum-rs-transport, but the OMEN/NomadNet message type and response
    /// convention still need compatibility verification before it can carry page fetches.
    ChannelMessage,
    /// Available in reticulum-rs-transport for large payload transfer, but page request/response
    /// negotiation must be proven before using it for browser fetches.
    ResourceTransfer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRuntimeCapabilityState {
    Available,
    NeedsVerification,
    MissingHighLevelAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRuntimeCapability {
    pub name: &'static str,
    pub state: NativeRuntimeCapabilityState,
    pub note: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeReticulum06CapabilityReport {
    pub stack: &'static str,
    pub transport_crate: &'static str,
    pub lxmf_crate: &'static str,
    pub capabilities: Vec<NativeRuntimeCapability>,
    pub blockers: Vec<&'static str>,
    pub recommended_next_step: &'static str,
}

impl NativeReticulum06CapabilityReport {
    pub fn has_blockers(&self) -> bool {
        !self.blockers.is_empty()
    }

    pub fn capability_state(&self, name: &str) -> Option<NativeRuntimeCapabilityState> {
        self.capabilities
            .iter()
            .find(|capability| capability.name == name)
            .map(|capability| capability.state)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeReticulum06RequestResponseProbe {
    pub request_context_available: bool,
    pub response_context_available: bool,
    pub received_data_request_id_available: bool,
    pub link_data_packet_available: bool,
    pub link_channel_packet_available: bool,
    pub public_bound_link_data_send_available: bool,
    pub public_bound_link_channel_send_available: bool,
    pub public_bound_request_context_send_available: bool,
    pub public_bound_link_identify_send_available: bool,
    pub request_resource_send_available: bool,
    pub resource_response_events_available: bool,
    pub public_packet_context_mutation_available: bool,
    pub public_transport_packet_dispatch_available: bool,
    pub high_level_link_request_send_available: bool,
    pub recommended_adapter: &'static str,
    pub note: &'static str,
}

pub fn native_reticulum06_request_response_probe() -> NativeReticulum06RequestResponseProbe {
    let _ = rns_transport::PacketContext::Request;
    let _ = rns_transport::PacketContext::Response;
    let _ = rns_transport::PacketContext::LinkIdentify;
    let _ = std::mem::size_of::<rns_transport::transport::ReceivedData>();
    let _ = std::mem::size_of::<rns_transport::destination::link::LinkPayload>();
    let _ = std::mem::size_of::<rns_transport::resource::ResourceEvent>();
    let _ = rns_transport::transport::Transport::send_to_out_links;
    let _ = rns_transport::transport::Transport::send_channel_message;
    let _ = rns_transport::transport::Transport::send_direct;

    NativeReticulum06RequestResponseProbe {
        request_context_available: true,
        response_context_available: true,
        received_data_request_id_available: true,
        link_data_packet_available: true,
        link_channel_packet_available: true,
        public_bound_link_data_send_available: true,
        public_bound_link_channel_send_available: true,
        public_bound_request_context_send_available: false,
        public_bound_link_identify_send_available: true,
        request_resource_send_available: true,
        resource_response_events_available: true,
        public_packet_context_mutation_available: true,
        public_transport_packet_dispatch_available: true,
        high_level_link_request_send_available: false,
        recommended_adapter:
            "use request-resource for clean-stack NomadNet request parity; send LinkIdentify by building encrypted link data and dispatching it with send_direct on the link ingress interface",
        note: "reticulum-rs-transport 0.6 exposes request/response contexts, inbound request IDs, direct bound sends for None/Channel link data, public packet context mutation, send_direct, and request/response resource helpers; direct small-packet Link.request still lacks public request-context link data dispatch, so the clean adapter uses request resources for parity",
    }
}

#[cfg(feature = "native-lxmf-sdk")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLxmfSdkCapabilityReport {
    pub sdk_crate: &'static str,
    pub config_available: bool,
    pub rpc_backend_config_available: bool,
    pub send_request_available: bool,
    pub direct_delivery_field_available: bool,
    pub propagation_retry_field_available: bool,
    pub stamp_cost_field_available: bool,
    pub ticket_field_available: bool,
    pub rpc_delivery_options_available: bool,
    pub rpc_ticket_record_available: bool,
    pub page_fetch_adapter_available: bool,
    pub recommended_use: &'static str,
}

#[cfg(feature = "native-lxmf-sdk")]
pub fn native_lxmf_sdk_capability_report() -> NativeLxmfSdkCapabilityReport {
    let config = lxmf::sdk::SdkConfig::desktop_full_default();
    let rpc_backend_config_available = config.rpc_backend.is_some();
    let start = lxmf::sdk::StartRequest::new(config);
    let send = lxmf::sdk::SendRequest::new(
        "source",
        "destination",
        serde_json::json!({ "content": "probe" }),
    )
    .with_delivery_method("direct")
    .with_stamp_cost(8)
    .with_try_propagation_on_fail(true)
    .with_include_ticket(true);
    let rpc_delivery = rns_rpc::OutboundDeliveryOptions {
        method: Some("direct".to_string()),
        stamp_cost: Some(8),
        include_ticket: true,
        try_propagation_on_fail: true,
        ticket: Some("probe-ticket".to_string()),
        source_private_key: None,
    };
    let rpc_ticket = rns_rpc::TicketRecord {
        destination: "destination".to_string(),
        ticket: "probe-ticket".to_string(),
        expires_at: 1,
    };
    let _ = std::mem::size_of::<lxmf::sdk::EventBatch>();
    let _ = std::mem::size_of::<lxmf::sdk::RuntimeSnapshot>();

    NativeLxmfSdkCapabilityReport {
        sdk_crate: "lxmf-sdk 0.6",
        config_available: start.config.validate().is_ok(),
        rpc_backend_config_available,
        send_request_available: true,
        direct_delivery_field_available: send.delivery_method.as_deref() == Some("direct"),
        propagation_retry_field_available: send.try_propagation_on_fail == Some(true),
        stamp_cost_field_available: send.stamp_cost == Some(8),
        ticket_field_available: send.include_ticket == Some(true),
        rpc_delivery_options_available: rpc_delivery.include_ticket
            && rpc_delivery.try_propagation_on_fail
            && rpc_delivery.stamp_cost == Some(8)
            && rpc_delivery.ticket.as_deref() == Some("probe-ticket"),
        rpc_ticket_record_available: rpc_ticket.destination == "destination"
            && rpc_ticket.ticket == "probe-ticket"
            && rpc_ticket.expires_at == 1,
        page_fetch_adapter_available: false,
        recommended_use:
            "use lxmf-sdk for LXMF send/status/events evaluation; keep NomadNet page fetch on a Link.request adapter or RPC transport boundary",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLinkRequestAdapterPlan {
    pub boundary: &'static str,
    pub request_context: rns_transport::PacketContext,
    pub response_context: rns_transport::PacketContext,
    pub request_id_source: &'static str,
    pub dispatch_status: NativeRuntimeCapabilityState,
    pub next_step: &'static str,
}

impl NativeLinkRequestAdapterPlan {
    pub fn reticulum06_available() -> Self {
        Self {
            boundary: "reticulum-rs-transport Link request adapter",
            request_context: PacketContext::Request,
            response_context: PacketContext::Response,
            request_id_source: "response ReceivedData.request_id or link payload request_id",
            dispatch_status: NativeRuntimeCapabilityState::Available,
            next_step: "keep request-resource as the verified clean-stack NomadNet request path; add a direct request-context link send API later for small packet efficiency",
        }
    }

    pub fn reticulum06_missing() -> Self {
        Self {
            boundary: "reticulum-rs-transport Link request adapter",
            request_context: PacketContext::Request,
            response_context: PacketContext::Response,
            request_id_source: "response ReceivedData.request_id or link payload request_id",
            dispatch_status: NativeRuntimeCapabilityState::MissingHighLevelAdapter,
            next_step: "wire reticulum-rs-transport request-context packet dispatch",
        }
    }

    pub fn is_ready(&self) -> bool {
        self.dispatch_status == NativeRuntimeCapabilityState::Available
    }
}

#[async_trait]
pub trait NativeLinkRequestAdapter: Send + Sync {
    fn adapter_name(&self) -> &'static str;

    fn plan(&self) -> NativeLinkRequestAdapterPlan;

    async fn send_request(
        &self,
        prepared: &NativePreparedPageLink,
        frame: &NativeLinkRequestFrame,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<NativeLinkResponseFrame>;
}

#[derive(Clone, Debug, Default)]
pub struct MissingReticulum06LinkRequestAdapter;

#[async_trait]
impl NativeLinkRequestAdapter for MissingReticulum06LinkRequestAdapter {
    fn adapter_name(&self) -> &'static str {
        "missing-reticulum06-link-request"
    }

    fn plan(&self) -> NativeLinkRequestAdapterPlan {
        NativeLinkRequestAdapterPlan::reticulum06_missing()
    }

    async fn send_request(
        &self,
        prepared: &NativePreparedPageLink,
        frame: &NativeLinkRequestFrame,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<NativeLinkResponseFrame> {
        let _ = (prepared, frame, timeout);
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        Err(AppError::from(NativeRuntimeError::Unsupported(
            "reticulum-rs 0.6 Link.request adapter is not wired; use the clean resource request path or add the reticulumd/RPC adapter",
        )))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Reticulum06LinkRequestAdapter;

#[async_trait]
impl NativeLinkRequestAdapter for Reticulum06LinkRequestAdapter {
    fn adapter_name(&self) -> &'static str {
        "reticulum06-link-request"
    }

    fn plan(&self) -> NativeLinkRequestAdapterPlan {
        NativeLinkRequestAdapterPlan::reticulum06_available()
    }

    async fn send_request(
        &self,
        prepared: &NativePreparedPageLink,
        frame: &NativeLinkRequestFrame,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<NativeLinkResponseFrame> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        self.send_request_resource(prepared, frame, timeout, cancel)
            .await
    }
}

impl Reticulum06LinkRequestAdapter {
    async fn send_request_resource(
        &self,
        prepared: &NativePreparedPageLink,
        frame: &NativeLinkRequestFrame,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<NativeLinkResponseFrame> {
        let Some(transport) = prepared.transport.as_ref() else {
            return Err(AppError::from(NativeRuntimeError::Unsupported(
                "native Reticulum Link.request resource adapter has no transport handle",
            )));
        };
        let Some(link) = prepared.link.as_ref() else {
            return Err(AppError::from(NativeRuntimeError::Unsupported(
                "native Reticulum Link.request resource adapter has no link handle",
            )));
        };
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        let link_ingress_iface = {
            let link = link.lock().await;
            link.ingress_iface()
        };
        let destination_path = transport.path_status(&prepared.destination_hash).await;
        let link_path = transport.path_status(&prepared.link_id).await;
        let mut resource_events = transport.resource_events();
        let request_resource_hash = transport
            .send_request_resource(
                &prepared.link_id,
                frame.request_id.to_vec(),
                frame.packed.clone(),
                None,
            )
            .await
            .map_err(|error| {
                AppError::from(NativeRuntimeError::Native(format!(
                    "native Reticulum 0.6 request-resource send failed: {error:?}"
                )))
            })?;
        tracing::debug!(
            adapter = self.adapter_name(),
            destination = %prepared.destination_hash,
            link_id = %prepared.link_id,
            path = %prepared.path,
            request_id = %hex_bytes(&frame.request_id),
            request_resource_hash = %request_resource_hash,
            request_bytes = frame.packed.len(),
            link_ingress_iface = ?link_ingress_iface,
            destination_path = %transport_path_status_summary(&destination_path),
            link_path = %transport_path_status_summary(&link_path),
            "native Reticulum 0.6 Link.request sent as request resource"
        );
        emit_clean_page_debug(
            prepared.event_tx.as_ref(),
            format!(
                "native Reticulum 0.6 clean page request-resource sent destination={} link_id={} path={} request_id={} request_resource={} bytes={} link_iface={:?}",
                prepared.destination_hash,
                prepared.link_id,
                prepared.path,
                hex_bytes(&frame.request_id),
                request_resource_hash,
                frame.packed.len(),
                link_ingress_iface
            ),
        );

        let deadline = tokio::time::Instant::now() + timeout;
        let mut target_events = 0usize;
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
                    "NomadNet request-resource response; link_iface={:?}; destination_path={}; \
                         link_path={}; request_resource={}; target_events={}; progress_events={}; \
                         unrelated_events={}; outbound_complete={}; last_error={}",
                    link_ingress_iface,
                    transport_path_status_summary(&destination_path),
                    transport_path_status_summary(&link_path),
                    request_resource_hash,
                    target_events,
                    progress_events,
                    unrelated_events,
                    outbound_complete,
                    last_error
                ))));
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            match tokio::time::timeout(wait, resource_events.recv()).await {
                Ok(Ok(event)) if event.link_id == prepared.link_id => {
                    target_events += 1;
                    match event.kind {
                        ResourceEventKind::Complete(complete) => {
                            match NativeLinkResponseFrame::parse_matching_response_resource(
                                &complete,
                                &frame.request_id,
                            ) {
                                Ok(Some(response)) => {
                                    tracing::debug!(
                                        adapter = self.adapter_name(),
                                        destination = %prepared.destination_hash,
                                        link_id = %prepared.link_id,
                                        path = %prepared.path,
                                        request_id = %hex_bytes(&frame.request_id),
                                        response_resource_hash = %event.hash,
                                        bytes = complete.data.len(),
                                        metadata = complete.metadata.as_ref().map(|value| value.len()),
                                        "native Reticulum 0.6 Link.request response resource received"
                                    );
                                    emit_clean_page_debug(
                                        prepared.event_tx.as_ref(),
                                        format!(
                                            "native Reticulum 0.6 clean page response-resource received destination={} link_id={} path={} request_id={} response_resource={} bytes={}",
                                            prepared.destination_hash,
                                            prepared.link_id,
                                            prepared.path,
                                            hex_bytes(&frame.request_id),
                                            event.hash,
                                            complete.data.len()
                                        ),
                                    );
                                    return Ok(response);
                                }
                                Ok(None) => {
                                    unrelated_events += 1;
                                }
                                Err(error) => {
                                    last_error = format!("response parse error: {error:?}");
                                    unrelated_events += 1;
                                }
                            }
                        }
                        ResourceEventKind::Progress(progress) => {
                            progress_events += 1;
                            emit_clean_page_resource_progress(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                progress.received_bytes,
                                progress.total_bytes,
                            );
                        }
                        ResourceEventKind::OutboundComplete
                            if event.hash == request_resource_hash =>
                        {
                            emit_clean_page_resource_lifecycle(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                ResourceLifecycleState::Complete,
                                None,
                                None,
                            );
                            outbound_complete = true;
                        }
                        ResourceEventKind::OutboundFailed
                            if event.hash == request_resource_hash =>
                        {
                            emit_clean_page_resource_lifecycle(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                ResourceLifecycleState::Failed,
                                None,
                                Some("outbound request-resource transfer failed".into()),
                            );
                            return Err(AppError::from(NativeRuntimeError::Native(
                                "native Reticulum 0.6 request-resource transfer failed".into(),
                            )));
                        }
                        ResourceEventKind::InboundFailed(failure) => {
                            emit_clean_page_resource_lifecycle(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                ResourceLifecycleState::Failed,
                                None,
                                Some(failure.reason.clone()),
                            );
                            last_error = failure.reason;
                        }
                        ResourceEventKind::OutboundCancelled
                            if event.hash == request_resource_hash =>
                        {
                            emit_clean_page_resource_lifecycle(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                ResourceLifecycleState::Cancelled,
                                None,
                                Some("cancelled".into()),
                            );
                            return Err(AppError::from(NativeRuntimeError::Cancelled));
                        }
                        _ => {}
                    }
                }
                Ok(Ok(_)) => {
                    unrelated_events += 1;
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                    tracing::debug!(
                        adapter = self.adapter_name(),
                        destination = %prepared.destination_hash,
                        link_id = %prepared.link_id,
                        skipped,
                        "native Reticulum request-resource event stream lagged"
                    );
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum resource event stream closed".into(),
                    )));
                }
                Err(_) => {}
            }
        }
    }
}

pub fn native_reticulum06_capability_report() -> NativeReticulum06CapabilityReport {
    let request_response = native_reticulum06_request_response_probe();
    NativeReticulum06CapabilityReport {
        stack: "reticulum-rs 0.6",
        transport_crate: "reticulum-rs-transport 0.6",
        lxmf_crate: "lxmf 0.6",
        capabilities: vec![
            NativeRuntimeCapability {
                name: "transport-runtime",
                state: NativeRuntimeCapabilityState::Available,
                note: "reticulum-rs runtime and transport configuration types are available",
            },
            NativeRuntimeCapability {
                name: "destination-links",
                state: NativeRuntimeCapabilityState::Available,
                note: "reticulum-rs-transport exposes destination link lifecycle primitives",
            },
            NativeRuntimeCapability {
                name: "channel-messages",
                state: NativeRuntimeCapabilityState::NeedsVerification,
                note: "channel messaging exists and sends directly on the bound link interface, but it uses PacketContext::Channel and is not NomadNet Link.request",
            },
            NativeRuntimeCapability {
                name: "bound-link-data",
                state: NativeRuntimeCapabilityState::NeedsVerification,
                note: "public bound link-data helpers exist for PacketContext::None/Channel; REQUEST-context direct dispatch is still missing",
            },
            NativeRuntimeCapability {
                name: "link-identify",
                state: NativeRuntimeCapabilityState::Available,
                note: "NomadNet identify-on-connect is live-verified as encrypted link data with PacketContext::LinkIdentify over the active link's ingress interface",
            },
            NativeRuntimeCapability {
                name: "resource-transfer",
                state: NativeRuntimeCapabilityState::Available,
                note: "request/response resource helpers are live-verified as the clean-stack NomadNet page request path while direct small-packet request-context dispatch is unavailable",
            },
            NativeRuntimeCapability {
                name: "link-request-receipt",
                state: if request_response.request_resource_send_available
                    && request_response.resource_response_events_available
                {
                    NativeRuntimeCapabilityState::Available
                } else {
                    NativeRuntimeCapabilityState::MissingHighLevelAdapter
                },
                note: request_response.note,
            },
            NativeRuntimeCapability {
                name: "lxmf-wire",
                state: NativeRuntimeCapabilityState::Available,
                note: "lxmf 0.6 wire encode/decode helpers compile in the native LXMF path",
            },
            NativeRuntimeCapability {
                name: "lxmf-sdk",
                state: NativeRuntimeCapabilityState::NeedsVerification,
                note: "SDK/RPC sidecar path is available as an opt-in evaluation path, not the default runtime",
            },
        ],
        blockers: vec![
            "direct Link.request helper remains unavailable in reticulum-rs-transport 0.6; clean resource requests are used instead",
            "continue live parity checks against direct, propagated, ticket, and attachment LXMF workflows",
        ],
        recommended_next_step:
            "keep live-testing clean-stack page fetches/submissions, then add or upstream a direct PacketContext::Request link-data helper for small page request efficiency",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePageExchangePlan {
    pub destination_hash: AddressHash,
    pub app_name: &'static str,
    pub aspect: &'static str,
    pub path: String,
    pub request_data: BTreeMap<String, String>,
    pub timeout: Duration,
    pub preferred_primitive: NativePageExchangePrimitive,
}

impl NativePageExchangePlan {
    pub fn from_fetch_plan(plan: &NativeFetchPlan) -> Self {
        Self {
            destination_hash: plan.request.destination_hash,
            app_name: NOMADNET_APP_NAME,
            aspect: NOMADNET_NODE_ASPECT,
            path: plan.request.path.clone(),
            request_data: plan.request.request_data.clone().unwrap_or_default(),
            timeout: plan.timeout,
            preferred_primitive: NativePageExchangePrimitive::LinkRequestReceipt,
        }
    }

    pub fn is_python_compatible_shape(&self) -> bool {
        self.app_name == NOMADNET_APP_NAME
            && self.aspect == NOMADNET_NODE_ASPECT
            && matches!(
                self.preferred_primitive,
                NativePageExchangePrimitive::LinkRequestReceipt
            )
    }
}

#[derive(Clone)]
pub struct NativePageFetchContext {
    pub transport: Arc<reticulum_rs::runtime::Transport>,
    pub identify_on_connect: bool,
    pub identify_identity: Option<Arc<rns_transport::identity::PrivateIdentity>>,
    pub event_tx: Option<broadcast::Sender<RuntimeBusEvent>>,
}

impl NativePageFetchContext {
    pub fn new(transport: Arc<reticulum_rs::runtime::Transport>) -> Self {
        Self {
            transport,
            identify_on_connect: false,
            identify_identity: None,
            event_tx: None,
        }
    }

    pub fn with_identify_on_connect(
        transport: Arc<reticulum_rs::runtime::Transport>,
        identify_on_connect: bool,
        identify_identity: Option<Arc<rns_transport::identity::PrivateIdentity>>,
        event_tx: Option<broadcast::Sender<RuntimeBusEvent>>,
    ) -> Self {
        Self {
            transport,
            identify_on_connect,
            identify_identity,
            event_tx,
        }
    }
}

fn emit_clean_page_debug(
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
    message: impl Into<String>,
) {
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::Debug(message.into()));
    }
}

fn emit_clean_page_resource_progress(
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
    transfer_id: String,
    received: u64,
    total: u64,
) {
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::ResourceProgress(ResourceProgressEvent {
            transfer_id,
            received,
            total: Some(total),
            source: Some("nomadnet-page".into()),
            purpose: Some("nomadnet-page".into()),
            direction: Some("inbound".into()),
            peer: None,
        }));
    }
}

fn emit_clean_page_resource_lifecycle(
    event_tx: Option<&broadcast::Sender<RuntimeBusEvent>>,
    transfer_id: String,
    state: ResourceLifecycleState,
    bytes: Option<u64>,
    reason: Option<String>,
) {
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(ResourceLifecycleEvent {
            transfer_id,
            state,
            bytes,
            reason,
            source: Some("nomadnet-page".into()),
            purpose: Some("nomadnet-page".into()),
            direction: Some("inbound".into()),
            peer: None,
        }));
    }
}

#[derive(Clone)]
pub struct NativePreparedPageLink {
    pub destination_hash: AddressHash,
    pub link_id: AddressHash,
    pub path: String,
    pub request_data: BTreeMap<String, String>,
    pub transport: Option<Arc<reticulum_rs::runtime::Transport>>,
    pub link: Option<Arc<Mutex<Link>>>,
    pub event_tx: Option<broadcast::Sender<RuntimeBusEvent>>,
}

impl std::fmt::Debug for NativePreparedPageLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePreparedPageLink")
            .field("destination_hash", &self.destination_hash)
            .field("link_id", &self.link_id)
            .field("path", &self.path)
            .field("request_data", &self.request_data)
            .field("transport", &self.transport.as_ref().map(|_| "<transport>"))
            .field("link", &self.link.as_ref().map(|_| "<link>"))
            .finish()
    }
}

impl PartialEq for NativePreparedPageLink {
    fn eq(&self, other: &Self) -> bool {
        self.destination_hash == other.destination_hash
            && self.link_id == other.link_id
            && self.path == other.path
            && self.request_data == other.request_data
    }
}

impl Eq for NativePreparedPageLink {}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeLinkRequestFrame {
    pub path: String,
    pub path_hash: [u8; 16],
    pub request_id: [u8; 16],
    pub packed: Vec<u8>,
}

impl NativeLinkRequestFrame {
    pub fn build(
        path: &str,
        request_data: &BTreeMap<String, String>,
        timestamp: f64,
    ) -> Result<Self, NativeRuntimeError> {
        Self::build_with_value(path, request_data_value(request_data), timestamp)
    }

    pub fn build_with_value(
        path: &str,
        data: Value,
        timestamp: f64,
    ) -> Result<Self, NativeRuntimeError> {
        let path_hash = truncated_sha256(path.as_bytes());
        let value = Value::Array(vec![
            Value::F64(timestamp),
            Value::Binary(path_hash.to_vec()),
            data,
        ]);
        let packed = pack_msgpack_value(&value)?;
        let request_id = truncated_sha256(&packed);
        Ok(Self {
            path: path.into(),
            path_hash,
            request_id,
            packed,
        })
    }

    pub fn requires_request_resource(&self) -> bool {
        self.packed.len() > rns_transport::packet::PACKET_MDU
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLinkResponseFrame {
    pub request_id: [u8; 16],
    pub body: Vec<u8>,
}

impl NativeLinkResponseFrame {
    pub fn parse(bytes: &[u8]) -> Result<Self, NativeRuntimeError> {
        let value = unpack_msgpack_value(bytes)?;
        let Value::Array(items) = value else {
            return Err(NativeRuntimeError::InvalidResponse(
                "Link.request response was not a msgpack array".into(),
            ));
        };
        if items.len() < 2 {
            return Err(NativeRuntimeError::InvalidResponse(
                "Link.request response array was too short".into(),
            ));
        }
        let request_id = match &items[0] {
            Value::Binary(bytes) if bytes.len() == 16 => {
                let mut id = [0u8; 16];
                id.copy_from_slice(bytes);
                id
            }
            _ => {
                return Err(NativeRuntimeError::InvalidResponse(
                    "Link.request response request_id was invalid".into(),
                ));
            }
        };
        let body = response_value_to_body(&items[1])?;
        Ok(Self { request_id, body })
    }

    pub fn parse_matching_response_resource(
        complete: &ResourceComplete,
        request_id: &[u8; 16],
    ) -> Result<Option<Self>, NativeRuntimeError> {
        if !complete.is_response || complete.request_id.as_deref() != Some(request_id) {
            return Ok(None);
        }
        Self::parse(&complete.data).map(Some)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeFetchPlan {
    pub request: NativePageRequest,
    pub timeout: Duration,
    pub expects_micron: bool,
}

impl NativeFetchPlan {
    pub fn new(
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        timeout_secs: u64,
    ) -> Result<Self, NativeRuntimeError> {
        let request = NativePageRequest::from_url(url, request_data)?;
        let expects_micron = request.path.ends_with(".mu") || request.path.ends_with('/');
        Ok(Self {
            request,
            timeout: Duration::from_secs(timeout_secs.max(1)),
            expects_micron,
        })
    }
}

pub fn build_native_link_request_frame(
    prepared: &NativePreparedPageLink,
    timestamp: f64,
) -> Result<NativeLinkRequestFrame, NativeRuntimeError> {
    NativeLinkRequestFrame::build(&prepared.path, &prepared.request_data, timestamp)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePageResponse {
    pub body: Vec<u8>,
    pub content_type: Option<String>,
}

impl NativePageResponse {
    pub fn into_browser_page(
        self,
        plan: &NativeFetchPlan,
    ) -> Result<BrowserPage, NativeRuntimeError> {
        let markup = String::from_utf8(self.body)
            .map_err(|_| NativeRuntimeError::InvalidResponse("page body was not UTF-8".into()))?;
        let mut metadata = BTreeMap::new();
        if let Some(content_type) = self.content_type {
            metadata.insert(
                "content_type".into(),
                serde_json::Value::String(content_type),
            );
        }
        metadata.insert(
            "native_destination".into(),
            serde_json::Value::String(plan.request.destination_hash.to_hex_string()),
        );
        metadata.insert(
            "native_path".into(),
            serde_json::Value::String(plan.request.path.clone()),
        );
        Ok(BrowserPage {
            url: plan.request.url.clone(),
            title: title_from_markup(&markup),
            markup,
            source: PageSource::Network,
            metadata,
            request_data: plan.request.request_data.clone(),
        })
    }
}

#[async_trait]
pub trait NativePageTransportClient: Send + Sync {
    async fn fetch_page(
        &self,
        plan: &NativeFetchPlan,
        context: Option<&NativePageFetchContext>,
        cancel: CancellationToken,
    ) -> AppResult<NativePageResponse>;
}

#[derive(Clone, Debug, Default)]
pub struct ReticulumPageTransportClient;

#[async_trait]
impl NativePageTransportClient for ReticulumPageTransportClient {
    async fn fetch_page(
        &self,
        plan: &NativeFetchPlan,
        context: Option<&NativePageFetchContext>,
        cancel: CancellationToken,
    ) -> AppResult<NativePageResponse> {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        let exchange = NativePageExchangePlan::from_fetch_plan(plan);
        if !exchange.is_python_compatible_shape() {
            return Err(AppError::from(NativeRuntimeError::Unsupported(
                "native Reticulum page request shape is not Python-compatible",
            )));
        }
        if let Some(context) = context {
            let prepared = prepare_nomadnet_page_link(plan, context, cancel.clone()).await?;
            let request_frame = build_native_link_request_frame(&prepared, unix_timestamp())?;
            let adapter = Reticulum06LinkRequestAdapter;
            let response = adapter
                .send_request(&prepared, &request_frame, exchange.timeout, cancel)
                .await?;
            return Ok(NativePageResponse {
                body: response.body,
                content_type: Some("text/x-micron".into()),
            });
        }
        Err(AppError::from(NativeRuntimeError::Unsupported(
            "native Reticulum page transport needs a verified Link.request response API",
        )))
    }
}

pub fn native_transport_api_available() -> bool {
    let _ = std::mem::size_of::<reticulum_rs::runtime::TransportConfig>();
    let _ = std::mem::size_of::<reticulum_rs::runtime::Transport>();
    true
}

pub fn native_page_exchange_api_available() -> bool {
    let _ = std::mem::size_of::<rns_transport::destination::DestinationDesc>();
    let _ = std::mem::size_of::<rns_transport::destination::DestinationName>();
    let _ = std::mem::size_of::<rns_transport::destination::link::LinkEventData>();
    let _ = std::mem::size_of::<rns_transport::transport::TransportChannel>();
    let _ = std::mem::size_of::<rns_transport::resource::ResourceEvent>();
    let _ = std::mem::size_of::<rns_transport::Packet>();
    true
}

fn truncated_sha256(bytes: &[u8]) -> [u8; 16] {
    let digest = Sha256::digest(bytes);
    let mut truncated = [0u8; 16];
    truncated.copy_from_slice(&digest[..16]);
    truncated
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn transport_path_status_summary(status: &rns_transport::transport::TransportPathStatus) -> String {
    if !status.path_found {
        return format!("{}:missing", status.destination);
    }
    format!(
        "{}:next_hop={}:iface={}:hops={}",
        status.destination,
        status
            .next_hop
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".into()),
        status
            .interface
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".into()),
        status
            .hops
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".into())
    )
}

fn request_data_value(request_data: &BTreeMap<String, String>) -> Value {
    if request_data.is_empty() {
        return Value::Nil;
    }
    Value::Map(
        request_data
            .iter()
            .map(|(key, value)| {
                (
                    Value::String(key.as_str().into()),
                    Value::String(value.as_str().into()),
                )
            })
            .collect(),
    )
}

fn pack_msgpack_value(value: &Value) -> Result<Vec<u8>, NativeRuntimeError> {
    let mut packed = Vec::new();
    rmpv::encode::write_value(&mut packed, value).map_err(|_| {
        NativeRuntimeError::InvalidResponse("failed to encode Link.request msgpack".into())
    })?;
    Ok(packed)
}

fn unpack_msgpack_value(bytes: &[u8]) -> Result<Value, NativeRuntimeError> {
    let mut cursor = std::io::Cursor::new(bytes);
    rmpv::decode::read_value(&mut cursor).map_err(|_| {
        NativeRuntimeError::InvalidResponse("failed to decode Link.request msgpack".into())
    })
}

fn response_value_to_body(value: &Value) -> Result<Vec<u8>, NativeRuntimeError> {
    match value {
        Value::Binary(bytes) => Ok(bytes.clone()),
        Value::String(text) => text
            .as_str()
            .map(|text| text.as_bytes().to_vec())
            .ok_or_else(|| {
                NativeRuntimeError::InvalidResponse(
                    "Link.request response string was not valid UTF-8".into(),
                )
            }),
        Value::Nil => Ok(Vec::new()),
        other => pack_msgpack_value(other),
    }
}

pub fn nomadnet_destination_desc(
    destination_hash: AddressHash,
    identity: rns_transport::identity::Identity,
) -> Result<DestinationDesc, NativeRuntimeError> {
    single_output_destination_desc(
        destination_hash,
        identity,
        NOMADNET_APP_NAME,
        NOMADNET_NODE_ASPECT,
    )
}

pub fn single_output_destination_desc(
    destination_hash: AddressHash,
    identity: rns_transport::identity::Identity,
    app_name: &str,
    aspect: &str,
) -> Result<DestinationDesc, NativeRuntimeError> {
    let destination =
        SingleOutputDestination::new(identity, DestinationName::new(app_name, aspect));
    if destination.desc.address_hash != destination_hash {
        return Err(NativeRuntimeError::PathUnavailable(
            destination_hash.to_hex_string(),
        ));
    }
    Ok(destination.desc)
}

pub fn build_nomadnet_link_identify_payload(
    link_id: AddressHash,
    identity: &rns_transport::identity::PrivateIdentity,
) -> Vec<u8> {
    let public_identity = identity.as_identity();
    let mut public_key = Vec::with_capacity(
        public_identity.public_key_bytes().len() + public_identity.verifying_key_bytes().len(),
    );
    public_key.extend_from_slice(public_identity.public_key_bytes());
    public_key.extend_from_slice(public_identity.verifying_key_bytes());

    let mut signed_data = Vec::with_capacity(link_id.len() + public_key.len());
    signed_data.extend_from_slice(link_id.as_slice());
    signed_data.extend_from_slice(&public_key);

    let signature = identity.sign(&signed_data);
    let mut proof_data = Vec::with_capacity(public_key.len() + signature.to_bytes().len());
    proof_data.extend_from_slice(&public_key);
    proof_data.extend_from_slice(&signature.to_bytes());
    proof_data
}

pub(crate) async fn send_reticulum_link_identify(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    link: &Arc<Mutex<Link>>,
    identity: &rns_transport::identity::PrivateIdentity,
    destination_hash: AddressHash,
) -> AppResult<()> {
    let (link_id, ingress_iface, packet) = {
        let link = link.lock().await;
        let link_id = *link.id();
        let ingress_iface = link.ingress_iface().ok_or_else(|| {
            AppError::from(NativeRuntimeError::Native(
                "native Reticulum page link has no bound ingress interface for LinkIdentify".into(),
            ))
        })?;
        let proof_data = build_nomadnet_link_identify_payload(link_id, identity);
        let mut packet = link.data_packet(&proof_data).map_err(|error| {
            AppError::from(NativeRuntimeError::Native(format!(
                "native Reticulum failed to build LinkIdentify packet: {error:?}"
            )))
        })?;
        packet.context = PacketContext::LinkIdentify;
        (link_id, ingress_iface, packet)
    };

    transport.send_direct(ingress_iface, packet).await;
    tracing::info!(
        destination = %destination_hash,
        link_id = %link_id,
        ingress_iface = %ingress_iface,
        identity = %identity.address_hash(),
        "native Reticulum 0.6 sent LinkIdentify on active link"
    );
    Ok(())
}

pub async fn prepare_nomadnet_page_link(
    plan: &NativeFetchPlan,
    context: &NativePageFetchContext,
    cancel: CancellationToken,
) -> AppResult<NativePreparedPageLink> {
    let identity = wait_for_destination_identity(plan, context, cancel.clone())
        .await?
        .ok_or_else(|| {
            AppError::from(NativeRuntimeError::PathUnavailable(
                plan.request.destination_hash.to_hex_string(),
            ))
        })?;
    let destination = nomadnet_destination_desc(plan.request.destination_hash, identity)
        .map_err(AppError::from)?;
    let mut link_events = context.transport.out_link_events();
    let link = context.transport.link(destination).await;
    let link_id = *link.lock().await.id();

    if link.lock().await.status() != LinkStatus::Active {
        let deadline = tokio::time::Instant::now() + plan.timeout;
        loop {
            if cancel.is_cancelled() {
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            if link.lock().await.status() == LinkStatus::Active {
                break;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(AppError::from(NativeRuntimeError::Timeout(
                    "NomadNet link establishment".into(),
                )));
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            match tokio::time::timeout(wait, link_events.recv()).await {
                Ok(Ok(event))
                    if event.id == link_id && matches!(event.event, LinkEvent::Activated) =>
                {
                    break;
                }
                Ok(Ok(event))
                    if event.id == link_id && matches!(event.event, LinkEvent::Closed) =>
                {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum link closed during page fetch setup".into(),
                    )));
                }
                Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum link event stream closed".into(),
                    )));
                }
                Err(_) => {}
            }
        }
    }
    emit_clean_page_debug(
        context.event_tx.as_ref(),
        format!(
            "native Reticulum 0.6 clean page link active destination={} link_id={} path={} identify_on_connect={}",
            plan.request.destination_hash,
            link_id,
            plan.request.path,
            context.identify_on_connect
        ),
    );

    if context.identify_on_connect {
        match context.identify_identity.as_deref() {
            Some(identity) => {
                if let Err(error) = send_reticulum_link_identify(
                    &context.transport,
                    &link,
                    identity,
                    plan.request.destination_hash,
                )
                .await
                {
                    tracing::warn!(
                        destination = %plan.request.destination_hash,
                        link_id = %link_id,
                        error = %error,
                        "native Reticulum 0.6 page link is active but NomadNet identify-on-connect could not be sent"
                    );
                    emit_clean_page_debug(
                        context.event_tx.as_ref(),
                        format!(
                            "native Reticulum 0.6 clean page LinkIdentify failed destination={} link_id={} error={}",
                            plan.request.destination_hash, link_id, error
                        ),
                    );
                } else {
                    emit_clean_page_debug(
                        context.event_tx.as_ref(),
                        format!(
                            "native Reticulum 0.6 clean page LinkIdentify sent destination={} link_id={} identity={}",
                            plan.request.destination_hash,
                            link_id,
                            identity.address_hash()
                        ),
                    );
                }
            }
            None => {
                tracing::warn!(
                    destination = %plan.request.destination_hash,
                    link_id = %link_id,
                    "native Reticulum 0.6 page link is active but NomadNet identify-on-connect was skipped because the active local identity could not be loaded"
                );
                emit_clean_page_debug(
                    context.event_tx.as_ref(),
                    format!(
                        "native Reticulum 0.6 clean page LinkIdentify skipped destination={} link_id={} reason=identity_unavailable",
                        plan.request.destination_hash, link_id
                    ),
                );
            }
        }
    }

    Ok(NativePreparedPageLink {
        destination_hash: plan.request.destination_hash,
        link_id,
        path: plan.request.path.clone(),
        request_data: plan.request.request_data.clone().unwrap_or_default(),
        transport: Some(context.transport.clone()),
        link: Some(link),
        event_tx: context.event_tx.clone(),
    })
}

async fn wait_for_destination_identity(
    plan: &NativeFetchPlan,
    context: &NativePageFetchContext,
    cancel: CancellationToken,
) -> AppResult<Option<rns_transport::identity::Identity>> {
    if let Some(identity) = context
        .transport
        .destination_identity(&plan.request.destination_hash)
        .await
    {
        return Ok(Some(identity));
    }

    context
        .transport
        .request_path(&plan.request.destination_hash, None, None)
        .await;

    let deadline = tokio::time::Instant::now() + plan.timeout;
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }
        if let Some(identity) = context
            .transport
            .destination_identity(&plan.request.destination_hash)
            .await
        {
            return Ok(Some(identity));
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(100))).await;
    }
}

fn normalize_native_nomadnet_path(path: &str) -> String {
    // Directory browse already opens DEFAULT_PATH explicitly. This catches manual native
    // `hash:/` input without changing mock/offline URLs handled by the mock runtime.
    if path == "/" {
        DEFAULT_PATH.into()
    } else {
        path.into()
    }
}

fn redacted_destination_url(address: &BrowserAddress) -> String {
    format!("<destination>:{}", address.path)
}

fn title_from_markup(markup: &str) -> String {
    markup
        .lines()
        .find_map(|line| line.strip_prefix('>').map(str::trim))
        .filter(|title| !title.is_empty())
        .unwrap_or("OMEN Page")
        .to_string()
}

fn unix_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEST: &str = "00112233445566778899aabbccddeeff";

    #[test]
    fn native_page_request_parses_destination_path_and_request_data() {
        let mut request_data = BTreeMap::new();
        request_data.insert("field_name".into(), "omen".into());

        let request = NativePageRequest::from_url(
            &format!("{DEST}:/page/index.mu"),
            Some(request_data.clone()),
        )
        .expect("native request");

        assert_eq!(request.url, format!("{DEST}:/page/index.mu"));
        assert_eq!(request.destination_hash.to_hex_string(), DEST);
        assert_eq!(request.path, "/page/index.mu");
        assert_eq!(request.request_data, Some(request_data));
    }

    #[test]
    fn native_fetch_plan_sets_timeout_and_micron_expectation() {
        let plan = NativeFetchPlan::new(&format!("{DEST}:/"), None, 0).expect("plan");

        assert_eq!(plan.timeout, Duration::from_secs(1));
        assert_eq!(plan.request.url, format!("{DEST}:/page/index.mu"));
        assert_eq!(plan.request.path, "/page/index.mu");
        assert!(plan.expects_micron);
    }

    #[test]
    fn native_page_request_treats_root_as_nomadnet_index() {
        let request =
            NativePageRequest::from_url(&format!("{DEST}:/"), None).expect("native request");

        assert_eq!(request.url, format!("{DEST}:/page/index.mu"));
        assert_eq!(request.path, "/page/index.mu");
    }

    #[test]
    fn native_page_exchange_plan_preserves_python_link_request_shape() {
        let mut request_data = BTreeMap::new();
        request_data.insert("field_name".into(), "omen".into());
        request_data.insert("var_next".into(), "/next.mu".into());
        let fetch = NativeFetchPlan::new(
            &format!("{DEST}:/page/form.mu"),
            Some(request_data.clone()),
            30,
        )
        .expect("fetch plan");

        let exchange = NativePageExchangePlan::from_fetch_plan(&fetch);

        assert_eq!(exchange.destination_hash.to_hex_string(), DEST);
        assert_eq!(exchange.app_name, "nomadnetwork");
        assert_eq!(exchange.aspect, "node");
        assert_eq!(exchange.path, "/page/form.mu");
        assert_eq!(exchange.request_data, request_data);
        assert_eq!(exchange.timeout, Duration::from_secs(30));
        assert_eq!(
            exchange.preferred_primitive,
            NativePageExchangePrimitive::LinkRequestReceipt
        );
        assert!(exchange.is_python_compatible_shape());
    }

    #[test]
    fn native_link_request_frame_models_lower_level_request_receipt_shape() {
        let mut request_data = BTreeMap::new();
        request_data.insert("field_name".into(), "omen".into());
        request_data.insert("var_next".into(), "/next.mu".into());

        let frame =
            NativeLinkRequestFrame::build("/page/form.mu", &request_data, 1234.5).expect("frame");
        let value = unpack_msgpack_value(&frame.packed).expect("decode frame");
        let Value::Array(items) = value else {
            panic!("request must encode as array");
        };

        assert_eq!(items.len(), 3);
        assert_eq!(items[0], Value::F64(1234.5));
        assert_eq!(items[1], Value::Binary(frame.path_hash.to_vec()));
        assert_eq!(frame.path_hash, truncated_sha256(b"/page/form.mu"));
        assert_eq!(frame.request_id, truncated_sha256(&frame.packed));
        assert!(matches!(items[2], Value::Map(_)));
    }

    #[test]
    fn native_link_request_frame_uses_nil_for_empty_data() {
        let frame = NativeLinkRequestFrame::build("/", &BTreeMap::new(), 1.0).expect("frame");
        let value = unpack_msgpack_value(&frame.packed).expect("decode frame");
        let Value::Array(items) = value else {
            panic!("request must encode as array");
        };

        assert_eq!(items[2], Value::Nil);
    }

    #[test]
    fn native_link_response_frame_extracts_binary_and_string_bodies() {
        let request_id = [0x42; 16];
        let binary = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(request_id.to_vec()),
            Value::Binary(b">Page\nBody".to_vec()),
        ]))
        .expect("pack binary response");
        let string = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(request_id.to_vec()),
            Value::String("Text body".into()),
        ]))
        .expect("pack string response");

        let binary = NativeLinkResponseFrame::parse(&binary).expect("binary response");
        let string = NativeLinkResponseFrame::parse(&string).expect("string response");

        assert_eq!(binary.request_id, request_id);
        assert_eq!(binary.body, b">Page\nBody");
        assert_eq!(string.body, b"Text body");
    }

    #[test]
    fn native_link_response_frame_matches_response_resources_by_request_id() {
        let request_id = [0x42; 16];
        let other_request_id = [0x24; 16];
        let response_data = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(request_id.to_vec()),
            Value::Binary(b">Resource Page\nBody".to_vec()),
        ]))
        .expect("pack resource response");
        let response = ResourceComplete {
            data: response_data.clone(),
            metadata: None,
            request_id: Some(request_id.to_vec()),
            is_request: false,
            is_response: true,
        };
        let wrong_id = ResourceComplete {
            request_id: Some(other_request_id.to_vec()),
            ..response.clone()
        };
        let not_response = ResourceComplete {
            is_response: false,
            ..response.clone()
        };
        let malformed = ResourceComplete {
            data: b"not-msgpack".to_vec(),
            ..response.clone()
        };

        let parsed =
            NativeLinkResponseFrame::parse_matching_response_resource(&response, &request_id)
                .expect("matching response parses")
                .expect("response matched");

        assert_eq!(parsed.request_id, request_id);
        assert_eq!(parsed.body, b">Resource Page\nBody");
        assert_eq!(
            NativeLinkResponseFrame::parse_matching_response_resource(&wrong_id, &request_id)
                .expect("wrong id ignored"),
            None
        );
        assert_eq!(
            NativeLinkResponseFrame::parse_matching_response_resource(&not_response, &request_id)
                .expect("non-response ignored"),
            None
        );
        assert!(
            NativeLinkResponseFrame::parse_matching_response_resource(&malformed, &request_id)
                .is_err()
        );
    }

    #[test]
    fn build_native_link_request_frame_uses_prepared_link_handoff() {
        let prepared = NativePreparedPageLink {
            destination_hash: AddressHash::new_empty(),
            link_id: AddressHash::new_empty(),
            path: "/".into(),
            request_data: BTreeMap::from([("field_name".into(), "omen".into())]),
            transport: None,
            link: None,
            event_tx: None,
        };

        let frame = build_native_link_request_frame(&prepared, 2.0).expect("frame");

        assert_eq!(frame.path, "/");
        assert_eq!(frame.path_hash, truncated_sha256(b"/"));
    }

    #[test]
    fn native_link_request_adapter_plan_names_verification_target() {
        let plan = NativeLinkRequestAdapterPlan::reticulum06_available();

        assert_eq!(plan.boundary, "reticulum-rs-transport Link request adapter");
        assert_eq!(plan.request_context, PacketContext::Request);
        assert_eq!(plan.response_context, PacketContext::Response);
        assert_eq!(
            plan.dispatch_status,
            NativeRuntimeCapabilityState::Available
        );
        assert!(plan.is_ready());
        assert!(plan.next_step.contains("request-resource"));
        assert!(plan.next_step.contains("direct request-context"));
    }

    #[tokio::test]
    async fn missing_reticulum06_link_request_adapter_fails_before_dispatch() {
        let adapter = MissingReticulum06LinkRequestAdapter;
        let prepared = NativePreparedPageLink {
            destination_hash: AddressHash::new_empty(),
            link_id: AddressHash::new_empty(),
            path: "/".into(),
            request_data: BTreeMap::new(),
            transport: None,
            link: None,
            event_tx: None,
        };
        let frame = build_native_link_request_frame(&prepared, 2.0).expect("frame");

        let error = adapter
            .send_request(
                &prepared,
                &frame,
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect_err("missing adapter reports unsupported");

        assert!(format!("{error}").contains("Link.request adapter is not wired"));
    }

    #[tokio::test]
    async fn reticulum06_link_request_adapter_requires_live_handles() {
        let adapter = Reticulum06LinkRequestAdapter;
        let prepared = NativePreparedPageLink {
            destination_hash: AddressHash::new_empty(),
            link_id: AddressHash::new_empty(),
            path: "/".into(),
            request_data: BTreeMap::new(),
            transport: None,
            link: None,
            event_tx: None,
        };
        let frame = build_native_link_request_frame(&prepared, 2.0).expect("frame");

        let error = adapter
            .send_request(
                &prepared,
                &frame,
                Duration::from_secs(5),
                CancellationToken::new(),
            )
            .await
            .expect_err("adapter needs prepared live transport handles");

        assert!(format!("{error}").contains("transport handle"));
    }

    #[test]
    fn nomadnet_destination_desc_requires_matching_node_destination_hash() {
        let private_identity =
            rns_transport::identity::PrivateIdentity::new_from_rand(rand_core::OsRng);
        let identity = *private_identity.as_identity();
        let destination = SingleOutputDestination::new(
            identity,
            DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
        );

        let desc =
            nomadnet_destination_desc(destination.desc.address_hash, identity).expect("desc");
        let error = match nomadnet_destination_desc(AddressHash::new_empty(), identity) {
            Ok(_) => panic!("mismatched hash should be rejected"),
            Err(error) => error,
        };

        assert_eq!(desc.address_hash, destination.desc.address_hash);
        assert!(matches!(error, NativeRuntimeError::PathUnavailable(_)));
    }

    #[test]
    fn native_page_request_rejects_clearweb_and_named_destinations() {
        let clearweb = NativePageRequest::from_url("https://example.com", None)
            .expect_err("clearweb rejected");
        let named = NativePageRequest::from_url("mock.node:/", None)
            .expect_err("named destination rejected");

        assert!(matches!(clearweb, NativeRuntimeError::InvalidAddress(_)));
        assert!(matches!(named, NativeRuntimeError::InvalidAddress(_)));
        assert!(!format!("{named:?}").contains("mock.node"));
    }

    #[test]
    fn native_page_response_maps_to_browser_page_without_ui_types() {
        let plan = NativeFetchPlan::new(&format!("{DEST}:/page/index.mu"), None, 5).expect("plan");
        let response = NativePageResponse {
            body: b">Native Page\nHello".to_vec(),
            content_type: Some("text/x-micron".into()),
        };

        let page = response.into_browser_page(&plan).expect("browser page");

        assert_eq!(page.url, format!("{DEST}:/page/index.mu"));
        assert_eq!(page.title, "Native Page");
        assert_eq!(page.source, PageSource::Network);
        assert_eq!(
            page.metadata
                .get("native_destination")
                .and_then(serde_json::Value::as_str),
            Some(DEST)
        );
    }

    #[test]
    fn native_page_response_rejects_non_utf8_body() {
        let plan = NativeFetchPlan::new(&format!("{DEST}:/"), None, 5).expect("plan");
        let error = NativePageResponse {
            body: vec![0xff, 0xfe],
            content_type: None,
        }
        .into_browser_page(&plan)
        .expect_err("invalid response");

        assert!(matches!(error, NativeRuntimeError::InvalidResponse(_)));
    }

    #[test]
    fn reticulum_transport_api_is_exposed_for_future_client_wiring() {
        assert!(native_transport_api_available());
    }

    #[test]
    fn reticulum_page_exchange_primitives_are_exposed_for_future_client_wiring() {
        assert!(native_page_exchange_api_available());
    }

    #[test]
    fn reticulum06_request_response_probe_marks_receive_side_available() {
        let probe = native_reticulum06_request_response_probe();

        assert!(probe.request_context_available);
        assert!(probe.response_context_available);
        assert!(probe.received_data_request_id_available);
        assert!(probe.link_data_packet_available);
        assert!(probe.link_channel_packet_available);
        assert!(probe.public_bound_link_data_send_available);
        assert!(probe.public_bound_link_channel_send_available);
        assert!(!probe.public_bound_request_context_send_available);
        assert!(probe.public_bound_link_identify_send_available);
        assert!(probe.request_resource_send_available);
        assert!(probe.resource_response_events_available);
        assert!(probe.public_packet_context_mutation_available);
        assert!(probe.public_transport_packet_dispatch_available);
        assert!(!probe.high_level_link_request_send_available);
        assert!(probe.recommended_adapter.contains("send LinkIdentify"));
    }

    #[test]
    fn nomadnet_link_identify_payload_matches_python_shape() {
        let identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-link-identify-test",
        );
        let link_id = AddressHash::new([0x42; 16]);
        let payload = build_nomadnet_link_identify_payload(link_id, &identity);
        let public_identity = identity.as_identity();
        let public_key_len =
            public_identity.public_key_bytes().len() + public_identity.verifying_key_bytes().len();

        assert_eq!(payload.len(), public_key_len + 64);
        assert_eq!(
            &payload[..public_identity.public_key_bytes().len()],
            public_identity.public_key_bytes()
        );
        assert_eq!(
            &payload[public_identity.public_key_bytes().len()..public_key_len],
            public_identity.verifying_key_bytes()
        );

        let mut signed_data = Vec::new();
        signed_data.extend_from_slice(link_id.as_slice());
        signed_data.extend_from_slice(&payload[..public_key_len]);
        let expected_signature = identity.sign(&signed_data);
        assert_eq!(&payload[public_key_len..], expected_signature.to_bytes());
    }

    #[test]
    fn native_link_request_frame_identifies_resource_sized_requests() {
        let request_data = BTreeMap::from([("body".into(), "x".repeat(2048))]);
        let frame =
            NativeLinkRequestFrame::build("/page/post.mu", &request_data, 2.0).expect("frame");

        assert!(frame.requires_request_resource());
        assert_eq!(frame.request_id, truncated_sha256(&frame.packed));
    }

    #[cfg(feature = "native-lxmf-sdk")]
    #[test]
    fn native_lxmf_sdk_report_covers_messages_not_page_fetch() {
        let report = native_lxmf_sdk_capability_report();

        assert_eq!(report.sdk_crate, "lxmf-sdk 0.6");
        assert!(report.config_available);
        assert!(report.rpc_backend_config_available);
        assert!(report.send_request_available);
        assert!(report.direct_delivery_field_available);
        assert!(report.propagation_retry_field_available);
        assert!(report.stamp_cost_field_available);
        assert!(report.ticket_field_available);
        assert!(report.rpc_delivery_options_available);
        assert!(report.rpc_ticket_record_available);
        assert!(!report.page_fetch_adapter_available);
        assert!(report.recommended_use.contains("LXMF"));
        assert!(report.recommended_use.contains("Link.request"));
    }

    #[test]
    fn reticulum06_capability_report_marks_link_request_resource_verification() {
        let report = native_reticulum06_capability_report();

        assert_eq!(report.stack, "reticulum-rs 0.6");
        assert_eq!(report.transport_crate, "reticulum-rs-transport 0.6");
        assert_eq!(report.lxmf_crate, "lxmf 0.6");
        assert!(report.has_blockers());
        assert_eq!(
            report.capability_state("transport-runtime"),
            Some(NativeRuntimeCapabilityState::Available)
        );
        assert_eq!(
            report.capability_state("link-request-receipt"),
            Some(NativeRuntimeCapabilityState::Available)
        );
        assert_eq!(
            report.capability_state("bound-link-data"),
            Some(NativeRuntimeCapabilityState::NeedsVerification)
        );
        assert_eq!(
            report.capability_state("link-identify"),
            Some(NativeRuntimeCapabilityState::Available)
        );
        assert_eq!(
            report.capability_state("resource-transfer"),
            Some(NativeRuntimeCapabilityState::Available)
        );
        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("LinkIdentify")));
    }

    #[test]
    fn reticulum06_capability_report_tracks_remaining_clean_stack_work() {
        let report = native_reticulum06_capability_report();

        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("OMENchat")));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("direct Link.request")));
        assert!(
            report.recommended_next_step.contains("request-resource")
                || report
                    .recommended_next_step
                    .contains("PacketContext::Request")
        );
    }
}
