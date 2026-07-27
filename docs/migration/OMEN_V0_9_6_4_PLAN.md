# OMENbrowser and omenchatd v0.9.6-4 plan

Baseline: published tag `v0.9.6-3`, commit
`414d8eafd1a845a986032bad993ac9c09cc378e4`  
Target: `v0.9.6-4`  
Reticulum/LXMF train: exact `0.9.6` unless a separate reviewed migration is
approved

## Objective

Finish the practical deferred work from the Reticulum 0.9.5 review without
rewriting the application, replacing primary frameworks, changing identity or
storage ownership, or implementing the experimental shared Reticulum runtime.

This revision should:

- reduce the remaining root application-module concentration;
- extend negotiated OMENchat durable mutation safety beyond room text;
- give GUI and TUI one truthful bounded Operations/Transfers model;
- improve delivery, propagation, GUI, and TUI controls;
- add bounded OMENchat replies, mentions, reactions, local search, and safe
  invitations only after their mutations have durable identities;
- repeat relevant resource, interoperability, native, and packaging evidence;
- preserve managed Reticulum as the supported runtime.

## Explicit exclusions

- No external/shared Reticulum runtime implementation or promotion.
- No Reticulum/LXMF dependency upgrade.
- No private upstream fork or `[patch.crates-io]`.
- No remote daemon administration.
- No voice/video, presence heartbeat, or constant typing traffic.
- No unrelated Iced redesign or generic event/dependency-injection framework.
- No claim of hardware/public-network support without corresponding evidence.

Existing `reticulum_instance_mode = "external"` remains a readable,
fail-closed deferred configuration. Work in this plan must not weaken that
gate.

## Current source-of-truth snapshot

- Root and standalone server packages are released as `0.9.6-3`.
- Production Reticulum/LXMF dependencies are exact registry `0.9.6`.
- `src/app.rs` is approximately 36,337 lines. The diagnostics-result reducer
  is extracted, but major smoke/interop, Network Doctor, log-state, and
  application orchestration regions remain concentrated there.
- `durable-mutations-v1` is capability-negotiated and active for room messages,
  `/me` room actions, `/notice` room notices, `/part` room leaves, `/topic`
  room metadata updates, `/create` room creation, `/role` role changes, and
  `/unban`, `/kick`, `/ban`, `/mute`, and `/unmute` status/moderation changes.
  The persistent intent contract admits `RoomMessage`, `RoomAction`,
  `RoomNotice`, `PartRoom`, and `Command`, while the desktop production sender
  prepares those text/leave families plus the narrowly admitted `topic`,
  `create`, `role`, `unban`, `kick`, `ban`, `mute`, and `unmute` command
  subsets.
- The server durable executor and replay store already cover room-text and
  command mutation families with bounded transactional replay behavior.
- Network Doctor and delivery evidence provide useful component state. A
  project-owned bounded Operations/Transfers history now exists in shared
  application state and is seeded from OMENchat durable-mutation recovery, but
  general runtime adapters and GUI/TUI surfaces remain incomplete.
- Propagation-node evidence, stamp/ticket boundaries, reduced motion, native
  packaging, bounded queues/caches, and managed runtime ownership already
  exist and should be extended rather than replaced.

## Sequencing and commit policy

Each unit must be independently buildable and reviewable. Do not combine
application decomposition, protocol activation, new OMENchat features, and
release packaging in one patch.

Use focused local validation during development. Keep the branch private from
PR-triggered CI until a coherent checkpoint is locally green. Open one
qualification PR near the end, run the native CI matrix once on the stable
candidate, run the long Python/mixed-version workflow once when protocol and
interop behavior are frozen, and run packaging only from the release tag.

Do not bump package versions until the behavioral units and compatibility
scope are stable. Root and server versions must move together to `0.9.6-4`.

## Phase 0 — close v0.9.6-3 evidence

### Work

- Mark the completed native-CI, packaging, checksum, and clean-candidate gates
  in `V0_9_6_3_RELEASE_CHECKLIST.md`.
- Update the older maintenance ledger only where later hosted evidence
  conclusively changes a status. Do not edit review artifacts under
  `official-sources/`.
- Record `v0.9.6-3` as the immutable rollback baseline.
- Remove local release-verification downloads outside the repository when the
  host permits it.

### Gate

- Documentation matches published run `30042355552`, PR `#17`, tag
  `v0.9.6-3`, and commit `414d8ea`.
- No runtime, protocol, dependency, schema, or configuration change.

## Phase 1 — mechanical application decomposition

Move one cohesive region at a time into private children of `app`. Preserve
types, callers, serialization, task ownership, and behavior.

### Unit 1A — network smoke and readiness helpers

Extract the pure formatting, classification, probe, and readiness helpers that
currently occupy the early `src/app.rs` network-smoke region. Keep runtime
calls behind the existing adapter/facade.

Status: complete. `app::network_smoke` now owns probe-summary construction,
failure-stage classification, queued-path retry guidance, trace formatting,
and announce matching. `src/app.rs` fell from 36,337 to 35,976 lines; the
424-line child includes two module-local regression tests. Existing types,
callers, runtime tasks, state ownership, and serialized output are unchanged.

Validation:

- focused pre-extraction probe/announce/load-failure matrix: pass;
- focused post-extraction matrix plus two module tests: pass;
- full `desktop-product` test profile: pass (1,267 library tests passed and 29
  explicit measurement/live cases ignored, plus all integration suites);
- `desktop-product` all-target Clippy with `-D warnings`: pass;
- TUI check, formatting, and `git diff --check`: pass.

### Unit 1B — Network Doctor state reduction

Extract Network Doctor path/link/resource/LXMF/OMENchat event reduction and
bounded history management. This is a mechanical move, not the new Operations
model.

Status: complete. `app::network_doctor` now owns the typed path, link,
Resource, LXMF, and active-Resource rows plus their bounded, duplicate-
suppressing state reducer. The existing public paths remain available through
`app` re-exports, while runtime-event projection and state ownership remain in
the parent. `src/app.rs` fell from 35,976 to 35,416 lines; the 631-line child
adds focused tests for the 12-row history bound and preservation of Resource
offer context across progress updates. No worker, timer, queue, protocol,
storage, serialization, dependency, or server behavior changed.

Validation:

- focused pre-extraction Network Doctor matrix: pass (16 tests);
- focused post-extraction matrix: pass (18 tests, including two module-local
  invariants);
- full `desktop-product` test profile: pass (1,298 library tests plus all
  selected integration suites; documented measurement harnesses remain
  ignored);
- `desktop-product` all-target Clippy with `-D warnings`: pass;
- TUI check, formatting, and `git diff --check`: pass.

### Unit 1C — structured application-log state

Extract in-memory filtering, byte accounting, persistence submission, and
worker-metric projection without changing the existing writer or queue.

Status: complete. `app::log_state` now owns `LogSeverity`, `LogSource`,
`LogEntry`, `LogBuffer`, the item/byte/message/startup-scan bounds, filtering,
persistence submission, flush access, and worker/disk metric aggregation.
Existing `app::Log*` paths remain stable through re-exports. The existing
bounded `StructuredLogWorker` remains the sole writer and retains its original
ownership, queue, rotation, shutdown, and failure behavior. `src/app.rs` fell
from 35,416 to 35,006 lines; the 451-line child owns the moved memory-bound
test and a focused stable-filter-order test. No log format, path, retention
default, queue, worker, protocol, configuration, dependency, or server
behavior changed.

Validation:

- focused pre-extraction structured-log pipeline matrix: pass (15 tests);
- focused post-extraction pipeline matrix: pass (15 tests);
- module-local state tests: pass (2 tests);
- full `desktop-product` test profile: pass (1,299 library tests plus all
  selected integration suites; documented measurement harnesses remain
  ignored);
- `desktop-product` all-target Clippy with `-D warnings`: pass;
- TUI check, formatting, and `git diff --check`: pass.

### Unit 1D — reassess

Measure the resulting file/module sizes and dependency edges. Select another
cohesive reducer only if the preceding extractions are clean. Do not attempt to
split all of `App` in this release.

Status: complete; Phase 1 is closed. The parent fell from 36,337 lines at
`v0.9.6-3` to 35,006 lines, a reduction of 1,331 lines (3.7%). The resulting
private children and their direct parent dependencies are:

| Module | Lines | Parent dependency |
| --- | ---: | --- |
| `diagnostics_results` | 460 | pre-existing, tightly coupled `impl App` using `super::*` |
| `network_smoke` | 424 | `BrowserProbeSummary` and `DirectoryKind` |
| `network_doctor` | 631 | three pure formatting/time helpers |
| `log_state` | 451 | the epoch clock helper |

No cyclic ownership boundary, worker, task, timer, queue, cache, or dependency
was introduced. Public paths required by existing callers remain stable.

No fourth extraction is selected for this phase. The remaining obvious
message-task and browser-task result handlers are approximately 379 and 359
lines and touch 20 and 15 distinct `App` fields/helper surfaces respectively.
Moving either whole `impl App` block into another `super::*` child would reduce
the parent line count without reducing state coupling. Separating those
handlers properly requires project-owned operation state and narrower
ownership boundaries, which belong to Phase 2/3 behavior work and must not be
mixed into this mechanical phase.

After the later reaction/revision qualification work made the root binary's
live-smoke region materially larger, that binary-only region was mechanically
extracted to `src/omenchat_smoke.rs`. The private child owns the bounded Link,
Resource, reconnect, upload, reaction, and revision smoke orchestration and its
report formatting; `src/main.rs` retains argument parsing and shared runtime
configuration helpers. The implementation was compared byte-for-byte after
accounting for the private entry-point rename and relocated isolated marker
test. Full desktop-product tests, strict Clippy, formatting, and the
mock/no-feature check pass. This does not reopen `src/app.rs` decomposition or
change runtime, protocol, storage, or CLI behavior. The extraction record is
`docs/audits/omenchat-smoke-module-extraction.md`.

Phase 1 validation is satisfied by the focused before/after matrices and the
full product, TUI, formatting, and strict-Clippy gates recorded in Units
1A–1C. This documentation-only reassessment adds no executable change and does
not repeat the expensive product matrix.

### Tests and gate

- Add or move focused reducer tests with each extraction.
- Run before/after focused tests, product tests, TUI check, formatting, and
  product Clippy with `-D warnings`.
- No new worker, timer, queue, cache, dependency, protocol, or storage change.
- `src/app.rs` is materially easier to navigate and no cyclic module boundary
  is introduced.

Rollback each extraction by restoring that region and its module-local tests
to the parent.

## Phase 2 — complete durable mutation activation

Extend the existing `durable-mutations-v1` path one operation family at a time.
Do not automatically resend uncertain operations.

### Unit 2A — room actions and notices

