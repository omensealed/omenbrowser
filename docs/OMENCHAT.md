# OMENchat

OMENchat consists of:

- the built-in OMENbrowser_rs chat client;
- the standalone `omenchatd` server under `src/server`.

The server owns its own home directory and must not use `~/.reticulum`,
`~/.nomadnetwork`, or `~/.lxmd` by default.

## Server Storage

Default server home:

```text
~/.omenchatd/
```

Typical isolated test home:

```text
/tmp/omenchatd-test
```

The server home contains identity material, Reticulum config/storage, the SQLite
database, logs, uploads, and the NomadNet portal page.

Persistent server databases use SQLite WAL mode with foreign-key enforcement,
NORMAL synchronization, and a finite five-second busy timeout. Room event IDs
are selected and inserted under one immediate transaction, preserving unique,
monotonic per-room IDs when multiple database connections contend. Database
calls remain synchronous inside the store, but live Reticulum session/database
handling runs through a single-admission Tokio blocking worker. Additional work
is rejected instead of creating unbounded blocking tasks; the bounded Reticulum
ingress queue retains pending events.
CLI room list/create/topic/archive operations and line-console room/user
administration use a separate single-owner
administrative database worker with a 16-item request queue, explicit overload
rejection, a six-second response deadline, and queue/in-flight/completion/
rejection/latency metrics. The interactive dashboard keeps room metadata in a
1,024-item/1 MiB cache, refreshes it asynchronously only while a room-consuming
panel is visible, and submits create/topic/archive operations without waiting
on SQLite. Interactive room and moderation views use bounded, visible-only
five-second caches; moderation mutations and transactional stale-user pruning
complete asynchronously through the same worker. The command-driven line
console waits synchronously on that same bounded owner. Upload-ledger repair
still uses its existing synchronous maintenance path.
The current schema is recorded as SQLite `user_version = 14`. The following
version-by-version paragraphs retain the migration history; capability states
described as dormant at an earlier checkpoint are superseded by the
authoritative production matrix in `docs/OMENCHAT_PROTOCOL.md`. Version 2 adds an
actor/time index for upload quota planning. Version 3 adds the
durable-mutation replay table and creation-order index. Version 4 adds nullable
reply-event and bounded mention-ID metadata plus a partial reply index. The
negotiated durable server path validates same-room live
reply targets and current numeric mention membership transactionally and uses
one encoder for fan-out and both history forms. Existing version-0 through
version-4 databases migrate
in one immediate transaction. A database from a
newer omenchatd version is refused without modification instead of being
silently downgraded.

Version 5 adds the constrained `room_reactions` active-state table and
`room_reaction_events` append-only audit table plus target/retention indexes.
The server executor couples add/remove effects to durable replay,
enforces active/audit bounds, creates authoritative bounded snapshots, and
limits reaction-event fan-out to capability-bound Links. omenchatd now accepts
`reactions-v1` only when an identified Link explicitly requests it together
with `durable-mutations-v1`; base and legacy Links receive no reaction state.
The authoritative live reaction event is returned to the capable originating
Link as well as other capable joined Links. The origin acknowledgement remains
mutation-correlation evidence only; it is not used as a substitute for the
authoritative reaction delta.
Version 6 adds constrained dormant current-state and append-only audit tables
for the reserved `message-revisions-v1` contract. Migration and recovery
support and a bounded transactional server executor are present. The executor
enforces authorization, revision/storage ceilings, exact replay, reaction
cleanup, and explicit-target snapshots. The capability is not requested or
accepted, so normal clients cannot reach it and no client action is enabled.
Dormant Link-scoped event and history-snapshot plumbing exists behind that
disabled acceptance gate; its presence does not advertise or activate the
wire feature.
Version 7 adds a persistent per-room event-ID high-water mark. Existing rooms
seed it lazily from the indexed maximum when their next event is committed, so
migration does not scan history. Allocation and insertion share one immediate
transaction. Deleting newest or all retained rows therefore cannot reuse a
committed event ID, while a rolled-back allocation remains safely reusable.
Integer exhaustion fails closed. Retention remains disabled and schema 7 does
not delete history.
Version 8 adds an initially empty per-room history usage ledger. Existing
history is not scanned during migration. New events update stable item/byte
totals in their existing immediate transaction, and legacy rows advance by at
most 256 per append or explicit maintenance call. Backfill target and cursor
survive restart, and retention remains unavailable until accounting is
complete. Accounting failure rolls back event insertion and sequence
advancement.
An explicit store-only compaction primitive can remove at most 64 original
events in one immediate transaction after accounting is complete. It bounds
dependent reply/reaction/revision work to 20,000 rows, atomically cleans those
projections and the usage ledger, preserves upload and durable-replay records,
and leaves the event-ID high-water mark intact. No runtime configuration,
admission path, timer, protocol capability, command, or UI invokes it yet, so
upgrading still does not delete room history.
The server configuration now records a typed `[history_retention]` policy. Its
compatibility default is disabled with ceilings of 365 days, 100,000 events,
and 256 MiB per room. Enabled zero limits are rejected, and documented hard
maxima prevent an unbounded policy. `status` and `status --json` perform a
read-only inspection of at most 256 room ledgers and report omitted rooms plus
complete/incomplete/missing accounting. They do not advance accounting or
delete data. Status reports configured admission behavior but does not claim to
observe whether a live runtime is currently active.
When enabled, the policy is attached only to the live server store. Every
ordinary and durable room-event insertion evaluates age, item, and byte
ceilings in its existing immediate transaction and removes at most 64 older
originals with their dependent projections. A single newest event may exceed
the byte ceiling until the next admission. Incomplete accounting or a ceiling
that cannot be restored in one batch fails closed and rolls back both insertion
and attempted compaction.
Isolated regressions reopen a compacted file-backed store, append through the
persistent event-ID sequence, page across removed IDs, and force the retained
history through the existing Resource-offer path. The reopened store preserves
only the surviving ordered IDs and Resource payloads serialize only those
events. The existing v0.6.0-1 byte fixtures remain exact; live mixed-version
retention behavior is still a separate release qualification rather than an
inference from these deterministic tests.
An operator can stop omenchatd and run
`database advance-history-usage --room-id <id> --confirm --home <path>` to
advance one 256-event accounting batch for one room. The command requires the
existing current schema, reports its durable cursor and target, and never
deletes history. Repeating it until `complete=true` closes the fail-closed
legacy-ledger admission condition without adding a startup sweep or worker.
The desktop has a matching rebuildable revision projection outside
immutable room history. It is bounded per room, server, and identity-scoped
cache by rows and stable retained bytes; strict deltas and authoritative
explicit-target snapshots are persisted transactionally and reconciled after
restart. Invalid snapshots retain prior rows while clearing authoritative
evidence. The reserved durable-intent operation has a bounded live sender and
exact typed acknowledgement correlation. The desktop has a bounded correction
draft separate from the ordinary composer, explicit deletion confirmation,
durable prepare-before-send actions, and author/moderator/mute/depth checks.
omenchatd returns the authoritative revision event to the capable originating
Link as well as other capable joined Links, so the editor updates from the
same validated delta as every other client. The acknowledgement remains
mutation-correlation evidence and exact replay does not fan out again.
Those controls require authoritative target evidence plus explicitly
negotiated `durable-mutations-v1` and `message-revisions-v1`. The client
requests the revision capability only with its persistent client instance
identifier; unsolicited acceptance and capability loss remain fail closed.
The shared Iced timeline renders the negotiated projection. It borrows only
authoritative rows for retained targets:
corrections show effective text with an edited marker, while tombstones hide
the original body plus reply, mention, media, reaction, resend, and mutation
actions. Stale restored rows remain hidden until an explicit-target snapshot
or a validated negotiated live delta re-establishes authority for that target.
An exact live replay restores stale target evidence once without changing the
retained row; stale or conflicting deltas restore nothing, and other targets
remain stale. This adds no worker, timer, automatic retry,
or per-redraw revision-body clone. The dormant sender never applies revision
state optimistically and uses the existing item-bounded per-session pending
mutation queue.

