# OMENbrowser v0.9.6-5 Phase 4 unit 2 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added an inert, frontend-neutral owner for validated LXMF invitation previews.
No runtime, message reducer, desktop view, connection path, persistence, or
automatic action uses it yet.

## Design

- One pending preview; successful newer admission replaces it.
- Authenticated LXMF sender comparison produces match, mismatch, or unavailable
  evidence. Mismatch blocks confirmation.
- SHA-256 covers canonical decoded JSON, preventing whitespace or field-order
  changes from bypassing duplicate suppression.
- Replay evidence is capped at 64 records and 64 KiB of accounted encoded
  input, with a seven-day maximum age.
- Expiry cleanup removes at most eight records per admission.
- Per-sender presentation is capped at four distinct invitations per five
  minutes.
- Duplicate, invalid, rate-limited, and overloaded admissions preserve the
  existing preview and bounds.
- Token-bearing payloads remain classified as requiring server-side token
  consumption; the client does not infer single-use behavior.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  chat::handoff::tests --lib -- --nocapture
cargo test --locked --no-default-features --features desktop-product \
  chat::handoff::tests::lxmf_preview_enforces_sender_rate_and_global_replay_bounds \
  --lib -- --exact
cargo clippy --locked --no-default-features --features desktop-product \
  --lib -- -D warnings
cargo fmt --all
```

Tests cover sender match/mismatch, confirmation blocking, explicit cancel,
canonical duplicate suppression, invalid-input preservation, redacted debug,
per-sender rate limiting, item capacity, byte capacity, incremental expiry
pruning, and accounting bounds.

## Compatibility and resource impact

- No protocol, schema, feature, dependency, product version, or current UI
  behavior changed.
- No worker, task, channel, timer, subscription, disk write, or network traffic.
- The pending payload is already limited to 4 KiB.
- Replay records retain hashes/public sender destinations and accounting only,
  not invitation tokens or introductions.

## Remaining activation gates

1. Bind admission to an authenticated LXMF application-event boundary without
   polling or automatic retry.
2. Add a user-confirmed preview presentation that exposes mismatch, expiry,
   requested-role, password, and token-policy warnings.
3. Define server-side token issuance/consumption before permitting token use.
4. Persist only bounded replay evidence if restart suppression is required;
   do not persist secret payload bodies.
5. Add current/current and mixed-version live LXMF tests.

The next smallest safe step is to inspect the existing typed LXMF inbound event
path and identify whether it exposes authenticated sender identity strongly
enough to feed this reducer. Do not wire it if sender authority is unavailable.
