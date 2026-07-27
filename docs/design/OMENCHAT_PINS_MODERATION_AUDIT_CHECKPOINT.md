# OMENchat pins and moderation-audit checkpoint

Status: pin protocol, bounded storage/execution, desktop projection, durable
controls, deterministic qualification, reversible production negotiation, and
isolated current/current restart/replay qualification implemented; dormant
moderation-audit wire contract implemented; moderation-audit storage,
execution, negotiation, and presentation remain design-only

Baseline: OMENbrowser/omenchatd `0.9.6-3`, planned `0.9.6-4`

Protocol baseline: `omenchat-v0.1`, numeric protocol version 1

Database baseline: omenchatd schema 8

## Decision

Pins and client-visible moderation history must be separate additive features.
They have different authorization, mutation, retention, and privacy contracts:

- pin and unpin are room mutations and require durable identity and exact replay;
- moderation history is read-only to clients and records committed policy
  changes, not operator logs or claimed network side effects;
- neither feature changes protocol version 1 or ordinary legacy traffic;
- each remains disabled until its independent deterministic, mixed-version,
  migration, resource, and activation review passes. Pins have now passed that
  deterministic review and are enabled for the still-required live process
  gate; moderation-audit storage is implemented but capability negotiation,
  paging, and presentation remain dormant.

This checkpoint authorizes design and staged implementation only. It does not
authorize production capability negotiation.

## Current implementation verified

- `omenchat-protocol::ChatOp` uses 1–45 for existing session, room, reaction,
  user, revision, and history operations; 46–49 are unassigned; commands begin
  at 50; 52–59 are unassigned; contact operations begin at 60.
- `durable-mutations-v1` already provides persistent random client and mutation
  identifiers, canonical request hashing, exact replay, conflict detection,
  bounded result retention, explicit retry, and no automatic resend.
- Moderator and administrator roles are fixed bit flags and are rechecked by
  current command and message-revision executors.
- Durable moderation commands couple database changes and replay results.
  Kick/ban Link effects remain one-use post-commit effects and are not
  transactional network-delivery evidence.
- Server history uses immutable positive room event IDs, schema-8 high-water
  marks, item/byte accounting, and dependency-aware bounded compaction.
- Reaction and revision state/audit are separate projections removed
  transactionally when their original target is compacted.
- `001_init.sql` contains an `audit_log` table, but current server code does not
  write or query it. Its nullable payload shape has no byte, age, item, or
  authorization contract and is not suitable for a client-visible API.
- omenchatd's existing Audit TUI reads bounded administrative lines from the
  runtime log. That log includes configuration and interface administration
  and is operator-only; it must never be sent to OMENchat clients.
- Desktop OMENchat state already has bounded history, explicit target
  authority, role evidence, capability loss handling, durable intent storage,
  and inline/Resource snapshot patterns that can be reused narrowly.

Source evidence:

- `src/server/crates/omenchat-protocol/src/lib.rs`: `ChatOp`,
  `PROTOCOL_VERSION`, and the current operation ranges;
- `src/server/crates/omenchat-protocol/src/durable.rs`:
  `DurableMutationEnvelope` and `canonical_mutation_request_hash`;
- `src/server/src/session.rs`: `ROLE_MODERATOR`, `ROLE_ADMIN`, durable command
  execution, and one-use moderation effects;
- `src/server/src/store.rs`: `SCHEMA_VERSION`, schema-10 migration hooks, and
  current room-event/reaction/revision/pin storage;
- `src/server/src/store/history_retention.rs`: bounded dependency preflight and
  transactional target cleanup;
- `src/server/migrations/001_init.sql`: the unused legacy `audit_log` shape;
- `src/server/src/tui.rs`: operator-log filtering and administrative moderation
  paths;
- `src/chat/client.rs`, `src/chat/store.rs`, and `src/chat/live.rs`: bounded
  identity-scoped projections, authority, persistence, and negotiation state.

## Capability boundaries

Two capabilities are proposed:

