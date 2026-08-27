# Testing

## v0.10.0-5 limitation evidence

The capability-doc verifier structurally preserves the passive-announce and
announce-broadcast limitations. A meaningful automated passive-table soak is
unavailable because official 0.10.0 exposes no public table accessor; missing
traffic or a process-only sample is not a pass. The routed fragment-loss and
maximum-UDP ignored sentinels remain separate known-red runs. Adjacent
`v0.10.0-4` OMENchat lanes run in both client/server directions without changing
wire protocol 1 or schema 14.

This is the current test and qualification guide for OMENbrowser_rs and the
independently packaged `omenchatd`. Repository scripts are authoritative when
their commands differ from prose.

## v0.10.0-1 qualification closure

The adjacent immutable baseline is v0.9.9-2 commit
`0a9a913ddf8bfc4388f065335770330495055da4`, not v0.6.0-1. The historical
`run-mixed-0-6-0-9-*` filenames are retained for compatibility, but their
defaults now exercise v0.9.9-2 and expect announcement-room capabilities.
Direct LXMF, 64 KiB Resource, propagation, live OMENchat, and schema 14 history
passed in both directions. Propagation unknown-sender recovery performs an
authenticated announce plus a fresh recipient sync and never repeats the
logical send.

Current Python drift passed with Python 3.14.7, RNS 1.4.2, LXMF 1.1.1, and
NomadNet 1.2.8. Pinned RNS/LXMF commits are recorded in
`migration/V0_10_0_1_RELEASE_EVIDENCE.md`. Native Windows/macOS/ARM64 hardware,
public radio/network, interactive media, and an external RPC endpoint were
unavailable. The ARM64 Cross/Podman/QEMU lane is emulated architecture evidence,
not a hardware claim.

## Protect normal identities and data

Never run integration tests against a normal browser root or server home. Use a
separate root for every process:

```text
/tmp/omenbrowser-rs-test
/tmp/omenbrowser-rs-test-2
/tmp/omenchatd-test
```

Validate the boundary before a live test:

```bash
bash scripts/release-root-sanity.sh \
  --browser-root /tmp/omenbrowser-rs-test \
  --browser-root-2 /tmp/omenbrowser-rs-test-2 \
  --server-home /tmp/omenchatd-test
```

Tests must not share identity, Reticulum storage, SQLite files, message stores,
or pane state unless the test explicitly exercises restart behavior using a
copy of generated test data.

## Standard gates

Run the fast gate while iterating:

```bash
bash scripts/release-check.sh quick
```

Run the full local gate before a release candidate:

```bash
bash scripts/release-check.sh full
```

Build a candidate and validate the staged archive:

```bash
bash scripts/release-package.sh
bash scripts/release-check.sh package
```

The package gate checks archive contents, product identity, isolated
`omenchatd` initialization, support-bundle redaction, and a multi-client
OMENchat smoke. It never needs a normal application root.

## Canonical Cargo matrix

Root desktop product:

```bash
cargo fmt --check
cargo check --locked --no-default-features --features native-lxmf
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
cargo test --locked --no-default-features \
  --features desktop-product-static-media
```

Terminal product:

```bash
cargo test --locked --no-default-features --features tui
cargo clippy --locked --no-default-features \
  --features tui --all-targets -- -D warnings
```

Standalone server:

```bash
cargo fmt --manifest-path src/server/Cargo.toml --check
bash src/server/scripts/verify-standalone.sh check
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
```

Do not use `--all-features` as a product test. Some optional features are
development-only, platform-specific, or intentionally excluded from canonical
products.

## Dependency and security checks

```bash
bash scripts/verify-release-version.sh
bash scripts/verify-reticulum-train.sh
bash scripts/verify-product-features.sh
bash scripts/verify-tui-dependencies.sh
bash scripts/verify-private-storage-policy.sh
bash scripts/verify-accepted-advisories.sh
bash scripts/verify-workflow-security.sh
bash scripts/verify-reticulum-resource-compat.sh
bash scripts/verify-reticulum-capability-docs.sh
cargo audit --locked
cargo audit --locked --file src/server/Cargo.lock
cargo deny --locked --all-features check licenses bans sources
cargo deny --manifest-path src/server/Cargo.toml \
  --locked --all-features check licenses bans sources
```

Both Cargo roots must resolve the exact registry Reticulum/LXMF train documented
in [Current Status](CURRENT_STATUS.md). Git sources, private forks, vendoring,
and patch overrides are release blockers.

## Local smoke suites

Run the complete scripted smoke set:

```bash
bash scripts/smoke/all.sh
```

Focused lanes are under `scripts/smoke/`:

