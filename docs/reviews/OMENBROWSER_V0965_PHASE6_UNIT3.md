# Phase 6 unit 3 — low-power measurement evidence

Status: **complete for software-rendered desktop evidence**.

## Scope and current-code decision

The existing desktop idle harness already owned isolated roots, `/proc`,
`pidstat`, `perf`, startup, normal-close, and Xvfb/i3 collection. Replacing it
would add unnecessary measurement code. This unit extends that harness with an
explicit Monitoring fixture and adds a paired normal/low-power runner.

The first static-media build also exposed a reporting defect: the maintained
feature profile identified itself as `custom`. The stable product identity now
names and reports `desktop-product-static-media`, allowing measurements and
support output to identify the actual product tested.

No application runtime behavior beyond Phase 6 unit 2 changed. No dependency,
wire, database, identity, storage path, queue, cache, retry, or limit changed.

## Harness guarantees

- Every case uses a newly generated temporary application root.
- The fixture selects external Reticulum mode, disables periodic LXMF sync,
  opens Monitoring, and sets only the requested low-power value.
- The fixture is validated before launch and after normal shutdown.
- Both paired cases must use the exact same binary SHA-256.
- Warmup/sample/interval values are identical.
- Case order is recorded and can be reversed.
- Configured monitoring events are labeled as subscription cadence, not an
  observed scheduler counter.
- GPU results are explicitly absent from the software-rendered harness.

## Current evidence

Host: Linux x86_64, Rust 1.97.1, Xvfb/i3, software rendering. Runtime networking
was disabled through the isolated external-runtime fixture. These results are
same-host comparisons, not native compositor, GPU, live Reticulum, or
cross-platform claims.

### Canonical animated product

Measurement binary SHA-256 (before the reporting-only static-profile token was
added):
`a76bfe305a1732bb1f3f9c15f263b9259f521ba7cdc5e850b0820564fb3964bb`

Twenty-second warmup and 120 one-second samples per case:

| Metric | Normal | Low power | Change |
|---|---:|---:|---:|
| Configured monitoring samples/min | 60 | 12 | -80.00% |
| Median CPU | 4.878% | 0.974% | -80.03% |
| P95 CPU | 6.853% | 6.844% | -0.13% |
| `perf` task-clock | 5,711.40 ms | 1,857.38 ms | -67.48% |
| Scheduler context-switch proxy/min | 226.891 | 66.555 | -70.67% |
| Median RSS | 222,652 KiB | 223,408 KiB | +0.34% |
| Median private dirty | 45,434 KiB | 44,268 KiB | -2.57% |
| Median FDs | 60 | 60 | unchanged |
| Normal-close latency | 171 ms | 170 ms | neutral |

### Static-media product

The corrected binary reports `profile=desktop-product-static-media` and
`desktop-product-static-media:on`.

Binary SHA-256:
`0092629a491e0838351de3df90fbf53e8c4de80ef169fd2e009fb62419ec1b06`

The recorded reverse-order run used a ten-second warmup and 60 one-second
samples per case:

| Metric | Normal | Low power | Change |
|---|---:|---:|---:|
| Configured monitoring samples/min | 60 | 12 | -80.00% |
| Median CPU | 3.945% | 0.976% | -75.26% |
| P95 CPU | 5.895% | 6.880% | +16.71% |
| `perf` task-clock | 2,731.66 ms | 1,012.38 ms | -62.94% |
| Scheduler context-switch proxy/min | 313.220 | 35.593 | -88.64% |
| Median RSS | 223,140 KiB | 223,940 KiB | +0.36% |
| Median private dirty | 68,110 KiB | 67,560 KiB | -0.81% |
| Median FDs | 60 | 60 | unchanged |
| Normal-close latency | 172 ms | 171 ms | neutral |

