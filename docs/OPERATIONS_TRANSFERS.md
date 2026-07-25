# Operations and Transfers model

`src/operations.rs` is the project-owned vocabulary and bounded in-memory
history intended for both the desktop and terminal frontends. OMENchat
restart-recovery is the first production adapter, and Network Doctor now
contains the first compact read-only desktop surface.

The model deliberately distinguishes:

- queue admission;
- dispatch;
- transport acceptance;
- receipt observation;
- peer delivery;
- locally completed non-message work;
- Resource offer and completion;
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
wire behavior changed.

## Shared owner

`App::operation_history` is the one bounded owner shared by frontend state. An
OMENchat restart-recovery completion atomically replaces only the
`OmenChatMutation` domain snapshot. Records from other domains remain intact,
duplicate IDs or mixed-domain snapshots are rejected, and capacity failure
leaves the previous snapshot untouched. Resolution, acknowledgement, or a
terminal server response removes the exact opaque mutation operation and
releases its retained byte budget.

The persistent mutation-intent database remains authoritative. Failure to
project its snapshot never removes or transitions an intent and is reported in
the recovery status/log. The owner initially records conservative recovery
actions with network transmission disabled; the existing recovery card still
evaluates the live negotiated-session guard when rendering explicit Send/Retry.
This avoids stale permission or connection claims in the shared snapshot.

Owner synchronization happens only at recovery and persisted transition
boundaries. There is no polling, redraw-time mutation, worker, timer,
subscription, or second persistence layer. A fuller desktop workspace, the TUI
surface, and additional runtime-domain adapters remain follow-up work.

## Shared presentation rows

`src/operations/presentation.rs` is the frontend-neutral read-only projection
used by future desktop and terminal surfaces. It supplies the same domain,
state, evidence-authority, evidence-kind, filter, and action labels to both
frontends. In particular, `transport accepted`, `receipt observed`, and
`delivered` remain distinct terms.

Each projection returns at most 128 rows (64 by default), reports the total
matching and omitted counts, sorts attention work first with deterministic
tie-breaking, and supports all/active/attention/completed/domain filters.
Optional search is limited to 128 bytes and matches only public target and
presentation/evidence text. Opaque operation IDs remain selection keys and are
never included in searchable or display text.

Row targets are control-character sanitized and capped at 160 UTF-8 bytes.
Evidence summaries retain only the latest bounded evidence description and are
capped at 256 UTF-8 bytes. Authoritative byte progress and valid actions remain
typed rather than converted to guessed percentages or clickable strings. The
projection clones at most the selected bounded rows and creates no retained
cache, worker, timer, subscription, or redraw trigger.

## Desktop Network Doctor panel

Network Doctor contains a passive `Operations & Transfers` card backed
exclusively by the shared presentation projection. It requests at most eight
attention-first rows, reports the retained and omitted counts, and shows the
history's retained-byte total. Empty history is explicit.

Each row uses shared domain, state, and authority labels plus the latest bounded
evidence summary. Opaque operation IDs remain selection-only and are not
rendered. Byte progress is shown only when the projection carries a typed total
and the evidence authority is `Authoritative`; the panel never derives or
animates a percentage. Its reminder explicitly states that transport acceptance
and receipt evidence are not peer delivery.

The card adds no controls, routing state, workspace preference, cache, worker,
timer, subscription, persistence, protocol field, or dependency. It is placed
inside the existing passive Network Doctor surface so this small slice does not
change existing saved-section compatibility.

## Terminal Network Doctor view

The existing TUI `NetworkDoctor` route now renders the same shared projection
instead of its former placeholder. It also requests at most eight
attention-first rows, reports retained and omitted counts plus exact retained
bytes, and has an explicit empty state. Attention rows receive presentation
emphasis, but their state and authority still come directly from the shared
typed labels.

The TUI renders exact `completed/total` bytes only for authoritative progress
and does not calculate a percentage. Opaque IDs are not rendered. The view has
no input action or mouse target, and the existing Network Doctor route and
saved preference remain unchanged. It adds no update loop, worker, timer,
subscription, persistence, protocol field, or dependency.

The eventual interactive desktop/TUI views must continue to consume the same
projection rather than define frontend-specific delivery vocabulary.

## Reticulum Resource lifecycle adapter

`src/operations/resource.rs` observes the existing typed
`ResourceProgress` and `ResourceLifecycle` runtime events at the application
event boundary. It changes no Resource transport, browser correlation,
Network Doctor tracking, retry, cancellation, or shutdown behavior.

The adapter derives a stable opaque 128-bit project key from the bounded
transfer identifier. The original transfer identifier and browser operation
correlation are not retained in the shared presentation record. Public target
text is assembled only from available source, purpose, direction, and peer
metadata and remains subject to the shared record and presentation bounds.

Mapping is deliberately conservative:

- an offer is authoritative `Waiting` work with `resource offer` evidence;
- progress is authoritative `Transferring` work, with exact byte progress only
  when a nonzero valid total exists;
- a previously observed authoritative total may be reused when a later update
  omits it, but byte regressions and malformed totals are not applied;
- Resource completion is terminal `Completed` with `resource completion`
  evidence, never `Delivered` or peer-delivery evidence;
- failure and cancellation remain distinct terminal outcomes and preserve the
  last valid progress when available;
- progress or offers after a terminal outcome are ignored;
- repeated progress replaces the prior progress evidence instead of growing
  history.

Admission failure leaves the prior authoritative record and all other
unresolved Operations intact and emits a bounded warning through the existing
application log. No worker, timer, subscription, queue, cache, persistence,
protocol field, action, or dependency is added.
