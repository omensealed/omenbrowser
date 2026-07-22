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

Status: negotiated room-text activation, conservative restart recovery,
non-network terminal resolution, and deliberate user-confirmed retry are
locally complete. Startup owns the bounded
intent worker only when the persistent client instance and authenticated active
identity are both available. Only then does the browser advertise the bounded
capability extension. omenchatd accepts only that known capability and binds the
instance to the authenticated Link. The browser records acceptance only when it
has a matching outstanding request; legacy, downgraded, unsolicited,
reconnected, and retired sessions remain inactive.

An ordinary room-text send on a negotiated session now queues a bounded worker
operation, commits `prepared`, transitions it to `sent_uncertain`, and only then
uses the validated durable-envelope boundary. The draft remains intact on
admission, persistence, transition, negotiation, or transport failure. A
matching acknowledgement is queued back to the same owner and persisted as
`acknowledged`. Worker replies are awaited through bounded blocking tasks rather
than blocking Iced or a Tokio worker. Negotiated sessions fail closed if the
persistence owner is unavailable; legacy and downgraded sessions retain the
unchanged legacy no-automatic-resend behavior.

The focused integration test proves negotiated activation, fail-closed owner
loss, prepared-before-transport ordering, uncertain-before-send ordering,
transport output, draft retention/clearing, and terminal acknowledgement
persistence. No prepared or uncertain intent is automatically replayed after a
restart. Room actions, commands, and other mutation types still use their
existing legacy path unless a later focused unit explicitly activates them.

The first maintenance deadline now submits one bounded recovery request. Its
reply is received off the async worker, filtered to the active authenticated
identity and persistent client instance, and retained within the store's
existing 4,096-item/16-MiB recovery ceiling. Other-identity records are counted
without exposing their bodies or identifiers. Prepared, uncertain, and
past-expiry counts are surfaced in redacted session diagnostics together with
worker queue health. Recovery changes no intent state and emits no transport
frame; the application status explicitly states that nothing was resent.

Each server pane shows at most four active-identity recovered entries at once,
with a bounded text preview and truthful `prepared`, `uncertain`, or
past-expiry language. A confirmation step is required before changing storage.
`Stop Tracking` transitions prepared or uncertain records to `abandoned` and
explicitly makes no delivery claim. `Finalize Expired` is available only after
the persisted deadline and transitions the record to `expired`. Both use the
bounded intent worker and emit no frame. Concurrent acknowledgement or another
terminal transition wins safely and removes the stale recovery row from the UI.

Before expiry, `Send Prepared` and `Retry Safely` require confirmation and
revalidate the active authenticated identity, persistent client instance,
original server and room, live transport, and current negotiated capability.
A prepared record is persisted as `sent_uncertain` before transport. An
uncertain record reuses its existing mutation ID, canonical hash, and body;
retry never creates a replacement operation. A mutation already awaiting a
response in the same process is rejected, recovered retries do not clear a
newer composer draft, and acknowledgement removes the recovered row. Nothing
is retried automatically.

The server transaction/replay executor and browser intent store are active only
for explicitly negotiated room-text operations. Client intent ownership,
advertisement/acceptance, durable envelope send, acknowledgement resolution,
restart recovery, and explicit uncertain-state actions now form one guarded
path. Room actions, commands, and other mutation types remain on their current
legacy paths. A legacy or downgraded peer retains the no-automatic-resend
behavior.

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
