# OMENchat Protocol

This document is the public compatibility contract between the OMENbrowser_rs
client plugin and the standalone `omenchatd` server.

## Transport

OMENchat uses Reticulum links for live room traffic. Larger history, userlist,
and media payloads may use Reticulum resources. LXMF is reserved for private
contact handoff and async notices, not normal room traffic.

### v0.6.0-1 / v0.9.7-7 compatibility boundary

The application release number does not version the OMENchat wire protocol.
The v0.9.7-7 release retains protocol version `1`, protocol name
`omenchat-v0.1`, the six-item MessagePack frame layout, operation numbers,
legacy link context `0x4f`, and `omenchat-resource:` resource metadata.

reticulum-rs 0.9's high-level link delivery helper emits generic link data
with context `0x00`, and its `PacketContext` conversion maps unknown
application contexts such as `0x4f` to generic data. The clean adapter
therefore accepts only generic link data at the Reticulum boundary and then
requires a valid bounded OMENchat frame before protocol dispatch. omenchatd
accepts a valid frame received as either generic context `0x00` or the legacy
`0x4f` context, ignores context-zero non-frames, and keeps legacy `0x4f`
responses for compatibility. This is transport adaptation; it does not change
the OMENchat frame protocol.

The exact 0.9.7 implementation temporarily caps upload negotiation and
admission so `3 + len("omenchat-resource:") + len(resource_id) + payload`
does not exceed 1,048,575 bytes. The configured value is retained, the default
512 KiB limit is unchanged, and no operation or capability identifier changes.
Unsafe Resource fetches return the existing bounded error operation before a
Resource-offer frame is sent.

`fixtures/omenchat/v0_6_0_1_wire.rs` records public v0.6.0-1 session-open,
room-message, and history-resource-offer bytes plus the protocol and transport
labels. Both the browser and independently built server must encode those
fixtures byte-for-byte and decode them to the same typed frames. These tests
prove deterministic codec compatibility in both directions, but do not replace
the pending multi-process v0.6/v0.9 link, resource, restart, and reconnect
matrix.

The protocol v1 enums, operation/error numbers, frame body/value types, and
public compatibility fixture are owned by the private `omenchat-protocol`
crate under `src/server/crates/`. Both browser and server re-export those types
through their existing module paths, so application code does not depend on a
new API surface. Their bounded codec implementations remain separate and both
must pass the same shared fixture bytes. The crate contains no transport,
runtime, storage, GUI, TUI, or server policy and remains part of the relocatable
standalone omenchatd source tree.

The standalone server's quiet NomadNet portal is a separate Reticulum request
surface. It accepts direct request-context packets for requests within packet
MDU and request Resources for oversized requests. Its established path request
is limited before MessagePack allocation
to 4 KiB input, 1 KiB scalar values, 32 container items, 64 total values, and
four nested levels, with no trailing data. This does not change the portal path
hash or response encoding.

Queue overload does not change the wire format. A server may reject outbound
work or drop an inbound payload before protocol dispatch when its documented
item, byte, or per-link budgets are exhausted. Clients must treat the resulting
missing response/link closure as transient overload and use their existing
bounded retry/reconnect behavior; they must not retry in a tight loop.
The browser's per-link adapter uses 64-item/4 MiB inbound and outbound frame
queues and a four-item/16 MiB outbound resource queue. These are local
backpressure limits only: frames and resources admitted within the existing
protocol ceilings retain their exact encoding.

Client and server frame and batch encoding use rmpv's borrowed MessagePack value tree.
Binary and string fields are written from their existing buffers rather than
copied into an owned intermediate tree. Decode remains owned, and the six-item
frame layout and encoded bytes are unchanged.

### Frame decode budgets

Client and server validate a complete MessagePack frame before constructing its
value tree. Current limits are 1 MiB encoded frame bytes, 512 KiB per string or
binary scalar, 4,096 items per container, 8,192 values across the frame, and 16
nested container levels. Exactly one MessagePack object must consume the input;
trailing objects or bytes are malformed. These limits do not alter the encoding
of accepted frames and remain above current inline upload, history, and user-list
producers. Large history and media continue to use bounded Reticulum resources.

