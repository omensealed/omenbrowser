# OMENchat moderation-audit paging qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `227e403`, plus this measurement unit.

## Scope and activation state

This unit implements the read-only paging boundary designed in
`docs/design/OMENCHAT_PINS_MODERATION_AUDIT_CHECKPOINT.md`. It does not activate
`moderation-audit-v1` in either production product:

- the desktop does not request the capability;
- omenchatd's normal constructors do not accept it;
- test-only state is required on both sides;
- no GUI/TUI control, recurring refresh, worker, timer, or persistent client
  cache was added.

## Invariants qualified

- Capability acceptance is independent of durable mutations but is attached to
  one authenticated Link and identity.
- Identity replacement, duplicate/closed Link retirement, and a new session
  negotiation clear the prior Link binding.
- Every request rechecks current moderator/admin role and current room
  membership; role loss fails before reading a page.
- Requests have an exclusive nonzero cursor and a protocol-bounded limit.
- Pages are newest-first. A short or empty page emits
  `ModerationAuditEnd`; a full page does not claim end.
- The same canonical values are used for compressed inline and Resource
  delivery. Resource purposes must begin with `moderation-audit:`.
- Malformed or unnegotiated operations fail closed.
- Oversized requests and invalid or oversized Resource offers fail before
  pending-offer retention.
- A valid offer may arrive before its Resource; the existing bounded owner
  retains and replays it once the matching Resource arrives.
- The client accepts inline/Resource pages and end markers only after explicit
  test-only negotiation.
- The client projection is memory-only, keyed by server and room, limited to
  1,024 records and 512 KiB, and cleared on capability loss or final session
  removal.
- Client output contains the bounded protocol fields only. It cannot expose
  identity hashes, Link identifiers, Reticulum endpoints, mutation identifiers,
  request hashes, reusable secrets, or operator text-log contents because none
  are present in the page contract.
- Invalid client pages clear the ephemeral projection instead of leaving stale
  rows presented as current.
- Schema-10 rows survive a file-backed server restart, and duplicate read-only
  requests return byte-identical responses without another mutation.

## Commands and results

Passed:

```text
cargo fmt --all --check
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product moderation_audit -- --nocapture
(cd src/server && cargo check --locked --no-default-features --features server-headless)
(cd src/server && cargo test --locked --no-default-features --features server-headless moderation_audit -- --nocapture)
cargo test --locked --no-default-features --features desktop-product \
  v0_9_6_3_ordinary_message_remains_byte_exact --lib
cargo test --locked --no-default-features --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless v0_9_6_3_ordinary_message_remains_byte_exact --lib)
cargo test --locked --no-default-features --features desktop-product \
  moderation_audit_projection_measurement --lib -- --ignored --nocapture
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless moderation_audit_retention_measurement \
  --lib -- --ignored --nocapture)
```

Focused results:

- desktop moderation-audit filter: 4 passed, 0 failed;
- omenchatd moderation-audit filter: 15 passed, 0 failed;
- ordinary v0.9.6-3 desktop/server fixtures: passed byte-exact;
- production desktop capability request remains absent;
- production omenchatd capability acceptance remains absent.

Isolated resource observations on this Linux development host:

- the bounded client projection admitted its 1,024-record ceiling across four
  server identities and rejected the next record. Accounted retained bytes were
  80,896; page admission was 85 us p50, 143 us p95, and 143 us maximum;
- the file-backed server retained its 2,048-row per-room ceiling using 161,792
  accounted bytes and 401,408 database bytes after WAL checkpoint. Individual
  committed appends were 883 us p50, 1,429 us p95, and 1,533 us maximum; bounded
  page reads were 1,155 us p50, 1,307 us p95, and 1,307 us maximum.

These are reproducible observations, not release thresholds. The tests use
isolated temporary roots, enforce the configured bounds, print their results,
and remove the database/WAL/SHM files.

Full local results:

- root tests: all executed tests passed (1,496 library tests passed, 31
  explicitly ignored; all binary/integration tests also passed);
- omenchatd tests: 373 passed, 11 explicitly ignored;
- root strict Clippy: passed with `-D warnings`;
- omenchatd strict Clippy: passed with `-D warnings`;
- omenchatd `server-full` check: passed;
- formatting and diff whitespace checks: passed.

Ignored tests are existing explicit live-Python, hardware/network, 60-second
soak, or release-mode measurement gates. None was converted to a pass.

## Not yet claimed

- a current/current multi-process fetch or process restart;
- continuation of an in-flight page across a replacement Link (the old Link
  authority is intentionally discarded);
- adjacent-version binary live traffic;
- cancellation during an active Reticulum Resource;
- decompression-bomb process evidence;
- user-facing audit presentation or production activation.

Those are Phase 12 qualification work. Until that evidence exists, production
negotiation stays disabled.

## Resource and compatibility impact

No queue, task, timer, retry, database schema, server retention policy, or wire
number changed. A successfully negotiated test client can retain at most one
page per server/room within the shared 1,024-record/512-KiB client budget.
Older peers request no capability and observe unchanged session and room
traffic.

## Rollback

Remove the paging handler, Link binding, client reducer/projection, tests, and
this document. Schema-10 rows remain valid operator-side data and require no
database rollback. Because production negotiation is still disabled, rollback
does not strand live client state or alter mixed-version behavior.