- `02_omenchat_server_loopback.sh` — server startup and local Link exchange;
- `03_omenchat_two_client.sh` — isolated two-client OMENchat;
- `04_omenchat_resource_transfer.sh` — Resource upload/download paths;
- `05_lxmf_service_loopback.sh` and `06_lxmf_cli_interop.sh` — LXMF paths;
- `08_nomadnet_page_fetch.sh` — NomadNet direct/Resource page fetch;
- `09_network_doctor.sh` — diagnostics;
- `10_omenchat_scroll.sh` — initial anchoring and scrollback preservation.

Other maintained process gates include:

```bash
bash scripts/test-desktop-shutdown.sh
bash scripts/test-tui-lifecycle.sh
bash scripts/test-tui-real-pty.sh
bash scripts/test-omenchatd-crash-recovery.sh
bash scripts/run-omenchat-continuous-reconnect.sh
bash scripts/run-omenchat-current-upload.sh
bash scripts/run-omenchat-current-upload.sh --routed
bash scripts/run-omenchat-current-upload.sh --impaired
bash scripts/run-nomadnet-current-page.sh
```

Report missing display servers, PTYs, peers, network routes, or platform tools;
do not translate an unavailable lane into a pass.

## Python and mixed-release interoperability

Pinned Python sources provide the release-blocking reference lane:

```bash
bash scripts/run-pinned-python-reticulum.sh \
  --rns-source /path/to/pinned/Reticulum \
  --lxmf-source /path/to/pinned/LXMF
```

The current Python drift lane is informational and may identify upstream
drift. For the v0.10.0-1 candidate it resolves RNS 1.4.2, LXMF 1.1.1, and NomadNet 1.2.8:

```bash
bash scripts/run-current-python-drift.sh \
  --report target/current-python-drift-report.json
```

Mixed-release scripts under `scripts/run-mixed-0-6-0-9-*.sh` qualify the
maintained compatibility boundary. They build immutable historical source in
isolated targets and must not reuse normal roots. The live OMENchat harness
defaults to the legacy capability-shape assertions used by the v0.6 boundary.
For an adjacent release that already implements the negotiated capability
fabric, set `OMEN_MIXED_EXPECT_LEGACY_CAPABILITIES=0`; this keeps the common
link/session/join/message/restart assertions while avoiding a false claim that
the adjacent peer should suppress capabilities it legitimately supports.

## Resource limitations and sentinels

The Reticulum 0.10.0 split-metadata regression is a normal passing test. The
independent maximum-UDP wire-buffer limitation remains an explicit ignored
sentinel:

```bash
bash scripts/verify-reticulum-resource-compat.sh

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  reticulum_split_metadata_assembly_preserves_segment_two_payload \
  -- --nocapture

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  reticulum_routed_resource_retransmission_survives_fragment_loss \
  -- --ignored --nocapture

cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  reticulum_udp_tx_buffer_covers_max_resource_wire_packet \
  -- --ignored --nocapture
```

The routed and UDP sentinels are expected to expose their independent upstream
limitations until an official fixed crate train is adopted and requalified.
Do not weaken, rename, merge, or silently skip either one. The routed boundary
is documented in
[Reticulum Transport API Gaps](RETICULUM_TRANSPORT_API_GAP.md).
The full release gate also runs fixture tests that prove both structural
verifiers reject a renamed, unignored, or weakened limitation and reject stale
or promoted capability claims. These checks do not treat an arbitrary failing
`cargo test` result as expected upstream evidence.

## Hosted and platform evidence

GitHub workflows provide evidence that cannot be inferred from Linux:

- `.github/workflows/ci.yml` — Linux quick gate and native Windows/macOS;
- `.github/workflows/python-interop.yml` — pinned Python, current drift, and
  mixed-release lanes;
- `.github/workflows/linux-arm64-headless.yml` — native ARM64 `omenchatd`;
- `.github/workflows/package.yml` — Windows, macOS, and Linux artifacts plus
  package smoke.

Record the workflow URL, commit SHA, and individual job conclusions. A workflow
on another commit is not evidence for the candidate.

## Measurements

Measurement scripts use generated or isolated data:

- `measure-desktop-idle.sh`, `compare-desktop-idle.sh`;
- `measure-low-power-desktop.sh`, `measure-pane-stress.sh`;
- `measure-omenchatd-idle.sh`, `measure-omenchatd-links.sh`;
- `measure-omenchatd-db.sh`, `measure-omenchatd-backpressure.sh`;
- `measure-omenchat-media.sh`, `measure-durable-mutation-retention.sh`.

Compare repeated samples on the same host. Do not claim a performance change
from a single noisy run.

## Release evidence policy

Current commands and supported behavior belong in this guide. Published release
notes preserve version-specific outcomes. Detailed phase-unit transcripts and
superseded implementation plans are intentionally absent from the current tree;
they remain available in Git history and immutable release tags. See
[Documentation History](HISTORY.md).
