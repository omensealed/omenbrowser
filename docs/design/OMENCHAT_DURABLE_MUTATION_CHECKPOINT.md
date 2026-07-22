# OMENchat durable mutation checkpoint

Status: checkpoint accepted; negotiated room-text durable transmission, conservative restart recovery, and explicit retry active
Baseline: OMENbrowser/omenchatd v0.9.5-2, OMENchat protocol v1  
Proposed capability: `durable-mutations-v1`

## Observed boundary

`src/server/src/live_retry_safety_tests.rs` now characterizes all required
uncertain outcomes. Protocol v1 safely deduplicates an exact mutation only
while the same Reticulum Link remains active. It can commit a mutation whose
response is lost; Link close removes the replay entry; a forced resend across a
new Link, client restart, or server restart creates a second event. Exact
same-Link duplicates return the original result and same-Link sequence reuse
with different content is rejected.

The product must continue to show the first group as uncertain and must not
automatically resend it. The existing 32-bit `seq` is transient response
correlation, not durable operation identity.

## Compatibility invariant

- Protocol version 1, `omenchat-v0.1`, destination names, operation numbers,
  and all legacy frame shapes remain valid.
- The extension activates only when both peers negotiate
  `durable-mutations-v1` on the current authenticated Link.
- The authenticated Reticulum identity remains the security principal. A
  client instance identifier is not an identity or trust claim.
- Legacy peers never receive a durable envelope they did not negotiate.
- Queue admission, Link send, and replay lookup are not final delivery.

## Identifiers and canonical hash

`client_instance_id` is 128 random bits from the operating-system CSPRNG,
created once per browser identity/application root and persisted owner-only. It
is never derived from identity hash, display name, clock, PID, or sequence.
Corruption must not silently replace it while uncertain intents exist.

Implemented foundation: desktop startup loads or atomically
creates the raw 16-byte value at the active identity-scoped
`omenchat/client-instance-id` path. Publication is create-without-replacement,
owner-only on Unix, synchronized, concurrency-safe, and rejects symlinks,
special files, wrong lengths, and permissive modes without rewriting them. The
value is retained in live client state and transmitted only in a bounded
`SessionOpen` capability request. Negotiated room-text sends use it as part of
their persistent replay key; uncertain intents are retried only after an
explicit, confirmed action.

`mutation_id` is a fresh random 128-bit value generated before each logical
mutation is persisted and remains stable only for retries of that intent.

`request_hash` is a 32-byte SHA-256 digest, using the already-present crate
family and domain `omenchat durable mutation v1\0`. It covers a canonical binary
encoding of protocol version, operation, room presence/value, body kind, and
complete legacy body. It excludes Link ID, `seq`, mutation ID, and display
name. The server recomputes and verifies it. Encoding must be defined by test
vectors and cannot depend on Rust layout, map order, JSON, or debug strings.

## Proposed negotiation

Optional trailing fields preserve existing positions:

```text
SessionOpen Fields:
  0 protocol name                 existing
  1 display name or Nil           existing
  2 client LXMF destination/Nil   existing deployed extension
  3 requested capabilities        Array<String>, optional
  4 client_instance_id            Bytes(16), optional

SessionAccept Fields:
  0 protocol name                 existing
  1 rooms                         existing
  2 MOTD                          existing
  3 upload quota                  existing
  4 ping interval                 existing
  5 upload max file bytes         existing
  6 accepted capabilities         Array<String>, optional
```

An old server returns no accepted capability, so a new client stays in legacy
mode. An old client never requests it, so a new server stays in legacy mode.
Descriptor evidence is informational; only `SessionAccept` activates it.

The original checkpoint draft placed requested capabilities at field 2. Current
code inspection found that omenchatd already consumes field 2 as the optional
client LXMF destination. The shared negotiation contract therefore preserves
that deployed field and uses trailing fields 3 and 4. This correction is based
on current implementation and avoids reinterpreting existing peers.

## Proposed mutation envelope

Only negotiated `RoomMessage`, `RoomAction`, `RoomNotice`, `PartRoom`, and
mutating commands use:

```text
FrameBody::Fields [
  String("durable-mutation-v1"),
  Bytes(mutation_id[16]),
  Bytes(request_hash[32]),
  U64(legacy_body_kind),       # 0 Empty, 1 Text, 2 Fields
  legacy_body_value            # Nil, String, or Array<FrameValue>
]
```

All current MessagePack byte/scalar/container/depth limits remain in force,
plus exact dedicated ID/hash lengths. The client instance comes from the
negotiated Link and is not repeated. The replay key is:

```text
(authenticated_identity_hash, client_instance_id, mutation_id)
```

Lookup semantics:

- missing: execute and atomically store the terminal origin result;
- exact request hash: return that original result without repeating rate
  accounting, mutation, fan-out, or moderation side effects; the transient
  response sequence is replaced with the current request sequence;
- different hash: return a machine-readable conflict;
- known expired/pruned identity: return an explicit expired result and never
  execute automatically.

The shared protocol crate reserves these stable protocol-v1 errors:

- `1011`: `durable_mutation_not_negotiated`;
- `1012`: `durable_mutation_malformed`;
- `1013`: `durable_mutation_conflict`;
- `1014`: `durable_mutation_result_expired`;
- `1015`: `durable_mutation_store_busy`.

The client can label these codes, but dormant mutation sending neither emits nor
acts on most of them yet. The server uses 1012 to reject malformed capability
negotiation. A valid durable request carrying a persistent client instance now
receives explicit acceptance; unknown requests retain the unchanged legacy
`SessionAccept` without accepted capabilities.

## Implemented dormant omenchatd schema v3

The server now uses SQLite `user_version = 3` with this dormant addition:

```sql
CREATE TABLE durable_mutation_results (
  identity_hash       BLOB    NOT NULL,
  client_instance_id  BLOB    NOT NULL CHECK(length(client_instance_id) = 16),
  mutation_id         BLOB    NOT NULL CHECK(length(mutation_id) = 16),
  request_hash        BLOB    NOT NULL CHECK(length(request_hash) = 32),
  result_frame        BLOB    NOT NULL,
  retained_bytes      INTEGER NOT NULL,
  created_at          INTEGER NOT NULL,
  last_seen_at        INTEGER NOT NULL,
  PRIMARY KEY(identity_hash, client_instance_id, mutation_id)
);

CREATE INDEX idx_durable_mutation_results_created
ON durable_mutation_results(created_at, identity_hash,
                            client_instance_id, mutation_id);

CREATE TABLE durable_mutation_clients (
  identity_hash       BLOB    NOT NULL,
  client_instance_id  BLOB    NOT NULL CHECK(length(client_instance_id) = 16),
  first_seen_at       INTEGER NOT NULL,
  last_seen_at        INTEGER NOT NULL,
  retired_at          INTEGER,
  PRIMARY KEY(identity_hash, client_instance_id)
);
```

The table and index exist, but no live request path reads or writes them yet.
`result_frame` retains the exact bounded encoded origin response and is
validated before reuse. Replay preserves its operation, room, and body while
replacing only the transient sequence with the current request sequence.
Mutation lookup/execution, event-ID allocation, room-event insertion,
response construction/encoding, and replay insertion share one
`BEGIN IMMEDIATE` transaction. It commits before fan-out. Failure of any step
rolls back both mutation and replay result. Concurrent duplicates serialize on
the same database transaction.

The dormant store implementation now provides that single immediate-
transaction boundary. Exact hashes return the validated original encoded
frame, different hashes return a conflict without running the callback, and
missing keys run the SQLite-only mutation callback once before replay
publication. Invalid or larger-than-64-KiB results, capacity exhaustion, and
callback failures roll back both mutation and replay state. Retention applies
the proposed global/per-identity item and byte ceilings and performs no more
than 128 deterministic oldest-row deletions per commit. Before deleting any
result, it permanently retires that authenticated identity/client-instance
pair. Every later request under the retired instance returns `Expired` before
the mutation callback can run, including after server restart. Retiring the
whole instance rather than keeping finite per-operation tombstones is
deliberately conservative: a client must rotate its instance only after it has
resolved or explicitly abandoned every pending intent. This API remains
disconnected from live sessions while that rotation protocol and numeric wire
error are unfinished.