The browser applies narrower local semantic limits after frame decoding and
before retaining UI metadata: 256-byte server/user/actor display names, 64-byte
room names, 4 KiB topics and status/errors, 16 KiB MOTDs, 4 KiB exact resource
IDs and filenames, and 1 KiB content types. Display-only values may be
UTF-8-safely shortened; operational room/user labels and resource identifiers
are rejected rather than rewritten. This policy does not change the codec
limits or accepted frame representation for other implementations.

The declarative page descriptor is a local discovery surface rather than a wire
frame. The browser accepts at most 64 KiB/128 lines per `[omenchat]` block and
32 KiB per line. Room hints and capabilities are limited to 64 items; room
hints use the 64-byte operational room-name limit and capabilities use 128 bytes
per item/8 KiB total. Micron OMENchat link metadata is limited to 32 fields and
16 KiB. Oversized exact fields or collections reject the descriptor/link before
session creation; display names remain UTF-8-safely shortened. Accepted
descriptor keys and lowered link syntax are unchanged.

The browser's Directory may persist the public identity hash authenticated by a
Reticulum `omenchat.node` announce. This is local discovery metadata, not an
OMENchat frame or descriptor field. Identity hashes must be exactly 32
hexadecimal characters, and a different identity cannot replace the identity
already bound to the same destination record. `announce-verified` describes
transport evidence only; it does not grant application trust.

Compressed history and user-list batches additionally have 4 MiB compressed and
4 MiB uncompressed ceilings. The advertised uncompressed length is checked
before decoding, and bzip2 output is streamed only through that length plus one
sentinel byte so a false length cannot cause unbounded expansion. Accepted
compression and batch encodings are unchanged.

Before constructing decoded batch values, both packages also enforce 4 MiB per
scalar, 16,384 items per container, 65,536 total values, 16 nested levels, and
exact consumption of one MessagePack object. These wider batch-shape limits
accommodate configured history and live user-list collections without weakening
the smaller live-frame limits.

For resource-backed history and user lists, the client validates the offer
before retaining it: the resource identifier must be non-empty and at most
4 KiB, advertised compressed/uncompressed sizes must fit the 4 MiB ceilings,
and the purpose must match the operation (`history` or `userlist`). Once the
resource arrives, its embedded compression, uncompressed length, and compressed
payload length must exactly match the offer before decompression or application
state mutation. A mismatch consumes/removes the received resource and reports a
protocol error; it is not silently reinterpreted as another batch type.

An inbound Reticulum resource failure or cancellation is a local transport
lifecycle event, not a new OMENchat frame. The desktop releases pending
history/user-list offers owned by that live link and leaves the link connected
for an explicit retry. Server outbound Resources retain a bounded association
between the application resource ID and the exact Reticulum hash until a
terminal, link close, shutdown, or six-hour TTL. The pinned 0.9.7 inbound
failure shape carries the exact hash and expected size but not the OMENchat
metadata/application ID. Server upload cleanup therefore removes one offer only
when the authenticated identity and expected size identify exactly one pending
candidate. An unmatched or ambiguous failure removes none; identity-wide
cleanup is reserved for link close/disconnect/replacement and expiry. This
changes no operation number, field, metadata prefix, protocol version,
destination, or mixed-version wire behavior, and it does not authorize an
automatic retry.

## Client URI

```text
omenchat://<destination_hash>
```

The destination hash identifies the chat server.

## Core Flow

1. Client requests/learns a path for the server destination.
2. Client opens a Reticulum link.
3. Client sends `SessionOpen`.
4. Server replies with `SessionAccept`.
5. Client joins a room.
6. Server replies with room state, userlist, topic, and recent history.
7. Client and server exchange room events.

### Capability negotiation boundary

Current activation note: the paragraphs below retain the staged design history,
but the negotiated room text, leave, and supported mutating-command families
documented below now use durable transmission. The browser
advertises only when the authenticated identity owns a healthy persistent
intent worker and client-instance ID. omenchatd accepts only on the
authenticated Link that requested it. Legacy, downgraded, unknown, and
unsolicited capability paths remain unchanged, and restart recovery never
retries automatically.

The replay store retains the exact bounded origin frame. On replay, omenchatd
preserves its operation, room, and body but replaces the transient sequence
with the current request sequence. This is required for a client to correlate
the acknowledgement after Link replacement or restart and does not repeat the
mutation, rate accounting, or fan-out.

