# OMENbrowser v0.9.6-5 Phase 1 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Phase 1 is complete for the current checkout. Canonical local gates pass.
The exact locked Reticulum/LXMF family remains 0.9.6 and no dependency,
protocol version, database version, queue/cache bound, or application version
changed.

The one known-red result is the previously documented upstream UDP
maximum-Resource boundary. Current 0.9.6 evidence confirms rather than closes
that limitation.

## 1. External SDK/RPC send-field conformance

The real published `lxmf_sdk::RpcBackendClient` was exercised against an
isolated loopback MessagePack RPC capture endpoint.

Proven preserved fields:

- source, destination, title, content, and fields;
- direct/propagated method;
- stamp cost;
- request-fresh-ticket flag;
- direct-to-propagated fallback choice;
- daemon-returned message ID used for cancellation.

Proven dropped or unavailable fields:

- TTL/absolute expiry;
- idempotency key;
- correlation identifier;
- extensions;
- an explicit remembered reply ticket.

OMEN continues to enforce its persisted absolute deadline locally and never
automatically retries an uncertain external send. It does not claim daemon-side
TTL, idempotency, or correlation in external RPC mode. An explicit remembered
reply ticket now fails before opening the RPC connection because silently
discarding it could alter stamp policy. Ordinary external sends remain
available with the reduced, documented guarantee set.

The embedded RPC bridge test independently proves that its project-owned
boundary retains delivery options. The minimal upstream-ready report is
`docs/upstream/LXMF_SDK_0_9_6_RPC_SEND_FIELD_REPRODUCER.md`.

## 2. Ticket-cache poison recovery

The two production `Mutex::lock().expect(...)` sites and the test helper now
share one narrow lock policy. On poison, the auxiliary bounded cache is cleared,
the mutex poison flag is reset, and a redacted warning is emitted. Ticket
material is neither logged nor retained after recovery.

The existing limits are unchanged:

- 1,024 items;
- 256 KiB total ticket text;
- 256 bytes per ticket field.

Tests cover normal capture/take, item/byte bounding, oversize rejection,
poison recovery, continued use after recovery, and redacted warning text.

## 3. Authoritative OMENchat capability matrix

Current code contradicted older staging prose: the canonical client and server
already request, accept, handle, persist, and present the reviewed capabilities.
`docs/OMENCHAT_PROTOCOL.md` now contains the authoritative active matrix for:

- durable mutations and durable notice acknowledgement;
- replies/mentions;
- reactions;
- message revisions/tombstones;
- room pins;
- announcement rooms;
- slow mode;
- room media policy;
- moderation audit.

`omenchat-protocol` now owns `BASE_DURABLE_SESSION_CAPABILITIES` and
`KNOWN_SESSION_CAPABILITIES`. The client builds its base negotiation request
from that shared list. Deterministic tests verify vocabulary bounds, the
canonical client request, and full canonical server acceptance. Optional
capabilities remain feature-gated and every capability remains explicitly
Link-negotiated.

Documentation distinguishes deterministic legacy/downgrade evidence from an
actual prior-binary live process lane; it does not claim the latter was run.

## 4. UDP maximum-Resource requalification

Locked source:

- `reticulum-rs-transport = 0.9.6`;
- crates.io checksum
  `149873f10b5c575718976ceb6be2dfc25a6adb0d4df012b7b80b135af40c788e`.

The invariant remains red:

```text
upstream UDP tx buffer (456) cannot serialize a maximum Resource wire packet (483)
```

The two-process gate also remains red. Current evidence:

- Resource advertisement received;
- advertised transfer 4,176 bytes / data 4,117 bytes / nine parts;
- receiver repeatedly requests four parts;
- sender finds the transfer and builds four responses per request;
- no Resource part reaches the receiver;
- receiver ends `retry_limit_exhausted`;
- sender times out awaiting terminal completion;
- cancellation/reuse stages are not reached.

No local fork, patch override, limit reduction, incompatible fragmentation, or
unbounded retry was introduced. Passing OMENchat/NomadNet Resource paths retain
their narrower tested interface and payload claims.

## 5. Reticulum/NomadNet request adapter

