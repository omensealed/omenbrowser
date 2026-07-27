# OMENchat corrections and tombstones checkpoint

Status: dormant implementation checkpoint; capability and mutation actions remain inactive
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

This checkpoint does not authorize activation. Unit 6G now supplies persistent
event-ID sequences, bounded usage accounting, dependency-aware compaction, a
disabled-by-default policy, atomic admission integration, and explicit bounded
ledger maintenance. That closes the structural retention prerequisite.
The capability remains dormant until its live current/current plus
mixed-version gates pass.

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

1. **Complete.** Review and approve exact capability, operations,
   request/result/event/snapshot shapes, authorization, state/audit schema,
   retention, mixed-version behavior, and the dependency on room-history
   retention. No production code or schema changes.
2. **Complete and dormant.** The shared crate reserves operations 35–39 and
   owns bounded correction/tombstone request, acknowledgement, event, and
   explicit-target snapshot codecs. It enforces exact action/replacement
   agreement, nonzero identifiers, revision numbers 1–9, 256-target/entry
   snapshots, canonical ordering, a 256 KiB replacement ceiling, and bounded
   display metadata. Negotiation rejects `message-revisions-v1` without the
   durable base. A stable canonical hash vector covers room, target, action,
   and replacement; both independent frame codecs preserve the same byte-exact
   correction fixture. The production client does not request the capability
   and omenchatd deliberately omits it from acceptance.
3. **Complete and dormant.** Schema 6 adds constrained current-state and
   append-only revision-audit tables plus three lookup/retention indexes in the
   existing immediate migration transaction. Fault injection covers every
   table/index/version/commit boundary. Recovery validation recognizes the new
   objects, schema-4 exports remove both schema-5 and schema-6 layers, and the
   separate schema-5 downgrade-copy command removes only revisions while
   preserving reactions. The capability remains unrequested and unaccepted.
4. **Server foundation complete and dormant.** The transactional store and
   durable session executor enforce author/moderator/mute policy, immutable
   originals, eight corrections plus a tombstone, soft correction and hard
   total-state ceilings, bounded audit age/count/bytes/pruning, transactional
   reaction cleanup, exact replay/conflict behavior, codec-failure rollback,
   and authoritative inline/Resource snapshots. Revision IDs are allocated
   across retained audit and current state so pruning cannot cause reuse.
   Link-scoped binding records revision support only when the session response
   actually accepts it, capability-filtered live fan-out excludes legacy and
   stale-identity Links, and capable history responses are followed by an
   explicit-target snapshot. Exact replay returns its original acknowledgement
   without repeating fan-out, and identity replacement or Link close clears
   the binding. Normal negotiation still rejects the capability, so these live
   paths remain unreachable by production clients.
5. **Client foundation complete and dormant.** The desktop owns one bounded
   `ChatMessageRevision` projection per retained message target, an additive
   rebuildable SQLite cache, strict delta ordering/idempotency, authoritative
   explicit-target snapshot replacement, restart reconciliation, stable
   retained-byte accounting, and the reserved durable-intent operation kind.
   Inline and Resource snapshots share the existing bounded transport. Invalid
   snapshots retain prior rows but clear authoritative evidence. Production
   session-open frames still do not request the capability, unsolicited
   acceptance cannot activate it, and no correction/tombstone control is
   exposed. Dormant GUI controls remain a later sub-slice.
6. **Retention prerequisite complete.**
   `OMENCHAT_ROOM_RETENTION_CHECKPOINT.md` now records and tests the persistent
   event-ID high-water mark, bounded resumable usage ledger, atomic
   revision/reaction/reply cleanup, disabled-by-default policy, live admission
   integration, rollback boundary, deterministic restart/Resource evidence,
   and explicit stopped-server ledger maintenance.
7. **Read-only presentation complete and dormant.** The shared Iced timeline
   borrows only authoritative revision rows for retained visible targets.
   Corrections replace displayed text and add an `edited` marker while
   preserving message actions. Tombstones suppress original text,
   reply/mention/media/reaction actions, and add a `deleted` marker. Stale
   restart cache without authoritative evidence is not rendered as current.
   An explicit-target snapshot establishes positive or negative authority; a
   validated negotiated live delta establishes authority only for its target.
   An exact delta replay re-establishes stale authority once, while stale or
   conflicting deltas establish nothing. No control, capability request,
   timer, or retry is added.
