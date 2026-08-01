# OMENbrowser v0.9.6-5 baseline

Date captured: 2026-07-30/31 (America/New_York and UTC evidence timestamps)

This is the no-behavior-change Phase 0 baseline for the reviewed
`OMENbrowser_rs_v0.9.6-5_review_and_phased_plan.md`. It records the state from
which work toward v0.9.6-6 begins. It does not qualify a release and does not
change application, protocol, configuration, database, or cache versions.

## Source state

- Branch: `hardening/v0.9.6-6-phase-plan`
- Baseline commit: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`
- Nearest tag: `v0.9.6-5`
- Initial and post-validation worktree: clean
- Root package: `omenbrowser_rs 0.9.6-5`
- Standalone package: `omenchatd 0.9.6-5`
- Shared wire-only package: `omenchat-protocol 0.1.0`
- Standalone IFAC compatibility package: `omen-ifac-tcp 0.9.5-1`

The root application and `src/server` remain independent Cargo roots with
independent lockfiles. The server relocation check copied `src/server` to a
temporary directory and passed without importing the browser application.

Canonical optimized product identities after rebuilding the selected profiles:

```text
OMENbrowser_rs 0.9.6-5 git_commit=2a77a753e80bb8e7db24a6411d923bf14a8e8722 target=x86_64-unknown-linux-gnu profile=desktop-product features=desktop-product:on,desktop-dev:off,desktop-test:off,mock-runtime:off,desktop-ui:on,tui:off,chat-client-reticulum:on,chat-client-rns:off,chat-client-rns-clean:on,native-reticulum:on,native-network:on
omenchatd 0.9.6-5 features=server-headless:on,server-full:on,live-reticulum:on,tui:on
```

Before the explicit server rebuild, `src/server/target/release/omenchatd` was
an old local `0.9.6-4` headless artifact. It was not treated as source truth.
Rebuilding `server-full` produced the identity above. No tracked file changed.

## Host and tools

```text
host: Linux galaxia 7.1.3-2-cachyos x86_64
rustc: 1.97.0 (2d8144b78 2026-07-07)
cargo: 1.97.0 (c980f4866 2026-06-30)
LLVM: 22.1.6
active toolchain: stable-x86_64-unknown-linux-gnu
installed Rust targets:
  x86_64-unknown-linux-gnu
  x86_64-pc-windows-gnu
rustfmt: 1.9.0-stable
Clippy: 0.1.97
cargo-nextest: 0.9.140
cargo-audit: 0.22.2
cargo-deny: 0.20.2
cargo-llvm-cov: 0.8.7
cargo-packager: unavailable
Podman: 6.0.1
Python: 3.14.6
```

The host also has `jq`, `rg`, Xvfb, i3, xdotool, xprop, xdpyinfo, perf, and
pidstat. Their presence was recorded; no host package was installed.

## Reticulum and LXMF train

Both lockfiles passed `scripts/verify-reticulum-train.sh`.

Root production graph:

- `lxmf 0.9.6` (registry)
- `lxmf-sdk 0.9.6` (registry)
- `lxmf-wire 0.9.6` (registry)
- `reticulum-rs 0.9.6` (registry)
- `reticulum-rs-core 0.9.6` (registry)
- `reticulum-rs-rpc 0.9.6` (registry)
- `reticulum-rs-transport 0.9.6` (registry)

Standalone server graph:

- `reticulum-rs 0.9.6` (registry)
- `reticulum-rs-core 0.9.6` (registry)
- `reticulum-rs-transport 0.9.6` (registry)

No private fork, Git source, or `[patch.crates-io]` override is present in this
train. Full feature-tree output was saved outside the repository:

| Profile | Lines | SHA-256 |
|---|---:|---|
| `desktop-product` | 3,432 | `de9fc3852dfc5be4c0861d1b2439b9fb5de8ee86ba818c5c127d324deec782d0` |
| `desktop-product-static-media` | 3,342 | `6589e8a2a82b7a68a2848086b7c40f5f7318b3e89436277f95bc90b010ac29a6` |
| `server-headless` | 687 | `cad058af88ef3ed2f39ec3b1f98157f4fbadeb53c9c108247ed07f00d78dc11a` |

Local evidence path: `/tmp/omenbrowser-v0965-baseline/`.

## Python interoperability references

The release-blocking pinned lane still uses immutable source commits:

- Python Reticulum:
  `15320e4d2cfabb143c1db20ca887e275fd521585`
- Python LXMF:
  `727830cefda83d9c6e3982b48675425f3f988f9c`

The informational current-package lane currently pins:

- RNS `1.4.0`
- LXMF `1.1.0`
- NomadNet `1.2.7`
- MessagePack `1.2.1`

Those live Python lanes were not rerun in Phase 0. Their versions and commands
were inspected from the current scripts and workflow, not inferred from older
review text.

## Commands and results

All commands below ran from the baseline commit without modifying tracked
source.

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo fmt --manifest-path src/server/Cargo.toml --check` | pass |
| `cargo test --locked --no-default-features --features desktop-product` | pass; 1,560 root library tests plus integration suites |
| `cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings` | pass; no compiler/Clippy warnings |
| `cargo test --locked --no-default-features --features desktop-product-static-media` | pass; 1,556 root library tests plus integration suites |
| `cargo test --locked --no-default-features --features tui` | pass; 690 root library tests plus integration suites |
| `cargo clippy --locked --no-default-features --features tui --all-targets -- -D warnings` | pass; no compiler/Clippy warnings |
| `cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full` | pass; 556 server tests, 12 explicitly ignored |
| `cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full --all-targets -- -D warnings` | pass; no compiler/Clippy warnings |
| `bash scripts/release-check.sh quick` | pass |
| `scripts/measure-omenchatd-db.sh /tmp/omenchatd-db-v0965-baseline` | pass, 60 seconds |
| `scripts/measure-omenchatd-logging.sh /tmp/omenchatd-log-v0965-baseline` | pass, 60 seconds |
| `cargo build --release --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full --bin omenchatd` | pass |

