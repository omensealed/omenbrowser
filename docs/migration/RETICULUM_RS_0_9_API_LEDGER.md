# Reticulum-rs/LXMF 0.9 API Migration Ledger

Status date: 2026-07-16. Starting application commit:
`d0c147391e89427b3e309ecfaa8de6e95b561df8`.

This is the Phase 2 compiler and semantic migration ledger. “Compiles” means
the symbol exists in the selected crates.io 0.9 train and the relevant OMEN product
profile resolves it. It does not mean Rust-Python, NomadNet, LXMF, OMENchat, or
physical-interface interoperability has been proven.

The initial API migration inspected the annotated `v0.9.0` tag, commit
`0859680cb45bcd0ac481e80f4cce6a52222c6fc0`. Phase 7 unit 15 subsequently
inspected and selected the published `v0.9.5` train with maintainer approval.
Both OMEN Cargo roots now resolve coherent crates.io package identities with
exact `=0.9.5` direct pins.

## Boundary inventory

Direct upstream use is contained in these application boundaries:

- `src/runtime/native/identity.rs`: private identity admission and conversion;
- `src/runtime/native/announce.rs`: announce classification and destination
  reconstruction;
- `src/server/crates/omen-ifac-tcp/src/lib.rs`: project-local IFAC TCP interface;
- `src/runtime/native/request.rs`: NomadNet link/request-resource adapter;
- `src/runtime/native/adapter.rs`: integrated transport owner and event bridges;
- `src/runtime/native_lxmf/codec.rs`: LXMF wire/identity conversion;
- `src/runtime/native_lxmf/client.rs`: SDK/RPC bridge and send planning;
- `src/bin/omen_reticulum_gateway.rs`: isolated gateway utility;
- `src/server/src/reticulum_live.rs`: independent omenchatd transport owner.

The UI, storage, browser, messaging, and OMENchat session models consume
project-owned events and DTOs. They do not construct a Reticulum transport or
import transport implementation details.

## Core, destination, and announce APIs

| 0.6 API | Verified 0.9 API | OMEN modules | Semantic status | Required proof | Fallback |
|---|---|---|---|---|---|
| `Identity`, `PrivateIdentity` | Same public core types | identity, request, adapter, LXMF codec/client, server | Deterministic 64-byte fixture roundtrips through core and transport types with identical public keys and identity hash | Pinned-Python fixture remains a Phase 9 interop gate | Preserve existing private-key file parser and isolated paths |
| `PRIVATE_KEY_LENGTH` | Same constant in `reticulum-rs-core::identity` | identity | Compiles | Exact/short/long private-key admission tests | None |
| `PrivateIdentity::from_private_key_bytes` | Same constructor | identity, LXMF client | Exact-length fixture accepted; malformed existing file rejected without mutation or regeneration | Existing representative user-state copies and pinned-Python fixture | Do not regenerate on error |
| `PrivateIdentity::new_from_rand`, `new_from_name` | Same constructors | request tests, gateway, server | Compiles | Deterministic named identity hash; random identity persistence | Existing application identity manager remains authoritative |
| `Identity::new_from_slices` | Same constructor | adapter, LXMF codec | Compiles | Public-key split and destination reconstruction fixtures | Reject malformed key material |
| `DestinationName`, `DestinationDesc` | Same public destination types | announce, request, adapter, server | Fixed hashes verified for `nomadnetwork.node`, `lxmf.delivery`, `lxmf.propagation`, and `omenchat.node` | Pinned-Python recomputation remains a Phase 9 interop gate | Names/aspects must not change |
| `SingleInputDestination`, `SingleOutputDestination` | Same aliases | announce, request, adapter, server | Input/output destination hashes are equal for all four fixed application names | Live announce reconstruction | None |
| `NAME_HASH_LENGTH` | Same 10-byte constant | announce | Known node, delivery, propagation, and OMENchat hashes classify correctly; unknown stays unknown | Pinned-Python name-hash fixture | Unknown hashes stay unknown |
| `AnnounceEvent`, `recv_announces` | Same event and transport receiver | announce, adapter | Compiles; ordering/lag semantics pending | classification, duplicate, malformed app-data, lag, shutdown | Existing bounded project event bus |
| LXMF announce encode/decode helpers | Same `lxmf::wire::announce` functions | announce, LXMF codec | Compiles | display name, propagation cost, malformed and oversized app-data | Existing MessagePack preflight remains |

## Transport, interface, and path APIs

| 0.6 API | Verified 0.9 API | OMEN modules | Semantic status | Required proof | Fallback |
|---|---|---|---|---|---|
| `TransportConfig`, `Transport::new` | Same public construction surface | adapter, gateway, server | Integrated runtime replacement and stop now cancel owned announce/path-save workers; regression test proves old transport owners release | State-root/live restart remain in Phase 2.4 and the live handoff | Preserve current integrated runtime owner |
| `iface_manager`, `InterfaceManager::new_context` | Same manager/context surface | adapter, gateway, server | TCP tasks remain tied to upstream transport cancellation; project long-lived workers now have an explicit owner token | Live connection shutdown and server task ownership | Current task ownership and status handles |
| `InterfaceSharedConfig` | Same config with additive upstream capabilities | adapter, gateway, server | Enabled TCP clients require a non-empty host and nonzero port before tasks spawn; passphrases remain redacted from `Debug` | IFAC mode mapping and live invalid-credential behavior | Existing validated project profiles |
| `Interface` trait and channel types | Same public trait, `InterfaceContext`, `IfaceSource`, `RxMessage` | IFAC adapter | Exact wire bytes match the pinned Python Reticulum transmit path; wrong credentials, missing flag, and tampering are rejected | Supported Python-server/Rust-client sockets now pass split/coalesced reads, reconnect, wrong credentials, full Transport announce/path/identity/link/link-data; role reversal, IPv6, Resources, and multi-client remain | Retain project-local IFAC client implementation |
| stock TCP client/server and status handles | Same public modules and handles | adapter, gateway, server | 0.9 source audit confirms Packet↔HDLC without IFAC wire transformation; non-IFAC paths compile; omenchatd rejects an IFAC-configured stock server instead of claiming enforcement | client/server roles, IPv4/IPv6 where available, reconnect/watchdog, multi-client | Non-IFAC continues to use stock upstream TCP; IFAC server mode remains unsupported |
| `request_path` | Same method; 0.9 exposes additional path/rate surfaces not yet adopted | adapter | Missing/known/invalid/not-started paths and request-count bounds have deterministic coverage | Live dispatch traces and repeated-request rate behavior | Existing path request policy |
| `knows_destination`, `destination_identity` | Same lookup surfaces | adapter | Destination identity and announce app-data caches are item bounded; app-data is also per-item and aggregate-byte bounded | Live announce-to-identity binding and stale identity replacement | Project destination cache remains bounded |
| `path_status`, `TransportPathStatus` | Same status plus broader 0.9 metadata elsewhere | request, adapter | Inspection now maps authoritative `path_found` and hop count instead of discarding hop metadata | Live next-hop/interface and first-hop-timeout evidence | Project-owned diagnostic summary |
| path table save/restore | Existing `save_reticulum_path_table` and 0.9 restore report | adapter | Empty save and corrupt restore verified under an explicit isolated root; corrupt data returns `InvalidData`; periodic owner stops before transport release | Live route/identity restore and crash-boundary durability | Existing storage root and bounded interval |
| `iface_rx`, link/announce receivers | Same broadcast/event surfaces | adapter | Announce lag now emits a payload-free skipped-count diagnostic; destination caches remain bounded after recovery | Multi-process lag/recovery and receiver close | Bounded internal event bus |
| `reset_out_link` | Same public method | adapter | Compiles | stale pending link reset does not destroy unrelated link/path state | Existing high-hop/stale-route guard |

Phase 2.3 found that the announce listener and periodic path saver retained
`Arc<Transport>` after the application removed its runtime handle. Each
transport handle now owns a cancellation token. Replacement, normal stop, and
failure teardown cancel that token before the old handle is dropped. The
regression test starts, replaces, and stops isolated transports and waits for
their worker-held strong references to release. No identity or persistent
state format changed.

Phase 2.4 bounds each clean destination identity and recent OMENchat announce
cache to 256 items. Announce app-data is limited to 256 items, 4 KiB per item,
and 256 KiB aggregate. Oversized app-data is rejected with destination, byte
count, and limit only; payload bytes are never logged. Eviction is deterministic
and does not alter Reticulum/LXMF wire data or persisted path-table formats.
The pinned-Python and live lanes still own proof that restored routes and
identities behave identically across processes.

## Link, request, and receipt APIs

| 0.6 API | Verified 0.9 API | OMEN modules | Semantic status | Required proof | Fallback |
|---|---|---|---|---|---|
| `Link`, `LinkStatus`, `LinkEvent`, `LinkEventData` | Same public link types | request, adapter, server | NomadNet setup replaces stale links; local 0.9 evidence proves active-link reuse, explicit close, and fresh reconnect; a fixed cancellation-aware destination-stripe owner serializes reuse and failure teardown | Current-Python repeated requests, a 32-request idle/forced-close soak, and an asserted optimized comparison prove one-link reuse, exactly one bounded replacement, and release-profile behavior; no pinned NomadNet reference is defined | Existing bounded link wait and request-resource path; successful links are retained while failed operations close them |
| `Transport::link`, `find_in_link` | Same methods | request, adapter, server | Compiles | destination binding, identity isolation, stale link replacement | Current link maps and session ownership |
| `out_link_events`, `in_link_events` | Same event receivers | request, adapter, server | NomadNet setup rechecks authoritative link status after lag and rejects a closed event/stream explicitly | multi-process duplicate/lagged/closed streams and no lost terminal event | Project event queues remain bounded |
| `Link::data_packet`, public packet context mutation, `ingress_iface`, `send_direct` | Same low-level surfaces; upstream 0.9 `reticulumd` composes them for direct request packets | request, adapter | A deterministic in-memory active-link harness now proves encrypted `Request` construction, bound-interface selection, final packet-hash request ID, peer request-ID equality, and correlated `Response` parsing. The candidate is not dispatched by production NomadNet; direct LinkIdentify remains active. | pinned-Python empty/small/form response equality, timeout, cancel, link close, and reuse | Preserve direct LinkIdentify and the request-resource page path |
| `PacketContext::{Request,Response,LinkIdentify,None}` | Same variants | request, adapter, server | Compiles | context and request-id equality in both directions | Do not substitute `None`/`Channel` for NomadNet requests |
| `send_request_resource`, response resources | Same public request-resource surface | request, adapter, server portal | Outer resource metadata and embedded response request IDs must both match; malformed/trailing payloads remain rejected; the live-verified compatibility transport remains active despite the 0.9 direct-packet candidate | empty/small/form/large request, timeout, cancellation, link close, reuse, and byte equality remain pinned/live gates | Retain request-resource path for all NomadNet requests through Phase 4 proof |
| `received_data_events`, `ReceivedData`, `LinkPayload` | Same event/payload surfaces | request, adapter, server | Packet and resource response correlation share one exact request-ID matcher; unrelated valid responses are ignored rather than completing another request | duplicate-event dedupe and Rust/Python correlation remain live gates | Existing request-id checks and preflight limits |
| `send_on_link`, `send_via_link`, `await_link_activation` | 0.9 `send_on_link_observed` additionally exposes the selected packet or original resource hash before dispatch | adapter, server | Active clean LXMF now records either correlation hash before dispatch. Resource completion/failure/cancellation is mapped back to the LXMF message, removes the bounded correlation entry exactly once, and does not claim peer delivery from resource completion alone. | live packet/resource threshold, completion/failure/cancel, link reuse, restart recovery, and Python transfer proof remain gates | Existing OMENchat/LXMF adapter boundaries |
| packet proof/link proof events | 0.9 `Transport::set_receipt_handler`, validated `DeliveryReceipt`, and observed link-packet hashes | adapter, LXMF router, messaging service | Active clean transport now registers packet-hash correlation before dispatch, removes it on failed dispatch, maps the validated receipt back to the LXMF message ID, suppresses duplicate receipts, and keeps both submission and transport proof peer-unconfirmed. Correlations are bounded to 4,096 metadata entries with deterministic oldest eviction. The prior bug that treated clean sender acceptance as peer delivery is fixed. | A pinned Python two-send sequence defers the old proof across a bounded no-proof window, rejects a forgery, and exposes correctly signed old/current hashes in order; the paired production handler ignores removed old ownership and advances only the retry. Production scheduler timeout, process restart recovery, and authoritative LXMF delivery remain gates; resource terminal handling is recorded separately below | Current peer-unconfirmed state remains; only an explicit LXMF router delivery callback may set delivered |

## Resource APIs

| 0.6 API | Verified 0.9 API | OMEN modules | Semantic status | Required proof | Fallback |
|---|---|---|---|---|---|
| `ResourceEventKind::Progress` | Same variant; 0.9 currently emits it for inbound receiver progress | request, adapter | OMENchat forwards bounded metadata-only progress events; outbound clean LXMF terminal correlation uses the original resource hash because 0.9 has no outbound byte-progress variant | monotonic inbound bytes/parts and bounded UI forwarding remain live gates | Existing project progress DTO |
| `Complete` | Same final owned `ResourceComplete` payload | request, adapter, server | NomadNet/LXMF response resources require response classification plus matching outer and embedded request IDs; OMENchat still accepts final assembled bytes only from `Complete` | exact Rust/Python bytes and transfer-time size rejection remain live gates | Existing byte caps and single-consumer ownership |
| `InboundFailed`, outbound terminal variants | Same variants | request, adapter | Active clean LXMF maps outbound complete/failed/cancelled exactly once, releases the bounded correlation entry, emits lifecycle plus typed delivery evidence, and keeps successful resource transfer peer-unconfirmed. OMENchat retains its separate lifecycle mapping. | live failure/cancel races, shutdown, and sender/receiver proof semantics remain gates | Existing lifecycle DTO |
| no segmented completion variant | New `SegmentComplete(ResourceSegmentProgress)` | adapter | Explicitly migrated | segment ordering, final `Complete`, no duplicate payload retention, bounded diagnostics | Final assembled bytes remain accepted only from `Complete` |
| `resource_events` | Same broadcast receiver | request, adapter, server | Compiles | lag/close, unrelated resource filtering, shutdown | Bounded application/event queues |
| inbound cancellation limitation | 0.9 rejects advertisements above its internal 64 MiB transfer/data ceiling and bounds advertised parts, but exposes no application admission callback or inbound-cancel API before metadata/payload completion | adapter | Partially addressed upstream globally; OMENchat's tighter frame/upload limits remain post-completion because metadata is only public on `Complete`. Outbound cancellation is public as `Transport::cancel_resource`. | upstream application-admission API or live proof of a safe earlier metadata boundary is required before claiming transfer-time OMENchat caps | Retain post-completion OMENchat caps and the transport's global allocation bound; do not weaken either |

The integrated OMENchat bridge handles `SegmentComplete` explicitly and does
not retain segment data. It emits bounded diagnostic metadata and waits for the
transport's final `Complete` event before forwarding one assembled payload.
Targeted request loops use a final wildcard for unrelated segment events because
the final `Complete` remains their sole payload owner. Outbound clean LXMF
resources persist a distinct resource hash and correlate terminal transport
events without changing the LXMF or OMENchat wire formats.

## LXMF wire, SDK, and RPC APIs

| 0.6 API | Verified 0.9 API | OMEN modules | Semantic status | Required proof | Fallback |
|---|---|---|---|---|---|
| `Message`, `Payload`, `WireMessage` | Same umbrella exports from `lxmf-wire` | LXMF codec/client, adapter | Compiles and deterministic unit tests pass | pinned-Python wire bytes, storage container, malformed/oversized input | Existing allocation preflight and attachment limits |
| `TransportMethod`, `DeliveryDecision`, `decide_delivery` | Same wire-facing types | LXMF codec/adapter | Compiles | direct/propagated/opportunistic/paper mapping | Project `DeliveryMode` remains authoritative UX type |
| propagation envelope helpers | Same `WireMessage` helpers | codec/adapter | Compiles | Python propagation envelope/stamp/sync | Existing 0.6-compatible envelope path retained |
| `SdkConfig`, `StartRequest`, `SendRequest::with_ttl_ms` | Same SDK types with additive 0.9 fields/surfaces | request capability probe, LXMF client, messaging service/store, retry preparation, message detail | New logical sends generate bounded idempotency/correlation identifiers plus an absolute 1-second-to-24-hour deadline before dispatch. SDK and embedded-RPC plans carry the remaining TTL; successful and failed rows persist the operation fields. Unchanged retries reuse the original identity/deadline, while an edited or expired retry creates a new operation. Startup and a narrow deadline worker durably reconcile local expiry. Source inspection found that 0.9 `RpcBackendClient::send_params` discards TTL/idempotency/correlation before `sdk_send_v2`, so external-daemon TTL is locally enforced but not claimed daemon-enforced. | Live daemon duplicate suppression, TTL expiry/cancellation, disconnect, late terminal correction, and restart remain release evidence | No wire/schema/config change; legacy identifiers without TTL receive one bounded window on explicit retry |
| `SdkBackend`, `RpcBackendClient` | Same trait/client plus additive typed operations; `RpcBackendClient::new` begins in `LocalTrusted` auth mode | LXMF client | Active OMEN configuration now admits only absolute Unix sockets on Unix or literal IPv4/IPv6 loopback with a nonzero port. Remote, hostname, credential-bearing, unknown-scheme, and implied-TLS endpoints are rejected before client construction. A validated endpoint is `configured`, not `ready`, until a probe succeeds. Diagnostics redact Unix paths. | Unix socket ownership/permissions, daemon disconnect/reconnect, and authenticated token/mTLS configurations remain live/Phase 3 gates | RPC remains explicit opt-in and local-trusted only; authenticated remote mode is not exposed |
| `EventBatch`, `EventCursor`, `SdkEvent`, `RuntimeSnapshot` | 0.9 adds authoritative event-stream position, bounded async subscription, sequenced events, and configuration revision | request capability probe and owned SDK/RPC event worker | The optional local RPC probe maps snapshot fields without inferring capabilities. A separately owned worker now negotiates `sdk.capability.async_events`, preserves the accepted upstream cursor across reconnects, bounds retained event IDs and accepted event bytes, reports sequence/explicit gaps, and triggers snapshot recovery. | A live local `reticulumd` endpoint and reconnect fault test remain release evidence | Snapshot/poll recovery remains the authoritative gap fallback; no 0.6 transport workaround removed |
| `DeliveryState`, `DeliveryStateTransition` | 0.9 exposes queued, dispatching, in-flight, sent, delivered, failed, cancelled, expired, rejected, unknown, and terminal metadata | SDK/RPC event adapter, messaging service/store, compact message status | Authoritative transition payloads deserialize through the upstream non-exhaustive enum into a project-owned typed update. Sent remains distinct from delivered; terminal and sequence guards prevent stale regression. Typed values persist in existing message fields while legacy booleans remain a compatibility projection. | Live daemon delivery ordering, reconnect replay, and snapshot reconciliation remain release evidence | No schema or wire change; remove by reverting the typed event projection and field writes |
| `MessageHistoryListRequest`, `MessageHistoryPage`, `MessageHistoryRecord` | 0.9 exposes typed, cursor-based SDK history through `app.message.history.list` | local SDK/RPC sender, runtime history DTO, startup/gap recovery, message store | OMEN history remains authoritative. Recovery follows at most four 128-item/4 MiB pages and reconciles by message ID; matched receipt state may advance metadata without replacing local content, missed inbound rows may be imported, SDK-only outbound rows are rejected, deletion tombstones are honored, and restart/replay is idempotent. | Live daemon restart, histories beyond 512 records, current Python history, and mixed-version evidence remain | No database/wire/config migration; disable the optional SDK history fetch and retain local history unchanged |
| `CancelResult`, `sdk_cancel_message_v2`, `RpcBackendClient::cancel` | 0.9 exposes accepted, already-terminal, not-found, too-late, and unsupported typed outcomes | SDK/RPC client, runtime adapter, messaging service, application task boundary, conversation UI | Every typed outcome is preserved. One request per peer/message may be pending; accepted remains nonterminal until an authoritative cancelled transition arrives, so delivery/cancellation races cannot fabricate terminal state. Integrated transport and mock paths explicitly report unsupported. | Live local-daemon cancellation during dispatch, too-late race, disconnect, and restart reconciliation remain release evidence | No wire/schema/config change; remove by reverting the cancellation trait method, task result, and field projection |
| `OutboundDeliveryOptions`, `TicketRecord` | Same RPC types | request probe, LXMF client | Compiles | ticket/stamp policy and secret redaction | Existing local ticket storage policy |
| `RpcDaemon`, `RpcRequest`, `OutboundBridge`, RPC `MessageRecord` | Same public RPC bridge surfaces | LXMF client | Compiles and bridge unit tests pass; configured external endpoints are locally validated before probe/send and public failure snapshots contain only redacted endpoint classes | Unix socket ownership/authentication, restart, delivery state | No server dependency; embedded daemon remains behind desktop feature |
| `lxmf-runtime::InProcessBackend` | Published 0.9 crate, not a current dependency | none | Deferred | dependency/lifecycle comparison and full parity | Preserve existing integrated adapter |

Phase 3 owns typed lifecycle, capability negotiation, delivery status, events,
idempotency, cancellation, and history reconciliation. Phase 2 must not claim
those features merely because their 0.9 types compile.

Phase 2.9 inspected the tagged 0.9 source rather than inferring behavior from
release notes. The transport crate still has no high-level `Link.request`
method. Upstream `reticulumd` does demonstrate direct request-context dispatch
by building an encrypted link packet, setting `PacketContext::Request`, deriving
the request ID from the final packet hash, and using the active link's ingress
interface. OMEN records that as the Phase 4 small-request candidate but does not
activate it yet: its request ID differs from the request-resource frame hash,
and only the pinned Python/NomadNet matrix can prove response correlation and
handler behavior. Production page fetch therefore continues to use the bounded,
live-verified request-resource adapter.

Phase 2.2 deterministic evidence lives in the unit tests in
`src/runtime/native/identity.rs` and `src/runtime/native/announce.rs`. The
fixture contains no maintainer identity material and all file tests use an
explicit temporary root. These tests prove local 0.9 deterministic semantics;
they do not replace the pinned-Python interoperability lane.

## Standalone omenchatd boundary

The server imports only `reticulum-rs`, `reticulum-rs-transport`, and the
private protocol-neutral `omen-ifac-tcp` crate it owns. It does not import
`lxmf`, `lxmf-sdk`, `reticulum-rs-rpc`, the desktop crate, or Iced.
Its transport owner uses input destinations, link/data/resource event bridges,
TCP interface contexts, the shared project-local IFAC crate, and bounded
application queues. Both `server-headless` and `server-full` compile and test
from `src/server/Cargo.toml` with its independent lockfile.

The Phase 2.10 audit rechecked invalid-interface startup, signal-driven drain,
link/resource cleanup, reconnect ownership, database-worker closure, and exit
codes. Compilation alone was not treated as the independence gate.

Phase 2.10 now gives the standalone 0.9 runtime explicit ownership of its three
Reticulum event bridges, transport-command worker, and configured interface
workers. Headless handlers are armed before readiness is advertised, and
SIGINT/SIGTERM plus the TUI stop action share one idempotent, bounded shutdown
path: active OMENchat links are closed first, cooperative
workers are cancelled, interface workers are aborted at their owned boundary,
all handles are joined, queue permits are released, and logs are flushed before
success. Join or flush failure becomes a process error. Enabled interface
records are fully validated before any worker starts, and a startup guard aborts
already-created interface tasks if later initialization fails. The synchronous
SQLite session/store has no detached database actor; its existing blocking gate
remains the bounded ownership boundary and drops with the live server.

The 0.9 TCP client already owns reconnect/backoff. OMENchat therefore does not
restart the entire runtime for ordinary `connecting`/`reconnecting` states;
the TUI retains runtime restart only for fatal event-processing or announce
failures. This avoids duplicate transports and conflicting interface owners.
All three TUI recovery branches now finish the old runtime's bounded shutdown
before constructing its replacement; if shutdown fails, replacement is refused.
If replacement startup then fails, the stopped runtime is not polled as though
it were live and the operator must explicitly stop/start after correcting the
cause.
Context-zero OMENchat frames, `omenchat-resource:` metadata, destination names,
identity binding, and database schema are unchanged. The new 0.9 public
path-expiry/alternate-interface rediscovery surfaces are recorded but remain a
Phase 4 fallback-retirement decision pending mixed and pinned-Python evidence.
The standalone identity loader was also hardened: only a missing file or the
exact generated first-run placeholder may create an identity. Existing invalid,
unreadable, non-regular, and symlinked identity paths fail without mutation;
first-run key publication uses the existing owner-only atomic file primitive.

## Known retained compatibility paths

These remain intentionally active until Phase 4 interoperability evidence:

1. NomadNet requests use request resources while the 0.9 direct request-context
   packet candidate awaits pinned-Python/NomadNet proof.
2. IFAC-configured TCP clients use the project-local public-trait
   implementation; stock TCP remains used without IFAC.
3. OMENchat normal frames use compatible context-zero encrypted link data and
   resources without changing the protocol.
4. Direct LXMF proof correlation remains conservative and does not equate send
   acceptance with peer delivery.
5. Propagation stamp and ticket compatibility logic remains in the project
   codec until typed 0.9 behavior passes pinned-Python tests.

## Phase 2.11 build and dependency matrix

The 2026-07-15 final local matrix passed with locked dependencies and explicit
feature identities:

- root `mock-runtime`: check and complete tests passed (519 library tests plus
  applicable integration tests); two release-mode measurement tests were
  explicitly ignored;
- root `desktop-product`: check, complete tests, and all-target Clippy with
  warnings denied passed (1,085 library tests plus all applicable integration
  tests); measurement and explicit isolated-root pane fixtures remained
  ignored;
- standalone server with empty features: check, 152 tests, and all-target
  Clippy with warnings denied passed; one explicit slow-filesystem soak was
  ignored;
- standalone `server-headless`: check, 171 tests, and all-target Clippy with
  warnings denied passed; three explicit 60-second soaks were ignored;
- standalone `server-full`: check, 293 tests, and all-target Clippy with
  warnings denied passed; the same three soaks were ignored;
- `scripts/release-check.sh quick`, formatting, and diff whitespace checks
  passed, including the isolated and real-PTY TUI lifecycle checks;
- an isolated `omenchatd run` process handled immediate SIGTERM through the
  orderly drain and exited zero with no worker join timeout/failure or retained
  transport/event queue items.

Cargo metadata reports one registry package identity per Reticulum/LXMF family
member. The desktop graph resolves `lxmf`, `lxmf-reference`, `lxmf-sdk`,
`lxmf-wire`, `reticulum-rs`, `reticulum-rs-core`, `reticulum-rs-rpc`, and
`reticulum-rs-transport` at exactly 0.9.5 from crates.io. The server graph
resolves only `reticulum-rs`, `reticulum-rs-core`, and
`reticulum-rs-transport`, also at exactly 0.9.5 from crates.io. Neither
production tree contains a 0.6 family package or a Git/registry split. Other
locked transitive duplicate versions were reviewed and are outside this
migration's dependency family; no collateral upgrade was made.

The installed cargo-audit 0.22.2 still rejects `--locked`; its supported audit
command scanned each committed lockfile. The standalone server audit passed.
The root reproduced only the two baseline-tracked high-severity build-time
`quick-xml` 0.39.2 findings, RUSTSEC-2026-0194 and RUSTSEC-2026-0195, through
`wayland-scanner 0.31.10`. The scanner constrains `quick-xml` to `^0.39` while
the fix requires 0.41 or newer, so no compatible crates.io update exists at
this snapshot. The package parses trusted Wayland protocol XML during Linux
builds and is absent from browser/network input and omenchatd, but the findings
remain visible and are not ignored or patched locally; full reachability and
the upstream-resolution gate remain documented in
`docs/maintenance/DEPENDENCY_SECURITY.md`. `cargo deny check` passed its
advisory, ban, license, and source policies with documented duplicate and
unused-license warnings only.

## Phase 2 completion gate

- Every row above is marked semantically verified, explicitly retained for
  Phase 4, or deferred to Phase 3 with a named test owner.
- Root `desktop-product`, server `server-headless`, and server `server-full`
  pass check, complete tests, formatting, and all-target Clippy with warnings
  denied.
- No upstream type escapes into UI or persistence ownership layers.
- No unbounded queue, unowned task, lock-across-await regression, or blocking
  UI/database operation is introduced.
- No fallback, protocol field, destination name, schema, identity path, or
  state root changes without interoperability and migration evidence.
- Unrun live and Python tests are enumerated rather than reported as passes.

Rollback for Phase 2 is source-only: restore the relevant adapter change while
keeping the exact 0.9 dependency train. Identity, configuration, databases,
messages, history, uploads, and cache state are not migrated by this phase.

## Phase 2 closure verdict

Phase 2 is complete for deterministic local compilation, project-owned adapter
semantics, standalone-server lifecycle, and locked dependency identity. This is
not a v0.9.5-1 release-readiness claim. The following evidence remains outside
Phase 2 and is explicitly carried forward:

1. pinned Python Reticulum/LXMF link, request/response, resource, direct,
   propagated, proof, ticket, and stamp interoperability;
2. current-Python and NomadNet page/form/request/resource drift reporting;
3. mixed OMENbrowser/omenchatd 0.6.0-1 and 0.9.5-1 handshake, room, backlog,
   resource, restart, and reconnect tests;
4. Python IFAC bidirectional, wrong-credential, framing, and reconnect tests
   before considering removal of the project-local implementation;
