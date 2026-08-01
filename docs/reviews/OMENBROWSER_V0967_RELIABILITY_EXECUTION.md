# OMENbrowser v0.9.6-7 reliability execution ledger

Date started: 2026-08-01 (America/New_York)  
Working branch: `hardening/v0.9.6-7-reliability`  
Released baseline: `v0.9.6-6` / `e04dc8f93e0121774fecc62c8f25f95f0fce6f71`  
Target version after pre-bump gates: `0.9.6-7`

## Scope and invariants

This ledger executes
`official-sources/OMENbrowser_v0.9.6-7_reliability_efficiency_phase_plan.md`.
The Reticulum/LXMF registry train remains exactly `0.9.6`; OMENchat remains
protocol version 1; browser and server roots, identities, databases, and
lockfiles remain independent. No uncertain operation gains an automatic retry.
No mandatory database migration is in scope.

The maintainer also supplied a live TUI screenshot showing upstream
`link: close /…/` lines overwriting the Ratatui alternate screen. The pinned
transport prints those lines directly to stdout from
`destination/link_parts/link_sections/next_watchdog_deadline.rs`. The local
maintenance requirement is to keep the TUI responsive and restore its owned
surface after runtime output without patching the upstream crate or changing
normal headless output contracts.

## Phase 0 — untouched baseline

### Environment

- Host: Linux `x86_64-unknown-linux-gnu`; kernel
  `7.1.3-2-cachyos` on `galaxia`.
- Compiler: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6.
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`.
- Active toolchain: `stable-x86_64-unknown-linux-gnu`.
- Root and standalone server packages: `0.9.6-6`, MSRV `1.85`.
- Direct Reticulum/LXMF dependencies: exact crates.io `0.9.6`; no patch
  override and no `rns-net` production dependency.

### Confirmed implementation findings

- `src/server/src/reticulum_live.rs::InterfaceHealth` has only `Connected` and
  `NoInterfaces`; `ReticulumLiveRuntime::interface_health` checks only whether
  configured status handles exist and `needs_runtime_restart` always returns
  false.
- `src/server/src/tui.rs::AdminTui::tick_live_runtime` performs three production
  five-second `std::thread::sleep` calls before recovery, blocking draw, input,
  Stop, quit, and shutdown.
- `src/server/src/reticulum_live.rs::run_live_server_async` drains with
  `try_recv_event` and wakes on a fixed 25 ms sleep even when idle.
- `OmenchatLinkEvent::ResourceReceived` and `ResourceTerminal` discard the
  exact Reticulum Resource hash. `ReticulumOmenchatTransport::offer_resource`
  discards its application `resource_id` before the upstream hash exists.
- An inbound Resource failure currently releases every pending upload offer
  for the identified peer. Pinned 0.9.6 exposes link, hash, received bytes,
  total bytes, and bounded failure reason but no inbound metadata on the
  failure event.
- Both normal headless and TUI paths use Tokio's unbounded host-default worker
  count through bare `Builder::new_multi_thread()`.
- `docs/RELEASE_NOTES_V0_9_6_6.md` still contains the active phrase
  `release-candidate draft`, confirming the need for a publication-finalization
  gate.

### Original commands and results

The following were started against the unmodified baseline:

```text
cargo fmt --check                                                    PASS
cargo fmt --manifest-path src/server/Cargo.toml --check              PASS
git diff --check                                                     PASS
bash scripts/release-check.sh quick                                  RUNNING at capture
```

The quick gate had passed version consistency, exact dependency-train,
accepted-advisory, product-identity, isolated TUI lifecycle, and real Linux PTY
terminal-restoration stages when this initial ledger entry was written. Its
final status and all subsequent focused/full gates are recorded below without
rewriting this original observation.

### Baseline limitations

- No normal browser or server user root is used by any test.
- Native Windows/macOS packaging, hosted CI, external `reticulumd`, physical
  radio, physical ARM board, compositor/GPU, and live public-network evidence
  are not available from this Linux checkout and will not be claimed.
- The locked-0.9.6 maximum UDP Resource reproducer remains an explicit upstream
  boundary unless the exact maintained gate proves otherwise.

## Phase decisions and results

### Phase 1 — typed interface health and restart policy

- Added a typed seven-state aggregate: `NoInterfaces`, `Starting`,
  `Connecting`, `Healthy`, `Reconnecting`, `Degraded`, and `Terminal`.
- Each configured interface contributes its current upstream runtime status and
  its owned task's `AbortHandle::is_finished` result. Configuration presence is
  no longer treated as connectivity.
- Only `Terminal` is restart-eligible. Starting, connecting, reconnecting,
  healthy, degraded, and no-interface states cannot create a competing runtime.
- Focused tests passed:

```text
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full interface_health --lib
  2 passed
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full reconnect_progress --lib
  1 passed
