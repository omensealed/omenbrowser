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

Status: **bounded projection, desktop/TUI presentation, refresh lifecycle
ownership, release identity alignment, and the isolated desktop idle comparison
are accepted; exact-final-commit artifact qualification remains**.
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

#### Recorded desktop idle comparison

On 2026-07-20, the published `v0.9.5-1` Linux product binary and the qualified
`0.9.5-2` Linux product binary from package run `29756591192` were measured with
the same `scripts/measure-desktop-idle.sh` harness, isolated temporary roots,
Xvfb/i3 session, 15-second warmup, and 60 one-second samples. Both binaries
closed normally after the sample window.

| Metric | `v0.9.5-1` | `0.9.5-2` | Result |
| --- | ---: | ---: | --- |
| CPU median | 0.976% | 0.486% | 50.20% reduction |
| CPU p95 | 2.954% | 1.970% | 33.31% reduction |
| RSS median | 222,192 KiB | 225,660 KiB | 1.56% increase |
| Private-dirty median | 42,480 KiB | 42,768 KiB | 0.68% increase |
| File descriptors median | 60 | 60 | unchanged |
| `perf` task clock | 555.58 ms | 538.38 ms | 3.10% reduction |

The idle CPU and memory review gate passes: neither memory measure approaches
the ten-percent investigation threshold, and CPU does not regress. The
scheduler context-switch proxy increased from 48.814 to 91.525 per minute, but
that process counter is not an application-message count and is not represented
as one. Direct recurring application-message instrumentation remains pending.
The feature adds no recurring subscription or timer; its inventory is cached
and refreshed only by explicit user action or existing authoritative events.

Deterministic tests establish the 256-item/512-KiB inventory ceilings, the
six-second refresh deadline, coalescing, cancellation, and shutdown ownership.
Hardware/network-specific refresh-to-path and propagation-sync latency remain
live-test evidence rather than invented local values. Raw local evidence is
kept under ignored `target/v0.9.5-2-idle-comparison/`; the immutable binary
SHA-256 values are `24534e5efff6cad9a25d37cf03928e96a567c818adb7618cddbb45becaf7a74e`
for `v0.9.5-1` and
`a7303d78a93f637bd225cdbfc6cab14f43c05a186e409eb08e0826277a0c4525`
for the measured `0.9.5-2` candidate.

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
merged `main` commit. Package run `29756591192` qualified the code and portable
checksum fix on commit `8ed3c9cf5444de0e72bc228513f583086fdd149b` across Linux, native Windows,
Intel macOS, and Apple Silicon macOS, but this evidence-recording documentation
unit changes packaged bytes. One final non-publishing run on its merge commit is
therefore still required. Only after Linux artifacts, Windows
portable/NSIS/WiX lifecycle checks, and all native prerequisites pass may
`v0.9.5-2` be created. Tag publication must remain isolated behind the existing
least-privilege tag-only workflow.