The desktop client's independent `chat.sqlite` adds a default-off
`rooms.mute_except_mentions` preference. It is shown only when a negotiated
nonzero local OMENchat user ID is known. When enabled, exact numeric rich
message metadata for that ID is required to increment the local room unread
counter; ordinary events remain stored and reconciled normally. This preference
does not itself request or enable the negotiated reply/mention wire capability.
The same identity-scoped database now has an additive constrained
`room_reactions` cache. Reaction rows stay outside message history and are
bounded per actor/target, target, room, server, and database by both items and
retained bytes. The shared client state applies only strictly decoded,
negotiated deltas and authoritative explicit-target snapshots, including the
existing bounded inline/Resource history paths, and restores only reactions
whose eligible target events remain in the bounded resident history. The Iced
timeline contains fixed-token controls which additionally require both
negotiated capabilities, a bound local user, a retained target, and current
authoritative snapshot evidence. They persist through the bounded durable
mutation owner before sending and never update counts optimistically.
Production session-open frames request `reactions-v1` only when the persistent
durable-mutation owner is ready. Older or capability-absent servers leave the
controls hidden and ordinary room behavior unchanged.
The desktop exposes the fixed reaction vocabulary as a compact emoji strip
with semantic hover labels instead of a wrapping block of textual buttons.
Reply uses the compact Nerd Font comments glyph with a `Reply` tooltip. Reaction
summaries use the same emoji vocabulary while retaining actor counts and the
explicit `you` marker.
Negotiated Links now receive an explicit reaction snapshot after join and
recent-history synchronization. The snapshot covers only the bounded history
range represented to that client. Inline and Resource forms use the same strict
decoder; base or legacy Links receive neither. The release smoke's one-byte
batch threshold is an isolated way to select Resource transport earlier and
does not change message, Resource, allocation, or retention limits.
The shared presentation reducer can summarize retained rows by fixed token and
distinct actor count. When retained reaction state is available, the
Iced timeline displays those summaries as chips and marks `you` only
when the negotiated numeric local-user ID is among the actors. The summary
chips remain read-only; a bounded token-control overlay appears only when
every dormant action gate above is satisfied and the pointer is over that
specific message, without adding timeline height. Moving between messages
retains at most one ephemeral hover owner and does not change reaction state.
Counts are visible only after a
validated live snapshot marks that explicit target complete. Cache restore and
reconnect clear this non-persistent evidence without deleting bounded rows, so
stale counts are not presented as current while reconciliation is pending. The
legacy Ratatui Messages
workspace currently represents LXMF conversations, not OMENchat sessions, so
it does not display OMENchat reaction state.
Before migrating a non-empty older database, omenchatd creates an owner-only
SQLite-consistent sibling backup named
`omenchat.sqlite.pre-v14-from-v<old>.bak`. It never overwrites an existing backup;
backup failure aborts migration, and a successful backup is retained for
operator recovery.
Schema statements and the version update share one immediate transaction, so a
failed migration leaves neither a partial schema nor an advanced version.
Offline recovery is available as `omenchatd database
restore-migration-backup --from <generated-sibling-backup> --confirm --home
<path>`. It rejects a running/WAL-active database, symlinks, corrupt backups,
current/future schema inputs, and filename/version mismatches. A private staging
copy must migrate and pass SQLite integrity/foreign-key checks before atomic
publication. The selected source remains unchanged and the prior active
database is retained as an owner-only `pre-restore` backup. Operators must run
`doctor` before restarting.