```text
room-pins-v1
moderation-audit-v1
```

`room-pins-v1` is accepted only when all of these are true:

- `durable-mutations-v1` was requested and accepted;
- a valid persistent client instance identifier was supplied;
- the server implementation flag is enabled;
- the session is authenticated.

Capability acceptance does not grant permission. Every pin mutation rechecks
the actor's current room membership and current moderator/administrator role
inside the writer transaction.

`moderation-audit-v1` is read-only and does not depend on durable mutations.
It is accepted only for an authenticated session, but every request rechecks
current moderator/administrator role. Role loss immediately blocks further
pages and clears client authority; prior cached rows are not presented as
current.

Older, base-only, rejected, downgraded, or unsolicited peers retain ordinary
protocol-v1 behavior. A client must not send either feature's operations unless
its exact capability is active on the current Link.

## Proposed pin wire contract

Reserve the contiguous free range:

```text
46 RoomPin
47 PinAck
48 PinEvent
49 PinSnapshot
```

No Resource snapshot operation is proposed. A pin snapshot covers at most the
explicit targets in one already-bounded history page, with at most 64 pinned
entries. Its encoded form must remain below the existing frame ceiling at the
maximum target and entry counts. If byte-exact tests cannot prove that bound,
the design must stop and reserve a separately reviewed Resource operation; it
must not silently overload another Resource purpose.

Request body:

```text
["room-pin-v1", target_event_id, action]
action: 1 pin, 2 unpin
```

The request is wrapped in the existing durable envelope. Its canonical request
hash therefore covers protocol version, operation, room ID, target event ID,
and action.

Acknowledgement body:

```text
[target_event_id, action, actor_user_id, changed, pin_event_id_or_nil]
```

`changed` and `pin_event_id_or_nil` must agree. An exact no-op returns
`changed = false` and no event ID. Exact durable replay returns the original
acknowledgement and emits no second audit row, state mutation, rate charge,
notification, or fan-out.

Live event body:

```text
[pin_event_id, target_event_id, action, actor_user_id, at_unix]
```

Snapshot body:

```text
["room-pin-snapshot-v1", [explicit_target_event_ids...], [
  [target_event_id, pin_event_id, actor_user_id, pinned_at_unix]...
]]
```

Targets and entries are sorted and unique. An empty entry list still carries
the explicit target set so it cannot erase pin state for another history page.
Snapshots are authoritative only for those targets. A delta restores authority
only for its exact target after reconnect.

Pin eligibility:

- positive retained target in the same room;
- target is an immutable server event, never a transient local echo;
- actor is currently joined and currently moderator or administrator;
- target has not been removed by retention;
- room and target match the durable request exactly.

A tombstoned but retained message may remain pinned as an explicit deleted
placeholder. This preserves moderation evidence without revealing the original
body. Compaction of the original removes its pin state and pin audit rows in
the same transaction.

## Proposed moderation-audit wire contract

Reserve a separate read-only range:

```text
52 ModerationAuditBefore
53 ModerationAuditInline
54 ModerationAuditResource
55 ModerationAuditEnd
```

Operations 56–59 remain unassigned. The history request supplies an optional
exclusive cursor and a requested count clamped to the configured history batch
and a hard maximum of 256. Inline and Resource forms use the existing bounded
compression, decompression, pending-offer, purpose, and cancellation owners
with a distinct purpose such as `moderation-audit:<room-id>:<cursor>`.

The exact request body is:

```text
["moderation-audit-v1", exclusive_before_audit_id_or_nil, requested_count]
```

`requested_count` is 1–256. A nil cursor requests the newest page. Returned
records are encoded as newest-first arrays inside the existing compressed
inline/Resource batch envelope. `ModerationAuditEnd` has no record payload and
marks that an explicit page request reached the oldest retained boundary.

Each returned record contains only:

```text
audit_id
room_id
actor_user_id
actor_display_name_at_action
target_user_id_or_nil
target_display_name_at_action_or_nil
action
committed_at_unix
result_role_bits_or_nil
result_status_bits_or_nil
```