The operation stays behind the existing bounded server database boundary. It
must not move SQLite onto arbitrary Tokio workers or hold a Reticulum lock
across disk work.

## Client intent store

Before first transmission, the identity-scoped browser root must persist:

- server destination and authenticated identity binding;
- client instance ID, mutation ID, canonical hash, and canonical body;
- creation/expiry times;
- state: `prepared`, `sent_uncertain`, `acknowledged`, `conflict`, `expired`, or
  `abandoned`;
- local message/event correlation.

Proposed initial bounds are 4,096 intents / 16 MiB total and 64 KiB per intent,
with a 30-day terminal ceiling. Pending/uncertain intents are not silently
dropped; new admission fails visibly. Publication uses the existing atomic,
owner-only, identity-scoped recovery rules. Legacy sends remain unchanged if
negotiation or persistence is unavailable.

The client implementation uses the identity-scoped, owner-only
`omenchat/mutation-intents.sqlite`. Preparing an intent generates a random
128-bit mutation ID, computes the shared canonical hash, encodes a sequence-zero
legacy request fixture, and commits all metadata in one immediate SQLite
transaction before returning. Admission enforces the proposed item/byte limits
and never evicts existing pending or uncertain rows. Recovery preflights field
lengths before blob/text allocation, then verifies the frame metadata,
canonical hash, expiry, state, and retained-byte accounting. The synchronous
store is never opened from Iced update handling. Desktop startup constructs its
bounded owner only after the persistent client instance and authenticated
active identity are available.

That owner is one named thread with a 32-item/2-MiB
`sync_channel`, pre-admission payload validation, nonblocking overload
rejection, queue-item/byte/rejection/completion metrics, and joined
draining shutdown. It owns the SQLite connection and supports prepare,
compare-and-transition, bounded nonterminal recovery, and incremental terminal
pruning. Worker replies are received through bounded blocking tasks rather than
blocking the Iced update path or an arbitrary Tokio worker. State transitions
are monotonic: prepared may become uncertain,
expired, or abandoned; uncertain may become acknowledged, conflict, expired,
or abandoned; terminal states never regress. Negotiated room-text sends now
use this owner; other mutations remain on the unchanged legacy path.

Desktop restart recovery is deliberately read-only. The first OMENchat
maintenance deadline submits one bounded recovery command and never transmits
an intent. Results are filtered to the active authenticated identity and
persistent client instance before retention in desktop state; records belonging
to another identity are counted without exposing their request bodies or
identifiers. Redacted session diagnostics report prepared, uncertain,
past-expiry, and worker-queue counts. Recovery failure is visible and does not
fall back to automatic resend.

The guarded terminal-resolution UI is active for recovered room-text intents.
It renders no more than four entries per server, bounds each preview, and requires
confirmation. `Stop Tracking` records `abandoned` without asserting whether an
uncertain server commit occurred. `Finalize Expired` rechecks the persisted
deadline before recording `expired`. Both operations use the bounded owner and
send no frame. Missing records and concurrent terminal transitions are handled
as stale local recovery state rather than overwritten.

Deliberate durable retry is active only through a separate confirmed action.
It revalidates the live authenticated identity, persistent client instance,
original server and active room, negotiated capability, expiry, transport, and
absence of an in-process pending echo. A prepared intent is transactionally
advanced to `sent_uncertain` before transmission. An uncertain intent reuses
the original mutation ID, request hash, and canonical body so the server replay
record remains authoritative across replacement Links and restarts. A retry
never creates a second logical operation, never runs automatically, and never
clears unrelated composer text. Conflict-specific remediation remains a later
unit.

