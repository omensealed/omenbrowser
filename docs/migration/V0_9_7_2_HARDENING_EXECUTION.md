# v0.9.7-2 conservative hardening execution record

This is the living evidence record for the bounded Track A maintenance scope.
It does not authorize the deferred request-module decomposition or later
user-facing feature phases.

## Phase 0 baseline

- Baseline commit and tag: `2bf21d4cc7abfed7afda5424a76ad2e7135b71e9`
  (`v0.9.7-1`), initially clean on `main`.
- Host: x86_64 Linux 7.1.3; rustc 1.97.1; Cargo 1.97.1. The declared MSRV
  remains Rust 1.85 and the edition remains 2021.
- Both independent Cargo roots reported package version `0.9.7-1`, empty
  default features, and one registry-only exact Reticulum/LXMF 0.9.7 train.
  No Git dependency, patch source, mixed family version, or server TUI edge
  appeared in the headless graph.
- `bash scripts/release-check.sh quick`: pass.
- `bash scripts/release-check.sh full`: pass. The standalone server completed
  566 tests with 12 explicitly ignored environment/measurement tests, followed
  by strict Clippy.
- External RPC reproducer: pass as a reproducer. The real registry 0.9.7
  `RpcBackendClient` preserved method, fallback, stamp cost, fresh-ticket
  request, and daemon cancellation identity, but omitted TTL, idempotency,
  correlation, extensions, and an explicit reply ticket from `sdk_send_v2`.
- Exact maximum-UDP Resource sentinel: expected failure, exit 101. The upstream
  transmit buffer remains 456 bytes while the maximum Resource wire packet is
  483 bytes. The ignored sentinel was not changed or relabeled.
