# OMENchat safe invitations checkpoint

Status: public URI/QR active; authenticated LXMF preview active; outbound LXMF product action disabled
Release target: `v0.9.6-6`
Protocol baseline: OMENchat protocol 1; no new frame operation or capability

## Current-state findings

OMENchat already has a compatible public launch address:

```text
omenchat://<server-destination>
```

omenchatd displays that address in status, its TUI, and the generated NomadNet
portal. OMENbrowser accepts it from the quick-open field, Directory actions,
and rendered Micron links. `OmenChatDescriptor` can add bounded display, room,
LXMF, and theme hints to a rendered Micron link, but a plain URI carries only
the server destination.

The root crate also contains the distinct bounded `OmenChatInvitePayload` used
for the authenticated native-LXMF presentation-only handoff. Its shape admits
an invite token, requested role, inviter destination, and password-required
metadata, so it is not the public URI/QR contract and must not be silently
treated as one. The hardening record below documents its decoder, sender
evidence, replay limits, and trust restrictions.

Iced 0.14 QR support and its locked `qrcode 0.13.0` dependency are already
available through the optional `desktop-qr` feature. Both canonical desktop
products now enable that reviewed feature. The transitive encoder is pure Rust,
MIT OR Apache-2.0, and already locked; it adds no camera, decoder, native
library, platform permission, network path, or second payload format. The
machine graph gate requires the feature and exact locked encoder in both
products. Native/package qualification remains a release gate.

## Scope and trust model

The safe invitation is public connection metadata, not an authorization
credential. It may contain only:

- the exact OMENchat server destination;
- one optional numeric room identifier;
- one optional bounded display label;
- one optional claimed server identity fingerprint.

It must never contain a password, IFAC credential, bearer/invite token,
moderation role, reusable ticket, client identity, private identity material,
or remote-RPC credential.

The destination is the only routing authority. A label is untrusted
presentation. A fingerprint supplied by an invitation is a claim until it
matches authenticated Directory/announce evidence; importing it must not mark
the server trusted. A room is a post-session join suggestion, not proof that
the room exists or that the recipient is authorized to enter it.

## Proposed canonical URI

The additive invitation form is:

```text
omenchat://<server-destination>?invite=1&room=<u32>&label=<pct>&identity=<hex>
```

Canonical serialization rules:

- lowercase scheme, destination, and identity;
- fixed query order: `invite`, `room`, `label`, `identity`;
- omit absent optional fields;
- `invite=1` is required for the enhanced form;
- percent-encode label bytes using uppercase hexadecimal;
- never emit `+` as a space;
- reject duplicate keys, unknown keys, fragments, user info, ports, nested
  URLs, control characters, invalid UTF-8, malformed percent escapes, and
  trailing material;
- a plain `omenchat://<destination>` remains the existing compatible launch
  address.

The exact limits proposed for implementation are:

| Input | Limit |
| --- | ---: |
| complete URI or QR payload | 2,048 bytes |
| decoded fields | 4 |
| server destination | existing `CHAT_SERVER_DESTINATION_MAX_BYTES` |
| room | one decimal `u32` |
| display label | existing `CHAT_SERVER_DISPLAY_MAX_BYTES` |
| identity fingerprint | exactly 32 hexadecimal characters (16 Reticulum hash bytes) |

The parser must apply the complete encoded-size bound before allocating decoded
fields. Decoding must be a small standard-library implementation or reuse an
already admitted dependency; no URL/QR crate is added merely for convenience.

## Confirmation and application behavior

An enhanced invitation never connects, joins, saves, or changes trust directly
from parse success.

1. Parse into a project-owned bounded preview.
2. Show destination, room suggestion, label, and fingerprint evidence.
3. Compare the claimed fingerprint with an exact current Directory entry when
   available and label it `verified match`, `conflict`, or `unverified`.
4. Require an explicit Open confirmation.
5. On confirmation, open the existing OMENchat session path with the exact
   destination. Do not save or trust the entry automatically.
6. Join the suggested room only after authenticated session negotiation and
   only if the server catalog contains that exact room identifier. Otherwise
   remain in the normal room and report the mismatch.
7. Cancel discards the ephemeral preview.

A fingerprint conflict must fail closed: Open is unavailable until the user
explicitly discards the claimed fingerprint and proceeds through the ordinary
manual destination flow. The invitation feature must not create a second Link,
reconnect loop, runtime owner, or session state machine.

Plain legacy launch links retain their current behavior for compatibility.
Server-generated enhanced invitation actions should use the confirmation flow.

## QR boundary

QR is a presentation of the exact canonical URI, not a second encoding or
protocol. The application must:

