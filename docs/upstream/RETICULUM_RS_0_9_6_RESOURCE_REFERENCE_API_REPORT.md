# reticulum-rs 0.9.6 Resource-reference API report

Date: 2026-07-31  
Locked crate: reticulum-rs-transport 0.9.6  
Application baseline: 2a77a753e80bb8e7db24a6411d923bf14a8e8722

## Question

Can OMENbrowser implement the planned deferred Resource-reference attachment
backend using only locked public APIs while preserving explicit pre-transfer
acceptance, exact correlation, bounded streaming storage, and cancellation?

## Public surface inspected

The locked source exposes:

- Transport::send_resource and Transport::send_resource_observed;
- Transport::send_request_resource and send_response_resource;
- Transport::cancel_resource;
- Transport::resource_events;
- ResourceAdvertisement and ResourceRequest;
- ResourceEvent with progress, completion, failure, cancellation, and segmented
  progress variants; and
- ResourceComplete data, metadata, request identity, and request/response flags.

No private field, local patch, alternate Reticulum stack, or upstream repository
change was used.

## Deterministic evidence

The repository test
locked_096_public_resource_hash_is_observable_before_dispatch_and_cancellable
creates an in-memory unicast interface and activated encrypted Link through
public APIs. It proves:

1. the send_resource_observed callback receives a Resource hash before
   interface dispatch completes;
2. that hash equals the method result;
3. the encrypted Resource advertisement contains the same hash and original
   hash;
4. successful advertisement does not emit terminal delivery evidence; and
5. cancellation by that Link/hash returns true and emits OutboundCancelled
   with the same Link/hash.

This is sufficient for the sender to create a bounded mapping from the
application offer ID to the observed active Resource hash after explicit
acceptance.

## Blocking boundary

The public send methods accept the full payload as an owned byte vector.
ResourceComplete delivers the full data and optional metadata as owned byte
vectors. Correlation metadata is recovered at completion, not from
ResourceAdvertisement or progress before the payload arrives.

Therefore the reviewed public 0.9.6 surface does not provide:

- a streaming file source for outbound Resource creation;
- a streaming inbound sink suitable for same-directory staged writes;
- application correlation metadata before completed bytes are materialized;
- a durable reference that can be redeemed independently of an active Link; or
- safe resume identity across process restart.

Segmented Resources improve wire transfer behavior but do not change these
application memory/ownership boundaries.

## Decision

Sender-side observed-hash correlation and cancellation are confirmed.
Implementation of the large/deferred attachment transfer backend remains
blocked because a whole-file allocation and completion buffer would violate the
plan's streaming and low-resource requirements.

OMENbrowser will retain the dormant envelope and bounded preview owner. It will
not add application fragmentation, a private fork, an older networking stack,
automatic uncertain retry, or a misleading fetch claim. Existing bounded inline
LXMF attachments remain supported.

Reassess this decision when a later locked upstream release exposes a public
streaming source/sink and a receiver correlation boundary, or when another
already-admitted SDK surface demonstrably provides the complete lifecycle.

## Commands and result

Passed:

    cargo test --locked --no-default-features --features desktop-product \
      locked_096_public_resource --lib
    cargo test --locked --no-default-features --features desktop-product \
      locked_096_resource_complete --lib
    cargo test --locked --no-default-features --features native-reticulum \
      locked_096_public_resource --lib
    cargo test --locked --no-default-features --features native-reticulum \
      locked_096_resource_complete --lib

The minimal native-reticulum test lane initially failed on unrelated test-only
imports in adapter.rs. Those references were made feature-independent and the
lane then passed. It currently emits 12 pre-existing conditional unused/mut
warnings; canonical desktop-product Clippy is the release warning gate.

No live interface, remote peer, filesystem payload, Python process, packaging,
non-Linux platform, or hardware was used.
