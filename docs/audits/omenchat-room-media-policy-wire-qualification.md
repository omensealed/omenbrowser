# OMENchat room media-policy wire qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `992eec4`

Target: `v0.9.6-4`

Status: dormant wire slice complete

## What changed

The transport-neutral `omenchat-protocol` crate now owns:

- capability name `room-media-policy-v1`;
- a 10-MiB maximum room upload scalar;
- `RoomCatalogShape::MediaPolicy`;
- an optional room upload maximum where `nil` inherits, zero disables, and a
  positive value imposes a per-file ceiling;
- stable typed upload-rejection codes `UploadsDisabled = 1` and
  `FileSizeExceeded = 2`; and
- exact room-delta and upload-rejection fixtures.

The cumulative negotiated room value is exactly:

```text
[room_id, name, topic_or_nil, room_revision, policy_bits,
 slow_mode_seconds, room_upload_max_file_bytes_or_nil]
```

The new capability is valid only with both `announcement-rooms-v1` and
`room-slow-mode-v1`; slow mode retains its existing durable-mutation
dependency. Invalid dependency sets, field counts, types, bounds, and rejection
codes fail closed.

## Dormancy proof

No production client request set, server acceptance set, or Link-scoped state
references `ROOM_MEDIA_POLICY_CAPABILITY`. The existing client state selects
only `Legacy`, `PolicyBits`, or `SlowMode`, and the server live shaper does the
same. The new shape appears only in the shared bounded codec and focused codec
tests.

No Cargo feature, operation number, protocol version, schema, configuration,
storage row, upload admission path, queue, worker, timer, cache, retry, or UI
behavior changed.

All existing room producers now initialize the dormant scalar to `None`.
Because production never selects the seven-field shape, legacy four-field,
announcement five-field, and slow-mode six-field bytes remain unchanged.

## Compatibility and rollback

- Legacy and unnegotiated peers retain their exact room and `UploadReject`
  shapes.
- A slow-mode peer cannot receive a seven-field room value.
- Unknown or unsolicited seven-field values are not selected by production
  parsers.
- Reverting this commit removes only dormant shared vocabulary and tests.
- No persistent state or user data requires rollback.

## Commands and results

Passed:

```text
cargo test --manifest-path src/server/crates/omenchat-protocol/Cargo.toml
  57 passed

cargo check --locked --no-default-features --features desktop-product

(cd src/server &&
 cargo check --locked --no-default-features --features server-headless)

cargo test --locked --no-default-features --features desktop-product \
  --lib media_policy_room_and_rejection_values_are_byte_exact_and_dormant
  1 passed

(cd src/server &&
 cargo test --locked --no-default-features --features server-headless \
   --lib media_policy_room_and_rejection_values_are_byte_exact_and_dormant)
  1 passed

cargo clippy --manifest-path \
  src/server/crates/omenchat-protocol/Cargo.toml \
  --all-targets -- -D warnings

cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings

(cd src/server &&
 cargo clippy --locked --no-default-features \
   --features server-headless --all-targets -- -D warnings)

(cd src/server &&
 cargo check --locked --no-default-features --features server-full)

cargo fmt --all --check
cargo fmt --manifest-path src/server/Cargo.toml --all --check
cargo fmt --manifest-path \
  src/server/crates/omenchat-protocol/Cargo.toml --all --check
git diff --check
```

The initial unqualified root test invocation began compiling every integration
target despite the name filter. It was terminated and replaced with the
explicit `--lib` command above; no test failure was hidden.

A source/product-graph search found no media-policy Cargo feature and no live
capability request/acceptance reference. Full root/server test matrices,
release quick, native platforms, Python interoperability, and packaging remain
batched release-candidate gates; repeating them for this dormant vocabulary
slice would not add proportional evidence.

## Next gate

The next independent slice is schema 13:

- nullable constrained room upload scalar;
- migration and injected rollback boundaries;
- representative schema-12 preservation fixtures; and
- confirmation-gated, stopped-server schema-12 copy export.

Negotiation and upload enforcement remain inactive through that storage slice.