8. **Bounded live sender complete and dormant.** A stored
   `RoomMessageRevision` intent can enter the existing item-bounded
   per-session pending mutation queue only when durable mutations and
   `message-revisions-v1` are both negotiated. The request hash, client
   instance, server, room, retained target, expiry, and typed body are
   revalidated before transmission. Typed acknowledgements must match the
   target, action, room, sequence, and authenticated local user. No optimistic
   projection change occurs. Recovery validation and redacted operation labels
   understand the operation, while ordinary composer drafts remain untouched.
   Production still has no prepare action and requests no capability.
9. **Desktop prepare/actions complete and dormant.** One correction draft per
   live session is bounded by the protocol replacement ceiling and remains
   separate from the ordinary composer. One deletion confirmation is owned
   globally and requires a second explicit action. Room/session changes cancel
   the relevant state. Controls require both negotiated capabilities,
   authoritative target evidence, a retained ordinary message, known local
   user/role state, and valid author/moderator/mute/revision-depth policy.
   Durable intent persistence completes before transmission, and successful
   correction dispatch clears only the matching correction draft. Production
   requests no revision capability, so these controls remain hidden. The view
   derives eligible targets once per bounded room render rather than rescanning
   room history for every message.
10. Run deterministic, mixed-version, retention, Resource, restart, fault, and
    live isolated smoke gates.
11. Request and accept `message-revisions-v1` only after every gate passes.

Each step must leave root and standalone server builds valid and independently
reversible. No step may introduce automatic retry.

## Checkpoint validation

On 2026-07-26 the shared protocol contract passed from the root, and focused
client/server codec and dormancy tests passed from their independent products:

```bash
cargo fmt --all -- --check
cargo test --locked -p omenchat-protocol
cargo test --locked --no-default-features --features desktop-product \
  message_revision --lib
cargo test --locked --no-default-features --features desktop-product \
  live_open_requests_supported_durable_extensions_with_persistent_client_identity --lib
(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    message_revision --lib
  cargo test --locked --no-default-features --features server-full \
    store::tests::every_message_revision_schema_fault_boundary_rolls_back_to_version_five -- --exact
  cargo test --locked --no-default-features --features server-full \
    database_recovery::tests
  cargo test --locked --no-default-features --features server-full
  cargo clippy --locked --no-default-features --features server-full \
    --all-targets -- -D warnings
)
cargo check --locked --no-default-features --features desktop-product
git diff --check
```

The shared crate now passes 34 tests, including six focused revision/negotiation
tests. The root fixture and server fixture/dormancy tests pass. The schema
migration, five rollback boundaries, recovery exports, and schema-5 downgrade
copy also pass isolated server tests. The schema/storage checkpoint full server
result was 416 passed and
9 ignored explicit soak/hardware/upstream cases; strict server Clippy and the
root desktop-product check pass. These results validate only the dormant
shared contract and storage foundation.

The first executor sub-slice subsequently passed:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-full \
    message_revision
  cargo test --locked --no-default-features --features server-full
  cargo clippy --locked --no-default-features --features server-full \
    --all-targets -- -D warnings
)
cargo check --locked --no-default-features --features desktop-product
```

The focused result is 14 passed. The full standalone result is 425 passed and
9 ignored explicit soak/hardware/upstream cases. This proves only the dormant
transactional executor and snapshot boundary; it is not Link-scoped fan-out,
client reducer, native package, mixed-version process, or live Reticulum
interoperability evidence.

The second executor sub-slice subsequently passed:

```bash
(
  cd src/server
  cargo test --locked --no-default-features --features server-full \
    message_revision -- --nocapture
  cargo test --locked --no-default-features --features server-full
  cargo clippy --locked --no-default-features --features server-full \
    --all-targets -- -D warnings
  cargo fmt --all --check
)
cargo check --locked --no-default-features --features desktop-product
cargo fmt --all --check
git diff --check
```

The focused result is 17 passed. The full standalone result is 428 passed and
9 ignored explicit soak/hardware/upstream cases. The tests prove dormant
Link-scoped acceptance state, same-room and identity-matched fan-out, exclusion
of base and stale-identity Links, history-following snapshots, replay without
repeat fan-out, and binding retirement. They inject the capable binding only
inside isolated tests; production negotiation remains disabled. These results
are not client reducer, native package, mixed-version process, or live
Reticulum interoperability evidence.

The first client sub-slice subsequently passed:

```bash
cargo test --locked --no-default-features --features desktop-dev \
  message_revision --lib -- --nocapture
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused result is 10 passed. It covers the immutable-original presentation
reducer, stable item/byte ceilings, additive SQLite restart recovery,
transactional room-capacity rollback, strict delta ordering and duplicate
suppression, authoritative snapshot replacement, malformed snapshot
fail-closed behavior, reserved durable-intent recovery, inline/Resource
decoding, and test-injected live gating. The current desktop-product library
result is 1,473 passed and 30 ignored explicit measurement/platform/live cases;
the full desktop-product target suite and strict Clippy also pass. The normal
session request still omits
`message-revisions-v1`, and even an unsolicited acceptance cannot activate the
client state. These results are not GUI controls, native package,
mixed-version-process, or live Reticulum interoperability evidence.