Status: complete. Negotiated `/me` room actions and `/notice` room notices now
persist through the existing bounded intent owner, become uncertain before
transport, use the canonical durable envelope, correlate `MessageAck`, survive
restart as visible but never automatically transmitted intents, and reuse their
mutation identity only through the existing explicit retry path.

Durable notices require the additive `durable-room-notice-ack-v1` capability
and use the protocol's existing notice kind in `MessageAck`. Older, ordinary,
and downgraded protocol-v1 notices retain their prior `RoomEvent` response and
legacy send path. Replacement-Link, client-restart, server-restart, exact
replay, content-conflict, capability-downgrade, and no-repeat fan-out coverage
is deterministic and isolated.

- Persist intent before transmission.
- Use stable client/mutation IDs and canonical request hash.
- Transition to uncertain before transport.
- Reuse the same mutation identity only through explicit retry.
- Correlate acknowledgement on the current Link sequence.

### Unit 2B — part-room

Completed on the `v0.9.6-4` branch:

- Negotiated `/part` persists a `PartRoom` intent with an empty canonical body
  before transport and transitions it to uncertain before sending.
- The live client keeps the current local membership unchanged until an exact
  sequence, room, and `part` `CommandResult` match is received.
- A missing response remains visibly uncertain and is never retried
  automatically. Explicit recovery retry accepts the original room from the
  same server catalog even when it is no longer the active room.
- Part correlation shares the existing bounded pending-mutation item budgets;
  it adds no worker, timer, queue, cache, schema, protocol number, or
  capability.
- Client restart fixtures recover uncertain PartRoom intents without
  transmitting. Existing server Link-replacement and restart regressions prove
  exact-result replay without repeating membership deletion, leave events,
  rate accounting, or fan-out.

### Unit 2C — commands

Activate only server commands already supported by the transactional durable
executor. Classify each command as:

- durable and replay-safe;
- read-only and not requiring mutation identity;
- legacy/deferred because an external effect is not transactional.

Do not send an unsupported command in a durable envelope. Keep exact legacy
behavior for old or downgraded peers.

Command classification and first subunit:

| Command | Classification | v0.9.6-4 desktop state |
| --- | --- | --- |
| `rooms` | read-only; mutation identity is unnecessary | legacy/read-only |
| `topic` | transactionally durable and replay-safe | activated |
| `create` | transactionally durable and replay-safe | activated |
| `role`, `unban` | transactionally durable and replay-safe | activated for catalog-known numeric-ID or exact-display targets |
| `kick`, `ban`, `mute`, `unmute` | durable database result plus one-use live target effects | activated for catalog-known numeric-ID or exact-display targets |

The topic subunit persists the exact normalized command before transport,
changes no local metadata until an exact sequence/room/command/returned-room
result arrives, and never retries after silence or disconnect. It shares the
existing bounded pending-mutation budget and base `durable-mutations-v1`
capability. Client restart recovery is visible but non-transmitting. Server
tests cover content conflict, replacement-Link replay, restart replay, and
one-use `RoomDelta` publication. No schema, protocol number, capability,
dependency, worker, timer, or queue changed.

The create subunit persists a roomless canonical `create` command before
transport and adds no room locally until an exact sequence, `room_id = None`,
command tag, and server-normalized requested room name are returned. A
mismatched new-room identity leaves the intent uncertain. Explicit retry binds
to the original server rather than an unrelated active room. Client and server
restart fixtures remain non-transmitting or exact-replay-only; replacement-Link
replay returns the original result and publishes no second `RoomDelta`.
Invalid names that normalize to empty are rejected before persistence. This
subunit reuses the existing capability, intent schema, worker, and bounded
pending-correlation budgets.

The role/unban subunit persists canonical role labels and target strings before
transport. It accepts a result only when sequence, room, command, catalog-known
numeric user ID or exact display name, and requested role/cleared-ban state all
match.
Identity-prefix-only targeting cannot be proven from the existing result shape,
which deliberately omits identity hashes, so those commands retain the legacy
path. Explicit retry preserves the original server and room audit scope.
Replacement-Link and server-restart tests prove that the exact result replays
without a second user mutation, audit event, rate charge, `UserDelta`, or
`RoomEvent`. No wire, schema, capability, dependency, worker, queue, or timer
changed.

The active-peer moderation subunit uses the same persisted target/result
correlation for `kick`, `ban`, `mute`, and `unmute`. Ban/mute state must match
the requested result; kick has no stored status bit and removes only the exact
correlated user after acknowledgement. The server's first execution captures
the target identity for a one-use kick/ban disconnect. Exact replay emits no
disconnect, broadcast, mutation, audit event, or rate admission, so a
replacement Link cannot be disconnected by an old result. Identity-prefix-only
targets retain the legacy path. Recovery remains visible and requires explicit
retry; it never automatically resends an uncertain moderation operation.

### Unit 2D — recovery UX

Use the existing bounded intent owner and recovered-intent UI for all newly
activated kinds. Show operation kind, room/server, state, expiry, and allowed
action without exposing mutation IDs or message bodies in logs.

Completed locally: the four-row-per-server panel now classifies every activated
operation without rendering its mutation ID, request hash, message body, or
command target. It shows the public server label, resolved room when available,
prepared/uncertain state, and bounded relative expiry. The exact production
retry guard controls whether Send/Retry is rendered; unavailable retry shows
its redacted reason and leaves only explicit Stop Tracking. Expired rows retain
only Finalize Expired. The pane now starts with a compact non-error notice and
shows the full warning/recovery controls only after explicit Review. It reports
the current connection state separately because a successful join or ping does
not establish whether an older mutation committed. Collapsing review clears a
pending UI confirmation but does not discard the durable record. Confirmation
and the bounded persistence owner are otherwise unchanged, and no automatic
retry, worker, queue, timer, schema, wire, or capability was added.

The compact recovery notice and explicit Review flow were manually smoke-tested
in a live joined OMENchat session on 2026-07-25. Join and ping health remained
visible and normal, and the earlier uncertain send no longer presented as a
connection-failure dialog.

### Required matrix for every newly activated family

- response lost after server commit;
- Link closes after commit;
- explicit retry on replacement Link;
- client restart;
- server restart;
- exact duplicate;
- same mutation ID with different content;
- concurrent duplicate;
- replay expiry/retired client instance;
- persistence-worker overload and shutdown;
- legacy client/current server;
- current client/legacy server;
- no automatic retry after silence or disconnect.

### Gate

- Every mutating operation advertised as durable has transactional server
  replay and persistent client intent coverage.
- Exact replay returns the original semantic result without repeating effects.
- Conflict and expiry remain terminal only when correctly correlated.
- Protocol v1 and capability negotiation remain backward compatible.
- Storage migrations retain guarded backup/recovery and standalone-server
  independence.

Rollback is per operation family by disabling durable selection while retaining
stored intents/replay rows for operator reconciliation.

## Phase 3 — unified Operations/Transfers domain

Create a small project-owned module, not a new generic framework, and feed both
GUI and TUI from it.

Phase 3A starts with `src/operations.rs`: a frontend-neutral vocabulary and
bounded in-memory history. It distinguishes queue, dispatch, transport,
receipt, delivery, progress, terminal, gap, and reconciliation evidence;
coalesces updates by stable project operation ID; evicts only terminal history;
and rejects admission rather than dropping unresolved work. Production event
adapters and GUI/TUI surfaces remain separate follow-up units. This model-only
unit adds no worker, timer, subscription, persistence, protocol, or dependency.

Phase 3B begins with the read-only OMENchat recovered-mutation adapter. It
projects the existing persistent `Prepared` and `SentUncertain` rows without
retaining bodies, hashes, correlation IDs, or identity material in
presentation text. Prepared rows claim persistence but no transmission;
uncertain rows remain nonterminal without fabricated transport or delivery
evidence; elapsed expiry remains unresolved until explicit finalization. The
existing retry guard controls whether explicit send/retry is a valid action,
and the current desktop recovery card consumes that shared decision. This
subunit changes no persistence transition, automatic retry behavior, wire,
worker, queue, timer, or dependency. Shared history ownership and the TUI
surface remain separate follow-ups.

Phase 3C places the bounded owner at `App::operation_history` and atomically
reconciles the OMENchat-mutation domain when durable restart recovery completes.
Other domains survive replacement; duplicate, mixed-domain, or saturated
snapshots leave the previous history intact; and terminal persistence outcomes
remove the exact opaque operation and byte budget. Projection rejection is
visible but never mutates the authoritative intent store. Synchronization is
limited to recovery and persisted transition boundaries, with no timer,
polling, worker, subscription, schema, protocol, or dependency. The owner keeps
transmission actions conservative because live capability/connection state is
not yet an owned Operations-domain input. GUI/TUI surfaces and broader runtime
adapters remain separate follow-ups.

Phase 3D adds the frontend-neutral read-only presentation projection. It caps
output at 128 rows, reports omitted matches, provides deterministic
attention-first sorting and bounded filtering/search, sanitizes controls,
UTF-8 truncates target/evidence text, keeps opaque IDs only as non-display
selection keys, and preserves typed authoritative progress and valid actions.
Shared labels keep queue, transport acceptance, receipt observation, and peer
delivery distinct. No GUI/TUI section, cache, retained clone, worker, timer,
subscription, persistence, protocol, or dependency is introduced. The two
frontends must consume this projection in later surface units.

Phase 3E adds the first minimal desktop consumer as a passive
`Operations & Transfers` card in Network Doctor. It requests at most eight
shared rows, reports retained/omitted counts and retained bytes, renders opaque
IDs nowhere, and displays typed progress only with authoritative evidence. The
card has no controls and adds no workspace route, saved setting, cache, worker,
timer, subscription, protocol, storage, or dependency. Keeping it inside the
existing Network Doctor avoids a desktop-only section while the corresponding
TUI surface and broader event adapters are still pending.

Phase 3F replaces the existing TUI Network Doctor placeholder with a passive
view over the same shared projection. It uses the same eight-row bound,
attention ordering, evidence terminology, opaque-ID omission, and
authoritative-progress rule as the desktop card. The existing route, mouse
behavior, keyboard behavior, and saved section preference remain unchanged.
The view adds no action, state machine, update loop, worker, timer,
subscription, persistence, protocol, storage, or dependency. Interactive
filter/action controls and broader runtime-domain adapters remain pending.

Phase 3G adds the first typed runtime-domain adapter for Reticulum Resource
progress and lifecycle events. A stable opaque key coalesces offers and
progress without retaining the raw transfer or browser-correlation identifier.
Typed valid totals remain exact; regressions and malformed totals preserve the
last valid record. Local Resource completion uses a new terminal `Completed`
state and `resource completion` evidence so it cannot be confused with peer
message delivery. Failure and cancellation remain distinct, terminal events
win over late progress, and history saturation leaves unresolved work intact
with a visible warning. The existing runtime, Network Doctor, browser transfer
correlation, and transport behavior are unchanged. No worker, timer,
subscription, queue, cache, persistence, protocol, action, or dependency is
introduced.

