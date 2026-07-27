# OMENchat announcement-room storage qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `c0b7b49`, plus this schema-11 unit

Verdict: schema migration and guarded schema-10 rollback copy pass locally;
announcement-room authorization, administration, presentation, and protocol
negotiation remain dormant

## Scope

This unit:

- advances omenchatd's SQLite schema from 10 to 11;
- adds constrained `rooms.policy_bits` storage with default `0`;
- projects the stored value through the server-owned room model;
- migrates existing schema-10 rows to ordinary-room policy inside the existing
  immediate migration transaction;
- retains the generated, owner-only pre-v11 schema-10 backup;
- adds `database export-schema10-copy --to <new-path> --confirm`, using the
  existing stopped-server, exclusive-access, no-overwrite, staged-copy,
  integrity-check, atomic-publication path;
- validates that the schema-10 copy removes only `policy_bits` while retaining
  moderation-audit and all earlier schema layers.

No production path can set a nonzero policy through the CLI, TUI, protocol, or
configuration in this unit.

## Storage and migration invariants

- `policy_bits` is `NOT NULL`, defaults to `0`, and is constrained to `0..=1`.
- Existing rooms remain ordinary without a data rewrite.
- Column addition, schema marker 11, and commit are one transaction.
- Injected failure before the column step, before the version update, or before
  commit leaves schema 10 and no partial column.
- Every on-disk migration retains a readable
  `.pre-v11-from-v10.bak` sibling.
- An old binary must not open the schema-11 live database. Rollback is a stopped
  server operation against the separately exported schema-10 copy.
- Export refuses an existing destination, active WAL/SHM state, a live database
  lock, source replacement, or publication failure through the common
  downgrade-copy implementation.
- Export never changes the active database and validates SQLite integrity,
  foreign keys, schema version, retained schema-10 objects, and removal of the
  schema-11 column before and after publication.

## Focused validation

Passed:

```text
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  version_ten_database_adds_constrained_ordinary_room_policy --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless room_policy_schema --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless schema_ten_export --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless --quiet)
```

Results:

- schema-10 migration/default/constraint/backup: 1 passed;
- all schema-11 migration fault boundaries: 1 passed;
- schema-10 export, publication fault, and confirmation parsing: 3 passed;
- standalone server-headless suite: 380 passed, 11 ignored.

Full local matrix:

- root `cargo fmt --all --check`: passed;
- root desktop-product tests: 1,497 passed, 31 ignored, plus all integration
  targets passed with their existing explicit ignores;
- root desktop-product all-target strict Clippy: passed with `-D warnings`;
- standalone server `cargo fmt --check`: passed;
- standalone server-headless tests: 380 passed, 11 ignored;
- standalone server-headless all-target strict Clippy: passed with
  `-D warnings`;
- standalone server-full check: passed;
- `git diff --check`: passed.

No hosted CI, Python interoperability, live Reticulum peer, GUI session,
packaging, or hardware run was needed for this SQLite-only server unit.

## Incidental test corrections

The full suite exposed two pre-existing timing races:

- moderation-audit inline/resource equivalence compared wall-clock commit
  seconds from two independently seeded stores; the test now proves timestamp
  shape and compares the transport representations after normalizing that
  intentionally nondeterministic field;
- the bounded administrative database test read completion metrics immediately
  after the response channel became ready; it now uses its existing bounded
  settle helper before asserting metrics.

Neither correction changes production behavior.

## Compatibility and rollback

Wire protocol v1, capability negotiation, destinations, identities,
configuration, room operations, and client models are unchanged. Existing
schema-10 data migrates forward automatically after a recovery backup is
created.

To roll back:

1. stop omenchatd cleanly;
2. run
   `omenchatd database export-schema10-copy --to <new-path> --confirm --home <home>`;
3. validate and preserve the schema-11 source;
4. replace it only through the documented operator-controlled recovery process;
5. start the older schema-10 binary against the exported copy.

## Not claimed

- room policy mutation or atomic authorization;
- server configuration or administration UI;
- policy-aware room catalogs/deltas;
- OMENbrowser GUI/TUI/mock projection;
- mixed-version process traffic;
- package or live Reticulum behavior;
- activation of `announcement-rooms-v1`.