5. the three explicit omenchatd 60-second soaks and desktop native performance
   measurements under isolated roots;
6. native Windows and macOS CI/build/runtime evidence; Linux results do not
   substitute for those gates;
7. upstream resolution of the two visible `quick-xml` build-time advisories.

No compatibility fallback is retired by Phase 2. Phase 3 may build typed
lifecycle, capability, delivery, and event-recovery behavior on these adapters;
Phase 4 owns fallback replacement decisions after interoperability evidence.

## Phase 3 deterministic closure audit

The 2026-07-16 aggregate audit closes Phase 3's deterministic implementation
gate. It does not close the live interoperability or release gates. The
project-owned lifecycle, capability, event, delivery, operation-identity, TTL,
cancellation, and history models described above remain behind the runtime
facade; no upstream SDK type is persisted directly or exposed to Iced.

The audit ran these canonical product identities from the independently locked
root and server workspaces:

- root `desktop-product`: complete tests passed, including 1,129 library tests
  plus every applicable binary, integration, and documentation target; two
  explicit measurement tests were ignored;
- root `mock-runtime`: complete tests passed, including 547 library tests plus
  every applicable integration and documentation target;
- root `desktop-product` and `mock-runtime`: all-target Clippy passed with
  warnings denied, formatting passed, and the working diff had no whitespace
  errors;
- standalone `server-headless`: check, complete tests, and all-target Clippy
  passed; 171 tests passed and three explicit soak/fault measurements were
  ignored;
- standalone `server-full`: check, complete tests, and all-target Clippy passed;
  293 tests passed and the same three explicit measurements were ignored;
- `scripts/release-check.sh quick` passed, including deterministic product
  feature assertions, release-version consistency, isolated TUI lifecycle,
  Linux real-PTY resize/signal restoration, focused OMENchat history/session
  checks, and independently initialized server checks.

The real-PTY smoke measured process delivery-to-exit at 64 ms for one SIGTERM,
66 ms for one SIGINT, and 64 ms for repeated SIGTERM on this host. These are
bounded lifecycle observations, not general performance claims. The audit did
not run a live external `reticulumd`, pinned or current Python peers, NomadNet,
mixed 0.6/0.9 peers, Windows/macOS native runners, hardware interfaces, or the
three explicit 60-second omenchatd soaks. Those remain named release evidence
and cannot be inferred from the green deterministic matrix.

Phase 3 is therefore complete for deterministic local behavior and product
profile validation. Phase 4 may now evaluate each retained 0.6 workaround, but
must preserve it unless its Rust-Rust and Rust-Python acceptance matrix passes.
Rollback remains source-only: remove the optional typed projections/workers and
retain the authoritative local history, existing wire formats, identities,
configuration, state roots, and compatibility transports unchanged.

## Phase 4 unit 2: IFAC TCP disposition

The 2026-07-16 source audit of published `reticulum-rs-transport` 0.9.0 found
that stock TCP client and server receive/transmit paths still convert directly
between `Packet` and HDLC frames. `InterfaceSharedConfig` records IFAC values
but those stock paths do not apply or verify the Python IFAC transform. The
project-local IFAC TCP client is therefore retained; this unit does not retire
the compatibility path.

An exact public wire fixture generated through pinned Python Reticulum commit
`15320e4d2cfabb143c1db20ca887e275fd521585` now passes in both the root and
standalone server workspaces. Wrong credentials, missing IFAC marking, and
tampering are rejected. omenchatd now fails startup validation for an
IFAC-configured stock TCP server instead of reporting unenforced metadata as an
active private gateway. Non-IFAC stock TCP server behavior and project-local
IFAC TCP client behavior are unchanged.

Validation passed:

- pinned Python fixture regeneration with
  `scripts/verify-ifac-python-vector.py`;
- root four-test focused IFAC suite and complete `desktop-product` suite;
- root all-target `desktop-product` Clippy with warnings denied;
- standalone seven-test focused IFAC suite and complete 174-test
  `server-headless` suite (three explicit soaks ignored);
- standalone `server-full` check and `server-headless` all-target Clippy with
  warnings denied;
- both formatting checks and the working-tree whitespace check.

This is deterministic wire evidence, not a completed live interoperability or
performance gate. Bidirectional Python sockets, split/coalesced reads,
reconnect, role reversal, multiple clients, IPv4/IPv6, and idle/reconnect
measurements remain pending. Rollback is source-only: remove the fixed fixture
and fail-closed server validation while retaining the existing local IFAC
client; no identity, configuration, protocol, database, or state migration was
performed.

## Phase 4 unit 3: OMENchat link-data/resource disposition

The tagged v0.6.0-1 client and server use OMENchat protocol version `1`, name
`omenchat-v0.1`, link context `0x4f`, and `omenchat-resource:` metadata. The
current frame and resource layouts remain unchanged; the intervening codec work
adds bounded validation and borrowed encoding without changing accepted bytes.

reticulum-rs 0.9's public high-level delivery helper sends generic context-zero
link data, and `PacketContext::from(0x4f)` represents the unknown application
context as generic `None`. The current adapter therefore cannot use that typed
enum to distinguish legacy `0x4f` from generic data after decoding. It accepts
only generic link data at that boundary and requires successful bounded
OMENchat frame decoding. omenchatd independently accepts valid `0x00` and
legacy `0x4f` frames, rejects context-zero non-frames, and retains `0x4f`
responses for old clients. Resources remain selected by their unchanged,
bounded metadata prefix.

The new shared public fixture records session-open, room-message, and
history-resource-offer bytes from the reviewed v0.6.0-1 contract. Both current
workspace roots encode each fixture exactly and decode it to the same typed
frame. This closes deterministic codec and context-admission evidence, but not
the multi-process interoperability gate. The current link-data/resource path is
retained; no wire, destination, identity, schema, or state migration is made.

Still required before any simplification:

1. v0.6 client to v0.9 server and v0.9 client to v0.6 server handshake, join,
   user list, backlog, room-message, compressed resource, and upload resource;
2. server restart, client reconnect, duplicate/replayed frame, malformed frame,
   wrong destination/identity, resource cancellation, and limit boundaries;
3. pinned-Python Reticulum link-data/resource transport evidence;
4. link-count, idle CPU, reconnect-rate, and resource-memory comparison.

Rollback is source-only: remove the shared fixtures and their assertions while
leaving the retained production compatibility path untouched.

## Phase 4 unit 4: direct proof/reply correlation disposition

Published `reticulum-rs-transport` 0.9.0 validates destination proof
signatures and link packet proofs before constructing `DeliveryReceipt` and
invoking the configured `ReceiptHandler`. OMEN's clean adapter records the
selected packet or original resource hash before dispatch, uses that exact hash
to recover the durable logical LXMF message identifier, and never treats the
transport receipt as peer-level LXMF delivery.

The existing bounded router tests cover duplicate receipts, oldest-entry
eviction at 4,096 metadata entries, failed-dispatch removal, timeout
idempotency, persisted packet/resource-hash recovery, and resource terminal
cleanup. This unit adds the missing retry-isolation regression: after the old
attempt correlation is retired, a stale receipt emits only a diagnostic and
cannot observe or complete the newer attempt; only the newer hash produces
peer-unconfirmed status and proof evidence.

The conservative compatibility behavior is retained. Still required before a
stronger delivery claim or fallback removal are live correct/stale/duplicate
proofs, timeout/retry races, process restart reconciliation, pinned-Python
receipt/hash equality, and an authoritative LXMF router/SDK delivery event.
No protocol, identity, destination, configuration, database schema, or state
root changed. Rollback is source-only: remove the added regression and these
disposition notes; the production correlation path remains unchanged.

Validation passed with the three focused clean-receipt/resource filters, root
formatting, root `desktop-product` check, the complete 1,138-test root library
suite plus integration/binary tests, all-target `desktop-product` Clippy with
warnings denied, the working-tree whitespace check, and
`scripts/release-check.sh quick`. The quick gate also rechecked deterministic
product features, version consistency, isolated TUI lifecycle/real-PTY signal
handling, focused OMENchat behavior, and standalone omenchatd feature/config
smokes. No live Python peer or performance measurement ran in this unit.

## Phase 4 unit 5: peer stamp-cost negotiation disposition

Published 0.9 exposes delivery/propagation announce cost helpers,
`SendRequest::stamp_cost`, RPC delivery options, ticket records, outbound-cost
queries, and router policy/status metadata. OMEN already retains advertised
delivery cost in its bounded directory, maps stamp cost and ticket intent into
the typed SDK/RPC plan, generates and validates low-cost direct/propagation
stamps, persists bounded reply tickets, rejects expired/invalid tickets, and
shows generated stamp/ticket evidence on messages.

This unit adds a project-owned direct-cost policy decision and differential
parser proof against published `lxmf-wire` 0.9. The policy distinguishes
unknown legacy/missing data, explicit no-cost, a required admitted cost, a
valid reply-ticket override, and unsupported malformed/out-of-range values.
Ticket precedence is deterministic and no reusable ticket bytes enter the
decision or diagnostics.

Automatic integrated direct-cost generation remains intentionally disabled.
An announce is untrusted input, and the current direct generator is CPU-heavy;
starting it automatically before defining a maximum-cost/work budget, bounded
cancellable worker, shutdown ownership, and user policy would create a remote
resource-exhaustion boundary. The gap is therefore retained rather than hidden
or weakened. Pinned/current Python required/accepted/rejected behavior,
propagation flexibility, ticket issue/use/expiry/reuse, high-cost performance,
and cancellation remain release evidence.

No wire, identity, destination, configuration, database schema, or state root
changed. Rollback is source-only: restore the prior optional-cost parser and
remove the policy tests/docs. Phase 4 is deterministically dispositioned: all
five 0.6 workarounds are either retained with explicit evidence gates or have a
tested 0.9 candidate; none was removed without live interoperability proof.

Validation passed with the focused direct stamp-policy/parser, generation, and
ticket filters; root formatting; root `desktop-product` check; the complete
1,140-test root library suite plus integration/binary tests; all-target
`desktop-product` Clippy with warnings denied; the working-tree whitespace
check; and `scripts/release-check.sh quick`. The quick gate also rechecked the
product feature assertion, release-version consistency, isolated TUI lifecycle
and real-PTY signal handling, focused OMENchat behavior, and standalone
omenchatd feature/config smokes. No live Python peer, high-cost proof-of-work,
cancellation, or performance measurement ran in this unit.

## Phase 5 unit 1: read-only lifecycle and capability diagnostics

The runtime facade already exposed project-owned lifecycle and typed capability
snapshots, but the general diagnostics service discarded both. This unit adds
them to the explicit diagnostics collection and redacted JSON export. After a
collection, the TUI and desktop diagnostics views show lifecycle/backend plus
supported, unsupported, and unknown capability counts. The individual records,
evidence sources, and user-safe details remain available in the JSON without
leaking Reticulum implementation types into UI state.

Collection remains user-driven and introduces no status polling, redraw timer,
queue, cache, dependency, configuration key, protocol field, schema migration,
or state-root change. Runtime failure technical detail is replaced with
`<redacted>` in exported snapshots; category, retryability, and user-safe
summary remain diagnosable. Missing or unnegotiated capabilities remain
`unknown`, so this does not claim shared-instance ownership, live interface
mutation, ticket/stamp negotiation, or current remote-daemon support.

Rollback is source-only: remove the two snapshot fields, stored panel copies,
compact view lines, and their tests/docs. Existing runtime lifecycle and
capability negotiation behavior is unchanged.

Validation passed with the 27-test focused diagnostics filter, root formatting,
the working-tree whitespace check, root `desktop-product` check, the complete
1,140-test root library suite plus integration/binary tests, all-target
`desktop-product` Clippy with warnings denied, and
`scripts/release-check.sh quick`. The quick gate additionally exercised the TUI
render/lifecycle path, real-PTY signal restoration, product/version assertions,
focused OMENchat behavior, and standalone omenchatd feature/config smokes. No
live Reticulum/Python peer or performance measurement ran in this unit.

## Phase 5 unit 2: authoritative path/interface availability

The existing network snapshot carried path-table count, request-failure count,
and shared-instance booleans, but both active adapters populated them with
hard-coded zero/false or configuration-mode projections. The desktop Network
Doctor could therefore label missing evidence as an empty path table, zero
failures, or managed/shared runtime ownership. This unit adds explicit additive
availability flags and makes both diagnostics views fail closed: unavailable
metrics are rendered as unavailable, never as observed zeroes.

Adapter-owned announce counts, pending announces, known destination count, and
typed interface samples remain visible. The compact diagnostics projection
reports interface availability plus enabled/attached/unsupported sample counts
and network cache counts. It does not expose interface credentials or endpoint
details in the compact line. Exact per-destination path/hop state remains in the
existing explicit inspection path; aggregate next-hop, selected interface,
first-hop timeout, blackhole, restored-path, active-link, and shared-instance
runtime evidence remain pending until a typed 0.9 source is integrated.

The three new snapshot fields are additive and default to unavailable during
deserialization. There is no network wire, identity, destination, application
configuration, database schema, dependency, queue, cache, polling, or state-root
change. Rollback is source-only: remove the availability fields and compact
lines, then restore the former Network Doctor rendering; no persisted user data
requires conversion.

Validation passed with the 27-test diagnostics filter, 16-test Network Doctor
filter, root formatting and whitespace checks, root `desktop-product` check,
the complete 1,141-test root library suite plus integration/binary tests,
all-target `desktop-product` Clippy with warnings denied, and
`scripts/release-check.sh quick`. The quick gate additionally covered TUI
render/lifecycle behavior, real-PTY signal restoration, deterministic product
and version assertions, focused OMENchat behavior, and standalone omenchatd
feature/config smokes. No live Reticulum/Python peer, native shared instance,
or performance measurement ran in this unit.

## Phase 5 unit 3: explicit local announce hardening

The identity UI's `Announce Now` action invoked the normal local
`lxmf.delivery` announce, but did not say that it was non-targeted and only
coalesced duplicate requests while one task remained in flight. A user could
immediately repeat the operation after completion. Pre-send and pre-inspection
announce failures could also leave a deferred action retained.

This unit relabels the action `Announce Local LXMF`, explains that it is a normal
non-targeted announce, and applies one 30-second monotonic cooldown across the
manual, pre-send, and pre-inspection application paths. Only one task may be in
flight. Deferred work is cleared when the required announce cannot start, and
success/refusal/failure now produce explicit task status. Startup behavior
continues to use the existing `announce_on_start` configuration; no periodic
announce, timer subscription, target field, new queue, cache, dependency, wire
field, identity change, or persisted setting is introduced.

The lower runtime API remains unchanged, so the separately labeled live interop
diagnostic can deliberately announce while collecting test evidence. Targeted
0.9 announces remain deferred until an exact public API, user story, rate
policy, and live interoperability test are integrated. Rollback is source-only:
remove the cooldown timestamp/checks and restore the former label; user data
requires no conversion.

Validation passed with the two-test local-announce filter, the focused
rate-limited pre-send regression, root formatting and whitespace checks, root
`desktop-product` check, the complete 1,143-test root library suite plus
integration/binary tests, all-target `desktop-product` Clippy with warnings
denied, and `scripts/release-check.sh quick`. The quick gate additionally
covered TUI render/lifecycle behavior, real-PTY signal restoration,
deterministic product and version assertions, focused OMENchat behavior, and
standalone omenchatd feature/config smokes. No live announce, targeted announce,
Python peer, or performance measurement ran in this unit.

## Phase 5 unit 4: external/shared runtime ownership gate

Audit found that the saved `External` mode still constructed OMENbrowser's
integrated transport and interface tasks; only destination preload behavior
changed. The network snapshot also projected configured Managed/External values
into booleans named like live shared-instance state. This could mislead users
and risk a conflicting second runtime beside an operator-managed instance.

This unit makes the deferred mode fail closed at both application startup and
the public native-adapter start boundary. No identity is auto-created and no
integrated interface task starts. The snapshot no longer copies configuration
mode into live shared-instance booleans. Diagnostics instead show configured
ownership (`managed_integrated` or `external_deferred`) separately from the
typed negotiated `SharedInstance` capability. Settings and Quickstart label
external/shared support deferred, and `docs/NETWORK_BACKENDS.md` records the
ownership and security contract.

Existing External configuration remains readable and unchanged. This is an
intentional behavior correction: users selecting External now receive a clear
blocker and must select Managed to run the integrated v0.9.0-1 backend. A local
LXMF SDK/RPC endpoint remains usable only for its negotiated SDK/event surfaces
and is not promoted to a full Reticulum/NomadNet/OMENchat backend. There is no
wire, identity format, destination, database schema, dependency, queue, cache,
polling, or state-root change.

Rollback is source-only, but restoring the former behavior would reintroduce
the ownership ambiguity. A future replacement must negotiate a live shared
backend, enforce local endpoint security, recover from disconnect, and pass
multi-process ownership/restart tests before this gate is removed.

Validation passed with the two-test external-mode filter, configured-versus-
negotiated ownership projection test, settings persistence regression, root
formatting and whitespace checks, root `desktop-product` check, the complete
1,146-test root library suite plus integration/binary tests, all-target
`desktop-product` Clippy with warnings denied, and
`scripts/release-check.sh quick`. The quick gate additionally covered TUI
render/lifecycle behavior, real-PTY signal restoration, deterministic product
and version assertions, focused OMENchat behavior, and standalone omenchatd
feature/config smokes. No external daemon, shared instance, multi-process
ownership test, Python peer, or performance measurement ran in this unit.

## Phase 5 unit 5: restart-scoped interface controls

The interface service's `apply` operation atomically rewrites the managed
Reticulum configuration file; it does not call a running transport or negotiate
the 0.9 live interface-mutation surface. The UI previously described most
changes as only "restart recommended", omitted that warning for the I2P
connectable toggle, and could therefore imply that a running runtime had been
changed.

This unit makes the boundary explicit for create, edit, enable/disable,
connectable-toggle, and delete operations: the profile and managed config are
saved immediately, while transport behavior changes only on the next runtime
start/restart. Desktop and TUI interface views state the same rule, delete
confirmation describes a config rewrite rather than a live reapply, and a
detected runtime/config mismatch requests a runtime restart. The native
capability record remains `interface_mutation=unknown`; no support is inferred
from the crate version.

There is no wire, identity, destination, application setting, database schema,
dependency, queue, cache, polling, or state-root change. Rollback is source-only
and would restore ambiguous UI language; profiles and generated configuration
remain compatible. Live mutation remains deferred until its public API,
ownership, cancellation, recovery, and native tests are established.

Validation passed with the 28-test interface filter, the native lifecycle and
capability regression, root formatting and whitespace checks, root
`desktop-product` check, the complete 1,146-test root library suite plus all
integration/binary/doc tests, all-target `desktop-product` Clippy with warnings
denied, and `scripts/release-check.sh quick`. The quick gate additionally
covered product/version assertions, isolated TUI lifecycle and real-PTY signal
restoration, focused OMENchat behavior, and standalone omenchatd feature/config
smokes. No live interface mutation, runtime-restart transport comparison,
Python peer, native platform, or performance measurement ran in this unit.

## Phase 5 unit 6: NomadNet request-resource ownership and truthful diagnostics

Production NomadNet page fetch correctly retained the live-proven
request-resource compatibility path, but its cancellation and timeout exits
returned without invoking Reticulum 0.9's public `cancel_resource()` API. Link
teardown followed at a higher boundary, but that did not prove outbound
resource state was released or send the peer's initiator-cancel packet. A
successful page also reported only `reticulum-transport`, leaving diagnostics
unable to distinguish the retained compatibility primitive from the unproven
direct request-context candidate.

This unit gives the adapter explicit ownership after request-resource
advertisement. Browser cancellation, response timeout, and resource-event
stream closure attempt bounded outbound cancellation before returning; cleanup
failure is diagnostic and never replaces the original operation result.
Request-resource terminal events are now labeled outbound, while response
resource progress/failure remains inbound. Successful pages add the
`native_request_primitive=request-resource` field, and browser status, trace,
desktop diagnostics, and TUI diagnostics render the backend/primitive pair.

Two isolated active-link regressions observe the outbound advertisement, then
require an actual `ResourceInitiatorCancel` packet for cancellation and timeout.
The cancellation case also requires an outbound `nomadnet-page` lifecycle event
with the user-cancellation reason. Direct request dispatch remains disabled;
the 0.6-compatible fallback, frame encoding, request IDs, response parsing,
destination names, identity behavior, and page data are unchanged.

There is no wire, configuration, database schema, dependency, queue, cache,
polling, or state-root change. The additive page metadata remains compatible
with older cached pages that lack it. Rollback is source-only, but would
reintroduce unowned resource cleanup and ambiguous diagnostics. Pinned/current
Python request, form, response equality, link-close/reuse, and performance
evidence remain pending.

Validation passed with the two active-link request-resource ownership tests,
backend/primitive browser status regression, desktop diagnostics regression,
TUI diagnostics regression, root formatting and whitespace checks, root
`desktop-product` check, the complete 1,148-test root library suite plus all
integration/binary/doc tests, all-target `desktop-product` Clippy with warnings
denied, and `scripts/release-check.sh quick`. The quick gate additionally
covered product/version assertions, isolated TUI lifecycle and real-PTY signal
restoration, focused OMENchat behavior, and standalone omenchatd feature/config
smokes. No live NomadNet/Python peer, direct request candidate, link reuse,
native platform, or performance comparison ran in this unit.

## Phase 5 unit 7: NomadNet transfer and valid-empty presentation

The request-resource adapter already emitted typed direction for its resource
events, but the application flattened all progress into a generic `resource`
message. During a page operation that made an outbound request-resource upload
indistinguishable from an inbound response-resource download. Separately, a
zero-byte UTF-8 response was correctly accepted as a network page but was
presented exactly like a non-empty page, leaving no positive indication that
the empty response was valid rather than missing.

This unit retains the existing runtime-wide resource status boundary while
making its language truthful. Outbound `nomadnet-page` events are labeled
`NomadNet request upload`; inbound events are labeled `NomadNet response
download`; complete, failed, and cancelled states keep their typed lifecycle
meaning. Native page conversion records additive boolean
`native_response_empty` metadata, and successful empty pages report a valid
empty response together with the verified backend/primitive pair.

The design deliberately does not associate a transfer with a browser tab by
destination or event timing. The current event contract lacks the browser task
generation or another stable operation identifier, and guessing would allow a
concurrent request to overwrite the wrong tab. Existing global status,
Network Doctor rows, timeout/cancellation ownership, page bytes, and browser
task cancellation remain unchanged.

There is no wire, protocol, configuration, database schema, dependency, queue,
cache, polling, timeout, or state-root change. Older cached pages without the
new metadata retain their prior status. Rollback is source-only and would
restore generic transfer wording and ambiguous empty-page presentation.
Pinned/current Python empty-response equality, concurrent live page transfers,
per-operation correlation, native platforms, and performance evidence remain
pending.

Validation passed for empty native conversion, directional NomadNet resource
status, and empty-page completion status, root formatting and whitespace
checks, root `desktop-product` check, the complete 1,151-test root library suite
plus all integration/binary/doc tests, all-target `desktop-product` Clippy with
warnings denied, and `scripts/release-check.sh quick`. The quick gate
additionally covered product/version assertions, isolated TUI lifecycle and
real-PTY signal restoration, focused OMENchat behavior, and standalone
omenchatd feature/config smokes. No live NomadNet/Python peer, concurrent live
page transfer, native platform, or performance measurement ran in this unit.

## Phase 5 unit 8: exact browser operation/resource correlation

Unit 7 deliberately left resource progress runtime-wide because the event
contract did not contain a browser operation identifier. Associating events by
destination or timing would be incorrect when two tabs fetch the same node or
when a cancelled operation emits a late terminal event.

This unit adds an optional opaque `operation_id` to project-owned resource
progress and lifecycle events. Each browser task creates a process-local ID,
passes it through `BrowserSession` and the default-compatible `NetworkRuntime`
operation methods, and attaches it to the native page fetch context. The
request-resource adapter preserves the ID on response progress and outbound
complete, failure, cancellation, and timeout cleanup. Other LXMF and OMENchat
resource producers explicitly use no browser operation ID.

The application retains at most one operation record per browser tab. Starting
a replacement removes the old record and clears its presentation state. An
event updates a tab only when its exact ID is still registered and its recorded
generation matches the tab session. A separately queued completion event
removes only that exact record, preventing a late completion from clearing a
newer operation. The desktop and TUI browser surfaces render the correlated
tab-local transfer status while the existing runtime-wide status and Network
Doctor evidence remain available.

There is no Reticulum, NomadNet, LXMF, or OMENchat wire change. There is no
configuration, identity, database schema, cache format, dependency, polling,
timeout, or state-root change. The runtime trait additions have default
implementations, preserving mock and auxiliary runtime implementations. Event
deserialization treats the new field as optional. Rollback is source-only and
would restore runtime-wide progress without affecting persisted data.

Validation passed for two-tab replacement/stale-event isolation, native
fetch-context propagation, cancellation/timeout lifecycle propagation, root
formatting and whitespace checks, root `desktop-product` check, the complete
1,153-test root library suite (1,151 passed and two ignored measurement
fixtures) plus all integration/binary/doc tests, all-target `desktop-product`
Clippy with warnings denied, and `scripts/release-check.sh quick`. The quick
gate additionally covered product/version assertions, isolated TUI lifecycle
and real-PTY signal restoration, focused OMENchat behavior, and standalone
omenchatd feature/config smokes. Live concurrent NomadNet/Python transfers,
native platforms, and performance measurements did not run in this unit.

## Phase 5 unit 9: NomadNet page-link ownership and local reuse evidence

Source inspection confirmed that Reticulum 0.9 `Transport::link()` reuses an
existing outbound link whenever that handle is not closed. OMENbrowser's page
transport intended one link owner per request and explicitly closed the link
after every result, but two concurrent requests for the same destination could
receive the same handle and independently tear it down. One task could
therefore close a link while the other still owned a request resource.

This unit adds a process-local coordinator with 32 fixed mutex stripes selected
from the destination hash. A page operation holds its stripe across link
preparation, request-resource transfer, cancellation/timeout cleanup, and link
teardown. Different stripes remain parallel. The existing cancellation token
now exposes a notification future, allowing a waiting task to exit immediately
without acquiring or retaining a guard; no polling timer or dependency was
added. The fixed array avoids an unbounded destination-lock map.

A deterministic Reticulum 0.9 harness activates an in-memory outbound link,
proves a second destination lookup returns the same `Arc`, observes the
`LinkClose` packet and closed shared state, and proves the next lookup creates a
distinct pending link. A coordinator regression proves same-stripe exclusion,
other-stripe progress, cancellation, and post-release acquisition. Existing
request cancellation, timeout, and pending-link cleanup regressions continue
to pass.

Production behavior deliberately remains one request-resource exchange per
link followed by close. The new evidence proves local upstream lifecycle
semantics and removes the concurrent teardown race; it does not prove that
longer-lived reuse interoperates with Python NomadNet or improves link count,
latency, CPU, or memory. Enabling keep-alive reuse remains gated on the pinned
Python repeated-request/close/reconnect matrix and measurements.

There is no Reticulum, NomadNet, LXMF, or OMENchat wire change. There is no
configuration, identity, database schema, cache format, dependency, timeout,
polling interval, or state-root change. Rollback is source-only and restores
the prior uncoordinated close-after-request behavior.

Validation passed for local active-link reuse/close/reconnect, cancellation-
aware stripe ownership, existing request-resource cancellation/timeout and
pending-link cleanup, root formatting and whitespace checks, root
`desktop-product` check, the complete 1,155-test root library suite (1,153
passed and two ignored measurement fixtures) plus all integration/binary/doc
tests, all-target `desktop-product` Clippy with warnings denied, and
`scripts/release-check.sh quick`. The quick gate additionally covered
product/version assertions, isolated TUI lifecycle and real-PTY signal
restoration, focused OMENchat behavior, and standalone omenchatd feature/config
smokes. No live NomadNet/Python peer, keep-alive production link reuse, native
platform, or performance measurement ran in this unit.

## Phase 6 unit 1: typed OMENchat client connection lifecycle

The desktop already owned bounded live transports, opening markers, reconnect
deadlines and counters, link/session mappings, and protocol join events. Its
connection state was nevertheless presented through free-form session status
and several ad hoc boolean checks, so callers could not reliably distinguish a
path request, link connection, protocol authentication, joined room, queued
reconnect, draining close, or retryable failure without interpreting text.

This unit introduces the project-owned `ChatConnectionState` model with
disconnected, resolving, connecting, authenticating, joined, reconnecting,
draining, and failed-with-retryability states. Existing desktop ownership
points update a per-session table: path request/result, existing-session open,
automatic/manual reconnect, transport registration, `RoomJoined`, heartbeat or
runtime close, retry exhaustion, and session removal. Transport registration
uses the authoritative room `joined` flag, and later join events advance the
state; no timer or status-string parser participates.

The state table can contain entries only for an existing `ChatClient` session,
which is already capped at 64, and session close removes its entry. Restored
sessions start disconnected because persisted UI history is not proof of a
live link. Workspace subtitles and live monitoring now show the typed state;
monitoring also reports its retryability. Existing detailed status text remains
available for human-readable reasons.

There is no OMENchat, Reticulum, or LXMF wire change and no destination,
identity, protocol version, configuration, database schema, cache format,
dependency, queue, timeout, polling, or state-root change. Reconnect delays,
attempt limits, generation rejection, heartbeat policy, and link ownership are
unchanged. Rollback is source-only and restores the prior boolean/string
presentation.

