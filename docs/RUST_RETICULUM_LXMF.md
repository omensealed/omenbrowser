# OMENbrowser Rust Port: Reticulum-rs + LXMF-rs Send/Receive/Propagation Notes

## Purpose

Use this document as the implementation guide for finishing the Rust networking side of OMENbrowser. The goal is to make the Rust version send and receive LXMF messages over Reticulum correctly, including direct delivery, inbound events, delivery status tracking, and propagated/store-and-forward messaging.

This handoff is based on the current published Rust crates and upstream docs as of 2026-05-08:

- `reticulum-rs = "0.2.0"`
- `reticulum-rs-core = "0.2.0"`
- `reticulum-rs-transport = "0.2.0"`
- `reticulum-rs-rpc = "0.3.0"`
- `lxmf = "0.3.0"`
- `lxmf-sdk = "0.2.1"`
- `lxmf-wire = "0.2.0"`

Important naming warning: do **not** assume the older `lxmf-rs` crate is the preferred crate for new work. The currently published umbrella crate is named `lxmf`, and it re-exports the SDK and wire crates. For OMENbrowser, target `lxmf` / `lxmf-sdk` first.

---

## Executive Summary

The Rust Reticulum/LXMF ecosystem is split into layers:

```text
OMENbrowser UI / app code
        |
        v
LXMF app-facing SDK
  crate: lxmf / lxmf-sdk
        |
        v
RPC boundary to daemon
  crate: reticulum-rs-rpc
        |
        v
lxmd + reticulumd
  binaries in LXMF-rs monorepo
        |
        v
Reticulum transport/runtime
  crates: reticulum-rs, reticulum-rs-core, reticulum-rs-transport
        |
        v
Interfaces: TCP, UDP, serial/RNode, LoRa, etc.
```

For the first real implementation, OMENbrowser should **not** try to directly build every RNS packet/link/resource operation itself. The stable route is:

1. Start or connect to a running `lxmd`/`reticulumd`.
2. Use `lxmf-sdk` with the RPC backend.
3. Start the SDK runtime.
4. Subscribe to events.
5. Send messages using `SendRequest`.
6. Track delivery through events and/or `status()`.
7. Use propagation-specific RPC methods and send options for store-and-forward.

This matches how the Rust project is shaped: `reticulum-rs` is the Reticulum primitive/transport umbrella crate, while `lxmf-sdk` is the host-facing API intended for LXMF clients.

---

## Crate Roles

### `reticulum-rs`

`reticulum-rs` is an umbrella crate for Reticulum primitives, transport, and daemon RPC contracts. It re-exports:

```rust
pub use rns_core as core;
pub use rns_transport as transport;
```

It also exposes module namespaces such as:

```text
destination
hash
identity
iface
packet
ratchets
resource
runtime
```

Use this crate when OMENbrowser needs Reticulum identity/hash/packet/transport primitives directly, but prefer SDK/RPC for normal LXMF messaging.

### `reticulum-rs-core`

This is the low-level Reticulum primitive layer. Its public item list includes:

```text
identity::Identity
identity::PrivateIdentity
destination::Destination
destination::DestinationName
destination::Single
destination::Group
packet::Packet
packet::Header
packet::PacketContext
packet::PacketType
packet::DestinationType
packet::PropagationType
hash::AddressHash
hash::Hash
```

It also exposes helpers relevant to LXMF identity/signature compatibility:

```rust
rns_core::hash::lxmf_address_hash
rns_core::identity::lxmf_sign
rns_core::identity::lxmf_verify
```

Use this for identity conversion, hash parsing, LXMF address derivation, verification, or tests. Do not use this as the first path for high-level OMENbrowser send/receive unless the SDK cannot cover the needed operation.

### `reticulum-rs-transport`

This is the transport boundary crate for runtime crates and daemon entrypoints. It exposes transport, interface, destination, link, resource, receipt, and storage concepts.

Relevant areas:

```text
transport::Transport
transport::TransportConfig
transport::DeliveryReceipt
transport::ReceivedData
transport::PathTable
transport::AnnounceTable

iface::InterfaceManager
iface::tcp_client::TcpClient
iface::tcp_server::TcpServer
iface::udp::UdpInterface
iface::serial::SerialInterface

destination::link::Link
resource::ResourceManager
storage::messages::MessagesStore
```

This is closer to a native RNS runtime implementation, but it is still lower-level than OMENbrowser should use for the first LXMF client path.

### `reticulum-rs-rpc`

This is the daemon boundary crate. It defines the protocol and daemon contracts used by CLI/app clients.

Important exposed concepts:

```text
RpcDaemon
RpcRequest
RpcResponse
RpcEvent
OutboundBridge
OutboundDeliveryOptions
DeliveryPolicy
DeliveryTraceEntry
PropagationState
StampPolicy
TicketRecord
MessagesStore
MessageRecord
AnnounceRecord
```

For OMENbrowser, this is the main bridge to a running daemon if using the LXMF SDK.

### `lxmf`

`lxmf = "0.3.0"` is the umbrella crate for library consumers. It re-exports:

```rust
pub use lxmf_core as wire;
pub use lxmf_sdk as sdk;
```

This means OMENbrowser can depend on either:

```toml
[dependencies]
lxmf = "0.3.0"
reticulum-rs = "0.2.0"
```

or directly:

```toml
[dependencies]
lxmf-sdk = "0.2.1"
lxmf-wire = "0.2.0"
reticulum-rs-rpc = "0.3.0"
reticulum-rs = "0.2.0"
```

Prefer the direct crates when the Rust compiler/import paths are clearer.

### `lxmf-wire`

This is the LXMF wire-format crate. It contains message primitives and identity helpers.

Important public items include:

```text
identity::Identity
identity::PrivateIdentity
message::Message
message::MessageContainer
message::Payload
message::WireMessage
message::DeliveryDecision
message::MessageMethod
message::MessageState
message::TransportMethod
inbound_decode::DecodedInboundMessage
inbound_decode::InboundPayloadMode
decode_inbound_message()
decide_delivery()
```

Use this crate to parse or generate LXMF wire messages, verify compatibility, decode inbound payloads, and write unit tests against known-good fixtures.

### `lxmf-sdk`

This is the app-facing SDK. This is the correct first target for OMENbrowser.

The core trait is:

```rust
pub trait LxmfSdk {
    fn start(&self, req: StartRequest) -> Result<ClientHandle, SdkError>;
    fn send(&self, req: SendRequest) -> Result<MessageId, SdkError>;
    fn cancel(&self, id: MessageId) -> Result<CancelResult, SdkError>;
    fn status(&self, id: MessageId) -> Result<Option<DeliverySnapshot>, SdkError>;
    fn configure(&self, expected_revision: u64, patch: ConfigPatch) -> Result<Ack, SdkError>;
    fn poll_events(&self, cursor: Option<EventCursor>, max: usize) -> Result<EventBatch, SdkError>;
    fn snapshot(&self) -> Result<RuntimeSnapshot, SdkError>;
    fn shutdown(&self, mode: ShutdownMode) -> Result<Ack, SdkError>;
}
```

With the async feature enabled, it also supports:

```rust
pub trait LxmfSdkAsync {
    fn subscribe_events(&self, start: SubscriptionStart) -> Result<EventSubscription, SdkError>;
}
```

The SDK is designed around lifecycle, send, cancel, status, configuration, event polling, snapshots, and shutdown.

---

## LXMF Message Model

An LXMF message is structured as:

```text
Destination
Source
Ed25519 Signature
Payload
  Timestamp
  Content
  Title
  Fields
```

Important details:

- Destination and Source are 16-byte Reticulum destination hashes.
- Signature is a 64-byte Ed25519 signature.
- Payload is MessagePack.
- Payload contains timestamp, content, title, and fields.
- The message id is derived from Destination + Source + Payload and is not normally stored directly inside the message.
- When a propagation node stores an encrypted message for an offline user, the actual message id may not be inferable, so a transient id can be used while the message is in storage/transit.

For OMENbrowser, this means:

- In the UI/database, keep both `message_id` and `transient_id` fields if possible.
- A direct message may have a stable message id immediately.
- A propagated/offline message may need a transient id until final delivery/fetch/decode.
- Do not assume every outbound ID is final if propagation is involved.

---

## Delivery Modes to Support

LXMF supports several practical delivery paths:

### 1. Direct link delivery

Default/normal message delivery over a Reticulum link.

Expected behavior:

- Sender queues outbound message.
- RNS/LXMF resolves path/identity if needed.
- A link is established.
- Message is transmitted.
- Delivery receipt or failure event updates local state.

This is the path OMENbrowser should use first when the recipient is reachable.

### 2. Opportunistic packet delivery

A message can be embedded in a single Reticulum packet and routed opportunistically.

Expected behavior:

- Works only when payload size and delivery policy allow it.
- Useful for very small messages and constrained links.
- Less stateful than link delivery.

Do not make this the default OMENbrowser path unless the SDK/daemon selects it automatically or a user chooses a low-bandwidth mode.

### 3. Propagated/store-and-forward delivery

Propagation nodes store and forward messages for users/endpoints that are not directly reachable. Propagation nodes can peer and synchronize, creating an encrypted distributed message store. Users can later fetch their messages from available propagation nodes.

Expected behavior:

- Sender tries direct delivery if requested.
- If direct delivery fails or is not viable, sender may attempt propagation.
- Message is placed into propagation storage under a transient id.
- Recipient later fetches from propagation node(s).
- Recipient decodes inbound message and records it locally.
- Message status moves through queued/sent/propagated/fetched/delivered/failed states depending on daemon events.

OMENbrowser must treat propagation as an LXMF-level store-and-forward feature, not the same thing as Reticulum transport-mode forwarding. A Reticulum Transport Node routes packets/hops. An LXMF Propagation Node stores encrypted messages for later retrieval.

---

## Preferred Runtime Architecture for OMENbrowser

Recommended abstraction:

```rust
#[async_trait::async_trait]
pub trait NetworkRuntime {
    async fn start(&mut self) -> Result<RuntimeInfo, NetworkError>;
    async fn stop(&mut self) -> Result<(), NetworkError>;

    async fn local_identity(&self) -> Result<LocalIdentity, NetworkError>;

    async fn announce(&self) -> Result<(), NetworkError>;

    async fn send_lxmf(&self, req: OmenSendRequest) -> Result<OmenSendReceipt, NetworkError>;

    async fn poll_events(&self) -> Result<Vec<OmenNetworkEvent>, NetworkError>;

    async fn propagation_status(&self) -> Result<PropagationStatus, NetworkError>;
    async fn enable_propagation(&self, cfg: PropagationConfig) -> Result<(), NetworkError>;
    async fn fetch_propagated(&self, transient_id: Option<String>) -> Result<Vec<OmenInboundMessage>, NetworkError>;
}
```

Provide at least two implementations:

```text
MockNetworkRuntime
  - current/dev behavior
  - deterministic tests
  - no live networking

SdkRpcNetworkRuntime
  - real path
  - talks to lxmf-sdk / lxmd / reticulumd
```

Optionally later:

```text
NativeTransportNetworkRuntime
  - talks directly to reticulum-rs-transport
  - only after SDK/RPC path works
```

---

## Dependency Setup

Start with:

```toml
[dependencies]
anyhow = "1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
thiserror = "1"

lxmf = "0.3.0"
lxmf-sdk = "0.2.1"
lxmf-wire = "0.2.0"
reticulum-rs = "0.2.0"
reticulum-rs-rpc = "0.3.0"
```

If feature flags are required by the crate version, enable the SDK RPC/async path explicitly after checking `cargo tree -e features` and docs.rs feature names:

```toml
lxmf-sdk = { version = "0.2.1", features = ["sdk-async"] }
```

If the exact RPC feature name differs, inspect:

```bash
cargo tree -e features -p lxmf-sdk
cargo doc -p lxmf-sdk --open
```

---

## SDK Lifecycle

The app-facing path is event-driven:

1. Create client.
2. Start runtime.
3. Subscribe to events or use cursor polling.
4. Send message.
5. Handle delivery/inbound events.
6. Periodically call snapshot for reconciliation.
7. Shutdown cleanly.

Pseudo-code shape:

```rust
use lxmf_sdk::app::{
    Client,
    Config,
    EventKind,
    SendRequest,
    SubscriptionStart,
};
use serde_json::json;
use tokio_stream::StreamExt;

pub async fn run_example() -> Result<(), lxmf_sdk::app::Error> {
    let client = Client::rpc("unix:/tmp/lxmf-rpc.sock");

    let handle = client
        .runtime()
        .start_async(Config::desktop_default())
        .await?;

    eprintln!("LXMF runtime started: {}", handle.runtime_id);

    let mut events = client.events().subscribe(SubscriptionStart::Tail)?;

    let receipt = client.messages()
        .send_async(
            SendRequest::new(
                "example.service",
                "example.peer",
                json!({
                    "title": "hello",
                    "content": "sdk quickstart"
                }),
            )
            .with_ttl_ms(30_000)
            .with_correlation_id("omenbrowser-send-1"),
        )
        .await?;

    eprintln!("queued message: {}", receipt.message_id);

    while let Some(event) = events.next().await.transpose()? {
        match event.kind {
            EventKind::InboundMessageReceived => {
                eprintln!("inbound message received");
            }
            EventKind::MessageDelivered
                if event.metadata.message_id.as_deref() == Some(receipt.message_id.as_str()) =>
            {
                eprintln!("message delivered");
                break;
            }
            EventKind::StreamGapDetected(gap) => {
                eprintln!("event stream gap: {:?}", gap);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
```

Verify exact import paths because docs may expose both umbrella and module paths. Preserve the architecture even if names need slight correction.

---

## Event Handling Requirements

Do **not** add a dumb one-second app polling loop as the main design.

Correct behavior:

- Prefer async subscription when available.
- Fall back to `poll_events(cursor, max)` only when async subscriptions are unavailable, for manual embedded hosts, deterministic tests, or recovery.
- Persist the returned cursor.
- Process events in order.
- Make handlers idempotent because delivery updates can be at-least-once.
- Treat stream gaps as data-loss indicators; call `snapshot()` and reconcile.
- Preserve `trace_ref` and `correlation_id` in logs.
- Avoid logging full message payloads by default.

Suggested OMENbrowser event enum:

```rust
pub enum OmenNetworkEvent {
    RuntimeStarted { runtime_id: String },
    RuntimeStopped,
    InboundMessage {
        message_id: String,
        source: String,
        destination: String,
        title: Option<String>,
        content: Vec<u8>,
        fields: serde_json::Value,
    },
    OutboundQueued { message_id: String, correlation_id: Option<String> },
    OutboundDelivered { message_id: String },
    OutboundFailed { message_id: String, reason: String },
    PropagationStored { transient_id: String, message_id: Option<String> },
    PropagationFetched { transient_id: String, message_id: Option<String> },
    StreamGapDetected,
    SnapshotRequired,
}
```

---

## Send Request Design for OMENbrowser

OMENbrowser should expose a higher-level request:

```rust
pub struct OmenSendRequest {
    pub source: Option<String>,
    pub destination: String,
    pub title: Option<String>,
    pub content: String,
    pub fields: serde_json::Value,

    pub ttl_ms: Option<u64>,
    pub correlation_id: Option<String>,

    pub prefer_propagation: bool,
    pub try_propagation_on_fail: bool,
    pub stamp_cost: Option<u32>,
    pub include_ticket: bool,
}
```

Map it into the SDK/RPC request.

For CLI/RPC compatibility, the stable daemon method `send_message_v2` accepts parameters:

```text
id
source
destination
title
content
optional fields
optional method
optional stamp_cost
optional include_ticket
optional try_propagation_on_fail
optional source_private_key
```

Implementation intent:

```rust
let payload = json!({
    "title": req.title.unwrap_or_default(),
    "content": req.content,
    "fields": req.fields,
});

let mut sdk_req = SendRequest::new(
    req.source.unwrap_or_else(|| "default".to_string()),
    req.destination,
    payload,
);

if let Some(ttl) = req.ttl_ms {
    sdk_req = sdk_req.with_ttl_ms(ttl);
}

if let Some(cid) = req.correlation_id {
    sdk_req = sdk_req.with_correlation_id(cid);
}

// If supported by the current SDK version, also set:
// - method
// - stamp_cost
// - include_ticket
// - try_propagation_on_fail
//
// If builder methods do not exist, use the lower-level RPC bridge
// or SDK config/fields mechanism.
```

Compile-check the builder methods because the docs show `with_ttl_ms()` and `with_correlation_id()`, but propagation-specific builder methods may differ or may require lower-level RPC.

---

## Receive Path

Inbound receive should come from events first.

Primary path:

```text
EventKind::InboundMessageReceived
        |
        v
fetch/read message record from SDK/daemon if event only contains metadata
        |
        v
normalize into OmenInboundMessage
        |
        v
store in OMENbrowser DB/message cache
        |
        v
notify UI
```

If the daemon returns raw LXMF payload bytes, decode with `lxmf-wire`:

```rust
use lxmf_wire::inbound_decode::{
    decode_inbound_message,
    InboundPayloadMode,
};

let decoded = decode_inbound_message(
    fallback_destination_hash,
    &payload_bytes,
    InboundPayloadMode::FullWire,
)?;
```

`DecodedInboundMessage` contains:

```text
id
source
destination
title
content
timestamp_f64
fields
```

If the inbound payload is destination-stripped, use:

```rust
InboundPayloadMode::DestinationStripped
```

and pass the fallback destination hash.

---

## Propagation Support

The Rust RPC contract explicitly includes propagation methods:

```text
propagation_status
propagation_enable
propagation_ingest
propagation_fetch
```

`propagation_enable` accepts:

```text
enabled
store_root
target_cost
```

`propagation_ingest` accepts:

```text
transient_id
payload_hex
```

`propagation_fetch` accepts:

```text
transient_id
```

The send method also supports:

```text
try_propagation_on_fail
stamp_cost
include_ticket
```

### Minimum viable propagation implementation

Implement these app-level operations:

```rust
pub async fn propagation_status(&self) -> Result<PropagationStatus, NetworkError>;

pub async fn enable_propagation(
    &self,
    enabled: bool,
    store_root: PathBuf,
    target_cost: Option<u32>,
) -> Result<(), NetworkError>;

pub async fn fetch_propagated(
    &self,
    transient_id: Option<String>,
) -> Result<Vec<OmenInboundMessage>, NetworkError>;
```

### Sender behavior

For a normal message with propagation fallback:

```text
1. Queue message with `try_propagation_on_fail = true`.
2. If direct delivery succeeds, mark delivered.
3. If direct delivery fails but propagation stores it, mark propagated/stored.
4. Store transient id if the daemon returns one.
5. Continue watching events/status.
```

For force-propagated/offline mode:

```text
1. Use method/option that selects propagation if exposed by SDK/RPC.
2. Include stamp/ticket settings if required by daemon policy.
3. Store transient id.
4. Display "stored for propagation" rather than "delivered".
```

### Recipient behavior

```text
1. On startup, check propagation status.
2. If enabled/available, fetch pending propagated messages.
3. Decode messages.
4. Deduplicate by message id and transient id.
5. Persist cursor/state.
6. Emit UI notifications.
```

### Propagation data model

Add fields to the local message DB/cache:

```text
message_id TEXT
transient_id TEXT NULL
source_hash TEXT
destination_hash TEXT
direction TEXT -- inbound/outbound
state TEXT -- queued/sending/delivered/propagated/fetched/failed
method TEXT -- link/opportunistic/propagated/unknown
title TEXT
content BLOB or TEXT
fields_json TEXT
created_at INTEGER
updated_at INTEGER
correlation_id TEXT NULL
delivery_trace_json TEXT NULL
last_error TEXT NULL
```

Deduplication rule:

```text
Prefer message_id when available.
Otherwise use transient_id + destination + source + timestamp/title/content hash.
When final message_id becomes available, merge transient record into final record.
```

