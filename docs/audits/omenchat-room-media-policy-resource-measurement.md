# OMENchat Room Media-Policy Resource Measurement

Date: 2026-07-28

Baseline: `release/v0.9.6-4` at `cd5a0e5`, plus this measurement unit.

## Scope and verdict

This unit closes the deterministic retention, pending-offer, storage, and
optimized latency portion of the `room-media-policy-v1` Resource measurement
gate. It also resolves the proposed receiver-side cancellation work into an
evidence-backed upstream limitation.

Verdict: the isolated optimized server measurement passed. Receiver-side
Reticulum Resource cancellation is not implementable through the public locked
0.9.6 API and is not claimed as passed.

## Cancellation API decision

The resolved crate is `reticulum-rs-transport 0.9.6`. Direct source inspection
found:

- `Transport::cancel_resource` looks up and removes an outbound Resource,
  emits `ResourceInitiatorCancel`, and reports `OutboundCancelled`;
- the Resource manager understands received initiator/receiver-cancel packet
  contexts and can report inbound failure;
- no public operation exposes an inbound Resource advertisement/receiver
  handle or sends `ResourceReceiverCancel`.

OMENchat therefore cannot initiate cancellation of an admitted inbound upload
without private transport access, a local upstream fork, or a new application
protocol that closes the Link. Private access and a fork violate the migration
boundaries; closing a healthy Link is not equivalent to per-Resource
cancellation and would worsen reconnect behavior. The existing conservative
behavior remains:

- remote initiator cancellation or inbound failure releases the identified
  peer's pending upload offers;
- the healthy Link stays open;
- no exact OMENchat Resource ID is invented from a transfer hash;
- completed payloads are still rejected before publication if size, identity,
  membership, or current room policy no longer permits them.

This limitation must be rechecked when a later Reticulum crate exposes a public
receiver cancellation operation.

## Measurement design

The ignored test `room_media_policy_resource_retention_measurement`:

- uses a unique temporary SQLite database and upload root;
- enables only the non-product room-media-policy qualification path;
- configures a 64-KiB room file ceiling and 512-KiB identity quota;
- publishes 32 negotiated 64-KiB Resources through the normal offer, durable
  same-filesystem upload replacement, SQLite ledger, eviction, and room-event
  path;
- verifies exactly eight retained files and 524,288 ledger/disk bytes;
- verifies zero pending items and identities after every accepted Resource has
  reached a terminal application outcome;
- requires no missing, mismatched, orphan, or unsafe ledger paths;
- rejects a 65,537-byte offer before pending Resource admission;
- checkpoints SQLite, records database bytes, reports offer/publication
  latency, and observes process RSS where Linux `/proc` is available;
- removes the database, WAL/SHM, and upload root.

The test is ignored because it performs repeated synchronous durable file and
SQLite operations and records host-dependent measurements. It adds no runtime
worker, timer, queue, cache, retry, dependency, configuration, or product
feature.

## Optimized local observation

Command:

```bash
cargo test --release --locked --manifest-path src/server/Cargo.toml \
  --no-default-features \
  --features server-headless,omenchat-room-media-policy-qualification \
  room_media_policy_resource_retention_measurement \
  --lib -- --ignored --nocapture
```

Observed on this Linux x86_64 host:

```text
attempts=32
upload_bytes=65536
retained_files=8
retained_bytes=524288
pending_items=0
pending_identities=0
database_bytes=229376
offer_p50_us=139
offer_p95_us=158
offer_max_us=191
publication_p50_us=458
publication_p95_us=497
publication_max_us=543
rss_before_bytes=9371648
rss_after_bytes=9932800
rss_delta_bytes=561152
```

The latency and RSS values are observations, not release thresholds. Exact
retention and ownership bounds are assertions.

## Compatibility, rollback, and remaining evidence

There is no wire, schema, identity, configuration, or product-profile change.
Rollback removes the ignored test and documentation only.

Still required before activation:

- adjacent current/previous mixed-version qualification;
- hosted Windows/macOS attachment presentation; native Linux Iced accepted,
  over-limit, and disabled cases now pass;
- live-process CPU, queue, and shutdown observation around an active upload;
- explicit production activation and rollback review.

Hosted CI, Python interoperability, native packaging, public gateways,
physical interfaces, GUI automation, and physical GPU measurements were not
run in this unit.
