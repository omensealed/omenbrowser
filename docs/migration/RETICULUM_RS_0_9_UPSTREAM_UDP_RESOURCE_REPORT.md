# Reticulum 0.9 UDP Resource transmit-buffer report

Date last verified: 2026-07-31

Current affected package: crates.io `reticulum-rs-transport = 0.9.6`,
checksum
`149873f10b5c575718976ceb6be2dfc25a6adb0d4df012b7b80b135af40c788e`

Historical affected packages: 0.9.0 through 0.9.5.

The exact locked 0.9.6 registry source still declares `BUFFER_SIZE` as
`size_of::<Packet>() * 3` in `src/iface/udp.rs`. OMEN retains the known-red
Resource gates and this report as upstream-ready evidence. It does not carry a
fork, patch override, protocol-limit reduction, or application fragmentation
workaround.

Suggested upstream issue title:

> UDP interface silently drops maximum Resource packets because its buffer is
> derived from `size_of::<Packet>()`

## Summary

The UDP interface sizes its receive and transmit buffers as
`core::mem::size_of::<Packet>() * 3`. `Packet` stores its payload in a
heap-backed `Vec`, so the Rust layout size does not bound the serialized wire
size. On 64-bit Linux the buffer is 456 bytes. A maximum type-one Reticulum
Resource packet is 483 bytes:

```text
2 header bytes + 16 destination bytes + 1 context byte + 464 PACKET_MDU
```

When the Resource sender builds full-size parts, `Packet::serialize` returns
`RnsError::OutOfMemory`. The UDP transmit task tests only `is_ok()` and silently
drops the error. The peer retries valid Resource requests until
`retry_limit_exhausted`.

This is not an OMENchat framing or Resource-request correlation error. The
sender receives and decrypts every request, finds the matching outbound
Resource, and builds every requested part before the UDP serialization
boundary.

## Reproduction

From the OMENbrowser repository:

```bash
cd src/server
cargo test --locked --no-default-features --features server-headless \
  reticulum_multiprocess_resource_complete_cancel_reuse \
  -- --ignored --nocapture
```

Unmodified locked 0.9.6 result on 2026-07-31:

- receiver accepts the Resource advertisement;
- the advertisement declares 4,176 transfer bytes, 4,117 data bytes, and nine
  parts;
- receiver sends ten valid encrypted Resource requests;
- sender diagnostics report `sender_present=true`, `built=4`, and
  `responses=4` for every request;
- no Resource part reaches the receiver;
- receiver fails with `retry_limit_exhausted`;
- sender times out waiting for `OutboundComplete`.

The smaller invariant test reports the platform values directly:

```bash
cargo test --locked --no-default-features --features server-headless \
  reticulum_udp_tx_buffer_covers_max_resource_wire_packet \
  -- --ignored --nocapture
```

Observed failure:

```text
upstream UDP tx buffer (456) cannot serialize a maximum Resource wire packet (483)
```

## Proposed correction

Use one constant for the UDP interface's declared MTU and its RX/TX buffers.
Handle serialization errors through the existing bounded runtime status and
logging surfaces instead of dropping them. The candidate is stored in
`docs/migration/reticulum-rs-0.9-udp-resource-buffer.patch`.

The correction is intentionally narrow:

- no Resource limit changes;
- no wire-format changes;
- no application fragmentation;
- no new dependency;
- fixed bounded buffers;
- approximately 3,184 additional bounded bytes across one worker's RX and TX
  buffers on this target (`2 * (2048 - 456)`);
- serialization failures increment the existing `tx_errors` counter and set
  `last_error` without logging packet contents or identity secrets.

## Candidate validation

The patch was applied only to an isolated copy of the published 0.9.0 crate.
OMEN's manifests, lockfiles, registry source, and production build remained
unchanged.

Results:

- upstream `reticulum-rs-transport` library suite: 509 passed;
- OMEN two-process Resource gate: passed in 0.57 seconds;
- baseline 4 KiB Resource completed;
- 16 KiB Resource cancellation completed;
- post-cancel 4 KiB Resource completed on the reused active link;
- no protocol, quota, or test assertion was weakened.

The candidate used a temporary Cargo source override solely for validation. It
must not be committed as OMEN's normal dependency strategy.

## Requested upstream regression coverage

1. Assert the UDP serialization buffer is at least `UdpInterface::mtu()`.
2. Send a maximum-size type-one packet through an actual UDP interface and
   compare the received bytes.
3. Complete a multi-part Resource across two UDP transports.
4. Exercise cancellation followed by Resource reuse on the same link.
5. Force an oversized serialization attempt and assert `tx_errors` and
   `last_error` change instead of silently dropping the packet.
6. Run the cases on 32-bit and 64-bit targets so Rust layout changes cannot
   become wire-size assumptions again.

## OMEN adoption gate

OMENbrowser must keep the production dependency pinned to the reviewed
registry release until one of these is explicitly approved:

1. an upstream immutable release containing the correction and regression
   tests; or
2. a maintainer-approved, immutable upstream commit dependency with documented
   source policy and rollback.

After adoption, rerun both explicit tests, the full desktop/server matrices,
pinned Python UDP/Resource interoperability, dependency-source checks, and the
Resource performance measurements. Do not mark UDP Resource parity complete
from this temporary candidate alone.

For the current 0.9.6-aligned product, the maintainer classifies this unchanged
published defect as a documented upstream limitation rather than an OMEN
release blocker. The known-red tests and adoption gate remain unchanged, and
release notes must not claim maximum UDP Resource parity. OMEN will wait for an
immutable upstream release and will not submit, fork, or carry the candidate
patch in production.