Schema 14 adds nullable, checked `users.nickname_colour_rgb` while reusing the
existing `profile_revision`. `NULL` means deterministic automatic presentation;
the migration does not backfill random values or rewrite identities. The
negotiated mutation is self-only, rate bounded, transactionally coupled to its
durable replay result, and broadcasts only on an actual change. Rolling back to
v0.9.8-3 requires restoring the automatic schema-13 pre-migration backup before
starting the old binary.

An offline non-destructive schema-9 downgrade artifact can be created with
`omenchatd database export-schema9-copy --to <new-path> --confirm --home
<path>`. It removes only schema-10 moderation-audit storage and preserves
schema-9 pins and every earlier layer. The audit capability remains dormant;
only durable in-room moderation paths whose user change and replay result
share one immediate transaction populate this bounded storage.

An offline non-destructive schema-8 downgrade artifact can be created with
`omenchatd database export-schema8-copy --to <new-path> --confirm --home
<path>`. It removes schema-10 moderation history and schema-9 pin state/audit
objects and preserves history usage, event sequences, history, reactions, and
dormant revisions.

The desktop maintains a separate bounded pin projection in its
identity-scoped `chat.sqlite`. It may retain pin rows across restart, but those
rows are explicitly stale until a negotiated exact-target snapshot or delta
restores authority. The timeline labels this distinction as `📌 pinned` versus
`📌 pinned · cached`. The production client requests `room-pins-v1` only with
its persistent durable identity, and controls remain hidden unless the current
Link accepts it and current role/target authority permits the action.

Pin/unpin controls require current moderator/administrator role, joined-room
membership, retained target eligibility, exact-target authority, durable
identity, and both negotiated capabilities. Intent is persisted before send;
an ACK is presented only as accepted pending an authoritative room update.
That authoritative pin event is returned to the capable originating Link as
well as other capable joined Links; exact replay returns only its original
result.
Older, base-only, downgraded, or unsolicited peers cannot activate them.

An offline non-destructive schema-7 downgrade artifact can be created with
`omenchatd database export-schema7-copy --to <new-path> --confirm --home
<path>`. It removes only schema-8 usage metadata and preserves event sequences,
history, reactions, and dormant revisions.

A schema-6 downgrade artifact can be created with
`omenchatd database export-schema6-copy --to <new-path> --confirm --home
<path>`. It removes schema-8 usage and schema-7 event sequence metadata from a
private staged copy; history, reactions, and dormant revisions remain. An
older schema-6 binary can then allocate from the retained maximum because no
compaction is active.

A schema-5 downgrade artifact can be created with
`omenchatd database export-schema5-copy --to <new-path> --confirm --home
<path>`. A private staged copy drops only schema-6 revision objects, moves to
`user_version = 5`, and preserves reaction state. For a deeper schema-4
artifact, use
`omenchatd database export-schema4-copy --to <new-path> --confirm --home
<path>`. That copy drops both schema-6 revisions and schema-5 reactions before
moving to `user_version = 4`. Both commands require a new destination, pass
integrity/foreign-key checks, atomically publish, and leave the active schema-10
database unchanged.

## Server Commands

```bash
omenchatd init --home /tmp/omenchatd-test
omenchatd status --home /tmp/omenchatd-test
omenchatd doctor --home /tmp/omenchatd-test
omenchatd tui --home /tmp/omenchatd-test
omenchatd run --home /tmp/omenchatd-test
```

Add a TCP gateway:

```bash
omenchatd interfaces tcp-client <gateway-host:port> --home /tmp/omenchatd-test
```

That command adds a client without replacing other configured TCP clients or
listeners. Multiple enabled TCP clients are started independently. Inspect the
redacted list or delete one exact endpoint with:

```bash
omenchatd interfaces list --home /tmp/omenchatd-test
omenchatd interfaces delete tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-test
```

The generated configuration is bounded to 64 interface sections and 2 MiB.
Add/delete writes retain an owner-only `config.before-interface-edit.bak`
recovery copy, and take effect after the live server restarts.

For IFAC-protected gateways:

```bash
printf '%s\n' 'your passphrase' > /tmp/omenchatd-ifac-passphrase
chmod 600 /tmp/omenchatd-ifac-passphrase
omenchatd interfaces tcp-client <gateway-host:port> \
  --home /tmp/omenchatd-test \
  --network-name <network-name> \
  --passphrase-file /tmp/omenchatd-ifac-passphrase
```

Use `--passphrase-prompt` for a hidden terminal prompt or
`--passphrase-stdin` for a protected pipe. Direct `--passphrase` values are
deprecated because argv may be visible to other users and support tools.

## Browser Client

Open a chat server with:

```text
omenchat://<destination_hash>
```