Validation passed for typed labels/retryability, resolving and authentication
failure, event-driven join, quick reconnect, manual disconnect, retry-limit
failure, ordinary joined-command error isolation, session-bound admission and
cleanup, default/mock compilation, root formatting and whitespace checks, root
`desktop-product` check, the complete 1,158-test root library suite (1,156
passed and two ignored measurement fixtures) plus all integration/binary/doc
tests, all-target `desktop-product` Clippy with warnings denied, and
`scripts/release-check.sh quick`. The quick gate additionally covered
product/version assertions, isolated TUI lifecycle and real-PTY signal
restoration, focused OMENchat behavior, and standalone omenchatd feature/config
smokes. Live Reticulum handshake, server restart, mixed-version peers, native
platforms, and performance measurements did not run in this unit.

## Phase 6 unit 2: OMENchat reconnect link ownership

Source inspection confirmed that Reticulum 0.9 `Transport::link()` returns the
same non-closed outbound link for a destination. Ordinary OMENchat traffic
already reused one registered link per active session, but overlapping desktop
reconnect generations could both receive that same handle. Rejecting and
closing a stale successful completion could consequently tear down the newer
generation as well.

This unit gives explicit reconnects one cancellation owner per existing chat
session. Launching a newer generation cancels its predecessor; current
completion, manual disconnect, reconnect-state cleanup, and session removal
release that owner. The map remains bounded by the existing 64-session client
catalog. A stale completion neither removes nor cancels the current owner.

The clean Reticulum adapter also serializes explicit OMENchat opens through 32
fixed destination stripes. Once path/identity discovery succeeds, the owner
retires tracked clean links for only that destination before calling
`Transport::link()`. It removes the retired ID from the active set before
teardown so deliberate replacement does not emit a false reconnect trigger.
The stripe is held through activation and registration, preventing two opens
from sharing one upstream handle. Waiters observe cancellation without polling,
and cancellation after link allocation closes and resets the pending upstream
handle before releasing the stripe. Unrelated stripes retain parallelism. No
unbounded lock registry or new dependency was added.

Deterministic regressions cover cancellation while waiting for a stripe,
matching-destination-only retirement and idempotent cleanup, newer-generation
cancellation, pending-link non-reuse, and stale-result isolation. Existing
session transport ownership continues to carry all frames and resources on the
registered link.

There is no OMENchat, Reticulum, or LXMF wire change and no destination,
identity, protocol version, configuration, database schema, cache format,
dependency, queue budget, timeout, retry delay, polling, or state-root change.
Rollback is source-only and restores the prior uncoordinated explicit-open
behavior.

Validation passed for all four ownership regressions plus pending-link non-reuse,
default and `desktop-product` checks, root formatting, the complete 1,162-test
root library suite (1,160 passed and two ignored measurement fixtures) plus all
integration/binary/doc tests, all-target `desktop-product` Clippy with warnings
denied, and `scripts/release-check.sh quick`. The quick gate additionally
covered product/version assertions, isolated TUI lifecycle and real-PTY signal
restoration, focused OMENchat behavior, and standalone omenchatd feature/config
smokes. Live Reticulum server restart, mixed 0.6/0.9 peers, pinned Python,
native platforms, and link-count/CPU/RSS measurements did not run in this unit.

## Phase 6 unit 3: OMENchat same-link mutation replay safety

The protocol audit confirmed that frame `seq` already correlates requests and
responses and that the browser retains its monotonic counter across an
in-process reconnect. omenchatd previously used `seq` only when forming a
response. Replaying the same room-message frame on a live link therefore
charged the rate limiter again, appended another SQLite event, and broadcast a
second room event.

This unit adds a live-server replay cache for `RoomMessage`, `RoomAction`, and
`RoomNotice`. The key is deliberately `(link_id, seq)` and the entry retains the
canonical request plus origin-only response. An exact replay skips the session
engine and returns the same response without database, rate, or fan-out side
effects. Different content under the same key returns `MalformedFrame`. The
original response is cached before transport delivery, prioritizing at-most-once
server mutation if response delivery fails.

Admission is capped at 1,024 entries/4 MiB globally, 64 entries/256 KiB per
link, and 64 KiB per entry using owned collection capacities rather than wire
length alone. Oldest per-link entries are evicted before global
entries, duplicate lookup does not refresh retention, and link close,
replacement, or administrative disconnect releases all associated entries.
Status exposes replay hits, collisions, rejected admissions, items, and bytes.
No unbounded payload cache or dependency was added.

The scope is intentionally same-link. Protocol v1 has no persisted
client-session nonce, and the browser sequence counter restarts with the
process. A durable `(identity, seq)` cache would therefore reject legitimate
new sessions as replays. Cross-link and post-restart mutation retries remain a
versioned capability-design task; the current client does not automatically
resend pending room mutations across reconnect.

There is no OMENchat frame, operation, context, resource, Reticulum, or LXMF
wire change and no identity, destination, protocol version, configuration,
database schema, dependency, rate limit, timeout, retry, polling, or state-root
change. Rollback is source-only and restores prior repeated execution.

Validation passed for exact replay, collision, durable single-event, single-
fan-out, rate-accounting, per-link item/byte eviction, oversized admission, and
link-close release. The standalone matrix passed formatting, `server-headless`
check/test/Clippy with warnings denied, and `server-full` check; 181 server tests
were discovered, with 178 passed and three explicit 60-second soak fixtures
ignored. The complete root `desktop-product` matrix also passed formatting,
check, 1,162 library tests (1,160 passed and two ignored measurement fixtures),
all integration/binary/doc tests, and all-target Clippy with warnings denied.
`scripts/release-check.sh quick` passed product/version assertions, isolated TUI
lifecycle and real-PTY restoration, focused OMENchat behavior, and standalone
omenchatd feature/config smokes. Live Reticulum replay, cross-link reconnect,
server restart, mixed 0.6/0.9, pinned Python, native platforms, and performance
measurements did not run in this unit.

## Phase 6 unit 4: OMENchat part and command replay ownership

The follow-on replay audit classified `rooms` as read-only and `topic`,
`create`, `kick`, `ban`, `mute`, `unmute`, `role`, and `unban` as mutating.
`PartRoom` and each mutating command can change SQLite state or append a durable
system event, so exact same-link retries now use the bounded unit-3 replay cache.
Read-only commands remain uncached. No new queue, cache, dependency, feature, or
configuration was introduced, and the existing item/byte/link bounds and
eviction policy are unchanged.

The audit also found that live-link part and kick/ban effects were inferred from
the incoming request before the session engine returned. Consequently, a
rate-limited kick could disconnect its target, and a rejected part could remove
the requester's live room. The live boundary now returns a narrow typed dispatch
outcome: room removal requires a successful `part` command result, while target
disconnect requires a successful `kick` or `ban` command result. Replay and
sequence-collision paths return no new side-effect authority. Existing join and
room-activity ownership behavior is otherwise unchanged.

There is no protocol, operation, frame, sequence, context, resource, Reticulum,
LXMF, identity, destination, database schema, state-root, or application-version
change. Exact retries still receive the retained origin response. Rollback is
source-only: restore the prior replay admission set and unconditional outer
part/moderation handling. Cross-link and post-restart retry idempotency remain
deferred until a versioned durable session identifier exists.

Validation passed for mutation classification, exact part, kick, topic, and
role replay, durable single-event/revision behavior, single user-list fan-out,
single target disconnect, replay bypass of an exhausted command-rate budget,
and the rate-limited-kick non-disconnect regression. The standalone matrix
passed formatting, headless/full checks, and warnings-denied headless Clippy;
186 tests were discovered, with 183 passed and three explicit 60-second soak
fixtures ignored. The root `desktop-product` matrix passed formatting, check, all-target
warnings-denied Clippy, 1,162 library tests (1,160 passed and two ignored
measurement fixtures), and all integration/binary/doc tests. The quick release
gate passed version/product assertions, isolated TUI/PTY lifecycle, focused
OMENchat behavior, and standalone server feature/config smokes. Live Reticulum,
cross-link/restart replay, mixed 0.6/0.9, pinned Python, native Windows/macOS,
and performance measurements did not run in this unit.

## Phase 6 unit 5: OMENchat history resource integrity binding

The Phase 6.4 audit found that compressed batch decoding already enforced 4 MiB
compressed/uncompressed ceilings, nesting/container/value limits, exact decoded
length, and trailing-data rejection. Desktop resource and pending-offer caches
were also item/byte bounded and released with their owning live transport.
However, a history or user-list resource offer was not semantically bound to
the eventual resource: advertised purpose, compression, compressed length, and
uncompressed length were accepted independently of the payload.

The shared client boundary now validates an offer before deferral. Resource IDs
must be non-empty and no larger than 4 KiB, lengths must fit existing 4 MiB
ceilings, and purpose must match `HistoryResourceOffer` or
`UserListSnapshotResource`. After resource arrival, the embedded compression,
uncompressed length, and compressed payload length must exactly equal the offer
before bounded decompression. The desktop transport's consuming fetch removes
the payload from its bounded cache even when validation fails. No polling,
background task, queue, cache, dependency, feature, configuration, or storage
root was added.

There is no frame, operation, field, resource metadata, protocol version,
Reticulum/LXMF, identity, destination, database, or persisted-state change.
Correct v0.6-compatible offers continue to decode unchanged; malformed or lying
offers now fail closed. Rollback is source-only by restoring the former payload-
only decoder. Live transfer progress/cancellation and link-loss behavior remain
for the next Phase 6.4 unit because this change deliberately covers one risk
class: offer-to-payload integrity.

Validation passed for valid immediate and delayed resource delivery, purpose
rejection before pending retention, exact offer-length boundaries, next-byte
length rejection, and compression/uncompressed/compressed mismatch failures.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy, and
the complete test matrix passed: 1,165 library tests were discovered, with
1,163 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The server source was
unchanged, so its full 186-test matrix was not repeated in this unit. Live
Reticulum resource progress/cancellation, link-loss recovery, mixed 0.6/0.9,
pinned/current Python, native Windows/macOS, and performance measurements did
not run.

## Phase 6 unit 6: OMENchat inbound resource terminal cleanup

The Phase 6.4 lifecycle trace confirmed that Reticulum 0.9 progress and terminal
events already reach global diagnostics and that link closure drops the owning
desktop transport. It also found one still-open-link gap: an inbound resource
failure or cancellation left deferred history/user-list offer frames retained,
so the session could remain indefinitely at `waiting for Resource` until the
link closed or its bounded offer cache saturated.

Failed/cancelled inbound OMENchat terminals now cross a dedicated 64-item,
256 KiB staging boundary. The desktop matches the terminal's peer link, clears
that transport's pending offers with exact byte-accounting release, reports a
retryable session status, persists it, and leaves the healthy link connected.
Progress remains on the existing runtime diagnostics path and is not duplicated
into this terminal queue. Monitoring reports terminal items, bytes, and rejected
events.

Cleanup is conservatively link-scoped. The Reticulum event exposes its resource
hash, while the protocol offer is keyed by a separate OMENchat resource ID; no
verified correlation exists at this boundary. Per-resource user cancellation
is therefore deferred rather than inventing a mapping or cancelling the wrong
transfer. There is no OMENchat/Reticulum/LXMF wire, frame, metadata, identity,
destination, configuration, database, dependency, feature, or state-schema
change. Rollback is source-only by removing terminal staging and restoring the
former wait-until-link-close behavior.

Focused validation passed for item-budget rejection/release, exact pending-offer
byte release, direction filtering, and failed inbound cleanup that preserves the
active link. Root formatting, `desktop-product` check, all-target
warnings-denied Clippy, and the complete test matrix passed: 1,167 library tests
were discovered, with 1,165 passed and two measurement fixtures ignored, plus
all integration, binary, and documentation tests. `scripts/release-check.sh
quick` passed product/version assertions, isolated TUI/PTY lifecycle, focused
OMENchat tests, and standalone omenchatd feature/configuration smokes. Live
Reticulum cancellation/progress, link loss during a physical transfer, mixed
0.6/0.9, pinned/current Python, native Windows/macOS, and performance
measurements remain pending.

## Phase 6 unit 7: OMENchat announce identity attribution

The Phase 6.5 discovery audit confirmed that `omenchat.node` announces were
classified, item-bounded, persisted with retention, exposed in Directory, and
refreshable through Request Path. The clean Reticulum listener also retained
the verified destination identity internally for link establishment. However,
the public announce DTO discarded that identity before Directory ingestion, so
the UI could show only the service destination hash and could not meet the
verified-server-identity display requirement.

Announce and directory-candidate DTOs now carry an optional public identity
hash. The clean Reticulum path derives it from the identity authenticated by the
announce event; the dormant legacy adapter maps its existing identity hash. The
Directory validates exactly 32 hexadecimal characters, persists the additive
optional field, preserves it across snapshot recovery, includes it in search,
and refuses a different identity for an existing destination. Selected
OMENchat servers show the full value as `announce-verified` or explicitly say a
fresh announce is needed. This label does not alter user-managed trust.

There is no OMENchat frame, descriptor, metadata prefix, destination namespace,
Reticulum/LXMF wire, dependency, feature, configuration, or database change.
Existing directory JSON remains readable because the new field defaults to
absent; newly written records contain the optional public identity hash without
changing a schema version. Rollback is source-only; older builds ignore the
unknown JSON member and may drop it on their next Directory save, without
affecting saved/trusted state or the destination record.

Focused validation covers runtime candidate retention, app event ingestion,
format and mutation rejection, persistence/reload, search, and selected-server
presentation. Root formatting, `desktop-product` check, all-target
warnings-denied Clippy, and the complete test matrix passed: 1,168 library tests
were discovered, with 1,166 passed and two measurement fixtures ignored, plus
all integration, binary, and documentation tests; the Directory integration
suite now passes 23 tests. `scripts/release-check.sh quick` passed
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The server source was
unchanged. Live Rust/Python announce verification, public-network discovery,
mixed 0.6/0.9 behavior, native Windows/macOS, and performance measurements
remain pending.

## Phase 6 unit 8: OMENchat reconnect action validity

The Phase 6.6 UX audit confirmed that disconnect reasons, retryable lifecycle
state, bounded backlog progress, queue/resource monitoring, non-blocking room
switching, and inactive-media animation gating were already present. It also
found that every OMENchat pane toolbar exposed `Reconnect` unconditionally.
Although the update path rejected or replaced conflicting work safely, the UI
offered a retry while connected, while opening/reconnecting, while draining,
and after a terminal failure.

`ChatConnectionState` now separates broad automatic `retryable` semantics from
the narrower `manual_reconnect_allowed` policy. Only disconnected and
retryable-failure states offer the manual toolbar action. All tiled, compact,
and maximized pane layouts consume that typed predicate, so a user cannot start
a second operation while resolution, connection, authentication, or reconnect
work is already in flight.

There is no Reticulum/LXMF/OMENchat wire, identity, destination, dependency,
feature, configuration, database, persisted-state, queue, or timer change.
Rollback is source-only by restoring the unconditional toolbar action and
removing the predicate. Focused validation covers every lifecycle state. Live
manual/automatic reconnect, server restart, mixed 0.6/0.9, pinned/current
Python, native Windows/macOS, and performance measurements remain pending.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy,
and the complete test matrix passed: 1,168 library tests were discovered, with
1,166 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed the
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The server source was
unchanged by this unit.

## Phase 6 unit 9: OMENchat outbound acceptance indicator

The Phase 6.6 outbound trace confirmed that a successfully enqueued room
message or action creates a temporary local echo and retains correlation state
until omenchatd returns `MessageAck`. A missing acknowledgement already kept
the local echo and enabled a delayed resend action, but the timeline rendered
the unaccepted event exactly like an accepted server event during that wait.

Timeline projection now carries an explicit `pending_acceptance` marker derived
from the existing local-echo identifier. Pending messages and actions display
`queued · awaiting server acceptance`; the marker disappears when the
correlated acknowledgement replaces the temporary identifier with the server
event ID. The existing resend timeout and operation correlation are unchanged.

There is no Reticulum/LXMF/OMENchat frame, acknowledgement, identity,
destination, dependency, feature, configuration, database, persisted-state,
queue, timeout, or timer change. Rollback is presentation-only by removing the
timeline marker and label. Focused tests cover pending versus accepted
projection, acknowledgement replacement, and missing-ack retention. Live
omenchatd acceptance latency, reconnect delivery, mixed 0.6/0.9,
pinned/current Python, native Windows/macOS, and performance measurements remain
pending.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy,
and the complete test matrix passed: 1,169 library tests were discovered, with
1,167 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed the
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The server source was
unchanged by this unit.

## Phase 6 unit 10: bounded OMENchat pending-message correlation

The Phase 6.6 acceptance trace found that unacknowledged room messages and
actions used a global `BTreeMap` of fixed-size sequence/session/room/event
correlation metadata. Acknowledgement, reconnect, and close cleanup existed,
but admission had no explicit ceiling, so an unresponsive server could grow
the map independently of the bounded visible event history.

The live client now admits at most 64 pending local echoes per session and 256
globally. Per-session admission prevents one stalled link from consuming the
whole client budget. Saturation increments a monotonic rejection counter,
returns a visible typed client error, and occurs before sequence reservation,
frame construction, or transport send. The desktop's existing error handling
therefore keeps the unsent composer draft. A valid `MessageAck` restores
capacity, while reconnect/session cleanup releases all entries owned by that
session. OMENchat monitoring reports pending and rejected message counts beside
the existing bounded upload/download metrics.

Only fixed-size correlation metadata is retained, so an item budget is the
relevant allocation bound; message bodies remain governed by the existing
bounded session history and protocol scalar limits. There is no
Reticulum/LXMF/OMENchat frame, acknowledgement, identity, destination,
dependency, feature, configuration, database, persisted-state, timeout, or
timer change. Rollback removes the admission checks, metrics, and monitoring
fields together. Focused tests cover per-session/global saturation before send,
acknowledgement release, cleanup release, and rejection accounting. Live slow
consumer/reconnect saturation, mixed 0.6/0.9, pinned/current Python, native
Windows/macOS, and memory measurements remain pending.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy,
and the complete test matrix passed: 1,171 library tests were discovered, with
1,169 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed the
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The standalone server
source was unchanged by this unit.

## Phase 6 unit 11: redacted copyable OMENchat session diagnostics

The Phase 6.6 UX audit found that connection state and bounded transport,
message, upload, and download metrics were visible in separate UI and
monitoring surfaces, but an OMENchat user could not copy one safe per-session
report for troubleshooting. Copying the existing free-form session status or
global diagnostic log would also risk including message text, local paths, or
other unrelated state.

Every tiled, compact, and maximized OMENchat pane now offers a redacted session
diagnostics action. It produces pretty JSON capped at 8 KiB containing typed
connection/retry state, public server destination and announce-verified
identity, room/event counts, bounded client queue/resource metrics, and safe
link/transport counters. It deliberately excludes message bodies, composer
drafts, user lists, room names, filenames, local paths, credentials, private
identity material, and free-form status/error text. The last disconnect is
mapped to a fixed category rather than copied verbatim. A missing or closed
session fails without writing stale clipboard content.

There is no Reticulum/LXMF/OMENchat wire, identity, destination, dependency,
feature, configuration, database, persisted-state, queue, timeout, timer, or
server change. The only newly exposed state is the already bounded per-session
pending-local-echo item count. Rollback is source/UI-only: remove the toolbar
actions, message route, formatter module, and public count accessor. Focused
tests cover the JSON size/schema, adversarial redaction, clipboard route, and
closed-session failure. Live Reticulum metrics, native platform clipboard
behavior, mixed 0.6/0.9, pinned/current Python, and performance measurements
remain pending.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy,
and the complete test matrix passed: 1,173 library tests were discovered, with
1,171 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed the
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The standalone server
source was unchanged by this unit.

## Phase 6 unit 12: link-scoped OMENchat resource progress

The Phase 6 completion audit confirmed typed lifecycle, reconnect ownership,
link reuse, replay safety, bounded history/resources, verified announce
identity, queue indication, room-switch behavior, and copyable diagnostics. It
found one remaining Phase 6.6 presentation gap: Reticulum 0.9 resource progress
was already captured in the bounded Network Doctor active-resource model, but
an OMENchat pane only reported that it was waiting for a Resource.

The active session pane now projects the newest inbound OMENchat progress row
only when its typed source, inbound direction, and peer link identity match the
session's current live transport. It displays received/total bytes and a
bounded percentage, plus the number of other matching active transfers. Rows
for another link, another source, or a terminal transfer are ignored. The UI
does not call the transfer a history backlog: Reticulum's public event exposes
the transfer hash while the OMENchat offer uses a separate resource ID, and no
verified correlation exists between them. The label therefore states that the
payload may contain history, users, or media.

No new queue, cache, timer, subscription, polling loop, dependency,
configuration, database, persisted state, Reticulum/LXMF/OMENchat wire field,
or server behavior is introduced. The unit reuses the existing bounded active
resource state and event-driven redraw path. Rollback is presentation-only by
removing the projection helper and pane status row. Focused tests cover exact
link attribution, newest-transfer selection, terminal/other-link exclusion,
multiple-transfer reporting, unknown totals, and overflow-safe percentage
formatting. Live transfer progress/cancellation, mixed 0.6/0.9,
pinned/current Python, native Windows/macOS, and performance measurements
remain pending.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy,
and the complete test matrix passed: 1,175 library tests were discovered, with
1,173 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed the
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests,
and standalone omenchatd feature/configuration smokes. The standalone server
source was unchanged by this unit.

## Phase 6 unit 13: OMENchat link-scoped sequence ownership

The Phase 6 correlation audit found that the live client's single global
`u32` sequence counter used saturating addition. After issuing `u32::MAX`, every
later operation reused that value. A naive wrap to `1` was not safe: omenchatd
retains up to 64 replay-guarded mutations per link, so an old same-link
sequence may remain cached and a different wrapped operation would correctly
be rejected as a replay collision.

Sequence ownership is now per live session/link. Session-open and initial join
reserve two IDs atomically, and all other operations reserve one nonzero ID.
An active link never wraps: exhaustion emits a bounded typed client error
before frame construction, transport send, pending-correlation insertion, or
local echo. The allocator resets only through the explicit link-retirement
path used by reconnect and session close. Generic transfer cancellation does
not reset it. Pending message and upload correlations are keyed by
`(session_id, sequence)`, allowing independent links to use the same numeric
sequence without cross-session acknowledgement or upload ownership.

There is no OMENchat frame/schema/version, Reticulum/LXMF wire, destination,
identity, database, configuration, dependency, queue budget, timer, polling,
or server behavior change. The existing `u32` field and omenchatd replay policy
remain intact. Rollback restores the global saturating counter and sequence-only
correlation keys, but would reintroduce permanent same-link collision after
exhaustion. Focused tests cover per-session allocation, atomic final-pair
reservation, fail-closed exhaustion, equal sequence values across independent
links, exact cleanup ownership, and reconnect-only reset. Four-billion-operation
soak, live reconnect/replay, mixed 0.6/0.9, pinned/current Python, native
Windows/macOS, and performance measurements remain pending.
Root formatting, `desktop-product` check, all-target warnings-denied Clippy,
and the complete test matrix passed: 1,178 library tests were discovered, with
1,176 passed and two measurement fixtures ignored, plus all integration,
binary, and documentation tests. `scripts/release-check.sh quick` passed with
exit code 0, including product/version assertions, isolated TUI/PTY lifecycle,
focused OMENchat tests, and standalone omenchatd feature/configuration smokes.
The standalone server source was unchanged by this unit.

## Phase 6 unit 14: jittered reconnect backoff and completion audit

The final Phase 6 lifecycle audit found one deterministic implementation gap.
Reconnect task ownership, cancellation, generation rejection, and the
five-attempt ceiling were already bounded, but a failed open waited a fixed 15
seconds, heartbeat loss waited a fixed two seconds, and successful link
registration immediately erased the retry count. Repeated short-lived links
could therefore restart the fastest recovery path rather than carrying forward
one backoff budget.

All automatic retry sources now share a project-owned scheduler. Attempts one
through five use 1, 2, 4, 8, and 16 second base delays with deterministic
per-session +/-20% jitter and a 30-second hard cap. A sixth scheduling request
pauses automatic recovery in the existing retryable-failure state. No random or
timer crate was added. The existing nearest-deadline subscription owns both
retry and stability wakes, so there is no fixed polling loop. A successful
replacement link clears pending open work but retains its attempt count until
the link remains registered for 30 seconds. A failure before that deadline
continues the prior budget; explicit user reconnect intentionally starts a new
budget. Session close, terminal disconnect, and stable completion release all
new metadata, which remains bounded by the existing 64-session catalog.

The deterministic Phase 6 audit now classifies the client work as follows:

| Area | Deterministic implementation | Evidence still required |
| --- | --- | --- |
| 6.1 connection lifecycle | Complete: typed states, one reconnect generation, jittered exponential retry, stable reset, bounded pause | live restart/reconnect soak and resource measurements |
| 6.2 link/backchannel reuse | Complete at project ownership boundaries | live link-count comparison and stale-link peer exercise |
| 6.3 correlation/idempotency | Complete for protocol-v1 same-link replay and per-link sequence ownership | cross-link/restart idempotency requires a separately negotiated versioned session identifier |
| 6.4 history/resources | Complete for bounds, integrity, delayed offers, cleanup, and presentation | live progress/cancel/link-loss transfer evidence |
| 6.5 discovery/identity | Complete for authenticated attribution, persistence, filtering, and mutation rejection | live announce signature/path/server-impersonation evidence |
| 6.6 justified UX | Complete for typed reasons/actions, queue acceptance, redacted diagnostics, progress, and non-blocking room state | native clipboard and performance evidence |
| 6.7 compatibility | No protocol namespace, frame, destination, identity, database, or history migration introduced | mixed 0.6/0.9 client/server matrix remains release evidence |

This closes deterministic Phase 6 implementation inspection, not the Phase 6
release evidence gate. Cross-link durable mutation idempotency is explicitly
deferred because protocol v1 has no persisted client-session nonce; adding one
silently would violate the preserved-wire requirement. Live Reticulum,
mixed-version, pinned/current Python, native Windows/macOS, server restart, and
performance measurements remain pending.

There is no OMENchat/Reticulum/LXMF wire, destination, identity, database,
configuration, dependency, queue budget, state-root, or standalone omenchatd
change. Timing policy changes only for automatic client reconnect. Rollback is
source-only: restore fixed retry deadlines, immediate counter reset, and remove
the stability map/deadline. That rollback would reintroduce synchronized retry
bursts and rapid short-lived-link cycles. Focused tests cover deterministic
jitter, monotonic/capped delays, all five attempts, pause after exhaustion,
stable-only reset, active-link preservation, timeout closure, generation
limits, and stale pending-work cleanup.
Root formatting and whitespace checks, `desktop-product` check, all-target
warnings-denied Clippy, and the complete test matrix passed: 1,181 library
tests were discovered, with 1,179 passed and two measurement fixtures ignored,
plus all integration, binary, and documentation tests.
`scripts/release-check.sh quick` passed with exit code 0, including product and
version assertions, isolated TUI/PTY lifecycle, focused OMENchat tests, and
standalone omenchatd feature/configuration smokes. The standalone server source
was unchanged by this unit.

## Phase 7 unit 1: redacted omenchatd machine status

The Phase 7 runtime audit confirmed that the standalone package already owns
its 0.9 Reticulum runtime, pre-readiness signal handlers, bounded queues,
blocking SQLite gate, active-link closure, cancellation, worker joins, log
flush, invalid-interface startup failure, and non-success shutdown errors.
Those invariants were implemented and tested during Phase 2.10 and remain
unchanged. The smallest remaining Phase 7.6 operations gap was that status and
doctor output existed only as human text containing local paths, so a service
monitor had no stable, redacted machine contract.

`omenchatd status --json` and `omenchatd doctor --json` now emit schema-version
1 JSON. Status identifies omenchatd `0.9.0-1`, the exact Reticulum 0.9.0 train,
independent in-process runtime ownership, fixed service names, public address
lines, file-presence booleans, interface readiness, room-catalog state, and
numeric limits. It explicitly reports that live metrics are unavailable from
this offline command rather than fabricating queue/link/resource health.
Doctor reports its aggregate outcome and typed check names/levels. Both omit
private paths, credentials, private identity material, operator/MOTD content,
and free-form error/check details. Existing human output is byte-behavior
compatible and remains the default.

There is no Reticulum/LXMF/OMENchat wire, destination derivation, identity,
database, configuration, dependency, feature, queue, timeout, runtime task,
state-root, or desktop change. The existing `serde_json` server dependency is
reused. Rollback removes the two CLI variants and renderers while preserving
human status/doctor behavior. Focused tests cover option ordering, schema and
version fields, runtime-mode projection in headless and transport-disabled
builds, typed checks, valid JSON, and adversarial redaction. An isolated CLI
smoke produced 1,530-byte status and 869-byte doctor documents, validated their
schemas, and found no isolated-root disclosure. Live-process RPC status,
external/shared runtime mode, systemd integration, native Windows service
monitoring, mixed-version peers, and soak evidence remain pending.
Standalone formatting and whitespace checks passed. The no-feature server
matrix discovered 164 tests, with 163 passed and one explicit logging soak
ignored. `server-headless` discovered 188 tests, with 185 passed and three
explicit 60-second measurement soaks ignored; all-target warnings-denied
Clippy passed. Both no-feature and `server-headless` checks/tests passed, and
`server-full` checked successfully from the independent manifest and lockfile.
`scripts/release-check.sh quick` passed with exit code 0, including product and
version assertions, isolated TUI/PTY lifecycle, focused OMENchat behavior, and
standalone omenchatd feature/configuration smokes.

## Phase 7 unit 2: duplicate peer-link physical retirement

