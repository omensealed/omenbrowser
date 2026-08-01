# OMENbrowser v0.9.6-5 Phase 4 unit 17 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added an explicit diagnostics-only capability probe command and an isolated
two-process current/prior harness. The live lane was not executed because this
environment was not supplied the required test identities and TCP gateway.

## Design

- `NetworkRuntime` exposes one cancellable invitation-capability probe. Mock,
  external, and unsupported profiles return an explicit unsupported result.
- The managed-native implementation obtains the authenticated public identity
  for the exact canonical LXMF delivery destination, then delegates to the
  bounded adapter.
- One outer 15-second deadline covers identity discovery and the adapter. On
  expiry it cancels and awaits cleanup rather than dropping an in-flight future.
- The CLI command performs one probe and always attempts orderly runtime stop.
  Its JSON contains only categorical outcome, elapsed/deadline values, retry and
  invitation counts, and shutdown status.
- The harness uses distinct explicit application roots and existing non-symlink
  identities. It proves current support and optionally prior-release absence,
  with zero invitation sends and retries.
- Discovery output is deleted after in-memory extraction. Stderr is scrubbed and
  the harness fails if the peer destination remains in retained evidence.

No UI action or capability-consumption path was added.

## Files changed

- `src/runtime/network.rs`
- `src/runtime/native/invitation_capability_probe.rs`
- `src/runtime/native/adapter.rs`
- `src/main.rs`
- `src/cli_help.rs`
- `scripts/run-lxmf-invitation-capability-live.sh`
- `docs/TESTING.md`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT17.md`

## Compatibility, storage, protocol, and resource impact

No automatic network activity exists. Only an explicit CLI invocation probes.
No database/config/schema, protocol payload, identity, dependency, product
feature, package, or version changed. The command performs one bounded attempt,
creates no retry task, and uses the runtime's existing owned shutdown path.

## Validation

Passed:

```text
bash -n scripts/run-lxmf-invitation-capability-live.sh
cargo test --locked --no-default-features --features desktop-product \
  cli_parses_redacted_lxmf_invitation_capability_probe --bin omenbrowser_rs
cargo test --locked --no-default-features --features desktop-product \
  cli_help --lib
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_probe --lib
cargo test --locked --no-default-features --features desktop-product \
  native_trait_lifecycle_and_capabilities_follow_active_transport --lib
cargo test --locked --no-default-features --features desktop-product \
  --bin omenbrowser_rs
cargo test --locked --no-default-features --features desktop-product --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

The live script was not run: no explicit isolated sender/receiver identities,
test TCP gateway, or reviewed prior binary path was supplied. Consequently this
unit does not claim current/current live support or prior-release timeout
evidence. The first help-focused run correctly failed because the documented
line-count invariant had not been updated for the two new help lines; the
expected count was updated and both help tests then passed.
The complete binary matrix passed 46 tests. The complete desktop-product
library matrix passed 1,561 tests with 31 explicitly environment-bound tests
ignored and no failures.

## Rollback and next gate

Remove the runtime method, CLI command, harness, and matching documentation. No
data cleanup is required.

The next gate is maintainer execution of the documented harness. If it passes,
record the redacted evidence and then add a user-confirmed invitation-send
preparation boundary that consumes one fresh supported result. If it fails,
retain outbound invitations as disabled and diagnose only the failing path,
Link, response, or shutdown stage without adding retry or fallback behavior.
