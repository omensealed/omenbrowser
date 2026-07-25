# OMENchat replies and mentions checkpoint

Status: implementation in progress; server/client storage dormant, no production wire activation
Baseline: OMENbrowser/omenchatd v0.9.6-3, planned v0.9.6-4  
Protocol baseline: version `1`, name `omenchat-v0.1`  
Proposed additive capability: `reply-mentions-v1`

## Current implementation evidence

The current checkout already has the prerequisites that Unit 6A must reuse:

- `src/server/crates/omenchat-protocol/` is the shared, independently
  relocatable wire-contract crate.
- `durable-mutations-v1` provides persistent client/mutation identifiers,
  canonical request hashing, exact replay results, transactional room-event
  insertion, bounded replay retention, and explicit conflict/expiry results.
- `RoomMessage` is an activated durable mutation when negotiated. Legacy peers
  retain the existing text body and cautious no-automatic-retry behavior.
- Room event IDs are monotonically allocated per room inside the same immediate
  SQLite transaction that inserts the event.
- Client and server history are bounded, and both preserve immutable
  `(room_id, event_id)` identifiers.
- Protocol-v1 room-event arrays use fields 0 through 5 for ordinary text
  events. Existing clients ignore trailing fields.
- A legacy server's text extraction takes the first string from a fields body.
  This permits a fail-soft rich request shape whose first field remains the
  ordinary message body, but capability negotiation is still mandatory.

The client event model still has no reply reference, mention identifiers,
local-user identifier, mention counter, or mute-except-mentions setting. The
client SQLite store has no schema version and applies idempotent `ALTER TABLE`
additions. omenchatd is now at schema version 4 and creates a guarded,
owner-only SQLite backup before migration.

## Compatibility invariants

- Do not change protocol version 1, `omenchat-v0.1`, existing operation
  numbers, destination/aspect names, Link context, or legacy frame shapes.
- `reply-mentions-v1` activates only when explicitly requested and accepted on
  the current authenticated Link.
- The capability additionally requires `durable-mutations-v1`; a rich message
  must never bypass the persistent intent/replay boundary.
- An old client talking to a new server receives ordinary protocol-v1 message
  events and may ignore appended fields without losing message text.
- A new client talking to an old or downgraded server does not expose an active
  Reply action and never silently strips a reply reference.
- A plain message remains a plain `RoomMessage` text body even on a capable
  Link. The extension is used only when reply or explicit mention metadata is
  present.
- Display names are not mention identities. Mention metadata contains bounded
  server-assigned numeric user IDs.
- Queue admission, Link send, and event fan-out are not final acknowledgement;
  the existing durable result remains the mutation authority.

## Proposed negotiation

Add `reply-mentions-v1` to the existing bounded capability list. A client may
request it only alongside `durable-mutations-v1` and an available persistent
client instance. omenchatd accepts it only when the base durable capability was
accepted.

To identify whether an inbound event mentions the authenticated local user,
append the server-assigned local user ID to a capable `JoinAccept`:

```text
JoinAccept Fields:
  0 room summary                 existing
  1 local user id                U64, only when reply-mentions-v1 is accepted
```

Legacy clients consume field 0 and ignore field 1. The user ID is scoped to
the authenticated server identity and is not a trust claim outside that
server.

## Proposed rich message request

Keep `ChatOp::RoomMessage`. The legacy body inside the durable envelope becomes:

```text
FrameBody::Fields [
  String(message_body),          # first string preserves fail-soft legacy text
  String("reply-mentions-v1"),   # exact shape tag
  Array[U64(room_id), U64(event_id)] | Nil,
  Array<U64>(mentioned_user_ids)
]
```

Dedicated bounds:

- body: existing configured message-byte limit;
- reply reference: zero or one current-room `(RoomId, EventId)` pair with
  nonzero values;
- mentions: at most 16 user IDs;
- mention IDs: strictly increasing and unique;
- body/container/depth/total bytes: existing frame and durable canonical
  limits.

The client sorts and deduplicates mention IDs before persisting the outbound
intent. The server nevertheless rejects non-canonical, duplicate, zero,
oversized, wrong-type, or trailing request fields. The complete rich body is
covered by the existing durable canonical request hash, so reusing a mutation
ID with different reply or mention metadata is a conflict.

The server validates in the durable transaction that:

- the referenced room ID exactly equals the request frame's room ID;
- the referenced event exists in that room and is not deleted;
- every mentioned user exists and is currently a room member;
- the sender remains joined, allowed, and within existing message limits.

Missing, pruned, deleted, and cross-room references return a typed error and do
not insert an event or replay-success result. Unit 6A should reserve dedicated
protocol-v1 error codes only after confirming that the existing generic
`MalformedFrame`, `UserNotFound`, and `HistoryUnavailable` codes cannot express
the UI distinction safely.

## Proposed event extension

Keep `ChatOp::RoomEvent` and ordinary message kind `1`. Append metadata to the
existing event value:

```text
RoomEvent Array:
  0 event id                     existing
  1 kind                         existing
  2 actor user id                existing
  3 timestamp                    existing
  4 body                         existing
  5 actor display name           existing
  6 reply event id or Nil        new
  7 Array<U64>(mentioned users)  new
```

The server emits these trailing fields only for a rich message. New clients
accept both six-field legacy and eight-field rich values. Legacy clients retain
the body and actor because the first six fields are unchanged.

History, live fan-out, durable acknowledgement reconciliation, and resource
batches must all use the same encoder/parser. Metadata must survive a server
restart, client restart, history-resource transfer, event-gap reconciliation,
and an exact durable replay without producing a second event.

## Proposed data models and storage

Project models should represent metadata separately from the event body:

```text
ChatMessageMetadata {
  reply_to_event_id: Option<EventId>,
  mentioned_user_ids: bounded sorted user IDs
}
```

Only message events may carry it. Action, notice, system, and upload events
reject or discard extension metadata according to the parsing boundary; they
must never be reinterpreted as replies.

omenchatd schema version 4 adds nullable/bounded metadata without rewriting
existing payloads:

```sql
ALTER TABLE room_events ADD COLUMN reply_to_event_id INTEGER;
ALTER TABLE room_events ADD COLUMN mention_user_ids BLOB;
CREATE INDEX idx_room_events_reply
ON room_events(room_id, reply_to_event_id)
WHERE reply_to_event_id IS NOT NULL;
```

`mention_user_ids` is an exact big-endian sequence of `u32` values, limited to
64 bytes for 16 IDs. It is decoded only after checking blob length, divisibility
by four, count, strict ordering, and nonzero IDs. Existing event kind and
payload remain unchanged, so the pre-v4 backup retains fully readable legacy
history.

The browser's identity-scoped `chat.sqlite` adds the same two nullable columns
to `room_events`. No index is required for Unit 6A because reply preview lookup
uses the existing `(server_id, room_id, event_id)` primary key and mentions are
evaluated while bounded events are admitted. If later local search requires a
mention index, it belongs to Unit 6C and needs separate measurements.

The server migration must:

1. create and synchronize the existing no-clobber pre-v4 SQLite backup;
2. add columns/index and set `user_version = 4` in one immediate transaction;
3. preserve all schema-v3 rows byte-for-byte in their existing columns;
4. pass integrity, injected failure, backup collision, future-version, and
   guarded restore tests.

The client migration must be idempotent, preserve existing events, and fail
visibly without deleting or recreating `chat.sqlite`.

## Retention and rendering

- Reply metadata has no separate history: it lives and expires with its room
  event.
- Reply previews contain at most the existing compact timeline preview limit;
  no referenced body is copied into the database or wire event.
- Preview lookup is restricted to the same retained room history. A pruned
  local original renders `Original message unavailable` without fetching or
  retrying.
- Jump-to-original operates only on an already retained event. It does not
  trigger unbounded history loading.
- Mention IDs are capped at 16 and add at most 64 stored bytes per rich event.
- Mention highlighting is render-only and adds no animation or timer.
- Mention counts saturate and derive from bounded retained events; no second
  unbounded notification history is introduced.

## Mention and mute behavior

Unit 6A must first persist the authenticated local server user ID learned from
capable `JoinAccept`. An event is a local mention only when its validated
mention list contains that exact ID.

`mute-except-mentions` is per saved server/room, defaults to off, and affects
only unread/notification presentation. It does not suppress storage,
reconciliation, moderation events, or history. The setting needs an explicit
bounded persistence field and migration test; it must not be inferred from
display-name text.

