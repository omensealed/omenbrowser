# OMENchat Protocol

This document is the public compatibility contract between the OMENbrowser_rs
client plugin and the standalone `omenchatd` server.

## Transport

OMENchat uses Reticulum links for live room traffic. Larger history, userlist,
and media payloads may use Reticulum resources. LXMF is reserved for private
contact handoff and async notices, not normal room traffic.

### v0.6.0-1 / v0.9.5-1 compatibility boundary

The application release number does not version the OMENchat wire protocol.
The v0.9.5-1 migration retains protocol version `1`, protocol name
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

`fixtures/omenchat/v0_6_0_1_wire.rs` records public v0.6.0-1 session-open,
room-message, and history-resource-offer bytes plus the protocol and transport
labels. Both the browser and independently built server must encode those
fixtures byte-for-byte and decode them to the same typed frames. These tests
prove deterministic codec compatibility in both directions, but do not replace
the pending multi-process v0.6/v0.9 link, resource, restart, and reconnect
matrix.

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
for retry. Cleanup is deliberately link-scoped: the Reticulum terminal exposes
a transfer hash but not the OMENchat resource ID needed for safe per-offer
selection. This changes no operation number, field, metadata prefix, protocol
version, destination, or mixed-version wire behavior.

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
