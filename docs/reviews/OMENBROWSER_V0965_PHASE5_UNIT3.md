# OMENbrowser v0.9.6-5 Phase 5 unit 3 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Audited and compiler-checked the exact locked `lxmf-sdk 0.9.6` topic surface.
Both OMENbrowser runtime modes remain truthfully inactive for NomadNet topic
updates; no SDK method or event consumer was added.

## Findings and decision

- The SDK exposes topic CRUD, subscribe/unsubscribe, publish, capability names,
  and RPC mappings.
- Profile support is not negotiated daemon evidence.
- OMENbrowser's RPC sender probe takes a snapshot but does not negotiate topic
  capabilities.
- The existing event worker requests asynchronous events only.
- `SdkEvent` is generic; its `peer_id` does not establish authenticated topic
  publisher provenance in the inspected public contract.
- Existing event cursor/gap tracking does not implement topic snapshot recovery.

The new pure classifier treats managed-native as adapter-missing. External
receive remains blocked unless bounded effective capabilities and three distinct
project proofs—gap recovery, topic-event contract, and publisher authentication—
are present together. Fanout/publish capability is reported separately and does
not imply safe receive.

## Files changed

- `src/runtime/mod.rs`
- `src/runtime/lxmf_topics.rs`
- `src/runtime/network.rs`
- `src/runtime/native/adapter.rs`
- `src/chat/invitation_capability.rs`
- `src/main.rs`
- `docs/upstream/LXMF_SDK_0_9_6_TOPIC_AUDIT.md`
- `docs/design/NOMADNET_LXMF_UPDATE_POINTER_CHECKPOINT.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT3.md`

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  lxmf_topics --lib
cargo test --locked --no-default-features lxmf_topics --lib
cargo check --locked --no-default-features
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_probe --lib
cargo test --locked --no-default-features --features desktop-product \
  cli_parses_redacted_lxmf_invitation_capability_probe --bin omenbrowser_rs
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Four tests cover exact public SDK topic types/traits/profile capability names,
current managed/external classifications, staged recovery/event/authentication
requirements, and capability item/name/byte bounds.

The first compiler run correctly found that the topic trait belongs to
`lxmf_sdk::Client`, not `lxmf_sdk::app::Client`, and that non-exhaustive
`SdkEvent` cannot be constructed externally. The test was corrected to the
actual public type and public deserialization contract; production behavior was
not weakened.

The empty-feature gate initially found an accumulated invitation-probe boundary
regression: the always-compiled `NetworkRuntime` trait returned a type inside the
`chat-client`-gated module. The fixed-size outcome and deadline were moved to the
always-compiled runtime boundary and re-exported from the chat module, preserving
all desktop paths while restoring empty-default compilation. This is a module
ownership correction only; probe behavior and wire data are unchanged. Three
topic classifier tests run in the empty profile, while the fourth exact SDK
surface test is correctly gated by `native-lxmf-sdk`.

Not run: live local-daemon topic negotiation, publish/subscribe, restart/gap,
publisher-authentication, Python interoperability, packaging, non-Linux native
platforms, or hardware. No such result is claimed.

## Compatibility, resources, rollback, and next gate

No protocol, database, configuration, identity, topic, page, attachment, cache,
network, task, timer, queue, or UI behavior changed. The classifier retains no
capability strings and validates at most 64 names/4 KiB before producing a
fixed-size report.

Rollback removes the classifier module/export and matching documents; no data
cleanup is required.

The next safe step is a diagnostics-only external-RPC negotiation capture that
requests only the required topic/event capabilities, applies one deadline, logs
no endpoint credentials or payloads, performs no subscribe/publish, and shuts
down cleanly. Managed-native remains unsupported unless a separate bounded
adapter design is justified.
