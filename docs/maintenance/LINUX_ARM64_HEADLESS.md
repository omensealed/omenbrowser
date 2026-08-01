# Linux ARM64 headless qualification

OMENbrowser's first Linux ARM64 product is the standalone `omenchatd`
`server-headless` profile. The desktop GPU stack is deliberately outside this
qualification unit.

## Evidence levels

The project records three different kinds of evidence and does not substitute
one for another:

1. **Cross-compile evidence** proves that the selected dependency graph and
   Rust code compile for `aarch64-unknown-linux-gnu`. It does not execute the
   target binary.
2. **Hosted native ARM64 evidence** runs tests and an isolated package lifecycle
   smoke on GitHub's `ubuntu-24.04-arm` runner. It proves execution on a native
   ARM64 Linux VM, not behavior on a constrained Raspberry Pi or a physical
   Reticulum interface.
3. **Physical-device evidence** runs the soak below on a Raspberry Pi or
   equivalent device. This is optional hardware-specific evidence and is only
   required for a Raspberry Pi or physical-hardware qualification claim.

## Local cross-compile gate

With `cross`, Podman, and the Rust target installed:

```bash
CROSS_CONTAINER_ENGINE=podman CARGO_TARGET_DIR=target/aarch64-cross \
  cross check --locked --manifest-path src/server/Cargo.toml \
  --target aarch64-unknown-linux-gnu \
  --no-default-features --features server-headless

CROSS_CONTAINER_ENGINE=podman CARGO_TARGET_DIR=target/aarch64-cross \
  cross check --locked \
  --manifest-path src/server/crates/omenchat-protocol/Cargo.toml \
  --target aarch64-unknown-linux-gnu
```

The maintained local release gate is:

```bash
bash scripts/test-linux-arm64-headless.sh
```

It runs the protocol and applicable server tests as ARM64 executables through
Cross's QEMU runner, builds the ARM64 release binary, runs isolated
`init`/`status`/`doctor`, and creates the checksummed tarball. Two parent tests
that directly re-exec their ARM test binary bypass Cross's runner and are
explicitly excluded from the emulated lane; they remain mandatory in the native
host matrix. Their child fixtures still compile in the ARM lane.

The package script defaults to native-only operation and fails closed on an
x86_64 host. The maintained gate selects its explicit `--cross-emulated` mode.

## Hosted native ARM64 gate

Run the **Linux ARM64 headless** workflow explicitly from GitHub Actions. It is
also reusable through `workflow_call`, but it has no `push` or `pull_request`
trigger. This avoids adding a long ARM job to ordinary changes.

The job checks and tests `omenchat-protocol`, checks/tests/Clippies
`omenchatd server-headless`, builds a native release binary, performs
`init`/`status`/`doctor` against a temporary isolated home, and uploads:

```text
omenchatd-<version>-linux-aarch64.tar.gz
omenchatd-<version>-linux-aarch64.tar.gz.sha256
```

The archive metadata records both `physical_device_qualified: false` and a
passed ARM64 release gate. This is sufficient for the project's Linux ARM64
headless release gate by maintainer decision; it does not assert Raspberry Pi
or physical-radio behavior. The archive does not install or start a service
automatically.

## Physical-device soak checkpoint

Run this only with an explicit isolated test home and test identity. Do not use
the maintainer's normal server home.

Record the device model, OS image, kernel, RAM limit, Rust/package source,
interface type, peer topology, and start/end times. Capture at least:

- settled and peak RSS, CPU, threads/tasks, and file descriptors;
- SQLite size, WAL behavior, restart, and recovery;
- interface loss and reconnect;
- retained-link replacement and client reconnect;
- upload quota acceptance and rejection at boundaries;
- log rotation under a bounded slow-writer workload;
- shutdown duration and post-shutdown process/handle state.

Minimum package and isolated-state setup:

```bash
sha256sum --check omenchatd-*-linux-aarch64.tar.gz.sha256
tar -xzf omenchatd-*-linux-aarch64.tar.gz
cd omenchatd-*-linux-aarch64
export OMEN_ARM64_SOAK_HOME="$PWD/soak-state"
./omenchatd init --home "$OMEN_ARM64_SOAK_HOME"
./omenchatd doctor --home "$OMEN_ARM64_SOAK_HOME"
./omenchatd run --home "$OMEN_ARM64_SOAK_HOME"
```

Configure only a test Reticulum peer/interface. Use the repository's bounded
OMENchat smoke and measurement procedures from `docs/TESTING.md` for the
traffic workload. Preserve redacted logs and measurements as release evidence;
never archive identity material, IFAC secrets, upload bodies, or message
bodies.

No physical-device result has been recorded by this document, and one is not a
release blocker unless a hardware-specific support claim is added.
