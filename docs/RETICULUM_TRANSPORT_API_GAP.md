# Reticulum Transport API Gap

OMENbrowser_rs now builds against the exact `reticulum-rs` / `lxmf` 0.9.7
train. Small NomadNet requests now use direct `PacketContext::Request` packets;
oversized packed requests retain the bounded request-resource path. Exact empty
and executable-form exchanges pass against current Python RNS 1.4.0 and
NomadNet 1.2.7.

This project intentionally uses the published crates as-is. Do not add a local
`[patch.crates-io]` override for `reticulum-rs-transport`; OMENbrowser_rs is not
the upstream transport maintainer and should not depend on private forked APIs
for normal builds.

The OMENchat client and `omenchatd` `live-reticulum` server have a clean-stack
transport path now: links are opened against `omenchat.node`, normal OMENchat
frames are sent as context-zero encrypted link data with
`rns_transport::delivery::send_on_link`, and OMENchat media/upload resources use
the public transport resource API with `omenchat-resource:` metadata. The live
server accepts context-zero data only when the payload decodes as an OMENchat
frame. This is intentionally separate from the legacy `0x4f` OMENchat packet
context because `reticulum-rs-transport` 0.6.0 does not expose an arbitrary
custom packet context variant.

The 0.9 transport library still lacks a high-level public `Link.request` helper.
OMENbrowser composes efficient small encrypted `PacketContext::Request` link
data from public primitives and sends it directly on the active link's bound
interface. No automatic Resource retry follows a direct request, because a
retry could repeat an executable form action whose response was merely lost.

The 0.9.7 crate also exposes public request/response Resource helpers.
OMENbrowser selects `Transport::send_request_resource()` only when the packed
request exceeds the Reticulum packet MDU, matching Python's primitive-selection
boundary. Current Python verifies that oversized path and also verifies that
response selection is independent: a Resource request may receive a direct
response, while a direct request may receive a large response Resource.

See `docs/UPSTREAM_RETICULUM_TRANSPORT_REQUEST.md` for the upstream-ready issue
text and local acceptance checklist.

## What Works

- `reticulum-rs-transport` can start configured TCP client interfaces.
- IFAC profile values can be passed through `InterfaceSharedConfig`.
- OMENbrowser_rs now has a project-local IFAC TCP client interface that
  implements `reticulum-rs-transport`'s public `Interface` trait and applies
  Python-compatible IFAC wire masking before HDLC framing. This is not a
  vendored crate patch.
- destination identities can be recalled after announces/path discovery.
- outbound links can be created and activated.
- inbound link data preserves context and request ids through
  `received_data_events()`.
- public transport request/response resource helpers send on
  `link.ingress_iface()`.
- public link-data packet construction plus public context mutation and
  `send_direct()` form the active small NomadNet request path; current Python
  empty and executable-form exchanges preserve exact response bytes.
- public channel helpers send on `link.ingress_iface()`, but they frame payloads
  as `PacketContext::Channel`.
- `PacketContext::LinkIdentify` exists, inbound link handling preserves it, and
  OMENbrowser can send it by building encrypted link data, marking the packet
  `LinkIdentify`, and dispatching with `Transport::send_direct()` on
  `link.ingress_iface()`.

## What Still Needs Improvement

### IFAC/private gateway support

Published `reticulum-rs-transport` 0.9.7 still exposes IFAC-related shared
configuration, but source inspection confirms that its stock TCP client and
server wire paths serialize/deserialize `Packet` values directly through HDLC.
They do not apply or verify the Python Reticulum IFAC transform. Shared IFAC
configuration is therefore metadata, not wire enforcement, for those stock
interfaces. That is why private-gateway OMENchat tests previously failed before
path/link establishment even though interface profiles showed
`ifac=configured`.

OMENbrowser_rs now handles configured IFAC TCP client profiles with a small
project-local interface implementation:

- `src/server/crates/omen-ifac-tcp/src/lib.rs` implements the public
  `rns_transport::iface::Interface` trait.
- It uses the published crate's public `Packet`, `Hdlc`, buffer, hash, identity,
  and interface-channel APIs.
- It derives the IFAC identity/key from configured `network_name` and
  `passphrase`, signs packet bytes, inserts the IFAC bytes, masks/unmasks the
  correct wire ranges, verifies inbound IFAC signatures, and then hands normal
  `Packet` values back to `reticulum-rs-transport`.
- Signature-tag verification uses `subtle`'s constant-time comparison. A
  poisoned interface configuration lock fails into one redacted terminal state
  instead of panicking or reconnecting with partially trusted state.