The Phase 7.4 server-link audit found no standalone omenchatd federation or
server-to-server protocol to migrate. The applicable existing boundary is
client peer-link ownership: both identified link-open and session-refresh paths
removed a superseded same-identity link from server maps, but neither asked the
Reticulum transport to close the physical link. One path also left its saved
response context behind. The old connection could therefore remain live after
omenchatd stopped accounting for it.

Duplicate replacement now has one retirement path. Before accepting the newer
link as sole owner, omenchatd records the replacement reason, requests exactly
one transport close, counts the retired link, and removes its peer, room,
response-context, replay-cache, open-time, and traffic state. A transport close
admission failure is surfaced through the existing bounded error statistics;
it does not discard the newly authenticated link or create a retry task. The
existing joined-room-only fan-out and same-link replay contract are unchanged.

There is no OMENchat/Reticulum/LXMF wire, destination, identity derivation,
database/schema, configuration, dependency, feature, queue budget, timeout,
runtime mode, state-root, or desktop change. No server-federation or cache-repair
protocol was invented. Rollback restores the two map-only cleanup loops, but
would again leave a superseded transport link open and one response-context
entry retained. Focused captured-transport tests cover physical close request,
single active identity ownership, closure accounting/summary, and complete
per-link state release. Live close delivery, reconnect-storm fairness,
server-restart, mixed 0.6/0.9, pinned/current Python, server federation, native
Windows/macOS, and CPU/RSS/task/link soak measurements remain pending.
Standalone formatting and whitespace checks passed. The no-feature server
matrix discovered 165 tests, with 164 passed and one explicit logging soak
ignored. `server-headless` discovered 189 tests, with 186 passed and three
explicit 60-second measurement soaks ignored. No-feature and `server-headless`
checks passed, `server-full` checked successfully, and `server-headless`
all-target warnings-denied Clippy passed from the independent manifest and
lockfile. `scripts/release-check.sh quick` passed with exit code 0, including
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat
behavior, and standalone omenchatd feature/configuration smokes.

## Phase 7 unit 3: bounded pending Resource ownership

The Phase 7.5 abuse-control audit confirmed that transport and event queues are
item/byte bounded, but `SessionEngine` retained generated history, user-list,
and upload-fetch Resource payloads in an unbounded map. Transport delivery
cloned those buffers without a production release owner. Repeated distinct
resource requests or failed sends could therefore retain payload memory for
the process lifetime even though the downstream transport queue was bounded.

The pending Resource store now admits at most 64 items, 16 MiB total, and 4 MiB
per entry. It accounts replacements exactly, restores capacity on removal, and
rejects overload before publishing a new response rather than evicting an
already promised payload. Logical response ownership preserves existing room
fan-out: a generated payload remains available to every intended joined link,
then is removed after the complete fan-out. All payloads generated by a batch
are also removed when frame or Resource transport admission fails. Live stats
now expose pending items, retained bytes, and cumulative rejected admissions.

There is no OMENchat/Reticulum/LXMF wire, destination, identity, database,
configuration, dependency, feature, transport queue, timeout, state-root, or
desktop change. The fixed limits are server safety boundaries rather than new
operator tuning knobs. Rollback restores the raw map and clone-only delivery,
but would reintroduce unbounded process-lifetime payload retention. Focused
tests cover item/entry/global byte saturation, exact boundaries, replacement
and release accounting, successful ownership cleanup, injected send failure,
two-recipient resource fan-out, and live metric projection. Live resource
cancellation/link loss, slow-recipient soak, pending upload-offer metadata,
unauthenticated link lifetime, concurrent link admission, mixed 0.6/0.9,
pinned/current Python, native Windows/macOS, and CPU/RSS/task measurements
remain pending.
Standalone formatting and whitespace checks passed. The no-feature server
matrix discovered 169 tests, with 168 passed and one explicit logging soak
ignored. `server-headless` discovered 193 tests, with 190 passed and three
explicit 60-second measurement soaks ignored. No-feature and `server-headless`
checks passed, `server-full` checked successfully, and `server-headless`
all-target warnings-denied Clippy passed from the independent manifest and
lockfile. `scripts/release-check.sh quick` passed with exit code 0, including
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat
behavior, and standalone omenchatd feature/configuration smokes.

## Phase 7 unit 4: bounded pending upload-offer ownership

The next Phase 7.5 audit found that accepted inbound upload offers accumulated
in an unbounded metadata map until a complete Resource arrived. Abandoned
offers survived link closure indefinitely. The Resource handler also removed a
reservation before checking its identity, allowing a guessed resource ID from
the wrong peer to invalidate the true owner's pending upload.

Pending upload admission is now capped at 256 offers globally and eight per
identity, with 255-byte filename and content-type limits. Same-owner replay of
the exact deterministic resource ID replaces rather than double-counts its
reservation; another identity cannot replace it. Admission overload returns a
typed `UploadReject` and preserves existing reservations. Offers expire after a
conservative six hours and are purged during admission, lookup, and monitoring.
Link close, administrative disconnect, and duplicate-link retirement release
all offers for that identity. Identity is checked while the store remains
locked, and mismatched lookup no longer consumes the owner's entry. Live stats
expose pending items, represented identities, admission rejects, and expiries.

There is no OMENchat/Reticulum/LXMF wire field, destination, identity
derivation, database/schema, configuration, dependency, feature, payload queue,
upload quota, state-root, or desktop change. The limits are fixed server safety
ceilings. Rollback restores the raw map and remove-before-check lookup, but
would reintroduce unbounded abandoned metadata and cross-identity reservation
invalidation. Focused tests cover exact global/per-identity saturation,
same-owner replacement, cross-identity fairness, expiry, metadata validation,
typed overload rejection, identity-mismatch preservation, successful owner
completion, link-close cleanup, and metric projection. Live low-bandwidth
expiry behavior, Resource cancellation/resumption, unauthenticated link
lifetime, total link admission, mixed 0.6/0.9, pinned/current Python, native
Windows/macOS, and CPU/RSS/task measurements remain pending.
Standalone formatting and whitespace checks passed. The no-feature server
matrix discovered 173 tests, with 172 passed and one explicit logging soak
ignored. `server-headless` discovered 197 tests, with 194 passed and three
explicit 60-second measurement soaks ignored. No-feature and `server-headless`
checks passed, `server-full` checked successfully, and `server-headless`
all-target warnings-denied Clippy passed from the independent manifest and
lockfile. `scripts/release-check.sh quick` passed with exit code 0, including
product/version assertions, isolated TUI/PTY lifecycle, focused OMENchat
behavior, and standalone omenchatd feature/configuration smokes.

## Phase 7 unit 5: bounded live-link admission and handshake lifetime

The next Phase 7.5 audit confirmed that activated Reticulum links entered the
live peer map without a total or incomplete-handshake ceiling and could remain
there indefinitely without completing OMENchat session negotiation. The 0.9
transport supplied typed `PeerIdentified` events, but omenchatd discarded them,
so the server could not distinguish transport-authenticated peers from
provisional link identifiers.

Live admission is now capped at 256 links and 32 incomplete handshakes. A link
becomes complete only after both the typed Reticulum peer-identification event
and a valid `SessionOpen`, in either order. The event bridge projects the public
peer identity hash into the existing server-owned peer model. A dedicated
one-second deadline sweep physically closes incomplete links at the exact
30-second boundary and releases room, response-context, replay-cache, timing,
traffic, authentication/session, and pending upload-offer ownership. Admission
and timeout paths expose pending, rejected, and expired counts in the existing
live status line. Transport close failure remains visible through the existing
protocol-error and last-error counters.

There is no OMENchat frame, operation number, destination/aspect, identity
derivation, database/schema, configuration, dependency, feature, state-root,
or desktop change. Reticulum link identification was already part of the 0.9
wire lifecycle; this unit stops discarding its typed local event. The fixed
limits are conservative server safety boundaries. Rollback removes the two
ownership sets, admission checks, deadline sweep, and `PeerIdentified`
projection, but would restore unbounded incomplete-link retention and link-ID
stand-ins for authenticated identities.

Focused tests cover exact 32-link pending saturation, physical overload close,
slot recovery after full handshake, both authentication/session prerequisites,
the exact 30-second expiry boundary, completed-link survival, exact 256-link
total saturation, cleanup, and live counter projection. Live slow-handshake and
reconnect-storm qualification, link-level fairness by source/path, mixed
0.6/0.9, pinned/current Python, native Windows/macOS, and CPU/RSS/task/handle
measurements remain pending. Validation results are recorded after the complete
standalone and release matrices run. Standalone formatting and whitespace
checks passed. The no-feature server matrix discovered 176 tests, with 175
passed and one explicit logging soak ignored. `server-headless` discovered 200
tests, with 197 passed and three explicit 60-second measurement soaks ignored.
No-feature and `server-headless` checks passed, `server-full` checked
successfully, and `server-headless` all-target warnings-denied Clippy passed
from the independent manifest and lockfile. `scripts/release-check.sh quick`
passed with exit code 0, including product/version assertions, isolated TUI/PTY
lifecycle, focused OMENchat behavior, and standalone omenchatd
feature/configuration smokes.

## Phase 7 unit 6: live-link reconnect and slow-handshake measurement

Unit 5 established deterministic link bounds but left resource stability and
cleanup behavior under repeated saturation unmeasured. This unit adds an
explicitly ignored, optimized Linux qualification around the production
`OmenchatLiveServer` ownership/admission implementation. It retains 224 fully
identified and session-negotiated peers, repeatedly fills all 32 remaining
handshake slots, proves one excess link is physically rejected, expires the 32
slow peers at the exact deadline, replaces an authenticated link by identity,
and drains all retained peers at completion.

The harness records peak/final active and pending ownership, rejection and
expiry accounting, physical close calls, maximum synchronous close-path
latency, RSS, file descriptors, and process tasks. Release assertions require
the exact 256/32 peaks, at least ten cycles per second, close latency no greater
than 250 ms, RSS growth no greater than 64 MiB, FD growth no greater than four,
task growth no greater than two, exact close accounting, and zero final links.
`scripts/measure-omenchatd-links.sh` validates the machine-readable summary and
stores the raw log, normalized summary, environment, and toolchain metadata.

There is no runtime dependency, OMENchat/Reticulum/LXMF wire, database/schema,
configuration, identity, state-root, feature, default-test-duration, or
production timer change. The soak uses in-memory SQLite and a discard/count
transport so it cannot touch maintainer state. Rollback removes the ignored
test, measurement wrapper, and documentation only; production link behavior
from unit 5 remains unchanged. The 2026-07-16 optimized 60-second qualification
completed 4,587 saturation/recovery cycles, reached exactly 256 active/32
pending links, rejected 4,587 excess links, expired 146,784 slow links, issued
156,182 exactly accounted physical closes, and drained to zero. Maximum
close-path latency was 691 us against the 250,000 us gate; RSS grew 176,128
bytes against the 64 MiB gate; file descriptors remained at four and tasks at
two. Raw local evidence and normalized metadata were written outside the
repository at `/tmp/omenchatd-link-60s`. Live Reticulum wire behavior,
mixed 0.6/0.9, pinned/current Python, and native Windows/macOS remain separate
gates. Standalone formatting passed. The no-feature server matrix discovered
177 tests, with 175 passed and the logging/link measurement soaks ignored.
`server-headless` discovered 201 tests, with 197 passed and four explicit
60-second measurement soaks ignored. `server-full` checked successfully, and
`server-headless` all-target warnings-denied Clippy passed from the independent
manifest and lockfile. The measurement wrapper passed shell syntax and its own
machine-readable release-mode assertions. `scripts/release-check.sh quick`
passed with exit code 0, including product/version assertions, isolated TUI/PTY
lifecycle, focused OMENchat behavior, and standalone omenchatd
feature/configuration smokes.

## Phase 7 unit 7: Reticulum Resource terminal ownership

The Phase 7.5 audit confirmed that generated outbound Resource payloads leave
the project store after bounded transport admission and that link closure
already clears identity-owned upload offers. The remaining gap was the
Reticulum 0.9 Resource event bridge: it forwarded only completed inbound
payloads and silently discarded inbound failure plus outbound completion,
failure, and cancellation terminals. That prevented prompt upload-reservation
cleanup while a link remained open and hid terminal outcomes from operations.

The bridge now sends typed Resource terminals through the reserved bounded
control lane. The live server counts outbound complete/failed/cancelled states
even after link cleanup. Inbound failure increments its own counter and releases
all pending upload offers owned by the active identified peer without closing
the link. Upstream 0.9 failure events expose a link, transfer hash, progress,
and reason but omit completed Resource metadata, so exact upload-offer
correlation is not available; peer-scoped cleanup is the conservative
fail-closed policy. Failure reasons are stripped of control characters and
limited to 128 characters before logging. Status reports all terminal counts
and the number of released offer reservations.

There is no OMENchat/Reticulum/LXMF wire, operation, destination, database,
schema, configuration, dependency, feature, identity, quota, state-root, or
desktop change. Successful Resource delivery remains byte-compatible. Rollback
removes the terminal event projection, counters, and failure cleanup, but would
restore silent terminal loss and retain failed upload offers until link close
or six-hour expiry. Focused tests cover peer-scoped inbound-failure cleanup,
healthy-link preservation, late terminal accounting after link cleanup,
outbound completion/failure/cancellation counters, status projection, existing
link-close cleanup, and successful upload completion. Live resumable-transfer
and cancellation timing, mixed 0.6/0.9, pinned/current Python, and native
Windows/macOS remain pending.

Validation completed on 2026-07-16. `cargo fmt --check` passed in the
standalone server root. The no-feature server suite passed 177 tests with two
explicit 60-second soak tests ignored; the `server-headless` suite passed 199
tests with four explicit soak/live-environment tests ignored. The
`server-full` check and `server-headless --all-targets` Clippy gate with
`-D warnings` passed. `bash scripts/release-check.sh quick` also passed with
exit code 0, covering release-version/product-feature assertions, isolated
desktop TUI and real-PTY lifecycle smokes, focused OMENchat behavior, and
standalone omenchatd feature/configuration smokes. No physical Reticulum peer,
mixed-version peer, Python peer, Windows runner, or macOS runner was available
for this unit, so those evidence lanes remain explicitly pending.

## Phase 7 unit 8: production Resource-event bridge regression

The next evidence unit extracts the production Resource-event receive loop
behind a narrow helper that still owns the live `Transport` required for
completed NomadNet response Resources. Production startup continues to pass
the receiver obtained from `Transport::resource_events()`; runtime behavior is
unchanged. A deterministic isolated test now constructs the public Reticulum
0.9 `ResourceEvent` terminal variants and drives that exact receiver loop,
reserved bounded control queue, and typed OMENchat projection. It proves
inbound failure and outbound completion/failure/cancellation retain order and
direction, queue permits return to zero, no payload bytes are charged to the
control lane, and cancellation joins the owned bridge within one second.

This closes the untested upstream-crate-event to project-event callback
boundary without copying private upstream test hooks, binding a network port,
or leaving interface tasks detached. It does not prove that an actual peer
emits an initiator-cancel packet, nor live transfer timing or resume behavior.
Those require the local multi-process/pinned-Python interoperability lane. No
wire, operation, destination, database/schema, configuration, dependency,
feature, identity, quota, state-root, UI, or production queue-limit change is
introduced. Rollback inlines `transport.resource_events()` into the original
bridge and removes the regression; production semantics remain identical.

Validation completed on 2026-07-16. The focused production-bridge regression
passed. Standalone formatting passed; the no-feature suite passed 177 tests
with two explicit measurement soaks ignored, and `server-headless` passed 200
tests with four explicit measurement/live-environment tests ignored.
`server-full` checked successfully and `server-headless --all-targets` Clippy
passed with `-D warnings`. `bash scripts/release-check.sh quick` passed with
exit code 0, including release version/product feature assertions, isolated
desktop TUI and real-PTY lifecycle smokes, focused OMENchat behavior, and
standalone omenchatd feature/configuration smokes. Physical Resource transfer,
initiator cancellation, mixed 0.6/0.9, pinned/current Python, and native
Windows/macOS were not run and remain explicit interoperability gates.

## Phase 7 unit 9: loopback Resource initiator cancellation

An explicit ignored Linux/local-host harness now creates two independent
Reticulum 0.9 transports with ephemeral identities and dynamically reserved
point-to-point UDP ports. It establishes an announce-derived path and active
link through public APIs, sends a bounded 4 KiB Resource advertisement, and
cancels the active sender Resource. The receiving transport's interface stream
must observe both the Link-destination Resource advertisement and
`ResourceInitiatorCancel` wire contexts. The sender's production Resource-event
bridge must independently project `OutboundCancelled` through its bounded
control lane. Both link ends must remain active, queue ownership must drain to
zero, both interfaces must detach, the bridge must join within one second, and
the isolated roots must be removed.

This supplies Rust-to-Rust physical loopback evidence for the initiator-cancel
packet and project terminal boundary without private upstream hooks or user
state. It is excluded from the fast default matrix because it binds sockets.
The test deliberately uses plain point-to-point UDP; upstream multicast host
interfaces reject direct traffic unless virtual peer routing has been
established. No wire, protocol, destination, database/schema, configuration,
dependency, feature, identity, quota, state-root, UI, or production limit is
changed. Rollback removes only the ignored harness and documentation.

A fresh Resource completion after cancellation was attempted but did not
produce completion events in the same single-process UDP topology, so that
claim is not made and no test was weakened to manufacture it. Post-cancel
completion/resume, multi-process Rust peers, mixed 0.6/0.9, pinned/current
Python, and native Windows/macOS remain explicit gates.

Validation completed on 2026-07-16. The explicit loopback cancellation harness
passed in 0.47 seconds on Linux. Standalone formatting passed; the no-feature
suite passed 177 tests with two explicit measurement soaks ignored, while
`server-headless` passed 200 tests with five explicit socket/measurement/live
tests ignored. `server-full` checked successfully and
`server-headless --all-targets` Clippy passed with `-D warnings` after the
test helper was simplified rather than lint-suppressed. The focused bridge
regression was rerun after that cleanup. `bash scripts/release-check.sh quick`
passed with exit code 0, including release version/product feature assertions,
isolated desktop TUI and real-PTY lifecycle smokes, focused OMENchat behavior,
and standalone omenchatd feature/configuration smokes.

## Phase 7 unit 10: two-process Resource completion blocker

A new explicit ignored gate re-executes the server test binary in independent
receiver and sender roles with bounded process lifetimes, ephemeral
deterministic test identities, dynamically reserved point-to-point UDP ports,
and an isolated coordination/evidence root. Its required sequence is a 4 KiB
baseline Resource completion, a 16 KiB active Resource cancellation, and a
fresh 4 KiB completion over the still-active reused link. The child roles emit
redacted stage traces and the parent reaps and reports both processes.

The 2026-07-16 run fails before cancellation. The receiver observes the
baseline advertisement and sends ten Resource requests before emitting
`InboundFailed(retry_limit_exhausted)`. The sender physically receives all ten
requests, decrypts each with the active link, decodes each public
`ResourceRequest`, and confirms every request hash equals the hash returned by
`Transport::send_resource`. It emits no Resource parts and no outbound
terminal; the sender gate times out waiting for `OutboundComplete`. Thus path
discovery, announce, link activation, UDP delivery, request encryption, request
decoding, and Resource-hash correlation are proven, while the Reticulum 0.9
sender request-to-part dispatch remains unresolved in a true two-process
topology.

This is a release-blocking live Resource gap for OMENchat history/uploads and
NomadNet response Resources. The earlier cancellation-only loopback test
remains valid evidence for advertisement/cancel wire contexts and project
terminal projection, but it cannot substitute for successful transfer. The
new gate is deliberately ignored in fast suites because it binds sockets and
is presently expected to fail when run explicitly. No production code,
dependency, patch override, wire protocol, configuration, state, quota, or
limit was changed. Rollback removes only the diagnostic gate and documentation,
but doing so would discard the reproducible blocker rather than solve it.

Validation completed on 2026-07-16. The explicit two-process gate failed with
exit code 101 after reproducing the request-to-part stall; the final local
redacted traces remain outside the repository under
`/tmp/omenchatd-resource-multiprocess-4023653-1784247020071`. Standalone
formatting passed. The no-feature suite passed 177 tests with two explicit
measurement soaks ignored; `server-headless` passed 200 tests with six explicit
socket/measurement/live gates ignored, including this known-red gate.
`server-full` checked successfully and `server-headless --all-targets` Clippy
passed with `-D warnings`. `bash scripts/release-check.sh quick` passed with
exit code 0, covering release version/product feature assertions, isolated
desktop TUI and real-PTY lifecycle smokes, focused OMENchat behavior, and
standalone omenchatd feature/configuration smokes. The green default matrix
does not override the explicit Resource interoperability failure.

## Phase 7 unit 11: Reticulum UDP Resource serialization root cause

Test-only logging at the already locked `log` 0.4.29 version exposes the
published transport crate's existing redacted Resource diagnostics without
adding a runtime dependency. The two-process gate now proves that the sender
receives and decrypts the request, finds the matching outbound sender, and
builds all four requested Resource parts. Dispatch reaches the UDP worker, but
no part crosses the socket.

The root cause is deterministic in
`reticulum-rs-transport-0.9.0/src/iface/udp.rs`: the worker allocates its RX and
TX buffers as `size_of::<Packet>() * 3`. On this 64-bit target that is 456
bytes. A maximum type-one wire packet is 483 bytes (`2` header bytes + `16`
destination bytes + `1` context byte + `PACKET_MDU` `464`). Since `Packet`
stores payload in a heap-backed `Vec`, its Rust layout size is not a wire-size
bound. `Packet::serialize` therefore returns `OutOfMemory` for full Resource
parts and the UDP worker's `if ... is_ok()` branch silently discards the error.

The ignored
`reticulum_udp_tx_buffer_covers_max_resource_wire_packet` regression records
this capacity invariant directly and fails `456 >= 483`. Inspection of the
official upstream repository at v0.9.0, v0.9.1, and current `main` on
2026-07-16 found the same buffer expression and ignored serialization error.
No corrected published source is currently available for an exact dependency
upgrade.

No production dependency, protocol limit, Resource bound, interface setting,
or fragmentation behavior was changed. `log = "=0.4.29"` is test-only and
already existed transitively in the locked graph. An OMEN-local protocol or
MTU workaround would weaken interoperability evidence, so the release gate
remains red pending an upstream serialized-size-derived buffer fix plus error
reporting. Rollback removes the logger, invariant test, and documentation, but
would only hide the proven defect.

Validation completed on 2026-07-16. The focused capacity regression failed as
designed with exit code 101 and the exact message `upstream UDP tx buffer (456)
cannot serialize a maximum Resource wire packet (483)`. The full two-process
gate also failed with exit code 101 after upstream diagnostics reported
`sender_present=true`, `built=4`, and `responses=4` for every retry while the
receiver saw no Resource part. The standalone no-feature suite passed 177
tests with two ignored; `server-headless` passed 200 tests with seven explicit
gates ignored. `server-full` check and `server-headless --all-targets` Clippy
with `-D warnings` passed. Final root/server formatting and `git diff --check`
passed, and `bash scripts/release-check.sh quick` completed successfully.

## Phase 7 unit 12: isolated upstream UDP fix validation

The published 0.9.0 transport crate and standalone server source were copied
to an isolated temporary root without build caches or user data. A candidate
upstream-only correction replaced the UDP worker's layout-derived 456-byte
buffers with the interface's existing 2,048-byte MTU constant and routed
serialization failures into the existing warning, `tx_errors`, and
`last_error` surfaces. The candidate added no dependency and changed no wire
or Resource limit.

With a temporary Cargo source override, the unmodified OMEN two-process harness
passed its entire sequence in 0.57 seconds: the first 4 KiB Resource completed,
the 16 KiB transfer cancelled, and a final 4 KiB Resource completed over the
same active link. The candidate upstream library suite passed all 509 tests.
This proves the buffer correction is sufficient for the observed failure; it
does not authorize a production source override or replace pinned-Python
interoperability.

The maintainer-ready reproduction and adoption criteria are recorded in
`RETICULUM_RS_0_9_UPSTREAM_UDP_RESOURCE_REPORT.md`; the minimal proposed diff
is `reticulum-rs-0.9-udp-resource-buffer.patch`. OMEN remains pinned to the
unmodified registry 0.9.0 source. Rollback removes only these proposal
artifacts and this ledger entry. The live release gate remains red until an
approved immutable upstream source is adopted and the full required matrix is
rerun.

Final repository validation continued to resolve `reticulum-rs-transport
v0.9.0` from the registry with no source override. `server-headless` passed 200
tests with seven explicit gates ignored, `server-full` checked, Clippy passed
with `-D warnings`, root/server formatting passed, `git diff --check` passed,
and `bash scripts/release-check.sh quick` completed successfully. The stored
patch applied without fuzz to a fresh published-crate source copy and produced
the byte-identical unformatted candidate used for validation.

The isolated-copy procedure also reconfirmed a pre-existing standalone-package
ownership gap, addressed separately in Phase 7 unit 13 below.

## Phase 7 unit 13: relocatable omenchatd source boundary

The project-local IFAC TCP implementation is now a private protocol-neutral
crate at `src/server/crates/omen-ifac-tcp`. The standalone server owns that
crate, while the desktop consumes the same source through an optional path
dependency. This replaces omenchatd's repository-relative import of a desktop
runtime source file without duplicating behavior or adding any third-party or
transitive package. The server workspace and independent lockfile include the
crate; both production features activate it only with native Reticulum.

The scan also found two test-only OMENchat compatibility-fixture includes that
escaped the server tree. A byte-identical copy now lives under the server's own
fixture directory, and the root release check compares both public fixtures to
prevent drift. No application protocol/version, destination, identity,
configuration, database/schema, runtime state, queue/quota, or network behavior
changed.

`src/server/scripts/verify-standalone.sh` supplies the machine-checkable gate.
It copies the server source without build output into a fresh temporary root,
then runs locked offline metadata; check mode additionally compiles and test-
compiles `server-headless` and executes the four IFAC regressions. The
temporary root is removed on exit and no user Reticulum or OMEN data is read.

Validation completed on 2026-07-16. The relocated locked headless check and
test compilation passed, the relocated v0.6 wire fixture regression passed,
and all four relocated IFAC tests passed. The desktop product suite passed
1,479 tests with six explicit measurement fixtures ignored; its all-targets
Clippy gate passed with `-D warnings`. Standalone `server-headless` passed 196
tests with seven explicit socket/measurement/live gates ignored;
`server-full` checked and the headless all-targets Clippy gate passed with
`-D warnings`. Root/server formatting, fixture equality, `git diff --check`,
and `bash scripts/release-check.sh quick` passed. Rollback must move the IFAC
source and dependency declarations together, but would reopen the confirmed
standalone-packaging defect. Native Windows/macOS source-package execution
remains a release gate; no live network or performance measurement was needed
because runtime behavior and dependency resolution were unchanged.

## Phase 7 unit 14: immutable dependency-source gate and upstream recheck

Official upstream tags, release metadata, and source were rechecked on
2026-07-16 after `v0.9.5` was published. The `v0.9.5` tag and current `main`
still size UDP RX/TX buffers as `size_of::<Packet>() * 3` and still discard the
serialization error. No immutable corrected release or commit is therefore
available for approved production adoption. Moving OMEN from 0.9.0 to 0.9.5
would be a broad train change that does not solve the release blocker, so both
products remain on the reviewed exact registry 0.9.0 train.

`scripts/verify-reticulum-train.sh` now makes that source policy a release
gate. It verifies every direct family declaration remains exactly pinned,
uses locked product metadata for both independent roots, rejects any family
version other than 0.9.0, rejects Git/path/alternate registry sources, and
rejects duplicate package identities. `release-check.sh quick` invokes it.
The script requires `jq` only as a release/development inspection tool and
adds no runtime dependency.

No application code, lockfile, protocol, identity, configuration, database,
queue/quota, or runtime behavior changed. Rollback removes the assertion and
documentation but would allow an accidental source/version split to enter a
release unnoticed. The UDP Resource gate remains red pending an immutable
upstream correction; the next safe work can continue on security and
interoperability evidence that does not depend on successful UDP Resource
completion.

Validation passed on 2026-07-16. The new gate reported the desktop's seven and
omenchatd's three resolved family packages at registry 0.9.0 with no duplicate
identity. Shell syntax, formatting, and `git diff --check` passed.
`bash scripts/release-check.sh quick` passed with the dependency assertion,
isolated TUI/real-PTY lifecycle tests, product-feature check, focused OMENchat
tests, relocated standalone source check, IFAC regressions, and focused server
configuration tests. The upstream inspection was read-only; no issue, pull
request, release, commit, tag, or external message was created.

## Phase 7 unit 15: coherent 0.9.5 train alignment

The maintainer explicitly superseded unit 14's stay-on-0.9.0 decision and
approved following the coherent Reticulum/LXMF 0.9.5 train. Both application
packages and the private `omen-ifac-tcp` crate now report `0.9.5-1`; direct
Reticulum/LXMF family dependencies are exact registry `=0.9.5` pins. Root,
server, and fuzz lockfiles were updated without a Git source, local patch, or
mixed family identity. Protocol, destination, identity, configuration schema,
database schema, and state-directory versions remain independent and
unchanged.

The 0.9.5 SDK adds ZeroMQ to its default feature set. OMEN does not have an
admitted ZeroMQ runtime mode, so the desktop now declares `lxmf-sdk` directly
as `lxmf_sdk`, disables its defaults, and enables only the already used `std`,
`sdk-async`, and `rpc-backend` surfaces. Existing SDK imports moved from the
umbrella's `lxmf::sdk` re-export to that direct crate path. The locked product
graph contains no `zeromq` package. This preserves the existing runtime facade
and avoids admitting an unused runtime dependency merely because upstream
changed a default.