- render only a URI produced by the canonical serializer;
- decode/import textual QR payload only after the 2,048-byte outer bound;
- pass imported text through the same parser and confirmation model;
- never embed images, binary payloads, credentials, or compressed JSON;
- provide copyable text alongside the QR so the feature remains usable without
  a camera or QR build profile.

Camera capture and image-file QR decoding are outside this release. They would
add platform permissions, media input, and a new untrusted-image decoder
boundary. The first release may generate QR data without importing images.

## Storage, wire, and mixed-version behavior

This design adds no OMENchat operation, capability, database schema, server
state, identity state, or durable mutation. Invitation preview state is
ephemeral and bounded.

Older OMENbrowser clients continue to use the plain URI. Older clients may not
understand the enhanced query, so omenchatd and public documentation must keep
the plain URI available beside any enhanced invitation. A v0.9.6-4 client must
not send query metadata to omenchatd; it resolves the destination locally and
uses the existing protocol-1 session flow.

No invite label or claimed fingerprint is written to Directory or trusted
storage without a separate existing explicit user action. No secret-bearing
legacy `OmenChatInvitePayload` value is logged, copied into the URI, or
rendered as QR data.

## LXMF payload hardening and preview activation

The separate `OmenChatInvitePayload` serialization boundary is fail-closed:

- encoded JSON is capped at 4 KiB before deserialization;
- unknown fields are rejected;
- protocol and version must exactly match `omenchat.lxmf.invite` version 1;
- server and inviter destinations must be canonical lowercase 32-character
  hexadecimal Reticulum destination hashes;
- room identifier, display fields, token, and introduction have explicit byte
  limits and reject invalid/control text;
- expiration is enforced with a documented five-minute clock-skew allowance;
- token and introduction content are redacted by both the log-safe projection
  and the type's `Debug` implementation;
- tokenless duplicates are classified only for bounded presentation, while a
  token-bearing payload explicitly requires server-side token consumption and
  is never described as one-time by the client.

The live receiver now permits only a presentation-only preview from a message
that carries the project-owned authenticated native LXMF source marker. It
does not persist an invitation, connect, join, trust an identity, consume a
token, or grant the requested role.

The frontend-neutral reducer is active at the application event boundary:

- one owner retains at most one pending LXMF preview;
- authenticated sender evidence is `match`, `mismatch`, or `unavailable`, and
  a mismatch blocks confirmation without being rewritten as trust;
- canonical payload hashing suppresses the same invitation even when JSON
  whitespace or field formatting differs;
- replay evidence retains at most 64 records and accounts at most 64 KiB of
  admitted encoded input for seven days;
- each admission prunes at most eight expired records, avoiding an unbounded
  cleanup pass;
- one authenticated sender can present at most four distinct invitations in a
  five-minute window;
- invalid, duplicate, rate-limited, or capacity-rejected input leaves the
  current pending preview unchanged;
- cancellation only releases the preview. Replay/rate evidence remains bounded
  so repeatedly dismissing a payload cannot bypass suppression.

The reducer is owned by the core application and consumes the existing runtime
event stream; it adds no subscription. It does not
claim the inviter, requested role, room, or token is authoritative. Explicit
Open/Join/Save confirmation, token enforcement, and mixed-version live tests
remain gates for any action beyond Dismiss.

The native direct and propagated LXMF decoders now retain one project-owned
`native_lxmf_source_authenticated=true` evidence field only after all of the
following succeed:

- the wire source equals the announced `lxmf.delivery` destination derived
  from the resolved sender identity;
- the LXMF signature verifies against that identity;
- normal bounded LXMF decoding succeeds.

Unverified decoding deliberately omits the field. The invitation helper also
requires an inbound message and a canonical 32-character peer destination
before exposing that peer as an authenticated sender candidate. Forged and
identity-mismatched direct/propagated messages remain rejected before this
evidence exists.

This supplies the sender-authority input used by the managed native invitation
reducer. The runtime capability matrix reports this evidence separately. It
does not prove that the external SDK/RPC backend exposes equivalent evidence;
external messages lack the marker and cannot enter preview state.

The extraction contract is active and tested:

```text
LXMF title:       omenchat.lxmf.invite
LXMF content:     validated bounded JSON OmenChatInvitePayload
LXMF attachments: none
```

All other titles remain ordinary messages. The exact invitation title requires
authenticated-source evidence before parsing content, rejects attachments, and
feeds the same bounded preview/replay owner. Failed extraction preserves the
existing preview. This avoids a new binary envelope, custom-field collision,
or automatic media/Resource handling.

Exact-title control messages are consumed by the control boundary instead of
being persisted or displayed as ordinary conversation history. Invalid,
attached, unauthenticated, duplicate, or rate-limited invitations are rejected
without exposing their token-bearing JSON as chat content. The external
SDK/RPC backend cannot create the required evidence marker with its current
published contract.

