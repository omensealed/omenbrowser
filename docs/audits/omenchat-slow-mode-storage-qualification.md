# OMENchat slow-mode storage qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `820aaa1`

Status: schema-12 storage and recovery complete; slow-mode negotiation,
runtime enforcement, configuration command, and UI projection remain inactive

## What changed

omenchatd now migrates its independent database to schema 12:

```text
rooms.slow_mode_seconds
room_slow_mode_admissions
idx_room_slow_mode_admissions_expiry
```

Existing and new rooms default to `0`, meaning disabled. The scalar is
constrained to `0..=86_400`. Migration creates no admission rows and scans no
history, memberships, messages, uploads, replay results, identities, or users.

The persistent admission owner is separate from `room_members`, so Part/rejoin
cannot erase a future cooldown. It stores only fixed-width room/user IDs and
timestamps. Its production limits are:

- 4,096 rows per room;
- 16,384 rows globally;
- 32 logical bytes per row and 512 KiB globally; and
- at most 64 expired deletions per admission attempt.

An active deadline is never evicted to admit another user. After bounded
expired pruning, saturation returns a typed storage result without inserting a
row. Disabled rooms retain no admission state. A transaction rollback consumes
neither the prior deadline nor a new one.

The store exposes an atomic, bounded scalar/revision update for the later
stopped-server administration slice. It is not wired to a CLI or live
mutation path in this unit.

## Migration and rollback

The column, table, index, schema version, and commit share the existing
immediate migration transaction. Injected failure at each boundary leaves the
source at schema 11 with `policy_bits` intact and no partial slow-mode object.
A non-empty source retains the existing owner-only sibling backup naming:

```text
omenchat.sqlite.pre-v12-from-v11.bak
```

The new confirmation-gated command:

```bash
omenchatd database export-schema11-copy \
  --to <new-database-path> --confirm --home <server-home>
```

requires exclusive stopped-server access and a destination that does not
exist. It copies through a sibling stage, removes only the scalar, admission
table, and expiry index, validates integrity/foreign keys/schema 11, publishes
atomically, and never edits the source. Announcement policy, room revision,
moderation audit, pins, retention metadata, revisions, reactions, durable
replay, history, uploads, users, and identities remain.

## Compatibility and resource impact

- OMENchat protocol version remains 1.
- No operation, capability vector, room wire value, or error frame changes.
- Existing four-/five-field clients retain identical traffic.
- No client requests and no server accepts `room-slow-mode-v1`.
- No Reticulum/LXMF dependency, feature, identity, destination, configuration,
  worker, task, timer, queue, retry loop, or cache was added.
- SQLite growth is bounded logically by fixed row count. Physical SQLite
  page/file reclamation is not misreported as exact logical bytes.

An old omenchatd must not open the schema-12 source. Roll back by stopping the
server, exporting and validating a schema-11 copy, preserving the schema-12
source, and explicitly selecting the copy. No automatic downgrade occurs.

## Validation

Focused passes:

```text
slow_mode: 9 passed
schema_eleven: 3 passed
every_room_policy_schema_fault_boundary: 1 passed
cli_schema_eleven_export_preserves_active_source_and_room_policy: 1 passed
```

The first full headless run exposed nine `InvalidColumnIndex(5)` failures in
durable transactional room lookups that still selected the prior five columns.
Those queries were corrected to select `slow_mode_seconds`; the second full
run passed:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless

399 passed; 11 ignored explicit soak/hardware tests
```

The ignored tests remain the documented 60-second link/database/queue/logging
soaks, multiprocess Resource tests, upstream maximum-UDP-Resource regression,
and explicit cancellation gate. None was reported as passed.

Strict static checks passed for both standalone server products:

```text
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
```

`bash scripts/release-check.sh quick` passed. It covered root and server
formatting, dependency/version/product-feature assertions, native CLI identity
isolation, TUI lifecycle, focused OMENchat client/server tests, IFAC vectors,
both standalone server profiles, and a relocated omenchatd build/test check.
The real-PTY lifecycle step initially timed out inside the restricted command
sandbox because the sandbox did not deliver the test signals. The same script
was rerun outside that signal sandbox and passed, with observed TERM/INT
shutdown latencies of 63--72 ms. This is recorded as an environment limitation,
not hidden as a product failure.

The quick gate continues to report the repository's accepted root-only
`RUSTSEC-2026-0194` and `RUSTSEC-2026-0195` advisories in the transitive
Wayland scanner/quick-xml path. omenchatd has neither advisory, and this unit
changes no dependency.

## Remaining work

The next slice is test-only atomic admission in durable and legacy
message/action paths. It must order exact durable replay before cooldown work,
couple a new event/replay result/deadline in one transaction, add a bounded
monotonic in-process owner, and remain production-inactive until live,
mixed-version, restart, UI, and measurement gates pass.
