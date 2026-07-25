# LXMF delivery and event model

This document defines OMENbrowser-owned lifecycle, capability, delivery, and
event semantics for the Reticulum/LXMF 0.9 migration. Upstream SDK, RPC, and
transport types remain inside runtime adapters; the UI, persistence services,
and application event bus consume the stable types exported by `src/runtime`.

## Runtime lifecycle

The application lifecycle states are:

- `new`: constructed but never started;
- `starting`: validating configuration and acquiring runtime resources;
- `running`: ready to accept new work;
- `draining`: refusing new work while bounded owned work shuts down;
- `stopped`: orderly terminal state that may be started again;
- `failed`: terminal failure with a structured category, user-safe summary,
  optional technical detail, and retryability flag.

Repeated transitions to the same state are idempotent. Normal startup follows
`new|stopped|failed -> starting -> running`. Normal shutdown follows
`running|starting -> draining -> stopped`. A runtime may enter `failed` from an
active startup/run/drain state. It must not jump directly from `running` to
`stopped`, because doing so would conceal the drain boundary.

Only `running` accepts new operations. `draining`, `stopped`, and `failed` must
not accept a new page fetch, message send, propagation sync, OMENchat link, or
resource transfer. The mock and integrated Reticulum adapters enforce this at
their network-operation boundaries. Read-only status, capability, diagnostics,
and local history inspection remain available after stop so shutdown and
support reporting can complete.

The mock adapter is a zero-resource, always-ready simulation, so its default
constructor begins in `running`; it still crosses `draining -> stopped` during
shutdown and rejects subsequent network simulation until explicitly restarted.
The integrated adapter begins in `new`, then follows the normal lifecycle.

Repeated start is idempotent only when the effective identity and interface
plan are unchanged. A conflicting start is rejected with the existing runtime
left running; callers must stop before reconfiguration. Repeated stop is
idempotent. Cleanup operations such as closing an existing link remain allowed
during shutdown, while new links, frames, resources, fetches, sends, announces,
path requests, and propagation synchronization are not admitted.

## Failure categories

Project-owned failures distinguish configuration, identity, interface,
transport, RPC, storage, protocol, shutdown, internal, and unknown failures.
User-visible summaries must not contain identity material, credentials, private
paths, tickets, or tokens. Technical details are for redacted diagnostics and
must not be treated as stable machine-readable state.

## Capability snapshots

Capability availability is `supported`, `unsupported`, or `unknown`. Missing
records are always `unknown`; they are never inferred as supported from an
application or dependency version. Each record also identifies whether its
evidence is compiled, configured, negotiated, or unknown.

The initial capability vocabulary covers direct, opportunistic, propagated,
and paper/URI delivery; cancellation; event streaming; history and conversation
listing; tickets and stamps; propagation status; attachments; shared instances;
path metadata; interface mutation; and integrated versus RPC backends.

Compiled evidence only means the code exists. Configured evidence means the
operator selected a usable mode. Negotiated evidence is required before an
adapter claims a remote or runtime-dependent operation is supported.

The mock adapter labels simulated behavior explicitly. The integrated adapter
claims active transport operations only while its Reticulum transport is
running. Optional RPC support is reported as supported only after a compatible
local SDK snapshot is returned; a configured but unreachable endpoint remains
`unknown`, and a missing or policy-rejected endpoint is `unsupported`. A
successful general RPC snapshot does not imply support for every RPC method.

Negotiated ticket/stamp capability, shared instances, and live interface
mutation remain `unknown` until their typed runtime paths are integrated and
tested. SDK event streaming and
cancellation are negotiated independently: a configured local SDK endpoint can
provide them, while the integrated clean-transport path reports cancellation as
unsupported instead of simulating success. Local OMENbrowser history remains
authoritative and is reported separately from SDK/router conversation-history
capabilities.

