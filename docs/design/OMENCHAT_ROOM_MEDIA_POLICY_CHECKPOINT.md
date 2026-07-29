# OMENchat room media-policy checkpoint

Date: 2026-07-28

Status: dormant shared wire vocabulary and independent codec fixtures complete;
schema, configuration, negotiation, administration, and runtime behavior remain
inactive

Baseline: `release/v0.9.6-4` at `70eb363`

Release target: `v0.9.6-4`

Protocol baseline: `omenchat-v0.1`, numeric protocol version 1

Database baseline: omenchatd schema 12

## Decision

The first room media-policy extension should govern server upload admission,
not attempt to define a broad media trust system. Each room may:

- inherit the existing server-wide upload file-size ceiling;
- disable new uploads; or
- impose a smaller per-file ceiling.

The global per-identity quota remains authoritative. A room policy may only
tighten the global server policy; it cannot increase the server maximum or
quota.

This slice does not add:

- a per-room storage quota or a second eviction algorithm;
- MIME-type or filename-extension trust;
- automatic attachment downloads;
- automatic media rendering;
- recurring policy polling;
- a live policy editor; or
- a new worker, timer, queue, cache, or Reticulum operation.

OMENbrowser currently fetches OMENchat attachments only after an explicit user
action and already bounds client transfer admission. That privacy-preserving
behavior remains unchanged. A future automatic-download threshold is a local,
identity-scoped client preference and must not be enabled by a server room
policy.

## Current implementation evidence

- `SessionLimits` owns a server-wide `upload_quota_bytes` and
  `upload_max_file_bytes`. Configuration clamps the latter to at most 10 MiB.
- `UploadOffer` is rejected before Resource admission when the advertised size
  exceeds the global file ceiling, the global identity quota is disabled or
  exhausted, the member is unauthorized, the room is announcement-only for
  that member, or the bounded pending-offer store is full.
- Pending upload offers are bounded, identity-bound, room-bound, and expire.
- Resource publication rechecks identity and announcement-room authorization,
  verifies the exact advertised length, then uses the existing per-identity
  serialization and same-filesystem durable-replacement path.
- The durable file replacement writes and syncs a temporary file, renames and
  syncs the directory, records the upload ledger, and only then removes planned
  older files. A failed post-commit ledger cleanup fails conservatively by
  invalidating ledger trust.
- `upload_files` already records `room_id`, but quota planning is intentionally
  per authenticated uploader identity. Changing that ownership would be a
  separate storage and eviction-policy migration.
- Schema 12 room state contains `policy_bits` and `slow_mode_seconds`.
- Negotiated room catalog values have exact cumulative shapes:
  four fields for legacy, five for announcement policy, and six for slow mode.
- Room-policy administration is restart-only. The command fails closed while
  the running server owns the database.
- `UploadReject` currently has the exact legacy body
  `[reason, quota_bytes, incoming_bytes]`. The client clears only its matching
  pending offer and displays bounded text.

## Policy model

Use one optional room scalar:

```text
room_upload_max_file_bytes:
  nil       inherit the server-wide maximum
  0         uploads disabled in this room
  1..10MiB  room-specific maximum
```

The effective upload maximum is:

```text
global uploads disabled          => disabled
room value is 0                  => disabled
room value is nil                => global upload_max_file_bytes
room value is positive           => min(room value, global upload_max_file_bytes)
```

The global per-identity `upload_quota_bytes` remains a separate ceiling. The
room scalar does not reserve bytes, alter eviction ordering, or grant quota.
Existing announcement-room authorization runs before this policy, so a member
who cannot publish receives the existing announcement-policy rejection and
does not consume pending-upload capacity.

The policy applies to new `UploadOffer` admission and is checked again before
Resource publication. A policy rejection:

- creates no pending offer when rejected at offer time;
- creates no file, upload-ledger row, or room event when rejected at
  publication time;
- does not evict a committed upload;
- does not consume a message/action slow-mode interval; and
- is never retried automatically.

The initial restart-only administration contract means policy cannot change
while a canonical server owns pending offers. Publication must nevertheless
re-read the current room policy so the invariant remains safe for tests,
recovery, and any later deliberately designed live-update path.

## Capability and exact wire shape

Proposed capability:

```text
room-media-policy-v1
```

The capability reports authoritative room policy to the client. Server
enforcement never depends on negotiation.

