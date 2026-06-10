use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rmpv::Value;
use rns_transport::destination::link::{LinkEvent, LinkStatus};
use rns_transport::destination::{DestinationDesc, DestinationName, SingleOutputDestination};
use rns_transport::hash::AddressHash;
use sha2::{Digest, Sha256};

use crate::browser::page::DEFAULT_PATH;
use crate::browser::{BrowserAddress, BrowserPage, PageSource};
use crate::error::{AppError, AppResult};
use crate::runtime::native::NativeRuntimeError;
use crate::runtime::network::CancellationToken;

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
}

impl NativePageFetchContext {
    pub fn new(transport: Arc<reticulum_rs::runtime::Transport>) -> Self {
        Self { transport }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePreparedPageLink {
    pub destination_hash: AddressHash,
    pub link_id: AddressHash,
    pub path: String,
    pub request_data: BTreeMap<String, String>,
}

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
        let path_hash = truncated_sha256(path.as_bytes());
        let data = request_data_value(request_data);
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
            let _request_frame = build_native_link_request_frame(&prepared, unix_timestamp())?;
            return Err(AppError::from(NativeRuntimeError::Unsupported(
                "native Reticulum Link.request frame is prepared, but encrypted dispatch/receipt is not wired yet",
            )));
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
    let destination = SingleOutputDestination::new(
        identity,
        DestinationName::new(NOMADNET_APP_NAME, NOMADNET_NODE_ASPECT),
    );
    if destination.desc.address_hash != destination_hash {
        return Err(NativeRuntimeError::PathUnavailable(
            destination_hash.to_hex_string(),
        ));
    }
    Ok(destination.desc)
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

    Ok(NativePreparedPageLink {
        destination_hash: plan.request.destination_hash,
        link_id,
        path: plan.request.path.clone(),
        request_data: plan.request.request_data.clone().unwrap_or_default(),
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
    fn build_native_link_request_frame_uses_prepared_link_handoff() {
        let prepared = NativePreparedPageLink {
            destination_hash: AddressHash::new_empty(),
            link_id: AddressHash::new_empty(),
            path: "/".into(),
            request_data: BTreeMap::from([("field_name".into(), "omen".into())]),
        };

        let frame = build_native_link_request_frame(&prepared, 2.0).expect("frame");

        assert_eq!(frame.path, "/");
        assert_eq!(frame.path_hash, truncated_sha256(b"/"));
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
}
