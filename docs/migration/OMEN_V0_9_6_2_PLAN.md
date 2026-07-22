# OMENbrowser and omenchatd v0.9.6-2 plan

Baseline: published tag `v0.9.6-1`, commit `7cbb470`  
Target: `v0.9.6-2`  
Working branch: `release/v0.9.6-2`

## Scope and sequencing

This revision keeps the exact Reticulum/LXMF 0.9.6 train unless a separately
reviewed upstream migration becomes necessary. It does not change OMENchat
protocol version 1, destination aspects, identity ownership, or managed runtime
defaults merely because the application revision changes.

1. Add native Intel and Apple Silicon unsigned DMGs plus separate omenchatd
   archives, checksums, mounted-image validation, and isolated-root application
   lifecycle smoke.
2. Activate the already staged durable OMENchat mutation contract only through
   explicit capability negotiation and the bounded persistent client-intent
   owner.
3. Prove response-loss, replacement-Link, client/server restart, duplicate,
   conflict, downgrade, retention, shutdown, and mixed-version behavior without
   silently replaying an uncertain legacy mutation.
4. Repeat focused resource measurements, full local validation, one bundled
   hosted CI/interoperability/package checkpoint, and release documentation.

Each numbered item is an independent rollback unit. The DMG unit adds no
runtime dependency or product behavior. Durable activation must not be combined
with unrelated UI or Reticulum restructuring.

## Unit 1 — native macOS packages

Status: complete. The repository-owned script uses only macOS system tools and
adds no runtime or build dependency.
Shell syntax, workflow security assertions, `actionlint`, `git diff --check`,
and the complete local quick release gate pass. A manual `macos` package scope
runs only the Intel and Apple Silicon package jobs during this development
checkpoint; tag builds continue to require the full native and artifact graph.

The first Apple Silicon hosted run built and mounted the image and launched the
application, but the smoke test looked for the mounted path in the process
command line. LaunchServices normalized that path, producing a false failure
even though runner cleanup found the live `omenbrowser_rs` process. The smoke
now records pre-existing exact-name processes and owns only the newly launched
process. The redundant Intel job was cancelled rather than spending another
runner cycle on the known-bad assertion.

Focused package run `29946984584` then passed on both native runners. Apple
Silicon completed build, qualification, and upload in 11m53s; Intel completed
the same gates in 27m37s. Both mounted images passed exact version, bundle,
architecture, unsigned-state, isolated-root launch, normal-quit, sentinel, and
checksum checks. No Linux, Windows, Python-interoperability, or general native
CI job was run for this package-only checkpoint.

Required outputs per native runner:

- `OMENbrowser_rs-<version>-macos-x86_64-unsigned.dmg`;
- `OMENbrowser_rs-<version>-macos-aarch64-unsigned.dmg`;
- matching standalone omenchatd `.tar.gz` archives;
- one SHA-256 file per artifact.

The `.app` is built and inspected before image creation. The qualification gate
mounts the DMG read-only, checks exact package/bundle/architecture identity,
launches against an explicit temporary application root, requests normal quit,
proves the isolated sentinel remains, and unmounts the image. No Developer ID,
notarization, universal-binary, service-install, or ordinary Gatekeeper claim is
made.

Cargo revision `0.9.6-2` maps to macOS short version `0.9.6` and numeric bundle
build `906.2`; protocol and database versions remain independent.

## Unit 2 — durable mutation live activation

Status: negotiation slice locally complete; durable envelope sending remains
pending. A browser with a persisted client-instance identity advertises the
bounded capability extension. omenchatd accepts only that known capability and
binds the instance to the authenticated Link. The browser records acceptance
only when it has a matching outstanding request; legacy, downgraded, unsolicited,
reconnected, and retired sessions remain inactive. Normal room mutations still
use the legacy no-automatic-resend path.

The next client boundary is also locally staged but remains unreachable from
ordinary UI actions. It accepts only an already-persisted `sent_uncertain`
intent, validates its negotiated client/server/room ownership, emits the
canonical durable envelope within existing pending-echo bounds, and returns the
mutation identifier on acknowledgement. Desktop intent-worker orchestration and
acknowledgement persistence remain pending; therefore production UI sends have
not switched to this path.

The server transaction/replay executors and browser intent store remain staged
but production capability acceptance is off in v0.9.6-1. Connect them in the
smallest order: client intent ownership, negotiated advertisement/acceptance,
durable envelope send, acknowledgement resolution, reconnect/restart recovery,
and explicit uncertain-state actions. A legacy or downgraded peer must retain
the current no-automatic-resend behavior.

## Unit 3 — release qualification

Status: pending.

Use focused local tests during development. Run the long Python/mixed-version
and native package workflows once after the protocol/storage behavior and
package graph are stable. The known upstream maximum UDP Resource boundary,
physical-radio/public-network tests, hardware-specific GPU measurements, and
unsigned macOS tester requirements remain explicit support boundaries rather
than hidden release claims.

## Rollback

DMG packaging can be removed without touching application or user state.
Durable capability activation must retain the v0.9.6-1 legacy path and guarded
database backups; rollback disables advertisement/acceptance and leaves
uncertain intents visible rather than deleting or resending them. Do not delete
identities, histories, replay records, or pending intents during rollback.