The fixed ten-field wire array uses that order exactly. Actor and target
display names are nonempty and limited to 256 UTF-8 bytes. Every currently
admitted action has a target; both target fields are therefore required for
the initial action vocabulary even though the schema keeps them nullable for
a separately reviewed future server-scoped action. Role results admit the
current standard/trusted/moderator/administrator values. Status
results admit only the current banned/muted bits and must agree with the
action's committed post-state. Records are strictly newest-first by unique
positive audit ID, limited to 256 rows and 512 KiB of retained owned data, and
must all match the frame's room.

The fixed action vocabulary initially covers the already-supported committed
room moderation effects:

```text
kick, ban, unban, mute, unmute, role-change
```

Topic changes, room creation/archive, server configuration, interface edits,
identity material, Reticulum endpoints, IP addresses, Link identifiers,
mutation identifiers, request hashes, tickets, tokens, and arbitrary log text
are excluded.

For kick, the record means the server committed and authorized the kick
operation. It must not claim that a particular Link closed or that a remote
client observed the effect. Post-commit one-use Link effects remain separate.

There is no automatic moderation-audit fan-out in the first version. An
authorized user requests bounded pages explicitly or as part of a deliberate
room refresh. This avoids a second event stream and prevents recurring polling.

## Proposed persistent schema

Use separate migrations so each boundary remains independently recoverable.

### Schema 9: pins

```sql
CREATE TABLE room_pins(
  room_id INTEGER NOT NULL,
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  pin_event_id INTEGER NOT NULL CHECK(pin_event_id > 0),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  pinned_at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0),
  PRIMARY KEY(room_id, target_event_id),
  UNIQUE(pin_event_id)
);

CREATE TABLE room_pin_events(
  pin_event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  room_id INTEGER NOT NULL,
  target_event_id INTEGER NOT NULL CHECK(target_event_id > 0),
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  pin_action INTEGER NOT NULL CHECK(pin_action IN (1, 2)),
  at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0)
);
```

Indexes must support explicit-target snapshots and bounded
age/room/ID retention. `AUTOINCREMENT` is intentional: pruning all pin audit
rows must not permit a committed pin event ID to be reused. Integer exhaustion
fails closed.

### Schema 10: moderation history

```sql
CREATE TABLE moderation_audit_events(
  audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
  room_id INTEGER NOT NULL,
  actor_user_id INTEGER NOT NULL CHECK(actor_user_id > 0),
  actor_display_name TEXT NOT NULL,
  target_user_id INTEGER,
  target_display_name TEXT,
  action_kind INTEGER NOT NULL,
  result_role_bits INTEGER,
  result_status_bits INTEGER,
  committed_at INTEGER NOT NULL,
  retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0)
);
```

Checks must enforce the fixed action/result combinations, positive target IDs
when present, bounded UTF-8 display-name bytes, and stable byte accounting.
Indexes must support `(room_id, audit_id)` paging and bounded age/room/global
retention.

Do not repurpose or expose the existing `audit_log` table. Leave it untouched
for compatibility until a separate migration proves whether any historical
installation populated it. Do not copy the operator runtime log into SQLite.

Schema 9 requires a confirmation-gated schema-8 copy that removes only pin
tables/indexes. Schema 10 requires both a schema-9 copy that removes only
moderation history and a schema-8 copy that removes both new feature families.
Copies use the existing stopped-server, no-WAL/no-SHM, private staged atomic
publication boundary and never modify or overwrite the active database.

The desktop cache is separately additive. A `room_pins` projection table may be
added to the existing identity-scoped `chat.sqlite` only through its current
bounded schema-initialization transaction. It must validate retained target
existence, roll back an over-capacity snapshot atomically, and remain safely
rebuildable. Moderation pages are ephemeral in the first version and require no
client database table.

## Bounds and retention

Initial hard ceilings proposed for qualification:

