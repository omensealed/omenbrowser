# OMENchat safe invitations checkpoint

Status: desktop confirmation active; deferred room selection and QR pending
Release target: `v0.9.6-4`
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

The root crate also contains a dormant `OmenChatInvitePayload` intended for an
LXMF handoff. No production caller encodes or decodes it. Its shape admits an
invite token, requested role, inviter destination, and password-required
metadata, and its current JSON decoder has no outer input bound. It is not an
appropriate public URI/QR contract and must not be activated or silently
treated as the new invitation format.

Iced 0.14 QR support and its locked `qrcode 0.13.0` dependency are already
available through the optional `desktop-qr` feature. The canonical desktop
product does not currently enable that feature. No new crate is required to
render QR data, but enabling the feature remains a deliberate product-graph
change that must pass native/package qualification.

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
bytes, authority tricks, noncanonical schemes, and the label limit. The type
has no production caller yet and therefore cannot connect, join, trust,
persist, or render QR data.

Step 2 is also complete as a dormant frontend-neutral reducer. It owns at most
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
and conflict cancellation. Step 3, desktop preview presentation and explicit
confirmation through the existing OMENchat session boundary, remains the next
gate.

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
and QR generation/import remain inactive.

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
mutation. Enhanced Micron links and QR rendering/import remain inactive.
