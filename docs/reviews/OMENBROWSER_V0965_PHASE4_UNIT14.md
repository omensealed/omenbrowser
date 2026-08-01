# OMENbrowser v0.9.6-5 Phase 4 unit 14 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Made the test-only managed-native invitation capability endpoint owner safe to
share through the production runtime's cloneable transport-handle ownership
shape. No production destination or behavior was activated.

## Lifecycle finding and design

`NativeTransportHandle` is cloned by active operations. The normal
`NetworkRuntime::stop_runtime` boundary is asynchronous, but the existing
internal `NativeNetworkRuntime::stop` helper synchronously cancels and drops
transport state. A unique, consuming endpoint owner therefore could not be
stored safely in that handle without losing the ability to join it.

The endpoint owner now wraps one shared inner owner:

- all clones share one cancellation token and task slot;
- the first asynchronous shutdown takes and awaits the task;
- destination deregistration is exactly once;
- later shutdown calls are bounded successful no-ops;
- only final-owner Drop uses abort as a non-blocking safety fallback.

This is deliberately still a test-only lifecycle primitive. Production
integration must make `stop_runtime` take/await the owner before releasing its
transport and must not treat the synchronous helper as proof of a graceful
join.

## Files changed

- `src/runtime/native/invitation_capability_endpoint.rs`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT14.md`

## Compatibility, storage, protocol, and resource impact

None in production. No feature, dependency, schema, configuration, identity,
wire, package, or version changed. The test endpoint retains one owned worker,
one bounded upstream receiver, no application queue, no per-request task, and
no retry loop.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_endpoint --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --tests -- -D warnings
cargo fmt --all --check
git diff --check
```

Three focused tests pass, including shared-reference exactly-once join and
deregistration within a one-second bound and final-owner Drop cancellation.

Live peer, external-RPC, mixed-version, package, Python, and hardware tests were
not run because this endpoint remains excluded from production builds.

## Rollback and next gate

Revert the shared owner wrapper and this documentation; no cleanup or migration
is required. The next smallest gate is production managed-runtime lifecycle
integration of the receiver endpoint only. It must be behind managed-native
availability, be joined by `stop_runtime`, preserve the existing broadcast Link
consumers, and expose no outbound probe or invitation action.
