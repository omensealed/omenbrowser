# reticulum-rs-transport 0.9.8 routed Resource retransmission

Verified: 2026-08-09 against the unmodified crates.io
`reticulum-rs-transport 0.9.8` source. OMEN does not carry a local patch,
fork, vendor copy, or Git override.

## Minimal topology and result

Use a client and omenchatd on opposite sides of a forwarding TCP transport,
drop a downstream Resource data fragment after it has crossed the forwarding
transport, and allow the receiver's bounded Resource request retry to run.
The direct/local control and a small routed Resource complete. A 13,613-byte
incompressible routed attachment emits one application offer and one Resource
dispatch, then terminates after repeated Resource requests without exact final
bytes. No application replay or primitive fallback occurs.

Expected behavior is the Python Reticulum behavior: a repeated Resource request
can cause the identical data fragment to traverse the forwarding transport
again and complete the receiver. The observed Rust result is bounded retry
exhaustion.

## Relevant source behavior

In `src/transport/handler.rs`, `filter_duplicate_packets()` admits repeated
`KeepAlive`, `LinkClose`, `ResourceRequest`, and `Channel` data packets. A later
diagnostic identifies duplicate `Resource` and `ResourceProof` candidates, but
the function still returns `is_new || allow_duplicate`; those two contexts are
not part of `allow_duplicate`. The duplicate filter is transport-internal and
the published 0.9.8 application API exposes no switch for this policy.

Python Reticulum admits the corresponding Resource request, data, and proof
retransmissions. This is evidence for the reproduced topology, not a claim that
every routed Resource failure has this cause.

## Smallest upstream correction and regression

In the appropriate duplicate second pass, admit repeated
`PacketContext::Resource` and `PacketContext::ResourceProof`, matching the
reference transport's retransmission behavior. Do not broadly admit every
Resource-adjacent context.

Add a deterministic three-node test that forwards an incompressible,
multi-fragment Resource, drops one initially forwarded data fragment, observes
the receiver request it again, and asserts:

- the repeated data packet crosses the forwarding node;
- the proof can cross again when required;
- the receiver obtains byte-for-byte original content;
- one application Resource operation was dispatched;
- cancellation and retry limits remain bounded.

OMEN retains direct/local Resource support and truthful routed failure. It does
not automatically resend an uncertain attachment. Removal of this limitation
requires an official fixed crate train and the routed exact-byte regression.
