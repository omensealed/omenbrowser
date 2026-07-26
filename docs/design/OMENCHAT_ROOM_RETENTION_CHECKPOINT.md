# OMENchat room-history retention checkpoint

Status: design checkpoint; no retention policy, compaction, capability, or UI
control is active  
Baseline: OMENbrowser/omenchatd `0.9.6-3`, planned `0.9.6-4`  
Protocol baseline: `omenchat-v0.1`, numeric protocol version 1

## Decision

Room-history retention must be an explicit, operator-controlled policy with a
disabled compatibility default. Existing databases must not begin deleting
history merely because omenchatd is upgraded.

The first implementation must separate three concerns:

1. preserve monotonically increasing room event identifiers after deletion;
2. establish bounded, byte-accounted history usage without an unbounded startup
   scan;
3. remove a bounded set of original events and every dependent projection in
   one immediate SQLite transaction.

`message-revisions-v1` remains dormant while retention is disabled or its
usage ledger is incomplete. Retention is not secure erasure: peers and local
client caches may already hold an event.

## Current implementation verified

- `room_events` is the authoritative server history table. History is currently
  retained indefinitely.
- `latest_events` and `events_before` page only `deleted = 0` rows and are
  bounded by the caller's configured history batch.
- An event identifier is currently allocated as
  `MAX(room_events.event_id) + 1` for one room inside the writer transaction.
  Compaction would therefore permit identifier reuse after deleting a room's
  newest event or all its events.
- Replies are ordinary immutable message rows with an optional
  `reply_to_event_id` projection. There is an index over that projection but no
  foreign key.
- Reactions have separate bounded current-state and append-only audit tables.
- Dormant corrections/tombstones have separate bounded current-state and
  append-only audit tables.
- Durable mutation results retain the exact original result independently of
  room history. Replaying an exact retained result does not reinsert or
  resurrect a compacted event.
- Upload file quota and eviction are independent of the history event that
  announced an upload. History compaction must not bypass the upload ledger or
  delete files.
- The client already bounds resident history to 1,024 events / 8 MiB and
  incrementally removes orphaned reaction and revision projections. Server
  compaction does not remotely erase older client copies.

## Persistent room event sequence

Before any room event can be deleted, schema 7 must add a persistent high-water
mark:

```sql
CREATE TABLE room_event_sequences(
  room_id INTEGER PRIMARY KEY,
  last_event_id INTEGER NOT NULL CHECK(last_event_id >= 0)
);
```

The first allocation for a room lazily seeds the row from the indexed maximum
existing event ID. The same immediate transaction then advances and returns
the high-water mark. Later allocation never derives an identifier solely from
retained history.

Lazy seeding avoids an unbounded full-history migration scan. It also preserves
an empty legacy room: if all historical rows are removed only after a sequence
row exists, the next ID remains greater than every deleted ID. Compaction must
refuse a room whose sequence row has not been seeded.

The allocator must fail on SQLite integer exhaustion; it must not wrap, reset,
or search for a reusable hole.

## Usage ledger and bounded backfill

Schema 8 adds one ledger row per room. A separate version is deliberate:
schema 7 was already a complete, runnable event-sequence boundary, and silently
adding a table under the same version would strand an intermediate schema-7
checkout.

```sql
CREATE TABLE room_history_usage(
  room_id INTEGER PRIMARY KEY,
  event_count INTEGER NOT NULL CHECK(event_count >= 0),
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  backfill_through_event_id INTEGER NOT NULL CHECK(backfill_through_event_id >= 0),
  backfill_target_event_id INTEGER NOT NULL CHECK(backfill_target_event_id >= 0),
  backfill_complete INTEGER NOT NULL CHECK(backfill_complete IN (0, 1)),
  last_compacted_at INTEGER
);
```

For an existing room, `backfill_target_event_id` is captured from its indexed
maximum when the ledger is first touched. New events above that target update
the ledger immediately. Historical accounting advances in batches of at most
256 events and records its cursor in the same transaction. It must not run an
unbounded startup query or move blocking SQLite work onto an arbitrary async
worker.

Stable retained bytes are:

- payload bytes;
- encoded reply and mention metadata bytes;
- a documented fixed per-row overhead used consistently by admission,
  backfill, deletion, and tests.

SQLite database-file size is not a per-room retained-byte measure and must not
be substituted for this ledger. Retention and revision capability activation
remain unavailable until the room ledger is complete.

## Policy

Compatibility default:

```text
enabled = false
max_age_days = 365
max_events_per_room = 100000
max_bytes_per_room = 268435456
```

The finite values are inert while disabled. Enabling retention requires all
three positive finite limits; zero does not mean unlimited. Configuration is
clamped to documented conservative maxima and rendered back explicitly.

Initial policy is server-wide. Per-room overrides belong to the later room
policy wire/admin slice and must not be inferred from room names or modes.
Until clients have typed per-room policy evidence, no GUI control may imply
that a particular event is guaranteed to remain available.

Age, item, and byte ceilings are independent reasons to select the oldest
event. A single event larger than the configured byte ceiling is admitted only
if the normal message/upload limits permit it, then becomes the oldest
compaction candidate according to policy. One compaction transaction removes
at most 64 original events.

