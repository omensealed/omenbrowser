# OMENchat corrections and tombstones checkpoint

Status: design checkpoint; no capability, wire operation, schema, or UI action is active  
Baseline: OMENbrowser/omenchatd `0.9.6-3`, planned `0.9.6-4`  
Protocol baseline: `omenchat-v0.1`, numeric protocol version 1  
Required base capability: `durable-mutations-v1`

## Decision

Corrections and deletions must be an additive, explicitly negotiated
protocol-v1 extension. They must not rewrite an original room event, reuse its
event ID, overload an ordinary message or command, or treat a deletion as
secure erasure.

The proposed capability name is:

```text
message-revisions-v1
```

It is valid only when `durable-mutations-v1` is requested and accepted on the
same authenticated Link. Capability state is Link-scoped and is cleared on
retirement, identity replacement, reconnect, or downgrade. Application
versions never imply support.

The initial extension covers corrections and tombstones for ordinary room
messages, including messages carrying reply/mention metadata. It does not edit
actions, notices, uploads, system events, reactions, pins, moderation records,
or room metadata.

This checkpoint does not authorize activation. The current server retains
ordinary room history indefinitely, so a tombstone cannot yet be pruned without
either resurrecting its target or removing the target and tombstone together.
The capability must remain dormant until Unit 6G supplies a tested room-history
retention/compaction rule or the release explicitly defers this feature.

## Existing boundaries verified

- `src/server/crates/omenchat-protocol` is the shared wire-contract crate. It
  contains no SQLite, Iced, Ratatui, Reticulum ownership, filesystem storage, or
  server policy.
- `DurableMutationEnvelope` already binds the exact operation, room, canonical
  body, persistent random client instance, and random 128-bit mutation ID.
- omenchatd's durable executor can commit a state mutation, append-only audit
  record, exact origin result, and replay publication in one immediate SQLite
  transaction. Exact replay does not repeat a rate charge or fan-out.
- The server is at schema 5. `room_events` has immutable room/event IDs, reply
  and mention metadata, and a legacy `deleted` flag. Production history queries
  currently hide `deleted = 1`; only tests set it. This flag must not become
  the correction/tombstone contract.
- The client history database is additive rather than version-numbered and its
  resident session history is bounded to 1,024 events / 8 MiB.
- Reactions already demonstrate the required pattern: separate current state,
  bounded append-only audit, explicit-target snapshots, capability-scoped
  fan-out, and exact replay.
- `RoomEventCode::MessageEdited` and `MessageDeleted` are historical enum
  assignments only. No current wire body, storage transaction, capability, or
  client reducer gives them safe correction/tombstone semantics.

## Wire proposal

Reserve the currently unused operation range between `RoleDelta` (34) and the
history family (40):

| Operation | Proposed number | Direction | Purpose |
| --- | ---: | --- | --- |
| `RoomMessageRevision` | 35 | client to server | Durable correction/tombstone request |
| `MessageRevisionAck` | 36 | server to origin | Exact retained semantic result |
| `MessageRevisionEvent` | 37 | server to capable room peers | One committed state change |
| `MessageRevisionSnapshotInline` | 38 | server to capable client | Current state for explicit history targets |
| `MessageRevisionSnapshotResource` | 39 | server to capable client | The same bounded snapshot over Resource |

No operation is added to `ChatOp` until the shared codec and byte fixtures are
implemented together. Once assigned, these numbers must never be reused.

### Request

`RoomMessageRevision` uses the frame room ID and this exact fields body:

```text
[
  "message-revision-v1",
  target_event_id: u64,
  action: u64,
  replacement: string | nil
]
```

`action = 1` is a correction and requires a nonempty replacement string.
`action = 2` is a tombstone and requires `nil`. No optional reason is included
in v1; this prevents an unbounded or misleading second message body from being
smuggled into a deletion. No trailing fields or alternate integer types are
accepted.

The replacement is bounded by the server's effective `max_message_bytes`
setting and by the existing protocol scalar/body ceilings. It must differ from
the current effective text. A request always targets the immutable original
message ID, never an earlier correction event.

The canonical durable hash covers operation 35, room ID, target event ID,
action, and exact replacement. Reusing a mutation ID with different content,
target, action, or room is a durable conflict.

### Acknowledgement

`MessageRevisionAck` uses:

```text
[
  target_event_id: u64,
  action: u64,
  actor_user_id: u64,
  changed: bool,
  revision_event_id: u64 | nil,
  revision_number: u64
]
```

First execution returns `changed = true` and the committed revision-event ID.
An exact durable replay returns the originally encoded acknowledgement with
only the transient frame sequence recoupled by the existing replay boundary.
The initial implementation should reject semantic no-ops rather than create a
second mutation identity with `changed = false`; this keeps audit meaning and
client reconciliation unambiguous.