The quick release gate additionally passed:

- TUI dependency identity;
- release-version consistency;
- exact Reticulum/LXMF train;
- the accepted-advisory boundary;
- native Linux CLI identities;
- deterministic TUI lifecycle;
- Linux real-PTY TUI lifecycle;
- browser product features and focused OMENchat tests;
- server format, feature, focused tests, and standalone relocation.

The dependency check reported five allowed warnings. The machine-checked
accepted vulnerability boundary remains only:

- `RUSTSEC-2026-0194`
- `RUSTSEC-2026-0195`

Both are the documented compile-time path
`wayland-scanner 0.31.10 -> quick-xml 0.39.2`. The standalone server has no
`quick-xml` package. This is an accepted upstream/tooling boundary, not an
unreported green audit.

## Resource baseline

### Standalone database worker

The optimized production worker/store soak used an isolated temporary SQLite
root for 60 seconds:

| Measurement | Result |
|---|---:|
| Accepted/committed operations | 6,000 |
| Explicit busy rejections | 42,000 |
| Maximum in flight | 1 |
| Worker average latency | 546 us |
| Worker maximum latency | 1,389 us |
| Heartbeat maximum lateness | 1,305 us |
| Baseline RSS | 10,690,560 bytes |
| Peak/final RSS | 11,796,480 bytes |
| RSS growth | 1,105,920 bytes |
| Baseline/peak/final FDs | 13 / 13 / 13 |
| SQLite bytes | 4,685,352 |
| Reopen integrity | `ok` |

This is deterministic process-level evidence for the bounded worker under an
in-memory/captured transport workload. It is not live Reticulum traffic or
physical-disk power-loss evidence.

### Standalone bounded logger

The optimized slow-writer soak used three isolated writer lifecycles over 60
seconds:

| Measurement | Result |
|---|---:|
| Submitted records | 382,101 |
| Explicit routine drops | 353,249 |
| Priority drops | 0 |
| Write failures | 0 |
| Peak queue | 64 items / 777,932 bytes |
| Peak oldest age | 19,998 ms |
| Admission median / p95 / max | 568 ns / 1,802 ns / 133,892 ns |
| RSS growth | 5,312,512 bytes |
| FDs before / after | 4 / 4 |
| Retained files / bytes | 12 / 97,271,033 |
| Rotated lifecycles | 3 |

The imposed writer delay is a deterministic slow-consumer boundary, not a
benchmark of a particular storage device.

