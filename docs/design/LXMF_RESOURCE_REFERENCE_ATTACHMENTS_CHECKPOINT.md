# LXMF Resource-reference attachment checkpoint

Date: 2026-07-31  
Status: dormant envelope and pending-preview owner; no runtime or transfer caller

## Decision

Existing LXMF attachments remain unchanged: they are bounded inline signed
fields whose bytes arrive before presentation. The proposed capability for a
future explicitly deferred attachment path is:

```text
omen-lxmf-resource-reference-v1
```

The envelope protocol is `omenbrowser.lxmf.resource-reference`, version 1, and
is capped at 2 KiB. This checkpoint adds no capability advertisement,
negotiation, message recognition, acceptance, transfer, storage, resume,
cancellation, UI, or compatibility fallback.

## Locked 0.9.6 Resource boundary

The public `reticulum-rs-transport` 0.9.6 API exposes Resource advertisements,
link-bound resource hashes, requests, progress, proof, completion,
cancellation, and observed outbound hashes. A Resource hash identifies an
active transfer on a Link; the reviewed public API does not expose it as a
durable object that can be fetched later from an arbitrary message.

A deterministic public-API reproducer now confirms that
send_resource_observed exposes the exact outbound Resource hash before
interface dispatch, the returned hash and encrypted advertisement hash match,
and cancellation plus its terminal event use that same Link/hash pair. This is
sufficient for a sender-side offer-to-Resource correlation record. Successful
advertisement is not terminal delivery evidence.

The same locked surface still accepts outbound data as an owned byte vector and
delivers inbound completed data and metadata as owned byte vectors.
Offer-correlation metadata is not exposed in the advertisement/progress event
before the payload arrives. Consequently it does not currently supply the
streaming file source/sink and pre-transfer metadata boundary required by this
plan for larger deferred attachments.

Consequently `resource_reference` is a random 128-bit application offer and
correlation identifier encoded as 32 lowercase hexadecimal characters. It is
not a Reticulum Resource hash or proof of availability. A future authenticated
accept exchange must echo this identifier, cause the sender to initiate the
actual Link Resource, and bind the observed Resource hash to the accepted
offer. No cross-primitive retry is permitted when that outcome is uncertain.

## Version-1 envelope

The signed-LXMF envelope contains:

- exact protocol and version;
- lowercase SHA-256 content hash;
- declared size from 1 byte through 64 MiB;
- bounded MIME-type hint;
- bounded cross-platform-safe display-name hint;
- canonical claimed sender identity;
- creation and expiry, with a 24-hour maximum lifetime and five-minute clock
  skew allowance; and
- the redacted application offer/correlation identifier.

Authenticated LXMF sender evidence is required separately and must match the
signed sender claim. A payload claim cannot authenticate itself. MIME and
display name are hints only. Private storage derives its filename solely from
the verified lowercase content hash; the display name is never used as a path.
Executable-looking names are not executed. The envelope always reports that it
permits no automatic transfer, decode, or executable launch.

Optional thumbnails are deliberately absent from version 1. Adding them before
transfer admission, media isolation, and image bounds are proven would create a
second unreviewed download/decode path.

## Dormant pending-preview owner

The caller-inert owner can validate and retain a preview without moving bytes.
It requires the separately authenticated sender to match the signed claim and a
bounded caller-supplied conversation key. That key is opaque application
context, is never used as a path, and is capped at 128 visible ASCII bytes.

Pending metadata is bounded to:

- 32 offers and 64 KiB globally;
- 8 offers and 16 KiB per authenticated peer;
- 8 offers and 16 KiB per conversation;
- 8 admissions per peer and 64 globally per ten-minute window; and
- 256 rate-accounting records.

Accounting includes the exact encoded envelope, authenticated sender, and
conversation key. Capacity exhaustion rejects immediately. Offer references
are unique within the authenticated sender namespace: exact replay is a
duplicate, while reuse with different metadata or conversation context is a
conflict. Locally rejecting a preview removes its retained bytes but preserves
the bounded rate record, so repeated reject/re-offer cycles cannot bypass abuse
limits.

Expiry/rate cleanup removes at most eight records per call. Explicit clearing
releases all ephemeral state. Local rejection sends no acknowledgement,
rejection, cancellation, or other network frame. A pending preview always
reports that it cannot start a transfer, and the owner intentionally has no
accept method.

## Required next checkpoint

Before a receive preview can be connected to a runtime or any accept path can
be added, define and test:

1. exact LXMF message-title and no-inline-attachment recognition;
2. negotiated mixed-version capability behavior with zero legacy fallback;
3. authoritative sender binding at the selected managed/external backend;
4. an explicit user preference/UI owner for presenting bounded pending offers;
5. authenticated accept/reject wire semantics that move no bytes before
   acceptance;
6. sender-side source-file identity, lifetime, mutation, and quota rules;
7. transfer correlation and cancellation without uncertain automatic retry
   (sender-side observed-hash correlation is proven; receiver-side
   pre-completion correlation remains open);
8. a public streaming source/sink path, same-directory temporary storage,
   hash/size verification, atomic
   publication, and restart cleanup; and
9. backend evidence that this does not duplicate a safer SDK attachment
   lifecycle.

The direct transfer backend is therefore blocked on the locked 0.9.6 public
streaming/correlation boundary. OMENbrowser will not substitute a 64-MiB
whole-file allocation, private fork, or incompatible application fragmentation.
See docs/upstream/RETICULUM_RS_0_9_6_RESOURCE_REFERENCE_API_REPORT.md.

There is no schema, configuration, identity, or storage migration. Rollback
removes the dormant module/export and this document.
