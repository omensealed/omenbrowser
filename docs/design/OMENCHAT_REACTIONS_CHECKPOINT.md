# OMENchat reactions checkpoint

Status: design checkpoint; no production capability, wire operation, schema,
storage, or UI behavior is activated by this document  
Baseline: OMENbrowser/omenchatd `0.9.6-3`, planned `0.9.6-4`  
Protocol baseline: `omenchat-v0.1`, numeric protocol version 1  
Required base capability: `durable-mutations-v1`

## Decision

Reactions will be an additive, explicitly negotiated protocol-v1 extension.
They will not be encoded as ordinary messages, commands, edits to target
messages, or untyped display strings.

The proposed capability name is:

```text
reactions-v1
```

It is valid only when `durable-mutations-v1` is requested and accepted on the
same authenticated Link. Capability state is Link-scoped and must be cleared
on retirement, identity change, reconnect, or downgrade.

The implementation must remain staged and dormant until the shared contract,
server transaction, client persistence, presentation, mixed-version fixtures,
and live isolated smoke all pass.

## Existing boundaries verified

The current repository provides the required foundation:

- `omenchat-protocol` owns protocol numbers, capability names, bounded wire
  types, canonical durable hashing, and compatibility fixtures without
  importing SQLite, Iced, Ratatui, Reticulum ownership, or filesystem policy.
- `DurableMutationEnvelope` already hashes the exact operation, room, and
  canonical body under a persistent random client instance and random 128-bit
  mutation identifier.
- omenchatd commits room mutations and the exact replay result in one immediate
  SQLite transaction and applies bounded replay retention.
- server schema 4 stores reply/mention metadata on ordinary room events.
- the client SQLite store is identity-scoped and uses additive, idempotent
  initialization for local presentation state.
- client room history is already bounded to 1,024 events / 8 MiB per session.
- legacy clients ignore unrequested extension frames at the network boundary,
  but their local SQLite decoder maps unknown room-event kinds to system text.
  Reactions therefore must not be stored as kind-6 rows in the existing client
  `room_events` table.

## Wire proposal

The exact numeric assignments must be reserved in `ChatOp`, fixture-tested, and
never reused:

| Operation | Proposed number | Direction | Purpose |
| --- | ---: | --- | --- |
| `RoomReaction` | 25 | client to server | Durable add/remove mutation |
| `ReactionAck` | 26 | server to origin | Exact retained semantic result |
| `ReactionEvent` | 27 | server to capable room peers | One committed state change |
| `ReactionSnapshotInline` | 28 | server to capable client | Bounded current state for a history page |
| `ReactionSnapshotResource` | 29 | server to capable client | Same bounded snapshot over the existing Resource boundary |

These numbers are currently unassigned; `MessageAck` is 24 and the existing
user-list family begins at 30.

### Request body

`RoomReaction` uses the frame room ID and this exact fields body:

```text
[
  "reaction-v1",
  target_event_id: u64,
  reaction_token: string,
  action: u64
]
```

`action` is `1` for add and `2` for remove. The target event ID must be nonzero.
No trailing fields, alternate integer types, or empty body are accepted.

The initial canonical token catalog is deliberately fixed and ASCII:

```text
thumbs_up
heart
laugh
surprised
sad
thumbs_down
celebrate
question
```

Wire tokens are case-sensitive and are never localized. The UI may render a
localized label or glyph beside the stable token. A fixed catalog avoids a
Unicode-normalization dependency, visually confusable arbitrary strings,
control characters, and unbounded reaction labels. The longest admitted token
is 11 bytes; the protocol constant will retain a 16-byte hard ceiling.

The canonical request hash covers `ChatOp::RoomReaction`, the room ID, target
event ID, token, and action. Reusing a mutation identity with a different
target, token, action, or room is a durable conflict.

### Acknowledgement

`ReactionAck` has this exact fields body:

