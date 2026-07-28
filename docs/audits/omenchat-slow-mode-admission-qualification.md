# OMENchat slow-mode atomic admission qualification

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `7973c65`

Status: test-only durable/legacy admission complete; configuration, capability
negotiation, production enforcement, and client projection remain inactive

## What changed

`SessionEngine` now owns a project-local monotonic deadline map keyed by
`(room_id, user_id)`. The map is bounded to the schema-12 ledger limits:

- 4,096 items per room;
- 16,384 items globally;
- 512 KiB of fixed logical state; and
- at most 64 expired removals per reservation.

It has no worker, channel, timer, retry loop, or shutdown task. Reservations
insert a deadline while releasing the mutex immediately. A committed
reservation retains the deadline. Dropping an uncommitted reservation restores
the prior entry or removes the new entry, so SQLite failure cannot consume
in-process admission.

The existing durable replay transaction remains authoritative for ordering.
Exact replay and mutation-ID hash conflict are resolved before the mutation
callback. For a new standard-member room message/action, the callback performs
authorization and body validation, reserves the monotonic deadline, writes the
schema-12 deadline, appends the event, encodes the origin result, and inserts
the durable replay result within the same immediate SQLite transaction. The
monotonic reservation is committed only after SQLite commit.

The test-only legacy path uses the same owner and a store transaction coupling
membership, persistent admission, and event append. Moderators/admins bypass
slow mode. Notices, commands, uploads, reactions, revisions, pins, joins, and
parts remain outside this policy.

## Production isolation

All normal `SessionEngine` constructors set slow-mode enforcement to false.
Only the test constructor enables it. No client requests and no server accepts
`room-slow-mode-v1`; no six-field room value or typed 1017 response is emitted
by a live negotiated session. A regression sets a nonzero stored interval and
proves two ordinary production-path messages still succeed without creating an
admission row.

This unit adds one empty `Arc<Mutex<BTreeMap<...>>>` owner to a session engine.
While enforcement is dormant it retains zero entries and causes no recurring
CPU, disk, network, redraw, or wakeup work.

## Failure and compatibility evidence

The tests prove:

- exact durable replay returns the original acknowledgement without consulting
  or extending cooldown state;
- mutation-ID reuse with different content remains a conflict;
- event, replay result, and persisted deadline roll back together when result
  encoding fails;
- legacy event and deadline roll back together on an injected fault;
- a second message/action is rejected without appending an event;
- leave/rejoin does not erase the deadline;
- reopening omenchatd retains conservative admission;
- a backward monotonic observation cannot shorten an active deadline;
- announcement policy and malformed-body rejection create no admission;
- moderators bypass slow mode and create no admission;
- disabling bypasses retained state while re-enabling conservatively restores
  a still-active prior deadline;
- competing reservations for one room/user serialize;
- expired pruning is incremental and active saturation fails closed; and
- production constructors preserve prior behavior.

No wire value, protocol version, capability vector, error shape, configuration
file, identity, destination, database path, dependency, or feature changed.
Existing schema-11 rollback/export behavior remains the recovery boundary.

## Validation

Focused matrix:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless slow_mode -- --nocapture

18 passed; 0 failed
```

The first focused run passed 14 tests and exposed one faulty owner-test
expectation: a reservation matched as a temporary value was immediately
dropped and correctly rolled itself back. The fixture now explicitly commits
that reservation; the rerun passed all 15 tests then present. Two additional
atomic-rollback and production-dormancy regressions raised the focused count to
17. The explicit disable/re-enable policy regression raised the final count to
18.

Full standalone headless matrix:

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless

409 passed; 0 failed; 11 ignored
```

Strict static checks passed for both standalone server products:

```text
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless --all-targets -- -D warnings
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
```

`bash scripts/release-check.sh quick` passed, including formatting,
dependency/version/product-feature assertions, native CLI identity isolation,
real-PTY lifecycle recovery, focused desktop/server OMENchat tests, both server
profiles, IFAC vectors, and relocated standalone omenchatd build/test checks.
The gate continues to report only the repository's accepted root build-time
Wayland scanner/quick-xml advisories; omenchatd has neither advisory and this
unit changes no dependency.

The ignored tests are the existing explicit 60-second link/database/queue/log
soaks, multiprocess Resource cases, known upstream maximum-UDP-Resource
regression, explicit cancellation gate, and isolated retention measurements.
None is claimed as passed by this unit.

## Remaining work

The next slice is the confirmation-gated stopped-server administration/status
boundary for the scalar. It must preserve exclusive database ownership, report
prior/effective values, retain room revision semantics, and leave production
negotiation disabled. Client projection, live current/current and mixed-version
process tests, measurements, and activation remain later gates.
