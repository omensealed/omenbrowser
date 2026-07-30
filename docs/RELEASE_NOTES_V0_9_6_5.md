# OMENbrowser_rs and omenchatd v0.9.6-5 release notes

Reticulum/LXMF crate train: exact `0.9.6`

## Focus

This is a narrow OMENchat correctness and presentation revision over
`v0.9.6-4`.

- omenchatd now returns each authoritative live reaction event to the capable
  originating Link as well as other capable clients joined to the room.
- Client session persistence now canonicalizes multi-user reaction snapshots,
  preventing repeated `invalid OMENchat reaction snapshot` warnings when
  different users select different reactions on one message.
- Authoritative correction/tombstone and pin events likewise return to the
  capable originating Link, so the initiating client updates live without
  treating an acknowledgement as final projection state.
- Reaction summaries remain visible, while the emoji mutation controls appear
  only while the pointer is over that specific message.
- The footer command-palette action uses a compact, centered terminal icon with
  its label available as a tooltip.
- Moderation audit is an explicit authorized action with a close control.
  Unauthorized sessions no longer show an unusable audit panel.
- Remote user names retain mention toggling without the room-button border.
- Passive room upload policy no longer occupies a persistent composer banner;
  upload enforcement and actionable disabled/rejection feedback remain.
- The release reaction smoke now waits for actual Resource snapshot evidence
  after the live delta instead of returning early from already-correct client
  state.

## Compatibility and storage

- OMENchat remains protocol version `1`; no operation, frame, capability,
  destination, or encoding changed.
- Browser and omenchatd SQLite schemas are unchanged.
- Identity, configuration, history, upload, and cache locations are unchanged.
- Reticulum/LXMF dependencies remain pinned to the exact `0.9.6` train.
- `v0.9.6-4` remains a direct binary rollback because this revision performs no
  persistent-data migration.

## Packaging

The release workflow should produce the same Linux, Windows, macOS, and
standalone omenchatd artifact families as `v0.9.6-4`. Published
`v0.9.6-4` tags and artifacts remain immutable; these corrected binaries use
the new `v0.9.6-5` revision and checksums.
