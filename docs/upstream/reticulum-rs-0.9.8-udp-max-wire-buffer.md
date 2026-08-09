# reticulum-rs-transport 0.9.8 maximum UDP wire buffer

Verified: 2026-08-09 against the unmodified crates.io
`reticulum-rs-transport 0.9.8` source. OMEN does not carry a local patch,
fork, vendor copy, application fragmentation layer, or Git override.

## Minimal reproducer and result

Run:

```bash
cargo test --locked --manifest-path src/server/Cargo.toml \
  --no-default-features --features server-headless \
  reticulum_udp_tx_buffer_covers_max_resource_wire_packet \
  -- --ignored --nocapture
```

The unchanged sentinel compares the upstream UDP buffer with the authoritative
maximum serialized type-one packet. On the qualified x86_64 host the buffer is
456 bytes while the wire packet is 483 bytes:

```text
2 header + 16 destination + 1 context + 464 PACKET_MDU = 483
```

Expected behavior is successful serialization and UDP loopback at the maximum
supported boundary. Observed behavior is insufficient output capacity; in the
live path the current source tests `serialize(...).is_ok()` and does not emit
the packet.

## Relevant source behavior

`src/iface/udp.rs` declares:

```text
BUFFER_SIZE = size_of::<Packet>() * 3
```

`Packet` owns heap-backed payload storage, so its Rust layout size cannot bound
its serialized wire size. The same fixed constant backs receive and transmit
arrays.

## Smallest upstream correction and regression

Size the UDP serialization buffer from the transport's authoritative maximum
wire-packet bound, not `size_of::<Packet>()`. Preserve a fixed, reviewed bound
and route serialization failures into the existing bounded `tx_errors` and
`last_error` diagnostics without packet contents.

Add tests that serialize and loop back:

- a maximum type-one Resource data packet;
- one byte below the maximum;
- an over-limit packet that fails explicitly;
- ordinary direct and broadcast packets.

This issue is independent of routed Resource retransmission and remains a
separately named expected-failure sentinel.
