# OMENbrowser v0.9.6-5 Phase 4 unit 8 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Added a reproducible deterministic sender-to-preview invitation evidence lane.
It uses the production native LXMF signing, verification, decoding, runtime
event, bounded admission, and application ownership paths.

## Evidence and limits

The fixture proves:

- a real bounded invitation payload can be encoded and signed;
- source destination derivation and signature verification produce the exact
  per-message authenticated-source marker;
- the normal bounded runtime event queue reaches the application reducer;
- the invitation becomes one authenticated-match review preview;
- the control JSON is not persisted as ordinary message history;
- no OMENchat connection action occurs;
- forged/mismatched or unauthenticated input is rejected;
- Dismiss remains non-mutating.

It does not claim a live Reticulum interface, real Link transfer, external RPC
provenance, prior-binary presentation, or peer support for outbound invitations.
Outbound remains disabled because there is no negotiated application-payload
support signal and an older client may render the JSON as ordinary content.

## Files changed

- `src/runtime/native_lxmf/codec.rs`
- `scripts/test-lxmf-invitation-evidence.sh`
- `docs/TESTING.md`
- `docs/design/OMENCHAT_INVITATIONS_CHECKPOINT.md`

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product --lib \
  signed_native_invitation_wire_enters_preview_without_history_or_action -- --nocapture
bash scripts/test-lxmf-invitation-evidence.sh
bash -n scripts/test-lxmf-invitation-evidence.sh
cargo fmt --all --check
cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings
git diff --check
```

The script runs four focused filters and exited `0`. All filesystem state uses
an explicit temporary application root. No identity, message, cache, or server
data under the maintainer's real roots is accessed.

The complete desktop-product suite and desktop/TUI Clippy gates passed in the
immediately preceding activation unit. This added test-only/script unit changes
no production path.

## Resource and compatibility impact

Production resource use is unchanged. The fixture creates one temporary root
and cleans it through its test owner. No dependency, feature, wire, schema,
server, packaging, or version change was made.

## Next gate

Add an opt-in two-process native transport lane only after the CLI can send the
bounded invitation kind explicitly and can record a receiver-side preview
without exposing the body. Prior-binary behavior must be classified before any
outbound UI action is enabled.
