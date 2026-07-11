# Reticulum/LXMF Migration Notes

These notes track the move toward the `reticulum-rs` / `lxmf` 0.6 crate family.

## Stack Decision

OMENbrowser_rs should not accidentally mix two Rust Reticulum implementations in
the normal native path.

Preferred primary stack:

- `reticulum-rs = 0.6.0`
- `reticulum-rs-transport = 0.6.0`
- `lxmf = 0.6.0`
- optionally `lxmf-sdk` / `reticulum-rs-rpc` if the SDK/RPC sidecar path proves useful

Previous compatibility stack:

- `rns-net = 0.5.10`
- `rns-core = 0.1.13`
- `rns-crypto = 0.1.8`

The compatibility stack is not part of the normal `native-network` feature path
anymore. After live Reticulum 0.6 parity testing for NomadNet page loads,
OMENchat, and LXMF direct/propagated messaging, the old crates were removed from
the manifests. The legacy feature names now exist only as explicit compile-time
error stubs for stale local commands.

## Feature Policy

Current browser feature intent:

- `default`: mock/UI/chat client only. This keeps ordinary builds from pulling
  either live Reticulum stack by accident.
- `native-reticulum`: reticulum-rs 0.6 core/transport only.
- `native-lxmf`: lxmf 0.6 wire support.
- `native-network`: `native-reticulum` + `native-lxmf`; this is the clean
  reticulum-rs 0.6 path.
- `native-rpc`: explicit `reticulum-rs-rpc` 0.6 dependency for daemon/RPC
  parity work.
- `native-lxmf-sdk`: opt-in SDK evaluation path through the `lxmf` umbrella
  crate's `sdk` feature plus the explicit `native-rpc` feature.
- `native-rns-net`, `experimental-rns-net-stack`, `legacy-live-rns-net`, and
  `chat-client-rns-legacy`: removed legacy feature names retained only to make
  stale commands fail clearly. They no longer pull `rns-net`, `rns-core`, or
  `rns-crypto`.
- `chat-client-reticulum`: preferred browser/client build for the clean
  reticulum-rs 0.6 path.
- `chat-client-rns` and `chat-client-rns-clean`: compatibility aliases for the
  current clean live path.

## Current Findings

`cargo tree -e features --features native-network` previously showed
`native-network -> native-rns-net -> rns-net/rns-core/rns-crypto`. That was the
accidental mixed-stack path.

`cargo tree -e features -i rns-net` showed `rns-net` was pulled directly by the
main crate through the `native-rns-net` feature, not by `reticulum-rs`.

After the feature split, `cargo tree -e features --features native-network`
shows only the reticulum-rs/lxmf family for Reticulum/LXMF:

- `reticulum-rs 0.6.0`
- `reticulum-rs-core 0.6.0`
- `reticulum-rs-transport 0.6.0`
- `lxmf 0.6.0`
- `lxmf-wire 0.6.0`

`rns-net`, `rns-core`, and `rns-crypto` are absent from `native-network` and
from the current manifests.

The old compatibility implementation files have now been removed:

- `src/runtime/native/rns_net.rs`
- `src/server/src/rns_net_live.rs`

Remaining `native-rns-net` / `live-rns-net` names are stale-command guardrails,
not implementations. Enabling them intentionally fails at compile time with a
message pointing operators to `chat-client-rns-clean`, `native-network`, and
server `live-reticulum`.

## Replacement Direction

Do not let UI code learn about either transport implementation. Keep live behavior
behind existing runtime traits and modules.

`src/runtime/native/request.rs` now exposes `native_reticulum06_capability_report()`
as the code-level status checkpoint for the clean stack. It currently reports:

- `reticulum-rs` runtime and `reticulum-rs-transport` link primitives are
  available.
- channel messages and resource transfer exist, but still need protocol
  compatibility verification for non-page OMEN workflows.
- Python-compatible `Link.request(path, data=...)` receipt handling has a local
  adapter boundary. Clean-stack request frames now use the public
  `Transport::send_request_resource()` API and wait for a response resource
  with the same request id. Python Reticulum normally reserves this path for
  oversized requests, but the receiver accepts request resources by advertisement
  flags/request id rather than by size. This path has been live-verified for
  basic NomadNet page loads and form-style page requests. A direct
  request-context link send primitive is still preferred later for small-packet
  efficiency.
- LXMF wire helpers compile, while SDK/RPC sidecar usage remains an opt-in
  evaluation path.
