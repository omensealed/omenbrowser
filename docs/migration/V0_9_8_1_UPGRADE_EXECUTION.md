# v0.9.8-1 Reticulum/LXMF upgrade execution evidence

This record tracks the conservative upgrade from released OMENbrowser_rs and
`omenchatd` v0.9.7-7 to the exact official registry Reticulum/LXMF v0.9.8
train. It is an implementation record, not a broad parity claim.

## Baseline

- Released tag: `v0.9.7-7`.
- Released and starting commit: `e0a1869a8c7eadd5ea52d397b86010a8945c2825`.
- Local branch: `upgrade/reticulum-v0.9.8`.
- Starting worktree: clean.
- Host: x86_64 CachyOS Linux, kernel 7.1.3-2-cachyos.
- Rust: rustc 1.97.1 and Cargo 1.97.1; manifests retain MSRV 1.85.
- Installed targets: x86_64 Linux, aarch64 Linux, and x86_64 Windows GNU.
- Starting packages: root/server v0.9.7-7; exact official registry
  Reticulum/LXMF v0.9.7.

Pre-edit dependency feature trees were captured locally as
`/tmp/omenbrowser-v0.9.7-7-root-tree.txt` and
`/tmp/omenbrowser-v0.9.7-7-server-tree.txt`.

## Pre-edit validation

The following completed successfully before any production or manifest edit:

```text
bash scripts/release-check.sh quick
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product --lib
cargo check --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless --lib
```

The root library reported 1,660 passing tests. The standalone-server library
reported 478 passing and 13 deliberately ignored environment/live gates. The
quick gate also passed formatting, private-storage, installer, dependency,
security, product-feature, CLI identity, TUI lifecycle/real-PTY, standalone
relocation, IFAC-vector, and focused OMENchat checks. No pre-existing local
failure was observed.

## Initial compatibility inventory

The v0.9.7 split-metadata workaround remains active at the start of the
upgrade. It consists of independent root/server `resource_compat` modules,
derived upload admission ceilings, outbound metadata preflight, inbound
multi-segment rejection, bounded rejected-transfer markers and counters,
late-completion suppression, affected-Link closure, and NomadNet split
response rejection. The unchanged split-metadata sentinel and independent
maximum-UDP sentinel live in
`src/server/src/reticulum_live_multiprocess_tests.rs` and are deliberately
ignored in fast suites.

Application-owned limits are independent and must remain unchanged: four
pending uploads, 16 MiB aggregate pending upload bytes, 8 MiB per pending
Resource, the default 512 KiB upload behavior, smaller negotiated peer/room
limits, and existing NomadNet/parser/history/deadline bounds.

## External baseline

Official upstream v0.9.8 is tag commit `5f7c962`. Its release notes include
the split-metadata correction, negotiated-MTU sizing, public Link request and
response packet helpers, opportunistic Resource compression, bounded Resource
scheduling, and additional abandonment/cancellation evidence. Upstream still
describes `RNS/Resource.py` parity as partial. No upstream statement is treated
as proof that OMEN's unchanged sentinels pass.

## Phase results

### Dependency train

Both manifests and independent lockfiles now resolve the exact official
registry 0.9.8 family. The local `omen-ifac-tcp` package version remains
0.9.5-1 while its direct transport dependency follows 0.9.8. No Git, fork,
vendor, or patch source was introduced.

### Resource sentinels and guard decision

The unchanged split-metadata sentinel passed against 0.9.8 before guard edits.
Because repetitive input may be compressed below the split threshold, the
promoted regression now uses deterministic incompressible bytes over TCP,
requires `total_segments > 1`, and verifies exact metadata and payload bytes.
It passes. The exact-0.9.7 efficient-Resource ceiling, inbound split rejection,
forced Link close, bounded rejection markers, late-completion suppression, and
their counters were therefore removed.

The independent maximum-UDP sentinel still fails with the expected 456-byte
transport buffer versus 483-byte maximum Resource packet. It remains explicitly
ignored and separately named; no local transport patch or application retry
hides it.

### Request and MTU decisions

Small native NomadNet requests use public `Link::request_packet`. Focused tests
preserve Request context, destination, decrypted bytes, final packet-hash ID,
active/bound Link checks, and one dispatch. `response_packet` was not adopted.
The conservative packet MDU selector remains because raw `link_mtu` is not a
public payload-safe boundary. Tests cover signalled MTU, the 500-byte fallback,
and smaller peer signalling; Resource internals use upstream negotiated sizing.

