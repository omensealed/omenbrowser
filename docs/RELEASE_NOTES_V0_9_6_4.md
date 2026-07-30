# OMENbrowser_rs and omenchatd v0.9.6-4 release notes

Reticulum/LXMF crate train: exact `0.9.6`

## Highlights

- Adds durable, capability-negotiated OMENchat mutations with persistent
  client operation identity, exact replay, conflict detection, bounded server
  retention, and cautious recovery after uncertain outcomes.
- Adds replies and mentions, reactions, bounded local search, invitation
  handling, message corrections and tombstones, pins, bounded moderation
  history, room-history retention, announcement rooms, slow mode, and
  per-room upload policy.
- Adds shared bounded Operations/Transfers state for desktop and TUI, clearer
  delivery evidence, propagation policy/status, actionable errors, workspace
  presets, command-palette actions, and TUI copy/select and reconnect QoL.
- Improves OMENchat reconnect, link replacement, history/resource recovery,
  attachment handling, scroll stability, and compact reaction/reply controls.
- Adds independent omenchatd management for multiple TCP client interfaces,
  stronger status/doctor output, bounded administration and storage paths, and
  guarded database downgrade-copy procedures.

## Compatibility

- OMENchat remains protocol version `1`; application version does not replace
  wire capability negotiation.
- New fields and mutations are sent only after explicit capability
  request/accept on the authenticated Link.
- Legacy and adjacent peers retain their existing room shapes, ordinary
  message behavior, global upload admission, and no-automatic-retry handling.
- Reticulum/LXMF dependencies remain pinned to the exact `0.9.6` train.
- Browser and omenchatd identities, configuration roots, and storage remain
  separate. Managed Reticulum remains the supported default; experimental
  shared runtime is not enabled.

## Storage and rollback

omenchatd schema 13 stores the bounded feature state. Existing databases are
backed up before migration, and guarded copy exports are available for
rollback boundaries. Do not replace a current database with an older binary
in place.

`v0.9.6-3` remains the binary rollback. Stop omenchatd cleanly, preserve the
current database and sidecars, create and validate the documented downgrade
copy required by the target binary, then switch binaries and the copy
together. Browser identity, history, cache, and server identity/upload roots
must be retained.

## Packages

The release workflow is intended to build:

- Linux tarball, Debian package, and AppImage.
- Windows x86_64 portable ZIP, unsigned NSIS setup, unsigned MSI, and
  standalone omenchatd ZIP.
- Unsigned Intel and Apple Silicon macOS DMGs and standalone omenchatd
  archives.

All distributable artifacts must include SHA-256 evidence. Windows and macOS
packages remain unsigned test/alpha packages.

## Known limitations

- The locked Reticulum 0.9.6 API does not expose receiver-side cancellation
  for an already incoming Resource. OMEN does not present a false cancel
  control; bounded admission, failure, Link-close, expiry, and shutdown cleanup
  remain in force.
- Per-room upload ceilings apply only to peers that negotiated the capability.
  The global server limit remains the universal hard ceiling for legacy peers.
- Physical radios, public-network topology, arbitrary third-party clients, and
  physical-GPU behavior are not implied by deterministic and isolated
  loopback coverage.
- External/shared Reticulum runtime remains explicitly deferred.
