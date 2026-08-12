use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use omen_ifac_tcp::IfacTcpClient;
use rns_transport::delivery::{await_link_activation, send_on_link_observed, LinkSendResult};
use rns_transport::destination::link::Link;
use rns_transport::hash::{AddressHash, Hash};
use rns_transport::identity::PrivateIdentity;
use rns_transport::transport::{DeliveryReceipt, ReceiptHandler, Transport, TransportConfig};

const NETWORK_NAME: &str = "omen-ifac-vector";
const PASSPHRASE: &str = "public-test-fixture";
const PYTHON_IDENTITY_HASH: &str = "aca31af0441d81dbec71e82da0b4b5f5";

struct ReceiptCapture {
    sender: tokio::sync::mpsc::Sender<[u8; 32]>,
}

impl ReceiptHandler for ReceiptCapture {
    fn on_receipt(&self, receipt: &DeliveryReceipt) {
        let _ = self.sender.try_send(receipt.message_id);
    }
}

struct IsolatedRoot(PathBuf);

impl IsolatedRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "omen-pinned-python-reticulum-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated Python Reticulum root");
        Self(path)
    }
}

impl Drop for IsolatedRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct PythonPeer {
    child: Child,
    json_lines: mpsc::Receiver<serde_json::Value>,
    reader: Option<JoinHandle<()>>,
    ready: serde_json::Value,
}

impl PythonPeer {
    fn spawn(root: &Path, port: u16) -> Self {
        let source = std::env::var_os("OMEN_PYTHON_RNS_SOURCE")
            .or_else(|| std::env::var_os("OMEN_PINNED_RNS_SOURCE"))
            .map(PathBuf::from)
            .expect("OMEN_PYTHON_RNS_SOURCE must name the verified Python RNS tree");
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pinned_python_reticulum_peer.py");
        let mut child = Command::new("python3")
            .arg(script)
            .arg("--rns-source")
            .arg(source)
            .arg("--root")
            .arg(root)
            .arg("--port")
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn pinned Python Reticulum peer");
        let (json_lines, reader) =
            json_line_reader(child.stdout.take().expect("Python peer stdout"));
        let ready = recv_json_line(&json_lines, Duration::from_secs(8), "Python peer readiness");
        assert_eq!(ready["ready"], true);
        assert_eq!(ready["port"], port);
        Self {
            child,
            json_lines,
            reader: Some(reader),
            ready,
        }
    }

    fn finish(mut self) -> serde_json::Value {
        let result = recv_json_line(
            &self.json_lines,
            Duration::from_secs(3),
            "Python peer result",
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait().expect("poll Python peer") {
                assert!(status.success(), "Python peer exited {status}");
                self.join_reader();
                return result;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("Python peer did not exit within bounded shutdown interval");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn join_reader(&mut self) {
        if let Some(reader) = self.reader.take() {
            reader.join().expect("Python stdout reader join");
        }
    }
}

impl Drop for PythonPeer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.join_reader();
    }
}

fn json_line_reader(stdout: ChildStdout) -> (mpsc::Receiver<serde_json::Value>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if sender.send(value).is_err() {
                    break;
                }
            }
        }
    });
    (receiver, reader)
}

fn recv_json_line(
    lines: &mpsc::Receiver<serde_json::Value>,
    timeout: Duration,
    description: &str,
) -> serde_json::Value {
    lines
        .recv_timeout(timeout)
        .unwrap_or_else(|error| panic!("{description}: {error}"))
}

fn reserve_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    listener.local_addr().expect("reserved address").port()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("interop Tokio runtime")
}

async fn send_small_link_packet(
    transport: &Transport,
    link: &Arc<tokio::sync::Mutex<Link>>,
    payload: &[u8],
) -> Hash {
    let send_result = send_on_link_observed(
        transport,
        link,
        payload,
        |_| {},
        |_| panic!("small link-data fixture unexpectedly used a Resource"),
    )
    .await
    .expect("direct link-data send");
    match send_result {
        LinkSendResult::Packet(packet) => packet.hash(),
        LinkSendResult::Resource(_) => {
            panic!("small link-data fixture unexpectedly used a Resource")
        }
    }
}

