# OMENchat slow-mode checkpoint

Date: 2026-07-27

Status: dormant wire/storage contracts and test-only atomic admission complete;
configuration, negotiation, production enforcement, and UI behavior remain
inactive

Baseline: `release/v0.9.6-4` at `634fa37`

Release target: `v0.9.6-4`

## Purpose

Define the smallest compatible slow-mode contract before changing OMENchat
protocol version 1 or omenchatd schema 11. Slow mode is a per-room publication
cooldown. It is not the existing server-wide messages-per-minute limiter and
must not weaken durable mutation replay, announcement-room authorization,
history retention, or mixed-version behavior.

This checkpoint does not authorize per-room upload/media policy. That remains a
separate Unit 6G risk class.

## Current implementation evidence

- Protocol version 1 room values are exactly four fields for legacy peers and
  five fields for peers that negotiated `announcement-rooms-v1`.
- `RoomCatalogEntry` currently carries `policy_bits` but no scalar room policy.
- Schema 11 stores only `rooms.policy_bits`; existing rows default to ordinary
  policy zero.
- The server-wide message limiter is a 60-second fixed window keyed by
  `(identity, message-or-command)`. It is not keyed by room and uses Unix time.
  Its rollback reservation is useful, but its semantics are not slow mode.
- New durable room events and their replay result commit through one immediate
  SQLite transaction. An exact duplicate returns the original result before
  executing the mutation closure; conflicting reuse of a mutation identifier
  fails closed.
- `room_members` is deleted on Part and therefore cannot own a cooldown that
  must survive leave/rejoin or server restart.
- The announcement policy is enforced regardless of negotiation. Negotiation
  controls evidence and room-value shape, not server authority.

## Behavioral contract

Slow mode applies to new member-authored:

- `RoomMessage`; and
- `RoomAction`.

It does not consume a cooldown for:

- an exact durable replay;
- a rejected, rolled-back, malformed, oversized, muted, banned, or unauthorized
  operation;
- moderator/admin `RoomNotice`;
- reactions, corrections, tombstones, pins, joins, parts, or commands; or
- upload offers/completions, which belong to the later room media-policy unit.

Moderators and administrators bypass slow mode so they can operate and moderate
the room. Standard and trusted members do not. Announcement-room authorization
runs first: a member prohibited from publishing does not consume slow-mode
state.

`0` disables slow mode. Enabled values are whole seconds from `1` through
`86_400`. Configuration outside this range fails closed rather than wrapping or
silently changing units.

An uncertain mutation is never automatically resent merely because its
cooldown may have elapsed.

## Capability and exact wire shapes

Proposed capability:

```text
room-slow-mode-v1
```

The production client requests it only alongside `durable-mutations-v1`. The
server accepts it only when explicitly requested and durable mutation identity
was accepted. Enforcement does not depend on negotiation: an older client is
still rate limited, but does not receive the richer room projection.

Room catalog values remain positional and exact:

```text
legacy:
  [room_id, name, topic_or_nil, room_revision]

announcement-rooms-v1 only:
  [room_id, name, topic_or_nil, room_revision, policy_bits]

room-slow-mode-v1:
  [room_id, name, topic_or_nil, room_revision, policy_bits,
   slow_mode_seconds]
```

Negotiating `room-slow-mode-v1` means the peer understands both the policy-bit
field and the bounded scalar field. A six-field value is never sent to a peer
that accepted only `announcement-rooms-v1`. Unknown policy bits, a wrong field
count/type, or a slow-mode value above `86_400` fails before projection.

The implementation should replace boolean room-shape parameters with a small
project-owned enum (`Legacy`, `PolicyBits`, `SlowMode`) rather than adding a
second ambiguous boolean.

Proposed typed rejection for a negotiated peer:

```text
ChatErrorCode::SlowModeActive = 1017
[1017, user-safe message, retry_after_seconds]
```

