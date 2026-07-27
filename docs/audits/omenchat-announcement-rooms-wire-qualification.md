# OMENchat announcement-room wire qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` through `00f7236`, plus this explicit
qualification-feature unit

Verdict: dormant shared wire contract plus explicit client/server
qualification profiles pass; canonical product request/acceptance remains
disabled

## Scope

This unit adds:

- `announcement-rooms-v1` as an independent capability name;
- the fixed `0x01` announcement-room policy bit and known-bit mask;
- a project-owned bounded room catalog value;
- exact legacy four-field and negotiated five-field encoders/decoders;
- shared and independent desktop/omenchatd codec fixtures;
- explicit tests proving the production desktop does not request the capability
  and production omenchatd does not accept it.

It adds no operation number, error code, schema, room field, configuration,
server predicate, client model field, UI/TUI control, queue, task, timer,
cache, retry, or network request.

Later separately qualified units added schema-11 storage, server enforcement,
bounded client projection, and non-negotiated real-Link authorization. This
follow-up wire unit adds a test-only server-engine enable boundary. With that
boundary enabled, an explicit `announcement-rooms-v1` request is accepted and
the initial room catalog uses the shared five-field encoder with authoritative
policy. The normal constructor keeps the boundary disabled, so production
omenchatd still omits acceptance and sends legacy four-field catalogs.

The following dormant Link-scoping unit binds accepted policy projection to
the authenticated Link identity. JoinAccept, room-list results, and RoomDelta
are shaped at the final destination boundary: a negotiated test Link receives
five fields while a simultaneous legacy Link receives the unchanged four.
Identity replacement, session replacement, disconnect, retirement, and close
remove the binding. The binding map is owned by the live server and bounded by
the existing 256 active-Link ceiling. It adds no worker, queue, retry, timer,
or retained payload.

Only negotiated room-bearing frames perform the authoritative room lookup and
bounded clone. Because the production server still cannot accept the
capability, its empty binding map takes the original borrowed-frame fast path
with no added lookup or clone.

At qualification time, the root and standalone server exposed a dependency-
free, non-default qualification feature. The activation unit promoted that
same boundary to `omenchat-announcement-rooms`: the client requests the
capability and retains the pending request, and the server enables its
already-qualified acceptance boundary. Canonical aliases now include the
production feature and the machine assertion requires it.

## Wire invariants

- Legacy room values remain exactly
  `[room_id, name, topic-or-nil, room_revision]`.
- A negotiated value is exactly the legacy value plus `policy_bits`.
- A legacy decoder rejects the negotiated shape at the shared typed boundary;
  a negotiated decoder rejects a missing policy field.
- Room IDs are nonzero `u32` values.
- Names are nonempty and at most 64 bytes.
- Topics are absent or nonempty and at most 4,096 bytes.
- Unknown policy bits fail closed.
- Encoding an announcement room for an unnegotiated peer deliberately omits
  policy and decodes as ordinary. Server enforcement will remain independent
  of negotiation in a later unit.
- The capability has no durable-mutation dependency because it carries
  read-only evidence. That does not activate acceptance.

## Focused validation

Passed:

```text
cargo test --locked -p omenchat-protocol room_policy -- --nocapture
cargo test --locked -p omenchat-protocol announcement_room -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  announcement_room_values_are_byte_exact_and_negotiation_scoped --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless announcement_room --lib -- --nocapture)
cargo test --locked --no-default-features --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless announcement_rooms --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  test_enabled_announcement_rooms_shape_join_and_delta_per_authenticated_link \
  --lib -- --nocapture)
cargo test --locked --no-default-features \
  --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity \
  --lib -- --nocapture
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  announcement_room --lib -- --nocapture)
bash scripts/verify-product-features.sh
```

Focused results:

- shared room-policy codec: 3 passed;
- shared independent negotiation: 1 passed;
- desktop independent MessagePack codec: 1 passed;
- omenchatd independent codec plus dormant acceptance: 2 passed;
- production desktop capability vector remains unchanged and excludes
  `announcement-rooms-v1`.
- test-enabled server catalog negotiation plus normal server dormancy: 2
  passed;
- mixed legacy/negotiated authenticated Link shaping: 1 passed;
- qualification client vector: 1 passed;
- qualification server policy tests: 6 passed;
- canonical animated/static desktop and headless server feature assertion:
  passed with qualification excluded;
- qualification desktop/server all-target strict Clippy: passed with
  `-D warnings`;
- standalone server-headless all-target strict Clippy: passed with
  `-D warnings`.

Full local matrix:

- root `cargo fmt --all --check`: passed;
- root desktop-product tests: 1,497 passed, 31 ignored;
- root desktop-product all-target strict Clippy: passed with `-D warnings`;
- standalone server `cargo fmt --check`: passed;
- standalone server-headless tests: 375 passed, 11 ignored;
- standalone server-headless all-target strict Clippy: passed with
  `-D warnings`;
- standalone server-full check: passed;
- `git diff --check`: passed.

The ignored tests retain their existing explicit live-peer, display, packaging,
platform, or fault-injection prerequisites. No hosted CI, Python
interoperability, live Reticulum peer, GUI session, native packaging, or
hardware test was needed for this dormant wire-only unit.

## Compatibility and storage impact

Ordinary protocol-v1 traffic is unchanged. Older peers never negotiate the
capability and receive the exact existing room shape. The application and
server package versions, schema 10, databases, configurations, identities,
history, and destination names are unchanged.

## Not claimed

- current/current or adjacent-version process traffic;
- native package behavior;
- production activation.

## Rollback

Remove `room_policy.rs`, its module export, fixture constants, independent codec
tests, dormancy assertions, and this audit. No data or configuration rollback
is needed.
