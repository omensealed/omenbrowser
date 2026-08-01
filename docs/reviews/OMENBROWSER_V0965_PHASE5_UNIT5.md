# OMENbrowser v0.9.6-5 Phase 5 unit 5 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Completed the locked 0.9.6 topic event-contract investigation. The current
public daemon cannot supply the authenticated publisher and cursor-recovery
evidence required by the NomadNet update-pointer admission model. Activation
remains blocked truthfully.

## Implementation and evidence

Added a fixed-size, fail-closed event finding and bounded shape inspection. It
recognizes only `sdk_topic_published`, accounts at most 8 KiB, bounds topic and
correlation identifiers, retains no payload, and never upgrades a generic SDK
peer or caller-controlled payload to authentication.

A deterministic public-API reproducer creates, subscribes, publishes, polls,
and queries telemetry against an in-memory `reticulum-rs-rpc 0.9.6` daemon. It
proves:

- the event has no publisher identity;
- telemetry recovery has no publisher identity;
- an arbitrary subscription cursor is accepted;
- the resulting event is inadmissible.

No upstream repository was changed or contacted.

## Files changed

- `src/runtime/lxmf_topics.rs`
- `docs/upstream/LXMF_SDK_0_9_6_TOPIC_AUDIT.md`
- `docs/upstream/LXMF_SDK_0_9_6_TOPIC_EVENT_PROVENANCE_REPORT.md`
- `docs/design/NOMADNET_LXMF_UPDATE_POINTER_CHECKPOINT.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT5.md`

## Compatibility and resources

There is no production caller, topic operation, protocol, database,
configuration, identity, UI, cache, task, timer, or network behavior change.
The classifier returns a fixed-size finding and performs one bounded JSON size
accounting operation only when explicitly called. The reproducer uses an
in-memory test store and no filesystem or network state.

## Validation

Passed focused tests:

```text
cargo test --locked --no-default-features --features desktop-product \
  lxmf_topics --lib
cargo test --locked --no-default-features \
  topic_event_classifier_never_upgrades_generic_peer_or_payload_to_authentication \
  --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Six tests passed, including exact locked SDK surface compilation, capability
bounds, staged readiness, event-shape bounds, fail-closed authentication, and
the in-memory daemon reproducer.

Live external-daemon topic traffic, Reticulum transport, Python
interoperability, packaging, non-Linux native platforms, and hardware were not
run and are not claimed.

## Rollback and next gate

Rollback removes the event finding/reproducer and matching documentation; no
state cleanup is required. The dormant pointer codec and admission owner remain
independently removable.

The external topic receive path cannot proceed safely on 0.9.6. The next
independent Phase 5 unit should return to a feature whose trust evidence is
available locally—bounded LXMF invitation/notice handoff or another explicitly
documented plan item—without connecting the topic owner.
