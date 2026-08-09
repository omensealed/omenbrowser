# OMENbrowser_rs and omenchatd v0.9.8-4 release notes

Status: final

Reticulum/LXMF crate train: exact official crates.io `0.9.8`.

## Negotiated nickname colours

- OMENchat wire protocol remains version 1. The local shared protocol crate is
  API version 0.2.0 and adds the explicitly negotiated
  `nickname-colours-v1` capability and operations 77–79.
- A user may persist only their own nullable RGB24 preference. `nil` selects a
  deterministic automatic colour derived from stable server and user IDs.
- The desktop corrects the rendered colour for the active theme to WCAG 4.5:1
  contrast or uses the theme foreground fallback. Stored RGB is never rewritten
  by theme selection and is never role, trust, or moderation evidence.
- Legacy peers retain the exact five-field user-list entry and receive no new
  event. New clients derive automatic local colours with old servers and
  disable the persistence editor with an explanation.
- Mutation intent is durably persisted before transmission. Exact duplicates
  replay the prior acknowledgement without a second database revision or
  broadcast; uncertain results are never automatically replayed.

## Storage and rollback

omenchatd schema 14 adds nullable checked `users.nickname_colour_rgb` and reuses
`profile_revision`. The existing migration mechanism creates a schema-13
backup before mutation; rows remain `NULL` unless a user explicitly chooses a
colour. No browser database migration is required.

Rollback to v0.9.8-3 is not binary-only: stop the server, preserve the schema-14
database, restore the automatic schema-13 pre-migration backup, install the old
binaries, validate integrity/counts, and restart.

## Attachment and transport truth

- Direct/local Resource attachments retain their current limits and support
  according to the release qualification gates.
- Routed multi-hop Resource retransmission remains unqualified on upstream
  0.9.8. OMEN applies no fork, patch, vendor override, guessed size ceiling,
  fragmentation, fallback, or automatic retry. Retry is manual after route or
  condition change.
- The independent maximum-UDP Resource sentinel remains visible and failing at
  its known upstream buffer boundary.
- Upstream-ready evidence is in `docs/upstream/`; OMEN does not apply those
  proposed corrections locally.

There is no OMENchat wire-version, operation-layout, database-content, config,
cache, identity, destination, ticket, upload-content, or Reticulum-storage
migration beyond the guarded schema-14 nullable column described above.
