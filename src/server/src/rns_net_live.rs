use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};
use crate::live::{OmenchatLinkEvent, OmenchatLiveServer};
use crate::protocol::codec::decode_frame;
use crate::protocol::{ChatOp, FrameBody};
use crate::session::{ServerPeer, SessionEngine};
use crate::store::OmenchatStore;
use crate::transport::{LinkId, OmenchatTransport, OMENCHAT_LINK_CONTEXT};

pub const OMENCHAT_RNS_APP_NAME: &str = "omenchat";
pub const NOMADNET_RNS_APP_NAME: &str = "nomadnetwork";

pub struct RnsNetLiveRuntime {
    announce_identity: rns_crypto::identity::Identity,
    announce_destination: rns_net::Destination,
    nomadnet_destination: rns_net::Destination,
    pub identity_hash: [u8; 16],
    pub destination_hash: [u8; 16],
    pub nomadnet_destination_hash: [u8; 16],
    pub destination_name: String,
    pub nomadnet_destination_name: String,
    pub event_rx: Receiver<OmenchatLinkEvent>,
    pub live_server: OmenchatLiveServer<RnsNetOmenchatTransport>,
}

pub struct RnsNetOmenchatTransport {
    node: Arc<rns_net::RnsNode>,
    log_path: Option<PathBuf>,
    sent_frames: u64,
    offered_resources: u64,
    sent_frame_bytes: u64,
    offered_resource_bytes: u64,
}

impl RnsNetOmenchatTransport {
    pub fn new(node: rns_net::RnsNode) -> Self {
        Self {
            node: Arc::new(node),
            log_path: None,
            sent_frames: 0,
            offered_resources: 0,
            sent_frame_bytes: 0,
            offered_resource_bytes: 0,
        }
    }

    pub fn from_shared_node(node: Arc<rns_net::RnsNode>) -> Self {
        Self {
            node,
            log_path: None,
            sent_frames: 0,
            offered_resources: 0,
            sent_frame_bytes: 0,
            offered_resource_bytes: 0,
        }
    }

    pub fn with_log_path(mut self, log_path: PathBuf) -> Self {
        self.log_path = Some(log_path);
        self
    }

    fn interface_stats_lines(&self) -> Vec<String> {
        match self.node.query(rns_net::QueryRequest::InterfaceStats) {
            Ok(rns_net::QueryResponse::InterfaceStats(stats)) => {
                let mut lines = vec![format!(
                    "interfaces: {} | transport: {} | received: {} | sent: {}",
                    stats.interfaces.len(),
                    stats.transport_enabled,
                    human_bytes(stats.total_rxb),
                    human_bytes(stats.total_txb)
                )];
                for interface in stats.interfaces {
                    lines.push(format!(
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
                    ));
                }
                lines
            }
            Ok(other) => vec![format!(
                "interface stats unavailable: unexpected response {other:?}"
            )],
            Err(_) => vec!["interface stats unavailable: query failed".into()],
        }
    }

    pub fn announce_destination(
        &self,
        destination: &rns_net::Destination,
        identity: &rns_crypto::identity::Identity,
        app_data: Option<&[u8]>,
    ) -> ServerResult<()> {
        self.node
            .announce(destination, identity, app_data)
            .map_err(|_| {
                ServerError::Message("rns-net failed to announce OMENchat link destination".into())
            })
    }
}

impl OmenchatTransport for RnsNetOmenchatTransport {
    fn send_frame(&mut self, link_id: LinkId, frame_bytes: Vec<u8>) -> ServerResult<()> {
        let byte_len = frame_bytes.len() as u64;
        let frame_summary = describe_frame_bytes(&frame_bytes);
        self.node
            .send_on_link(link_id, frame_bytes, OMENCHAT_LINK_CONTEXT)
            .map_err(|_| {
                ServerError::Message("rns-net failed to send OMENchat link frame".into())
            })?;
        self.sent_frames = self.sent_frames.saturating_add(1);
        self.sent_frame_bytes = self.sent_frame_bytes.saturating_add(byte_len);
        if let Some(log_path) = self
            .log_path
            .as_ref()
            .filter(|_| !is_heartbeat_frame_bytes(&frame_summary))
        {
            append_server_log_path(
                log_path,
                format!(
                    "link send link={} context=0x{:02x} bytes={} {}",
                    hex_lower(&link_id),
                    OMENCHAT_LINK_CONTEXT,
                    byte_len,
                    frame_summary
                ),
            );
        }
        Ok(())
    }