Because room values are currently cumulative positional arrays,
`room-media-policy-v1` requires both `announcement-rooms-v1` and
`room-slow-mode-v1`. The latter already requires `durable-mutations-v1`.
Reject a requested or accepted capability list that violates these
dependencies. This dependency is about exact shape comprehension; upload
publication itself does not become a durable mutation.

Room catalog and `RoomDelta` shapes remain:

```text
legacy:
  [room_id, name, topic_or_nil, room_revision]

announcement-rooms-v1:
  [room_id, name, topic_or_nil, room_revision, policy_bits]

room-slow-mode-v1:
  [room_id, name, topic_or_nil, room_revision, policy_bits,
   slow_mode_seconds]

room-media-policy-v1:
  [room_id, name, topic_or_nil, room_revision, policy_bits,
   slow_mode_seconds, room_upload_max_file_bytes_or_nil]
```

Validation rules:

- the value has exactly seven fields for the media-policy shape;
- the final field is `nil` or an unsigned integer no larger than 10 MiB;
- zero is the explicit disabled value, not inheritance;
- unknown policy bits and invalid earlier fields continue to fail closed;
- unnegotiated peers receive the exact existing four-field shape;
- announcement-only peers receive exactly five fields;
- slow-mode-only peers receive exactly six fields; and
- capability loss, Link replacement, identity replacement, active-room
  replacement, or malformed authoritative data clears the client projection.

The shared protocol crate may own the capability name, bound, typed optional
scalar, shape variant, fixtures, and validation. It must not own SQLite,
Reticulum, Iced, Ratatui, filesystem storage, quota eviction, or server policy.

### Rejection evidence

Legacy and non-negotiating peers retain the existing three-field
`UploadReject` body exactly.

A peer that negotiated `room-media-policy-v1` may receive a fourth trailing
unsigned reason code while preserving the first three fields:

```text
[reason, effective_limit_bytes, incoming_bytes, reason_code]
```

Reserve stable reason codes for at least:

```text
1  room uploads disabled
2  room file-size ceiling exceeded
```

The negotiated client must use the numeric code for controls and diagnostics,
not parse the human string. Unknown reason codes remain a generic rejection.
This extension must have byte fixtures in both independent codecs before use.
If a separate typed error is found to preserve pending-upload correlation more
cleanly during implementation, stop and amend this checkpoint before changing
the wire; do not silently substitute a different shape.

## Persistent schema proposal

Schema 13 adds one nullable constrained column:

```sql
ALTER TABLE rooms
ADD COLUMN upload_max_file_bytes INTEGER DEFAULT NULL
CHECK(
  upload_max_file_bytes IS NULL OR
  upload_max_file_bytes BETWEEN 0 AND 10485760
);
```

Properties:

- `NULL` preserves every existing room's current effective behavior.
- `0` disables uploads for the room.
- A positive value only tightens the global maximum at runtime.
- No existing upload, ledger, room event, history, replay result, identity, or
  slow-mode row is rewritten.
- A policy update and `room_revision` increment occur in one immediate
  transaction.
- Reapplying the same value is a no-op and does not increment the revision.
- No new row-per-room, row-per-user, or row-per-upload table is needed.

Migration must:

- create the normal pre-schema-13 backup;
- add the column and update `user_version` in the existing single transaction;
- inject failure at column, version-update, and commit boundaries;
- prove every failure leaves a valid schema-12 source;
- preserve representative schema-12 announcement/slow-mode state and uploads;
  and
- reject a database newer than the supported schema.

Add a confirmation-gated, stopped-server:

```text
omenchatd database export-schema12-copy \
  --to <new-path> --confirm --home <server-home>
```

The staged copy must remove only `upload_max_file_bytes`, set
`user_version = 12`, run integrity and schema-shape validation, sync, and
atomically publish without touching the live database. Existing deeper export
paths should continue to remove each later layer in descending order.

## Server enforcement boundaries

One store-owned function should return a typed effective room upload policy
from an existing room row. Do not duplicate `NULL`/zero/minimum semantics in
offer handling, publication handling, administration, status, and tests.

Offer order:

1. validate room, authenticated user, membership, mute/ban state, and
   announcement authorization;
2. parse bounded upload metadata and reject zero length;
3. derive the effective room/global maximum;
4. reject disabled or oversized offers;
5. apply the existing command-rate and global identity-quota checks in their
   reviewed order;