Source review found the 0.9.5 changes used by OMEN are additive; no Reticulum
transport, wire, LXMF message, or project DTO semantic migration was required.
The release retains upstream's pinned Python reference commits. The known UDP
Resource defect remains present in published 0.9.5 and current upstream source:
the worker still derives its wire buffer from `size_of::<Packet>() * 3` and
silently drops serialization failure. Both ignored known-red regressions were
run explicitly against registry 0.9.5: the capacity assertion failed `456 >=
483`, and the two-process Resource transfer exhausted retries after building
all four responses without receiving a part. The locally validated upstream
patch remains proposal evidence only; no production override was introduced.

Validation completed on 2026-07-16. The desktop product suite passed 1,479
tests with six explicit measurement/live fixtures ignored, and all-target
Clippy passed with `-D warnings`. Standalone no-default and `server-headless`
tests passed; the headless run passed 196 tests with seven explicit
measurement/live gates ignored. `server-full` checked, server all-target
Clippy passed with `-D warnings`, and all four IFAC regressions passed. The
quick release gate passed, including version/train assertions, isolated and
real-PTY TUI lifecycle smoke, focused OMENchat tests, standalone relocation,
and server feature/configuration smoke. Root and server `cargo deny check`
passed. The server RustSec audit passed; the root audit reproduced only the
already documented `quick-xml 0.39.2` advisories RUSTSEC-2026-0194 and
RUSTSEC-2026-0195 through Iced's Wayland build tooling.

The fuzz package resolves the new application version, but its standalone
`chat-client`-only check still fails because current desktop OMENchat state
references connection-state storage gated behind native Reticulum features.
That profile activates no Reticulum/LXMF family package, so this is a
pre-existing feature-boundary defect rather than a 0.9.5 API failure; it is
recorded, not hidden or repaired as collateral dependency work.

Rollback restores the 0.9.0 application and direct dependency versions, all
three lockfiles, the prior SDK feature/path declarations, version/train
assertions, UI/status strings, and migration documentation together. It does
not touch user identity, configuration, history, messages, uploads, databases,
or caches. The next release-critical unit remains upstream UDP Resource
adoption when an approved immutable fix exists, followed by pinned-Python,
current-Python, NomadNet, and mixed-version interoperability evidence.

## Phase 9 unit 1: pinned Python deterministic Reticulum vectors

The first release-blocking pinned-Python lane now verifies the immutable Python
Reticulum commit `15320e4d2cfabb143c1db20ca887e275fd521585` without installing
or floating a Python package. `scripts/run-pinned-python-reticulum.sh` either
fetches exactly that commit into an automatically removed temporary directory
or accepts an operator-supplied checkout. The Python oracle requires the exact
Git `HEAD` and a clean tracked/untracked tree before importing it. A wrong
revision and a modified pinned checkout were each exercised and rejected.

The oracle derives a fixed public 64-byte identity hash, name hashes and
destination hashes for `nomadnetwork.node`, `lxmf.delivery`,
`lxmf.propagation`, and `omenchat.node`, plus the existing public IFAC transmit
vector. Rust asserts the same values through registry Reticulum 0.9.5 and the
private `omen-ifac-tcp` crate. It does not initialize a Python Reticulum
runtime, bind a socket, read a user configuration, or touch identity/message
state. Fixed test credentials are public fixtures and are not suitable for a
real gateway.

`.github/workflows/python-interop.yml` supplies a read-only, manual and weekly
CI lane. Both repository checkouts and the Rust toolchain/cache actions use
immutable action commits; the Python checkout itself names the approved source
commit and disables credential persistence. This lane is intentionally
separate from the fast PR job. The existing workflow-security gate checks its
action pinning and least-privilege workflow permissions.

Validation completed on 2026-07-16. The fresh-fetch oracle matched identity
`aca31af0441d81dbec71e82da0b4b5f5`, all four name/destination pairs, and the
reviewed IFAC bytes. The focused Rust identity/destination test and standalone
IFAC test passed. Wrong-ref and dirty-tree negative checks passed. Root library
Clippy for `desktop-product` passed with `-D warnings`; root/server formatting,
shell syntax, workflow security, and `git diff --check` passed. The quick
release gate passed, including exact 0.9.5 train/version assertions, isolated
and real-PTY lifecycle checks, focused OMENchat behavior, standalone source
relocation, IFAC regressions, and server feature/configuration tests.

This is deterministic derivation and transform evidence, not live
interoperability. It does not yet prove announces, path discovery, packet
receipts/proofs, bidirectional TCP IFAC framing/reconnect, links, requests,
Resources, LXMF delivery/propagation, NomadNet, or mixed application versions.
Rollback removes the new runner/workflow, restores the narrower Python oracle
and Rust fixture assertion, and reverts the testing/ledger text; no dependency,
wire format, runtime configuration, or user data changes are involved. The next
safe unit is a two-process pinned-Python TCP/IFAC link-data lane; the known UDP
Resource defect remains excluded rather than weakened.

## Phase 9 unit 2: pinned Python IFAC TCP sockets

The pinned-Python runner now exercises the retained `IfacTcpClient` over real
IPv4 loopback sockets against a bounded peer imported from the exact clean
Python Reticulum reference. The Python peer uses the pinned TCP interface's
HDLC framing and Reticulum IFAC cryptographic primitives; its ingress transform
mirrors the pinned `Transport.inbound` authentication sequence. Rust sends
normal `Packet` values through the production interface manager and receives
decoded packets through the production bounded interface channel.

The correct-credential sequence authenticates Rust traffic in Python, returns
one Python frame split across two socket writes, returns two frames coalesced in
one write, closes the connection, accepts the production client's reconnect,
and authenticates a second exchange. A separate wrong-credential sequence
proves Python rejects the Rust packet and Rust rejects a correctly formed
Python response signed for the expected credentials. Payloads include both
HDLC flag and escape bytes. The Python peer caps frames at 4 KiB, uses bounded
accept/read/shutdown deadlines, accepts no more than two connections, binds
only `127.0.0.1:0`, and never initializes Python Reticulum storage or user
configuration.

Role reversal remains deliberately unsupported. OMEN's compatibility crate is
a TCP client, and omenchatd already fails closed when asked to use an
IFAC-configured stock upstream TCP server because that server does not enforce
the Python transform. This unit does not create a second server implementation
or weaken that validation merely to fill a matrix cell. IPv6, multiple
simultaneous clients, a complete Python Transport instance, long-running soak,
and idle/reconnect resource measurements remain pending.

Validation completed on 2026-07-16. The two explicit live tests passed in 5.87
seconds, including two authenticated exchanges separated by the production
five-second reconnect delay and the two-sided credential rejection. The
complete vector/TCP runner passed from a fresh temporary pinned checkout.
Standalone relocation check/test compilation and all four non-live IFAC tests
passed. `server-headless --all-targets` Clippy passed with `-D warnings`.
Root/server formatting, Python and shell syntax, workflow security, and
`git diff --check` passed. `bash scripts/release-check.sh quick` passed with the
0.9.5 version/train assertions, TUI lifecycle smokes, focused OMENchat tests,
standalone relocation, and server feature/configuration tests. The scheduled
GitHub job is defined but has not yet supplied remote-run evidence.

No production source, dependency, queue, timeout, wire format, interface
configuration, identity, database, or user state changed. Rollback removes the
ignored integration test and Python peer, restores the deterministic-only
runner/workflow label, and reverts the testing/gap/ledger text. The next safe
pinned-Python unit is announce/path/link establishment over a fully isolated
software interface; request/Resource coverage must continue to respect the
known upstream UDP Resource blocker.

## Phase 9 unit 3: pinned Python Reticulum path and link data

The pinned lane now initializes a complete Python Reticulum instance from the
exact clean reference commit. It uses an isolated generated configuration and
storage root, a fixed public test identity, one inbound `omeninterop.link`
destination, and Python's real IFAC `TCPServerInterface` on an ephemeral IPv4
loopback port. The Rust peer uses OMEN's production `IfacTcpClient` and the
registry Reticulum 0.9.5 `Transport`; no product dependency or runtime path was
changed.

The test subscribes to announces before requesting the Python destination path.
It requires the path request to trigger a validated zero-hop Python announce,
the exact destination identity to be recalled, an outbound Rust link to become
active, encrypted `rust-link-data` to reach Python's established-link callback,
and encrypted `python-link-data` to return through Rust's typed received-data
stream with the matching link identifier. It then detaches the only interface,
joins the interface task and Python stdout reader, waits for the Python process,
and deletes the isolated root. Fixed fixture credentials are public test data.

Validation completed on 2026-07-16. The focused full-runtime test passed in
0.73 seconds after the harness was made timeout-bounded. The combined pinned
runner passed the deterministic identity/destination/IFAC vector, raw TCP
framing/reconnect/wrong-credential tests, and this full Transport test. The
`omen-ifac-tcp --all-targets` Clippy gate passed with `-D warnings`.

This closes only the supported Python-server/Rust-client announce, path,
identity, link, and small link-data cells. It does not prove request/response,
Resources, receipts/proofs, LXMF, NomadNet, OMENchat, role reversal, IPv6,
multiple simultaneous clients, restart persistence, current-Python drift, or
performance/soak behavior. In particular, it does not weaken or bypass the
known upstream UDP Resource blocker. Rollback removes the ignored test and its
Python peer, restores the narrower runner and documentation, and changes no
identity, configuration, protocol, schema, dependency, or production code.
The next safe pinned unit is receipt/proof correlation over this established
software path; Resource-dependent work remains separately blocked.

## Phase 9 unit 4: pinned Python link proof correlation

The full-runtime pinned peer configures its inbound link destination to return
a Python Reticulum packet proof. Rust sends the existing small link-data
fixture with registry 0.9.5's `send_on_link_observed`, the same bound-interface
helper used by the production clean LXMF path. The helper returns the finalized
encrypted packet, while `Transport::set_receipt_handler` exposes only a proof
that passed upstream cryptographic validation.

The test requires the `DeliveryReceipt` hash to equal that finalized packet's
32-byte hash exactly. The receipt handler uses a four-item `try_send` metadata
channel, so it never blocks the transport and cannot grow without bound. After
the matching receipt, a bounded quiet interval requires that the one Python
packet did not generate a duplicate callback. The existing Python echo and
link-ID assertion still pass, which proves the proof addition did not replace
the data exchange.

Validation completed on 2026-07-16. The focused pinned full-runtime case passed
in 0.75 seconds, and `omen-ifac-tcp --all-targets` Clippy passed with
`-D warnings`. The complete runner, standalone relocation, formatting,
workflow-security, syntax, diff, and quick release gates are recorded with this
unit after their final run.

This is a cryptographically validated Reticulum link-packet proof, not an LXMF
delivery receipt and not proof that a user or remote router processed a
message. A cryptographically valid stale proof, timeout/retry, restart
reconciliation, destination-packet proofs, Resources, propagation, current Python, and mixed
versions remain separate gates. Production continues to label transport proof
as peer-unconfirmed. Rollback removes the Python proof strategy and bounded
receipt assertion and restores the prior labels/docs; no production code,
dependency, wire protocol, schema, configuration, or user state changes.
The next safe pinned unit is a negative cryptographically valid stale-proof
correlation case over the same isolated transport, followed by timeout/retry
correlation. Resource work remains blocked on the separately documented
upstream defect.

## Phase 9 unit 5: pinned Python forged-proof rejection

The pinned Python destination now uses `PROVE_APP` so one received Rust link
packet drives an explicit two-proof sequence. Python first flips one bit in the
received packet hash, appends a zeroed invalid signature, constructs a normal
link proof packet, and sends it through the real IFAC TCP interface. It then
calls the pinned implementation's `packet.prove()` for the unmodified packet
and reports that both transmissions occurred before the fixture may finish.

Rust retains the four-item nonblocking receipt channel from unit 4. The test
requires its first and only callback to equal the finalized encrypted Rust
packet hash. Therefore accepting the forged proof would either fail the exact
hash assertion or produce a forbidden second callback. The subsequent valid
proof must still succeed, demonstrating that rejecting malformed evidence does
not poison the active link or proof path.

Validation completed on 2026-07-16. The focused real-wire case passed in 0.74
seconds. The complete pinned runner passed, including the two raw IFAC socket
tests in 5.84 seconds and the full link/proof-rejection case in 0.75 seconds.
`omen-ifac-tcp --all-targets` Clippy passed with `-D warnings`; standalone
relocation, root/server formatting, Python/shell syntax, workflow security,
`git diff --check`, and `bash scripts/release-check.sh quick` also passed. The
scheduled GitHub job remains defined but has not yet supplied remote evidence.

This proves rejection of a modified-hash proof with an invalid signature. It
does not imply that Reticulum should reject a correctly signed proof carrying
an old packet hash: that evidence is cryptographically valid at the transport
boundary and must instead be ignored by OMEN's bounded pending-correlation
owner. A live valid-stale case, timeout/retry ordering, restart reconciliation,
destination proofs, Resources, propagation, current Python, and mixed versions
remain pending. Rollback restores the single-valid-proof Python policy and
removes the negative assertions/docs; production behavior and user state are
unchanged. The next unit should send a correctly signed stale hash before a
current proof and verify that only the current pending application operation
advances.

## Phase 9 unit 6: pinned Python stale-proof correlation

The explicit Python proof sequence now contains three wire packets for one
received Rust link packet: the invalid forged proof from unit 5, a correctly
signed proof over fixed stale hash `a5` repeated 32 times, and the valid proof
over the current finalized encrypted packet hash. The link identity signs both
valid hashes using pinned Python Reticulum, so registry transport must expose
the stale receipt before the current receipt while continuing to reject the
forgery. A bounded quiet interval requires no fourth callback.

Transport cannot decide whether a correctly signed hash still belongs to an
active application operation. The pinned runner therefore also executes the
production `clean_reticulum_stale_receipt_cannot_complete_a_newer_retry`
regression. That test registers an old attempt, removes it as retry ownership
changes, registers the newer hash for the same logical message, and feeds both
receipts through `CleanLxmfReceiptHandler`. The stale receipt must emit only a
`no_pending_correlation` diagnostic and leave the retry unproved; only the
newer hash may emit status and proof evidence, still marked peer-unconfirmed.

Validation completed on 2026-07-16. The focused real-wire ordering case passed
in 0.78 seconds and the focused production correlation test passed. The final
complete pinned runner passed, including the production correlation regression,
the two raw IFAC socket tests in 5.84 seconds, and the full proof-ordering case
in 0.76 seconds. `omen-ifac-tcp --all-targets` Clippy passed with `-D warnings`;
standalone relocation, root/server formatting, Python/shell syntax, workflow
security, `git diff --check`, and `bash scripts/release-check.sh quick` also
passed. The scheduled GitHub job has not yet supplied remote evidence.

This is combined evidence across the real transport boundary and the exact
production application-correlation owner; it does not pretend the standalone
IFAC crate owns LXMF state. Live retry deadlines, delayed proofs crossing an
actual retry transition, process restart reconciliation, destination proofs,
Resources, propagation, current Python, and mixed versions remain pending.
Rollback removes the stale proof packet/assertions and the explicit production
test invocation, restoring the forged/current sequence without changing
production behavior or user state. The next unit should exercise delayed proof
arrival across a bounded live retry window; restart recovery remains a separate
follow-up.

## Phase 9 unit 7: delayed old proof across a live replacement send

The pinned full-runtime peer now receives two real encrypted packets over one
active link. On `rust-link-data-old-attempt`, Python retains the received packet
object and deliberately sends no proof. Rust requires a bounded 250 ms interval
with no receipt before sending `rust-link-data-retry`. Only after receiving that
replacement does Python transmit the invalid forged proof, call `prove()` on
the retained old packet, and call `prove()` on the replacement packet.

The Rust transport test records both finalized packet hashes from
`send_on_link_observed`. It requires the first accepted receipt to equal the
actual old-attempt hash, the second to equal the replacement hash, and no third
callback. This proves the late evidence belongs to the exact earlier wire send,
not a fabricated fixed hash, while preserving one link and the Python echo.
The same runner immediately executes the production clean LXMF stale-retry
regression, which removes old correlation ownership before processing the late
receipt and advances only the replacement.

Validation completed on 2026-07-16. The focused two-send real-wire test passed
in 1.28 seconds. The final complete pinned runner passed, including the
production stale-retry regression, the two raw IFAC socket tests in 5.88
seconds, and the full two-send proof case in 1.29 seconds.
`omen-ifac-tcp --all-targets` Clippy passed with `-D warnings`; standalone
relocation, root/server formatting, Python/shell syntax, workflow security,
`git diff --check`, and `bash scripts/release-check.sh quick` also passed. The
scheduled GitHub job has not yet supplied remote evidence.

This closes delayed proof ordering across two real sends and the exact
production correlation decision, but does not claim the desktop retry scheduler
itself initiated the replacement or that 250 ms is a production retry timeout.
Production timeout/backoff integration, process restart recovery, destination
proofs, Resources, propagation, current Python, and mixed versions remain
pending. Rollback restores the one-send proof sequence and removes the bounded
no-proof assertion; no product behavior or user state changes. The next safe
proof unit is persisted correlation recovery across a runtime/process restart,
using isolated copied state and no real user database.

## Phase 9 unit 8: persisted receipt correlation across runtime replacement

The former clean-runtime recovery regression supplied an in-memory message row
directly to one runtime, so its name overstated the persistence evidence. The
replacement regression uses a unique temporary `MessageStore` root. It writes
recoverable old and current packet correlations, proves the first runtime can
recover both, durably deletes the obsolete thread, drops the store and runtime,
then reopens the bytes through a new store and a fresh `NativeNetworkRuntime`.
The fresh runtime must recover exactly the surviving current hash.

Both post-restart receipts pass through the production
`CleanLxmfReceiptHandler`. The deleted old hash may emit only the bounded
`no_pending_correlation` diagnostic. The surviving hash must map back to the
persisted current LXMF message and peer before emitting transport-proof evidence.
The test removes its isolated root after success and never discovers or opens a
real identity, Reticulum, LXMF, or message directory. The pinned runner invokes
this regression alongside the two-send Python proof ordering case so the live
wire hashes and application restart ownership remain one release lane.

Validation on 2026-07-16 passed the focused desktop-product test (1 test, 1,176
filtered) and the complete pinned runner. The runner passed both production
correlation regressions, both raw IFAC socket cases in 6.01 seconds, and the
two-send Python link/proof case in 1.29 seconds. Root desktop-product library
Clippy passed with `-D warnings`; root/server formatting, shell/Python syntax,
workflow security, `git diff --check`, standalone omenchatd relocation, and
`bash scripts/release-check.sh quick` also passed. The scheduled GitHub job has
not yet supplied remote evidence.

This unit changes no production behavior, protocol, schema, configuration, or
dependency. It proves store reopen plus runtime object replacement, not
operating-system process execution, crash-time filesystem fault injection, the
desktop startup scheduler, authoritative LXMF delivery, or
current-Python/mixed-version behavior. Rollback restores the in-memory-only
recovery test and removes its pinned-runner invocation; no user state migration
is involved. The next safe proof unit is an isolated child process restart that
exercises the same persisted recovery contract without opening real user roots.

## Phase 9 unit 9: persisted receipt correlation across process restart

The isolated recovery contract now crosses an operating-system process
boundary. The parent test writes old and current recoverable correlations,
proves its runtime owns both, durably deletes the old thread, and drops its
runtime and store. It then launches the current unit-test executable with an
exact ignored helper test. The temporary application root is carried only in a
test environment variable, never a command-line argument.

The child process independently opens the `MessageStore`, requires exactly the
current row, creates a new `NativeNetworkRuntime`, and recovers exactly one
correlation. It feeds old and current receipts through
`CleanLxmfReceiptHandler`: the deleted hash is diagnostic-only and the current
hash maps to the persisted LXMF message and peer before proof evidence is
emitted. Parent polling bounds the child to ten seconds, captures stdout/stderr
for failure diagnosis, reaps the process, and deletes the isolated root. The
helper is ignored and inert without the explicit environment variable.

Validation on 2026-07-16 passed the focused desktop-product parent/child test
(1 test, 1,178 filtered) in 0.01 seconds after compilation and the complete
pinned runner. The runner passed the runtime/store and executable-restart
regressions, both raw IFAC socket cases in 5.85 seconds, and the two-send Python
link/proof case in 1.27 seconds. Root desktop-product library Clippy passed with
`-D warnings`; root/server formatting, shell syntax, workflow security,
`git diff --check`, standalone omenchatd relocation, and
`bash scripts/release-check.sh quick` also passed. The scheduled GitHub job has
not yet supplied remote evidence.

This is a clean executable restart over safely committed state; it does not
simulate power loss, kill the writer at a filesystem boundary, exercise the
desktop startup scheduler, or claim authoritative LXMF delivery. No production
code path, protocol, schema, configuration, dependency, or user root changed.
Rollback removes the two test-only functions and pinned-runner invocation. The
next safe proof unit is production timeout/backoff integration with persisted
replacement ownership; abrupt crash-boundary coverage remains a separate
filesystem fault unit.

## Phase 9 unit 10: clean timeout and persisted replacement ownership

Inspection found that the scheduled durable direct-LXMF timeout recognized
only the legacy `submitted_to_rns_net`/`submitted_to_runtime` state with
`waiting_for_packet_proof`. The active registry 0.9 clean transport persists
`submitted_to_clean_reticulum` with `waiting_for_transport_receipt`. Those rows
therefore could remain pending indefinitely in the durable UI/store even though
the runtime correlation map could independently observe a no-receipt timeout.

The shared timeout transition now admits both exact proof-wait states and treats
the clean submitted state as `Submitted`. It retains the existing 45-second
policy, peer-unconfirmed result, propagation-fallback decision, and retry text;
no timer, queue, protocol, or schema changed. A focused transition regression
guards the clean state/proof pair.

The ownership regression uses an isolated `MessageStore` and production
`NativeNetworkRuntime`. It persists an old clean attempt, recovers it, runs the
runtime no-receipt transition and durable scheduled timeout, then persists a
replacement with the same logical operation identity and a different packet
hash. Before restart, a late proof for the retained old hash maps only to the
old message and leaves the replacement untouched. After dropping and reopening
the store/runtime, the timed-out old row is intentionally not recovered; its
late proof is diagnostic-only, while the replacement hash still maps to the
current message.

Validation passed on 2026-07-16: the clean transition test and persisted
replacement-ownership test each passed (1 test, 1,180 filtered), followed by the
complete pinned runner. The runner passed all production correlation/restart
regressions, both raw IFAC socket cases in 5.98 seconds, and the two-send Python
link/proof case in 1.26 seconds. Root desktop-product library Clippy passed with
`-D warnings`; root/server formatting, shell syntax, workflow security,
`git diff --check`, standalone omenchatd relocation, and
`bash scripts/release-check.sh quick` also passed. The scheduled GitHub job has
not yet supplied remote evidence.

Automatic retry dispatch remains user-explicit by design, and this unit does
not claim crash-boundary durability, authoritative LXMF delivery, or current-
Python/mixed-version behavior. Rollback removes the two clean-state alternatives
and the two focused regressions, returning to the prior behavior where clean
rows did not reach durable timeout. The next safe unit is abrupt process
termination after the committed timeout/replacement boundary, using only
isolated copied state.

## Phase 9 unit 11: abrupt termination after committed replacement

The timeout/replacement recovery contract now includes abrupt operating-system
process termination after a known durable boundary. An ignored child helper
uses only a unique temporary application root. It persists an old clean direct
attempt, commits the production timeout transition, persists the replacement
with the same logical operation identity and a different packet hash, and
verifies both rows can be reopened. Only then does it create and `sync_all` a
readiness marker before parking indefinitely.

The parent polls the marker and child status with a ten-second deadline. Once
the committed marker is visible, it terminates and reaps the still-running
child, requires a non-success exit, and reopens the store. Both committed rows
must be intact: the old row is `submitted_unconfirmed`/`proof_not_observed`, and
the replacement remains waiting. A fresh `NativeNetworkRuntime` must recover
exactly the replacement. The old receipt is diagnostic-only, while the current
receipt maps to the replacement message through `CleanLxmfReceiptHandler`.

Validation on 2026-07-16 passed the focused parent/kill/reopen regression (1
test, 1,182 filtered) in 0.01 seconds after compilation and the complete pinned
runner. The runner passed every production correlation/restart/termination
regression, both raw IFAC socket cases in 5.95 seconds, and the two-send Python
link/proof case in 1.28 seconds. Root desktop-product library Clippy passed with
`-D warnings`; root/server formatting, shell syntax, workflow security,
`git diff --check`, standalone omenchatd relocation, and
`bash scripts/release-check.sh quick` also passed. The scheduled GitHub job has
not yet supplied remote evidence.

The test uses environment variables for temporary paths, never command-line
secrets, and removes the isolated root. This establishes abrupt loss after
successfully committed atomic store operations. It does not kill during
serialization, staging-file sync, rename, or directory sync and therefore is
not a simulated power-loss/mid-write test. No production behavior, protocol,
schema, configuration, or dependency changed in this unit. Rollback removes
the parent/helper tests and pinned-runner entry. The next safe unit is injected
interruption at the message-store staging and replacement boundaries, reusing
the store's existing atomic-publish contract.

## Phase 9 unit 12: message-store publication fault boundaries

The private thread publisher now exposes a narrow internal boundary callback
covering stage creation, stage write, stage sync, destination commit, and parent
directory sync. The production caller supplies a no-op callback, so its write,
flush, file-sync, atomic-replace, and directory-sync sequence is unchanged. The
hook exists to test exact ownership boundaries without adding a runtime feature,
dependency, configuration key, or environment-controlled production behavior.

A deterministic fault matrix injects an error at every boundary. Before the
destination commit, the original timed-out thread must remain byte-exact. At or
after destination commit, the complete replacement thread must be byte-exact.
Every resulting destination is parsed through `MessageStore`, carries either
the old-only or old-plus-current message set, and every returned-error path
removes its staging file.

A separate child-process matrix parks at two boundaries and is killed by its
parent within ten seconds. Kill after stage sync leaves the original committed
thread and one non-JSON `.message.tmp` artifact; `MessageStore` ignores that
artifact and loads old ownership. Kill after destination commit leaves the
complete replacement and no stage; the current message is present. Thus no
partial JSON, mixed record set, or resurrected replacement is admitted at the
filesystem boundary.

Focused validation on 2026-07-16 passed the five-boundary injected-error matrix
(1 test, 1,185 filtered) and the two-boundary process-kill matrix (1 test, 1,185
filtered; two child processes) in 0.02 seconds after compilation. The pre-rename
orphan is intentionally observed rather than silently deleted: immediate cleanup
could race another live writer, while repeated old-stage accumulation can count
toward the bounded 4,096-entry directory scan. Age/owner-safe orphan cleanup and
a repeated-crash soak therefore remain pending. The complete pinned runner also
passed every production correlation/restart/publication regression, both raw IFAC
socket cases in 5.97 seconds, and the two-send Python link/proof case in 1.28
seconds. Root desktop-product library Clippy passed with `-D warnings`;
root/server formatting, shell syntax, workflow security, `git diff --check`,
standalone omenchatd relocation, and `bash scripts/release-check.sh quick` also
passed. The older single-boundary regression's isolated-root teardown was fixed
after the final audit found retained `/tmp` fixtures; its focused rerun passed and
left no matching test roots. The scheduled GitHub job has not yet supplied remote
evidence.

Rollback restores the single precommit callback/test and removes the boundary
matrices. The next safe unit is a bounded stale-stage inventory/recovery policy
that cannot delete a live writer's file.

## Phase 9 unit 13: leased stale-stage recovery

Message publication now creates a separate zero-byte lease beside each staging
file and holds an exclusive operating-system lock from before stage creation
through atomic destination replacement and directory sync. The payload handle
can still be closed before rename, preserving Windows replacement semantics.
Normal success and returned-error paths remove both artifacts and sync their
directory. The message JSON format, destination name, operation identity,
Reticulum/LXMF protocol, and timeout policy are unchanged.

`MessageStore` performs a bounded recovery scan when it opens. A stage is
removed only when its matching regular, zero-byte lease can be locked
nonblockingly and its path is absent from the process-local active-lease
registry. The registry closes the same-process cleanup race independently of
platform-specific lock semantics; the OS lock closes the cross-process race. A
locked or locally active lease is retained. A legacy stage without a lease is also retained
rather than using unsafe age or PID-reuse heuristics. Malformed/symlink-like
artifacts and lock errors are counted but retained. Normal directory entries
and publication artifacts have separate 4,096-entry ceilings, preventing crash
debris from silently making discovery unbounded while allowing abandoned
leased artifacts to be removed before normal thread inventory.

Focused tests cover abandoned leased recovery beside a retained legacy stage,
live-writer exclusion followed by cleanup after lock release, artifact-limit
rejection, and both real process-kill boundaries. The pre-rename child initially
leaves a stage and lease; the post-rename child leaves only its lease. Reopening
removes the abandoned artifacts and recovers exactly the previously established
old/current correlation ownership.

This unit admits exact `fs4 1.1.0` with only `sync`: it is maintained,
cross-platform, MIT/Apache-2.0, and declares Rust 1.75. It adds the OS-lock
primitive required to distinguish a live writer from crash debris without an
unsafe heuristic. Its removal path is the equivalent standard-library file-lock
API after OMEN deliberately raises its MSRV from 1.85 to at least 1.89.