The integrated clean sender consumes the authenticated delivery announce's
direct cost before encoding. A valid remembered reply ticket always takes
precedence. Otherwise, required costs through 8 use a bounded direct proof with
at most 65,536 attempts, two concurrent blocking jobs, and cooperative runtime
shutdown cancellation. A typed policy decision distinguishes `unknown`, `not
required`, `required`, `ticket accepted`, and malformed/out-of-range
`unsupported` announcements. Missing/legacy policy preserves compatibility by
sending without proof; malformed policy or a required cost above the product
safety ceiling fails locally and explicitly. External `reticulumd` mode keeps
this policy delegated to the daemon.

The propagation-stamp algorithm is also checked against both Python lanes. A
Rust cost-2 generator has a 4,096-attempt ceiling; Python LXMF must preserve the
transient and stamp, calculate the same achieved value, accept at that exact
value, and reject the identical bytes at value+1. This is exact
`LXStamper.validate_pn_stamps` compatibility evidence. A separate live test now
drives the production clean sender through Python's network-facing propagation
handler. Python accepts and locally delivers an advertised-cost-13 message,
then rejects a second envelope after its live admission floor changes while the
Rust sender retains the earlier announce. The rejected envelope does not
increment Python's accepted-client counter or invoke delivery again. This is
node-admission evidence, not peer-level delivery, automatic policy refresh, or
peer-delivery evidence.

Ticket byte semantics and lifecycle boundaries are checked independently in
both Python lanes. Rust generates a 16-byte ticket stamp from
`ticket || message_id`; pinned LXMF 0.9.6 and current LXMF 1.0.1 accept it at
the ticket cost and reject it with the wrong ticket. The same isolated matrix
requires Python's three-week default issue window, reuse while more than two
weeks remain, renewal near expiry, one-day delivery throttle, remembered
outbound use, exact expiry preservation, expired-use rejection, and grace-period
cleanup. OMEN's durable message store separately rejects malformed or expired
reply tickets and uses a valid remembered ticket ahead of direct proof-of-work.
Reusable bytes exist only in isolated fixture files or private bounded state and
are never written to test output or diagnostics. The integrated runtime now
applies the same issuer lifecycle at an owned persistence boundary: one ticket
per normalized peer is reused while more than two weeks remain, renewed near
expiry, and included at most once per day. The decision is serialized across
runtime instances in the process and persisted before dispatch, so a crash
cannot make an ambiguous send immediately issue another ticket. Consequently,
the one-day interval means "last attempted inclusion", not proof that the peer
received the ticket.

Issuer state lives in `omen_lxmf_issued_tickets.json` under the managed
Reticulum storage root. It is written through a same-directory atomic replace,
is limited to 256 peers and 128 KiB, rejects corrupt/symlinked state instead of
regenerating it, and is private on Unix. Filesystem work runs behind a two-job
blocking gate rather than on an async worker. External `reticulumd` mode does
not read or mutate this file; ticket issuance remains delegated to that daemon.
Application diagnostics report only `included_new`, `included_reused`,
`suppressed_interval`, `not_requested`, or `delegated_external_runtime`, never
ticket bytes.

A separate live round trip now covers the network boundary in both Python
lanes. OMEN's production signed direct codec issues a ticket over an activated
Reticulum link. Python's real `LXMRouter` authenticates the message, remembers
the ticket, attaches it to a direct reply, and receives the reply packet proof.
Rust verifies the Python identity signature and requires the received stamp to
equal the truncated hash of the privately retained issued ticket plus the reply
message ID before passing the wire through the production decoder. The fixture
reports only boolean results and message identifiers; ticket bytes are never
placed in arguments, stdout, diagnostics, or repository files.

Both Python lanes exercise this boundary across an activated Reticulum link.
Pinned LXMF 0.9.6 and current LXMF 1.0.1 advertise cost 1, accept OMEN's
production-signed and stamped message, and reject an otherwise valid unstamped
control without a second delivery callback. The final runs completed in 304 ms
and 311 ms respectively, including path/link setup and both sends; generated
proofs took two and one attempts. Those low-cost observations are
interoperability evidence, not a high-cost benchmark.