Typed terminal response handling is active for conflict and replay expiry.
Only a response correlated to the outstanding durable request sequence can
release its bounded pending echo and transition the persistent intent from
`sent_uncertain` to `conflict` or `expired`. The UI reports that no further
retry will occur. Store-busy, malformed, unnegotiated, generic, and
uncorrelated errors remain uncertain; they cannot fabricate a delivery or
terminal result. A persistence failure leaves the durable intent recoverable
and is reported instead of being hidden.

On Link retirement, transient pending correlations are discarded. Durable
optimistic echoes owned by that Link are removed from the timeline because the
persistent uncertain intent is the recovery authority; legacy uncertain echoes
remain visible and are not resent. A replacement Link must negotiate the
capability again before a confirmed retry. Intent-worker shutdown places its
sentinel after already admitted bounded commands and joins only after those
SQLite operations complete.

## Server retention

Initial ceilings, subject to soak evidence:

- age 30 days;
- 100,000 items / 64 MiB globally;
- 10,000 items / 8 MiB per authenticated identity;
- 64 KiB maximum stored result;
- at most 128 rows pruned per newly committed mutation.
- 100,000 remembered client instances globally and 1,024 per authenticated
  identity, including retired instances.

Prune expired then oldest terminal rows until all ceilings hold. Admission may
not scan or delete an unbounded set. Metrics expose items, bytes, oldest age,
hits, conflicts, expiry, pruning, and busy rejection without logging IDs.

Expired keys are deterministic without trusting clocks: pruning a result first
retires its complete authenticated identity/client-instance pair in a permanent,
bounded registry. Requests from a retired instance return `Expired` and never
execute. Registry capacity fails closed before callback execution. The tradeoff
is intentional coarse invalidation and a finite lifetime rotation budget.

The inactive client store implements the safe rotation rule: it holds an
immediate intent-database transaction, refuses rotation while any prepared or
uncertain intent exists, then atomically replaces and synchronizes the
owner-only ID file. Terminal historical intents keep their original instance
ID. A crash after replacement is safe because the exclusive database check
proved that no unresolved intent referenced the old ID. Production does not
call this boundary.

## Migration and rollback

The implemented migration uses the existing sibling pre-migration
backup, uses an immediate transaction, refuses future schemas, and provides a
guarded restore command. The table/index are part of the idempotent schema and
`SCHEMA_VERSION` is 3. A v2 preservation fixture plus the existing injected-
failure, backup-collision, integrity, restore, and future-schema tests cover the
migration boundary.

v0.9.5-2 rejects schema v3. Rollback therefore requires stopping omenchatd and
using the guarded restore command with the generated pre-v3 backup. Existing
room events/uploads are not rewritten. This compatibility decision requires
approval before implementation.

## Crash boundaries

- Before client intent commit: nothing transmits.
- After intent commit, before transmit: intent remains `prepared`.
- After transmit, before acknowledgement: intent remains uncertain and can be
  queried/retried only after capability re-negotiation.
- Server failure before SQLite commit: neither mutation nor replay exists.
- Server failure after commit, before response/fan-out: duplicate returns the
  stored result under the retry's response sequence and history reconciliation
  supplies the event.
- Client persistence failure after response: exact retry returns stored result.
- Replay insert failure rolls back the mutation.
- Stored-result corruption fails closed and never executes again.
- Database busy returns explicit admission failure without claiming execution.

## Mixed versions

| Client | Server | Behavior |
|---|---|---|
| old | old | current v1 Link-scoped replay |
| new | old | current v1; no envelope or automatic uncertain retry |
| old | new | current v1; existing Link cache |
| new | new, rejected | current v1 |
| new | new, accepted | persisted intent and durable envelope |

Mixed v0.9.5-2/v0.9.6-1 fixtures must prove byte-identical legacy frames and
that neither side infers capability from application version.

## Shared protocol crate decision

Browser and server previously duplicated protocol enums/constants while their
codec implementations differed in organization. The first approved,
compatibility-only unit created the shared boundary at:

```text
src/server/crates/omenchat-protocol/
```

