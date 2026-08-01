# OMENbrowser v0.9.6-5 Phase 4 unit 7 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Activated receipt of presentation-only LXMF OMENchat invitations for messages
with per-message authenticated native LXMF source evidence. Dismiss remains the
only action. External SDK/RPC provenance remains unproven and cannot enter the
preview path.

## Architecture and compatibility

- Added the explicit `AuthenticatedLxmfSourceEvidence` runtime capability.
- Managed native mode reports it only with active native LXMF transport. Mock
  mode reports it unsupported; external RPC is explicitly unproven.
- The bounded preview/replay owner moved from desktop-only state into the core
  application so runtime admission and presentation share one owner. It remains
  compiled only with `chat-client`; the TUI-only feature graph is unchanged.
- Exact `omenchat.lxmf.invite` messages are control messages. They are never
  stored or rendered as ordinary chat history, preventing token-bearing JSON
  from becoming visible when authentication or validation fails.
- Valid input opens no OMENchat session and performs no Join, Save, Trust, role,
  token, retry, attachment, or download action.
- No wire format, schema, dependency, version, server behavior, or legacy
  ordinary-message behavior changed.

## Resource impact

The existing one-preview, 64-item, 64-KiB, seven-day replay owner is reused.
No queue, task, timer, subscription, polling loop, disk write, or network action
was added.

## Validation

Passed:

```text
cargo fmt --all --check
cargo test --locked --no-default-features --features desktop-product --lib \
  runtime_invitation_activation_requires_per_message_authenticated_source_evidence -- --nocapture
cargo test --locked --no-default-features --features desktop-product --lib \
  lxmf_invitation -- --nocapture
cargo test --locked --no-default-features --features desktop-product --lib \
  native_trait_lifecycle_and_capabilities_follow_active_transport -- --nocapture
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product --lib -- -D warnings
cargo clippy --locked --no-default-features --features tui --lib -- -D warnings
git diff --check
```

The complete desktop-product test command exited `0`; environment-gated tests
remained explicitly ignored by their existing contracts.

Two iterative test/compile corrections were required: the conversation API
returns an empty thread rather than `Option`, and the first fixture used a
noncanonical peer label instead of a 32-character destination. Neither changed
production behavior. The TUI gate also required preserving the existing
`chat-client` compile boundary around the new owner.

Live peer, external-daemon, Python, prior-binary, packaging, and physical-device
lanes were not run. A deterministic verified native-codec chain and the runtime
admission tests cover this local unit, but mixed-version/live evidence remains
required before enabling Open, Join, Save, or outbound invitation sending.

## Rollback

Remove the runtime control-message branch and move the owner back to desktop
state. The existing payload/replay hardening can remain dormant. No persisted
data requires migration or cleanup.

## Next gate

Add a live managed-native sender/receiver smoke fixture and mixed-version
classification. Until then the preview remains Dismiss-only and outbound LXMF
invitations remain disabled.
