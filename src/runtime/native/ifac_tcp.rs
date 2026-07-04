use std::sync::{Arc, Mutex};
use std::time::Duration;

use hkdf::Hkdf;
use rns_transport::buffer::{InputBuffer, OutputBuffer};
use rns_transport::hash::{AddressHash, Hash};
use rns_transport::identity::PrivateIdentity;
use rns_transport::iface::hdlc::Hdlc;
use rns_transport::iface::{
    IfaceSource, Interface, InterfaceContext, InterfaceRxSender, InterfaceTxReceiver, RxMessage,
};
use rns_transport::serde::Serialize;
use rns_transport::Packet;
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

const DEFAULT_MTU: usize = 262_144;
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const IFAC_SALT: [u8; 32] = [
    0xad, 0xf5, 0x4d, 0x88, 0x2c, 0x9a, 0x9b, 0x80, 0x77, 0x1e, 0xb4, 0x99, 0x5d, 0x70, 0x2d, 0x4a,
    0x3e, 0x73, 0x33, 0x91, 0xb2, 0xa0, 0xf5, 0x3f, 0x41, 0x6d, 0x9f, 0x90, 0x7e, 0x55, 0xcf, 0xf8,
];

#[derive(Clone)]
pub struct IfacTcpRuntimeStatusHandle {
    inner: Arc<Mutex<IfacTcpRuntimeStatus>>,
}

impl IfacTcpRuntimeStatusHandle {
    pub fn to_json(&self) -> serde_json::Value {
        self.inner
            .lock()
            .map(|status| status.to_json())
            .unwrap_or_else(|_| {
                serde_json::json!({
                    "stream_state": "unknown",
                    "last_error": "status lock poisoned",
                    "bytes_rx": 0,
                    "bytes_tx": 0,
                })
            })
    }
}

#[derive(Debug)]
struct IfacTcpRuntimeStatus {
    endpoint: String,
    state: &'static str,
    last_error: Option<String>,
    bytes_rx: u64,
    bytes_tx: u64,
}

impl IfacTcpRuntimeStatus {
    fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            state: "created",
            last_error: None,
            bytes_rx: 0,
            bytes_tx: 0,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "endpoint": self.endpoint,
            "stream_state": self.state,
            "last_error": self.last_error.as_deref().unwrap_or("none"),
            "bytes_rx": self.bytes_rx,
            "bytes_tx": self.bytes_tx,
        })
    }
}

pub struct IfacTcpClient {
    addr: String,
    ifac: IfacContext,
    status: Arc<Mutex<IfacTcpRuntimeStatus>>,
}

impl IfacTcpClient {
    pub fn new(
        addr: String,
        network_name: Option<String>,
        passphrase: Option<String>,
        ifac_size: usize,
    ) -> Result<Self, &'static str> {
        let ifac = IfacContext::new(network_name.as_deref(), passphrase.as_deref(), ifac_size)?;
        Ok(Self {
            status: Arc::new(Mutex::new(IfacTcpRuntimeStatus::new(addr.clone()))),
            addr,
            ifac,
        })
    }

    pub fn runtime_status_handle(&self) -> IfacTcpRuntimeStatusHandle {
        IfacTcpRuntimeStatusHandle {
            inner: self.status.clone(),
        }
    }

    pub async fn spawn(context: InterfaceContext<IfacTcpClient>) {
        let iface_stop = context.channel.stop.clone();
        let iface_address = context.channel.address;
        let (addr, ifac, status) = {
            let guard = context
                .inner
                .lock()
                .expect("ifac tcp client mutex poisoned");
            (guard.addr.clone(), guard.ifac.clone(), guard.status.clone())
        };
        let (rx_channel, tx_channel) = context.channel.split();
        let tx_channel = Arc::new(AsyncMutex::new(tx_channel));

        loop {
            if context.cancel.is_cancelled() || iface_stop.is_cancelled() {
                break;
            }
            mark_status(&status, "connecting", None, 0, 0);
            let stream =
                tokio::time::timeout(DEFAULT_CONNECT_TIMEOUT, TcpStream::connect(&addr)).await;
            let stream = match stream {
                Ok(Ok(stream)) => stream,
                Ok(Err(error)) => {
                    mark_status(
                        &status,
                        "reconnecting",
                        Some(format!("tcp connect failed: {error}")),
                        0,
                        0,
                    );
                    wait_reconnect(&context.cancel, &iface_stop).await;
                    continue;
                }
                Err(_) => {
                    mark_status(
                        &status,
                        "reconnecting",
                        Some("tcp connect timed out".to_string()),
                        0,
                        0,
                    );
                    wait_reconnect(&context.cancel, &iface_stop).await;
                    continue;
                }
            };

            mark_status(&status, "connected", None, 0, 0);
            let (read_stream, write_stream) = stream.into_split();
            run_ifac_stream(
                iface_address,
                context.cancel.clone(),
                iface_stop.clone(),
                rx_channel.clone(),
                tx_channel.clone(),
                read_stream,
                write_stream,
                ifac.clone(),
                status.clone(),
            )
            .await;

            if context.cancel.is_cancelled() || iface_stop.is_cancelled() {
                break;
            }
            mark_status(
                &status,
                "reconnecting",
                Some("tcp stream closed".to_string()),
                0,
                0,
            );
            wait_reconnect(&context.cancel, &iface_stop).await;
        }
        mark_status(&status, "closed", None, 0, 0);
        iface_stop.cancel();
    }
}

