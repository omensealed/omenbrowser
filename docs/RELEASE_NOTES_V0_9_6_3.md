# OMENbrowser_rs and omenchatd v0.9.6-3 release notes

Reticulum/LXMF crate train: exact `0.9.6`

## Highlights

- Fixes OMENchat timeline positioning by using a bottom-anchored Iced
  scrollable while preserving existing saved scroll semantics.
- New rooms settle at the newest message, attachment/media expansion follows
  the tail only when the user was already at the tail, and manual history
  position is preserved.
- Fixes the native LXMF SDK wire path so bounded file attachments are included
  in the outgoing message instead of only appearing in the local summary.
- Adds deterministic OMENchat scroll smoke coverage and LXMF attachment
  round-trip/interoperability coverage.

## Compatibility

- OMENchat wire protocol remains version `1`.
- Reticulum/LXMF dependencies remain pinned to the exact `0.9.6` train.
- Identity, configuration, SQLite, cache, destination, and storage contracts
  are unchanged.
- The standalone omenchatd remains independently packaged.

## Packages

The release workflow builds:

- Linux tarball, Debian package, and AppImage.
- Windows x86_64 portable ZIP, unsigned NSIS setup, unsigned MSI, and
  standalone omenchatd ZIP.
- Unsigned Intel and Apple Silicon macOS DMGs and standalone omenchatd
  archives.

All artifacts include SHA-256 evidence. Windows and macOS packages remain
unsigned test/alpha packages.

## Known limitations

- Physical radios, public-network topology, and arbitrary third-party clients
  are not implied by deterministic loopback coverage.
- The documented upstream Reticulum maximum-size UDP Resource limitation
  remains outside OMEN's local fix boundary.

## Rollback

`v0.9.6-2` remains the binary rollback. This patch does not migrate persistent
data; do not delete identities, histories, configuration, or server state when
rolling back.
