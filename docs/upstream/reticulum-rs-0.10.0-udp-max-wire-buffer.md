# reticulum-rs-transport 0.10.0 maximum UDP wire buffer

Verified on 2026-08-12 against the unmodified crates.io
`reticulum-rs-transport 0.10.0` source. OMEN does not carry a local patch,
fork, vendor copy, application fragmentation layer, or Git override.

## Reproducer and result

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
supported boundary. Observed behavior remains insufficient output capacity.

## Relevant source behavior

`src/iface/udp.rs` still declares a buffer derived from
`size_of::<Packet>() * 3`. `Packet` owns heap-backed payload storage, so its
Rust layout size cannot bound serialized wire size. The fixed constant backs
receive and transmit arrays.

## Smallest upstream correction and regression

Size UDP serialization storage from the authoritative maximum wire-packet
bound, not the in-memory `Packet` layout. Preserve a fixed reviewed bound and
route serialization failures into bounded, content-free diagnostics.

Add tests that serialize and loop back a maximum type-one Resource data packet,
one byte below it, an explicitly rejected over-limit packet, and ordinary
direct/broadcast packets. This issue is independent of routed Resource
retransmission and remains a separately named expected-failure sentinel.
