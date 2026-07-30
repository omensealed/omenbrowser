# OMENchat slow-mode draft recovery qualification

Date: 2026-07-28

Status: local typed rejection and durable-intent recovery complete; product
slow-mode activation remains off.

## Problem and invariant

A server can commit one room message and then reject the next durable mutation
with typed `SlowModeActive` (1017). The client previously removed the composer
text when the second request entered transport, retained its local echo, and
left its durable intent `SentUncertain`. That could lose the editable draft and
later show a false uncertain-mutation recovery warning even though the server
had definitively rejected the attempt.

The client now treats 1017 as a typed, retryable rejection, not delivery and not
an uncertain transport outcome. It removes the correlated local echo, emits
`DurableMutationRejected`, and does not resend automatically.

## Persistence and race policy

The existing bounded mutation-intent worker owns a new state-checked removal
operation. An immediate SQLite transaction removes and returns the original
intent only while it is still `SentUncertain`.

- An acknowledgement or existing terminal state wins the race.
- No database schema or stored-state value was added.
- A failed removal leaves the intent recoverable and does not restore text that
  could then be sent twice.
- The worker remains bounded at 32 commands and 2 MiB, and keeps its existing
  shutdown/join path.

After successful removal, a room-message or `/me` action body is restored only
when the same room is active and the composer is empty. Newer composer text is
never overwritten. Rich reply/mention wire bodies restore their visible text;
reply and mention selections are deliberately not reconstructed because their
referenced room state may have changed.

## Compatibility and resource impact

There is no wire, capability, database-schema, identity, configuration, or
server change. Older stored intent rows remain readable. This adds no worker,
timer, retry loop, queue, cache, or recurring wakeup. The only added database
work occurs after a correlated typed rejection and uses the existing bounded
storage worker.

Slow mode remains qualification-only and absent from canonical product
capability advertisement. Rollback is a source revert; no data migration is
required.

## Validation

Focused local tests cover:

- typed slow-mode rejection and removal of only the correlated local echo;
- preservation of the generic user-visible error alongside the typed event;
- atomic removal of only `SentUncertain` intents;
- worker ownership and restart recovery with no remaining rejected row;
- exact plain and rich draft restoration;
- refusal to overwrite newer text or restore into another room.

Commands:

```text
cargo fmt --all --check
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product rejected
cargo test --locked --no-default-features --features desktop-product slow_mode_rejection
```

The canonical test, Clippy, release-check, and standalone-server results for
this unit are recorded in the implementing commit report. No hosted CI, Python
interop, package build, public-network peer, or physical-interface result is
claimed.

## Remaining activation gates

- Observe draft recovery in the real Iced GUI.
- Prove adjacent released-binary four-/five-field compatibility.
- Record real client/server resource measurements.
- Make an explicit capability activation and rollback decision.