| State | Per room | Global | Age | Byte ceiling |
| --- | ---: | ---: | ---: | ---: |
| active pins | 64 | 4,096 | target lifetime | 1 MiB global |
| pin audit | 1,024 | 16,384 | 180 days | 4 MiB global |
| moderation audit | 2,048 | 8,192 | 365 days | 4 MiB global |
| client pin projection | 1,024/server | 4,096 | retained target lifetime | 1 MiB |
| client moderation page | 256/session | 1,024 | current Link authority | 512 KiB |

Every mutation prunes at most 64 expired/old audit rows. Capacity pressure may
replace only the oldest eligible audit rows; it must never evict active pin
state to admit another pin. If the active pin ceiling is full, a new pin fails
explicitly while an unpin remains possible. No startup scan, polling task,
recurring timer, or unbounded cleanup loop is allowed.

Display-name byte limits must reuse the existing protocol display-name limit.
Stable byte accounting includes a documented per-row overhead and all retained
UTF-8 fields. SQLite file size is not a substitute.

Room-history compaction must preflight and remove pin state and pin audit for
selected targets inside the existing immediate transaction and bounded
dependency-work ceiling. If the added dependency work exceeds the ceiling,
admission fails closed and rolls back the new event, all cleanup, and ledger
changes. Moderation audit is independent of target message retention and is
pruned only by its own bounds.

## Transaction and failure boundaries

Pin mutation and durable replay result must share one immediate transaction:

1. validate current actor, room membership, role, and target;
2. check exact replay or changed-content conflict;
3. prune one bounded audit batch;
4. enforce active/audit item and byte capacity;
5. append at most one pin audit event;
6. update or remove current pin state;
7. encode and store the exact acknowledgement;
8. commit;
9. only after commit, fan out one live event to same-room, identity-matched,
   capable Links other than the origin.

Codec failure, database busy/full, capacity rejection, transaction rollback,
Link close, or process crash before commit leaves no state/audit/replay result.
A crash after commit may lose the response/fan-out; exact explicit retry returns
the stored acknowledgement without repeating any effect.

Moderation audit insertion must occur in the same database transaction as the
corresponding user role/status mutation and durable replay result. Legacy
current-server moderation paths and local omenchatd administrative moderation
must use the same store boundary so the history is not misleadingly partial.
If a path cannot couple the mutation and record transactionally, that action
must remain absent from client-visible audit until it can.

Existing text audit logging remains best-effort operational evidence after the
database action. Its failure must not roll back a committed moderation
mutation, and its contents are never returned through the protocol.

## Client ownership and presentation

- Pin state is an additive identity-scoped cache/projection, not message
  history and not an optimistic rewrite.
- Restart may retain bounded pin rows but clears authority until an explicit
  negotiated snapshot or valid delta arrives.
- Capability loss, identity replacement, Link replacement, room transition,
  and target pruning clear corresponding authority and mutation controls.
- Pin/unpin actions appear only with current capability, current role,
  authoritative target evidence, and retained target eligibility.
- One pending pin mutation per target still consumes the existing per-session
  durable-mutation item budget; no new payload queue is added.
- Recovered pin intents remain visible and require explicit confirmation.
  They are never automatically transmitted.
- Moderation audit rows are read-only and ephemeral in the first version.
  They are cleared on capability or role loss and are never used to infer
  current user status.
- GUI and any future client TUI consume one project-owned bounded projection.
  omenchatd's administrative TUI remains a separate operator surface.

The presentation must distinguish:

- pinned current state;
- an accepted pin mutation awaiting its authoritative room event/snapshot;
- stale cached pin state awaiting reconciliation;
- moderation action committed;
- kick network effect not observed;
- page end versus transport/event-stream gap.

## Mixed-version behavior

- Current client/current server uses each feature only after explicit
  negotiation.
- Current client/older server receives no capability and sends no new
  operation.
- Older client/current server receives unchanged session acceptance and
  ordinary room/history traffic because it requested neither capability.
- Capability loss on reconnect disables actions and clears authority without
  deleting persistent current-server data.