Before the first integrated direct send is encoded, a missing policy entry now
resolves the destination identity and waits event-driven for an authenticated
announce, requesting the path once when needed. The wait is bounded by the
configured request timeout and an absolute five-second ceiling and is cancelled
with runtime shutdown. Authenticated empty app data is retained as an explicit
legacy/unknown policy instead of being confused with an announce that has not
arrived. Matching app data rejected by the 4 KiB admission limit fails closed;
unrelated announces do not wake the send. Summary metadata reports whether the
policy came from the cache, initial discovery, a requested refresh, a bounded
timeout, or external delegation without exposing app data.

This is pre-encode discovery, not automatic resend. The integrated packet proof
does not reveal Python `LXMRouter` stamp rejection, so retrying after silence
could duplicate an accepted message. Stale cached-policy refresh after an
authoritative rejection therefore remains unavailable until the backend
provides peer-level rejection evidence. User-selectable costs above 8 also
remain deferred. Reply-ticket bytes remain secret state and are never included
in diagnostics.

## Inbound direct-message admission

The integrated clean transport admits a direct LXMF message only when its
16-byte source resolves to an identity learned from a transport-validated
`lxmf.delivery` announce. The resolver is the existing 256-entry bounded
destination cache. The cached public identity must derive the exact claimed
delivery destination and `WireMessage::verify` must accept the signature before
a `MessageSummary` is produced or attachment bytes are written.

Unknown sources, mismatched identities, missing/invalid signatures, and
malformed bounded wire data are rejected. Successful messages then pass through
the existing five-minute, message-ID replay window before publication. Source
parsing and cache resolution run inside the bounded blocking decoder; the cache
mutex is released before signature/payload decoding and is never held across an
await. Verification therefore adds no unbounded or async-worker CPU path.
Rejections emit a redacted ingress diagnostic and never include key material.

This contract covers direct clean-transport link data/resources and decrypted
propagation payloads. Propagated data is decrypted with the local recipient
identity inside the same bounded worker, then its embedded source is resolved
through the authenticated announce/path cache and verified before application
or attachment state is created. An unknown or rejected local payload is not
added to the delivered-transient store and is not acknowledged to the
propagation node, allowing a later retry after identity discovery. The
release-blocking pinned lane and informational current-Python lane now each
prove one isolated Python node can queue a signed encrypted transient, the
production Rust sync can authenticate and publish it, and the node removes it
after Rust acknowledgement. The pinned lane uses immutable Reticulum/LXMF
commits with module versions 1.2.2/0.9.6; the drift lane uses 1.3.8/1.0.1.
Stamp/ticket, restart, Resource, and mixed-version propagation remain separate
interoperability evidence.

When sender resolution is the reason for deferral, the bounded decoder returns
the exact unresolved source hash as structured process-local state. The sync
coordinator issues at most one path request per source and at most 32 sender path
requests per sync. It awaits only bounded transport dispatch, never waits for
path resolution, and holds no identity-cache lock across the request. The
unacknowledged payload is retried by a later sync after normal announce/path
processing populates the identity cache.

Each propagation response also has a bounded transient-ID admission set. Exact
duplicate candidates are discarded before decryption and publication, and an
ID already present in the durable delivered cache is acknowledged without
republishing its message. The response parser's existing 4,096-item limit bounds
this set and the acknowledgement inventory.

## Event and delivery follow-up

### Per-peer delivery default

The existing Directory `Direct` or `Propagated` preference initializes the
delivery mode of a newly opened conversation for that peer. Reopening an
existing conversation, restoring a saved conversation, and manually changing a
conversation's delivery mode preserve that explicit per-tab choice. An unset
or legacy preference remains Direct. This is a local composer default, not
automatic fallback policy, delivery evidence, or permission to retry an
uncertain send.