### Committed event

`MessageRevisionEvent` uses the frame room ID and:

```text
[
  revision_event_id: u64,
  target_event_id: u64,
  action: u64,
  actor_user_id: u64,
  at_unix: i64,
  replacement: string | nil,
  revision_number: u64,
  actor_display_name: string | nil
]
```

It is emitted only after commit, only once for first execution, and only to
same-room Links that accepted `message-revisions-v1`. It does not increment the
ordinary message unread count, generate a new reply/mention notification, or
claim that previously delivered copies were erased.

### Explicit-target snapshot

Every capable history page is followed by current revision state for only the
original message targets represented by that page:

```text
[
  "message-revision-snapshot-v1",
  [target_event_id, ...],
  [
    [
      target_event_id,
      latest_revision_event_id,
      action,
      actor_user_id,
      at_unix,
      replacement: string | nil,
      revision_number
    ],
    ...
  ]
]
```

Targets are sorted and unique. Rows are sorted by target ID and every row must
belong to the explicit target set. An empty row list is authoritative for that
target set and removes stale local revision state. A snapshot never clears
another page or room.

The shared decoder ceiling is 256 targets and 256 state rows. Inline and
Resource forms share one decoder and the existing compressed-batch,
decompression, cancellation, byte, item, and Link-ownership bounds. The server
selects Resource before the safe inline threshold and never truncates a
snapshot while presenting it as complete.

## Server authorization and semantics

A mutation is admitted only when:

- the Link is authenticated, currently joined, and negotiated both required
  capabilities;
- the target is a non-deleted ordinary message in the same room;
- the target has a stable nonzero actor user ID;
- the request shape, replacement, and durable hash are canonical;
- the target has not already been tombstoned;
- the correction-depth, state, audit, rate, and replay ceilings permit it.

The original author may correct or tombstone their message. A moderator or
administrator may tombstone another user's message but must not rewrite that
user's words. A muted author may tombstone their own message but may not
correct it, because a correction could bypass mute policy. Banned or parted
users cannot mutate room history.

Corrections preserve the original message's reply target and mentioned-user
metadata. They do not send a second mention notification. A tombstone removes
the body, reply preview, attachment/reaction controls, and mention emphasis
from the effective presentation, but retains original event ID, original
author attribution, deletion actor, time, and revision number. Existing
reaction rows for a tombstoned target are removed in the same transaction and
the capable client receives an authoritative empty reaction snapshot/delta for
that target.

At most eight corrections are accepted for one target. A tombstone remains
available after the correction limit. Nothing can correct or restore a
tombstoned target in v1.

On a replay miss, target lookup, authorization, rate reservation, current-state
update, append-only audit insertion, reaction cleanup when applicable, exact
acknowledgement encoding, durable replay publication, and incremental audit
pruning occur in one immediate transaction. A codec, storage, or replay
publication failure rolls everything back. Only the first committed execution
produces a fan-out effect.

Use the existing command-rate admission initially. Do not add a timer, retry
loop, rate-limiter worker, or unbounded cache for revisions.

## Server schema proposal

omenchatd moves from schema 5 to schema 6 only after migration and downgrade
tests exist. Original `room_events` rows and the legacy `deleted` column remain
unchanged.

```sql
CREATE TABLE room_message_revision_state(
  room_id INTEGER NOT NULL,
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  latest_revision_event_id INTEGER NOT NULL CHECK(latest_revision_event_id > 0),
  revision_action INTEGER NOT NULL CHECK(revision_action IN (1, 2)),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  replacement_body BLOB,
  revision_number INTEGER NOT NULL CHECK(revision_number BETWEEN 1 AND 9),
  at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  PRIMARY KEY(room_id, target_event_id),
  UNIQUE(room_id, latest_revision_event_id),
  CHECK(
    (revision_action = 1 AND replacement_body IS NOT NULL) OR
    (revision_action = 2 AND replacement_body IS NULL)
  )
);

CREATE INDEX idx_room_message_revision_state_event
ON room_message_revision_state(room_id, latest_revision_event_id);

CREATE TABLE room_message_revision_events(
  room_id INTEGER NOT NULL,
  revision_event_id INTEGER NOT NULL CHECK(revision_event_id > 0),
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  revision_action INTEGER NOT NULL CHECK(revision_action IN (1, 2)),
  replacement_body BLOB,
  revision_number INTEGER NOT NULL CHECK(revision_number BETWEEN 1 AND 9),
  at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  PRIMARY KEY(room_id, revision_event_id),
  CHECK(
    (revision_action = 1 AND replacement_body IS NOT NULL) OR
    (revision_action = 2 AND replacement_body IS NULL)
  )
);

CREATE INDEX idx_room_message_revision_events_target
ON room_message_revision_events(room_id, target_event_id, revision_event_id);

CREATE INDEX idx_room_message_revision_events_retention
ON room_message_revision_events(at, room_id, revision_event_id);
```

