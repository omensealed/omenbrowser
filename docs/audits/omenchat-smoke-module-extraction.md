# OMENchat smoke module extraction

Date: 2026-07-27

## Scope

The binary-only OMENchat live-smoke implementation was mechanically moved from
the root `src/main.rs` module to `src/omenchat_smoke.rs`. The extracted module
owns:

- the in-memory `ChatLinkTransport` used only by the smoke harness;
- bounded Link-data and Resource wait loops;
- initial connection and continuous replacement-Link orchestration;
- upload, reaction, correction, and tombstone qualification flows;
- smoke event/report formatting and isolated reconnect-marker creation.

The root CLI continues to own argument parsing, product command selection,
runtime configuration overrides, and the shared configuration-loading helpers.
The extracted module is private to the binary and exposes only its single
`run` entry point to its parent.

## Compatibility and resource impact

This is a mechanical ownership change. CLI flags, report schema and paths,
timeouts, queue bounds, retry behavior, runtime ownership, protocol operations,
database schemas, identities, and application/server storage are unchanged.
It adds no worker, timer, channel, cache, dependency, or persistent data.

The reconnect-marker regression moved alongside its implementation. It still
uses an explicit temporary directory, requires create-new semantics, and
removes the isolated fixture after the test.

## Validation

Focused validation:

```text
cargo fmt --all -- --check
cargo check --locked --no-default-features
cargo check --locked --no-default-features --features desktop-product
cargo test --locked --no-default-features --features desktop-product \
  --bin omenbrowser_rs omenchat -- --nocapture
cargo test --locked --no-default-features --features desktop-product
cargo clippy --locked --no-default-features --features desktop-product \
  --all-targets -- -D warnings
git diff --check
```

The focused OMENchat binary tests passed, including CLI parsing and the
module-local reconnect-marker regression. The full desktop-product tests and
strict Clippy gate also passed. The standalone server, shared protocol, native
package, and Python interoperability gates were not repeated because this
mechanical extraction changes only the root binary's private module ownership;
their implementations, manifests, wire fixtures, scripts, and dependencies are
unchanged.

## Rollback

Rollback is the mechanical restoration of `src/omenchat_smoke.rs` below the
native-smoke command in `src/main.rs`, restoration of its feature-gated imports,
and replacement of `omenchat_smoke::run` with the former local function call.
No data or compatibility rollback is required.
