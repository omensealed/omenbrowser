# v0.9.6-2 release qualification checklist

Target: `v0.9.6-2`  
Baseline: published `v0.9.6-1` (`7cbb470`)  
Reticulum/LXMF train: exact `0.9.6`

This checklist records release evidence. A checked local gate is not a claim
that an unrun native, Python, packaging, or live-network case passed.

## Version and compatibility boundaries

- [x] Root `omenbrowser_rs` manifest and lock entry report `0.9.6-2`.
- [x] Standalone `omenchatd` manifest and lock entry report `0.9.6-2`.
- [x] Active smoke and mixed-version scripts require `0.9.6-2` binaries.
- [x] Reticulum/LXMF production dependencies remain exact `0.9.6`.
- [x] OMENchat wire protocol remains version 1.
- [x] Existing SQLite, config, RPC, cache, identity, and destination contracts
  remain independently versioned.
- [x] The private `omen-ifac-tcp` adapter remains `0.9.5-1`; its wire contract
  did not change in this application revision.

## Local gates

- [x] `bash scripts/release-check.sh quick` (local Linux, 2026-07-22)
- [x] Root full product tests: 1,260 passed, 29 explicitly ignored; binary and
  integration suites also passed (local Linux, 2026-07-22).
- [x] Root product Clippy with `-D warnings`.
- [x] Standalone `omenchatd` full tests: 367 passed, 8 explicitly ignored.
- [x] Standalone `omenchatd` full Clippy with `-D warnings`.
- [x] Durable mutation focused regression matrix; see
  `OMENCHAT_DURABLE_V0_9_6_2_EVIDENCE.md`.
- [x] Release-mode durable retention, queue saturation, SQLite worker, Link
  reconnect, and native desktop shutdown measurements recorded in
  `OMENCHAT_DURABLE_V0_9_6_2_EVIDENCE.md`.

## Bundled hosted checkpoint

Run these once from a stable candidate rather than on each development commit:

- [ ] Native Linux/Windows/macOS CI.
- [ ] Pinned and current Python interoperability.
- [ ] Mixed v0.6.0-1 and adjacent v0.9.5-2 compatibility.
- [x] Mixed published v0.9.6-1 and candidate v0.9.6-2 OMENchat local
  compatibility: state reopen, both live directions, and old-client/current-
  server restart pass. The hosted checkpoint repeats this evidence.
- [ ] Linux formats and checksums.
- [ ] Windows portable ZIP, NSIS setup, WiX MSI, and standalone omenchatd.
- [ ] Unsigned Intel and Apple Silicon DMGs and standalone omenchatd archives.
- [ ] Package install/launch/isolated-root/uninstall and data-preservation gates.

## Release evidence

- [ ] Hosted workflow URLs and artifact manifest recorded.
- [x] Known upstream maximum UDP Resource limitation remains documented.
- [x] Accepted build-time advisory boundary is unchanged and reviewed by the
  passing local release gate.
- [ ] Release notes distinguish deterministic, live, native, and untested claims.
- [ ] Working tree is clean and the candidate commit is identified.
- [ ] Maintainer explicitly authorizes tag and publication.

## Rollback

Disable durable capability advertisement/acceptance to return to the legacy
v0.9.6-1 behavior. Preserve pending intents and server replay records; do not
delete identities, histories, configurations, databases, or caches. DMG
packaging is independent of runtime and persistent state.