Inspection confirmed one narrow adapter owns direct request packet composition,
request-Resource selection, response correlation, timeout, and cancellation.
Direct packets dispatch only through the active Link's bound ingress interface.
Both request primitives accept independently correlated direct or Resource
responses.

New deterministic coverage proves:

- a pre-cancelled direct request dispatches nothing;
- a pre-cancelled request Resource dispatches nothing;
- cancellation and timeout emit cleanup without another direct request or
  Resource advertisement;
- terminal cleanup never replays through the alternate primitive.

Existing golden/correlation tests for direct requests, response Resources,
request Resources, bounds, timeout, cancellation, and current-Python primitive
combinations remain in place.

## Files changed

- `Cargo.toml`
- `README.md`
- `docs/LXMF_DELIVERY_AND_EVENT_MODEL.md`
- `docs/OMENCHAT_PROTOCOL.md`
- `docs/RETICULUM_TRANSPORT_API_GAP.md`
- `docs/TESTING.md`
- `docs/migration/RETICULUM_RS_0_9_UPSTREAM_UDP_RESOURCE_REPORT.md`
- `docs/reviews/OMENBROWSER_V0965_BASELINE.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE1.md`
- `docs/upstream/LXMF_SDK_0_9_6_RPC_SEND_FIELD_REPRODUCER.md`
- `src/chat/live.rs`
- `src/runtime/native/request.rs`
- `src/runtime/native_lxmf/client.rs`
- `src/server/Cargo.toml`
- `src/server/README.md`
- `src/server/crates/omenchat-protocol/src/lib.rs`
- `src/server/src/session.rs`

## Commands and results

Passed:

```text
cargo fmt --all --check
cargo fmt --manifest-path src/server/Cargo.toml --check
cargo test --locked -p omenchat-protocol authoritative_capability_vocabulary
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings
cargo test --locked --no-default-features --features desktop-product-static-media
cargo test --locked --no-default-features --features tui
cargo clippy --locked --no-default-features --features tui --all-targets -- -D warnings
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full --all-targets -- -D warnings
bash scripts/release-check.sh quick
```

Focused conformance, poison, capability, request, response-correlation, and
embedded-RPC tests also passed.

Expected known-red evidence (exit 101):

```text
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  reticulum_udp_tx_buffer_covers_max_resource_wire_packet \
  -- --ignored --nocapture

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  reticulum_multiprocess_resource_complete_cancel_reuse \
  -- --ignored --nocapture
```

## Tests not executed

- A separately installed `reticulumd`/`lxmf-cli` live external-daemon process
  was unavailable in this environment. The published client drops the affected
  fields before daemon transport, so the loopback wire capture is the
  authoritative client-boundary evidence; daemon lifecycle/event recovery
  remains a Phase 2 live lane.
- Python interoperability was not rerun for this local Phase 1 patch. No
  Python-facing wire contract changed; it remains a Phase 2/full qualification
  lane.
- Windows, macOS, ARM64, package, GPU, and physical-device lanes were not run
  from this x86_64 Linux environment.
- `release-check.sh full` and release packaging were intentionally deferred;
  this is not a release candidate and the product version remains 0.9.6-5.

## Resource and compatibility impact

- No new worker, queue, cache, recurring timer, dependency, or background task.
- Ticket-cache bounds are unchanged; poison recovery intentionally loses only
  auxiliary cached tickets.
- Loopback RPC capture uses one bounded test thread, fixed deadlines, and a
  bounded synchronous channel.
- Request tests add only bounded 50 ms observation windows.
- No wire, schema, identity, state-root, or mixed-version format change.
- External RPC explicit reply-ticket dispatch changes from silent loss to a
  pre-dispatch typed unsupported result. Rollback is the single validation call
  plus its tests/docs; no stored data migration is involved.

## Remaining limitations and next phase

- External RPC 0.9.6 cannot prove daemon-side TTL, idempotency, correlation, or
  explicit reply-ticket semantics.
- Maximum-size UDP Resource transfer remains unavailable on the locked
  transport.
- Live external-daemon, Python, prior-binary, restart, and kill-point evidence
  remains for Phase 2.

Phase 2 is safe to begin. The smallest next unit is shared interoperability
support and a bounded external-daemon lifecycle/send evidence lane, without
creating a new crate unless real duplication across the two workspaces appears.
