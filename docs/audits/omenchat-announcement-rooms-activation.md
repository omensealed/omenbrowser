# OMENchat announcement-room production activation

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `af5274f`

Verdict: local activation gates pass. Canonical OMENbrowser and omenchatd
products now negotiate `announcement-rooms-v1`; legacy and non-negotiating
Links retain four-field room values and unconditional server authorization.
Hosted native-platform and packaging gates remain deferred until the release
candidate is batched.

## Activation decision

The temporary qualification identity was replaced by the dependency-free
production feature:

```text
omenchat-announcement-rooms
```

It is required by:

- `desktop-product`;
- `desktop-product-static-media`;
- `server-headless`; and
- `server-full`.

The client requests the capability only when this feature is present. The
server accepts only an explicit request and records the negotiated state per
authenticated Link in the already bounded active-Link map. Room catalogs,
JoinAccept, room-list responses, and RoomDelta values use five fields only for
that Link. A simultaneous legacy Link continues receiving the exact four-field
shape.

Server authorization remains independent of negotiation. An old client may
not display the room policy, but its prohibited mutation still receives typed
server error `1016` without commit.

## Compatibility and storage

This unit does not change:

- OMENchat protocol version 1 or any operation number;
- database schema 11 or its recovery/export path;
- destination names/aspects;
- identity ownership or paths;
- room-policy defaults;
- message, history, cache, or configuration schema versions; or
- the Reticulum/LXMF dependency train.

Existing rooms remain ordinary. Existing peers that omit the capability retain
their prior wire shape. The feature adds no crate, worker, task, timer, retry,
queue, cache, database row, or unbounded retained state.

## Validation

Focused production and opt-out gates passed:

```text
omenchat-protocol room_policy: 3 passed
desktop announcement_policy: 2 passed
desktop production capability vector: 1 passed
desktop component vector with only announcement support omitted: 1 passed
omenchatd production announcement_room: 6 passed
omenchatd live-reticulum-only dormant capability: 1 passed
product feature verification: pass
```

The opt-out client used the complete product component set minus only
`omenchat-announcement-rooms`. This proves the feature remains an explicit
negotiation boundary rather than an unconditional wire change.

Full local gates passed:

```text
root library: 1499 passed, 31 ignored explicit live/measurement tests
root binary: 39 passed
root integration/doc tests: passed
root strict all-target Clippy: passed
omenchatd headless: 389 passed, 11 ignored explicit soak/hardware tests
omenchatd server-full check: passed
omenchatd strict headless/full all-target Clippy: passed
release-check.sh quick: passed
standalone omenchatd relocation: passed
```

Canonical binaries, without an extra qualification feature, also passed:

```bash
bash scripts/release-omenchat-smoke.sh \
  --browser-bin target/debug/omenbrowser_rs \
  --server-bin src/server/target/debug/omenchatd \
  --tcp 127.0.0.1:<unused-port> \
  --path-wait 20 \
  --out /tmp/omenbrowser-announcement-production-activation \
  --message 'production announcement activation qualification' \
  --announcement-negotiation-smoke \
  --restart-server
```

Both initial and replacement-Link reports required authoritative policy,
local preflight rejection, no queued publication frame, and no committed
message. The server stop was orderly and its destination remained stable.

One deliberately incomplete feature command,
`--features chat-client-reticulum`, did not compile because the existing
QR-conditioned composer row also requires the product's `desktop-qr`
component. That combination is not a documented product identity and the
failure predates this activation. It was not hidden or fixed as collateral
work; the complete component set minus only the new feature passed.

## Not run

- Hosted Linux/Windows/macOS CI.
- Pinned/current Python interoperability.
- Packaging and final-file installation smoke.
- Physical GPU or hardware-interface tests.

These expensive gates are reserved for the stable release-candidate batch.
The activation changes no Reticulum/LXMF primitive, so repeating Python
interop for this isolated feature-graph change is not justified before that
batch.

## Rollback

Revert the activation unit: remove `omenchat-announcement-rooms` from canonical
aliases and return the request/accept `cfg!` boundary to disabled. Server
authorization and schema 11 may remain safely in place, as already proven
before activation. No database or identity rollback is required.
