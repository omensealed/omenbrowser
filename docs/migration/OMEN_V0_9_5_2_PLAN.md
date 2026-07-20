# OMEN v0.9.5-2 Implementation Plan

## Baseline and release identity

This plan starts from published tag `v0.9.5-1`, commit
`e9e06a7b456ac6ae0ce91004cbbb41ac5f522a0a`. OMENbrowser_rs and the standalone
omenchatd remain on the exact reviewed Reticulum/LXMF `0.9.5` registry train
unless a later, separately approved dependency-migration unit changes that
baseline.

The next application release is `0.9.5-2` with Git tag `v0.9.5-2`. The package
version does not change the OMENchat wire version, SQLite schema, configuration
schema, destination aspects, identity format, cache format, or RPC contract.
Manifest and lockfile versions will move together in a dedicated release
identity unit after the first feature unit is accepted; development work must
not create or move a release tag.

## Already complete; do not rebuild

The current product already provides:

- explicit preferred propagation-node selection and clearing;
- manual and post-acceptance propagation sync;
- typed bounded propagation sync progress and recovery events;
- authenticated propagation announce metadata and stamp-cost parsing;
- bounded sender-path recovery and duplicate suppression;
- ticket/stamp, pinned/current Python, mixed-version, restart, and crash tests;
- read-only lifecycle, capability, interface, path, and propagation diagnostics;
- restart-scoped interface profile controls;
- deterministic managed-runtime ownership and refusal of deferred external mode.

New work must extend these owners rather than add a second state store, polling
loop, sync worker, propagation protocol, or UI-only interpretation.

## Release scope

### Required units

1. **Propagation-node inventory and health projection**
   - Derive a project-owned, read-only node record from bounded authenticated
     directory/announce and path state.
   - Show identity hash, authenticated display name when present, selected
     state, last-seen age, path state, advertised stamp cost, compatibility,
     and evidence freshness.
   - Bound the inventory by items and retained bytes, define deterministic
     eviction, and expire stale health without deleting saved directory data.
   - Mark unknown values as unknown; do not infer trust or health from a name.

2. **Explicit refresh, selection, and sync workflow**
   - Reuse the existing path-request, preferred-node, propagation diagnostics,
     and sync owners.
   - Provide one deliberate refresh action with cooldown, in-flight coalescing,
     cancellation, and visible outcome.
   - Keep node selection manual. Changing a node must persist atomically and
     update the active runtime through the existing adapter boundary.
   - Present existing sync stage/progress/terminal state without a new timer or
     duplicate worker.

3. **Failure recovery and operator evidence**
   - Distinguish no node, no path, stale announce, unsupported stamp cost,
     unavailable runtime, queue overload, timeout, cancellation, and malformed
     metadata.
   - Add a bounded redacted diagnostic export for the inventory and selected
     node. Exclude identity secrets, tickets, stamps, payloads, and private
     filesystem paths.
   - Preserve the current node and queued messages across restart; never select
     another node automatically in this release.

4. **Release identity and qualification**
   - Set root and server manifests/lockfiles to `0.9.5-2` together.
   - Update user-facing release/version documentation without changing protocol
     or schema versions.
   - Run the complete product, native-platform, package, interop, security, and
     performance gates before tagging.

### Optional units, admitted one at a time

- propagation-node comparison and a user-confirmed switch recommendation;
- queue/peer/router statistics when supplied by authoritative current APIs;
- improved NomadNet request/download diagnostics and progress presentation;
- read-only external/shared-instance discovery through a secure local endpoint;
- common-interface configuration guidance and restart validation;
- a redacted support bundle that orchestrates existing diagnostics tools.

Each optional unit requires its own observed problem, invariant, dependency and
configuration review, tests, measurements, rollback, and completion gate. It
must not be bundled with the required propagation-manager unit.

## Explicitly deferred

- automatic propagation-node failover or trust decisions;
- live interface mutation without a negotiated typed API and native tests;
- unauthenticated or non-loopback remote RPC;
- hardware/radio support claims without hardware-in-the-loop evidence;
- removal of IFAC, request-resource, or OMENchat compatibility fallbacks;
- a broad Reticulum/LXMF upgrade, local upstream patch, or private fork;
- protocol, database, identity, destination, or unrelated Iced redesign work.

