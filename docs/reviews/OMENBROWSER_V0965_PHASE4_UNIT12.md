# OMENbrowser v0.9.6-5 Phase 4 unit 12 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Implemented the accepted inert invitation-capability codec and state model.
No Reticulum destination, request, Link, invitation send, or UI action is
active.

## Design

- MessagePack uses fixed request/response arrays under 128/1,024-byte outer
  limits and rejects trailing data.
- Nonces are exactly 16 bytes. Capability names are bounded ASCII, unique, and
  canonically sorted; at most 16 are admitted.
- Evidence is frontend-neutral and distinguishes supported, unsupported,
  unknown, stale, checking, and identity conflict.
- Support is fresh and one-use. Consuming it immediately makes it stale.
- The owner enforces one probe per peer, eight global probes, a 15-second
  deadline, 60-second cooldown, 256 records, 64-KiB accounting, bounded TTLs,
  incremental pruning, and explicit clear ownership.
- An initial focused test found cooldown evidence was pruned immediately after
  support consumption. Cooldown validation now precedes bounded expiry pruning,
  and the regression passes.

## Files changed

- `src/chat/mod.rs`
- `src/chat/invitation_capability.rs`
- `docs/TESTING.md`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT12.md`

## Compatibility, storage, and resource impact

There is no active compatibility or runtime change because nothing calls the
module over a network. No dependency, feature, schema, config, identity,
history, server, package, or version changed. The only potential memory is
owned by a future caller and is statically bounded; no owner is instantiated in
this unit.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --lib -- -D warnings
cargo clippy --locked --no-default-features --features tui \
  --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

Six focused tests passed. Live, mixed-version, external-RPC, package, Python,
and hardware tests do not apply to this inert unit.

## Rollback and next gate

Remove the module export, module, and matching documentation; no data cleanup
is required. The next smallest unit is a compiler-verified managed-native
destination/handler ownership spike behind no UI action. It must prove clean
startup/shutdown and authenticated destination binding before any probe caller
or invitation send is added.