The deterministic sender/receiver evidence fixture now builds the actual
bounded invitation payload, encodes and signs it through the native LXMF wire
path, verifies and decodes it through the production receiver, and feeds the
normal application runtime queue under an isolated root. It requires an
authenticated-match preview, no ordinary message-history row, and no
connection action. The companion script also runs forged/mismatched-signature,
missing-authentication, and Dismiss regressions.

The repository now includes an explicit opt-in managed-native two-process live
harness. Its sender uses the existing bounded readiness path and submits one
tokenless, expiring direct invitation without automatic retry. Its receiver
examines at most 256 events within a caller-bounded 1--300 second window and
requires the authenticated preview, an inspected empty ordinary-history result,
and no connection action. The evidence report contains hashes and states but
not the invitation body or token. This harness has not been run in the current
environment because no isolated live identities and TCP test gateway were
provided.

The diagnostic harness is not product activation or a mixed-version pass.
Outbound invitations remain disabled in the UI because LXMF contacts do not
currently negotiate this application payload capability; a prior client may
display the JSON as ordinary content. That compatibility classification is
fail-closed rather than guessed from the application version.

The exact `v0.9.6-5` source classification confirms the prior client does
display/persist the control payload as ordinary LXMF history: its payload type
was dormant, its inbound reducer had no title-specific control branch, and it
lacked the current authenticated-source marker. The tag and reviewed commit are
asserted by `scripts/test-lxmf-invitation-prior-version.sh`. This closes the
deterministic downgrade classification but is not a live prior/current process
pass. Consequently outbound product sending remains disabled until an explicit
peer capability can prevent delivery to an incompatible client.

The proposed fresh peer-proof contract, resource limits, mixed-version policy,
and explicit pre-wire decision are recorded in
`docs/design/LXMF_OMENCHAT_INVITATION_CAPABILITY_CHECKPOINT.md`. It deliberately
does not alter the standard LXMF announce, OMENchat protocol, or storage.

The desktop presentation-only boundary now renders the application-owned live
preview:

- server destination and room claim;
- inviter display/destination claim and authenticated-sender evidence;
- requested role explicitly labeled as a claim rather than a grant;
- password-required claim without collecting or storing a password;
- expiration, replay/token policy, and bounded introduction text;
- a statement that opening remains disabled and no trust or role is granted.

Dismiss is the only action. It releases the pending preview without opening a
session, joining a room, saving a server, changing Directory trust, consuming a
token, or clearing the bounded replay/rate evidence. An unauthenticated exact-
title message never enters desktop preview state.

## Failure and resource boundaries

- Oversized or malformed input is rejected before connection or state change.
- Percent-decoding cannot exceed the bounded encoded input.
- Parser errors are user-safe and do not echo the full hostile input.
- Preview state owns at most one invitation; a newer explicit import replaces
  it.
- Closing the preview releases all owned strings.
- Confirmation routes through the existing bounded path/link/session owners.
- No worker, timer, subscription, retry, cache, queue, or recurring task is
  introduced.
- Connection, path, and room failures remain ordinary typed OMENchat/Reticulum
  evidence and never rewrite the invitation as trusted.

## Test matrix

- plain URI compatibility;
- canonical enhanced round trip and fixed field ordering;
- exact and next-byte URI/label/destination bounds;
- empty, duplicate, unknown, malformed-percent, invalid-UTF-8, control,
  fragment, user-info, port, and trailing-data rejection;
- room `0`, `u32::MAX`, and overflow;
- exact fingerprint length/hex validation;
- no secret key name or value can serialize;
- verified fingerprint match, unverified claim, and conflict;
- parse/cancel performs no connection or storage mutation;
- confirmation opens exactly one existing session path;
- room suggestion applies only after exact catalog match;
- legacy/current URI fixtures remain accepted;
- QR presentation uses byte-identical canonical URI;
- canonical desktop and static-media feature graphs remain deterministic if
  `desktop-qr` is admitted.

## Rollback

Disable enhanced invitation generation/import and retain the plain
`omenchat://<destination>` path. Because preview state is ephemeral and no
schema, protocol, or identity state changes, rollback requires no data
migration. Do not delete or reinterpret the dormant LXMF invite type as part of
this unit; its separate removal or hardening needs compatibility evidence if a
future caller adopts it.

## Implementation order

1. Add a project-owned canonical URI value/parser with exhaustive boundary
   tests and no UI behavior.
2. Add ephemeral preview/trust-evidence reduction with no connection action.
3. Route explicit confirmation through the existing OMENchat open/session
   boundary and validate deferred room selection.
4. Add copyable invitation generation.
5. Enable QR rendering only after the product feature graph and native package
   gates are reviewed.

