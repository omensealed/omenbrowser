# v0.9.6-4 release qualification checklist

Target: `v0.9.6-4`  
Baseline and binary rollback: published `v0.9.6-3` (`414d8ea`)  
Reticulum/LXMF train: exact `0.9.6`

## Version and compatibility boundaries

- [x] Root and standalone server manifests/lock entries report `0.9.6-4`.
- [x] Active current-version smoke scripts require `0.9.6-4` binaries.
- [x] Immutable v0.9.6-3 fixtures and adjacent-version defaults remain
      unchanged.
- [x] Reticulum/LXMF production dependencies remain exact `0.9.6`.
- [x] OMENchat wire protocol remains version `1`.
- [x] Browser and omenchatd identities and storage roots remain separate.
- [x] Managed Reticulum remains the supported default; shared runtime remains
      deferred.

## Local feature and resource evidence

- [x] Planned v0.9.6-4 feature families pass their deterministic, fault,
      restart, mixed-shape, and bounded-resource gates.
- [x] Canonical animated/static desktop and standalone headless/full product
      graphs contain production capabilities and exclude qualification hooks.
- [x] Canonical room-media policy real-Link rejection/restart smoke passes
      without qualification hooks.
- [x] Local quick release check passes before and after the version transition.
- [x] No unexplained regression appears in the recorded deterministic and
      isolated-process resource measurements.

## Candidate gates

- [ ] Formatting and strict Clippy pass on the exact versioned candidate.
- [ ] Canonical root and standalone server test matrices pass on the exact
      versioned candidate.
- [ ] Native GitHub CI passes once on the frozen candidate.
- [ ] Pinned/current Python and mixed-version interoperability pass once on the
      frozen candidate.
- [ ] Linux, Windows, Intel macOS, and Apple Silicon packaging/lifecycle jobs
      pass from the annotated release tag.
- [ ] Published artifacts and adjacent/aggregate SHA-256 files are downloaded
      and independently verified.
- [ ] PR is merged and annotated tag `v0.9.6-4` resolves to the reviewed
      candidate.

## Rollback

Return to `v0.9.6-3` using the guarded database downgrade-copy procedure
appropriate to the active omenchatd schema. Preserve the current database and
sidecars, identities, configuration, messages, uploads, browser history, and
cache. Do not run an older server directly against the schema-13 database.
