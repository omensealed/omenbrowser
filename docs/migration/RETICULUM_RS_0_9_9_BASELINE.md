# Reticulum/LXMF 0.9.9 upgrade baseline

Baseline recorded on 2026-08-12 before changing dependency pins.

## Source and toolchain

- Branch: `upgrade/v0.9.9-1`
- Source commit: `ede5e131db711463f9f125f36aa8ed7bec4c44c8`
- Released comparison tag: `v0.9.8-5`, commit
  `5de9b897f79fbf2309549c680e05946da0fc9f6c`
- The source commit is two intentional documentation-only commits ahead of the
  release tag. Those current-documentation changes are preserved.
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`
- Host: `x86_64-unknown-linux-gnu`, Linux 7.1.3
- Installed Rust targets: Linux x86_64, Linux aarch64, Windows GNU x86_64

The application and standalone server both reported `0.9.8-5`. Their selected
Reticulum/LXMF packages resolved exclusively from crates.io at `0.9.8`; the
protocol crate was `0.2.0`, the server schema was 14, and the local IFAC crate
was `0.9.5-1`.

## Baseline gates

The following passed on the source commit above:

- `bash scripts/release-root-sanity.sh --browser-root /tmp/omenbrowser-rs-v0991-a --browser-root-2 /tmp/omenbrowser-rs-v0991-b --server-home /tmp/omenchatd-v0991`
- `bash scripts/release-check.sh quick`
- `cargo check --locked --no-default-features --features desktop-product`
- `cargo test --locked --no-default-features --features desktop-product`
  (`1655` passed, `31` deliberate ignored tests)
- `cargo clippy --locked --no-default-features --features desktop-product --all-targets -- -D warnings`
- `cargo check --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-headless`
- `cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full`
  (`615` passed, `15` deliberate ignored tests)
- `cargo clippy --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full --all-targets -- -D warnings`
- `cargo deny --locked --all-features check licenses bans sources`
- `cargo deny --manifest-path src/server/Cargo.toml --locked --all-features check licenses bans sources`

The installed `cargo-audit` does not accept the documented `--locked` option.
The supported `cargo audit` and `cargo audit --file src/server/Cargo.lock`
commands reported no vulnerabilities. They reported seven root and one server
allowed warnings, including the newly published `lru 0.18.1`
`RUSTSEC-2026-0253` unsoundness warning. This is baseline evidence, not an
accepted-vulnerability-policy change; the repository's exact advisory verifier
passed and continues to accept zero vulnerabilities.

## Upstream sentinels on 0.9.8

Both deliberately ignored limitation tests were executed explicitly and
failed for their documented reasons:

- `reticulum_routed_resource_retransmission_survives_fragment_loss`: the
  0.9.8 forwarding duplicate filter suppresses requested duplicate Resource
  data/proof packets.
- `reticulum_udp_tx_buffer_covers_max_resource_wire_packet`: the upstream UDP
  transmit buffer is 456 bytes while the maximum serialized Resource wire
  packet is 483 bytes.

They remain separate sentinels. Neither result is hidden or treated as a
product-test pass.

## Resource baseline

An exact-current release desktop binary embedded the source SHA and canonical
`desktop-product` identity. A five-second headless sample after one second of
warmup measured:

- startup to window: 1065 ms;
- close latency: 167 ms;
- median CPU: 1.949%;
- median RSS: 220052 KiB;
- median private dirty memory: 43332 KiB;
- median file descriptors: 61.

A five-second isolated headless server sample measured 10540 KiB RSS, seven
threads, thirteen file descriptors, and two CPU ticks at 100 ticks/second. A
five-second release Link/reconnect soak completed 374 cycles with bounded
pending/active limits, 176128 bytes RSS growth, zero FD growth, zero task
growth, and complete final cleanup.

These short samples are comparison anchors, not long-duration qualification.
The first desktop attempt was correctly rejected from comparison because its
binary identified itself as stale `0.9.8-4`; it remains only in ignored target
evidence.

## Environment-bound baseline lanes

No normal user root was used. Live public-network NomadNet, LXMF propagation,
and routed OMENchat attachment operations were not repeated during this local
baseline capture. Pinned/current Python, Linux ARM64, native Windows, native
macOS, and release package installation remain post-change qualification lanes.
Their absence here is not a passing result.

Raw command output, metadata, feature trees, and measurements are retained only
under ignored `target/upgrade-v0.9.9-1/baseline/`. The retained document omits
identity material, destination hashes, credentials, content, and private state
paths.
