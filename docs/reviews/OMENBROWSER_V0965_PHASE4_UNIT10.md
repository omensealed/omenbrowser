# OMENbrowser v0.9.6-5 Phase 4 unit 10 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Classified the exact prior-release behavior for LXMF OMENchat invitations.
`v0.9.6-5` treats `omenchat.lxmf.invite` as an ordinary persisted message, so a
current sender must not transmit the control payload without negotiated peer
support.

## Evidence

The deterministic gate pins both tag and commit, then requires:

- the invitation payload declaration exists only as the dormant handoff type;
- no production caller references its type or control title;
- the inbound runtime reducer sends every message to ordinary persistence;
- no invitation-specific branch exists in that reducer;
- the authenticated native-LXMF source marker does not exist.

This is stronger than inferring behavior from a version number, but it remains
source-level evidence. It is not a live prior/current transport or packaged
binary interaction test.

## Files changed

- `scripts/test-lxmf-invitation-prior-version.sh`
- `docs/TESTING.md`
- `docs/design/OMENCHAT_INVITATIONS_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT10.md`

## Compatibility and resource impact

The downgrade outcome is explicitly incompatible for outbound invitation
control messages. Product sending remains disabled; current reception remains
authenticated, bounded, and Dismiss-only. No runtime code, dependency, feature,
wire format, schema, task, timer, queue, cache, storage policy, server behavior,
package metadata, or version changed.

## Validation

```text
bash -n scripts/test-lxmf-invitation-prior-version.sh
bash scripts/test-lxmf-invitation-prior-version.sh
cargo fmt --all --check
git diff --check
```

## Rollback and next gate

Remove the source-classification script and documentation. No data cleanup is
required. The next compatibility prerequisite for outbound product invitations
is an explicit peer capability with a live current/current pass; absence of the
capability must continue to disable sending. The controlled live receiver lane
and external-RPC provenance also remain unexecuted/unproven.