The dormant read-only presentation sub-slice subsequently passed ten focused
timeline tests under `desktop-product`. Corrected text retains reply/reaction
actions and carries its revision marker; a tombstone exposes neither original
text nor reply, mention, media, reaction, resend, or mutation actions. The live
view borrows only rows whose explicit target has authoritative snapshot
evidence, avoiding both stale-cache claims and revision-body clones during
redraw. Production negotiation and mutation actions remain disabled.

The following target-authority sub-slice makes negotiated live delta evidence
usable without broadening it into room-wide snapshot evidence. A valid new
delta marks only its retained target authoritative. An exact replay after
reconnect/stale marking emits one applied transition to restore presentation;
another exact replay is idempotent. A stale or conflicting delta cannot restore
authority, and untouched targets remain stale. Focused client and live-frame
reducer tests cover these transitions. Production negotiation and mutation
actions remain disabled.

The bounded sender sub-slice subsequently added no new queue or retry owner.
It reuses the existing pending mutation item limits and durable-intent
persistence state. Focused live tests prove both-capability gating, no
optimistic projection change, exact typed acknowledgement correlation, and
pending-state retention after a mismatched acknowledgement. Desktop recovery
validation rejects missing capability or malformed bodies, and recovered
operation labels do not expose correction text. At that transport-only stage,
there was no desktop prepare action or visible correction/tombstone control.

The desktop action sub-slice then added one bounded correction editor per
session and one explicit deletion confirmation without adding a worker, timer,
or retry path. Focused tests prove that controls remain hidden without
authoritative target evidence, author correction and author deletion are
distinct, muted authors cannot rewrite text, banned users receive no action,
moderators can delete but cannot rewrite another author's words, and ordinary
composer drafts remain unchanged. An isolated-root persistence test proves the
typed correction intent is durable in `Prepared` state before any transmission.
Another isolated-root test proves deletion creates no durable intent until the
explicit confirmation action.
At this sub-slice boundary production negotiation remained disabled, so no
release user could invoke these actions.

The pre-activation deterministic qualification then closed the explicit
capability-loss edge. Action-target derivation becomes empty as soon as
negotiation is absent, and a late matching acknowledgement cannot resolve the
pending durable intent until test-scoped capability restoration. Shared,
client, server, restart, Resource, retention, migration, recovery, and fault
filters pass. Adjacent peers retain byte-exact ordinary protocol-v1 behavior;
revision traffic is not sent because they cannot negotiate the new optional
capability. Exact commands, results, and the remaining live current/current
gate are recorded in
`docs/audits/omenchat-message-revisions-qualification.md`.

The subsequent reversible activation requests and accepts
`message-revisions-v1` only beside `durable-mutations-v1` and a persistent
client instance identifier. Unsolicited acceptance, base-only peers, downgrade,
and capability loss remain fail closed. Isolated revision-only, two-client, and
continuous replacement-Link smokes pass correction, deliberately lost
acknowledgement, exact replay, forced Resource recovery, tombstone, clean
intent recovery, and orderly omenchatd restart. No worker, queue, timer,
automatic retry, protocol number, schema, or retention default changed.

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

This checkpoint alone authorized no activation. The separately reviewed and
reversible activation is qualified in
`docs/audits/omenchat-message-revisions-qualification.md`.