```text
[
  target_event_id: u64,
  actor_user_id: u64,
  reaction_token: string,
  action: u64,
  changed: bool,
  reaction_event_id: u64 | nil
]
```

`changed = false` is the successful idempotent result for adding an already
active reaction or removing an absent reaction under a new mutation identity.
It has a nil reaction-event ID and produces no broadcast. An exact durable
replay returns the originally encoded acknowledgement byte-for-byte.

### Committed event

`ReactionEvent` uses the frame room ID and:

```text
[
  reaction_event_id: u64,
  target_event_id: u64,
  actor_user_id: u64,
  reaction_token: string,
  action: u64,
  at_unix: i64
]
```

It is emitted only after the database transaction commits and only to Links
that accepted `reactions-v1`. It is not a message, delivery receipt, unread
message, mention, or typing/presence signal.

### History snapshot

Every capable history page returns the active reaction rows for only the target
events represented by that page. An inline or Resource snapshot contains this
exact tagged body:

```text
[
  "reaction-snapshot-v1",
  [target_event_id, ...],
  [
    [target_event_id, actor_user_id, reaction_token, created_at_unix],
    ...
  ]
]
```

Rows are sorted by target event, token, then actor ID. A snapshot replaces the
client's reaction state only for the sorted, unique, explicitly listed
target-event set; every row must belong to that set. It
must not clear reactions for another page or room. Empty snapshots are
explicit so stale cached state can be removed.

The shared contract limits one snapshot body to 256 target events and 1,024
active rows. These are decoder bounds, not a promise that the inline frame can
carry the maximum encoded body; the existing byte/value limits still apply and
the server must select the Resource operation before the inline threshold.

Inline and Resource forms share one decoder and the existing compressed-batch,
decompression, item, byte, cancellation, and Link-ownership rules. The server
must select Resource transfer before an encoded snapshot exceeds the current
safe inline threshold. It must never truncate a snapshot and present it as
complete.

## Server semantics

A reaction mutation is admitted only when:

- the Link is identified and negotiated both required capabilities;
- the sender is not banned or muted and is currently joined to the room;
- the target exists in the same room, is not deleted, and is a message, rich
  message, action, notice, or upload;
- the target is not itself a reaction, moderation/system event, or a different
  room's event;
- the token and action have the exact canonical shape;
- existing command-rate admission and the reaction-state ceilings allow the
  change.

Users may remove only their own reaction row. Add and remove affect one
`(room, target event, actor, token)` tuple.

On a replay miss, validation, rate reservation, active-state change, append-only
reaction audit insertion, exact acknowledgement encoding, durable replay
publication, and retention pruning occur in one immediate transaction. A
result-encoding or replay-publication failure rolls the entire change back.
Only the first changed execution produces a one-use `ReactionEvent` effect.

Existing errors remain sufficient:

- malformed shape/token/action: `MalformedFrame`;
- missing, deleted, or ineligible target: `HistoryUnavailable`;
- banned/muted/not joined: the existing permission or membership errors;
- rate or retained-state saturation: `RateLimited`;
- durable negotiation, conflict, expiry, and busy outcomes: existing 1011–1015
  errors.

No error number is reserved by this checkpoint.

## Server schema proposal

omenchatd moves from schema 4 to schema 5 only after migration tests exist.
Ordinary `room_events` remains unchanged.

```sql
CREATE TABLE room_reactions(
  room_id INTEGER NOT NULL,
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  reaction_token TEXT NOT NULL CHECK(length(reaction_token) BETWEEN 1 AND 16),
  created_at INTEGER NOT NULL,
  PRIMARY KEY(room_id, target_event_id, actor_user_id, reaction_token)
);

CREATE INDEX idx_room_reactions_target
ON room_reactions(room_id, target_event_id, reaction_token, actor_user_id);

CREATE TABLE room_reaction_events(
  room_id INTEGER NOT NULL,
  reaction_event_id INTEGER NOT NULL CHECK(reaction_event_id > 0),
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  reaction_token TEXT NOT NULL CHECK(length(reaction_token) BETWEEN 1 AND 16),
  reaction_action INTEGER NOT NULL CHECK(reaction_action IN (1, 2)),
  at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  PRIMARY KEY(room_id, reaction_event_id)
);

CREATE INDEX idx_room_reaction_events_retention
ON room_reaction_events(at, room_id, reaction_event_id);
```

