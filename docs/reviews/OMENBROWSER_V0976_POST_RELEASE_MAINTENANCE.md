# OMENbrowser v0.9.7-6 post-release maintenance record

## Baseline

- Date: 2026-08-06
- Branch: `maintenance/v0.9.7-6-post-release`
- Commit/tag: `81c6a70e584a93d0c2eff06ec90a4e059a9be7bb` / `v0.9.7-6`
- Initial worktree: clean
- Host: Linux x86_64
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`
- Installed Rust targets: Linux x86_64, Linux ARM64, Windows GNU x86_64
- Root and standalone-server package versions: `0.9.7-6`
- Reticulum/LXMF train: exact official registry `0.9.7` in both Cargo roots

The checkout matches the released commit. No version bump, tag, release, wire
change, persistent-state migration, dependency override, retry, fallback, or
second-dispatch behavior is in scope for this maintenance.

## Initial call-path inventory

- The ordinary desktop upload command already derives and enforces the local
  Reticulum 0.9.7 single-segment-safe ceiling before reading or offering an
  upload.
- The reusable `ChatClientRequest::SendUpload` path reaches
  `send_live_upload_offer`. At baseline it checks the older bounded in-memory
  queue limit, then reserves a sequence and inserts pending state before the
  later Resource dispatch guard. It therefore needs the same derived ceiling
  at this lower ownership boundary.
- Native NomadNet request handling already rejects observed split Resource
  evidence, records bounded event-lag evidence, cancels owned Resources, and
  does not replay. Its ordinary deadline text does not distinguish observed
  Resource activity that never completed.
- Desktop and server bridges already keep bounded, TTL-limited rejected split
  Resource marker maps. They deduplicate split evidence and suppress a late
  completion, but do not expose cumulative redacted safeguard counters.
- The maximum-UDP and split-metadata upstream sentinels are separate and remain
  visible.

## Baseline commands

```text
git branch --show-current
git rev-parse HEAD
git status --short
rustc -Vv
cargo -V
uname -a
rustup target list --installed
bash scripts/release-check.sh quick
```

The quick release check passed against the untouched tree. It included format,
private-storage policy, service installer, dependency/version/security guards,
desktop/TUI identity and PTY smoke checks, focused OMENchat checks, standalone
server relocation, feature checks, and focused server tests. The explicit
pinned-Python IFAC tests remained ignored in this quick lane by design.

## Maintenance results

- The reusable upload API now applies the shared derived Reticulum 0.9.7
  ceiling before sequence reservation, pending-state insertion, offer-frame
  construction, or Resource dispatch. A smaller negotiated server/room limit
  remains authoritative; a larger advertised value cannot bypass the local
  guard.
- Native NomadNet timeout diagnostics now distinguish ordinary inactivity from
  observed Resource activity that did not reach a valid completion. The latter
  explicitly states that no retry was attempted and retains bounded event-lag
  evidence. Existing deadlines, cleanup, cancellation, link-close, and explicit
  split-rejection paths are unchanged.
- Desktop and server split-Resource bridges now maintain saturating cumulative
  counters for unique split rejections, suppressed late completions, and actual
  TTL marker expiry. The counters are ephemeral, contain no transfer identity or
  payload data, and use existing diagnostics/status projections without adding
  polling. Marker item and TTL bounds remain unchanged.

The current release remains v0.9.7-6. This maintenance does not prepare
v0.9.7-7 and does not alter protocol or persistent state.

## Qualification evidence

The following post-change gates passed on the baseline host:

```text
cargo fmt --all -- --check
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings
cargo fmt --manifest-path src/server/Cargo.toml --check
cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full --all-targets -- -D warnings
bash scripts/release-check.sh quick
bash scripts/release-check.sh full
bash scripts/verify-release-version.sh
bash scripts/verify-reticulum-train.sh
bash scripts/verify-accepted-advisories.sh
bash scripts/verify-reticulum-resource-compat.sh
bash scripts/verify-product-features.sh
bash scripts/verify-workflow-security.sh
bash src/server/scripts/verify-standalone.sh check
cargo audit
cargo audit --file src/server/Cargo.lock
cargo deny check
bash scripts/run-omenchat-current-upload.sh --report /tmp/omenbrowser-v0976-maint-upload.json
bash scripts/run-omenchat-continuous-reconnect.sh --report /tmp/omenbrowser-v0976-maint-reconnect.json
bash scripts/run-nomadnet-current-page.sh --report /tmp/omenbrowser-v0976-maint-nomadnet.json
bash scripts/run-current-python-drift.sh --report /tmp/omenbrowser-v0976-maint-current-python.json
bash scripts/run-pinned-python-reticulum.sh
bash scripts/test-linux-arm64-headless.sh
```

The full root suite passed with 31 deliberately ignored tests and no failures.
The full standalone-server suite passed with 604 tests and 13 deliberately
ignored tests. Strict Clippy passed in both Cargo roots. The current-Python
informational lane passed with RNS 1.4.0, LXMF 1.1.0, and NomadNet 1.2.7,
including direct and Resource requests, timeout/cancellation without replay,
retained-link recovery, IFAC, stamps, and tickets. The pinned parity lane passed
against the immutable RNS and LXMF revisions recorded by its script. Its first
run encountered a transient propagation-link activation timeout; an unchanged
clean rerun passed the entire lane. This was recorded rather than hidden or
worked around.

The isolated current-upload lane passed one upload and two client Resource
fetches. Continuous reconnect passed with stable server identity and recovered
messages, reactions, revisions, and pins. The current-Python direct NomadNet
page lane returned a non-empty Micron page through exactly one direct request.
The Linux ARM64 gate passed protocol tests, headless server tests, package
creation, checksum verification, and a QEMU/Podman lifecycle smoke. This is
cross/emulation evidence, not a claim of native hardware testing.

`cargo audit` reported no accepted vulnerabilities. It retained five reviewed
warnings in the desktop dependency graph; the standalone-server graph had no
vulnerability report. `cargo deny check` passed advisories, bans, licenses, and
sources with its existing unmatched-license and Windows-target duplicate
warnings.

Native Windows and macOS jobs were not rerun because this maintenance branch
was not pushed and the host is Linux. A new versioned desktop package candidate
was not built because this task deliberately keeps v0.9.7-6 current and does
not prepare a release. Local full builds, standalone relocation, ARM64 package
smoke, two-client upload, reconnect, and live Python lanes cover the changed
source boundaries. The two deliberately ignored upstream sentinels remain
visible and separate: maximum-UDP Resource transmission and split-metadata
Resource assembly on Reticulum 0.9.7.
