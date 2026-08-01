# OMENbrowser v0.9.6-5 Phase 4 unit 9 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added an opt-in two-process managed-native transport harness for the already
hardened LXMF OMENchat invitation preview. This is a diagnostic evidence lane,
not a product invitation action.

## Design and compatibility

- The receiver announces and reports its local native LXMF destination, then
  waits on the existing bounded runtime event stream.
- The sender uses the existing bounded announce/path readiness collector and
  submits exactly one direct, tokenless, expiring invitation only when ready.
- A failed or uncertain send is never retried automatically and transport
  acceptance is explicitly not called peer delivery.
- Receiver work is bounded to 256 examined events and a caller-selected 1--300
  second deadline.
- The production reducer creates the same authenticated, Dismiss-only preview.
  The report inspects the isolated message store to prove the control message
  was not persisted and records that this command invoked no connection action.
- The shell harness requires two distinct explicit roots and pre-existing test
  identities. It rejects home/root paths and does not create or inspect the
  maintainer's normal identity or application data.
- Evidence JSON contains public destination hashes and state only. It omits the
  invitation content and token; an optional identity passphrase is read through
  the existing protected-file option rather than command arguments.
- No dependency, feature, wire format, schema, server behavior, product UI,
  package metadata, or application version changed.

## Files changed

- `src/app.rs`
- `src/main.rs`
- `src/cli_help.rs`
- `scripts/run-lxmf-invitation-live.sh`
- `docs/TESTING.md`
- `docs/design/OMENCHAT_INVITATIONS_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT9.md`

## Validation

Passed during implementation:

```text
bash -n scripts/run-lxmf-invitation-live.sh
bash scripts/test-lxmf-invitation-evidence.sh
cargo test --locked --no-default-features --features desktop-product --lib \
  live_invitation_report_rejects_bounds_before_runtime_work
cargo test --locked --no-default-features --features desktop-product \
  --bin omenbrowser_rs cli_parses_bounded_lxmf_invitation_sender_and_receiver_modes
cargo test --locked --no-default-features --features desktop-product \
  cli_help::tests
cargo fmt --all --check
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo clippy --locked --no-default-features --features tui \
  --all-targets -- -D warnings
git diff --check
```

The TUI all-target gate exposed two invitation-only application tests and one
report-path helper that lacked their `chat-client` compile guards. The guards
were restored without changing behavior; both TUI and desktop product graphs
then passed with warnings denied.

The live harness was not run: this environment was not supplied two disposable
native LXMF identity files and a controlled TCP Reticulum gateway. Therefore no
live peer-delivery or interoperation result is claimed.

## Resource impact

Normal product behavior is unchanged. The diagnostic command owns one existing
broadcast receiver, one readiness deadline, and at most one send. It adds no
recurring timer, polling worker, cache, queue, detached task, automatic retry,
attachment, or download. The shell owner terminates its receiver on exit.

## Rollback and next gate

Remove the diagnostic command and shell harness; no persistent migration or
cleanup is required. The next gate is a recorded controlled execution followed
by prior-binary behavior classification. Open, Join, Save, token consumption,
and outbound invitation UI actions remain disabled.