The additive `Direct only` and `Propagated only` policies reuse the existing
bounded Directory entry and JSON field. They initialize new conversations to
the permitted transport, prevent the composer from switching to the forbidden
transport, and are checked again immediately before a native send. Legacy
`direct` and `propagated` JSON values retain preferred—not exclusive—semantics.
The outbound operation snapshot records whether propagation fallback is
permitted so an explicit retry cannot silently weaken `Direct only`; older
operation records default to the prior fallback-permitted behavior. Malformed
policy metadata is not reused. Strict policy does not trigger a fallback,
retry, path request, propagation sync, or stamp expenditure.

The integrated runtime broadcast is consumed by one owned worker and forwarded
through the existing 256-item application channel. Payload-bearing OMENchat
events and SDK history pages additionally retain the shared 32 MiB queued-byte
budget. The worker assigns a
monotonic process-local cursor, suppresses exact duplicate control events only
within a 64-event window, and bounds its deduplication memory to 512 keys and
256 KiB. Payload frames, resources, debug messages, and errors are never
deduplicated.

A broadcast lag or downstream payload-byte rejection emits an explicit
`stream_gap` event instead of silently continuing. Recovery samples runtime
status, interfaces, network state, propagation state, at most 256 directory
candidates, and queued runtime messages through the existing snapshot APIs,
then emits `stream_recovered` with component results. A rejected OMENchat
payload also closes the affected link so normal reconnect/backlog recovery can
repair protocol state rather than continuing after a missing frame.

The process-local cursor is not represented as an upstream SDK cursor. When an
optional local-only SDK/RPC endpoint is configured, a separately owned worker
negotiates `sdk.capability.async_events`, subscribes from the tail, and retains
the last accepted upstream `EventCursor` across reconnects. The upstream stream
has a 256-item channel; OMENbrowser additionally caps accepted encoded events at
the smaller of the negotiated byte limit and 256 KiB, retains at most 512 event
identifiers using 128 KiB, and applies a capped jittered reconnect delay. The
worker is cancelled and joined by native runtime shutdown.

The RPC `event_stream` capability is `supported` only while a negotiated stream
is connected, `unsupported` when negotiation explicitly rejects async events or
no endpoint is configured, and otherwise `unknown`. Sequence discontinuities,
explicit upstream `StreamGap` events, and local byte-limit rejection enter the
same bounded snapshot recovery path described above. Raw SDK event metadata and
bounded JSON payloads remain project-owned neutral events unless the upstream
event is an authoritative `DeliveryStateTransition`. Those transitions
deserialize through the upstream non-exhaustive `DeliveryState` and become
project-owned typed delivery updates.

The project preserves `queued`, `dispatching`, `in_flight`, `sent`, `delivered`,
`failed`, `cancelled`, `expired`, `rejected`, and a forward-compatible `unknown`
state. `sent` is explicitly nonterminal and is never presented as peer
delivery. Delivered, failed, cancelled, expired, and rejected are terminal by
default; an explicit upstream terminal flag is also retained. Event sequence
numbers reject replayed updates, and a terminal stored state cannot regress to
a later nonterminal state.

No message schema migration is required. Canonical typed values, terminal state,
attempt count, reason code, event sequence, and cursor are stored in the
existing message field map. The legacy delivered/failed booleans remain a
compatibility projection: delivered maps only from `delivered`, while failed,
cancelled, expired, and rejected map to the legacy failure surface. Compact UI
status checks the typed value first so these terminal reasons remain distinct.
Typed delivery metadata strings are capped at 4 KiB before entering durable or
UI state.

Every newly authored send receives a bounded project-owned idempotency key and
correlation identifier before dispatch. Both identifiers are carried by the
0.9 `SendRequest` and persisted in the existing message field map. An explicit
retry reuses them only while the destination and complete draft payload remain
unchanged; editing the prepared retry creates a new logical operation instead
of risking an upstream idempotency conflict. Send failures retain both fields,
so retry identity survives restart without a schema migration. Identifiers are
metadata, not secrets, but logs do not need to print them.

