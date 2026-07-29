# OMENchat room media-policy storage qualification

Date: 2026-07-28

Branch: `release/v0.9.6-4`

Baseline commit: `7dcc7ed`

## Outcome

The second staged room media-policy slice is complete. omenchatd now uses
SQLite schema 13 and stores one dormant nullable per-room upload file ceiling.
No production client requests `room-media-policy-v1`, the server does not
accept it, and upload admission/publication behavior is unchanged.

## Storage contract

`rooms.upload_max_file_bytes` has these storage meanings:

- `NULL`: inherit the global server file ceiling;
- `0`: disable room uploads once enforcement is activated;
- `1..=10485760`: impose that per-file ceiling once activated.

SQLite rejects negative values and values above 10 MiB. Existing schema-12
rooms migrate to `NULL`. The field is projected through the store-owned room
model so later policy resolution does not need an unbounded side table, cache,
worker, or timer.

## Migration and failure boundaries

The schema field and `user_version = 13` update use the existing immediate
migration transaction. A non-empty older database retains the existing
owner-only, non-overwriting pre-migration backup. Tests inject failure at:

- media-policy column creation;
- schema version update;
- transaction commit.

Every injected failure leaves a readable schema-12 source without the new
column. Announcement policy, slow-mode settings/admissions, and upload-ledger
rows are preserved on successful migration and in the schema-12 backup.

## Rollback copy

The stopped-server, confirmation-gated command is:

```bash
omenchatd database export-schema12-copy \
  --to <new-database-path> --confirm --home <server-home>
```

The destination must not exist. A private staging copy drops only
`upload_max_file_bytes`, sets `user_version = 12`, validates schema shape,
integrity, and foreign keys, synchronizes the file, and atomically publishes
it. Slow-mode admissions, announcement policy, uploads, history, replay state,
users, rooms, and identities remain. Injected publication failure removes the
staging reservation and leaves both source and destination boundaries intact.

## Compatibility and resource impact

Wire bytes, capability vectors, OMENchat protocol version, configuration,
identity paths, upload files, and existing schema layers are unchanged.
Schema-13 binaries open and migrate schema-12 databases. Older binaries must
use a separately exported schema-12 copy; the active schema-13 file is never
rewritten for downgrade.

The steady-state cost is one nullable scalar per room and one additional
column in existing bounded room queries. This slice adds no queue, cache,
background task, retry, timer, polling subscription, database table, or index.

## Commands and results

Passed:

```text
cargo fmt --manifest-path src/server/Cargo.toml --all --check
cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib version_twelve_database_adds_nullable_constrained_room_upload_policy -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib every_media_policy_schema_fault_boundary_rolls_back_to_version_twelve -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib schema_twelve_export -- --nocapture
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --lib
cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless --all-targets -- -D warnings
cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
```

The full standalone library result was 420 passed, 0 failed, and 11 explicitly
ignored hardware/soak/interoperability tests.

One initial focused export run failed because its new preservation assertion
expected an upload-ledger row that the test fixture had not inserted. The
fixture was corrected to insert an isolated representative row; the focused
export matrix and full library suite then passed. This was a test-fixture
omission, not a product migration or export failure.

## Not executed

No hosted Windows/macOS, Python interoperability, packaging, hardware, public
Reticulum, or long-running soak gate was triggered for this storage-only unit.
It changes neither platform code nor wire/runtime behavior. Those expensive
gates remain batched for the stable release candidate.

## Remaining risk and next step

The scalar is intentionally not operator-configurable and has no effect on
upload admission yet. The next smallest unit is a store-owned effective-policy
resolver plus test-only offer and publication enforcement. It must reuse the
existing global ceiling, quota serialization, and durable file commit boundary,
and must keep production negotiation disabled.
