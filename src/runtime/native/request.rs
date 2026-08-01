use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rmpv::Value;
use rns_transport::destination::link::{Link, LinkEvent, LinkStatus};
use rns_transport::destination::{DestinationDesc, DestinationName, SingleOutputDestination};
use rns_transport::hash::{AddressHash, Hash};
use rns_transport::resource::{ResourceComplete, ResourceEventKind};
use rns_transport::{Packet, PacketContext};
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
pub const MAX_NOMADNET_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_NOMADNET_RESPONSE_CONTAINER_ITEMS: usize = 256;
const MAX_NOMADNET_RESPONSE_TOTAL_VALUES: usize = 512;
const MAX_NOMADNET_RESPONSE_DEPTH: usize = 8;
const NOMADNET_PAGE_LINK_GATE_STRIPES: usize = 32;

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
pub struct NativeReticulum09CapabilityReport {
    pub stack: &'static str,
    pub transport_crate: &'static str,
    pub lxmf_crate: &'static str,
    pub capabilities: Vec<NativeRuntimeCapability>,
    pub blockers: Vec<&'static str>,
    pub recommended_next_step: &'static str,
}

impl NativeReticulum09CapabilityReport {
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
pub struct NativeReticulum09RequestResponseProbe {
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

pub fn native_reticulum09_request_response_probe() -> NativeReticulum09RequestResponseProbe {
    let _ = rns_transport::PacketContext::Request;
    let _ = rns_transport::PacketContext::Response;
    let _ = rns_transport::PacketContext::LinkIdentify;
    let _ = std::mem::size_of::<rns_transport::transport::ReceivedData>();
    let _ = std::mem::size_of::<rns_transport::destination::link::LinkPayload>();
    let _ = std::mem::size_of::<rns_transport::resource::ResourceEvent>();
    let _ = rns_transport::transport::Transport::send_to_out_links;
    let _ = rns_transport::transport::Transport::send_channel_message;
    let _ = rns_transport::transport::Transport::send_direct;

    NativeReticulum09RequestResponseProbe {
        request_context_available: true,
        response_context_available: true,
        received_data_request_id_available: true,
        link_data_packet_available: true,
        link_channel_packet_available: true,
        public_bound_link_data_send_available: true,
        public_bound_link_channel_send_available: true,
        public_bound_request_context_send_available: true,
        public_bound_link_identify_send_available: true,
        request_resource_send_available: true,
        resource_response_events_available: true,
        public_packet_context_mutation_available: true,
        public_transport_packet_dispatch_available: true,
        high_level_link_request_send_available: false,
        recommended_adapter:
            "use the current-Python-verified direct request-context packet for small NomadNet requests and retain request-resource for oversized requests",
        note: "reticulum-rs-transport 0.9 exposes request/response contexts, inbound request IDs, public link packet construction, packet context mutation, send_direct, and request/response resource helpers; OMEN's adapter now matches Python Link.request packet selection for small requests while retaining bounded request resources above the packet MDU",
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
    let config = lxmf_sdk::SdkConfig::desktop_full_default();
    let rpc_backend_config_available = config.rpc_backend.is_some();
    let start = lxmf_sdk::StartRequest::new(config);
    let send = lxmf_sdk::SendRequest::new(
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
    let _ = std::mem::size_of::<lxmf_sdk::EventBatch>();
    let _ = std::mem::size_of::<lxmf_sdk::RuntimeSnapshot>();

    NativeLxmfSdkCapabilityReport {
        sdk_crate: "lxmf-sdk 0.9",
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
    pub fn reticulum09_available() -> Self {
        Self {
            boundary: "reticulum-rs-transport Link request adapter",
            request_context: PacketContext::Request,
            response_context: PacketContext::Response,
            request_id_source: "response ReceivedData.request_id or link payload request_id",
            dispatch_status: NativeRuntimeCapabilityState::Available,
            next_step: "retain direct packets for small requests and request resources above the packet MDU; keep the exact current-Python lane while no pinned NomadNet reference exists",
        }
    }

    pub fn reticulum09_missing() -> Self {
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
pub struct MissingReticulum09LinkRequestAdapter;

#[async_trait]
impl NativeLinkRequestAdapter for MissingReticulum09LinkRequestAdapter {
    fn adapter_name(&self) -> &'static str {
        "missing-reticulum09-link-request"
    }

    fn plan(&self) -> NativeLinkRequestAdapterPlan {
        NativeLinkRequestAdapterPlan::reticulum09_missing()
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
            "reticulum-rs 0.9 Link.request adapter is not wired; use the verified resource request path or add a tested reticulumd/RPC adapter",
        )))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Reticulum09LinkRequestAdapter;

#[async_trait]
impl NativeLinkRequestAdapter for Reticulum09LinkRequestAdapter {
    fn adapter_name(&self) -> &'static str {
        "reticulum09-link-request"
    }

    fn plan(&self) -> NativeLinkRequestAdapterPlan {
        NativeLinkRequestAdapterPlan::reticulum09_available()
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

        if frame.requires_request_resource() {
            self.send_request_resource(prepared, frame, timeout, cancel)
                .await
        } else {
            self.send_direct_request(prepared, frame, timeout, cancel)
                .await
        }
    }
}

impl Reticulum09LinkRequestAdapter {
    async fn send_direct_request(
        &self,
        prepared: &NativePreparedPageLink,
        frame: &NativeLinkRequestFrame,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> AppResult<NativeLinkResponseFrame> {
        let Some(transport) = prepared.transport.as_ref() else {
            return Err(AppError::from(NativeRuntimeError::Unsupported(
                "native Reticulum direct Link.request adapter has no transport handle",
            )));
        };
        let Some(link) = prepared.link.as_ref() else {
            return Err(AppError::from(NativeRuntimeError::Unsupported(
                "native Reticulum direct Link.request adapter has no link handle",
            )));
        };
        if cancel.is_cancelled() {
            return Err(AppError::from(NativeRuntimeError::Cancelled));
        }

        let candidate = {
            let link = link.lock().await;
            build_reticulum09_direct_request_packet(&link, frame).map_err(AppError::from)?
        };
        let mut received_data = transport.received_data_events();
        let mut resource_events = transport.resource_events();
        transport
            .send_direct(candidate.ingress_iface, candidate.packet)
            .await;
        emit_clean_page_debug(
            prepared.event_tx.as_ref(),
            format!(
                "native Reticulum 0.9 direct page request sent destination={} link_id={} path={} request_id={} bytes={} link_iface={}",
                prepared.destination_hash,
                prepared.link_id,
                prepared.path,
                hex_bytes(&candidate.request_id),
                frame.packed.len(),
                candidate.ingress_iface
            ),
        );

        let deadline = tokio::time::Instant::now() + timeout;
        let mut active_response_resource = None;
        loop {
            if cancel.is_cancelled() {
                if let Some(response_resource_hash) = active_response_resource {
                    cancel_clean_page_response_resource(
                        transport,
                        prepared,
                        response_resource_hash,
                        "browser request cancelled",
                    )
                    .await;
                }
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                if let Some(response_resource_hash) = active_response_resource {
                    cancel_clean_page_response_resource(
                        transport,
                        prepared,
                        response_resource_hash,
                        "NomadNet response timeout",
                    )
                    .await;
                }
                return Err(AppError::from(NativeRuntimeError::Timeout(
                    "NomadNet direct request response".into(),
                )));
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            tokio::select! {
                direct = received_data.recv() => match direct {
                Ok(data)
                    if data.destination == prepared.link_id
                        && data.context == Some(PacketContext::Response) =>
                {
                    if let Some(response) = NativeLinkResponseFrame::parse_matching(
                        data.data.as_slice(),
                        &candidate.request_id,
                    )
                    .map_err(AppError::from)?
                    {
                        emit_clean_page_debug(
                            prepared.event_tx.as_ref(),
                            format!(
                                "native Reticulum 0.9 direct page response received destination={} link_id={} path={} request_id={} bytes={}",
                                prepared.destination_hash,
                                prepared.link_id,
                                prepared.path,
                                hex_bytes(&candidate.request_id),
                                response.body.len()
                            ),
                        );
                        return Ok(response);
                    }
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum direct response stream closed".into(),
                    )));
                }
                },
                resource = resource_events.recv() => match resource {
                    Ok(event) if event.link_id == prepared.link_id => match event.kind {
                        ResourceEventKind::Complete(complete) => {
                            if let Some(response) =
                                NativeLinkResponseFrame::parse_matching_response_resource(
                                    &complete,
                                    &candidate.request_id,
                                )
                                .map_err(AppError::from)?
                            {
                                emit_clean_page_resource_lifecycle(
                                    prepared.event_tx.as_ref(),
                                    event.hash.to_string(),
                                    ResourceLifecycleState::Complete,
                                    Some(complete.data.len() as u64),
                                    None,
                                    "inbound",
                                    prepared.operation_id.as_deref(),
                                );
                                emit_clean_page_debug(
                                    prepared.event_tx.as_ref(),
                                    format!(
                                        "native Reticulum 0.9 direct page request received response-resource destination={} link_id={} path={} request_id={} response_resource={} bytes={}",
                                        prepared.destination_hash,
                                        prepared.link_id,
                                        prepared.path,
                                        hex_bytes(&candidate.request_id),
                                        event.hash,
                                        complete.data.len()
                                    ),
                                );
                                return Ok(response);
                            }
                        }
                        ResourceEventKind::Progress(progress) => {
                            active_response_resource = Some(event.hash);
                            emit_clean_page_resource_progress(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                progress.received_bytes,
                                progress.total_bytes,
                                prepared.operation_id.as_deref(),
                            );
                        }
                        ResourceEventKind::InboundFailed(failure) => {
                            emit_clean_page_resource_lifecycle(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                ResourceLifecycleState::Failed,
                                None,
                                Some(failure.reason),
                                "inbound",
                                prepared.operation_id.as_deref(),
                            );
                        }
                        _ => {}
                    },
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err(AppError::from(NativeRuntimeError::Native(
                            "native Reticulum resource response stream closed".into(),
                        )));
                    }
                },
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }

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
        let mut received_data = transport.received_data_events();
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
                    "native Reticulum 0.9 request-resource send failed: {error:?}"
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
            "native Reticulum 0.9 Link.request sent as request resource"
        );
        emit_clean_page_debug(
            prepared.event_tx.as_ref(),
            format!(
                "native Reticulum 0.9 clean page request-resource sent destination={} link_id={} path={} request_id={} request_resource={} bytes={} link_iface={:?}",
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
        let mut active_response_resource = None;
        let mut last_error = String::from("none");
        loop {
            if cancel.is_cancelled() {
                cancel_clean_page_request_resource(
                    transport,
                    prepared,
                    request_resource_hash,
                    "browser request cancelled",
                )
                .await;
                if let Some(response_resource_hash) = active_response_resource {
                    cancel_clean_page_response_resource(
                        transport,
                        prepared,
                        response_resource_hash,
                        "browser request cancelled",
                    )
                    .await;
                }
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                cancel_clean_page_request_resource(
                    transport,
                    prepared,
                    request_resource_hash,
                    "NomadNet response timeout",
                )
                .await;
                if let Some(response_resource_hash) = active_response_resource {
                    cancel_clean_page_response_resource(
                        transport,
                        prepared,
                        response_resource_hash,
                        "NomadNet response timeout",
                    )
                    .await;
                }
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
            tokio::select! {
                resource = resource_events.recv() => match resource {
                Ok(event) if event.link_id == prepared.link_id => {
                    target_events += 1;
                    match event.kind {
                        ResourceEventKind::Complete(complete) => {
                            match NativeLinkResponseFrame::parse_matching_response_resource(
                                &complete,
                                &frame.request_id,
                            ) {
                                Ok(Some(response)) => {
                                    if !outbound_complete {
                                        emit_clean_page_resource_lifecycle(
                                            prepared.event_tx.as_ref(),
                                            request_resource_hash.to_string(),
                                            ResourceLifecycleState::Complete,
                                            Some(frame.packed.len() as u64),
                                            None,
                                            "outbound",
                                            prepared.operation_id.as_deref(),
                                        );
                                    }
                                    emit_clean_page_resource_lifecycle(
                                        prepared.event_tx.as_ref(),
                                        event.hash.to_string(),
                                        ResourceLifecycleState::Complete,
                                        Some(complete.data.len() as u64),
                                        None,
                                        "inbound",
                                        prepared.operation_id.as_deref(),
                                    );
                                    tracing::debug!(
                                        adapter = self.adapter_name(),
                                        destination = %prepared.destination_hash,
                                        link_id = %prepared.link_id,
                                        path = %prepared.path,
                                        request_id = %hex_bytes(&frame.request_id),
                                        response_resource_hash = %event.hash,
                                        bytes = complete.data.len(),
                                        metadata = complete.metadata.as_ref().map(|value| value.len()),
                                        "native Reticulum 0.9 Link.request response resource received"
                                    );
                                    emit_clean_page_debug(
                                        prepared.event_tx.as_ref(),
                                        format!(
                                            "native Reticulum 0.9 clean page response-resource received destination={} link_id={} path={} request_id={} response_resource={} bytes={}",
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
                            active_response_resource = Some(event.hash);
                            progress_events += 1;
                            emit_clean_page_resource_progress(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                progress.received_bytes,
                                progress.total_bytes,
                                prepared.operation_id.as_deref(),
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
                                "outbound",
                                prepared.operation_id.as_deref(),
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
                                "outbound",
                                prepared.operation_id.as_deref(),
                            );
                            return Err(AppError::from(NativeRuntimeError::Native(
                                "native Reticulum 0.9 request-resource transfer failed".into(),
                            )));
                        }
                        ResourceEventKind::InboundFailed(failure) => {
                            emit_clean_page_resource_lifecycle(
                                prepared.event_tx.as_ref(),
                                event.hash.to_string(),
                                ResourceLifecycleState::Failed,
                                None,
                                Some(failure.reason.clone()),
                                "inbound",
                                prepared.operation_id.as_deref(),
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
                                "outbound",
                                prepared.operation_id.as_deref(),
                            );
                            return Err(AppError::from(NativeRuntimeError::Cancelled));
                        }
                        _ => {}
                    }
                }
                Ok(_) => {
                    unrelated_events += 1;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::debug!(
                        adapter = self.adapter_name(),
                        destination = %prepared.destination_hash,
                        link_id = %prepared.link_id,
                        skipped,
                        "native Reticulum request-resource event stream lagged"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    cancel_clean_page_request_resource(
                        transport,
                        prepared,
                        request_resource_hash,
                        "resource event stream closed",
                    )
                    .await;
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum resource event stream closed".into(),
                    )));
                }
                },
                direct = received_data.recv() => match direct {
                    Ok(data)
                        if data.destination == prepared.link_id
                            && data.context == Some(PacketContext::Response) =>
                    {
                        match NativeLinkResponseFrame::parse_matching(
                            data.data.as_slice(),
                            &frame.request_id,
                        ) {
                            Ok(Some(response)) => {
                                if !outbound_complete {
                                    emit_clean_page_resource_lifecycle(
                                        prepared.event_tx.as_ref(),
                                        request_resource_hash.to_string(),
                                        ResourceLifecycleState::Complete,
                                        Some(frame.packed.len() as u64),
                                        None,
                                        "outbound",
                                        prepared.operation_id.as_deref(),
                                    );
                                }
                                emit_clean_page_debug(
                                    prepared.event_tx.as_ref(),
                                    format!(
                                        "native Reticulum 0.9 request-resource received direct page response destination={} link_id={} path={} request_id={} bytes={}",
                                        prepared.destination_hash,
                                        prepared.link_id,
                                        prepared.path,
                                        hex_bytes(&frame.request_id),
                                        response.body.len()
                                    ),
                                );
                                return Ok(response);
                            }
                            Ok(None) => {
                                unrelated_events += 1;
                            }
                            Err(error) => {
                                last_error = format!("direct response parse error: {error:?}");
                                unrelated_events += 1;
                            }
                        }
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        unrelated_events += 1;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        cancel_clean_page_request_resource(
                            transport,
                            prepared,
                            request_resource_hash,
                            "direct response event stream closed",
                        )
                        .await;
                        return Err(AppError::from(NativeRuntimeError::Native(
                            "native Reticulum direct response stream closed".into(),
                        )));
                    }
                },
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
}

pub fn native_reticulum09_capability_report() -> NativeReticulum09CapabilityReport {
    let request_response = native_reticulum09_request_response_probe();
    NativeReticulum09CapabilityReport {
        stack: "reticulum-rs 0.9",
        transport_crate: "reticulum-rs-transport 0.9",
        lxmf_crate: "lxmf 0.9",
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
                state: NativeRuntimeCapabilityState::Available,
                note: "0.9 public link packet construction, context mutation, and bound send_direct provide the active small-request path; current Python verifies direct and Resource response selection independently of request primitive",
            },
            NativeRuntimeCapability {
                name: "link-identify",
                state: NativeRuntimeCapabilityState::Available,
                note: "NomadNet identify-on-connect is live-verified as encrypted link data with PacketContext::LinkIdentify over the active link's ingress interface",
            },
            NativeRuntimeCapability {
                name: "resource-transfer",
                state: NativeRuntimeCapabilityState::Available,
                note: "request/response resource helpers remain bounded and current Python verifies both oversized requests and large response Resources",
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
                note: "lxmf 0.9 wire encode/decode helpers compile in the native LXMF path",
            },
            NativeRuntimeCapability {
                name: "lxmf-sdk",
                state: NativeRuntimeCapabilityState::NeedsVerification,
                note: "SDK/RPC sidecar path is available as an opt-in evaluation path, not the default runtime",
            },
        ],
        blockers: vec![
            "reticulum-rs-transport 0.9 still has no high-level Link.request helper; OMEN composes the verified small direct request from public packet/link primitives",
            "continue live parity checks against direct, propagated, ticket, and attachment LXMF workflows",
        ],
        recommended_next_step:
            "retain primitive-independent NomadNet response handling without automatic request retry and advance the native-platform release gates",
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
    pub operation_id: Option<String>,
}

impl NativePageFetchContext {
    pub fn new(transport: Arc<reticulum_rs::runtime::Transport>) -> Self {
        Self {
            transport,
            identify_on_connect: false,
            identify_identity: None,
            event_tx: None,
            operation_id: None,
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
            operation_id: None,
        }
    }

    pub fn with_operation_id(mut self, operation_id: Option<String>) -> Self {
        self.operation_id = operation_id;
        self
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
    operation_id: Option<&str>,
) {
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::ResourceProgress(ResourceProgressEvent {
            transfer_id,
            received,
            total: Some(total),
            operation_id: operation_id.map(str::to_owned),
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
    direction: &'static str,
    operation_id: Option<&str>,
) {
    if let Some(event_tx) = event_tx {
        let _ = event_tx.send(RuntimeBusEvent::ResourceLifecycle(ResourceLifecycleEvent {
            transfer_id,
            state,
            bytes,
            reason,
            operation_id: operation_id.map(str::to_owned),
            source: Some("nomadnet-page".into()),
            purpose: Some("nomadnet-page".into()),
            direction: Some(direction.into()),
            peer: None,
        }));
    }
}

async fn cancel_clean_page_request_resource(
    transport: &reticulum_rs::runtime::Transport,
    prepared: &NativePreparedPageLink,
    request_resource_hash: Hash,
    reason: &'static str,
) -> bool {
    match transport
        .cancel_resource(&prepared.link_id, request_resource_hash)
        .await
    {
        Ok(true) => {
            emit_clean_page_resource_lifecycle(
                prepared.event_tx.as_ref(),
                request_resource_hash.to_string(),
                ResourceLifecycleState::Cancelled,
                None,
                Some(reason.into()),
                "outbound",
                prepared.operation_id.as_deref(),
            );
            emit_clean_page_debug(
                prepared.event_tx.as_ref(),
                format!(
                    "native Reticulum 0.9 cancelled NomadNet request-resource request_resource={} reason={reason}",
                    request_resource_hash
                ),
            );
            true
        }
        Ok(false) => {
            emit_clean_page_debug(
                prepared.event_tx.as_ref(),
                format!(
                    "native Reticulum 0.9 NomadNet request-resource cleanup found no active transfer request_resource={} reason={reason}",
                    request_resource_hash
                ),
            );
            false
        }
        Err(error) => {
            emit_clean_page_debug(
                prepared.event_tx.as_ref(),
                format!(
                    "native Reticulum 0.9 NomadNet request-resource cleanup failed request_resource={} reason={reason} error={error:?}",
                    request_resource_hash
                ),
            );
            false
        }
    }
}

async fn cancel_clean_page_response_resource(
    transport: &reticulum_rs::runtime::Transport,
    prepared: &NativePreparedPageLink,
    response_resource_hash: Hash,
    reason: &'static str,
) -> bool {
    match transport
        .cancel_resource(&prepared.link_id, response_resource_hash)
        .await
    {
        Ok(true) => {
            emit_clean_page_resource_lifecycle(
                prepared.event_tx.as_ref(),
                response_resource_hash.to_string(),
                ResourceLifecycleState::Cancelled,
                None,
                Some(reason.into()),
                "inbound",
                prepared.operation_id.as_deref(),
            );
            emit_clean_page_debug(
                prepared.event_tx.as_ref(),
                format!(
                    "native Reticulum 0.9 cancelled NomadNet response-resource response_resource={} reason={reason}",
                    response_resource_hash
                ),
            );
            true
        }
        Ok(false) => {
            emit_clean_page_debug(
                prepared.event_tx.as_ref(),
                format!(
                    "native Reticulum 0.9 NomadNet response-resource cleanup found no active transfer response_resource={} reason={reason}",
                    response_resource_hash
                ),
            );
            false
        }
        Err(error) => {
            emit_clean_page_debug(
                prepared.event_tx.as_ref(),
                format!(
                    "native Reticulum 0.9 NomadNet response-resource cleanup failed response_resource={} reason={reason} error={error:?}",
                    response_resource_hash
                ),
            );
            false
        }
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
    pub operation_id: Option<String>,
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
            .field("operation_id", &self.operation_id)
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

#[derive(Debug)]
pub(crate) struct NativeDirectRequestPacket {
    pub ingress_iface: AddressHash,
    pub request_id: [u8; 16],
    pub packet: Packet,
}

/// Builds the Reticulum 0.9 direct-request packet before dispatch.
///
/// The request identifier for a direct link request is derived from the final
/// encrypted packet hash, not from the packed NomadNet request frame. Keeping
/// construction separate from dispatch lets the conformance suite verify that
/// distinction while the adapter selects packets or Resources by the Python MDU
/// boundary.
pub(crate) fn build_reticulum09_direct_request_packet(
    link: &Link,
    frame: &NativeLinkRequestFrame,
) -> Result<NativeDirectRequestPacket, NativeRuntimeError> {
    if link.status() != LinkStatus::Active {
        return Err(NativeRuntimeError::Native(
            "native Reticulum direct request candidate requires an active link".into(),
        ));
    }
    let ingress_iface = link.ingress_iface().ok_or_else(|| {
        NativeRuntimeError::Native(
            "native Reticulum direct request candidate requires a bound link interface".into(),
        )
    })?;
    let mut packet = link.data_packet(&frame.packed).map_err(|error| {
        NativeRuntimeError::Native(format!(
            "native Reticulum direct request candidate could not encrypt request: {error:?}"
        ))
    })?;
    packet.context = PacketContext::Request;
    let packet_hash = packet.hash().to_bytes();
    let mut request_id = [0u8; 16];
    request_id.copy_from_slice(&packet_hash[..16]);

    Ok(NativeDirectRequestPacket {
        ingress_iface,
        request_id,
        packet,
    })
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

    pub fn parse_matching(
        bytes: &[u8],
        request_id: &[u8; 16],
    ) -> Result<Option<Self>, NativeRuntimeError> {
        let response = Self::parse(bytes)?;
        Ok((response.request_id == *request_id).then_some(response))
    }

    pub fn parse_matching_response_resource(
        complete: &ResourceComplete,
        request_id: &[u8; 16],
    ) -> Result<Option<Self>, NativeRuntimeError> {
        if !complete.is_response || complete.request_id.as_deref() != Some(request_id) {
            return Ok(None);
        }
        Self::parse_matching(&complete.data, request_id)
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
        metadata.insert(
            "native_response_empty".into(),
            serde_json::Value::Bool(markup.is_empty()),
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

#[derive(Clone, Debug)]
struct NativePageLinkCoordinator {
    gates: Arc<[Mutex<()>; NOMADNET_PAGE_LINK_GATE_STRIPES]>,
}

impl Default for NativePageLinkCoordinator {
    fn default() -> Self {
        Self {
            gates: Arc::new(std::array::from_fn(|_| Mutex::new(()))),
        }
    }
}

impl NativePageLinkCoordinator {
    fn stripe(destination: &AddressHash) -> usize {
        usize::from(destination.as_slice()[0]) % NOMADNET_PAGE_LINK_GATE_STRIPES
    }

    async fn lock<'a>(
        &'a self,
        destination: &AddressHash,
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

#[derive(Clone, Debug, Default)]
pub struct ReticulumPageTransportClient {
    coordinator: NativePageLinkCoordinator,
}

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
            // Reticulum 0.9 reuses an existing non-closed outbound link for a
            // destination. Keep the complete request/response owner inside one
            // fixed-size destination stripe so successful operations can reuse
            // the link and a failed operation cannot tear it down underneath a
            // concurrent request.
            let _link_owner = self
                .coordinator
                .lock(&plan.request.destination_hash, &cancel)
                .await?;
            let prepared = prepare_nomadnet_page_link(plan, context, cancel.clone()).await?;
            let response = async {
                let request_frame = build_native_link_request_frame(&prepared, unix_timestamp())?;
                let adapter = Reticulum09LinkRequestAdapter;
                adapter
                    .send_request(&prepared, &request_frame, exchange.timeout, cancel)
                    .await
            }
            .await;
            return match response {
                Ok(response) => {
                    emit_clean_page_debug(
                        context.event_tx.as_ref(),
                        format!(
                            "native Reticulum 0.9 retained successful NomadNet page link destination={} link_id={} path={}",
                            prepared.destination_hash, prepared.link_id, prepared.path
                        ),
                    );
                    Ok(NativePageResponse {
                        body: response.body,
                        content_type: Some("text/x-micron".into()),
                    })
                }
                Err(error) => {
                    close_nomadnet_page_link(&context.transport, &prepared.link).await;
                    Err(error)
                }
            };
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
    crate::msgpack::validate_msgpack_with_limits(
        bytes,
        MAX_NOMADNET_RESPONSE_BYTES,
        MAX_NOMADNET_RESPONSE_BYTES,
        MAX_NOMADNET_RESPONSE_CONTAINER_ITEMS,
        MAX_NOMADNET_RESPONSE_TOTAL_VALUES,
        MAX_NOMADNET_RESPONSE_DEPTH,
    )
    .map_err(|error| NativeRuntimeError::InvalidResponse(error.to_string()))?;
    let mut cursor = std::io::Cursor::new(bytes);
    let value = rmpv::decode::read_value(&mut cursor).map_err(|_| {
        NativeRuntimeError::InvalidResponse("failed to decode Link.request msgpack".into())
    })?;
    if cursor.position() != bytes.len() as u64 {
        return Err(NativeRuntimeError::InvalidResponse(
            "trailing Link.request msgpack data".into(),
        ));
    }
    Ok(value)
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
        "native Reticulum 0.9 sent LinkIdentify on active link"
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
    let mut link = context.transport.link(destination).await;
    if link.lock().await.status() == LinkStatus::Stale {
        close_nomadnet_page_link(&context.transport, &Some(link.clone())).await;
        context
            .transport
            .reset_out_link(&plan.request.destination_hash)
            .await;
        link = context.transport.link(destination).await;
    }
    let link_id = *link.lock().await.id();

    if link.lock().await.status() != LinkStatus::Active {
        let deadline = tokio::time::Instant::now() + plan.timeout;
        loop {
            if cancel.is_cancelled() {
                close_nomadnet_page_link(&context.transport, &Some(link.clone())).await;
                return Err(AppError::from(NativeRuntimeError::Cancelled));
            }
            let link_status = link.lock().await.status();
            match link_status {
                LinkStatus::Active => break,
                LinkStatus::Stale | LinkStatus::Closed => {
                    close_nomadnet_page_link(&context.transport, &Some(link.clone())).await;
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum link became unavailable during page fetch setup".into(),
                    )));
                }
                LinkStatus::Pending | LinkStatus::Handshake => {}
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                close_nomadnet_page_link(&context.transport, &Some(link.clone())).await;
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
                    close_nomadnet_page_link(&context.transport, &Some(link.clone())).await;
                    return Err(AppError::from(NativeRuntimeError::Native(
                        "native Reticulum link closed during page fetch setup".into(),
                    )));
                }
                Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    close_nomadnet_page_link(&context.transport, &Some(link.clone())).await;
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
            "native Reticulum 0.9 clean page link active destination={} link_id={} path={} identify_on_connect={}",
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
                        "native Reticulum 0.9 page link is active but NomadNet identify-on-connect could not be sent"
                    );
                    emit_clean_page_debug(
                        context.event_tx.as_ref(),
                        format!(
                            "native Reticulum 0.9 clean page LinkIdentify failed destination={} link_id={} error={}",
                            plan.request.destination_hash, link_id, error
                        ),
                    );
                } else {
                    emit_clean_page_debug(
                        context.event_tx.as_ref(),
                        format!(
                            "native Reticulum 0.9 clean page LinkIdentify sent destination={} link_id={} identity={}",
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
                    "native Reticulum 0.9 page link is active but NomadNet identify-on-connect was skipped because the active local identity could not be loaded"
                );
                emit_clean_page_debug(
                    context.event_tx.as_ref(),
                    format!(
                        "native Reticulum 0.9 clean page LinkIdentify skipped destination={} link_id={} reason=identity_unavailable",
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
        operation_id: context.operation_id.clone(),
    })
}

async fn close_nomadnet_page_link(
    transport: &Arc<reticulum_rs::runtime::Transport>,
    link: &Option<Arc<Mutex<Link>>>,
) -> bool {
    let Some(link) = link else {
        return false;
    };
    let teardown = {
        let mut link = link.lock().await;
        let ingress_iface = link.ingress_iface();
        let packet = link.teardown();
        packet.map(|packet| (ingress_iface, packet))
    };
    if let Some((Some(ingress_iface), packet)) = teardown {
        transport.send_direct(ingress_iface, packet).await;
    }
    true
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
    fn reticulum09_direct_request_candidate_round_trips_packet_hash_correlation() {
        let remote_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-phase-4-direct-request-peer",
        );
        let remote_destination = rns_transport::destination::SingleInputDestination::new(
            remote_identity,
            DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
        );
        let (link_events, mut link_event_rx) = broadcast::channel(16);
        let mut outbound = Link::new(remote_destination.desc, link_events.clone());
        let link_request = outbound.request();
        let mut inbound = Link::new_from_request(
            &link_request,
            remote_destination.sign_key().clone(),
            remote_destination.desc,
            link_events,
        )
        .expect("in-memory peer accepts link request");
        let ingress_iface = AddressHash::new([0x31; 16]);
        assert!(matches!(
            outbound.handle_packet(&inbound.prove(), ingress_iface),
            rns_transport::destination::link::LinkHandleResult::Activated
        ));

        let frame = NativeLinkRequestFrame::build("/page/index.mu", &BTreeMap::new(), 1_234.5)
            .expect("small request frame");
        assert!(!frame.requires_request_resource());
        let candidate = build_reticulum09_direct_request_packet(&outbound, &frame)
            .expect("active link builds direct request candidate");

        assert_eq!(candidate.ingress_iface, ingress_iface);
        assert_eq!(candidate.packet.context, PacketContext::Request);
        assert_eq!(candidate.packet.destination, *outbound.id());
        assert_eq!(
            candidate.request_id.as_slice(),
            &candidate.packet.hash().to_bytes()[..16]
        );
        assert_ne!(candidate.request_id, frame.request_id);

        let _ = inbound.handle_packet(&candidate.packet, ingress_iface);
        let inbound_payload = receive_link_payload(&mut link_event_rx, PacketContext::Request);
        assert_eq!(inbound_payload.as_slice(), frame.packed);
        assert_eq!(inbound_payload.request_id(), Some(candidate.request_id));

        let response_bytes = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(candidate.request_id.to_vec()),
            Value::Binary(b">Direct Request Candidate\nRound trip".to_vec()),
        ]))
        .expect("response frame");
        let mut response_packet = inbound
            .data_packet(&response_bytes)
            .expect("peer encrypts response packet");
        response_packet.context = PacketContext::Response;
        let _ = outbound.handle_packet(&response_packet, ingress_iface);
        let response_payload = receive_link_payload(&mut link_event_rx, PacketContext::Response);
        let response = NativeLinkResponseFrame::parse_matching(
            response_payload.as_slice(),
            &candidate.request_id,
        )
        .expect("response is valid")
        .expect("response correlation matches final packet hash");
        assert_eq!(response.body, b">Direct Request Candidate\nRound trip");
    }

    #[test]
    fn reticulum09_direct_request_candidate_rejects_inactive_links() {
        let identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-phase-4-inactive-direct-request",
        );
        let destination = SingleOutputDestination::new(
            *identity.as_identity(),
            DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
        );
        let (link_events, _receiver) = broadcast::channel(4);
        let link = Link::new(destination.desc, link_events);
        let frame = NativeLinkRequestFrame::build("/", &BTreeMap::new(), 1.0).expect("frame");

        let error = build_reticulum09_direct_request_packet(&link, &frame)
            .expect_err("pending link must not build a direct request");

        assert!(format!("{error:?}").contains("requires an active link"));
    }

    fn receive_link_payload(
        receiver: &mut broadcast::Receiver<rns_transport::destination::link::LinkEventData>,
        expected_context: PacketContext,
    ) -> Box<rns_transport::destination::link::LinkPayload> {
        for _ in 0..16 {
            match receiver.try_recv() {
                Ok(event) => {
                    if let LinkEvent::Data(payload) = event.event {
                        if payload.context() == expected_context {
                            return payload;
                        }
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        panic!("missing link payload with context {expected_context:?}");
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
    fn native_link_response_rejects_unbounded_or_trailing_msgpack() {
        let request_id = [0x42; 16];
        let mut trailing = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(request_id.to_vec()),
            Value::Binary(b">Page".to_vec()),
        ]))
        .expect("pack response");
        trailing.push(0xc0);
        assert!(NativeLinkResponseFrame::parse(&trailing).is_err());

        let oversized_scalar = [0xdb, 0x00, 0x40, 0x00, 0x01];
        assert!(NativeLinkResponseFrame::parse(&oversized_scalar).is_err());

        let mut deep = vec![0x91; MAX_NOMADNET_RESPONSE_DEPTH + 2];
        deep.push(0xc0);
        assert!(NativeLinkResponseFrame::parse(&deep).is_err());

        assert!(
            NativeLinkResponseFrame::parse(&vec![0xc0; MAX_NOMADNET_RESPONSE_BYTES + 1]).is_err()
        );
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
        let conflicting_inner_id = ResourceComplete {
            data: pack_msgpack_value(&Value::Array(vec![
                Value::Binary(other_request_id.to_vec()),
                Value::Binary(b">Wrong Response\nBody".to_vec()),
            ]))
            .expect("pack conflicting response"),
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
        assert_eq!(
            NativeLinkResponseFrame::parse_matching_response_resource(
                &conflicting_inner_id,
                &request_id
            )
            .expect("conflicting inner id is unrelated"),
            None
        );
    }

    #[test]
    fn native_link_response_frame_matches_packet_payload_by_request_id() {
        let request_id = [0x42; 16];
        let other_request_id = [0x24; 16];
        let matching = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(request_id.to_vec()),
            Value::Binary(b">Matching Response\nBody".to_vec()),
        ]))
        .expect("pack matching response");
        let unrelated = pack_msgpack_value(&Value::Array(vec![
            Value::Binary(other_request_id.to_vec()),
            Value::Binary(b">Unrelated Response\nBody".to_vec()),
        ]))
        .expect("pack unrelated response");

        assert_eq!(
            NativeLinkResponseFrame::parse_matching(&matching, &request_id)
                .expect("matching packet parses")
                .expect("packet matched")
                .body,
            b">Matching Response\nBody"
        );
        assert_eq!(
            NativeLinkResponseFrame::parse_matching(&unrelated, &request_id)
                .expect("unrelated packet parses"),
            None
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
            operation_id: None,
        };

        let frame = build_native_link_request_frame(&prepared, 2.0).expect("frame");

        assert_eq!(frame.path, "/");
        assert_eq!(frame.path_hash, truncated_sha256(b"/"));
    }

    #[test]
    fn native_link_request_adapter_plan_names_active_primitive_boundary() {
        let plan = NativeLinkRequestAdapterPlan::reticulum09_available();

        assert_eq!(plan.boundary, "reticulum-rs-transport Link request adapter");
        assert_eq!(plan.request_context, PacketContext::Request);
        assert_eq!(plan.response_context, PacketContext::Response);
        assert_eq!(
            plan.dispatch_status,
            NativeRuntimeCapabilityState::Available
        );
        assert!(plan.is_ready());
        assert!(plan.next_step.contains("request resources"));
        assert!(plan.next_step.contains("direct packets"));
    }

    #[tokio::test]
    async fn missing_reticulum09_link_request_adapter_fails_before_dispatch() {
        let adapter = MissingReticulum09LinkRequestAdapter;
        let prepared = NativePreparedPageLink {
            destination_hash: AddressHash::new_empty(),
            link_id: AddressHash::new_empty(),
            path: "/".into(),
            request_data: BTreeMap::new(),
            transport: None,
            link: None,
            event_tx: None,
            operation_id: None,
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
    async fn reticulum09_link_request_adapter_requires_live_handles() {
        let adapter = Reticulum09LinkRequestAdapter;
        let prepared = NativePreparedPageLink {
            destination_hash: AddressHash::new_empty(),
            link_id: AddressHash::new_empty(),
            path: "/".into(),
            request_data: BTreeMap::new(),
            transport: None,
            link: None,
            event_tx: None,
            operation_id: None,
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

    async fn active_nomadnet_request_fixture() -> (
        NativePreparedPageLink,
        NativeLinkRequestFrame,
        rns_transport::iface::InterfaceChannel,
        broadcast::Receiver<RuntimeBusEvent>,
    ) {
        let local_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-nomadnet-request-cancel-local",
        );
        let config = reticulum_rs::runtime::TransportConfig::new(
            "omenbrowser-nomadnet-request-cancel",
            &local_identity,
            false,
        );
        let transport = Arc::new(reticulum_rs::runtime::Transport::new(config));
        let channel = transport
            .iface_manager()
            .lock()
            .await
            .new_channel_with_role(8, rns_transport::iface::IfaceRole::Unicast);
        let ingress_iface = *channel.address();

        let remote_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-nomadnet-request-cancel-remote",
        );
        let destination = SingleOutputDestination::new(
            *remote_identity.as_identity(),
            DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
        );
        let link = transport.link(destination.desc).await;
        let request = link.lock().await.request();
        let (inbound_events, _) = broadcast::channel(4);
        let mut inbound = Link::new_from_request(
            &request,
            remote_identity.sign_key().clone(),
            destination.desc,
            inbound_events,
        )
        .expect("inbound link");
        assert!(matches!(
            link.lock()
                .await
                .handle_packet(&inbound.prove(), ingress_iface),
            rns_transport::destination::link::LinkHandleResult::Activated
        ));

        let link_id = *link.lock().await.id();
        let (event_tx, event_rx) = broadcast::channel(16);
        let prepared = NativePreparedPageLink {
            destination_hash: destination.desc.address_hash,
            link_id,
            path: "/page/post.mu".into(),
            request_data: BTreeMap::from([("body".into(), "x".repeat(2048))]),
            transport: Some(transport),
            link: Some(link),
            event_tx: Some(event_tx),
            operation_id: Some("browser-operation-7".into()),
        };
        let frame = build_native_link_request_frame(&prepared, 2.0).expect("request frame");
        assert!(frame.requires_request_resource());
        (prepared, frame, channel, event_rx)
    }

    #[tokio::test]
    async fn pre_cancelled_nomadnet_request_dispatches_neither_packet_nor_resource() {
        for request_resource in [false, true] {
            let (prepared, resource_frame, mut channel, _event_rx) =
                active_nomadnet_request_fixture().await;
            let direct_frame =
                NativeLinkRequestFrame::build("/", &BTreeMap::new(), 2.0).expect("direct frame");
            let frame = if request_resource {
                resource_frame
            } else {
                direct_frame
            };
            assert_eq!(frame.requires_request_resource(), request_resource);
            let cancel = CancellationToken::new();
            cancel.cancel();

            let error = Reticulum09LinkRequestAdapter
                .send_request(&prepared, &frame, Duration::from_secs(1), cancel)
                .await
                .expect_err("pre-cancelled request");
            assert!(format!("{error}").contains("cancelled"));
            assert!(channel.tx_channel.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn cancelled_nomadnet_request_releases_outbound_resource_and_reports_direction() {
        let (prepared, frame, mut channel, mut event_rx) = active_nomadnet_request_fixture().await;
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            Reticulum09LinkRequestAdapter
                .send_request(&prepared, &frame, Duration::from_secs(5), task_cancel)
                .await
        });

        let advertisement = tokio::time::timeout(Duration::from_secs(1), channel.tx_channel.recv())
            .await
            .expect("request-resource advertisement timeout")
            .expect("request-resource advertisement");
        assert_eq!(
            advertisement.packet.context,
            PacketContext::ResourceAdvrtisement
        );

        cancel.cancel();
        let cancel_packet = tokio::time::timeout(Duration::from_secs(1), channel.tx_channel.recv())
            .await
            .expect("request-resource cancel timeout")
            .expect("request-resource cancel");
        assert_eq!(
            cancel_packet.packet.context,
            PacketContext::ResourceInitiatorCancel
        );
        let error = task
            .await
            .expect("request task join")
            .expect_err("cancelled request");
        assert!(format!("{error}").contains("cancelled"));

        let lifecycle = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let RuntimeBusEvent::ResourceLifecycle(event) =
                    event_rx.recv().await.expect("runtime event")
                {
                    if event.state == ResourceLifecycleState::Cancelled {
                        break event;
                    }
                }
            }
        })
        .await
        .expect("cancelled lifecycle timeout");
        assert_eq!(lifecycle.direction.as_deref(), Some("outbound"));
        assert_eq!(lifecycle.source.as_deref(), Some("nomadnet-page"));
        assert_eq!(
            lifecycle.operation_id.as_deref(),
            Some("browser-operation-7")
        );
        assert_eq!(
            lifecycle.reason.as_deref(),
            Some("browser request cancelled")
        );
        assert_no_request_primitive_replay(&mut channel).await;
    }

    #[tokio::test]
    async fn timed_out_nomadnet_request_releases_outbound_resource() {
        let (prepared, frame, mut channel, mut event_rx) = active_nomadnet_request_fixture().await;
        let task = tokio::spawn(async move {
            Reticulum09LinkRequestAdapter
                .send_request(
                    &prepared,
                    &frame,
                    Duration::from_millis(20),
                    CancellationToken::new(),
                )
                .await
        });

        let advertisement = tokio::time::timeout(Duration::from_secs(1), channel.tx_channel.recv())
            .await
            .expect("request-resource advertisement timeout")
            .expect("request-resource advertisement");
        assert_eq!(
            advertisement.packet.context,
            PacketContext::ResourceAdvrtisement
        );
        let cancel_packet = tokio::time::timeout(Duration::from_secs(1), channel.tx_channel.recv())
            .await
            .expect("timed-out request-resource cancel timeout")
            .expect("timed-out request-resource cancel");
        assert_eq!(
            cancel_packet.packet.context,
            PacketContext::ResourceInitiatorCancel
        );
        let error = task
            .await
            .expect("request task join")
            .expect_err("timed-out request");
        assert!(format!("{error}").contains("timeout"));

        let lifecycle = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let RuntimeBusEvent::ResourceLifecycle(event) =
                    event_rx.recv().await.expect("runtime event")
                {
                    if event.state == ResourceLifecycleState::Cancelled {
                        break event;
                    }
                }
            }
        })
        .await
        .expect("timeout cleanup lifecycle");
        assert_eq!(lifecycle.direction.as_deref(), Some("outbound"));
        assert_eq!(
            lifecycle.operation_id.as_deref(),
            Some("browser-operation-7")
        );
        assert_eq!(
            lifecycle.reason.as_deref(),
            Some("NomadNet response timeout")
        );
        assert_no_request_primitive_replay(&mut channel).await;
    }

    async fn assert_no_request_primitive_replay(
        channel: &mut rns_transport::iface::InterfaceChannel,
    ) {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(50);
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            let Ok(Some(delivery)) =
                tokio::time::timeout(remaining, channel.tx_channel.recv()).await
            else {
                break;
            };
            assert!(
                !matches!(
                    delivery.packet.context,
                    PacketContext::Request | PacketContext::ResourceAdvrtisement
                ),
                "terminal request cleanup must not replay through another request primitive"
            );
        }
    }

    #[tokio::test]
    async fn nomadnet_page_link_cleanup_closes_pending_link_without_an_interface() {
        let identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-phase-2-link-cleanup",
        );
        let destination = SingleOutputDestination::new(
            *identity.as_identity(),
            DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
        );
        let (link_events, _receiver) = broadcast::channel(4);
        let link = Arc::new(Mutex::new(Link::new(destination.desc, link_events)));
        let config = reticulum_rs::runtime::TransportConfig::new(
            "omenbrowser-phase-2-link-cleanup",
            &identity,
            false,
        );
        let transport = Arc::new(reticulum_rs::runtime::Transport::new(config));

        assert_eq!(link.lock().await.status(), LinkStatus::Pending);
        assert!(close_nomadnet_page_link(&transport, &Some(link.clone())).await);
        assert_eq!(link.lock().await.status(), LinkStatus::Closed);
    }

    #[tokio::test]
    async fn nomadnet_page_link_coordinator_serializes_same_stripe_and_is_cancellable() {
        let coordinator = NativePageLinkCoordinator::default();
        let first_destination = AddressHash::new([0x00; 16]);
        let same_stripe = AddressHash::new([0x20; 16]);
        let other_stripe = AddressHash::new([0x01; 16]);
        let cancel = CancellationToken::new();
        let first_guard = coordinator
            .lock(&first_destination, &cancel)
            .await
            .expect("first destination gate");

        assert!(
            coordinator.gates[NativePageLinkCoordinator::stripe(&same_stripe)]
                .try_lock()
                .is_err()
        );
        assert!(
            coordinator.gates[NativePageLinkCoordinator::stripe(&other_stripe)]
                .try_lock()
                .is_ok()
        );

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let error = coordinator
            .lock(&same_stripe, &cancelled)
            .await
            .expect_err("cancelled waiter");
        assert!(matches!(error, AppError::Runtime(message) if message.contains("cancelled")));

        drop(first_guard);
        assert!(
            coordinator.gates[NativePageLinkCoordinator::stripe(&same_stripe)]
                .try_lock()
                .is_ok()
        );
    }

    #[tokio::test]
    async fn reticulum09_reuses_active_page_link_and_reconnects_only_after_close() {
        let local_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-page-link-reuse-local",
        );
        let config = reticulum_rs::runtime::TransportConfig::new(
            "omenbrowser-page-link-reuse",
            &local_identity,
            false,
        );
        let transport = Arc::new(reticulum_rs::runtime::Transport::new(config));
        let mut channel = transport
            .iface_manager()
            .lock()
            .await
            .new_channel_with_role(8, rns_transport::iface::IfaceRole::Unicast);
        let ingress_iface = *channel.address();
        let remote_identity = rns_transport::identity::PrivateIdentity::new_from_name(
            "omenbrowser-page-link-reuse-remote",
        );
        let destination = SingleOutputDestination::new(
            *remote_identity.as_identity(),
            DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
        );
        let first = transport.link(destination.desc).await;
        let request = first.lock().await.request();
        let (inbound_events, _) = broadcast::channel(4);
        let mut inbound = Link::new_from_request(
            &request,
            remote_identity.sign_key().clone(),
            destination.desc,
            inbound_events,
        )
        .expect("inbound link");
        assert!(matches!(
            first
                .lock()
                .await
                .handle_packet(&inbound.prove(), ingress_iface),
            rns_transport::destination::link::LinkHandleResult::Activated
        ));

        let reused = transport.link(destination.desc).await;
        assert!(Arc::ptr_eq(&first, &reused));
        assert_eq!(reused.lock().await.status(), LinkStatus::Active);

        assert!(close_nomadnet_page_link(&transport, &Some(first.clone())).await);
        let close = tokio::time::timeout(Duration::from_secs(1), channel.tx_channel.recv())
            .await
            .expect("link close packet timeout")
            .expect("link close packet");
        assert_eq!(close.packet.context, PacketContext::LinkClose);
        assert_eq!(reused.lock().await.status(), LinkStatus::Closed);

        let replacement = transport.link(destination.desc).await;
        assert!(!Arc::ptr_eq(&first, &replacement));
        assert_eq!(replacement.lock().await.status(), LinkStatus::Pending);
        assert!(close_nomadnet_page_link(&transport, &Some(replacement.clone())).await);
        assert_eq!(replacement.lock().await.status(), LinkStatus::Closed);
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
    fn native_page_response_marks_valid_empty_body_without_treating_it_as_failure() {
        let plan = NativeFetchPlan::new(&format!("{DEST}:/empty.mu"), None, 5).expect("plan");
        let page = NativePageResponse {
            body: Vec::new(),
            content_type: Some("text/x-micron".into()),
        }
        .into_browser_page(&plan)
        .expect("empty response is valid UTF-8");

        assert!(page.markup.is_empty());
        assert_eq!(page.source, PageSource::Network);
        assert_eq!(
            page.metadata
                .get("native_response_empty")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
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
    fn reticulum09_request_response_probe_marks_direct_and_resource_paths_available() {
        let probe = native_reticulum09_request_response_probe();

        assert!(probe.request_context_available);
        assert!(probe.response_context_available);
        assert!(probe.received_data_request_id_available);
        assert!(probe.link_data_packet_available);
        assert!(probe.link_channel_packet_available);
        assert!(probe.public_bound_link_data_send_available);
        assert!(probe.public_bound_link_channel_send_available);
        assert!(probe.public_bound_request_context_send_available);
        assert!(probe.public_bound_link_identify_send_available);
        assert!(probe.request_resource_send_available);
        assert!(probe.resource_response_events_available);
        assert!(probe.public_packet_context_mutation_available);
        assert!(probe.public_transport_packet_dispatch_available);
        assert!(!probe.high_level_link_request_send_available);
        assert!(probe
            .recommended_adapter
            .contains("current-Python-verified"));
        assert!(probe.recommended_adapter.contains("request-resource"));
        assert!(probe.note.contains("small requests"));
        assert!(probe.note.contains("packet MDU"));
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

        assert_eq!(report.sdk_crate, "lxmf-sdk 0.9");
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
    fn reticulum09_capability_report_exposes_direct_and_resource_paths() {
        let report = native_reticulum09_capability_report();

        assert_eq!(report.stack, "reticulum-rs 0.9");
        assert_eq!(report.transport_crate, "reticulum-rs-transport 0.9");
        assert_eq!(report.lxmf_crate, "lxmf 0.9");
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
            Some(NativeRuntimeCapabilityState::Available)
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
    fn reticulum09_capability_report_tracks_remaining_nomadnet_interop_gates() {
        let report = native_reticulum09_capability_report();

        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("OMENchat")));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("high-level Link.request")));
        assert!(report
            .recommended_next_step
            .contains("primitive-independent NomadNet response"));
        assert!(report.recommended_next_step.contains("native-platform"));
        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("performance")));
        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("cancellation")));
        assert!(!report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("repeated-link")));
    }
}