Composer mention resolution may offer current room members, but sending uses
the selected numeric IDs. Plain `@text` without selected IDs remains ordinary
message text and does not create authoritative mention evidence.

## Failure and crash boundaries

- Before outbound intent commit: no frame is sent.
- After intent commit and before send: the rich body remains prepared.
- After send and before result: the same reply/mention metadata remains part of
  the uncertain canonical request and is never reconstructed from current UI.
- Server validation failure: no event or success replay result commits.
- Server crash before commit: neither event nor replay result exists.
- Server crash after commit: exact retry returns the stored acknowledgement;
  history reconciliation supplies the single rich event.
- Client crash after acknowledgement: history restores the metadata; replay
  cannot duplicate it.
- Missing local reply target: body and metadata remain stored, preview is
  unavailable, and no network fetch starts.
- Capability loss after reconnect: an uncertain rich mutation remains
  recoverable but cannot be retried until the capability is negotiated again.
- Client/server schema migration failure: prior databases and migration backups
  remain usable; identities are never regenerated.

## Mixed-version matrix

| Client | Server | Required behavior |
|---|---|---|
| old | old | unchanged protocol-v1 messages |
| new | old | no negotiated Reply UI; ordinary messages unchanged |
| old | new | ordinary messages; trailing rich event fields ignored safely |
| new | new, capability rejected | ordinary messages; rich send unavailable |
| new | new, capability accepted | durable rich request and metadata |
| new capable reconnect, capability lost | no automatic resend; explicit blocker |

Mixed tests must include the v0.9.6-3 fixture/binary boundary and must prove
that neither application version nor descriptor metadata activates the
extension.

## Test matrix

Shared protocol crate:

- capability list acceptance/rejection and dependency on durable mutations;
- exact rich request and event vectors;
- canonical hash changes for body, reply, and mention changes;
- exact limits and next-item/next-byte rejection;
- malformed types, duplicate/unsorted IDs, trailing fields, and depth limits;
- legacy six-field event parsing and old fixture byte stability.

omenchatd:

- exact same-room reference commit;
- missing, deleted, pruned, and cross-room rejection;
- mentioned user missing/not-member rejection;
- one transactional event plus replay result;
- lost response, replacement Link, restart, exact replay, and conflict;
- concurrent duplicate execution and rate-reservation rollback;
- schema-v3 preservation, pre-v4 backup, injected migration failure, restore,
  and future-version refusal;
- history inline/resource and fan-out preserve identical metadata;
- old client event compatibility.

OMENbrowser:

- capability absent/rejected/accepted UI states;
- persist-before-send and canonical intent recovery;
- local user ID binding;
- bounded reply preview, missing preview, and retained jump;
- mention highlighting/count and mute-except-mentions;
- reconnect, event gap, history resource, and restart recovery;
- old server sends remain byte-identical;
- malformed/oversized rich events do not mutate state;
- no automatic retry after uncertain send or capability loss.

## Approval and implementation order

This checkpoint deliberately makes no wire, schema, configuration, or runtime
change. After review, implement as separate rollback units:

1. inert shared protocol capability, bounded rich-body/event helpers, and
   compatibility vectors;
2. server schema-v4 migration and typed metadata persistence, still not
   advertised;
3. server negotiated validation/transaction/fan-out;
4. client model/store parsing and capability state;
5. composer Reply action, bounded preview/jump, mention selection/highlighting,
   count, and mute-except-mentions;
6. mixed-version, crash, resource, and measurement gates.

Do not advertise `reply-mentions-v1` until all six units pass. Do not combine
schema activation and user-facing controls into one patch.

## Implementation progress

Unit 1 is implemented as an inert shared-contract foundation:

- `omenchat-protocol` owns the capability/tag constants, typed reply reference,
  typed rich request, typed event metadata, and content-free validation errors.
- Requests require the exact four-field shape. Events accept only the legacy
  six-field or rich eight-field message shape.
- Bodies are nonempty and capped at 512 KiB. Reply room/event identifiers are
  nonzero. Mention identifiers are nonzero, strictly increasing, unique, and
  capped at 16.
- Negotiation validation rejects `reply-mentions-v1` unless
  `durable-mutations-v1` is present in the same capability set.
- The client and standalone server codecs share one exact locked MessagePack
  vector. Existing v0.6 fixture tests remain unchanged.
