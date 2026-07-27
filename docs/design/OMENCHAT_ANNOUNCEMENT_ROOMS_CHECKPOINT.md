# OMENchat announcement-room checkpoint

Date: 2026-07-27

Status: dormant shared wire constants/codecs and schema-11 recovery storage
implemented; no configuration, authorization, client projection, GUI, TUI, or
production negotiation is activated

Baseline: `release/v0.9.6-4` at `1097a10`

Protocol baseline: `omenchat-v0.1`, numeric protocol version 1

Database baseline: omenchatd schema 10

## Decision

Read-only announcement rooms should be an additive per-room policy, not a new
room type or protocol version. Existing rooms remain ordinary by default.
Members may discover, join, leave, and read an announcement room, but only a
current moderator or administrator may publish or mutate room content.

This checkpoint does not authorize implementation. It records the compatibility
and transaction boundaries that must be satisfied before schema 11 or a live
capability is introduced.

## Current implementation verified

- `ServerRoom` stores `room_id`, `name`, optional `topic`, and
  `room_revision`; the `rooms` table also owns `archived`.
- Room catalog values are four-element arrays:
  `[room_id, name, topic-or-nil, room_revision]`.
- The current desktop parser consumes the identifier, name, and topic and
  tolerates later array elements. Unnegotiated server output must nevertheless
  remain byte-identical to preserve adjacent-version fixtures.
- Room messages, actions, reactions, corrections, tombstones, uploads, topic
  changes, notices, pins, and moderation commands have separate authorization
  and durable/non-durable paths. A policy cannot be safely enforced in only the
  composer or one command dispatcher.
- Durable mutations already execute through an immediate SQLite transaction.
  The authoritative policy check must occur inside the same transaction as the
  mutation and retained replay result.
- Join, part, room listing, history paging, user lists, and moderation-audit
  paging are not content publication and remain available subject to their
  existing identity, membership, role, and capability rules.
- Room history retention, event sequence ownership, upload quotas, and
  moderation audit are independent contracts. Announcement policy must not
  silently alter any of them.

## Proposed wire contract

Capability name:

```text
announcement-rooms-v1
```

The capability is independent of `durable-mutations-v1`. A client requests it
only to receive typed room-policy evidence; the server enforces policy for all
clients regardless of negotiation.

When and only when both peers negotiate the capability, append one unsigned
policy-bit field to room catalog and `RoomDelta` values:

```text
[room_id, name, topic-or-nil, room_revision, policy_bits]
```

Initial fixed bit:

```text
0x01  announcement/read-only-for-members
```

Rules:

- zero means the existing ordinary room behavior;
- values outside the known mask fail closed for a negotiated v1 decoder;
- unnegotiated peers receive the existing four-field value exactly;
- capability loss clears typed policy evidence and disables policy-dependent
  client controls until a fresh authoritative room catalog arrives;
- `room_revision` increments whenever policy changes;
- no operation number, protocol version, destination aspect, or ordinary frame
  changes.

No policy is inferred from a room name, topic, server display name, or error
string.

## Proposed storage contract

Schema 11 adds one constrained column:

```sql
ALTER TABLE rooms
ADD COLUMN policy_bits INTEGER NOT NULL DEFAULT 0
CHECK(policy_bits >= 0 AND policy_bits <= 1);
```

Migration properties:

- existing rows become ordinary rooms without a data scan or rewrite;
- a failed migration rolls back the column/version step and retains the
  pre-v11 recovery backup;
- a confirmation-gated schema-10 copy export removes only the new policy
  column while preserving rooms, history, identities, replay results,
  reactions, revisions, pins, moderation audit, event sequences, and usage
  ledgers;
- old binaries must not open the migrated live database directly; rollback uses
  the validated exported copy;
- changing policy and incrementing `room_revision` occur in one immediate
  transaction;
- no per-message policy snapshot or new unbounded audit table is introduced.

The lobby may be configured as an announcement room, but it retains the
existing prohibition against archival.

## Authorization invariant

