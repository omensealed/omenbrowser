# OMENchat Room Media-Policy Activation Review

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `793c6f0`, plus this activation unit.

## Decision

Verdict: activate `room-media-policy-v1` in the canonical animated/static
desktop and standalone headless/full server profiles for `v0.9.6-4`.

The production capability is dependency-free and cumulative over the already
active durable-mutation, announcement-room, and slow-mode capabilities. The
separate `omenchat-room-media-policy-qualification` feature now implies the
production feature and owns only deterministic GUI/process hooks. Product
feature verification requires the production feature and continues to reject
the qualification feature.

## Default and compatibility behavior

- Existing schema-13 `NULL` room values inherit the current global server file
  ceiling. Upgrading therefore introduces no new default restriction.
- Zero disables room uploads and positive values select the lower of the room
  and global file ceiling, exactly as shown by stopped-server administration.
- Policy is applied only when the current authenticated Link explicitly
  negotiated the complete capability set.
- Non-negotiating and adjacent peers retain their byte-exact legacy room shape,
  three-field generic upload errors, and prior global upload admission.
- Current peers receive the seven-field room value and typed trailing rejection
  code only after request/accept.
- Offer and Resource publication independently recheck the same Link-scoped
  authority and current store policy.
- Identity replacement, Link close, reconnect, and server restart clear or
  rebuild authority through the existing lifecycle.

The negotiation scope means a legacy peer can still upload under the global
limit even when a room has a stricter configured value. This is intentional
wire compatibility, not a claim that the room value is a security boundary
against old clients. Operators requiring a universal hard ceiling must retain
the global server limit until unsupported peers are excluded by a separate
protocol policy.

## Evidence reconciled

The activation decision is supported by:

- byte-exact independent wire fixtures and bounded parsing;
- schema-13 migration/fault/restart and guarded schema-12 copy export;
- transactional stopped-server administration and bounded status output;
- explicit offer and Resource-publication enforcement;
- client projection and static Iced admission controls;
- current/current real-Link success, rejection, restart, and recovery;
- 32-Resource bounded retention/quota measurement;
- native Linux Iced accepted, over-limit, and disabled cases;
- optimized CPU/RSS/thread/FD/queue/shutdown observation;
- immutable adjacent `v0.9.6-3` process traffic in both directions;
- simultaneous legacy/negotiated current-server shaping and admission.

The locked Reticulum 0.9.6 API still has no public receiver-side cancellation
operation. Activation does not fabricate one: outbound initiator cancellation,
transport failure, Link close, offer expiry, and shutdown cleanup remain
bounded; the UI does not present a false receiver-cancel action. This known
upstream limitation is visible and is not a reason to leave validated
admission policy dormant.

## Implementation

Both manifests add `omenchat-room-media-policy`. Canonical product aliases
include it, and qualification aliases depend on it. The client requests the
capability only when the production feature is compiled. Normal server
constructors accept and enforce it only when the production feature is
compiled.

One masked boundary was corrected during activation: generic session helpers
now default to **non-negotiated** media policy. Only the production live Link
dispatcher or an explicit test call can pass negotiated authority. Compilation
of a feature can never stand in for Link negotiation.

Human, JSON, and TUI room status now derive `active`/`inactive` from the
production feature. Stored configuration remains visible in feature-disabled
builds but is truthfully labeled inactive.

## Storage and protocol impact

No new protocol operation, byte shape, capability label, schema migration,
configuration key, identity, storage path, or dependency was introduced in
this unit. It activates the already qualified schema and wire contract.

Existing browser state remains readable. Existing server databases remain at
schema 13. No automatic rewrite or deletion occurs.

## Rollback

Fast source rollback:

1. remove `omenchat-room-media-policy` from both canonical product aliases;
2. rebuild root and standalone server together;
3. keep schema-13 room values intact and report them inactive;
4. do not delete identities, messages, uploads, or room policy.

Binary rollback to published `v0.9.6-3`:

1. stop omenchatd cleanly;
2. preserve the active schema-13 database and sidecars as a backup;
3. create and validate the guarded schema-12 copy with
   `database export-schema12-copy`;
4. install the matching prior client/server binaries together;
5. move the original database aside and place the validated copy at the
   configured database path;
6. retain browser identity/history/cache and server identity/upload roots.

Clients automatically fall back when the peer does not accept the capability.
No protocol downgrade message or destructive client migration is required.

## Remaining release evidence

- later batched hosted CI, Python interoperability, native packaging and
  package lifecycle checks;
- public-network, physical-interface, physical-GPU, and optional longer soak
  evidence remain accurately unclaimed.

## Local activation validation

The exact activated product graph passed on 2026-07-28/29:

- root formatting, standalone formatting, shell syntax, ShellCheck, and
  `scripts/verify-product-features.sh`;
- `omenchat-protocol`: 59 passed;
- canonical animated desktop library tests: 1,528 passed, 31 explicit
  live/hardware/measurement cases ignored;
- canonical animated desktop strict Clippy across all targets;
- canonical static-media desktop compilation;
- canonical standalone `server-headless`: 432 passed, 12 ignored;
- canonical standalone `server-headless` strict Clippy across all targets;
- canonical standalone `server-full` compilation and strict Clippy across all
  targets.

The canonical binaries were then built without the qualification feature and
run through an isolated real-Link smoke. A 32-KiB room ceiling rejected a
64-KiB upload before admission, the server restarted orderly, the destination
remained stable, and the reopened client recovered the projected policy. The
smoke passed with isolated browser/server roots and no IFAC secret.

`bash scripts/release-check.sh quick` also passed on the same source tree,
including version/dependency/advisory checks, native CLI identities, TUI
lifecycle and real-PTY restoration, product feature verification, focused
OMENchat tests, standalone relocation, IFAC vectors, and isolated omenchatd
configuration tests. Explicit pinned-Python network tests remained ignored by
this local quick lane and are reserved for the later batched interop checkpoint.

The initially activated server suite caught one regression before this result:
a generic session helper had treated compile-time feature presence as Link
negotiation. The corrected helper now defaults to non-negotiated authority, and
the upload-policy regression plus the full matrix pass with that boundary.