```

### Phase 2 — nonblocking TUI recovery and surface repair

- Removed all three production `std::thread::sleep` recovery backoffs.
- Added one owned, bounded pending recovery with generation, cause, scheduled
  time, due time, and attempt number. Three terminal samples schedule one
  five-second deadline. Duplicate scheduling retains the original deadline;
  Stop, quit, generation change, or observed progress cancels it.
- A failed bounded restart reports the error and requires explicit operator
  action; it does not start an automatic retry chain.
- Draining live runtime events marks the alternate screen for a complete redraw.
  This is the narrow local repair for pinned upstream direct stdout output such
  as `link: close /…/`; normal headless output is unchanged.
- Focused test `pending_live_recovery_is_deadline_driven_deduplicated_and_cancelled_by_stop`
  passed. Source search confirms no `std::thread::sleep` remains in
  `src/server/src/tui.rs`.

### Phase 3 — event-driven headless loop

- Removed the unconditional 25 ms production wakeup. The loop now waits on the
  shutdown signal, prioritized bounded event queues, and the earliest announce,
  handshake-sweep, or statistics deadline.
- One received event is followed by the existing bounded drain. Queue control
  priority and timer fairness remain intact.
- A closed control lane does not terminate or spin while payload remains live;
  both closed lanes return a fatal stopped-queue result promptly.
- Focused tests `headless_loop_has_no_fixed_25ms_idle_poll` and
  `event_wait_prioritizes_control_and_tolerates_one_closed_lane` passed.

### Phase 4 — Resource identity and conservative cleanup

- Exact 32-byte Reticulum hashes cross received and terminal internal events.
  Failure reason is UTF-8 safely bounded to 128 bytes and expected size is
  retained where upstream supplies it.
- Outbound `(link_id, resource_hash) -> resource_id` correlation is bounded to
  256 items globally, 16 per link, 1 MiB of retained IDs, and a six-hour TTL.
  It releases on exact terminal, link close, shutdown, or expiry.
- Pinned `reticulum-rs-transport 0.9.6` supplies inbound failure hash/link and
  byte progress, but no OMENchat Resource metadata. No wire field was invented.
  Upload cleanup uses the approved unique identity-plus-expected-size fallback;
  unmatched and ambiguous failures remove none, while disconnect/link close/TTL
  retain identity-wide cleanup ownership.
- The `stats:` suffix reports outbound exact, unique inbound, unmatched, and
  ambiguous correlation without changing its existing prefix.
- Focused Resource filter: 25 passed, 5 explicit environment/soak tests ignored.

### Phase 5 — bounded runtime and readiness

- Both standalone server paths use one local builder policy: host parallelism
  clamped to `1..=4`, exactly eight maximum blocking threads, and stable
  `omenchatd-headless` / `omenchatd-tui` names.
- The existing readiness line remains byte-for-byte present for package/service
  compatibility. An adjacent machine label now distinguishes `configured`,
  `connecting`, `reconnecting`, `operational`, `degraded`, `terminal`, and
  `no_interface`; periodic statistics repeat the live label.
- Two focused runtime-policy tests passed, including execution on a named worker.

An optional Linux `/proc` measurement helper was added at
`scripts/measure-omenchatd-idle.sh`. A five-second no-interface sample of the
pre-bump `server-headless` release binary on this host reported:

```text
binary_sha256=88555bd112ff241e498f25c84f8c84a336e7b2f2e008f0a1224b012d67c70ca0
sample_seconds=5
cpu_ticks=1 (CLK_TCK=100)
rss_kib=10424
threads=7
file_descriptors=13
readiness=no_interface
```

The isolated server home was removed. This single short sample proves the new
helper and absence of a 25 ms loop in source; it is machine-specific evidence,
not a universal CPU/RSS threshold or before/after hardware comparison.

### Focused implementation gate

```text
cargo fmt --all --check                                      PASS
cargo fmt --manifest-path src/server/Cargo.toml --check      PASS
git diff --check                                             PASS
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full --all-targets -- -D warnings
                                                              PASS
```

No application or server version has been changed at this checkpoint. Full
pre-bump qualification follows before version advancement.

## Phase 6 — integration and regression qualification

The first post-change complete server run exposed one defect in the new
source-regression test itself: its forbidden literal appeared inside its own
assertion, so the test matched itself. Production code was unaffected. The
literal is now assembled from noncontiguous fragments; the complete rerun
passed 566 tests with 12 explicit ignored soak/upstream tests.

Pre-bump results:

```text
cargo test --locked --no-default-features --features desktop-product       PASS
cargo clippy --locked --no-default-features --features desktop-product
  --all-targets -- -D warnings                                             PASS
cargo test --locked --no-default-features
  --features desktop-product-static-media                                 PASS
cargo test --locked --no-default-features --features tui                  PASS
cargo clippy --locked --no-default-features --features tui
  --all-targets -- -D warnings                                             PASS
cargo test --locked --manifest-path src/server/Cargo.toml
  --no-default-features --features server-full                            PASS (566/12 ignored)
cargo clippy --locked --manifest-path src/server/Cargo.toml
  --no-default-features --features server-full --all-targets -- -D warnings
                                                                            PASS
