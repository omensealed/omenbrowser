# v0.9.8-4 nickname-colour and attachment-truth execution record

Baseline: `v0.9.8-3` at
`966360ce9c9dd95b7a73b9c596357f2136613ed5` on branch
`feat/v0.9.8-4-nickname-colours`.

## Compatibility decisions

- OMENchat remains wire protocol 1. The private shared crate advances to API
  0.2.0 for strict RGB24 and operations 77–79.
- `nickname-colours-v1` requires explicit negotiation beside
  `durable-mutations-v1`. Legacy Links retain exact five-field user entries;
  negotiated Links alone receive the nullable sixth field and live events.
- `NULL` means automatic. Schema 14 adds only the checked nullable column and
  reuses `profile_revision`; no random backfill or browser schema change occurs.
- The mutation is authenticated-self-only, six attempts per 60 seconds per
  identity, one bounded client pending intent, and no automatic replay after an
  uncertain dispatch. Exact replay returns the stored ack without a second
  revision or fan-out.
- Automatic colour uses SHA-256 with a fixed domain and stable server/user IDs.
  Stored RGB is never theme-mutated; presentation meets 4.5:1 contrast or uses
  the theme foreground fallback.
- Attachment transport is unchanged. Only the structured upstream reason
  `retry_limit_exhausted` receives routed-0.9.8 guidance; all other failures
  remain generic. No attachment replay, fragmentation, or transport patch was
  added.

## Storage and rollback evidence

The schema-13 migration test creates the existing SQLite-consistent sibling
backup before schema mutation, verifies that the backup remains user_version
13 without the new column, and verifies that the migrated database is version
14 with original identity/user data intact and old rows set to `NULL`. Checked
endpoint, no-op, revision-once, and invalid direct-SQL cases pass.

Rollback requires stopping omenchatd and restoring that schema-13 backup before
installing v0.9.8-3. It is not binary-only.

## Qualification status

- Root desktop-product suite: pass.
- Standalone server-full suite: pass, 615 passed and 15 explicitly ignored
  environment/upstream lanes.
- Focused protocol/store/client/contrast/rate/fan-out/terminal tests: pass.
- Strict root and server Clippy: pass after the final narrow lint correction.
- Exact registry Reticulum/LXMF 0.9.8 train and zero accepted advisory policy:
  pass.
- `scripts/release-check.sh quick` and `full`: pass. The full root suite ran
  1,668 tests; server-full ran 615 tests with 15 deliberate ignored lanes.
- Current-product loopback upload, continuous reconnect, and NomadNet page
  process gates: pass. The upload fixture was 873 bytes and both isolated
  clients retrieved the exact Resource.
- Adjacent compatibility: v0.9.8-4 client to v0.9.8-3 server and v0.9.8-3
  client to v0.9.8-4 server both opened, joined, sent, and observed the echo.
- Pinned Python RNS/LXMF, IFAC, NomadNet four-quadrant, propagation, stamp,
  ticket, and Resource lanes: pass. The informational current Python
  RNS 1.4.0/LXMF 1.1.0 drift lane passed IFAC, NomadNet, and 9/10 LXMF cases,
  but the propagated-stamp network test repeatably timed out activating its
  Link. This did not occur in the pinned release-reference lane.
- Linux ARM64 Cross/QEMU: protocol 64 passed; headless server 484 passed with
  15 deliberate ignores and five process-only exclusions; package lifecycle
  passed.
- Sixty-second link soak: 4,451 cycles, 300 KiB RSS growth, zero FD/task
  growth, 780 microsecond maximum close, and no retained active/pending Links.
- Three-cycle 50-pane desktop stress: median settled CPU 0.000%, median RSS
  236,668 KiB, median private dirty 56,048 KiB, and median close 222 ms.
- Native Windows MSVC and native Intel/Apple-Silicon macOS remain hosted gates;
  unavailable local results are not inferred.

The separately named routed Resource and maximum-UDP expected-upstream
sentinels remain visible. Their upstream-ready documents contain no private
payload, identity, path, or credential material.
