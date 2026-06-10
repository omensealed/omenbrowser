# 05 — Reticulum/LXMF Runtime

## Runtime goal

Reticulum and LXMF must be hidden behind a runtime abstraction. The app should run in mock mode without optional networking dependencies and should use real Reticulum/LXMF only through a controlled adapter.

The Python implementation has a very large `ReticulumAdapter`. Do not begin the Rust port by trying to rewrite all of it natively. Start with the trait, mock implementation, and bridge boundary.

## Python source

Reference files:

```text
src/omenbrowser/services/runtime.py
src/omenbrowser/protocols/mock_adapter.py
src/omenbrowser/protocols/reticulum_adapter.py
src/omenbrowser/services/propagation_probe.py
```

## Runtime service responsibilities

The runtime service must provide:

- current status;
- identity attachment;
- page fetching;
- downloads;
- message list;
- direct send;
- propagated send;
- contact creation if supported;
- delivery callbacks/events;
- outbound status callbacks/events;
- announce callbacks/events;
- debug callbacks/events;
- interface stats;
- network snapshot;
- path request;
- path warming;
- propagation node selection;
- propagation sync;
- destination inspection.

## Trait-first design

Define a trait similar to:

```rust
#[async_trait::async_trait]
pub trait RuntimeAdapter: Send + Sync {
    async fn status(&self) -> RuntimeStatus;
    async fn attach_identity(&self, identity: IdentityProfile) -> anyhow::Result<()>;
    async fn announce_identity(&self) -> anyhow::Result<bool>;

    async fn fetch_page(
        &self,
        url: &str,
        request_data: Option<BTreeMap<String, String>>,
        cancel: CancellationToken,
    ) -> anyhow::Result<BrowserPage>;

    async fn download_file(
        &self,
        url: &str,
        downloads_dir: &Path,
        cancel: CancellationToken,
    ) -> anyhow::Result<DownloadedFile>;

    async fn list_messages(&self) -> anyhow::Result<Vec<MessageSummary>>;
    async fn send_message(&self, draft: OutboundMessage) -> anyhow::Result<MessageSummary>;
    async fn create_contact(&self, peer_hash: &str, label: &str) -> anyhow::Result<()>;

    async fn set_outbound_propagation_node(&self, hash: Option<String>) -> anyhow::Result<()>;
    async fn get_outbound_propagation_node(&self) -> anyhow::Result<Option<String>>;
    async fn sync_propagation_messages(&self, limit: Option<u32>) -> anyhow::Result<()>;

    async fn request_path(&self, destination_hash: &str, reason: &str, sibling_aspects: bool) -> anyhow::Result<bool>;
    async fn warm_paths(&self, hashes: &[String], max_requests: u32, cooldown_secs: u64) -> anyhow::Result<u32>;

    async fn interface_stats(&self) -> anyhow::Result<InterfaceStats>;
    async fn network_snapshot(&self) -> anyhow::Result<NetworkSnapshot>;
    async fn directory_candidates(&self, limit: Option<usize>, include_propagation_usable: bool) -> anyhow::Result<Vec<DirectoryCandidate>>;
    async fn inspect_destination(&self, destination_hash: &str) -> anyhow::Result<DestinationInspection>;
}
```

Callbacks can be represented as channels:

```rust
pub enum RuntimeEvent {
    Delivery(MessageSummary),
    OutboundStatus(OutboundStatus),
    Announce(AnnouncePayload),
    Debug(String),
    Status(RuntimeStatus),
}
```

## Mock adapter

Port `mock_adapter.py` first. It must support:

- deterministic mock identity status;
- mock page fetches for `mock.node:` URLs;
- mock downloads into the downloads directory;
- mock messages;
- fake direct/propagated sends;
- outbound status events;
- fake directory candidates;
- fake propagation status;
- fake path request/warm behavior.

Mock mode is not optional. It is the foundation for UI and service tests.

## Real Reticulum/LXMF strategy

Because the current working libraries are Python, the Rust project should support one of these staged approaches:

### Stage A — subprocess/sidecar bridge

A Rust `BridgeRuntimeAdapter` starts or connects to a helper process that exposes the behavior needed by `RuntimeAdapter` over stdin/stdout JSON-RPC, Unix socket, or localhost socket.

This helper can initially reuse known-good Python Reticulum/LXMF code while Rust UI/services mature.

### Stage B — native Rust pieces where practical

Replace sidecar functions with native Rust implementation only when behavior is validated. Keep the trait stable.

### Stage C — fully native if ecosystem allows

Only attempt full native Reticulum/LXMF when protocol compatibility is understood and tested.

## Bridge protocol requirements

If using a bridge, commands should be explicit:

```json
{ "id": 1, "cmd": "status" }
{ "id": 2, "cmd": "attach_identity", "identity_path": "/path" }
{ "id": 3, "cmd": "fetch_page", "url": "hash:/page", "request_data": {} }
{ "id": 4, "cmd": "send_message", "peer_hash": "...", "title": "...", "content": "...", "via_propagation": false }
```

Events should be pushed independently:

```json
{ "event": "announce", "destination_hash": "...", "kind": "node", "display_name": "..." }
{ "event": "delivery", "peer_hash": "...", "title": "...", "content": "..." }
{ "event": "debug", "message": "..." }
```

Never pass identity private material over logs. If identity paths are passed, document and protect them.

## Page fetching behavior

Real page fetch must preserve Python behavior:

- parse destination hash and path;
- request/wait for path;
- recall identity;
- establish or reuse link;
- send NomadNet request path/data;
- handle cancellation;
- record success/failure;
- return `BrowserPage` with markup and metadata.

## LXMF behavior

Preserve:

- incoming message ingestion;
- message ID handling;
- attachment summaries;
- direct send;
- propagated send;
- include-ticket behavior if supported;
- propagation node selection;
- propagation sync;
- outbound delivered/failed events;
- stale/pending reconciliation.

## OMENchat Link behavior

OMENchat live sessions run over native Reticulum Links. Low-RTT Links in the
current Rust Reticulum stack can use a short stale window, so the client sends a
small application ping below that stale interval instead of waiting for a long
chat-idle timeout. This is intentionally lighter than repeated Link teardown,
reconnect, user-list refresh, and history sync.

Live room events are still treated as lossy until confirmed by the bounded room
history cache. If a client receives a live event with an event-id gap, it
schedules a bounded recent-history sync for that room so missed events are
repaired without polling continuously.

OMENchat client monitoring must expose live Link health without requiring users
to read debug logs: active link id, connection age, last RX/TX age, ping wait
state, frame/byte counts, resource bytes, reconnect attempts, and accumulated
connect/disconnect counts. `omenchatd` should create its log file during init and
start, but routine ping/pong frames should be counted in stats instead of being
written line-by-line; logs should stay focused on connect/disconnect, chat,
history, resource, announce, and error events.

Native Rust direct-send status must preserve the Python distinction between "queued/submitted" and "delivered". Python returns the outbound row with `delivered=false` and only flips delivery from the LXMF outbound delivery callback. The Rust native path may receive an RNS packet proof before any LXMF-router delivery callback equivalent exists; that proof is useful transport evidence, but it must remain peer-delivery-unconfirmed until LXMF delivery, peer activity, or another verified recipient-side receipt is observed.

RNS packet proof must not close the native direct-LXMF correlation. The pending correlation should remain available after packet proof so a later LXMF router delivered/failed callback or inbound peer-activity evidence can still update the original outbound row. A packet proof can suppress "no proof observed" timeout handling, but it is not terminal delivery evidence.

Native outbound LXMF messages must use the local registered `lxmf.delivery` destination hash as the LXMF source, not the raw Reticulum identity hash. The Python adapter registers delivery with `LXMRouter.register_delivery_identity(...)` and passes that delivery destination as the outbound message source. Rust direct sends, propagated sends, and LXMF delivery probes should therefore build/sign packets from the same local `lxmf.delivery` destination hash that was registered or announced at runtime startup.

Native direct sends should use the same practical transport shape as the Python LXMF router: establish an RNS link to the peer `lxmf.delivery` destination and transfer the signed LXMF wire message as a resource. Resource progress is in-flight transport evidence; resource completion is the closest current native equivalent to Python's direct outbound delivered callback. Timeout-style resource failures are not final peer-delivery failure evidence because live testing can show the peer received the message while the local resource completion callback timed out; those rows should remain unconfirmed/retryable. Raw RNS packet proof remains useful diagnostics for legacy packet-based sends and lower-level transport, but it is not peer delivery.

Direct resource status must not be flattened back into legacy packet-proof status. `direct_transfer_state=resource_advertised|resource_progress|resource_timeout|resource_completed|resource_failed` should populate direct-resource fields and proof/receipt wording, and the old stale packet-proof reconciler should only touch rows that are explicitly waiting for `waiting_for_packet_proof`. Otherwise successful Python-interoperable resource sends can be mislabeled as packet sends with missing RNS proof even though the remote peer received them.

For direct LXMF sends, use Python/LXMF router representation rules: if the signed LXMF wire payload fits in the link packet MDU (`431` bytes), send it as generic encrypted link data with context `0` and treat successful dispatch on the established link as delivered. Only larger direct messages should fall back to RNS resource transfer and wait for resource progress/completion/failure callbacks. This matches the Python-router behavior that triggers delivery callbacks for small direct messages without a resource proof lifecycle.