It remains inside the independently relocatable server tree while the browser
can use a path dependency. It may contain only versions, ops, capabilities,
wire types, bounds, codec/canonical hashing, error codes, fixtures, and
conformance tests—never SQLite, Iced, Ratatui, Reticulum ownership, filesystem
storage, or policy. The initial crate contains existing protocol v1
types/numbers and the shared public fixture. Both existing bounded codecs pass
the same byte vectors, and standalone relocation passes. A subsequent
non-activating contract unit added fixed-size client-instance, mutation, and
request-hash types; the proposed durable envelope shape; and bounded canonical
SHA-256 hashing. Its fixed vector is locked by a crate-local test. Envelope
construction and parsing enforce canonical scalar/container/value/depth limits
before the extension can be connected to either live codec. Only a client with
a persistent instance identity advertises the capability, and only an explicit
matching `SessionAccept` activates it for that Link. Legacy frames remain
unchanged. Negotiated room-text sends and explicit retries use the durable
envelope; legacy and downgraded sessions do not.

## Required test matrix

- the seven characterization cases under legacy and negotiated modes;
- duplicate after new Link, client/server restart, and two concurrent Links;
- mutation ID conflict across operation, room, body, and hash;
- authenticated-identity isolation;
- malformed/oversized envelope, ID, hash, and stored result;
- every pre/post-commit response-loss boundary;
- database busy/full/I/O failure/corruption/migration interruption;
- bounded age/count/byte pruning and incremental-work ceiling;
- retention floor, expiry, and clock skew;
- client atomic-publication crash and corrupt-intent recovery;
- capability downgrade/server replacement and old/new peer matrix;
- rate accounting, room/user fan-out, and moderation exactly once;
- shutdown with queued intent/database work;
- queue/SQLite/RSS/FD soak, format, Clippy, product/server profiles, pinned
  Python interoperability, relocation, and package gates.

## Remaining staged decisions

The checkpoint is approved in principle. Each irreversible or wire-visible
piece still requires its tests to be green before the next piece begins:

1. schema v3 and its guarded-backup rollback requirement;
2. durable envelope transmission after the now-active negotiated Link binding;
3. measured retention ceilings; and
4. live integration of the now-defined protocol-crate durable contract.

Until resolved, preserve uncertain mutations, surface them to the user, and do
not silently resend them.

Malformed negotiation is now fail-closed at the session boundary. The engine
parses optional trailing fields before returning `SessionAccept`; malformed
fields receive error 1012. The live Link is marked session-open only when the
origin response actually contains `SessionAccept`, so an error cannot satisfy
the handshake. A corrected legacy request can recover on the same Link. This
does not by itself transmit an envelope or enable retry.

The inactive server persistence layer can now append a room event and retain
the exact encoded origin reply in one SQLite transaction. First execution
returns both reply and event; replay returns only the original reply, so a
future caller cannot fan out the event twice. Conflict, expiry, invalid reply,
and transaction failure do not append an event. This is deliberately below the
live session boundary: permissions, membership, in-memory rate accounting,
broadcast policy, negotiated Link ownership, and retry remain unchanged.

The next activation gate is not another schema change. It is a tested live
orchestration rule that (a) validates permission and membership, (b) reserves
rate capacity without double-charging replay or leaking a charge on rollback,
(c) binds the negotiated client instance to the authenticated Link, and (d)
broadcasts only a newly stored event.

Rate admission is now cancellation-safe without changing legacy behavior. A
new durable mutation may acquire an opaque reservation only from the store
finisher that runs after replay lookup; a replay or conflict never invokes it.
Rollback drops the reservation, while a successful first commit returns it to
the session owner for explicit finalization. This closes the double-charge and
failed-transaction leak in the inactive boundary. Live capability acceptance
and authenticated Link/client-instance binding are now active; client envelope
dispatch remains the next gate.

The authenticated binding gate is active only after explicit acceptance.
`SessionOpen` display/LXMF metadata remains provisional until the engine emits
`SessionAccept`; malformed negotiation cannot mutate the Link's retained peer.
A durable binding requires a valid request, explicit durable capability in the
accept response, and an already authenticated Link. It is keyed by Link,
retains the authenticated identity plus fixed-size client instance, and is
removed on every Link/identity retirement path. Legacy and downgraded sessions
never create this binding.

