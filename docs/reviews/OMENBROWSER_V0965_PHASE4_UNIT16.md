# OMENbrowser v0.9.6-5 Phase 4 unit 16 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added a managed-native outbound invitation capability probe adapter with no
application or UI caller. It cannot send an invitation or consume capability
support.

## Design and invariants

- The adapter is owned per managed-native transport lifecycle and shares the
  established bounded evidence owner.
- It verifies the supplied public identity derives the expected LXMF delivery
  destination before any path or Link work.
- The capability destination is derived from that same identity. A single
  random 128-bit nonce correlates one Request with one Response on the exact
  Link.
- One total 15-second deadline covers path discovery, Link establishment,
  dispatch, and response. No stage resets it.
- Cancellation is checked before admission and throughout every await boundary.
  A pre-cancelled request creates no path request or Link.
- Any created Link is torn down after success or failure. There is no automatic
  retry, Resource fallback, or cross-primitive replay.
- Exact support, explicit absence, identity conflict, nonce conflict, malformed
  response, timeout, cancellation, and closed streams remain distinct results.
- Runtime replacement cancels the adapter. Asynchronous shutdown cancels it and
  clears all ephemeral evidence.

The adapter has no frontend, diagnostics, or invitation-send entry point. No
support evidence can currently be consumed to enable sending.

## Files changed

- `src/runtime/native/mod.rs`
- `src/runtime/native/invitation_capability_probe.rs`
- `src/runtime/native/adapter.rs`
- `docs/TESTING.md`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT16.md`

## Compatibility, storage, protocol, and resource impact

There is no network traffic because no production caller invokes the adapter.
No wire payload, database/config/schema, identity material, dependency, package
feature, or version changed. The adapter adds one small per-transport owner but
no task, timer, subscription, Link, or queue while idle. Its evidence remains
bounded at 256 items/64 KiB and eight concurrent probes if a future owner calls
it.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_probe --lib
cargo test --locked --no-default-features --features desktop-product \
  native_trait_lifecycle_and_capabilities_follow_active_transport --lib
cargo test --locked --no-default-features --features desktop-product --lib
cargo test --locked --no-default-features \
  --features desktop-product-static-media invitation_capability_probe --lib
cargo test --locked --no-default-features \
  --features desktop-product-static-media \
  native_trait_lifecycle_and_capabilities_follow_active_transport --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features tui \
  --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

Four focused probe tests and the product lifecycle regression pass in both
desktop media profiles. The initial implementation was corrected before this
gate to replace three sequential 15-second windows with one total deadline and
to tear down pending Links on every error boundary. The complete desktop-product
library matrix passed 1,561 tests with 31 explicitly environment-bound tests
ignored and no failures.

Live current/current response, prior-version timeout, external-RPC, package,
Python, and hardware tests were not run. This unit does not claim live peer
capability proof.

## Rollback and next gate

Remove the probe module/transport-handle field and matching documentation. No
data cleanup or migration is required.

The next smallest gate is a controlled two-process managed-native capability
probe harness using isolated roots and identities. It must prove current/current
support, prior-version absence without sending an invitation, cancellation,
one total deadline, zero automatic retry, clean Link/task shutdown, and redacted
evidence. Product UI and invitation sending remain out of scope until that
evidence passes.