The browser recognizes conflict (1013) and replay-expired (1014) as terminal
only when the authenticated server response sequence matches an outstanding
durable mutation. It then removes that mutation's optimistic local echo and
persists `conflict` or `expired`. Uncorrelated errors and nonterminal outcomes,
including store-busy (1015), do not terminalize or silently retry uncertain
work.

The shared protocol crate defines the bounded optional negotiation extension.
Existing `SessionOpen` fields remain protocol name, display name, and optional
client LXMF destination at indexes 0 through 2. Requested capabilities and a
16-byte client-instance ID occupy trailing indexes 3 and 4. Accepted
capabilities occupy trailing `SessionAccept` index 6.

Capability lists are limited to 64 unique ASCII names of at most 128 bytes.
Requesting `durable-mutations-v1` requires an exact 16-byte client-instance ID.
Missing trailing fields mean no capabilities; application version and
descriptor metadata never imply acceptance. Unknown well-formed capabilities
receive the unchanged legacy response. A malformed
trailing negotiation receives error 1012 and never marks the Link handshake
complete; the client may correct the request and retry `SessionOpen` on that
Link. Handshake completion requires an actual `SessionAccept`, not merely an
inbound frame carrying the `SessionOpen` operation number.

### Authoritative production capability matrix

This table describes the canonical `desktop-product` client and
`server-headless`/`server-full` server at 0.9.7-7. Capability names come from
`omenchat-protocol::KNOWN_SESSION_CAPABILITIES`; deterministic tests check the
shared vocabulary, the client's request, and the canonical server's acceptance.
Definition alone never activates a capability: each Link must request it and
receive explicit acceptance.

| Capability | Defined | Client requests | Server accepts | Handler live | Persisted | UI available | Mixed-version evidence | Status |
|---|---:|---:|---:|---:|---:|---:|---|---|
| `durable-mutations-v1` | yes | yes | yes | yes | replay result and outbound intent | uncertain/retry controls | v0.6 fixture and downgrade tests; prior-binary live lane separate | Production behavior |
| `durable-room-notice-ack-v1` | yes | yes | with durable mutations | yes | bounded notice/intention state | recovery detail | downgrade tests; prior-binary live lane separate | Production behavior |
| `reply-mentions-v1` | yes | yes | with durable mutations | yes | reply target and mention IDs | reply, counts, filtering | legacy event shape and downgrade tests; prior-binary live lane separate | Production behavior |
| `reactions-v1` | yes | yes | with durable mutations | yes | bounded current state and audit | add/remove and snapshots | base-only peer isolation and downgrade tests; prior-binary live lane separate | Production behavior |
| `message-revisions-v1` | yes | yes | with durable mutations | yes | bounded current revision and audit | correction/tombstone | base-only peer isolation and downgrade tests; prior-binary live lane separate | Production behavior |
| `room-pins-v1` | yes | yes | with durable mutations | yes | bounded pins and audit | pin/unpin and jump target | base-only peer isolation and downgrade tests; prior-binary live lane separate | Production behavior |
| `announcement-rooms-v1` | yes | product feature | product feature | yes | room policy and revision | read-only policy/action gating | exact legacy room fallback; prior-binary live lane separate | Production behavior |
| `room-slow-mode-v1` | yes | product feature | product feature with durable mutations | yes | room interval and bounded admission | interval/retry evidence | legacy fallback/rejection tests; prior-binary live lane separate | Production behavior |
| `room-media-policy-v1` | yes | product feature | with durable + announcement + slow mode | yes | nullable room ceiling | attachment admission/effective limit | process and adjacent-shape fallback; prior-binary live lane separate | Production behavior |
| `moderation-audit-v1` | yes | product feature | product feature | authorized read only | bounded audit records | moderator/administrator panel | downgrade/authorization tests; prior-binary live lane separate | Production behavior |

Persistence means authoritative server or client recovery state survives
restart; it does not mean unlimited retention. Every store, snapshot, page,
intent, and audit path keeps its documented item/byte/age bounds.
“Mixed-version evidence” deliberately distinguishes deterministic downgrade
coverage from an actual older-binary process lane.

#### Historical design record

Earlier revisions reserved operations 35–39 and staged
`message-revisions-v1` without production acceptance. That statement is
historical: the current canonical client requests it, omenchatd accepts it only
beside durable mutations, and its handler, persistence, snapshots, fan-out, and
UI are active. Peers that do not negotiate it retain ordinary protocol-v1
history and message behavior.