The Directory also lists announced OMENchat servers when their announces are
seen. A live Reticulum announce now retains both the `omenchat.node` destination
hash and the announcing public identity hash. The selected-server panel labels
the latter `announce-verified`, meaning Reticulum authenticated the announce and
the destination/identity relationship; it does not mean the operator or user
has trusted that server. User-managed Saved/Trusted state remains separate.
Legacy directory records without identity metadata remain readable and show
that a fresh live announce is required. Request Path provides a deliberate,
rate-controlled discovery refresh without periodic UI polling.

In `omenchatd tui`, use **Announce Now** after the live server is running to
send the OMENchat and NomadNet portal announces immediately. This is useful for
testing discovery without waiting for the configured announce interval.

Canonical desktop and server products negotiate `room-slow-mode-v1` only
alongside durable mutations. A negotiated desktop session shows a static
`Slow mode · Ns` indicator; it does not run a countdown or treat elapsed
client time as permission to send. omenchatd atomically admits a new room
message/action with its durable event and replay result, while moderators
bypass the interval. Legacy and non-negotiating peers retain byte-exact
four-/five-field room values and prior behavior. The omenchatd CLI and TUI
report `enforcement active` in canonical server builds. The legacy root
Ratatui frontend does not host OMENchat sessions, so it has no duplicate
OMENchat policy state machine.

## Interface Recovery

`omenchatd` watches live Reticulum interface stats while the server is running.
If configured interfaces repeatedly report disconnected, or interface stats stop
responding, the server rebuilds its live Reticulum runtime and announces again.
This is intended to recover after a TCP gateway, private gateway, or local RNS
instance restarts without requiring an `omenchatd` process restart.

Check the TUI Monitoring panel or `~/.omenchatd/omenchatd.log` for lines that
include `interface watchdog` and `live runtime restarted after interface
watchdog`.

## Uploads And Media

- Default per-file upload limit: `512 KiB`.
- Default rotating per-user upload quota: `50 MiB`.
- Server admins can change limits in `omenchatd` config.
- NomadNet/Reticulum images can be loaded inline.
- Clearweb image loading is gated by media privacy settings and SOCKS5/Tor
  detection.
- SOCKS clearweb image bodies are streamed to a create-new same-directory
  temporary file with an 8 MiB cumulative cap. Oversized declared lengths fail
  before file creation; unknown/chunked lengths are checked before every write.
  Successful content is flushed, synchronized, and atomically renamed to its
  final cache name. Any response, write, overflow, flush, sync, or rename
  failure removes the temporary file, and an existing final file is preserved.
- SOCKS clients are reused through a four-entry LRU keyed by proxy endpoint and
  timeout. Initial and redirected URLs reject credentials, non-HTTP schemes,
  local/single-label/mDNS names, and private/link-local/special-use IP literals.
  Redirects are capped at five hops and may not downgrade HTTPS to HTTP.
- Non-image clearweb links open through the external browser prompt. Use Copy
  URL for Tor Browser and paste into the running Tor Browser window.

Upload quota changes are serialized per identity within the server process. A
replacement is written to an owner-only same-directory temporary file, flushed
and synchronized, renamed to its final path, and recorded in SQLite before any
older committed upload is evicted. A failed write or database record leaves the
older committed files intact. Failed post-commit eviction is conservative: the
server retains the extra old file rather than invalidating the new upload.
The new ledger row commits before any old file is removed. Old ledger rows are
deleted only for files whose physical eviction succeeded. A crash before file
eviction therefore over-counts quota; a crash after file eviction leaves a
missing row that restart reconciliation blocks. Neither boundary can
under-count physical storage.
Linux fault coverage also drives a real kernel `ENOSPC` from `/dev/full`
through the upload write path and proves the prior committed upload survives.