## First implementation unit

Status: **bounded projection, desktop/TUI presentation, and refresh lifecycle ownership implemented and accepted in PRs #9-#11; release identity alignment is under validation**.
The directory owner now derives a deterministic, read-only inventory capped at
256 records and 512 KiB. Diagnostics include authoritative selected-node path
state when the runtime supplies it; synchronous UI projections leave path state
unknown. No network action, polling loop, or new persistence was added.

### Problem

Selection, diagnostics, and sync exist, but an operator must currently assemble
node suitability from separate Directory, path, diagnostics, and message views.
That makes a safe manual choice harder without creating any missing transport
capability.

### Invariants

- One authoritative project-owned projection; no duplicate network state.
- Only authenticated announce metadata can supply a node name or stamp policy.
- Unknown and stale evidence remain visibly distinct from negative evidence.
- Inventory, event delivery, and rendered rows are bounded by items and bytes.
- No network work starts from rendering, periodic redraw, or passive selection.
- Refresh and sync remain explicit, cancellable, and single-flight.
- Existing saved node, messages, identities, and configuration remain readable
  by `v0.9.5-1`.

### Expected source boundary

- runtime/network projection types and conversions;
- directory service selection and bounded persistence owners;
- desktop/TUI view models and message routing;
- existing diagnostics export;
- focused fixtures and tests;
- `docs/LXMF_DELIVERY_AND_EVENT_MODEL.md`, `docs/NETWORK_BACKENDS.md`, and the
  Reticulum migration ledger.

UI code must not import Reticulum/LXMF implementation types. omenchatd is not
changed by this desktop feature unit.

### Tests

- authenticated, malformed, unknown, stale, and refreshed announce metadata;
- item/byte ceilings and deterministic eviction;
- saved-selected node retained when transient inventory is full;
- path known/unknown/expired and stamp supported/unsupported/unknown states;
- refresh cooldown, coalescing, overload, cancellation, timeout, and shutdown;
- atomic selection persistence and runtime notification failure recovery;
- restart reconstruction without duplicate nodes or automatic reselection;
- redacted bounded diagnostics;
- exhaustive desktop/TUI routing, keyboard/focus, and narrow-layout behavior;
- pinned/current Python announce and manual sync regression;
- existing mixed-version propagation and restart cases.

Tests use explicit isolated temporary roots and must not read or mutate real
Reticulum, LXMF, OMENbrowser, or omenchatd state.

### Measurements

Record before/after:

- idle CPU and application messages/wakeups per minute;
- retained node inventory item and byte counts;
- diagnostics export size;
- refresh-to-path/status latency;
- propagation sync latency and event count;
- RSS during maximum inventory and repeated refresh;
- task/link/handle count after refresh cancellation and shutdown.

No fixed polling subscription is admitted. A greater than ten-percent
unexplained idle CPU, RSS, task, or link regression requires investigation.

### Completion gate

- Focused tests and strict Clippy pass for affected profiles.
- Root mock, desktop-product, and TUI matrices pass.
- Standalone omenchatd remains independently green and unchanged.
- Pinned/current Python and mixed-version propagation regressions pass.
- Advisory, dependency-source, feature-identity, and quick release gates pass.
- Windows MSVC, Intel macOS, and Apple Silicon native checks pass.
- Before/after measurements and rollback evidence are recorded.

### Rollback

Remove the projection, view-model integration, and new diagnostics fields as one
unit. Retain the existing preferred-node setting, runtime selection call,
manual/automatic sync workers, directory entries, messages, and compatibility
paths. No state conversion or destructive cleanup is permitted.

## Later release gate

After all accepted units, run the non-publishing package workflow on the exact
merged `main` commit. Only after Linux artifacts, Windows portable/NSIS/WiX
lifecycle checks, and all native prerequisites pass may `v0.9.5-2` be created.
Tag publication must remain isolated behind the existing least-privilege
tag-only workflow.