The same logical operation owns an absolute creation time, TTL, and expiry
deadline. New sends default to 24 hours and reject values below one second or
above 24 hours; the upper bound matches the upstream desktop SDK's default
idempotency-retention window so an operation cannot intentionally outlive its
duplicate-suppression identity. OMEN persists all three values in the existing
bounded message field map and passes only the remaining TTL to the 0.9
`SendRequest`. Retries before expiry reuse the operation and its original
deadline. An explicit retry after expiry creates a new idempotency/correlation
identity and a new bounded deadline.

Admission is checked by the messaging service, mock runtime, native runtime,
and final SDK sender boundary. A process restart therefore cannot reset an
existing deadline, and a dispatch delayed across the boundary is rejected
rather than sent with a zero TTL. Startup and the existing narrowly scoped LXMF
deadline worker reconcile expired durable nonterminal rows to typed `expired`
state; the selected-message detail shows the fixed TTL and absolute expiry
without adding a per-message redraw timer. Legacy rows that already have both
operation identifiers but no deadline receive one 24-hour migration window on
their next explicit retry.

The tagged 0.9 `RpcBackendClient` currently removes `ttl_ms`, idempotency, and
correlation fields when translating its SDK request to `sdk_send_v2`. OMEN still
sets the typed request and enforces the absolute deadline locally, while the
embedded RPC bridge retains the fields. A live external-daemon test and an
upstream transport fix remain required before claiming daemon-side TTL
enforcement. A late authoritative terminal event may still correct local
history after reconciliation.

Cancellation preserves the upstream typed outcomes `accepted`,
`already_terminal`, `not_found`, `too_late_to_cancel`, and `unsupported` at the
runtime boundary. The application permits one pending cancellation request per
peer/message pair and offers the action only for outgoing, nonterminal SDK
states. An `accepted` response acknowledges the request but does not mark the
message terminal: only an authoritative later `cancelled` delivery transition
does that. This prevents a cancellation/delivery race from being reported as a
false cancellation. Outcomes and the next recovery action are persisted in the
existing message field map; a transport/RPC failure is recorded as a failed
request without changing delivery state. No ticket, token, endpoint credential,
or identity material is added to these fields.

Deterministic tests cover the complete 0.9 outcome mapping, an embedded-RPC
`not_found` response, unsupported mock behavior, duplicate in-flight request
suppression, and the `accepted`-then-authoritative-`cancelled` race. Live daemon
tests still own proof of cancellation during dispatch, a too-late race, daemon
disconnect, and restart reconciliation.

## Transport receipts and logical-message correlation

The integrated 0.9 transport records the selected packet hash or original
resource hash before dispatch and maps that hash to the durable logical LXMF
message identifier. Upstream validates destination and link proofs
cryptographically before invoking its `ReceiptHandler`; OMEN then requires an
exact bounded correlation match. An unmatched or duplicate receipt emits only
a diagnostic. A retired attempt's hash cannot change the correlation or state
of a newer retry, even when both attempts represent the same logical message.

Packet receipts, successful resource transfer, and inbound peer activity are
useful evidence, but none is promoted to peer-level LXMF `delivered`. They
remain peer-unconfirmed until an authoritative LXMF router/SDK delivery event
arrives. The correlation map contains at most 4,096 metadata entries, evicts
the deterministic oldest entry at capacity, removes failed dispatches and
resource terminals, and can recover nonterminal hash mappings from the
existing message field map after restart. Live pinned-Python proof equality,
timeout/retry, restart, and authoritative delivery tests remain release gates.

## History ownership and restart reconciliation

OMENbrowser's isolated, bounded JSON thread store remains the authoritative UX
history. It owns locally authored content, drafts, labels, unread state,
attachments, deletion tombstones, and retry identity. The SDK/router history is
a reconciliation source, not a replacement database.

