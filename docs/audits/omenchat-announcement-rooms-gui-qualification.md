# OMENchat announcement-room GUI qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `3763989`

Verdict: the explicit current/current qualification build presents negotiated
announcement-room policy truthfully in the native Linux Iced GUI and prevents
a standard member from submitting a room message. This closes the local GUI
observation gate; it is not Windows/macOS or physical-GPU evidence.

## Isolated setup

The root and standalone server were built independently:

```bash
cargo build --locked --no-default-features \
  --features desktop-product,omenchat-announcement-qualification \
  --bin omenbrowser_rs
cargo build --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-announcement-qualification \
  --bin omenchatd
```

An isolated omenchatd root was initialized with a loopback TCP server. The
server was started once to complete normal schema ownership, stopped, and its
lobby was changed through the confirmation-gated stopped-server command:

```text
room policy updated: id=1 policy=announcement revision=1
```

An isolated browser root received a newly generated test identity and one
no-IFAC loopback TCP client profile. No maintainer identity, interface, message,
or server state was read or modified.

The desktop ran at 1400x900 under Xvfb and i3 with software rendering. The
native quick-open field opened the server by its generated
`omenchat://<destination>` URI.

## Observed GUI behavior

After the real Reticulum Link authenticated and joined:

- the pane reported `joined | room: #lobby | 1 users`;
- the room-policy banner read
  `Announcement room · only moderators and admins can publish`;
- the attach action explained `Read-only announcement room`;
- the message input retained local draft editing but had no submit action;
- the Send button had no press action;
- clicking Send preserved the draft and appended no timeline event; and
- normal live pings continued without freezing or changing policy state.

At orderly server shutdown, its counters recorded one active/opened Link,
five inbound frames, and:

```text
requests=session:1 room:1 chat:0 history:1 ping:2
```

The zero chat-request count is independent server-side evidence that the GUI
click did not transmit a publication. The desktop then handled Alt-F4 through
the normal ordered drain and logged `desktop shutdown drained successfully`.

## Resource and compatibility impact

This observation added no code, dependency, persistent schema, protocol field,
worker, timer, retry, cache, queue, or release feature. It used the already
isolated qualification feature. The ordinary four-field room contract and
canonical release aliases remained unchanged.

The visual evidence was retained only in the disposable local evidence root;
binary screenshots were not added to the repository. The setup and assertions
above are reproducible with existing repository binaries and standard X11 test
tools.

## Limitations and next decision

- Xvfb/software rendering proves native Linux Iced layout and input routing,
  not physical GPU behavior.
- Windows and macOS native presentation remain part of the later release
  candidate matrix.
- The server had one standard member; moderator publication remains covered by
  the separate real-Link message/Resource process gate.
- The TUI has no member composer and therefore no equivalent send-control
  observation.

All local wire, storage, authorization, client projection, restart,
replacement-Link, Resource, and Linux GUI gates now pass. The next coherent
unit is the explicit production activation review: give the capability a
production feature identity, include it in canonical client/server aliases,
retain per-Link legacy shaping, and rerun the full local product matrices
before hosted/native release qualification.