An earlier normal-first static run had a cold first launch and materially
higher RSS/private-dirty in only that first process. The reverse-order run made
RSS effectively neutral. That first pair is retained as raw evidence but is not
used to claim a memory reduction. Likewise, P95 CPU did not improve reliably;
the defensible benefit is lower recurring median/task-clock work with no
material paired RSS or FD regression.

### Binary size

Current release linkage after the identity correction:

- animated: 57,451,976 bytes;
- static media: 57,298,080 bytes;
- reduction: 153,896 bytes (0.268%).

This is a linkage-size comparison only. The binaries differ by their intended
GIF feature graph and by their profile identity string. The reporting-only
identity correction occurred after the canonical runtime samples and does not
change the sampled subscription or application behavior.

## Files changed

- `scripts/measure-desktop-idle.sh`
- `scripts/measure-low-power-desktop.sh`
- `scripts/release-check.sh`
- `src/product_identity.rs`
- `README.md`
- `docs/TESTING.md`
- `docs/maintenance/LOW_POWER_PRESET.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE6_UNIT2.md`
- this report

## Commands and results

Passed:

```text
bash -n scripts/measure-desktop-idle.sh
bash -n scripts/compare-desktop-idle.sh
bash -n scripts/measure-low-power-desktop.sh
git diff --check

cargo build --release --locked --no-default-features \
  --features desktop-product --bin omenbrowser_rs
cargo build --release --locked --no-default-features \
  --features desktop-product-static-media --bin omenbrowser_rs
cargo test --locked --no-default-features \
  --features desktop-product-static-media product_identity::tests --lib
cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
cargo clippy --locked --no-default-features \
  --features desktop-product-static-media --all-targets -- -D warnings
bash scripts/verify-product-features.sh
bash scripts/test-native-cli-identity.sh
shellcheck scripts/measure-desktop-idle.sh \
  scripts/compare-desktop-idle.sh scripts/measure-low-power-desktop.sh

WARMUP_SECONDS=3 SAMPLE_SECONDS=10 HEADLESS=1 \
  bash scripts/measure-low-power-desktop.sh \
  /tmp/omenbrowser-low-power-harness-smoke-20260731

WARMUP_SECONDS=20 SAMPLE_SECONDS=120 HEADLESS=1 \
  bash scripts/measure-low-power-desktop.sh \
  /tmp/omenbrowser-low-power-canonical-20260801

WARMUP_SECONDS=10 SAMPLE_SECONDS=60 HEADLESS=1 \
  CASE_ORDER=low-power-first \
  OMENBROWSER_BINARY=<static-media-binary> \
  bash scripts/measure-low-power-desktop.sh \
  /tmp/omenbrowser-low-power-static-media-reverse-20260801
```

The long pairs passed their same-binary, fixture-preservation, process-return,
configured-message reduction, and idle CPU gates.

## Tests not run

- No native interactive compositor or vendor GPU capture was available.
- No Windows or macOS measurement was run.
- No live Reticulum/LXMF/OMENchat peer or identity was used.
- The existing pane/media/server workloads were not rerun because this unit
  changes only the desktop Monitoring cadence and build identity. Their current
  procedures and prior evidence remain recorded separately.
- No remote CI or packaging workflow was triggered.

## Compatibility, rollback, and remaining risks

The measurement fixture is generated only under `mktemp` and contains no
identity or secrets. Product identity gains one stable feature token; consumers
that search tokens remain compatible. There is no persistent data or protocol
migration.

Rollback removes the paired runner and fixture options, removes the static
profile/token from `product_identity`, and reverts the associated docs. The
Phase 6 unit 2 runtime setting can be rolled back independently.

Software-rendered evidence supports the low-power policy without showing an RSS
or FD regression. Native GPU/frame evidence and longer interactive multi-pane,
live-transfer, and hardware-specific qualification remain distinct pending
work; they are not blockers for the bounded setting itself.

## Next smallest justified step

Move to Phase 7 documentation/release-qualification inventory. Do not add more
low-power cache or concurrency knobs without a workload identifying a concrete
resource owner.
