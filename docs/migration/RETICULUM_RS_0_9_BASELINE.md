# Reticulum-rs/LXMF 0.9 Migration Baseline

Captured on 2026-07-15 before changing application or dependency versions.
This is the rollback and regression baseline for the `upgrade/reticulum-rs-v0.9.0`
branch. Generated command output is retained locally under the ignored
`target/migration-evidence/0.6.0-1/` directories in the root and standalone
server package.

## Source and environment

- Reviewed release tag: `v0.6.0-1` at `ce3a964`.
- Hardened migration starting point: `d0c147391e89427b3e309ecfaa8de6e95b561df8`.
- Starting point is seven commits ahead of `v0.6.0-1` and is the merge commit
  from the fully green hardening pull request.
- Branch: `upgrade/reticulum-rs-v0.9.0`.
- Initial worktree: clean.
- Host: Linux x86_64, kernel `7.1.3-2-cachyos`.
- Compiler: `rustc 1.97.0`; Cargo `1.97.0`; active stable toolchain.
- Upstream 0.9 tag: annotated tag `v0.9.0`, commit
  `0859680cb45bcd0ac481e80f4cce6a52222c6fc0`.
- Upstream workspace MSRV: Rust 1.85.

Installed tools include rustfmt 1.9.0, Clippy 0.1.97, cargo-nextest 0.9.140,
cargo-audit 0.22.2, cargo-deny 0.20.2, cargo-llvm-cov, and Podman 6.0.1.
`cargo-packager` is not installed. No host package was installed for this
capture.

## Independent Cargo roots

The application root and `src/server` are independent Cargo roots with separate
lockfiles. The fuzz target is also independent. Manifests under
`official-sources/` and `vendor/` are reference material and are not production
workspace members.

No `Config.toml` file exists in the checkout. Runtime configuration is generated
or loaded beneath an explicitly selected application/server root; baseline
commands did not inspect an operator's live configuration.

The canonical product features remain:

- desktop: `--no-default-features --features desktop-product`;
- server daemon/CLI: `--no-default-features --features server-headless`;
- server with TUI: `--no-default-features --features server-full`.

Root and server default feature sets are empty. The root production graph does
not enable `mock-runtime` or the legacy `rns-net` guard features.

## Resolved 0.6 dependency train

The root production graph resolves the published crates.io 0.6.0 family:

- `lxmf`, `lxmf-sdk`, `lxmf-wire`, and `lxmf-reference` 0.6.0;
- `reticulum-rs`, `reticulum-rs-core`, `reticulum-rs-transport`, and
  `reticulum-rs-rpc` 0.6.0.

The standalone server resolves `reticulum-rs`, `reticulum-rs-core`, and
`reticulum-rs-transport` 0.6.0. Root `cargo tree -d` produced no enabled default
graph output; the server duplicate report is retained for comparison. No 0.9
package was introduced into either lockfile during capture.

Crates.io inspection confirms that 0.9.0 is published for `lxmf`, `lxmf-sdk`,
`lxmf-wire`, `lxmf-runtime`, `reticulum-rs`, `reticulum-rs-core`,
`reticulum-rs-transport`, and `reticulum-rs-rpc`. Each declares Rust 1.85 and
EPL-2.0. `lxmf-runtime` therefore does not require a Git dependency merely to
be evaluated, but it is not admitted to the application by this finding.

## Deterministic validation

The following passed against the unmodified 0.6 lockfiles:

- `cargo fmt --all --check`;
- root product `cargo check`;
- root product test matrix: the main library reported 1,048 passed and two
  ignored, followed by all integration and documentation test binaries with no
  failures;
- root canonical product Clippy without `--all-targets` and warnings denied;
- standalone server format checks;
- server `server-headless` and `server-full` checks;
- server `server-headless` tests: 167 passed, three explicit measurement tests
  ignored;
- server `server-full` tests: 289 passed, three explicit measurement tests
  ignored;
- server headless and full `clippy --all-targets -D warnings`;
- `bash scripts/release-check.sh quick`, including product feature assertions,
  Linux PTY/TUI lifecycle smoke, and focused isolated OMENchat/server tests;
- root and server `cargo deny check` (warnings reviewed; advisories, licenses,
  bans, and sources accepted by current policy).

The first requested root `clippy --all-targets -D warnings` run exposed 12
test-target lints under Rust 1.97: constant assertions, one test-only enum
variant name, test-module placement, two default-field assignments, two small
idiom lints, and one temporary-vector fixture. A prerequisite-only cleanup
resolved those findings without changing runtime behavior. Root all-target
Clippy then passed, and the affected desktop test selection passed 270 tests
with two explicit measurement tests ignored.

Installed cargo-audit 0.22.2 rejects the historical `--locked` option. Running
the supported `cargo audit` command scans the committed lockfile. The server
passes. The root reports the two already-tracked `quick-xml` 0.39.2 advisories
RUSTSEC-2026-0194 and RUSTSEC-2026-0195 through `wayland-scanner` in the Linux
Iced/rfd build graph. The required fixed `quick-xml` is outside the parent
version requirement; the exposure and no-vendor/no-ignore decision are recorded
in `docs/maintenance/DEPENDENCY_SECURITY.md`. Five unmaintained-package warnings
remain visible. This migration must re-run the audit after resolving 0.9 and
must not conceal either advisory.

## Working 0.6 behavior and retained fallbacks