impl Interface for IfacTcpClient {
    fn mtu() -> usize {
        DEFAULT_MTU
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_ifac_stream<R, W>(
    iface_address: AddressHash,
    cancel: CancellationToken,
    iface_stop: CancellationToken,
    rx_channel: InterfaceRxSender,
    tx_channel: Arc<AsyncMutex<InterfaceTxReceiver>>,
    mut read_stream: R,
    mut write_stream: W,
    ifac: IfacContext,
    status: Arc<Mutex<IfacTcpRuntimeStatus>>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let stop = CancellationToken::new();
    let stop_rx = stop.clone();
    let stop_tx = stop.clone();
    let cancel_rx = cancel.clone();
    let cancel_tx = cancel.clone();
    let iface_stop_rx = iface_stop.clone();
    let iface_stop_tx = iface_stop.clone();

    let rx_task = {
        let ifac = ifac.clone();
        let status = status.clone();
        tokio::spawn(async move {
            let mut tcp_buffer = vec![0u8; DEFAULT_MTU.saturating_mul(16)];
            let mut frame_buffer: Vec<u8> = Vec::with_capacity(DEFAULT_MTU.saturating_mul(4));
            let mut hdlc_buffer = vec![0u8; DEFAULT_MTU.saturating_add(64)];
            loop {
                tokio::select! {
                    _ = cancel_rx.cancelled() => break,
                    _ = iface_stop_rx.cancelled() => {
                        stop_rx.cancel();
                        break;
                    }
                    _ = stop_rx.cancelled() => break,
                    result = read_stream.read(&mut tcp_buffer) => {
                        match result {
                            Ok(0) => {
                                stop_rx.cancel();
                                break;
                            }
                            Ok(n) => {
                                add_status_traffic(&status, n as u64, 0);
                                frame_buffer.extend_from_slice(&tcp_buffer[..n]);
                                while let Some((start, end)) = Hdlc::find(&frame_buffer) {
                                    let frame = &frame_buffer[start..=end];
                                    let mut output = OutputBuffer::new(&mut hdlc_buffer);
                                    match Hdlc::decode(frame, &mut output)
                                        .ok()
                                        .and_then(|_| ifac.decode_inbound(output.as_slice()).ok())
                                        .and_then(|raw| Packet::deserialize(&mut InputBuffer::new(&raw)).ok())
                                    {
                                        Some(packet) => {
                                            let _ = rx_channel
                                                .send(RxMessage {
                                                    address: iface_address,
                                                    packet,
                                                    source: IfaceSource::None,
                                                })
                                                .await;
                                        }
                                        None => {
                                            mark_status(&status, "connected", Some("ifac packet decode failed".to_string()), 0, 0);
                                        }
                                    }
                                    frame_buffer.drain(..=end);
                                }
                                if frame_buffer.len() > DEFAULT_MTU.saturating_mul(64) {
                                    frame_buffer.clear();
                                }
                            }
                            Err(error) => {
                                mark_status(&status, "closed", Some(format!("tcp read failed: {error}")), 0, 0);
                                stop_rx.cancel();
                                break;
                            }
                        }
                    }
                }
            }
        })
    };

    let tx_task = tokio::spawn(async move {
        let mut tx_channel = tx_channel.lock().await;
        let mut packet_buffer = vec![0u8; DEFAULT_MTU];
        let mut hdlc_buffer = vec![0u8; DEFAULT_MTU.saturating_mul(2).saturating_add(128)];
        loop {
            tokio::select! {
                _ = cancel_tx.cancelled() => break,
                _ = iface_stop_tx.cancelled() => {
                    stop_tx.cancel();
                    break;
                }
                _ = stop_tx.cancelled() => break,
                Some(message) = tx_channel.recv() => {
                    let mut output = OutputBuffer::new(&mut packet_buffer);
                    let Ok(_) = message.packet.serialize(&mut output) else {
                        mark_status(&status, "connected", Some("packet serialize failed".to_string()), 0, 0);
                        continue;
                    };
                    let Ok(raw) = ifac.encode_outbound(output.as_slice()) else {
                        mark_status(&status, "connected", Some("ifac packet encode failed".to_string()), 0, 0);
                        continue;
                    };
                    let mut hdlc_output = OutputBuffer::new(&mut hdlc_buffer);
                    if Hdlc::encode(&raw, &mut hdlc_output).is_err() {
                        mark_status(&status, "connected", Some("hdlc encode failed".to_string()), 0, 0);
                        continue;
                    }
                    if let Err(error) = write_stream.write_all(hdlc_output.as_slice()).await {
                        mark_status(&status, "closed", Some(format!("tcp write failed: {error}")), 0, 0);
                        stop_tx.cancel();
                        break;
                    }
                    if let Err(error) = write_stream.flush().await {
                        mark_status(&status, "closed", Some(format!("tcp flush failed: {error}")), 0, 0);
                        stop_tx.cancel();
                        break;
                    }
                    add_status_traffic(&status, 0, hdlc_output.as_slice().len() as u64);
                }
            }
        }
    });

    let _ = tx_task.await;
    let _ = rx_task.await;
}

async fn wait_reconnect(cancel: &CancellationToken, iface_stop: &CancellationToken) {
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = iface_stop.cancelled() => {}
        _ = tokio::time::sleep(RECONNECT_DELAY) => {}
    }
}

fn mark_status(
    status: &Arc<Mutex<IfacTcpRuntimeStatus>>,
    state: &'static str,
    error: Option<String>,
    rx: u64,
    tx: u64,
) {
    if let Ok(mut guard) = status.lock() {
        guard.state = state;
        guard.last_error = error;
        guard.bytes_rx = guard.bytes_rx.saturating_add(rx);
        guard.bytes_tx = guard.bytes_tx.saturating_add(tx);
    }
}

fn add_status_traffic(status: &Arc<Mutex<IfacTcpRuntimeStatus>>, rx: u64, tx: u64) {
    if let Ok(mut guard) = status.lock() {
        guard.bytes_rx = guard.bytes_rx.saturating_add(rx);
        guard.bytes_tx = guard.bytes_tx.saturating_add(tx);
    }
}

#[derive(Clone)]
struct IfacContext {
    key: [u8; 64],
    identity: PrivateIdentity,
    size: usize,
}

impl IfacContext {
    fn new(
        network_name: Option<&str>,
        passphrase: Option<&str>,
        ifac_size: usize,
    ) -> Result<Self, &'static str> {
        let mut origin = Vec::new();
        if let Some(network_name) = network_name.filter(|value| !value.is_empty()) {
            origin.extend_from_slice(Hash::new_from_slice(network_name.as_bytes()).as_slice());
        }
        if let Some(passphrase) = passphrase.filter(|value| !value.is_empty()) {
            origin.extend_from_slice(Hash::new_from_slice(passphrase.as_bytes()).as_slice());
        }
        if origin.is_empty() {
            return Err("ifac requires network name or passphrase");
        }
        let origin_hash = Hash::new_from_slice(&origin);
        let mut key = [0u8; 64];
        Hkdf::<Sha256>::new(Some(&IFAC_SALT), origin_hash.as_slice())
            .expand(&[], &mut key)
            .map_err(|_| "ifac key derivation failed")?;
        let identity =
            PrivateIdentity::from_private_key_bytes(&key).map_err(|_| "ifac identity failed")?;
        Ok(Self {
            key,
            identity,
            size: ifac_size.clamp(1, 64),
        })
    }