Validation on 2026-07-16 passed all seven non-helper publication tests (one
ignored child helper), all 17 message-store integration tests, and the complete
desktop-product test matrix. The complete pinned runner passed every
correlation/restart/publication regression, both raw IFAC socket cases in 5.99
seconds, and the two-send Python link/proof case in 1.30 seconds. Root
desktop-product library Clippy passed with `-D warnings`; root/server formatting,
shell syntax, workflow security, `git diff --check`, standalone omenchatd
relocation, and `bash scripts/release-check.sh quick` also passed. The release
smoke's real PTY signal cases completed in 65-72 ms.

Dependency inspection shows `fs4` is the only newly resolved package and uses
the already present `rustix 1.1.4`. Cargo-deny licenses and sources pass. Its
bans gate retains the pre-existing path-dependency wildcard failure at
`omen-ifac-tcp`; the advisory scan retains the documented two high-severity
`quick-xml 0.39.2` findings and five maintenance warnings. None names `fs4`, but
those repository-level findings remain release work rather than being hidden by
this unit. The scheduled GitHub job has not yet supplied native Windows/macOS or
remote evidence.

Network-filesystem lock semantics and physical power-loss durability remain
outside this local process-termination proof. Rollback must remove lease
creation, recovery, the `fs4` dependency/lock entry, tests, runner entries, and
documentation together; existing `.message.tmp.lock` files are non-JSON and
ignored by the prior reader. The next safe unit is a bounded repeated-crash soak
that measures recovery time, artifact count, and retained disk bytes without
using the maintainer's real roots.

## Phase 9 unit 14: repeated publication-crash recovery soak

A deterministic soak now accumulates sixteen real pre-rename process crashes
inside one unique temporary message root before invoking recovery. Each child
uses the production publisher, syncs its complete replacement stage, creates
and syncs a readiness marker, and parks while holding the lease. The parent
terminates and reaps it, removes only that marker, and verifies the original
committed thread remains byte-exact. Child paths are supplied through test-only
environment variables rather than command-line arguments.

After all crashes, the fixture must contain exactly sixteen complete stages and
sixteen zero-byte leases. One production recovery pass must acquire every
released lease, remove all 32 artifacts, sync the directory, retain zero
artifact bytes, and reopen the original one-message thread. Per-child readiness
is bounded to five seconds, so failure cannot create an unbounded wait. The test
prints a machine-readable summary containing crash count, before/after artifact
counts and bytes, recovery microseconds, and total elapsed milliseconds; it does
not impose a hardware-specific performance threshold.

`scripts/measure-message-publication-recovery.sh` runs the same regression in a
locked release build with `--nocapture` and a two-job default. The normal focused
debug run on 2026-07-16 accumulated 32 artifacts / 20,064 bytes, recovered them
in 532 microseconds, retained zero artifacts / bytes, and completed all sixteen
child terminations in 177 milliseconds. The release run used Rust 1.97.0 on
Linux 7.1.3-2-cachyos x86_64, recovered the same 32 artifacts / 20,064 bytes in
274 microseconds, retained zero artifacts / bytes, and completed in 171
milliseconds. These are reproducible observations, not portable release
thresholds.

The first complete parallel matrix exposed that the original same-process test
used a manually locked descriptor rather than a production publisher. The test
was replaced with a child-held cross-process lease, and production publication
now also registers its active lease in a bounded-by-active-writers process-local
set. A concurrent thread regression parks the real publisher after stage sync,
runs recovery from the same process, and proves the stage is retained until the
publisher commits. The publication-module parallel rerun passes nine tests with
one ignored child helper, and the complete desktop-product library rerun passes
1,186 tests with five intentional measurement/helper ignores.

This unit changes no dependency, format, protocol, or configuration. Its only
production hardening is the process-local active-lease exclusion set; entries
have publisher-stack ownership and are removed by `Drop` on every normal/error
return. Rollback removes that registry, the soak and concurrent-writer tests,
the test-only child-launch helper refactor, measurement script, pinned-runner
entries, and documentation together. Network filesystems, kernel/power loss,
and crash counts near the 4,096-artifact admission ceiling remain separate
evidence tasks.

Final validation on 2026-07-16 passed root desktop-product library Clippy with
`-D warnings`, the complete desktop-product matrix (1,187 library tests passed;
five intentional measurement/helper ignores), and the complete pinned runner.
The pinned runner passed every correlation/restart/publication case, both raw
IFAC socket cases in 5.95 seconds, and the two-send Python link/proof case in
1.24 seconds. Root/server formatting, both measurement/pinned shell syntax,
workflow security, `git diff --check`, standalone omenchatd relocation, and
`bash scripts/release-check.sh quick` passed; real PTY signal shutdown measured
64-72 ms. No native Windows/macOS, network-filesystem, or physical power-loss
result is claimed. The scheduled GitHub job has not yet supplied remote
evidence.

## Phase 9 unit 15: exact publication-artifact ceiling recovery

The recovery suite now exercises the exact 4,096-artifact admission ceiling
rather than only a small crash batch and the first rejected entry. An isolated
fixture creates 2,047 abandoned complete-stage/zero-byte-lease pairs, then parks
one real child publisher after its stage sync while it holds the final lease.
The initial inventory is exactly 2,048 pairs: 4,096 artifacts and 2,568,192
retained bytes.

One production recovery pass must remove all 2,047 abandoned pairs while
retaining the child-owned pair. The parent then terminates and reaps the child,
removes its readiness marker, and a second recovery pass must remove the final
stage and lease. The committed old thread remains byte-exact and reopens as one
valid message. The existing overload regression continues to reject artifact
4,097, so both sides of the bound are covered.

The focused debug run on 2026-07-17 removed 4,094 abandoned artifacts in 36,147
microseconds, retained exactly two artifacts / 1,254 bytes for the live writer,
then removed that released pair in 45 microseconds. The release run on Rust
1.97.0 and Linux 7.1.3-2-cachyos x86_64 removed the same abandoned inventory in
29,468 microseconds, retained the live pair, and removed it in 30 microseconds.
The companion release soak recovered 32 artifacts / 20,064 bytes from sixteen
real process crashes in 544 microseconds and completed in 175 milliseconds. No
portable latency threshold is inferred from these host observations. Final gate
results are recorded after validation.

This unit changes no production behavior, dependency, protocol, schema,
configuration, or cleanup limit. Rollback removes the exact-ceiling test,
measurement/pinned-runner entries, and documentation. Network-filesystem and
physical power-loss behavior remain outside this deterministic local evidence.

Final validation on 2026-07-17 passed all ten non-helper publication tests in
parallel (one child helper ignored), root desktop-product library Clippy with
`-D warnings`, and the complete desktop-product matrix (1,188 library tests
passed; five intentional helper/measurement ignores, plus all integration and
doc tests). The complete pinned runner passed the exact-ceiling case and every
prior correlation/restart/publication regression, both raw IFAC socket cases in
6.01 seconds, and the two-send Python link/proof case in 1.30 seconds.
Root/server formatting, measurement/pinned/release shell syntax, workflow
security, `git diff --check`, standalone omenchatd relocation, and
`bash scripts/release-check.sh quick` passed; real PTY signal shutdown measured
61-74 milliseconds. The temporary-root audit found no retained publication
fixtures. No native Windows/macOS, network-filesystem, physical power-loss, or
scheduled GitHub result is claimed.

## Phase 9 unit 16: current Python drift lane

The current-Python drift lane is now explicit and cannot replace the immutable
pinned-reference gate. On 2026-07-17 the official PyPI metadata still reported
RNS 1.3.8, LXMF 1.0.1, and NomadNet 1.2.7. A new isolated runner installs those
exact top-level versions in a disposable virtual environment, verifies all
three imports, records Python/pip/package versions in a machine-readable JSON
report, and removes the environment on exit. It does not read or create normal
Reticulum, LXMF, NomadNet, browser, or server roots.

The Python fixtures now accept either the existing clean immutable Git checkout
or an explicitly version-checked installed RNS tree. The legacy pinned source
environment variable and exact-commit verification remain unchanged. The
current lane reuses only the public compatibility-vector, IFAC TCP, and
link/proof cases: identity/destination/IFAC bytes, split/coalesced frames,
reconnect, wrong-credential rejection, announce/path/link data, forged-proof
rejection, and old/current proof ordering. LXMF and NomadNet are installed and
import-checked but their application behavior is not claimed.

The scheduled job pins Python 3.12.11 and every action by immutable commit,
uploads the report for fourteen days, and is marked `continue-on-error`. The
pinned Python job remains a separate normal gate. Transitive Python packages
are intentionally a fresh drift observation rather than a reproducible release
input; their movement can make this informational lane fail but cannot silently
change the pinned parity contract.

The local Linux/Python 3.14.6 run passed with pip 26.1.2. Current RNS reproduced
all reviewed compatibility vectors, both IFAC socket cases passed in 5.89
seconds, and the two-send forged/stale/current proof case passed in 1.29
seconds. Final repository gates are recorded after validation. No current LXMF
delivery/propagation, NomadNet page/request/resource, Python-client role
reversal, IPv6, mixed 0.6/0.9, native platform, or public-network result is
claimed.

This unit changes no production Rust path, dependency, protocol, schema,
configuration, state root, or release-blocking pin. Rollback removes the
current-drift runner/job/report documentation and restores pinned-only fixture
source selection. The next current-Python expansion must add one application
behavior at a time rather than treating successful imports as interoperability.

Final validation on 2026-07-17 passed the current lane twice. Its final run
captured all twelve resolved Python distributions, reproduced the reviewed
vectors, passed both IFAC socket cases in 5.89 seconds, and passed the proof
sequence in 1.26 seconds. The pinned runner then passed while deliberately
given hostile current-lane source/version variables; it discarded both,
verified commit `15320e4d2cfabb143c1db20ca887e275fd521585`, passed IFAC in
6.02 seconds, and passed the proof sequence in 1.26 seconds.

Root and standalone formatting, root desktop-product library Clippy, standalone
`omen-ifac-tcp` all-target warnings-denied Clippy, Python entrypoint syntax,
shell syntax, workflow security, the complete desktop-product matrix (1,188
library tests passed; five intentional helper/measurement ignores, plus all
integration/doc tests), `git diff --check`, standalone omenchatd relocation,
and `bash scripts/release-check.sh quick` passed. Real PTY shutdown measured
67-68 milliseconds. No scheduled CI execution or native-platform result is
claimed yet.

## Phase 9 unit 17: current Python LXMF direct delivery

The informational current-Python lane now exercises one application-level LXMF
behavior instead of stopping at imports. An isolated Python LXMF 1.0.1
`LXMRouter` registers a fixed delivery identity on an IFAC-protected loopback
TCP server. The Rust side uses the production OMEN LXMF message builder and wire
signer, the published Reticulum 0.9.5 transport, and the project IFAC interface.
It announces its real `lxmf.delivery` identity and waits for Python to confirm
that discovery before creating the delivery link.

The regression requires an activated link, exact title/content/source and
destination hashes, Python's direct transport method, Python signature
validation, and a Reticulum packet proof whose hash matches the Rust send. The
fixture initially exposed an unverifiable message when sender discovery was
omitted; the final test deliberately proves the authentic announce path instead
of preloading sender trust. A two-worker bounded Tokio runtime is used because
announce dispatch and IFAC receive work must continue while the external
process acknowledgement is read.

The Python environment, Reticulum configuration/storage, LXMF storage, and Rust
fixture root are unique temporary directories removed on exit. The lane remains
informational and exact-versioned at RNS 1.3.8/LXMF 1.0.1/NomadNet 1.2.7; it
does not change the immutable pinned gate. No production behavior, dependency,
wire format, schema, configuration, or normal state root changes in this unit.
Rollback removes the Python LXMF peer, ignored Rust interop test, runner entry,
CI root-build prerequisites, and these documentation entries together.

The focused local Linux/Python 3.14.6 test passed in 0.88 seconds after the
sender announce was made observable. Python-to-Rust LXMF delivery,
Resources/attachments, propagation, ticket/stamp behavior, NomadNet application
behavior, mixed 0.6/0.9, public-network, and native-platform evidence remain
separate units.

Final validation on 2026-07-17 passed the complete current lane: the IFAC
socket cases took 5.90 seconds, the forged/stale/current proof case took 1.29
seconds, and the Rust-to-Python direct LXMF delivery took 0.77 seconds. Its JSON
report records all twelve resolved Python distributions and the four executed
check groups. The immutable pinned runner passed independently against commit
`15320e4d2cfabb143c1db20ca887e275fd521585`, including IFAC in 5.86 seconds and
proof ordering in 1.25 seconds.

Root/server formatting, root desktop-product library Clippy, standalone
`omen-ifac-tcp` all-target warnings-denied Clippy, Python/shell syntax, workflow
security, and `git diff --check` passed. The complete desktop-product matrix
passed 1,194 library tests with six intentional helper/measurement/interop
ignores, plus every integration and doc test. `bash scripts/release-check.sh
quick` passed product/version/dependency identity, real PTY shutdown at 64-73
milliseconds, focused browser/server tests, and standalone omenchatd relocation.
No scheduled CI or native-platform result is claimed.

## Phase 9 unit 18: current Python-to-Rust LXMF direct delivery

The informational current-Python lane now covers the reciprocal small-message
direction. An isolated LXMF 1.0.1 `LXMRouter` registers and announces a fixed
source identity, learns the Rust `lxmf.delivery` destination from the Rust
authenticated announce, and sends an ordinary direct message. The Rust 0.9.5
transport accepts the inbound link, returns the packet proof, and supplies the
link data to the production OMEN decoder.

The regression requires exact source/destination hashes, title, content, and
message ID across Python and Rust; Python must report its direct method and a
successful delivery callback. Rust also parses the same bytes with
`lxmf-wire` and verifies the signature against the identity carried by the
validated Python announce. The focused Linux/Python 3.14.6 exchange passed in
0.80 seconds. Together with unit 17, both small direct-message directions now
have current-stack evidence.

This test exposed a remaining admission boundary: the production OMEN decoder
parses signed LXMF wire but does not currently receive a resolved announce
identity and enforce `WireMessage::verify` before producing `MessageSummary`.
The test therefore performs verification separately and does not claim that
production rejects forged inbound LXMF. Moving identity resolution and
verification into the bounded production worker requires a dedicated security
unit with unknown-source, invalid-signature, identity-mismatch, replay, and
resource-path regressions.

The fixture uses unique temporary Reticulum/LXMF roots, fixed public test IFAC
credentials, bounded waits, one guarded send, and process cleanup. This unit
changes no production behavior, dependency, protocol, schema, configuration, or
normal state. Rollback removes the sender fixture, reciprocal ignored test,
runner filter/report rename, and these documentation entries. Resources,
attachments, propagation, tickets/stamps, NomadNet, mixed 0.6/0.9, forged-input
rejection, public-network, and native-platform evidence remain separate units.

Final validation on 2026-07-17 passed the complete current lane. Both direct
directions completed in 1.57 seconds together; the IFAC cases took 5.87 seconds
and proof ordering took 1.30 seconds. The independent pinned runner passed
against commit `15320e4d2cfabb143c1db20ca887e275fd521585`, including IFAC in
5.97 seconds and proof ordering in 1.27 seconds.

Root/server formatting, root desktop-product library Clippy, standalone
`omen-ifac-tcp` all-target warnings-denied Clippy, Python/shell syntax, workflow
security, and `git diff --check` passed. The complete desktop-product matrix ran
1,195 library tests: 1,188 passed and seven intentional helper, measurement, or
interop tests were ignored; every integration and doc test passed. The quick
release gate passed product/version/dependency identity, focused browser/server
tests, standalone omenchatd relocation, and real PTY shutdown at 64-69
milliseconds. No scheduled CI or native-platform result is claimed.

## Phase 9 unit 19: production direct-LXMF signature admission

The clean Reticulum bridge now resolves every direct inbound LXMF source through
the existing bounded destination-identity cache populated by transport-validated
announces. Admission requires an exact `lxmf.delivery` source derivation and a
successful `lxmf-wire 0.9.5` `WireMessage::verify` before the runtime creates a
`MessageSummary`, writes attachment bytes, or enters the replay inventory.
Unknown sources, a cached identity under the wrong source, and forged signatures
are rejected. Link-data, full-wire, and resource paths emit a key-free ingress
diagnostic for non-truncation rejection; partial link fragments remain quiet.

Source parsing and identity-cache resolution run inside the existing bounded
blocking gate. The cache mutex is released before signature/payload decoding and
is never held across an await. No queue, cache, worker, dependency, protocol
byte, schema, configuration key, or state root is added. Verified messages
retain the existing five-minute message-ID replay suppression. The current-
Python reciprocal test now invokes the same verified codec boundary rather than
verifying separately after unverified production decoding.

Six deterministic regressions cover a valid matching identity, forged
signature, source/identity mismatch, unknown announce identity, verified replay,
and a forged attachment that must cause no filesystem write. The current Python
RNS 1.3.8/LXMF 1.0.1/NomadNet 1.2.7 informational lane passed both direct LXMF
directions together in 1.56 seconds, both IFAC cases in 5.89 seconds, and proof
ordering in 1.30 seconds. Propagated LXMF sender verification remains a separate
encrypted-envelope task; this unit makes no Resources interoperability,
mixed-version, public-network, scheduled-CI, or native-platform claim.

Rollback removes the verified decoder entry points, announce-cache resolution
at the four clean inbound call sites, the six regressions, rejection diagnostics,
and this documentation together. That rollback would deliberately restore the
unit-18 security gap and must not be used for a release claiming authenticated
direct inbound LXMF.

Final validation on 2026-07-17 passed root desktop-product library Clippy with
`-D warnings` and the complete desktop-product matrix. The library ran 1,201
tests: 1,194 passed and seven intentional helper, measurement, or live-interop
tests were ignored; all integration and doc tests passed. The independent pinned
runner verified commit `15320e4d2cfabb143c1db20ca887e275fd521585`, passed
both IFAC cases in 5.95 seconds, and passed forged/stale/current proof ordering
in 1.26 seconds.

Root/server formatting, standalone `omen-ifac-tcp` all-target warnings-denied
Clippy, shell syntax, workflow security, and `git diff --check` passed. The quick
release gate passed product/version/dependency identity, focused browser/server
tests, standalone omenchatd relocation, and real PTY shutdown at 65-73
milliseconds. No scheduled CI execution or native Windows/macOS result is
claimed.

## Phase 9 unit 20: propagated-LXMF sender signature admission

The production clean propagation-sync decoder now decrypts a payload with the
local recipient identity and then applies the same authenticated sender policy
as direct inbound LXMF. The embedded source hash must resolve in the existing
256-entry authenticated announce/path identity cache, derive the exact
`lxmf.delivery` destination, and pass `WireMessage::verify` before message or
attachment state is created. Source extraction, cache resolution, signature
verification, MessagePack decoding, and attachment handling remain inside the
existing bounded blocking gate; the cache mutex is released before payload
decoding and is never held across an await.

An unknown, mismatched, or forged sender returns a decode rejection. Because the
encrypted envelope is addressed to the local identity, propagation sync treats
that rejection as deferred: it does not update the delivered-transient store or
send the transient ID in the acknowledgement list. This preserves a retry path
after sender identity discovery instead of accepting unauthenticated history or
causing message loss. The legacy disabled `native-rns-net` compatibility branch
retains its prior decoder; the canonical clean product path always supplies the
authenticated identity cache.

Six focused regressions cover matching sender admission, unknown sender
deferral, cached-identity mismatch, forged signature, forged attachment
pre-write rejection, and the no-ack/no-delivered-marker sync invariant. No
dependency, wire byte, protocol version, schema, configuration, cache limit,
state root, queue, or worker is added. Live current/pinned Python propagation,
ticket/stamp interaction, propagation-node restart, and public-network behavior
remain separate interoperability evidence.

Rollback removes propagation wire normalization, the verified propagated decode
entry point, authenticated-cache injection at the clean sync call, the six
regressions, and these documentation changes together. Such a rollback restores
unauthenticated propagated-message admission and is incompatible with a release
claiming sender-authenticated propagation history.

Final validation on 2026-07-17 passed root desktop-product library Clippy with
`-D warnings` and the complete desktop-product matrix. The library ran 1,207
tests: 1,200 passed and seven intentional helper, measurement, or live-interop
tests were ignored; every integration and doc test passed. The current-Python
informational lane passed its direct-delivery pair in 1.61 seconds, IFAC in 5.88
seconds, and proof ordering in 1.29 seconds. It does not yet exercise live
propagation.

The independent pinned runner verified commit
`15320e4d2cfabb143c1db20ca887e275fd521585`, passed IFAC in 5.96 seconds, and
passed forged/stale/current proof ordering in 1.29 seconds. The quick release
gate passed product/version/dependency identity, focused browser/server tests,
standalone omenchatd relocation, and real PTY shutdown at 64-67 milliseconds.
No scheduled CI execution, native Windows/macOS result, or live Python
propagation result is claimed.

## Phase 9 unit 21: deferred propagation sender recovery and replay bounds

The propagated decoder now returns an unresolved 16-byte sender destination as
structured process-local state when authenticated identity resolution fails.
The clean sync coordinator dispatches an exact Reticulum path request for that
destination, at most once per source and at most 32 unique sources per sync. It
awaits only transport dispatch, does not wait for path resolution, and holds no
identity-cache lock across the request. The local encrypted payload remains
unacknowledged so a later sync can verify it after normal announce/path handling
populates the bounded identity cache.

The same response loop now owns a bounded transient-ID admission set. Exact
duplicate candidates are suppressed before decrypt/verify/attachment/history
work. A transient already in the durable delivered cache is acknowledged, when
needed, without republishing its message. The existing propagation response
limit bounds the per-response set to 4,096 entries; no payload-bearing cache or
queue is added. Sync diagnostics report duplicate suppression and sender path
request counts.

Focused regressions require the unknown-sender decoder result to retain the
exact source hash, a matching sender to retain no unresolved state, duplicate
transients to admit only once, path requests to be unique and stop exactly at
32 sources, and the deferred branch to request recovery without touching the
delivered-marker or acknowledgement inventories. No dependency, protocol byte,
schema, configuration, state root, timeout, or user-visible delivery state
changes.

Rollback removes the structured unresolved-source field, both bounded admission
helpers, sender-path dispatch, duplicate/already-delivered suppression, sync
counters, regressions, and these documentation changes together. The strict
unit-20 signature gate remains valid after rollback, but unknown senders would
again require unrelated announce activity before a later sync could succeed.

Final validation on 2026-07-17 passed root desktop-product library Clippy with
`-D warnings` and the complete desktop-product matrix. The library ran 1,209
tests: 1,202 passed and seven intentional helper, measurement, or live-interop
tests were ignored; every integration and doc test passed. The current-Python
informational lane passed its direct-delivery pair in 1.60 seconds, IFAC in 5.87
seconds, and proof ordering in 1.28 seconds; it still does not claim live
propagation.

The independent pinned runner verified commit
`15320e4d2cfabb143c1db20ca887e275fd521585`, passed IFAC in 5.86 seconds, and
passed forged/stale/current proof ordering in 1.24 seconds. The quick release
gate passed product/version/dependency identity, focused browser/server tests,
standalone omenchatd relocation, and real PTY shutdown at 65-66 milliseconds.
No scheduled CI execution, native Windows/macOS result, or live Python
propagation result is claimed.

## Phase 9 unit 22: current-Python propagation enqueue/sync/ack

The informational drift lane now starts an exact RNS 1.3.8/LXMF 1.0.1 router
as an isolated propagation node over the existing project IFAC process harness.
After learning the Rust receiver through an authenticated `lxmf.delivery`
announce, Python creates a signed recipient-encrypted propagated message,
ingests it into the real router store, and announces both the sender and
`lxmf.propagation` destinations. The canonical Rust runtime then exercises its
production path/link/identify and `/get` list/get/ack sequence.

The test requires production admission to resolve the exact Python sender from
the authenticated announce cache, verify the embedded signature, publish one
propagated `MessageSummary` with byte-exact title/content/source, and acknowledge
the transient. The Python router must retain the entry until that acknowledgement
and then report an empty store. The Python configuration, identity, message
store, Rust identity, attachments, and runtime state all live under unique
temporary roots; the child and every protocol wait have bounded shutdown.

This is one current-Python topology, not universal propagation parity. It does
not cover pinned Python, propagation stamps or tickets, Resources, cancellation,
node restart during transfer, multiple messages/recipients, mixed 0.6/0.9,
public networks, or peer-level delivery after node acceptance. It changes no
production code, dependency, wire byte, protocol/schema/configuration version,
queue, cache, timeout default, or state root. The existing strict unknown-sender
deferral and bounded recovery policy remain unchanged.

Rollback removes the Python propagation fixture, ignored adapter test, drift
report check label, and these documentation updates. The production propagation
implementation and unit-20/unit-21 security invariants remain intact after that
rollback; only the new live evidence is lost.

Final validation on 2026-07-17 passed root desktop-product all-target Clippy
with `-D warnings` and the complete desktop-product matrix. The library ran
1,210 tests: 1,202 passed and eight intentional helper, measurement, or
live-interop tests were ignored; every integration and doc test passed. The
focused propagation case passed in 2.30 seconds. The complete current-Python
informational lane passed IFAC in 5.86 seconds, proof ordering in 1.28 seconds,
and both direct directions plus propagation in 3.87 seconds; its JSON report
records the new `python_propagation_node_rust_sync_ack` check.

The independent pinned runner verified commit
`15320e4d2cfabb143c1db20ca887e275fd521585`, passed IFAC in 5.98 seconds, and
passed forged/stale/current proof ordering in 1.27 seconds. The quick release
gate passed product/version/dependency identity, focused browser/server tests,
standalone omenchatd relocation, and real PTY shutdown at 65-71 milliseconds.
Formatting, shell syntax, Python syntax, and `git diff --check` passed. No
scheduled CI execution or native Windows/macOS result is inferred from these
local Linux runs.

## Phase 9 unit 23: release-blocking pinned-Python propagation

The pinned interoperability runner now fetches and verifies both immutable
upstream Python references independently: Reticulum commit
`15320e4d2cfabb143c1db20ca887e275fd521585` identifies as module version 1.2.2,
and LXMF commit `727830cefda83d9c6e3982b48675425f3f988f9c` identifies as 0.9.6. A supplied
checkout must match its exact commit and have no tracked or untracked changes.
The runner imports directly from those source roots; it does not install a host
package, use a floating branch, or introduce a production dependency.

The unit parameterizes the unit-22 isolated propagation fixture without
weakening the current-package assertions. The pinned and current tests remain
distinct ignored gates with exact expected module versions. In the pinned gate,
Python learns the Rust `lxmf.delivery` receiver, stores one signed
recipient-encrypted transient in its real propagation router, and announces the
sender and node. The canonical Rust runtime uses its production
path/link/identify and `/get` list/get/ack path, authenticates the sender,
publishes the byte-exact message once, and acknowledges it. Python must retain
the transient until the acknowledgement and then report an empty store.

The pinned GitHub job now checks out both references with immutable action and
source revisions and passes both roots to the runner. This makes the narrow
single-message propagation topology release-blocking. It does not establish
Resources, required propagation stamps/tickets, cancellation, node restart
during sync, multiple recipients, mixed 0.6/0.9, public-network behavior, or
peer-level delivery after propagation-node acceptance. No production code,
dependency, protocol byte, schema, configuration, state root, queue, cache, or
runtime default changes.

Rollback removes the pinned test wrapper, dual-source fixture arguments, LXMF
checkout/verification in the runner and workflow, and these documentation
updates. The informational current-Python test and production propagation path
remain intact, but the release would again lack its required pinned LXMF
propagation evidence.

The focused pinned propagation case passed in 2.63 seconds on its first run and
2.31 seconds inside the complete pinned lane. That lane also passed IFAC in
5.89 seconds and forged/stale/current proof ordering in 1.26 seconds. The
current package lane remained green after parameterization: IFAC 5.90 seconds,
proof ordering 1.30 seconds, and its direct pair plus propagation 3.83 seconds.
Final validation passed root desktop-product all-target Clippy with `-D
warnings` and the complete desktop-product integration/doc matrix. The library
ran 1,211 tests: 1,202 passed and nine intentional helper, measurement, or live
interop tests were ignored. Workflow security verification and the quick
release gate passed, including product/version/dependency identities,
standalone omenchatd relocation, focused browser/server tests, and real PTY
shutdown at 66-69 milliseconds. Formatting, shell/Python syntax, and
`git diff --check` passed. No scheduled CI execution or native Windows/macOS result is
inferred from these local Linux runs.

## Phase 9 unit 24: pinned/current propagation-stamp boundaries

The release-blocking pinned lane and informational current-package lane now
exercise the production Rust propagation-stamp generator against the exact
Python LXMF validator. Rust builds one bounded cost-2 stamp with at most 4,096
attempts. The isolated Python fixture imports either the immutable pinned
Reticulum/LXMF sources or the current disposable environment, invokes
`LXStamper.validate_pn_stamps`, and must preserve the transient/stamp and report
the same achieved value. It then raises the required value by exactly one and
must reject the identical bytes. The achieved-value boundary is deterministic;
the test does not depend on whether a randomly corrupted stamp happens to retain
a low proof value.

The fixture caps transient input at one MiB, requires an exact 32-byte stamp,
uses only explicit temporary roots, and never reads application identity,
message, cache, or server state. The current drift JSON report records
`rust_python_propagation_stamp_boundaries`; the pinned workflow runs the same
case after verifying both immutable source trees. No runtime dependency,
production behavior, wire format, protocol/schema version, configuration,
state root, queue, cache, or default changed.

