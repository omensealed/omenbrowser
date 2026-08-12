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

## OMENchat connection-state adapter

`src/operations/connection.rs` projects the existing typed
`ChatConnectionState` reducer into one shared record per OMENchat session. This
is the complete project-owned connection lifecycle boundary used by the
desktop: the runtime bus currently broadcasts link closure but does not
broadcast a matching typed link-open or general Reticulum link-state event.
The adapter therefore does not parse link logs or claim transport evidence
that the shared event surface does not provide.

The mapping is deliberately conservative:

- disconnected and resolving sessions remain unresolved `Waiting`;
- connecting, authenticating, joined, and draining sessions are `Active`;
- reconnecting sessions are `Reconciling`;
- typed failures are `Failed`, with retry availability retained in fixed
  bounded evidence text;
- joined means the OMENchat session is active, never that a message was
  delivered;
- repeated transitions coalesce by numeric session ID and stale observations
  are ignored;
- closing the session removes its connection record and releases its exact
  retained-byte budget;
- history saturation preserves existing unresolved work and rejects the new
  projection.

The normalized server destination is the public target. Link identifiers,
authentication material, frame contents, error strings, and correlation data
are not retained. The adapter adds no transport event, retry, worker, timer,
subscription, queue, persistence, protocol field, or dependency. Existing
OMENchat reconnect controls and retry policy remain authoritative.

## Typed LXMF SDK delivery adapter

`src/operations/lxmf.rs` projects the existing
`RuntimeBusEvent::SdkDeliveryUpdated` surface from the locked lxmf-sdk 0.9.8
train. The update already carries a typed delivery state, terminal flag,
attempt count, timestamp, sequence number, message identifier, and optional
peer. This unit deliberately does not parse human-readable delivery strings.

The mapping preserves delivery boundaries:

- queued is `Queued` with queue-admission evidence;
- dispatching and in-flight remain `Dispatching`;
- nonterminal sent is `TransportAccepted`, not delivered;
- terminal sent is local `Completed` when the backend lacks receipt
  terminality, never delivered;
- only typed delivered becomes `Delivered` with authoritative peer-delivery
  evidence;
- failed, cancelled, expired, and rejected remain distinct terminal outcomes;
- unknown becomes uncertain `Reconciling`;
- inconsistent state/terminal combinations are rejected.

The message identifier is validated and converted to an opaque 128-bit
operation key; it is never rendered. A known peer is retained as the bounded
public target and survives a later update that omits peer metadata. Attempts
and numeric event sequence are retained exactly. Transitions coalesce, bounded
evidence keeps only the latest 16 entries, duplicate/stale updates are ignored,
and a terminal record cannot regress to later nonterminal state. Reason codes
are retained only when control-free and within 512 bytes; otherwise a fixed
omission notice is stored.

The typed native `LxmfDeliveryEvidence` surface is reconciled only when it
carries an exact message ID. Packet submission and propagation-node acceptance
are transport acceptance, RNS packet proof is receipt evidence with peer
delivery explicitly unconfirmed, and only the LXMF router-delivered event is
peer delivery. Router/propagation failure remains failure. Peer activity,
no-receipt observation, and propagation sync without payload remain inferred
or uncertain reconciliation rather than delivery.

Native evidence uses its typed observation timestamp or the application event
time when absent. Stale evidence and non-delivery updates after a terminal
record are ignored; a later authoritative router-delivered event may resolve a
prior failure. The raw detail and RTT fields are not retained because detail
may embed packet, link, resource, node, and failure data. Evidence without an
exact message ID is omitted rather than correlated by peer.

The coarser legacy `MessageDeliveryUpdated` surface is also correlated only by
exact message ID. Submitted-to-runtime is queue admission,
submitted-to-Reticulum is transport acceptance, and unknown status remains
uncertain. Explicit delivered/failed states must agree with their legacy
boolean flags. For compatibility with older serialized events where the state
field was absent, default-unknown plus exactly one delivered/failed boolean is
accepted. Mutually true flags and every other contradiction are rejected.