`retry_after_seconds` is an integer in `1..=86_400`. A non-negotiating peer
receives the existing two-field `RateLimited` error so adjacent clients keep a
shape they already parse. The client may show static retry evidence and enable
send after the deadline or the next authoritative room update; it must not add
a perpetual one-second redraw subscription.

## Persistent schema proposal

Schema 12 adds:

```sql
ALTER TABLE rooms
  ADD COLUMN slow_mode_seconds INTEGER NOT NULL DEFAULT 0
  CHECK(slow_mode_seconds BETWEEN 0 AND 86400);

CREATE TABLE room_slow_mode_admissions(
  room_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  not_before_unix INTEGER NOT NULL CHECK(not_before_unix >= 0),
  updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
  PRIMARY KEY(room_id, user_id)
);

CREATE INDEX idx_room_slow_mode_admissions_expiry
ON room_slow_mode_admissions(not_before_unix, room_id, user_id);
```

The admission table is separate from `room_members` so Part/rejoin cannot erase
the cooldown. It stores no identity bytes, message content, or mutation
identifier.

Logical retention limits:

- at most 4,096 admission rows per room;
- at most 16,384 rows globally;
- fixed logical accounting of 32 bytes per row, at most 512 KiB globally;
- at most 64 expired rows pruned by one successful admission attempt; and
- rows whose `not_before_unix` is no longer active are eligible for pruning.

The actual fixed-row SQLite overhead is implementation evidence to measure, not
a reason to claim exact file bytes. If caps remain saturated after bounded
pruning, a new cooldown owner fails closed with a typed store-busy/rate-limit
result. It must not evict an active cooldown.

Migration must:

- add the column, table, index, and version update in one transaction;
- create no admission rows and scan no history, users, or memberships;
- retain a pre-schema-12 backup through the existing recovery boundary;
- inject faults after the column, table, index, version, and commit boundaries;
  and
- provide a confirmation-gated, stopped-server schema-11 copy export that
  removes only slow-mode state and the scalar column.

## Atomic and monotonic enforcement

Two clocks serve different failure boundaries:

1. A bounded in-process monotonic deadline map prevents wall-clock jumps from
   shortening an active cooldown while omenchatd is running.
2. The transactional `not_before_unix` row preserves conservative enforcement
   across restart. A backward wall-clock jump fails closed until the persisted
   deadline is reached; it never clears the row automatically.

The monotonic map is owned by `SessionEngine`, keyed by `(room_id, user_id)`,
bounded by the same 16,384-item/512-KiB logical ceiling, and uses incremental
expired-entry pruning. It has no task, timer, channel, or retry loop. Link
retirement does not erase it. Process shutdown drops only the in-memory copy;
SQLite remains authoritative for restart.

For a new durable message/action in the test-only admission path:

1. Resolve exact replay/conflict before cooldown work.
2. Validate membership, ban/mute state, announcement policy, body, and bounds.
3. In the existing immediate durable transaction callback, read the current
   room setting and reserve the in-process monotonic deadline. The owner lock is
   released immediately; it is never held across subsequent SQLite work.
4. Read the persisted admission row in the same transaction.
5. Reject if either authoritative deadline is active.
6. Append the event, update `not_before_unix`, and store the durable result in
   that same transaction.
7. Commit the monotonic reservation only after SQLite commit; dropping a failed
   reservation restores the prior in-memory deadline.

A crash after SQLite commit but before the monotonic commit remains protected
by `not_before_unix`. A rollback consumes neither deadline. Competing Links for
the same identity/user must serialize through the same bounded reservation.

Legacy non-durable message/action paths use the same admission owner but cannot
gain replay guarantees. Their rejection text remains cautious and they are
never retried automatically.

## Administration and client projection

The first release uses the existing stopped-server maintenance boundary:

```text
omenchatd rooms set-slow-mode <room-id> off --confirm
omenchatd rooms set-slow-mode <room-id> <seconds> --confirm
```

