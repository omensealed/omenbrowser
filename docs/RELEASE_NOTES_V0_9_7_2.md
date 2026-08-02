# OMENbrowser_rs and omenchatd v0.9.7-2 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Conservative reliability and security hardening

- The optional external LXMF SDK/RPC sender now fails closed before connection
  or dispatch when an operation requires TTL, idempotency, correlation,
  extensions, or an explicit remembered reply ticket. The published 0.9.7 RPC
  client cannot preserve those guarantees. Rejection is reported as unsupported,
  never as sent or uncertain, and does not select another backend or retry.
- Managed/integrated Reticulum and LXMF sending is unchanged. Delivery method,
  propagation policy, dynamic stamp policy, ticket handling, cancellation, and
  the no-automatic-replay rule remain intact.
- The project-local IFAC TCP adapter uses `subtle` for constant-time tag
  comparison and converts a poisoned interface-configuration lock into a
  redacted terminal state. Python-compatible KDF, masking, tag length, HDLC,
  MTU, passphrase, and reconnect behavior are unchanged.
- IFAC retained and temporary receive-buffer ceilings are explicitly tested:
  524,416 retained bytes and 589,952 temporary bytes after one 64 KiB read.

## Dependency and CI maintenance

- A precise registry-only update moves `wayland-scanner` 0.31.10 to 0.31.11
  and its `quick-xml` dependency 0.39.2 to fixed 0.41.0. This resolves
  RUSTSEC-2026-0194 and RUSTSEC-2026-0195 without upgrading Iced or changing
  the standalone server graph. The release audit accepts no vulnerabilities.
- GitHub Actions use full-SHA-pinned Node-24-compatible checkout v5.0.1,
  upload-artifact v6.0.0, and download-artifact v7.0.0. Workflow permissions,
  runners, artifact names, and package matrix are unchanged.

## Compatibility and known boundary

OMENchat remains protocol v1. There is no wire, capability, database,
configuration, cache, identity, destination, or storage migration. The root
application and standalone omenchatd remain independent Cargo roots and keep
the official exact Reticulum/LXMF 0.9.7 train.

The deliberately ignored maximum-UDP Resource sentinel still fails at the
known upstream boundary: a 456-byte transmit buffer cannot serialize the
483-byte maximum Resource packet. No private fork, weaker limit, application
fragmentation, or automatic replay hides that limitation.

Track B request-module decomposition and all later feature work remain
deferred.

## Rollback

No persistent migration is introduced. `v0.9.7-1` can reuse unchanged browser
and server roots. Roll back the external validation, IFAC dependency/worker
hardening, precise Wayland lock update and audit gate, workflow pins, and
associated documentation together.