Do not combine this work with corrections, tombstones, room policy, or a new
network runtime.

## Implementation progress

Step 1 is complete in `chat::invitation`. The frontend-neutral value accepts
the exact legacy plain URI or the additive enhanced query, applies the 2 KiB
outer bound before decoding, normalizes exact 32-character hexadecimal
destination and identity values, and owns a small percent encoder/decoder for
the bounded public label. Serialization emits only the fixed no-secret field
set and order.

Focused tests cover canonical round trip, legacy compatibility, uppercase
normalization, destination/fingerprint length and hex rules, room boundaries,
unknown/duplicate/trailing fields, malformed percent and UTF-8 input, control
bytes, authority tricks, noncanonical schemes, and the label limit. The same
type is now used by the desktop preview, explicit confirmation, copy, and QR
paths described below.

Step 2 is also complete as a frontend-neutral reducer. It owns at most
one preview, replaces it only after a new URI parses successfully, and requires
explicit cancellation. Identity evidence is one of no claim, unverified,
verified match (with Directory trust reported separately), or conflict. Only
exact OMENchat Directory destinations participate. Any conflicting identity
for that destination wins over a matching duplicate and blocks confirmation;
the blocked preview remains visible until explicitly cancelled. Taking a
confirmable invitation consumes the preview but still performs no connection
or state mutation because no production caller exists yet.

Focused tests cover every evidence class, duplicate conflict precedence,
single-item replacement, invalid-import preservation, confirmable consumption,
and conflict cancellation. Desktop preview presentation and explicit
confirmation through the existing OMENchat session boundary are active.

The first half of step 3 is now active in the desktop quick-open surface.
Enhanced invitation input creates the single preview and opens no session.
The card shows the exact destination, claimed label, suggested numeric room,
and identity-evidence classification; a conflict exposes only Cancel. Import
does not modify Directory or trust state. Explicit Open consumes a confirmable
preview and routes the plain destination plus bounded display hint through the
existing OMENchat Link boundary. Legacy plain links retain their existing
behavior.

The deferred-room half of step 3 is now active. Confirmation may own one
in-memory numeric room suggestion, bound first to the exact normalized
destination and then to the exact opened session. It is consumed only by that
session's authenticated bounded `RoomsUpdated` catalog and only when the
catalog contains the exact numeric room ID; the existing join owner receives
the returned room name. A different session cannot consume it. A missing room,
open/handshake error, close, explicit cancellation, or a replacement invitation
clears it. Existing already-authenticated live sessions may use their retained
authenticated catalog immediately. Nothing is persisted and no mutation is
retried.

Focused desktop-dev tests cover the exact match, cross-session isolation,
catalog mismatch, cancellation, and replacement paths. Micron enhanced links
and QR generation were still inactive in that subunit.

Step 4 is now active as a synchronous desktop clipboard action in each
OMENchat composer. It serializes the exact open-session destination, includes
the active numeric room only when that room is joined, and includes the
bounded server display label. It includes a Directory identity fingerprint
only when every valid exact-destination OMENchat entry agrees; malformed or
conflicting evidence is omitted rather than guessed. The canonical serializer
remains the final 2 KiB/no-secret gate.

Generation does not create or alter Directory evidence, trust, sessions,
rooms, storage, protocol state, or network activity. It owns no worker, timer,
queue, cache, or retry. The existing plain `omenchat://<destination>` form
remains available for older clients and documentation. Focused tests cover
canonical field selection, percent encoding, unjoined-room omission,
conflicting-identity omission, missing-session failure, and zero state
mutation.

Enhanced OMENchat links activated from Micron now enter the same single
confirmation owner as quick-open text. Valid enhanced links open no session and
do not save or trust Directory evidence. Malformed enhanced links are handled
as invitation errors rather than falling through to browser navigation or a
plain Link open. Link form-forwarding fields are not invitation fields and are
ignored. Plain `omenchat://<destination>` Micron links preserve their existing
open behavior. Keyboard-focused and pointer-hit activation share this reducer.
Focused tests cover valid, malformed, and legacy paths.

Step 5 adds an explicit QR toggle beside the existing copy action. One
ephemeral owner retains at most one canonical URI and Iced QR matrix/cache.
Input is already capped at 2 KiB and contains only the approved public fields.
The card displays the exact URI alongside the QR; Copy uses the retained URI
while the card is open, so Directory changes cannot make the clipboard differ
from the visible matrix. Opening another QR replaces the owner. Toggle/Close,
session close, and room transition release it. There is no recurring render
subscription, camera, image decode/import, permission, storage, network action,
or wire change. Textual QR payload import already uses the normal quick-open
parser; camera and image-file QR decoding remain outside scope.
