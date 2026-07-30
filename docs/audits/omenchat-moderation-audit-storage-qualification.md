# OMENchat moderation-audit storage qualification

Date: 2026-07-27
Branch: `release/v0.9.6-4`
Baseline commit: `a4d2066`
Database transition: schema 9 to schema 10

## Scope and verdict

This unit adds constrained server-side moderation-audit storage and couples it
only to durable moderation paths that already own the user mutation and replay
result inside one immediate SQLite transaction. Capability negotiation,
network paging, Resource transport, client state, and presentation remain
dormant.

Verdict: schema migration, storage constraints, bounded retention, durable
transaction coupling, exact-replay suppression, fault rollback, and guarded
schema-9/schema-8 copies pass deterministic tests.

## Ownership and bounds

`moderation_audit_events` is separate from the legacy `audit_log` and the
operator runtime log. It stores only the fixed client-safe fields defined by
`moderation-audit-v1`.

- 2,048 rows per room and 8,192 globally;
- 4 MiB of stable retained-row accounting globally;
- 365-day age limit;
- at most 64 deletions during one admitted mutation;
- 256-byte actor and target display names;
- fixed action/result checks in both Rust and SQLite;
- positive room, actor, target, and audit identifiers;
- newest-first exclusive-cursor reads of at most 256 rows.

Every insertion incrementally removes expired or oldest eligible rows. It
adds no startup scan, worker, queue, cache, timer, retry, or recurring traffic.
`AUTOINCREMENT` prevents pruned audit identifiers from being reused.

## Atomic coverage

The following durable, in-room commands now commit user state, one audit row,
and the durable replay result together:

- kick;
- ban and unban;
- mute and unmute;
- role change, including the existing standard role value zero.

Exact replay returns the stored response and does not invoke the mutation
callback, so it cannot add another audit row. Injected failure after both the
user update and audit insertion rolls back those changes and leaves no replay
result.

Legacy non-durable client moderation, roomless role/unban, and local
administrative TUI operations remain absent from client-visible history. Their
current storage boundaries cannot make the mutation and audit row atomic.
Including them would make the history misleadingly partial after a fault.

## Migration and rollback

Schema 9 migrates transactionally to an empty schema-10 audit table and two
indexes without scanning users, events, pins, or operator logs. Every injected
table/index/version/commit boundary rolls back to schema 9, and the generated
schema-9 backup remains valid.

Offline `export-schema9-copy` removes only schema-10 moderation-audit storage
while preserving pins and all earlier layers. `export-schema8-copy` removes
both moderation-audit and pin layers. Both use the existing confirmation,
exclusive-access, no-sidecar, owner-only staging, integrity, foreign-key, and
atomic-publication boundary. Neither changes the active database.

## Validation

Focused gates:

```bash
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless moderation_audit --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless durable_active_peer_moderation_executes_once_for_each_action --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless durable_role_changes_once_and_replays_without_broadcasts --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless durable_unban_changes_once_and_replays_without_broadcasts --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless schema_eight_export --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless schema_nine_export --lib)
```

All applicable gates passed:

- shared protocol: 47 passed;
- desktop product library: 1,493 passed, 30 explicit live/platform/soak
  ignores;
- standalone omenchatd: 368 passed, 10 explicit live/soak ignores;
- root and server formatting;
- desktop-product check and strict Clippy;
- server-headless and server-full checks and strict Clippy.

Live peers, Python interoperability, native packaging, and hosted CI are not
required for a dormant server-only schema/storage unit. They remain release
gates when capability negotiation and client traffic are activated.

## Next gate

The next unit may add authorized bounded paging and ephemeral client
projection behind test-only negotiation. It must prove inline/Resource
equality, role loss, cursor/end behavior, capability loss, and privacy before
production negotiation changes.
