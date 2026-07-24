# Operations and Transfers model

`src/operations.rs` is the project-owned vocabulary and bounded in-memory
history intended for both the desktop and terminal frontends. This first Phase
3 unit does not yet connect production events or add a view.

The model deliberately distinguishes:

- queue admission;
- dispatch;
- transport acceptance;
- receipt observation;
- peer delivery;
- authoritative Resource progress;
- cancellation, rejection, expiry, and failure;
- event gaps and reconciliation.

Only `Delivered` claims peer delivery. Queue admission, transport acceptance,
and a receipt remain separate evidence. Byte progress can be constructed only
with a nonzero authoritative total and is currently admitted only for LXMF and
Resource domains.

The production history admits at most 512 records, 512 KiB total, 8 KiB per
record, 16 evidence entries per record, four unique valid actions, and 1 KiB per
retained text field. Updating the same project operation ID replaces its record
instead of appending high-frequency progress. Capacity pressure evicts oldest
terminal history first and never evicts unresolved work; admission fails
explicitly when unresolved records occupy the budget. Completed-history expiry
is incremental and caller-bounded.

No worker, timer, subscription, persistence schema, protocol field, or network
operation is introduced by this unit. GUI/TUI integration must reuse this
module rather than define separate delivery vocabulary.

## OMENchat recovered-mutation adapter

`src/operations/omenchat.rs` projects the existing persistent durable-mutation
recovery records into the shared vocabulary. The full random 128-bit mutation
identity is retained only as the opaque operation key. Retained presentation
text contains the server/room reference and fixed evidence descriptions, not
the message or command body, request hash, correlation identifier,
authenticated identity, or mutation identifier.

The mapping is deliberately conservative:

- `Prepared` is authoritative local persistence in `Waiting`; it does not
  carry dispatch or transport evidence.
- `SentUncertain` is nonterminal `Reconciling` work with uncertain authority;
  it does not imply that transport accepted or the server committed it.
- Reaching the persisted expiry adds authoritative expiration evidence but
  remains nonterminal reconciliation work until the operator explicitly
  finalizes it.
- Explicit send or safe retry is exposed only when the existing production
  retry guard permits it. Reconciliation and redacted diagnostics remain
  available without enabling transmission.
- Terminal intent rows are rejected by this recovery-only adapter rather than
  being reinterpreted.

The current OMENchat recovery card consumes this projection for its state and
transmission-action decision. No automatic resend, persistence transition, or
wire behavior changed. A shared Operations history owner and TUI surface remain
follow-up work.
