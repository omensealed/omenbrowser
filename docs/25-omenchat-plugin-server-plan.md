# OMENchat Built-In Plugin and Standalone Server Plan

## Goal

Build OMENchat as two deliberately separate Rust deliverables:

1. `omenchat_lxmf`, a default-enabled first-party OMENbrowser_rs client plugin.
2. `omenchatd`, a standalone Rust server hosted under `src/server/` during development.

`omenchatd` must be independently movable. A server operator should be able to copy only `src/server/`, build it, and install/run `omenchatd` without copying the rest of OMENbrowser_rs.

The two sides communicate through `docs/26-omenchat-protocol-v0.1.md` and neutral protocol fixtures. They must not share Rust modules or path dependencies.

## Product Direction

OMENchat should feel closer to a compact Discord-style room client than an IRC clone:

- room list;
- room timeline;
- room-local userlist;
- scrollback history;
- uploads and rich media previews;
- per-identity client cache;
- Reticulum Link room transport;
- Reticulum Resource transfer for larger compressed batches;
- LXMF handoff for private contact, not server-relayed private history.

The code under `official-sources/rrcd/` and `official-sources/rrc-gui/` is reference material only. It is useful for envelope validation, session boundaries, rate limits, and GUI persistence ideas. OMENchat must not become a clone of RRC.

## Client Plugin

The client lives inside the browser crate under:

```text
src/chat/
```

The root crate may expose it behind the `chat-client` feature:

```rust
#[cfg(feature = "chat-client")]
pub mod chat;
```

The built-in plugin id remains:

```text
omenchat_lxmf
```

The plugin should be default-enabled once the client scaffold compiles. It must not require Python plugin execution.

Initial client responsibilities:

- parse `omenchat://<destination_hash>` links while accepting legacy `omenchat:<destination_hash>` links;
- parse declarative `[omenchat]` blocks;
- lower declarative `[omenchat]` blocks into ordinary Micron links before page rendering;
- maintain saved servers and room cache in identity-scoped plugin storage;
- encode/decode OMENchat protocol frames;
- provide a mock transport for UI/store testing;
- open an OMENchat desktop pane from a descriptor;
- render cached state immediately and synchronize in the background.

Live Reticulum transport comes after the mock transport and UI shell are stable.

## Standalone Server

The server lives under:

```text
src/server/
```

It must have its own `Cargo.toml`, source tree, README, config, and tests. The root OMENbrowser_rs `Cargo.toml` must not list it as a member, dependency, feature, or binary.

The server crate must not import anything from OMENbrowser_rs. It implements the same protocol contract from documentation.

Minimum standalone checks:

```text
cd src/server
cargo check
cargo run --bin omenchatd -- init
cargo run --bin omenchatd -- init --home /tmp/omenchatd-demo --tcp-server 127.0.0.1:42420
cargo run --bin omenchatd -- run --home /tmp/omenchatd-demo --tcp-server 127.0.0.1:42420
```

## Protocol

The canonical protocol reference is:

```text
docs/26-omenchat-protocol-v0.1.md
```

Both client and server independently implement:

- compact MessagePack frames;
- numeric opcodes;
- numeric room event codes;
- numeric error codes;
- bzip2-compressed history/userlist batches;
- Resource offers for large batches;
- local rendering of routine event/error text.

No shared protocol crate is allowed for v0.1 because the server must stay independently copyable.

## Storage

Client plugin storage is identity-scoped:

```text
identity_storage/<identity>/plugins/omenchat_lxmf/
  servers.sqlite
  media/
  thumbnails/
```

The first client store implementation should be SQLite via a `ChatStore` trait so tests can run without UI or live networking.

Server storage is owned by `omenchatd`:

```text
~/.omenchatd/
  config.toml
  identity
  omenchat.sqlite
  reticulum/
```

The server must create its own identity, database, Reticulum config, and Reticulum storage on `omenchatd init` without touching OMENbrowser identity storage or user-global Reticulum/NomadNet/LXMF folders such as `~/.reticulum`, `~/.nomadnetwork`, or `~/.lxmd`. Operators may explicitly configure custom paths later, but the default must be all-in-one under the server root.

The `omenchatd tui` admin console is the server setup and operations tool:
identity/storage inspection, Reticulum interfaces, destination announce, rooms,
limits, upload policy, moderation, logs, audit, monitoring, and the NomadNet
portal. It must preserve the same identity safety rule as the browser: no
silent overwrites. The scriptable line console remains available as the non-TTY
fallback for headless setup and automation.

## Implementation Order