Animated GIF previews enforce an 8 MiB encoded limit, 4096-pixel dimension
limit, 128-frame limit, and 64 MiB decoded estimate before decoder admission.
Remote cached GIF reads and decoding run through two bounded blocking permits,
not the Iced update path. Decoded animations are retained in a deterministic
12-item/64 MiB cache; monitoring reports its item and decoded-byte totals.
Inbound upload-resource writes and accepted local-upload copies now use the
same worker boundary and return through typed Iced completion messages. Their
pending payload queue is capped at 16 jobs and 16 MiB reserved bytes; local
file jobs reserve the full 8 MiB per-file ceiling. Failed decode removes the
new cache file rather than retaining an untracked partial result.
Each job uniquely owns its queued byte buffer. After disk publication,
animated input moves into `iced_gif`; the worker no longer retains an
`Arc<[u8]>` and creates a second full `Vec<u8>` solely for decoder ownership.
Every queued cache job carries a monotonically increasing per-key generation.
Only the current session/key generation may update UI cache state. Replaced or
closed-session completions are ignored, and any file already produced by a
stale completion is removed asynchronously.
Each session-owned cache/decode job also carries a cooperative cancellation
token. Replacing a key cancels the previous generation, and closing a session
cancels all of its queued or active jobs. File admission, source loading, GIF
policy stages, decoder entry/return, pruning, and final publication check the
token; cancellation failures remove any partial output. The third-party GIF
decoder call itself is monolithic and cannot be interrupted mid-instruction.
Animated frame handles are supplied to the OMENchat view only while its pane is
actually visible in the Browser/Messages workspace. Tiled panes remain visible;
siblings hidden by pane maximization and every pane behind another top-level
section receive the static image fallback and submit no GIF animation widget.
The persisted desktop **Reduce motion** preference applies at this same frame
handle boundary. When enabled, even visible panes render the cached static GIF
image and do not construct an animated widget. Encoded media limits, cache
ownership, and wire behavior are unchanged.
Decoder regression coverage includes named empty, truncated, zero/oversized
dimension, excessive-frame, and malformed-LZW inputs plus 512 deterministic
mutations of a valid one-pixel GIF. The corpus is dependency-free, bounded to
tiny inputs, and exercises the complete production admission/panic boundary.
Animation is an explicit `chat-client-gif` capability. The canonical
`desktop-product` enables it; `desktop-product-static-media` excludes the
decoder and frame widget while retaining the same live networking and media
cache. In that profile GIF files remain ordinary static image previews and
never enter the animation decoder/cache.
The separate UI media-state cache is limited to 256 entries and 256 KiB of
URL/resource/path/status metadata. Deterministic oldest-entry eviction bounds
hostile or long-running status accumulation, and monitoring exposes the
retained metadata byte estimate. The identity-scoped on-disk `omenchat-media`
cache is capped at 64 regular files and 128 MiB. A serialized, bounded-memory
persistent index protects the just-committed file, retains the newest
admissible files, and removes evicted paths from UI state without enumerating
the directory on normal writes. A missing, malformed, internally inconsistent,
or path-unsafe index is rebuilt from regular files under the cache root; the
index never authorizes paths outside that root. Writers synchronize a dirty
marker before publication. If a process stops between file commit and index
commit, the next prune performs one repair scan, incorporates committed files,
removes abandoned `.tmp` files, saves the repaired index, and only then clears
the marker. Upload-resource, local-upload,
Reticulum-download, and SOCKS media writers all pass this boundary.
Buffered Reticulum and mock downloads use the shared same-directory atomic
writer: a create-new temporary file is written, flushed, synchronized, renamed,
and followed by a parent-directory sync on Unix. Buffered writes run behind two
blocking permits, so filesystem synchronization cannot create an unbounded
blocking-worker backlog. Existing final files are never deliberately replaced;
numbered download selection preserves prior content.
Live Reticulum resource payloads are single-consumer: decoding removes the
stored payload instead of cloning and retaining it. Unmatched completed
resources are capped at 8 MiB each and 16 items/16 MiB per link with oldest
eviction. Deferred resource-offer frames are capped at 32 items/4 MiB per link;
matching arrival releases their byte accounting. Monitoring exposes retained
items/bytes and rejected resource/offer totals, and overload is surfaced in the
session status.
The live client session engine independently bounds transfers waiting above the
link adapter. Outgoing upload offers retain at most four payloads/16 MiB, with
an 8 MiB per-payload cap. Inline downloads reserve at most 16 resources/16 MiB,
with an 8 MiB per-resource cap and at most 1,024 out-of-order fragments per
resource. Retained fragment bytes may never exceed the resource's declared
length; conflicting, overlapping-over-budget, inconsistent, and oversized
chunks abort that assembly and release its state. Closing or reconnecting a
session cancels all of its retained transfers. Monitoring shows current items,
reserved/retained bytes, fragments, and cumulative overload rejections.
The desktop link adapter also bounds its transient per-link transport queues.
Incoming and outgoing frames each retain at most 64 items/4 MiB; outbound
resources retain at most four items/16 MiB, with the existing 8 MiB per-resource
ceiling and a 4 KiB resource identifier cap. Consuming a frame or taking an
outbound batch releases its exact byte accounting. Incoming overload is dropped
before protocol dispatch and shown in session status; outbound overload returns
an error to the existing session engine. Monitoring shows current queue
items/bytes and rejection totals for all three directions.
The internal event channel remains bounded to 256 total events. OMENchat frame,
resource, and close payloads additionally share a cumulative 32 MiB permit.
The permit travels with the queued event across asynchronous waits and deferred
drains, and is released only when the receiver handles or rejects the event.
Byte-budget and item-channel overload are counted; both queue depth/bytes and
rejections are visible in OMENchat monitoring. After the channel wakes the
desktop, completed OMENchat events cross one short-lived global staging
boundary before session dispatch. That boundary retains at most 256 frames/16
MiB, 16 resources/32 MiB, and 256 close events/256 KiB of close reasons. Frame
and resource payload ownership is moved into staging rather than cloned. Drain
resets exact byte accounting; staging overload is counted separately.
Failed or cancelled inbound resource terminals use a separate 64-item/256 KiB
staging budget. On dispatch, the owning live link conservatively releases all
pending history/user-list offers because Reticulum's transfer hash is not yet
correlated to an OMENchat resource identifier. The healthy link stays open so
the user can retry history or reconnect, and monitoring exposes terminal depth,
bytes, and rejections. Link closure still drops the entire session transport
and all of its bounded pending state.
At the clean reticulum-rs bridge, explicit `omenchat-frame:` completions are
limited to the 1 MiB frame ceiling and `omenchat-resource:` completions to
8 MiB before an application event is created. Oversize completions produce a
failed lifecycle event and are not cloned into the desktop bus. The pinned
transport reports advertised totals during progress, but exposes cancellation
only for outbound resources, so it cannot yet stop an oversized inbound
receiver before `ResourceComplete.data` is assembled.