This evidence covers the Rust/Python propagation workblock, stamp value, and
accept/reject boundary. It does not yet send the stamped transient through the
network-facing Python propagation packet/resource admission handler, establish
automatic policy negotiation, test direct-message stamps, or cover tickets,
high costs, cancellation, and shutdown during proof work. Those remain separate
units. Rollback removes the fixture, two ignored adapter tests, runner/report
entries, workflow labels, and these documentation updates; the existing
cost-zero propagation topology remains intact.

The focused pinned case passed in 0.76 seconds; it passed in 0.38 seconds inside
the complete pinned lane. That lane also passed IFAC in 5.88 seconds,
forged/stale/current proof ordering in 1.25 seconds, and propagation sync/ack in
2.32 seconds. The complete current drift lane remained green: IFAC 5.90
seconds, proof ordering 1.29 seconds, and both direct directions plus
propagation sync and the new stamp matrix in 4.18 seconds. The library ran 1,213
tests: 1,202 passed and 11 intentional live or measurement helpers were
ignored. The complete desktop-product integration/doc matrix, all-target Clippy
with warnings denied, workflow-security verification, and the quick release
gate passed. The quick gate rechecked product/version/dependency identity,
standalone omenchatd relocation and focused tests, and isolated real-PTY
shutdown at 65-68 milliseconds. Formatting, shell/Python syntax, and
`git diff --check` passed. No scheduled CI or native Windows/macOS result is
inferred from these local Linux runs.

## Phase 9 unit 25: network-facing propagation-stamp admission

The pinned and current Python lanes now exercise required propagation stamps
through both implementations' real network paths. The isolated Python router
advertises its upstream-enforced minimum propagation cost of 13. The production
Rust clean sender learns the receiver and propagation identities/app-data,
generates work behind the existing two-job blocking gate with the production
2^22-attempt ceiling, packs the recipient-encrypted signed propagation
envelope, establishes the propagation link, and uses the normal link
packet/Resource selector. Python's actual `propagation_packet` handler invokes
the upstream stamp validator, accepts the message, verifies its signature, and
delivers it locally with the propagated method.

The fixture then raises the Python router's live admission floor to 255 without
issuing a new announce, deliberately modelling a stale sender policy. Rust sends
a second production envelope using the previously advertised cost 13. Python's
same network handler rejects it, tears down the link, leaves
`client_propagation_messages_received` at one, and does not invoke a second
delivery callback. A narrow observation wrapper records the inputs/results of
the upstream validator but delegates validation unchanged. This proves remote
accept/reject behavior rather than inferring admission from local validation.

The cost-255 floor is validation-only; neither implementation generates
cost-255 work. The Rust generator remains bounded, the runtime has two Tokio
workers, all Reticulum/LXMF/config/storage state is under a unique temporary
root, and child/read deadlines are explicit. The pinned fixture needed a
45-second lifetime because its earlier 20-second process deadline could expire
during destination discovery, stamp work, and link setup; the focused pinned
case completes materially below that ceiling.

No production code, dependency, protocol/schema version, configuration, state
root, queue/cache limit, or default changed. The current drift report adds
`network_propagation_stamp_accept_reject`; the pinned runner makes the same test
release blocking. Automatic propagation-cost refresh/retry, direct-message
required stamps, tickets, Resource-sized stamped envelopes, cancellation during
work, and high-cost performance remain separate evidence. Rollback removes the
fixture, its two ignored adapter tests, runner/report entries, and these
documentation updates; the unit-24 primitive validator matrix and cost-zero
sync topology remain.

The focused pinned network case passed in 17.04 seconds and then in 6.25 seconds
inside the complete pinned lane. That lane also passed IFAC in 5.97 seconds,
proof ordering in 1.27 seconds, propagation sync/ack in 2.34 seconds, and the
primitive stamp matrix in 0.33 seconds. The complete current lane passed five
LXMF cases, including both direct directions, propagation sync/ack, the
primitive matrix, and network admission, in 36.59 seconds; IFAC passed in 5.88
seconds and proof ordering in 1.29 seconds. The root library ran 1,215 tests:
1,202 passed and 13 intentional live or measurement helpers were ignored. The
complete integration/doc matrix, all-target Clippy with warnings denied,
workflow-security verification, and the quick release gate passed, including
standalone omenchatd relocation and focused tests. Formatting, shell/Python
syntax, and `git diff --check` passed. No scheduled CI or native Windows/macOS
result is inferred from these local Linux runs.

## Phase 9 unit 26: pinned/current ticket lifecycle compatibility

The release-blocking pinned lane and informational current-package lane now run
the same isolated LXMF ticket matrix. Rust creates random 16-byte ticket and
message-ID material under an explicit temporary root and uses the production
codec to calculate the ticket stamp. Python LXMF must accept that stamp using
the same ticket, reject it with a different ticket, and preserve the 16-byte
contract. Reusable bytes are passed only in bounded fixture files; stdout and
failure summaries contain boolean checks, versions, and sizes, never ticket
material.

