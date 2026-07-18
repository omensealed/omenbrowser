use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use omen_ifac_tcp::IfacTcpClient;
use rns_transport::iface::{InterfaceManager, TxMessage, TxMessageType};
use rns_transport::Packet;

const NETWORK_NAME: &str = "omen-ifac-vector";
const PASSPHRASE: &str = "public-test-fixture";

struct PythonPeer {
    child: Child,
    stdout: BufReader<ChildStdout>,
    port: u16,
}

impl PythonPeer {
    fn spawn(mode: &str) -> Self {
        let source = std::env::var_os("OMEN_PYTHON_RNS_SOURCE")
            .or_else(|| std::env::var_os("OMEN_PINNED_RNS_SOURCE"))
            .map(PathBuf::from)
            .expect("OMEN_PYTHON_RNS_SOURCE must name the verified Python RNS tree");
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pinned_python_ifac_peer.py");
        let mut child = Command::new("python3")
            .arg(script)
            .arg("--rns-source")
            .arg(source)
            .arg("--mode")
            .arg(mode)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn pinned Python IFAC peer");
        let mut stdout = BufReader::new(child.stdout.take().expect("Python peer stdout"));
        let mut ready = String::new();
        stdout.read_line(&mut ready).expect("read Python peer port");
        let port = serde_json::from_str::<serde_json::Value>(&ready)
            .expect("Python peer ready JSON")
            .get("port")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .expect("Python peer TCP port");
        Self {
            child,
            stdout,
            port,
        }
    }

    fn finish(mut self) -> serde_json::Value {
        let mut result = String::new();
        self.stdout
            .read_line(&mut result)
            .expect("read Python peer result");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = self.child.try_wait().expect("poll Python peer") {
                assert!(status.success(), "Python peer exited {status}");
                return serde_json::from_str(&result).expect("Python peer result JSON");
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("Python peer did not exit within bounded shutdown interval");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for PythonPeer {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn packet(marker: u8) -> Packet {
    let mut raw = vec![0x01, 0x02];
    raw.extend(0u8..16);
    raw.extend([0x09, marker, 0x7e, 0x7d]);
    Packet::from_bytes(&raw).expect("fixed Reticulum packet")
}

async fn wait_connected(status: &omen_ifac_tcp::IfacTcpRuntimeStatusHandle) {
    tokio::time::timeout(Duration::from_secs(7), async {
        loop {
            if status.to_json()["stream_state"] == "connected" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("IFAC TCP client connection timeout");
}

async fn receive_marker(
    receiver: &std::sync::Arc<tokio::sync::Mutex<rns_transport::iface::InterfaceRxReceiver>>,
) -> u8 {
    let message = tokio::time::timeout(Duration::from_secs(2), async {
        receiver.lock().await.recv().await
    })
    .await
    .expect("receive pinned Python packet timeout")
    .expect("IFAC receive queue remains open");
    message.packet.data.as_slice()[0]
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("interop Tokio runtime")
}

#[test]
#[ignore = "explicit pinned-Python TCP interoperability test"]
fn pinned_python_ifac_tcp_handles_split_coalesced_and_reconnect() {
    runtime().block_on(async {
        let peer = PythonPeer::spawn("roundtrip");
        let mut manager = InterfaceManager::new(16);
        let receiver = manager.receiver();
        let client = IfacTcpClient::new(
            format!("127.0.0.1:{}", peer.port),
            Some(NETWORK_NAME.into()),
            Some(PASSPHRASE.into()),
            16,
        )
        .expect("IFAC TCP client");
        let status = client.runtime_status_handle();
        let context = manager.new_context(client);
        let address = *context.channel.address();
        let task = tokio::spawn(IfacTcpClient::spawn(context));

        wait_connected(&status).await;
        let trace = manager
            .send(TxMessage {
                tx_type: TxMessageType::Direct(address),
                packet: packet(0x51),
            })
            .await;
        assert_eq!(trace.sent_ifaces, 1);
        assert_eq!(receive_marker(&receiver).await, 0xA1);
        assert_eq!(receive_marker(&receiver).await, 0xA2);
        assert_eq!(receive_marker(&receiver).await, 0xA3);

        tokio::time::sleep(Duration::from_millis(100)).await;
        wait_connected(&status).await;
        let trace = manager
            .send(TxMessage {
                tx_type: TxMessageType::Direct(address),
                packet: packet(0x52),
            })
            .await;
        assert_eq!(trace.sent_ifaces, 1);
        assert_eq!(receive_marker(&receiver).await, 0xB1);

        assert!(manager.stop_interface(address));
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("IFAC task shutdown")
            .expect("IFAC task join");
        let result = peer.finish();
        assert_eq!(result["connections"], 2);
        assert_eq!(result["accepted"], 2);
        assert_eq!(result["rejected"], 0);
    });
}

#[test]
#[ignore = "explicit pinned-Python TCP interoperability test"]
fn pinned_python_ifac_tcp_rejects_wrong_credentials_both_directions() {
    runtime().block_on(async {
        let peer = PythonPeer::spawn("wrong-credential");
        let mut manager = InterfaceManager::new(4);
        let receiver = manager.receiver();
        let client = IfacTcpClient::new(
            format!("127.0.0.1:{}", peer.port),
            Some(NETWORK_NAME.into()),
            Some("wrong-public-test-fixture".into()),
            16,
        )
        .expect("wrong-credential IFAC TCP client");
        let status = client.runtime_status_handle();
        let context = manager.new_context(client);
        let address = *context.channel.address();
        let task = tokio::spawn(IfacTcpClient::spawn(context));

        wait_connected(&status).await;
        let trace = manager
            .send(TxMessage {
                tx_type: TxMessageType::Direct(address),
                packet: packet(0x61),
            })
            .await;
        assert_eq!(trace.sent_ifaces, 1);
        assert!(
            tokio::time::timeout(Duration::from_millis(400), async {
                receiver.lock().await.recv().await
            })
            .await
            .is_err(),
            "wrong-credential Python response crossed the Rust IFAC boundary"
        );

        assert!(manager.stop_interface(address));
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("wrong-credential IFAC task shutdown")
            .expect("wrong-credential IFAC task join");
        let result = peer.finish();
        assert_eq!(result["accepted"], 0);
        assert_eq!(result["rejected"], 1);
    });
}
