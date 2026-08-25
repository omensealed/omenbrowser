# OMENbrowser_rs and omenchatd v0.10.0-1 release notes

Status: released

Reticulum/LXMF crate train: exact official crates.io `0.10.0`.

## Dependency and compatibility target

- Root OMENbrowser_rs and standalone omenchatd target `0.10.0-1`.
- Selected Reticulum/LXMF dependencies target the exact official crates.io
  `0.10.0` train while retaining independent root/server lockfiles.
- OMENchat wire protocol 1, `omenchat-protocol` 0.2.0, SQLite schema 14,
  `omen-ifac-tcp` 0.9.5-1, Rust 1.85, edition 2021, identities, storage, and
  existing bounds remain unchanged.
- No Git dependency, fork, vendor copy, private registry, patch, migration,
  automatic replay, primitive fallback, backend switch, application
  fragmentation, or second dispatch is introduced.

## Qualification state

- Dependency train and package-version migration are implemented.
- Canonical product, standalone, Resource, Request/Response, IFAC, Python,
  reconnect, upload, SDK/RPC, performance, platform, security, rollback, and
  package qualification are recorded in the release evidence report.
- Managed integrated mode remains primary. External/shared mode remains
  deferred and endpoint availability is not send-equivalence evidence.

## Compatibility and retained limits

- Routed multi-hop Resource retransmission after downstream fragment loss is
  expected to remain an upstream `reticulum-rs-transport 0.10.0` limitation. The sentinel
  remains ignored and deliberately fails for its exact documented reason.
- Maximum-size UDP Resource wire serialization is expected to remain an independent upstream
  0.10.0 limitation: the observed 456-byte buffer is smaller than the 483-byte
  maximum serialized packet. Its separate sentinel remains visible.
- OMEN carries no upstream patch, fork, vendor copy, Git override, private
  registry, application fragmentation, automatic retry, primitive fallback,
  backend switch, or second dispatch.
- Direct/local Resource qualification does not imply routed fragment-loss
  qualification. Existing upload, parser, queue, item, byte, deadline,
  cancellation, retention, and negotiated smaller limits are unchanged.
- OMENchat wire protocol remains 1, `omenchat-protocol` remains 0.2.0,
  omenchatd SQLite schema remains 14, and `omen-ifac-tcp` remains 0.9.5-1.
- No database, configuration, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration is introduced. Adjacent
  rollback to v0.9.9-2 passed with copied isolated schema 14 state in both
  directions. Stop services cleanly, preserve state, and never regenerate an
  identity.

## Diagnostics and maintainability decision

Typed 0.10 transport-health fields are mapped into optional, bounded, redacted,
project-owned snapshots through the existing event/snapshot path. Unknown stays
distinct from zero. Diagnostics are never treated as delivery or durable-commit
evidence.

## Qualification closure

The root/server canonical products, standalone relocation, current and pinned
Python lanes, adjacent v0.9.9-2 mixed-version lanes, deterministic reconnect and
upload matrices, 128-generation reconnect test, bounded server Link soak,
security checks, TUI lifecycle, and emulated ARM64 package gate passed. Native
Windows/macOS/ARM64 hardware, public-network/radio, interactive media, and a
configured external RPC endpoint were unavailable and are not claimed.

The unchanged routed Resource retransmission and UDP maximum-wire sentinels were
run and remain red. Direct/local Resources and uploads are qualified; routed
uploads and maximum-size UDP Resources are not. No workaround, retry, replay,
fallback, fragmentation, backend switch, or second dispatch was added.

Detailed commands, measurements, limitations, and rollback evidence are in
`migration/V0_10_0_1_RELEASE_EVIDENCE.md`.
