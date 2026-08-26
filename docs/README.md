# OMENbrowser_rs documentation

This directory separates current product guidance from historical release
evidence. Start with [Current Status](CURRENT_STATUS.md); it is the concise
answer to “what does the current release actually support?”

## Users and testers

- [Quickstart](QUICKSTART.md) — build, run, and create isolated profiles.
- [Getting Online](GETTING_ONLINE.md) — interfaces, identity, and first
  announces.
- [OMENchat](OMENCHAT.md) — browser client and standalone server usage.
- [Configuration](CONFIGURATION.md) — managed paths, interfaces, and runtime
  settings.
- [Troubleshooting](TROUBLESHOOTING.md) — common startup and network issues.
- [Testing](TESTING.md) — current commands and qualification boundaries.

## Operators and packagers

- [Private Storage](PRIVATE_STORAGE.md) — owner-only storage policy.
- [Network Backends](NETWORK_BACKENDS.md) — supported managed mode and deferred
  external/shared mode.
- [Packaging](PACKAGING.md) — release archives and native packages.
- [Release Versioning](RELEASE_VERSIONING.md) — product and dependency version
  rules.
- [Current Release Notes](RELEASE_NOTES_V0_10_0_4.md).

## Developers

- [Developer Notes](DEVELOPERS.md) — source layout, features, and checks.
- [OMENchat Protocol](OMENCHAT_PROTOCOL.md) — authoritative capability and wire
  compatibility matrix.
- [LXMF Delivery and Events](LXMF_DELIVERY_AND_EVENT_MODEL.md).
- [Operations and Transfers](OPERATIONS_TRANSFERS.md).
- [Reticulum Transport Gaps](RETICULUM_TRANSPORT_API_GAP.md).
- [Reticulum 0.10.0 OMEN Capability Ledger](upstream/RETICULUM_0_10_0_OMEN_CAPABILITY_LEDGER.md).
- [Dependency Security](maintenance/DEPENDENCY_SECURITY.md).

## Evidence and history

- `RELEASE_NOTES_*.md` files describe immutable published releases.
- `migration/` contains only evidence still referenced by active or released
  documentation, rollback-relevant schema evidence, and the current release
  checklist.
- `upstream/` contains source-referenced upstream-ready reports.
- [Documentation History](HISTORY.md) explains what was intentionally removed
  from the current tree and how to retrieve it from Git.

Phase-unit transcripts, superseded design checkpoints, and obsolete execution
plans are not current product documentation. Do not infer support from an old
tag, commit message, or historical report; use current source, tests, and the
documents listed above.
