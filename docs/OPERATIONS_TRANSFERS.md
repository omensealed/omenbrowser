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
