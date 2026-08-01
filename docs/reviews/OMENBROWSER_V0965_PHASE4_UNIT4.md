# OMENbrowser v0.9.6-5 Phase 4 unit 4 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Defined and tested a pure LXMF invitation-envelope extraction contract. It is
not connected to runtime events or UI state.

## Envelope

- Exact title: `omenchat.lxmf.invite`.
- Content: the existing validated, 4 KiB-capped JSON payload.
- Attachments: prohibited.
- Sender: must carry the managed native decoder's authenticated-source
  evidence and a canonical source destination.

Ordinary LXMF titles are ignored. Exact-title messages without authenticated
source evidence are rejected, not downgraded into an unverified invitation.
Invalid extraction leaves the existing preview untouched.

## Validation

Passed:

```text
cargo test --locked --no-default-features --features desktop-product --lib \
  chat::handoff::tests -- --nocapture
cargo clippy --locked --no-default-features --features desktop-product \
  --lib -- -D warnings
cargo fmt --all
git diff --check
```

Ten handoff tests pass. The extraction regression covers ordinary-message
ignore behavior, unauthenticated rejection, authenticated admission, sender
match evidence, attachment rejection, and prior-preview preservation.

## Compatibility and resource impact

- Uses ordinary LXMF title/content fields and adds no new dependency or binary
  wire envelope.
- No worker, queue, cache beyond the already bounded preview owner, timer,
  subscription, disk write, or network operation.
- No current message is reclassified because extraction has no runtime caller.
- No automatic connect, join, trust, role grant, token use, retry, or media
  download.

## Remaining gate

Managed native mode now has enough source evidence for an opt-in event reducer,
but external SDK/RPC parity is unproven. Before activation, add an explicit
desktop preview owner/action model with no connection side effect, then decide
whether managed-only invitation reception is an acceptable capability or
whether the external backend must first expose equivalent verified provenance.
