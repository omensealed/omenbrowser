# v0.9.8-3 release qualification checklist

Target: `v0.9.8-3`

Released baseline: `v0.9.8-2` /
`e19b3ee726afaee4a5575062eaacd2ccce14a4bd`

## Release scope

- [x] Direct and Resource request ingress use one responder.
- [x] Complete packed response length alone selects the response primitive.
- [x] Direct responses use `Link::response_packet()`.
- [x] Large responses use `Transport::send_response_resource()`.
- [x] Portal read and complete response envelope are bounded.
- [x] Constructor and dispatch failures do not fall back or dispatch twice.
- [x] Four-quadrant Rust and pinned-Python matrices preserve exact bytes and IDs.
- [x] Exact official registry Reticulum/LXMF 0.9.8 remains unchanged.
- [x] No protocol or persistent-state migration exists.
- [x] Maximum-UDP Resource sentinel remains independently visible.

## Qualification

- [x] Focused responder unit and in-process transport tests.
- [x] Isolated Rust process direct-small-request / large-response smoke.
- [x] Pinned Python requester to Rust responder four-quadrant matrix.
- [x] Current Python drift matrix.
- [x] Full release check and local Linux package candidate.
- [x] Hosted cross-platform CI on the current candidate branch.
- [ ] Routed realistic multi-packet attachment retransmission gate; accepted as
      a disclosed upstream-bound limitation for v0.9.8-3.
- [ ] Hosted Python interoperability and native platform package gates not
      already represented by the branch CI result.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag and published release.

## External boundaries

- [ ] Upstream maximum-UDP Resource boundary fixed and sentinel passes.
- [ ] Stock IFAC parity proven sufficiently to remove the local adapter.
- [ ] Dynamic negotiated payload-MDU selector available through a public API.
- [ ] Physical interface/radio testing.

Tagging and publication require all applicable release gates to pass.
The routed attachment failure is not hidden by the passing 873-byte
direct/local fixture. The maintainer accepted it as a disclosed limitation for
this revision rather than adopting a fork, protocol change, or unsafe replay.