- OMENchat clean-stack parity now uses the same transport shape on both sides.
  `chat-client-rns-clean` opens `omenchat.node` links through
  `reticulum-rs-transport`, sends OMENchat frames as normal context-zero link
  data, sends upload/media resources through the public transport resource API,
  and drains active-link context-zero/resource events back into the existing
  desktop OMENchat client event flow. The standalone `omenchatd`
  `live-reticulum` build resolves the active inbound link id and sends normal
  response frames with `rns_transport::delivery::send_on_link`, while
  `omenchat-resource:` metadata is reserved for upload/media/history resources.
  The server accepts context-zero packets only when they decode as OMENchat
  protocol frames, with legacy `0x4f` support kept only for compatibility.
  This avoids the custom packet context in the clean path because the clean
  `PacketContext` enum cannot represent arbitrary user-defined contexts.
- Clean-stack LXMF direct receive now registers the local `lxmf.delivery`
  destination through `Transport::add_destination()` before the transport is
  shared. Announces use that same registered destination, so inbound link
  requests target a destination that can actually answer. Direct sends build
  LXMF wire deliveries with the `lxmf`/SDK encoder path and submit them through
  the clean Reticulum transport link/resource boundary instead of the legacy
  `rns-net` router. The receive bridge decodes LXMF wire payloads from the
  relevant clean transport event surfaces and de-duplicates by LXMF message id,
  because `reticulum-rs-transport` can report the same inbound LXMF payload as
  both full-wire received data and inbound link data.

`NetworkRuntime::probe_page_fetch()` now uses that report when built with clean
`native-network` and without `native-rns-net`. The probe parses the destination,
confirms the native runtime is running, and reports that reticulum-rs 0.6 link
primitives are available. It now marks the request-send stage as available
through the verified request-resource compatibility path. Remaining OMENchat and
LXMF replacement work is reported as parity context instead of a failed
browser-page diagnostic.

The narrower request/response probe shows the useful public pieces and the
remaining verification target:

- `reticulum-rs-transport` exposes `PacketContext::Request` and
  `PacketContext::Response`.
- inbound `ReceivedData` includes `request_id`, so a receiver can observe
  request/response-style data.
- `LinkPayload` also preserves context and request id on inbound link data.
- public `Link` constructors cover normal data packets and channel packets.
- public `Transport::send_to_out_links()` can send directly to active outbound
  links, but builds `PacketContext::None` packets.
- public `Transport::send_channel_message()` can send directly on a bound link
  interface, but frames payloads as `PacketContext::Channel`.
- public `Transport::send_request_resource()` and `resource_events()` cover and
  have live-verified the Python request-resource path. OMENbrowser uses it as
  the clean-stack request path for all NomadNet page requests until a direct
  small-packet helper exists.
- `Packet` exposes a mutable public context field, so OMENbrowser can build an
  encrypted link data packet and mark it as `PacketContext::Request` or
  `PacketContext::LinkIdentify`.
- public `Transport::send_packet` dispatches that packet through the normal
  transport path; public `Transport::send_direct()` can dispatch an already
  built packet on a specific interface.
- public `received_data_events()` exposes response-context link data for
  matching and decoding.
- `PacketContext::LinkIdentify` exists and inbound link handling preserves it.
  OMENbrowser sends it by building encrypted link data with `Link::data_packet`,
  marking the packet context, and calling `Transport::send_direct()` on the
  active link's ingress interface. This has been live-verified on an
  identify-on-connect NomadNet page.

That means the clean 0.6 stack now has a live-verified page-fetch path through
request resources and most of the public receive-side pieces for direct packet
requests, but not the key small-packet direct send-side primitive. An
established outbound link records an `ingress_iface`, and upstream
resource/channel helpers correctly send packets directly on that bound
interface. Those public helpers are not enough for small direct NomadNet page
requests, though: link-data helpers force `PacketContext::None`, and channel
helpers force `PacketContext::Channel`. Python Reticulum only invokes registered
request handlers for `PacketContext::Request`. The public generic packet send
path can carry a manually marked request packet, but routes by packet
destination instead. For link data packets the destination is the link id, which
is not normally in the path table, so generic dispatch falls back to broadcast
when transport broadcast mode is enabled. Live testing showed this exact
pattern: active link, outbound packet sent as broadcast across the three TCP
clients, and no response events.