- Delimiter-free HDLC input retains at most 524,416 bytes between reads. One
  fixed 64 KiB read may temporarily raise the buffer to 589,952 bytes before
  frame scanning and retained-limit enforcement; larger or still-undelimited
  state is cleared. These are transport-memory ceilings, not wire changes.
- Non-IFAC TCP client profiles continue to use the stock upstream TCP client.
- omenchatd refuses an IFAC-configured stock TCP server instead of falsely
  reporting that the private gateway is enforced. Its supported IFAC topology
  remains the project-local TCP client connecting to an enforcing gateway.

The deterministic fixture in `src/server/crates/omen-ifac-tcp/src/lib.rs`
matches output generated by the exact pinned Python Reticulum reference commit.
The pinned interoperability runner verifies that immutable clean source before
testing identity/destination vectors and real TCP traffic. The supported Rust
client direction passes Python authentication, Python-to-Rust authentication,
split and coalesced HDLC framing, reconnect, and wrong-credential rejection.
The fixtures contain only public test credentials.

This keeps OMENbrowser on published crates as requested, while avoiding a
private `[patch.crates-io]` transport fork. The remaining upstream improvement
would be for `reticulum-rs-transport`'s stock TCP interfaces to apply the same
wire transform internally when IFAC config is present.

Python NomadNet page fetch uses `Link.request(path, data=...)`. Python sends
small requests as a `PacketContext::Request` link-data packet, and sends
oversized packed requests as a request resource with the truncated request hash
as the resource request id.

OMENbrowser_rs builds a Python-compatible request frame. Within the packet MDU,
it derives the request ID from the final encrypted packet hash, dispatches the
packet directly on the link's ingress interface, and accepts only a matching
`PacketContext::Response`. Above the packet MDU it sends a request Resource and
waits for a response Resource with matching correlation. The direct path is
live-verified for an empty request and field/variable form data against current
Python NomadNet; the oversized Python path is not yet qualified.

For encrypted link data, the destination is the link id. That link id is not a
normal destination path-table entry. In broadcast-enabled transport configs,
generic dispatch therefore broadcasts the active link packet instead of sending
it directly on the interface that proved the link.

The existing public `send_to_out_links()` helper is not a compatible shortcut:
it sends directly to active outbound links, but it builds packets with
`PacketContext::None`. The public channel helpers also send on the bound
interface, but use `PacketContext::Channel`. Python Reticulum's request handler
only dispatches registered NomadNet page handlers for `PacketContext::Request`,
so using `None` or `Channel` would silently bypass the server's request path.
Identify-on-connect has a narrower workaround: OMENbrowser builds the encrypted
link-data packet with `Link::data_packet()`, changes the public packet context
to `PacketContext::LinkIdentify`, and sends it directly through
`Transport::send_direct()` on the active link's ingress interface.

Earlier clean-stack logs showed why generic packet dispatch must not be used
for small requests:

- destination path known;
- link established;
- link has an ingress interface;
- link id has no path-table route;
- generic dispatch used broadcast;
- no response events arrived.

The adapter never uses generic packet dispatch for link requests. It sends the
small direct packet only through `send_direct()` on the bound interface and
subscribes before dispatch so an immediate response is not lost. Timeout and
cancellation are terminal; there is no automatic cross-primitive retry.
Oversized requests remain on the separately bounded Resource lifecycle. A
high-level upstream helper would still reduce local protocol risk.

The retained production path now owns the outbound request resource through
its terminal boundary. Browser cancellation, response timeout, or closure of
the resource event stream calls the public 0.9 `Transport::cancel_resource()`
before page-link teardown. Deterministic active-link tests require the actual
`ResourceInitiatorCancel` packet and an outbound cancelled lifecycle record;
returning a cancelled/timeout error alone is not sufficient. Response-resource
progress remains inbound, while request-resource completion, failure, and
cancellation are reported as outbound.

Successful native pages include both `native_request_backend` and
`native_request_primitive` metadata. Diagnostics therefore distinguish
`reticulum-transport/direct-request` from the oversized
`reticulum-transport/request-resource` path.

Application status also preserves the resource direction already emitted by
the adapter: outbound `nomadnet-page` lifecycle events are described as the
NomadNet request upload, while inbound progress is described as the NomadNet
response download. A zero-byte UTF-8 response remains a successful network
page and carries additive `native_response_empty=true` metadata so the browser
can distinguish a valid empty response from timeout, cancellation, and failed
transfer states. Browser tasks now create an opaque, process-local operation
identifier and pass it through the session/runtime boundary into these resource
events. The application maps only exact identifiers back to the originating
tab generation; destination hashes and event timing are never used as
correlation guesses. This identifier is application metadata and is not added
to the NomadNet or Reticulum wire payload.

