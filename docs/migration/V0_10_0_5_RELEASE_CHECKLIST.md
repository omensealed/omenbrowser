# v0.10.0-5 release checklist

Target: `v0.10.0-5`

- [x] Root/server manifests and current version guards updated.
- [x] Reticulum/LXMF remains exact official crates.io 0.10.0.
- [x] Protocol 1, schema 14, and IFAC 0.9.5-1 unchanged; protocol crate API bumped to 0.3.0 for negotiated Channel attachment frames.
- [x] Two announce limitation records, markers, and negative guards added.
- [x] Diagnostics are bounded/redacted and ordinary roles remain non-transport.
- [ ] Negotiated Channel attachment implementation passes every final proof lane; direct and three-node routed lanes pass, while injected impairment and adjacent-binary repeats remain.
- [x] Managed RNode is honestly deferred without named hardware evidence.
- [x] Root/server lockfiles regenerated through Cargo without transitive drift.
- [ ] Quick, full, security, smoke, reconnect, Channel upload, NomadNet, and package creation gates pass on the final candidate. Local quick/full/reconnect/NomadNet/direct+routed Channel lanes pass; final package rerun remains.
- [x] Known-red routed Resource and maximum-UDP sentinels recorded separately.
- [x] Adjacent v0.10.0-4 Channel-downgrade upload/download lanes pass in both directions against immutable commit 33971db.
- [ ] Native/hosted package workflows and checksums recorded.
- [ ] Maintainer review authorizes tag, push, publication, or GitHub release.
