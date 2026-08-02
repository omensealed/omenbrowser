# OMENbrowser_rs and omenchatd v0.9.7-1 release notes

Reticulum/LXMF crate train: exact official registry `0.9.7`

Status: final

## Dependency and compatibility update

- Both independent Cargo roots now resolve one exact crates.io 0.9.7
  Reticulum/LXMF family with no Git source, patch override, or mixed train.
- OMENchat remains protocol v1. Identity, destination naming, configuration,
  database, cache, and wire fixture formats are unchanged.
- The project-local Python-compatible IFAC TCP adapter remains enabled because
  stock TCP IFAC replacement has not been proven end to end.

## Reliability and security

- 0.9.7 transport worker supervision is qualified together with OMEN's
  generation-scoped outer recovery: ordinary reconnect remains interface-owned
  and only terminal aggregate failure may schedule one delayed recovery.
- The IFAC adapter now cancels a backpressured receive path promptly,
  supervises paired stream tasks, reports worker join failures, uses a
  constant-time authentication-tag comparison, and uses bounded read/frame
  allocations without changing its MTU or wire bytes.
- Existing dynamic authenticated stamp-cost policy, ticket precedence,
  cancellation, bounded proof work, and no-automatic-replay rules remain.
- Advanced startup/support diagnostics include upstream's advisory software
  parity inventory, explicitly labeled as capability metadata rather than live
  interoperability proof.

## Known upstream boundaries

- The official 0.9.7 `RpcBackendClient` still omits TTL, idempotency,
  correlation, and extensions from `sdk_send_v2`, and cannot represent an
  explicit remembered reply ticket. OMEN retains fail-closed behavior and does
  not claim those daemon guarantees.
- The exact maximum-UDP Resource sentinel still fails because the upstream
  456-byte buffer cannot serialize a 483-byte maximum Resource packet. No fork,
  fragmentation workaround, weaker limit, or retry loop hides this boundary.

## Qualification status

Canonical deterministic desktop and standalone-server tests, strict Clippy,
pinned/current Python interoperability, selected mixed-version lanes, current
OMENchat/NomadNet process smokes, identity continuity, bounded resource
measurements, Linux ARM64 Cross/Podman QEMU qualification, and the Linux x86_64
release package with isolated two-client smoke are green locally. Qualification
details and external boundaries are reported in
`docs/migration/RETICULUM_RS_0_9_7_REQUALIFICATION.md`.

## Rollback

No persistent migration is introduced. The released `v0.9.6-7` binaries can
reuse the unchanged browser/server roots. Preserve normal backups and never
replace an existing identity to perform rollback.