Updating the scalar and `room_revision` is one immediate transaction. A no-op
does not increment the revision. Disabling slow mode makes existing admission
rows inactive; bounded pruning removes them later. Re-enabling before a prior
deadline expires conservatively makes that prior deadline authoritative again;
traffic accepted while disabled does not create or extend a cooldown. The
command must refuse an active database writer and print the prior and effective
value.

Negotiated clients show a compact `Slow mode · Ns` room indicator. On typed
rejection, the pending draft remains intact and the UI reports the authoritative
retry delay. GUI and TUI consume the same project-owned room-policy state. No
client-side countdown is treated as permission to send; the server remains
authoritative.

## Compatibility

- Protocol version remains 1.
- Destination aspects, identity ownership, database paths, and message/event
  formats do not change.
- Existing rooms migrate with slow mode disabled.
- Legacy clients retain four-field rooms and generic rate-limit errors.
- Announcement-capable adjacent clients retain five-field rooms.
- Current clients accept six fields only after explicit slow-mode negotiation.
- Current servers enforce configured slow mode even for old clients.
- Exact durable duplicates return their original result even if room policy or
  cooldown state changed afterward.

## Test matrix before activation

Wire:

- exact four-/five-/six-field fixtures in both independent codecs;
- wrong shape/type, unknown bits, and `86_401` rejection;
- capability request/accept/loss and replacement-Link clearing;
- adjacent client/server room catalog and delta compatibility.

Storage:

- schema-11 migration defaults without history scan;
- every injected migration fault rolls back to schema 11;
- schema-11 copy export retains all unrelated state;
- atomic scalar/revision update and no-op behavior;
- admission item/byte caps and bounded incremental pruning;
- clock-backward fail-closed behavior.

Mutation:

- first member message/action commits and starts one cooldown;
- early second mutation returns typed rejection and does not append;
- moderator/admin bypass;
- announcement rejection consumes no cooldown;
- malformed/oversized/rolled-back mutation consumes no cooldown;
- exact duplicate returns the original result;
- mutation-ID hash conflict remains a conflict;
- two Links for one identity cannot race through admission;
- leave/rejoin, client restart, and server restart preserve the cooldown;
- disabling/re-enabling policy has documented state behavior.

Process/UI:

- current/current real-Link message, rejection, expiry, and restart;
- current/adjacent mixed-version traffic;
- GUI and TUI retain drafts and show truthful static evidence;
- no automatic uncertain retry;
- no recurring polling/redraw loop;
- idle CPU/RSS and retained admission-state measurements.

## Staged implementation order

1. **Complete (2026-07-27):** add dormant protocol constants, shape enum,
   byte-exact independent codec fixtures, error type, and durable-capability
   dependency. Evidence:
   `docs/audits/omenchat-slow-mode-wire-qualification.md`.
2. **Complete (2026-07-28):** add schema-12 disabled scalar, bounded persistent
   admission ledger, migration/restart/fault tests, and guarded schema-11 copy
   export. Evidence:
   `docs/audits/omenchat-slow-mode-storage-qualification.md`.
3. **Complete (2026-07-28):** add test-only atomic admission to durable and
   legacy message/action paths, a bounded rollback-on-drop monotonic owner, and
   replay/restart/rollback/dormancy tests. Evidence:
   `docs/audits/omenchat-slow-mode-admission-qualification.md`.
4. Add stopped-server administration and status evidence.
5. Add bounded client projection and shared GUI/TUI evidence.
6. Run current/current, restart, mixed-version, resource, and measurement
   gates.
7. Activate negotiation only after every gate passes.

Rollback before activation is a code revert plus schema-11 copy restore.
Rollback after activation disables the capability first, preserves admission
rows, and follows the guarded schema-11 copy procedure. No identity, room
history, upload, durable replay, or Reticulum state is deleted automatically.