    fn offer_resource(
        &mut self,
        link_id: LinkId,
        _resource_id: String,
        payload: Vec<u8>,
        metadata: Vec<u8>,
    ) -> ServerResult<()> {
        let byte_len = payload.len() as u64;
        self.node
            .send_resource(link_id, payload, Some(metadata))
            .map_err(|_| {
                ServerError::Message("rns-net failed to send OMENchat resource payload".into())
            })?;
        self.offered_resources = self.offered_resources.saturating_add(1);
        self.offered_resource_bytes = self.offered_resource_bytes.saturating_add(byte_len);
        Ok(())
    }

    fn sent_frame_count(&self) -> u64 {
        self.sent_frames
    }

    fn offered_resource_count(&self) -> u64 {
        self.offered_resources
    }

    fn sent_frame_bytes(&self) -> u64 {
        self.sent_frame_bytes
    }

    fn offered_resource_bytes(&self) -> u64 {
        self.offered_resource_bytes
    }

    fn close_link(&mut self, link_id: LinkId) -> ServerResult<()> {
        self.node
            .teardown_link(link_id)
            .map_err(|_| ServerError::Message("rns-net failed to close OMENchat link".into()))
    }
}

pub fn start_live_server(config: &ServerConfig) -> ServerResult<RnsNetLiveRuntime> {
    crate::config::init_files(config)?;

    let live_identity = load_live_identity(config)?;
    let destination = destination_for_identity(config, live_identity.identity_hash);
    let nomadnet_destination = nomadnet_destination_for_identity(live_identity.identity_hash);
    let (callbacks, event_rx) = RnsNetOmenchatCallbacks::channel();
    let node =
        rns_net::RnsNode::from_config(Some(&config.reticulum_config_path), Box::new(callbacks))?;

    node.register_link_destination(
        destination.hash.0,
        live_identity.signing_private_key,
        live_identity.signing_public_key,
        1,
    )
    .map_err(|_| {
        ServerError::Message("rns-net failed to register OMENchat link destination".into())
    })?;
    node.register_link_destination(
        nomadnet_destination.hash.0,
        live_identity.signing_private_key,
        live_identity.signing_public_key,
        1,
    )
    .map_err(|_| {
        ServerError::Message(
            "rns-net failed to register OMENchat NomadNet portal destination".into(),
        )
    })?;
    let portal_path =
        crate::config::ensure_nomadnet_portal(config, &hex_lower(&destination.hash.0))?;
    for path in crate::config::nomadnet_portal_paths(config) {
        let portal_path = portal_path.clone();
        node.register_request_handler(&path, None, move |_link_id, _path, _data, _remote| {
            std::fs::read(&portal_path).ok()
        })
        .map_err(|_| {
            ServerError::Message(format!(
                "rns-net failed to register OMENchat NomadNet portal handler for {path}"
            ))
        })?;
    }
    node.announce(
        &destination,
        &live_identity.identity,
        Some(config.name.as_bytes()),
    )
    .map_err(|_| {
        ServerError::Message("rns-net failed to announce OMENchat link destination".into())
    })?;
    node.announce(
        &nomadnet_destination,
        &live_identity.identity,
        Some(config.name.as_bytes()),
    )
    .map_err(|_| {
        ServerError::Message(
            "rns-net failed to announce OMENchat NomadNet portal destination".into(),
        )
    })?;

    let store = OmenchatStore::open(&config.database_path)?;
    let engine =
        SessionEngine::with_limits_and_motd(store, config.into(), Some(config.motd.clone()));
    let transport = RnsNetOmenchatTransport::new(node).with_log_path(config.log_path());
    let destination_name = format!(
        "{}.{}",
        OMENCHAT_RNS_APP_NAME,
        destination.aspects.join(".")
    );
    let destination_hash = destination.hash.0;
    let nomadnet_destination_name = "nomadnetwork.node".to_string();
    let nomadnet_destination_hash = nomadnet_destination.hash.0;

    Ok(RnsNetLiveRuntime {
        announce_identity: live_identity.identity,
        announce_destination: destination,
        nomadnet_destination,
        identity_hash: live_identity.identity_hash,
        destination_hash,
        nomadnet_destination_hash,
        destination_name,
        nomadnet_destination_name,
        event_rx,
        live_server: OmenchatLiveServer::new(engine, transport),
    })
}

