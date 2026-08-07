# v0.9.7-7 release qualification checklist

Target: `v0.9.7-7`

Released baseline: `v0.9.7-6` /
`81c6a70e584a93d0c2eff06ec90a4e059a9be7bb`

## Release scope

- [x] Reusable uploads fail before offer state above the exact-train ceiling.
- [x] Smaller peer and room limits remain authoritative.
- [x] Incomplete Resource assembly diagnostics preserve no-replay evidence.
- [x] Split-Resource safeguard counters are bounded, redacted, and ephemeral.
- [x] Marker item and TTL bounds remain unchanged.
- [x] No protocol or persistent-state migration exists.
- [x] Maximum-UDP and split-metadata sentinels remain separately visible.
- [x] Both Cargo roots retain the exact official registry 0.9.7 family.

## Qualification

- [x] v0.9.7-6 maintenance root/server tests and strict Clippy.
- [x] Isolated two-client upload and continuous reconnect/restart.
- [x] Direct and Resource NomadNet timeout/cancellation/no-replay lanes.
- [x] Pinned and current Python interoperability.
- [x] Linux ARM64 Cross/QEMU test/package/lifecycle gate.
- [x] Post-version quick/full release checks.
- [x] Package candidate and package smoke.
- [ ] Hosted native Windows/macOS gates.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag and published release.

## External boundaries

- [ ] Upstream split-metadata issue fixed in an official registry release and
      the sentinel passes unchanged against it.
- [ ] Upstream maximum-UDP Resource boundary fixed.
- [ ] Physical interface/radio testing.

Tagging and publication require maintainer authorization after the applicable
release gates pass.
