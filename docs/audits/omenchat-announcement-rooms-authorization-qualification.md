# OMENchat announcement-room authorization qualification

Date: 2026-07-27

Baseline: `release/v0.9.6-4` at `acedddc`, plus this authorization unit

Verdict: server authorization and stopped-server policy administration pass
locally; policy capability negotiation and room-policy presentation remain
dormant

## Scope

This unit adds:

- one store-owned room-content admission predicate;
- current-policy reads inside every durable content-mutation transaction;
- legacy message, action, notice, and upload admission enforcement;
- durable message, notice, reaction, and message-revision enforcement;
- upload policy checks both before accepting an offer and immediately before
  durable file publication;
- stable protocol-v1 error `1016` (`RoomPolicyRestricted`);
- an atomic, idempotent policy/revision update;
- confirmation-gated stopped-server administration accepting only
  `ordinary` or `announcement`;
- human and JSON room listings with effective policy and revision.

Pins and moderation commands retain their stricter existing role checks. Join,
part, room/user lists, history, upload fetch, and read-only audit paging remain
readable. Production acceptance of `announcement-rooms-v1` remains disabled
and legacy four-field room values remain unchanged.

## Authorization invariant

For an announcement room:

- moderators and administrators may publish;
- standard and trusted members receive typed error `1016`;
- server enforcement does not depend on client negotiation or UI state;
- durable exact replay returns its retained original result without executing
  after later policy or role changes;
- a new mutation identity reads current policy inside the same immediate
  replay/mutation transaction;
- policy rejection occurs before rate reservation, event allocation, replay
  effect fanout, pending-upload insertion, filesystem publication, or ledger
  mutation;
- changing policy and incrementing `room_revision` commit together;
- an idempotent same-policy update does not increment revision.

## Administrative boundary

With omenchatd stopped:

```bash
omenchatd rooms policy <room_id> ordinary|announcement \
  --confirm --home <server-home>
omenchatd rooms list --json --home <server-home>
```

The database must already exist at current schema 11. The command uses the
bounded single-owner administrative database worker and obtains exclusive
SQLite access before mutation. It exposes no arbitrary numeric policy input,
does not create a missing server home, and adds no live reload, queue, timer,
worker, or network fanout.

## Focused validation

Passed:

```text
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless announcement_room --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless room_content_policy --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless room_policy_update --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  room_policy_maintenance_refuses_an_active_writer --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless cli_parses_admin_config_and_room_commands \
  --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  cli_room_mutations_use_the_initialized_administrative_database_path \
  --lib -- --nocapture)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless --quiet)
```

Results:

- announcement wire dormancy plus legacy/durable authorization: 4 passed;
- shared role/policy predicate: 1 passed;
- atomic policy update, rollback, idempotency, and restart: 2 passed;
- CLI validation and isolated administrative database mutation: 2 passed;
- exclusive-maintenance serialization: 1 passed;
- standalone server-headless suite: 386 passed, 11 ignored.

Full local matrix:

- shared protocol number stability: 1 passed;
- current OMENbrowser typed error-label regression: 1 passed;
- root `cargo fmt --all --check`: passed;
- root desktop-product tests: 1,497 passed, 31 ignored, plus every integration
  target passed with its existing explicit ignores;
- root desktop-product all-target strict Clippy: passed with `-D warnings`;
- standalone server `cargo fmt --check`: passed;
- standalone server-headless tests: 386 passed, 11 ignored;
- standalone server-headless all-target strict Clippy: passed with
  `-D warnings`;
- standalone server-full check: passed;
- `git diff --check`: passed.

No hosted CI, Python interoperability, live Reticulum peer, GUI session,
packaging, or hardware run was needed for this server-policy unit.

## Compatibility and resource impact

Ordinary rooms retain existing behavior. Existing clients receive a normal
protocol error with a stable numeric code and explanatory text; they need not
understand policy evidence to remain safe. Current OMENbrowser labels `1016`
as `room is read-only for members`.

The unit adds one indexed room-row lookup per legacy content admission and one
room-row lookup inside each existing durable mutation transaction. It adds no
queue, cache, background task, timer, retry, scan, history, or recurring
traffic. No protocol, identity, destination, or additional database migration
is introduced beyond the already-qualified schema 11.

## Not claimed

- negotiated five-field room catalog/delta delivery;
- capability-loss clearing in clients;
- GUI/TUI composer disabling or policy badges;
- automatic live policy reload or fanout;
- current/current or adjacent-version process traffic;
- Python, Reticulum, package, or hardware behavior.

## Rollback

Before policy use, revert this unit. After a nonzero policy is stored, stop
omenchatd and use the qualified schema-10 copy export documented in
`docs/audits/omenchat-announcement-rooms-storage-qualification.md`. Preserve
the schema-11 source until rollback is verified.