pub fn configured_destination_status(config: &ServerConfig) -> ServerResult<String> {
    let live_identity = load_live_identity(config)?;
    let destination = destination_for_identity(config, live_identity.identity_hash);
    let nomadnet_destination = nomadnet_destination_for_identity(live_identity.identity_hash);
    let destination_name = format!(
        "{}.{}",
        OMENCHAT_RNS_APP_NAME,
        destination.aspects.join(".")
    );
    let omenchat_destination_hash = hex_lower(&destination.hash.0);
    crate::config::ensure_nomadnet_portal(config, &omenchat_destination_hash)?;
    let nomadnet_destination_hash = hex_lower(&nomadnet_destination.hash.0);
    Ok(format!(
        "identity hash: {}\ndestination: {} ({})\nclient uri: omenchat://{}\nnomadnet portal: nomadnetwork.node ({}) path={}\nportal url: {}:{}\n",
        hex_lower(&live_identity.identity_hash),
        destination_name,
        omenchat_destination_hash,
        omenchat_destination_hash,
        nomadnet_destination_hash,
        crate::config::NOMADNET_PORTAL_PATH,
        nomadnet_destination_hash,
        crate::config::NOMADNET_PORTAL_PATH
    ))
}

pub fn run_live_server(config: ServerConfig) -> ServerResult<()> {
    const LIVE_RUNTIME_RESTART_BACKOFF: Duration = Duration::from_secs(5);
    append_server_log(
        &config,
        format!(
            "live server starting config={} reticulum_config={} announce_interval_minutes={}",
            config.config_path.display(),
            config.reticulum_config_file().display(),
            config.announce_interval_minutes.max(1)
        ),
    );
    let mut runtime = match start_live_server(&config) {
        Ok(runtime) => runtime,
        Err(error) => {
            append_server_log(&config, format!("live server startup failed: {error}"));
            return Err(error);
        }
    };
    let announce_interval = Duration::from_secs(config.announce_interval_minutes.max(1) * 60);
    let stats_interval = Duration::from_secs(30);
    let interface_stats_interval = Duration::from_secs(30);
    let mut next_announce = Instant::now() + announce_interval;
    let mut next_stats = Instant::now() + stats_interval;
    let mut next_interface_stats = Instant::now() + interface_stats_interval;
    let mut last_reported_stats = runtime.live_server.stats().clone();
    println!("omenchatd live server ready");
    println!("identity: {}", hex_lower(&runtime.identity_hash));
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
    println!(
        "portal url: {}:{}",
        hex_lower(&runtime.nomadnet_destination_hash),
        crate::config::NOMADNET_PORTAL_PATH
    );
    println!(
        "nomadnet page file: {}",
        config.nomadnet_index_page_path().display()
    );
    println!("database: {}", config.database_path.display());
    println!("reticulum: {}", config.reticulum_config_path.display());
    append_server_log(
        &config,
        format!(
            "live server ready destination={} hash={} client_uri=omenchat://{} nomadnet_portal={} nomadnet_hash={} portal_url={}:{} page={} page_file={} database={} reticulum={}",
            runtime.destination_name,
            hex_lower(&runtime.destination_hash),
            hex_lower(&runtime.destination_hash),
            runtime.nomadnet_destination_name,
            hex_lower(&runtime.nomadnet_destination_hash),
            hex_lower(&runtime.nomadnet_destination_hash),
            crate::config::NOMADNET_PORTAL_PATH,
            crate::config::NOMADNET_PORTAL_PATH,
            config.nomadnet_index_page_path().display(),
            config.database_path.display(),
            config.reticulum_config_path.display()
        ),
    );
    append_server_log(
        &config,
        format!(
            "startup announce sent destination={} hash={} nomadnet_portal={} nomadnet_hash={} next_announce_minutes={}",
            runtime.destination_name,
            hex_lower(&runtime.destination_hash),
            runtime.nomadnet_destination_name,
            hex_lower(&runtime.nomadnet_destination_hash),
            config.announce_interval_minutes.max(1)
        ),
    );
    let mut last_interface_stats = emit_interface_stats(&config, &runtime, true);

    loop {
        match runtime.event_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => {
                if should_log_live_event(&event) {
                    append_server_log(&config, describe_live_event(&event));
                }
                runtime.live_server.handle_event(event)?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                append_server_log(
                    &config,
                    format!(
                        "live event channel disconnected; restarting rns-net runtime after {}s",
                        LIVE_RUNTIME_RESTART_BACKOFF.as_secs()
                    ),
                );
                println!(
                    "live event channel disconnected; restarting rns-net runtime after {}s",
                    LIVE_RUNTIME_RESTART_BACKOFF.as_secs()
                );
                std::thread::sleep(LIVE_RUNTIME_RESTART_BACKOFF);
                runtime = match start_live_server(&config) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        append_server_log(&config, format!("live runtime restart failed: {error}"));
                        return Err(error);
                    }
                };
                append_server_log(
                    &config,
                    format!(
                        "live runtime restarted destination={} hash={} nomadnet_portal={} nomadnet_hash={}",
                        runtime.destination_name,
                        hex_lower(&runtime.destination_hash),
                        runtime.nomadnet_destination_name,
                        hex_lower(&runtime.nomadnet_destination_hash),
                    ),
                );
                next_announce = Instant::now() + announce_interval;
                next_stats = Instant::now() + stats_interval;
                next_interface_stats = Instant::now() + interface_stats_interval;
                last_reported_stats = runtime.live_server.stats().clone();
                last_interface_stats = emit_interface_stats(&config, &runtime, true);
                continue;
            }
        }
        if Instant::now() >= next_announce {
            match runtime.announce() {
                Ok(()) => {
                    append_server_log(
                        &config,
                        format!(
                            "announced destination={} hash={}",
                            runtime.destination_name,
                            hex_lower(&runtime.destination_hash)
                        ),
                    );
                }
                Err(error) => {
                    append_server_log(
                        &config,
                        format!(
                            "announce failed: {error}; restarting rns-net runtime after {}s",
                            LIVE_RUNTIME_RESTART_BACKOFF.as_secs()
                        ),
                    );
                    println!(
                        "announce failed: {error}; restarting rns-net runtime after {}s",
                        LIVE_RUNTIME_RESTART_BACKOFF.as_secs()
                    );
                    std::thread::sleep(LIVE_RUNTIME_RESTART_BACKOFF);
                    runtime = start_live_server(&config)?;
                    append_server_log(
                        &config,
                        format!(
                            "live runtime restarted after announce failure destination={} hash={}",
                            runtime.destination_name,
                            hex_lower(&runtime.destination_hash),
                        ),
                    );
                    last_reported_stats = runtime.live_server.stats().clone();
                    last_interface_stats = emit_interface_stats(&config, &runtime, true);
                }
            }
            next_announce = Instant::now() + announce_interval;
        }
        if Instant::now() >= next_stats {
            let stats = runtime.live_server.stats();
            if stats != &last_reported_stats {
                println!("{}", stats.summary_line());
                append_server_log(&config, stats.summary_line());
                if let Some(error) = stats.last_error.as_deref() {
                    println!("last_error: {error}");
                    append_server_log(&config, format!("last_error: {error}"));
                }
                last_reported_stats = stats.clone();
            }
            next_stats = Instant::now() + stats_interval;
        }
        if Instant::now() >= next_interface_stats {
            let interface_stats = runtime.live_server.transport().interface_stats_lines();
            if interface_stats != last_interface_stats {
                for line in &interface_stats {
                    println!("{line}");
                    append_server_log(&config, line);
                }
                last_interface_stats = interface_stats;
            }
            next_interface_stats = Instant::now() + interface_stats_interval;
        }
    }
}