### Desktop measurements not collected

The long native desktop idle, pane-stress, interactive media/GPU, and physical
ARM64 measurements were not rerun in this no-change slice. The first two
require dedicated graphical measurement time and like-for-like durations; the
media harness requires interactive phase confirmation; GPU activity requires
vendor tooling; ARM64 runtime qualification requires physical hardware. The
maintainer commands remain:

```bash
HEADLESS=1 scripts/measure-desktop-idle.sh /tmp/omenbrowser-v0965-idle
scripts/measure-pane-stress.sh /tmp/omenbrowser-v0965-pane-stress
scripts/measure-omenchat-media.sh /tmp/omenbrowser-v0965-media
```

Shortened harness smoke values would not be reported as an authoritative
performance baseline.

## Evidence not run in Phase 0

- Pinned Python Reticulum/LXMF interoperability: requires the immutable source
  checkouts and is a later release/interoperability gate.
- Current Python drift lane: network/package environment and approximately
  half-hour process matrix; informational rather than a substitute for pinned
  evidence.
- Live external `reticulumd`/SDK-RPC field conformance: this is Phase 1 work,
  not established by deterministic mapping tests.
- Live NomadNet and OMENchat process matrices: already documented gates, but
  not prerequisites for recording an unchanged compile/unit baseline.
- Native Windows MSVC and macOS execution/package gates: unavailable on this
  Linux host. The installed GNU Windows target is compile-only evidence and was
  not used to claim native success.
- Radio, I2P, BLE, serial, RNode, Meshtastic, public-gateway topology, and
  physical ARM64 testing: no corresponding peer/hardware was attached.
- Physical power-loss durability: process-kill and SQLite recovery tests do
  not establish storage-device durability.

## Confirmed baseline risks and documentation drift

1. The exact ignored
   `reticulum_udp_tx_buffer_covers_max_resource_wire_packet` gate is documented
   red on pinned Reticulum 0.9.6. Current evidence attributes this to the
   upstream UDP transmit buffer being smaller than the maximum serialized
   Resource packet. It must be rerun before keeping or closing the blocker; no
   local fragmentation or reduced protocol bound is authorized.
2. Current delivery documentation says external `sdk_send_v2` loses or does
   not prove TTL/expiry, idempotency, correlation, cancellation identity,
   method/propagation, and ticket/stamp fields. This requires current public-API
   tracing and a local-daemon conformance test. No automatic uncertain retry is
   permitted.
3. `src/runtime/native_lxmf/client.rs` still contains a production
   `Mutex::lock().expect("native LXMF SDK ticket cache")` boundary. A narrow
   poison policy and focused regression are required while preserving the
   existing item/byte limits.
4. Active documentation contradicts current implementation for OMENchat
   reactions, revisions, pins, announcement rooms, slow mode, room media
   policy, and moderation audit. Several sections still label shipped paths
   dormant or cite v0.9.5/v0.9.6-2. One code-sourced authoritative capability
   matrix and drift gate are required before more activation work.
5. The Reticulum/NomadNet request adapter has substantial deterministic and
   current-Python evidence, but its complete direct request, request Resource,
   independent response Resource, correlation, timeout, cancellation, and
   no-cross-primitive-replay contract must remain protected while nearby code
   changes.

No unbounded queue, task, cache, retry, timer, or history regression was
observed by the Phase 0 gates. This is not a claim that static inspection has
proved their absence in every optional or physical runtime.

## Evidence classification and next step

- Deterministic evidence: format, compile, unit/component tests, feature/train
  assertions, codec fixtures, and bounded in-process fault tests passed.
- Process evidence: standalone relocation, CLI identities, real-PTY lifecycle,
  SQLite worker soak, and logger soak passed.
- Live local-network evidence: not rerun in Phase 0.
- Python interoperability evidence: not rerun in Phase 0.
- Physical-interface/hardware evidence: unavailable and unclaimed.

Phase 1 is safe to begin. The smallest first unit is external SDK/RPC send-field
conformance because it decides which guarantees the UI may truthfully expose.
That unit should begin with source/API tracing and deterministic tests, make no
automatic retry change, and produce an upstream-ready reproducer if the
published client still drops fields.
