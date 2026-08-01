# LXMF OMENchat invitation capability checkpoint

Status: bounded codec/state foundation implemented; transport and UI inactive  
Release target: earliest `v0.9.6-6` after implementation and live evidence  
Locked upstream baseline: LXMF/Reticulum `0.9.6`

## Problem and invariant

The current client can safely receive an authenticated, bounded, Dismiss-only
LXMF OMENchat invitation. It cannot safely send one to an arbitrary contact.
The exact `v0.9.6-5` client persists the control JSON as an ordinary visible
message. A package version, display name, Directory entry, path, successful
ordinary LXMF exchange, or runtime SDK capability is not proof that the remote
application understands this payload.

The invariant is:

> An outbound invitation is unavailable unless the currently reachable peer
> instance provides fresh authenticated evidence for the exact invitation
> capability. Unknown, stale, unavailable, malformed, or conflicting evidence
> disables sending.

No probe failure or uncertain invitation send may trigger an automatic retry.

## Locked 0.9.6 API findings

The local locked sources expose several different capability concepts:

- `lxmf-sdk` session negotiation reports operations supported by the selected
  SDK backend. It does not attest to a remote LXMF contact's application.
- `IdentityAnnounceRequest.capabilities` and discovery identity metadata exist
  in the SDK/RPC surface, but the managed native path emits the
  Python-compatible `lxmf.delivery` announce application data. The project has
  no tested cross-backend contract binding arbitrary application capability
  names from those SDK fields to the exact remote delivery destination.
- Reticulum announce storage has a general capabilities field, but a stored
  value is not a fresh challenge/response from the active peer instance.
- OMENchat Link negotiation applies between its client and omenchatd. It cannot
  attest to a separate LXMF contact's OMENbrowser behavior.

None independently satisfies the invariant. Extending standard
`lxmf.delivery` announce data is rejected because it risks Python/LXMF
interoperability and can remain stale after another client reuses the identity.

## Proposed capability and endpoint

Proposed capability name:

```text
omenchat-lxmf-invitations-v1
```

Its meaning is deliberately narrow:

- accepts `omenchat.lxmf.invite` version 1 as an authenticated LXMF application
  payload;
- applies the current 4-KiB envelope and field limits;
- supports tokenless, expiring invitations only;
- presents a preview and never automatically connects, joins, saves, trusts,
  consumes a token, or grants a role;
- consumes the control message without ordinary-history persistence.

It does not advertise token consumption, Open/Join/Save actions, notices,
attachments, paper messages, external-RPC provenance, or final delivery.

The candidate proof endpoint is a dedicated Reticulum Single destination owned
by the same browser identity as its LXMF delivery destination, with conceptual
name/aspects:

```text
application: omenbrowser
aspects:     lxmf.capabilities
```

The exact public `reticulum-rs-transport 0.9.6` construction and request-handler
APIs must be compiler-verified in an isolated spike before these strings become
a wire commitment. The endpoint must not reuse omenchatd's server identity or
storage and must not alter the standard LXMF delivery announce payload.

## Proposed wire structures

Use bounded MessagePack arrays with no map, extension, body, identity secret,
token, or free-form diagnostic string:

```text
request  = ["omenbrowser.lxmf.peer-capabilities", 1, nonce_bin16]
response = ["omenbrowser.lxmf.peer-capabilities", 1, nonce_bin16,
            ["omenchat-lxmf-invitations-v1", ...]]
```

Proposed limits:

| Value | Limit |
| --- | ---: |
| complete request | 128 bytes |
| complete response | 1,024 bytes |
| nonce | exactly 16 random bytes |
| capability items | 16 |
| capability name | 64 ASCII bytes |
| nesting | fixed arrays shown above |
| trailing data | forbidden |

The response is authoritative only when returned over the authenticated Link
to the destination derived from the resolved peer identity and when it echoes
the exact nonce. Another identity/destination, a duplicate field/name, unknown
version, malformed or excessive value, or trailing data is rejected.

## Ownership and resource policy

Candidate implementation boundaries:

- managed native runtime owns the destination, request handler, bounded probe
  executor, and shutdown join;
- the application owns only project-level `Supported`, `Unsupported`,
  `Unknown`, `Stale`, and `Checking` evidence;
- at most one probe is in flight per peer and eight globally;
- each probe has one total deadline no greater than 15 seconds;
- repeated explicit probes for one peer have a 60-second cooldown;
- cache at most 256 peer results and 64 KiB accounted bytes;
- successful evidence expires after 10 minutes and immediately on runtime
  restart, identity change, backend change, or authenticated identity conflict;
- negative/failed evidence expires after 60 seconds;
- pruning is incremental and bounded;
- no recurring polling, announce loop, detached task, or automatic probe occurs.