Phase 3H adds a narrow adapter for typed Reticulum `PathUpdated` observations.
Normalized destinations correlate to opaque project keys; known paths are
locally `Completed`, unknown paths remain unresolved `Waiting`, and neither
state is message delivery or a fabricated request failure. Hop evidence appears
only when supplied by the event. Repeated observations coalesce, route loss can
reopen a completed record, stale timestamps are ignored, and saturation
preserves unresolved work. The typed event does not identify request
initiation, failure, timeout, or reason, so those states remain explicitly
outside this unit rather than being parsed from logs. Existing path requests,
warmups, browser retries, Network Doctor behavior, and runtime transport remain
unchanged. No worker, timer, subscription, queue, persistence, protocol,
action, or dependency is introduced.

Phase 3I projects the existing typed OMENchat `ChatConnectionState` reducer
into one shared Operations record per session. Disconnected/resolving remain
`Waiting`; connecting/authenticating/joined/draining are `Active`;
reconnecting is `Reconciling`; and typed failures are `Failed`. Joined is
connection state, never message-delivery evidence. Repeated transitions
coalesce, stale observations are ignored, invalid targets are rejected,
saturation preserves existing unresolved work, and explicit session close
removes the record and its byte budget. The runtime bus does not expose a
matching typed link-open or general Reticulum link-state event, so this unit
does not parse logs or fabricate transport acceptance. Link identifiers,
frames, authentication material, and unbounded error strings are not retained.
Existing reconnect policy and controls remain unchanged. No worker, timer,
subscription, queue, persistence, protocol, transport behavior, action, or
dependency is introduced.

Phase 3J adds a bounded adapter for typed lxmf-sdk 0.9.6 delivery updates.
Queued, dispatching/in-flight, sent, delivered, failed, cancelled, expired,
rejected, and unknown map to distinct project states and evidence. Nonterminal
sent is transport acceptance; terminal sent is local completion for backends
without receipt terminality; only typed delivered claims peer delivery.
Message IDs become opaque operation keys and are not rendered. Peer targets,
attempts, timestamps, and numeric sequence are retained within existing
bounds; event IDs, cursor strings, and message IDs are omitted from
presentation. Transitions coalesce, evidence is capped, stale/duplicate updates
and terminal regressions are ignored, impossible state/terminal combinations
are rejected, and oversized reason codes become a fixed omission notice.
Legacy native status and evidence events remain a separate correlation unit so
RNS proof, propagation acceptance, peer activity, and router delivery cannot be
conflated. No send, retry, cancel, worker, timer, subscription, queue,
persistence, protocol, or dependency is introduced.

Phase 3K reconciles typed native `LxmfDeliveryEvidence` into the same opaque
message operation only when an exact message ID is present. Packet submission
and propagation-node acceptance are transport evidence; RNS packet proof is
receipt evidence with peer delivery unconfirmed; router delivery alone is peer
delivery. Failures remain failures, while peer activity, absent receipt, and a
propagation sync with no payload remain inferred/uncertain reconciliation.
Typed evidence time or bounded application event time controls ordering.
Stale updates and non-delivery changes after terminal state are ignored, while
later router delivery can resolve a prior failure. Raw detail and RTT are
omitted because detail can contain packet, link, resource, propagation-node,
and failure data. Evidence lacking message identity is not correlated by peer.
The coarser legacy `MessageDeliveryUpdated` event remains separate. No send,
retry, cancellation, worker, timer, subscription, queue, persistence, protocol,
or dependency is introduced.

Phase 3L completes the current LXMF status boundary by reconciling the coarser
legacy `MessageDeliveryUpdated` event only with exact message identity.
Submitted-to-runtime is queue admission, submitted-to-Reticulum is transport
acceptance, and unknown remains uncertain. Delivered/failed enum states must
agree with their boolean flags. Backward-compatible default-unknown events may
use exactly one legacy delivered/failed boolean; contradictory combinations
are rejected. Application observation time orders updates, stronger receipt
evidence is not regressed by a coarse submitted status, delivery resists later
failure, and later consistent delivery may resolve prior failure. Raw evidence
and RTT are omitted rather than parsed. No send, retry, cancellation, worker,
timer, subscription, queue, persistence, protocol, or dependency is
introduced.

Phase 3M projects the existing typed `StreamGap` and `StreamRecovered` events
into one bounded Operations record per integrated-broadcast or SDK/RPC event
source. A gap is authoritative event-gap evidence, never a fabricated message,
path, or transfer failure. Snapshot recovery is locally completed only when it
reports no errors and every typed snapshot-success flag is true; otherwise it
remains uncertain reconciliation, and a later gap can reopen the record.
Numeric cursors order recovery while duplicate and stale observations are
ignored. Evidence is capped and raw upstream cursor strings and recovery error
text are omitted. Recovery without a retained gap does not create history. The
adapter passively observes the existing owned recovery worker and adds no
snapshot, retry, action, worker, timer, subscription, queue, persistence,
protocol, or dependency.

Phase 3N adds app-owned propagation-sync correlation. Runtime sync events have
no operation identity and are also emitted for outbound propagation acceptance,
so the adapter creates an operation from the existing single pending app
generation and ignores events without that correlation. Queue, typed stage
progress, blockers, failures, final completion, and the existing task result
coalesce into that record. Intermediate completion is not terminal and local
sync completion never claims peer delivery. The ambiguous `Complete/Progress`
shape used by outbound acceptance and cleanup is ignored. Runtime detail,
arbitrary count-map keys, and backend errors remain in Network Doctor rather
than shared presentation; only fixed typed labels and bounded final counts are
retained. Identical stage progress coalesces, runtime terminal states resist
later runtime updates, and only the correlated app task result may resolve the
final outcome. Existing history bounds and terminal eviction apply. No sync,
automatic retry, worker, timer, subscription, queue, persistence, protocol, or
dependency is introduced.

Phase 3O retires the 0.6-era OMENchat one-hop connection cutoff. A known
Reticulum route is now accepted at any hop count allowed by the locked 0.9.6
transport, so valid client -> public gateway -> private gateway -> omenchatd
topologies are not mislabeled as stale. Unknown paths still trigger bounded
discovery, link establishment retains its timeout and cancellation owner, and
failed pending links retain close/reset/rediscovery handling. Reticulum 0.9.6
owns its 128-hop ceiling, path replacement, timeout expiry, and rediscovery
after an unactivated link closes. Focused regression coverage admits known
1-, 3-, 13-, and 127-hop paths while rejecting an unknown path. The removed
recent-announce cache and wait were used only to enforce the obsolete cutoff,
so this unit reduces retained state and waiting rather than adding a worker,
timer, queue, cache, protocol, storage, configuration, or dependency.

Phase 3P fixes omenchatd's multi-gateway management boundary. The live runtime
already parsed, validated, started, monitored, and shut down multiple enabled
TCP interface sections, but every explicit CLI/TUI client edit regenerated the
whole Reticulum configuration and silently replaced the previous interface.
`interfaces tcp-client` and the TUI/line-console gateway action now add a
uniquely named TCP client while preserving listeners and other clients.
Redacted `interfaces list` and endpoint-specific `interfaces delete tcp-client`
commands provide inspection and removal; the line console also accepts
`tcp-client-delete`. Edits reject duplicates, control-character injection,
non-regular configuration files, more than 64 interfaces, and more than 2 MiB,
and retain an owner-only pre-edit recovery copy. Bootstrap-only
`init/run --tcp-client` overrides deliberately keep their single generated
configuration behavior. No live worker, identity/storage ownership, IFAC wire
behavior, protocol, database, dependency, or default interface is changed.

### Model

Each bounded record should contain only what its domain supports:

- stable project operation ID and domain;
- destination, peer, server, or room reference;
- state and whether it is authoritative, inferred, stale, or uncertain;
- queue/dispatch/transport/receipt/delivery evidence;
- Resource progress only when upstream reports authoritative totals;
- attempt count, stamp cost, propagation node, timestamps, and last error;
- event cursor/gap/reconciliation evidence;
- valid actions such as cancel, reconcile, explicit safe retry, or copy
  diagnostics.

Cover path discovery, link establishment, LXMF direct/propagated work,
Resources/attachments, OMENchat connection and mutations, cancellation,
failure, expiry, and rejection.

### Bounds and lifecycle

- Explicit item and byte ceilings for active and completed records.
- Coalesce high-frequency progress before rendering.
- Incremental expiry of completed history.
- Event-driven updates; no recurring one-second poll.
- Persist only unresolved records required for restart reconciliation.
- One owner and shutdown path for any new worker. Prefer no new worker.

### Surfaces

- Add an Operations/Transfers GUI panel using existing workspace conventions.
- Add a TUI view over the same records with filter, search, evidence copy, and
  valid actions.
- Reuse Network Doctor for detailed network evidence instead of duplicating
  it.

### Gate

- GUI and TUI render the same fixture records and vocabulary.
- Queue admission is never labeled delivery.
- `sent`, transport receipt, propagation-node acceptance, and peer delivery
  remain distinct.
- Gap/deduplication/snapshot reconciliation and saturation tests pass.
- Idle-message/redraw measurements show no unexplained regression.

## Phase 4 — delivery and propagation policy

Extend existing directory/settings models conservatively.

### Per-contact settings

- direct preferred;
- propagated preferred;
- direct only;
- propagated only;
- automatic fallback;
- ask before fallback;
- maximum automatic stamp cost;
- ask above a threshold;
- ticket preference;
- attachment automatic-download threshold.

Existing behavior remains the migration default. Automatic uncertain mutation
retry remains prohibited. Policy decisions must be visible in Operations and
diagnostics.

Phase 4A begins by making the already-persisted Directory `Direct` or
`Propagated` preference effective as the initial mode for a newly opened peer
conversation. Existing and restored conversation tabs preserve their explicit
mode, and an unset preference remains Direct. This adds no automatic fallback,
retry, setting, schema, protocol, worker, timer, or dependency.

Phase 4B adds `Direct only` and `Propagated only` as additive values in that
same bounded field. New conversations start in the permitted mode; manual
composer switching and the native pre-send boundary both enforce exclusivity.
The legacy `direct` and `propagated` encodings retain preferred semantics and
allow either manual mode. The retry-safe outbound operation snapshot records
whether propagation fallback is permitted; missing legacy metadata preserves
the prior permitted behavior and malformed metadata is rejected. This unit
adds no automatic fallback, retry, stamp spending, worker, timer, schema
version, protocol, or dependency.

Phase 4C adds a per-peer `Ask before fallback` / `Automatic safe fallback`
choice. Missing and older Directory data defaults to asking, which preserves
the existing explicit `Retry via propagation` flow. Automatic fallback is
snapshotted into the durable outbound operation and activates the typed 0.9.6
SDK/RPC `try_propagation_on_fail` option only for a direct send that also
permits propagation. The integrated clean sender retries the same signed LXMF
message through the selected propagation node only when no packet or Resource
submission was observed. Once submission begins, failure remains uncertain and
requires the existing explicit confirmation; it is never resent
automatically. `Direct only` always disables fallback. This adds no worker,
timer, dependency, protocol version, or database migration.