The next gate is the inactive session-level envelope executor: canonical hash
verification, permission and membership validation, transactional room-event
commit, reversible rate admission, exact origin response, and one-time fan-out.
Only after that passes may the server advertise acceptance.

That inactive session-level executor is now implemented for room messages and
actions. Canonical hash and body validation occurs before replay admission.
Only a replay miss enters the immediate transaction and evaluates room/user
policy, reversible rate admission, membership, event insertion, exact
acknowledgement encoding, and replay publication. Stored terminal rejections
remain stable even if policy later changes. A first commit returns one event
for fan-out; replay returns no event. Restart, conflict, malformed hash,
permission change, rate, and duplicate cases pass. Client envelope dispatch is
active only for negotiated, persistently owned room text.

The live envelope-routing gate is now implemented behind the authenticated
binding. Tagged malformed envelopes fail with 1012; valid envelopes without a
matching negotiated binding fail with 1011. Bound requests bypass the legacy
Link-local sequence cache and use the durable transaction as their replay
authority. First execution sends the retained acknowledgement to the origin
and broadcasts its event; exact replay sends the original acknowledgement and
has no broadcast. Tests cover duplicate delivery under a different sequence,
malformed and unbound requests, and legacy isolation. Production capability
acceptance is now on for a valid negotiated request. The browser reaches this
route only for ordinary room-text sends after its intent is persisted as
uncertain; other mutation actions remain legacy.

The live client has a guarded durable-send boundary for
room messages and actions. It accepts only an intent already persisted in the
`sent_uncertain` state and verifies negotiated session, persistent client
instance, server destination, active room, operation, body shape, sequence, and
pending-echo budgets before sending the canonical envelope. A matching
`MessageAck` reports the fixed mutation identifier so the desktop owner can
transition the intent to `acknowledged`. Missing negotiation and merely
`prepared` intents fail before transport output. Ordinary negotiated room-text
sends call this boundary only through the bounded persistence worker, and no
uncertain intent is automatically replayed.

`PartRoom` now uses the same durable replay authority. Its membership deletion,
departure event, exact legacy-compatible `CommandResult`, and replay record are
one transaction. The store accepts a pre-encoded origin result for event-backed
mutations and validates it before commit; a validation failure rolls back both
membership and event changes. First execution performs live room cleanup and
one user-list update. If origin delivery failed after commit, replay returns the
original result and repairs stale live room ownership; fan-out occurs only when
that ownership was still present.

`RoomNotice` is now covered by the transaction-backed room-event executor. It
retains moderator/admin authorization decisions, uses reversible bounded
message-rate admission, returns the original room event to the sender, and
fans out only the first committed event. Replay remains stable after a role
change and cannot charge or broadcast again. At that checkpoint, capability
activation remained blocked on mutating-command semantics; the later command
sections record their completion.

The command executor now covers `topic` and `create`. A small store primitive
associates an exact retained origin result with one in-memory first-execution
effect, allowing the room mutation and replay result to commit atomically while
keeping `RoomDelta` fan-out outside SQLite. Replay returns no effect, so room
revisions, creation, rate admission, and observer deltas cannot repeat. Durable
`role` and `unban` now atomically update the target user, append an optional
room audit event, retain the exact origin result, and return a bounded list of
live effects only on first execution. The internal dispatch list is bounded by
command semantics rather than a queue.

The command executor now also covers active-peer `kick`, `ban`, `mute`, and
`unmute`. Target resolution retains the existing room-presence, self-target,
moderator/admin, and protected-admin rules. Target status mutation, room audit
event, exact result, and replay publication share one transaction. First
execution owns the bounded user/event broadcasts. `kick` and `ban` also own one
target-identity disconnect, which live orchestration applies immediately after
commit and before response I/O; replay has no disconnect identity and therefore
cannot close a replacement Link. The server-side operation set proposed by the
capability is staged, but capability acceptance remains blocked on activating
the browser's bounded persistent-intent owner and negotiated send/recovery
path, followed by mixed-version and failure-boundary validation.