1. Protocol/spec alignment and skeletons.
2. Client descriptor parser, protocol model, codec, store trait, mock transport.
3. Desktop pane shell for `DesktopPane::OmenChat(ChatSessionId)`.
4. Standalone `omenchatd` crate skeleton and config/database init path.
5. Browser activation path for `omenchat://` links and `[omenchat]` blocks.
6. Server protocol/store/session engine.
7. Reticulum Link and Resource transport on both sides.
8. LXMF contact handoff.
9. Uploads, media cache, moderation, TUI admin console, and monitoring counters.
10. Alpha hardening: reconnect behavior, history sync, media privacy, scroll
    ergonomics, and tester/operator documentation.

## Current Status

Completed:

- `omenchat://` links open or reuse OMENchat desktop panes.
- Declarative `[omenchat]` blocks are parsed and lowered into clickable Micron links during browser page normalization.
- Client sessions persist in identity-scoped plugin storage and restore on launch.
- Mock transport covers open, send, history, and userlist flows.
- `src/server/` builds and tests as a standalone crate.
- `omenchatd` has standalone SQLite room/user/member/event storage.
- `omenchatd` has a protocol session engine for session open, room join, room message, history-before, and ping frames.
- Client and server protocol modules can encode/decode bzip2 compressed batch bodies and Resource offer descriptors.
- `omenchatd` chooses inline compressed snapshots or Resource offers according to compressed byte size.
- `omenchatd` stores pending Resource payloads for oversized history/userlist batches and exposes them through a transport boundary.
- Client `chat-client-rns` has a Link transport boundary that sends encoded frames and decodes inline or Resource-backed batches.
- Client `chat-client-rns` has a native `rns-net` event adapter for OMENchat link-data packets and Resource-received callbacks.
- Client `chat-client-rns` has an async native sender wrapper for `send_on_link` frames and Resource payloads with OMENchat metadata.
- Client `chat-client-rns` has a transport-neutral live driver that opens sessions, joins rooms, sends room messages, requests older history, decodes inline/Resource batches, and mutates the same `ChatClient` model used by the pane UI.
- The native runtime exposes OMENchat Link open/send/close methods and emits OMENchat LinkData/Resource runtime events instead of letting those packets fall through the LXMF decoder.
- The desktop OMENchat pane can open a native Link from an `omenchat://<destination_hash>` descriptor, retain one live transport per session, send queued OMENchat frames through the runtime, and drain incoming LinkData/Resource callbacks on the UI tick.
- Client live transports defer Resource offer frames when the Resource payload has not arrived yet, then replay those offers through the normal decoder when the delayed Resource callback lands.
- OMENbrowser_rs has a headless `--omenchat-smoke <destination_hash>` command that starts the configured runtime, optionally requests/waits for a path, opens an OMENchat Link, sends the live session-open/join frames, sends one room message, and writes a JSON stage report.
- `omenchatd` transport responses attach the same OMENchat Resource metadata expected by the client.
- `omenchatd` has a live link-session router that tracks active links, ignores non-OMENchat contexts, routes Link data into the session engine, and counts frames/resources.
- `omenchatd` has an optional `live-rns-net` adapter that sends response frames with `send_on_link` and Resource payloads with `send_resource`.
- `omenchatd` has optional `rns-net` callbacks that map inbound Link establishment, remote identification, Link data, and Link close events into the live router event model.
- `omenchatd init` creates a real 64-byte RNS identity when built with `live-rns-net`; the non-live build still remains testable without native RNS dependencies.
- `omenchatd run` is wired in `live-rns-net` builds: it initializes files, loads the standalone identity, starts `rns-net`, registers the inbound `omenchat.node` Link destination, and drains callback events into `OmenchatLiveServer<RnsNetOmenchatTransport>`.
- `omenchatd run` announces its OMENchat destination at startup and repeats the announce while running so late-starting clients can learn the destination identity key.
- OMENchat server discovery is based on the first-class `omenchat.node` announce. This is separate from `nomadnetwork.node`; live `omenchatd` also registers a separate NomadNet portal destination for the same identity so clients can browse a quiet Micron launch page at `:/page/index.mu` that links to `omenchat://<destination_hash>`. The generated page is mirrored to the server-owned Reticulum-style `reticulum/storage/pages/index.mu` path.
- `omenchatd run` prints and logs rns-net interface stats at startup, including interface name/type, connected status, RX/TX counters, packet counts, and IFAC size without exposing passphrases. It also logs interface-state changes while running so operators can verify the standalone server is using its own Reticulum config and has attached to the expected gateway.
- OMENbrowser_rs Reticulum interface config now emits `interface_enabled` alongside the existing `enabled` key for compatibility with RNS-style interface configs.
- `omenchatd init/run` accept `--home <path>` and `--tcp-server <listen_ip:port>` so smoke tests and operators can create an isolated server root with an owned Reticulum TCP server config. The generated config uses `share_instance = No`, points `network_identity` at the server-owned identity file, and writes only under the selected server home.
- OMENchat Link open now recalls a destination identity key from `rns-net` known destinations when the in-memory runtime key cache is empty. The local TCP smoke path now reaches complete: path request, Link open, session open, room join, message send, and message echo.
- The desktop OMENchat pane now renders session status in the timeline header and stores live-open failures in a visible pane session. Missing path/key, Link handshake timeout, server response timeout, runtime-not-running, live request errors, and delayed Resource waits are surfaced in the pane instead of only the status strip/logs.
- `omenchatd` live monitoring counters now track active Links, total Link opens/closes, inbound/outbound frames, Resource offers, ignored non-OMENchat contexts, packets for unknown Links, protocol/session errors, and last error text. The live server prints a compact stats line every 30 seconds only when counters changed.
- `omenchatd run/status/config` now read the server-owned `config.toml` rather than only deriving default paths, and admin CLI commands can set the server name/operator/aspect, add/list rooms, and write the isolated server-owned Reticulum TCP server interface config.
- `omenchatd tui` now opens a Ratatui/Crossterm admin dashboard when attached to a terminal. It has tabbed Overview/Setup/Rooms/Moderation/Identity/Interfaces/Portal/Help panels, keyboard navigation, explicit mouse hitboxes for tab/row selection, pop-up editors for server name/operator/MOTD/TCP interface config, a setup panel with first-run checklist and next-step guidance, a portal panel with copyable public addresses and the server-owned NomadNet page preview, room creation, visible role permission summaries, known-user trust/ban/mute controls, explicit Standard/Trusted/Moderator/Admin role actions, persisted standard/trusted/mod/admin role levels, and a non-TTY fallback to the scriptable line console.
- The server session engine now rejects banned users with a permission error, so the Moderation panel has immediate enforcement for reconnects/new actions rather than being display-only.
- OMENbrowser_rs now identifies its active Reticulum identity on live OMENchat Links before sending the OMENchat session open frame, so reconnects from the same browser identity should map to the same server-side moderation user instead of one row per transient Link.
- `omenchatd` also merges late Link identification with any display-name/LXMF metadata already received on that Link, preserving a user's visible label when `on_remote_identified` arrives after the first session frame.
- OMENbrowser_rs now receives live OMENchat Link-close events from the native runtime. Closed Links mark the chat pane disconnected with an on-screen status, and the Reconnect button force-closes any existing Link and opens a fresh one instead of refusing to act while a stale Link is still cached.
- The desktop OMENchat client now caches the server room list from `SessionAccept`, renders room buttons, and sends `JoinRoom` when a room is selected. Active-room persistence keeps reloads on the last selected room.
- Room buttons track unread counts for inactive rooms. Joining/selecting the
  room clears its unread count, and hidden/minimized OMENchat panes highlight
  when new room activity arrives.
