# OMENbrowser v0.9.6-5 Phase 5 unit 9 report

Date: 2026-07-31 UTC  
Branch: hardening/v0.9.6-6-phase-plan  
Baseline SHA: 2a77a753e80bb8e7db24a6411d923bf14a8e8722

## Outcome

Completed Phase 5.2 rollout step 2 at a caller-inert boundary: a bounded
pending-offer owner can produce and locally reject a Resource-reference preview
without transferring bytes. It has no runtime, application-state, UI, storage,
or network caller and deliberately exposes no accept method.

## Ownership and bounds

The owner requires the envelope's signed sender claim to match separately
provided authenticated sender evidence. A caller-supplied conversation key is
limited to 128 visible ASCII bytes and is retained only as opaque context,
never as a filesystem path.

Pending metadata is capped at 32 items/64 KiB globally, 8 items/16 KiB per
authenticated peer, and 8 items/16 KiB per conversation. Accounting includes
the exact encoded input, sender, and conversation key. Admission is limited to
eight offers per peer and 64 globally per ten-minute window, with at most 256
rate records. Overload rejects synchronously without waiting or task creation.

References are namespaced by authenticated sender. An exact replay is rejected
as a duplicate; reuse with changed metadata or conversation context is rejected
as a conflict. Local rejection releases pending byte accounting but retains its
rate record. Cleanup removes at most eight expired pending/rate records per
call, and explicit clearing releases all ephemeral state.

## Files changed

- src/messaging/mod.rs
- src/messaging/resource_reference.rs
- docs/design/LXMF_RESOURCE_REFERENCE_ATTACHMENTS_CHECKPOINT.md
- docs/TESTING.md
- docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT9.md

## Compatibility, storage, and resources

Existing inline LXMF attachments are unchanged. No capability advertisement,
message-title recognition, acceptance frame, Resource transfer, network
response, database/configuration/schema change, identity change, filesystem
write, media decode, cache, worker, timer, retry, or UI action was added.

The maximum pending metadata accounting is 64 KiB plus bounded container
overhead; rate accounting is capped at 256 small records. All work is
caller-driven and bounded by at most 32 pending entries. Rollback removes the
owner/export additions and matching documentation; no migration or persisted
cleanup is required.

## Validation

Passed focused gate:

    cargo test --locked --no-default-features resource_reference --lib

Fourteen tests cover the envelope and pending owner, including authenticated
sender binding, inert preview/local rejection, duplicate/conflict handling,
rejection-resistant peer/global rates, item and byte ceilings at all three
scopes, invalid conversation context, incremental pruning, and shutdown
clearing.

Also passed:

    cargo clippy --locked --no-default-features --all-targets -- -D warnings
    cargo clippy --locked --no-default-features --features desktop-product \
      --all-targets -- -D warnings
    cargo fmt --all --check
    git diff --check

Live Resource transfer, capability negotiation, mixed versions, external
daemon, Python interoperability, packaging, non-Linux platforms, and hardware
were not run and are not claimed.

## Remaining blockers and next gate

The next rollout step would be one direct transfer backend, but it is not yet
safe to implement. First define an authenticated accept/reject exchange and
prove exact offer-to-Link-Resource correlation, source-file mutation/lifetime
rules, bounded streaming storage, cancellation, and capability downgrade
behavior. Until then the owner remains disconnected and cannot accept an offer.

A safe next independent unit is a locked-0.9.6 public Resource lifecycle/API
mapping and deterministic correlation reproducer. It should determine whether
the required accept-to-observed-hash binding is possible without a private fork
or invented Reticulum behavior.