---

## Announce and Path Discovery

OMENbrowser needs a way to announce the user’s LXMF destination and discover recipients.

The Rust RPC stable method set includes:

```text
announce_now
list_peers
peer_sync
peer_unpeer
clear_peers
```

Implement:

```rust
pub async fn announce_now(&self) -> Result<(), NetworkError>;
pub async fn list_peers(&self) -> Result<Vec<PeerRecord>, NetworkError>;
pub async fn sync_peer(&self, peer: String) -> Result<(), NetworkError>;
```

UI actions:

```text
Network -> Announce Now
Network -> Peers
Network -> Sync Peer
```

For debugging, include visible path/delivery traces in an advanced pane.

---

## Config and Interface Handling

The RPC contract includes:

```text
list_interfaces
set_interfaces
reload_config
```

But mutation rules are strict:
- Startup-only kinds such as `serial`, `ble_gatt`, `lora`, or unknown future kinds must return `CONFIG_RESTART_REQUIRED`.
- No partial apply is allowed when rejected.
- `reload_config` with startup-only changes should also require restart.

OMENbrowser should therefore:

```text
- If daemon says CONFIG_RESTART_REQUIRED, show "Restart networking required".
- Do not pretend serial/LoRa changes applied live.
```

---

## Error Handling

Create a local `NetworkError` with categories:

```rust
pub enum NetworkError {
    NotConfigured(String),
    DaemonUnavailable(String),
    Rpc(String),
    Sdk(String),
    InvalidState(String),
    Unsupported(String),
    PropagationUnavailable(String),
    Decode(String),
    Timeout(String),
}
```

Map SDK errors using:

```text
machine_code
category
retryability hints
```

Recovery policy:

```text
validation/configuration error -> show actionable UI error
runtime invalid state -> snapshot + restart runtime if safe
transport/auth error -> reconnect/backoff
stream gap -> snapshot and reconcile
decode error -> store raw payload for debug, do not crash
```

---

## Feature Negotiation

The SDK docs say to branch on effective capabilities returned by `start`.

Implementation:

```text
1. Request desired capabilities.
2. Inspect ClientHandle.effective_capabilities.
3. Enable only supported paths.
4. Missing optional capability = graceful degradation.
5. Missing required capability = fail fast.
```
If propagation capability is missing, direct messaging should still work and the UI should show propagation as unavailable.

---

## Testing Plan

### Unit tests

Add tests for:

```text
- OmenSendRequest -> SDK/RPC request mapping
- event -> OmenNetworkEvent mapping
- duplicate message merge by message_id/transient_id
- inbound decode from lxmf-wire fixture
- stream gap triggers snapshot-required event
- propagation unavailable returns typed error
```

### Events and receive

1. Implement async event subscription if available.
2. Otherwise implement cursor polling.
3. Map inbound/outbound delivery events.
4. Add snapshot recovery for stream gaps.
5. Decode raw inbound payloads with `lxmf-wire` if necessary.

Deliverable:

```text
OMENbrowser can display inbound message events and delivery status changes
```

### Propagation

1. Add `propagation_status()`.
2. Add `enable_propagation()`.
3. Add `fetch_propagated()`.
4. Add send option path for `try_propagation_on_fail`.
5. Store `transient_id`.
6. Deduplicate transient/final records.

Deliverable:

```text
OMENbrowser can use propagation fallback and fetch stored messages when daemon supports it
```

---

## Implementation Cautions

- Do not replace the mock runtime; keep it as the default test path.
- Do not silently spawn multiple unrelated Reticulum identities.
- Do not expose RPC outside localhost/Unix socket by default.
- Do not implement propagation as Reticulum transport forwarding. LXMF propagation is store-and-forward above RNS.
- Do not assume direct delivery failure means message failure if propagation fallback is enabled.
- Do not assume transient id equals message id.
- Do not busy-poll events if subscription is available.
- Do not crash on unknown future event kinds; log and continue.
- Do not hot-apply LoRa/serial config changes; daemon may require restart.

---

## Likely Code Skeleton

