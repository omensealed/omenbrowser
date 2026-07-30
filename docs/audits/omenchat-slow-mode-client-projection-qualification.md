# OMENchat slow-mode client projection qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `737d6ac`; dormant activation-gate follow-up
began at `0970abc`

Status: bounded shared policy, frontend projection, deterministic lifecycle,
and qualification-only current/current real-Link gate complete; production
capability negotiation and enforcement remain inactive

## Scope

This unit introduces `RoomPolicyProjection` at the existing standalone
`omenchat-protocol` boundary. It carries only validated known policy bits and a
bounded `slow_mode_seconds` scalar. It contains no GUI, TUI, SQLite, transport,
runtime, filesystem, identity, or server-policy ownership.

The desktop `ChatClient` now retains that typed value instead of discarding the
slow-mode scalar and storing only a raw `u64`. The map remains keyed by
`(session_id, room_id)`, admits only rooms present in that bounded session
catalog, and is limited by the existing 256-room-per-session ceiling. Removing
a session or losing negotiated policy evidence clears its entries.

The former `room_policy_bits` accessor remains as a compatibility projection.
Callers can additionally obtain the typed value or bounded slow-mode seconds.
There is no persistent client migration.

## Frontend evidence

The Iced OMENchat pane consumes the `ChatClient` projection and can show:

```text
Slow mode · 30s
```

It is static policy evidence. It has no countdown, timer subscription, retry,
automatic resend, or client-side admission authority. Draft and composer
behavior remain unchanged in this dormant slice.

The repository has two Ratatui surfaces with different ownership:

- the legacy root TUI has no OMENchat session/timeline surface; and
- the standalone omenchatd admin TUI owns server room administration.

The omenchatd TUI now retains the same neutral shared policy DTO in its existing
bounded room cache and renders publication policy plus
`Slow mode: Ns configured · enforcement inactive`. Invalid policy values fail
closed before projection. No live editor or runtime policy fanout was added.

## Dormant activation boundary

`parse_room_policy_for_shape` can deterministically validate and project an
explicit six-field `RoomCatalogShape::SlowMode` fixture. Production runtime
selection still supplies only `Legacy` or `PolicyBits`, based on the already
active announcement-room negotiation. A six-field room value is rejected by
both production selections.

Canonical product builds do not request or accept `room-slow-mode-v1`; normal
product behavior keeps enforcement disabled. Existing four-/five-field traffic
and announcement-room behavior remain unchanged. The explicit non-product
`omenchat-slow-mode-qualification` feature activates both sides only for the
isolated process gate documented in
`omenchat-slow-mode-real-link-qualification.md`.

The follow-up activation gate adds link-scoped request and acceptance state
behind test-only entry points. Exact acceptance requires the durable-mutation
capability, selects `RoomCatalogShape::SlowMode`, and is cleared on capability
loss, identity replacement, link close, administrative retirement, or client
reconnect. The shared negotiation codec independently rejects a slow-mode
capability list that omits durable mutations.

The test-only server constructor now enables both the already test-only
monotonic admission owner and slow-mode capability acceptance. Normal
constructors leave both disabled. The normal desktop capability vector has an
explicit regression assertion that it does not advertise slow mode.

## Resource impact

The projection is two fixed-size scalars per retained room plus the existing
bounded map key/node overhead. At most 256 entries are retained in one session
and at most 64 client sessions exist. It retains no room strings, message
content, attachment bytes, deadlines, or retry state.

The omenchatd TUI replaces its former room tuple with a fixed-size typed policy
field inside the existing 1,024-item/1-MiB cache. Its five-second administrative
refresh already existed; this unit adds no poll, worker, queue, task, cache,
timer, or network request.

## Focused validation

Passed locally:

```text
cargo test --locked -p omenchat-protocol room_policy -- --nocapture
5 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  slow_mode_projection -- --nocapture
1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  slow_mode_indicator -- --nocapture
1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  room_policy_projection_is_catalog_bounded -- --nocapture
1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product \
  announcement_policy -- --nocapture
3 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full dashboard_room -- --nocapture
3 passed; 0 failed
```

Both canonical roots also passed their compile checks:

```text
cargo check --locked --no-default-features --features desktop-product
cargo check --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
```

Full local product matrices passed:

```text
cargo test --locked --no-default-features --features desktop-product

root library: 1,503 passed; 0 failed; 31 ignored
all integration and binary targets: passed; 4 additional explicit ignores

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full

534 passed; 0 failed; 11 ignored
```

The ignored cases remain explicit measurement, soak, hardware/multiprocess,
and upstream Resource gates; none is claimed as passed here.

Strict Clippy also passed for both affected product profiles:

```text
cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
```

The local quick release gate passed, including product-feature identity,
version/dependency checks, TUI lifecycle/PTY smoke, focused OMENchat coverage,
and standalone omenchatd relocation:

```text
bash scripts/release-check.sh quick

release check complete
```

The gate reported only the two already accepted root Wayland build-time
advisories (`RUSTSEC-2026-0194` and `RUSTSEC-2026-0195`); standalone omenchatd
reported no affected `quick-xml` package. This unit changes no dependency.

No hosted CI, Python interoperability, Reticulum peer, package build, physical
interface, or live GUI observation is claimed by these deterministic dormant
tests.

The dormant activation-gate follow-up additionally passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  test_only_slow_mode_requires_durable_acceptance_and_clears_with_link_state --lib
1 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  test_enabled_slow_mode_requires_durable_mutations_and_encodes_exact_shape --lib
1 passed; 0 failed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full \
  test_enabled_slow_mode_is_link_scoped_and_shapes_session_and_join_catalogs --lib
1 passed; 0 failed

cargo test --locked --no-default-features --features desktop-product
root library: 1,504 passed; 0 failed; 31 ignored
all integration and binary targets: passed; 4 additional explicit ignores

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
536 passed; 0 failed; 11 ignored

cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
passed

cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
passed

bash scripts/release-check.sh quick
release check complete
```

## Compatibility and rollback

There is no protocol version, wire activation, database, configuration,
identity, destination, history, or storage change. Reverting this unit restores
the prior dormant six-field parser without changing any production peer's
capability vector. The client adds two session-id sets bounded by the existing
64-session ceiling; the server adds one identity binding per admitted live link,
bounded by its existing 256-link ceiling. Neither retains payload data or adds a
worker, timer, queue, retry, filesystem write, or network request. Schema-12
storage, stopped-server administration, and dormant test-only admission remain
independently reversible through their documented boundary.

## Remaining activation gates

Before production activation:

- [x] implement deterministic request/accept/loss and replacement-Link state
  for `room-slow-mode-v1` without production advertisement;
- [x] prove current/current real-Link catalog/rejection/expiry/restart;
- [x] prove real-Link room-delta projection after an administrative policy
  change (see `omenchat-slow-mode-live-delta-qualification.md`);
- [x] prove adjacent four-/five-field mixed-version behavior (see
  `omenchat-room-shape-adjacent-qualification.md`);
- [x] add typed rejection and draft-retention evidence;
- run GUI observation and server/client resource measurements; and
- jointly review the final capability activation and rollback.