`NativeLinkRequestAdapter` is the local boundary for this missing piece. The
current clean-stack implementation is `Reticulum06LinkRequestAdapter`, which
prepares the Python-compatible request frame and sends it through
`Transport::send_request_resource()` regardless of size. It intentionally does
not use generic packet dispatch for small requests because that path can
broadcast active link packets. If the transport grows a direct request-context
link-data helper, the adapter can switch small requests to that helper while
leaving large requests on request-resource.
`MissingReticulum06LinkRequestAdapter` remains as a guard/test adapter for
explicit unsupported cases. Future changes should keep improving this adapter
boundary, not bypass the runtime abstraction or teach the UI about transport
internals.

Clean-stack NomadNet identify-on-connect is represented explicitly in the page
fetch context. When a destination is marked identify-on-connect and the clean
stack establishes a page link, OMENbrowser sends the Python-compatible
LinkIdentify proof over the active link's bound interface. If the active local
identity cannot be loaded, or the link has no ingress interface, identify is
logged as skipped while the page request continues.

The clean adapter now logs the destination path status, link path status, and
bound link interface before dispatch. It sends a request resource and waits for
a response resource whose `request_id` matches the request frame. Once a future
transport version exposes a public direct request-context send helper, the
adapter should call that helper for small requests and then wait for
response-context link data whose framed `request_id` matches the request frame.

See `docs/RETICULUM_TRANSPORT_API_GAP.md` for the concrete upstream helper shape
OMENbrowser needs before the clean stack can make small page requests as
efficient as Python Reticulum's direct `PacketContext::Request` path.
See `docs/UPSTREAM_RETICULUM_TRANSPORT_REQUEST.md` for an upstream-ready issue
draft and acceptance checklist.

Request-resource timeout errors now include compact transfer counters such as
`target_events`, `progress_events`, `unrelated_events`, `outbound_complete`, and
`last_error`. These are surfaced through the normal page-load failure path so
clean-stack field tests can be diagnosed from the runtime log without requiring
a separate trace subscriber.

TCP client IFAC profile values are no longer only metadata on the clean path.
Inspection of published `reticulum-rs-transport` 0.6.0 showed that the stock TCP
client stores IFAC settings but still serializes packets directly to HDLC.
OMENbrowser_rs therefore uses a project-local TCP client interface for
IFAC-configured profiles. The adapter implements the public
`rns_transport::iface::Interface` trait, uses the published packet/HDLC/buffer
APIs, applies the Python-compatible IFAC wire transform before transmit, and
verifies/unmasks IFAC packets before handing decoded `Packet` values back to the
transport. Non-IFAC TCP profiles continue to use the stock upstream TCP client.

The same adapter is shared by the desktop runtime and the standalone
`omenchatd` `live-reticulum` server. A private-gateway smoke with one
`omenchatd` TCP client and two isolated OMENbrowser TCP clients passed through
the configured IFAC gateway: both clients opened the OMENchat link, joined the
lobby, and observed message echo. This keeps the clean path on published crates
as-is, without a vendored `[patch.crates-io]` transport fork.

The `native-lxmf-sdk` feature now has a small compile-time capability report.
It confirms the v0.6 SDK/RPC surface has desktop configuration, RPC backend
configuration, send requests, direct delivery selection, propagation retry
hints, stamp cost, ticket inclusion, RPC delivery options, and RPC ticket
records. Those map to existing OMENbrowser LXMF behavior: direct send,
propagated fallback, propagation retry/acceptance state, ticket/stamp metadata,
and delivery status tracking. It does not provide NomadNet page fetch or
Python-style `Link.request(path, data=...)`; page fetch still belongs behind
`NativeLinkRequestAdapter`.

`src/runtime/native_lxmf/client.rs` now has a small SDK send-plan adapter. It
maps the existing OMENbrowser `MessageEnvelope` into both
`lxmf::sdk::SendRequest` and `reticulum-rs-rpc::OutboundDeliveryOptions`.
This is the clean-stack send contract for `native-network`, which now enables
`native-lxmf-sdk` rather than only the wire codec. It preserves the current
direct/propagated delivery choice,
include-ticket flag, reply-ticket metadata, optional stamp cost, and attachment
metadata so the eventual SDK/RPC sender can replace custom internals without the
UI learning SDK-specific types.

The same module now defines a small `NativeLxmfSdkSender` trait and an explicit
`MissingNativeLxmfSdkSender`. This keeps the clean SDK/RPC boundary honest:
callers can depend on a sender-shaped interface, but the default implementation
fails before dispatch with a clear unsupported error until a real
`lxmf-sdk`/`reticulum-rs-rpc` endpoint is configured and live-tested.