After a connected runtime startup, and after an SDK/RPC event-stream gap, the
application requests typed history through the 0.9
`app.message.history.list` surface. Each request is capped at 128 records, its
cursor at 4 KiB, and the accepted encoded page at 4 MiB. Recovery follows at
most four cursors, for a hard ceiling of 512 records and 16 MiB per attempt.
The synchronous SDK/RPC calls run behind the existing bounded blocking
boundary, and accepted pages share the application's 32 MiB payload-event queue
budget. A repeated cursor is rejected. History beyond the four-page ceiling is
not automatically reconciled; the fixed ceiling prevents an unbounded restart
scan and remains an explicit release limitation.

Reconciliation is by message identifier across the bounded local thread
inventory. A matching SDK row may advance receipt/delivery metadata, including
correcting a locally expired row with later authoritative delivery evidence,
but it cannot replace the local title, body, timestamp, label, attachment list,
or operation identity. A nonterminal history state cannot regress a locally
terminal row. A missed inbound row may be imported with its SDK identifier and
marked unread. An SDK-only outbound row is never imported, because its authorship
and operation identity are not owned by this OMENbrowser profile.

Conversation deletion tombstones are checked before durable reconciliation.
Rows at or before the deletion timestamp remain deleted; a genuinely newer
inbound row may reopen the conversation through the existing behavior. Repeated
reconciliation and process restart are idempotent. Malformed directions,
oversized records, exhausted thread/item budgets, and unknown outbound rows are
counted as skipped rather than weakening the store limits.

The integrated clean-transport path continues to drain newly received messages
through `list_messages`; it does not claim durable SDK history. Typed history is
available only from a configured local SDK/RPC endpoint. Live daemon restart,
multi-page cursor traversal, and mixed Python history evidence remain release
tests rather than inferred compatibility.

Existing compatibility paths remain until deterministic and live
interoperability evidence permits their removal.

## Direct Resource-sized delivery

The integrated 0.9.5 sender lets `reticulum-rs-transport` select a Link Resource
when the signed direct wire exceeds link-packet capacity. The application keeps
the Resource hash correlated to the logical LXMF message under the existing
4,096-item pending limit. At advertisement it emits an outbound
`ResourceProgress` sample of `0 / signed-wire-bytes` and an `Offered` lifecycle
record; terminal transport events emit `Complete`, `Failed`, or `Cancelled` and
release the correlation. Completion means transport transfer completion, not
peer LXMF delivery, so the message remains peer-unconfirmed until stronger
evidence arrives.

Reticulum 0.9.5 reports incremental `Progress` for the receiving side, but not
for an outbound sender. OMENbrowser therefore does not invent intermediate
outbound percentages. The signed LXMF decoder retains its 16 MiB wire cap and
8 MiB scalar cap. A deterministic 64 KiB body fixture forces the Resource path
well below those limits and verifies byte equality by size and SHA-256 in both
the pinned and current Python lanes. The fixture uses isolated roots and logs no
payload or private identity bytes.

## Mixed 0.6.0-1 and 0.9.6-2 direct delivery

The Linux interoperability harness exports immutable hardened commit
`5ba6683055fb6c59111919fbad1ac37f56a4c203` into a temporary source root and
builds its `0.6.0-1` application and lockfile independently. It does not add
0.6 crates to either current production dependency tree. That process and the
current `0.9.6-2` process receive separate identities, configuration, storage,
and application roots and connect only through a temporary Python RNS 1.3.8
transport with public fixture IFAC credentials.

Both applications announce and authenticate the reciprocal announce. The
normal case sends one small direct message. The Resource case applies a
one-line fixture patch only inside disposable source copies, changing the
diagnostic body to 65,536 ASCII bytes. That exceeds the 431-byte Link-packet MDU
used by both adapters and therefore selects their existing direct Resource
branches. Each process must decode exactly one peer-bound reciprocal message
with the expected title/content length. Readiness races are bounded to three
paired attempts.