- Exact existing v0.6.0-1 and v0.9.6-3 protocol fixtures remain byte-identical.
- A pin created by a capable peer is invisible to older peers; it does not
  alter or replace the original room event.
- Client-visible moderation history is optional evidence. Its absence never
  changes authorization or current user state.

## Implementation order

1. **Complete (2026-07-26):** add shared dormant pin types, operations 46–49,
   exact bounds, canonical hash fixtures, and independent codec agreement.
   Negotiation remains unchanged; the production client does not request
   `room-pins-v1`, and omenchatd does not accept it.
2. **Complete (2026-07-26):** add schema-9 pin state/audit, migration fault
   injection, recovery validation, bounded store operations, compaction
   dependencies, and schema-8 copy.
3. **Complete (2026-07-26):** add transactional durable server execution and
   Link-scoped dormant fan-out/snapshot plumbing. Production negotiation
   remains disabled.
4. **Complete (2026-07-26):** add bounded identity-scoped client projection,
   additive SQLite persistence, exact-target authority, restart-stale recovery,
   dormant delta/snapshot reducers, and read-only current/cached presentation.
   The desktop still does not request `room-pins-v1`; no mutation control is
   present.
5. **Complete (2026-07-27):** add durable pin/unpin controls behind test-only
   negotiated state. Intent persistence precedes transmission, one pending
   mutation is admitted per target within the existing mutation budget, exact
   acknowledgements remain distinct from authoritative pin state, and
   production negotiation remains disabled.
6. **Complete (2026-07-27):** qualify replay, restart, retention, maximum inline
   frame size, adjacent-version ordinary fixtures, per-room/global overload,
   bounded audit replacement, and isolated resource measurements. Deterministic
   evidence is recorded in
   `docs/audits/omenchat-pins-qualification.md`; production activation and its
   required current/current process smoke remain a separate review.
7. **Complete (2026-07-27):** enable client request and server acceptance
   together at the existing durable capability boundary. Unsolicited
   acceptance, pin-only requests, downgrade, capability loss, identity
   replacement, and Link retirement remain fail closed. No operation, schema,
   bound, queue, worker, timer, or retry changed.
8. **Complete (2026-07-27):** qualify one continuously running current client
   across orderly omenchatd restart and replacement Link. The isolated gate
   covers moderator setup, pin, withheld acknowledgement, exact replay,
   semantic no-op, authoritative snapshot reconciliation, unpin, and clean
   durable-intent completion on both Links. It also corrected and now guards
   the live server/client compressed-inline `PinSnapshot` encoding boundary.
9. **Complete (2026-07-27):** add shared dormant moderation-audit types,
   operations 52–55, fixed action/result vocabulary, cursor/page/display/byte
   bounds, and an independent byte-exact desktop/server request fixture.
   Production negotiation, storage, execution, Resource dispatch, and
   presentation remain unchanged; omenchatd explicitly refuses to accept the
   dormant capability.
10. **Complete (2026-07-27):** add empty-on-migration schema-10 constrained
    audit storage, bounded age/item/byte pruning, newest-first cursor reads,
    fault rollback, and confirmation-gated schema-9/schema-8 copies. Durable
    in-room kick/ban/mute/unmute/role/unban now commit their user mutation,
    audit row, and replay result in one immediate transaction. Legacy
    non-durable client commands, roomless role/unban, and local administrative
    paths remain absent because their current boundaries cannot provide that
    atomic guarantee.
11. **Complete (2026-07-27):** add authorized bounded inline/Resource paging
   and an ephemeral client projection behind test-only negotiated state.
   Authorization is rechecked against current moderator/admin role and room
   membership on every page. Capability state is bound to the authenticated
   Link identity and cleared on identity replacement or Link retirement.
   Desktop retention is capped at 1,024 records and 512 KiB and is cleared on
   capability loss. Production request and server acceptance remain disabled;
   there is no UI control, polling worker, timer, persistence, or automatic
   refresh.