`native_lxmf_sdk_runtime_boundary_decision()` records the current clean LXMF
runtime direction in code. The preferred next path is an SDK RPC sidecar/client
boundary through `lxmf::sdk::RpcBackendClient`, not embedding
`reticulum-rs-rpc::RpcDaemon` directly inside the Iced UI process. The embedded
daemon is available and useful for tests/probes, but live use still requires an
OMENbrowser-owned message store plan plus a real `OutboundBridge` that can
deliver over Reticulum transport. Keeping the first production integration as a
managed/local RPC endpoint matches the existing runtime abstraction, limits UI
process risk, and gives us a cleaner comparison against the live-tested legacy
direct/propagated/ticket/stamp/attachment behavior.

`RpcNativeLxmfSdkSender` is now the first concrete sidecar-facing sender shape.
It holds a configured local endpoint, reports `MissingEndpoint` before dispatch
when no endpoint is set, and uses the public `lxmf::sdk::SdkBackend::send`
method on `RpcBackendClient` when an endpoint exists. The implementation runs the
blocking RPC call off the async task thread. The clean `NetworkRuntime`
`send_message` branch now uses this sender when `native_lxmf_sdk_rpc_endpoint`
is configured. A missing endpoint fails locally with a clear unsupported error
instead of silently falling back to `rns-net` or pretending delivery happened.
Full parity still depends on a compatible local sidecar or embedded
`reticulum-rs-rpc` daemon that implements direct delivery, propagation delivery,
propagation sync, stamps/tickets, and attachment transfer over
`reticulum-rs-transport`.

The sender also exposes a diagnostic `probe()` call. It uses the public
`SdkBackend::snapshot` method through `RpcBackendClient` and returns the sidecar
runtime id, state, active contract version, and queue counts. A missing endpoint
or missing sender fails locally before any RPC attempt. This is a reachability
primitive only; it does not change the send path.

`EmbeddedNativeLxmfSdkSender` is now available as a testable in-process
`reticulum-rs-rpc::RpcDaemon` boundary. It submits the same SDK send plan through
the daemon's public `sdk_send_v2` method and lets an injected `OutboundBridge`
observe the resulting delivery. This is not yet the production live transport
path, but it proves the OMENbrowser send envelope can pass through the v0.6
SDK/RPC contract without using `rns-net`.

One upstream RPC detail is important for the real bridge: current
`reticulum-rs-rpc` parses `sdk_send_v2` delivery options without carrying the
reply ticket into `OutboundDeliveryOptions::ticket`, and it strips private
`_lxmf` fields before calling `OutboundBridge::deliver`. OMENbrowser therefore
places reply-ticket metadata in `_lxmf.ticket` for daemon submission, and any
transport-backed `OutboundBridge` must cache that value during
`validate_delivery` before the sanitized record reaches `deliver`. Tests now
cover direct/propagated plus ticket/no-ticket delivery through this daemon
boundary. `NativeLxmfSdkTicketCache` is the small reusable helper for that
validate/deliver handoff.

`AppSettings::native_lxmf_sdk_rpc_endpoint` now carries the optional SDK/RPC
endpoint through `NativeRuntimeConfig`. Blank values are treated as missing, and
debug output redacts the configured endpoint value. Native runtime status appends
`native_lxmf_sdk_rpc=disabled`, `missing_endpoint`, or `ready` depending on
compiled features and endpoint configuration. In the clean LXMF feature path,
this setting is also the dispatch endpoint for SDK/RPC sends.
Diagnostics snapshots also include `native_lxmf_sdk_rpc_probe`, which reports
`disabled`, `missing_endpoint`, `unreachable`, or the SDK sidecar snapshot state
without failing the rest of diagnostics export. The latest collected probe is
shown in the desktop and TUI Diagnostics panels so sidecar reachability can be
checked from the app without inspecting raw JSON.

Near-term cleanup path:

1. Keep mock runtime and UI builds green.
2. Keep stale legacy feature names as clear compile-time errors so old commands
   fail loudly instead of silently selecting a removed transport path.
3. Continue live-testing the reticulum-rs 0.6 `NativeLinkRequestAdapter`
   against real NomadNet pages, form posts, and timeout/error cases.
4. Use the SDK/RPC snapshot probe and clean `send_message` branch for configured
   endpoint reachability, then add controlled sidecar launch/connect handling
   around it.
5. Implement the real transport-backed SDK/RPC `OutboundBridge`: cache private
   LXMF ticket metadata in `validate_delivery`, encode/send the sanitized record
   over the clean `reticulum-rs-transport`/`lxmf` path in `deliver`, and wire
   propagation sync receive/update events back into OMENbrowser's message store.
