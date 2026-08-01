# OMENbrowser v0.9.6-5 Phase 3 unit 1 report

Date: 2026-07-31 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

The review's “first activate replies/mentions” premise is superseded by newer
code. The current production client and server already request, accept,
execute, persist, render, and downgrade `reply-mentions-v1`. This unit made no
behavior, wire, schema, feature, dependency, or limit change.

The original design checkpoint contained both the complete activation record
and a stale pre-activation header. It is now explicitly labeled as a historical
staged record, while `docs/OMENCHAT_PROTOCOL.md` remains the authoritative
current matrix. A stale testing sentence was similarly corrected so a focused
local unread test is no longer mistaken for the product's activation state.

## Inspected production path

- The shared protocol capability requires durable mutations and owns the exact
  bounded rich request/event shapes.
- The canonical client requests the capability only with a persistent client
  instance and the durable base capability.
- The server accepts it only after explicit request and binds the authenticated
  numeric local user ID.
- Rich metadata is covered by the durable mutation identity/hash and cannot be
  silently dropped on retry or downgrade.
- Server persistence, live fan-out, inline history, Resource history, exact
  replay, and restart retain reply and numeric mention metadata.
- Client persistence, history recovery, reply preview/jump, mention rendering,
  retained counts, and mute-except-mentions use the project-owned event model.
- Reconnect without renewed capability leaves uncertain rich work blocked and
  never resends or converts it to plain text.
- No mention polling, presence traffic, notification worker, or unbounded
  secondary mention history exists.

## Focused validation

Passed:

```text
cargo test --locked -p omenchat-protocol reply_mention
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full reply_mention
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-full rich_message
cargo test --locked --no-default-features --features desktop-product \
  --lib rich_message
```

Exact client tests also passed for:

- durable rich send capability and local-echo metadata;
- inline history reply/mention recovery;
- reconnect without capability and no resend;
- retained reply and authoritative mention timeline presentation;
- persisted mute-except-mentions default/restart behavior;
- exact numeric mention-only unread admission.

The Phase 2 live loopback smoke observed `reply-mentions-v1` negotiation before
and after an orderly server restart. It did not submit a rich reply/mention
mutation, so that narrower process-level case remains release evidence rather
than being inferred from negotiation alone.

## Resource and compatibility impact

- Documentation-only change.
- No production allocation, task, timer, queue, cache, or binary-size impact.
- Protocol remains `omenchat-v0.1`; the capability remains additive and
  Link-scoped.
- Legacy/downgraded sessions continue ordinary byte-compatible room messages.

## Remaining limitation and next step

A dedicated isolated process smoke should submit one rich reply/mention and
verify another client's authoritative live event plus inline/Resource history.
That test-support addition is justified before release qualification, but it
must not change the already active production contract or introduce automatic
retry.

Room pins, reactions, and revisions are also already active in the current
checkout and have their own qualification records. Their Phase 3 work should
therefore be inspection and gap closure, not reactivation.