Phase 4D adds a per-peer maximum automatic **direct** stamp cost. Missing and
older Directory or outbound-operation data uses the existing hard ceiling of
8, so migration does not increase or decrease automatic work. The Directory
control cycles through default, disabled (0), 1, 2, 4, and 8. The effective
limit is snapshotted into the durable outbound operation and checked against
authenticated peer announce data before acquiring the bounded blocking stamp
permit. A valid reply ticket retains precedence and avoids stamp generation.
Values above the compiled hard ceiling are clamped down, never up. This unit
does not change propagation-stamp defaults, add confirmation UI, or weaken the
global attempt/concurrency bounds.

Phase 4E adds an optional per-peer direct-stamp confirmation threshold. It is
disabled for missing and older Directory data. Presets ask above 0, 1, 2, 4,
or 8. Only authenticated peer announce cost evidence can open a confirmation,
and a valid reply ticket bypasses it because no stamp work is needed. Desktop
and TUI preserve the draft and state explicitly that nothing was sent; both
offer Confirm and Cancel actions. The durable operation carries the threshold
and exact approved cost, and the integrated runtime rejects a required cost
above the threshold unless that exact cost was approved. Draft changes, peer
policy changes, cancellation, and prepared retries clear approval. The
automatic ceiling still wins, so confirmation cannot authorize cost above it.
This adds no timer, worker, retry, dependency, protocol version, or database
schema migration.

Phase 4F makes the existing reply-ticket composer choice available as a
per-peer default. Missing and older Directory data remains off. A peer can use
the application default, offer a reply ticket, or explicitly not offer one.
The preference initializes only a newly opened conversation; existing and
restored tabs retain their explicit choice. Desktop and TUI use the same
vocabulary, and users can still change the composer choice before each send.
The existing signed LXMF ticket implementation remains the sole wire/runtime
path. This unit adds no retry, worker, timer, dependency, protocol version, or
database schema migration.

Phase 4G records that an attachment automatic-download threshold is not
applicable to the current LXMF attachment representation. Attachments are
bounded inline fields in the signed message, not deferred Resources that can be
admitted after metadata inspection. Existing per-file, aggregate, item, name,
wire, and isolated-storage limits remain the correct admission boundary.
Implementing a real threshold would require a separately negotiated
resource-reference protocol and mixed-version design, so no inert setting is
added in this release.

Phase 4H improves the existing bounded propagation-node snapshot without
changing selection behavior. It labels the persisted selected node as pinned,
other entries as candidates, records announce age at snapshot time, and derives
an explicit evidence state from authenticated identity, freshness, and path
facts. Desktop and TUI render the same evidence. The snapshot remains
event-driven and subject to the existing item and byte budgets; no scanner,
timer, request, retry, or automatic failover is added.

Phase 4I projects existing propagation refresh and synchronization operations
into that same bounded inventory. Refresh outcome, observation time, and the
cooldown remaining at snapshot time come from the existing single-flight
refresh owner. Sync state, last update, last successful completion, and the
last bounded error come from the shared Operations history rather than a
second history. Only records whose normalized target exactly matches a node
are projected. Start, typed progress, and completion events refresh the
snapshot; no recurring clock or status polling is introduced.

### Propagation-node quality of life

Extend the existing bounded inventory with:

- verified identity and user trust;
- path/hop and announce-age evidence;
- advertised stamp cost and capabilities;
- last successful sync and error;
- cooldown;
- selected, pinned, or temporary status;
- explicit compatibility/rejection reason.

Selection and refresh remain bounded, deadline-limited, event-driven, and
manual by default. Do not introduce constant scanning.

### Gate

- Settings validate, persist, migrate, and roll back safely.
- GUI/TUI share policy vocabulary and decisions.
- Boundary, timeout, cooldown, over-cost, unavailable-node, and restart tests
  pass.

## Phase 5 — GUI and TUI quality of life

Build on existing workspace, settings, Network Doctor, and Operations models.
Use existing dependencies or the standard library.

### GUI

- Small in-memory command palette for open/switch/request/copy/diagnostic
  actions.
- Actionable error cards for no path, over-cost policy, uncertain delivery,
  queue overload, and reconnect.
- Named workspace presets built on current workspace persistence.
- Consistent evidence labels and valid-action controls.

Phase 5A adds the small GUI command palette without introducing another action
or networking layer. Its fixed command inventory routes through the existing
typed desktop messages for workspace switching, browser path actions,
diagnostics, and identity-hash copying. Query text is bounded to 128 characters
and 256 bytes, results are capped at eight, and the palette owns no worker,
timer, persistence, history, or network state. Ctrl+K and the status-strip
button open it; Escape closes it.

Phase 5B turns retained shared Operations rows that need attention into compact
desktop cards. Each card navigates to the already-owned related workspace and,
when the operation advertises that action, copies a bounded diagnostic for the
exact retained record. The copied text omits opaque operation identifiers and
is capped at 2 KiB. Retry, cancel, and reconcile buttons are deliberately not
fabricated where no typed operation-specific route exists. This reuses the
bounded Operations history and adds no error history, parser, worker, timer, or
network state.

Phase 5C adds bounded TUI Operations search and filtering on the existing
Network Doctor view. It reuses `OperationPresentationQuery`: `/` edits an
ephemeral query capped by the shared 128-byte limit, `f` cycles all, active,
needs-attention, and completed, and `c` clears the query. Rendering uses the
active edit buffer without retaining a second history or index. Invalid state
falls back visibly to the selected filter without panicking. No persistence,
worker, timer, or networking behavior is added.

Phase 5D adds TUI Operations row selection and a copy/select diagnostic view.
Up/Down or j/k select among the same eight bounded presentation rows; Enter or
v opens the existing redacted, 2 KiB operation diagnostic. While that view is
open, the TUI explicitly releases terminal mouse capture so the user's terminal
can perform its normal text selection/copy, then restores capture on Esc or q.
PageUp/PageDown or j/k scroll the preview. No clipboard crate, OSC-52 sequence,
diagnostic copy, persistence, worker, timer, or additional history is added.

Phase 5E replaces the stale one-size-fits-all TUI help text with a contextual
shortcut overlay built from the active workspace. It documents the existing
browser, messaging, Network Doctor, diagnostics, settings, logs, and other
section key routes without creating another command registry or action layer.
The footer now describes normal mouse use as navigation; terminal text
selection is advertised only inside the copy/select view where capture is
actually released. This is render-only and adds no state, command history,
worker, timer, persistence, or network behavior.

Phase 5F adds truthful TUI backpressure visibility for the existing bounded
application event channel. Operations shows total channel occupancy against
the 256-item bound, payload-bearing item count and bytes against the 32 MiB
bound, and cumulative payload admission rejections. Payload rejections are
highlighted without inventing queue progress or delivery evidence. The
TUI-only product profile does not compile the OMENchat client and therefore
owns no OMENchat reconnect deadline; no fake reconnect countdown or polling
clock is added. The desktop OMENchat monitor remains the owner of its existing
reconnect timers.

Phase 5G completes the TUI Directory one-key network controls by routing `d` to
the existing selected-entry path request. The already-existing `r` propagation
refresh remains single-flight, cancellation-owned, deadline-limited to six
seconds, and subject to its 30-second cooldown; `x` cancels it. Directory
titles and contextual help now expose path request, refresh, cancellation,
selection, and sync keys. No new request implementation, retry, timer, worker,
queue, or networking state is introduced.

Phase 5H exposes the existing persisted reduced-motion preference through the
TUI Settings action list. The label states that the preference controls desktop
animated previews and that the TUI has no animation loop. Static media remains
the deliberate `desktop-product-static-media` compile-time product profile,
not a runtime switch that could pretend to unload GIF support. This reuses the
existing atomic settings save/rollback path and adds no renderer timer, media
cache, dependency, worker, or schema change.

Phase 5I adds three fixed desktop workspace presets: Browser focus, Messages
focus, and Browser + Messages. Applying a preset changes only the visible Iced
pane grid and persists it through the existing bounded workspace-layout
settings. It does not delete browser tabs, LXMF conversations, OMENchat
sessions, histories, or drafts; hidden targets remain available through the
existing restore controls or the preset buttons themselves. Presets add no new
settings schema, layout engine, worker, timer, queue, dependency, or networking
behavior.

Phase 5J removes the expanded LXMF message card's raw delivered/failed Boolean
display and reuses the same evidence-aware summary shown on message bubbles.
Bubble and detail actions now come from one valid-action decision, so retry,
cancel, and propagation-sync controls cannot drift between the two views.
Terminal delivery, useful transport evidence, SDK cancellation support, and
sync-first states retain their existing conservative gates. This is a
presentation/routing consolidation only: it adds no state, automatic retry,
worker, timer, queue, dependency, protocol, or storage change.

### TUI

- Search/filter mode.
- Copy/select mode and copyable diagnostics.
- Shortcut overlay and command history.
- Reconnect countdown.
- Operations/transfer and queue/backpressure visibility.
- One-key path request and propagation refresh.
- Reduced-motion/static-media controls where applicable.

### Gate

- No second networking state machine.
- Keyboard/focus and reduced-motion tests pass.
- Hidden panes and terminal redraws add no recurring work.
- Workspace migration and rollback preserve existing layouts.

## Phase 6 — OMENchat feature additions

No mutating feature may ship until Phase 2 covers its operation identity and
server replay semantics. Prefer append-only events and explicit capability
negotiation. Mixed-version peers must continue using the existing protocol.

### Unit 6A — replies and mentions

- Reply references immutable room-event IDs.
- Bounded preview and jump-to-original behavior.
- Mention highlighting/count and mute-except-mentions.
- Reject missing, cross-room, oversized, or pruned references safely.

Unit 6A begins with
`docs/design/OMENCHAT_REPLIES_MENTIONS_CHECKPOINT.md`. Current code confirms
that an additive `reply-mentions-v1` capability can preserve protocol version
1 by using a tagged rich `RoomMessage` body and trailing `RoomEvent` fields.
The checkpoint defines exact item/byte bounds, durable canonical hashing,
same-room reference and numeric-mention validation, server schema-v4 and
client-column migrations, guarded rollback, mixed-version behavior, crash
boundaries, and the staged activation order. No production capability, wire,
schema, setting, or UI behavior is activated by the checkpoint itself.

Status: complete and activated. The inert shared contract, omenchatd schema-v4
storage, transactional server validation/replay/fan-out, client
model/storage/parsing, read-only presentation, server-scoped local user
binding, bounded composer controls, durable send/recovery bridge, exact legacy
fixtures, and deterministic retention evidence all pass. The client now
requests `reply-mentions-v1` only with its persistent identity-scoped instance
ID, and the server accepts it only with the durable base capability.
Capability rejection/loss keeps controls disabled and cannot downgrade or
resend an uncertain rich mutation.