The shared contract also contains the production `room-media-policy-v1`
vocabulary. It reserves no operation number. Canonical current clients and
servers request and accept it only with `durable-mutations-v1`,
`announcement-rooms-v1`, and `room-slow-mode-v1`, and therefore selects the
cumulative seven-field room value:

```text
[room_id, name, topic_or_nil, room_revision, policy_bits,
 slow_mode_seconds, room_upload_max_file_bytes_or_nil]
```

The final scalar is `nil` for inherited server policy, zero for disabled room
uploads, or at most 10 MiB. The typed upload-rejection extension keeps
the existing reason/quota/incoming fields and appends a numeric reason code.
Exact fixtures pass in both independent codecs. Qualification clients and
servers prove request/acceptance, authenticated-Link shape ownership,
identity replacement cleanup, reconnect cleanup, and current/current
projection over a real isolated Link. Canonical profiles now activate that
reviewed boundary. omenchatd schema 13 stores the nullable room ceiling and
provides a guarded schema-12 copy export.

The production server applies room upload admission only when the
current authenticated Link owns negotiated media-policy authority. It rechecks
the same authority at Resource publication. Negotiated disabled and over-limit
rejections append stable numeric codes `1` and `2`; the client exposes the
typed code only for a negotiated session and treats unknown codes as generic
rejection. Non-negotiating peers retain the exact three-field rejection and
legacy admission behavior.

The shared client projection distinguishes absent negotiation from inherited,
disabled, and bounded room policy. Seven-field negotiation carries that
projection into the desktop client's existing 256-room-per-session map. Static
Iced presentation shows the effective room/server minimum and disables Attach
for authoritative disabled evidence. Production runtime shape selection
produces that evidence only after all cumulative capabilities are accepted.
Current/current Resource, rejection, restart, adjacent fallback, GUI, and
bounded-process gates passed before activation. Receiver-side Resource
cancellation remains unavailable through the locked upstream public API and is
not presented as a supported action.

The browser persists the client-instance value under its active identity-scoped
application storage and retains it in live client state. Invalid, unsafe, or
overly permissive stored state disables durable negotiation instead of
generating a replacement; ordinary legacy OMENchat remains available.

The persistent-intent boundary records a negotiated mutation's server
destination, authenticated identity binding, stable client/mutation IDs,
canonical request hash and body, expiry, state, and local correlation before it
can be handed to a transport owner. Recovery length-preflights SQLite values
before allocation and then revalidates frame metadata, canonical hashing, and
retained-byte accounting. Negotiated room-message, `/me` room-action,
`/notice`, `/part`, `/topic`, `/create`, `/role`, `/unban`, `/kick`, `/ban`,
`/mute`, and `/unmute` sends call this boundary before transport. A durable
notice activates
only when the server also accepts `durable-room-notice-ack-v1` and then receives
a kind-3 `MessageAck`. An older, ordinary, or downgraded protocol-v1 notice
retains its legacy `RoomEvent` response and is never persisted as a durable
notice intent. Identity-prefix-only administration targets retain their legacy
path because the result does not expose the identity hash needed for exact
correlation.
Prepared intents can move only to uncertain, expired, or abandoned; uncertain
intents can move only to acknowledged, conflict, expired, or abandoned.
Terminal states cannot regress. Recovery returns only prepared/uncertain rows,
and incremental maintenance removes at most 128 terminal rows older than 30
days. A dedicated 32-request/2-MiB storage owner starts only when its identity
and persistent client-instance prerequisites are satisfied.

Recovered intents are presented without their mutation identifier, request
hash, or message/command body. Each bounded row shows only a semantic operation
kind, public server label, room scope, prepared/uncertain state, and relative
expiry. Send/retry is offered only when the same production guard confirms the
original identity, client instance, server, room, live transport, negotiated
capability, and absence of an in-process pending result. An unavailable retry
shows the redacted reason and retains only the explicit stop-tracking action.
Nothing is resent automatically.