The final Resource run passed on bounded attempt two in 15.930 seconds. Both sides
decoded all 65,536 content bytes; neither observed a packet proof. The inbound
messages prove reciprocal application admission and byte-count preservation,
while Resource completion remains transport completion rather than peer LXMF
delivery. Reports exclude identities, destination hashes, payloads, paths, and
credentials.

The restart case performs each direction sequentially, exits both application
processes, reopens the same isolated state roots, and repeats. The completed
initial and reopened rounds took 40.074 and 40.067 seconds respectively. Both
local LXMF destinations remained stable; every outbound logical message and
corresponding inbound message used a new ID; each inbound ID matched the peer's
new send; and each receiver admitted exactly one message. Raw IDs and
destinations are compared only inside the temporary root and are absent from
the retained report. This proves identity/config/Reticulum-root reopening, not
SQLite conversation-history migration. Mixed propagation and 0.6
release-tag-before-hardening behavior remain separate evidence.

The mixed propagation case now proves one direction without changing either
application protocol or runtime adapter. The current `0.9.6-2` application
submits one propagated message to an isolated Python RNS 1.3.8/LXMF 1.0.1
transport/propagation node. The immutable hardened `0.6.0-1` application then
reopens its identity, requests one transient, authenticates and decodes the
sender, and acknowledges the node entry. The passing report records one queued
and one received message, a sender-match boolean, and zero remaining entries;
it excludes message IDs, destination hashes, payloads, paths, and credentials.
The reverse case also passes. The old application submits once and exits. The
current application first defers the unauthenticated payload, requests one
sender path, and leaves the node transient intact. After authenticated announce
recovery without resending the message, a new current process waits for its
path-table restore worker, decodes the retained message, and acknowledges it.
This is node acceptance plus recipient sync evidence in both directions, not
peer-level delivery state for either sender. Propagation-node restart remains
unproven for abrupt termination.

The orderly node-restart case is now proven in the current-to-old direction.
The sender submits once; the Python router reports one queued transient and
exits cleanly. A new router process reopens the same storage with the same
propagation identity on a different loopback port, reports exactly one restored
entry, and serves it to the old recipient. Acknowledgement removes the entry.
No resend occurs. This does not claim crash-boundary or power-loss durability.

The isolated abrupt-process case now also passes after explicit persistence
evidence. The fixture requires its LXMF storage snapshot to change from the
pre-queue baseline, stabilize, and contain nonzero bytes before the harness
kills that exact node PID. Reopening preserves the node identity and one
transient, and recipient acknowledgement removes it. This proves recovery from
process termination after observed settled application storage, not physical
power-loss or filesystem/device durability.

The mixed stamp/ticket case now requires the current sender to follow the
Python propagation node's authenticated effective stamp policy and include a
fresh reply ticket in the encrypted LXMF message. Python queue admission proves
the propagation stamp passed enforcement. The hardened old application then
decodes the message and its 16-byte ticket and acknowledges the transient.
The retained summary exposes only policy/ticket validation booleans; it does
not retain stamp bytes, ticket bytes, message IDs, or identities. This proves
ticket carriage through stamped propagation, not subsequent ticket use on a
propagated reply.

This model does not change the OMENchat wire protocol, destination names,
identity format, SQLite schema, state roots, or persisted legacy delivery field
values.

## Read-only runtime diagnostics

The diagnostics service collects the project-owned lifecycle snapshot and the
complete typed capability snapshot alongside the existing network, interface,
propagation, and SDK/RPC status. Collection occurs only for an explicit
diagnostics preview/export; it does not introduce a periodic status poll or UI
redraw subscription. The TUI and desktop diagnostics views show a compact
lifecycle line and supported/unsupported/unknown capability counts after the
first collection. The redacted JSON retains individual capability records,
their evidence source, and their user-safe detail for troubleshooting.