The UI may probe only from an explicit invitation-send preparation action. A
fresh supported result enables one user-confirmed send attempt. It is consumed
after the attempt so another invitation requires a fresh probe. Transport
acceptance remains distinct from peer delivery.

## Persistence and migration

No database schema, index, config schema, identity format, message-history
format, or durable cache is proposed. Capability evidence is ephemeral so a
current process cannot leave stale permission for an older process using the
same identity after restart.

There is no persistent migration or downgrade operation. Rollback removes the
endpoint/probe code and leaves existing data untouched.

## Backend and mixed-version behavior

| Peer/backend evidence | Result |
| --- | --- |
| fresh verified response containing exact capability | one send may be confirmed |
| verified response omitting capability | unsupported; send disabled |
| `v0.9.6-5` or older peer | no endpoint; unknown/timeout; send disabled |
| no path, timeout, close, cancellation, malformed response | unknown; send disabled |
| stale cached support | stale; fresh probe required |
| identity/destination mismatch | conflict; send disabled and surfaced |
| external SDK/RPC backend | unsupported/unproven until it can own/prove endpoint |
| mock backend | test-only deterministic response; never live evidence |

No fallback sends an invitation as ordinary LXMF text. No application-version
heuristic or manually edited Directory flag can enable the action.

## Failure and crash boundaries

- A probe is read-only and creates no message/history/database mutation.
- Cancellation before dispatch sends nothing; cancellation after uncertain
  dispatch returns unknown and does not retry.
- Runtime shutdown cancels probes, closes the capability destination, joins its
  owner, and clears all evidence.
- A capability response never queues an invitation automatically.
- Outbound intent is constructed only after explicit user confirmation and is
  submitted once. An uncertain result remains uncertain.
- Receiver preview/replay limits and token redaction remain unchanged.
- A crash loses ephemeral support evidence and fails closed after restart.

## Required test matrix before activation

Deterministic Rust tests:

- exact request/response encode/decode and every byte/item/nesting boundary;
- nonce mismatch, duplicate capability, invalid ASCII, unknown version, and
  trailing-data rejection;
- authenticated identity/destination match and conflict;
- supported, unsupported, unknown, stale, checking, cancellation, timeout, and
  shutdown transitions;
- per-peer/global concurrency, cooldown, item/byte cache, expiry, and bounded
  pruning;
- no automatic probe and no automatic or uncertain retry;
- mock current/current success and current/prior absence;
- backend/identity/runtime changes clear evidence.

Live/process tests:

- current/current managed-native probe then one invitation preview;
- current sender to `v0.9.6-5`: no endpoint and zero invitation message;
- `v0.9.6-5` sender/current receiver retains ordinary legacy behavior;
- receiver and sender restart;
- path timeout, Link close, cancellation, malformed response, replayed response,
  and identity replacement;
- external backend remains visibly unsupported unless separately implemented;
- no history row, automatic connection, or token/body logging;
- idle CPU/task/link count is unchanged without explicit probe activity.

Package interaction remains a release-candidate gate. Python peers are expected
to omit this OMEN-specific capability and remain unaffected.

## Decision checkpoint

No wire or runtime implementation is made by this document. Before proceeding,
confirm all of the following:

1. a dedicated authenticated capability destination is acceptable product
   scope;
2. managed-native-only outbound invitations are acceptable initially;
3. the proposed destination naming and MessagePack contract are acceptable;
4. ephemeral fresh-proof policy is preferred over persistent capability state;
5. token-bearing invitations and action buttons remain outside this activation.

If any answer is no, keep outbound invitations disabled. Reception and the
diagnostic evidence lanes can remain without this protocol.

## Implementation progress

The checkpoint was accepted for the inert first slice. The project-owned
`chat::invitation_capability` module now defines:

- the exact protocol, destination, version, and capability constants;
- bounded fixed-array MessagePack request/response codecs;
- exact 16-byte nonces, canonical sorted unique capability names, and rejection
  of malformed, oversized, unsupported-version, or trailing input;
- nonce correlation without treating a response as transport authentication;
- frontend-neutral `Supported`, `Unsupported`, `Unknown`, `Stale`, `Checking`,
  and `Conflict` evidence;
- one-use support consumption, 15-second probe deadlines, 60-second cooldowns,
  eight global in-flight slots, 256 records, 64-KiB accounting, two TTL classes,
  incremental pruning, and explicit clear-on-shutdown ownership.

The initial codec/state slice registered no destination, opened no Link, sent
no request or invitation, created no task/timer/subscription, changed no UI,
and persisted nothing.

The next compiler-verified slice adds a test-only managed-native endpoint
ownership spike. It proves that the public pinned Reticulum API can:

- derive and register the dedicated same-identity
  `omenbrowser`/`lxmf.capabilities` destination deterministically;
- filter inbound Link events to that exact destination and request context;
- decode a bounded request and construct the exact bounded capability response;
- send a response on the Link's bound interface without a detached per-request
  task; and
- cancel, join, and deregister the endpoint under explicit ownership.

This endpoint began as a desktop-product library-test-only spike. That stage
did not register a production destination or spawn a production worker.

The endpoint owner now matches the cloneable managed transport-handle shape:
clones share one cancellation token, one join handle, and one exactly-once
deregistration flag. The first asynchronous shutdown cancels and joins the
worker before deregistration; a later shutdown through another reference is a
bounded no-op. Dropping the final owner remains a cancellation/abort fallback,
not the normal shutdown contract. This removes the ownership mismatch found in
the production lifecycle audit.

The receiver endpoint is now active only in the clean managed-native browser
runtime when `chat-client` is present. Startup registers the deterministic
same-identity destination before the transport becomes active. The endpoint
uses the transport's bounded broadcast Link-event stream, processes one request
at a time, emits only the fixed bounded capability response, and creates no
per-request task or application queue. Existing Link consumers retain their own
broadcast receivers and continue to filter by destination/context.

Transport replacement synchronously cancels the endpoint. Normal asynchronous
`stop_runtime` cancellation then joins it within a one-second internal deadline
and deregisters the destination exactly once, even when an operation retains a
clone of the underlying transport. Deadline expiry aborts the owned task,
deregisters the destination, reports a runtime shutdown error, and still leaves
the application lifecycle stopped.

No outbound probe, invitation send, UI action, persistent capability evidence,
recurring timer, announce, or automatic retry is enabled. External/shared RPC,
mock, TUI-only, and standalone omenchatd runtimes do not register this browser
endpoint. A sender still must prove the destination/identity and correlated
nonce through a separately reviewed probe before the evidence model can enable
one user-confirmed invitation attempt.

The clean managed-native transport now also owns an inert outbound probe
adapter, but no application, UI, diagnostics, or invitation-send caller invokes
it. The adapter:

- verifies that the supplied public identity derives the exact expected
  `lxmf`/`delivery` destination before requesting a capability path;
- derives the capability destination from that same identity and establishes
  an authenticated Reticulum Link;
- uses one random 128-bit nonce and one direct Request packet;
- accepts only a Response on that exact Link with a valid bounded payload and
  matching nonce;
- applies one total 15-second budget across path discovery, Link establishment,
  dispatch, and response rather than restarting the deadline per stage;
- tears down a created Link on success, timeout, cancellation, stream failure,
  malformed response, or correlation conflict;
- uses the existing one-per-peer/eight-global admission, cooldown, item/byte
  cache, TTL, and incremental pruning owner; and
- is cancelled and its ephemeral evidence cleared on transport replacement or
  runtime shutdown.

No retry or fallback primitive exists. Pre-cancellation performs no path or
Link work. Identity and nonce mismatches are retained as conflicts; timeout,
malformed data, cancellation, and transport failure do not become support.
There remains no method that consumes this evidence for a send. A controlled
two-process probe must prove the actual Request/Response exchange before any
explicit application caller is considered.

An explicit diagnostics-only CLI boundary now invokes the probe:
`--lxmf-invitation-capability-probe <peer_hash>`. It starts an isolated managed
runtime, performs at most one probe, stops the runtime, and reports only a
categorical result, elapsed time, the fixed deadline, zero automatic retries,
no invitation send, and shutdown status. It never returns the peer hash or
nonce in JSON. The associated two-process harness supports a current receiver
and an optional exact prior binary and redacts the extracted peer destination
from retained evidence.

This does not activate product UI or invitation sending. The live harness is
environment-bound and must actually pass before support evidence may be
consumed by any product action.

The diagnostics command also has an explicit bounded cancellation control:
`--lxmf-invitation-capability-cancel-after-ms <ms>`. Values above the existing
15-second total deadline are rejected, and the flag is invalid without the
probe command. The zero-delay case cancels before the probe future is polled,
then awaits the same owned cleanup path. Its report cannot become support and
records only categorical cancellation evidence. This adds no recurring timer,
retry, persistence, UI action, or invitation path.

The live harness now validates every report with a strict allowlisted-schema
validator and includes that deterministic zero-delay case. This strengthens
the eventual process evidence but does not replace the still-unrun current-peer
and prior-peer live exchange.

The fixed-size probe outcome and 15-second deadline now live at the always-
compiled runtime trait boundary and are re-exported by the chat capability
module. This restores the repository's empty-default-feature build without
changing the probe protocol, runtime behavior, or feature activation.
