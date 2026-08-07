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

Phase-specific commands, guard decisions, interoperability evidence, and
unavailable external lanes will be appended as work proceeds.