For an announcement room, current moderators and administrators may perform
the same operations they can perform in an ordinary room. Standard and trusted
members may read but must receive a typed policy rejection before any of these
content mutations commit:

- room message or action;
- room notice;
- reaction add/remove;
- message correction or tombstone;
- upload offer or publication;
- any future member-authored room-content mutation.

Pins and moderation commands retain their stricter existing role checks.
Topic, room-policy, and archive changes retain moderator/administrator or
administrator rules as explicitly defined by their administrative command.
Join, part, room/user listing, history, and read-only audit paging are not
blocked by announcement policy.

Every legacy and durable server mutation boundary must call one project-owned
policy predicate. Durable paths must re-read the current room policy inside the
mutation transaction. A replay returns its original retained result even if
policy changed later; it must not execute or fan out again. A new mutation
attempt uses current policy.

The client may hide or disable invalid controls only after authoritative policy
evidence. Server enforcement is mandatory and cannot depend on UI state or
capability negotiation.

## Error and client evidence

Use a stable typed error code if the existing protocol error vocabulary has an
appropriate policy-rejection value; otherwise reserve one additive code in a
separate wire slice. Human text is explanatory only and must not be parsed.

GUI/TUI presentation should say `Announcements` or `Read-only for members`,
identify that moderators can publish, and preserve the draft after rejection.
It must not describe local queue admission as acceptance or automatically retry
the rejected mutation.

Older clients remain safe: they may show a composer because they lack policy
evidence, but the current server rejects publication. Older servers ignore the
unrequested capability and expose ordinary-room semantics; a current client
must not fabricate read-only state.

## Administrative surface

The first implementation should use an explicit stopped-server administrative
command and the existing single-owner database worker:

```text
room-policy <room_id> ordinary|announcement
```

Requirements:

- positive existing room identifier;
- explicit validated vocabulary, never arbitrary numeric bits;
- current-schema database only;
- one atomic update plus revision increment;
- no config-file secret or command-line secret;
- human and machine-readable status show the effective policy;
- the running server receives policy changes only through an owned reload or
  restart path until safe live mutation/fan-out is separately proven.

Do not combine this slice with slow mode, retention changes, upload/media
limits, or a general room-policy editor.

## Bounds and resource impact

- One constrained integer per server room and one optional integer per
  negotiated room catalog entry.
- Existing room catalog item/byte ceilings remain authoritative and must include
  the added field in retained-size tests.
- No queue, cache, worker, timer, retry loop, background scan, or recurring
  network traffic is added.
- Policy checks are indexed room lookups inside already-owned mutation
  transactions; measurement must confirm no material database-latency
  regression.

## Migration and compatibility tests

Before activation, require:

1. Schema-10 to schema-11 migration with ordinary defaults and every injected
   fault boundary.
2. Confirmation-gated schema-10 export and validated restore.
3. Ordinary four-field room fixtures remain byte-exact when unnegotiated.
4. Negotiated five-field catalog/delta round-trips, bounds, unknown-bit
   rejection, downgrade, and capability-loss clearing.
5. Every standard/trusted content mutation is rejected without event, upload,
   replay, rate-limit, policy, or fan-out side effects.
6. Moderator/admin publication preserves existing behavior.
7. Durable exact replay returns the original result after later policy or role
   changes; changed mutation identity/content follows existing conflict rules.
8. Concurrent policy change and mutation serialize deterministically through
   SQLite ownership.
9. File-backed restart preserves policy and monotonically increasing
   `room_revision`.
10. Current/current process traffic covers ordinary and announcement rooms,
    rejection evidence, moderator publication, restart, and replacement Link.
11. Adjacent-version traffic proves old-client/current-server rejection safety
    and current-client/old-server ordinary fallback.
12. GUI/TUI controls clear on downgrade and never silently resend a rejected
    mutation.

## Failure and crash boundaries

- Crash before commit leaves the old policy and revision.
- Crash after commit exposes the new policy and incremented revision together.
- A server response lost after a durable rejected or accepted mutation is
  recovered only through existing explicit durable replay.
