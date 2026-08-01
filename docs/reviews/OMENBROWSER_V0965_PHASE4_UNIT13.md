# OMENbrowser v0.9.6-5 Phase 4 unit 13 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added a compiler-verified, test-only managed-native invitation capability
endpoint ownership spike. Production runtime behavior remains unchanged.

## Design

- The endpoint derives and registers `omenbrowser`/`lxmf.capabilities` from the
  browser identity using the public pinned Reticulum API.
- Its single owned worker filters exact destination and request-context Link
  events, decodes the bounded project codec, and constructs only the exact
  `omenchat-lxmf-invitations-v1` response.
- Responses use the existing Link's bound interface. The worker processes
  requests sequentially and creates no per-request task or application queue.
- An explicit cancellation token, join handle, and destination registration are
  owned together. Graceful shutdown cancels, joins, then deregisters; Drop
  cancels and aborts as a last-resort non-blocking guard.
- The whole module is gated to desktop-product library tests. It is not wired
  into startup, UI, probing, invitation sending, or persistent state.

## Files changed

- `src/chat/invitation_capability.rs`
- `src/runtime/native/mod.rs`
- `src/runtime/native/invitation_capability_endpoint.rs`
- `docs/TESTING.md`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT13.md`

## Compatibility, storage, and resource impact

There is no production compatibility, wire, storage, identity, configuration,
feature, dependency, package, or version change. The test worker has one owner,
one upstream bounded broadcast receiver, no retry loop, and no payload queue.
It exists only for the duration of its focused tests.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_endpoint --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --tests -- -D warnings
cargo clippy --locked --no-default-features --features desktop-product \
  --lib -- -D warnings
cargo clippy --locked --no-default-features --features tui \
  --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

Three focused tests passed. They cover the exact bounded response and malformed
request rejection, deterministic same-identity destination registration,
one-second-bounded cancellation/join/deregistration, and Drop cancellation.

Live peer, mixed-version, external-RPC, package, Python, and hardware tests were
not run because this slice is not compiled into a production runtime. It does
not claim authenticated live request/response interoperability.

## Rollback and next gate

Remove the test module, module declaration, combined-aspect constant, and
matching documentation. No data cleanup is required. Production lifecycle
integration is not yet justified: it first needs a design that joins the worker
during normal synchronous runtime stop and proves authenticated peer/destination
binding without interfering with existing Link event consumers. Until then,
outbound invitation probing and sending remain disabled.
