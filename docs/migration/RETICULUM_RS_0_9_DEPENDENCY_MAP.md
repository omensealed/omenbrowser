# Reticulum-rs/LXMF 0.9 Dependency Map

Current resolved baseline remains the exact 0.9.5 train described below. The
approved successor target and its staged gates are recorded in
`RETICULUM_RS_0_9_6_PLAN.md`; manifests and lockfiles have not yet been changed.

This map records the Phase 1 dependency decision. It does not claim live or
Python interoperability.

## Resolved production train

Both independent Cargo roots use exact crates.io pins and committed lockfiles.
There is no Git dependency, local patch, private fork, or 0.6/0.9 source split.
`scripts/verify-reticulum-train.sh` enforces that invariant for both locked
production graphs and verifies the direct manifests retain exact `=0.9.5`
pins. It intentionally fails if an upstream family package changes version or
source, so a future immutable fix must be an explicit reviewed unit.

| OMEN root | Direct package | Version | Activated by | Purpose |
|---|---|---:|---|---|
| desktop | `lxmf` | `=0.9.5` | `native-lxmf` | Transitional umbrella exposing only the wire surface used by the adapter |
| desktop | `lxmf-sdk` as `lxmf_sdk` | `=0.9.5` | `native-lxmf-sdk` | Direct SDK dependency with explicit `std`, `sdk-async`, and `rpc-backend`; upstream's unused default ZeroMQ backend remains off |
| desktop | `reticulum-rs` | `=0.9.5` | `native-reticulum` | Core and transport umbrella |
| desktop | `reticulum-rs-transport` as `rns_transport` | `=0.9.5` | `native-reticulum` | Existing link, request/resource, interface, receipt, and event integration |
| desktop | `reticulum-rs-rpc` as `rns_rpc` | `=0.9.5` | `native-rpc` | Existing optional typed SDK/RPC backend boundary |
| server | `reticulum-rs` | `=0.9.5` | `live-reticulum` | Standalone headless core/transport entry point |
| server | `reticulum-rs-transport` as `rns_transport` | `=0.9.5` | `live-reticulum` | Standalone link/resource/interface runtime |

The root production resolution contains `lxmf-sdk`, `lxmf-wire`,
`lxmf-reference`, and `reticulum-rs-core` 0.9.5 transitively. The server
contains `reticulum-rs-core` 0.9.5 transitively. All resolve from the crates.io
registry source.

## Feature identity

Phase 1 deliberately preserves the existing feature names and product aliases:

- `native-reticulum`: Reticulum core/transport integration;
- `native-rpc`: RPC contracts and the optional external backend boundary;
- `native-lxmf`: LXMF umbrella with only its wire feature explicitly selected;
- `native-lxmf-sdk`: adds the direct SDK dependency with explicit async/RPC
  features and the RPC boundary;
- `native-network`: canonical integrated Reticulum/LXMF graph;
- `desktop-product`: canonical non-mock desktop product;
- `server-headless`: standalone daemon/admin product;
- `server-full`: headless product plus the standalone TUI.

Default features remain empty. No embedded, BLE, hardware, daemon executable,
TLS server, or ZMQ feature was enabled merely by the version alignment.

## Crate disposition

- `lxmf`: retained as the least disruptive wire entry point with defaults off.
- `lxmf-sdk`: declared directly because 0.9.5 adds ZeroMQ to its default
  features. OMEN explicitly activates only the existing async/RPC surface;
  ZeroMQ requires a separate admission and runtime-mode decision.
- `lxmf-wire`: used transitively by the existing wire feature. A direct
  declaration is unnecessary while all imported types remain available through
  the umbrella.
- `lxmf-runtime`: confirmed published at 0.9.5, but not admitted. The current
  in-process adapter must first compile and pass parity; a later distinct
  `native-lxmf-inprocess` decision must compare dependency and lifecycle cost.
- `reticulum-rs`: retained and upgraded as the normal umbrella.
- `reticulum-rs-core`: remains transitive because application/UI modules should
  consume project-owned DTOs rather than low-level core types.
- `reticulum-rs-transport`: retained directly in both roots because existing
  NomadNet, OMENchat, IFAC, link, and resource code imports it.
- `reticulum-rs-rpc`: retained only in the desktop's explicit RPC feature. It
  is not added to omenchatd.
- `omen-ifac-tcp`: a private, protocol-neutral local crate owned inside the
  standalone server tree and consumed by both products. It replaces a
  repository-relative source include, adds no third-party package, and has a
  removal path once upstream IFAC enforcement passes the pinned-Python gate.
- embedded/FFI/mininode crates: deferred from all default products.
- `lxmf-cli`, `reticulumd`, and `rns-tools`: test and diagnostics peers, not
  linked application dependencies.

## MSRV and licenses

Both OMEN packages declare Rust 1.85. Upstream 0.9.5 packages declare the same
MSRV, and the current resolved project metadata has no dependency declaring a
higher minimum. The local machine does not have Rust 1.85 installed; native CI
must add an MSRV compile lane before release rather than inferring MSRV support
from Rust 1.97 compilation.

The 0.9 family is EPL-2.0 and the application is AGPL-3.0-or-later. EPL-2.0 is
already admitted by `deny.toml`; `cargo deny check` remains the machine gate.

## Phase 1 evidence and rollback

Cargo resolution shows one coherent 0.9.5 registry train in each production
graph and no stale 0.6 family declaration in any active manifest. The direct
SDK feature selection prevents the new unused ZeroMQ default from entering the
product graph. Root desktop and server-headless checks pass; full validation is
recorded in the migration ledger.

Rollback restores the exact 0.9.0 declarations, both application versions,
three lockfiles, the prior SDK paths/feature edge, version assertions, and this
documentation together. It does not touch identity, configuration, protocol,
databases, history, uploads, or caches.