6. Compare the clean path against legacy behavior only when a regression needs
   isolation:
   page fetch, request/response resources, path requests, link lifecycle,
   OMENchat frames/resources, LXMF direct sends, propagation sync, tickets, and
   file attachments.
7. Remove the compatibility stack after enough release mileage.

If direct embedded `reticulum-rs-transport` cannot provide the necessary behavior
cleanly, prefer a managed `reticulumd` / `reticulum-rs-rpc` sidecar boundary over
reimplementing transport internals inside the Iced process.

## Verification

Latest verification after the feature split:

- `cargo fmt --check`
- `cargo check`
- `cargo check --no-default-features`
- `cargo check --no-default-features --features mock-runtime`
- `cargo check --no-default-features --features "mock-runtime desktop-ui chat-client"`
- `cargo check --no-default-features --features native-lxmf`
- `cargo check --no-default-features --features native-lxmf-sdk`
- `cargo check --no-default-features --features native-network`
- `cargo check --features chat-client-rns-clean`
- `cargo check --features chat-client-rns`
- `cargo tree -e features -i rns-net --features native-network || true`
- `cargo test --no-default-features --features native-network native_reticulum06_page_fetch_probe -- --nocapture`
- `cargo test --no-default-features --features native-network reticulum06_capability_report -- --nocapture`
- `cargo test --no-default-features --features native-network reticulum06_link_request_adapter -- --nocapture`
- `bash scripts/release-check.sh quick`

All passed.

Quick runtime identity check:

```bash
./target/debug/omenbrowser_rs --version
```

For the current clean live build, this should show `chat-client-rns-clean:on`
and `native-network:on`. Legacy comparison builds were removed; successful
page/chat/LXMF runs now come from the clean `reticulum-rs` 0.6 adapter.

## Release Notes

The 0.6 Reticulum/LXMF crate family is EPL-2.0 licensed and has MSRV 1.85. The
project should acknowledge this before cutting a release based on the 0.6 stack.

## 2026-07-03 OMENchat IFAC Attach Ordering

Clean local OMENchat smoke tests passed, but live private-gateway runs could
still time out before `omenchatd` saw inbound links. The failing live configs
used IFAC on both OMENbrowser_rs and `omenchatd`.

Both clean attach paths were spawning TCP interfaces before applying
`InterfaceSharedConfig`. With `reticulum-rs-transport` 0.6, the interface
manager supports creating an interface context, applying shared config to that
channel address, and then spawning the task. OMENbrowser_rs and `omenchatd` now
apply IFAC before spawning TCP client/server tasks.

Normal OMENchat frames remain context-zero Reticulum link data. OMENchat
resources remain `omenchat-resource:` payloads. Clean `native-network` and
server `live-reticulum` still do not depend on `rns-net`.

Live verification markers:

- Browser startup should show `ifac_interfaces=[PrivateGateway:private_ret]`.
- Browser announce logs should include `native Reticulum classified announce`
  for OMENchat when the server announce is observed.
- `omenchatd` attached-interface logs should include `ifac=configured`.
- Old browser logs containing `reset stale clean out-link before link` indicate
  an older binary is still being run.

## 2026-07-03 OMENchat Multi-Gateway Path Selection

The next live private-gateway test showed the server and browser TCP interfaces
connected with IFAC configured, and the browser observed OMENchat announces, but
the browser still timed out during clean Reticulum link establishment while
`omenchatd` never saw an inbound OMENchat link.

The clean OMENchat client now records the Reticulum interface hash for each
attached TCP client and logs the hop count plus interface hash for every
classified announce. `omenchatd` also prints the interface hash for attached
TCP client/server interfaces. This makes it possible to compare:

- the server interface hash printed by `omenchatd`;
- the announce interface hash observed by OMENbrowser_rs;
- the path-table interface hash used before OMENchat link creation.

Follow-up live logs showed that using the locally attached interface hash as a
preferred-route key is too brittle. `request_path(..., Some(iface))` used a local
interface address, while `path_status.interface` and announce events reported
the route interface address learned by the transport. Those values did not match
in the live private-gateway setup, so OMENbrowser_rs kept waiting on a
nonexistent "preferred" OMENchat path and then retried stale wider-network
paths.

The clean OMENchat client now sends a normal all-interface path request and
accepts the path table reported by `reticulum-rs-transport`. Interface hashes
remain in debug logs for diagnosis, but OMENbrowser_rs no longer treats them as
an app-level routing policy. Reticulum remains responsible for selecting the
usable path.

