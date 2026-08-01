# v0.9.6-6 release qualification checklist

Target: `v0.9.6-6`

Baseline commit: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

This checklist reconciles the reviewed v0.9.6-5 phase plan against the newer
checkout. The product version advanced to `0.9.6-6` only after the accepted
deterministic release scope passed. Experimental shared Reticulum runtime work
is excluded by maintainer direction.

## Accepted scope

- [x] Keep the Reticulum/LXMF direct dependency train pinned exactly to `0.9.6`.
- [x] Preserve empty default features and the canonical product identities.
- [x] Preserve independently buildable and packaged `omenchatd` state and
  identity ownership.
- [x] Prove the external SDK/RPC send-field boundary and report its reduced
  guarantees without automatic uncertain retry.
- [x] Recover the auxiliary native LXMF ticket cache after lock poisoning while
  retaining its item and byte bounds.
- [x] Reconcile the authoritative OMENchat capability matrix with production
  client, server, persistence, UI, and downgrade behavior.
- [x] Re-run and document the locked-0.9.6 UDP maximum-Resource reproducer.
- [x] Prove the narrow NomadNet request/Resource adapter's cancellation,
  correlation, interface binding, and no-cross-primitive-replay behavior.
- [x] Reconcile already-active replies/mentions rather than reimplementing them.
- [x] Add a bounded, fail-closed LXMF OMENchat invitation receive/preview path
  with authenticated sender evidence and user confirmation ownership.
- [x] Add event-driven propagation/backend status without a new polling loop.
- [x] Qualify Linux ARM64 headless `omenchatd` through the maintainer-approved
  Podman/Cross/QEMU gate and package lifecycle.
- [x] Add a persisted, default-off low-power policy and repeatable same-binary
  normal/low-power measurement harness.

## Truthful deferred or blocked scope

- [x] External `reticulumd` disconnect/restart remains unclaimed because an
  exact compatible daemon executable is unavailable on the qualification host.
- [x] Maximum legal UDP Resource transfer remains a documented upstream 0.9.6
  transmit-buffer limitation; no private fork or incompatible fragmentation was
  added.
- [x] Outbound LXMF OMENchat invitations remain disabled until a live peer
  capability probe succeeds.
- [x] NomadNet LXMF topic pointers remain dormant because authenticated
  publisher and cursor/gap evidence is unavailable from the locked public API.
- [x] LXMF OMENchat notices remain dormant.
- [x] Large Resource-reference attachments remain dormant because the locked
  public API requires whole-vector send/completion and exposes receiver metadata
  too late for the reviewed admission contract.
- [x] Experimental shared Reticulum runtime is not part of this release.
- [x] Hardware-specific ARM64 and native GPU claims are not made. Successful
  Podman/Cross/QEMU evidence is the accepted ARM64 headless release boundary.

## Recorded deterministic and resource evidence

- [x] Phase 0 baseline and Phase 1 through Phase 6 reports exist under
  `docs/reviews/`.
- [x] Isolated OMENchat restart/reconnect and exact durable-replay evidence
  passed; uncertain client work is never resent automatically.
- [x] Static-media and canonical product identities are machine-distinguishable.
- [x] Paired normal/low-power measurements use the same binary hash, isolated
  roots, validated settings, controlled ordering, and raw output retention.
- [x] Normal/low-power software-rendered monitoring evidence recorded materially
  lower median CPU and task-clock consumption without queue, identity,
  persistence, or network-semantic changes. The p95 result is not claimed as an
  improvement.
- [x] Canonical/static release binary size comparison is recorded.

## Final local deterministic gates

- [x] `cargo fmt --check`.
- [x] Canonical desktop tests and all-target Clippy with warnings denied.
- [x] Static-media desktop tests.
- [x] TUI tests and all-target Clippy with warnings denied.
- [x] Standalone `server-full` tests and all-target Clippy with warnings denied.
- [x] `bash scripts/release-check.sh full`.
- [x] Product versions changed together to `0.9.6-6` only after the preceding
  accepted-scope gates pass.
- [x] Version consistency, dependency-train, feature-identity, workflow-security,
  and accepted-advisory checks pass after the version change.
- [x] `bash scripts/release-package.sh`.
- [x] `bash scripts/release-check.sh package` against the new archive.

## Release-candidate hosted and live gates

Run these once for the complete candidate rather than on every intermediate
commit.

- [ ] Pull-request CI passes.
- [ ] Pinned Python interoperability passes.
- [ ] Current-Python drift lane produces its report.
- [ ] Native Linux, Windows, Intel macOS, and Apple Silicon macOS packaging
  passes; macOS artifacts include the maintained DMG family.
- [ ] Two-client isolated OMENchat smoke passes on the candidate binaries.
- [ ] Relevant direct/propagated LXMF and NomadNet request/Resource live smokes
  pass, or an exact environment limitation is recorded.
- [ ] Artifact versions, checksums, and machine-readable manifests report
  `0.9.6-6` and contain no development/mock feature path.
- [ ] Reviewed candidate commit is merged to `main`.
- [ ] Annotated `v0.9.6-6` tag resolves to the reviewed `main` commit.
- [ ] Published release contains the reviewed native artifacts and release
  notes.

## Release blockers

Any deterministic gate failure, undocumented advertised capability, data-root
crossing, automatic uncertain retry, unbounded work, development/mock release
feature leakage, dependency-train drift, or version/artifact mismatch blocks
the release. The explicitly documented dormant/upstream-limited items above do
not block this bounded release scope.