#[test]
#[ignore = "explicit pinned-Python full Reticulum interoperability test"]
fn pinned_python_reticulum_rejects_forgery_and_orders_stale_current_proofs() {
    let root = IsolatedRoot::new();
    let port = reserve_port();
    let peer = PythonPeer::spawn(&root.0, port);
    let destination = AddressHash::new_from_hex_string(
        peer.ready["destination"]
            .as_str()
            .expect("Python destination hash"),
    )
    .expect("valid Python destination hash");
    assert_eq!(peer.ready["identity"], PYTHON_IDENTITY_HASH);

    runtime().block_on(async {
        let local_identity = PrivateIdentity::new_from_name("pinned-python-interop-client");
        let (receipt_tx, mut receipt_rx) = tokio::sync::mpsc::channel(4);
        let mut transport = Transport::new(TransportConfig::new(
            "pinned-python-interop",
            &local_identity,
            true,
        ));
        transport
            .set_receipt_handler(Box::new(ReceiptCapture { sender: receipt_tx }))
            .await;
        let transport = Arc::new(transport);
        let mut announces = transport.recv_announces().await;
        let mut received_data = transport.received_data_events();

        let (iface_address, iface_task) = {
            let manager = transport.iface_manager();
            let mut manager = manager.lock().await;
            let client = IfacTcpClient::new(
                format!("127.0.0.1:{port}"),
                Some(NETWORK_NAME.into()),
                Some(PASSPHRASE.into()),
                16,
            )
            .expect("IFAC TCP client");
            let context = manager.new_context(client);
            let address = *context.channel.address();
            (address, tokio::spawn(IfacTcpClient::spawn(context)))
        };

        assert!(
            transport
                .await_path(&destination, Duration::from_secs(8), Some(iface_address))
                .await,
            "Rust path request did not yield a Python announce"
        );
        let event = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let event = announces
                    .recv()
                    .await
                    .expect("announce stream remains open");
                if event.destination.lock().await.desc.address_hash == destination {
                    return event;
                }
            }
        })
        .await
        .expect("matching Python announce event");
        // reticulum-rs-transport 0.9.9 mirrors reference Reticulum by
        // incrementing the packet hop count on receipt before publishing the
        // AnnounceEvent. A directly connected Python peer is therefore one
        // observed network hop away.
        assert_eq!(event.hops, 1);

        let identity = transport
            .destination_identity(&destination)
            .await
            .expect("identity recalled from validated announce");
        assert_eq!(identity.address_hash.to_hex_string(), PYTHON_IDENTITY_HASH);
        let destination_desc = event.destination.lock().await.desc;
        let link = transport.link(destination_desc).await;
        await_link_activation(&transport, &link, Duration::from_secs(8))
            .await
            .expect("Rust-to-Python link activation");

        let old_packet_hash =
            send_small_link_packet(&transport, &link, b"rust-link-data-old-attempt").await;
        assert!(
            tokio::time::timeout(Duration::from_millis(250), receipt_rx.recv())
                .await
                .is_err(),
            "old attempt unexpectedly received a proof before retry admission"
        );
        let retry_packet_hash =
            send_small_link_packet(&transport, &link, b"rust-link-data-retry").await;
        assert_ne!(old_packet_hash, retry_packet_hash);
        let reply = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let event = received_data
                    .recv()
                    .await
                    .expect("link-data event stream remains open");
                if event.data.as_slice() == b"python-link-data" {
                    return event;
                }
            }
        })
        .await
        .expect("Python link-data reply");
        assert_eq!(reply.destination, *link.lock().await.id());

        let stale_receipt = tokio::time::timeout(Duration::from_secs(4), receipt_rx.recv())
            .await
            .expect("Python packet proof timeout")
            .expect("receipt channel remains open");
        assert_eq!(stale_receipt, old_packet_hash.to_bytes());
        let current_receipt = tokio::time::timeout(Duration::from_secs(4), receipt_rx.recv())
            .await
            .expect("current Python packet proof timeout")
            .expect("receipt channel remains open");
        assert_eq!(current_receipt, retry_packet_hash.to_bytes());
        assert!(
            tokio::time::timeout(Duration::from_millis(250), receipt_rx.recv())
                .await
                .is_err(),
            "one Python packet generated more than one receipt callback"
        );

        assert_eq!(transport.detach_interfaces().await, 1);
        tokio::time::timeout(Duration::from_secs(1), iface_task)
            .await
            .expect("IFAC task shutdown")
            .expect("IFAC task join");
    });

    let result = peer.finish();
    assert_eq!(result["links"], 1);
    assert_eq!(result["received"], true);
    assert_eq!(result["replied"], true);
    assert_eq!(result["old_attempt_deferred"], true);
    assert_eq!(result["forged_proof_sent"], true);
    assert_eq!(result["stale_proof_sent"], true);
    assert_eq!(result["valid_proof_sent"], true);
}