    fn encode_outbound(&self, raw: &[u8]) -> Result<Vec<u8>, &'static str> {
        if raw.len() < 2 {
            return Err("raw packet too short");
        }
        let ifac = self.sign_tail(raw);
        let mut masked = Vec::with_capacity(raw.len().saturating_add(self.size));
        masked.push(raw[0] | 0x80);
        masked.push(raw[1]);
        masked.extend_from_slice(&ifac);
        masked.extend_from_slice(&raw[2..]);
        let mask = self.mask(&ifac, masked.len())?;
        for (index, byte) in masked.iter_mut().enumerate() {
            if index == 0 {
                *byte = (*byte ^ mask[index]) | 0x80;
            } else if index == 1 || index > self.size + 1 {
                *byte ^= mask[index];
            }
        }
        Ok(masked)
    }

    fn decode_inbound(&self, raw: &[u8]) -> Result<Vec<u8>, &'static str> {
        if raw.len() < 2 + self.size {
            return Err("ifac packet too short");
        }
        if raw[0] & 0x80 == 0 {
            return Err("missing ifac flag");
        }
        let ifac = raw[2..2 + self.size].to_vec();
        let mask = self.mask(&ifac, raw.len())?;
        let mut unmasked = raw.to_vec();
        for (index, byte) in unmasked.iter_mut().enumerate() {
            if index <= 1 || index > self.size + 1 {
                *byte ^= mask[index];
            }
        }
        unmasked[0] &= 0x7f;
        let mut packet = Vec::with_capacity(raw.len().saturating_sub(self.size));
        packet.push(unmasked[0]);
        packet.push(unmasked[1]);
        packet.extend_from_slice(&unmasked[2 + self.size..]);
        if self.sign_tail(&packet) != ifac {
            return Err("ifac signature mismatch");
        }
        Ok(packet)
    }

    fn sign_tail(&self, data: &[u8]) -> Vec<u8> {
        let signature = self.identity.sign(data).to_bytes();
        signature[signature.len().saturating_sub(self.size)..].to_vec()
    }

    fn mask(&self, ifac: &[u8], len: usize) -> Result<Vec<u8>, &'static str> {
        let mut mask = vec![0u8; len];
        Hkdf::<Sha256>::new(Some(&self.key), ifac)
            .expand(&[], &mut mask)
            .map_err(|_| "ifac mask derivation failed")?;
        Ok(mask)
    }
}

#[cfg(test)]
mod tests {
    use super::IfacContext;

    #[test]
    fn ifac_round_trip_preserves_packet_bytes() {
        let ifac = IfacContext::new(Some("private_ret"), Some("secret"), 16).unwrap();
        let raw = b"\x01\x02this-is-a-reticulum-packet";
        let encoded = ifac.encode_outbound(raw).unwrap();
        assert_ne!(encoded, raw);
        assert_eq!(encoded[0] & 0x80, 0x80);
        let decoded = ifac.decode_inbound(&encoded).unwrap();
        assert_eq!(decoded, raw);
    }

    #[test]
    fn ifac_rejects_wrong_passphrase() {
        let sender = IfacContext::new(Some("private_ret"), Some("secret"), 16).unwrap();
        let receiver = IfacContext::new(Some("private_ret"), Some("wrong"), 16).unwrap();
        let encoded = sender
            .encode_outbound(b"\x01\x02this-is-a-reticulum-packet")
            .unwrap();
        assert!(receiver.decode_inbound(&encoded).is_err());
    }
}
