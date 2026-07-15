# OMENchat Protocol

This document is the public compatibility contract between the OMENbrowser_rs
client plugin and the standalone `omenchatd` server.

## Transport

OMENchat uses Reticulum links for live room traffic. Larger history, userlist,
and media payloads may use Reticulum resources. LXMF is reserved for private
contact handoff and async notices, not normal room traffic.

The standalone server's quiet NomadNet portal is a separate request-resource
surface. Its established path request is limited before MessagePack allocation
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