The Phase 4 deterministic harness now establishes an isolated in-memory pair of
0.9 links, activates the outbound link on a synthetic interface, encrypts the
existing bounded NomadNet request frame, changes only its context to `Request`,
and derives the request ID from the final packet hash. The peer receives the
original plaintext plus that exact request ID and returns a `Response` packet
whose embedded correlation is accepted. Pending links are rejected before
packet construction. This proves the local composition and the important fact
that the direct packet request ID is not the request-resource frame ID. It does
not by itself prove Python handler dispatch or interface delivery. The separate
current-Python lane now proves both plus exact response bytes for empty and
executable-form requests, oversized request/response Resources, a bounded
response timeout, and cancellation after confirmed dispatch. The fault fixture
observes exactly one Python request per Rust operation after delayed handlers
drain, so neither exit performs an automatic retry. Two sequential executable
requests now use one active link and return exact visit-specific bytes. The
bounded 32-request soak also preserves one active link across idle time, then
recovers on exactly one fresh generation after the Python peer closes it. The
same exact workload passes in an asserted optimized release profile. No pinned
NomadNet reference is defined by the migration contract.

Reticulum 0.9's `Transport::link()` returns the existing non-closed outbound
link for a destination. A deterministic lifecycle harness now proves that an
active link is reused, an explicit close emits `LinkClose` and marks that
shared handle closed, and the next lookup creates a distinct pending link.
That behavior makes uncoordinated close-after-request ownership unsafe when
two page tasks target the same destination. Production page operations now
hold one of 32 fixed destination stripes from link preparation through request
resource cleanup and any failure teardown. Waiters can be cancelled without
acquiring or retaining the guard, and destinations in other stripes retain
bounded parallelism. Successful operations retain the active link; failures,
timeouts, and cancellation close it. A stale retained link is closed, removed,
and replaced before the next dispatch. Current Python proves two sequential
executable requests use one link and return exact visit-specific bytes.

## Phase 4 workaround decision matrix

