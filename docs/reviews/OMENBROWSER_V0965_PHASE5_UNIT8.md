# OMENbrowser v0.9.6-5 Phase 5 unit 8 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Completed Phase 5.2 rollout step 1 by defining and testing a dormant bounded
LXMF Resource-reference attachment envelope. Existing inline attachments remain
unchanged. No preview, transfer, persistence, or UI behavior was activated.

## Locked upstream finding and design decision

Inspection of the locked `reticulum-rs-transport` 0.9.6 source found public
Resource advertisements, requests, progress, proofs, completion, cancellation,
and observed outbound hashes. The Resource manager correlates those hashes and
part requests inside an active Link transfer; no reviewed public API exposes a
Resource hash as a durable object that an arbitrary later LXMF message can make
independently fetchable.

The envelope's `resource_reference` is therefore a random 128-bit application
offer/correlation ID, not a Reticulum Resource hash. A future authenticated
accept exchange must echo that ID, start the actual Link Resource, and bind the
observed Resource hash to the accepted offer. The application must not retry
across those primitives when the result is uncertain.

## Envelope invariants

The proposed `omen-lxmf-resource-reference-v1` capability uses protocol
`omenbrowser.lxmf.resource-reference`, version 1, with a 2-KiB total metadata
limit. It requires a lowercase SHA-256 content hash, 1-byte through 64-MiB
declared size, bounded MIME and display-name hints, canonical sender identity,
a 24-hour maximum lifetime, and the redacted application offer ID.

Authenticated sender evidence is supplied separately and must exactly match the
signed sender claim. The display name is never used as a storage path; the
future private staging name derives only from the verified content hash.
Executable-looking display names remain inert. The type always reports that it
permits no automatic transfer, decode, or executable launch. Optional thumbnail
references remain excluded.

## Files changed

- `src/messaging/mod.rs`
- `src/messaging/resource_reference.rs`
- `docs/design/LXMF_RESOURCE_REFERENCE_ATTACHMENTS_CHECKPOINT.md`
- `docs/LXMF_DELIVERY_AND_EVENT_MODEL.md`
- `docs/TESTING.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE5_UNIT8.md`

## Compatibility, storage, and resources

There is no capability advertisement, network recognition, transfer backend,
message fallback, database/configuration/schema change, identity change,
filesystem write, cache, queue, task, worker, timer, retry, media decode, or UI
action. Older peers receive nothing. Existing bounded inline LXMF attachments
retain their current wire and storage behavior.

Envelope validation allocates at most the 2-KiB encoded input and decoded
metadata. Rollback removes the module/export and matching documentation; no
data cleanup or downgrade migration is required.

## Validation

Passed:

```text
cargo test --locked --no-default-features resource_reference --lib
cargo test --locked --no-default-features --features desktop-product \
  clean_sdk_wire_envelope_preserves_file_attachment_bytes --lib
cargo test --locked --no-default-features --features desktop-product \
  outbound_envelope_encodes_python_style_file_attachments --lib
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

Five tests cover authenticated round trip, fail-closed sender binding,
metadata/hash/time/size limits, display-name/path separation, reference
redaction, unknown fields, invalid identifiers, and pre-decode byte overflow.
Two existing regression tests additionally prove the clean SDK and Python-style
inline attachment encoders still preserve their prior behavior. Live Resource
transfer, capability negotiation, mixed versions, external daemon, Python
interoperability, packaging, non-Linux platforms, and hardware were not run and
are not claimed.

## Next gate

Phase 5.2 rollout step 2 is a bounded caller-inert pending-offer owner that can
produce a receive preview and explicit rejection without transferring bytes.
It must require authoritative sender evidence, cap items and accounted bytes
globally and per peer/conversation, deduplicate offer IDs, prune incrementally,
and expose no accept-to-transfer path until capability and Resource correlation
evidence exists.
