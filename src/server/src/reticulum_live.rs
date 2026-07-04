use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rand_core::OsRng;
use rns_transport::destination::link::LinkEvent;
use rns_transport::destination::{DestinationName, SingleInputDestination};
use rns_transport::hash::AddressHash;
use rns_transport::identity::PrivateIdentity;
use rns_transport::resource::ResourceEventKind;
use rns_transport::transport::{ReceivedPayloadMode, Transport, TransportConfig};
use rns_transport::PacketContext;
use tokio::sync::mpsc;

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::live::{OmenchatLinkEvent, OmenchatLiveServer};
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

pub struct ReticulumLiveRuntime {
    transport: Arc<Transport>,
    destination: Arc<tokio::sync::Mutex<SingleInputDestination>>,
    nomadnet_destination: Arc<tokio::sync::Mutex<SingleInputDestination>>,
    pub identity_hash: [u8; 16],
    pub destination_hash: [u8; 16],
    pub nomadnet_destination_hash: [u8; 16],
    pub destination_name: String,
    pub nomadnet_destination_name: String,
    event_rx: mpsc::UnboundedReceiver<OmenchatLinkEvent>,
    pub live_server: OmenchatLiveServer<ReticulumOmenchatTransport>,
    interface_statuses: Vec<ReticulumInterfaceStatus>,
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