| Gap/workaround | 0.6 reason | 0.9 candidate | Rust-Rust evidence | Rust-Python evidence | Performance | Current decision | Earliest fallback removal |
|---|---|---|---|---|---|---|---|
| NomadNet request-resource for every request | No safe public arbitrary-context direct link send | Active-link `data_packet`, `Request` context, final packet-hash ID, bound `send_direct` | Deterministic encrypted request/response round trip and primitive boundary pass; Resource cancel/timeout ownership and destination-serialized link reuse/teardown remain covered | Current Python RNS 1.3.8/NomadNet 1.2.7 returns exact bytes for direct empty/form requests, oversized request Resource with direct response, and direct request with large response Resource; typed Resource completion passes; delayed timeout and post-dispatch cancellation each terminate without replay; two sequential executable requests and the comparative workload use one active link; the 32-request soak uses exactly two bounded link generations around an explicit peer close with no replay or concurrent link growth; no pinned NomadNet reference is defined | Four sequential primitive-matrix pages complete in about 1.6 seconds locally; the two-case fault run completes in about 7.1 seconds including deliberate delays; repeated requests measured 184 ms initial and 34 ms reused locally. After two warmups, eight debug-profile samples per primitive measured direct at 34,339 us median/39,979 us p95 and request Resource at 80,474 us median/87,872 us p95. The asserted release profile measured 35,138/40,998 us direct median/p95 and 78,756/86,923 us request-Resource median/p95. The focused soak exchange/recovery measured 4,411/1,004 ms | Select request primitive by packed MDU, accept either correlated response primitive without request retry, retain successful active links, and close failures under destination-serialized ownership | No fallback removal planned; both primitives are required by Python semantics |
| Project-local IFAC TCP | Stock TCP did not apply the Python IFAC transform | Stock 0.9 source audit confirms TCP client/server still use raw Packet↔HDLC; shared IFAC config is not enforcement | Exact pinned-Python wire vector, local round trip, wrong-key, missing-flag, tamper, framing, and queue tests pass | Supported Rust-client direction passes exact pinned-Python authentication/transmit, split/coalesced HDLC, reconnect, wrong-credential rejection, plus full Python Transport path request/announce/identity recall/link activation/bidirectional link data; role reversal, IPv6, Resources, and multi-client remain pending | Bounded reconnect completes in the production five-second delay; full path/link-data exchange completes in under two seconds locally; idle/soak comparison pending | Retain project-local IFAC client; reject unsupported IFAC stock-server config | No earlier than v0.9.5-2 after upstream server enforcement plus broader field evidence |
| OMENchat context-zero link data/resources | Needed public encrypted link/resource APIs without changing the OMENchat context contract | 0.9 generic link data, reusable links, resources, and delivery trace; unknown legacy `0x4f` is represented as generic data by `PacketContext` | Shared v0.6.0-1 session-open, room-message, and history-resource-offer fixtures encode/decode byte-exactly in both current crates; protocol name/version, legacy context, metadata prefix, context-zero valid-frame admission, non-frame rejection, and bounded resources pass. Active sessions reuse one registered link; explicit reconnect generations cancel superseded opens, serialize by a fixed destination stripe, and retire only the matching prior link before requesting a fresh handle. Current-client→old-server and old-client→current-server each open a live session, join, send, and observe the echoed event. Each client state root also reopens for a second exchange after the opposite-version server restarts with a stable destination; current-server shutdown is orderly while old-server shutdown is a bounded legacy signal stop. Both client versions also decode the opposite-version server's history from a live OMENchat Resource and observe the exact prior message. One continuously running current product process observes an old-link close, opens a different link, reconnects the same session, and receives a post-restart echo | Interactive Iced-window restart soak and pinned-Python transport tests remain pending | Link-count/reconnect comparison pending | Retain current dual-admission/context-zero send and resource path; no wire change. Keep explicit reconnect ownership until live reuse/restart evidence justifies any broader pool | Only after mixed-version link/resource/restart tests; no protocol change implied |
| Direct proof/reply correlation fallback | 0.6 lacked the required typed correlation at the application boundary | 0.9 cryptographically validates destination/link proofs before invoking `ReceiptHandler`; observed packet/resource hashes provide the correlation key | Bounded correlation, persisted recovery, duplicate suppression, failed-dispatch cleanup, resource-terminal separation, stale-old-attempt/new-retry isolation, runtime/process restart, clean timeout, post-commit termination, and message-store staging/replace fault boundaries pass. Injected errors leave complete old/new JSON and clean stages; process kill after stage sync preserves old, while kill after rename preserves new. Reopen removes abandoned leased artifacts but preserves a live writer and unleased legacy stages. Direct and recipient-decrypted propagated clean-transport admission reject unknown announce identities, source/identity mismatch, forged signatures, and unsafe attachment writes; direct replay is suppressed, response/durable propagated duplicates are suppressed, and rejected propagated payloads remain unacknowledged while bounded sender path recovery is dispatched | Rust sends an old packet, observes a bounded no-proof interval, then sends a replacement; pinned Python rejects the forged proof and returns the correctly signed old/current hashes in order. The informational current-Python lane exercises reciprocal small direct messages and one isolated Python propagation-node enqueue/Rust production sync/ack: LXMF 1.0.1 validates the Rust direct signature, Rust verifies Python direct and propagated signatures against authenticated announces, and the Python node removes the transient only after Rust acknowledgement. Automatic message retry dispatch, physical power-loss evidence, propagation Resources/stamps/tickets/restart, and application consumption of authoritative remote delivery state remain pending | Correlation and publication-artifact inventories are each capped at 4,096 metadata entries; the announce identity cache is capped at 256 entries; propagation responses are capped at 4,096 entries and missing-sender path dispatch is capped at 32 unique sources per sync; isolated two-send link/proof exchange completes in under two seconds locally; each current direct-delivery direction completes in under one second locally; the current propagation enqueue/sync/ack completes in under three seconds locally; child boundaries are bounded. Cleanup performs no payload read and only removes an artifact after acquiring its zero-byte lease nonblockingly | Retain peer-unconfirmed conservative state; a transport proof or propagation-node acknowledgement is not peer-level LXMF delivery, and the Python callbacks are currently test evidence rather than OMEN status inputs. Retain strict authenticated-announce signature admission for direct and propagated inbound payloads; unknown propagated senders remain deferred while exact bounded path recovery runs | After application-level delivery-state integration and broader pinned/mixed propagated interoperability evidence, not transport receipt or node-acceptance evidence alone |
| Peer stamp-cost negotiation gap | Required peer/router policy was not available at the UX boundary | 0.9 `SendRequest::stamp_cost`, RPC delivery/ticket records, announce helpers, and router metadata | Admitted delivery costs match the upstream 0.9 parser; explicit `unknown`, `not required`, `required`, `ticket accepted`, and `unsupported` decisions pass; directory persistence, SDK/RPC field mapping, low-cost generation/validation, expired-ticket rejection, ticket precedence, restart-stable issuer reuse/renewal, one-day attempted-inclusion throttling, concurrent-runtime serialization, exact issuer-field injection, bounded direct worker admission, permit release, shutdown cancellation, and bounded event-driven first-send policy discovery pass | Current and pinned Python `LXStamper` accept the bounded Rust propagation stamp at its exact achieved value and reject the same bytes at value+1. Both Python lanes also accept/deliver a production Rust envelope at the node's advertised minimum cost 13 and reject a second stale-policy envelope at a raised live floor without a second delivery. Both lanes accept the Rust `ticket || message_id` truncated-hash stamp, reject a wrong ticket, and pass Python issue/reuse/renewal/throttle/use/expiry/cleanup boundaries. In both live lanes Python authenticates a Rust ticket-bearing direct message, remembers its ticket, uses it on a direct reply, and Rust verifies the exact ticket stamp and signature. Both lanes advertise direct cost 1, accept the production Rust stamped message, and reject an unstamped control without a second callback. Both lanes also start OMEN's integrated runtime with policy removed, require authenticated discovery before first-send encoding, accept that stamped send, and reject the unstamped control. Post-rejection refresh, propagation tickets, and live issuer restart remain pending | Cost-2 primitive generation is bounded to 4,096 attempts; production direct proof work is limited to cost 8, 65,536 attempts, and two blocking jobs; production propagation generation is bounded to 2^22 attempts. The direct cost-1 codec cases complete in roughly 0.3 seconds; integrated first-send discovery plus the control/observation boundary completed in 2.751 seconds pinned and 2.780 seconds current. Policy discovery is event-driven and capped at five seconds. Ticket material is fixed at 16 bytes; the issuer is capped at 256 peers/128 KiB and private SDK caches retain item/byte bounds. High-cost proof latency is not measured | Retain the direct product safety ceiling and explicit rejection above cost 8 until user policy and high-cost measurements exist. Authenticated empty legacy policy sends without work; matching over-limit policy fails. Do not automatically resend after silence: the integrated transport exposes packet proof but no authoritative peer `LXMRouter` stamp rejection, so a resend could duplicate an accepted message. Propagation-policy changes currently require a fresh announce/retry. Integrated issuance uses a persisted attempted-inclusion interval; external-daemon issuance remains delegated | Authoritative post-rejection refresh/retry, user-configurable high-cost policy, propagation tickets, and broader live restart evidence remain before full closure |