An isolated current/current two-client smoke passed before and after an
omenchatd restart. All three Links recorded durable mutations, notice
acknowledgements, replies/mentions, and local numeric user binding as active;
ordinary room traffic and persisted history remained compatible. Exact scope,
bounds, rollback, tests, measurements, and the local evidence path are recorded
in `docs/design/OMENCHAT_REPLIES_MENTIONS_CHECKPOINT.md`.

### Unit 6B — reactions

- Append-only add/remove events keyed by event ID, actor, and bounded reaction.
- Per-event/per-actor/global retention limits.
- Durable exact replay without duplicate counts or fan-out.

The pre-implementation contract is
`docs/design/OMENCHAT_REACTIONS_CHECKPOINT.md`. It proposes an additive
`reactions-v1` capability, exact operation/body/result/snapshot shapes,
fixed ASCII reaction tokens, separate bounded active and append-only audit
tables, server schema 5 with a validated downgrade-copy path, additive
identity-scoped client state, item/byte/age/pruning-work ceilings, staged
activation, and the complete fault/mixed-version matrix. The checkpoint changes
no production capability, wire, schema, storage, or UI behavior.

The first dormant protocol slice is complete. `omenchat-protocol` reserves
operations 25–29 and owns strict bounded request, acknowledgement, event, and
explicit-target snapshot types, a fixed eight-token ASCII catalog, canonical
durable hash coverage, and the `reactions-v1` dependency on
`durable-mutations-v1`. The independent desktop and omenchatd codecs share a
byte-exact fixture. No client or server advertises or accepts the capability,
and no schema, storage, executor, fan-out, history, or UI behavior changed.
Implementation exposed and corrected one checkpoint ambiguity: even an empty
snapshot carries its sorted target-event set, so reconciliation cannot clear
another page's state.

The second dormant slice is complete. omenchatd schema 5 creates constrained
active-reaction and append-only audit tables plus target/retention indexes in
the existing immediate migration transaction. Existing version 0–4 fixtures,
pre-v5 backups, and injected failures at all schema/version/commit boundaries
prove recoverability. The stopped-server, confirmation-gated
`database export-schema4-copy` command creates a private, integrity-checked
schema-4-compatible copy through staged atomic publication, refuses overwrite
or active WAL/SHM state, omits only reaction state, and never modifies the
active database. At the conclusion of this slice, `reactions-v1` remained
unrequested and unaccepted.

The third dormant slice is complete. omenchatd now has a transactionally
coupled durable reaction executor, exact replay/conflict behavior, joined-user
and target eligibility checks, command-rate admission, bounded active state,
bounded incremental audit retention, and authoritative explicit-target
snapshots over the existing compressed inline/resource path. A changed
mutation produces one acknowledgement and one reaction delta; a semantic
no-op produces an acknowledgement without an audit row or fan-out. Live
reaction deltas are scoped to same-room Links with authenticated,
identity-matching reaction bindings. Restart replay, changed-content conflict,
no-op add, inline/resource snapshot decoding, active/audit bounds, and
capability-scoped fan-out are covered by isolated tests. The production server
flag remained off at this stage, so `reactions-v1` was neither accepted nor
reachable from a negotiated client.

The fourth dormant slice is complete. The desktop client's identity-scoped
`chat.sqlite` now has an additive active-reaction cache, and the shared
`ChatClient` model keeps reaction state separate from message history under
explicit per-actor/target, target, room, server, and global item/byte bounds.
Negotiated deltas and authoritative explicit-target snapshots reconcile the
same state for GUI and TUI consumers; inline and Resource snapshots share the
existing bounded compressed transport. Persistence and restart restore operate
only on retained eligible targets in protocol-sized pages. Focused tests cover
strict negotiation, duplicate deltas, authoritative replacement, overload
rollback, transport decoding, persistence, and restart. At this stage there
were no reaction controls, the client did not request `reactions-v1`, and the
server did not advertise or accept it.
The first full desktop-dev run also caught the snapshot persistence pass
considering transient high-bit local echo IDs after ordinary history correctly
excluded them. The client model, persistence, and restore paths now all exclude
those non-server IDs from reaction eligibility; the focused regression and the
complete suite pass after the correction.

The fifth dormant slice adds one project-owned read-only reaction presentation
model. It deduplicates actor rows, preserves the fixed protocol token order,
counts exact distinct actors, and highlights `you` only from the negotiated
numeric local-user binding. The Iced OMENchat timeline renders non-interactive
summary chips and limits its lookup to the active server/room range rather than
scanning the global reaction ceiling on every view construction. The bounded
room result is indexed once by target rather than rescanned for every timeline
event. Counts are
visible only for targets with non-persistent completion evidence from a
validated live snapshot. Restart restore and reconnect keep bounded cache rows
but clear that evidence, and the next snapshot prunes rows/evidence that no
longer have a retained history target before becoming visible. The production
capability remained disabled at this stage and no mutation action, retry,
worker, timer, or polling path was added. The legacy Ratatui workspace does not currently contain
OMENchat sessions (its Messages section is LXMF), while omenchatd's TUI is an
administrative server view with no client-local identity. Therefore this slice
does not fabricate a TUI reaction panel; that portion of checkpoint step 5
remains unavailable until a separately justified OMENchat TUI exists.

The sixth dormant slice connects capability-gated Iced controls to the existing
bounded durable-mutation owner without activating the capability. Each action
derives add/remove from authoritative local-user state, persists the canonical
intent before sending, creates no optimistic count, and accepts only a strictly
matching acknowledgement. Recovered reactions remain explicit-confirmation
operations; capability loss blocks retry and restart never resends them.
Production client request and server acceptance flags remain unchanged.

The seventh qualification slice closes the dormant deterministic gate. Client
and standalone-server reaction filters, adjacent `0.6.0-1`/`0.9.6-3` ordinary
wire fixtures, restart/replay/conflict, inline/Resource, schema fault,
downgrade-copy, capability-loss, and fan-out tests pass. New exact-boundary
regressions cover server-global active saturation and bounded replacement at a
full non-expired room audit. An ignored isolated measurement records active,
audit, snapshot, SQLite, and latency observations at 1,024 rows and at the
4,096-row room ceiling without imposing hardware-specific thresholds. The
evidence is in `docs/audits/omenchat-reactions-qualification.md`; production
activation and the real two-client smoke remain separate.

The eighth activation slice enables `reactions-v1` at the existing negotiation
boundary. The client requests it only when its persistent durable-mutation
owner is ready and records Link-scoped request state; omenchatd accepts it only
with a valid durable request. Tests preserve fail-closed behavior for
unsolicited acceptance, base-only/older peers, downgrade, reconnect, identity
change, and Link retirement. No wire number, schema, limit, queue, worker,
timer, or ordinary protocol-v1 frame changed. The isolated current/current
two-client smoke subsequently passed with a graceful server restart and forced
Resource snapshots. It exposed and fixed missing live snapshot attachment,
missing `ReactionSnapshotResource` transport dispatch/release coverage, and
the client's missing `recent` history Resource-purpose acceptance.

The ninth presentation slice replaces the wrapping textual reaction-button
block with the same fixed eight-token vocabulary rendered as compact emoji
controls. Semantic hover labels preserve discoverability, authoritative
summary chips retain actor counts and the explicit `you` marker, and the reply
action uses the existing Nerd Font/tooltip infrastructure. This is a
presentation-only change: negotiation, durable intent ownership, wire tokens,
storage, bounds, and fallback behavior are unchanged.

The tenth qualification slice extends the existing single-process reconnect
harness. The same client now completes reactions before restart, observes Link
replacement, renegotiates the capability, and repeats lost-ack exact replay,
authoritative Resource reconciliation, semantic no-op, removal, and persistent
intent cleanup against a post-reconnect message. Stage names are separately
prefixed so pre-restart evidence cannot satisfy the replacement-Link gate.

### Unit 6C — bounded local search

- Search existing LXMF and OMENchat history by text, sender, room, date,
  attachment name, and delivery state.
- Verify packaged SQLite FTS5 before choosing it. Otherwise use bounded
  parameterized SQLite queries or a bounded token index.
- Incremental, cancellable, low-priority indexing with explicit queue and batch
  bounds if an index worker is required.

Unit 6C begins with `docs/design/LOCAL_HISTORY_SEARCH_CHECKPOINT.md`. Current
code confirms that LXMF JSON threads and OMENchat SQLite/session history have
different authoritative owners. The first slice therefore adds a zero-schema,
read-only reducer over resident bounded models rather than a second persistent
source of truth. Query, terms, examined items, results, and copied display text
all have hard ceilings; opaque identifiers, extension fields, and private paths
are excluded. The packaged `portable-sqlite` profile also has a functional
FTS5 probe, but a persistent index remains deferred until measurement justifies
its owner/rebuild lifecycle. UI activation requires one cancellable or
superseding owned task and may not scan history on Iced's update/view path.

Status: bounded desktop surface complete. Read-only store loaders search both
authoritative histories sequentially with a fair 4,096-item reservation per
source, one active blocking scan, and one replaceable pending query. The
Messages workspace submits explicitly, exposes source and exact limit state,
and renders a bounded result window. Persisted LXMF keys require the same open
peer, index, and stable message key before navigation; OMENchat keys require
the same open server and retained room/event. Missing, moved, or changed
targets fail closed. The reducer measurement, owner, presentation, router,
store, and target validation tests pass. Advanced desktop filter controls,
Ratatui search, and an interactive packaged-display smoke remain explicit
follow-ups; no index, schema, timer, subscription, or recurring worker was
added.

### Unit 6D — safe invitations

- Include server destination, room, verified server fingerprint/evidence, and
  optional label.
- Never include passwords, IFAC secrets, bearer tokens, or reusable moderation
  credentials.
- Bound URI/QR input before decode and require user confirmation before trust
  or connection changes.

Unit 6D begins with
`docs/design/OMENCHAT_INVITATIONS_CHECKPOINT.md`. The repository already owns
the compatible plain `omenchat://<destination>` launch path and optional locked
Iced QR support. The checkpoint deliberately does not reuse the dormant
secret-bearing LXMF invite JSON shape. It defines a 2 KiB canonical no-secret
URI, fixed public fields, claimed-versus-verified identity evidence, one
ephemeral confirmation owner, exact room-catalog admission, mixed-version
fallback, QR-as-identical-text behavior, and exhaustive malformed/boundary
tests. No production parser, generation, QR feature, connection behavior,
wire, schema, storage, capability, or dependency changes in the checkpoint
unit.

