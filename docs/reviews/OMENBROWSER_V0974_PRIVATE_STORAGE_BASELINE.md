# OMENbrowser v0.9.7-4 private-storage baseline

Date: 2026-08-04
Baseline branch: `main`
Baseline commit: `67ab1b910e0d51d14d2f63c3e764e86bbccfe2cc`
Baseline tag: `v0.9.7-3`
Initial worktree: clean

## Environment

- Host: Linux x86_64 (`7.1.3-2-cachyos`)
- Rust: `rustc 1.97.1`, host `x86_64-unknown-linux-gnu`, LLVM 22.1.6
- Cargo: `cargo 1.97.1`
- Root and standalone-server package versions: `0.9.7-3`
- Reticulum/LXMF: one exact official registry `0.9.7` train in each applicable
  Cargo root; no Git or patch source
- Default features: empty in both roots

## Pre-change validation

`bash scripts/release-check.sh quick` passed on the untouched checkout. It
included formatting, version/train/advisory checks, native product identities,
isolated TUI lifecycle and real-PTY checks, product-feature checks, focused
OMENchat tests, standalone relocation, server feature checks, and focused server
tests. The explicit pinned-Python IFAC process tests remained ignored in this
quick lane as documented; the deterministic pinned-Python byte vector passed.

No pre-existing quick-gate failure was observed. Native Windows/macOS, Linux
ARM64, live current/pinned Python processes, package install lifecycle, and live
Reticulum peers were not run during this local Phase 0 pass; their established
CI/scripts remain later qualification gates.

## Confirmed permissive-umask reproducer

The current headless `omenchatd init` was run against a newly generated `/tmp`
root in a dedicated shell with `umask 0000`. Only modes and file classes were
printed, and the root contained no real identity or user data.

Observed before changes:

| Path class | Mode |
| --- | --- |
| selected server home | `0777` |
| Reticulum directory/storage/pages | `0777` |
| server config | `0666` |
| placeholder identity | `0666` |
| active server log | `0666` |
| SQLite main database | `0644` |
| Reticulum config | `0600` |

This reproduces the reviewed finding: private creation currently depends on
the caller's umask. WAL/SHM were not created by `init` alone and require the
planned live-connection test.

## Owned-path inventory and boundary

Product-owned private desktop directories are the exact `AppPaths` managed
root, identities/backups, identity storage, managed Reticulum config/storage,
messages, attachments, cache, downloads, plugins, logs, diagnostics, and exact
identity-scoped descendants. Existing private desktop files include settings,
identities/backups, messages, directory/interface/gateway/form state, transient
IDs, and structured logs.

Product-owned private server paths are the selected server home, exact managed
Reticulum config/storage, generated NomadNet pages, uploads, config/backups,
identity/backups, SQLite main/WAL/SHM, migration backups, and active/rotated
logs. Generated NomadNet pages are locally owner-managed even though the server
publishes their bytes through Reticulum.

Custom identity/database/Reticulum paths are external boundaries: only the
actual sensitive file, SQLite sidecars, and a dedicated final directory created
by the product may be protected. Arbitrary existing ancestors are not owned.
User-selected imports, exports, attachment sources, diagnostic export targets,
and package/runtime files are not recursively chmodded. Legacy source trees are
read-only migration inputs; only destination managed directories and the exact
one-shot marker are private-managed.

## Confirmed gaps and already-correct behavior

- `AppPaths::ensure`, server initialization, and several exact managed
  directory sites use generic directory creation without mode repair.
- Server SQLite writable opens do not yet pre-create/repair the main file or
  qualify live WAL/SHM modes.
- Server and desktop rotating log writers use generic create/append and do not
  repair retained rotations.
- The user-service installer uses ordinary `mkdir -p`, and the unit lacks
  `UMask=0077`.
- Existing private atomic writers already request/repair `0600` for several
  desktop state files, server migration backups, Reticulum config edits, and
  upload payloads. Those implementations should be retained.
- Existing symlink/non-regular refusal and bounded log rotation should be
  preserved.

## Implementation recommendation

Add one small local permission helper per Cargo root; apply it only to exact
managed paths and bounded known sidecar/rotation names; prove creation and
metadata-only repair under an isolated permissive umask; then add the systemd
and installer defense-in-depth settings. No content, schema, identity, wire,
retry, cancellation, or dependency-train change is justified.
