# OMENbrowser v0.9.6-6 Phase 7 local qualification report

Date: 2026-08-01 UTC  
Branch: `hardening/v0.9.6-6-phase-plan`  
Baseline SHA: `2a77a753e80bb8e7db24a6411d923bf14a8e8722`

## Outcome

The accepted local v0.9.6-6 scope passes its deterministic build, test, lint,
identity, version, dependency, standalone-relocation, PTY, packaging, extraction,
and isolated two-client OMENchat gates. Root and standalone server package
versions advanced together from `0.9.6-5` to `0.9.6-6` only after the pre-bump
deterministic matrix passed.

This is local release-candidate evidence, not final publication evidence.
Hosted CI, pinned/current Python interoperability, and native Windows/macOS
packaging remain pending for one batched candidate run.

## Scope and architecture decision

This release closes the reviewed correctness, truthful capability, invitation
receive-preview, propagation diagnostics, Linux ARM64 headless, and low-power
evidence work. It does not activate features whose locked-0.9.6 provenance,
cursor, peer capability, or streaming evidence is insufficient.

- Managed Reticulum remains the supported product runtime; experimental shared
  runtime remains excluded.
- `omenchatd` remains a separate package with its own lockfile, configuration,
  identity, database, and release binary.
- OMENchat remains wire protocol version 1. No database, configuration, cache,
  destination, or RPC contract version changed merely to match the application.
- Reticulum/LXMF remains on exact registry version `0.9.6`; no private fork or
  patch override was added.
- No uncertain send, request, Resource operation, or durable mutation gained an
  automatic retry.

## Documentation and release metadata

Created the v0.9.6-6 checklist, release notes, and this report. Updated the
tester sheet with the paired low-power procedure. Updated current release
references in manifests, lockfiles, version checks, mixed/live scripts, Python
workflow report names, quickstart, backend documentation, protocol document,
server README, and release-version policy. Historical v0.9.6-5 reports and the
explicit prior-version invitation fixture remain unchanged.

## Commands and results

All commands below exited 0:

```text
cargo fmt --check
cargo fmt --manifest-path src/server/Cargo.toml --check
git diff --check
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings
cargo test --locked --no-default-features --features desktop-product-static-media
cargo test --locked --no-default-features --features tui
cargo clippy --locked --no-default-features --features tui --all-targets -- -D warnings
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full --all-targets -- -D warnings
bash scripts/release-check.sh full
bash scripts/verify-release-version.sh
bash scripts/verify-reticulum-train.sh
bash scripts/verify-product-features.sh
bash scripts/verify-workflow-security.sh
bash scripts/release-package.sh /tmp/omenbrowser-v0966-dist-rc1
bash scripts/release-check.sh package /tmp/omenbrowser-v0966-dist-rc1/OMENbrowser_rs-latest.tar.gz /tmp/omenbrowser-v0966-package-smoke
```

The canonical library ran 1,635 tests before its integration suites. The
static-media library ran 1,631 tests. The TUI library profile ran 719 tests.
The standalone full server reported 557 passed and 12 explicitly ignored
environment/soak tests. The consolidated gate additionally passed exact
version/feature/dependency identity, the accepted advisory boundary, native CLI
identity, isolated TUI lifecycle, real Linux PTY restoration, headless dependency
isolation, and a relocated standalone server build/test.

The package gate verified checksum, extraction, required files, script syntax,
root isolation, binary identity, isolated server init/status/doctor, redacted
collection, and two isolated browser clients against one isolated server. Its
report is under `/tmp/omenbrowser-v0966-package-smoke/`; normal user and server
roots were not used.

## Resource impact

Phase 7 added no production behavior. Phase 6's same-binary paired evidence is
the candidate resource baseline: canonical normal/low-power median CPU was
4.878%/0.974%, task clock was 5711.40/1857.38 ms, RSS was 222,652/223,408 KiB,
and file descriptors remained 60. Static reverse-order evidence also reduced
median CPU and task clock. p95 CPU and native GPU/compositor improvement are not
claimed.

## Tests not run and remaining limitations

- Hosted CI, native Windows/macOS packaging, pinned Python interoperability, and
  current-Python drift were not rerun during local iteration. They remain one
  batched candidate gate.
- An exact external `reticulumd` executable was unavailable, so daemon
  disconnect/restart recovery remains unclaimed.
- The locked-0.9.6 maximum UDP Resource reproducer remains upstream-red.
- Outbound invitations, NomadNet topic pointers, LXMF OMENchat notices, and
  large Resource-reference attachments remain disabled or dormant under their
  documented capability/provenance/streaming evidence.
- No physical ARM device or native GPU was tested. Maintainer-approved
  Podman/Cross/QEMU evidence satisfies only the ARM64 headless boundary.

## Compatibility, rollback, and next gate

The revision changes package/artifact identity without a mandatory persistent
data migration. Rolling back to v0.9.6-5 requires no database downgrade.

The next safe step is one reviewable candidate commit followed by a single
batched hosted CI/Python/native-packaging run. Publication, merge, tag, and
release remain contingent on those results and artifact identity review.
