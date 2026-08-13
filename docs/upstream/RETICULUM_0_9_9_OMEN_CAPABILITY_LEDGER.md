# Reticulum 0.9.9 OMEN capability ledger

This ledger records what OMEN has actually qualified on the exact official
crates.io Reticulum/LXMF `0.9.9` train. `supported`, `unsupported`, and
`unknown` are evidence states, not inferences from a crate version. Direct or
local Resource success never promotes a routed fragment-loss result.

The machine-readable markers below are intentionally narrow release guards.
Change one only with the evidence and removal condition in the same section.

## NomadNet Request and Response

<!-- omen-capability:nomadnet-direct-request-response=supported -->

**Supported.** Rust/Rust and current-Python tests preserve the final encrypted
packet-hash request ID and exact response bytes for small direct Request and
Response packets. Removal condition: a failing exact-byte primitive test.

<!-- omen-capability:nomadnet-request-resource=supported -->

**Supported.** Oversized packed requests use one bounded request Resource.
Current Python proves the request-Resource quadrants without fallback or a
second dispatch. Removal condition: an exact-byte or exactly-once regression.

<!-- omen-capability:nomadnet-response-resource=supported -->

**Supported.** Either request ingress may receive a bounded response Resource;
the response primitive is selected independently from the packed response
size. Removal condition: a four-quadrant correlation or byte-equality failure.

## Resource transport

<!-- omen-capability:resource-split-metadata=supported -->

**Supported.** The normal, non-ignored split-metadata regression transfers
incompressible multi-segment data and verifies exact metadata and payload
bytes. Removal condition: that unchanged regression fails on the selected
official train.

<!-- omen-capability:resource-direct-local=supported -->

**Supported.** Direct/local Resource completion, cancellation, Link reuse, and
OMENchat upload/download are qualified within existing application limits.
Removal condition: a direct/local exact-byte or lifecycle regression.

<!-- omen-capability:resource-routed-fragment-loss=unsupported -->

**Unsupported.** A forwarding node on official `0.9.9` suppresses requested
duplicate Resource data/proof packets after downstream fragment loss. The
separately named ignored sentinel and upstream reproducer remain mandatory.
Removal requires an official fixed train plus a real bounded three-node loss,
retransmission, exact-byte, and one-dispatch pass.

<!-- omen-capability:resource-maximum-udp=unsupported -->

**Unsupported.** The upstream layout-derived 456-byte UDP buffer cannot hold
the 483-byte maximum serialized packet. The independent ignored sentinel and
upstream reproducer remain mandatory. Removal requires an official fixed train
plus maximum-boundary serialization and UDP loopback qualification.

## Backend contracts

<!-- omen-capability:managed-integrated-runtime=supported -->

**Supported.** The managed integrated runtime owns its interfaces, workers,
correlation, bounded cancellation, and shutdown. Removal condition: a canonical
product or lifecycle gate fails.

<!-- omen-capability:external-rpc-durable-send=unsupported -->

**Unsupported.** Published `lxmf-sdk 0.9.9` RPC serialization drops OMEN's TTL,
idempotency, correlation, and extension fields and cannot express an explicit
remembered reply ticket. Such operations are rejected before endpoint
connection or dispatch. Removal requires published end-to-end daemon capture
of every required field and cancellation identity.

<!-- omen-capability:external-shared-runtime=unknown -->

**Unknown.** Configuration is preserved, but full shared runtime ownership is
not a product mode. A configured endpoint is not capability evidence. Removal
requires bounded lifecycle, ownership, field-conformance, and interoperation
qualification without starting a competing runtime.

## Related boundaries

The stock upstream TCP interfaces do not enforce Python-compatible IFAC wire
transforms, so OMEN retains its narrow local client adapter. Dynamic
packet/Resource selection from raw `link_mtu()` is not claimed because no
public effective application-payload budget has been qualified. Neither fact
changes the two Resource limitation markers above.
