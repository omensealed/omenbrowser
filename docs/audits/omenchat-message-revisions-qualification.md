# OMENchat dormant message-revision qualification

Date: 2026-07-26  
Branch: `release/v0.9.6-4`  
Baseline commit: `b049b9d`

## Scope and verdict

This audit qualifies the deterministic correction/tombstone foundation before
production activation of `message-revisions-v1`. It changes no protocol
number, wire shape, database schema, retention default, client request list, or
server acceptance default.

Verdict: the deterministic gates are complete. Activation and release
qualification are not complete. Production peers still cannot negotiate the
capability, so a live current/current correction, tombstone, lost-response
replay, restart, and forced-Resource smoke must follow a separately reversible
activation unit.

## Evidence matrix

| Boundary | Result | Evidence |
| --- | --- | --- |
| Shared wire contract | Pass | 34 shared protocol tests cover the exact request, acknowledgement, event, snapshot, bounds, dependency, and canonical hash contract. |
| Independent codecs | Pass | Root and standalone-server codecs encode and decode the same byte-exact correction fixture. Existing `v0.6.0-1` and `v0.9.6-3` ordinary protocol-v1 fixtures remain unchanged. |
| Client persistence/restart | Pass | The isolated client store and durable-intent tests reopen revision state and uncertain intent without activating or resending it. |
| Server persistence/restart | Pass | A file-backed server test commits once, reopens the database, returns the original replay result without another fan-out, and rejects changed-content identifier reuse. |
| Inline/Resource recovery | Pass | Server snapshot selection and client transport decoding cover both inline and forced-Resource representations with explicit target authority. |
| Fault boundaries | Pass | Schema migration faults, result-encoding failure, client snapshot-capacity failure, compaction faults, and recovery-copy failure paths roll back without partial revision state. |
| Retention | Pass | Bounded room compaction removes both revision state and audit rows with the original target, never resurrects it, and preserves unrelated upload and durable-replay records. |
| Capability absence/rejection | Pass | Production session-open omits `message-revisions-v1`; the server declines an unsolicited request while still accepting the durable base capability. |
| Capability loss | Pass | Action-target derivation becomes empty immediately. A matching late acknowledgement cannot resolve the pending durable intent until test-scoped negotiation is restored. Recovered retry remains blocked while the capability is absent. |
| Mixed version | Pass for ordinary protocol-v1 behavior; revision operation not applicable | Adjacent peers cannot negotiate a capability they do not implement. Byte-exact `v0.6.0-1` and `v0.9.6-3` fixtures prove unchanged ordinary traffic. No correction/tombstone is sent to an adjacent peer. |
| Live current/current | Pending | Requires the activation unit and an isolated two-client/server process smoke over Reticulum. |

## Resource and ownership review

This unit adds no queue, cache, worker, timer, retry loop, database table, or
network request. Revision drafts remain limited to one per live session, delete
confirmation remains a single global value, durable intents use the existing
bounded owner, client projection state has item and byte ceilings, and server
state/audit retention has item, byte, age, and bounded-work ceilings.

No CPU, RSS, or link-count measurement was collected because the capability
remains dormant and this patch adds only assertions and documentation.
Activation smoke must record process/link stability; it must not infer resource
behavior from unit tests.

## Commands and results

All commands used repository-owned feature profiles and isolated test roots:

```bash
cargo test --locked -p omenchat-protocol
cargo test --locked --no-default-features --features desktop-product \
  message_revision --lib
cargo test --locked --no-default-features --features desktop-product \
  durable_message_revision_ack_must_match_exact_request_and_local_identity --lib
cargo test --locked --no-default-features --features desktop-product \
  revision_controls_require_authority_preserve_drafts_and_confirm_deletion --lib

(
  cd src/server
  cargo test --locked --no-default-features --features server-headless \
    message_revision --lib
  cargo test --locked --no-default-features --features server-full \
    message_revision -- --nocapture
  cargo test --locked --no-default-features --features server-full \
    compaction_removes_target_projections_and_clears_only_surviving_reply_reference --lib
  cargo test --locked --no-default-features --features server-full \
    every_compaction_fault_boundary_rolls_back_all_dependencies_and_ledger --lib
  cargo test --locked --no-default-features --features server-full \
    database_recovery::tests --lib
)
```

Results:

- shared protocol: 34 passed;
- root revision filter: 12 passed;
- standalone server headless revision filter: 18 passed;
- standalone server full revision filter: 18 passed;
- retention cleanup: 1 passed;
- retention fault injection: 1 passed;
- database recovery: 9 passed;
- focused capability-loss controls and acknowledgement tests: 2 passed.

## Not executed

- Live current/current revision traffic: production negotiation is deliberately
  dormant.
- Live adjacent correction/tombstone traffic: an adjacent peer cannot negotiate
  this new optional operation; sending it would violate the compatibility
  design.
- Native Windows/macOS package interaction, Python Reticulum peers, public
  network paths, and physical interfaces: this deterministic operation-layer
  unit does not claim those environments.
- Hosted CI: no hosted run is justified for two local assertions while the
  capability remains disabled.

## Remaining activation risks

- A live Link may close after server commit but before acknowledgement; the
  current deterministic replay proof must be repeated across a real replacement
  Link.
- Two independent capable clients must agree on correction and tombstone fan-out
  before and after an orderly server restart.
- Forced Resource snapshots must restore explicit authority in a live client.
- Activation must request and accept the capability only with
  `durable-mutations-v1`, and downgrade/capability-loss must leave uncertain
  intent user-controlled rather than automatically resent.

The next smallest justified step is one reversible activation commit plus an
isolated current/current smoke extension. If that smoke fails, revert
negotiation while retaining the dormant wire, storage, and presentation
foundation.
