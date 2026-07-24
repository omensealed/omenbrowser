# v0.9.6-3 release qualification checklist

Target: `v0.9.6-3`  
Baseline: published `v0.9.6-2` (`5b64626`)  
Reticulum/LXMF train: exact `0.9.6`

## Version and compatibility boundaries

- [x] Root and standalone server manifests/lock entries report `0.9.6-3`.
- [x] Active smoke scripts require `0.9.6-3` binaries.
- [x] Reticulum/LXMF production dependencies remain exact `0.9.6`.
- [x] OMENchat wire protocol remains version 1.
- [x] Persistent identity, configuration, SQLite, cache, and destination
  contracts remain unchanged.

## Fix evidence

- [x] Bottom-anchor conversion and scroll-policy unit tests pass.
- [x] OMENchat deterministic scroll smoke passes.
- [x] Native LXMF service attachment round-trip smoke passes.
- [x] Current Python direct Resource interoperability passes with a
  deterministic 2 KiB binary attachment and verified SHA-256.
- [x] Maintainer manually confirmed corrected room-load and attachment-open
  scroll behavior.

## Release gates

- [x] Release version assertion passes.
- [x] Formatting passes.
- [x] Product tests pass (1,265 passed, 29 explicitly ignored) and product
  all-target Clippy passes with `-D warnings`.
- [x] Standalone omenchatd focused/relocation gates pass; its unchanged full
  test and Clippy matrix passed on the `v0.9.6-2` baseline.
- [x] Native GitHub checks pass on exact candidate `c5f0daf` in run
  `30040924119`.
- [x] Linux, Windows, Intel macOS, and Apple Silicon package jobs pass in tag
  run `30042355552`.
- [x] All 11 published distributable artifacts pass their adjacent SHA-256
  checks after an independent release download; the Linux artifacts also pass
  `release-artifacts.sha256`.
- [x] PR `#17` merged as clean candidate `414d8ea`; `main`, `origin/main`, and
  annotated tag `v0.9.6-3` resolve to that commit.

## Rollback

Return to `v0.9.6-2`. No persistent-data downgrade or conversion is required.
