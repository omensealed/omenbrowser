# OMENbrowser v0.9.6-5 Phase 4 unit 18 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Hardened the diagnostics-only invitation capability lane with deterministic
pre-cancellation and a strict report validator. Live peer support remains
unproven and invitation sending remains disabled.

## Design and resource impact

- `--lxmf-invitation-capability-cancel-after-ms <ms>` is valid only with the
  explicit probe and rejects values above its existing 15-second total budget.
- A zero delay wins before the probe future is polled, cancels the owned token,
  and awaits cleanup. It does not detach work, retry, or convert cancellation
  into support.
- Reports record only whether cancellation was requested and its bounded delay.
- The standalone validator allowlists every JSON key and checks redaction,
  shutdown, fixed deadline, zero retries, and zero invitation sends.
- The live harness runs cancellation first, then current support and optional
  prior-version absence. Evidence is still held only in the explicit isolated
  harness root.

No dependency, product version, feature, configuration, database, protocol,
identity, persistent application state, UI, or invitation-send behavior changed.
The only added timer is owned by one explicit CLI invocation and bounded by the
probe deadline.

## Files changed

- `src/main.rs`
- `src/cli_help.rs`
- `scripts/run-lxmf-invitation-capability-live.sh`
- `scripts/validate-lxmf-invitation-capability-report.py`
- `docs/TESTING.md`
- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT18.md`

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product \
  cli_parses_redacted_lxmf_invitation_capability_probe --bin omenbrowser_rs
cargo test --locked --no-default-features --features desktop-product \
  cli_help --lib
cargo test --locked --no-default-features --features desktop-product \
  invitation_capability_probe --lib
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
cargo fmt --all --check
bash -n scripts/run-lxmf-invitation-capability-live.sh
python3 syntax compilation of the report validator
validator positive cancelled-report check
validator negative unreviewed-field check
```

The focused parser gate covers zero-delay parsing, above-deadline rejection,
and rejection when the cancellation flag has no probe. Four probe-adapter tests
passed; two help tests passed. Clippy passed for every desktop-product target.

Not run: the two-process live harness, because no explicit isolated identities,
test TCP gateway, or reviewed prior binary were supplied. Consequently there is
no claim of current/current support, downgrade behavior, or mid-flight network
cancellation. Native Windows/macOS, packaging, Python interoperability, and
hardware lanes were unaffected and were not triggered for this local diagnostic
hardening slice.

## Compatibility, rollback, and next gate

Existing probe invocations are unchanged; the new switch is opt-in. Roll back
by removing the switch/report fields, validator, harness cancellation case, and
matching documentation. No data migration or cleanup is needed.

The next release-relevant gate remains maintainer execution of the documented
live harness. Until it passes, do not expose or send LXMF OMENchat invitations.