- Client slash commands include `/create-room`/`/create`/`/mkroom`, `/topic`,
  `/notice`, `/kick`, `/ban`, `/mute`, and `/role`; the server enforces the
  admin/moderator permission boundaries for those actions.
- The desktop OMENchat client now sends idle `Ping` frames and expects `Pong` or any other inbound frame to prove the Link is still alive. If the heartbeat times out, the pane is marked disconnected and future sends require Reconnect instead of silently falling back to mock handling. OMENchat panes also snap to the newest event when opened or when new messages arrive.
- The OMENbrowser_rs Help section now includes OMENchat history sync behavior, `Load Older` expectations, and troubleshooting pointers for HistoryRecent/HistoryBefore frames so testers can diagnose cache drift without leaving the app.
- OMENchat uploads now use `/upload <path>` or the desktop attach button's
  native file picker, enforce the advertised server max-file policy before
  transfer when possible, cache accepted/fetched media under the active
  identity, and render supported images and animated GIFs inline. Clearweb image
  previews follow the shared media privacy policy and use SOCKS/Tor only, with
  no direct-TCP fallback.

Next:

- Improve multi-room UX with room-specific scrollback state and richer visible
  room activity summaries.
- Expand the `omenchatd tui` dashboard with richer room/user action review
  workflows. Live server counters have a dedicated Monitoring panel, and local
  admin actions have a dedicated Audit panel. Deeper room-specific permission
  policy editing remains future work.

## Logging and Monitoring

Normal room traffic must not spam runtime logs. OMENchat should feed Monitoring with counters:

- frames in/out;
- resource bytes in/out;
- history batches;
- userlist snapshots/deltas;
- path requests;
- upload offers/accepts/rejects;
- rate limits and decode failures.

Logs should capture actionable failures, not routine successful traffic.
