use super::*;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use rns_transport::delivery::await_link_activation;
use rns_transport::destination::link::LinkStatus;
use rns_transport::iface::tcp_client::TcpClient;
use rns_transport::iface::tcp_server::TcpServer;
use rns_transport::iface::udp::UdpInterface;
use rns_transport::packet::PacketDataBuffer;
use rns_transport::resource::ResourceRequest;

struct ResourceDiagnosticLogger;

impl log::Log for ResourceDiagnosticLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Debug && metadata.target().starts_with("rns_transport")
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let message = record.args().to_string();
            if message.contains("[resource-diag]") || message.contains("resource transfer failed") {
                eprintln!("upstream {}: {message}", record.level());
            }
        }
    }

    fn flush(&self) {}
}

static RESOURCE_DIAGNOSTIC_LOGGER: ResourceDiagnosticLogger = ResourceDiagnosticLogger;

fn install_resource_diagnostic_logger() {
    if log::set_logger(&RESOURCE_DIAGNOSTIC_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

const ROLE_ENV: &str = "OMENCHATD_RESOURCE_MULTIPROCESS_ROLE";
const ROOT_ENV: &str = "OMENCHATD_RESOURCE_MULTIPROCESS_ROOT";
const NONCE_ENV: &str = "OMENCHATD_RESOURCE_MULTIPROCESS_NONCE";
const SERVER_PORT_ENV: &str = "OMENCHATD_RESOURCE_MULTIPROCESS_SERVER_PORT";
const CLIENT_PORT_ENV: &str = "OMENCHATD_RESOURCE_MULTIPROCESS_CLIENT_PORT";
const TEST_NAME: &str =
    "reticulum_live::multiprocess_tests::reticulum_multiprocess_resource_complete_cancel_reuse";
const SPLIT_SENTINEL_TEST_NAME: &str =
    "reticulum_live::multiprocess_tests::reticulum_split_metadata_assembly_preserves_segment_two_payload";
const SPLIT_SENTINEL_ENV: &str = "OMENCHATD_RESOURCE_SPLIT_METADATA_SENTINEL";

fn reserve_udp_ports() -> (u16, u16) {
    let first = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve first UDP port");
    let second = std::net::UdpSocket::bind("127.0.0.1:0").expect("reserve second UDP port");
    let first_port = first.local_addr().expect("first UDP address").port();
    let second_port = second.local_addr().expect("second UDP address").port();
    assert_ne!(first_port, second_port);
    (first_port, second_port)
}

fn child_value(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing child environment {name}"))
}

fn child_port(name: &str) -> u16 {
    child_value(name)
        .parse()
        .unwrap_or_else(|_| panic!("invalid child port {name}"))
}

fn marker(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

fn publish_marker(path: &Path) {
    std::fs::write(path, b"ready\n").expect("publish process coordination marker");
}

fn append_trace(root: &Path, role: &str, line: &str) {
    use std::io::Write as _;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join(format!("{role}-trace.txt")))
        .expect("open process trace");
    writeln!(file, "{line}").expect("append process trace");
}

async fn wait_marker(path: &Path, wait: Duration) {
    tokio::time::timeout(wait, async {
        while !path.is_file() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for marker {}", path.display()));
}

fn transport_config(name: &str, identity: &PrivateIdentity) -> TransportConfig {
    let mut config = TransportConfig::new(name, identity, true);
    config.set_resource_retry_interval_secs(1);
    config.set_resource_retry_limit(10);
    config
}

async fn attach_plain_udp(transport: &Transport, bind_port: u16, peer_port: u16) {
    transport.iface_manager().lock().await.spawn(
        UdpInterface::new(
            format!("127.0.0.1:{bind_port}"),
            Some(format!("127.0.0.1:{peer_port}")),
        ),
        UdpInterface::spawn,
    );
    tokio::time::sleep(Duration::from_millis(150)).await;
}

async fn attach_plain_tcp_server(transport: &Transport, port: u16) {
    let manager = transport.iface_manager();
    let server = TcpServer::new(format!("127.0.0.1:{port}"), manager.clone());
    let context = manager.lock().await.new_context(server);
    tokio::spawn(TcpServer::spawn(context));
    tokio::time::sleep(Duration::from_millis(150)).await;
}

async fn attach_plain_tcp_client(transport: &Transport, port: u16) {
    let manager = transport.iface_manager();
    let client = TcpClient::new(format!("127.0.0.1:{port}"));
    let context = manager.lock().await.new_context(client);
    tokio::spawn(TcpClient::spawn(context));
    tokio::time::sleep(Duration::from_millis(250)).await;
}

async fn receiver_child(root: &Path, nonce: &str, server_port: u16, client_port: u16) {
    let server_identity =
        PrivateIdentity::new_from_name(&format!("omenchatd-multiprocess-server-{nonce}"));
    let transport = Transport::new(transport_config(
        "omenchatd-resource-multiprocess-server",
        &server_identity,
    ));
    let destination = transport
        .add_destination(
            server_identity,
            DestinationName::new(OMENCHAT_RNS_APP_NAME, "resource-multiprocess"),
        )
        .await;
    let transport = Arc::new(transport);
    attach_plain_udp(&transport, server_port, client_port).await;

    let mut wire_events = transport.iface_rx();
    let mut resource_events = transport.resource_events();
    let announce_shutdown = CancellationToken::new();
    let announce_task = {
        let transport = transport.clone();
        let destination = destination.clone();
        let shutdown = announce_shutdown.clone();
        tokio::spawn(async move {
            while !shutdown.is_cancelled() {
                transport.send_announce(&destination, None).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    };
    publish_marker(&marker(root, "receiver-ready"));

    let mut advertisements = 0usize;
    let mut first_complete = false;
    let mut cancel_received = false;
    let mut reuse_complete = false;
    tokio::time::timeout(Duration::from_secs(25), async {
        while !(first_complete && cancel_received && reuse_complete) {
            tokio::select! {
                wire = wire_events.recv() => {
                    let wire = wire.expect("receiver interface stream remains open");
                    match wire.packet.context {
                        PacketContext::ResourceAdvrtisement => {
                            append_trace(root, "receiver", "wire=advertisement");
                            advertisements += 1;
                            if advertisements == 2 {
                                publish_marker(&marker(root, "cancel-ready"));
                            }
                        }
                        PacketContext::ResourceInitiatorCancel => {
                            append_trace(root, "receiver", "wire=initiator_cancel");
                            cancel_received = true;
                        }
                        _ => {}
                    }
                }
                resource = resource_events.recv() => {
                    let resource = resource.expect("receiver Resource stream remains open");
                    match resource.kind {
                        ResourceEventKind::Complete(complete) => {
                            append_trace(root, "receiver", "event=complete");
                            assert_eq!(
                                complete.metadata,
                                Some(OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec())
                            );
                            match complete.data.first().copied() {
                                Some(0x11) => {
                                    assert_eq!(complete.data, vec![0x11; 4 * 1024]);
                                    first_complete = true;
                                }
                                Some(0x33) => {
                                    assert_eq!(complete.data, vec![0x33; 4 * 1024]);
                                    reuse_complete = true;
                                }
                                value => panic!("unexpected completed Resource marker {value:?}"),
                            }
                        }
                        ResourceEventKind::InboundFailed(failure) => {
                            append_trace(root, "receiver", "event=inbound_failed");
                            panic!("receiver Resource failed: {}", failure.reason)
                        }
                        ResourceEventKind::Progress(_) => {
                            append_trace(root, "receiver", "event=progress");
                        }
                        _ => {}
                    }
                }
            }
        }
    })
    .await
    .expect("receiver complete/cancel/reuse sequence timed out");

    assert!(advertisements >= 3);
    announce_shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(1), announce_task)
        .await
        .expect("announce task shutdown")
        .expect("announce task join");
    assert!(transport.detach_interfaces().await >= 1);
    publish_marker(&marker(root, "receiver-complete"));
}

async fn wait_outbound_terminal(
    events: &mut tokio::sync::broadcast::Receiver<rns_transport::resource::ResourceEvent>,
    wire_events: &mut tokio::sync::broadcast::Receiver<rns_transport::iface::RxMessage>,
    link: &Arc<tokio::sync::Mutex<rns_transport::destination::link::Link>>,
    hash: rns_transport::hash::Hash,
    expected: LiveResourceOutcome,
    root: &Path,
) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let event = tokio::select! {
                event = events.recv() => event.expect("sender Resource stream remains open"),
                wire = wire_events.recv() => {
                    let wire = wire.expect("sender interface stream remains open");
                    match wire.packet.context {
                        PacketContext::ResourceRequest => {
                            let mut decrypted = PacketDataBuffer::new();
                            let plain_len = {
                                let link = link.lock().await;
                                let plain = link
                                    .decrypt(
                                        wire.packet.data.as_slice(),
                                        decrypted.accuire_buf_max(),
                                    )
                                    .expect("sender decrypts receiver Resource request");
                                plain.len()
                            };
                            decrypted.resize(plain_len);
                            let request = ResourceRequest::decode(decrypted.as_slice())
                                .expect("sender decodes receiver Resource request");
                            append_trace(
                                root,
                                "sender",
                                if request.resource_hash == hash {
                                    "wire=request hash_match=true"
                                } else {
                                    "wire=request hash_match=false"
                                },
                            );
                        }
                        PacketContext::ResourceProof => append_trace(root, "sender", "wire=proof"),
                        PacketContext::ResourceReceiverCancel => append_trace(root, "sender", "wire=receiver_cancel"),
                        _ => {}
                    }
                    continue;
                }
            };
            if event.hash != hash {
                continue;
            }
            let actual = match event.kind {
                ResourceEventKind::OutboundComplete => {
                    append_trace(root, "sender", "event=outbound_complete");
                    LiveResourceOutcome::Complete
                }
                ResourceEventKind::OutboundFailed => {
                    append_trace(root, "sender", "event=outbound_failed");
                    LiveResourceOutcome::Failed
                }
                ResourceEventKind::OutboundCancelled => {
                    append_trace(root, "sender", "event=outbound_cancelled");
                    LiveResourceOutcome::Cancelled
                }
                _ => continue,
            };
            assert_eq!(actual, expected);
            break;
        }
    })
    .await
    .expect("sender terminal event timed out");
}

async fn sender_child(root: &Path, nonce: &str, server_port: u16, client_port: u16) {
    append_trace(
        root,
        "sender",
        &format!(
            "udp_tx_buffer_capacity={} max_resource_wire_len={}",
            std::mem::size_of::<rns_transport::packet::Packet>() * 3,
            2 + rns_transport::hash::ADDRESS_HASH_SIZE + 1 + rns_transport::packet::PACKET_MDU,
        ),
    );
    let server_identity =
        PrivateIdentity::new_from_name(&format!("omenchatd-multiprocess-server-{nonce}"));
    let destination = SingleInputDestination::new(
        server_identity,
        DestinationName::new(OMENCHAT_RNS_APP_NAME, "resource-multiprocess"),
    )
    .desc;
    let client_identity =
        PrivateIdentity::new_from_name(&format!("omenchatd-multiprocess-client-{nonce}"));
    let transport = Arc::new(Transport::new(transport_config(
        "omenchatd-resource-multiprocess-client",
        &client_identity,
    )));
    attach_plain_udp(&transport, client_port, server_port).await;
    assert!(
        transport
            .await_path(&destination.address_hash, Duration::from_secs(5), None)
            .await,
        "sender must learn receiver path"
    );
    let link = transport.link(destination).await;
    await_link_activation(&transport, &link, Duration::from_secs(5))
        .await
        .expect("multiprocess link activation");
    let link_id = *link.lock().await.id();
    let mut resource_events = transport.resource_events();
    let mut wire_events = transport.iface_rx();

    let first_hash = transport
        .send_resource(
            &link_id,
            vec![0x11; 4 * 1024],
            Some(OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec()),
        )
        .await
        .expect("send first complete Resource");
    wait_outbound_terminal(
        &mut resource_events,
        &mut wire_events,
        &link,
        first_hash,
        LiveResourceOutcome::Complete,
        root,
    )
    .await;

    let cancelled_hash = transport
        .send_resource(
            &link_id,
            vec![0x22; 16 * 1024],
            Some(OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec()),
        )
        .await
        .expect("send cancellable Resource");
    wait_marker(&marker(root, "cancel-ready"), Duration::from_secs(5)).await;
    assert!(transport
        .cancel_resource(&link_id, cancelled_hash)
        .await
        .expect("send initiator cancellation"));
    wait_outbound_terminal(
        &mut resource_events,
        &mut wire_events,
        &link,
        cancelled_hash,
        LiveResourceOutcome::Cancelled,
        root,
    )
    .await;

    let reuse_hash = transport
        .send_resource(
            &link_id,
            vec![0x33; 4 * 1024],
            Some(OMENCHAT_RESOURCE_METADATA_PREFIX.to_vec()),
        )
        .await
        .expect("send post-cancel Resource");
    wait_outbound_terminal(
        &mut resource_events,
        &mut wire_events,
        &link,
        reuse_hash,
        LiveResourceOutcome::Complete,
        root,
    )
    .await;
    assert_eq!(link.lock().await.status(), LinkStatus::Active);
    assert!(transport.detach_interfaces().await >= 1);
    publish_marker(&marker(root, "sender-complete"));
}

fn spawn_role_for_test(
    test_name: &str,
    role: &str,
    root: &Path,
    nonce: &str,
    server_port: u16,
    client_port: u16,
) -> Child {
    Command::new(std::env::current_exe().expect("current test binary"))
        .args([
            "--exact",
            test_name,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ROLE_ENV, role)
        .env(ROOT_ENV, root)
        .env(NONCE_ENV, nonce)
        .env(SERVER_PORT_ENV, server_port.to_string())
        .env(CLIENT_PORT_ENV, client_port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn {role} child: {error}"))
}

fn spawn_role(role: &str, root: &Path, nonce: &str, server_port: u16, client_port: u16) -> Child {
    spawn_role_for_test(TEST_NAME, role, root, nonce, server_port, client_port)
}

fn spawn_split_sentinel_role(
    role: &str,
    root: &Path,
    nonce: &str,
    server_port: u16,
    client_port: u16,
) -> Child {
    Command::new(std::env::current_exe().expect("current test binary"))
        .args([
            "--exact",
            SPLIT_SENTINEL_TEST_NAME,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ROLE_ENV, role)
        .env(ROOT_ENV, root)
        .env(NONCE_ENV, nonce)
        .env(SERVER_PORT_ENV, server_port.to_string())
        .env(CLIENT_PORT_ENV, client_port.to_string())
        .env(SPLIT_SENTINEL_ENV, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn split sentinel {role}: {error}"))
}

async fn wait_child(mut child: Child, wait: Duration) -> (Output, bool) {
    let timed_out = tokio::time::timeout(wait, async {
        loop {
            if child.try_wait().expect("poll child").is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_err();
    if timed_out {
        let _ = child.kill();
    }
    let output = child.wait_with_output().expect("reap child");
    (output, timed_out)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit two-process Reticulum Resource completion/cancel/reuse interoperability test"]
async fn reticulum_multiprocess_resource_complete_cancel_reuse() {
    if let Ok(role) = std::env::var(ROLE_ENV) {
        install_resource_diagnostic_logger();
        let root = PathBuf::from(child_value(ROOT_ENV));
        let nonce = child_value(NONCE_ENV);
        let server_port = child_port(SERVER_PORT_ENV);
        let client_port = child_port(CLIENT_PORT_ENV);
        match role.as_str() {
            "receiver" => receiver_child(&root, &nonce, server_port, client_port).await,
            "sender" => sender_child(&root, &nonce, server_port, client_port).await,
            _ => panic!("unknown multiprocess role {role}"),
        }
        return;
    }

    let nonce = format!("{}-{}", std::process::id(), current_epoch_ms());
    let root = std::env::temp_dir().join(format!("omenchatd-resource-multiprocess-{nonce}"));
    std::fs::create_dir_all(&root).expect("create isolated multiprocess root");
    let (server_port, client_port) = reserve_udp_ports();
    let mut receiver = spawn_role("receiver", &root, &nonce, server_port, client_port);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !marker(&root, "receiver-ready").is_file() {
            if let Some(status) = receiver.try_wait().expect("poll receiver startup") {
                panic!("receiver exited before readiness: {status}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("receiver readiness timeout");

    let sender = spawn_role("sender", &root, &nonce, server_port, client_port);
    let (sender_output, sender_timeout) = wait_child(sender, Duration::from_secs(35)).await;
    let (receiver_output, receiver_timeout) = wait_child(receiver, Duration::from_secs(35)).await;
    assert!(
        !sender_timeout
            && !receiver_timeout
            && sender_output.status.success()
            && receiver_output.status.success(),
        "multiprocess Resource gate failed\nsender status={} timed_out={} stdout={} stderr={}\nreceiver status={} timed_out={} stdout={} stderr={}\nsender trace={}\nreceiver trace={}",
        sender_output.status,
        sender_timeout,
        String::from_utf8_lossy(&sender_output.stdout),
        String::from_utf8_lossy(&sender_output.stderr),
        receiver_output.status,
        receiver_timeout,
        String::from_utf8_lossy(&receiver_output.stdout),
        String::from_utf8_lossy(&receiver_output.stderr),
        std::fs::read_to_string(root.join("sender-trace.txt")).unwrap_or_default(),
        std::fs::read_to_string(root.join("receiver-trace.txt")).unwrap_or_default(),
    );
    assert!(marker(&root, "sender-complete").is_file());
    assert!(marker(&root, "receiver-complete").is_file());
    std::fs::remove_dir_all(&root).expect("remove isolated multiprocess root");
}

async fn split_sentinel_receiver_child(
    root: &Path,
    nonce: &str,
    server_port: u16,
    _client_port: u16,
) {
    let server_identity =
        PrivateIdentity::new_from_name(&format!("omenchatd-split-sentinel-server-{nonce}"));
    let transport = Transport::new(transport_config(
        "omenchatd-split-sentinel-server",
        &server_identity,
    ));
    let destination = transport
        .add_destination(
            server_identity,
            DestinationName::new(OMENCHAT_RNS_APP_NAME, "split-sentinel"),
        )
        .await;
    let transport = Arc::new(transport);
    attach_plain_tcp_server(&transport, server_port).await;
    let announce_shutdown = CancellationToken::new();
    let announce_task = {
        let transport = transport.clone();
        let shutdown = announce_shutdown.clone();
        tokio::spawn(async move {
            while !shutdown.is_cancelled() {
                transport.send_announce(&destination, None).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    };
    publish_marker(&marker(root, "split-receiver-ready"));

    let expected = std::fs::read(root.join("split-expected.bin")).expect("read split fixture");
    let (complete, saw_split) = tokio::time::timeout(Duration::from_secs(30), async {
        let mut events = transport.resource_events();
        let mut saw_split = false;
        loop {
            let event = events.recv().await.expect("split Resource stream open");
            match event.kind {
                ResourceEventKind::SegmentComplete(segment) => {
                    saw_split |= segment.total_segments > 1;
                }
                ResourceEventKind::Complete(complete) => break (complete, saw_split),
                ResourceEventKind::InboundFailed(failure) => {
                    panic!("split Resource failed before assembly: {}", failure.reason)
                }
                _ => {}
            }
        }
    })
    .await
    .expect("split Resource completion timeout");

    // This is the retained regression proof for issue #553 and PR #556. It
    // exercises the unmodified official 0.9.8 transport crate.
    assert!(saw_split, "fixture must exercise multi-segment assembly");
    assert_eq!(complete.metadata, Some(b"split-sentinel".to_vec()));
    assert_eq!(complete.data, expected);
    announce_shutdown.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(1), announce_task).await;
}

async fn split_sentinel_sender_child(root: &Path, nonce: &str, server_port: u16, client_port: u16) {
    let server_identity =
        PrivateIdentity::new_from_name(&format!("omenchatd-split-sentinel-server-{nonce}"));
    let destination = SingleInputDestination::new(
        server_identity,
        DestinationName::new(OMENCHAT_RNS_APP_NAME, "split-sentinel"),
    )
    .desc;
    let client_identity =
        PrivateIdentity::new_from_name(&format!("omenchatd-split-sentinel-client-{nonce}"));
    let transport = Arc::new(Transport::new(transport_config(
        "omenchatd-split-sentinel-client",
        &client_identity,
    )));
    let _ = client_port;
    attach_plain_tcp_client(&transport, server_port).await;
    assert!(
        transport
            .await_path(&destination.address_hash, Duration::from_secs(5), None)
            .await
    );
    let link = transport.link(destination).await;
    await_link_activation(&transport, &link, Duration::from_secs(5))
        .await
        .expect("split sentinel Link activation");
    let link_id = *link.lock().await.id();
    let payload = std::fs::read(root.join("split-expected.bin")).expect("read split fixture");
    transport
        .send_resource(&link_id, payload, Some(b"split-sentinel".to_vec()))
        .await
        .expect("dispatch split metadata Resource");
    wait_marker(
        &marker(root, "split-receiver-complete"),
        Duration::from_secs(35),
    )
    .await;
}

async fn run_python_nomadnet_rust_responder_matrix(source_env: &str, expected_rns: &str) {
    let rns_source = std::env::var_os(source_env)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{source_env} must name a Python RNS source"));
    let nonce = format!("{}-{}", std::process::id(), current_epoch_ms());
    let root = std::env::temp_dir().join(format!("omenchatd-python-responder-{nonce}"));
    let config = ServerConfig::for_root(root.join("server"));
    crate::config::init_files(&config).expect("initialize isolated Rust responder");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve TCP port");
    let port = listener.local_addr().expect("reserved TCP address").port();
    drop(listener);

    let identity = PrivateIdentity::new_from_name(&format!("omenchatd-python-responder-{nonce}"));
    let transport = Transport::new(transport_config(
        "omenchatd-python-nomadnet-responder",
        &identity,
    ));
    let destination = transport
        .add_destination(
            identity,
            DestinationName::new(NOMADNET_RNS_APP_NAME, "node"),
        )
        .await;
    let transport = Arc::new(transport);
    attach_plain_tcp_server(&transport, port).await;

    let budget = QueueBudget::new(EVENT_QUEUE_BYTES, EVENT_PER_LINK_BYTES);
    let (payload_tx, _payload_rx) = mpsc::channel(EVENT_QUEUE_ITEMS);
    let (control_tx, _control_rx) = mpsc::channel(EVENT_CONTROL_ITEMS);
    let event_tx = EventQueueSender {
        payload_tx,
        control_tx,
        budget,
        log_path: config.log_path(),
    };
    let shutdown = CancellationToken::new();
    let correlations = Arc::new(Mutex::new(OutboundResourceCorrelations::default()));
    let link_bridge = spawn_link_event_bridge(
        transport.clone(),
        event_tx.clone(),
        config.clone(),
        shutdown.clone(),
        correlations.clone(),
    );
    let resource_bridge = spawn_resource_event_bridge(
        transport.clone(),
        event_tx,
        config.clone(),
        shutdown.clone(),
        correlations,
    );
    let announce_shutdown = CancellationToken::new();
    let announce_task = {
        let transport = transport.clone();
        let destination = destination.clone();
        let stop = announce_shutdown.clone();
        tokio::spawn(async move {
            while !stop.is_cancelled() {
                transport.send_announce(&destination, None).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        })
    };

    let destination_hash = destination.lock().await.desc.address_hash.to_hex_string();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/omen-ifac-tcp/tests/fixtures/python_nomadnet_rust_responder_matrix.py");
    let python_root = root.join("python");
    let page = config.nomadnet_index_page_path();
    let expected_rns = expected_rns.to_string();
    let fixture_expected_rns = expected_rns.clone();
    let output = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::task::spawn_blocking(move || {
            Command::new("python3")
                .arg(fixture)
                .arg("--rns-source")
                .arg(rns_source)
                .arg("--expected-rns")
                .arg(fixture_expected_rns)
                .arg("--root")
                .arg(python_root)
                .arg("--port")
                .arg(port.to_string())
                .arg("--destination")
                .arg(destination_hash)
                .arg("--page")
                .arg(page)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .expect("run Python NomadNet requester")
        }),
    )
    .await
    .expect("Python NomadNet matrix timeout")
    .expect("Python NomadNet blocking task");

    assert!(
        output.status.success(),
        "Python NomadNet matrix failed status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .expect("Python matrix JSON result");
    assert_eq!(result["passed"], true);
    assert_eq!(result["requests"], 4);
    assert_eq!(result["rns"], expected_rns.as_str());
    assert_eq!(
        result["matrix"],
        serde_json::json!([
            {"request": "direct", "response": "direct", "response_bytes": 43},
            {"request": "direct", "response": "resource", "response_bytes": 32799},
            {"request": "resource", "response": "direct", "response_bytes": 43},
            {"request": "resource", "response": "resource", "response_bytes": 32799}
        ])
    );

    assert!(crate::server_log::flush(Duration::from_secs(1)));
    let log = std::fs::read_to_string(config.log_path()).expect("read Rust responder log");
    let selected = log
        .lines()
        .filter(|line| line.contains("NomadNet response selected"))
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        4,
        "one response selection per Python request"
    );
    assert_eq!(
        selected
            .iter()
            .filter(|line| line.contains("ingress=direct-packet"))
            .count(),
        2
    );
    assert_eq!(
        selected
            .iter()
            .filter(|line| line.contains("ingress=request-resource"))
            .count(),
        2
    );
    assert_eq!(
        selected
            .iter()
            .filter(|line| line.contains("primitive=direct-packet"))
            .count(),
        2
    );
    assert_eq!(
        selected
            .iter()
            .filter(|line| line.contains("primitive=response-resource"))
            .count(),
        2
    );

    announce_shutdown.cancel();
    shutdown.cancel();
    for task in [announce_task, link_bridge, resource_bridge] {
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("Python matrix task shutdown bounded")
            .expect("Python matrix task joins");
    }
    assert!(transport.detach_interfaces().await >= 1);
    std::fs::remove_dir_all(&root).expect("remove Python matrix root");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit pinned-Python RNS requester to Rust omenchatd NomadNet matrix"]
async fn pinned_python_nomadnet_rust_responder_four_quadrants() {
    run_python_nomadnet_rust_responder_matrix("OMEN_PINNED_RNS_SOURCE", "1.2.2").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit current-Python RNS requester to Rust omenchatd NomadNet matrix"]
async fn current_python_nomadnet_rust_responder_four_quadrants() {
    run_python_nomadnet_rust_responder_matrix("OMEN_PYTHON_RNS_SOURCE", "1.4.0").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reticulum_split_metadata_assembly_preserves_segment_two_payload() {
    if std::env::var_os(SPLIT_SENTINEL_ENV).is_some() {
        install_resource_diagnostic_logger();
        let root = PathBuf::from(child_value(ROOT_ENV));
        let nonce = child_value(NONCE_ENV);
        let server_port = child_port(SERVER_PORT_ENV);
        let client_port = child_port(CLIENT_PORT_ENV);
        match child_value(ROLE_ENV).as_str() {
            "receiver" => {
                split_sentinel_receiver_child(&root, &nonce, server_port, client_port).await;
                publish_marker(&marker(&root, "split-receiver-complete"));
            }
            "sender" => split_sentinel_sender_child(&root, &nonce, server_port, client_port).await,
            role => panic!("unknown split sentinel role {role}"),
        }
        return;
    }

    let nonce = format!("{}-{}", std::process::id(), current_epoch_ms());
    let root = std::env::temp_dir().join(format!("omenchatd-split-sentinel-{nonce}"));
    std::fs::create_dir_all(&root).expect("create split sentinel root");
    let metadata_len = b"split-sentinel".len();
    let second_segment_offset = rns_transport::resource::MAX_EFFICIENT_SIZE - 3 - metadata_len;
    let mut state = 0x9e37_79b9_u32;
    let mut payload = (0..(second_segment_offset + 4096))
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    payload[second_segment_offset..second_segment_offset + 3].copy_from_slice(&[0, 0, 8]);
    std::fs::write(root.join("split-expected.bin"), payload).expect("write split fixture");
    let (server_port, client_port) = reserve_udp_ports();
    let receiver = spawn_split_sentinel_role("receiver", &root, &nonce, server_port, client_port);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !marker(&root, "split-receiver-ready").is_file() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("split receiver readiness");
    let sender = spawn_split_sentinel_role("sender", &root, &nonce, server_port, client_port);
    let (sender_output, sender_timed_out) = wait_child(sender, Duration::from_secs(40)).await;
    let (receiver_output, receiver_timed_out) = wait_child(receiver, Duration::from_secs(40)).await;
    assert!(
        !sender_timed_out
            && !receiver_timed_out
            && sender_output.status.success()
            && receiver_output.status.success(),
        "upstream split-metadata regression sentinel failed on official 0.9.8\n\
         sender status={} timed_out={} stdout={} stderr={}\n\
         receiver status={} timed_out={} stdout={} stderr={}",
        sender_output.status,
        sender_timed_out,
        String::from_utf8_lossy(&sender_output.stdout),
        String::from_utf8_lossy(&sender_output.stderr),
        receiver_output.status,
        receiver_timed_out,
        String::from_utf8_lossy(&receiver_output.stdout),
        String::from_utf8_lossy(&receiver_output.stderr),
    );
    std::fs::remove_dir_all(&root).expect("remove split sentinel root");
}

#[test]
#[ignore = "known upstream Reticulum 0.9.8 UDP maximum-Resource serialization regression"]
fn reticulum_udp_tx_buffer_covers_max_resource_wire_packet() {
    // reticulum-rs-transport 0.9.8 still sizes both buffers as
    // `size_of::<Packet>() * 3`. Packet payload storage is heap-backed, so
    // that Rust layout size is unrelated to the largest serialized packet.
    let upstream_udp_buffer = std::mem::size_of::<rns_transport::packet::Packet>() * 3;
    let max_type_one_wire_packet =
        2 + rns_transport::hash::ADDRESS_HASH_SIZE + 1 + rns_transport::packet::PACKET_MDU;

    assert!(
        upstream_udp_buffer >= max_type_one_wire_packet,
        "upstream UDP tx buffer ({upstream_udp_buffer}) cannot serialize a maximum Resource wire packet ({max_type_one_wire_packet})"
    );
}

#[test]
#[ignore = "known upstream Reticulum 0.9.8 routed Resource retransmission regression; requires the documented three-node fragment-loss topology"]
fn reticulum_routed_resource_retransmission_survives_fragment_loss() {
    // This deliberately failing expected-upstream sentinel keeps the realistic
    // three-node boundary separately named in normal test inventories. The
    // deterministic topology and acceptance assertions are maintained in
    // docs/upstream/reticulum-rs-0.9.8-routed-resource-retransmission.md. It
    // must be replaced by, and pass as, the real topology gate before OMEN can
    // claim routed attachment qualification on a future official crate train.
    panic!(
        "reticulum-rs-transport 0.9.8 suppresses requested duplicate Resource data/proof packets at a forwarding node"
    );
}