- Canonical durable-request tests prove that changing a reply or mention set
  changes the request hash.

The live client does not request the capability and omenchatd does not accept
or advertise it. No runtime branch, configuration field, or UI control was
added.

Unit 2 is implemented as a dormant omenchatd storage boundary:

- schema version 4 adds nullable `reply_to_event_id` and bounded
  `mention_user_ids` columns plus the partial same-room reply index;
- version-3 migration uses the existing immediate transaction, owner-only
  no-clobber backup, future-version refusal, and staged restore machinery;
- legacy rows preserve their existing values and decode with no metadata;
- typed metadata uses exact big-endian `u32` mention IDs and rejects empty,
  oversized, misaligned, zero, duplicate, or unsorted stored values;
- only message events may carry the typed metadata;
- ordinary live event insertion continues storing both new columns as `NULL`;
- preservation, pre-v4 backup, injected rollback, malformed-storage, exact
  round-trip, and non-message refusal tests cover the boundary.

Unit 3 is implemented behind the still-false server activation gate:

- capability dependency and Link-scoped binding plumbing are complete, but
  production negotiation deliberately omits `reply-mentions-v1`;
- a rich request is accepted only through a durable authenticated binding with
  the dormant flag active; exact tagged shape and canonical hash remain
  mandatory;
- the sender must already be joined, replies must reference a non-deleted
  event in the same room, and every numeric mention must be a current member;
- validation, metadata insertion, exact origin acknowledgement, and durable
  replay publication share one immediate transaction;
- validation failures are retained as exact durable results, so a later policy
  or membership change cannot reinterpret the same mutation;

Unit 4 is implemented as a dormant client model and persistence boundary:

- `ChatMessageMetadata` is separate from the message body, and rich messages
  remain distinguishable from legacy message events without changing their
  displayed text;
- the client accepts established five/six-field legacy events and only treats
  the exact eight-field extension as rich; partial, noncanonical, and
  oversized extension metadata is rejected before it mutates session state;
- identity-scoped `chat.sqlite` adds nullable `reply_to_event_id` and
  `mention_user_ids` columns idempotently, propagates migration errors, and
  retains existing event rows unchanged;
- metadata uses the same bounded big-endian `u32` representation and survives
  client-store close/reopen; malformed stored metadata fails visibly instead
  of being reinterpreted as plain text;
- live fan-out, inline history, resource history, event-gap reconciliation,
  bounded history accounting, GUI display, and TUI display share the enriched
  project-owned event model;
- dormant Link state records reply capability acceptance and the authenticated
  local user ID only after an explicit request/accept sequence, and clears both
  on downgrade or Link retirement.

The production client still does not request `reply-mentions-v1`, the server
activation constant remains false, and there is no Reply or mention composer
control yet.

Unit 5A implements read-only presentation without activating mutation:

- one project-owned helper derives reply and local-mention presentation from
  the already bounded retained room history;
- reply previews are capped at 160 UTF-8 bytes and never copy into storage;
- a retained same-server/same-room original exposes a jump target, while a
  missing or pruned original renders `Original message unavailable` and starts
  no fetch, retry, timer, or history expansion;
- jump targeting updates the existing room-specific bottom-anchored scroll
  state only when the target remains in the active room;
- `mentioned you` is shown only when validated numeric metadata contains the
  authenticated local user ID learned on the negotiated Link; display-name
  text remains non-authoritative;
- machine-readable OMENchat smoke/TUI output includes the reply event ID and
  bounded numeric mention IDs for rich events.

The Reply composer action, member mention picker, mention count,
mute-except-mentions setting, capability request, and server activation remain
deferred to later Unit 5 slices.
- the single room-event encoder preserves metadata across live fan-out,
  bounded inline history, and resource history;
- exact replay after restart returns the stored acknowledgement without a
  second event or broadcast, while changed metadata conflicts;
- a capable dormant Link receives its server-scoped local user ID in
  `JoinAccept`; legacy production responses remain unchanged.

Missing, deleted/pruned, cross-room, non-member, non-negotiated, restart,
resource-history, conflict, and local-user binding tests cover the boundary.
No new error numbers were needed: the existing typed protocol-v1 errors retain
their documented meanings.

Unit 4—client model/store parsing and capability state—remains the next rollback
boundary. Capability advertisement stays blocked.
