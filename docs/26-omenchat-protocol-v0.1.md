# 26 — OMENchat Protocol v0.1

## Scope

OMENchat is a first-party chat client plugin in OMENbrowser_rs plus an independent standalone Rust server named `omenchatd`.

The browser client and server must communicate only through this protocol contract. They must not share Rust modules, path dependencies, or app state. Shared fixtures are allowed when they are neutral protocol examples rather than imported code.

The server must remain movable:

```text
cp -a src/server /tmp/omenchatd
cd /tmp/omenchatd
cargo check
```

## Transport Model

The live room transport is Reticulum Link based. Reticulum Resource transfers are used for larger compressed history/userlist/media payloads. LXMF is used for private-contact handoff and async notices, not for normal room traffic.

NomadNet/MicronPlus pages are discovery surfaces only. They may link to OMENchat but do not carry live room traffic.

## Descriptor Discovery

The client supports a direct link:

```text
=> omenchat://<destination_hash> Open OMENchat
```

Clients still accept the legacy `omenchat:<destination_hash>` form for already-published pages, but new pages and UI surfaces should emit `omenchat://<destination_hash>`.

It also supports a declarative MicronPlus-style block:

```text
[omenchat]
server = "<omenchat_destination_hash>"
lxmf = "<optional_server_lxmf_destination_hash>"
name = "Node Chat"
descriptor = "/omenchat/descriptor"
theme = "field-terminal"
rooms_hint = "lobby,radio,support"
```

The descriptor is metadata, not executable content.

## Core IDs

```rust
pub type ServerId = String;
pub type RoomId = u32;
pub type UserId = u32;
pub type EventId = u64;
pub type Seq = u32;
pub type Revision = u64;
```

## Compression

```rust
#[repr(u8)]
pub enum Compression {
    None = 0,
    Bzip2 = 1,
}
```

## Frame Shape

Frames are compact MessagePack arrays:

```text
[version, op, flags, seq, room_id_or_nil, body]
```

The body is opcode-specific. Routine hot-path bodies should prefer arrays over string-keyed maps.

## Batch Bodies

Room history and userlist snapshots use compressed batch bodies when they are sent inline:

```text
[compression, uncompressed_len, compressed_msgpack_bytes]
```

`compressed_msgpack_bytes` decompresses into a MessagePack array of compact values. The current required compression is `Bzip2`.

When the compressed body is larger than the server inline threshold, the server sends a Resource offer instead:

```text
[resource_id, compression, uncompressed_len, compressed_len, purpose]
```

`purpose` is currently `history` or `userlist`. The resource payload has the same compressed MessagePack batch shape as an inline payload, but it is transferred with Reticulum Resource instead of inside the Link frame.

Servers must keep the offered payload available long enough for the Link transport to initiate the Resource transfer. Clients treat a missing Resource payload as an incomplete transport event rather than as lost chat history.

OMENchat Link packets use an application-specific link context so they do not collide with NomadNet page request traffic on shared native runtime plumbing. Resource metadata begins with `omenchat-resource:` followed by the Resource offer id.

Servers should route Link data only after associating the Link id with a peer identity. Packets on unknown links or with a non-OMENchat context are ignored and counted for monitoring rather than treated as protocol errors.

The standalone server's optional `live-rns-net` build uses `send_on_link` with the OMENchat context for response frames and `send_resource` with OMENchat Resource metadata for oversized batches.

When the native transport reports `on_remote_identified`, the server upgrades the Link's peer record from a provisional Link-based identity to the reported Reticulum identity hash. Until then, the server may use a provisional Link id based peer key only to keep the session routeable.

The standalone server's RNS application name is `omenchat`. The chat service announces as `omenchat.node`; this is intentionally fixed in the admin UI so operators do not accidentally fragment discovery across custom announce labels.

This is intentionally separate from `nomadnetwork.node`. OMENchat discovery is a first-class OMENchat service announce, not a fake NomadNet node announce. When `omenchatd` serves a NomadNet portal page, it registers and announces that as a separate `nomadnetwork.node` destination/page service for the same identity. The Micron portal is only a quiet discovery surface that can show MOTD/rooms and link to `omenchat://<destination_hash>`; chat traffic remains on the OMENchat Link protocol. The canonical portal request path is `:/page/index.mu`, mirrored on disk under the standalone server's Reticulum storage at `reticulum/storage/pages/index.mu`.

The client live driver treats `SessionOpen` and `JoinRoom` as the initial request pair for an opened `omenchat://<destination_hash>` pane. `SessionOpen` may use an empty body, but current clients should send `[protocol_name, display_name, optional_lxmf_destination]` so servers can present a human-readable user list instead of provisional Link labels. The server's `SessionAccept` room list is cached in the client and rendered as joinable room buttons. Room sends use `RoomMessage` with the active room id. Upward scrollback uses `HistoryBefore` with the oldest known event id for the active room. Returned `JoinAccept`, `RoomEvent`, inline batches, and Resource-backed batches update the same cached session model used by the desktop pane.

