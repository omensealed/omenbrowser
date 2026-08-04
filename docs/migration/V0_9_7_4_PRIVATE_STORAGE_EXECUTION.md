# v0.9.7-4 private-storage execution record

Baseline: released `v0.9.7-3` commit
`67ab1b910e0d51d14d2f63c3e764e86bbccfe2cc`.

The implementation adds separate standard-library permission helpers to the
browser and standalone server Cargo roots. Exact product-owned Unix
directories are created and repaired as `0700`; sensitive regular files are
created and repaired as `0600`. Existing known paths receive metadata-only
repair through exact lists or pre-existing bounded scans. Symlinks and
unexpected path types fail closed, while existing custom parents and legacy
source trees are validated without having their modes claimed.

Server coverage includes the selected home, Reticulum config/storage,
generated NomadNet pages, uploads, config and identity backups, SQLite
main/WAL/SHM and migration backups, and active/rotated logs. Browser coverage
includes exact managed and identity-scoped directories, private settings and
state writers, identities/backups, message files/databases, plugin registry,
and structured logs. The systemd user unit adds `UMask=0077`; its installer
uses a restrictive umask, repairs only the selected home, rejects a symlinked
home, and preserves data on uninstall.

## Pre-bump qualification evidence

- The untouched quick baseline passed at `v0.9.7-3`.
- A dedicated `umask 0000` reproducer confirmed the prior permissive server
  creation behavior without using real roots.
- The isolated replacement scenario passed with the server home/directories at
  `0700` and config, identity, Reticulum config, database, and log at `0600`.
- A live SQLite connection under a dedicated `umask 0000` child verified main,
  WAL, and SHM at `0600` while representative data remained committed.
- Existing-mode repair tests preserve config, identity, message, backup, and
  log bytes; identity hashes remain stable and migration tests preserve SQLite
  rows and schema ordering.
- The root `desktop-product` suite passed 1,648 unit/integration tests, with 31
  established opt-in tests ignored. The full server suite passed 579 tests,
  with 12 established opt-in tests ignored.
- Root and server strict all-target Clippy passed.
- `bash scripts/release-check.sh full` passed, including standalone relocation,
  TUI lifecycle/real-PTY shutdown, product/train/advisory checks, the complete
  root/server tests, and strict Clippy.
- `cargo audit` reported no vulnerabilities; five root maintenance/unsoundness
  warnings remain visible. `cargo deny check` passed advisories, bans,
  licenses, and sources in both roots.

## Post-bump qualification evidence

- Root and standalone-server package metadata report `0.9.7-4`; the only
  lockfile changes are those two local package versions.
- `bash scripts/release-check.sh full` passed again after the version bump.
- A real headless `omenchatd init` under `umask 0000` created the selected home
  and managed directories as `0700`, and config, identity, SQLite, log, and
  Reticulum config files as `0600`; the unrelated parent remained `0755`.
- The current two-client upload/Resource lane passed, including sender and
  second-client retrieval. The continuous reconnect lane passed across an
  orderly server restart with destination continuity and reaction, revision,
  and pin recovery. The current NomadNet direct page request passed.
- Pinned Python interoperability passed for the immutable RNS/LXMF references,
  including IFAC, Link/proof, propagation/stamps, tickets, and direct Resource
  attachment transfer. The informational current-Python lane passed with RNS
  1.4.0, LXMF 1.1.0, and NomadNet 1.2.7, including request/Resource,
  cancellation/no-replay, retained-link, stamp, ticket, and attachment cases.
- Mixed `0.6.0-1`/`0.9.7-4` SQLite reopening passed. Live OMENchat passed in
  both directions between the current and historical products.
- The Linux ARM64 Cross/Podman gate passed 60 protocol tests, 448 headless
  server tests, a release build, QEMU lifecycle, archive, and checksum. Four
  parent tests that re-execute the ARM binary directly are skipped only in the
  Cross lane because that child process bypasses QEMU; all four pass in the
  native host matrix.
- The release archive and package gate passed checksum, extraction, file and
  script inventory, isolated server init/status, redacted collection, and the
  packaged two-client OMENchat smoke. The final locally qualified archive is
  `OMENbrowser_rs-0.9.7-4-20260804T155732Z.tar.gz`.
- Post-bump `cargo audit` found no vulnerabilities in either root. Root audit
  retains five visible non-vulnerability warnings; both `cargo deny check`
  invocations and strict all-target Clippy invocations passed.
- The deliberately ignored maximum-UDP Resource sentinel was invoked directly
  and retained its expected exit 101: the upstream 456-byte buffer cannot
  serialize the 483-byte maximum Resource packet.

## Compatibility and rollback

Both Cargo roots retain the exact official registry Reticulum/LXMF 0.9.7
train. No dependency source, OMENchat wire/capability, database schema,
configuration schema, cache, identity, destination, message, ticket, upload,
or Reticulum-storage format changes. Send/request retry, replay, fallback,
cancellation, and dispatch behavior are unchanged. Rollback to `v0.9.7-3`
needs no state conversion; corrected owner-only Unix modes may remain.

The known maximum-UDP Resource limitation and deliberately ignored sentinel
remain unchanged. GitHub CI run `30942224547` passed Linux quick checks and the
native Windows MSVC, Intel macOS, and Apple Silicon macOS build/test/strict
Clippy matrix on the reviewed candidate. Two preceding runs exposed and then
verified narrow test-only portability corrections: Unix permission fixtures
are compiled only on Unix, and the isolated RPC capture socket explicitly
returns to blocking mode after a nonblocking accept loop. Production storage,
send, cancellation, and replay behavior did not change. Native installer/DMG
lifecycle and physical radio/interface evidence remain external boundaries.
No tag or publication occurred during this work.
