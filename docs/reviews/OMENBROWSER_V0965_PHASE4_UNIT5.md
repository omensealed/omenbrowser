# OMENbrowser v0.9.6-5 Phase 4 unit 5 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added a presentation-only desktop boundary for a validated authenticated LXMF
OMENchat invitation. It remains unreachable from production runtime events.

## Behavior

The review card displays bounded, untrusted claims and authenticated sender
evidence. Requested moderator/administrator roles are explicitly labeled as
ungranted claims. Token replay limitations, password requirement, expiry, and
the bounded introduction are visible.

Dismiss is the sole action. There is intentionally no Open, Join, Save, Trust,
Accept role, Retry, or token action.

## Tests

Passed:

```text
cargo test --locked --no-default-features --features desktop-product --lib \
  lxmf_invitation -- --nocapture
```

The complete applicable root matrix also passed after this unit:

```text
cargo fmt --all --check
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings
cargo test --locked --no-default-features --features desktop-product-static-media
cargo test --locked --no-default-features --features tui
cargo clippy --locked --no-default-features --features tui --all-targets -- -D warnings
```

The chained command exited `0`. Environment-bound Python/live peer tests and
packaging were not run because this presentation-only root change neither
activates a network path nor changes packaging.

The regressions prove:

- authenticated input creates one review-only preview;
- sessions and Directory entries are unchanged;
- status text states that opening remains disabled;
- dismissal removes only the preview and opens no connection;
- unauthenticated exact-title input is rejected before desktop state changes.

## Resource and compatibility impact

- One existing `DesktopApp` owner field contains the already bounded preview,
  64-record/64-KiB replay evidence, and counters.
- No subscription, worker, timer, channel, polling, disk write, or network
  traffic was added.
- No wire, schema, dependency, product version, server, TUI, or current
  invitation behavior changed.
- Secret tokens remain redacted and are not rendered; only their unproven
  replay-policy warning is shown.

## Remaining activation boundary

Do not route inbound runtime messages into this owner yet. Managed native mode
has verified sender evidence; external SDK/RPC mode does not yet have equivalent
proof. Activation needs an explicit backend capability/provenance decision,
event-driven admission with no polling, and live/mixed-version tests.