Revision-event IDs are allocated transactionally from a revision-specific
room sequence; they are not ordinary `room_events.event_id` values and are
never accepted as message/reply/reaction targets.

### State and audit bounds

Initial conservative ceilings:

- eight corrections plus one tombstone per target;
- 3,072 correction-state rows / 6 MiB per room soft ceiling;
- 49,152 correction-state rows / 96 MiB server-wide soft ceiling;
- 4,096 total state rows / 8 MiB per room hard ceiling;
- 65,536 total state rows / 128 MiB server-wide hard ceiling;
- 8,192 append-only audit rows / 8 MiB per room;
- 131,072 append-only audit rows / 128 MiB server-wide;
- 365-day audit age;
- at most 64 audit rows pruned during one committed mutation.

New corrections stop at the correction-state soft ceiling, reserving capacity
for tombstones. Tombstones stop at the hard ceiling and report a bounded,
operator-visible failure; they never evict another live tombstone or silently
resurrect a message. Audit pruning never removes the current-state row and
never scans or deletes an unbounded result set.

A tombstone state row is retained for at least as long as its original target.
The future room-retention transaction must remove the original, its revision
state, revision audit, reactions, and dependent reply projections together.
Until that transaction exists, hard-ceiling saturation is a known activation
blocker rather than permission for unbounded growth.

## Client model and storage proposal

The project-owned model adds one `ChatMessageRevision` per retained target:
server, room, target event, latest revision event, action, actor, replacement,
time, and revision number. It remains separate from `ChatEventKind`; the
original event stays immutable and the presentation reducer derives effective
text/tombstone state.

The identity-scoped client database adds:

```sql
CREATE TABLE room_message_revision_state(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  latest_revision_event_id INTEGER NOT NULL CHECK(latest_revision_event_id > 0),
  revision_action INTEGER NOT NULL CHECK(revision_action IN (1, 2)),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  replacement_body TEXT,
  revision_number INTEGER NOT NULL CHECK(revision_number BETWEEN 1 AND 9),
  at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  PRIMARY KEY(server_id, room_id, target_event_id)
);
```

The table is additive and ignored by older clients. It is a rebuildable cache,
not the authority. Deltas and explicit-target snapshots replace it
transactionally only after complete validation.

Client retention is limited to targets still present in bounded local history:

- at most one state row per target;
- 1,024 rows / 8 MiB per room;
- 8,192 rows / 32 MiB per server;
- 32,768 rows / 64 MiB per identity-scoped store.

History eviction prunes orphaned local revision rows incrementally. Saturated
or malformed snapshots leave the prior target set unchanged and mark revision
state unavailable rather than rendering partial state as authoritative.

GUI and any future OMENchat TUI must share the same reducer and terminology:
`edited`, `deleted`, `revision state unavailable`, and `uncertain`. The current
Ratatui workspace has LXMF conversations but no OMENchat timeline; this unit
must not invent a separate TUI state machine or claim TUI support that does not
exist.

## Mixed-version behavior

- A current client does not send operation 35 until the server explicitly
  accepts `message-revisions-v1`.
- A current server sends operations 37–39 only to Links that accepted the
  capability.
- Current/current capable history keeps ordinary original events unchanged and
  follows each page with an explicit revision snapshot.
- Current client / older server hides correction/delete controls and retains
  ordinary history.
- Older client / current server receives no revision frames and retains its
  immutable view. A deletion is therefore a visible moderation tombstone for
  capable clients, not retroactive erasure from an older peer that already
  received the body.
- Capability loss blocks explicit retry of a recovered revision intent.
  Nothing converts it into a message/command or resends it automatically.
- Adjacent-version fixtures must prove ordinary protocol-v1 messages and
  history remain byte-compatible.

## Migration and rollback

Before schema 6 activation:

1. Copy and validate representative schema 0–5 databases.
2. Create the existing guarded pre-migration backup.
3. Add state, audit, and indexes in one immediate migration transaction.
4. Set `user_version = 6` only immediately before commit.
5. Inject failures before tables, between tables, before indexes, before the
   version update, and before commit.
6. Verify every failure leaves the active database at its prior version and
   preserves a restorable backup.

