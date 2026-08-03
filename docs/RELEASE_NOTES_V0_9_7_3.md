# OMENbrowser_rs and omenchatd v0.9.7-3 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Reliability maintenance

- Standalone omenchatd live-server status, monitoring, moderation, and shutdown
  accessors fail with a typed redacted error when their mutex is poisoned. They
  no longer panic or substitute empty/zero data.
- Headless statistics poison enters the existing fatal/drain path. TUI
  monitoring remains responsive and reports unavailable current data; a failed
  moderation disconnect is never reported as successful.
- Shutdown still attempts cancellation and bounded worker joins when
  best-effort active-link enumeration fails. Normal per-link closing and
  idempotent shutdown behavior are unchanged.
- NomadNet direct and Resource response waits retain bounded event-lag counts.
  A final timeout reports that evidence and the potentially uncertain remote
  outcome, while continuing to use exactly one outbound request primitive.

## Security and compatibility

- Active documentation now matches the current audit state: the locked Wayland
  build path uses fixed `quick-xml 0.41.0`, and zero vulnerabilities are
  accepted. Historical audit reports remain unchanged as historical evidence.
- OMENchat remains protocol v1. There is no wire or capability change and no
  database, configuration, cache, identity, destination, message, ticket,
  upload, or Reticulum-storage migration.
- Send and request dispatch policy is unchanged. No uncertain operation is
  automatically retried, replayed, switched to another primitive, or moved to
  another backend.
- The known upstream maximum-UDP Resource boundary remains visible and its
  deliberately ignored sentinel is unchanged.

## Dependencies and deferred work

Both products retain the exact official registry Reticulum/LXMF 0.9.7 train.
No Git dependency, private fork, patch override, or unrelated dependency
upgrade is introduced. Large application, native-request, LXMF-client,
`reticulum_live.rs`, and TUI decompositions remain deferred.

## Rollback

No persistent migration is introduced. Rolling back to `v0.9.7-2` requires no
state conversion; browser and server roots remain reusable unchanged.
