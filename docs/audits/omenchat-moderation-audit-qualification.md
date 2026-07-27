# OMENchat dormant moderation-audit qualification

Date: 2026-07-27
Branch: `release/v0.9.6-4`
Baseline commit: `35f1c82`
Protocol: `omenchat-v0.1`, numeric version 1

## Scope and verdict

This first reversible slice reserves operations 52–55 and defines a bounded
read-only `moderation-audit-v1` wire contract in the shared protocol crate.
It does not add schema 10, write or read the legacy `audit_log`, couple a
moderation mutation, accept capability negotiation, send a page, retain
client state, or expose UI controls.

Verdict: dormant shared contract and independent codec agreement pass.
Storage, transactional mutation coupling, paging transport, authorization,
presentation, live qualification, and activation remain pending.

## Contract evidence

The shared crate now defines:

- an exact tagged newest/exclusive-cursor request with a 1–256 page limit;
- operations `ModerationAuditBefore`, `ModerationAuditInline`,
  `ModerationAuditResource`, and `ModerationAuditEnd` at 52–55;
- a fixed action vocabulary for kick, ban, unban, mute, unmute, and role
  change;
- a fixed ten-field record that cannot carry identity hashes, Reticulum
  endpoints, Link IDs, mutation IDs, request hashes, tickets, tokens,
  arbitrary payload, or operator-log text;
- positive room/user/audit IDs, nonnegative timestamps, exact action/result
  combinations, known role/status bits, 256-byte display names, 256 records,
  512 KiB retained bytes, newest-first unique IDs, and explicit room matching;
- one byte-exact request fixture decoded and encoded independently by the
  desktop and standalone server codecs.

The server negotiation regression requests `moderation-audit-v1` both beside
the durable base and alone. The server accepts neither occurrence. This
feature is read-only and intentionally does not acquire a durable-mutation
dependency merely because the test also covers the existing extended
handshake.

## Validation

```bash
cargo test --locked -p omenchat-protocol moderation_audit -- --nocapture
cargo test --locked -p omenchat-protocol
cargo test --locked --no-default-features --features desktop-product \
  moderation_audit --lib
cargo check --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo test --locked --no-default-features --features desktop-product --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless moderation_audit --lib)
(cd src/server && cargo check --locked --no-default-features \
  --features server-headless)
(cd src/server && cargo check --locked --no-default-features \
  --features server-full)
(cd src/server && cargo clippy --locked --no-default-features \
  --features server-headless --all-targets -- -D warnings)
(cd src/server && cargo clippy --locked --no-default-features \
  --features server-full --all-targets -- -D warnings)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless --lib)
cargo fmt --all --check
(cd src/server && cargo fmt --all --check)
```

All applicable commands passed. The shared protocol suite passed 47 tests.
The desktop product library suite passed 1,493 tests with 30 explicit
live/platform/soak ignores. The independently rooted omenchatd suite passed
359 tests with 10 explicit live/soak ignores. Both desktop-product and
server-headless/server-full checks passed; strict Clippy passed for the
desktop product and both server profiles.

The first validation attempt accidentally supplied `--features
server-headless` to the root `omenbrowser_rs` manifest. Cargo rejected the
unknown feature before compilation or mutation. The server command was then
run from its independent manifest root as documented above.

Live Reticulum peers, Python interoperability, native Windows/macOS runners,
and packaging were not run because this dormant contract has no active
network caller, dependency change, platform branch, or artifact change.

## Compatibility and resource impact

Protocol version 1, existing operation assignments, ordinary v0.6.0-1 and
v0.9.6-3 fixtures, capability requests, and capability acceptance remain
unchanged. Older peers never receive these operations because there is no
active caller or accepted capability. The contract adds no runtime
dependency, database object, queue, cache, worker, task, timer, retry, or
network traffic.

Rollback is code-only for this dormant slice.

## Next gate

The next separately reviewed unit is schema-10 constrained storage and
transactional coupling to only those existing moderation paths that can
commit the user mutation, audit row, and durable replay result atomically.
It must include schema-9 and schema-8 guarded downgrade copies and must leave
the legacy operator `audit_log` and text runtime log unexposed.