12. **Deterministic portion complete (2026-07-27):** qualify role and
    membership loss, pagination, duplicate read stability, file-backed server
    restart, malformed/oversized input, delayed Resource replay, Resource
    metadata/size rejection, ordinary v0.9.6-3 fixture stability, dormant
    production negotiation, and privacy by construction. Invalid client pages
    clear the ephemeral projection. Current/current process restart,
    adjacent-binary live traffic, and cancellation during an active Reticulum
    Resource remain explicit pre-activation gates. Isolated bounded
    server-store and client-projection measurements passed on 2026-07-27.
13. Separately review activation only after the remaining process and
    measurement gates pass.

Each step is a separate commit-ready risk class. Pin activation does not imply
moderation-audit activation, and either feature may be deferred independently.

## Test matrix

Pins:

- exact request/ack/event/snapshot round trips and trailing-data rejection;
- canonical hash changes for operation, room, target, and action;
- explicit snapshot target replacement including empty results;
- author/trusted user denied; moderator/admin allowed;
- missing, cross-room, transient, and compacted targets;
- a new pin on an already tombstoned target is denied, while an existing pin
  becomes a pinned deleted placeholder until unpinned or compacted;
- exact no-op, exact duplicate, changed-content conflict, and concurrent
  duplicate;
- response/fan-out lost after commit;
- Link replacement, identity replacement, client restart, and server restart;
- capability absent, unsolicited, rejected, downgraded, and lost;
- active/audit item, byte, age, and bounded-pruning limits;
- database busy/full, result codec failure, migration fault, downgrade copy,
  recovery, and integer exhaustion;
- compaction removes target pin dependencies atomically;
- current/current two-client and adjacent-version ordinary traffic;
- no automatic retry after silence, disconnect, or restart.

Moderation audit:

- every admitted action/result shape and every forbidden field;
- current moderator/admin authorization and immediate role-loss denial;
- legacy, durable, and local administrative mutation coverage;
- database mutation and audit insertion fault rollback;
- kick record does not claim Link close;
- exact durable replay creates no second audit row;
- per-room/global item, byte, age, and bounded-pruning limits;
- cursor ordering, page boundary, end marker, inline/Resource equality;
- malformed, oversized, decompression, deferred-offer, and cancellation paths;
- schema-10 migration faults, recovery, schema-9/schema-8 copies;
- no identity hashes, Reticulum endpoints, Link IDs, tokens, mutation IDs,
  request hashes, arbitrary payload, or operator-log text in output;
- current/current, current/older, and older/current behavior;
- no polling, automatic refresh, or recurring network traffic.

Release validation still requires root/server formatting, checks, tests, strict
Clippy, protocol conformance, release quick gate, isolated live smoke, native
platform CI, bundled interoperability, resource measurements, and package
lifecycle evidence.

## Rollback

Before activation, rollback removes dormant callers/types and uses the
appropriate guarded downgrade copy. After activation, first disable capability
request/acceptance while retaining all schema-9/schema-10 data for operator
reconciliation. Do not delete unresolved pin intents or durable replay rows.

Binary rollback to `v0.9.6-3` requires a confirmed schema-8 copy. Pin and
moderation-audit state are additive and cannot be represented by that release;
the original immutable room history, identities, rooms, users, uploads,
reactions, revisions, durable replay rows, and schema-8 usage/sequence metadata
must remain preserved in the copy.

## Completion gate

Do not advertise either capability until:

- independent codecs and exact fixtures agree;
- every persistent row has item, byte, age, and work bounds;
- migrations, recovery, downgrade copies, and injected faults pass;
- current roles are enforced transactionally on every request;
- exact replay returns the original result without repeated state, audit,
  rate, notification, disconnect, or fan-out;
- room compaction cannot orphan or resurrect pin state;
- client authority is explicit and capability loss fails closed;
- mixed-version ordinary traffic remains unchanged;
- privacy tests prove operator logs and secret/identity/network material cannot
  enter client-visible audit;
- isolated current/current process and release resource gates pass.
