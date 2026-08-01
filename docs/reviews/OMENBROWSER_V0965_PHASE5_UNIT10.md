# OMENbrowser v0.9.6-5 Phase 5 unit 10 report

Date: 2026-07-31 UTC  
Branch: hardening/v0.9.6-6-phase-plan  
Baseline SHA: 2a77a753e80bb8e7db24a6411d923bf14a8e8722

## Outcome

Mapped and deterministically exercised the locked 0.9.6 public Resource
lifecycle needed by the dormant Resource-reference design. Sender-side
offer-to-hash correlation and cancellation are feasible, but a compliant
large-file direct backend is blocked by whole-vector send/completion and
receiver metadata timing.

## Changes and evidence

A test-only native module creates an in-memory activated encrypted Link using
public APIs. The test proves the pre-dispatch observed hash, method result,
advertisement hash, cancellation selector, and terminal cancellation event are
identical. It separately records the owned-vector shape of ResourceComplete
payload and metadata. Successful advertisement is explicitly not treated as
terminal delivery.

The first minimal native-reticulum run exposed missing test-only PathBuf and
TransportMethod references in adapter.rs. They were repaired inside tests only;
no production feature imports or behavior changed. The minimal lane now passes
and reports 12 pre-existing conditional warnings. The canonical product Clippy
gate remains the warning-free release gate.

## Files changed

- src/runtime/native/mod.rs
- src/runtime/native/resource_reference_evidence.rs
- src/runtime/native/adapter.rs
- docs/design/LXMF_RESOURCE_REFERENCE_ATTACHMENTS_CHECKPOINT.md
- docs/TESTING.md
- docs/upstream/RETICULUM_RS_0_9_6_RESOURCE_REFERENCE_API_REPORT.md
- docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT10.md

## Compatibility, storage, and resources

The module is test-only. No production binary code, transfer, protocol
activation, capability advertisement, queue, task, retry, storage, identity,
schema, UI, or inline attachment behavior changed. The reproducer uses one
small in-memory payload, bounded 200-ms event waits, and no filesystem or real
network interface.

Rollback removes the test module registration/file and documentation, plus the
two test-only import/reference corrections. No data cleanup is required.

## Validation

The four focused commands and their results are recorded in the upstream
report. Also passed:

    cargo clippy --locked --no-default-features --features desktop-product \
      --all-targets -- -D warnings
    cargo fmt --all --check
    git diff --check

Live Resource completion, Python interoperability, external daemon, mixed
versions, restart/resume, filesystem fault injection, packaging, non-Linux
platforms, and hardware were not run and are not claimed.

## Decision and next step

Do not implement the direct large-file backend on 0.9.6 by buffering up to
64 MiB or inventing application fragmentation. Keep the envelope and preview
owner dormant and retain current inline attachments.

The next safe plan work should move to an independent Phase 6 ARM64/headless or
low-power evidence slice, or close another bounded local feature. Revisit this
backend only when the required public streaming/correlation surface exists.
