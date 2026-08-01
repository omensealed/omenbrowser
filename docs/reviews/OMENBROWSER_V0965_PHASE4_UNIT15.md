# OMENbrowser v0.9.6-5 Phase 4 unit 15 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Activated only the receive-side invitation capability endpoint in the clean
managed-native browser runtime. Outbound probing, invitation sending, and UI
actions remain disabled.

## Architecture and lifecycle

- Startup registers `omenbrowser`/`lxmf.capabilities` from the existing browser
  identity before wrapping the transport in its shared handle.
- One owned worker reads the transport's bounded broadcast Link-event stream,
  filters the exact destination and Request context, and emits only the fixed
  bounded capability response.
- Existing inbound/outbound Link consumers retain independent broadcast
  receivers; this endpoint does not consume events away from them.
- There is no per-request task, application queue, retry, announce, polling
  timer, persistence, or payload-bearing cache.
- Transport replacement cancels the endpoint synchronously. Normal
  `stop_runtime` joins it within one second and deregisters exactly once before
  reporting the stopped lifecycle. Timeout aborts the owned task, still
  deregisters, and returns a truthful shutdown error.
- The external/shared backend, TUI-only profile, mock backend, and independent
  omenchatd server do not register this browser endpoint.

## Files changed

- `src/runtime/native/mod.rs`
- `src/runtime/native/invitation_capability_endpoint.rs`
- `src/runtime/native/adapter.rs`
- `docs/TESTING.md`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT15.md`

## Compatibility, storage, protocol, and resource impact

Managed-native browser instances now answer the exact versioned capability
request on a deterministic same-identity destination. Older peers do not know
or contact that destination and are unaffected. No OMENchat frame, LXMF message
body, database/config/schema, identity material, package feature, dependency,
or application version changed.

The endpoint adds one runtime-owned task and one subscription to the existing
bounded broadcast stream. Processing concurrency is one, response size is
bounded by the existing codec, and shutdown is bounded. Idle behavior has no
timer or network traffic because the endpoint neither announces nor probes.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_endpoint --lib
cargo test --locked --no-default-features --features desktop-product \
  native_trait_lifecycle_and_capabilities_follow_active_transport --lib
cargo test --locked --no-default-features --features desktop-product --lib
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

The lifecycle test retains a cloned transport, proves the destination is
registered while running, stops through the public asynchronous runtime
boundary, then proves deregistration and idempotent repeated endpoint/runtime
shutdown. The complete desktop-product library matrix passed 1,557 tests with
31 explicitly environment-bound tests ignored and no failures.

Live two-peer response, mixed-version process, external-RPC, package, Python,
and hardware tests were not run. This unit does not claim live capability proof
or permit invitation sending.

## Rollback and next gate

Remove endpoint registration/ownership from `NativeTransportHandle`, restore
the module's test-only gate, and revert the matching documentation. No data
cleanup or migration is required.

The next smallest gate is a managed-native outbound probe adapter with no UI
caller. It must authenticate the expected peer identity/destination, correlate
the exact nonce, enforce the existing deadline/cooldown/concurrency/cache
bounds, cancel cleanly on runtime replacement, and never send or retry an
invitation automatically.
