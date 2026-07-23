# OMENbrowser_rs and omenchatd v0.9.6-2 release notes

Release candidate source qualification: `46ff3e0`  
Reticulum/LXMF crate train: exact `0.9.6`

## Highlights

- Adds an explicitly negotiated OMENchat durable-mutation path for room text.
  The client persists intent before transport, uncertain operations are never
  retried automatically, exact retries reuse their mutation identity, and the
  server transactionally retains replay results.
- Makes uncertain durable work visible after restart with identity-scoped,
  bounded recovery and deliberate retry, abandon, or expire actions.
- Preserves the legacy OMENchat v1 wire path for peers that do not negotiate
  the extension. Application revision `0.9.6-2` does not change OMENchat
  protocol version 1, destination aspects, identity ownership, or SQLite
  schema version 3.
- Adds separately qualified unsigned Intel and Apple Silicon DMGs, plus a
  standalone omenchatd archive for each macOS architecture.
- Retains Linux tarball, Debian, and AppImage formats and Windows portable ZIP,
  unsigned NSIS setup, unsigned MSI, and standalone omenchatd ZIP.

## Compatibility evidence

- Local full product tests: 1,260 passed; 29 explicit live/measurement cases
  remained ignored by the ordinary suite and were handled separately where in
  release scope.
- Local standalone omenchatd tests: 367 passed; 8 explicit soak/hardware or
  upstream-boundary cases remained ignored by the ordinary suite.
- Product and server Clippy passed with `-D warnings`.
- Pinned Python Reticulum/LXMF, current Python drift, v0.6.0-1, v0.9.5-2, and
  published v0.9.6-1 compatibility jobs passed in hosted run `29969738610`.
- Published v0.9.6-1 and candidate v0.9.6-2 passed bidirectional OMENchat live
  operation, old-client/current-server restart, and bidirectional SQLite state
  reopening.
- Native Linux, Windows MSVC, Intel macOS, and Apple Silicon macOS checks
  passed. Packaging run `29969739533` built and qualified every listed format.

## Resource and shutdown evidence

Release-mode local measurements covered 1,024 durable operations, 60-second
payload saturation, 6,000 SQLite commits under contention, 4,537 reconnect
cycles, and native desktop shutdown. Queues, links, file descriptors, tasks,
and admitted writes drained within their documented bounds. See
`migration/OMENCHAT_DURABLE_V0_9_6_2_EVIDENCE.md` for figures and limitations.

## Packages

Linux:

- `OMENbrowser_rs-latest.tar.gz`
- `omenbrowser-rs_0.9.6-2_amd64.deb`
- `OMENbrowser_rs-0.9.6-2-x86_64.AppImage`

Windows x86_64:

- `OMENbrowser_rs-0.9.6-2-windows-x86_64-portable.zip`
- `OMENbrowser_rs-0.9.6-2-windows-x86_64-setup-unsigned.exe`
- `OMENbrowser_rs-0.9.6-2-windows-x86_64-unsigned.msi`
- `omenchatd-0.9.6-2-windows-x86_64.zip`

macOS:

- `OMENbrowser_rs-0.9.6-2-macos-x86_64-unsigned.dmg`
- `OMENbrowser_rs-0.9.6-2-macos-aarch64-unsigned.dmg`
- `omenchatd-0.9.6-2-macos-x86_64.tar.gz`
- `omenchatd-0.9.6-2-macos-aarch64.tar.gz`

All artifacts have adjacent SHA-256 files. The manually qualified artifact
checksums were downloaded and independently verified before release review.

## Known limitations

- The macOS DMGs and Windows installers are unsigned test/alpha packages.
  Gatekeeper and SmartScreen acceptance is not claimed.
- The documented upstream Reticulum 0.9.6 maximum-size UDP Resource
  serialization regression remains outside OMEN's local fix boundary. Smaller
  Resource paths and bounded failure behavior remain supported and tested.
- Physical radios, public-network topology, and arbitrary third-party clients
  were not proven by deterministic or hosted software tests.
- Shared/external Reticulum mode remains experimental/deferred; managed mode
  remains the supported default.
- Durable mutation activation currently covers negotiated room text. Legacy
  and other mutation paths keep conservative no-automatic-retry behavior.

## Rollback

v0.9.6-1 remains the binary rollback. Do not delete identities, histories,
configuration, pending mutation intents, or server replay records. Disabling
durable capability advertisement/acceptance returns OMENchat to its legacy
behavior while preserving uncertain work for operator review.