Normal clean OMENchat frames are still context-zero Reticulum link data.
Upload/history/media resources are still `omenchat-resource:` payloads. This
change does not reintroduce `rns-net` into clean `native-network` or server
`live-reticulum`.

Follow-up live logs still showed OMENbrowser_rs repeatedly opening OMENchat
links against cached 12-13 hop paths while the server never saw an inbound
link. Matching OMENchat announces arrived immediately after those link attempts
timed out, which made the failure look like stale path-table use rather than an
OMENchat frame/protocol regression.

The clean OMENchat link setup now tracks recently observed OMENchat announces
and subscribes to runtime announce events before link setup. If the currently
known path is missing or high-hop, the client requires a recent/fresh matching
`omenchat.node` announce before opening the link. If no fresh announce arrives
and the only path remains missing/high-hop, the client refuses that stale cached
route and asks the user to trigger a fresh server announce/reconnect.
Direct/low-hop paths can still proceed immediately.

Verification:

- `cargo fmt --check`
- `cargo check --no-default-features --features "desktop-ui chat-client-rns-clean native-network"`
- `cargo check --manifest-path src/server/Cargo.toml --features live-reticulum`
- `cargo build --no-default-features --features chat-client-rns-clean`
- `cargo build --manifest-path src/server/Cargo.toml --features live-reticulum`
- `cargo tree -e features --features native-network -i rns-net || true`
- `cargo tree -e features --manifest-path src/server/Cargo.toml --features live-reticulum -i rns-net || true`
- `cargo check`
- `cargo test --features chat-client-rns-clean app::tests::app_persists_browser_and_conversation_session_descriptors_to_settings -- --nocapture`
- `cargo test --manifest-path src/server/Cargo.toml --features live-reticulum reticulum -- --nocapture`
- `bash scripts/release-omenchat-smoke.sh --browser-bin target/debug/omenbrowser_rs --server-bin src/server/target/debug/omenchatd --tcp 127.0.0.1:42424 --path-wait 75 --multi-client --keep-roots --out /tmp/omenbrowser-rs-clean-smoke-route-pref`

## 2026-07-04 Clean OMENchat Local Gateway Smoke

Clean OMENchat live parity was verified with an isolated local Reticulum 0.6
gateway, IFAC, a `live-reticulum` `omenchatd`, and two clean-stack
OMENbrowser_rs smoke clients.

The relevant transport behavior is:

- normal OMENchat frames are encrypted Reticulum link data with context zero;
- `omenchatd` sends responses with `reticulum-rs-transport`
  `delivery::send_on_link`;
- upload/history/media payloads remain Reticulum resources with
  `omenchat-resource:` metadata;
- clean `native-network`, `chat-client-rns-clean`, and server
  `live-reticulum` do not pull `rns-net`.

The earlier live private-gateway failure was reproduced in logs as a
pre-protocol transport problem: OMENbrowser_rs opened clean links against cached
12-13 hop paths or failed to learn the direct private-gateway path while
`omenchatd` never observed inbound links. Later inspection showed the underlying
IFAC issue was below path selection: the stock published TCP client stores IFAC
metadata but does not apply the IFAC wire transform.

OMENbrowser_rs must not rely on local/vendor patches for the
`reticulum-rs-transport` crate. The clean path now builds against the published
`reticulum-rs-transport = 0.6.0` API only. That means route-expiry and
"request path except this interface" diagnostics must stay at the app boundary
unless upstream exposes those helpers.

OMENbrowser_rs now handles that private-gateway case with a project-local TCP
client interface that implements the published `reticulum-rs-transport`
`Interface` trait. It uses public packet/HDLC/buffer APIs, applies the
Python-compatible IFAC signing/masking before transmit, verifies inbound IFAC
packets, and then returns normal decoded packets to the transport. This is not
a vendored crate patch. Non-IFAC TCP clients continue to use the upstream
interface.

Additional server visibility fix:

- `omenchatd` now counts outbound clean Reticulum frames/resources when they are
  accepted into the async Reticulum send queue. The file log still records the
  actual `send_on_link` result. This prevents the TUI/status summary from
  showing `frames_out=0` after a successful exchange.

Verification:

- `cargo fmt`
- `cargo test --no-default-features --features chat-client-rns-clean page_widget::tests -- --nocapture`
- `cargo check --no-default-features --features chat-client-rns-clean --bin omenbrowser_rs --bin omen-reticulum-gateway`
- `cargo check --manifest-path src/server/Cargo.toml --features live-reticulum`
- `cargo build --no-default-features --features chat-client-rns-clean --bin omenbrowser_rs --bin omen-reticulum-gateway`
- `cargo build --manifest-path src/server/Cargo.toml --features live-reticulum`
- `cargo tree -e features --features native-network -i rns-net || true`
- `cargo tree -e features --manifest-path src/server/Cargo.toml --features live-reticulum -i rns-net || true`

Smoke result:

- local IFAC gateway: `target/debug/omen-reticulum-gateway --listen 127.0.0.1:42442 --network-name private_ret --passphrase private-pass`
- server: `src/server/target/debug/omenchatd run --home /tmp/omenbrowser-rs-clean-smoke2/server-home --tcp-client 127.0.0.1:42442 --network-name private_ret --passphrase private-pass`
- client: `target/debug/omenbrowser_rs --omenchat-smoke <dest> --tcp-client 127.0.0.1:42442 --network-name private_ret --passphrase private-pass --path-wait 90 --omenchat-message 'clean smoke stats'`
- outcome: pass; link opened, room joined, message echo observed;
  server stats reported `frames_in=3 frames_out=4`.

Follow-up live fix:

- If a clean OMENchat link establishment times out, OMENbrowser_rs resets the
  pending outbound link with the published `Transport::reset_out_link()` API and
  requests path rediscovery again. Published `reticulum-rs-transport` 0.6.0 does
  not expose explicit route expiry, so OMENbrowser_rs logs that limitation
  instead of relying on a local patch.

Follow-up route diagnostics:

- Clean OMENchat link setup now uses the published `Transport::link()` API. Logs
  include the known route interface and hop count from public path status.
- After a clean OMENchat link-establishment timeout, OMENbrowser_rs can request
  rediscovery on attached interfaces or all interfaces. Published
  `reticulum-rs-transport` 0.6.0 does not expose "request path except interface",
  so the app does not attempt that patched-only behavior.
- The clean OMENchat failure boundary is still below OMENchat frame dispatch
  when `omenchatd` reports `active_links=0`; server-side protocol logs are not
  expected until an inbound Reticulum link actually activates.

Follow-up high-hop route guard:

- Live logs showed OMENbrowser_rs receiving the target OMENchat announce only as
  12-13 hop evidence while the server was expected to be reachable through the
  same private gateway. The desktop client was treating "recent OMENchat
  announce observed" as sufficient and then sending link requests through that
  high-hop route; `omenchatd` never saw an inbound link.
- Clean OMENchat announce tracking now records observed hops and interface for
  recent OMENchat announces. If the path table still only knows a high-hop route
  after the fresh announce wait, OMENbrowser_rs blocks the clean OMENchat link
  attempt with a diagnostic instead of repeatedly timing out a route that never
  reaches the server.
- OMENbrowser_rs does not currently override upstream route replacement
  behavior. If published `reticulum-rs-transport` keeps a stale/public high-hop
  path instead of replacing it with a nearer private-gateway announce, that needs
  an upstream transport fix or a sidecar/runtime boundary with complete route
  management.

Follow-up OMENchat service-route behavior:

- `omenchatd` now registers announce app-data with the clean Reticulum
  transport's local destination table before sending OMENchat and NomadNet
  announces. This keeps explicit announce packets and later local path responses
  aligned with Reticulum 0.6 local-destination behavior.
- OMENbrowser_rs now treats clean OMENchat link setup as a service-route lookup,
  not a generic "any cached path" lookup. If the cached OMENchat route is
  missing or more than one hop, the client requests path discovery on each
  attached clean interface with IFAC-configured interfaces first, and waits for
  a low-hop route before opening the link. It does not expire the route directly
  because that is not exposed by the published transport API.
- This is intentionally scoped to OMENchat link setup. Normal NomadNet page
  fetches still use the existing route behavior because high-hop public routes
  are valid for ordinary browsing.

Follow-up IFAC/private-gateway smoke result:

- The clean OMENchat protocol path passes on the published Reticulum 0.6 stack
  when the test server exposes a local non-IFAC `TCPServerInterface` and two
  isolated OMENbrowser clients connect through that endpoint. The smoke run
  observed link open, session accept, room join, and a second client receiving
  the first client's room message.
- The IFAC-gated private gateway topology now also passes with one `omenchatd`
  TCP client plus two isolated OMENbrowser TCP clients attached to the same
  configured gateway. Both clients opened the OMENchat link, joined the room,
  and observed message echo.