Current-product OMENchat upload update (2026-07-17): two canonical `0.9.5-1`
clients with separate application and identity roots now pass a live upload and
Resource retrieval case against current standalone omenchatd. The sender emits
typed upload-complete and Resource-available events at the exact deterministic
873-byte fixture size; the second client discovers the upload through room
history and fetches the same Resource at that exact size. The bounded harness
retains only public versions, the fixture size, and validation booleans. This
adds current-current Resource evidence without changing the context-zero wire
path; pinned-Python OMENchat transport and interactive native-window soak remain
pending.

Maintainer release disposition (reverified 2026-07-31): the separate known-red
two-process UDP Resource gate reflects the exact locked crates.io
`reticulum-rs-transport = 0.9.6` implementation and remains visible as an
upstream parity limitation. It blocks claims that maximum UDP Resource
transfer works, but does not block the version-aligned OMEN release. No
fallback, local patch, fork, test weakening, or upstream coordination is
introduced. The passing OMENchat, NomadNet, pinned/current Python, and
mixed-version Resource cases remain scoped to their tested interfaces and
primitives.

Reticulum 0.9.6 requalification (2026-07-21): the same deterministic sentinel
still fails with a 456-byte upstream UDP buffer and a 483-byte maximum type-one
Resource wire packet. The limitation therefore remains in parity with the
published 0.9.6 train. OMEN continues to keep the test explicit and ignored in
normal suites, rejects any claim that the maximum UDP Resource boundary works,
and does not add a fork, local transport patch, larger application buffer, or
unbounded retry. Smaller Resource cases retain only their separately tested
interface and payload scopes.

Reticulum 0.9.7 requalification (2026-08-01): the exact unchanged sentinel
still fails at 456 versus 483 bytes against the official registry 0.9.7 train.
The same conservative disposition remains: maximum-size UDP Resource parity is
not claimed, the red test remains explicit, and no fork, patch override,
application fragmentation, lowered protocol limit, or automatic retry hides
the upstream boundary. Separately passing TCP, smaller-Resource, OMENchat, and
NomadNet cases retain only their tested scope.