For propagated LXMF sends, use the same representation rule after building the propagation envelope (`[timestamp, [encrypted_lxmf_data(+stamp)]]`). If the propagation envelope fits in the link packet MDU (`431` bytes), send it as generic link data with context `0` to the selected `lxmf.propagation` node and mark it accepted by the propagation node while leaving peer delivery unconfirmed. Only larger propagation envelopes should use RNS resource transfer. This mirrors Python `LXMessage.PROPAGATED`, where the propagation node handoff can be a link packet or resource depending on packed size.

Propagation-node handoff is a visible outbound message state, not a background-only diagnostic. Once the selected propagation node accepts the link-packet or resource handoff, the conversation must show the sent row with peer delivery still unconfirmed. `propagation_status` should report `ready` when a selected propagation node has a known path and valid propagation app-data and there is no active non-terminal outbound transfer; `router_deferred` is reserved for missing readiness or deferred work.

Propagation sync returning no peer payload is not a failed send and must not replace the earlier propagation-node handoff receipt. If the outbound row already records `propagation_node_accepted_peer_unconfirmed`, later no-payload sync evidence should be stored as sync metadata while preserving the primary send state as accepted by the propagation node. The local "sent" criterion for propagated outbound LXMF is successful handoff to the selected propagation node; final peer delivery remains separate evidence.

The native Rust stack should keep Python `LXMRouter` parity behind a router boundary instead of letting the UI infer message state from raw link/resource callbacks. The native LXMF router layer owns translation from propagated transfer events to higher-level LXMF evidence such as `PropagationNodeAccepted` and `PropagationNodeFailed`. Propagation-node acceptance is not final peer delivery, but it is a successful router handoff that should clear pending composer state and produce a visible sent row. For propagated resource sends, the resource transfer must be queued asynchronously and the outbound row returned immediately, matching Python `LXMRouter.handle_outbound` behavior. Successful resource advertisement from the Rust stack is treated as propagation-node handoff because live Python propagation nodes can receive and store the payload even when the sender-side resource-completion callback is never observed. Final peer delivery remains reserved for explicit recipient evidence such as an inbound peer reply, a decoded propagation-sync payload, or a future verified LXMF delivered callback equivalent.

Propagated send path discovery must use a short active wait after requesting the selected propagation node path. Live runtime logs can show the path arriving less than a second after `request_path`; immediately storing `router_deferred` in that case creates false failures and delays visible delivery until restart/retry. The wait must remain bounded so UI tasks are not frozen indefinitely.

The persisted preferred propagation node is part of runtime startup state. On launch, the app should restore `preferred_propagation_node_hash`, apply it to the runtime adapter, and display it in the status strip without requiring the user to select the same propagation node again from Directory.

Native inbound LXMF must listen on every Reticulum delivery shape exposed by the selected Rust stack. For `rns-net`, that currently means local-destination delivery packets, inbound resources on the registered `lxmf.delivery` link destination, and generic link-data callbacks. Each path should pass bytes through the same LXMF wire decoder with a RawPacket payload fallback and emit normal message-store events. Decode failures should be logged with redacted link/context/size diagnostics so live Python-router drift can be identified without leaking message content.

Inbound peer activity is delivery evidence for any pending outbound to the same Reticulum identity family, not only an exact hash match. If a reply decodes with a NomadNet node, propagation, or raw sibling hash while the outbound row was keyed to `lxmf.delivery`, the runtime should resolve known sibling destinations and emit `InboundPeerMessage` evidence against the original pending outbound peer hash. This keeps the stored/visible sent row from staying in "no receipt observed" after the peer has demonstrably replied.

Startup should keep live directory and Reticulum known-destination preloads bounded to recent data. Live-only directory entries and known-destination rows older than the configured retention window should be pruned from active startup state so stale peer/path data does not slow down normal browsing and messaging. Managed `known_destinations` snapshots must preserve the actual observed timestamp when saving and must apply the same recent-data window on periodic saves; otherwise a stale key can be rewritten as fresh on every run and never expire.

## Propagation diagnostics

Port the diagnosis concept from `propagation_probe.py`. Show summaries such as:

- no propagation node selected;
- path missing;
- propagation sync active/stale;
- outbound queued;
- stamp/ticket issue;
- delivered;
- failed;
- timed out.

The UI should expose propagation problems as visible status cards, not just logs.

## Security and privacy

- Do not expose private identity data.
- Do not log passphrases or raw private keys.
- Redact message bodies from diagnostic export unless explicitly included.
- Treat plugin access to runtime as capability-gated.
- Make identify-on-connect explicit and directory-controlled.