- `scripts/release-omenchat-smoke.sh` now has `--server-tcp-client`,
  `--network-name`, and `--passphrase` options so this external-gateway topology
  remains easy to retest. Prefer `OMENCHAT_PASSPHRASE` for real secrets.

Clean LXMF SDK bridge progress:

- OMENbrowser now has a clean `NativeLxmfSdkOutboundBridge` for the embedded
  `reticulum-rs-rpc` daemon path. It receives SDK/RPC outbound records, preserves
  reply-ticket metadata captured during validation, converts records into signed
  LXMF wire messages with the `lxmf` 0.6 wire API, and hands the resulting bytes
  to a small submitter trait.
- The bridge now covers both ticket directions needed by OMENbrowser: outbound
  ticket offers are encoded as the LXMF ticket field, and reply tickets are
  converted into LXMF stamps. Private SDK metadata is stripped from user fields
  before wire encoding.
- This no longer stops at the submitter boundary for direct delivery. The clean
  `reticulum-rs-transport` direct submitter now sends signed LXMF wire bytes to
  direct peers and the receive bridge maps clean-stack LXMF deliveries back into
  OMENbrowser conversation events. The remaining clean LXMF step is the proper
  propagation envelope/sync path for propagation nodes.

Clean LXMF direct submitter progress:

- OMENbrowser now has a clean `reticulum-rs-transport` submitter for embedded
  `reticulum-rs-rpc` SDK deliveries when no external SDK endpoint is configured.
  The submitter derives the local source as the active identity's
  `lxmf.delivery` destination, resolves the peer `lxmf.delivery` destination,
  opens a Reticulum link, and submits the signed LXMF wire bytes through the
  published `delivery::send_via_link` helper.
- The submitter is intentionally scoped to direct delivery for this step. If the
  SDK asks for `method=propagated`, OMENbrowser returns a clear unsupported
  error instead of pretending a bare peer LXMF wire message was delivered to a
  propagation node. The next LXMF clean-stack task is the proper propagation
  envelope/sync path using the 0.6 crates.
- The normal clean feature graph remains free of `rns-net`; the legacy stack is
  still isolated behind explicit compatibility features.

Clean LXMF propagation parity update:

- Clean propagated LXMF sends now build a propagation transient with the `lxmf`
  0.6 wire API, wrap it in the LXMF propagation envelope, and submit it to the
  selected propagation node through the clean `reticulum-rs-transport` link
  request path.
- Propagation stamps are generated locally with the Python-compatible LXMF
  stamp workblock algorithm. When propagation-node announce app-data is cached,
  OMENbrowser uses the advertised target cost. If a short-lived CLI/app process
  has a valid path but has not observed the node's app-data yet, it uses the
  common LXMF propagation default target cost of 16 instead of sending an
  unstamped envelope that the node will not retain.
- Clean propagation sync validates and strips propagation stamps before
  decrypting payloads. The decrypt path now matches the `lxmf` 0.6 wire crate:
  propagation transients are decrypted with the recipient identity hash as the
  transport-encryption salt, not the `lxmf.delivery` destination hash prefix.
- Local smoke against the configured private gateway and propagation node passed
  in both directions: A-to-B decoded two retained payloads with zero failures;
  B-to-A decoded one retained payload with zero failures. A direct A-to-B smoke
  also delivered to a live receive-only B listener. The sender-side direct
  smoke still timed out waiting for proof/reply evidence even though the
  receiver observed the message, so direct proof correlation remains a separate
  polish item.

Clean LXMF ticket parity update:

- Clean native LXMF now supports both ticket directions in the normal
  `chat-client-rns-clean` path without enabling `rns-net`.
- `include_ticket=true` inserts the LXMF ticket field into the signed wire
  payload. Inbound decode extracts that field into `native_lxmf_reply_ticket`
  metadata so future direct replies can reuse it.
- A valid `native_reply_ticket` now produces the LXMF reply-ticket stamp in the
  clean codec path. Direct stamp generation skips over that ticket-derived
  stamp, preserving the cheaper reply-ticket behavior.
- The app no longer clears persisted/native conversation ticket state merely
  because clean native LXMF is active. Retry restores the ticket flag when the
  stored failed row records `native_lxmf_include_ticket=true`.
- Verified with clean feature tests for ticket offer encoding, signed wire
  ticket metadata decode, reply-ticket stamp generation, ticket toggle/send
  state, and native ticket restore behavior.