Legacy status uses application observation time, preserves stronger receipt
evidence, cannot replace delivery with a later failure, and may resolve a prior
failure with a later consistent delivery. Its raw evidence and RTT are not
retained or parsed. No send, retry, cancellation, worker, timer, subscription,
queue, persistence, protocol field, or dependency is added.

## Runtime event-stream recovery adapter

`src/operations/event_stream.rs` projects the existing typed `StreamGap` and
`StreamRecovered` events into one bounded record for each integrated-broadcast
or SDK/RPC source. A gap is authoritative `EventGap` evidence and carries only
the typed source, reason category, dropped count, and numeric recovery cursor.
It never implies that a message, path, or transfer failed.

A recovery event updates an existing gap record only. Recovery without a
corresponding retained gap is ignored. An error-free recovery is locally
`Completed`; recovery that reports a snapshot error or a missing typed
snapshot-success flag remains uncertain `Reconciling`. A later gap can reopen a
completed source record.
Stale cursors and duplicate gaps/recoveries are ignored, evidence remains
limited to 16 entries, and the attempt count records bounded gap cycles.

Upstream cursor strings and recovery error strings are deliberately omitted
from the shared presentation record because they can contain backend or
deployment detail. The existing Network Doctor log remains the detailed
diagnostic surface. This adapter observes the current owned recovery worker; it
adds no recovery attempt, snapshot request, retry, action, worker, timer,
subscription, queue, persistence, protocol field, or dependency.

## LXMF propagation-sync adapter

`src/operations/propagation.rs` uses the application's existing single pending
sync generation as the stable operation identity. This ownership is required
because the runtime `PropagationSync` event has no operation identifier and is
also emitted for outbound propagation acceptance outside a user-requested
sync. Events received without the current app generation are ignored.

The application records queue admission before spawning the existing sync
task. Typed started, progress, intermediate-complete, blocked, failed, and
final-complete stages update that exact operation. Intermediate stage
completion remains active; only the typed final-complete stage or the existing
task report locally completes the sync. Completion never claims peer message
delivery. The task report is authoritative for the final success/blocker
outcome and retains only bounded message and delivery-update counts.

`Complete/Progress` is deliberately ignored because current producers use that
ambiguous shape for outbound acceptance and link cleanup without operation
identity. Runtime detail strings and arbitrary count-map keys are also omitted;
they may contain link, identity, message, or backend details. Fixed typed stage
labels provide the shared evidence, while Network Doctor retains the detailed
diagnostic report. Repeated identical stage progress coalesces in place, typed
runtime terminal states resist later runtime updates, and the app task result
alone may resolve the final outcome.

The adapter adds a bounded record per app generation and reuses the existing
512-record/512-KiB history with terminal eviction. It introduces no sync,
automatic retry, worker, timer, subscription, queue, persistence, protocol
field, or dependency.

## Reticulum path-observation adapter

`src/operations/path.rs` observes the existing typed `PathUpdated` runtime
event at the application event boundary. The current upstream-facing event
contains destination, known/unknown state, and optional hop count. It does not
contain a typed request identity, request failure, timeout, or reason, so this
unit does not infer those states from logs or UI task results.

Destination text is trimmed, control-free, byte bounded, and normalized for
stable case-insensitive correlation. The shared operation key is an opaque
128-bit digest while the normalized destination remains the public target.
Repeated observations replace the prior path evidence:

- `known=false` is authoritative unresolved `Waiting`, never `Failed`;
- `known=true` is locally terminal `Completed`, never `Delivered`;
- hop count is shown only when the typed known-path event supplies it;
- a later unknown observation can reopen a completed path after route loss;
- an observation older than the retained record is ignored;
- history saturation leaves existing unresolved work intact and logs rejection.

The adapter adds no request, warmup, retry, timeout, path-table mutation,
worker, timer, subscription, queue, persistence, protocol field, action, or
dependency. Path-request initiation remains future work until a reliable shared
typed event boundary exists.

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