- A stale catalog may affect control presentation but never server
  authorization.
- Unknown policy bits, malformed catalog fields, future database versions, and
  noncanonical administrative values fail closed.

## Rollback

Before activation, remove the dormant capability/types/tests and retain schema
10. After schema 11 exists, stop omenchatd, export the confirmation-gated
schema-10 copy, validate it, preserve the schema-11 database as a backup, and
replace through the existing recovery workflow. No identity, history, or
protocol-version migration is required.

## Completion gate

Implementation may begin only as separate slices:

1. shared dormant policy constants and negotiated room-value codec;
2. schema 11, fault tests, and schema-10 export;
3. one atomic server policy predicate covering every content mutation;
4. stopped-server administration and truthful status;
5. bounded desktop model/presentation with downgrade clearing;
6. deterministic, process, mixed-version, resource, performance, and native
   platform qualification;
7. joint activation review.

Production negotiation remains disabled until all applicable gates pass.

## Implementation record

1. **Complete (2026-07-27):** add the independent capability name, fixed policy
   bit/mask, bounded four/five-field room value codec, shared fixtures, and
   byte-exact agreement in both independent MessagePack codecs. The production
   client request vector and server acceptance remain unchanged. Evidence is in
   `docs/audits/omenchat-announcement-rooms-wire-qualification.md`.
2. **Complete (2026-07-27):** schema 11 adds constrained room policy storage,
   migration fault injection proves transactional rollback, and the
   confirmation-gated stopped-server schema-10 copy removes only that column.
   Evidence is in
   `docs/audits/omenchat-announcement-rooms-storage-qualification.md`.
3. **Complete (2026-07-27):** one store-owned policy predicate now gates
   legacy/durable content mutations; durable paths read current policy inside
   replay transactions; error 1016 is stable; and a confirmation-gated
   stopped-server command changes policy/revision atomically. Human/JSON room
   listings report effective policy. Production negotiation remains dormant.
   Evidence is in
   `docs/audits/omenchat-announcement-rooms-authorization-qualification.md`.
4. **Complete (2026-07-27):** a bounded, session-owned client projection now
   decodes exact negotiated room catalogs/deltas, clears evidence on capability
   loss, rechecks live/durable mutations, and disables member desktop composer,
   attachment, reaction, and revision controls. No client persistence changed.
   omenchatd's TUI has no member composer and retains stopped-server policy
   administration during dormancy. Evidence is in
   `docs/audits/omenchat-announcement-rooms-client-projection-qualification.md`.
5. **Partial (2026-07-27):** current/current real-Link authorization now proves
   typed member rejection with no committed event before and after an orderly
   server restart. The same isolated identity root opens a new Link and no
   uncertain mutation is resent automatically. Exact adjacent `v0.9.6-3`
   ordinary fixtures remain compatible, but negotiated adjacent announcement
   traffic is inapplicable while older peers cannot advertise the capability.
   Evidence is in
   `docs/audits/omenchat-announcement-rooms-process-qualification.md`.
6. **Partial (2026-07-27):** a canonical headless omenchatd now qualifies
   confirmation-gated moderator promotion without requiring the optional TUI.
   Real-Link moderator message and upload/Resource publication pass in an
   announcement room and again after an orderly restart. No capability vector
   changed. Evidence is in
   `docs/audits/omenchat-announcement-rooms-process-qualification.md`.
7. **Complete (2026-07-27):** real-Link standard-member upload offers receive
   typed policy rejection without acceptance, completion, committed upload
   event, ledger row, or server file. The same assertions pass after orderly
   restart, with the server ledger remaining at zero files/bytes. Evidence is
   in `docs/audits/omenchat-announcement-rooms-process-qualification.md`.
8. Next: negotiated current/current catalog/delta traffic, same-process
   replacement-Link recovery, native GUI observation, and a restart-only versus
   live reload/fanout decision. Do not activate production negotiation before
   that joint review.
