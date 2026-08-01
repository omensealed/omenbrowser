# OMENbrowser v0.9.6-5 Phase 5 unit 2 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added a caller-inert, frontend-neutral admission owner for followed NomadNet
update targets and update-pointer evidence. Runtime topic support remains
unproven and no topic or browser operation is active.

## Ownership and bounds

- 64 followed targets and 64 KiB of followed destination/path strings.
- 128 retained notices and 64 KiB of encoded-pointer/publisher bytes.
- 512 fixed-size publisher-rate records.
- Eight admitted pointers per canonical publisher per ten-minute window.
- Destination/path/revision deduplication.
- At most eight expired notice/rate records removed per prune call.
- Explicit authenticated or unverified publisher evidence that payload fields
  cannot upgrade.
- Unfollow removes matching bounded notices; clear releases all ephemeral state.
- Automatic page fetch is always disabled.

Capacity and overload produce typed rejection. No waiting, eviction loop,
background task, recurring timer, retry, persistence, cache, SDK call, network
operation, page fetch, rendering, or UI state was introduced.

## Files changed

- `src/browser/update_pointer.rs`
- `docs/design/NOMADNET_LXMF_UPDATE_POINTER_CHECKPOINT.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT2.md`

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  update_pointer --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Seven focused tests pass. They cover codec validation, deduplication, exact
follow and notice item/byte ceilings, per-publisher rate limits, authority
preservation, no-auto-fetch semantics, unfollow accounting, clear, expiry, and
batch-limited pruning.

Not run: live SDK topics, external daemon, Python interoperability, packaging,
native non-Linux platforms, or physical peers. This owner has no runtime caller,
so none of those behaviors can be inferred.

## Compatibility, rollback, and next gate

No existing protocol, configuration, database, identity, cache, attachment, or
browser behavior changed. Roll back the owner types/tests and the matching
documentation; no data cleanup is required.

The next safe gate is a compiler-verified audit of exact locked SDK topic
capabilities and event provenance for managed-native and external backends. Do
not connect this owner until authenticated publisher evidence, snapshot/gap
recovery, cancellation, and shutdown ownership are proven.
