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
  room metadata updates, and `/create` room creation.
  The persistent intent contract admits `RoomMessage`, `RoomAction`,
  `RoomNotice`, `PartRoom`, and `Command`, while the desktop production sender
  currently prepares the first four operation families plus the narrowly
  admitted topic command.
- The server durable executor and replay store already cover room-text and
  command mutation families with bounded transactional replay behavior.
- Network Doctor and delivery evidence provide useful component state, but
  there is no single project-owned Operations/Transfers history shared by GUI
  and TUI.
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
| `role`, `unban` | transactionally durable and replay-safe | deferred to target-resolution unit |
| `kick`, `ban`, `mute`, `unmute` | durable database result plus live target effects | deferred to moderation side-effect unit |

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

### Unit 2D — recovery UX

Use the existing bounded intent owner and recovered-intent UI for all newly
activated kinds. Show operation kind, room/server, state, expiry, and allowed
action without exposing mutation IDs or message bodies in logs.

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

### Unit 6B — reactions

- Append-only add/remove events keyed by event ID, actor, and bounded reaction.
- Per-event/per-actor/global retention limits.
- Durable exact replay without duplicate counts or fan-out.

### Unit 6C — bounded local search

- Search existing LXMF and OMENchat history by text, sender, room, date,
  attachment name, and delivery state.
- Verify packaged SQLite FTS5 before choosing it. Otherwise use bounded
  parameterized SQLite queries or a bounded token index.
- Incremental, cancellable, low-priority indexing with explicit queue and batch
  bounds if an index worker is required.

### Unit 6D — safe invitations

- Include server destination, room, verified server fingerprint/evidence, and
  optional label.
- Never include passwords, IFAC secrets, bearer tokens, or reusable moderation
  credentials.
- Bound URI/QR input before decode and require user confirmation before trust
  or connection changes.

### Unit 6E — corrections and delete tombstones

- Append correction and tombstone events rather than rewriting history.
- Preserve immutable original IDs and moderation/audit evidence.
- Define bounded correction depth/count and tombstone retention.
- Durable replay must not repeat edits, deletions, notifications, or fan-out.

### Unit 6F — pins and moderation audit history

- Append bounded pin/unpin and moderation-audit events.
- Enforce current server roles transactionally.
- Expose audit history only to authorized users and redact private operational
  evidence.
- Bound retained audit records by room, age, count, and bytes.

### Unit 6G — room policy controls

- Retention policies with explicit defaults and guarded migration.
- Read-only announcement rooms.
- Slow mode with monotonic server enforcement.
- Per-room upload and media policy.
- Clear client evidence when policy rejects an operation.

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
- [ ] Selected mechanical `src/app.rs` extractions complete and validated.
- [ ] Durable activation covers every mutation advertised by the client.
- [ ] Legacy/mixed peers retain cautious no-automatic-retry behavior.
- [ ] Shared bounded Operations/Transfers model drives GUI and TUI.
- [ ] Delivery/propagation policies have conservative migrated defaults.
- [ ] Command palette, actionable errors, workspace presets, and selected TUI
      QoL pass focus/input/resource tests.
- [ ] Replies/mentions, reactions, search, invitations, corrections,
      tombstones, pins, moderation history, retention, announcement rooms,
      slow mode, and room media policy pass their complete gates.
- [ ] No unbounded queue, cache, history, retry, timer, worker, or index.
- [ ] Managed Reticulum remains the supported default.
- [ ] External/shared mode remains explicitly deferred and fail-closed.
- [ ] Root and standalone server report `0.9.6-4`.
- [ ] Exact Reticulum/LXMF 0.9.6 train remains coherent.
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