A stopped-server, confirmation-gated `database export-schema5-copy` command
must stage a separate copy, drop only revision tables/indexes, set
`user_version = 5`, run integrity and foreign-key checks, and atomically publish
without overwrite. It never modifies the active database in place.

Disabling capability advertisement/acceptance is the normal rollback and keeps
all state for later reconciliation. Restoring a pre-schema-6 backup is an
emergency rollback and can omit post-migration activity. Older clients may
drop only the additive, rebuildable local revision-state table while stopped.

## Failure and crash matrix

Deterministic isolated tests must cover:

- commit with lost acknowledgement;
- Link close after commit but before origin delivery;
- explicit exact retry on a replacement Link;
- client restart with prepared and uncertain intent;
- server restart after commit;
- exact duplicate, concurrent exact duplicate, and changed-content conflict;
- target, action, room, and replacement mutation-ID conflicts;
- author correction and author/moderator tombstone authorization;
- moderator correction of another user's text rejection;
- missing, cross-room, already-tombstoned, non-message, and deleted legacy
  target rejection;
- correction number eight acceptance and number nine rejection;
- tombstone acceptance after the correction ceiling;
- soft/hard state limits and audit item/byte/age pruning;
- reaction cleanup and no repeat cleanup/fan-out on replay;
- no repeat notification, unread increment, rate charge, or audit insertion;
- malformed, oversized, trailing, noncanonical, and invalid-action wire data;
- inline/Resource snapshot equality, cancellation, decompression bounds, and
  replacement-Link ownership;
- event-before-snapshot, stale snapshot, duplicate event, snapshot saturation,
  local history eviction, and restart rebuild;
- database busy, disk/write failure, result-codec failure, and each migration
  transaction boundary;
- current/current, current/adjacent, capability rejection/loss, and shutdown.

Every filesystem and SQLite test uses an explicit temporary isolated root. No
test opens the maintainer's browser, identity, messages, upload cache, or
omenchatd state.

## Implementation sequence

1. **This checkpoint.** Review and approve exact capability, operations,
   request/result/event/snapshot shapes, authorization, state/audit schema,
   retention, mixed-version behavior, and the dependency on room-history
   retention. No production code or schema changes.
2. Add shared bounded codecs, negotiation dependency validation, canonical
   hash vectors, and byte-exact client/server fixtures. Keep the capability
   unrequested and unaccepted.
3. Add schema 6 migration, injected rollback tests, recovery allowlist updates,
   and the separate schema-5 downgrade-copy command. Keep the capability
   dormant.
4. Add the dormant transactional omenchatd executor, state/audit bounds,
   exact replay/conflict behavior, reaction cleanup, capability-scoped fan-out,
   and explicit-target snapshots.
5. Add the bounded client state/cache, persistent intent kind, reducer,
   snapshots/deltas, restart reconciliation, and dormant GUI controls.
6. Complete room-history retention/compaction so an original and all dependent
   revision/reaction/reply projections are removed atomically. This is required
   before capability activation.
7. Run deterministic, mixed-version, retention, Resource, restart, fault, and
   live isolated smoke gates.
8. Request and accept `message-revisions-v1` only after every gate passes.

Each step must leave root and standalone server builds valid and independently
reversible. No step may introduce automatic retry.

## Checkpoint validation

On 2026-07-26 the unchanged shared protocol contract passed independently from
both Cargo roots:

```bash
cargo fmt --all -- --check
cargo test --locked -p omenchat-protocol
(
  cd src/server
  cargo test --locked -p omenchat-protocol
)
git diff --check
```

Each protocol invocation passed 28 tests. These results validate only the
existing durable, negotiation, reply/mention, reaction, and compatibility
foundation. No correction/tombstone codec, schema, migration, executor, client
reducer, native package, mixed-version process, or live Reticulum test exists
yet, and this checkpoint does not claim otherwise.

## Completion gate

Do not advertise `message-revisions-v1` until:

- both independent codecs share byte-exact fixtures;
- schema migration, downgrade copy, recovery, restart, and fault injection pass;
- original message IDs and bodies remain immutable at rest;
- state and audit tables stay inside item, byte, age, and work ceilings;
- room-history retention cannot orphan or resurrect a target;
- exact replay returns the original result without another state change,
  audit row, rate charge, reaction cleanup, notification, or fan-out;
- changed-content reuse conflicts;
- capable history reconciliation never fabricates revision state;
- older peers retain ordinary protocol-v1 behavior and the documented
  non-erasure limitation is visible;
- capability absence/rejection/loss hides controls and blocks retry;
- no maintainer data or identity is touched.

This checkpoint authorizes no wire operation, capability advertisement,
database migration, worker, timer, retry, or UI control by itself.
