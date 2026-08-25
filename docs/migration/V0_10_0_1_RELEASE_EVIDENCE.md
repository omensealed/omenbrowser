# v0.10.0-1 release evidence

Status: released source for immutable tag `v0.10.0-1`.

## Source and dependency boundary

- Baseline commit: `0a9a913ddf8bfc4388f065335770330495055da4`, the
  commit named by annotated tag `v0.9.9-2`.
- Release source: the qualified commit named by immutable tag `v0.10.0-1`.
- Root package and standalone `omenchatd`: `0.10.0-1`.
- Reticulum/LXMF dependencies: exact official crates.io `=0.10.0` in the
  independent root and server lockfiles. No selected 0.9.x train member, Git
  source, fork, vendor, private registry, or `[patch.crates-io]` is used.
- Preserved compatibility: OMENchat wire protocol 1,
  `omenchat-protocol 0.2.0`, SQLite schema 14, and local
  `omen-ifac-tcp 0.9.5-1`.
- Initial qualification used Rust 1.97.1. The ARM64 Cross gate updated the host
  stable toolchain to Rust 1.98.0; final canonical gates therefore record Rust
  1.98.0. The declared product MSRV remains Rust 1.85.

## API migration decisions

- New upstream configuration fields are obtained from upstream defaults. OMEN
  does not create a discovery trust-root migration or reuse a node identity as
  an operator address.
- Channel payloads retain an explicit `u16::MAX` pre-dispatch boundary.
- Typed queue, traffic, violation, active-Link, and medium-timeout data is
  mapped at the native adapter into bounded project-owned snapshots. Unknown
  remains distinct from zero, identifiers and private endpoint data are not
  exposed, and no high-frequency poller was added.
- The local IFAC adapter remains because upstream daemon IFAC authentication is
  not implemented. It continues to fail closed on wrong credentials.
- Packet-versus-Resource selection, upload bounds, identities, storage, schema,
  cancellation, and no-replay behavior remain unchanged.

## Qualification evidence

- Root desktop tests and strict Clippy passed. Server-headless and server-full
  tests and strict Clippy passed. TUI lifecycle/real-PTY, static feature,
  standalone relocation, private-storage, private-service, source/train,
  capability-documentation, workflow-security, and accepted-advisory gates
  passed.
- The native LXMF SDK/RPC test filter passed 118 tests with 6 live Python tests
  ignored; those Python lanes were run independently. Unsupported TTL,
  idempotency, correlation, extension, and remembered-ticket requirements are
  still rejected before dispatch.
- Current Python drift passed with Python 3.14.7, RNS 1.4.2, LXMF 1.1.1, and
  NomadNet 1.2.8. The immutable pinned lane passed with RNS commit
  `e32d4df754a7b87b1bf1bb0d08675d12ff505ae6` and LXMF commit
  `727830cefda83d9c6e3982b48675425f3f988f9c`.
- Adjacent-release tests used v0.9.9-2 commit
  `0a9a913ddf8bfc4388f065335770330495055da4`. Direct LXMF, 64 KiB
  Resources, propagation, OMENchat live operation, and SQLite history passed in
  both directions. Unknown-sender propagation recovery used an authenticated
  announce and a fresh recipient sync, not a second logical send.
- The rollback proof reopened schema 14 data in both directions: v0.10 read
  v0.9.9-2 writes and v0.9.9-2 read v0.10 writes with content, order, and
  metadata preserved. There is no database, identity, configuration, cache,
  message, ticket, upload-content, or Reticulum-storage migration. Rollback is
  stop-cleanly, preserve/copy state, and start the v0.9.9-2 binary; identities
  must never be regenerated.
- A one-server/two-client process gate passed message echo, reactions,
  revisions, pins, durable upload commit/fetch, old-Link retirement, server
  restart, and same-session recovery. Terminal server state had zero active or
  pending Links/Resources and zero protocol, replay, cache, or queue errors.
- A deterministic 128-generation reconnect test retained one current Link per
  destination and rejected wrong-destination opens. A 60-second server Link
  soak completed 4,911 cycles, returned active/pending Links to zero, grew RSS
  by 307,200 bytes, and grew neither file descriptors nor tasks.
- Direct/local upload qualification passed under-limit commit/fetch, restart
  projection recovery, negotiated-limit and disabled-media pre-dispatch
  rejection, cancellation, Resource reuse, multi-segment bridge cancellation,
  database/disk/quota failures, stale cleanup, and atomic storage invariants.
  No uncertain operation is automatically retried or replayed.
- Linux idle `omenchatd` measurement over 15 seconds recorded 3 CPU ticks at
  100 Hz, 11,108 KiB RSS, 7 threads, and 13 file descriptors. Runtime worker
  qualification completed 5,000 tasks with bounded queue depth. These are
  candidate measurements, not a strict cross-toolchain performance claim.
- The maintained Linux ARM64 Cross/Podman/QEMU gate passed protocol tests,
  server tests, package lifecycle, and checksums. This is emulated architecture
  evidence and is not a native ARM64 hardware claim.

## Upstream sentinels and limitations

- Split-metadata assembly and direct/local Resource complete, cancellation, and
  reuse tests pass.
- The unchanged routed fragment-loss sentinel fails because duplicate admission
  accepts `ResourceRequest` but not duplicate Resource data/proof traffic.
  Routed Resource retransmission and routed uploads remain unsupported.
- The unchanged UDP maximum-wire sentinel fails because the upstream transmit
  buffer is 456 bytes while the asserted maximum wire packet is 483 bytes.
  Maximum-size UDP Resource qualification remains unsupported.
- No local fork, fragmentation, primitive fallback, backend switch, automatic
  replay, lowered assertion, or second dispatch masks either limitation.
- Typed transport health is diagnostic only. It is not delivery, Resource
  completion, or durable application-commit evidence.

## Unavailable lanes

- No configured live external `reticulumd` RPC endpoint was available. Signed
  invitation and deterministic RPC behavior passed, but external endpoint
  availability is not claimed as send equivalence.
- Native Windows, macOS, and ARM64 hardware execution were unavailable on this
  Linux host. Maintained workflow/static checks passed; ARM64 has the emulated
  evidence described above.
- Public-network, native radio hardware, and interactive operator media/display
  measurements were unavailable and are not inferred from local parity.

## Evidence locations

Execution artifacts are isolated below
`target/upgrade-v0.10.0-1/candidate/`, including current and pinned Python,
mixed-version JSON reports, two-client smoke logs, room-media-policy results,
reconnect soak output, and idle measurements. ARM64 package evidence is below
`target/arm64-dist/`.

No crate publication or identity replacement was performed.