```rust
// src/network/sdk_adapter.rs

use async_trait::async_trait;
use serde_json::json;

use crate::network::{
    NetworkRuntime,
    NetworkError,
    RuntimeInfo,
    LocalIdentity,
    OmenSendRequest,
    OmenSendReceipt,
    OmenNetworkEvent,
    PropagationStatus,
    PropagationConfig,
    OmenInboundMessage,
};

pub struct SdkRpcNetworkRuntime {
    endpoint: String,
    // client: Option<lxmf_sdk::app::Client<...>>,
    // handle: Option<...>,
    cursor: Option<String>,
}

impl SdkRpcNetworkRuntime {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            cursor: None,
        }
    }
}

#[async_trait]
impl NetworkRuntime for SdkRpcNetworkRuntime {
    async fn start(&mut self) -> Result<RuntimeInfo, NetworkError> {
        // 1. Client::rpc(&self.endpoint)
        // 2. runtime().start_async(Config::desktop_default()).await
        // 3. store handle/client
        // 4. return runtime id + capabilities
        todo!("wire to lxmf-sdk exact API and compile-check");
    }

    async fn stop(&mut self) -> Result<(), NetworkError> {
        // shutdown graceful if SDK exposes it
        Ok(())
    }

    async fn local_identity(&self) -> Result<LocalIdentity, NetworkError> {
        // Use daemon_status_ex/status/snapshot depending on SDK exposure.
        // Must return identity_hash if available.
        todo!("read identity_hash from daemon status/snapshot");
    }

    async fn announce(&self) -> Result<(), NetworkError> {
        // call announce_now through SDK/RPC
        todo!("call announce_now");
    }

    async fn send_lxmf(
        &self,
        req: OmenSendRequest,
    ) -> Result<OmenSendReceipt, NetworkError> {
        // Build SendRequest.
        // Set TTL/correlation.
        // Add propagation options if supported.
        // Return message_id and maybe transient_id.
        todo!("send via lxmf-sdk");
    }

    async fn poll_events(&self) -> Result<Vec<OmenNetworkEvent>, NetworkError> {
        // Prefer subscription task in real implementation.
        // Fall back to poll_events(cursor, max).
        todo!("event mapping");
    }

    async fn propagation_status(&self) -> Result<PropagationStatus, NetworkError> {
        // call propagation_status through RPC/SDK
        todo!("propagation_status");
    }

    async fn enable_propagation(
        &self,
        cfg: PropagationConfig,
    ) -> Result<(), NetworkError> {
        // call propagation_enable(enabled, store_root, target_cost)
        todo!("propagation_enable");
    }

    async fn fetch_propagated(
        &self,
        transient_id: Option<String>,
    ) -> Result<Vec<OmenInboundMessage>, NetworkError> {
        // call propagation_fetch
        // decode returned payloads with lxmf-wire if raw
        todo!("propagation_fetch");
    }
}
```

---

## Source Pointers

Use these upstream references while implementing:

```text
https://docs.rs/lxmf/latest/lxmf/
https://docs.rs/lxmf-sdk/latest/lxmf_sdk/
https://docs.rs/lxmf-wire/latest/lxmf_core/
https://docs.rs/reticulum-rs/latest/reticulum_rs/
https://docs.rs/reticulum-rs-core/latest/rns_core/
https://docs.rs/reticulum-rs-transport/latest/rns_transport/
https://docs.rs/reticulum-rs-rpc/latest/rns_rpc/
https://github.com/FreeTAKTeam/LXMF-rs
https://raw.githubusercontent.com/FreeTAKTeam/LXMF-rs/main/docs/sdk/quickstart.md
https://raw.githubusercontent.com/FreeTAKTeam/LXMF-rs/main/docs/sdk/lifecycle-and-events.md
https://raw.githubusercontent.com/FreeTAKTeam/LXMF-rs/main/docs/sdk/advanced-embedding.md
https://raw.githubusercontent.com/FreeTAKTeam/LXMF-rs/main/docs/contracts/rpc-contract.md
https://github.com/markqvist/LXMF
https://reticulum.network/manual/
```
