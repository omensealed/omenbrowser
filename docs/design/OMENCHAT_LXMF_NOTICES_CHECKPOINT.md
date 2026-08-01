# OMENchat asynchronous LXMF notices checkpoint

Date: 2026-07-31  
Status: dormant bounded envelope and admission owner; no runtime caller

## Scope

LXMF may eventually carry small asynchronous OMENchat pointers when a user has
opted in. It must not duplicate ordinary room traffic or become a presence,
typing, heartbeat, broadcast, or history channel.

This checkpoint adds the content-minimizing envelope and a caller-inert,
in-memory admission owner. Nothing currently sends, receives, recognizes,
persists, renders, retries, downloads, connects, joins, trusts, or grants
authority from it.

## Proposed capability and payload

The separate capability name is:

```text
omenchat-lxmf-notices-v1
```

It is not implied by `omenchat-lxmf-invitations-v1`. A peer must negotiate it
explicitly before any future send action. The payload protocol/title is
`omenchat.lxmf.notice`, version 1, with a maximum encoded size of 1 KiB.

The envelope contains only:

- a random 128-bit notice identifier encoded as 32 lowercase hexadecimal
  characters;
- notice kind;
- canonical server destination;
- kind-specific numeric room/event/activity/maintenance pointers;
- creation and expiry times.

There is deliberately no message body, room history, display name, invitation
token, role claim, attachment, filename, URL, command, or arbitrary metadata.
The user-facing label can be derived locally from the kind. Opening the related
server/room/event, if later implemented, remains an explicit user action.

## Kinds and shape

- `offline_mention`: positive room and event IDs.
- `directed_moderation`: positive room and moderation-event IDs.
- `planned_maintenance`: a maintenance timestamp inside the notice lifetime;
  no room/event pointer.
- `followed_room_summary`: positive room/latest-event IDs and a count from 1 to
  1000.

Notices live at most seven days. Creation may be at most five minutes ahead of
the receiver clock, and expiry has the same skew allowance. Unknown fields,
noncanonical identifiers, mismatched kind fields, malformed input, and
oversized input are rejected atomically.

## Implemented admission owner

The dormant owner now supplies:

1. exact protocol/version decoding with no attachment field;
2. a required canonical authenticated-sender value kept outside the payload;
3. disabled-by-default per-kind opt-in/mute state;
4. deduplication by authenticated sender plus notice ID;
5. retention capped at 128 items and 64 KiB of encoded notice plus sender
   accounting;
6. at most eight notices per sender and 64 globally per ten-minute window,
   with at most 512 rate records;
7. coalescing only for a newer summary from the same sender, server, and room;
   the new count replaces the old count and is never summed;
8. at most eight expired notice/rate entries pruned per call and explicit
   ephemeral shutdown clearing;
9. no ordinary chat-history or database persistence; and
10. an API that always reports that retained notices permit no automatic
    action.

The owner does not authenticate LXMF senders itself. A future runtime caller
must supply authoritative sender evidence from the transport boundary; payload
fields and generic topic peer metadata are not sufficient. Activation also
still requires exact message-title recognition, no-attachment enforcement at
the transport boundary, negotiated mixed-version capability evidence, and zero
fallback as plain LXMF text.

An uncertain send is never retried automatically. Receipt of a notice never
downloads media, connects to OMENchat, joins a room, changes trust, or performs
moderation.

## Storage and compatibility

There is no schema or configuration change in this slice. Opt-in state and
retained/rate records exist only in the unconnected owner and are not wired to
application state. If durable notice preferences or retained notices are later
justified, their migration and retention policy require a separate checkpoint.
Older versions must receive zero notice traffic because send requires fresh
negotiated capability evidence.

Rollback removes `src/chat/notice.rs`, its module export, and this document; no
data cleanup is required.