The first implementation slice adds the frontend-neutral canonical URI value.
It accepts the exact legacy plain launch form and a 2 KiB enhanced invitation,
normalizes exact Reticulum destination/fingerprint hashes, decodes only a
bounded public label, rejects unknown/duplicate/trailing fields and authority
tricks, and serializes only the fixed no-secret field set. It has no production
caller, connection action, preview, storage, QR, wire, schema, worker, timer,
or dependency. Ephemeral confirmation and trust-evidence reduction remain the
next slice.

The second dormant slice adds that frontend-neutral preview reducer without
wiring it to a desktop action. It owns one replaceable invitation, classifies
no-claim, unverified, verified, and conflicting identity evidence from exact
OMENchat Directory entries, and makes any conflict block confirmation until
explicit cancellation. Invalid replacement input preserves the prior preview.
The reducer has no connection, join, trust, persistence, QR, worker, timer,
queue, protocol, or schema behavior. Desktop presentation and explicit
confirmation remain the next slice.

The third slice activates preview and confirmation only in the desktop
quick-open surface. Parsing an enhanced invitation opens no connection and
does not mutate Directory/trust state. The card renders bounded public fields
and exact identity evidence; conflicts allow cancellation only. Explicit Open
consumes the preview and routes a plain destination through the existing
OMENchat Link owner. Legacy plain links remain unchanged.

The fourth slice completes deferred room admission. One ephemeral suggestion
is bound to the exact destination/session and is consumed only when that
session's authenticated bounded room catalog contains the exact numeric room
ID. Cross-session catalogs cannot consume it. Catalog mismatch, open or
handshake failure, session close, cancellation, and replacement clear it.
Existing authenticated sessions can use their already-returned catalog. The
normal join owner remains authoritative, uncertain mutations are not retried,
and enhanced Micron links and QR were not activated by that slice. No new
connection owner,
retry, worker, timer, queue, persistence, wire, schema, capability, or
dependency was added.

The fifth slice adds canonical copy generation to the existing OMENchat
composer. It emits only the exact session destination, a joined active room,
the bounded server label, and an identity fingerprint when exact OMENchat
Directory evidence is valid and unambiguous. Conflicting or malformed identity
evidence is omitted; it is never promoted to trust. The existing serializer
enforces the 2 KiB and no-secret contract before the current Iced clipboard
owner receives the text. Missing sessions fail closed. No QR feature, enhanced
Micron link, persistence, wire behavior, network action, worker, timer, queue,
cache, retry, schema, capability, or dependency was added.

The sixth slice routes enhanced OMENchat links from Micron through the existing
bounded preview/confirmation owner. Valid enhanced links do not connect,
persist, or change trust before confirmation; malformed enhanced links fail as
invitation input and cannot fall through to browser navigation or a plain
session open. Form-forwarding fields are ignored at this boundary. Plain legacy
Micron links retain their established direct-open behavior. Keyboard-focused
and pointer activation use the same reducer. QR was not activated by that
slice, and this
slice adds no runtime owner, persistence, wire behavior, worker, timer, queue,
cache, retry, schema, capability, dependency, or product-feature change.

The seventh slice admits Iced's existing QR feature into both canonical desktop
products and adds an explicit OMENchat invitation QR card. The locked transitive
`qrcode 0.13.0` encoder is pure Rust and MIT OR Apache-2.0; no crate version,
camera, decoder, native library, platform permission, or network surface was
added. One ephemeral owner holds one canonical URI plus one encoded matrix/cache
and clears on toggle, Close, replacement, session close, or room transition.
The visible text and clipboard use the exact retained QR URI. The product graph
gate requires `desktop-qr`, Iced QR support, and the exact encoder in animated
and static-media products. Native/package builds remain the platform gate.

### Unit 6E — corrections and delete tombstones

- Append correction and tombstone events rather than rewriting history.
- Preserve immutable original IDs and moderation/audit evidence.
- Define bounded correction depth/count and tombstone retention.
- Durable replay must not repeat edits, deletions, notifications, or fan-out.

The design checkpoint is recorded in
`docs/design/OMENCHAT_CORRECTIONS_TOMBSTONES_CHECKPOINT.md`. It proposes the
additive `message-revisions-v1` capability over the existing durable base,
reserves the currently unused operation range 35–39, defines exact
request/ack/event/explicit-target-snapshot shapes, and keeps original room
events immutable. Current state and bounded append-only audit use separate
schema-6 tables; the legacy `room_events.deleted` test projection is not the
new contract. The checkpoint also records an activation dependency: current
server room history is retained indefinitely, so a live tombstone cannot be
pruned safely until Unit 6G can remove an original and every dependent
revision, reaction, and reply projection atomically. No capability, operation,
schema, persistence, worker, timer, retry, or UI action is activated by this
checkpoint.

The first dormant slice reserves operations 35–39 in the shared protocol crate
and adds exact bounded request, acknowledgement, event, and explicit-target
snapshot codecs. Negotiation requires the durable base, canonical hashing
covers every mutation field, and both independent frame codecs share one
byte-exact correction fixture. Production request and acceptance lists remain
unchanged and have direct regressions.

The second dormant slice advances omenchatd to schema 6 with constrained
revision current-state and append-only audit tables plus bounded lookup and
retention indexes. Version-5 reaction rows survive migration. The migration is
one immediate transaction, creates a private pre-v6 backup, and has injected
rollback coverage before tables, between tables, before indexes, before the
version update, and before commit. The confirmation-gated
`database export-schema5-copy` path stages and atomically publishes a private
schema-5 copy that omits revisions but preserves reactions. The existing
schema-4 export now also removes revision objects. Neither command overwrites a
destination or modifies the active database.

The third dormant slice adds the transactional store and durable session
executor. It enforces author/moderator/mute policy, immutable original events,
eight corrections plus a tombstone, soft correction and hard state ceilings,
bounded audit pruning, transactional reaction cleanup, exact restart replay
without repeat fan-out, conflict detection, result-codec rollback, and
authoritative inline/Resource snapshots. Audit pruning cannot reuse a revision
ID because allocation considers both audit and current-state maxima. Production
negotiation still refuses `message-revisions-v1`.

The fourth dormant slice adds Link-scoped server plumbing without activating
the feature. A revision binding can be recorded only from an actually accepted
session response and is cleared on authenticated-identity replacement or Link
close. Isolated tests inject that otherwise-unreachable binding to prove that
live events reach only same-room, identity-matched capable Links; base, legacy,
and stale-identity Links receive no revision state. Capable join/history
responses are followed by authoritative explicit-target snapshots, and exact
durable replay returns the original acknowledgement without a second fan-out.
The production acceptance constant remains false. There is still no client
state, UI action, worker, timer, retry, or capability activation.

The fifth dormant slice adds the desktop client foundation without activation.
Original timeline events remain immutable while a separate
`ChatMessageRevision` projection derives edited/deleted presentation state.
The projection is limited to one row per retained message target and bounded
by items and stable retained bytes per room, server, and identity-scoped
store. An additive SQLite cache replaces only explicit snapshot target sets in
one transaction, survives restart, and rolls back saturation or malformed
updates without partial state. Deltas are strictly ordered and exact
duplicates are idempotent. The reserved durable-intent operation can be
persisted and recovered. Inline/Resource decoding and desktop persistence
routing exist behind a test-only negotiated-state injection. Production
requests still omit `message-revisions-v1`, unsolicited acceptance is ignored,
and there is no GUI action, timer, retry, or capability activation.

The sixth dormant slice connects that project-owned projection to the shared
Iced OMENchat timeline without adding a mutation action. Only retained targets
with authoritative snapshot evidence supply borrowed revision rows.
Corrections derive effective displayed text and an edited marker while
preserving reply/reaction actions. Tombstones replace the body with an explicit
deleted presentation and suppress original reply, mention, media, reaction,
resend, and mutation actions. Stale restart rows are not presented as current,
and redraw does not clone retained revision bodies. Production capability
request/acceptance remains disabled.

The seventh dormant slice closes the live-delta presentation evidence gap.
Reconnect still clears revision authority for the server. A subsequently
validated, negotiated revision delta restores authority only for its explicit
retained target; it does not make the room or untouched targets authoritative.
An exact replay restores stale target authority once and then remains
idempotent. Stale/conflicting deltas restore no evidence. This makes the
read-only projection react safely to live committed revisions without adding a
sender, capability request, retry, timer, or room-wide inference.

The eighth dormant slice adds the bounded transport sender without adding a
desktop prepare action. It revalidates the durable request hash, client
instance, expiry, server, room, retained target, typed body, and both negotiated
capabilities before using the existing per-session pending mutation limit.
Typed acknowledgements require the original sequence, room, target, action,
and authenticated local user before completing the durable intent. A send or
ack never changes the revision projection optimistically. Recovered-intent
validation and redacted labels understand the operation, and revision sends do
not clear an unrelated ordinary composer draft. Production negotiation remains
disabled.

The ninth dormant slice adds desktop correction/deletion actions without
activating the capability. One bounded correction draft per session is separate
from the ordinary message composer; one deletion confirmation requires a
second explicit action. Eligibility is fail-closed on both negotiated
capabilities, authoritative target evidence, retained message type, local
identity and role evidence, mute/ban state, tombstone state, and revision
depth. Correction intent persistence completes before transport admission,
room/session changes cancel local action state, and a successful revision send
clears only its matching correction draft. No worker, timer, automatic retry,
or optimistic timeline change is added. Eligible targets are derived once per
bounded room render, avoiding a history rescan per message. Since production
still omits `message-revisions-v1`, the controls remain hidden.

The tenth slice completes deterministic qualification and reversibly activates
the capability. The production client requests `message-revisions-v1` only
beside `durable-mutations-v1` and its persistent identity-scoped client
instance identifier. The server accepts it only when the durable request is
valid. Unsolicited acceptance, base-only and adjacent peers, capability loss,
downgrade, identity replacement, and Link retirement remain fail closed.
Existing schema-8 retention removes revision state and audit with a compacted
target, so activation does not create orphaned revision projections.

The current/current process gate passes deliberately lost acknowledgement,
exact correction replay, forced-Resource correction and tombstone snapshots,
two isolated client roots, clean persistent-intent completion, and one
continuous client across orderly omenchatd restart with a replacement Link.
No automatic retry, wire number, schema, retention default, worker, queue, or
timer changed. Exact evidence, commands, limitations, and rollback are in
`docs/audits/omenchat-message-revisions-qualification.md`.

### Unit 6F — pins and moderation audit history

- Append bounded pin/unpin and moderation-audit events.
- Enforce current server roles transactionally.
- Expose audit history only to authorized users and redact private operational
  evidence.
- Bound retained audit records by room, age, count, and bytes.

The pre-implementation checkpoint is
`docs/design/OMENCHAT_PINS_MODERATION_AUDIT_CHECKPOINT.md`. Current source has
one free four-operation range between current-history and command operations,
an unused legacy `audit_log` table, transactional durable moderation, and a
separate operator-only text audit in the omenchatd TUI. The checkpoint keeps
those concerns separate: pins use an additive durable capability and explicit
target snapshots; client-visible moderation history uses a separate read-only
capability and constrained table and never exposes the operator log. It
proposes separate schema-9/schema-10 migrations, bounded retention, compaction
dependencies, guarded downgrade copies, and staged dormant activation. No
protocol number, schema, capability, storage, or UI behavior is changed by the
checkpoint itself.