fn emit_interface_stats(
    config: &ServerConfig,
    runtime: &RnsNetLiveRuntime,
    print: bool,
) -> Vec<String> {
    let lines = runtime.live_server.transport().interface_stats_lines();
    for line in &lines {
        if print {
            println!("{line}");
        }
        append_server_log(config, line);
    }
    lines
}

impl RnsNetLiveRuntime {
    pub fn announce(&mut self) -> ServerResult<()> {
        self.live_server.transport_mut().announce_destination(
            &self.announce_destination,
            &self.announce_identity,
            Some(b"OMENchat Node"),
        )?;
        self.live_server.transport_mut().announce_destination(
            &self.nomadnet_destination,
            &self.announce_identity,
            Some(b"OMENchat Portal"),
        )
    }

    pub fn interface_stats_lines(&self) -> Vec<String> {
        self.live_server.transport().interface_stats_lines()
    }
}

pub fn drain_live_events(
    runtime: &mut RnsNetLiveRuntime,
    max_events: usize,
) -> ServerResult<usize> {
    drain_live_events_inner(runtime, max_events, None)
}

pub fn drain_live_events_logged(
    runtime: &mut RnsNetLiveRuntime,
    max_events: usize,
    config: &ServerConfig,
) -> ServerResult<usize> {
    drain_live_events_inner(runtime, max_events, Some(config))
}

