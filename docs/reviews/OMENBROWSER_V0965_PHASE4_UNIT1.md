# OMENbrowser v0.9.6-5 Phase 4 unit 1 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

The dormant LXMF `OmenChatInvitePayload` is now bounded and fail-closed. It
remains disconnected from production input and causes no network, storage,
trust, role, connection, or join behavior.

## Changes

- Added a 4 KiB pre-deserialization envelope cap.
- Rejected unknown JSON fields and unsupported protocol/version values.
- Required canonical lowercase 32-character hexadecimal server and inviter
  destinations.
- Added explicit byte/content limits for room ID, room/inviter display names,
  invite token, and introduction.
- Enforced expiry with a five-minute skew allowance.
- Replaced token-leaking derived `Debug` with a redacted implementation;
  introduction content is redacted as message-like private text as well.
- Added an explicit replay classification. A token-bearing invitation requires
  server-side token consumption; the client does not call it single-use.
- Added exact boundary, malformed/unknown-field, expiry, canonical destination,
  replay, and redaction tests.

## Compatibility and resource impact

- No current production caller was changed; the payload was dormant.
- No crate, worker, task, queue, cache, timer, schema, feature, or protocol
  operation was added.
- The public safe `omenchat://` invitation format is unchanged.
- The dormant JSON decoder now rejects previously tolerated unsafe/noncanonical
  payloads. That is intentional before activation.
- Maximum retained untrusted input is 4 KiB.

## Validation

Passed:

```text
cargo fmt --all
cargo test --locked --no-default-features --features desktop-product \
  chat::handoff::tests --lib -- --nocapture
```

Five handoff tests passed, including the existing Resource metadata round trip.

## Remaining gates

Do not wire the payload to LXMF yet. A production activation still needs:

1. an untrusted preview type/owner with one-item and aggregate bounds;
2. authenticated LXMF sender versus claimed inviter comparison;
3. inbound presentation rate limiting and bounded replay retention;
4. explicit user confirmation;
5. server-side definition and enforcement for any invite token;
6. mixed-version and live LXMF tests.

The next safe unit is the frontend-neutral untrusted preview and sender-evidence
reducer. It must still perform no connection, join, trust, or role action.