The matrix then exercises the upstream `LXMRouter` lifecycle directly: default
three-week issue window, reuse while validity exceeds the two-week renewal
threshold, renewal below that threshold, one-day delivery throttling,
remembered outbound use, exact expiry retention, expired outbound rejection,
active-only inbound selection, and cleanup after the five-day grace period.
The pinned sources are immutable Reticulum
`15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF
`727830cefda83d9c6e3982b48675425f3f988f9c`; their modules report 1.2.2 and
0.9.6. The informational lane installs exact RNS 1.3.8, LXMF 1.0.1, and
NomadNet 1.2.7 and records `ticket_issue_use_expiry_reuse` in its JSON report.

This closes primitive ticket stamp and lifecycle-boundary compatibility, not a
live ticket exchange. OMEN already emits the canonical field, extracts and
durably retains received tickets, rejects invalid/expired tickets, prefers a
valid ticket over direct proof-of-work, and bounds the private SDK ticket cache.
OMEN does not yet implement Python's issuer-side reuse/renewal/delivery-throttle
cache: an explicit include-ticket send still creates fresh ticket material.
That semantic gap, live network exchange, direct required-stamp admission, and
automatic policy refresh remain separate work.

The final focused pinned case passed in 0.76 seconds. The complete current lane passed
six LXMF cases in 20.77 seconds; its IFAC and link/proof cases passed in 5.90 and
1.28 seconds. No production code, dependency, protocol/schema version,
configuration, user state, or queue/cache limit changed. Rollback removes the
fixture, two ignored tests, runner/report entries, and these documentation
updates. The root library matrix ran 1,217 tests: 1,202 passed and 15 explicit
live/measurement helpers were ignored. The 23 non-live ticket tests, all-target
desktop-product Clippy with warnings denied, formatting/syntax checks, complete
pinned runner, current drift runner/report, and quick release check (including
standalone omenchatd relocation and focused tests) all passed.

## Phase 9 unit 27: live Rust-issued/Python-used ticket round trip

Both Python lanes now exercise tickets across real Reticulum links. OMEN builds
and signs a direct LXMF message through its production codec with
`include_ticket`; the normal link packet sender transmits it to an isolated
Python `LXMRouter`. Python must validate the Rust signature, recognize the
canonical ticket field, and retain that ticket as outbound state for the exact
Rust delivery destination. It then sends a direct reply through its real router,
which must apply the retained ticket, report ticket cost, and receive a packet
proof from Rust.

Rust receives the reply on an inbound link, parses it with `lxmf-wire` 0.9.5,
requires the Python source/destination identities to match, verifies the Python
signature, and independently calculates
`truncated_hash(issued_ticket || reply_message_id)`. The received stamp must be
byte-exact before the normal verified-wire decoder produces application state.
Only message identifiers and boolean results cross the child-process output;
the generated ticket remains inside Rust memory, encrypted link data, and
Python's isolated temporary store. It is never a command-line argument or log
field.

The current RNS 1.3.8/LXMF 1.0.1 case passed inside a seven-case LXMF lane;
the final complete run took 30.02 seconds. The pinned Reticulum 1.2.2/LXMF
0.9.6 case passed in 0.76 seconds inside the final complete lane. This closes the live network ticket-exchange
gap for one direct software topology. It does not add OMEN issuer-side ticket
reuse, renewal, or one-day delivery throttling; an explicit include-ticket send
still generates a fresh ticket. It also does not prove ticket behavior through
propagation nodes, Resources, restart, mixed 0.6/0.9 peers, or physical links.

No production code, dependency, configuration, protocol/schema version, or
state location changed. Rollback removes the Python fixture, the two ignored
tests, runner/report entries, and these documentation updates while leaving the
unit-26 primitive/lifecycle matrix intact.

One earlier complete pinned invocation timed out while waiting for the existing
second-message propagation-stamp rejection, before it reached the ticket case.
The focused propagation case immediately passed in 24.54 seconds, and the next
complete pinned run passed the same case in 11.32 seconds plus both ticket
matrices. No timeout or retry was added to hide that observed timing variance.
The root library matrix ran 1,219 tests: 1,202 passed and 17 explicit live or
measurement helpers were ignored. All-target desktop-product Clippy with
warnings denied, formatting and syntax checks, the final pinned/current lanes,
and the quick release gate passed, including standalone omenchatd relocation
and focused server tests.

## Phase 9 unit 28: integrated issuer-side ticket lifecycle

The integrated clean runtime now owns issuer-side ticket policy at its managed
Reticulum storage boundary. A requested ticket is generated as 16 random bytes
with the Python-compatible three-week lifetime, reused while more than two
weeks remain, renewed near expiry, and included no more than once per day for a
normalized peer. The interval is persisted before transport dispatch and is
therefore deliberately an attempted-inclusion throttle, not peer-delivery
evidence. Concurrent runtime instances in one process share a serialized
decision boundary, preventing duplicate issuance against the same state file.

State is stored in `omen_lxmf_issued_tickets.json` under the identity-scoped
managed Reticulum root. The versioned file is capped at 256 peers and 128 KiB,
evicts the oldest attempted inclusion, rejects malformed, oversized,
unsupported-version, non-regular, and symlinked state, and is atomically
replaced only after flush/sync. Unix roots/files use private permissions.
Filesystem work runs in `spawn_blocking` behind a two-job semaphore. Corrupt
state is never silently replaced, and ticket bytes do not enter runtime debug
messages or application fields.

The embedded SDK wire adapter accepts the issuer's exact bytes and expiry and
rejects wrong-sized or expired overrides. A throttled send clears the effective
include-ticket option before encoding. Summary metadata distinguishes request
from actual inclusion and reports only `included_new`, `included_reused`,
`suppressed_interval`, or `not_requested`. External SDK/RPC mode reports
`delegated_external_runtime` and continues delegating issuance to
`reticulumd`; it does not claim that a ticket was observed on the wire.

No dependency, public configuration key, OMENchat wire version, database
schema, cache version, destination aspect, or existing identity/message format
changed. The only new durable artifact is the private internal issuer file.
Rollback removes the issuer module/runtime field, exact-ticket builder wrapper,
summary annotations, and this documentation; existing received-ticket message
storage and the unit-27 live codec interoperability remain intact. Deleting the
new issuer file during rollback affects only future reply-ticket reuse, not
identities or message history.

The focused native-LXMF matrix passed 84 tests with four explicit live helpers
ignored before the final capacity test was added. The final complete library ran
1,225 tests: 1,208 passed and 17 explicit live/measurement helpers were
ignored. The complete integration-test matrix passed. Warning-denied
desktop-product Clippy, formatting, and diff checks passed. The release-blocking
pinned lane passed IFAC in 5.97 seconds, proof ordering in 1.27 seconds,
propagation sync in 2.31 seconds, primitive stamps in 0.32 seconds,
network-facing stamp admission in 34.82 seconds, ticket lifecycle in 0.67
seconds, and live ticket round trip in 0.78 seconds. The informational current
RNS 1.3.8/LXMF 1.0.1/NomadNet 1.2.7 lane passed IFAC in 5.93 seconds, proof
ordering in 1.25 seconds, and all seven LXMF cases in 22.80 seconds. The quick
release gate passed product/version/dependency identity, real-PTY shutdown,
focused OMENchat checks, and standalone omenchatd relocation/build/tests. No
native Windows/macOS, physical-link, propagation-ticket, Resource-ticket, or
mixed-0.6/0.9 issuer result is inferred.

## Phase 9 unit 29: bounded direct-stamp admission

The canonical integrated sender now turns an authenticated `lxmf.delivery`
announce policy into direct-message proof work at an explicit product boundary.
A valid remembered reply ticket takes precedence. Otherwise, required costs 1
through 8 are generated with at most 65,536 attempts in one of two blocking
jobs. Runtime shutdown cancels permit acquisition and cooperatively interrupts
generation. Missing/legacy policy preserves the prior unstamped compatibility
send, while malformed policy and required costs above 8 fail locally with an
explicit unsupported-policy error. External SDK/RPC mode remains delegated to
`reticulumd`.

The SDK wire adapter applies the stamp before signing and returns non-secret
cost/value/attempt metadata. The integrated message summary records only that
evidence and never reusable ticket bytes. Deterministic tests cover stamp
validity, cancellation, ticket precedence, the cost ceiling, two-job fairness,
permit release, and shutdown ownership. No dependency, feature, configuration
key, protocol byte, database schema, state path, or default changed.

Both isolated Python lanes now prove receiver admission across a live IFAC TCP
Reticulum link. Python advertises a required cost of 1 and enforces stamps.
OMEN sends a production-signed stamped message followed by an otherwise valid
unstamped control; pinned LXMF 0.9.6 and current LXMF 1.0.1 each invoke exactly
one delivery callback. The pinned run generated value 3 in two attempts and
completed path/link setup plus both sends in 304 ms. The current run generated
value 2 in one attempt and completed in 311 ms. These are low-cost
interoperability observations, not high-cost performance claims.

`scripts/run-pinned-python-reticulum.sh` selects the release-blocking test, and
the current drift runner's existing `current_python_lxmf` filter selects its
counterpart and reports `direct_stamp_accept_reject`. The complete pinned lane
and the current RNS 1.3.8/LXMF 1.0.1/NomadNet 1.2.7 lane passed. Rollback removes
the direct proof metadata/builder path, integrated admission gate, Python
fixture/tests, runner entries, and this documentation together; issuer and
received-ticket state remain wire-compatible. Remaining work is automatic
policy refresh/retry, high-cost measurement and user policy, propagation
tickets, Resource-sized stamped delivery, mixed 0.6/0.9 behavior, and native or
physical-link evidence.

Final validation passed the nine non-live direct-stamp tests, the complete
desktop-product library suite (1,212 passed, 19 explicit live/measurement
helpers ignored), and the complete integration-target matrix. Warning-denied
all-target Clippy, formatting, shell syntax, Python fixture compilation, and
diff checks passed. The quick release gate passed product/version/dependency
identity, real-PTY shutdown, focused OMENchat checks, and standalone omenchatd
relocation/build/tests.

## Phase 9 unit 30: first-send direct policy discovery

Inspection established that the integrated link sender receives Reticulum
packet/resource completion and packet proofs, but Python performs stamp
admission later inside `LXMRouter`. No peer rejection event returns on this
path. Retrying automatically after proof timeout or silence would therefore
risk sending a second copy of a message that the peer accepted. This unit does
not add such a retry.

Instead, the canonical integrated sender now resolves missing direct policy
before constructing the first wire message. It subscribes before identity/path
resolution, consumes only a matching authenticated announce, and requests the
path once if policy is still absent. The event-driven wait uses the configured
request timeout with an absolute five-second ceiling and runtime-shutdown
cancellation. Authenticated empty app data is retained in the existing bounded
cache as explicit legacy/unknown policy; it is no longer conflated with no
announce. A matching announce whose raw app data exceeded the existing 4 KiB
admission ceiling fails closed. Unrelated events are ignored and event lag
rechecks the bounded cache.

Message metadata records only
`cached_authenticated_announce`, `discovered_authenticated_announce`,
`refreshed_authenticated_announce`, `refresh_timeout_unknown`,
`not_applicable_propagated`, or `delegated_external_runtime`. Raw app data,
ticket bytes, identities, and endpoints are not copied into this field. There
is no dependency, feature, configuration, protocol, schema, state-path, or
default change.

Both Python lanes exercise the complete application boundary. The test starts
the integrated runtime, deliberately removes cached policy, then calls
`send_message`. The sender must discover authenticated cost 1 before encoding;
Python accepts that stamped message and rejects an unstamped control without a
second callback. Pinned RNS 1.2.2/LXMF 0.9.6 completed the first-send/control
case in 2.751 seconds, and current RNS 1.3.8/LXMF 1.0.1 completed it in 2.780
seconds. These times include control delivery and Python's bounded rejection
observation, not only policy lookup.

Rollback removes the event-driven resolver, explicit empty-policy caching,
policy-source metadata, application-boundary tests, runner entries, and this
documentation together. The unit-29 bounded stamp generator and ticket
lifecycle remain compatible. Remaining work is authoritative peer rejection
or an upstream delivery event that can safely trigger one idempotent refresh,
high-cost user policy, propagation tickets, Resources, mixed-version behavior,
and native/physical interface evidence.

Final validation passed the deterministic policy/cache tests, the complete
desktop-product library suite (1,213 passed, 21 explicit live/measurement
helpers ignored), and the complete integration-target matrix. Warning-denied
all-target Clippy, formatting, shell syntax, current-lane report validation,
and diff checks passed. The complete pinned lane and nine-case current LXMF
lane passed. The quick release gate passed product/version/dependency identity,
real-PTY shutdown, focused OMENchat checks, and standalone omenchatd
relocation/build/tests.

## Phase 9 unit 31: stamped direct Resource delivery

The canonical integrated sender already delegated packet-versus-Resource
selection to `reticulum-rs-transport` 0.9.5. This unit makes the large-message
boundary observable without changing the LXMF wire: a Resource submission keeps
its hash under the existing bounded direct-correlation map, emits a project-owned
outbound offer with zero received bytes and the exact signed-wire total, then
maps transport completion, failure, or cancellation to terminal lifecycle and
releases the correlation. Resource completion remains explicitly
peer-unconfirmed.

Inspection and the first live run established that upstream incremental
`ResourceEventKind::Progress` is receiver-side; outbound senders expose offered
and terminal state. OMEN therefore does not fabricate sender percentages. The
existing 16 MiB wire and 8 MiB scalar decoder limits remain unchanged. The
application still lacks an upstream Resource handle with which to initiate
cancellation; it does handle cancellation/failure terminal events and tests
that they release ownership.

An isolated Python fixture enforces direct stamps, receives a deterministic
64 KiB ASCII body through a Link Resource, and checks body size/SHA-256, source
signature, and stamp validity without printing payload or secrets. The final
current RNS 1.3.8/LXMF 1.0.1 case completed in 1.777 seconds and the pinned RNS
1.2.2/LXMF 0.9.6 case in 1.779 seconds. No dependency, feature, configuration,
protocol, schema, state path, or default changed.

Rollback removes the offer helper, Python fixture/tests, runner entries, and
this documentation together. The underlying upstream Resource route and prior
terminal correlation remain wire-compatible. Remaining work includes an
application-owned cancellation surface when upstream exposes a safe handle,
mixed 0.6/0.9 large-message behavior, propagation tickets, high-cost stamp user
policy, and native/physical-link evidence.

Final validation passed the deterministic offer/terminal tests, the complete
desktop-product library suite (1,214 passed, 23 explicit live/measurement
helpers ignored), and all integration targets. Warning-denied all-target
Clippy, formatting, shell syntax, Python fixture compilation, diff checks, and
the quick release gate passed. The complete release-blocking pinned lane and the
10-case current LXMF lane passed. One earlier current-lane run saw an unrelated
propagation link-activation timeout; the isolated 10-case rerun and subsequent
complete lane both passed, so this remains recorded test flakiness rather than a
Resource regression.

## Phase 9 unit 32: mixed 0.6.0-1/0.9.5-1 direct applications

A new Linux multi-process harness now exports the hardened 0.6 application from
immutable commit `5ba6683055fb6c59111919fbad1ac37f56a4c203`, builds it from its
own lockfile, and runs it beside the current 0.9.5-1 binary. The 0.6 family never
enters a current production dependency tree. Each process owns a separate
temporary application, identity, config, storage, and Reticulum root. Exact
Python RNS 1.3.8 supplies an isolated loopback transport with fixed public IFAC
fixture credentials passed through an owner-only file.

After a bounded announce warmup, both real application commands send one direct
Link-packet message and require one reciprocal peer-bound inbound message. The
harness validates application versions, direct method, classification, inbound
count, peer matching, and the established 32-byte title/102-byte content shape.
The gate now uses the unit-34 directional topology for every case: it starts a
receive-only peer before each sender and attempts each logical message exactly
once. This removes flaky simultaneous cross-link activation without hiding an
ambiguous send behind a retry. The final local shared-target run passed both
directions in 40.081 seconds. The 0.9.5 side observed a matching RNS
packet proof and the 0.6 side did not, which remains conservative transport
evidence rather than a delivery-state mismatch.

Only a redacted JSON summary can be retained. It contains versions, immutable
old commit, gateway version, attempt/timing, direction results, and proof
booleans—no identities, destination hashes, paths, payloads, or credentials.
The scheduled Python interoperability workflow runs the case as a gate and
uploads that summary for 14 days. No dependency, feature, runtime default,
protocol, schema, or persistent path changed.

Rollback removes the harness, workflow job, and documentation together. The
application implementations remain untouched. Resource-sized delivery is
covered by the following unit; propagation, restart, state reopening, OMENchat
processes, native platforms, and physical interfaces remain pending and are not
inferred from this result.

## Phase 9 unit 33: mixed direct Resource applications

The unit-32 harness now supports a separate `--resource` case without adding a
production command, feature, or dependency. It copies the immutable hardened
0.6.0-1 source and the current working source into isolated temporary roots and
applies the same reviewed one-line test-driver patch to each. Only the existing
live-interop diagnostic body changes, from its normal 102-byte message to
65,536 ASCII bytes. The patch does not touch either runtime adapter, manifest,
lockfile, protocol, identity, configuration, or storage implementation.

Both versions declare a 431-byte direct Link-packet MDU and select their
existing `send_resource` branches for larger signed wires. Across exact Python
RNS 1.3.8 loopback transport, the final directional shared-target run passed on
one logical send per direction in 40.072 seconds. Each process sent direct and
decoded exactly one peer-bound reciprocal message with a 32-byte title and all
65,536 content bytes. Neither sender observed a packet proof, so the result is
reported as reciprocal application admission and content-length preservation,
not proof-derived delivery or sender-side Resource progress.

The scheduled mixed-application job now gates both small Link-packet and large
Resource cases and retains two redacted summaries for 14 days. No payload,
identity, destination hash, private path, or credential is retained. Rollback
removes the fixture patch, `--resource` harness mode, second workflow command,
and these documentation entries together. Mixed propagation, restart/state
reopening, OMENchat mixed processes, current native platforms, and physical
interfaces remain pending.

## Phase 9 unit 34: mixed restart and state reopening

The mixed-application harness now has an explicit `--restart` case. Its first
prototype exposed a real topology problem in the test: simultaneous reciprocal
link opens could let the current peer deliver while the 0.6 peer timed out link
activation. Increasing retries would risk sending a second logical message
after an unobserved delivery. The finalized case instead tests one direction at
a time, starting a receive-only peer before its sender and attempting each
logical message exactly once.

After the first current-to-old and old-to-current exchanges, all four processes
have exited. Both applications then reopen the same isolated application,
identity, configuration, storage, and Reticulum roots, perform a receive-only
announce warmup, and repeat the two directional exchanges. The final run passed:
the initial round completed in 40.074 seconds and the reopened round in 40.067
seconds. Both local LXMF destinations were unchanged. Each direction admitted
exactly one 32-byte-title/102-byte-content message, each inbound message ID
matched its peer's outbound ID, and all second-round outbound/inbound IDs were
new. The retained report records only booleans, counts, timings, versions, and
the immutable old commit—not identifiers, destinations, payloads, paths, or
credentials.

No manifest, dependency, production source, protocol, schema, configuration
default, or state path changed. The workflow now gates small direct, Resource,
and restart cases and uploads three redacted reports. Rollback removes the
`--restart` mode, workflow command, and documentation together. SQLite
conversation-history reopening, abrupt process termination, mixed propagation,
OMENchat mixed processes, native platforms, and physical interfaces remain
pending.

## Phase 9 unit 35: current-to-old mixed propagation

A separate Linux harness now exercises the real current `0.9.5-1` application
as a propagated sender and the immutable hardened `0.6.0-1` application as the
recipient. Exact Python RNS 1.3.8/LXMF 1.0.1 supplies both an isolated loopback
transport and propagation node. A bounded announce warmup runs before the
single send attempt so readiness does not become a retry policy.

The passing run submitted one propagated message, observed exactly one node
transient, and then reopened the old application with the same isolated
identity for an explicit one-message sync. The old application authenticated
the current sender, decoded the established 32-byte-title/102-byte-content
shape, and acknowledged the transient; the node reported zero remaining
entries. The retained summary contains only public versions, the immutable old
commit, counts, and validation booleans. Raw identifiers, destinations,
payloads, paths, credentials, and all temporary state are deleted.

No manifest, lockfile, production Rust source, protocol, schema, configuration
default, or persistent path changed. CI gates the case beside direct, Resource,
and restart mixed-version evidence. Rollback removes the Python fixture,
harness, workflow step, and these documentation entries together. The reverse
old-to-current propagation direction, propagation-node restart, ticket/stamp
interaction, SQLite conversation-history reopening, OMENchat mixed processes,
native platforms, and physical interfaces remain pending.

## Phase 9 unit 36: old-to-current mixed propagation and restore ordering

The mixed propagation harness now supports `--reverse`. The immutable hardened
`0.6.0-1` application submits one propagated message to exact Python RNS
1.3.8/LXMF 1.0.1 and exits. The first current `0.9.5-1` sync intentionally
defers the unknown sender, emits one bounded sender-path request, decodes no
message, and acknowledges nothing. A fresh authenticated announce is then
learned without retransmitting the logical message. The next current process
decodes exactly one retained transient and acknowledges it, leaving zero node
entries.

This case exposed an application restart race: Reticulum path-table restoration
was spawned asynchronously, so propagation decode could run before restored
sender identities reached the strict authentication cache. The transport handle
now owns a watch-based completion signal from that existing restore worker, and
propagation sync awaits it. There is no polling, sleep, timeout increase, or
validation weakening. A stopped restore worker fails explicitly. Two focused
tests cover blocking until completion and failure on premature worker closure.

Both mixed propagation directions pass with exactly one send and one admitted
message. A single retry is permitted only for link activation failure before
any payload admission; the completed reverse run needed one final sync attempt.
CI runs and retains both redacted direction reports. No dependency, protocol,
schema, configuration default, state path, or wire behavior changed. Rollback
removes the restore signal/wait, its tests, reverse harness mode, CI step, and
these documentation entries together. Propagation-node restart, ticket/stamp
interaction, SQLite conversation-history reopening, OMENchat mixed processes,
native platforms, and physical interfaces remain pending.

## Phase 9 unit 37: orderly propagation-node restart

The mixed propagation harness now supports a separate `--node-restart` case in
the proven current-to-old direction. The current `0.9.5-1` application submits
one logical message. Exact Python RNS 1.3.8/LXMF 1.0.1 reports one queued
transient and exits cleanly. A second Python process reopens the same isolated
LXMF storage with the same deterministic router identity on a new ephemeral
loopback port.

The restarted node reported one restored entry and the same propagation
destination. The immutable hardened `0.6.0-1` recipient then authenticated and
decoded exactly one expected message, acknowledged it, and reduced the node
queue to zero. The sender was not retried. The retained report contains only
versions, counts, and validation booleans; identities, transient IDs, payloads,
paths, credentials, and node storage are deleted.

No production Rust source, dependency, protocol, schema, configuration default,
or state path changed in this unit. CI runs the case beside both propagation
directions. Rollback removes the fixture phase flag, harness mode, CI step, and
these documentation entries together. Abrupt termination/power-loss durability,
propagation ticket/stamp interaction, SQLite conversation-history reopening,
OMENchat mixed processes, native platforms, and physical interfaces remain
pending.

## Phase 9 unit 38: abrupt propagation-node process recovery

The mixed propagation harness now has a separate `--node-crash` case. The
current `0.9.5-1` application submits one logical message to exact Python RNS
1.3.8/LXMF 1.0.1. After the node reports one transient, the fixture requires
its isolated LXMF storage snapshot to differ from the pre-queue baseline,
remain stable across bounded samples, and contain nonzero bytes. The harness
then sends `SIGKILL` only to that recorded temporary fixture PID.

A new Python process reopens the same storage on a new ephemeral loopback port.
It reports the same propagation identity and exactly one restored transient.
The immutable hardened `0.6.0-1` recipient authenticates and decodes one
expected message, acknowledges it, and leaves zero entries. The sender is never
retried. Retained evidence contains only versions, counts, and booleans; raw
identifiers, payloads, credentials, paths, and node storage are deleted.

This proves isolated process-crash recovery after observed settled application
storage. It does not claim `fsync`, physical power-loss, filesystem, kernel, or
storage-device durability. No production Rust source, dependency, protocol,
schema, configuration default, or state path changed. CI runs the case beside
the orderly restart and both direction tests. Rollback removes the fixture
storage-settle signal, crash harness mode, CI step, and these documentation
entries together. Propagation ticket/stamp interaction, SQLite conversation
history reopening, OMENchat mixed processes, native platforms, and physical
interfaces remain pending.

## Phase 9 unit 39: mixed propagation stamp and ticket carriage

The mixed propagation harness now has a separate `--stamp-ticket` case in the
current-to-old direction. Exact Python RNS 1.3.8/LXMF 1.0.1 enables propagation
stamp enforcement and advertises its effective positive cost. The current
`0.9.5-1` application generates bounded proof work at that exact cost, exposes
the non-secret cost/value/attempt count in its diagnostic report, and includes
one fresh reply ticket in the encrypted LXMF message.

Python queue admission proves the stamped transient passed its enforcement
boundary. The immutable hardened `0.6.0-1` application then syncs and decodes
exactly one message, recovers a correctly shaped 16-byte ticket, acknowledges
the transient, and leaves the node queue empty. The retained report contains
only versions, counts, and validation booleans; ticket/stamp bytes, identities,
message IDs, payloads, credentials, paths, and temporary state are deleted.

The clean runtime now returns generated propagation-stamp metadata from its
submitter to the project-owned message summary, matching the existing legacy
adapter diagnostic fields. It also marks a successfully embedded issued ticket
as offered. These are diagnostic/QOL fields only; no wire protocol, dependency,
schema, configuration default, or state path changed. Rollback removes those
summary annotations, the fixture policy flag, harness mode, CI step, and these
documentation entries. Propagated reply-ticket use, SQLite conversation-history
reopening, OMENchat mixed processes, native platforms, and physical interfaces
remain pending.

## Phase 9 unit 40: mixed OMENchat SQLite history reopening

The pending SQLite-history item belongs to integrated OMENchat, not LXMF's
authoritative JSON thread store. A network-free compatibility harness now builds
a small probe against each application's canonical `desktop-product` graph and
uses only the public `SqliteChatStore` boundary against one isolated database.

The immutable hardened `0.6.0-1` probe creates server, room, active-room, and
event state. Current `0.9.5-1` reopens and verifies it before appending a second
event. Old then reopens the current write and appends a third event. Current
performs the final reopen and requires all three event identifiers, bodies, and
metadata in order. This proves current reads old writes and old reads current
writes without direct SQL fixture manipulation.

The helper refuses an empty/symlink root; the harness always supplies a fresh
temporary root and deletes the database and archived old source. Retained
evidence contains versions, counts, and booleans only. No production runtime,
dependency, wire protocol, database schema, configuration default, or state
path changed. Rollback removes the example probe, harness, CI step, and these
documentation entries. Live mixed OMENchat processes, history Resource transfer,
SQLite crash durability, native platforms, and physical interfaces remain.

## Phase 9 unit 41: current client to hardened old OMENchat server

The first live mixed-process OMENchat direction now uses the current canonical
`0.9.5-1` desktop product as client and the standalone server built with
`--locked` from immutable hardened `0.6.0-1` commit `5ba6683`. A wrapper around
the existing release smoke gives both processes separate explicit temporary
application, identity, configuration, and Reticulum roots and connects them
only through an ephemeral loopback TCP interface.

The passing run required typed evidence for runtime start, link open, session
open/authentication frames, room join, message submission, and the echoed room
event. The retained JSON contains only public versions, the immutable old
commit, and stage-validation booleans. Raw identities, destinations, payload,
port, paths, logs, and all temporary state are deleted. The scheduled mixed
workflow runs this as its tenth release-blocking compatibility case.

No production Rust source, dependency, wire protocol, database schema,
configuration default, or persistent path changed. Rollback removes the live
harness, workflow step/report path, and these documentation entries together.
The reverse old-client to current-server direction, server restart/reconnect,
history Resource transfer, pinned-Python OMENchat transport, native platforms,
and physical interfaces remain pending.

## Phase 9 unit 42: hardened old client to current OMENchat server

The reciprocal live direction now builds the immutable hardened `0.6.0-1`
canonical desktop product as client and current `0.9.5-1` standalone omenchatd
as server. The existing mixed live wrapper gained an explicit `--reverse`
mode; it still delegates protocol exercise to the release smoke and verifies
the selected binary versions before starting either process.

The passing run required runtime start, link open, session
open/authentication frames, room join, one message submission, and the echoed
room event. Together with unit 41, this proves reciprocal single-session
message compatibility between the hardened releases. CI runs and retains a
separate redacted report for each direction, bringing the mixed-application
matrix to eleven cases.

Both directions use fresh separate temporary roots and ephemeral loopback
ports. Raw identities, destinations, payloads, ports, paths, logs, and state
are deleted. No production Rust source, dependency, wire protocol, database
schema, configuration default, or persistent path changed. Rollback removes
the `--reverse` mode, reciprocal workflow step/report, and these documentation
entries together. Mixed server restart/reconnect, history Resource transfer,
pinned-Python OMENchat transport, native platforms, and physical interfaces
remain pending.

## Phase 9 unit 43: current-client state reopen after old-server restart

The mixed live wrapper now has an explicit `--restart` mode in the proven
current-client to hardened-old-server direction. Its underlying release smoke
first completes the normal link/session/join/message/echo exchange, stops the
server within a 20-second deadline, and reopens the same server home on the
same ephemeral loopback interface. The restarted server must expose the exact
same destination before client work resumes.

A fresh current `0.9.5-1` client process reuses the original isolated
application root and completes a second full exchange. Both stage reports are
validated, but only public versions and booleans are retained. Hardened
`0.6.0-1` predates the current owned SIGTERM drain path and returns the expected
signal status; the harness classifies that as a bounded signal stop rather
than claiming orderly shutdown.

No production Rust source, dependency, protocol, schema, configuration
default, or persistent path changed. CI runs this as the twelfth mixed-release
gate. Rollback removes `--restart-server`, the mixed `--restart` mode, workflow
step/report, and these documentation entries together. A continuously running
desktop's automatic reconnect, the reciprocal old-client to current-server
restart, history Resource transfer, pinned-Python OMENchat transport, native
platforms, and physical interfaces remain pending.

## Phase 9 unit 44: old-client state reopen after current-server restart

The reciprocal restart case combines the existing `--reverse` and `--restart`
modes. Hardened `0.6.0-1` first completes a client exchange with current
`0.9.5-1` omenchatd. The current server then receives SIGTERM, must exit through
its owned orderly drain path within the 20-second deadline, and reopens the
same isolated server home/interface with an unchanged destination.

A fresh old-client process reuses its original isolated application root and
completes the second link/session/join/message/echo exchange. The mixed wrapper
now enforces direction-specific shutdown evidence: `sigterm` only for the
legacy old server and `orderly` only for current omenchatd. The retained report
contains those public classifications and booleans but no identities,
destinations, payloads, ports, paths, PIDs, logs, or state.

No production Rust source, dependency, protocol, schema, configuration
default, or persistent path changed. CI runs this as the thirteenth
mixed-release gate. Rollback removes the direction-specific stop assertion,
reciprocal workflow step/report, and these documentation entries together.
Automatic reconnect by a continuously running desktop, mixed history Resource
transfer, pinned-Python OMENchat transport, native platforms, and physical
interfaces remain pending.

## Phase 9 unit 45: current-client history Resource from old server

The live mixed wrapper now has a separate `--history-resource` case in the
current-client to hardened-old-server direction. It sets only the isolated
server's large-batch threshold to one byte and runs the existing multi-client
release smoke with a normal small message. This forces the production OMENchat
history path to choose a Resource without changing a committed default or
depending on an oversized room-message frame.

The second current `0.9.5-1` client uses a distinct application/identity root.
Its report must contain a `resource_data` event whose decoded children include
`history_prepended`, and the existing smoke assertion requires the exact first
client message in that second report. The passing retained report contains only
public versions, the immutable old commit, the isolated threshold, and
validation booleans. Raw Resources, messages, identities, destinations, ports,
paths, logs, and state are deleted.

No production Rust source, dependency, wire protocol, database schema,
configuration default, or persistent path changed. CI runs this as the
fourteenth mixed-release gate. Rollback removes the release-smoke threshold
option, mixed `--history-resource` mode, workflow step/report, and these
documentation entries together. The reciprocal old-client to current-server
history Resource direction, automatic reconnect by a continuously running
desktop, pinned-Python OMENchat transport, native platforms, and physical
interfaces remain pending.

## Phase 9 unit 46: old-client history Resource from current server

The reciprocal history-Resource case combines `--reverse` with the established
`--history-resource` mode. Current `0.9.5-1` omenchatd uses the one-byte
large-batch threshold only in its isolated temporary configuration. Two
hardened `0.6.0-1` clients use distinct roots: the first sends a normal small
message, and the second must receive `resource_data`, decode
`history_prepended` within that event, and observe the exact first-client
message.

The live case passed without a production code change. Together with unit 45,
both mixed application directions now consume Resource-backed OMENchat history.
The retained report contains only public versions, the immutable old commit,
the isolated threshold, and validation booleans; all raw Resources, messages,
identities, destinations, ports, paths, logs, and state are deleted.

No production Rust source, dependency, wire protocol, database schema,
configuration default, or persistent path changed. CI runs this as the
fifteenth mixed-release gate. Rollback removes the reciprocal workflow
step/report and these documentation entries; the shared harness remains useful
for unit 45. Automatic reconnect by a continuously running desktop,
pinned-Python OMENchat transport, native platforms, and physical interfaces
remain pending.

## Phase 9 unit 47: continuous current-product OMENchat reconnect

The release smoke now has an explicit `--continuous-client-reconnect` mode and
the product CLI has a bounded coordination option used only when that mode is
requested. After the first session/join/message/echo exchange, the client writes
a create-new marker inside the isolated harness root and remains alive. Current
omenchatd then performs its owned orderly shutdown and restarts from the same
home with an unchanged destination.

The same `0.9.5-1` client process must observe the old link's typed close event,
open a different link identifier within six bounded attempts and 75 seconds,
invoke the shared `reconnect_live_server` state transition for the existing
session, and receive a second echoed message. The live harness passed. Its
retained report contains only public versions and validation booleans; all raw
reports, marker, identities, destinations, payloads, ports, paths, logs, and
state are deleted.

This changes no OMENchat wire byte, dependency, schema, normal configuration
default, or desktop reconnect policy. CI runs the redacted case beside the
mixed-version matrix. Rollback removes the CLI coordination fields/helper,
release-smoke mode, wrapper, workflow step/report, and these documentation
entries. Interactive Iced-window restart soak, pinned-Python OMENchat transport,
native platforms, and physical interfaces remain pending.

## Phase 9 unit 48: current-product OMENchat upload Resource

A dedicated current-product harness now runs two canonical `0.9.5-1` clients
against current standalone omenchatd over an ephemeral loopback interface. The
first client uploads the existing public deterministic 873-byte OMENchat wire
fixture, receives typed upload completion, and fetches the resulting Resource.
A second client with a distinct application/identity root discovers the upload
through room history and fetches the same Resource.

The harness requires `upload_completed` and `upload_resource_available` events
with the exact fixture byte count for the sender plus a matching
`upload_resource_available` event for the second client. Reticulum Resource
integrity remains active. The retained report contains only public versions,
the byte count, and validation booleans; all raw payloads, Resource identifiers,
identities, destinations, ports, paths, logs, and state are deleted.

No production Rust source, dependency, wire protocol, schema, quota, normal
configuration default, or persistent path changed. CI runs the redacted case
beside current reconnect and mixed-version evidence. Rollback removes the
wrapper, workflow step/report, and these documentation entries. Upload
replacement fault durability remains covered separately; interactive native
platforms and physical interfaces remain pending.

## Phase 9 unit 49: current-product NomadNet page request

A dedicated wrapper now converts the existing isolated NomadNet portal smoke
into a scheduled, redacted release gate. Canonical `0.9.5-1` omenchatd exposes
its fixed `nomadnetwork.node` portal from a temporary server home over an
ephemeral loopback interface. The canonical browser owns a separate temporary
application, identity, Reticulum configuration, and storage root.

The live run passed link setup and the production request send, then
returned a network page whose decoded shape exactly matches the deterministic
portal: 309 markup bytes, 17 lines, `text/x-micron`, and non-empty content. The
retained report contains only public application versions, that page shape, the
request primitive, and validation booleans. Raw destinations, URLs, identities,
paths, ports, logs, and state are deleted.

No production Rust source, dependency, protocol, schema, configuration default,
or persistent path changed. CI retains the redacted report beside the current
OMENchat and mixed-version evidence. Rollback removes the wrapper, workflow
step/report, and these documentation entries. At this unit boundary Python
NomadNet request/form/resource interoperability, direct-versus-Resource
measurements, link-reuse measurements, native platforms, and physical
interfaces remained pending; unit 50 closes the current-Python small-request
portion only.

## Phase 4 unit 50: current-Python direct NomadNet requests

The production page adapter now matches Python's primitive-selection boundary:
packed requests within Reticulum packet MDU use an encrypted
`PacketContext::Request` sent directly on the active link's ingress interface;
oversized requests retain the bounded request-resource lifecycle. The response
subscription is established before dispatch and accepts only the matching link,
`PacketContext::Response`, and final packet-hash request ID. A timeout or
cancellation terminates the direct attempt without an automatic Resource retry,
avoiding duplicate executable-form actions.

An ignored-by-default fixture starts the real NomadNet 1.2.7 `Node` with RNS
1.3.8 under an isolated root and public test IFAC credentials. The production
Rust runtime fetches a static page with an empty request and an executable page
with deterministic `field_*` and `var_*` values. Both exact response byte
comparisons pass; the two sequential page exchanges completed in about 1.6
seconds locally. The informational current-Python drift report records only the
public check name and package versions.

The current-product portal wrapper now asserts `direct-request`, so both the
Python handler and current standalone portal paths exercise the active small
request primitive. omenchatd admits direct requests on its inbound link-event
stream, validates the same bounded path frame, serializes its portal read in one
owned blocking job, and responds only on the link's bound interface. No
dependency, wire format, identity/config location, schema, or normal limit
changed. Rollback restores the all-Resource selector and its metadata/report
assertion, removes the server direct handler, and removes the Python
fixture/check. Oversized
Python request-resource, timeout/cancellation, repeated-link behavior,
direct-versus-Resource measurement, native platforms, and physical interfaces
remain pending. The migration contract defines pinned Reticulum/LXMF commits
but no pinned NomadNet reference, so this unit is current-Python evidence only.

## Phase 4 unit 51: primitive-independent NomadNet responses

Request and response primitive selection are now independent, matching Python
Reticulum behavior. Before dispatch, the page adapter subscribes to both direct
received-data and Resource event streams. A direct request accepts either its
correlated `PacketContext::Response` or a response Resource; an oversized
request Resource likewise accepts either response. Correlation still requires
the exact request ID and link. Neither branch retries the request, so a lost
response cannot duplicate an executable action. Once inbound Resource progress
identifies the transfer hash, browser cancellation or timeout explicitly
cancels that response Resource before link teardown.

The current-Python fixture now runs four exact-byte cases against RNS 1.3.8 and
NomadNet 1.2.7: empty direct/direct, executable-form direct/direct, a 2,048-byte
form value sent as request Resource with a small direct response, and a direct
request returning a deterministic 5,919-byte response Resource. The test also
requires typed outbound request-Resource completion and inbound
response-Resource completion. All four sequential exchanges completed in about
1.6 seconds locally, and the complete informational drift lane passed.

No dependency, request/response encoding, schema, configuration default,
identity path, or size limit changed. Rollback removes the second response
receiver from each adapter branch, restores the two-case fixture/check name, and
reverts these evidence statements. Timeout/cancellation against Python,
repeated-link reuse, comparative performance, native platforms, and physical
interfaces remain pending. There is still no pinned NomadNet reference in the
migration contract.

## Phase 4 unit 52: current-Python NomadNet timeout and cancellation

The current-Python fixture now has a fault-only scenario with two fixed
executable pages. Each waits three seconds and emits only deterministic public
text. The production browser uses its existing two-second response deadline for
the first request. The test requires the exact direct-response timeout rather
than accepting a link-setup failure. For the second request, a bounded event
listener waits for the production `direct page request sent` event before
cancelling the caller token, so the test cannot pass by cancelling before
dispatch.

After both delayed handlers have drained, Python must report exactly two served
page requests. This proves the timeout and cancellation exits do not
automatically replay executable actions. Both links close, neither late
response becomes a page, and the isolated live case passes against RNS 1.3.8
and NomadNet 1.2.7 in about 7.1 seconds. The fault scenario shares the existing
temporary identity, Reticulum configuration, storage, and loopback isolation;
its retained drift report adds only the public
`nomadnet_timeout_cancellation_no_replay` check name.

No production timeout, retry, wire byte, dependency, schema, configuration
default, identity path, or payload limit changed. Rollback removes the two
delayed fixture pages/scenario, the ignored live test and drift check, restores
timeout/cancellation to the open capability list, and reverts these evidence
statements. Repeated-link reuse, comparative performance, native platforms,
physical interfaces, and a pinned NomadNet reference remain pending.

## Phase 4 unit 53: current-Python NomadNet successful-link reuse

Successful native page exchanges now retain their active outbound link instead
of unconditionally closing it. The existing fixed destination stripe remains
held across preparation and the complete request/response exchange, so no
second page operation can share that link while the first can still fail or
tear it down. Timeout, cancellation, and every other exchange error continue to
close the link. If a later lookup returns a stale retained handle, the adapter
closes and removes it before creating the replacement; it does not dispatch on
a stale link or automatically replay an earlier request.

The current-Python fixture adds one deterministic executable page that records
its link identifier only in a temporary file inside the isolated root. Two
sequential production fetches return exact visit-specific bytes, and Python
reports a request count of two plus a same-link boolean without exposing the
identifier. The live RNS 1.3.8/NomadNet 1.2.7 run passed on one active link. The
initial request measured 184 ms and the reused request 34 ms locally; these are
observations, not release thresholds. The retained drift report adds only the
public `nomadnet_repeated_request_link_reuse` check name.

No request/response wire byte, timeout, retry, dependency, schema,
configuration default, identity path, channel budget, or payload limit changed.
Rollback restores unconditional post-exchange link close, removes stale-link
replacement and the reuse fixture/test/report entry, and restores repeated-link
reuse to the open capability list. Comparative direct-versus-Resource
measurement, a bounded long-running keep-alive/recovery soak, native platforms,
physical interfaces, and a pinned NomadNet reference remain pending.

## Phase 4 unit 54: direct versus request-Resource measurement

The current-Python fixture now includes a reproducible comparative workload on
one retained link. It performs one direct and one request-Resource warmup, then
eight measured samples of each primitive. Pair order alternates on every
iteration to reduce ordering bias. The one-byte direct field and 2,048-byte
request-Resource field are fixed public fixtures; Python returns only the
sequential request number and field byte count. Every response byte and the
runtime's `native_request_primitive` metadata must match before a timing sample
is admitted.

Against RNS 1.3.8/NomadNet 1.2.7 in the local debug test profile, direct
requests measured 34,339 us median and 39,979 us p95. Request Resources measured
80,474 us median and 87,872 us p95. All 18 requests, including warmups, used one
active link. These observations establish the harness and relative local cost;
they are not pass thresholds or release-mode performance claims. The retained
drift report adds only the public
`nomadnet_direct_request_resource_measurement` check name.

No production source, timeout, retry, wire byte, dependency, schema,
configuration default, identity path, channel budget, or payload limit changed.
Rollback removes the measurement page/scenario, ignored test and drift report
entry, and reverts these evidence statements. A release-mode confirmation,
bounded long-running keep-alive/recovery soak, native platforms, physical
interfaces, and a pinned NomadNet reference remain pending.

## Phase 4 unit 55: bounded retained-link keep-alive and recovery soak

The current-Python fixture now runs 32 exact executable NomadNet requests while
alternating the direct and request-Resource primitives. The first 16 share one
active link and include a two-second idle interval. Python then explicitly
tears down that server-side link and emits a payload-free synchronization
marker. The production runtime must establish one replacement within an
eight-second outer bound and serve the remaining 16 requests on that generation.

The Python-owned isolated state requires exactly two link generations with 16
requests each, at most one active inbound link, no third generation, and exactly
32 served requests. Every Rust response is byte-equal and carries the expected
primitive metadata; 32 retained-success events are required. The focused RNS
1.3.8/NomadNet 1.2.7 reference run completed the exchange in 4,411 ms, including
the deliberate idle, and completed replacement plus the second 16 requests in
1,004 ms. The complete informational drift lane passed.

No production source, timeout, retry, wire byte, dependency, schema,
configuration default, identity path, channel budget, or payload limit changed.
Rollback removes the soak page/scenario/test/report entry and reverts the
capability and evidence statements. Optimized release-profile confirmation,
native platforms, physical interfaces, and a pinned NomadNet reference remain
pending.

## Phase 4 unit 56: optimized NomadNet primitive measurement

The existing exact-byte comparative fixture now has a machine-checkable
optimized invocation. When
`OMEN_REQUIRE_OPTIMIZED_NOMADNET_MEASUREMENT=1` is present, the test rejects a
build with debug assertions. The current-Python drift lane invokes that same
two-warmup, eight-sample-per-primitive, alternated workload using
`cargo test --release`; it does not maintain a second performance implementation.

The complete RNS 1.3.8/NomadNet 1.2.7 lane measured direct requests at 35,138 us
median and 40,998 us p95. Request Resources measured 78,756 us median and 86,923
us p95. All 18 requests used one link and every exact response plus primitive
classification passed before its sample was admitted. The retained drift JSON
contains only the `release` profile label, eight-sample count, same-link boolean,
and four aggregate timing values.

No production source, timeout, retry, wire byte, dependency, schema,
configuration default, identity path, channel budget, or payload limit changed.
Rollback removes the optimized invocation, explicit profile assertion, aggregate
report object, and these evidence statements. Native platforms, physical
interfaces, and a pinned NomadNet reference remain pending; the migration
contract supplies no immutable NomadNet reference to execute.

## Release qualification unit 57: native all-target preflight

The reusable Windows/macOS workflow now applies strict Clippy to every declared
target for `desktop-product`, root `tui`, standalone `server-headless`, and
standalone `server-full`. The workflow-security verifier asserts each exact
all-target command. This extends the existing checks to examples and test-only
code rather than changing any production feature identity.

A complete Windows-GNU cross-target preflight passed for bare `native-lxmf`,
`desktop-product`, root `tui`, `server-headless`, and `server-full`. Product
tests were compiled with `--no-run`; strict Clippy covered all targets. The
preflight exposed two narrow portability defects. The mixed-version SQLite
probe is now explicitly gated by its actual `chat-client` dependency, so Cargo
does not compile it for unrelated TUI profiles. The server log soak snapshot
helper now has the same Linux target gate as every caller, so Windows does not
compile unused test code.

The strengthened Linux all-target matrix also passed. Desktop-product library
tests reported 1,216 passed and 28 explicit ignores; root TUI library tests
reported 581 passed and one ignore, with 32 main-binary tests also passing;
server-headless reported 196 passed and seven ignores; server-full reported 318
passed and seven ignores. Associated integration/example tests and strict
all-target Clippy gates passed. Formatting, Actionlint, workflow security,
product-feature identity, and TUI dependency identity checks passed.

No production runtime, wire byte, dependency version, configuration, schema,
identity path, or storage behavior changed. Rollback removes the example's
required-feature declaration, the matching Linux test-helper target gate, and
the workflow/verifier all-target assertions independently. Cross-compilation is
not native execution: hosted Windows MSVC and both macOS jobs, interactive
launch/file-dialog/terminal behavior, and installer lifecycle remain release
gates.

## Release qualification unit 58: native release CLI identity smoke

The reusable native matrix now executes the actual OMENbrowser and omenchatd
entry points after their compile/test/Clippy gates. A state-free shell harness
runs `--version` for `desktop-product`, root `tui`, `server-headless`, and
`server-full`. It requires the Rust host target in both browser identities,
requires the canonical desktop product and native-network flags, rejects mock
or test leakage, and verifies the standalone server's intended headless/full
feature split. Browser and headless-server `--help` are also executed, with
stable isolated-root, startup, status, and doctor controls asserted.

The workflow-security verifier requires the named step and exact harness path.
The harness passed locally for `x86_64-unknown-linux-gnu`; the existing quick
release gate also passed. It performs no GUI/TUI launch, Reticulum startup,
identity creation, configuration load, or default-root access. No production
source, dependency, wire byte, configuration, schema, identity path, or storage
behavior changed. Rollback removes the harness, workflow step, verifier
assertions, and these documentation statements together. Hosted Windows MSVC
and macOS execution remains the completion evidence; interactive application
and installer lifecycle smokes remain separate gates.

## Release qualification unit 59: exact local-crate dependency identity

The first pull-request quick job correctly rejected both local
`omen-ifac-tcp` dependency declarations as wildcard requirements when its
pinned cargo-deny command evaluated all features. Both the root application and
standalone server now pair their existing relative path with the private crate's
exact `=0.9.5-1` package version. Cargo still resolves the same local source and
package identity; the version clause prevents an accidentally mismatched local
crate from satisfying either product manifest.

The exact CI cargo-deny commands now pass for both independent manifests, as do
locked checks for the affected native profiles and release-version consistency.
No crate source, lockfile package, runtime behavior, wire byte, configuration,
schema, identity path, or storage format changed. Rollback removes the two
version constraints together, but would restore a denied wildcard requirement.

## Release qualification unit 60: bounded quick-runner build storage

The corrected pull-request quick job passed dependency policy and then exhausted
its GitHub-hosted runner filesystem during the release quick gate. The hosted
annotation reports `No space left on device` while the Actions runner attempted
to write its own diagnostic log; no test assertion or compiler diagnostic
failed. The quick job previously restored workspace target caches for both the
root application and standalone server before compiling their overlapping
product profiles.

The quick job now retains registry and installed-tool caching but sets the
pinned rust-cache action's `cache-targets` input to `false`. It also disables
incremental compilation for that ephemeral runner. Every release command,
feature identity, test, and package-script assertion is unchanged; only retained
build artifacts are reduced. The action input was verified against the exact
pinned action source. Workflow syntax/security checks and the local quick gate
remain the focused regression gates, followed by a fresh hosted run.

No production source, dependency, lockfile, runtime behavior, wire byte,
configuration, schema, identity path, or storage format changed. Rollback
removes the environment setting and cache input together, but can restore the
observed hosted-runner disk exhaustion when both workspace targets are cached.

## Release qualification unit 61: Windows portable package boundary

The package workflow now builds the canonical desktop application and standalone
omenchatd on the native Windows 2025 MSVC runner only after the reusable native
matrix passes. A PowerShell packaging boundary verifies package-version parity
and compiled feature/target identity before creating separate unsigned ZIPs and
SHA-256 files. The browser archive does not include or start omenchatd; the
server archive installs no service and remains independently configured.

The read-only Windows builder uploads an intermediate artifact. The narrowly
privileged tag publication job checks out no repository code and now depends on
and downloads both the Linux and Windows build artifacts. Workflow policy tests
preserve native gating, runner identity, script ownership, read-only builders,
and both publication dependencies. NSIS, WiX MSI, install/upgrade/uninstall,
GUI launch, and signing remain explicit release gates; portable ZIP success does
not satisfy them.

The first hosted package execution compiled both release binaries and exposed an
invalid packaging assertion: unlike the browser, omenchatd's stable version
output does not include a target triple. The corrected boundary verifies the
native `rustc` host is exactly `x86_64-pc-windows-msvc`, retains the browser's
compiled target assertion, and checks omenchatd's version and full/live feature
identity according to its existing contract. Production output is unchanged.

No runtime crate, production source, wire byte, configuration, schema, identity
path, or storage format changed. Rollback removes the PowerShell script, Windows
job/artifact dependency, verifier assertions, and packaging documentation
together; existing Linux packaging remains independent.

The same unit closes a tag-path qualification gap found during the manual
artifact run. `run_package_smoke` is a workflow-dispatch input and is absent on
tag pushes, so the previous equality check skipped the package smoke for actual
releases. Manual runs may still disable it, while every non-manual `v*` run now
executes the isolated packaged OMENchat gate. The workflow verifier preserves
that event condition.

## Release qualification unit 62: asynchronous TUI room-test completion

Hosted Apple Silicon qualification exposed a race in one existing TUI test. The
production dashboard correctly enqueues room creation on the bounded
administrative database actor, but `dashboard_input_creates_room_and_updates_config`
read SQLite immediately after admission. It could therefore observe the old
state even though creation completed moments later. The test now uses the
existing bounded two-second actor-completion helper before asserting the exact
room name/topic. No sleep, retry in production, timeout expansion, assertion
weakening, or runtime behavior is introduced.

The focused full-server test is repeated under the same feature identity before
the native rerun. Rollback removes the single completion wait and this ledger
entry, restoring nondeterministic observation of an accepted asynchronous
operation.

## Release qualification unit 63: native Windows installer boundary

The Windows package job now pins cargo-packager 0.11.8 and produces browser-only
unsigned NSIS setup and WiX MSI artifacts in addition to the separate browser
and omenchatd portable ZIPs. The installer configuration explicitly retains
omenchatd as a separately deployed package and never installs or starts it as a
service. NSIS is current-user scoped; both formats reject downgrades. Cargo
numeric revision `0.9.5-1` maps deterministically to MSI `0.9.5.1`.

The reviewed script pre-seeds every downloaded NSIS/WiX executable archive and
plugin from an immutable release URL after checking a repository-pinned SHA-256,
including the ApplicationID plugin that cargo-packager 0.11.8 otherwise fetches
without a strong hash. It creates a bounded prior-revision fixture from the same
reviewed binary, installs it, upgrades to the current package, launches the GUI
with an explicit temporary app root, uninstalls, and requires a user-data
sentinel to survive for each format. Final artifacts must be unsigned and have
individual SHA-256 files. Hosted Windows execution remains the completion gate;
Linux can validate only script policy and workflow structure. Rollback removes
the installer script, workflow steps/artifacts, verifier assertions, and these
documents together without affecting portable packages.
