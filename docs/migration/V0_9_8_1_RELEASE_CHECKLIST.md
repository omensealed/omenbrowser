# v0.9.8-1 release qualification checklist

Target: `v0.9.8-1`

Released baseline: `v0.9.7-7` /
`e0a1869a8c7eadd5ea52d397b86010a8945c2825`

## Release scope

- [x] Both Cargo roots resolve the exact official registry 0.9.8 train.
- [x] No Git dependency, fork, vendoring, or patch override exists.
- [x] Split-metadata sentinel passes and is promoted to a normal regression.
- [x] Temporary exact-0.9.7 split safeguards are removed.
- [x] Maximum-UDP sentinel remains independently visible.
- [x] Public `Link::request_packet` preserves request bytes and correlation.
- [x] Conservative primitive selection and application limits are unchanged.
- [x] No protocol or persistent-state migration exists.
- [x] No replay, fallback after dispatch, or second dispatch exists.

## Qualification

- [x] Root/server canonical tests and strict Clippy.
- [x] Quick/full release checks.
- [x] Two-client OMENchat, upload, reconnect, and restart.
- [x] Direct/Resource NomadNet and primitive matrix.
- [x] Pinned Python interoperability; current Python drift evidence recorded.
- [x] Linux ARM64 Cross/QEMU gate.
- [x] Package candidate and package smoke.
- [x] Mixed 0.6/0.9.8 live, history, direct-LXMF, Resource, and propagation lanes.
- [ ] Hosted native Windows/macOS gates.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag and published release.

## External boundaries

- [ ] Upstream maximum-UDP Resource boundary fixed and sentinel passes.
- [ ] Stock IFAC parity proven sufficiently to remove the local adapter.
- [ ] Physical interface/radio testing.

Tagging and publication require maintainer authorization after applicable
release gates pass.