The production server store has deterministic post-retention behavior. Before
pruning any durable result, it permanently retires that authenticated
identity/client-instance pair. Any later operation under the retired instance
returns `Expired` before mutation execution, even after server restart.
Remembered active and retired instances are bounded (100,000 globally and
1,024 per authenticated identity), and admission fails closed at capacity.
The intent store can rotate the owner-only instance file only while an
immediate SQLite transaction proves there are no prepared or uncertain intents.
It never rewrites terminal historical intents. Protocol-v1 codes 1011 through
1015 are reserved respectively for not-negotiated, malformed, conflict,
result-expired, and store-busy durable outcomes. Production negotiates and
emits these outcomes where applicable, but never automatically retries
uncertain work.

The store additionally has an active atomic room-event primitive: event
insertion and the exact encoded origin response are committed together. A new
result carries the event for one-time fan-out; an exact replay carries
only the retained response. Invalid response encoding rolls back the event.
Live durable handlers compose authorization, membership, rate reservation,
negotiated client-instance ownership, and broadcast timing without double
accounting or duplicate fan-out. The durable store finisher runs only on a
replay miss, returns the reservation with a successful first commit, and
releases it automatically on rollback. This mechanism remains inactive for a
Link until it has negotiated and bound a client-instance identifier.

The live server now stages session display/LXMF metadata until a real
`SessionAccept` is produced, so malformed negotiation cannot mutate the
retained peer record. A future durable client-instance binding requires both a
valid request and explicit capability acceptance on an authenticated Link. The
binding is Link-scoped, bounded by active-Link admission, identity-bound, and
cleared on identity replacement or every Link retirement path. Production
continues to return the legacy accept, so well-formed durable requests remain
unbound and cannot send durable envelopes.

An inactive session executor now implements the negotiated semantics for room
messages, actions, notices, and part-room operations behind the staged live
Link gate. It checks
the canonical hash, stores mutation and exact origin result transactionally,
returns a broadcast event only for first execution, preserves terminal policy
rejections, avoids a second rate charge, and replays across server restart.
Errors 1012 through 1015 have executor mappings. Current live peers can receive
1011 for an unnegotiated durable envelope or 1012 for malformed negotiation or
envelope data, but cannot activate successful durable execution.

Live dispatch recognizes the durable envelope marker before applying the
protocol-v1 same-Link sequence replay cache. A tagged malformed envelope returns
1012, while a valid envelope without an authenticated, identity-matched durable
binding returns 1011. With a binding, the durable replay record is authoritative:
the first execution can broadcast one room event and exact replay returns only
the originally encoded origin acknowledgement. Capability acceptance is still
disabled, so production peers cannot create that binding yet.

For durable `PartRoom`, membership deletion, the departure event, the exact
legacy-compatible `CommandResult`, and replay publication commit atomically.
Only first execution changes live room ownership and emits a refreshed user
list. Replay returns the retained result without repeating those effects.
The negotiated desktop sender persists an empty-body PartRoom intent before
transport and does not change local membership on queue admission or frame
send. Only a matching Link sequence, room identity, `part` command, and returned
room identity retires the pending intent and applies the server result. A lost
result remains uncertain across reconnect or restart and requires an explicit
user retry; it is never resent automatically.

Durable `RoomNotice` retains the moderator/admin decision and exact origin room
event in the same transaction as event insertion. Only first execution is
fanned out to other room Links; replay returns the retained origin event
without another rate charge or broadcast.

Durable `topic` and `create` commands atomically retain their exact
`CommandResult` with the room update. Their `RoomDelta` is a one-use live effect
returned only for first execution. Exact replay cannot increment a room
revision, recreate a room, consume another command-rate slot, or repeat the
delta. Durable `role` and `unban` commands use the same transaction boundary for
the user mutation, optional audit event, and retained result. Their bounded
first-execution effects contain a `UserDelta` and, when room-scoped, a
`RoomEvent`; replay emits neither. Durable `kick`, `ban`, `mute`, and `unmute`
resolve only active room peers and commit the target status change, audit event,
and exact result together. First execution emits the bounded deltas; `kick` and
`ban` additionally carry a one-use target-identity disconnect effect that runs
immediately after commit and before response I/O. Replay cannot disconnect a
replacement Link.

