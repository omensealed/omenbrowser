# v0.9.9-2 maintenance execution record

This is the active, redacted execution record for the narrow maintenance
candidate. It is not evidence that an unrun hosted, hardware, Python, package,
or public-network lane passed.

## Baseline

- Branch: `maintenance/v0.9.9-2`
- Released source: tag `v0.9.9-1`, commit
  `a6493f18922cfaef53ceaf9d78d8f29a78fefcf0`
- Initial worktree: clean
- Host: x86_64 Linux; Rust/Cargo 1.97.1; repository MSRV remains 1.85
- Installed Rust targets: x86_64 Linux, aarch64 Linux, and Windows GNU
- Root/server package versions: `0.9.9-1` during baseline
- Reticulum/LXMF: one exact official crates.io `0.9.9` train in both Cargo
  roots; no Git, fork, vendor, private registry, or patch override

The requested `docs/SECURITY.md` and `src/server/deny.toml` paths do not exist
in this checkout. Current security policy is
`docs/maintenance/DEPENDENCY_SECURITY.md`; the independent server manifest is
checked with the root `deny.toml` through `--manifest-path`.

## Untouched results

| Command | Result |
|---|---|
| `bash scripts/release-root-sanity.sh --browser-root /tmp/omenbrowser-rs-test --browser-root-2 /tmp/omenbrowser-rs-test-2 --server-home /tmp/omenchatd-test` | pass |
| `bash scripts/verify-reticulum-train.sh` | pass |
| `bash scripts/release-check.sh quick` | pass |
| `bash src/server/scripts/verify-standalone.sh check` | pass |
| `bash scripts/verify-reticulum-resource-compat.sh` | pass under its pre-maintenance scope; it protected split metadata and UDP but not routed retransmission |

No pre-existing baseline failure was observed. The quick gate reported zero
accepted vulnerabilities while retaining reviewed non-vulnerability warnings.
Ignored pinned-Python tests remained explicitly inventory-visible rather than
being reported as run.

## Verified findings and decisions

- The active transport-gap document contained a stale sentence saying the
  oversized current-Python request path was unqualified, despite later
  four-quadrant evidence. It is corrected without changing runtime behavior.
- Split metadata is a normal passing regression. Routed fragment-loss
  retransmission and maximum UDP remain separate ignored upstream `0.9.9`
  sentinels.
- The published SDK/RPC capture and pre-connection rejection tests already
  preserve the exact lossy-field boundary. Maintenance adds focused evidence;
  it does not enable external/shared sending or alter managed mode.
- No authoritative public API justifies new interface/path/Resource diagnostic
  values beyond the existing typed lifecycle/capability model. This revision
  therefore avoids placeholder fields, polling, and a new diagnostics history.
- A dev-only Reticulum testkit has not been created: the independent Cargo
  roots do not currently have two consumers sharing a stable helper API without
  coupling standalone omenchatd to the browser workspace. Large native-module
  splitting is likewise deferred because no behavior fix requires it.

## Implementation and local qualification

- `scripts/verify-reticulum-capability-docs.sh` now structurally verifies the
  supported/unsupported/unknown ledger and rejects the retired oversized
  Request Resource statement or promotion of either upstream limitation.
- `scripts/verify-reticulum-resource-compat.sh` now protects the exact names,
  attributes, critical assertions, evidence documents, and independent status
  of the routed fragment-loss and maximum-UDP sentinels. Temporary-fixture
  negative tests exercise both verifiers from the full release gate.
- The published SDK/RPC lane has focused per-field coverage proving TTL,
  idempotency, correlation, extensions, and explicit remembered reply-ticket
  guarantees reject before an endpoint connection.
- A test-only Reticulum constant reference was removed from the storage-only
  server feature closure. This restores the crash-recovery lane without adding
  Reticulum to that closure or changing production behavior.

The final local canonical matrix passed: root desktop product, static-media,
TUI, server headless/full, strict Clippy, formatting, standalone relocation,
private-storage, workflow/source policy, accepted-advisory, full release, smoke,
crash recovery, continuous reconnect, current upload, current NomadNet page,
and package gates. The smoke suite passed OMENchat loopback, two-client,
Resource transfer, LXMF loopback, NomadNet, doctor, and scroll tests. Its
optional `lxmf-cli` and `reticulumd` cases were skipped because those executables
are not installed; these optional tools are not supported product prerequisites.

Both deliberate limitation sentinels were run with `--ignored --nocapture`.
Routed retransmission returned the documented duplicate Resource data/proof
suppression failure; maximum UDP returned the documented `456 < 483` failure.
Split metadata passed as a normal exact-byte regression.

Pinned Python interoperability passed at RNS revision
`15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF revision
`727830cefda83d9c6e3982b48675425f3f988f9c`. Current drift passed with RNS
1.4.2, LXMF 1.1.1, NomadNet 1.2.8, and msgpack 1.2.1, including IFAC,
proof ordering, the four NomadNet quadrants, cancellation without replay,
retained-Link recovery, and LXMF propagation/stamp/ticket/Resource cases.

Adjacent `v0.9.9-1` interoperability passed in both directions. The candidate
client also passed an orderly adjacent-server restart with stable destination
and copied client state, plus Resource-backed history. A separate copied-state
rollback ran `v0.9.9-1 -> v0.9.9-2 -> v0.9.9-1`; schema 14, three rooms, and
the initialized identity bytes remained intact.

Five interleaved, same-host five-second server-idle samples produced these
medians: CPU ticks `2 -> 2`, RSS `18828 -> 18624 KiB`, threads `7 -> 7`, and
file descriptors `13 -> 13`. No new production polling, task, queue, or retained
diagnostic state was added.

The first ARM64 Cross/QEMU full run had one load-sensitive 250 ms Tokio timer
test exceed its deadline. The exact isolated rerun passed in 0.37 seconds, and
an unchanged complete rerun then passed 484 tests with 15 deliberate ignores,
followed by the cross-built ARM64 archive and QEMU lifecycle smoke. This is
emulated ARM64 evidence, not a claim of physical-device testing.

Package candidate:

- `dist/OMENbrowser_rs-0.9.9-2-20260813T190012Z.tar.gz`
- SHA-256 `5b7f1ebf15b648d015de7a057a4d52684cd6b7d89ee03f9b0a936d4d4abc4a34`
- archive extraction, help/version, isolated server init/status, and two-client
  OMENchat package smoke passed.

The installed `cargo-audit` does not implement its newer `--locked` option.
The root implicit lockfile and explicit `src/server/Cargo.lock` forms both
passed with the repository's reviewed warning policy. `cargo deny` passed
licenses, bans, and sources for both manifests.

## External boundaries

No candidate commit was pushed as part of this implementation. Candidate-SHA
native Windows, Intel macOS, Apple Silicon, hosted ARM64, hosted Python,
mixed-release, and package workflow URLs therefore do not yet exist and are
not reported as passing. Those hosted jobs remain the final external release
boundary after explicit push authorization. No public-network or physical
device evidence is claimed.