The 0.9.7 source audit also confirms that upstream now increments received hop
counts before publishing announce events and supervises its transport worker
group. Pinned Python observes a directly connected peer at one hop and the
complete path/identity/Link-data test passes with that reference-compatible
value. OMEN retains its outer generation-scoped terminal recovery: interface
workers still own ordinary reconnect, while repeated terminal observations can
schedule at most one delayed runtime replacement and clean shutdown schedules
none. Upstream supervision is therefore defense in depth, not grounds to start
a competing reconnect loop.

Stock 0.9.7 TCP remains insufficient evidence to remove OMEN's IFAC adapter.
The retained adapter passes pinned/current Python vectors, bidirectional
authentication, wrong-credential/tamper rejection, split/coalesced HDLC,
reconnect, path/announce/identity recall, Link activation, and Link data. Its
read and delimiter-free accumulation allocations are now explicitly bounded,
backpressured delivery is cancellable, paired tasks are supervised, and tag
comparison is constant-time without changing wire bytes or MTU.

The 0.9.7 stamp/ticket decision remains unchanged: OMEN owns one authoritative
decision and final stamp using authenticated relay-advertised cost, its existing
safety ceiling, and reply-ticket precedence. Pinned/current Python and mixed
0.6.0-1/0.9.7-4 propagation tests retain advertised cost and ticket wire checks.
The upstream default propagation cost is not substituted for a raised live
relay cost.

Current-product NomadNet update (2026-07-18): the canonical `0.9.5-1` browser
now passes a scheduled live page fetch against the standalone server's fixed
`nomadnetwork.node` portal over an ephemeral loopback interface. The production
direct-request path returns and decodes the exact non-empty 309-byte, 17-line
`text/x-micron` page. The retained report excludes the destination, URL,
identity, path, port, logs, and state. This closes the current-product portal
case in the first matrix row. Current Python RNS 1.3.8/NomadNet 1.2.7 also
returns exact bytes across direct and Resource request/response combinations.
Delayed response timeout and cancellation after confirmed dispatch also pass
with exactly two Python requests and no replay. Two sequential executable page
requests also pass on one active link, measuring 184 ms for initial setup and
34 ms for reuse locally. The alternated debug-profile comparison measured
direct requests at 34,339 us median/39,979 us p95 and request Resources at
80,474 us median/87,872 us p95 over eight samples each. Release-mode
confirmation remains pending. A separate bounded soak alternates 16 direct and
request-Resource exchanges around a two-second idle interval, forces the Python
node to close the retained link, and completes 16 more exchanges on one fresh
generation. The focused reference run served exactly 32 requests with no
replay, third generation, or more than one active Python-side link; exchange
and recovery measured 4,411 ms and 1,004 ms respectively. The migration plan
defines no pinned NomadNet ref.

Pinned propagation update (2026-07-17): the release-blocking immutable Python
Reticulum and LXMF references, whose modules identify as 1.2.2 and 0.9.6, now
pass the same isolated propagation-node enqueue/Rust production sync/ack case
as current Python 1.3.8/LXMF 1.0.1. Both complete in under three seconds
locally. This resolves the single-message pinned software-topology gap in the
direct-proof row above; Resources, required stamps/tickets, node restart,
multiple recipients, mixed versions, and peer delivery after node acceptance
remain pending. Propagation-node acknowledgement is not peer-level delivery.

Mixed-application direct update (2026-07-17): the hardened `0.6.0-1`
application at immutable commit `5ba6683` and the current `0.9.5-1` application
now pass reciprocal direct Link-packet delivery through isolated Python RNS
1.3.8 transport. Each process authenticates the other's announce, submits one
direct message, and admits exactly one matching peer reply with the expected
message shape. The final rerun completed its paired exchange in 16.055
seconds on the first bounded attempt. Packet proof availability differed by
direction, but neither application used proof as peer-level LXMF delivery.
The companion Resource case decoded 65,536 bytes in both directions, as
documented below. A separate current-to-old propagation case now queues one
transient at exact Python RNS 1.3.8/LXMF 1.0.1, lets the old application sync
and authenticate exactly one expected message, and verifies acknowledgement
removes the transient. The reverse direction now passes too: current initially
defers the unknown old sender without acknowledgement, performs bounded path
recovery, restores the authenticated identity before retry decode, and then
acknowledges exactly one retained message. An orderly current-to-old
propagation-node restart also preserves one queued transient and the node
identity across a new process/port, then removes it after recipient
acknowledgement. Abrupt process termination after observed settled LXMF storage
also restores and serves the same transient exactly once. A separate mixed
case enforces the Python node's advertised propagation-stamp floor, verifies
the current sender's matching bounded-work evidence, carries a fresh reply
ticket, and lets the hardened old application recover that ticket before its
acknowledgement empties the queue. Ticket use on a propagated reply, physical
power-loss, filesystem/device durability, and physical interfaces remain
pending.