In the desktop client, OMENchat Link data and Resource callbacks are routed as OMENchat-specific runtime events by context/metadata before LXMF decoding is attempted. Each live pane owns one Link transport keyed by Link id, so incoming frames cannot bleed into unrelated browser or conversation panes.

Desktop clients use the existing `Ping`/`Pong` opcodes as a low-noise Link health check. A client sends `Ping` only after the Link has been idle long enough to be suspicious, treats any inbound frame as proof of life, and marks the pane disconnected if a pending `Pong` never arrives. Sends must not fall back to mock behavior when a real OMENchat session has no live Link.

Resource offer frames may arrive before the matching Resource payload callback. Clients defer unmatched Resource offers by id and replay the original offer through normal batch decoding when the payload arrives. This keeps history/userlist delivery independent of callback ordering.

The headless client smoke command uses the same live driver and runtime Link methods as the desktop pane. Its success criteria are: native runtime started, OMENchat Link opened, `SessionOpen`/`JoinRoom` sent, room joined, `RoomMessage` sent, and the resulting room event observed on the Link.

## Rich Media Privacy Policy

OMENchat messages may contain links to media. Rendering policy is client-side and must be enforced before any bytes are fetched:

- Reticulum/NomadNet media links may render inline automatically because they remain inside the Reticulum/NomadNet transport path.
- Clearweb `http://` and `https://` image links must not render inline by default.
- If the user enables remote media and the configured SOCKS5/Tor proxy is reachable, clearweb images may render inline through that proxy.
- Proxy detection checks the configured SOCKS5 port first, then the common local Tor ports `9050` and `9150`; `9150` is used by Tor Browser Bundle.
- If the proxy is disabled or unavailable, the chat pane should show a small explicit load/open control next to the link instead of fetching automatically.
- `.onion` links must never fall back to direct TCP. They require a reachable SOCKS5/Tor proxy for inline fetches or a Tor-capable external browser selected by the user.
- Non-image clearweb links continue to use the external browser prompt.

The shared Rust policy lives in the browser crate's media layer so OMENchat and NomadNet rendering use the same privacy decision. The OMENchat timeline applies this policy while rendering message text: Reticulum/NomadNet media is marked safe for inline loading, clearweb media is blocked behind an explicit control unless the user's remote-media and SOCKS5/Tor settings allow it, and non-image clearweb links are routed to the external browser prompt. Reticulum/NomadNet image links expose a `Load` action that uses the browser runtime download path and stores the file under the identity-scoped OMENchat media cache.

Clearweb image links expose a `Load` action only when remote media is enabled and a SOCKS5/Tor proxy is detected. The client fetches those bytes through `socks5h://host:port` so name resolution also stays behind the proxy, rejects non-image responses, caps the cached object size, and renders the cached image inline after the fetch succeeds. There is no direct-TCP fallback for clearweb or onion media.

Server-side uploads are governed by the standalone server policy advertised in
`SessionAccept`. `upload_quota_bytes` is the per-identity rolling upload cache
quota, defaults to 50 MiB, and `0` disables uploads. `upload_max_file_bytes` is
the per-upload file size cap and defaults to 512 KiB. Accepted uploaded files
must live under the server home upload cache, grouped by sender identity hash,
and the server must evict the oldest files for that identity before accepting a
new file that would exceed quota.

Upload transfer begins with a bounded Link-frame handshake before any file bytes
are accepted:

```text
UploadOffer  body = [filename, byte_len, content_type_or_nil]
UploadAccept body = [resource_id, quota_bytes, incoming_bytes, evict_count]
UploadReject body = [reason, quota_bytes, incoming_bytes]
```

Servers must reject offers from users who are banned, muted, not joined to the
target room, over quota, or when uploads are disabled. A successful
`UploadAccept` reserves/approves the transfer path. The client then sends a
Reticulum Resource using OMENchat Resource metadata with the accepted
`resource_id`. The server stores the bytes through its per-identity upload cache
policy and responds with:

```text
UploadComplete body = [resource_id, filename, stored_bytes, evict_count]
```

The server emits a typed room upload event after storing the file so other
joined clients can see that an upload occurred. The browser client can send
uploads through `/upload <path>` or the composer attach button's native file
picker. Before reading/sending the file, the client applies the advertised
server policy and rejects empty, disabled, or oversized uploads locally when
possible. Accepted local image/GIF uploads are copied into the active identity's
OMENchat media cache so the sender can render/open the upload without waiting
for a later fetch.

Stored uploads can be fetched by joined room members with:

```text
UploadFetch         body = [resource_id]
UploadResourceOffer body = [resource_id, filename, stored_bytes, content_type_or_nil]
UploadInlineChunk   body = [resource_id, offset, total_bytes, bytes]
```

For stored uploads at or below the negotiated per-file cap, the server may reply
with ordered `UploadInlineChunk` Link frames instead of requiring a Reticulum
Resource fallback. The desktop client assembles chunks, reports progress, caches
the received bytes under its identity-scoped OMENchat media cache, and renders
supported images and animated GIFs inline. Larger or future transfer modes may
still use `UploadResourceOffer`; after that offer, the server offers a
Reticulum Resource using the same `resource_id` metadata. Upload timeline
controls are compact inline icon actions for load/download/open rather than
separate debug attachment rows.