SQLite upload metadata now exposes a non-mutating reconciliation report per
identity: tracked/disk file and byte totals, missing tracked paths, untracked
orphans, and tracked paths outside the expected identity directory. Uncertain
files and rows are never deleted automatically. Admission uses this defensive
scan once before trusting an identity's indexed quota ledger.
`omenchatd doctor` now performs this inspection through a read-only bounded
database actor. Missing/orphan discrepancies produce a warning; a tracked path
outside its identity root is a failure. Doctor never repairs or deletes these
paths implicitly.

Explicit repair is available only as the offline operator command
`omenchatd uploads repair-ledger --confirm --home <path>`. It transactionally
removes database records for missing or out-of-root files, but never deletes
any file and deliberately preserves orphan files as recovery evidence. It
refuses missing or non-current schemas instead of creating or migrating the
database. The confirmed repair waits for the owned worker's final result rather
than timing out while a commit could still occur. Normal startup and `doctor`
remain non-repair paths.

Upload admission now performs one lazy reconciliation scan per identity and
server process. A discrepancy blocks uploads until the operator resolves it.
After a clean scan, quota totals and oldest-first eviction plans come from the
indexed SQLite ledger; normal offers and commits do not rescan the directory.
Recorded byte lengths must match regular-file metadata before the ledger is
trusted.

## Expected Client Behavior

- Project connection state is typed as disconnected, resolving, connecting,
  authenticating, joined, reconnecting, draining, or failed with explicit
  retryability. The desktop updates this state at path, link, handshake/join,
  close, retry, and session-removal ownership boundaries; UI and diagnostics do
  not infer it from free-form status text. The state table is bounded by the
  existing 64-session catalog and is not persisted as network truth. Restored
  sessions begin disconnected and reconcile from new live events.
- Offer a manual reconnect only while disconnected or after a retryable
  failure. Resolving, connecting, authenticating, joined, reconnecting,
  draining, and terminal-failure states do not expose a competing retry action;
  automatic recovery remains independently bounded.
- Reconnect when a live link drops.
- Retry dropped links with deterministic per-session jittered exponential
  backoff. Automatic attempts use 1, 2, 4, 8, and 16 second base delays with
  bounded +/-20% jitter, then pause after five attempts. A successful open
  preserves the attempt budget until the replacement link remains active for
  30 seconds; short-lived reconnects therefore continue the existing backoff
  instead of creating a rapid reconnect loop. Manual reconnect deliberately
  starts a fresh user-requested budget.
- Preserve recent history after restart.
- Sync recent room history on join/reconnect.
- Schedule live heartbeat, reconnect, and delayed recent-history maintenance
  from the nearest explicit deadline; idle desktops do not poll these paths.
- Keep local echo messages and retry failed sends.
- Label each locally queued room message or action as awaiting server
  acceptance until its correlated `MessageAck` replaces the temporary event
  identifier. The existing delayed resend action remains available if that
  acknowledgement never arrives.
- Retain at most 64 unacknowledged local-echo correlations per session and 256
  across the client. Saturation rejects before sending a frame, keeps the draft
  through the normal error path, and asks the user to wait for acceptance or
  reconnect. Acknowledgement, reconnect, and session close release entries;
  monitoring reports current and rejected counts.
- Offer a copyable, redacted JSON diagnostics report from every OMENchat pane.
  The report is capped at 8 KiB and contains typed connection state, the public
  server destination and announce-verified identity, bounded queue/resource
  counters, link identifiers, and transport counters. It deliberately omits
  message bodies, composer drafts, user lists, room names, filenames, local
  paths, credentials, private identity material, and all free-form status/error
  text. Disconnect detail is reduced to a fixed non-secret category.
- Show recovered durable mutations as a compact, non-error notice by default,
  explicitly separate current connection health from an earlier uncertain send,
  and reveal the bounded four-row-per-server review panel only on request. The
  review panel contains no
  mutation IDs, request hashes, message bodies, or command targets. Each row
  identifies the operation kind, public server, room scope, state, and relative
  expiry. Send/Retry appears only when the production identity, client-instance,
  original-room, live-transport, capability, expiry, and pending-result guard
  passes. Otherwise the panel explains why retry is unavailable and offers only
  explicit local stop-tracking. Nothing is resent automatically.
- Show byte progress for the newest active inbound OMENchat Resource in the
  matching session pane. Attribution requires the typed runtime source,
  inbound direction, and exact active link identity to agree; another session's
  transfer is never shown. The current Reticulum API does not expose a verified
  mapping between its transfer hash and the OMENchat history/user-list offer
  identifier, so the pane truthfully labels the payload as potentially history,
  users, or media instead of claiming a more specific purpose.
- Show unread state when chat panes are minimized.
- Open and restored chat panes at the newest event. A pane that is already
  following the newest event remains bottom-anchored while an attachment or
  media preview changes the timeline height. Manual scrollback disables that
  follow behavior until the user returns to the bottom or selects **Jump To
  Present**; loading an attachment must not interrupt history reading.

Frame `seq` values correlate live requests and responses. omenchatd treats an
exact same-link replay of a room message, action, notice, part, or mutating
command as the same logical operation: it returns the retained origin response
without repeating storage, rate accounting, peer/user-list fan-out, or a
moderation disconnect. Read-only commands remain uncached. A
same-sequence/different-content collision is rejected. Part and kick/ban link
effects occur only after a successful typed command result, so rejection or
rate limiting cannot part or disconnect a peer. This protection is deliberately
link-scoped because protocol v1 has no persisted client-session nonce;
cross-link or server-restart mutation retry remains an explicit compatibility
gap rather than an unsafe global identity/sequence assumption.

