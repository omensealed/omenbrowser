# OMENbrowser v0.9.6-5 Phase 4 unit 11 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

Completed the required checkpoint before adding an invitation capability wire
contract. No protocol or runtime implementation was made.

## Findings and decision

Locked LXMF SDK capability negotiation describes the selected local backend,
not an authoritative remote contact application. SDK identity/discovery fields
and Reticulum announce storage do not currently provide a tested fresh binding
from an application capability to the exact active managed-native LXMF peer.
Extending the Python-compatible LXMF delivery announce is therefore rejected.

The proposed fail-closed solution is a dedicated same-identity Reticulum
capability endpoint with a bounded nonce challenge/response. Only a fresh,
authenticated exact `omenchat-lxmf-invitations-v1` response could permit one
user-confirmed send. Prior, absent, stale, malformed, external-RPC, or
conflicting evidence disables sending. Capability state is ephemeral and
bounded; no schema or configuration migration is proposed.

## Files changed

- `docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`
- `docs/design/OMENCHAT_INVITATIONS_CHECKPOINT.md`
- `docs/reviews/OMENBROWSER_V0965_PHASE4_UNIT11.md`

## Compatibility, storage, and resource impact

None: this unit changes documentation only. Product invitation sending remains
disabled. No dependency, feature, wire, schema, identity, storage, runtime,
server, package, or version changed. The proposed design defines strict future
deadlines, concurrency, cache, byte, cooldown, expiry, cancellation, and
shutdown ownership.

## Validation

```text
cargo fmt --all --check
git diff --check
```

No live, mixed-version, package, external-RPC, Python, or hardware test applies
to this documentation-only checkpoint. The prior-release source classification
and current deterministic receiver evidence passed in units 8--10.

## Rollback and next gate

Remove the checkpoint documents; no cleanup is required. A wire implementation
must not begin until the five decisions in the checkpoint are accepted. If
accepted, the next smallest unit is a pure bounded request/response codec and
state model with no registered destination, network action, or UI.