fn drain_live_events_inner(
    runtime: &mut RnsNetLiveRuntime,
    max_events: usize,
    log_config: Option<&ServerConfig>,
) -> ServerResult<usize> {
    let mut drained = 0usize;
    while drained < max_events {
        match runtime.event_rx.try_recv() {
            Ok(event) => {
                if let Some(config) = log_config {
                    if should_log_live_event(&event) {
                        append_server_log(config, describe_live_event(&event));
                    }
                }
                runtime.live_server.handle_event(event)?;
                drained += 1;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }
    Ok(drained)
}

fn append_server_log(config: &ServerConfig, message: impl AsRef<str>) {
    let path = config.log_path();
    append_server_log_path(&path, message);
}

fn append_server_log_path(path: &std::path::Path, message: impl AsRef<str>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let timestamp = utc_now_string();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{timestamp} {}", message.as_ref());
    }
}

fn utc_now_string() -> String {
    let unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    unix_to_utc_string(unix_secs)
}

fn unix_to_utc_string(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let seconds_of_day = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

fn describe_live_event(event: &OmenchatLinkEvent) -> String {
    match event {
        OmenchatLinkEvent::LinkOpened { link_id, peer } => format!(
            "link opened link={} peer={} identity={}",
            hex_lower(link_id),
            peer.display_name,
            hex_vec(&peer.identity_hash)
        ),
        OmenchatLinkEvent::LinkData {
            link_id,
            context,
            data,
        } => {
            let decoded = match decode_frame(data) {
                Ok(frame) => format!(
                    " op={:?} seq={} room={:?} body={}",
                    frame.op,
                    frame.seq,
                    frame.room_id,
                    frame_body_summary(&frame.body)
                ),
                Err(error) => format!(" op=decode_error error={error}"),
            };
            format!(
                "link data link={} context=0x{context:02x} bytes={}{}",
                hex_lower(link_id),
                data.len(),
                decoded
            )
        }
        OmenchatLinkEvent::ResourceReceived {
            link_id,
            data,
            metadata,
        } => {
            let resource =
                resource_id_from_metadata(metadata.as_deref()).unwrap_or_else(|| "unknown".into());
            format!(
                "resource received link={} resource={} bytes={}",
                hex_lower(link_id),
                resource,
                data.len()
            )
        }
        OmenchatLinkEvent::LinkClosed { link_id, reason } => {
            let reason = reason.as_deref().unwrap_or("unknown");
            format!("link closed link={} reason={reason}", hex_lower(link_id))
        }
    }
}

fn resource_id_from_metadata(metadata: Option<&[u8]>) -> Option<String> {
    let metadata = metadata?;
    let id = metadata.strip_prefix(crate::transport::OMENCHAT_RESOURCE_METADATA_PREFIX)?;
    String::from_utf8(id.to_vec())
        .ok()
        .filter(|value| !value.is_empty())
}

fn should_log_live_event(event: &OmenchatLinkEvent) -> bool {
    match event {
        OmenchatLinkEvent::LinkData { data, .. } => !is_heartbeat_data(data),
        _ => true,
    }
}

fn is_heartbeat_data(data: &[u8]) -> bool {
    decode_frame(data)
        .map(|frame| matches!(frame.op, ChatOp::Ping | ChatOp::Pong))
        .unwrap_or(false)
}

fn is_heartbeat_frame_bytes(summary: &str) -> bool {
    summary.starts_with("op=Ping ") || summary.starts_with("op=Pong ")
}

fn describe_frame_bytes(data: &[u8]) -> String {
    match decode_frame(data) {
        Ok(frame) => format!(
            "op={:?} seq={} room={:?} body={}",
            frame.op,
            frame.seq,
            frame.room_id,
            frame_body_summary(&frame.body)
        ),
        Err(error) => format!("op=decode_error error={error}"),
    }
}

fn frame_body_summary(body: &FrameBody) -> String {
    match body {
        FrameBody::Empty => "empty".to_string(),
        FrameBody::Text(text) => format!("text:{}", text.len()),
        FrameBody::Fields(fields) => format!("fields:{}", fields.len()),
    }
}

struct LiveIdentity {
    identity: rns_crypto::identity::Identity,
    identity_hash: [u8; 16],
    signing_private_key: [u8; 32],
    signing_public_key: [u8; 32],
}

fn load_live_identity(config: &ServerConfig) -> ServerResult<LiveIdentity> {
    let identity = rns_net::storage::load_identity(&config.identity_path)?;
    let private_key = identity
        .get_private_key()
        .ok_or_else(|| ServerError::Message("OMENchat identity has no private key".into()))?;
    let public_key = identity
        .get_public_key()
        .ok_or_else(|| ServerError::Message("OMENchat identity has no public key".into()))?;
    let mut signing_private_key = [0u8; 32];
    signing_private_key.copy_from_slice(&private_key[32..64]);
    let mut signing_public_key = [0u8; 32];
    signing_public_key.copy_from_slice(&public_key[32..64]);
    let identity_hash = *identity.hash();

    Ok(LiveIdentity {
        identity,
        identity_hash,
        signing_private_key,
        signing_public_key,
    })
}

fn destination_for_identity(
    _config: &ServerConfig,
    identity_hash: [u8; 16],
) -> rns_net::Destination {
    let aspects = normalized_destination_aspects(crate::config::OMENCHAT_DESTINATION_ASPECT);
    let aspect_refs = aspects.iter().map(String::as_str).collect::<Vec<_>>();
    rns_net::Destination::single_in(
        OMENCHAT_RNS_APP_NAME,
        &aspect_refs,
        rns_net::IdentityHash(identity_hash),
    )
}

fn nomadnet_destination_for_identity(identity_hash: [u8; 16]) -> rns_net::Destination {
    rns_net::Destination::single_in(
        NOMADNET_RNS_APP_NAME,
        &["node"],
        rns_net::IdentityHash(identity_hash),
    )
}

fn normalized_destination_aspects(configured: &str) -> Vec<String> {
    let trimmed = configured.trim();
    let without_app = trimmed
        .strip_prefix(OMENCHAT_RNS_APP_NAME)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(trimmed);
    let aspects = without_app
        .split('.')
        .map(str::trim)
        .filter(|aspect| !aspect.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if aspects.is_empty() {
        vec!["node".to_string()]
    } else {
        aspects
    }
}

pub struct RnsNetOmenchatCallbacks {
    event_tx: Sender<OmenchatLinkEvent>,
}

impl RnsNetOmenchatCallbacks {
    pub fn new(event_tx: Sender<OmenchatLinkEvent>) -> Self {
        Self { event_tx }
    }

    pub fn channel() -> (Self, Receiver<OmenchatLinkEvent>) {
        let (event_tx, event_rx) = channel();
        (Self::new(event_tx), event_rx)
    }

    fn send_event(&self, event: OmenchatLinkEvent) {
        let _ = self.event_tx.send(event);
    }
}

impl rns_net::Callbacks for RnsNetOmenchatCallbacks {
    fn on_announce(&mut self, _announced: rns_net::common::destination::AnnouncedIdentity) {}

    fn on_path_updated(&mut self, _dest_hash: rns_net::DestHash, _hops: u8) {}

    fn on_local_delivery(
        &mut self,
        _dest_hash: rns_net::DestHash,
        _raw: Vec<u8>,
        _packet_hash: rns_net::PacketHash,
    ) {
    }

    fn on_link_established(
        &mut self,
        link_id: rns_net::LinkId,
        dest_hash: rns_net::DestHash,
        _rtt: f64,
        is_initiator: bool,
    ) {
        if is_initiator {
            return;
        }
        self.send_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_id.0,
            peer: provisional_peer(link_id.0, dest_hash.0),
        });
    }

    fn on_link_closed(
        &mut self,
        link_id: rns_net::LinkId,
        reason: Option<rns_net::TeardownReason>,
    ) {
        self.send_event(OmenchatLinkEvent::LinkClosed {
            link_id: link_id.0,
            reason: reason.map(|reason| format!("{reason:?}")),
        });
    }

    fn on_remote_identified(
        &mut self,
        link_id: rns_net::LinkId,
        identity_hash: rns_net::IdentityHash,
        _public_key: [u8; 64],
    ) {
        self.send_event(OmenchatLinkEvent::LinkOpened {
            link_id: link_id.0,
            peer: identified_peer(identity_hash.0),
        });
    }

    fn on_link_data(&mut self, link_id: rns_net::LinkId, context: u8, data: Vec<u8>) {
        self.send_event(OmenchatLinkEvent::LinkData {
            link_id: link_id.0,
            context,
            data,
        });
    }

    fn on_resource_received(
        &mut self,
        link_id: rns_net::LinkId,
        data: Vec<u8>,
        metadata: Option<Vec<u8>>,
    ) {
        self.send_event(OmenchatLinkEvent::ResourceReceived {
            link_id: link_id.0,
            data,
            metadata,
        });
    }

    fn on_resource_accept_query(
        &mut self,
        _link_id: rns_net::LinkId,
        _resource_hash: Vec<u8>,
        _transfer_size: u64,
        has_metadata: bool,
    ) -> bool {
        has_metadata
    }
}

fn provisional_peer(link_id: LinkId, destination_hash: [u8; 16]) -> ServerPeer {
    ServerPeer {
        identity_hash: link_id.to_vec(),
        display_name: format!("link-{}", short_hex(&destination_hash)),
        lxmf_destination: None,
    }
}

fn identified_peer(identity_hash: [u8; 16]) -> ServerPeer {
    ServerPeer {
        identity_hash: identity_hash.to_vec(),
        display_name: format!("peer-{}", short_hex(&identity_hash)),
        lxmf_destination: None,
    }
}

fn short_hex(bytes: &[u8; 16]) -> String {
    bytes
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_lower(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_vec(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use rns_net::Callbacks;

    #[test]
    fn callbacks_emit_live_events_for_inbound_links_and_data() {
        let (mut callbacks, rx) = RnsNetOmenchatCallbacks::channel();
        let link_id = rns_net::LinkId([7u8; 16]);
        let dest_hash = rns_net::DestHash([8u8; 16]);

        callbacks.on_link_established(link_id, dest_hash, 0.1, false);
        callbacks.on_remote_identified(link_id, rns_net::IdentityHash([9u8; 16]), [1u8; 64]);
        callbacks.on_link_data(link_id, OMENCHAT_LINK_CONTEXT, b"frame".to_vec());
        callbacks.on_link_closed(link_id, None);

        let opened = rx.recv().expect("opened");
        assert!(matches!(
            opened,
            OmenchatLinkEvent::LinkOpened { link_id, ref peer }
                if link_id == [7u8; 16] && peer.display_name == "link-08080808"
        ));
        let identified = rx.recv().expect("identified");
        assert!(matches!(
            identified,
            OmenchatLinkEvent::LinkOpened { link_id, ref peer }
                if link_id == [7u8; 16] && peer.display_name == "peer-09090909"
        ));
        assert!(matches!(
            rx.recv().expect("data"),
            OmenchatLinkEvent::LinkData {
                link_id,
                context: OMENCHAT_LINK_CONTEXT,
                ref data,
            } if link_id == [7u8; 16] && data == b"frame"
        ));
        assert!(matches!(
            rx.recv().expect("closed"),
            OmenchatLinkEvent::LinkClosed { link_id, .. } if link_id == [7u8; 16]
        ));
    }

    #[test]
    fn callbacks_ignore_outbound_link_established_events() {
        let (mut callbacks, rx) = RnsNetOmenchatCallbacks::channel();

        callbacks.on_link_established(
            rns_net::LinkId([7u8; 16]),
            rns_net::DestHash([8u8; 16]),
            0.1,
            true,
        );

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn resource_accept_requires_metadata() {
        let (mut callbacks, _rx) = RnsNetOmenchatCallbacks::channel();

        assert!(callbacks.on_resource_accept_query(rns_net::LinkId([1u8; 16]), vec![], 10, true));
        assert!(!callbacks.on_resource_accept_query(rns_net::LinkId([1u8; 16]), vec![], 10, false));
    }

    #[test]
    fn destination_uses_omenchat_app_and_configured_aspect() {
        let config = ServerConfig::for_root(std::env::temp_dir().join("unused-omenchatd"));
        let destination = destination_for_identity(&config, [3u8; 16]);

        assert_eq!(destination.app_name, OMENCHAT_RNS_APP_NAME);
        assert_eq!(destination.aspects, vec!["node"]);
        assert_eq!(
            destination.identity_hash,
            Some(rns_net::IdentityHash([3u8; 16]))
        );
    }

    #[test]
    fn destination_normalizes_legacy_dotted_aspect() {
        let mut config = ServerConfig::for_root(std::env::temp_dir().join("unused-omenchatd"));
        config.chat_aspect = "omenchat.node".into();

        let destination = destination_for_identity(&config, [3u8; 16]);

        assert_eq!(destination.aspects, vec!["node"]);
    }

    #[test]
    fn live_log_timestamps_are_readable_utc() {
        assert_eq!(unix_to_utc_string(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(unix_to_utc_string(1_700_000_000), "2023-11-14 22:13:20 UTC");
    }
}
