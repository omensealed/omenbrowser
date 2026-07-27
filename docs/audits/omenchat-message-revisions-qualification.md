# OMENchat message-revision qualification

Date: 2026-07-26  
Branch: `release/v0.9.6-4`  
Baseline commit: `b049b9d`

## Scope and verdict

This audit qualifies the deterministic correction/tombstone foundation and its
separately reversible activation of `message-revisions-v1`. Activation changes
only the explicit client request and server acceptance lists; it changes no
protocol number, wire shape, database schema, retention default, worker, queue,
timer, or automatic-retry policy.

Verdict: deterministic and isolated current/current activation gates pass.
Adjacent peers continue ordinary protocol-v1 behavior because they neither
request nor receive the optional revision operations. Native package and
interactive GUI smoke remain release-level evidence rather than blockers for
the operation contract.

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
| Capability absence/rejection | Pass | The client activates only an explicitly requested acceptance, and the server accepts revisions only with an explicit durable request and client instance identifier. Base-only/adjacent peers remain revision-free. |
| Capability loss | Pass | Action-target derivation becomes empty immediately. A matching late acknowledgement cannot resolve the pending durable intent until test-scoped negotiation is restored. Recovered retry remains blocked while the capability is absent. |
| Mixed version | Pass for ordinary protocol-v1 behavior; revision operation not applicable | Adjacent peers cannot negotiate a capability they do not implement. Byte-exact `v0.6.0-1` and `v0.9.6-3` fixtures prove unchanged ordinary traffic. No correction/tombstone is sent to an adjacent peer. |
| Live current/current | Pass | Isolated loopback runs passed lost-ack correction replay, forced-Resource correction/tombstone snapshots, clean intent recovery, two independent client roots, and one continuous client across orderly omenchatd restart with a different Link identifier. |

## Resource and ownership review

This unit adds no queue, cache, worker, timer, retry loop, database table, or
network request. Revision drafts remain limited to one per live session, delete
confirmation remains a single global value, durable intents use the existing
bounded owner, client projection state has item and byte ceilings, and server
state/audit retention has item, byte, age, and bounded-work ceilings.

No CPU or RSS threshold is inferred from these functional smokes. The
continuous report proves one client process, one active logical session, an
observed old-Link close, a different replacement Link, and orderly server
shutdown. Broader resource measurement remains part of release qualification.

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
bash scripts/release-omenchat-smoke.sh --revision-smoke --multi-client ...
bash scripts/run-omenchat-continuous-reconnect.sh \
  --report target/omenchat-continuous-reconnect-revision-report.json

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
- revision-only isolated smoke: passed;
- revision two-client isolated smoke: passed;
- continuous reaction/revision replacement-Link smoke: passed.

## Not executed

- Live adjacent correction/tombstone traffic: an adjacent peer cannot negotiate
  this new optional operation; sending it would violate the compatibility
  design.
- Native Windows/macOS package interaction, Python Reticulum peers, public
  network paths, and physical interfaces: this deterministic operation-layer
  unit does not claim those environments.
- Hosted CI: deferred until this activation is grouped with the next worthwhile
  branch checkpoint.

## Remaining release risks

- Interactive packaged desktop correction/tombstone controls need a display
  smoke on native release artifacts.
- The live harness proves two isolated capable clients in sequence and one
  continuous client across restart; it does not emulate a public-network event
  storm or physical interface.
- Adjacent peers cannot erase a corrected/deleted original because that
  optional capability did not exist in their release. The UI and compatibility
  documentation must retain that limitation.

Rollback removes the capability from the client request list and resets
`MESSAGE_REVISIONS_SERVER_ENABLED` to `false`; the additive stored projection,
audit, and immutable original events remain safe and readable.