bash scripts/release-check.sh full                                         PASS
```

The full release gate included the clean standalone-server relocation check,
native CLI identities, feature/train/advisory checks, isolated TUI lifecycle,
and Linux real-PTY restoration (`SIGTERMx1=67ms`, `SIGINTx1=73ms`,
`SIGTERMx2=65ms` on this host).

Isolated live/process results:

```text
bash scripts/release-root-sanity.sh ...                                    PASS
bash scripts/release-omenchat-smoke.sh --multi-client --restart-server     PASS
bash scripts/run-omenchat-continuous-reconnect.sh ...                      PASS
bash scripts/run-omenchat-current-upload.sh ...                            PASS
bash scripts/run-nomadnet-current-page.sh ...                              PASS
bash scripts/test-linux-arm64-headless.sh                                  PASS
```

Continuous reconnect retained one process and stable destination, observed old
link close/new link identity and post-restart echo, and recovered negotiated
reaction/revision/pin state. Current upload transferred 873 bytes and completed
sender plus second-client Resource fetch. NomadNet returned 309 bytes/17 Micron
lines over the direct-request primitive. ARM64 Podman/Cross/QEMU passed 60
protocol tests, 440 server tests (12 explicit ignores), release checksum, and
the emulated isolated lifecycle.

The exact ignored maximum UDP Resource test was deliberately run and failed as
expected: pinned 0.9.6 exposes a 456-byte transmit buffer for a 483-byte maximum
Resource packet. This remains an upstream limitation and is not hidden by local
fragmentation, relaxed bounds, a fork, or retry.

## Phase 7 — synchronized version and release material

After all pre-bump gates above passed, root and standalone package manifests and
lock identities advanced together from `0.9.6-6` to `0.9.6-7`. Active version
checks, current-smoke assertions, mixed-current workflow artifact names,
quickstart/backend/protocol references, release notes, and the current checklist
were updated. Historical reports, fixtures, and design checkpoints remain
historical.

The exact post-bump candidate was then qualified rather than inferring success
from the pre-bump code-equivalent runs:

```text
bash scripts/release-check.sh full                                         PASS
bash scripts/run-omenchat-continuous-reconnect.sh ...                      PASS
bash scripts/run-omenchat-current-upload.sh ...                            PASS
bash scripts/run-nomadnet-current-page.sh ...                              PASS
bash scripts/test-linux-arm64-headless.sh                                  PASS
bash scripts/release-package.sh /tmp/omenbrowser-v0967-dist.utLQGw         PASS
bash scripts/release-check.sh package ...                                  PASS
```

The final-version reconnect report identifies both binaries as `0.9.6-7` and
records orderly server stop, stable destination, old/new link evidence,
same-session recovery, post-restart echo, and reaction/revision/pin recovery.
The current upload report records exact 873-byte completion and fetch by both
isolated clients. The NomadNet report records a non-empty network response of
309 bytes/17 Micron lines over `direct-request`. The ARM64 archive is
`omenchatd-0.9.6-7-linux-aarch64.tar.gz`; its protocol tests, headless tests,
checksum, and QEMU lifecycle all passed through Podman/Cross.

The final `0.9.6-7` no-interface release sample reported:

```text
binary_sha256=138b014dcb743e6a8bfdc88ce1281bd9d4966a665554acb2c9d991971ece13cd
sample_seconds=5
cpu_ticks=1 (CLK_TCK=100)
rss_kib=10460
threads=7
file_descriptors=13
readiness=no_interface
```

The local x86_64 candidate archive is
`/tmp/omenbrowser-v0967-dist.utLQGw/OMENbrowser_rs-latest.tar.gz`. Its package
gate verified the checksum, extraction, required files, script syntax, product
feature identity, isolated server init/status/doctor, diagnostics collection,
and two-client packaged OMENchat smoke. This path is disposable local evidence;
no artifact was uploaded or published.

```text
archive_sha256=784ebbba67fc3dab00cd2d8d549f4f6cf34345465984db25a872b3bd63492e8e
cargo fmt --all --check                                                PASS
cargo fmt --manifest-path src/server/Cargo.toml --check                PASS
git diff --check                                                       PASS
bash scripts/verify-release-version.sh                                 PASS
bash scripts/verify-reticulum-train.sh                                 PASS
bash scripts/verify-product-features.sh                                PASS
bash scripts/verify-workflow-security.sh                               PASS
bash scripts/verify-release-finalization.sh                            PASS
cargo clippy --locked --no-default-features --features desktop-product
  --all-targets -- -D warnings                                         PASS
cargo clippy --locked --manifest-path src/server/Cargo.toml
  --no-default-features --features server-full --all-targets
  -- -D warnings                                                       PASS
```

## Unverified external boundaries

- Hosted CI and its pinned/current Python interoperability lanes were not run
  from this checkout.
- Native Windows and Intel/Apple Silicon macOS packaging were not run.
- External `reticulumd`, public-network, physical radio, physical ARM device,
  and compositor/GPU behavior were not exercised.
- The Podman/Cross/QEMU ARM64 gate is accepted as the maintained headless gate;
  it is not represented as physical-device qualification.

`docs/RELEASE_NOTES_V0_9_6_7.md` is explicitly `Status: final`. The package-mode
finalization check rejects draft/candidate notes, root/server version mismatch,
or a current checklist that names another target. No commit, merge, tag,
publication, upload, or remote push is performed by this execution.
