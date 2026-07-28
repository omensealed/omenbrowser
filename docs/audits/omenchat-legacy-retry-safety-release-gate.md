# OMENchat legacy and mixed-peer retry-safety release gate

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `f888ccf`

Verdict: pass. A current client does not send a durable mutation until the
current Link explicitly accepts the required capability, does not downgrade a
richer uncertain mutation into an older wire shape, and does not transmit
recovered work during startup. Legacy peers continue using ordinary protocol
version 1 traffic without being assigned capabilities they cannot advertise.

## Invariants

- Opening or replacing a Link clears prior capability authority.
- Capability request is not capability acceptance.
- A durable or richer mutation remains blocked after rejection or downgrade.
- An uncertain rich mutation is never converted into a legacy message.
- Reconnect does not automatically resend an uncertain mutation.
- Restart recovery is identity-scoped, bounded, visible, and non-transmitting.
- Explicit retry is the only path that reuses a persisted mutation identity.
- A server rejects malformed or unnegotiated durable envelopes without
  breaking ordinary legacy messages.
- Base durable negotiation does not alter the legacy room-notice response.
- Root and standalone-server codecs retain the immutable v0.9.6-3 ordinary
  message bytes.

## Current-source validation

Passed locally:

```text
cargo test --locked --no-default-features --features desktop-product \
  durable_session_activation_requires_acceptance_and_is_cleared_on_downgrade --lib
cargo test --locked --no-default-features --features desktop-product \
  live_reconnect_does_not_resend_rich_intent_when_capability_is_lost --lib
cargo test --locked --no-default-features --features desktop-product \
  restart_recovery_is_identity_scoped_visible_and_never_transmits --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_room_text_requires_negotiation_and_uncertain_persistence_before_send --lib
cargo test --locked --no-default-features --features desktop-product \
  v0_9_6_3_ordinary_message_remains_byte_exact --lib
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  durable_envelopes_fail_closed_without_breaking_legacy_room_messages --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  base_durable_capability_preserves_legacy_notice_origin_response --lib)
(cd src/server && cargo test --locked --no-default-features \
  --features server-headless \
  v0_9_6_3_ordinary_message_remains_byte_exact --lib)
```

Eight focused tests passed. The restart fixture covers prepared and uncertain
messages, actions, notices, part-room, topic/create, and active-user moderation
operations. It observes zero transmission while loading fourteen unresolved
records and reports that nothing was resent.

## Adjacent process evidence

`docs/audits/omenchat-room-shape-adjacent-qualification.md` records the
isolated process matrix against the peeled immutable `v0.9.6-3` commit
`414d8eafd1a845a986032bad993ac9c09cc378e4`:

- current client to adjacent server used a legacy four-field catalog and
  negotiated no new room-policy capability;
- adjacent client to current server completed ordinary open, join,
  publication, and echo;
- current server shaping kept simultaneous negotiated and legacy Links
  separate;
- no capability was fabricated for the adjacent peer.

That process matrix is intentionally not repeated for this documentation and
focused-regression unit. The immutable peer and wire contract have not
changed, and rebuilding/running three process cases would add substantial time
without exercising a new boundary. It remains a manual release-candidate gate.

## Compatibility, resources, and rollback

This unit changes no executable code, protocol, schema, feature, configuration,
dependency, task, timer, queue, cache, retry, or storage behavior. It
reconciles already-passing evidence with the release checklist.

Rollback removes this audit and reopens the checklist item. No user or server
state is affected.

Hosted CI, Python interoperability, packaging, public-network peers, and
hardware were not run because this unit has no executable or artifact change.
