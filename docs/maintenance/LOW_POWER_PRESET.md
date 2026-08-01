# Low-power desktop preset

OMENbrowser provides two complementary low-resource controls:

- Build `desktop-product-static-media` to omit the animated GIF integration
  from the product graph.
- Enable **Low-power mode** in Settings to apply runtime policy without
  changing Reticulum, LXMF, OMENchat, persistence, or verification semantics.

The setting is persisted as `ui.low_power_mode` and defaults to `false` when
loading an older settings file. It does not overwrite the independent
`ui.reduce_motion` accessibility preference.

## Current policy

When enabled, low-power mode:

- applies the existing reduced-motion boundary, so visible OMENchat media uses
  its static fallback and hidden panes still receive no animation frame handle;
- changes the recurring interface/monitoring sample used only while the
  Interfaces, Monitoring, or Network Doctor section is visible from one second
  to five seconds.

That changes the configured monitoring cadence from at most 60 to at most 12
samples per minute while one of those views is visible, an 80% reduction. The
subscription remains absent outside those views. This is a deterministic policy
comparison, not a claim that total process CPU falls by 80%.

The preset does **not**:

- change network retries, timeouts, delivery interpretation, synchronization,
  cryptography, identity handling, or durable writes;
- enable speculative NomadNet or attachment downloads (those remain disabled);
- raise or lower untrusted-input and abuse-control ceilings;
- create another timer, worker, queue, cache, or task;
- alter the static-media Cargo feature graph at runtime.

The standalone omenchatd server uses its own bounded runtime policy rather than
the desktop preset: available async parallelism is clamped to one through four
workers and the Tokio blocking backstop is capped at eight threads. Its headless
loop is event-driven and wakes for queue work, shutdown, announces, handshake
sweeps, and statistics deadlines; it no longer uses a fixed 25 ms idle poll.
These ownership bounds do not weaken the smaller SQLite, compression, queue, or
Resource admission limits.

## Why there is no central `ResourceBudget`

The current image/media, browser-page, message-history, operation-history,
SQLite, and network queues already have domain-specific item and byte bounds.
Many are security ceilings rather than user-tunable performance policy. A
second structure would duplicate those constants without owning the resources
it purports to budget. Further reductions should be introduced only when a
measurement identifies a particular owner and tests prove its eviction or
backpressure behavior.

The current decode concurrency of two is also retained. The static-media build
does not perform animated GIF frame decoding, while changing shared image or
file concurrency without a workload measurement could increase latency without
reducing settled idle use.

## Validation and measurement

Run the focused policy and persistence tests:

```bash
cargo test --locked --no-default-features --features desktop-product \
  settings_low_power_toggle_persists_without_overwriting_motion_preference --lib
cargo test --locked --no-default-features --features desktop-product \
  low_power_mode_reduces_visible_monitoring_wakeups_without_disabling_samples --lib
cargo test --locked --no-default-features --features desktop-product \
  --test app_settings low_power_preference_round_trips_without_changing_motion_preference
cargo test --locked --no-default-features --features tui \
  settings_low_power_action_routes_shared_persisted_policy --lib
```

Validate the product split with:

```bash
bash scripts/verify-product-features.sh
cargo test --locked --no-default-features \
  --features desktop-product-static-media
```

For native before/after measurements, use the existing Phase 0 resource
harness with the same isolated root and workload. Record settled/peak RSS, CPU,
threads, wakeups, and GPU/frame submissions for all four cases:

1. animated product, preset off;
2. animated product, preset on;
3. static-media product, preset off;
4. static-media product, preset on.

Leave the same monitoring view visible for the wakeup comparison and keep all
network peers, panes, fixtures, and warmup durations identical. A headless test
environment cannot provide trustworthy compositor/GPU or interactive idle
measurements; report those as unavailable rather than inferring them from the
configured interval.

The maintained paired runner creates and verifies the isolated Monitoring
fixture, requires the same binary hash for both cases, and records the case
order:

```bash
OMENBROWSER_BINARY=target/release/omenbrowser_rs \
  WARMUP_SECONDS=60 SAMPLE_SECONDS=600 HEADLESS=1 \
  bash scripts/measure-low-power-desktop.sh /tmp/omen-low-power-animated
```

For order-bias investigation, repeat with a new output path and
`CASE_ORDER=low-power-first`. The configured 60/12 samples-per-minute values
come from the tested subscription policy; `/proc`, `pidstat`, and `perf` provide
the observed process values. Do not describe the scheduler context-switch proxy
as an application-message count.