### Preserved boundaries

The default 512 KiB upload, four pending items, 16 MiB aggregate pending bytes,
8 MiB per Resource, smaller peer/room/server limits, parser/deadline bounds,
and no-retry/no-fallback/no-second-dispatch rules remain unchanged. There is no
wire, database, configuration, cache, identity, destination, ticket, upload, or
Reticulum-storage migration.

### Deterministic and local product qualification

The following release gates pass on the recorded Linux host:

- root `desktop-product`, `desktop-product-static-media`, and `tui` tests;
- root strict all-target Clippy for the canonical desktop, TUI, and focused
  native-Reticulum feature identities;
- standalone `server-headless` and `server-full` all-target tests and strict
  Clippy;
- `scripts/release-check.sh quick` and `full`;
- dependency-train, version, release-finalization, product-feature, TUI
  dependency, accepted-advisory, architecture, and standalone relocation
  verifiers;
- local two-client OMENchat, reaction/restart, continuous reconnect, current
  upload, and current NomadNet page gates;
- the exact direct/Resource NomadNet primitive matrix after increasing the
  deterministic response fixture above the negotiated TCP MTU and verifying
  exact bytes through the Resource branch;
- a local package candidate and the complete package extraction, checksum,
  isolated-init, help, collector, and two-client OMENchat smoke;
- Linux ARM64 cross-compilation and QEMU lifecycle/package smoke through
  Podman/Cross.

The promoted split regression is mandatory and passes natively. It is skipped
only inside the ARM64 Cross test command because that multiprocess test directly
re-executes the ARM test binary and bypasses the script's QEMU runner, the same
environmental limitation as the existing process-kill/permissive-umask
subprocess tests. All production ARM64 code still compiles and the isolated
ARM64 lifecycle runs under QEMU.

### Python and mixed-version evidence

The pinned release-blocking lane passes against Python Reticulum commit
`15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF commit
`727830cefda83d9c6e3982b48675425f3f988f9c`. It covers IFAC vectors and live
TCP/Link/proof/propagation/stamp/ticket/Resource/restart behavior.

The current drift environment used Python 3.14.6, RNS 1.4.0, LXMF 1.1.0,
NomadNet 1.2.7, and msgpack 1.2.1. The exact NomadNet direct/Resource matrix
passes with exact response bytes. The broader current-Python harness remains
informational because propagation-Link activation intermittently timed out:
one run passed all ten LXMF cases, while isolated and later full attempts timed
out before that one propagation case activated. No application retry or
semantic workaround was added for this environmental/current-drift result.

Local mixed-version gates pass for current-client/old-server and
old-client/current-server OMENchat traffic, SQLite history reopening in both
directions, reciprocal direct LXMF packet traffic, and reciprocal 64 KiB LXMF
Resource traffic between 0.6.0-1 and 0.9.8-1. The dedicated mixed propagation
lane also passes through current Python RNS 1.4.0/LXMF 1.1.0 with stable
propagation identity, queue drain, sender identity, and stamp/ticket policy
checks.

### Security and dependency policy

The repository's machine-checked advisory verifier passes with zero accepted
vulnerabilities. Root `cargo audit --no-fetch` exits successfully while listing
five warning-class findings already monitored by policy (`bincode`, `paste`,
`rustybuzz`, `ttf-parser`, and `event-listener`); the standalone server is
clean. `cargo deny check licenses bans sources` passes in both roots. A raw
root `cargo deny check advisories` reports those warning-class entries as
errors, while the repository verifier remains the authoritative policy gate.
No broad UI dependency upgrade was folded into this transport migration.

### Unavailable external lanes

Native Windows, Intel macOS, and Apple Silicon builds were not run on the Linux
host and remain hosted pre-release gates. No physical radio/public-network
claim is made. The project-local IFAC adapter remains because the upgrade does
not prove stock-interface operational equivalence.

### Package and rollback

The local candidate reports 0.9.8-1 for both products and passes the package
gate. Rollback is binary-only: v0.9.7-7 can reopen unchanged identity,
configuration, database, messages, cache, tickets, uploads, and Reticulum state.
No state conversion or downgrade procedure is required.
