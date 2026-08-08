# OMENbrowser v0.9.8-3 NomadNet responder baseline

Date: 2026-08-08

## Checkout and toolchain

- Branch: `fix/v0.9.8-3-nomadnet-response-selection`
- Baseline commit: `e19b3ee726afaee4a5575062eaacd2ccce14a4bd`
- Baseline tag: `v0.9.8-2`
- Initial working tree: clean
- Host: `x86_64-unknown-linux-gnu`
- Rust: `rustc 1.97.1`
- Cargo: `cargo 1.97.1`
- Installed targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and `x86_64-pc-windows-gnu`

Both independent lockfiles resolve the official registry Reticulum/LXMF `0.9.8`
train. No selected family member uses a Git or patch source.

The pinned Python lane remains RNS commit
`15320e4d2cfabb143c1db20ca887e275fd521585` and LXMF commit
`727830cefda83d9c6e3982b48675425f3f988f9c`. The current-drift lane is
configured for RNS `1.4.0`, LXMF `1.1.0`, NomadNet `1.2.7`, and msgpack
`1.2.1`.

## Clean baseline results

The following command chain passed before production edits:

```text
cargo fmt --check
cargo test --locked --no-default-features --features desktop-product
cargo fmt --manifest-path src/server/Cargo.toml --check
cargo test --locked --manifest-path src/server/Cargo.toml --no-default-features --features server-full
bash scripts/release-check.sh quick
```

The root suite passed 1,658 library tests. The standalone server suite passed
601 tests; 12 explicitly documented environment/soak tests remained ignored.
The quick release gate also passed version, exact dependency-train, advisory,
product-feature, TUI lifecycle/PTY, and standalone relocation checks.

## v0.9.8-2 defect reproduction

An isolated two-process reproduction used temporary browser/server roots, a
plain local TCP interface, a small direct request for `/page/index.mu`, and a
32,768-byte portal body. The request reached the live server once. The old
direct-ingress responder attempted a direct response packet with a 32,790-byte
packed envelope, logged:

```text
reticulum-rs direct NomadNet response packet failed ... bytes=32790 error=OutOfMemory
```

No response Resource was advertised, the browser timed out in
`response_wait`, and the report classified the live fetch as failed. This is
the expected red baseline: the response primitive was selected from request
ingress instead of complete response-envelope size.

The reproduction used only isolated `/tmp` roots. No normal browser or server
state was read or modified.

## Source inventory

- `LinkEvent::Data` with `PacketContext::Request` calls the direct-only
  `send_direct_nomadnet_response` path.
- Completed inbound request Resources always call
  `Transport::send_response_resource`.
- Both branches build the same `[request_id, response_body]` envelope through
  `nomadnet_response_resource_payload`, but the direct branch manually mutates
  a `data_packet` context and the Resource branch performs a synchronous portal
  read in the event receiver.
- `std::fs::read` currently leaves portal input bounded only by later parser or
  transport behavior.
- The browser already accepts either correlated response primitive for either
  request primitive and never replays after possible dispatch.

The next change is therefore limited to one bounded common server responder,
an envelope-length-only primitive selector, and the four-quadrant regression
matrix.
