# Phase 6 unit 2 — user-selectable low-power runtime policy

Status: **implemented; software-rendered resource evidence recorded, native GPU pending**.

## Current-code decision

The repository already had the important low-resource foundations: a tested
`desktop-product-static-media` release identity, a persisted reduced-motion
preference, no speculative attachment or NomadNet prefetch, visibility-gated
animation, and bounded media/cache/task owners. The only general recurring
desktop sample relevant to this unit was the one-second monitoring subscription,
and it was already inactive outside Interfaces, Monitoring, and Network Doctor.

This unit therefore adds a small persisted policy instead of duplicating all
resource constants or introducing a central budget abstraction. No upstream,
wire, database, dependency, or product-version change is included.

## Changes

- Added a default-off `ui.low_power_mode` setting with compatible loading of
  older settings files.
- Added GUI and TUI controls using the same application-owned toggle.
- Made low-power mode imply effective reduced motion without overwriting the
  user's explicit reduced-motion preference.
- Slowed only the visible diagnostics/interface sample from one second to five
  seconds under the preset.
- Added focused persistence, routing, TUI, animation-policy, and subscription
  tests.
- Documented the exact policy, exclusions, rollback, and native measurement
  procedure in `docs/maintenance/LOW_POWER_PRESET.md`.

## Files changed

- `src/storage/settings.rs`
- `src/app.rs`
- `src/desktop/message.rs`
- `src/desktop/theme.rs`
- `src/desktop/subscriptions.rs`
- `src/desktop/update.rs`
- `src/desktop/views/settings.rs`
- `src/desktop/views/workspace.rs`
- `src/ui/mod.rs`
- `src/ui/workspace.rs`
- `tests/app_settings.rs`
- `README.md`
- `docs/TESTING.md`
- `docs/maintenance/LOW_POWER_PRESET.md`
- this report

## Compatibility, storage, and protocol impact

Older settings files deserialize `low_power_mode` as `false`. Enabling it adds
one ordinary JSON boolean. Disabling the preset restores the normal monitoring
cadence while leaving `reduce_motion` exactly as the user set it. There is no
schema-number migration, Reticulum/LXMF/OMENchat wire change, identity change,
database change, or mixed-version requirement.

## Resource impact

The configured visible monitoring cadence changes from at most 60 to at most
12 samples per minute (80% fewer). No subscription is added, and the existing
section visibility gate remains. Animated frame handles are withheld through
the already-tested reduced-motion boundary.

The original implementation unit did not claim new resource results. Phase 6
unit 3 subsequently records a current same-host software-rendered comparison
and current binary sizes. Native compositor/GPU evidence remains pending.

No cache, queue, retry, deadline, cryptographic, persistence, or security limit
was weakened. No new dependency, worker, timer, channel, or task was added.

## Validation

Passed locally on Linux x86_64:

```text
cargo fmt --all --check
cargo test --locked --no-default-features --features desktop-product \
  settings_low_power_toggle_persists_without_overwriting_motion_preference --lib
cargo test --locked --no-default-features --features desktop-product \
  low_power_mode_reduces_visible_monitoring_wakeups_without_disabling_samples --lib
cargo test --locked --no-default-features --features desktop-product \
  --test app_settings low_power_preference_round_trips_without_changing_motion_preference
cargo test --locked --no-default-features --features desktop-product-static-media \
  low_power_mode_reduces_visible_monitoring_wakeups_without_disabling_samples --lib
cargo test --locked --no-default-features --features tui \
  settings_low_power_action_routes_shared_persisted_policy --lib

cargo test --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product-static-media
cargo test --locked --no-default-features --features tui

cargo clippy --locked --no-default-features \
  --features desktop-product --all-targets -- -D warnings
cargo clippy --locked --no-default-features \
  --features desktop-product-static-media --all-targets -- -D warnings
cargo clippy --locked --no-default-features \
  --features tui --all-targets -- -D warnings

bash scripts/verify-product-features.sh
bash scripts/release-check.sh quick
git diff --check
```

The full animated, static-media, and TUI suites passed with their documented
measurement/hardware ignores only. The quick release gate also passed its
version/dependency/advisory checks, native CLI identities, TUI lifecycle and
real-PTY smoke, focused browser/OMENchat coverage, standalone-server relocation,
server feature checks, and focused `omenchatd` tests.

No remote workflow was dispatched. Python/live Reticulum interoperability,
Windows/macOS native execution, package generation, compositor/GPU capture,
and physical hardware were not run because this local settings/subscription
unit does not change their behavior. Phase 6 unit 3 records the separate
software-rendered resource evidence.

## Limitations and rollback

The runtime preset cannot remove `iced_gif` from an already-built binary; use
the static-media product for that graph-level saving. It does not yet tune page
or history cache size because the current limits are owner-specific and no
same-workload measurement justifies another policy. Native compositor/GPU and
settled CPU/RSS evidence is pending because this environment does not provide a
controlled interactive desktop workload.

Rollback removes the setting field, the two shared UI actions, the effective
motion predicate, the interval selection, their tests, and these docs. Existing
settings readers ignore the removed JSON field, and no user data requires
migration.

## Next smallest justified step

Complete the static-media/TUI/Clippy and quick-release gates, then move to Phase
6.3 evidence closure. Do not centralize further budgets until native evidence
identifies a concrete owner whose bounded policy should change.
