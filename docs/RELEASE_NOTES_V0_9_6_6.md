# OMENbrowser_rs and omenchatd v0.9.6-6 release notes

Reticulum/LXMF crate train: exact `0.9.6`

Status: release-candidate draft. Product manifests advanced together only after
the accepted local scope passed its deterministic gates.

## Correctness and interoperability

- External LXMF RPC sends now expose their real guarantee boundary. The
  published client preserves delivery method, propagation fallback, stamp
  policy, fresh-ticket choice, and cancellation identity, but does not preserve
  TTL/expiry, idempotency, correlation, extensions, or an explicit remembered
  reply ticket. OMEN retains local deadline enforcement and never retries an
  uncertain send automatically.
- Native LXMF ticket-cache poisoning now clears and recovers the bounded
  auxiliary cache without exposing ticket material or panicking the product.
- The NomadNet request adapter has deterministic evidence for direct requests,
  request Resources, independent direct/Resource responses, bound-interface
  dispatch, correlation, timeout, cancellation, and no automatic replay through
  a different primitive.
- OMENchat capability documentation now follows the shared protocol vocabulary
  and actual production request, acceptance, handling, persistence, UI, and
  downgrade paths.
- Restart and reconnect evidence covers uncertain durable mutations, exact
  replay, conflict rejection, server persistence, and Link replacement without
  automatic resend.

## OMENchat and LXMF handoff

- Incoming LXMF OMENchat invitations use bounded fields, version and
  destination validation, expiration, token-redacted diagnostics, authenticated
  sender evidence, replay policy, and a user-controlled preview. Receiving an
  invitation never automatically connects, joins, trusts, or grants a role.
- The managed native runtime owns a bounded local invitation capability endpoint
  and cancellation-safe diagnostics probe. Outbound invitation sending remains
  disabled until live peer capability evidence exists.
- Diagnostics include event-driven propagation/backend state without adding a
  recurring high-frequency network poll.

## Resource efficiency and platform coverage

- A default-off Low-power mode forces static media presentation and reduces the
  visible monitoring sample cadence from one second to five seconds without
  changing Reticulum, LXMF, OMENchat, identity, or persistence semantics.
- The paired measurement harness validates isolated settings, records the exact
  binary hash and case order, and retains raw normal/low-power evidence. Current
  software-rendered evidence shows substantially lower median CPU and task-clock
  use; p95 CPU is not claimed as improved.
- The static-media product reports its own canonical profile identity.
- Linux ARM64 headless `omenchatd` and `omenchat-protocol` pass the maintained
  Podman/Cross/QEMU compile, test, and isolated package-lifecycle gate. This is
  not a claim of Raspberry Pi hardware qualification.

## Deliberately unavailable in this revision

- The locked 0.9.6 maximum-packet UDP Resource reproducer still fails at the
  upstream transmit-buffer boundary. OMEN does not hide it with a fork,
  weakened limit, incompatible fragmentation, or unbounded retry.
- External `reticulumd` disconnect/restart behavior remains unclaimed where an
  exact daemon executable was unavailable for testing.
- Outbound LXMF OMENchat invitations, LXMF room notices, NomadNet topic-pointer
  activation, and large Resource-reference attachments remain disabled or
  dormant pending the documented capability, provenance, cursor, and streaming
  evidence.
- Experimental shared Reticulum runtime is outside this release scope.

## Compatibility and rollback

- Reticulum/LXMF direct dependencies remain pinned to exact `0.9.6` registry
  releases; there is no private fork or `[patch.crates-io]` override.
- OMENchat remains wire protocol version 1. Existing capability negotiation
  continues to isolate older peers from unsupported extensions.
- Browser and omenchatd database schema versions and storage roots are
  unchanged.
- `omenchatd` remains independently buildable, configured, packaged, and
  deployed.
- The release introduces no mandatory persistent-data migration. Rolling back
  to `v0.9.6-5` does not require a database downgrade; preserve normal identity,
  configuration, and message backups as usual.
