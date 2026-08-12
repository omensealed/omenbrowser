# Reticulum/LXMF 0.9.9 upgrade execution

Date: 2026-08-12

Baseline: branch `upgrade/v0.9.9-1` at
`ede5e131db711463f9f125f36aa8ed7bec4c44c8`, compared with released
`v0.9.8-5` at `5de9b897f79fbf2309549c680e05946da0fc9f6c`.
The baseline contained two intentional current-documentation commits beyond
the release tag and no uncommitted changes.

## Dependency result

Both independent Cargo roots resolve only official crates.io packages for the
selected train:

- `reticulum-rs`, `reticulum-rs-core`, and `reticulum-rs-transport` 0.9.9;
- root-only `reticulum-rs-rpc` 0.9.9;
- root-only `lxmf`, `lxmf-reference`, `lxmf-sdk`, and `lxmf-wire` 0.9.9.

The local `omen-ifac-tcp` package remains 0.9.5-1 and follows transport 0.9.9.
`omenchat-protocol` remains 0.2.0. No Git, fork, vendor, private registry, or
patch source is present. The root lock refresh consolidated compatible
Windows-only `windows-sys` dependency references onto an already resolved
0.61.2 entry; it introduced no new package or product feature.

## Public API and behavior decision

The existing OMEN adapter compiled against 0.9.9 without a production API
rewrite. Public `Link::request_packet`, `Link::response_packet`, Resource
request/response helpers, final packet-hash correlation, bound-interface
dispatch, subscribe-before-dispatch ordering, and cancellation ownership are
unchanged. Unknown lifecycle events remain bounded and cannot mean success.

Source and capture tests show the 0.9.9 `RpcBackendClient` still drops TTL,
idempotency, correlation, and extension fields and cannot carry an explicit
remembered reply ticket through the shipped send contract. OMEN therefore
retains pre-dispatch rejection for those external operations. Managed
integrated sending is unaffected and there is no backend fallback.

## Resource and transport boundaries

The normal split-metadata compatibility verifier and incompressible
multi-segment exact-byte regression pass on 0.9.9. Two deliberately ignored
sentinels remain independently red:

- routed fragment-loss retransmission: forwarding permits repeated Resource
  requests but not the repeated Resource data/proof needed to recover the
  dropped fragment;
- maximum UDP: the 456-byte layout-derived buffer remains smaller than the
  483-byte maximum serialized packet.

OMEN does not patch either condition, lower application bounds, fragment at a
new wire layer, or replay an uncertain transfer. Upstream-ready 0.9.9 evidence
is in `docs/upstream/`.

## Compatibility and persistence

Product versions advance together to 0.9.9-1. OMENchat wire protocol 1,
protocol crate 0.2.0, SQLite schema 14, configuration/cache formats,
identities/destinations, message/ticket/upload formats, and Reticulum storage
remain unchanged. No migration runs. The intended rollback is binary-only
after orderly shutdown and preservation of both roots.

## Qualification record

The pre-upgrade command/result and resource anchors are recorded in
`RETICULUM_RS_0_9_9_BASELINE.md`. Post-upgrade locally verified results include:

- all narrow declared root feature closures and standalone headless/full
  compilation;
- root desktop-product tests: 1655 passed, 31 deliberate ignored;
- standalone server-full tests: 615 passed, 15 deliberate ignored;
- strict root/server formatting and Clippy;
- exact train, source, product-feature, version, documentation, and Resource
  compatibility verifiers;
- consolidated quick release check, including isolated standalone relocation,
  native CLI identities, TUI lifecycle and real-PTY restoration, private
  storage/service checks, and focused product tests;
- consolidated full release check, including complete desktop-product and
  standalone server-full tests plus strict Clippy;
- maintained smoke matrix: build/feature inventory, server loopback,
  two-client OMENchat, 640 KiB Resource upload/fetch with exact storage,
  integrated LXMF loopback, NomadNet page fetch, network doctor, and chat
  scrolling passed. Optional external `lxmf-cli` and `reticulumd` lanes were
  skipped because those executables are not installed;
- Linux ARM64 protocol/server tests (484 passed, 15 deliberate ignored, five
  documented host-reexec skips) and the release archive lifecycle passed under
  Cross/QEMU through Podman;
- focused SDK/RPC rejection, Resource timeout/no-replay, LXMF topic, IFAC, and
  NomadNet responder tests;
- zero-vulnerability project advisory policy plus supported `cargo audit` and
  `cargo deny` checks.

The informational current-Python lane passes with Python 3.14.6, RNS 1.4.2,
LXMF 1.1.1, NomadNet 1.2.8, and msgpack 1.2.1. It covers IFAC vectors and
traffic, proof ordering, the four-quadrant responder, direct/propagated LXMF,
stamp/ticket policy, Resource delivery, timeout/cancellation without replay,
retained-Link recovery, and release-profile direct/Resource measurements. The
release measurement observed one reused Link, direct median/p95 of
33,867/35,563 microseconds, and request-Resource median/p95 of
35,514/38,231 microseconds across eight samples per primitive.

Short same-host idle samples showed no sustained resource growth. Standalone
omenchatd matched the baseline exactly at 10,540 KiB RSS, seven threads,
thirteen descriptors, and one versus two CPU ticks over five seconds. Desktop
median CPU moved from 1.949% to 1.940%, RSS from 220,052 to 218,756 KiB,
private dirty memory from 43,332 to 42,932 KiB, descriptors remained 61, and
close latency moved from 167 to 169 ms. One startup sample moved from 1,065 to
1,404 ms; because this was not a repeatable median, it is retained as noisy
evidence rather than described as a regression or optimized away.

The immutable pinned Python lane passes at RNS revision
`15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF revision
`727830cefda83d9c6e3982b48675425f3f988f9c`. Its first four-quadrant NomadNet
attempt completed two cases and then reached the existing deadline without a
third response. A clean rerun of the complete pinned lane passed all cases and
all subsequent IFAC, proof, propagation, stamp, direct-Resource, ticket, and
restart-recovery checks. No deadline or assertion was weakened; the initial
transient result remains retained as evidence.

Remaining live process, package, ARM64, and final wrapper results are recorded
in the retained ignored report under
`target/upgrade-v0.9.9-1/` and in the final release handoff. A lane not executed
on this commit is not treated as passing.
