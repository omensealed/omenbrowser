# OMENbrowser v0.9.6-5 Phase 5 unit 4 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added a diagnostics-only, local external-SDK/RPC topic capability negotiation
probe. It requests the exact topic/event capability set once, classifies only a
bounded capability snapshot, and leaves both topic adapters inactive.

## Design and compatibility

The probe reuses the existing local endpoint validator and the public
`lxmf-sdk 0.9.6` asynchronous `Client::start_async` negotiation path. One
10-second total deadline owns cancellation. The client is dropped after the
response; no SDK shutdown, subscribe, publish, retry, worker, timer, storage,
identity, page fetch, or protocol action occurs. Reports redact Unix paths and
contain no runtime ID, credentials, payloads, or capability strings.

This changes no default runtime, wire protocol, database, cache, UI, or release
feature behavior. A negotiated fanout surface is explicitly separate from the
inactive OMENbrowser publisher. Receive remains blocked by the unproven topic
event contract, publisher authentication, and cursor-gap reconciliation.

## Files changed

- `src/runtime/native_lxmf/client.rs`
- `src/main.rs`
- `src/cli_help.rs`
- `docs/upstream/LXMF_SDK_0_9_6_TOPIC_AUDIT.md`
- `docs/design/NOMADNET_LXMF_UPDATE_POINTER_CHECKPOINT.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT4.md`

## Validation

Deterministic tests capture the exact single `sdk_negotiate_v2` request, all
five requested capabilities, successful fixed-state classification, absence of
any second RPC operation, deadline cancellation, and endpoint redaction. The
CLI parser and help contract are also covered.

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  topic_capability_probe
cargo test --locked --no-default-features cli_help --lib
cargo check --locked --no-default-features
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Live local-daemon negotiation, topic subscribe/publish, daemon restart, event
gap recovery, publisher authentication, Python interoperability, packaging,
non-Linux native platforms, and hardware were not run and are not claimed.

## Resources, rollback, and next gate

The command is explicit and one-shot. It retains only a fixed report, makes no
automatic retry, and owns no persistent task. Rollback removes the command,
probe method/tests, and these documentation additions; no data cleanup or
migration is required.

The next safe gate is a controlled local-daemon event-contract capture. It must
first prove that a subscribed topic delivery carries authoritative publisher
identity and that snapshot recovery exists after a cursor gap. Until then, the
dormant update-pointer owner must remain disconnected.
