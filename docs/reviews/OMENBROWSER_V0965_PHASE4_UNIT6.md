# OMENbrowser v0.9.6-5 Phase 4 unit 6 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added one project-owned, event-driven propagation/backend evidence summary to
the desktop and TUI diagnostics. It reports only evidence already held by the
application and does not activate LXMF invitations or add network work.

## Design and resource impact

- The latest typed `PropagationStatus` is retained in the existing monitoring
  owner and replaced on each runtime event.
- The existing bounded operation history is summarized by state; no second
  history or queue was added.
- The summary distinguishes queued, in-flight, settled, failed, expired,
  cancelled, and uncertain operations. Uncertainty is orthogonal to lifecycle
  state and never claims peer delivery.
- Managed mode reports application-level TTL/idempotency/correlation while
  preserving the authoritative-delivery requirement.
- External mode reports daemon TTL/idempotency/correlation as unproven and says
  uncertain sends are never automatically retried.
- No worker, subscription, timer, task, channel, disk write, or network request
  was added. Closed-panel idle behavior is unchanged.

## Files changed

- `src/operations/propagation.rs`
- `src/app.rs`
- `src/desktop/views/diagnostics.rs`
- `src/ui/workspace.rs`
- `docs/LXMF_DELIVERY_AND_EVENT_MODEL.md`

## Validation

Passed:

```text
cargo fmt --all --check
cargo test --locked --no-default-features --features desktop-product --lib \
  propagation_backend_panel_is_event_driven_and_external_guarantees_stay_unproven -- --nocapture
cargo test --locked --no-default-features --features desktop-product --lib \
  status_counts_are_domain_scoped_and_keep_uncertainty_orthogonal -- --nocapture
cargo clippy --locked --no-default-features --features desktop-product --lib -- -D warnings
cargo clippy --locked --no-default-features --features tui --lib -- -D warnings
git diff --check
```

The first focused run exposed only an incorrect test expectation for the
existing `8 chars..6 chars` compact-hash presentation. The expectation was
corrected without changing production behavior and both focused tests passed.

Live propagation, external-daemon, Python, packaging, and physical-peer lanes
were not run: this unit changes a read-only projection and does not add a new
runtime action. The complete root matrix had passed immediately before this
unit; the affected desktop and TUI profiles were then compiled under Clippy.

## Remaining Phase 4 boundary

Invitation activation remains blocked on a truthful backend provenance policy
and live/mixed-version evidence. Outbound invitations and asynchronous notices
are not implemented. Manual propagation actions remain the existing bounded
operations; this unit does not add reconnect or unsupported SDK controls.
