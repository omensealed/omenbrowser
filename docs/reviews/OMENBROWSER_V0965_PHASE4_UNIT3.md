# OMENbrowser v0.9.6-5 Phase 4 unit 3 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

The managed native LXMF decoder now preserves explicit authenticated-source
evidence at the project-owned `MessageSummary` boundary. Invitations remain
dormant and no message content is interpreted as an invitation.

## Evidence semantics

`native_lxmf_source_authenticated=true` is added only when:

1. the sender identity came from the authenticated `lxmf.delivery` announce
   cache;
2. the wire source matches the delivery destination derived from that identity;
3. the LXMF signature verifies;
4. bounded message decoding succeeds.

Plain/unverified decoding does not add the field. Forged or mismatched direct
and propagated messages fail rather than receiving weaker evidence.

The frontend-neutral invitation helper returns a sender only for an inbound
message with this exact evidence and a canonical 32-character peer hash.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product --lib \
  runtime::native_lxmf::codec::tests::verified_signed_wire_message_requires_matching_source_identity \
  -- --exact
cargo test --locked --no-default-features --features desktop-product --lib \
  runtime::native_lxmf::codec::tests::verified_propagated_wire_accepts_matching_announced_sender \
  -- --exact
cargo test --locked --no-default-features --features desktop-product --lib \
  runtime::native_lxmf::codec::tests::signed_wire_message_decodes_to_message_summary \
  -- --exact
cargo test --locked --no-default-features --features desktop-product --lib \
  chat::handoff::tests -- --nocapture
cargo clippy --locked --no-default-features --features desktop-product \
  --lib -- -D warnings
```

The verified direct and propagated tests require the marker. The ordinary
decoder test requires it to be absent. Handoff tests require incoming state,
the exact marker, and a canonical peer hash.

## Compatibility and resource impact

- No wire, schema, dependency, runtime mode, task, queue, timer, or product
  version changed.
- Verified inbound message summaries gain one small bounded diagnostic field;
  existing bounded history/storage accounting applies.
- No identity key material, signature, token, message body, or attachment is
  copied into the evidence field.
- External SDK/RPC mode is not claimed equivalent; its sender-authentication
  provenance still requires separate evidence.

## Next step

Define the exact LXMF invitation envelope placement using existing bounded LXMF
fields or content without colliding with ordinary messages. Add pure extraction
tests first. Do not attach it to the runtime event stream until both managed
native and external-backend sender evidence are classified truthfully.