Mixed-application restart update (2026-07-17): after a complete sequential
two-direction exchange, both application processes exit and reopen their same
isolated roots. The second exchange preserves both LXMF destinations, generates
new outbound/inbound message IDs, correlates each inbound ID to its peer send,
and admits exactly one message per direction. The initial and reopened rounds
completed in 40.074 and 40.067 seconds. This covers identity, configuration,
and Reticulum state reopening; it does not infer SQLite conversation-history,
propagation-node restart, or abrupt-crash recovery.

Mixed OMENchat store update (2026-07-17): a separate network-free probe now
opens one isolated `chat.sqlite` through each application's public store API in
old-to-current-to-old-to-current order. Each version reads the prior writer's
server, room, active-room, ordered event, and content state before appending.
The final current reopen sees all three unique events. This closes deterministic
SQLite format reopening between the hardened applications; it does not prove a
mixed live OMENchat link, history Resource transfer, or crash durability.

Mixed live OMENchat update (2026-07-17): both application directions now pass
over isolated ephemeral loopback Reticulum interfaces. The current `0.9.5-1`
canonical desktop client connects to the immutable hardened `0.6.0-1`
standalone server, and the old client separately connects to the current
server. Each starts the runtime, opens a link and OMENchat session, joins the
room, sends one message, and observes the echoed room event. This closes the
reciprocal single-session handshake/message matrix. Server restart/reconnect,
history Resource transfer, pinned-Python transport, native platforms, and
physical interfaces remained pending at this stage; the bounded restart/state
reopen case below narrows that list further.

Mixed OMENchat restart update (2026-07-17): after the current client completes
one exchange with the hardened old server, the server stops within a bounded
SIGTERM deadline and reopens the same server home/interface with an unchanged
destination. A fresh current-client process reuses its original application
root and completes a second link/session/join/message/echo exchange. The old
server predates the owned SIGTERM drain path, so this proves bounded
process-restart and state reopening, not orderly old-server shutdown or
automatic reconnect by a continuously running desktop. The reciprocal restart
also passes: current omenchatd completes its owned orderly SIGTERM drain,
reopens the same destination, and the hardened old client reuses its original
root for a second exchange. Automatic reconnect by a continuously running
desktop remains pending.

Mixed OMENchat history-Resource update (2026-07-17): the current client now
passes a live history transfer from the hardened old server. The harness sets
only the isolated server's large-batch threshold to one byte, sends a normal
small message, and opens a second isolated client. That client receives a real
`resource_data` event, decodes `history_prepended` from within it, and observes
the exact first-client message. No production default or wire behavior changed.
The reciprocal old-client to current-server case now passes under the same
isolated threshold: it receives a real `resource_data` event, decodes
`history_prepended`, and observes the exact prior message. Mixed application
history-Resource transfer is therefore covered in both directions. Continuous
automatic reconnect now passes in one continuously running headless product
process against current omenchatd: the old link closes, a different link opens,
the same session reconnects, and a second echo arrives. Interactive Iced-window
restart soak, pinned-Python transport, native platforms, and physical interfaces
remain pending.

## Desired Upstream Shape

A minimal public helper could look like this:

```rust
impl Transport {
    pub async fn send_link_data_with_context(
        &self,
        link_id: &AddressHash,
        payload: &[u8],
        context: PacketContext,
    ) -> Result<SendPacketOutcome, RnsError>;
}
```

The implementation should:

- find the active inbound or outbound link by `link_id`;
- build `link.packet_with_context(payload, context)` or equivalent;
- send the packet directly on `link.ingress_iface()`;
- return an error if the link is not active or has no bound interface.

That generic helper would cover both `Request` and `LinkIdentify`, although
OMENbrowser no longer needs it for identify-on-connect. A narrower identify
helper would still be useful upstream:

```rust
pub async fn identify_link(
    &self,
    link_id: &AddressHash,
    identity: &PrivateIdentity,
) -> Result<SendPacketOutcome, RnsError>;
```

It should build Python-compatible proof data:

```text
signed_data = link_id || identity_public_key
proof_data  = identity_public_key || sign(signed_data)
context     = PacketContext::LinkIdentify
```

An alternate narrower helper specific to requests would also work:

```rust
pub async fn send_request_to_link(
    &self,
    link_id: &AddressHash,
    payload: &[u8],
) -> Result<SendPacketOutcome, RnsError>;
```

## OMENbrowser Boundary

The UI should not call this helper directly. It should remain behind:

- `NativeLinkRequestAdapter`;
- `ReticulumPageTransportClient`;
- `NetworkRuntime::fetch_page`.

If the direct candidate passes the interoperability gate,
`Reticulum09LinkRequestAdapter` can route small frames through a narrow helper
and continue to wait on
`received_data_events()` for `PacketContext::Response` data. The current
request-resource path should remain for large form submissions because it
matches Python Reticulum's `Link.request()` behavior and provides a safe
compatibility fallback.

For identify-on-connect, `NativePageFetchContext` carries both the policy flag
and the loaded local identity. Clean-stack page loads send the Python-compatible
LinkIdentify proof after the page link is active. If the identity cannot be
loaded or the link lacks a bound ingress interface, page fetch still proceeds
and the skipped identify is logged.

## IFAC-Gated Gateway Status

The clean OMENchat protocol has now been smoke-tested with one `omenchatd`
server and two isolated OMENbrowser clients all attached as TCP clients to the
same IFAC-gated private gateway. With the project-local IFAC TCP interface in
place, both clients opened the Reticulum link, joined the room, and observed a
message echo. The server saw the expected `SessionOpen`, `JoinRoom`, and
`RoomMessage` frames.

The smoke helper supports this topology:

```text
scripts/release-omenchat-smoke.sh \
  --server-tcp-client <gateway-host:port> \
  --network-name <ifac-network> \
  --multi-client
```

Pass the IFAC passphrase through `OMENCHAT_PASSPHRASE` or `--passphrase`; avoid
committing real gateway secrets. The same helper still supports non-IFAC local
TCP server smoke tests.

The desired upstream behavior remains:

- derive the IFAC access code from configured `network_name` and `passphrase`;
- mark authenticated packets with `IfacFlag::Authenticated` or the equivalent
  upstream packet representation;
- serialize the IFAC bytes in the correct Reticulum wire position;
- verify/drop inbound IFAC packets before normal packet dispatch;
- keep non-IFAC interfaces behavior unchanged.

## Clean LXMF Propagation Status

The published Reticulum 0.6 transport APIs are now sufficient for OMENbrowser's
clean LXMF propagation send/sync path:

- outbound propagation uses active Reticulum links plus request packets to the
  selected propagation node;
- propagation envelopes are generated by the `lxmf` 0.6 wire API;
- propagation stamps are generated in OMENbrowser with the LXMF-compatible
  HKDF/SHA256 workblock algorithm;
- sync validates and strips stamps before decrypting with the recipient
  identity-hash salt expected by `lxmf` 0.6.

No `rns-net` compatibility crate is required for the clean path. The remaining
known gap is sender-side direct proof/reply correlation: direct messages can be
delivered to an online peer, but short CLI smoke runs may still time out waiting
for proof evidence even when the receiver observed the message.

Clean LXMF ticket support is now implemented in the clean codec/app path:
outbound include-ticket sends emit the LXMF ticket field, inbound decode
extracts reply-ticket metadata, and direct replies can use a valid stored reply
ticket to produce the LXMF ticket stamp. Integrated sends persist a bounded
per-peer issuer cache under the managed Reticulum storage root, reuse or renew
the exact ticket according to the Python-compatible three-week/two-week
boundaries, and suppress another attempted inclusion for one day. External
daemon sends delegate this policy. This does not require `rns-net`; it uses the
`lxmf` 0.9.5 wire payload field/stamp surfaces.

Direct signed wires that exceed link-packet capacity now have live
Rust-to-Python evidence through the upstream 0.9.5 Link Resource path. OMEN
retains bounded Resource-hash correlation, publishes an honest offered byte
total and terminal lifecycle, and keeps completion peer-unconfirmed. Sender-side
incremental Resource progress and an application cancellation handle remain
upstream API gaps; failure/cancellation terminal events are handled and release
correlation, but the UI cannot yet initiate cancellation of this direct send.

The same Resource boundary now has mixed-application evidence. Disposable
0.6.0-1 and 0.9.5-1 source copies use an identical one-line diagnostic fixture
that supplies a 65,536-byte body. Both adapters declare a 431-byte Link-packet
MDU and route larger signed wires through `send_resource`. In the completed
loopback run, each real application decoded exactly one 65,536-byte reciprocal
message from the other version. This supports retaining the existing Resource
path across the upgrade; it does not close sender-side incremental progress or
application-initiated cancellation gaps.
