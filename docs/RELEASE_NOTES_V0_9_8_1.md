# OMENbrowser_rs and omenchatd v0.9.8-1 release notes

Reticulum/LXMF crate train: exact official registry `0.9.8`

Status: final

## Reticulum/LXMF upgrade

- Both independent Cargo roots resolve one coherent official crates.io 0.9.8
  Reticulum/LXMF family. No Git dependency, fork, vendoring, or patch override
  is used.
- OMENbrowser now constructs small encrypted NomadNet requests through the
  public `Link::request_packet` helper while preserving active-Link validation,
  bound-interface dispatch, final packet-hash correlation, and exactly one
  dispatch.
- The conservative packet-versus-Resource selector remains based on the
  established packet MDU. Upstream negotiated MTU is qualified and used by
  Resource internals; dynamic primitive selection is deferred because no public
  safe payload boundary accounts for all framing overhead.

## Resource requalification

- The unchanged split-metadata sentinel passed against official 0.9.8. The
  promoted regression uses incompressible data over TCP, observes multiple
  segments, and verifies exact metadata and payload bytes.
- The temporary exact-0.9.7 efficient-Resource ceiling, split-event rejection,
  rejected-transfer markers, forced Link close, late-completion suppression,
  and associated counters are removed.
- The independent maximum-UDP sentinel still fails at the known 456-byte versus
  483-byte transmit-buffer boundary. Maximum-size UDP Resource parity is not
  claimed, and the sentinel remains explicitly ignored and separately named.
- The default 512 KiB upload behavior, 8 MiB per-Resource application ceiling,
  four-item/16 MiB pending-upload bounds, smaller negotiated peer/room/server
  limits, parser limits, cancellation, and deadlines remain unchanged.

## Compatibility boundaries

- Package versions advance to `0.9.8-1`; OMENchat protocol version `1`, frame
  layouts, operation numbers, and capability identifiers remain unchanged.
- No database, configuration, cache, identity, destination, message, ticket,
  upload-content, or Reticulum-storage migration is introduced.
- No automatic request/send retry, replay, backend switch, primitive fallback
  after dispatch, or second dispatch was added.
- The project-local `omen-ifac-tcp` adapter remains. Stock 0.9.8 TCP does not by
  itself prove Python-compatible IFAC enforcement.
- The external SDK/RPC sender remains fail-closed for TTL, idempotency,
  correlation, extensions, and explicit remembered reply-ticket guarantees
  that the published 0.9.8 client cannot preserve.

## Qualification scope

Local deterministic and live qualifications are recorded in
`docs/migration/V0_9_8_1_UPGRADE_EXECUTION.md`. The pinned Python lane, mixed
0.6/0.9.8 lanes, local package smoke, and Linux ARM64 Cross/QEMU gate pass. The
current Python drift lane is informational and retains an intermittent
propagation-Link activation timeout; its NomadNet direct/Resource matrix passes
with exact bytes. Native Windows, Intel macOS, and Apple Silicon remain hosted
release gates and must not be inferred from local Linux compilation.

## Rollback

This release changes dependency and runtime implementation code only. A
v0.9.7-7 binary can reopen the same configuration, identity, database, cache,
messages, tickets, uploads, and Reticulum storage without conversion. Roll back
the binaries and clear only transient build/runtime caches if required; do not
delete identities or application state.
