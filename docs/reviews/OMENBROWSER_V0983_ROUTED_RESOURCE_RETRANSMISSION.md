# v0.9.8-3 routed Resource retransmission review

Date: 2026-08-08

Candidate branch baseline: `3ce01040e107b5ff3f8c38076255465135e18cdb`

Dependency boundary: exact official registry `reticulum-rs-transport 0.9.8`;
no Git source, patch override, fork, or vendored source.

## Reproduction summary

All runs used isolated temporary client roots and synthetic attachment bytes.
No normal user identity, message body, attachment body, private destination, or
credential is retained here.

| Topology | Payload | Result |
| --- | ---: | --- |
| direct/local two-client smoke | 873 bytes | upload and fetch completed exactly |
| direct/local two-client smoke | 54,427 bytes | upload and fetch completed exactly |
| multi-hop TCP-gateway route | 873 bytes | upload and fetch completed exactly |
| multi-hop TCP-gateway route | 13,613 incompressible bytes | repeated Resource requests, then bounded retry exhaustion |

The larger routed attempt sent one upload offer and one Resource operation. It
did not perform application replay, primitive fallback, or a second dispatch.
Changing the selected gateway changed route ownership as designed but did not
make the in-progress Resource complete.

## Source comparison

The exact Rust `0.9.8` transport duplicate filter admits duplicates for
`KeepAlive`, `LinkClose`, `ResourceRequest`, and `Channel`. It records a
diagnostic for a duplicate `Resource` packet but does not admit that packet.

The installed Python Reticulum transport filter explicitly admits duplicate
`RESOURCE_REQ`, `RESOURCE_PRF`, and `RESOURCE` packets. Resource data fragments
are deterministic for a repeated send. On a routed transfer, a forwarding
transport can therefore remember the first fragment and suppress the identical
retransmission even when the initially forwarded copy was lost downstream.

This comparison explains the observed retransmission failure and identifies a
parity gap. It does not claim that every routed Resource failure has the same
cause.

## Product decision

- Preserve the exact official registry train.
- Preserve bounded retries and truthful terminal failure.
- Preserve route-scoped recovery only for a later explicit user attempt.
- Do not replay an uncertain upload automatically.
- Do not add an OMENchat wire change or application fragmentation here.
- Do not patch or fork the upstream crate.
- Disclose realistic routed attachment failure in v0.9.8-3 and defer broader
  qualification rather than adopting a fork, wire change, or unsafe replay.

Removal of this boundary requires an official corrected crate train plus a
routed, incompressible, multi-packet upload/fetch test that exercises
retransmission and verifies exact final bytes.