- Pinned-Python Reticulum/LXMF lane: pass against RNS
  `15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF
  `727830cefda83d9c6e3982b48675425f3f988f9c`. This included the exact IFAC
  vector, split/coalesced frames, reconnect, wrong credentials, path/announce,
  Link/proof, propagation, stamps, Resources, tickets, and restart correlation.
- Baseline raw audit: two vulnerabilities, exactly RUSTSEC-2026-0194 and
  RUSTSEC-2026-0195 on build-time `quick-xml 0.39.2` through the sole
  `wayland-scanner 0.31.10` proc-macro parent. The standalone server resolved
  neither package.
- Five-second isolated no-interface server sample: 1 CPU tick at 100 Hz,
  10,544 KiB RSS, seven threads, 13 file descriptors, readiness
  `no_interface`. The isolated root was deleted. This maintenance scope does
  not alter the server event loop or runtime thread policy.

Evidence was retained outside the repository under
`/tmp/omenbrowser-v0972-baseline`. It contains public test-fixture output and
redacted summaries, not live identities, IFAC credentials, message bodies, or
normal user paths.

## Environment-bound baseline limits

Native Windows, Intel macOS, Apple Silicon macOS, release installer lifecycle,
and hosted workflow execution cannot be proven by this Linux checkout and
remain CI gates. Physical radio/I2P hardware evidence was not run because this
maintenance scope does not change those paths. A fresh desktop GPU capture was
not run; the changed code adds no UI timer, renderer, media, or cache behavior.

## Phase decisions

### External SDK/RPC sending

The optional external sender now rejects any operation requiring a TTL,
idempotency key, correlation identifier, extensions, or explicit remembered
reply ticket before connection or dispatch. The error names only the missing
guarantee classes. Managed/integrated sending, embedded bridge behavior,
method/fallback selection, and cancellation identity are unchanged. No
automatic fallback or replay is introduced.

### Project-local IFAC TCP

The adapter retains its Python-compatible KDF, tag length, masking, HDLC, MTU,
and reconnect behavior. Tag verification now uses `subtle`'s established
constant-time primitive. A poisoned interface-configuration lock becomes one
redacted terminal status instead of panicking a long-running worker. The
retained HDLC accumulation ceiling is 524,416 bytes and the temporary
read-append ceiling is 589,952 bytes; both remain explicit and bounded.

### Advisory and workflow boundary

Registry `wayland-scanner 0.31.11` became available during execution and is the
upstream fix: it selects registry `quick-xml 0.41.0`. The precise two-package
lock update removes both accepted vulnerabilities without changing Iced or the
standalone server graph. The verifier now requires that exact fixed proc-macro
path and a zero-vulnerability raw audit. GitHub's completed-run warnings named
checkout v4, upload-artifact v4, and download-artifact v4 as Node-20 actions;
they are replaced by full-SHA-pinned Node-24 checkout v5.0.1,
upload-artifact v6.0.0, and download-artifact v7.0.0.

## Compatibility and rollback

There is no OMENchat wire, capability, database, configuration, cache,
identity, destination, or storage migration. Reticulum/LXMF remains exact
official registry 0.9.7. Rollback is the runtime validation change, IFAC
dependency/lock and worker changes, precise Wayland lock update and audit gate,
workflow pins, and associated documentation as one release-scoped unit.

## Final qualification evidence

- Post-bump `CARGO_BUILD_JOBS=2 bash scripts/release-check.sh full`: pass.
  The complete desktop suite, strict desktop Clippy, TUI lifecycle and real-PTY
  smoke, standalone relocation proof, 566 `server-full` tests (12 explicit
  environment-bound ignores), and strict server Clippy all passed. An initial
  uncapped invocation was terminated before completion when Cargo launched 27
  simultaneous integration-test linkers and host free space fell from 31 GiB
  to 13 GiB; no test failed. The identical gate was rerun with the bounded
  two-job setting and passed, with a 15 GiB low-water mark.
- Current-Python drift lane: pass with Python 3.14.6, RNS 1.4.0, LXMF 1.1.0,
  and NomadNet 1.2.7. IFAC, LXMF direct/propagated delivery, Resources,
  stamps/tickets, NomadNet direct/request-Resource/cancellation/no-replay, and
  retained-link coverage passed. Direct-send median was 39,038 microseconds
  (p95 44,173); request-Resource median was 85,520 microseconds (p95 88,645).
- Current product OMENchat upload: pass. The exact 873-byte fixture completed
  and was fetched by the sender and a second isolated client.
- Continuous reconnect: pass. One client process survived an orderly server
  restart, observed a replacement Link, and recovered message, reaction,
  revision, and pin state without replaying uncertain work.
- Current NomadNet page lane: pass. A direct encrypted request returned 309
  bytes / 17 lines of Micron from the isolated network responder.
- Mixed-version compatibility: pass against immutable application commit
  `5ba6683055fb6c59111919fbad1ac37f56a4c203` (`0.6.0-1`). Direct LXMF passed
  in both directions through Python RNS 1.4.0 with exact message-shape and
  reciprocal-ID correlation. OMENchat passed for a current client against the
  old server with legacy capability downgrade, and for the old client against
  the current server; both observed Link establishment, room join, send, and
  echo from isolated roots.
- `cargo deny check` in both Cargo roots: pass for advisories, bans, licenses,
  and sources. Only the existing duplicate-package and unused-license-allowance
  warnings remain.
- `bash scripts/release-package.sh` and
  `CARGO_BUILD_JOBS=2 bash scripts/release-check.sh package`: pass. Archive
  checksum, extraction, required files, help/version output, isolated server
  init/status, collector, and two-client packaged OMENchat smoke passed.
- `CARGO_BUILD_JOBS=2 bash scripts/test-linux-arm64-headless.sh`: pass under
  Cross/Podman/QEMU. Sixty protocol tests and 440 headless server tests passed
  (12 explicit ignores and two parent-process crash tests excluded by the
  maintained cross gate), followed by optimized packaging, checksum, and an
  isolated emulated lifecycle. This is cross/emulation evidence, not a claim
  of physical-device qualification.
- The exact ignored maximum-UDP Resource sentinel was rerun after all changes
  and retained its expected exit 101: 456-byte upstream buffer versus a
  483-byte maximum packet.

Native Windows, Intel macOS, Apple Silicon macOS, and their installer/DMG
lifecycle jobs remain hosted-CI gates because this Linux run did not push a
candidate. External reticulumd deployment was not available; the official
client loopback capture deterministically proves validation occurs before an
endpoint connection. Physical radio/I2P and GPU evidence remain outside this
maintenance scope.

## Deferred work

Track B request-module decomposition and all Track C features remain deferred.
The upstream maximum-size UDP Resource limitation remains visible and is not
worked around locally.
