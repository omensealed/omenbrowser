# OMENchat slow-mode wire qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `764fc0a`

Status: dormant wire contract passes; no schema, negotiation, enforcement,
configuration, or UI behavior is active

## What changed

The shared protocol boundary now defines:

- capability `room-slow-mode-v1`;
- typed error number `SlowModeActive = 1017`;
- maximum interval `86_400` seconds;
- explicit `RoomCatalogShape::{Legacy, PolicyBits, SlowMode}`; and
- a bounded six-field room catalog codec carrying `slow_mode_seconds`.

The existing boolean codec entry points remain as compatibility wrappers and
still select only the exact four-field legacy or five-field announcement shape.
Production call sites therefore retain their prior bytes. A later activation
slice must move per-Link shaping to the enum before it can emit six fields.

`room-slow-mode-v1` requires `durable-mutations-v1` in both requests and
acceptance. The protocol crate rejects the dependent capability without its
base rather than allowing a client to infer retry safety.

## Compatibility and resource impact

- Protocol version remains 1.
- Existing operation numbers and error numbers are unchanged.
- The v0.9.6-3 ordinary frame fixture remains unchanged.
- Legacy room values remain four fields.
- Announcement-capable room values remain five fields.
- Six fields are decodable only through explicit `SlowMode` shape selection.
- No production capability vector requests or accepts slow mode.
- Schema remains 11 and no database is opened or migrated by this unit.
- No dependency, feature, worker, timer, queue, cache, task, retry, or retained
  runtime state was added.

The new `slow_mode_seconds` scalar is validated before projection. Values above
`86_400`, unknown policy bits, wrong shapes, and wrong scalar types fail closed.
The desktop recognizes error 1017 as static typed text but does not yet treat it
as authoritative retry evidence or schedule a countdown.

## Tests

Passed:

```text
cargo test --locked -p omenchat-protocol -- --nocapture
  53 passed

cargo test --locked --no-default-features --features desktop-product \
  slow_mode_room_value_is_byte_exact_and_shape_scoped --lib -- --nocapture
  1 passed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  slow_mode_room_value_is_byte_exact_and_shape_scoped --lib -- --nocapture
  1 passed

cargo test --locked --no-default-features --features desktop-product \
  announcement_room_values_are_byte_exact_and_negotiation_scoped \
  --lib -- --nocapture
  1 passed

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  announcement_room_values_are_byte_exact_and_negotiation_scoped \
  --lib -- --nocapture
  1 passed

cargo test --locked --no-default-features --features desktop-product \
  parse_error_text_includes_known_error_code_label --lib -- --nocapture
  1 passed

cargo clippy --locked --no-default-features --features desktop-product \
  --lib -- -D warnings
  passed

cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless --lib -- -D warnings
  passed
```

The desktop and standalone server fixtures use independent MessagePack codecs
and require the same canonical bytes:

```text
96 01 21 00 0c c0 91 96 07 ad "announcements"
b0 "Operator updates" 03 01 1e
```

## Not run

No live, Python, native-platform, packaging, storage, or process tests were run.
This slice cannot negotiate or enforce slow mode and does not touch Reticulum,
LXMF, SQLite, identities, or packaging. Those expensive gates would not add
evidence for the dormant codec boundary.

## Rollback and next gate

Rollback is a code/doc revert; no data restore is required.

The next coherent slice is schema-12 storage and recovery: disabled-by-default
room interval, bounded admission ledger, injected migration faults, and a
confirmation-gated schema-11 copy export. It must not activate negotiation or
runtime enforcement.