The following are known-good migration targets, not deletion candidates:

- NomadNet page and form requests through the public request-resource
  compatibility path, including identify-on-connect;
- the project-local Python-compatible IFAC TCP client built on the published
  transport `Interface` trait;
- OMENchat context-zero encrypted link data plus public Reticulum resources;
- OMENchat handshake, join, room traffic, multi-client echo, resource/history,
  and reconnect behavior through the documented isolated smoke harness;
- clean LXMF direct receive/send and propagation envelope/sync paths;
- LXMF ticket offer/reply-ticket handling and locally generated propagation
  stamps;
- independent omenchatd identity, configuration, database, upload, log, and
  Reticulum roots.

Current documented limitations are also migration targets:

- no efficient public small `PacketContext::Request` helper in 0.6, so normal
  page requests use request resources;
- stock 0.6 TCP client does not apply the required Python IFAC wire transform;
- arbitrary custom OMENchat packet context is unavailable, so compatible
  context-zero link data is used;
- direct LXMF sender proof/reply correlation can time out after the receiver
  observed the message;
- peer direct stamp-cost negotiation remains incomplete;
- inbound resource cancellation before `ResourceComplete.data` allocation is
  not exposed by the pinned transport.

No fallback may be removed until its Rust-Rust and pinned-Python cases pass on
0.9 and the decision is recorded in `docs/RETICULUM_TRANSPORT_API_GAP.md`.

## Performance and resource baseline

The hardened starting commit already contains reproducible isolated harnesses
and current reference results. These are accepted as the pre-migration 0.6
baseline because `d0c1473` is the merge of that measured tree:

- desktop idle, 60-second warmup plus 600 samples: median/p95 CPU
  0.000%/2.940%, median RSS 179,754 KiB, private dirty 6,404 KiB, zero recurring
  idle application messages per minute, and 4,600.45 ms `perf stat` task-clock;
- pane stress: startup-to-window 354 ms median/406 ms p95, CPU
  2.014%/2.513%, and RSS 233,040/233,148 KiB;
- omenchatd backpressure: more than 21x producer/consumer pressure, bounded
  16/32 MiB payload lanes, 56,900/56,642 explicit rejects, maximum control
  latency 21 ms, 11 file descriptors, peak RSS delta 55,377,920 bytes, and zero
  retained permits after cancellation;
- omenchatd database: 6,000 committed events and 42,000 explicit busy rejects,
  355/1,272 microsecond average/maximum worker latency, 1,817 microsecond maximum
  heartbeat lateness, 794,624-byte RSS growth, 13 file descriptors, consecutive
  event IDs, and successful SQLite integrity check;
- omenchatd bounded logging: 382,037 records, 353,185 explicit routine drops,
  no priority loss or write failure, 565/1,778 ns median/p95 admission, bounded
  retention, and stable four-file-descriptor use.

GPU/frame-submission observation, native media visible/hidden measurements,
and physical/public-network measurements remain pending rather than being
assigned invented values. Post-migration measurements must use the same
harnesses, durations, build profile, isolated-root rules, and host where a
numerical comparison is claimed.

## Interoperability lanes

Upstream 0.9.0 pins:

- Reticulum conformance: `0319444b20e0815f26c6b9ceeba8fa44de037c9b`;
- Python Reticulum: `15320e4d2cfabb143c1db20ca887e275fd521585`;
- Python LXMF: `727830cefda83d9c6e3982b48675425f3f988f9c`.

The 2026-07-15 PyPI drift snapshot remains RNS 1.3.8, LXMF 1.0.1, and NomadNet
1.2.7. Pinned parity will be release-blocking. The explicitly versioned current
Python lane remains informational until it is stable enough to promote.

No live Reticulum, LXMF, NomadNet, public I2P, hardware-radio, mixed 0.6/0.9,
or current-Python operation was run during this local capture because no
isolated external peers or hardware were provisioned. Existing 0.6 live evidence
is historical baseline evidence; every release-required case must be rerun on
0.9 before a readiness verdict.

## CI and artifacts

Current CI has a least-privilege Linux quick job and a reusable native matrix
for Linux, Windows MSVC, macOS Intel, and macOS Apple Silicon. The package
workflow builds Linux archive/AppImage/Debian artifacts plus native Windows ZIP,
NSIS, MSI, separate omenchatd artifacts, and separate Intel/Apple Silicon DMGs,
then uploads reviewed intermediate artifacts before a narrowly permissioned
publication job. The last pre-migration PR checks were green on every native
runner.

Migration work must keep explicit `--locked --no-default-features` product
identities, update both independent lockfiles, and add the pinned/current Python
lanes without exposing secrets to untrusted jobs.

## Phase 0 gate and rollback

The dependency-edit gate is met with one explicit baseline exception: root
cargo-audit has two documented build-time `quick-xml` advisories. The Rust 1.97
all-target Clippy delta was repaired as a prerequisite. The audit must remain
visible until its parent dependency admits a fixed version.

Rollback from any later migration phase is clean while no state schema,
protocol version, identity path, or configuration path changes. Restore the two
manifests and independent lockfiles from `d0c1473`; retain identity, message,
history, configuration, cache, upload, and server state unchanged. Transient
0.9-only caches may be removed only when later migration documentation names
them explicitly. The known-good 0.6 binary/source rollback remains tag
`v0.6.0-1` plus the hardened `d0c1473` fixes.
