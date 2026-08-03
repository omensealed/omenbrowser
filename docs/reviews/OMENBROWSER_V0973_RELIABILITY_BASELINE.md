# OMENbrowser v0.9.7-3 reliability baseline

Captured: 2026-08-03T09:59:41-04:00

This note records the unmodified `v0.9.7-2` baseline required by
`official-sources/OMENbrowser_v0.9.7-2_review_and_v0.9.7-3_phase_plan.md`.
It is current execution evidence, not a rewrite of historical reports.

## Checkout and host

- Branch: `main`.
- HEAD: `7deaafa6a1827588fec3a444b8707ff93fa1c93d`.
- Describe: `v0.9.7-2`.
- Working tree before the baseline: clean.
- The checkout exactly matches released `v0.9.7-2`; it does not differ from
  reviewed commit `7deaafa`.
- Host: Linux `galaxia` `7.1.3-2-cachyos`, x86_64.
- Rust: `rustc 1.97.1`, host `x86_64-unknown-linux-gnu`, LLVM 22.1.6.
- Cargo: `cargo 1.97.1` from the stable toolchain.
- Filesystem at capture: 1.9 TiB total, 417 GiB used, 1.4 TiB available.

## Package and dependency identity

- Root `omenbrowser_rs`: `0.9.7-2`; default features are empty; canonical
  product feature is `desktop-product`.
- Standalone `src/server` `omenchatd`: `0.9.7-2`; default features are empty;
  canonical headless/full profiles remain independent.
- Root train: registry-only `lxmf`, `lxmf-sdk`, `lxmf-wire`, `reticulum-rs`,
  `reticulum-rs-core`, `reticulum-rs-rpc`, and `reticulum-rs-transport` 0.9.7.
- Server train: registry-only `reticulum-rs`, `reticulum-rs-core`, and
  `reticulum-rs-transport` 0.9.7.
- No mixed family version, Git source, patch override, or ZeroMQ SDK backend
  was resolved by the train verifier.
- The headless server dependency graph excludes Ratatui and Crossterm.

## Confirmed findings before behavior changes

- `LiveServerWorker<T>` has six synchronous production accessors that call
  `expect("live-server worker lock")`: statistics, closed-link summaries,
  room counts, link summaries, identity counts, and identity disconnection.
  Async access already maps poison to the redacted typed message
  `live-server worker lock poisoned`.
- Callers include the headless statistics loop, TUI startup/recovery/status and
  monitoring projections, active-user checks, moderation, and shutdown.
- `src/runtime/native/request.rs` has localized ignored broadcast-lag branches
  in both direct-response and Resource-response waits. The request-Resource
  path logs Resource lag but does not retain it in final timeout evidence; the
  direct stream also discards lag counts. A narrow no-replay change appears
  feasible and will be tested before adoption.
- Active README security text is stale: it still describes two
  `quick-xml 0.39.2` advisories as accepted. The lockfile/verifier instead use
  `quick-xml 0.41.0` and accept zero vulnerabilities.
- The deliberately ignored maximum-UDP Resource sentinel remains an explicit
  upstream boundary. This revision must not hide or weaken it.

## Unmodified quick baseline

Command:

```text
CARGO_BUILD_JOBS=2 bash scripts/release-check.sh quick
```

Result: pass (exit 0).

Observed passing gates:

- formatting and shell-script syntax;
- Ratatui 0.30.2 / Crossterm 0.29.0 dependency boundary;
- root/server version consistency;
- registry-only exact Reticulum/LXMF 0.9.7 train;
- zero accepted vulnerabilities and absence of `quick-xml` from omenchatd;
- desktop, TUI, headless-server, and full-server CLI product identities;
- isolated TUI lifecycle tests;
- Linux real-PTY resize and signal restoration (66--67 ms observed shutdown);
- canonical browser product checks and focused OMENchat tests;
- standalone omenchatd relocation/build check from a temporary root;
- pinned Python IFAC byte-vector unit test and local IFAC bounds/tamper tests;
- headless and full omenchatd checks and focused history/configuration tests.

No pre-existing failure occurred in the quick gate. The relocation test kept
all state under temporary roots. Three explicit live pinned-Python tests were
reported ignored by their test definitions and were not represented as passes.

## Environment-bound work not run in the quick baseline

- Native Windows and macOS jobs require their hosted native runners.
- Linux ARM64 requires the repository Podman/container lane or hosted ARM64
  job; it was not part of this initial quick command.
- Pinned/current Python multi-process interoperability requires its explicit
  scripts and local Python environments.
- Live multi-client, restart, upload/Resource, and NomadNet peers require their
  explicit isolated smoke lanes.
- The deliberately ignored maximum-UDP Resource sentinel requires explicit
  invocation and remains expected to expose the documented 456-versus-483-byte
  upstream failure.
- No GPU or physical-radio evidence is claimed by this maintenance baseline.

## Baseline decision

The checkout is suitable for the scoped implementation. Begin with typed
live-server poison handling and deterministic poison regression tests, retain
normal successful projections and shutdown behavior, then evaluate the narrow
request-lag evidence change. Do not alter protocol, persistence, identity,
dispatch count, or dependency train.