The first dormant pin slice is complete. The shared protocol crate reserves
operations 46–49 and defines exact request, acknowledgement, event, and
target-scoped snapshot shapes. Snapshot input is limited to 256 explicit
targets and 64 active pin entries, requires strictly increasing unique target
order, and rejects entries outside the replacement set. Canonical durable
hashing covers operation, room, target, and action. The desktop and standalone
server codecs agree on one frozen byte fixture. Production capability request,
acceptance, durable execution, persistence, fan-out, and UI behavior remain
unchanged; a server regression test proves `room-pins-v1` is not accepted.

The second dormant pin slice advances omenchatd to schema 9 with constrained
current-pin and append-only audit tables. Store admission enforces 64 active
pins per room, 4,096 globally, a 1 MiB active-byte ceiling, 1,024 audit rows
per room, 16,384 globally, per-room/global audit-byte ceilings, 180-day
retention, and at most 64 pruned audit rows per mutation. Active pin audit is
not evicted; additions fail closed at capacity while unpins remain possible.
Migration faults roll back at every table/index/version/commit boundary.
History compaction preflights and removes pin dependencies in its existing
bounded transaction. A stopped-server, confirmation-gated schema-8 copy
removes only pin objects and preserves all prior history layers. Capability
negotiation and live execution remain dormant.

The third dormant pin slice couples role and membership validation, pin
state/audit mutation, and the exact replay acknowledgement in the existing
immediate durable transaction. Exact restart replay emits no second audit row
or fan-out; changed-content mutation-ID reuse conflicts. Internal Link state
now supports bounded inline pin snapshots and same-room, identity-matched,
pin-capable event fan-out for tests. Production capability acceptance remains
hard-disabled, so current and legacy clients observe no new operation.

The fourth dormant pin slice adds the desktop's bounded identity-scoped
projection and additive `chat.sqlite` cache. Snapshot authority is exact to
its explicit target set; deltas restore authority only for their target.
Restart, Link replacement, capability loss, and invalid snapshots retain
bounded cached rows but clear authority. The timeline is read-only and
distinguishes `📌 pinned` from `📌 pinned · cached`. The desktop still does not
request `room-pins-v1`, and no pin/unpin control, queue, timer, worker, or
automatic retry was added.

The fifth dormant pin slice adds test-negotiated pin/unpin controls without
activating production negotiation. Current moderator/administrator,
membership, target retention, exact-target authority, and durable identity are
required before intent admission. The existing persistence worker records the
canonical request before transmission, and one target cannot hold parallel pin
mutations. A matching ACK marks the durable intent acknowledged but shows
`awaiting room update` until a matching authoritative delta or snapshot
arrives. Those bounded confirmation slots share the existing mutation budget
and clear on capability or Link loss. No optimistic pin projection or
automatic retry was added.

The sixth qualification slice closes the dormant deterministic gate. Root and
standalone-server pin filters cover the independent exact wire fixture,
restart-stale projection, persistence-before-send, capability/role/authority
loss, transactional replay/conflict, Link-scoped fan-out, migration faults,
downgrade copy, compaction, per-room and global active/audit ceilings, bounded
pruning, and maximum inline snapshot encoding. An ignored isolated measurement
records 64-row ceiling storage and latency observations without imposing
hardware-specific thresholds. Evidence is in
`docs/audits/omenchat-pins-qualification.md`; production activation and the
two-client process smoke remain separate.

The seventh slice reversibly activates `room-pins-v1` at the existing durable
negotiation boundary. The client requests it only with its persistent
identity-scoped client instance and durable base request; omenchatd accepts it
only alongside that valid durable request. Unsolicited acceptance, pin-only
requests, downgrade, capability loss, identity replacement, and Link
retirement remain fail closed. No operation, schema, limit, queue, worker,
timer, retry, or ordinary protocol-v1 frame changed.

The eighth pin slice completes the isolated current/current process gate. One
client stays alive across orderly omenchatd restart, observes Link closure and
replacement, restores its session, and repeats moderator-authorized pin,
deliberately withheld acknowledgement, exact replay, semantic no-op,
authoritative snapshot reconciliation, unpin, and intent-store cleanup. The
gate exposed a server/client encoding mismatch for `PinSnapshot`; the server
now emits the bounded compressed inline body required by the existing live
client decoder. Focused tests and the independent-process smoke guard that
boundary. Pin snapshots remain bounded inline frames rather than being
misclassified as Resource batches.

The first moderation-audit slice reserves the separate read-only operations
52–55 and adds the bounded shared `moderation-audit-v1` request, record, and
page types. The independent desktop/server codecs agree on one byte-exact
request fixture. Page count, retained bytes, display names, identifiers,
timestamps, action/result combinations, role/status bits, ordering, and room
scope fail closed. Production capability request/acceptance, schema, mutation
execution, Resource dispatch, client projection, and UI remain unchanged.
Evidence is in
`docs/audits/omenchat-moderation-audit-qualification.md`.

The second moderation-audit slice advances omenchatd from schema 9 to schema
10 with empty-on-migration, SQL-constrained, item/byte/age-bounded audit
storage. Durable in-room moderation now commits the user mutation, one
client-safe audit row, and the replay result atomically; exact replay creates
no second row. Non-durable and local administrative paths remain excluded
until their mutations can share that transaction. Confirmation-gated schema-9
and schema-8 copies preserve all representable older layers. Capability
negotiation and client traffic remain dormant. Evidence is in
`docs/audits/omenchat-moderation-audit-storage-qualification.md`.

The third moderation-audit slice adds the test-only read boundary. Paging is
exclusive-cursor, newest-first, command-rate-limited, and rechecks current
moderator/admin role plus room membership for every request. Inline and
Resource responses carry identical bounded values and explicit short-page end
evidence. Capability state belongs to one authenticated Link identity and is
discarded on replacement or close. The desktop projection is memory-only and
capped at 1,024 records/512 KiB; capability loss clears it. Production still
requests and accepts no `moderation-audit-v1`, and no UI, timer, worker,
automatic refresh, schema, or retention setting changed. Evidence is in
`docs/audits/omenchat-moderation-audit-paging-qualification.md`.

The fourth moderation-audit slice qualifies deterministic failure and restart
boundaries without activating the feature. Oversized requests and malformed,
wrong-purpose, or oversized Resource offers fail before pending retention.
Delayed valid Resources replay through the existing bounded owner. Invalid
client pages clear ephemeral evidence. Schema-10 records survive a file-backed
server restart and duplicate read-only requests remain byte-stable. Current
desktop/server ordinary v0.9.6-3 fixtures remain exact, the desktop still
requests no audit capability, and production omenchatd still refuses it.
The next qualification unit completed isolated measurements at the actual
configured ceilings: 1,024 client records stayed within the 512-KiB client
budget and the next admission failed closed; 2,048 file-backed server rows
stayed within the per-room and global byte budgets. Exact host observations and
reproducible commands are recorded in the paging qualification audit.
Current/current process restart, adjacent-binary live traffic, and active
Resource cancellation remain separate pre-activation gates.

The subsequent cancellation review ran the existing real loopback Reticulum
gate successfully: sender cancellation crosses the physical wire and the
production bounded bridge while both Link ends remain active. This does not
complete the moderation-audit client gate. The locked
`reticulum-rs-transport 0.9.6` API exposes cancellation for an outbound
Resource, but no public receiver-side cancellation operation for an inbound
Resource. Production moderation-audit negotiation remains disabled; OMEN does
not fork upstream, fabricate cancellation evidence, or close an otherwise
healthy chat Link to hide the limitation.

### Unit 6G — room policy controls

- Retention policies with explicit defaults and guarded migration.
- Read-only announcement rooms.
- Slow mode with monotonic server enforcement.
- Per-room upload and media policy.
- Clear client evidence when policy rejects an operation.

The announcement-room checkpoint is recorded in
`docs/design/OMENCHAT_ANNOUNCEMENT_ROOMS_CHECKPOINT.md`. It keeps existing rooms
ordinary by default and proposes one capability-negotiated room policy bit
without changing protocol version 1 or unnegotiated four-field room values.
Server authorization applies regardless of negotiation: members may
discover/join/read, while only current moderators or administrators may publish
or mutate room content. The proposed schema-11 column defaults to zero without
a history scan and requires a confirmation-gated schema-10 copy export.
Implementation is deliberately split into dormant wire, storage/recovery,
atomic authorization, administration, bounded presentation, qualification, and
activation units. No behavior changes in the checkpoint slice.

The first announcement-room implementation slice adds only the independent
capability constant, fixed policy bit/mask, bounded legacy/negotiated room-value
codec, and byte-exact fixtures shared by both independent MessagePack codecs.
Legacy values remain four fields; negotiated values require exactly one policy
field and reject unknown bits. Production OMENbrowser requests no capability
and production omenchatd accepts none. Schema 10, room storage, authorization,
configuration, client models, and presentation remain unchanged. Evidence is
in `docs/audits/omenchat-announcement-rooms-wire-qualification.md`.

The second announcement-room slice advances only omenchatd storage to schema
11. Existing rooms receive constrained ordinary policy `0`; migration faults
roll back the column and version together while retaining a schema-10 backup.
The confirmation-gated, stopped-server `export-schema10-copy` path removes only
the new column and retains moderation-audit and every earlier layer. No
configuration, authorization, client projection, negotiation, or UI is
activated. Evidence is in
`docs/audits/omenchat-announcement-rooms-storage-qualification.md`.

The third announcement-room slice centralizes server authorization. Standard
and trusted members receive stable error 1016 before content side effects;
moderators/admins retain publication rights; durable replay returns its
original result after policy/role changes; upload policy is checked at offer
and publication; and policy/revision updates are atomic and confirmation-gated
while the server is stopped. Human and JSON room listings expose effective
policy. Production capability negotiation and client policy projection remain
dormant. Evidence is in
`docs/audits/omenchat-announcement-rooms-authorization-qualification.md`.

