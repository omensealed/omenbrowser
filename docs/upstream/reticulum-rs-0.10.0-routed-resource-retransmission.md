# reticulum-rs-transport 0.10.0 routed Resource retransmission

Verified on 2026-08-12 against the unmodified crates.io
`reticulum-rs-transport 0.10.0` source. OMEN does not carry a local patch,
fork, vendor copy, or Git override.

## Topology and observed boundary

The release sentinel models a client and recipient separated by a forwarding
transport, an incompressible multi-packet Resource, loss of a downstream data
fragment, and the receiver's bounded request for that fragment. Direct/local
Resources and small routed controls are not equivalent evidence.

The unchanged 0.10.0 source still permits duplicate `ResourceRequest` packets
through the forwarding duplicate filter, but does not permit the corresponding
duplicate `Resource` data or `ResourceProof` packets. The routed loss case
therefore remains an expected upstream limitation: one application Resource is
dispatched, retransmission cannot complete through the forwarding node, and
OMEN does not replay or switch primitives.

## Relevant source behavior

In `src/transport/handler.rs`, `filter_duplicate_packets()` builds an
`allow_duplicate` decision for selected contexts including
`PacketContext::ResourceRequest`. Diagnostic classification later recognizes
duplicate `Resource` and `ResourceProof` candidates, but those contexts are
not included in the returned duplicate allowance. The policy is internal to
the transport crate and the official 0.10.0 public API exposes no application
setting that changes it.

Python Reticulum permits the corresponding Resource request, data, and proof
retransmissions. That comparison explains this exact topology; it does not
claim every routed Resource failure has the same cause.

## Smallest upstream correction and regression

In the appropriate duplicate second pass, admit repeated
`PacketContext::Resource` and `PacketContext::ResourceProof`, matching the
reference transport's retransmission behavior. Do not broadly admit all
Resource-adjacent contexts.

Add a deterministic three-node test that forwards an incompressible,
multi-fragment Resource, drops one initially forwarded data fragment, observes
the receiver request it again, and asserts:

- the repeated data packet crosses the forwarding node;
- the proof can cross again when required;
- the receiver obtains byte-for-byte original content;
- one application Resource operation was dispatched;
- cancellation and retry limits remain bounded.

OMEN retains direct/local Resource support and reports routed failures without
automatic resend. Removing this limitation requires an official fixed crate
train plus the routed exact-byte regression.