Application validation still enforces the fixed token catalog; SQLite length
checks are defense in depth, not the semantic validator.

### Server bounds

Initial conservative ceilings:

- three active tokens per actor/target;
- 128 active rows per target;
- 4,096 active rows and 128 KiB represented bytes per room;
- 65,536 active rows and 2 MiB represented bytes server-wide;
- 8,192 append-only audit rows / 512 KiB per room;
- 131,072 audit rows / 8 MiB server-wide;
- 90-day audit age;
- at most 64 audit rows pruned in one committed mutation.

An add fails closed at an active-state ceiling. Remove remains available so an
overloaded room can recover. Incremental audit pruning never removes active
state and never scans an unbounded result set. Counts and represented bytes are
queried through indexes in the mutation transaction; no unbounded in-memory
cache is introduced.

## Client model and storage proposal

The client adds a project-owned `ChatReaction` value containing server, room,
target event, actor user, token, and created time. It is separate from
`ChatEventKind`: reaction deltas do not become timeline messages and do not
increment unread or mention counters.

Identity-scoped `chat.sqlite` adds only:

```sql
CREATE TABLE room_reactions(
  server_id TEXT NOT NULL,
  room_id INTEGER NOT NULL,
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  reaction_token TEXT NOT NULL CHECK(length(reaction_token) BETWEEN 1 AND 16),
  created_at INTEGER NOT NULL,
  PRIMARY KEY(server_id, room_id, target_event_id, actor_user_id, reaction_token)
);
```

The table is additive and ignored safely by older clients. Deltas and snapshots
update it transactionally. State is retained only for currently cached target
events:

- at most three tokens per actor/target and 128 rows per target;
- at most 4,096 rows / 128 KiB per room;
- at most 8,192 rows / 512 KiB per server;
- at most 32,768 rows / 2 MiB per identity-scoped store.

When bounded message-history eviction removes a target from all retained
history ranges, its local reaction rows may be pruned incrementally. A new
history page snapshot restores authoritative current rows. Saturation rejects
the snapshot before changing the prior page state and surfaces a bounded
reconciliation error.

GUI and TUI derive per-token counts from this bounded shared model. The local
actor's active tokens are distinguished using the already negotiated
server-scoped numeric user ID. Counts are current only for pages whose snapshot
completed; otherwise the UI says `reaction state unavailable` rather than
showing a partial count as authoritative.

## Migration and rollback

Before server schema 5 is activated:

1. Copy and validate representative schema 0–4 databases.
2. Create the existing guarded pre-migration backup.
3. Add both reaction tables and indexes in the same migration transaction.
4. Set `user_version = 5` only at commit.
5. Inject failures before table creation, between tables, before index creation,
   before version update, and before commit.
6. Verify the active database stays at its old version and the generated backup
   remains restorable after every failure.

A stopped-server downgrade command must create a separate schema-4-compatible
copy from schema 5 by dropping only the two reaction tables/indexes, setting
`user_version = 4`, running `integrity_check`, and atomically publishing the
copy only after validation. Ordinary rooms, messages, uploads, users, durable
results, identities, and post-migration non-reaction history remain intact.
The command never edits the sole active database in place.

Disabling `reactions-v1` is the normal application rollback and preserves
reaction state for later reactivation. Restoring the pre-v5 backup is an
emergency rollback and can omit post-migration activity; documentation must
state that explicitly.

Client rollback needs no destructive migration. Older clients ignore the
additive table; operators may delete only the rebuildable local reaction table
while the application is stopped.