If one batch cannot restore the configured item/byte ceilings, new room-event
admission fails with a bounded operator-visible error. It must not loop,
increase the batch, or partially commit a new event outside the compaction
transaction. Subsequent admissions or an explicit maintenance action may make
further bounded progress.

## Atomic dependency cleanup

For the selected `(room_id, event_id)` set, one immediate transaction must:

1. seed/verify the room event high-water mark;
2. clear `reply_to_event_id` only on surviving reply messages that target a
   selected event;
3. delete reaction current-state rows for selected targets;
4. delete reaction audit rows for selected targets;
5. delete message-revision current-state rows for selected targets;
6. delete message-revision audit rows for selected targets;
7. delete the selected original `room_events`;
8. decrement the history usage ledger by the exact selected item/byte totals;
9. commit the new room event, when compaction is admission-driven, only after
   every prior step succeeds.

A reply is not itself deleted merely because its preview target expired. Its
body, author, timestamp, mentions, and event ID remain. Clearing the target
turns it into an ordinary retained message and prevents a dangling preview.
If the reply is also in the selected batch, it is deleted with that batch.

The transaction does not:

- rewrite a retained original body;
- set or depend on the legacy `deleted` test flag;
- delete an upload file or upload-ledger row;
- delete durable replay results;
- reuse an event, reaction-event, or revision-event identifier;
- emit a correction, tombstone, reaction, mention, or delivery notification.

Any SQL, accounting, codec, disk, or commit failure rolls back dependency
cleanup, event deletion, ledger updates, and admission together.

## Paging and reconciliation

History cursors remain event IDs. Gaps caused by retention are valid and do not
mean a transport event-stream gap. A request before the oldest retained event
returns an empty page and the existing history-end boundary.

Revision and reaction snapshots continue to cover only explicit targets in the
returned page. A compacted target cannot reappear in a later snapshot. Client
local pruning remains bounded and rebuildable; the server does not claim it
has erased prior peer copies.

## Migration and rollback

Schemas 7 and 8 are additive:

- create the sequence table in schema 7 and the usage table in schema 8, each
  in the existing guarded immediate migration transaction;
- do not scan or rewrite `room_events` during migration;
- update recovery validation and fault boundaries;
- provide a schema-7 copy that removes only usage metadata and a schema-6 copy
  that removes both usage and sequence metadata;
- preserve the active schema-8 database and all original history;
- leave retention disabled after migration.

Rolling back to the immediately prior binary requires a confirmed schema-7
copy, which loses only usage metadata. Rolling back past persistent sequences
requires a schema-6 copy, which loses both sequence and usage metadata. No
compaction may occur before the ledger and policy gates pass. Once any history
has been compacted, rollback cannot recreate deleted history; operators must
restore a pre-compaction backup if they require it.

## Test matrix

Required before enabling retention:

- schema 0–7 migration and schema-7/schema-6 downgrade copies;
- injected rollback at each new table/version/commit boundary;
- lazy sequence seeding from legacy history;
- concurrent writers allocate distinct increasing IDs;
- deleting newest and deleting all retained events never reuses an ID;
- sequence integer exhaustion fails closed;
- bounded 256-event ledger backfill and restart continuation;
- new append during incomplete backfill is counted exactly once;
- stable item/byte accounting at exact boundaries;
- at most 64 originals removed per transaction;
- original plus revision state/audit and reaction state/audit disappear
  together;
- surviving replies lose only the expired reply reference;
- a reply selected in the same batch is removed;
- upload file and upload ledger survive history compaction;
- durable exact replay does not recreate a compacted event;
- injected failure after each dependency step rolls everything back;
- age, item, and byte selection independently;
- saturation rejects admission without partial compaction;
- disabled policy preserves indefinite current behavior;
- restart, history paging, inline/Resource snapshots, and mixed-version peers;
- isolated database copies only; no maintainer state.

## Implementation order

1. **Complete.** Schema 7 adds lazy persistent event sequence storage, guarded
   migration/fault coverage, and a separate confirmation-gated schema-6 copy
   export. Allocation now advances the high-water mark in the same immediate
   transaction as insertion. Concurrent writers remain monotonic; deletion of
   the newest or every retained event cannot reuse an ID; exhaustion fails
   closed. No history is deleted and retention remains disabled.
2. **Complete.** Schema 8 adds an empty per-room usage ledger without scanning
   history. New events update stable item/byte totals in their existing
   immediate transaction, while legacy rows advance by at most 256 per append
   or explicit maintenance call. The cursor and target survive restart; an
   append during incomplete backfill is counted exactly once; accounting
   overflow rolls back the event and sequence. A guarded schema-7 copy removes
   only usage metadata. No event is deleted and retention remains disabled.
3. Add the atomic bounded compaction primitive and fault tests.
4. Add disabled-by-default validated configuration and explicit maintenance
   status.
5. Integrate bounded compaction with event admission.
6. Run server/client restart, Resource, mixed-version, and live isolated smoke
   gates.
7. Reassess `message-revisions-v1` activation. Do not activate it merely
   because schemas 7 and 8 exist.

Each step must leave omenchatd independently buildable and reversible. No step
adds a polling worker, recurring timer, automatic network retry, or client
control.
