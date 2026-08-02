# v0.9.7-1 release qualification checklist

Target: `v0.9.7-1`

Released baseline: `v0.9.6-7` /
`14359fd567660839eb2ab0995b73acf542a1c4ac`

## Upgrade scope

- [x] Root and standalone server packages report `0.9.7-1`.
- [x] Both roots resolve one exact official registry 0.9.7 family.
- [x] The nested IFAC adapter pins transport exactly to 0.9.7.
- [x] Rust 1.85, edition 2021, feature identities, and independent roots remain.
- [x] OMENchat protocol/fixtures and persistent schemas remain unchanged.
- [x] Dynamic relay stamp policy and no-automatic-replay behavior remain.
- [x] External SDK/RPC field loss is requalified and remains fail closed.
- [x] The maximum-UDP Resource failure is rerun and remains visible.

## Local deterministic gates

- [x] Baseline and upgraded formatting/check profiles.
- [x] Canonical desktop tests and all-target strict Clippy.
- [x] Standalone server-full tests and all-target strict Clippy.
- [x] IFAC wire/bounds/tamper/wrong-credential tests and strict Clippy.
- [x] Static-media desktop and TUI feature gates.
- [x] Standalone relocation verification.
- [x] Full release-check and release-finalization checks.
- [x] Current upload, reconnect, restart, retained-link, and NomadNet smokes.
- [x] Pinned and current Python interoperability reports.
- [x] Same-host post-upgrade resource measurements.
- [x] ARM64 headless Cross/Podman QEMU qualification (not physical hardware).
- [x] Linux x86_64 release package and isolated package-smoke qualification.

## External/publication gates not claimed locally

- [ ] Hosted pull-request CI.
- [ ] Native Windows packaging and smoke.
- [ ] Native Intel and Apple Silicon macOS packaging/DMGs and smoke.
- [ ] Physical interface/radio testing.
- [ ] Reviewed candidate merged to `main`.
- [ ] Annotated tag and published release.

## Release blockers

Candidate notes must remain final for release finalization. Any deterministic
failure, train drift, identity change, automatic uncertain replay, protocol or
schema drift, unbounded work, IFAC regression, or hidden maximum-UDP limitation
also blocks publication. No tag or remote publication is authorized by this
checklist.