The negotiated desktop sender activates `topic`, `create`, `role`, `unban`,
`kick`, `ban`, `mute`, and `unmute` from this command family. It persists the
normalized `topic` command under the active room before
transport, retains the prior local topic while the result is uncertain, and
accepts only an exact sequence, room, command name, and returned room identity.
`create` is persisted with no request room and adds no room locally until the
exact sequence, roomless result, command name, and server-normalized requested
room name match. Role and unban results must match the original room,
catalog-known numeric user ID or exact display name, and requested role or
cleared-ban state. Active-peer moderation uses the same user correlation and
requires the requested ban/mute state. Kick has no durable user-state bit, so
its exact correlated result removes the target from the active user list.
Kick and ban disconnect only the identity selected during first execution;
replay cannot disconnect a replacement Link.
Identity-prefix-only targets remain on the legacy path because the command
result does not expose identity hashes. Old or downgraded peers retain the
protocol-v1 command path. `rooms` remains read-only.

## Operation correlation and same-link replay

The existing 32-bit frame `seq` is the request/response correlation identifier.
The browser allocates it monotonically from its live client state and retains
that state across an in-process reconnect. Protocol v1 does not include a
persisted client-session nonce, so `seq` is not globally unique across process
restart and must not be treated as a durable `(identity, seq)` key.

omenchatd suppresses exact same-link replays of `RoomMessage`, `RoomAction`,
`RoomNotice`, `PartRoom`, and mutating commands (`topic`, `create`, `kick`,
`ban`, `mute`, `unmute`, `role`, and `unban`). It retains the canonical request
bytes and origin response before delivery. An exact `(link, seq, request)`
replay receives the same acknowledgment or response without another database
mutation, rate-limit charge, room fan-out, user-list fan-out, or moderation
disconnect. Read-only `rooms` commands are not retained. Reusing `(link, seq)`
with different content returns `MalformedFrame`. Closing or replacing the link
removes its replay entries.

Part and kick/ban transport side effects are response-gated: a part changes the
live link's room only after a successful `part` result, and a moderation target
is disconnected only after a successful `kick` or `ban` result. Denied,
malformed, missing-target, and rate-limited operations therefore cannot change
live transport ownership.

The cache is limited to 1,024 entries/4 MiB globally, 64 entries/256 KiB per
link, and 64 KiB per entry. Monitoring reports hits, collisions, rejected cache
admissions, retained items, and retained owned-capacity bytes. This is a server execution
guard, not a frame change: operation numbers, body fields, sequence encoding,
and acknowledgments remain byte-compatible. Cross-link and post-restart retry
idempotency require a separately negotiated/versioned session identifier and
remain unsupported; the current client does not silently resend an unacknowledged
room mutation across those boundaries.

## Rooms

Servers may expose multiple rooms. Room creation is admin-only. Topic changes
are admin/moderator operations.

## Moderation

Supported concepts:

- owner/admin/moderator roles;
- kick;
- ban;
- user records;
- room permissions;
- upload permissions and quota limits.

## Media

Uploaded media is server-hosted under the server home. Clients cache media under
their identity-specific browser storage. Media display follows OMENbrowser_rs
privacy policy: Reticulum/NomadNet media is treated differently from clearweb
HTTP/HTTPS media.

The server validates the advertised resource length against the received
bounded resource before storage. Storage commit ordering is an implementation
durability guarantee and does not alter upload frames or resource metadata.
The browser's local live-transfer admission retains at most four outgoing
offers/16 MiB and 16 inline downloads/16 MiB, with an 8 MiB per-resource cap.
Inline assembly also caps out-of-order fragments at 1,024 per resource and
requires stable metadata and retained payload no larger than the declared
length. Local overload aborts or refuses the affected transfer without changing
frame encoding, operation numbers, or server-side quotas.

## History

On join or reconnect, clients request a bounded recent-history sync. `Load
Older` requests older history before the current local window. Clients should
deduplicate events by server id, room id, and event id.

The browser client keeps a bounded in-memory history window per session while
retaining accepted history in its persistent store. Directional local eviction
and later re-pagination are implementation details and do not alter history
frames, event ordering identifiers, or server interoperability.
The browser also bounds its local catalog view to 64 open sessions, 256 rooms
and 512 KiB of room metadata per session, and 1,024 users and 1 MiB of user
metadata for the active room. Excess session opens fail locally, while excess
snapshot entries remain a server/persistent-store concern and are not reflected
back as a protocol mutation. These admission limits therefore require no wire
version negotiation.