6. admit to the existing bounded pending-offer store.

The exact placement of rate admission must preserve current rollback behavior.
Tests must prove a room-policy rejection does not consume a rate slot. If the
current ordering does consume one, refactor with the existing owned reservation
rather than adding refunds, counters, sleeps, or retries.

Publication order:

1. take only the exact identity-bound pending offer;
2. validate exact Resource length;
3. re-resolve authenticated user and room authorization;
4. re-read and apply the effective room upload policy;
5. enter the existing per-identity upload serialization and quota planner;
6. preserve the current durable replacement, ledger, eviction, and room-event
   ordering.

No permit or pending offer remains held after rejection, error, cancellation,
Link retirement, or shutdown. Policy code must not hold a SQLite transaction
or mutex across filesystem I/O.

## Administration and frontend projection

The first administrative surface is restart-only:

```text
omenchatd rooms set-upload-policy <room_id> inherit|disabled|<bytes> \
  --confirm --home <server-home>
```

Requirements:

- positive existing room ID;
- exact validated vocabulary;
- positive values no larger than 10 MiB;
- current-schema database only;
- exclusive stopped-server ownership;
- transactional scalar/revision update;
- explicit prior/configured/effective values in human and JSON output; and
- no secrets or identity material in arguments or output.

The omenchatd TUI remains an operator surface. It may show configured policy
but must not pretend to be a member attachment client.

The desktop client keeps one bounded, session-owned room policy projection.
After authoritative negotiation it may:

- show `Uploads disabled` or `Uploads ≤ <size>`;
- disable Attach only when uploads are authoritatively disabled;
- reject a locally selected file above the effective room/server ceiling
  before allocating or queueing it;
- preserve the selected file/draft after a server rejection; and
- show a copyable typed rejection reason.

Without negotiated evidence, the client keeps legacy behavior and lets the
server decide. It must not infer policy from a rejection string, room topic,
descriptor, previous Link, or cached server display name.

No recurring subscription is needed. Initial catalog and `RoomDelta` events
update the projection; Link/capability/room retirement clears it.

## Compatibility behavior

- Current server + legacy client: server enforces policy; client may still show
  Attach and receives the exact legacy `UploadReject`.
- Current client + legacy server: capability is not accepted; the client shows
  no room-specific policy and keeps the current server-global checks.
- Current/current: exact seven-field room evidence and optional typed trailing
  rejection code are available only after explicit acceptance.
- Adjacent product builds without this feature retain four/five/six-field
  decoding according to their negotiated capability set.
- Application version, descriptor capability hints, and room topic never imply
  activation.
- Protocol version, destination/aspects, operation numbers, resource metadata,
  upload content, and existing database identifiers remain unchanged.

## Failure and crash boundaries

Tests must cover:

- schema-12 migration success and each injected rollback boundary;
- schema-12 copy export success, existing-target refusal, permission failure,
  integrity failure, and source/destination alias refusal;
- inherit, disabled, lower limit, equal limit, and attempted above-global
  administration;
- no-op updates and revision behavior;
- offer rejected while disabled;
- offer rejected above the room ceiling but below the global ceiling;
- global quota and global maximum still dominate;
- standard and privileged users in ordinary and announcement rooms;
- rejection consumes no pending item, rate slot, file, ledger row, event, or
  eviction;
- policy recheck before publication;
- Resource length mismatch and cancellation;
- policy/storage/read failure after offer and before publication;
- disk-full/write/sync/rename/database-commit failures with existing durable
  replacement invariants;
- exact current/current negotiated shape and rejection code;
- simultaneous legacy, announcement, slow-mode, and media-policy Links;
- Link replacement, capability loss, server restart, and client restart;
- malformed `nil`/integer types, oversized integers, wrong field counts, and
  unsolicited seven-field values;
- bounded client projection and static UI behavior; and
- no automatic uncertain retry.

Process qualification must use isolated server/client identity, database,
upload, cache, and Reticulum roots. It must prove a rejected Resource leaves no
server file or database record and a permitted upload survives orderly restart.

## Resource and measurement gate

This design adds one nullable fixed-width scalar per room and one scalar in a
negotiated room projection. It adds no worker, timer, queue, cache, history, or
polling loop.

Before activation record:

- database growth for 1, 256, and the maximum supported room count;
- room-catalog retained bytes at the existing 256-room/512-KiB client cap;
- offer rejection latency and SQLite operation latency;
- pending-offer item/byte counts before and after rejection/cancellation;
- upload completion and shutdown duration;
- idle desktop/server CPU and RSS under a settled real Link; and
- file/ledger/event counts across rejection and successful publication.

Physical GPU activity and non-local network topology remain manual evidence.
Do not invent measurements.

## Staged implementation order

1. **Complete (2026-07-28):** dormant shared constant, bound, shape variant,
   typed optional scalar, capability dependency, rejection-code vocabulary,
   and byte-exact fixtures. Both independent codecs reproduce the exact
   seven-field room delta and typed upload-rejection fixtures. Production
   clients do not request and servers do not accept the capability. Evidence:
   `docs/audits/omenchat-room-media-policy-wire-qualification.md`.
2. **Complete (2026-07-28):** schema-13 nullable scalar, migration/fault/restart
   tests, and guarded schema-12 copy export. Runtime enforcement remains
   inactive. Evidence:
   `docs/audits/omenchat-room-media-policy-storage-qualification.md`.
3. **Complete (2026-07-28):** store-owned effective-policy resolver plus
   test-only offer/publication enforcement using existing quota and durable
   file boundaries. Production constructors keep enforcement disabled.
   Evidence:
   `docs/audits/omenchat-room-media-policy-enforcement-qualification.md`.
4. **Complete (2026-07-28):** confirmation-gated stopped-server
   administration and bounded human/JSON/TUI status evidence explicitly
   labeled inactive. Evidence:
   `docs/audits/omenchat-room-media-policy-administration-qualification.md`.
5. **Complete (2026-07-28):** bounded desktop projection and static Iced
   controls. omenchatd TUI reports configured/inactive operator evidence only.
   Evidence:
   `docs/audits/omenchat-room-media-policy-client-projection-qualification.md`.
6. **Complete (2026-07-28):** current/current capability, Link
   ownership, under-limit Resource commit/fetch, typed over-limit and disabled
   rejection, clean rejection ledger/filesystem, orderly restart, and policy
   re-projection pass in isolated real-process qualification. The optimized
   isolated retention measurement additionally crosses the configured upload
   quota four times and verifies exact retained file/byte and zero-pending
   bounds. The locked Reticulum 0.9.6 public API exposes initiator/outbound
   cancellation only, so receiver-side cancellation remains an explicit
   upstream limitation rather than a fabricated pass. Native Linux Iced now
   also passes accepted, over-limit, and disabled attachment cases with
   independent durable storage assertions. Adjacent-version and optimized
   live-process CPU/RSS/handle/queue/shutdown evidence pass. Physical-network
   evidence remains unclaimed. Evidence:
   `docs/audits/omenchat-room-media-policy-resource-qualification.md` and
   `docs/audits/omenchat-room-media-policy-resource-measurement.md` and
   `docs/audits/omenchat-room-media-policy-gui-qualification.md`.
7. **Complete (2026-07-28):** activate negotiation and enforcement together in
   canonical desktop and standalone server product profiles; retain
   qualification hooks outside release graphs. Evidence:
   `docs/audits/omenchat-room-media-policy-activation-review.md`.
8. Batch Windows/macOS native presentation, Python interoperability, and
   packaging with the stable release candidate.

Each slice must remain independently buildable and reversible. Do not combine
schema introduction, production negotiation, and activation in one patch.

## Rollback

After activation:

1. stop omenchatd;
2. preserve the schema-13 database and sidecars;
3. create and validate the schema-12 copy;
4. disable `room-media-policy-v1` request/accept in both rebuilt binaries or
   install the matching prior pair;
5. move the live database aside rather than deleting it;
6. restore the copy only when the selected prior binary requires schema 12.

No rollback deletes identities, uploads, history, replay results, slow-mode
admissions, room policy, or client attachment caches automatically.

## Approval decision

The checkpoint was approved through the completed staged gates. The activated
contract remains:

- the v1 scope is inherit/disabled/per-file ceiling only;
- `NULL`/zero/positive semantics;
- the cumulative seven-field shape and capability dependencies;
- the negotiated trailing `UploadReject` reason code;
- schema 13 and schema-12 copy strategy; and
- restart-only administration.

Changing any of these compatibility decisions requires an explicit checkpoint
amendment before implementation.