    fn is_connected(&self) -> bool {
        match &self.kind {
            ReticulumInterfaceStatusKind::TcpClient(handle) => {
                json_str(&handle.to_json(), "stream_state") == "connected"
            }
            ReticulumInterfaceStatusKind::IfacTcpClient(handle) => {
                json_str(&handle.to_json(), "stream_state") == "connected"
            }
            ReticulumInterfaceStatusKind::TcpServer(handle) => {
                json_str(&handle.to_json(), "listener_state") == "listening"
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
    tx: mpsc::UnboundedSender<TransportCommand>,
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
        payload: Vec<u8>,
        metadata: Vec<u8>,
    },
    CloseLink {
        link_id: LinkId,
    },
}

impl ReticulumOmenchatTransport {
    fn new(transport: Arc<Transport>, log_path: std::path::PathBuf) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<TransportCommand>();
        let sent_frames = Arc::new(AtomicU64::new(0));
        let offered_resources = Arc::new(AtomicU64::new(0));
        let sent_frame_bytes = Arc::new(AtomicU64::new(0));
        let offered_resource_bytes = Arc::new(AtomicU64::new(0));
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    TransportCommand::SendFrame {
                        link_id,
                        frame_bytes,
                    } => {
                        let link_hash = AddressHash::new(link_id);
                        let link = transport.find_in_link(&link_hash).await;
                        let Some(link) = link else {
                            append_server_log_path(
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
                            Err(error) => append_server_log_path(
                                &log_path,
                                format!(
                                    "reticulum-rs OMENchat frame send failed link={} bytes={} error={error:?}",
                                    hex_lower(&link_id),
                                    frame_bytes.len()
                                ),
                            ),
                        }
                    }
                    TransportCommand::OfferResource {
                        link_id,
                        payload,
                        metadata,
                    } => {
                        match transport
                            .send_resource(&AddressHash::new(link_id), payload.clone(), Some(metadata))
                            .await
                        {
                            Ok(hash) => {
                                append_server_log_path(
                                    &log_path,
                                    format!(
                                        "reticulum-rs OMENchat resource offered link={} hash={} bytes={}",
                                        hex_lower(&link_id),
                                        hash,
                                        payload.len()
                                    ),
                                );
                            }
                            Err(error) => append_server_log_path(
                                &log_path,
                                format!(
                                    "reticulum-rs OMENchat offer resource failed link={} bytes={} error={error:?}",
                                    hex_lower(&link_id),
                                    payload.len()
                                ),
                            ),
                        }
                    }
                    TransportCommand::CloseLink { link_id } => {
                        let channel = transport.channel(AddressHash::new(link_id));
                        if let Err(error) = channel.close().await {
                            append_server_log_path(
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
        self.tx
            .send(TransportCommand::SendFrame {
                link_id,
                frame_bytes,
            })
            .map_err(|_| ServerError::Message("reticulum-rs transport task stopped".into()))?;
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
        self.tx
            .send(TransportCommand::OfferResource {
                link_id,
                payload,
                metadata,
            })
            .map_err(|_| ServerError::Message("reticulum-rs transport task stopped".into()))?;
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
        self.tx
            .send(TransportCommand::CloseLink { link_id })
            .map_err(|_| ServerError::Message("reticulum-rs transport task stopped".into()))
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
        while let Ok(event) = runtime.event_rx.try_recv() {
            if let Err(error) = runtime.live_server.handle_event(event) {
                append_server_log(&config, format!("reticulum-rs live event failed: {error}"));
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

    let (event_tx, event_rx) = mpsc::unbounded_channel();
    spawn_link_event_bridge(transport.clone(), event_tx.clone(), config.log_path());
    spawn_received_data_bridge(transport.clone(), event_tx.clone(), config.log_path());
    spawn_resource_event_bridge(transport.clone(), event_tx, config.log_path());

    let store = OmenchatStore::open(&config.database_path)?;
    let engine =
        SessionEngine::with_limits_and_motd(store, config.into(), Some(config.motd.clone()));
    let transport_impl = ReticulumOmenchatTransport::new(transport.clone(), config.log_path());

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
        event_rx,
        live_server: OmenchatLiveServer::new(engine, transport_impl),
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
        } else if self
            .interface_statuses
            .iter()
            .any(ReticulumInterfaceStatus::is_connected)
        {
            InterfaceHealth::Connected
        } else {
            InterfaceHealth::Connected
        }
    }
}

pub fn drain_live_events_logged(
    runtime: &mut ReticulumLiveRuntime,
    max_events: usize,
    config: &ServerConfig,
) -> ServerResult<usize> {
    let mut drained = 0usize;
    while drained < max_events {
        match runtime.event_rx.try_recv() {
            Ok(event) => {
                append_server_log(config, describe_live_event(&event));
                runtime.live_server.handle_event(event)?;
                drained += 1;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
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
    event_tx: mpsc::UnboundedSender<OmenchatLinkEvent>,
    log_path: std::path::PathBuf,
) {
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
                        let _ = event_tx.send(OmenchatLinkEvent::LinkOpened { link_id, peer });
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
                        let _ = event_tx.send(OmenchatLinkEvent::LinkData {
                            link_id,
                            context: payload.context() as u8,
                            data: payload.as_slice().to_vec(),
                        });
                    }
                    LinkEvent::Closed => {
                        let link_id = address_hash_bytes(event.id);
                        append_server_log_path(
                            &log_path,
                            format!("reticulum-rs in-link closed link={}", hex_lower(&link_id)),
                        );
                        let _ = event_tx.send(OmenchatLinkEvent::LinkClosed {
                            link_id,
                            reason: Some("closed".into()),
                        });
                    }
                    LinkEvent::PeerIdentified(_) => {}
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    append_server_log_path(
                        &log_path,
                        format!("reticulum-rs in-link event receiver lagged skipped={skipped}"),
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn spawn_received_data_bridge(
    transport: Arc<Transport>,
    event_tx: mpsc::UnboundedSender<OmenchatLinkEvent>,
    log_path: std::path::PathBuf,
) {
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
                    let _ = event_tx.send(OmenchatLinkEvent::LinkData {
                        link_id,
                        context: 0,
                        data: event.data.as_slice().to_vec(),
                    });
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    append_server_log_path(
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
    event_tx: mpsc::UnboundedSender<OmenchatLinkEvent>,
    log_path: std::path::PathBuf,
) {
    let mut events = transport.resource_events();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    let ResourceEventKind::Complete(complete) = event.kind else {
                        continue;
                    };
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
                    let _ = event_tx.send(OmenchatLinkEvent::ResourceReceived {
                        link_id,
                        data: complete.data,
                        metadata: Some(metadata),
                    });
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    append_server_log_path(
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

fn append_server_log_path(path: &Path, message: impl AsRef<str>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{} {}", unix_timestamp(), message.as_ref());
    }
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

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn hex_lower(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