## Failure and crash boundaries

Tests must cover:

- server commits a reaction but the acknowledgement is lost;
- Link closes after commit and before origin delivery;
- explicit retry on a replacement Link;
- client restart with prepared and uncertain reaction intents;
- server restart after commit;
- exact duplicate and concurrent exact duplicate;
- one mutation identity reused with another target, token, action, or room;
- add already active and remove already absent;
- missing, deleted, cross-room, and ineligible targets;
- banned, muted, parted, and unauthenticated actors;
- active-state item/byte ceilings and remove-at-capacity recovery;
- audit age/count/byte pruning and its 64-row work ceiling;
- replay-result expiry and retired client instance;
- database busy, disk-full/write failure, result-encoding failure, and process
  termination at each transaction boundary;
- malformed, oversized, trailing, noncanonical, and unknown-token wire values;
- inline/Resource snapshot equality, cancellation, decompression bounds, and
  replacement-Link ownership;
- client snapshot saturation, stale snapshot, event-before-snapshot,
  duplicate event, and restart recovery;
- legacy client/current server and current client/legacy server;
- capability rejection/loss and no automatic retry;
- shutdown with prepared, uncertain, and in-flight work.

All filesystem and SQLite tests use explicit temporary roots. No fixture may
open the maintainer's real browser or omenchatd data.

## Mixed-version behavior

- A current client does not send `RoomReaction` until the server accepts
  `reactions-v1`.
- A current server does not send reaction events or snapshots to a Link that
  did not request and receive the capability.
- An older client and current server retain ordinary protocol-v1 room behavior.
- A current client and older server hide reaction controls and retain ordinary
  history.
- Capability loss blocks explicit retry of a recovered reaction intent; it is
  never converted to a message or command.
- Application versions never imply capability support.

## Implementation sequence

1. **Complete.** Shared protocol constants reserve operations 25–29; exact
   request, acknowledgement, event, and explicit-target snapshot codecs enforce
   the fixed token catalog and item bounds; the canonical durable hash vector
   covers room, target, token, and action; negotiation rejects
   `reactions-v1` without `durable-mutations-v1`; and both independent
   client/server codecs preserve the same byte-exact add fixture. The
   capability remains unrequested and unaccepted in production.
2. Add schema-5 migration, guarded downgrade-copy command, and fault tests with
   the capability still unadvertised.
3. Add dormant transactional server executor, bounds, replay, snapshots, and
   Link-scoped fan-out.
4. Add the separate bounded client model/table, snapshot reconciliation, and
   parser with no controls.
5. Add read-only GUI/TUI counts and current-user highlighting.
6. Add the durable composer action and recovered-intent presentation while the
   production capability remains disabled.
7. Run deterministic duplicate/restart/mixed-version/resource/retention gates
   and isolated measurements.
8. Activate explicit client request/server acceptance in a separate commit.
9. Run a real isolated two-client add/remove/restart/Resource smoke before
   release qualification.

Each unit is independently reversible and must leave both Cargo roots
buildable. No unit adds a worker, timer, polling subscription, dependency, or
automatic resend unless a later reviewed design proves one necessary.

## Completion gate

Do not advertise `reactions-v1` until:

- the exact wire fixture is byte-stable in both client and server;
- schema migration, downgrade copy, fault injection, and restart pass;
- active and audit state stay inside item, byte, age, and pruning-work bounds;
- exact replay returns the original result without another state change,
  audit event, rate charge, or fan-out;
- changed-content reuse conflicts;
- client snapshots and deltas reconcile without fabricated counts;
- GUI and TUI share the same bounded model and terminology;
- capability absence/rejection/loss keeps controls and retries disabled;
- current/current and adjacent mixed-version tests pass;
- isolated retention/resource measurements pass;
- no maintainer identity, message, database, or server root is touched.

This checkpoint authorizes no wire or database migration by itself.
