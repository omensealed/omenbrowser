# OMENbrowser v0.9.6-5 Phase 5 unit 7 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added the caller-inert admission owner for dormant asynchronous OMENchat LXMF
notices. It is disabled by default, entirely in memory, and has no runtime,
application-state, UI, storage, or send caller.

## Design and invariants

Admission requires a canonical authenticated-sender value supplied separately
from the untrusted payload and explicit opt-in for the notice kind. The owner
does not claim to derive or authenticate that evidence. A future transport
boundary must do so authoritatively; generic topic peer metadata and payload
claims remain insufficient.

Deduplication is scoped by authenticated sender plus 128-bit notice ID. The
owner accepts at most eight notices per sender and 64 notices globally per
ten-minute window, retaining at most 512 bounded rate records. Retained notices
are capped at 128 items and 64 KiB, accounting for the exact encoded input and
sender identifier. Capacity exhaustion rejects immediately and creates no task
or wait queue.

Only a newer followed-room summary from the same authenticated sender, server,
and room can replace an earlier one. The incoming activity count replaces the
prior count; counts are never summed or inferred. Cleanup removes at most eight
expired notice/rate entries per call. Explicit shutdown clearing releases all
ephemeral records while preserving the caller-owned opt-in selection. Every
retained record reports that it permits no automatic action.

## Files changed

- `src/chat/notice.rs`
- `docs/design/OMENCHAT_LXMF_NOTICES_CHECKPOINT.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT7.md`

## Compatibility, storage, and resources

There is no wire, database, configuration, history, identity, or migration
change. Existing and older peers receive nothing. No worker, timer,
subscription, task, retry, network operation, download, rendering path, or
automatic connect/join/trust/moderation action was added.

The maximum retained accounting is 64 KiB plus bounded container overhead; the
maximum rate-accounting cardinality is 512. Runtime cost is incurred only when
a future caller explicitly invokes admission or pruning. Rollback removes the
owner additions and this report; no persisted cleanup is required.

## Validation

Passed focused tests:

```text
cargo test --locked --no-default-features --features desktop-product \
  chat::notice --lib
```

Ten tests cover the envelope plus opt-in/authentication gates, sender-scoped
deduplication, sender/global rate limits, item/byte capacity, exact summary
coalescing, incremental pruning, and shutdown clearing.

Also passed:

```text
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Live LXMF, message-title/attachment dispatch, capability negotiation, mixed
versions, external daemon, Python interoperability, packaging, non-Linux
platforms, and hardware were not run and are not claimed.

## Remaining activation blockers and next gate

The locked 0.9.6 topic event surface still lacks authenticated publisher
provenance and cursor recovery, so it cannot call this owner. Activation also
requires a transport path with authoritative LXMF sender binding, exact title
and attachment validation, negotiated `omenchat-lxmf-notices-v1` evidence,
mixed-version tests, and a separately reviewed preference/UI owner.

The next independent Phase 5 unit should avoid wiring notices around these
blockers. A safe next candidate is bounded local OMENchat quality-of-life work
or another plan item whose authority and interoperability evidence already
exists.
