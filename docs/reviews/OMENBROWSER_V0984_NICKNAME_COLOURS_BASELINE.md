# OMENbrowser v0.9.8-4 baseline and inventory

Date: 2026-08-09

## Baseline

- Branch: `feat/v0.9.8-4-nickname-colours`
- Released baseline: `v0.9.8-3`
- Commit: `966360ce9c9dd95b7a73b9c596357f2136613ed5`
- Initial working tree: clean
- Rust: `rustc 1.97.1`, host `x86_64-unknown-linux-gnu`
- Cargo: `cargo 1.97.1`
- Installed Rust targets: Linux x86_64, Linux AArch64, Windows GNU x86_64
- Host Python package metadata: RNS 1.1.4, LXMF 0.9.4, NomadNet 0.9.8. The
  repository-pinned Python interoperability environments remain separate release
  gates and are not inferred from these host packages.

Both Cargo roots resolve the exact official registry Reticulum/LXMF 0.9.8
family. No Git source, patch override, fork, or vendored transport was present.

## Pre-change results

- `cargo fmt --check`: pass.
- `cargo test --locked --no-default-features --features desktop-product`: pass.
- `cargo fmt --manifest-path src/server/Cargo.toml --check`: pass.
- `cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full`:
  pass, 608 passed and 14 explicitly ignored upstream/environment sentinels.
- `bash scripts/release-check.sh quick`: pass, including standalone relocation,
  TUI lifecycle/real-PTY, product feature, focused OMENchat, and native CLI
  identity checks.

## Inventory decisions

- OMENchat wire protocol remains version 1. The local shared protocol crate was
  0.1.0 and has independent crate SemVer.
- Operation values 77 through 79 were unused. Capability negotiation is already
  bounded and durable mutations already provide canonical request hashing and
  exact replay storage.
- The legacy user-list entry is exactly five fields. It must remain byte-shape
  compatible unless `nickname-colours-v1` was accepted for the target Link.
- The server schema was 13. `users.profile_revision` already existed, while no
  nickname-colour column or projection existed.
- Upload and Resource transport remain project-bounded. The exact registry 0.9.8
  duplicate filter admits repeated Resource requests but not retransmitted
  Resource data/proof packets. The prior isolated routed test therefore remains
  an upstream expected-failure boundary, not an application retry opportunity.
- The independent maximum-UDP sentinel remains deliberately ignored and named;
  its upstream buffer is derived from Rust object layout rather than maximum
  serialized wire size.

No normal user root, identity, message body, attachment body, or credential was
used by these baseline commands.