Client sequence allocation is scoped to the live session/link. Each new link
starts at nonzero sequence `1`; the session-open and initial-join pair is
reserved atomically. The client never wraps on an active link because
omenchatd's bounded replay cache may still retain an older mutation with the
same numeric sequence. Exhaustion therefore rejects before frame construction
and asks for reconnect. Only the existing link-retirement boundary clears the
allocator and pending `(session, sequence)` correlations; cancelling an
individual transfer does not reset sequence ownership. Independent links may
use the same numeric sequence without cross-session acknowledgement or upload
correlation.

An active client session owns one registered Reticulum link. Room frames,
heartbeats, history requests, and resource transfers reuse that link; they do
not open a link per operation. An explicit reconnect is a new ownership
generation. Starting a newer generation cancels the prior open task, and clean
Reticulum opens are serialized through 32 fixed destination stripes. Before a
new explicit open calls Reticulum 0.9 `Transport::link()`, it retires any
tracked clean link for that destination. This is required because Reticulum
0.9 otherwise returns the same non-closed outbound link, allowing a stale
completion to close the newer attempt's handle. Other destination stripes stay
parallel. Cancellation after link allocation closes and resets the pending
upstream handle before releasing the stripe, and session close removes and
cancels the bounded per-session owner.
These lifecycle rules do not change OMENchat frames, resources, identity
binding, destination names, or reconnect timing.

History and user-list resource offers are bound to their eventual payloads.
The client rejects an invalid purpose or advertised size before consuming
pending-offer capacity, then requires the payload's compression and compressed/
uncompressed lengths to match exactly before decoding. A failed integrity check
cannot update room history or user state, and the consumed payload is not left
in the session resource cache.

Each open client session retains at most 1,024 history events and an estimated
8 MiB of owned event/string storage. Initial restore and new live events keep
the recent edge; **Load Older** keeps the older edge so pagination continues to
move backward. Eviction changes only the in-memory view. Received history is
written to the SQLite store before the retained window is persisted, including
rows from a batch that do not fit in memory, and can be paged again later. The
byte estimate uses owned string capacities plus event storage rather than wire
length alone. These are local cache limits and do not change event IDs, frames,
history page sizes, or deduplication keys.

Mixed application-store compatibility is exercised independently of the live
transport. Hardened `0.6.0-1` seeds an isolated `chat.sqlite`; `0.9.6-2`
reopens it and appends; `0.6.0-1` reopens that current write and appends; and
`0.9.6-2` performs the final reopen. Server metadata, room metadata, active
room, ordered event identifiers, and event content must remain exact at every
stage. This proves bidirectional store-format reopening only. It does not yet
prove a history Resource transfer.

Live mixed-version compatibility is gated separately in both directions. The
current `0.9.6-2` desktop client connects to the immutable hardened `0.6.0-1`
standalone server, and the hardened old client separately connects to the
current standalone server. Each isolated ephemeral-loopback case opens its
link and OMENchat session, joins a room, sends one message, and observes the
echoed room event. The harness retains only public versions and validation
booleans. Automatic reconnect remains pending outside this single-exchange
matrix.

A separate mixed restart case covers the current client and hardened old
server. After one complete exchange, the old server stops within a bounded
SIGTERM deadline, reopens the same server home with the same destination, and
the current client reopens its original application root in a fresh process.
The second link/session/join/message/echo exchange passes. Because the old
server predates the owned SIGTERM drain path, this is bounded process-restart
and state-reopen evidence, not orderly old-server drain or automatic reconnect
inside a continuously running desktop. The reciprocal case also passes: the
old client reopens its original root after current omenchatd performs its owned
orderly SIGTERM drain and restarts with the same destination. Automatic
reconnect inside a continuously running desktop remains pending.

A separate current-product harness now keeps one OMENbrowser process alive
while current omenchatd performs its owned orderly shutdown and restarts with
the same destination. The client observes the old link close, opens a different
link, reconnects the same in-memory session, and receives a post-restart message
echo. This closes the headless product-process reconnect boundary. An
interactive Iced-window restart soak remains separate evidence.

Current client/server upload qualification also passes with two isolated
clients. The first client uploads and fetches a deterministic 873-byte fixture;
the second discovers and fetches the same server-held Resource. Both decoded
Resource events report the exact byte count. Raw payloads and identifiers are
discarded, and this does not change upload quotas or the OMENchat protocol.

The current client also passes a live history-Resource case against the
hardened old server. The isolated server threshold is set to one byte so a
normal small message uses its production Resource history path. A second client
with a distinct root receives `resource_data`, decodes `history_prepended`
inside that Resource event, and observes the exact first-client message. This
does not change the production threshold. The reciprocal case also passes: the
hardened old client decodes the current server's Resource-backed history and
observes the exact first-client message under the same isolated threshold.

The client admits at most 64 simultaneously retained server sessions. Opening
another session is refused with a visible error; an existing session is never
silently evicted. Each retained session's room catalog is limited to 256 items
and an estimated 512 KiB of owned storage, while its active-room user catalog is
limited to 1,024 items and an estimated 1 MiB. Active, joined, and unread rooms
are preferred when a room snapshot must be reduced. Oversized live snapshots
are truncated deterministically and reported in session status. SQLite restore
applies the same item and byte admission before materializing catalogs, prefers
the active then joined rooms, and leaves every non-resident row on disk. These
limits are local view policy only; they do not alter OMENchat frames or server
quotas.