## Opcodes

```rust
#[repr(u16)]
pub enum ChatOp {
    SessionOpen = 1,
    SessionAccept = 2,
    SessionReject = 3,

    JoinRoom = 10,
    JoinAccept = 11,
    PartRoom = 12,
    RoomSubscribe = 13,
    RoomUnsubscribe = 14,

    RoomMessage = 20,
    RoomAction = 21,
    RoomNotice = 22,
    RoomEvent = 23,

    UserListSnapshotInline = 30,
    UserListSnapshotResource = 31,
    UserDelta = 32,
    RoomDelta = 33,
    RoleDelta = 34,

    HistoryBefore = 40,
    HistoryInline = 41,
    HistoryResourceOffer = 42,
    HistoryEnd = 43,

    Command = 50,
    CommandResult = 51,

    ContactRequest = 60,
    ContactOffer = 61,
    ContactAccept = 62,
    ContactReject = 63,

    UploadOffer = 70,
    UploadAccept = 71,
    UploadReject = 72,
    UploadComplete = 73,
    UploadFetch = 74,
    UploadResourceOffer = 75,

    Error = 90,
    Ping = 100,
    Pong = 101,
}
```

## Room Event Codes

```rust
#[repr(u16)]
pub enum RoomEventCode {
    UserJoined = 1,
    UserParted = 2,
    UserQuit = 3,
    UserKicked = 4,
    UserBanned = 5,
    UserUnbanned = 6,
    TopicSet = 7,
    ModeChanged = 8,
    RoleChanged = 9,
    MessageEdited = 10,
    MessageDeleted = 11,
    RoomNotice = 12,
}
```

## Error Codes

```rust
#[repr(u16)]
pub enum ChatErrorCode {
    PermissionDenied = 1001,
    NotJoined = 1002,
    RoomNotFound = 1003,
    UserNotFound = 1004,
    RateLimited = 1005,
    HistoryUnavailable = 1006,
    MalformedFrame = 1007,
    UnsupportedProtocolVersion = 1008,
    CompressionUnsupported = 1009,
    ResourceUnavailable = 1010,
}
```

Clients render common error text locally from codes. Servers should not send free-form UI copy for routine protocol errors.

## Session Open

Session open is a compact cache/capability handshake after Reticulum Link establishment. It is not an IRC/RRC-style `HELLO`/`WELCOME` text flow.

The client sends protocol version, client name/version, capabilities, supported compression, known dictionary revisions, and room resume cursors.

The server replies with server id/name/epoch, local user id, server capabilities, dictionary revisions, joined room summaries, and an optional quiet MOTD string. The desktop client renders MOTD as server context above the timeline, not as chat history and not as a repeated room event.

## Join and History Rules

When joining a room, the client requests:

- current userlist snapshot;
- latest 50 events;
- known dictionary revisions.

The server replies with `JoinAccept` plus either inline follow-up data or a Resource offer for compressed data.

Older history is requested only on upward scroll in pages of 50. Duplicate history pages must merge idempotently.

## Userlist Rules

User and room IDs are server-local numeric IDs. Hot-path events use IDs, not repeated display names or hashes.

Normal userlist snapshots do not expose full RNS identity hashes or LXMF addresses. They may include whether LXMF contact is available.

Room event values carry the current actor display label as an optional cache hint:

```text
[event_id, kind, actor_user_id_or_nil, at_unix, body, actor_display_name_or_nil]
```

Clients still treat `actor_user_id` as the stable room-local actor key. The display label is persisted so historical messages do not render as `unknown` when the actor is not in the live userlist, and it can be backfilled if older cached events were received before this field existed.

Current event `kind` values:

```text
1 = message
2 = action
3 = notice
4 = system
5 = upload
```

Upload events extend the compact event array with the stored upload metadata:

```text
[event_id, 5, actor_user_id_or_nil, at_unix, display_body, actor_display_name_or_nil,
 resource_id, filename, stored_bytes]
```

`display_body` is a compatibility hint such as `uploaded file.png (12.4 KiB)`.
Clients should persist and render the structured `resource_id`, `filename`, and
`stored_bytes` fields when present. Current desktop clients expose compact
load/download/open controls and render cached supported images and animated GIFs
inline.

## Slash Commands

Slash commands are client UI syntax and must parse into typed operations. They are sent as `ChatOp::Command` when server-side action is required.

Private messaging commands (`/dm`, `/query`) use the server only for contact consent/handoff. Actual private chat uses the existing OMENbrowser LXMF conversation flow.

## Traffic Minimization

OMENchat must avoid noisy defaults:

- no federation;
- no global presence;
- no global userlist;
- no typing indicators by default;
- no read receipts by default;
- MOTD is optional quiet context, not chat noise;
- userlist only on join or explicit refresh;
- deltas after snapshot;
- latest 50 events on join;
- older 50 events only on scrollback;
- no automatic attachment downloads.

## Implementation Boundary

Client implementation lives under:

```text
src/chat/
```

Server implementation lives under:

```text
src/server/
```

`src/lib.rs` may expose `pub mod chat` behind the client feature. It must not expose `pub mod server`.
