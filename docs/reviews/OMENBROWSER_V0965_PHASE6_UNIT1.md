# Phase 6 unit 1 — Linux ARM64 headless build and package gate

Status: **complete** under the maintainer-approved Podman/Cross release gate.

## Scope and current-code decision

The Phase 6.1 plan asks for ARM64 coverage before any full ARM desktop work.
The current repository already packages macOS Apple Silicon, but had no Linux
ARM64 `omenchatd` gate or artifact. The standalone server and shared protocol
crate remain the correct first target; no root GUI dependency is required by
either one.

This unit adds build/package infrastructure and evidence documentation only.
It changes no runtime behavior, feature identity, protocol, database, storage
path, limits, or product version.

The maintainer accepted successful Podman/Cross ARM64 execution as the release
qualification boundary. Physical-device testing remains useful for a specific
Raspberry Pi or hardware claim but is not a general Linux ARM64 release blocker.

## Changes

- Added an opt-in/reusable native `ubuntu-24.04-arm` workflow. It deliberately
  has no push or pull-request trigger.
- Added a native-host-only ARM64 package script for `server-headless`.
- Added an explicit cross-emulated package mode and one maintained local gate
  that runs ARM64 tests, lifecycle smoke, and packaging through Podman/Cross.
- Added deterministic archive metadata/order/timestamps and a SHA-256 sidecar.
- Added an isolated `init`/`status`/`doctor` package smoke.
- Extended workflow policy checks so the ARM job remains least-privilege,
  native, bounded, pinned, and opt-in.
- Added separate cross-compile, hosted-native, and physical-device procedures.

## Files changed

- `.github/workflows/linux-arm64-headless.yml`
- `.github/workflows/ci.yml`
- `scripts/package-linux-arm64-omenchatd.sh`
- `scripts/test-linux-arm64-headless.sh`
- `scripts/release-check.sh`
- `scripts/verify-workflow-security.sh`
- `docs/maintenance/LINUX_ARM64_HEADLESS.md`
- `docs/TESTING.md`
- `src/server/README.md`
- this report

## Commands and results

Passed locally on x86_64 Linux:

```text
CROSS_CONTAINER_ENGINE=podman CARGO_TARGET_DIR=target/aarch64-cross \
  cross check --locked --manifest-path src/server/Cargo.toml \
  --target aarch64-unknown-linux-gnu \
  --no-default-features --features server-headless

CROSS_CONTAINER_ENGINE=podman CARGO_TARGET_DIR=target/aarch64-cross \
  cross check --locked \
  --manifest-path src/server/crates/omenchat-protocol/Cargo.toml \
  --target aarch64-unknown-linux-gnu

bash -n scripts/package-linux-arm64-omenchatd.sh
bash scripts/verify-workflow-security.sh
git diff --check

cargo test --locked \
  --manifest-path src/server/crates/omenchat-protocol/Cargo.toml
cargo clippy --locked \
  --manifest-path src/server/crates/omenchat-protocol/Cargo.toml \
  --all-targets -- -D warnings
cargo check --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless
cargo clippy --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  --all-targets -- -D warnings

cargo fmt --all --check
cargo fmt --manifest-path src/server/Cargo.toml --check
actionlint .github/workflows/linux-arm64-headless.yml
cargo run --quiet --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless -- --version

bash scripts/test-linux-arm64-headless.sh
```

The server cross-check and protocol cross-check completed successfully. The
protocol suite passed all 60 tests, and the corresponding protocol/headless
Clippy/check gates passed with warnings denied. YAML/action validation, the
workflow security policy, and the x86_64 package fail-closed test passed. The
root/server formatting gates passed, and the headless identity reported
`server-headless:on`, `server-full:off`, and `live-reticulum:on`. The
packaging script cannot and must not produce a qualified archive on this x86_64
host in its default native mode.

The explicit Podman/Cross execution lane then produced these results:

- `omenchat-protocol`: 60 passed, 0 failed;
- applicable `omenchatd server-headless`: 432 passed, 0 failed, 12 ignored,
  2 explicitly filtered native-only parent tests;
- emulated ARM64 `init`, `status`, and `doctor`: passed against an isolated
  target directory;
- release binary ELF machine: AArch64;
- `omenchatd-0.9.6-5-linux-aarch64.tar.gz`: built and SHA-256 verified.

The first unfiltered emulated server run exposed exactly two harness failures.
Both parent tests call `current_exe()` directly, bypassing Cross's QEMU runner,
and exited before their crash marker. They are not application/storage
failures. The same tests remain mandatory and passing in the native host lane;
their child fixtures compile in the ARM target lane.

A direct `podman run --arch arm64` probe also returned `Exec format error`
because this host has no globally registered ARM binfmt handler. Cross's
container-local QEMU runner executed the ARM binaries successfully, which is
why the maintained command uses Cross rather than raw Podman execution.

During evidence setup, invoking the installed `cross` installation triggered a
Rustup update and left the default toolchain incomplete. The user-local stable
toolchain was repaired without host packages or repository changes. The final
recorded local toolchain is Rust/Cargo 1.97.1 and the target is
`aarch64-unknown-linux-gnu` through `cross` 0.2.5 with Podman 6.0.1.

## Not executed

- The new GitHub workflow was not dispatched. This unit does not push changes
  or consume a long CI run.
- No physical ARM64 device was attached, so there is no Raspberry Pi soak,
  constrained-memory, SQLite restart, live interface reconnect, upload quota,
  log rotation, RSS, FD, or task evidence.
- No Debian-family ARM package was added. The conservative tarball is the first
  artifact; distro packaging remains conditional on native/package evidence.
- The full root desktop/TUI matrix, `release-check.sh quick/full`, release
  packaging, Python interoperability, live Reticulum peers, and hardware lanes
  were not rerun for this build-infrastructure-only unit. Their prior results
  are not reclassified as results of this unit.

## Compatibility, storage, and resource impact

There is no application, wire, schema, configuration, or storage migration.
The package smoke uses a temporary isolated server home and deletes it on exit.
The workflow caps build concurrency at two jobs, has a 60-minute deadline, and
uploads only the tarball and checksum. It does not run on ordinary pushes or
pull requests.

## Remaining limitations and rollback

The Linux ARM64 headless release gate is complete under the maintainer-approved
Podman/Cross boundary. Hosted native ARM64 evidence remains optional until the
workflow is explicitly run. Physical-device qualification remains necessary
only before making a Raspberry Pi or physically tested claim. The full ARM
desktop remains out of scope.

Rollback is removal of the new workflow/script/documentation and the two
syntax/security verifier entries; no user or server state is affected.

## Next smallest justified step

Begin Phase 6.2 by inventorying existing static-media/resource constants and measuring the
canonical versus static-media baseline. Do not introduce a `ResourceBudget`
until that inventory proves it replaces real duplicated policy.