Retained presentation metadata has smaller semantic limits than the general
MessagePack scalar ceiling. Server display names, user/actor display names, and
room names are limited to 256, 256, and 64 UTF-8 bytes respectively; room topics
to 4 KiB; MOTDs to 16 KiB; and session status/error text to 4 KiB. Resource IDs
remain exact and are rejected above 4 KiB; upload filenames and content types
are limited to 4 KiB and 1 KiB. Display-only text is shortened on a UTF-8
boundary with an ellipsis. Oversized operational room/user names are rejected
rather than renamed, so later join/moderation commands cannot target a truncated
identifier. SQLite restore applies byte-length predicates before materializing
these rows. These are client retention limits, not new server-side validation or
wire fields.

Declarative `[omenchat]` descriptors are separately capped at 64 KiB, 128
lines, and 32 KiB per line before recognized values are retained. Room hints
and capabilities each allow at most 64 entries; room hints use the 64-byte room
limit, while capabilities allow 128 bytes each and 8 KiB total. Destination,
LXMF, descriptor-path, theme, and signature fields are exact and rejected above
their respective limits; display names use the same UTF-8-safe 256-byte
shortener as session metadata. Lowering does not join a descriptor exceeding
the block budget. Micron OMENchat links admit at most 32 fields/16 KiB and apply
the same recognized-field rules atomically before opening a session.

## Server Backpressure

The live Reticulum server uses bounded payload and control queues. Transport
commands are limited to 256 payload items and 16 MiB globally, with a 4 MiB
per-link payload share. Inbound live events are limited to 512 payload items and
32 MiB globally, with an 8 MiB per-link share. Small separate control lanes keep
link-open/link-close work responsive while payload queues are saturated.

Outbound overload is returned to the session engine as an explicit error rather
than retaining unlimited payloads. Inbound payloads that cannot be admitted are
dropped at the bridge and logged with `action=drop`; lifecycle control events
wait up to two seconds for their bounded lane. Monitoring shows queue item
count, queued bytes, actual oldest remaining item age, and cumulative rejects.
Reservation identities advance the oldest timestamp whenever the prior oldest
item is consumed or cancelled; a continuously non-empty queue therefore reports
queue dwell time rather than total overload duration.

The explicit release-mode backpressure soak is:

```sh
scripts/measure-omenchatd-backpressure.sh /tmp/omenchatd-backpressure-results
```

It drives the production transport-command and inbound-event queue types with
64 KiB resources at a 1 ms producer interval and 20 ms consumer interval for 60
seconds. Eight link identities share each budget while repeated link-close
controls probe the priority lanes. The Linux harness records queue items/bytes,
oldest age, rejects, RSS, file descriptors, control latency, and final permit
release under an isolated temporary root. This is queue/backpressure evidence;
Reticulum/LXMF wire interoperability remains a separate live smoke gate.

Outbound resource queueing transfers its uniquely owned payload and metadata
buffers directly into reticulum-rs. The server records the payload length
before that move for logging, avoiding the former full resource clone. These
buffers remain `Vec<u8>` at this boundary because reticulum-rs consumes that
type; shared immutable storage should be introduced only at actual fan-out
boundaries.
The same monitoring line reports live database-worker in-flight, completed,
rejected, average-latency, and maximum-latency counters.

The release-mode database contention gate is:

```sh
scripts/measure-omenchatd-db.sh /tmp/omenchatd-db-results
```

It drives the production session engine, live blocking-worker boundary, frame
decoder, and persistent SQLite store for 60 seconds under an isolated temporary
root. It records explicit busy rejection, worker and independent Tokio
heartbeat latency, RSS, file descriptors, database size, restart persistence,
consecutive event IDs, and `integrity_check`. The 2026-07-13 reference run
committed 6,000 events while rejecting 42,000 concurrent submissions; maximum
worker/heartbeat latency was 1,272/1,817 us, RSS grew 794,624 bytes, and file
descriptors stayed at 13. A discard-only test transport keeps wire networking
out of this database measurement, so the live Reticulum smoke remains a
separate qualification gate.

Live Reticulum logs use one background buffered writer rather than opening and
writing the file on callback paths. Admission is capped at 1,024 records, 1 MiB
queued bytes, and 16 KiB per record. Monitoring reports queued items/bytes,
oldest age, overload drops, and file-write failures. The writer flushes at most
250 ms after activity and supports an explicit bounded flush. Rotation caps
each file at 8 MiB and keeps three backups (`omenchatd.log.1` through `.3`),
bounding normal retained log storage to approximately 32 MiB. Priority sampling
uses a reserved lane: call sites submit routine records as typed `Info` and
operational failures/overload/lag/malformed requests as typed `Warning` or
`Error`. Priority records receive 128 items/256 KiB and are drained first;
message words never select the lane. Monitoring reports priority depth and
drops separately. Severity controls admission only, preserving the existing
timestamp/text file format.
`scripts/measure-omenchatd-logging.sh` runs the optimized repeated-lifecycle
slow-writer qualification with real isolated rotating files and a deterministic
2 ms delay at the write boundary. The 2026-07-13 reference run kept callback
admission at 1,778 ns p95 under sustained explicit overload, lost no priority
records, reported no write failures, held FDs flat, rotated all three
lifecycles, and stayed within each writer's 32 MiB retention cap. Against the
identical former text-classifier run, median/p95 admission fell 95.1%/86.6%.
This is a repeatable slow-disk simulation, not Reticulum wire evidence.