The client projection slice adds a bounded, session-owned policy map and
disables member composer, upload, reaction, and revision controls only when
the independent capability was explicitly requested and accepted. Capability
loss clears evidence. Production negotiation remains dormant. A following
current/current real-Link gate proves typed member rejection without a
committed event before and after an orderly server restart, using the same
isolated identity root and a new Link without automatic uncertain retry.
Adjacent `v0.9.6-3` ordinary frames remain byte-exact; negotiated announcement
traffic is not claimed for peers that cannot advertise it. Evidence is in
`docs/audits/omenchat-announcement-rooms-client-projection-qualification.md`
and
`docs/audits/omenchat-announcement-rooms-process-qualification.md`.
The same process gate now registers one isolated user through ordinary traffic,
promotes it through a confirmation-gated headless command, applies announcement
policy, and proves moderator message plus upload/Resource publication before
and after orderly restart. The user listing is redacted and role maintenance
uses the existing exclusive stopped-server database boundary; no TUI
dependency or capability activation is required.
The standard-member Resource boundary now also passes over real Links before
and after orderly restart: upload offers receive typed policy rejection with
no acceptance, completion, committed event, ledger entry, or server file.
Machine-readable doctor redaction remains unchanged; the isolated harness
checks the existing human ledger detail.
v0.9.6-4 adopts a restart-only room-policy administration contract. Every
announcement process mode proves the confirmation-gated command fails closed
while omenchatd owns the database, then uses the existing orderly
stop-maintain-restart path. Live reload and cross-process policy-delta fanout
remain outside this release; they are not activation prerequisites.
The production live-client reconnect/retirement path also has deterministic
captured-transport coverage: policy evidence clears before replacement,
remains absent when the replacement does not accept the capability, and
returns only after a fresh request/accept. Real negotiated current/current
catalog/delta and replacement-Link process traffic remains an activation gate.
A test-enabled server-engine boundary now also proves that only an explicit
`announcement-rooms-v1` request is accepted and that its initial catalog uses
the authoritative shared five-field encoder. The normal server constructor
remains dormant. Link-scoped join/delta shaping and mixed-peer process traffic
must pass before production activation.

The first Unit 6G checkpoint is recorded in
`docs/design/OMENCHAT_ROOM_RETENTION_CHECKPOINT.md`. Current history is
indefinite and its event allocator derives the next identifier from retained
rows, so deletion could reuse an immutable event ID. The approved order is:
persistent per-room high-water mark, bounded resumable byte/item ledger,
atomic dependency-aware compaction, disabled-by-default policy, then
admission integration and live gates. Surviving replies lose only an expired
reply projection; reaction and revision state/audit for a deleted target are
removed in the same transaction. No retention behavior or
`message-revisions-v1` activation is introduced by the checkpoint.

The first implementation slice advances omenchatd to schema 7 with a persistent
per-room event-ID high-water mark. Legacy rooms seed lazily from the indexed
maximum, avoiding a migration history scan; committed IDs remain monotonic
after deleting newest or all retained rows, while transaction rollback may
reuse only an uncommitted allocation. Integer exhaustion fails closed. A
confirmation-gated schema-6 copy export removes the later usage ledger plus
this sequence metadata from a staged copy and preserves history, reactions,
and dormant revision state.
Retention remains disabled and no production path deletes history.

The second implementation slice advances omenchatd to schema 8 with a
per-room item/byte usage ledger. Migration creates no room rows and scans no
history. A room captures one fixed backfill target, accounts new events in the
same immediate writer transaction, and advances legacy accounting by at most
256 rows per append or explicit maintenance call. Cursor progress is durable;
append-during-backfill is counted once; overflow rolls back the event and
sequence. A confirmation-gated schema-7 copy removes only usage metadata.
Retention and message revisions remain inactive.

The third implementation slice adds an explicit, dormant store compaction
primitive. One immediate transaction removes at most 64 original events and
preflights no more than 20,000 surviving reply/reaction/revision projection
rows. It shrinks a multi-event candidate batch when projection work is too
large and fails closed when a single event exceeds the bound. Surviving
replies retain their message and mention data but lose the expired target
reference; selected reaction and revision state/audit disappear with the
original; usage accounting is decremented exactly. Upload ledgers/files,
durable mutation replay, and persistent event-ID high-water marks remain
independent. Injected faults prove atomic rollback. No configuration,
admission hook, timer, capability, CLI, or UI activates this primitive.

The fourth implementation slice adds typed `[history_retention]`
configuration and bounded read-only maintenance evidence. Existing and newly
generated configurations default to `enabled = false`, 365 days, 100,000
events, and 256 MiB per room. Enabled zero limits fail closed; excessive values
clamp to documented maxima of 3,650 days, 1,000,000 events, and 10 GiB.
Human and JSON status inspect no more than 256 rooms and distinguish complete,
incomplete, and missing usage ledgers without advancing backfill. JSON
reports whether admission compaction is configured and that runtime activity is
not observable from the status process.

The fifth implementation slice attaches that policy only to the live server
store and routes ordinary and durable room-event writers through one atomic
admission boundary. Disabled configuration preserves indefinite history.
Enabled policy independently evaluates age, item, and byte ceilings after
insertion, selects no more than 64 older originals, and commits insertion plus
dependency-aware cleanup together. A sole newest event larger than the byte
ceiling is retained; a later admission may compact it. Incomplete usage
accounting, excessive projection work, or a ceiling requiring another batch
rejects admission and rolls back the new event, sequence advance, cleanup, and
ledger changes. No timer, polling worker, startup sweep, RPC, or UI compaction
path is added.

The sixth implementation slice adds recovery and transport-boundary evidence
without changing policy. A file-backed restart regression proves retained
event IDs remain monotonic, the usage ledger remains complete, and
`HistoryBefore` treats compacted ID gaps as an ordinary end of retained
history. A forced-Resource session regression proves that only surviving
events are serialized after admission compaction. The existing v0.6.0-1
bidirectional byte fixtures still pass unchanged. These are deterministic
store/session/codec results; they do not replace the later live
current/current or mixed-version process gates.

The seventh implementation slice exposes the bounded ledger backfill as an
explicit stopped-server maintenance command. One confirmed invocation targets
one positive room ID and advances at most 256 usage rows through the existing
single-owner administrative database worker. It requires an existing
current-schema database, prints cursor/target/item/byte/completion evidence,
and never invokes compaction or deletes history. A 300-event file-backed
regression requires two invocations and preserves all 300 events.

These units are in the `v0.9.6-4` target scope. If any unit cannot satisfy its
wire, storage, mixed-version, resource, and rollback gates, do not advertise
that capability or call the release complete; record the blocker and make an
explicit scope decision before tagging rather than shipping a partial hidden
implementation.

### Gate

- Wire structures and schema changes have a review checkpoint before
  activation.
- All histories and indexes have item/byte/age bounds.
- Duplicate/restart/mixed-version/malformed/oversized tests pass.
- Existing room history remains readable and downgrade/restore steps exist.

## Phase 7 — upstream boundaries and live evidence

Do not locally reimplement upstream Reticulum to close evidence gaps.

### Release-blocking software evidence

- Existing current and pinned Python NomadNet page/direct-request paths.
- Existing LXMF direct, propagated, stamp, ticket, Resource, and attachment
  paths affected by this revision.
- OMENchat current/current and adjacent-version durable operation families.
- Native Linux, Windows, Intel macOS, and Apple Silicon compile/test.
- GUI/TUI and standalone omenchatd smoke.

### Evidence-bound or upstream-blocked

- Stock IFAC server enforcement remains unsupported; keep the narrow local
  client adapter and fail closed.
- Oversized Python NomadNet request-Resource behavior remains unclaimed until
  qualified.
- Upstream maximum-size UDP Resource behavior remains documented, bounded, and
  non-retried.
- Earlier inbound Resource cancellation remains limited by the public upstream
  API.
- Physical radios, I2P/public topology, and physical GPU activity require
  corresponding hardware/peers. Record them as untested, never as zero or
  passed.

## Phase 8 — resource and release qualification

### Local checkpoint

- formatting and `git diff --check`;
- root product/TUI checks, tests, and strict Clippy;
- standalone server headless/full checks, tests, and strict Clippy;
- protocol crate conformance tests;
- release quick gate;
- dependency train, product identity, advisory, license, and source policy;
- OMENchat scroll and LXMF attachment smoke;
- focused database/replay/intent fault tests.

### Measurements

Repeat the existing harnesses for:

- settled desktop idle CPU/RSS/wakeups/shutdown;
- pane and Operations-history stress;
- OMENchat reconnect and durable-replay soak;
- omenchatd queue/SQLite/link/log bounds;
- search-index and new-feature storage growth;
- active transfer cancellation and shutdown.

A greater-than-10% unexplained regression is a review trigger. Do not invent
GPU/hardware measurements.

### Hosted checkpoint

After the candidate is stable:

1. Freeze behavioral scope.
2. Set root and server versions to `0.9.6-4` together and update active
   smoke/version assertions and release documentation.
3. Open one PR and run native CI once on that exact candidate.
4. Run pinned/current Python and mixed-version interoperability once.
5. Resolve or document every failure without weakening validation.
6. Merge only when required checks pass.
7. Create annotated tag `v0.9.6-4`.
8. Let the tag workflow build Linux, Windows, Intel macOS, Apple Silicon, and
   standalone omenchatd artifacts.
9. Download published artifacts and verify adjacent and aggregate SHA-256
   files.

## Release checklist

### Required

- [x] v0.9.6-3 evidence and ledgers reconciled.
- [x] Selected mechanical `src/app.rs` extractions complete and validated.
- [x] Durable activation covers every mutation advertised by the client.
- [ ] Legacy/mixed peers retain cautious no-automatic-retry behavior.
- [x] Shared bounded Operations/Transfers model drives GUI and TUI.
- [x] Delivery/propagation policies have conservative migrated defaults.
- [x] Command palette, actionable errors, workspace presets, and selected TUI
      QoL pass focus/input/resource tests.
- [ ] Replies/mentions, reactions, search, invitations, corrections,
      tombstones, pins, moderation history, retention, announcement rooms,
      slow mode, and room media policy pass their complete gates.
- [ ] No unbounded queue, cache, history, retry, timer, worker, or index.
- [x] Managed Reticulum remains the supported default.
- [x] External/shared mode remains explicitly deferred and fail-closed.
- [ ] Root and standalone server report `0.9.6-4`.
- [x] Exact Reticulum/LXMF 0.9.6 train remains coherent.
- [ ] Local product/server/protocol/release gates pass.
- [ ] Native CI and bundled interoperability checkpoint pass.
- [ ] Resource measurements show no unexplained regression.
- [ ] Packaging and lifecycle smoke pass on all release platforms.
- [ ] Published artifact checksums are independently verified.
- [ ] README and support claims distinguish tested, upstream-limited, and
      untested behavior.

## Rollback

- `v0.9.6-3` remains the binary rollback.
- Preserve identity, configuration, message history, mutation intents, replay
  rows, room history, search data, and server state during rollback.
- Every schema change requires a pre-migration backup and guarded restore
  procedure.
- Disable a new capability rather than deleting unresolved records.
- Operations history and local search indexes must be rebuildable or removable
  without deleting authoritative messages.
- Each application extraction is source-only and independently reversible.

## First implementation unit

Start with Phase 0 documentation reconciliation and Unit 1A, the mechanical
network smoke/readiness helper extraction. This unit changes no behavior or
persistent state and provides a smaller review surface before durable mutation
activation expands.