Failure category and retryability may be shown. Technical failure detail is
always replaced with `<redacted>` in the exported snapshot because it can
contain private paths or backend-specific context. A missing capability remains
`unknown`; shared-instance ownership, interface mutation, tickets, and stamps
are not promoted merely because the 0.9 crates expose related APIs.

The network snapshot separately marks whether aggregate path-table,
path-request-failure, and shared-instance status values are authoritative. The
current integrated and mock adapters mark those three aggregate surfaces
unavailable: their former zero/false placeholders must not be presented as
observed facts. Diagnostics may still show bounded announce counts, known
destination cache count, pending announces, and typed interface samples because
those values come from current adapter-owned state. Per-destination path/hop
inspection remains available through explicit path diagnostics rather than an
invented aggregate table.

### Propagation-node inventory projection

The directory service derives one project-owned propagation-node inventory
from its existing bounded announce and saved-entry state. The projection is
read-only and capped at 256 records and 512 KiB; truncation is explicit and
does not delete saved or transient directory records. Ordering keeps the
selected node first, then saved/trusted and fresher evidence, with destination
hash as the deterministic final key.

A display name and advertised stamp cost are treated as authenticated only
when the directory entry contains the announce-bound identity hash. An entry
without that identity displays its destination hash, exposes no stamp policy,
and reports compatibility as unknown. Fresh, stale, and unknown timestamps and
known, not-known, and unknown path evidence remain distinct. Only a runtime
status for the matching selected node can turn path state into known or
not-known; passive directory presence never invents a route.

The projection performs no path request, sync, selection, timer, or storage
write. It is available to diagnostics and UI view models while the existing
preferred-node and propagation-sync owners remain authoritative.

Desktop and TUI Directory views consume a cached copy of this bounded
projection. The cache is rebuilt after directory, path-evidence, or
preferred-node changes, not during redraw. Both views expose explicit
selection, Refresh Node, Cancel Refresh, and Sync Now controls through the
existing application owners; rendering or merely highlighting a row never
starts network work.

Refresh Node has a 30-second monotonic cooldown, one global in-flight owner,
six-second total timeout, at most three path candidates, explicit coalescing,
and cooperative cancellation. Success, no path, timeout, cancellation, and
failure remain typed visible outcomes. Desktop/TUI shutdown cancels the owner.
Manual node selection no longer invokes the older generic path-request helper;
it only persists the selected hash and notifies the runtime adapter.

## Local identity announce policy

The identity action sends the existing normal `lxmf.delivery` announce. It is
explicitly labeled local and not targeted; the current runtime boundary does
not accept a target destination for this operation. Startup announcement
remains controlled only by `announce_on_start`, and no periodic announce timer
is added.

Manual, pre-send, and pre-inspection requests share a 30-second monotonic
cooldown and a single in-flight task. Concurrent requests are coalesced, and a
completed or failed attempt cannot immediately be repeated. If a deferred send
or inspection cannot start its required announce because of the cooldown or a
missing async runtime, the deferred action is cleared instead of lingering and
resuming after an unrelated later announce. Success, refusal, and failure each
produce an explicit user-visible result.

The live interoperability diagnostic remains a separately labeled network test
that deliberately announces as part of its evidence collection. This policy
does not claim the upstream 0.9 targeted-announce surface is integrated.

## Runtime ownership

Managed mode owns the integrated Reticulum runtime and its configured
interfaces. External mode is a preserved but deferred configuration state; it
is not a live capability. Application startup and the native adapter both
refuse integrated interface startup while External is selected. This prevents
the former configuration label from silently starting a second runtime beside
an operator-managed instance.

Diagnostics display configured ownership separately from negotiated shared
capability. A configured SDK/RPC endpoint can negotiate its own LXMF operations
and event stream, but does not establish a complete external Reticulum,
NomadNet, or OMENchat backend. See `docs/NETWORK_BACKENDS.md`.
